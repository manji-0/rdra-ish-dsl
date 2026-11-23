//! The marks of tracked events (docs/state-derivation.md, "Triggers"): the last step of a use
//! case that raises an event sets `<E>._raised_<Ev>` on its rows, and the first step of a use
//! case the event triggers runs only on a row that carries the mark, so it runs after the
//! raise and never before. A counted edge (`triggers(Ev, UC).counted(k)`) has its own counter
//! in place of the latch: a raise adds one, a run of the handler takes one, and a raise at
//! `k` breaks `EventOverflow_<E>_<Ev>_<UC>` (docs/design/counted-events.md).

use std::collections::{BTreeSet, HashMap, HashSet};

use super::Builder;
use crate::event_flow::{collect_event_flows, collect_timer_flows, EventFlow};
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    ActId, Action, ActionOrigin, Arm, At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, InvText,
    Invariant, ModelRef, Param, ParamRole, Pred, Rhs, Site, Term, Value, VarId, VarKind,
};
use crate::model::{EventKey, NodeRef, Raiser, RelKind, UseCaseKey};

/// How the first step of a triggered use case reads the mark of one mark entity.
enum Read {
    /// On its own params of the entity, each the raiser's row.
    Own(Vec<u8>),
    /// On one extra `Triggered` param, linked to one of the action's rows by the one direct
    /// key between the two entities, or unjoined.
    Extra(Option<Link>),
}

/// The owner var that joins a `Triggered` param to the action's param `other`; the
/// `Triggered` param is the child end when `triggered_is_child`.
struct Link {
    owner: VarId,
    triggered_is_child: bool,
    other: u8,
}

/// One `triggers(Ev, UC)` edge the marks order: the first step of `usecase` reads, on each
/// mark entity, the mark that the last step of each raiser sets.
struct Edge {
    event: EventKey,
    usecase: UseCaseKey,
    raisers: Vec<UseCaseKey>,
    reads: Vec<(EntId, Read)>,
    /// The `triggers` line.
    site: Site,
}

impl<'m> Builder<'m> {
    pub(super) fn event_marks(&mut self) {
        let edges = self.tracked_edges();
        let mut by_uc: Vec<(UseCaseKey, Vec<Edge>)> = Vec::new();
        for edge in edges {
            match by_uc.iter_mut().find(|(uc, _)| *uc == edge.usecase) {
                Some((_, edges)) => edges.push(edge),
                None => by_uc.push((edge.usecase, vec![edge])),
            }
        }
        let mut marks: HashMap<(EntId, EventKey), VarId> = HashMap::new();
        let mut counted_edges: Vec<(EventKey, UseCaseKey)> = Vec::new();
        for (uc, edges) in by_uc {
            let handlers = self.trigger_variants(uc, &edges);
            for (edge, handler) in edges.into_iter().zip(handlers) {
                let Edge {
                    event,
                    raisers,
                    reads,
                    site,
                    ..
                } = edge;
                if let Some(k) = self.counted(event, uc) {
                    counted_edges.push((event, uc));
                    match self.countable(event, uc, &raisers, &reads) {
                        Err(why) => self.uncounted(event, uc, why, &site),
                        Ok(()) => {
                            for (entity, read) in reads {
                                let var = self.counter_var(entity, event, uc, k, &site);
                                for &raiser in &raisers {
                                    self.count_up(raiser, entity, var, k, &site);
                                }
                                self.read_mark(handler, entity, var, read, event, &site);
                                self.count_down(handler, var, k, &site);
                            }
                            continue;
                        }
                    }
                }
                for (entity, read) in reads {
                    let var = match marks.get(&(entity, event)) {
                        Some(var) => *var,
                        None => {
                            let var = self.mark_var(entity, event, raisers[0]);
                            marks.insert((entity, event), var);
                            var
                        }
                    };
                    for &raiser in &raisers {
                        self.raise(raiser, event, entity, var);
                    }
                    self.read_mark(handler, entity, var, read, event, &site);
                }
            }
        }
        self.counted_without_edge(&counted_edges);
    }

    /// The `k` of `triggers(event, uc).counted(k)`, when the edge is counted.
    fn counted(&self, event: EventKey, uc: UseCaseKey) -> Option<u8> {
        self.model
            .counted_triggers
            .iter()
            .find(|(e, u, _)| (*e, *u) == (event, uc))
            .map(|(_, _, k)| *k)
    }

    /// Why a tracked counted edge cannot count: a raiser that is the handler itself, which
    /// would add and take one in one step, or a raiser that binds a mark entity twice, whose
    /// two rows may be one and would then count one raise once or twice by the binding.
    fn countable(
        &self,
        event: EventKey,
        uc: UseCaseKey,
        raisers: &[UseCaseKey],
        reads: &[(EntId, Read)],
    ) -> Result<(), String> {
        let model = self.model;
        let ev = &model.events[event].id;
        if raisers.contains(&uc) {
            return Err(format!(
                "{} raises {ev} itself, so one step would add and take the same raise",
                model.use_cases[uc].id
            ));
        }
        for raiser in raisers {
            let last = &self.ts.actions[self.uc_actions[raiser]
                .last()
                .expect("a raiser has an action")
                .index()];
            for (entity, _) in reads {
                if last.params.iter().filter(|p| p.entity == *entity).count() > 1 {
                    return Err(format!(
                        "{} binds two rows of {}, so a raise has no one row to count on",
                        model.use_cases[*raiser].id,
                        self.ts.entities[entity.index()].id
                    ));
                }
            }
        }
        Ok(())
    }

    /// `UncountedTrigger` on the `triggers` line of a counted edge that has no counter.
    fn uncounted(&mut self, event: EventKey, uc: UseCaseKey, why: String, site: &Site) {
        let model = self.model;
        let entities = self
            .uc_actions
            .get(&uc)
            .map(|acts| {
                self.ts.actions[acts[0].index()]
                    .params
                    .iter()
                    .filter(|p| p.role.is_written())
                    .map(|p| self.ts.entities[p.entity.index()].key)
                    .collect()
            })
            .unwrap_or_default();
        self.diag(
            entities,
            StateDiag::UncountedTrigger {
                event: model.events[event].id.clone(),
                usecase: model.use_cases[uc].id.clone(),
                why,
                site: site.clone(),
            },
        );
    }

    /// `UncountedTrigger` for each counted edge that is not a tracked edge: the handler writes
    /// nothing, the event is free, a free trigger also starts the handler, or the raisers
    /// share no row (`UnanchoredTrigger` says so too).
    fn counted_without_edge(&mut self, counted: &[(EventKey, UseCaseKey)]) {
        let model = self.model;
        let flows = collect_event_flows(model);
        let is_free = |f: &EventFlow| {
            f.raised_by.is_empty()
                || f.raised_by.iter().any(|r| match r {
                    Raiser::ExtSystem(_) => true,
                    Raiser::UseCase(u) => !self.uc_actions.contains_key(u),
                })
        };
        let freed: HashSet<UseCaseKey> = flows
            .iter()
            .filter(|f| is_free(f))
            .flat_map(|f| f.triggers_ucs.iter().copied())
            .chain(
                collect_timer_flows(model)
                    .into_iter()
                    .flat_map(|f| f.triggers_ucs),
            )
            .collect();
        let mut missing: Vec<(EventKey, UseCaseKey, String)> = Vec::new();
        for &(event, uc, _) in &model.counted_triggers {
            if counted.contains(&(event, uc)) {
                continue;
            }
            let ev = &model.events[event].id;
            let name = &model.use_cases[uc].id;
            let flow = flows.iter().find(|f| f.event == event);
            let why = if !self.uc_actions.contains_key(&uc) {
                format!("{name} writes nothing, so it has no step to run once per raise")
            } else if flow.is_none_or(is_free) {
                format!(
                    "{ev} is free (nothing raises it, an external system raises it, or a raiser writes nothing), so there is no raise to count"
                )
            } else if freed.contains(&uc) {
                format!("a timer or a free event also starts {name}, so it runs without a raise")
            } else {
                format!("the raisers of {ev} bind no entity in common, so a raise has no row")
            };
            missing.push((event, uc, why));
        }
        for (event, uc, why) in missing {
            let site = self.relation_site(
                RelKind::Triggers,
                NodeRef::Event(event),
                NodeRef::UseCase(uc),
            );
            self.uncounted(event, uc, why, &site);
        }
    }

    /// `<E>._pending_<Ev>_<UC>`: `0`..`k` raises pending, or `over`, sited at the `triggers`
    /// line, with its rule `EventOverflow_<E>_<Ev>_<UC>`.
    fn counter_var(
        &mut self,
        entity: EntId,
        event: EventKey,
        uc: UseCaseKey,
        k: u8,
        site: &Site,
    ) -> VarId {
        let model = self.model;
        let (ev, name) = (&model.events[event].id, &model.use_cases[uc].id);
        let values: Vec<String> = (0..=k)
            .map(|i| i.to_string())
            .chain(std::iter::once("over".to_string()))
            .collect();
        let var = self.push_var(
            VarKind::Raised {
                entity,
                event: ev.clone(),
                handler: Some(name.clone()),
            },
            Domain::Enum(values),
            Value(0),
            site.clone(),
        );
        self.overflows.push(Invariant {
            name: format!(
                "EventOverflow_{}_{ev}_{name}",
                self.ts.entities[entity.index()].id
            ),
            binders: vec![entity],
            body: Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Bound(0),
                },
                CmpOp::Ne,
                Term::Lit(Value(k + 1)),
            ),
            premises: Vec::new(),
            text: InvText::Overflow {
                event: ev.clone(),
                usecase: name.clone(),
                cap: k,
            },
            label: None,
            site: site.clone(),
        });
        var
    }

    /// The last step of `raiser` adds one to `var` on its param on `entity`: `i -> i+1` below
    /// `k`, `k -> over`, and no arm from `over`, which stays.
    fn count_up(&mut self, raiser: UseCaseKey, entity: EntId, var: VarId, k: u8, site: &Site) {
        let last = self.uc_actions[&raiser]
            .last()
            .expect("a raiser has an action")
            .index();
        let a = &mut self.ts.actions[last];
        let Some(p) = a.params.iter().position(|q| q.entity == entity) else {
            return;
        };
        a.effects.push(Effect::Transition {
            var,
            param: p as u8,
            arms: (0..=k).map(|i| arm(i, i + 1, site)).collect(),
        });
    }

    /// The handler's first step takes one off `var` on each row its mark guard reads:
    /// `i -> i-1` from 1 to `k`, and no arm from `over`, where it runs as under the latch.
    fn count_down(&mut self, handler: ActId, var: VarId, k: u8, site: &Site) {
        let a = &mut self.ts.actions[handler.index()];
        let params: Vec<u8> = a
            .guards
            .iter()
            .filter_map(|g| match (&g.why, &g.pred) {
                (
                    GuardWhy::Raised { .. },
                    Pred::Cmp(
                        Term::Slot {
                            var: v,
                            at: At::Param(p),
                        },
                        ..,
                    ),
                ) if *v == var => Some(*p),
                _ => None,
            })
            .collect();
        for p in params {
            a.effects.push(Effect::Transition {
                var,
                param: p,
                arms: (1..=k).map(|i| arm(i, i - 1, site)).collect(),
            });
        }
    }

    /// The `triggers(Ev, UC)` edges of tracked events to use cases with an action that no
    /// free trigger also starts, by event id then use case id, each with how the use case
    /// reads the marks. An event is free when nothing raises it, an external system raises
    /// it, or a raiser has no action; a timer is free.
    fn tracked_edges(&mut self) -> Vec<Edge> {
        let model = self.model;
        let mut flows = collect_event_flows(model);
        flows.sort_by_key(|f| model.events[f.event].id.clone());
        let is_free = |f: &EventFlow| {
            f.raised_by.is_empty()
                || f.raised_by.iter().any(|r| match r {
                    Raiser::ExtSystem(_) => true,
                    Raiser::UseCase(u) => !self.uc_actions.contains_key(u),
                })
        };
        let mut free_ucs: HashSet<UseCaseKey> = collect_timer_flows(model)
            .into_iter()
            .flat_map(|f| f.triggers_ucs)
            .collect();
        for f in flows.iter().filter(|f| is_free(f)) {
            free_ucs.extend(f.triggers_ucs.iter().copied());
        }
        let tracked: Vec<(EventKey, Vec<UseCaseKey>, Vec<UseCaseKey>)> = flows
            .iter()
            .filter(|f| !is_free(f))
            .map(|f| {
                let mut raisers = f.raising_usecases();
                raisers.sort();
                raisers.dedup();
                let mut handlers = f.triggers_ucs.clone();
                handlers.sort_by_key(|u| model.use_cases[*u].id.clone());
                handlers.dedup();
                handlers.retain(|uc| !free_ucs.contains(uc) && self.uc_actions.contains_key(uc));
                (f.event, raisers, handlers)
            })
            .collect();
        let mut out = Vec::new();
        for (event, raisers, handlers) in tracked {
            for uc in handlers {
                if let Some(edge) = self.edge(event, uc, &raisers) {
                    out.push(edge);
                }
            }
        }
        out
    }

    /// The edge `triggers(event, uc)`, or `None` with an `UnanchoredTrigger` when the raisers
    /// bind no entity in common. The use case reads the mark on each common entity it has a
    /// corresponding param on; with none, on one extra `Triggered` param of the first common
    /// entity that one direct key links to a row of the use case, or else, with an
    /// `UnjoinedTrigger`, of the first common entity.
    fn edge(&mut self, event: EventKey, uc: UseCaseKey, raisers: &[UseCaseKey]) -> Option<Edge> {
        let model = self.model;
        let site = self.relation_site(
            RelKind::Triggers,
            NodeRef::Event(event),
            NodeRef::UseCase(uc),
        );
        let raiser_last: Vec<&Action> = raisers
            .iter()
            .map(|r| {
                &self.ts.actions[self.uc_actions[r]
                    .last()
                    .expect("a raiser has an action")
                    .index()]
            })
            .collect();
        let mut common: Vec<EntId> = raiser_last[0].params.iter().map(|p| p.entity).collect();
        common.dedup();
        for a in &raiser_last[1..] {
            let bound: BTreeSet<EntId> = a.params.iter().map(|p| p.entity).collect();
            common.retain(|e| bound.contains(e));
        }
        let handler = &self.ts.actions[self.uc_actions[&uc][0].index()];
        let written: Vec<_> = handler
            .params
            .iter()
            .filter(|p| p.role.is_written())
            .map(|p| self.ts.entities[p.entity.index()].key)
            .collect();
        if common.is_empty() {
            self.diag(
                written,
                StateDiag::UnanchoredTrigger {
                    event: model.events[event].id.clone(),
                    usecase: model.use_cases[uc].id.clone(),
                    raisers: raisers
                        .iter()
                        .map(|r| model.use_cases[*r].id.clone())
                        .collect(),
                    site,
                },
            );
            return None;
        }
        let mut reads: Vec<(EntId, Read)> = common
            .iter()
            .filter(|&&e| corresponds(handler, &raiser_last, e))
            .map(|&e| (e, Read::Own(present(handler, e))))
            .collect();
        if reads.is_empty() {
            let linked = common.iter().find_map(|&e| {
                let mut links: Vec<Link> = Vec::new();
                for &(owner, child, parent) in &self.owners {
                    if child == e {
                        links.extend(present(handler, parent).into_iter().map(|other| Link {
                            owner,
                            triggered_is_child: true,
                            other,
                        }));
                    }
                    if parent == e {
                        links.extend(present(handler, child).into_iter().map(|other| Link {
                            owner,
                            triggered_is_child: false,
                            other,
                        }));
                    }
                }
                let keys: BTreeSet<VarId> = links.iter().map(|l| l.owner).collect();
                (keys.len() == 1).then(|| (e, links.swap_remove(0)))
            });
            reads.push(match linked {
                Some((e, link)) => (e, Read::Extra(Some(link))),
                None => {
                    self.diag(
                        written,
                        StateDiag::UnjoinedTrigger {
                            event: model.events[event].id.clone(),
                            usecase: model.use_cases[uc].id.clone(),
                            entity: self.ts.entities[common[0].index()].id.clone(),
                            site: site.clone(),
                        },
                    );
                    (common[0], Read::Extra(None))
                }
            });
        }
        Some(Edge {
            event,
            usecase: uc,
            raisers: raisers.to_vec(),
            reads,
            site,
        })
    }

    /// The first step of `uc` per edge: itself for one edge, and for several one copy per
    /// edge, `<UC>_by_<Ev>`, each the first step with that event's trigger, in the use case's
    /// actions after the first.
    fn trigger_variants(&mut self, uc: UseCaseKey, edges: &[Edge]) -> Vec<ActId> {
        let first = self.uc_actions[&uc][0];
        if edges.len() == 1 {
            return vec![first];
        }
        let model = self.model;
        let template = self.ts.actions[first.index()].clone();
        let mut out = Vec::new();
        for (k, edge) in edges.iter().enumerate() {
            let mut variant = template.clone();
            variant.name = format!("{}_by_{}", template.name, model.events[edge.event].id);
            if let ActionOrigin::UseCase { trigger, .. } = &mut variant.origin {
                *trigger = Some(edge.event);
            }
            let id = if k == 0 {
                self.ts.actions[first.index()] = variant;
                first
            } else {
                let id = ActId::at(self.ts.actions.len());
                self.ts.actions.push(variant);
                self.uc_actions
                    .get_mut(&uc)
                    .expect("the use case")
                    .insert(k, id);
                id
            };
            out.push(id);
        }
        out
    }

    /// `<E>._raised_<Ev>`, sited at the `raises` line of `raiser`, the first raiser.
    fn mark_var(&mut self, entity: EntId, event: EventKey, raiser: UseCaseKey) -> VarId {
        let site = self.relation_site(
            RelKind::Raises,
            NodeRef::UseCase(raiser),
            NodeRef::Event(event),
        );
        self.push_var(
            VarKind::Raised {
                entity,
                event: self.model.events[event].id.clone(),
                handler: None,
            },
            Domain::Bool,
            Value::FALSE,
            site,
        )
    }

    /// The last step of `raiser` sets `var` on each of its params on `entity`, once.
    fn raise(&mut self, raiser: UseCaseKey, event: EventKey, entity: EntId, var: VarId) {
        let site = self.relation_site(
            RelKind::Raises,
            NodeRef::UseCase(raiser),
            NodeRef::Event(event),
        );
        let last = self.uc_actions[&raiser]
            .last()
            .expect("a raiser has an action")
            .index();
        let a = &mut self.ts.actions[last];
        for p in 0..a.params.len() as u8 {
            if a.params[usize::from(p)].entity != entity {
                continue;
            }
            let set = a.effects.iter().any(|eff| {
                matches!(eff, Effect::Assign { var: v, param: q, .. } if *v == var && *q == p)
            });
            if !set {
                a.effects.push(Effect::Assign {
                    var,
                    param: p,
                    rhs: Rhs::Lit(Value::TRUE),
                    site: site.clone(),
                });
            }
        }
    }

    /// The guards of `handler` on the mark: `var = true` on each param of `read`, which for an
    /// extra param is a `Triggered` one with its link guard.
    fn read_mark(
        &mut self,
        handler: ActId,
        entity: EntId,
        var: VarId,
        read: Read,
        event: EventKey,
        site: &Site,
    ) {
        // A latch holds `true`; a counter holds a raise while it is not `0`.
        let (op, lit) = match &self.ts.vars[var.index()].kind {
            VarKind::Raised {
                handler: Some(_), ..
            } => (CmpOp::Ne, Value(0)),
            _ => (CmpOp::Eq, Value::TRUE),
        };
        let a = &mut self.ts.actions[handler.index()];
        let params = match read {
            Read::Own(params) => params,
            Read::Extra(link) => {
                a.params.push(Param {
                    entity,
                    role: ParamRole::Triggered,
                    via_api: None,
                });
                let p = (a.params.len() - 1) as u8;
                if let Some(link) = link {
                    let (child, parent) = if link.triggered_is_child {
                        (p, link.other)
                    } else {
                        (link.other, p)
                    };
                    a.guards.push(Guard {
                        pred: Pred::Cmp(
                            Term::Slot {
                                var: link.owner,
                                at: At::Param(child),
                            },
                            CmpOp::Eq,
                            Term::Index(At::Param(parent)),
                        ),
                        why: GuardWhy::Owner,
                        site: site.clone(),
                    });
                }
                vec![p]
            }
        };
        for p in params {
            a.guards.push(Guard {
                pred: Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(p),
                    },
                    op,
                    Term::Lit(lit),
                ),
                why: GuardWhy::Raised { event },
                site: site.clone(),
            });
        }
    }

    /// The site of the relation `kind` from `from` to `to`.
    fn relation_site(&self, kind: RelKind, from: NodeRef, to: NodeRef) -> Site {
        let (i, rel) = self
            .model
            .relations
            .iter()
            .enumerate()
            .find(|(_, r)| r.kind == kind && r.from == from && r.to == to)
            .expect("the event flow comes from a relation");
        Site::new(ModelRef::Relation(i), &rel.site)
    }
}

/// One arm of a counter, `from -> to`, sited at the `triggers` line.
fn arm(from: u8, to: u8, site: &Site) -> Arm {
    Arm {
        from: Value(from),
        to: Value(to),
        site: site.clone(),
        conds: Vec::new(),
    }
}

/// The params of `handler` on `e` that have a row before the step: all but a `New` one.
fn present(handler: &Action, e: EntId) -> Vec<u8> {
    (0..handler.params.len() as u8)
        .filter(|p| {
            let q = handler.params[usize::from(*p)];
            q.entity == e && q.role != ParamRole::New
        })
        .collect()
}

/// Whether the one present param of `handler` on `e` is the row the event was raised on: the
/// action's own row of `e`, or a parent through a key some raiser binds `e` through. Two
/// params on `e`, or a parent through a key no raiser uses, say nothing about which row the
/// event was about.
fn corresponds(handler: &Action, raisers: &[&Action], e: EntId) -> bool {
    let [p] = present(handler, e)[..] else {
        return false;
    };
    match handler.params[usize::from(p)].role {
        ParamRole::Parent { key } => raisers.iter().any(|a| {
            a.params
                .iter()
                .any(|q| q.entity == e && (q.role.is_row() || q.role == ParamRole::Parent { key }))
        }),
        _ => true,
    }
}
