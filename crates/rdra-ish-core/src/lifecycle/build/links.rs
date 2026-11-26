//! What `links`, `relinks`, and `unlinks` do to an owner slot, and how an action binds its owners.

use super::{ActionDraft, Builder};
use crate::analysis::{applied_bodies, AppliedBody};
use crate::lifecycle::build::preds;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    Action, At, CmpOp, Domain, Effect, Guard, GuardWhy, ModelRef, ParamRole, Pred, Rhs, Site, Term,
    Value, VarId, VarKind,
};
use crate::model::{CrossConstraintScope, EntityKey, FkId, Join, LinkKind, SetValue, UseCaseKey};
use rdra_ish_syntax::ast::Kind;

impl<'m> Builder<'m> {
    /// R-bind: a created child belongs to its parent, a present one unless the relate is
    /// optional; a child the action updates must already belong to a parent the action only
    /// updates. An optional key also links a created child to a present parent when a guard
    /// of the action reads the parent through that key. A row an earlier step of the run
    /// wrote (`Earlier`) is the action's row of its entity here, as a written one is.
    ///
    /// Relink: an action that creates the parent and updates an existing child links the child.
    /// For an optional key the child must be unlinked, so a linked child is not relinked; a
    /// mandatory child always has a parent, so the link is a move. A parent param is a row
    /// the action reads, not writes, so it is never relinked. Each key has its own parent
    /// param, so a child created under two keys to one parent the action does not write may
    /// point at two rows. A parent the action writes binds only the keys
    /// [`Self::written_parent_key`] gives it; every other key is bound as if the action did
    /// not write the parent, and several keys none of which a `.by` names are
    /// `AmbiguousOwnerLink`.
    pub(super) fn owner_bindings(
        &mut self,
        uc: UseCaseKey,
        bodies: &[AppliedBody<'m>],
        a: &mut ActionDraft,
    ) {
        for &(var, child, parent) in &self.owners.clone() {
            let Some(c) = a.row_of(child) else { continue };
            let VarKind::Owner { fk, .. } = self.ts.vars[var.index()].kind else {
                continue;
            };
            if self.linked_key(uc, fk) {
                // `links` binds this key: see `link_effects`.
                continue;
            }
            let site = self.ts.vars[var.index()].site.clone();
            let written = match a.row_of(parent) {
                Some(p) => match self.written_parent_key(uc, fk) {
                    Join::Key(k) if k == fk => Some(p),
                    Join::Ambiguous(candidates) => {
                        self.ambiguous_written_parent(uc, fk, &candidates);
                        None
                    }
                    _ => None,
                },
                None => None,
            };
            let parent_is_new =
                written.is_some_and(|p| a.params[usize::from(p)].role == ParamRole::New);
            let child_is_new = a.params[usize::from(c)].role == ParamRole::New;
            if child_is_new || parent_is_new {
                let optional = matches!(
                    self.ts.vars[var.index()].domain,
                    Domain::Instance { optional: true, .. }
                );
                if optional && !child_is_new {
                    a.guards.push(Guard {
                        pred: Pred::Cmp(
                            Term::Slot {
                                var,
                                at: At::Param(c),
                            },
                            CmpOp::Eq,
                            Term::Lit(Value::NULL),
                        ),
                        why: GuardWhy::Owner,
                        site: site.clone(),
                    });
                }
                let guarded = || self.guard_reads_parent(bodies, fk);
                let rhs = match written {
                    Some(p) => Rhs::Param(p),
                    None if optional && !(child_is_new && guarded()) => Rhs::Lit(Value::NULL),
                    None => Rhs::Param(a.param(parent, ParamRole::Parent { key: fk })),
                };
                a.effects.push(Effect::Assign {
                    var,
                    param: c,
                    rhs,
                    site,
                });
            } else if let Some(p) = written {
                a.guards.push(Guard {
                    pred: Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(c),
                        },
                        CmpOp::Eq,
                        Term::Index(At::Param(p)),
                    ),
                    why: GuardWhy::Owner,
                    site,
                });
            }
        }
    }

    /// Whether a `links`, `relinks` or `unlinks` of the use case follows key `fk`.
    fn linked_key(&self, uc: UseCaseKey, fk: FkId) -> bool {
        self.model
            .links
            .iter()
            .any(|l| l.origin == uc && l.key == Some(fk))
    }

    /// `links`, `relinks` or `unlinks(UC, Child, Parent)` on the step that writes the child,
    /// each a guard (`GuardWhy::Link`) and an assignment on the child's owner slot of the key.
    /// The parent the step binds is the row it writes, the row an earlier step of its run
    /// wrote, or else any present parent, bound as a `Parent` param of the key.
    /// - `links`: the child is unlinked, and the owner becomes the bound parent.
    /// - `relinks`: the child is linked (no guard on a mandatory key, whose child always is),
    ///   and the owner becomes the bound parent, the one it already has included: a move
    ///   that had to differ could never run at one instance per entity, which is what
    ///   `check` and `states` explore.
    /// - `unlinks`: the child is linked, to the bound parent when the step writes one, and the
    ///   owner becomes unlinked. The step binds no parent otherwise, since nothing reads it.
    pub(super) fn link_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        for (idx, link) in model.links.iter().enumerate() {
            if link.origin != uc {
                continue;
            }
            let Some(fk) = link.key else { continue };
            let (Some(&child), Some(&parent)) =
                (self.ent_of.get(&link.child), self.ent_of.get(&link.parent))
            else {
                continue;
            };
            let (Some(c), Some(var)) = (a.param_of(child), self.owner_var(fk)) else {
                continue;
            };
            let site = Site::new(ModelRef::Link(idx), &link.site);
            let optional = matches!(
                self.ts.vars[var.index()].domain,
                Domain::Instance { optional: true, .. }
            );
            let owner = Term::Slot {
                var,
                at: At::Param(c),
            };
            let bound = a.row_of(parent);
            let guard = match link.kind {
                LinkKind::Links => Some(Pred::Cmp(owner, CmpOp::Eq, Term::Lit(Value::NULL))),
                LinkKind::Relinks if !optional => None,
                LinkKind::Relinks => Some(Pred::Cmp(owner, CmpOp::Ne, Term::Lit(Value::NULL))),
                LinkKind::Unlinks => Some(match bound {
                    Some(p) => Pred::Cmp(owner, CmpOp::Eq, Term::Index(At::Param(p))),
                    None => Pred::Cmp(owner, CmpOp::Ne, Term::Lit(Value::NULL)),
                }),
            };
            if let Some(pred) = guard {
                a.guards.push(Guard {
                    pred,
                    why: GuardWhy::Link,
                    site: site.clone(),
                });
            }
            let rhs = match link.kind {
                LinkKind::Unlinks => Rhs::Lit(Value::NULL),
                LinkKind::Links | LinkKind::Relinks => Rhs::Param(match bound {
                    Some(p) => p,
                    None => a.param(parent, ParamRole::Parent { key: fk }),
                }),
            };
            a.effects.push(Effect::Assign {
                var,
                param: c,
                rhs,
                site,
            });
        }
    }

    /// Which key joins a row of `fk`'s parent that the use case writes to a child it writes
    /// (#76): `Join::Key(fk)` for the one key from that child to that parent, or, among
    /// several, when a `.by` of the use case's `holds` guards (the body's or an `.along` hop's)
    /// or of its copies names `fk`, or a `links` of the use case follows it, so `.by` names the
    /// same row as elsewhere; `Join::Key` of another key when a `.by` names only others;
    /// `Join::Ambiguous` when no `.by` names any of them, which is `AmbiguousOwnerLink` as for
    /// any other join.
    fn written_parent_key(&self, uc: UseCaseKey, fk: FkId) -> Join {
        let model = self.model;
        let key = model.foreign_key(fk);
        let parallel: Vec<FkId> = model
            .foreign_keys()
            .filter(|(_, k)| (k.child, k.parent) == (key.child, key.parent))
            .map(|(id, _)| id)
            .collect();
        if parallel.len() == 1 {
            return Join::Key(fk);
        }
        let linked = model
            .links
            .iter()
            .filter(|l| l.origin == uc)
            .filter_map(|l| l.key)
            .filter(|id| parallel.contains(id));
        let guards = applied_bodies(model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .flat_map(|b| {
                let hops = match &b.body.along {
                    CrossConstraintScope::RelationPath { by, .. } => by.as_slice(),
                    CrossConstraintScope::GlobalProduct => &[],
                };
                b.body.by.iter().chain(hops)
            });
        let copies = model
            .column_effects
            .iter()
            .filter(|ce| ce.origin == uc && matches!(ce.value, SetValue::Copy(_)))
            .flat_map(|ce| ce.by.iter());
        let named: Vec<FkId> = guards
            .chain(copies)
            .filter_map(|role| {
                parallel
                    .iter()
                    .copied()
                    .find(|id| model.key_named(model.foreign_key(*id), role))
            })
            .chain(linked)
            .collect();
        match named.iter().find(|id| **id == fk).or(named.first()) {
            Some(id) => Join::Key(*id),
            None => Join::Ambiguous(parallel),
        }
    }

    /// `AmbiguousOwnerLink` for a parent the use case writes that several keys join to a
    /// child it writes, none of which a `.by` of the use case names. The statement is the
    /// first `holds` body applied to the use case that compares the parent, else its first
    /// copy from the parent, with `.role(<first role>)` as the rewrite; a use case with neither
    /// is told where a `.by` can go.
    fn ambiguous_written_parent(&mut self, uc: UseCaseKey, fk: FkId, candidates: &[FkId]) {
        let model = self.model;
        let key = model.foreign_key(fk);
        let (child, parent) = (key.child, key.parent);
        let holds = applied_bodies(model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .find(|b| {
                b.body
                    .conds
                    .iter()
                    .flat_map(preds::cond_entities)
                    .any(|ek| ek == parent)
            })
            .map(|b| {
                (
                    format!("holds({}, ...)", model.conditions[b.cond].id),
                    Site::new(ModelRef::Condition(b.cond), &b.body.site),
                )
            });
        let copy = model
            .column_effects
            .iter()
            .enumerate()
            .find_map(|(idx, ce)| {
                let SetValue::Copy(source) = &ce.value else {
                    return None;
                };
                (ce.origin == uc && ce.entity == child && source.entity == parent).then(|| {
                    (
                        format!(
                            "sets({}, {}.{} == {}.{})",
                            model.use_cases[uc].id,
                            model.entities[child].id,
                            ce.column,
                            model.entities[parent].id,
                            source.column
                        ),
                        Site::new(ModelRef::ColumnEffect(idx), &ce.site),
                    )
                })
            });
        let diag = match holds.or(copy) {
            Some((statement, site)) => {
                preds::ambiguous_owner_link(model, &statement, child, parent, candidates, &site)
            }
            None => {
                let uc_id = &model.use_cases[uc].id;
                let roles: Vec<&str> = candidates
                    .iter()
                    .map(|id| model.key_role(model.foreign_key(*id)))
                    .collect();
                StateDiag::AmbiguousOwnerLink {
                    statement: format!("usecase {uc_id}"),
                    a: model.entities[child].id.clone(),
                    b: model.entities[parent].id.clone(),
                    roles: roles.join(", "),
                    rewrite: format!(
                        ".role({}) on a holds guard applied to {uc_id} that compares {}, or on a copy from {} in it",
                        roles[0], model.entities[parent].id, model.entities[parent].id
                    ),
                    site: Site::new(
                        ModelRef::UseCase(uc),
                        &model.decl_sites.located(Kind::UseCase, uc_id),
                    ),
                }
            }
        };
        self.diag(vec![child], diag);
    }
}

impl Builder<'_> {
    /// `UnjoinedWrite` for each pair of rows a step of `uc` writes or binds that no owner
    /// guard or assignment of the step joins, directly or through other rows of it, while a
    /// key path through entities the step does not bind relates them: one run may then write
    /// rows of the two entities that belong to different rows of the entities between. A
    /// pair with a direct key is left to the owner binding, and a write marked `.apart()` is
    /// meant to be a row of its own.
    pub(super) fn unjoined_writes(&mut self, uc: UseCaseKey, actions: &[Action]) {
        let model = self.model;
        let apart: Vec<EntityKey> = model
            .apart_writes
            .iter()
            .filter(|(u, _)| *u == uc)
            .map(|(_, e)| *e)
            .collect();
        let keys: std::collections::HashMap<_, EntityKey> =
            self.ent_of.iter().map(|(k, e)| (*e, *k)).collect();
        let entity_key = |e| keys[&e];
        let mut reported: Vec<(EntityKey, EntityKey)> = Vec::new();
        let mut found = Vec::new();
        for action in actions {
            let n = action.params.len();
            let mut root: Vec<usize> = (0..n).collect();
            fn find(root: &mut [usize], i: usize) -> usize {
                let mut at = i;
                while root[at] != at {
                    root[at] = root[root[at]];
                    at = root[at];
                }
                at
            }
            let mut union = |a: u8, b: u8| {
                let (ra, rb) = (find(&mut root, a.into()), find(&mut root, b.into()));
                root[ra] = rb;
            };
            let is_owner =
                |var: VarId| matches!(self.ts.vars[var.index()].kind, VarKind::Owner { .. });
            for effect in &action.effects {
                if let Effect::Assign {
                    var,
                    param,
                    rhs: Rhs::Param(p),
                    ..
                } = effect
                {
                    if is_owner(*var) {
                        union(*param, *p);
                    }
                }
            }
            let mut joins = Vec::new();
            for guard in &action.guards {
                owner_joins(&guard.pred, &is_owner, &mut joins);
            }
            for (a, b) in joins {
                union(a, b);
            }
            let mut components: Vec<(usize, Vec<EntityKey>)> = Vec::new();
            for (i, p) in action.params.iter().enumerate() {
                let ek = entity_key(p.entity);
                if p.role.is_written() && apart.contains(&ek) {
                    continue;
                }
                let r = find(&mut root, i);
                match components.iter_mut().find(|(c, _)| *c == r) {
                    Some((_, ents)) if !ents.contains(&ek) => ents.push(ek),
                    Some(_) => {}
                    None => components.push((r, vec![ek])),
                }
            }
            let bound: Vec<EntityKey> = components.iter().flat_map(|(_, e)| e.clone()).collect();
            for i in 0..components.len() {
                for j in i + 1..components.len() {
                    let path = components[j]
                        .1
                        .iter()
                        .filter_map(|&to| model.key_path(&components[i].1, to))
                        .min_by_key(Vec::len);
                    // A path of two is a direct key, left to the owner binding; one of one is
                    // two rows of the same entity.
                    let Some(path) = path.filter(|p| p.len() > 2) else {
                        continue;
                    };
                    let (a, b) = (path[0], path[path.len() - 1]);
                    let between = &path[1..path.len() - 1];
                    if between.iter().any(|e| bound.contains(e)) {
                        continue;
                    }
                    let pair = if model.entities[a].id <= model.entities[b].id {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    if reported.contains(&pair) {
                        continue;
                    }
                    reported.push(pair);
                    let path = if pair.0 == a {
                        path.clone()
                    } else {
                        path.iter().rev().copied().collect()
                    };
                    found.push((
                        vec![pair.0],
                        StateDiag::UnjoinedWrite {
                            usecase: model.use_cases[uc].id.clone(),
                            a: model.entities[pair.0].id.clone(),
                            b: model.entities[pair.1].id.clone(),
                            path: path.iter().map(|e| model.entities[*e].id.clone()).collect(),
                            site: action.site.clone(),
                        },
                    ));
                }
            }
        }
        for (entities, diag) in found {
            self.diag(entities, diag);
        }
    }
}

/// The param pairs an owner comparison `owner(c) = index(p)` joins, under conjunctions only.
fn owner_joins(pred: &Pred, is_owner: &impl Fn(VarId) -> bool, out: &mut Vec<(u8, u8)>) {
    match pred {
        Pred::And(preds) => {
            for p in preds {
                owner_joins(p, is_owner, out);
            }
        }
        Pred::Cmp(
            Term::Slot {
                var,
                at: At::Param(c),
            },
            CmpOp::Eq,
            Term::Index(At::Param(p)),
        )
        | Pred::Cmp(
            Term::Index(At::Param(p)),
            CmpOp::Eq,
            Term::Slot {
                var,
                at: At::Param(c),
            },
        ) if is_owner(*var) => out.push((*c, *p)),
        _ => {}
    }
}
