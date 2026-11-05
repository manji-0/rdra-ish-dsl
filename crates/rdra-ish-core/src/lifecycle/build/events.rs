//! The marks of tracked events (docs/state-derivation.md, "Triggers"): the last step of a use
//! case that raises an event sets `<E>._raised_<Ev>` on its rows, and the first step of a use
//! case the event triggers runs only on a row that carries the mark, so it runs after the
//! raise and never before.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::Builder;
use crate::event_flow::{collect_event_flows, collect_timer_flows, EventFlow};
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    ActId, Action, ActionOrigin, At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, ModelRef,
    Param, ParamRole, Pred, Rhs, Site, Term, Value, VarId, VarKind,
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
                    CmpOp::Eq,
                    Term::Lit(Value::TRUE),
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
