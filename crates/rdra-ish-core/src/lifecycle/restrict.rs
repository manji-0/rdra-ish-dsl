//! The subsystem over a set of vars (`restrict_to`), and the renumbering of params and ids
//! it needs.

use std::collections::BTreeSet;

use super::cone::{named_params, VarSet};
use super::types::{
    ActId, Action, AfterAssert, AfterStep, At, Domain, Effect, EntId, EntityDecl, Guard, Invariant,
    Param, ParamRole, Pred, Premise, Property, Reach, Rhs, Term, VarDecl, VarId, VarKind,
};
use super::TransitionSystem;

impl TransitionSystem {
    pub(crate) fn var_entity(&self, var: VarId) -> Option<EntId> {
        self.var(var).kind.entity()
    }

    /// The system over `vars` only. An action stays when it writes a var in `vars` or is in
    /// `keep`; it keeps every guard and param, except a copy's guard on a var outside `vars`,
    /// a guard on a var outside `vars` (a mark a cone left out), and a `Triggered` param
    /// whose mark is outside `vars`, which goes with the guards that name it; and it keeps
    /// only its effects on `vars`. Invariants, asserts, and properties stay when everything
    /// they read is in `vars`. For a closed `vars` (see `cone`) its reachable states are the
    /// projection of this system's.
    ///
    /// An owner var in `vars` whose parent's existence is not (the sub-lifecycle of
    /// `TransitionSystem::reduced` may hold a child of an entity outside it) keeps its domain
    /// through a stub of the parent: an entity of its existence alone, after every var of
    /// `vars`, which no action here writes.
    pub fn restrict_to(&self, vars: &VarSet, keep: &BTreeSet<ActId>) -> TransitionSystem {
        self.restrict_to_mapped(vars, keep, true).0
    }

    /// `restrict_to`, with the map from this system's items to the restricted system's. With
    /// `havoc` off, a `Triggered` param whose mark is outside `vars` is treated like any
    /// other param (the extended sub-lifecycle of `reduced` reads the mark as an input, or
    /// folds it into a bit). A param on an entity the system neither keeps nor stubs goes
    /// with the guards that name it either way: a rise of that sub-lifecycle binds rows of T
    /// it does not hold.
    pub(crate) fn restrict_to_mapped(
        &self,
        vars: &VarSet,
        keep: &BTreeSet<ActId>,
        havoc: bool,
    ) -> (TransitionSystem, Remap) {
        let mut map = Remap {
            ent: vec![None; self.entities.len()],
            var: vec![None; self.vars.len()],
            act: vec![None; self.actions.len()],
            params: vec![None; self.actions.len()],
        };
        let kept_entities: Vec<usize> = (0..self.entities.len())
            .filter(|&i| vars.contains(&self.entities[i].exists))
            .collect();
        for (n, &i) in kept_entities.iter().enumerate() {
            map.ent[i] = Some(EntId::at(n));
        }
        let mut stubs: Vec<usize> = Vec::new();
        for &v in vars {
            if let VarKind::Owner { parent, .. } = self.vars[v.index()].kind {
                if map.ent[parent.index()].is_none() {
                    map.ent[parent.index()] = Some(EntId::at(kept_entities.len() + stubs.len()));
                    stubs.push(parent.index());
                }
            }
        }
        let mut out = TransitionSystem::empty(self.bounds.clone());
        out.api_ids = self.api_ids.clone();
        for &v in vars {
            map.var[v.index()] = Some(VarId::at(out.vars.len()));
            let mut decl = self.vars[v.index()].clone();
            decl.kind = map.kind(&decl.kind);
            if let Domain::Instance { parent, optional } = decl.domain {
                decl.domain = Domain::Instance {
                    parent: map.e(parent),
                    optional,
                };
            }
            if decl.kind == VarKind::Now {
                out.now = map.var[v.index()];
            }
            out.vars.push(decl);
        }
        for &i in &kept_entities {
            let mut decl = self.entities[i].clone();
            decl.exists = map.v(decl.exists);
            decl.columns = decl
                .columns
                .iter()
                .filter_map(|v| map.var[v.index()])
                .collect();
            decl.owners = decl
                .owners
                .iter()
                .filter_map(|v| map.var[v.index()])
                .collect();
            out.entities.push(decl);
        }
        for &i in &stubs {
            let parent = &self.entities[i];
            let exists = VarId::at(out.vars.len());
            out.vars.push(VarDecl {
                kind: VarKind::Exists(map.e(EntId::at(i))),
                ..self.vars[parent.exists.index()].clone()
            });
            out.entities.push(EntityDecl {
                exists,
                columns: Vec::new(),
                owners: Vec::new(),
                ..parent.clone()
            });
        }
        for (i, action) in self.actions.iter().enumerate() {
            let effects: Vec<Effect> = action
                .effects
                .iter()
                .filter(|eff| {
                    self.effect_writes(action, eff)
                        .iter()
                        .any(|v| vars.contains(v))
                })
                .cloned()
                .collect();
            if effects.is_empty() && !keep.contains(&ActId::at(i)) {
                continue;
            }
            let mut action = action.clone();
            action.effects = effects;
            let mut havoced: Vec<u8> = self
                .triggered_params(&action)
                .into_iter()
                .filter(|(_, mark)| havoc && !vars.contains(mark))
                .map(|(p, _)| p)
                .collect();
            havoced.extend(
                action
                    .params
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        p.role != ParamRole::New && map.ent[p.entity.index()].is_none()
                    })
                    .map(|(i, _)| i as u8),
            );
            action.guards.retain(|g| {
                if g.copy_target().is_some_and(|t| !vars.contains(&t)) {
                    return false;
                }
                let mut reads = VarSet::new();
                self.pred_reads(&g.pred, &mut reads);
                reads.is_subset(vars) && !named_params(&g.pred).iter().any(|p| havoced.contains(p))
            });
            let (action, params) = drop_params(action, |p, param| {
                (param.role == ParamRole::New && !vars.contains(&self.entity(param.entity).exists))
                    || havoced.contains(&p)
            });
            map.act[i] = Some(ActId::at(out.actions.len()));
            map.params[i] = params;
            out.actions.push(map.action(&action));
        }
        let reads_inside = |binders: &[EntId], preds: &[&Pred]| {
            self.check_reads(binders, preds.iter().copied())
                .is_subset(vars)
        };
        for inv in &self.invariants {
            if reads_inside(&inv.binders, &[&inv.body]) {
                out.invariants.push(Invariant {
                    binders: inv.binders.iter().map(|e| map.e(*e)).collect(),
                    body: map.pred(&inv.body),
                    premises: inv
                        .premises
                        .iter()
                        .map(|p| Premise {
                            pred: map.pred(&p.pred),
                            text: p.text.clone(),
                        })
                        .collect(),
                    ..inv.clone()
                });
            }
        }
        for a in &self.after {
            let steps: Vec<AfterStep> = a
                .steps
                .iter()
                .filter(|step| reads_inside(&step.binders, &[&step.post]))
                .filter_map(|step| {
                    let i = step.act.index();
                    let mut post = step.post.clone();
                    if let Some(index) = &map.params[i] {
                        renumber_pred(&mut post, &renumber_by(index));
                    }
                    Some(AfterStep {
                        act: map.act[i]?,
                        binders: step.binders.iter().map(|e| map.e(*e)).collect(),
                        post: map.pred(&post),
                    })
                })
                .collect();
            if steps.len() == a.steps.len() {
                out.after.push(AfterAssert { steps, ..a.clone() });
            }
        }
        for p in &self.properties {
            if self.property_reads(p).is_subset(vars) {
                out.properties.push(Property {
                    binders: p.binders.iter().map(|e| map.e(*e)).collect(),
                    link: map.pred(&p.link),
                    formula: p.formula.map(|x| map.pred(x), |t| map.term(t)),
                    ..p.clone()
                });
            }
        }
        for r in &self.reach {
            let preds: Vec<&Pred> = r.preds().collect();
            if reads_inside(&r.binders, &preds) {
                out.reach.push(Reach {
                    binders: r.binders.iter().map(|e| map.e(*e)).collect(),
                    link: map.pred(&r.link),
                    goal: map.pred(&r.goal),
                    ..r.clone()
                });
            }
        }
        out.layout();
        (out, map)
    }
}

/// Drops each param `drop` accepts (a `New` param whose entity is outside the system, with
/// nothing left to point at it: outside a target's cone an entity never runs out of slots;
/// a `Triggered` param whose mark is outside it; or a param on an entity outside it) and
/// renumbers the rest; the returned map takes an old param index to its new one, and is
/// `None` when nothing was dropped.
fn drop_params(
    mut action: Action,
    drop: impl Fn(u8, &Param) -> bool,
) -> (Action, Option<Vec<Option<u8>>>) {
    let mut index: Vec<Option<u8>> = Vec::with_capacity(action.params.len());
    let mut params = Vec::new();
    for (i, p) in action.params.iter().enumerate() {
        if drop(i as u8, p) {
            index.push(None);
        } else {
            index.push(Some(u8::try_from(params.len()).expect("params fit in u8")));
            params.push(*p);
        }
    }
    if params.len() == action.params.len() {
        return (action, None);
    }
    action.params = params;
    {
        let at = renumber_by(&index);
        for g in &mut action.guards {
            renumber_pred(&mut g.pred, &at);
        }
        for eff in &mut action.effects {
            match eff {
                Effect::Create { param } | Effect::Delete { param } => at(param),
                Effect::Assign { param, rhs, .. } => {
                    at(param);
                    if let Rhs::Param(p) = rhs {
                        at(p);
                    }
                }
                Effect::Transition { param, .. } | Effect::Choose { param, .. } => at(param),
                Effect::Tick => {}
            }
        }
    }
    (action, Some(index))
}

pub(crate) fn renumber_by(index: &[Option<u8>]) -> impl Fn(&mut u8) + '_ {
    |p: &mut u8| *p = index[usize::from(*p)].expect("a dropped param is unreferenced")
}

pub(crate) fn renumber_pred(pred: &mut Pred, at: &impl Fn(&mut u8)) {
    let term = |t: &mut Term| {
        if let Term::Slot {
            at: At::Param(p) | At::Next(p),
            ..
        }
        | Term::Index(At::Param(p) | At::Next(p)) = t
        {
            at(p);
        }
    };
    match pred {
        Pred::True | Pred::False => {}
        Pred::Cmp(a, _, b) => {
            term(a);
            term(b);
        }
        Pred::In(a, vs) => {
            term(a);
            vs.iter_mut().for_each(term);
        }
        Pred::Not(p) => renumber_pred(p, at),
        Pred::And(ps) | Pred::Or(ps) => ps.iter_mut().for_each(|p| renumber_pred(p, at)),
        Pred::Forall { body, .. } | Pred::Exists { body, .. } | Pred::Count { body, .. } => {
            renumber_pred(body, at)
        }
    }
}

/// This system's ids to the restricted system's.
pub(crate) struct Remap {
    ent: Vec<Option<EntId>>,
    var: Vec<Option<VarId>>,
    pub(crate) act: Vec<Option<ActId>>,
    /// Per source action, old param index to new, when `drop_params` dropped one.
    pub(crate) params: Vec<Option<Vec<Option<u8>>>>,
}

impl Remap {
    /// The restricted system's id of an entity, when it kept it.
    pub(crate) fn ent_of(&self, e: EntId) -> Option<EntId> {
        self.ent[e.index()]
    }

    /// The restricted system's id of a var, when it kept it.
    pub(crate) fn var_of(&self, v: VarId) -> Option<VarId> {
        self.var[v.index()]
    }

    pub(crate) fn e(&self, e: EntId) -> EntId {
        self.ent[e.index()].expect("entity outside the component")
    }

    pub(crate) fn v(&self, v: VarId) -> VarId {
        self.var[v.index()].expect("var outside the component")
    }

    fn kind(&self, kind: &VarKind) -> VarKind {
        match kind {
            VarKind::Exists(e) => VarKind::Exists(self.e(*e)),
            VarKind::Column {
                entity,
                column,
                lifecycle,
                variation,
            } => VarKind::Column {
                entity: self.e(*entity),
                column: column.clone(),
                lifecycle: *lifecycle,
                variation: *variation,
            },
            VarKind::Prop { entity, key } => VarKind::Prop {
                entity: self.e(*entity),
                key: key.clone(),
            },
            VarKind::Run { entity, usecase } => VarKind::Run {
                entity: self.e(*entity),
                usecase: usecase.clone(),
            },
            VarKind::Raised { entity, event } => VarKind::Raised {
                entity: self.e(*entity),
                event: event.clone(),
            },
            VarKind::Owner {
                child,
                parent,
                fk,
                role,
            } => VarKind::Owner {
                child: self.e(*child),
                parent: self.e(*parent),
                fk: *fk,
                role: role.clone(),
            },
            VarKind::Now => VarKind::Now,
        }
    }

    fn term(&self, t: &Term) -> Term {
        match t {
            Term::Slot { var, at } => Term::Slot {
                var: self.v(*var),
                at: *at,
            },
            other => other.clone(),
        }
    }

    pub(crate) fn pred(&self, p: &Pred) -> Pred {
        match p {
            Pred::True => Pred::True,
            Pred::False => Pred::False,
            Pred::Cmp(a, op, b) => Pred::Cmp(self.term(a), *op, self.term(b)),
            Pred::In(a, vs) => Pred::In(self.term(a), vs.iter().map(|v| self.term(v)).collect()),
            Pred::Not(x) => Pred::Not(Box::new(self.pred(x))),
            Pred::And(xs) => Pred::And(xs.iter().map(|x| self.pred(x)).collect()),
            Pred::Or(xs) => Pred::Or(xs.iter().map(|x| self.pred(x)).collect()),
            Pred::Forall { entity, body } => Pred::Forall {
                entity: self.e(*entity),
                body: Box::new(self.pred(body)),
            },
            Pred::Exists { entity, body } => Pred::Exists {
                entity: self.e(*entity),
                body: Box::new(self.pred(body)),
            },
            Pred::Count {
                entity,
                body,
                op,
                n,
            } => Pred::Count {
                entity: self.e(*entity),
                body: Box::new(self.pred(body)),
                op: *op,
                n: *n,
            },
        }
    }

    fn action(&self, a: &Action) -> Action {
        let mut a = a.clone();
        for p in &mut a.params {
            p.entity = self.e(p.entity);
        }
        a.guards = a
            .guards
            .iter()
            .map(|g| Guard {
                pred: self.pred(&g.pred),
                ..g.clone()
            })
            .collect();
        for eff in &mut a.effects {
            match eff {
                Effect::Assign { var, .. }
                | Effect::Transition { var, .. }
                | Effect::Choose { var, .. } => *var = self.v(*var),
                Effect::Create { .. } | Effect::Delete { .. } | Effect::Tick => {}
            }
        }
        a
    }
}
