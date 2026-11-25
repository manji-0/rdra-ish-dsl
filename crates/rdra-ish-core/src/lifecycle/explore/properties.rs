//! The property forms BFS decides on the graph it explored (`LtlKind::is_explored`):
//! `stable` and `unchanged` on each edge, and `precedes` on each state with the latch the
//! TLA+ printer spells `<P>_seen`. `always` is decided on each state too, as an invariant
//! over the present binders, on the graphs `verify` explores for an `always` property. The
//! latch is no var of the system: a state where `q` holds and the latch is unset is one a
//! path reaches through states where `p` never held, so the search walks the graph that
//! keeps only those states.

use std::collections::VecDeque;

use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::expr::{State, StateSlice};
use crate::lifecycle::types::{Ltl, Pred, Property};

use super::{Caps, ComponentRun, Edge};
use crate::lifecycle::TransitionSystem;

/// What the explorer decides of one property on one run.
#[derive(Clone, Debug)]
pub enum PropertyVerdict {
    Holds,
    /// The shortest trace to the step (`stable`, `unchanged`) or state (`always`,
    /// `precedes`) that breaks it, or the lasso of `eventually` and `leads_to`
    /// (`decide_liveness`).
    Violated(Box<StateDiag>),
    /// The run stopped at the state cap with no violation in what it reached.
    Undecided,
}

impl TransitionSystem {
    /// `properties()[i]`, `always` or a form `LtlKind::is_explored` names, decided by BFS on
    /// its cone under `caps`. An `always` reads the cone reduced by its independent
    /// sub-lifecycle, as TLC's run of it does: the reduction keeps what a state predicate over
    /// the property's vars sees. The other forms read the unreduced cone. A cone that frees an
    /// entity's slots (`cone_frees_slots`) reaches a superset, so a violation there is decided
    /// again on the whole system, under `whole`: a whole-model run is bounded as `verify`
    /// bounds one, by its own state cap and `--temporal-budget`.
    pub fn decide_property(&self, i: usize, caps: Caps, whole: Caps) -> PropertyVerdict {
        let cone = self.property_cone(i);
        let system = match self.properties()[i].formula {
            Ltl::Always(_) => self.reduced_cone(&cone, [self.property_seed(i)], caps),
            _ => self.restrict_to(&cone.vars, &cone.keep),
        };
        let name = &self.properties()[i].name;
        let index = system
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the cone holds what the property reads");
        let verdict = system
            .run(caps)
            .decide_property(index)
            .expect("an explored form");
        match verdict {
            PropertyVerdict::Violated(_) if self.cone_frees_slots(&cone) => self
                .clone()
                .run(whole)
                .decide_property(i)
                .expect("an explored form"),
            verdict => verdict,
        }
    }
}

impl ComponentRun {
    /// `properties()[i]` of the explored system on its graph; `None` for `eventually` and
    /// `leads_to`, which no finite path breaks.
    pub fn decide_property(&self, i: usize) -> Option<PropertyVerdict> {
        let ts = &self.system;
        let p = &ts.properties()[i];
        let found = match &p.formula {
            Ltl::Always(x) => self.unsafe_state(p, x),
            Ltl::Stable(_) | Ltl::Unchanged { .. } => self.breaking_step(p),
            Ltl::Precedes(x, y) => self.unlatched_state(p, x, y),
            Ltl::Eventually(_) | Ltl::LeadsTo(..) => return None,
        };
        let Some((first, path)) = found else {
            return Some(match self.complete {
                true => PropertyVerdict::Holds,
                false => PropertyVerdict::Undecided,
            });
        };
        let states: Vec<State> = std::iter::once(first)
            .chain(path.iter().map(|e| e.to))
            .map(|k| self.states.at(k as usize).into_state())
            .collect();
        let (instance, at) = ts
            .property_failure(p, &states, states.len() - 1)
            .expect("the path the search found breaks the property");
        debug_assert_eq!(at, path.len(), "the path ends where the property fails");
        // On a reduced system the trace is lifted and may take more steps than the path; the
        // property fails at its last state either way.
        let trace = self.trace_along(first, &path);
        Some(PropertyVerdict::Violated(Box::new(
            StateDiag::PropertyViolated {
                name: p.name.clone(),
                kind: p.formula.kind(),
                instance: p
                    .binders
                    .iter()
                    .zip(&instance)
                    .map(|(e, slot)| ts.inst(*e, usize::from(*slot)))
                    .collect(),
                witness: trace.steps.len(),
                lasso: None,
                site: p.site.clone(),
                trace,
            },
        )))
    }

    /// The first state, in the order BFS found them, where the trigger holds and `x` fails for
    /// some binding, and the BFS path to it: the shortest walk to a state that breaks
    /// `always(x)`.
    fn unsafe_state(&self, p: &Property, x: &Pred) -> Option<(u32, Vec<Edge>)> {
        let ts = &self.system;
        let bindings: Vec<Vec<u8>> = ts.binder_slots(&p.binders).collect();
        let at = self.states.iter().position(|s| {
            bindings
                .iter()
                .any(|bd| ts.triggers(p, bd, &s) && !ts.holds_bound(x, &s, bd))
        })? as u32;
        let path = self.path_to(at);
        Some((path.first().map_or(at, |e| e.from), path))
    }

    /// The first edge, in the order BFS found them, that breaks `p` for some binding, and the
    /// BFS path to its source before it. BFS expands states in depth order, so no edge from a
    /// shallower state breaks it.
    fn breaking_step(&self, p: &Property) -> Option<(u32, Vec<Edge>)> {
        let ts = &self.system;
        let bindings: Vec<Vec<u8>> = ts.binder_slots(&p.binders).collect();
        let edge = self.edges().iter().find(|e| {
            let (a, b) = (
                &self.states.at(e.from as usize),
                &self.states.at(e.to as usize),
            );
            e.from != e.to && bindings.iter().any(|bd| ts.step_breaks(p, bd, a, b))
        })?;
        let mut path = self.path_to(edge.from);
        let first = path.first().map_or(edge.from, |e| e.from);
        path.push(edge);
        Some((first, path))
    }

    /// The shortest walk to a state where the trigger and `y` hold and the latch of `x` is
    /// unset, over every binding: a walk from an initial state through states where the
    /// trigger and `x` never hold.
    fn unlatched_state(&self, p: &Property, x: &Pred, y: &Pred) -> Option<(u32, Vec<Edge>)> {
        let ts = &self.system;
        let mut out: Vec<Vec<u32>> = vec![Vec::new(); self.states.len()];
        let edges = self.edges();
        for (k, e) in edges.iter().enumerate() {
            out[e.from as usize].push(k as u32);
        }
        let mut best: Option<(u32, Vec<Edge>)> = None;
        for bd in ts.binder_slots(&p.binders) {
            let on = |s: &StateSlice, pred: &Pred| {
                ts.triggers(p, &bd, s) && ts.holds_bound(pred, s, &bd)
            };
            let open: Vec<bool> = self.states.iter().map(|s| !on(&s, x)).collect();
            // Per reached state: the edge into it, or `None` for an initial state.
            let mut via: Vec<Option<Option<u32>>> = vec![None; self.states.len()];
            let mut queue: VecDeque<u32> = VecDeque::new();
            for k in 0..self.states.len() {
                if self.parent(k as u32).is_none() && open[k] {
                    via[k] = Some(None);
                    queue.push_back(k as u32);
                }
            }
            let mut found = None;
            while let Some(k) = queue.pop_front() {
                if on(&self.states.at(k as usize), y) {
                    found = Some(k);
                    break;
                }
                for &ek in &out[k as usize] {
                    let to = edges.get(ek as usize).to as usize;
                    if open[to] && via[to].is_none() {
                        via[to] = Some(Some(ek));
                        queue.push_back(to as u32);
                    }
                }
            }
            let Some(mut at) = found else {
                continue;
            };
            let mut path = Vec::new();
            while let Some(Some(ek)) = via[at as usize] {
                let e = edges.get(ek as usize);
                path.push(e);
                at = e.from;
            }
            path.reverse();
            if best.as_ref().is_none_or(|(_, b)| path.len() < b.len()) {
                best = Some((at, path));
            }
        }
        best
    }
}
