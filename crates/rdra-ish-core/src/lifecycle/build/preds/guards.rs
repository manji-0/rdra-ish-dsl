//! The guards a `holds` condition body gives an action: one per body, with its `.along` path.

use std::collections::HashSet;

use super::{at_of, cond_entities, implies, Binding};
use crate::analysis::{applied_bodies, AppliedBody};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, Guard, GuardWhy, ModelRef, Param, ParamRole, Pred, Site};
use crate::model::{
    ApiKey, ConditionKey, CrossConstraintScope, EntityKey, FkId, HoldsBody, Join, QuantifierKind,
    RelatedQuantifier, UseCaseKey,
};
use crate::tx::{UcWrite, WriteKind};

/// What an entity of a `holds` body's `.along` path is to the guard ([`Builder::path_roles`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathRole {
    /// A row the action binds.
    Bound,
    /// A row some present instance supplies: the body compares it, or nothing counts it.
    Witness,
    /// The row a `.has`/`.none` counts along the path.
    Counted,
    /// A row of a counted entity's chain, between its anchor and it.
    Hop,
}

impl PathRole {
    /// Bound in the guard's outer scope, where a counted entity's chain starts.
    fn is_outer(self) -> bool {
        matches!(self, PathRole::Bound | PathRole::Witness)
    }
}

/// The part of an `.along` path a `.has`/`.none` counts along ([`Builder::quantified_along`]):
/// from its anchor to the counted entity, with the keys of its hops, and what an unbindable
/// hop is reported on.
pub(super) struct Chain<'a> {
    pub(super) segment: &'a [EntityKey],
    pub(super) keys: &'a [Option<FkId>],
    pub(super) owner: PathOwner<'a>,
    pub(super) site: &'a Site,
}

/// The statement an `.along` path belongs to, which a hop no key binds is reported on
/// ([`Builder::path_links`]).
pub(super) enum PathOwner<'a> {
    /// The `holds` body of `ck` applied to `uc`, comparing `read`: `UnbindableCondition`.
    Body {
        uc: UseCaseKey,
        ck: ConditionKey,
        read: &'a [EntityKey],
    },
    /// A rule over `scope`: `UnbindableScope`.
    Rule {
        constraint: &'a str,
        scope: &'a [EntityKey],
    },
}

impl<'m> Builder<'m> {
    // ── R-guard: condition bodies ────────────────────────────────────────────

    /// One guard per `holds` body the action carries (`Builder::step_bodies`), each read on
    /// the action's pre-state. An entity the action binds reads its param: on the pre-state,
    /// or in the successor (`At::Next`) when the action creates it, so the body constrains
    /// what is created. Any other entity, an entity a later step of the use case writes
    /// included, is `Exists` over present instances of the pre-state, one witness per entity
    /// shared by the body's atoms, linked by a direct owner var (one hop) to each param or
    /// other witness it relates to; with no such relate it is independent.
    pub(in crate::lifecycle::build) fn condition_guards(
        &mut self,
        bodies: &[AppliedBody<'m>],
        params: &[Param],
    ) -> Vec<Guard> {
        let mut guards = Vec::new();
        for b in bodies {
            let site = Site::new(ModelRef::Condition(b.cond), &b.body.site);
            let pred = self.body_pred(b.usecase, b.cond, params, &site);
            guards.push(Guard {
                pred,
                why: GuardWhy::Condition {
                    cond: b.cond,
                    later: self.later_reads.get(&(b.usecase, b.cond)).copied(),
                },
                site,
            });
        }
        guards
    }

    /// `GuardReadsLaterStep` for each `holds` body that step 1 of the split use case `uc`
    /// carries and that compares an entity step 1 does not write and a later step creates:
    /// the guard is read before step 1, when the run has no row of it yet, so it reads rows
    /// that are not the run's. `groups` lists each step's writes, in transaction order.
    pub(in crate::lifecycle::build) fn later_step_reads(
        &mut self,
        uc: UseCaseKey,
        groups: &[Vec<UcWrite>],
        written_apis: &HashSet<ApiKey>,
    ) {
        let model = self.model;
        let Some(first) = groups.first() else {
            return;
        };
        for b in self.step_bodies(uc, first, 0, written_apis) {
            let later = b
                .body
                .conds
                .iter()
                .flat_map(cond_entities)
                .filter(|ek| !first.iter().any(|w| w.entity == *ek))
                .find_map(|ek| {
                    let (step, writes) =
                        groups.iter().enumerate().skip(1).find(|(_, writes)| {
                            writes
                                .iter()
                                .any(|w| w.entity == ek && w.kind == WriteKind::Creates)
                        })?;
                    let mut apis: Vec<String> = Vec::new();
                    for api in writes
                        .iter()
                        .filter(|w| w.entity == ek && w.kind == WriteKind::Creates)
                        .filter_map(|w| w.via_api)
                    {
                        let id = model.apis[api].id.clone();
                        if !apis.contains(&id) {
                            apis.push(id);
                        }
                    }
                    Some((ek, step + 1, apis))
                });
            let Some((ek, step, apis)) = later else {
                continue;
            };
            self.later_reads.insert((uc, b.cond), ek);
            self.diag(
                vec![ek],
                StateDiag::GuardReadsLaterStep {
                    condition: model.conditions[b.cond].id.clone(),
                    usecase: model.use_cases[uc].id.clone(),
                    entity: model.entities[ek].id.clone(),
                    step,
                    apis,
                    site: Site::new(ModelRef::Condition(b.cond), &b.body.site),
                },
            );
        }
    }

    /// The `holds` body of `ck` over an action's params, as [`Self::condition_guards`] reads it.
    /// With `.along(..)`, the path's keys join its rows and only the hops link: a path entity
    /// the action does not bind is a witness, or, when a `.has`/`.none` counts it and the body
    /// does not compare it, the quantifier's row, counted along the path from the nearest
    /// entity before it that the action binds or the body compares ([`Self::path_roles`]).
    /// Without a path, each witness links to each bound entity or other witness that one key
    /// joins it to.
    pub(in crate::lifecycle::build) fn body_pred(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        params: &[Param],
        site: &Site,
    ) -> Pred {
        let model = self.model;
        let Some(body) = model.conditions[ck].body.as_ref() else {
            return Pred::True;
        };
        let statement = format!("holds({}, ...)", model.conditions[ck].id);
        let body_site = Site::new(ModelRef::Condition(ck), &body.site);
        let mut binding: Binding = Vec::new();
        for e in params.iter().map(|p| p.entity) {
            let ek = self.ts.entities[e.index()].key;
            if at_of(&binding, ek).is_some() {
                continue;
            }
            let Some(i) = self.read_param(params, e, &body.by) else {
                continue;
            };
            let at = match params[usize::from(i)].role {
                ParamRole::New => At::Next(i),
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Earlier { .. } => {
                    At::Param(i)
                }
            };
            binding.push((ek, at));
        }
        let read: Vec<EntityKey> = body.conds.iter().flat_map(cond_entities).collect();
        let (path, keys): (&[EntityKey], &[Option<FkId>]) = match &body.along {
            CrossConstraintScope::RelationPath { path, links, .. } => (path, links),
            CrossConstraintScope::GlobalProduct => (&[], &[]),
        };
        let roles = self.path_roles(uc, ck, body, path, &binding, &read, &body_site);
        let mut witnesses = Vec::new();
        let outer = path
            .iter()
            .zip(&roles)
            .filter(|(_, role)| **role == PathRole::Witness)
            .map(|(ek, _)| *ek)
            .chain(read.iter().copied());
        for ek in outer {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(witnesses.len() as u8)));
                witnesses.push(self.ent_of[&ek]);
            }
        }
        let witness = |(_, at): (EntityKey, At)| matches!(at, At::Bound(_));
        let mut parts = Vec::new();
        let owner = PathOwner::Body {
            uc,
            ck,
            read: &read,
        };
        // The pairs of entities a key joins, linked or reported as ambiguous.
        let mut joined: Vec<(EntityKey, EntityKey)> = Vec::new();
        if path.is_empty() {
            for (i, &x) in binding.iter().enumerate() {
                for &y in &binding[i + 1..] {
                    if !(witness(x) || witness(y)) {
                        continue;
                    }
                    if model.join_key(x.0, y.0, &body.by) != Join::Unrelated {
                        joined.push((x.0, y.0));
                    }
                    parts.extend(self.owner_link(x, y, &body.by, &statement, &body_site));
                }
            }
        } else {
            // The hops between two entities of the outer scope link there; a hop into a
            // counted entity's chain links inside its quantifier.
            let mut from = 0;
            while from < path.len() {
                let mut to = from + 1;
                while to < path.len() && roles[from].is_outer() && roles[to].is_outer() {
                    to += 1;
                }
                if to - from >= 2 {
                    let (run, hops) = (&path[from..to], &keys[from..to - 1]);
                    parts.extend(self.path_links(run, hops, &binding, &owner, &body_site));
                }
                from = to;
            }
            if path.len() < 2 {
                parts.extend(self.path_links(path, keys, &binding, &owner, &body_site));
            }
            joined.extend(path.windows(2).map(|hop| (hop[0], hop[1])));
        }
        self.unjoined_reads(uc, ck, &binding, &joined, &read, &body_site);
        parts.extend(
            body.conds
                .iter()
                .map(|c| self.cross_atom(c, &binding, site)),
        );
        let depth = witnesses.len() as u8;
        for q in &body.quantifiers {
            let counted = path
                .iter()
                .position(|ek| *ek == q.related)
                .filter(|&r| roles[r] == PathRole::Counted);
            let pred = match counted {
                Some(r) => {
                    let anchor = (0..r)
                        .rev()
                        .find(|&i| roles[i].is_outer())
                        .expect("a counted entity follows an anchor");
                    let chain = Chain {
                        segment: &path[anchor..=r],
                        keys: &keys[anchor..r],
                        owner: PathOwner::Body {
                            uc,
                            ck,
                            read: &read,
                        },
                        site: &body_site,
                    };
                    self.quantified_along(q, &binding, depth, chain, site)
                }
                None => self.quantified(q, &binding, depth, site),
            };
            parts.push(pred);
        }
        witnesses
            .into_iter()
            .rev()
            .fold(Pred::And(parts), |inner, entity| Pred::Exists {
                entity,
                body: Box::new(inner),
            })
    }

    /// The role of each entity of a `holds` body's `.along` path on an action that binds
    /// `binding` (its params), where the body compares `read`. A counted entity's chain runs
    /// from the nearest bound or compared entity before it; a path that names a counted
    /// entity before any such anchor, or goes on past a counted entity to an entity nothing
    /// counts, is `UnbindableCondition`, and the entity is read as a witness.
    #[allow(clippy::too_many_arguments)]
    fn path_roles(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        body: &HoldsBody,
        path: &[EntityKey],
        binding: &Binding,
        read: &[EntityKey],
        site: &Site,
    ) -> Vec<PathRole> {
        let model = self.model;
        let outer = |ek: EntityKey| at_of(binding, ek).is_some() || read.contains(&ek);
        let counting = |ek: EntityKey| {
            body.quantifiers
                .iter()
                .find(|q| q.related == ek)
                .map(|q| match q.kind {
                    QuantifierKind::Has => "has",
                    QuantifierKind::None => "none",
                })
        };
        let id = |ek: EntityKey| model.entities[ek].id.as_str();
        let mut roles: Vec<PathRole> = Vec::with_capacity(path.len());
        for (i, &ek) in path.iter().enumerate() {
            let anchor = (0..i).rev().find(|&j| outer(path[j]));
            let run_end = (i + 1..path.len())
                .find(|&j| outer(path[j]))
                .unwrap_or(path.len());
            let last_counted = (anchor.map_or(0, |a| a + 1)..i)
                .rev()
                .find(|&j| roles[j] == PathRole::Counted);
            let counted_after = (i + 1..run_end).any(|j| counting(path[j]).is_some());
            let reason = if at_of(binding, ek).is_some() {
                roles.push(PathRole::Bound);
                continue;
            } else if read.contains(&ek) {
                roles.push(PathRole::Witness);
                let Some(c) = last_counted else {
                    continue;
                };
                format!(
                    "along(..) joins {}, which the body compares, through {}, which `.{}` counts; put {} before {}",
                    id(ek), id(path[c]), counting(path[c]).unwrap_or("none"), id(ek), id(path[c])
                )
            } else if let Some(kind) = counting(ek) {
                if anchor.is_some() {
                    roles.push(PathRole::Counted);
                    continue;
                }
                roles.push(PathRole::Witness);
                format!(
                    "along(..) names {}, which `.{kind}` counts, before any entity {} binds or the body compares; start the path at one",
                    id(ek),
                    model.use_cases[uc].id
                )
            } else if counted_after && anchor.is_some() {
                roles.push(PathRole::Hop);
                continue;
            } else {
                roles.push(PathRole::Witness);
                let Some(c) = last_counted else {
                    continue;
                };
                format!(
                    "along(..) goes on past {}, which `.{}` counts, to {}, which nothing reads; end the path at {}",
                    id(path[c]), counting(path[c]).unwrap_or("none"), id(ek), id(path[c])
                )
            };
            self.unbindable(uc, ck, vec![ek], reason, site);
        }
        roles
    }

    /// `UnbindableCondition` on `entities`: the body of `ck` applied to `uc` guards nothing
    /// for `reason`.
    fn unbindable(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        entities: Vec<EntityKey>,
        reason: String,
        site: &Site,
    ) {
        let model = self.model;
        self.diag(
            entities,
            StateDiag::UnbindableCondition {
                condition: model.conditions[ck].id.clone(),
                usecase: model.use_cases[uc].id.clone(),
                reason,
                site: site.clone(),
            },
        );
    }

    /// [`Self::hop_links`] over `path`, or the error it reports on `owner` and no link.
    fn path_links(
        &mut self,
        path: &[EntityKey],
        keys: &[Option<FkId>],
        binding: &Binding,
        owner: &PathOwner<'_>,
        site: &Site,
    ) -> Vec<Pred> {
        match self.hop_links(path, keys, binding) {
            Ok(links) => links,
            Err(reason) => {
                match *owner {
                    PathOwner::Body { uc, ck, read } => {
                        self.unbindable(uc, ck, read.to_vec(), reason, site);
                    }
                    PathOwner::Rule { constraint, scope } => {
                        self.unbindable_scope(scope, constraint, reason, site);
                    }
                }
                Vec::new()
            }
        }
    }

    /// [`Self::quantified`] along a path: `chain.segment` runs from an entity `binding` binds
    /// to `q.related`, whose entities after the first are bound at `depth`, `depth + 1`, .. in
    /// path order. Every row of each entity between, and of `q.related`, that the hops' keys
    /// chain to the anchor's row is counted: `Forall` for `.none`, so no chain reaches a
    /// matching row, and `Exists` for `.has`, so some chain does.
    pub(super) fn quantified_along(
        &mut self,
        q: &RelatedQuantifier,
        binding: &Binding,
        depth: u8,
        chain: Chain<'_>,
        site: &Site,
    ) -> Pred {
        let Chain {
            segment,
            keys,
            owner,
            site: path_site,
        } = chain;
        let rows = &segment[1..];
        let mut inner: Binding = binding
            .iter()
            .filter(|(ek, _)| !rows.contains(ek))
            .copied()
            .collect();
        for (i, &ek) in rows.iter().enumerate() {
            inner.push((ek, At::Bound(depth + i as u8)));
        }
        let links = self.path_links(segment, keys, &inner, &owner, path_site);
        let conds = self.cross_all(&q.conditions, &inner, site);
        let link = Pred::And(links);
        let body = match q.kind {
            QuantifierKind::Has => Pred::And(vec![link, conds]),
            QuantifierKind::None => implies(link, Pred::Not(Box::new(conds))),
        };
        rows.iter().rev().fold(body, |body, ek| {
            let entity = self.ent_of[ek];
            let body = Box::new(body);
            match q.kind {
                QuantifierKind::Has => Pred::Exists { entity, body },
                QuantifierKind::None => Pred::Forall { entity, body },
            }
        })
    }

    /// `UnjoinedGuardRead` for each entity the body compares that is a witness no chain of
    /// `joined` pairs reaches from a row the action binds: the guard holds when any row of it
    /// satisfies the body, wherever it is.
    fn unjoined_reads(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        binding: &Binding,
        joined: &[(EntityKey, EntityKey)],
        read: &[EntityKey],
        site: &Site,
    ) {
        let model = self.model;
        let bound: Vec<EntityKey> = binding
            .iter()
            .filter(|(_, at)| !matches!(at, At::Bound(_)))
            .map(|(ek, _)| *ek)
            .collect();
        let mut reached = bound.clone();
        let mut grew = true;
        while grew {
            grew = false;
            for &(a, b) in joined {
                for (from, to) in [(a, b), (b, a)] {
                    if reached.contains(&from) && !reached.contains(&to) {
                        reached.push(to);
                        grew = true;
                    }
                }
            }
        }
        let mut reported: Vec<EntityKey> = Vec::new();
        for &ek in read {
            if reached.contains(&ek) || reported.contains(&ek) {
                continue;
            }
            reported.push(ek);
            let usecase = &model.use_cases[uc].id;
            let entity = &model.entities[ek].id;
            let fix = match model.key_path(&bound, ek) {
                Some(path) => {
                    let ids: Vec<&str> =
                        path.iter().map(|e| model.entities[*e].id.as_str()).collect();
                    format!(
                        "add `.along({})` to read the row joined to the use case's, or write {entity} in {usecase}",
                        ids.join(", ")
                    )
                }
                None => format!(
                    "relate {entity} to an entity {usecase} writes and add `.along(...)`, or write {entity} in {usecase}"
                ),
            };
            self.diag(
                vec![ek],
                StateDiag::UnjoinedGuardRead {
                    condition: model.conditions[ck].id.clone(),
                    usecase: usecase.clone(),
                    entity: entity.clone(),
                    fix,
                    site: site.clone(),
                },
            );
        }
    }

    /// A body applied to a use case that writes nothing has no action to guard.
    pub(in crate::lifecycle::build) fn inert_conditions(&mut self) {
        let model = self.model;
        for b in applied_bodies(model) {
            if self.active_ucs.contains(&b.usecase) {
                continue;
            }
            self.unbindable(
                b.usecase,
                b.cond,
                b.body.conds.iter().flat_map(cond_entities).collect(),
                format!(
                    "the use case writes nothing, so no action carries the guard; to give it a step, write what it records, such as a flag `sets({uc}, Entity.reminded == true)`, or raise an event and guard the use case it triggers",
                    uc = model.use_cases[b.usecase].id
                ),
                &Site::new(ModelRef::Condition(b.cond), &b.body.site),
            );
        }
    }
}
