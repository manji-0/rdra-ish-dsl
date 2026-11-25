//! `eventually` and `leads_to` decided on an explored graph under weak fairness, by fair
//! components (docs/design/native-temporal-verification.md, "Liveness by fair components").
//!
//! Per binding, `N` is the states where the goal fails, with the moves between them that
//! change a var. A behavior that stays in `N` forever either stops at one state, which is fair
//! when no fair instance is enabled there (and, under `WF_vars(Next)`, no move leaves it), or
//! cycles through a strongly connected component of `N`, which admits a fair cycle when it
//! has a move and every fair instance labels a move inside it or is disabled at one of its
//! states. Enabledness reads every move of the graph, so an instance whose only step leaves
//! `N` is still enabled. The property fails when a triggered state of `N` reaches such an end
//! inside `N`.

use std::collections::hash_map::Entry;
use std::collections::VecDeque;

use crate::fasthash::FastMap;
use crate::lifecycle::expr::State;
use crate::lifecycle::lasso::{Fairness, Loop, Unfair};
use crate::lifecycle::trace::ReplayError;
use crate::lifecycle::types::{ActId, Ltl, Pred};

use super::graph::Adjacency;
use super::{ComponentRun, Edge, PropertyVerdict};

/// Why a lasso the search built did not certify itself: a bug in the tool, never a finding
/// about the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LassoFault {
    /// The lasso does not replay through `moves`.
    Replay(ReplayError),
    /// The lasso replays and satisfies the property (`explain_property`).
    Satisfies,
    /// The lasso's loop is not fair (`TransitionSystem::unfair_loop`).
    Unfair(Unfair),
    /// A loop of a run up to instance symmetry, repeated, did not come back to its first
    /// concrete state within the copies tried (`OrbitRun::decide_liveness`).
    Unclosed,
}

const NONE: u32 = u32::MAX;

/// The fair instances of one property on one graph.
pub(super) struct FairSteps {
    /// Per binding of the graph: the fair instance its moves are steps of, or `NONE`.
    instance: Vec<u32>,
    /// The fair instances some move steps.
    count: usize,
    /// `u64` words per state in `enabled`.
    words: usize,
    /// Per state, `words` words: the fair instances with a move to another state.
    enabled: Vec<u64>,
    /// Per state: some move goes to another state.
    moves_on: Vec<bool>,
}

impl FairSteps {
    fn enabled(&self, at: u32) -> &[u64] {
        let from = at as usize * self.words;
        &self.enabled[from..from + self.words]
    }
}

/// What one binding's search found: per state of `N`, the next state on a shortest walk
/// inside `N` to the nearest fair end, and how each end ends.
pub(super) struct Avoid {
    comp: Vec<u32>,
    /// Per component: it admits a fair cycle.
    fair_cycle: Vec<bool>,
    /// Per state: the next state toward the nearest fair end, itself at an end, `NONE` when
    /// the state cannot avoid the goal forever or is not in `N`.
    toward: Vec<u32>,
    /// Per state that is an end: it may stop there.
    stops: Vec<bool>,
}

/// A counterexample before it certifies itself: `actions[k]` takes `states[k]` to
/// `states[k + 1]`, and with `Loop::Back` the last action takes the last state back.
struct Lasso {
    states: Vec<State>,
    actions: Vec<ActId>,
    tail: Loop,
}

/// Which states of `N` may stop.
#[derive(Clone, Copy)]
pub(super) enum StopRule<'a> {
    Fair(Fairness),
    /// `WF_vars(Next)` on a reduced graph: per state, whether its class can loop inside
    /// itself, where a behavior of the cone may step forever while the reduced graph stands
    /// still.
    Looping(&'a [bool]),
}

/// What the run with `WF_vars(Next)` on a reduced cone decides (`decide_liveness_reduced`).
#[derive(Clone, Debug)]
pub enum ReducedVerdict {
    /// No counterexample on a complete graph: the property holds on the unreduced cone under
    /// `WF_vars(Next)`.
    Holds,
    /// No counterexample on a graph a cap stopped.
    Undecided,
    /// A counterexample lifted to the unreduced cone that replays there, fails the property,
    /// and passes the fairness check under `WF_vars(Next)`.
    Violated(Box<crate::lifecycle::diag::StateDiag>),
    /// A counterexample of the reduced graph that does not lift to one of the unreduced cone.
    Spurious(Spurious),
}

/// Why a counterexample of the reduced graph is not one of the unreduced cone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spurious {
    /// The lifted loop did not close within the copies tried.
    Unclosed,
    /// From the concrete state of a stop, no walk of S steps that keeps T and `obs` reaches a
    /// state with no move or a cycle.
    NoLoop,
    /// The lifted lasso does not replay, satisfies the property, or is not fair.
    Fails,
}

impl ComponentRun {
    /// `properties()[i]`, an `eventually` or a `leads_to`, decided on this run's graph under
    /// `fairness`. A counterexample is a lasso that replays through `moves`, fails the
    /// property (`explain_property`), and passes `unfair_loop`; one that does not is a
    /// `LassoFault`. On a graph a cap stopped, a state the run did not finish expanding is no
    /// end, so a counterexample is real, and no counterexample is `Undecided`.
    pub fn decide_liveness(
        &self,
        i: usize,
        fairness: Fairness,
    ) -> Result<PropertyVerdict, LassoFault> {
        let Some((fair, avoid, start)) = self.liveness_search(i, StopRule::Fair(fairness)) else {
            return Ok(match self.complete {
                true => PropertyVerdict::Holds,
                false => PropertyVerdict::Undecided,
            });
        };
        let lasso = self.lasso(&fair, &avoid, start);
        let ts = &self.system;
        let replay = ts
            .replay_lasso(lasso.states, &lasso.actions, lasso.tail)
            .map_err(LassoFault::Replay)?;
        let diag = ts
            .explain_property(i, &replay)
            .ok_or(LassoFault::Satisfies)?;
        if let Some(unfair) = ts.unfair_loop(i, &replay, fairness) {
            return Err(LassoFault::Unfair(unfair));
        }
        Ok(PropertyVerdict::Violated(Box::new(diag)))
    }

    /// `properties()[i]` under `WF_vars(Next)` on this run of a reduced cone, with every state
    /// whose class can loop (`Reduction::can_loop`) taken as one where a behavior may stop
    /// when no fair instance is enabled (docs/formal-verification.md, "WF_vars(Next) on a
    /// reduced cone"). No counterexample is the unreduced cone's `ok` under `WF_vars(Next)`.
    /// A counterexample is lifted to `cone`, the unreduced cone with the property at
    /// `cone_i`, and must replay there, fail the property, and pass `unfair_loop` under
    /// `WF_vars(Next)`; one that does not is `Spurious`, never printed.
    pub fn decide_liveness_reduced(
        &self,
        i: usize,
        cone: &crate::lifecycle::TransitionSystem,
        cone_i: usize,
    ) -> ReducedVerdict {
        let ts = &self.system;
        let r = ts.reduction().expect("a reduced cone");
        let looping: Vec<bool> = self
            .states
            .iter()
            .map(|s| r.class(ts, &s).is_some_and(|c| r.can_loop(c)))
            .collect();
        let Some((fair, avoid, start)) = self.liveness_search(i, StopRule::Looping(&looping))
        else {
            return match self.complete {
                true => ReducedVerdict::Holds,
                false => ReducedVerdict::Undecided,
            };
        };
        let (first, walk, tail) = self.walk(&fair, &avoid, start);
        let step = |e: &Edge| {
            let (act, params) = self.step(e);
            (
                act,
                params.into(),
                self.states.at(e.to as usize).into_state(),
            )
        };
        let lifted = match tail {
            Loop::Stutter => {
                let prefix: Vec<_> = walk.iter().map(step).collect();
                ts.lift_stop(cone, &self.states.at(first as usize).into_state(), &prefix)
            }
            Loop::Back { to } => {
                let prefix: Vec<_> = walk[..to - 1].iter().map(step).collect();
                let cycle: Vec<_> = walk[to - 1..].iter().map(step).collect();
                ts.lift_cycle(
                    &self.states.at(first as usize).into_state(),
                    &prefix,
                    &cycle,
                )
            }
        };
        let (states, actions, tail) = match lifted {
            Ok(lasso) => lasso,
            Err(why) => return ReducedVerdict::Spurious(why),
        };
        let Ok(replay) = cone.replay_lasso(states, &actions, tail) else {
            return ReducedVerdict::Spurious(Spurious::Fails);
        };
        let Some(diag) = cone.explain_property(cone_i, &replay) else {
            return ReducedVerdict::Spurious(Spurious::Fails);
        };
        if cone
            .unfair_loop(cone_i, &replay, Fairness::WfNext)
            .is_some()
        {
            return ReducedVerdict::Spurious(Spurious::Fails);
        }
        ReducedVerdict::Violated(Box::new(diag))
    }

    /// The search of the binding whose failing triggered state BFS found first, the first
    /// binding on a tie, with that state; `None` when every binding holds.
    pub(super) fn liveness_search(
        &self,
        i: usize,
        rule: StopRule,
    ) -> Option<(FairSteps, Avoid, u32)> {
        let ts = &self.system;
        let p = &ts.properties()[i];
        let (cause, goal) = match &p.formula {
            Ltl::Eventually(g) => (None, g),
            Ltl::LeadsTo(x, g) => (Some(x), g),
            other => panic!("decide_liveness on {:?}, not a liveness form", other.kind()),
        };
        let fair = self.fair_steps(i);
        let mut best: Option<(u32, Avoid)> = None;
        for b in ts.binder_slots(&p.binders) {
            let avoid = self.avoid(&fair, goal, &b, rule);
            let found = (0..self.states.len() as u32).find(|&s| {
                let st = &self.states.at(s as usize);
                avoid.toward[s as usize] != NONE
                    && ts.triggers(p, &b, st)
                    && cause.is_none_or(|x| ts.holds_bound(x, st, &b))
            });
            if let Some(s) = found.filter(|s| best.as_ref().is_none_or(|(at, _)| s < at)) {
                best = Some((s, avoid));
            }
        }
        let (start, avoid) = best?;
        Some((fair, avoid, start))
    }

    /// The fair instances of `properties()[i]` and where each is enabled.
    fn fair_steps(&self, i: usize) -> FairSteps {
        let ts = &self.system;
        let p = &ts.properties()[i];
        let mut ids: FastMap<(ActId, Box<[u8]>), u32> = FastMap::default();
        let instance: Vec<u32> = self
            .bindings
            .iter()
            .map(|(act, params)| {
                let Some(slots) = p.fair_instance(&ts.actions()[act.index()], params) else {
                    return NONE;
                };
                let next = ids.len() as u32;
                *ids.entry((*act, slots)).or_insert(next)
            })
            .collect();
        let count = ids.len();
        let words = count.div_ceil(64).max(1);
        let n = self.states.len();
        let mut enabled = vec![0u64; n * words];
        let mut moves_on = vec![false; n];
        // `for_each` folds the edges a state at a time (`EdgeIter::fold`).
        self.edges()
            .iter()
            .filter(|e| e.from != e.to)
            .for_each(|e| {
                moves_on[e.from as usize] = true;
                set(
                    &mut enabled[e.from as usize * words..],
                    instance[e.binding as usize],
                );
            });
        FairSteps {
            instance,
            count,
            words,
            enabled,
            moves_on,
        }
    }

    /// The states the run expanded: all of them on a complete graph. A cap stops the run
    /// inside the expansion of the state its last edge leaves, so on a capped graph only the
    /// states before that one have every move recorded.
    pub(super) fn expanded(&self) -> usize {
        match self.complete {
            true => self.states.len(),
            false => self.edges().last().map_or(0, |e| e.from as usize),
        }
    }

    /// The search for one binding: `N`, its components, the fair ends, and the backward walk.
    fn avoid(&self, fair: &FairSteps, goal: &Pred, b: &[u8], rule: StopRule) -> Avoid {
        let ts = &self.system;
        let n = self.states.len();
        let expanded = self.expanded();
        let in_n: Vec<bool> = self
            .states
            .iter()
            .map(|s| !ts.holds_bound(goal, &s, b))
            .collect();
        let inside = |e: &Edge| in_n[e.from as usize] && in_n[e.to as usize];
        let adjacency = Adjacency::new(n, || {
            self.edges().iter().filter(inside).map(|e| (e.from, e.to))
        });
        let (comp, count) = adjacency.components();
        let w = fair.words;
        let mut size = vec![0u32; count];
        let mut open = vec![false; count];
        let mut always = vec![!0u64; count * w];
        for s in (0..n).filter(|&s| in_n[s]) {
            let c = comp[s] as usize;
            size[c] += 1;
            open[c] |= s >= expanded;
            for (acc, en) in always[c * w..][..w].iter_mut().zip(fair.enabled(s as u32)) {
                *acc &= en;
            }
        }
        let mut taken = vec![0u64; count * w];
        self.edges().iter().filter(inside).for_each(|e| {
            let c = comp[e.from as usize];
            if e.from != e.to && c == comp[e.to as usize] {
                set(
                    &mut taken[c as usize * w..],
                    fair.instance[e.binding as usize],
                );
            }
        });
        let fair_cycle: Vec<bool> = (0..count)
            .map(|c| {
                let range = c * w..(c + 1) * w;
                size[c] >= 2
                    && !open[c]
                    && always[range.clone()]
                        .iter()
                        .zip(&taken[range])
                        .all(|(en, tk)| en & !tk == 0)
            })
            .collect();
        let stops: Vec<bool> = (0..n)
            .map(|s| {
                in_n[s]
                    && s < expanded
                    && match rule {
                        StopRule::Fair(Fairness::Plain) => {
                            fair.enabled(s as u32).iter().all(|w| *w == 0)
                        }
                        StopRule::Fair(Fairness::WfNext) => !fair.moves_on[s],
                        StopRule::Looping(looping) => {
                            fair.enabled(s as u32).iter().all(|w| *w == 0)
                                && (!fair.moves_on[s] || looping[s])
                        }
                    }
            })
            .collect();
        let mut toward = vec![NONE; n];
        let mut queue = VecDeque::new();
        for s in 0..n {
            if stops[s] || (in_n[s] && fair_cycle[comp[s] as usize]) {
                toward[s] = s as u32;
                queue.push_back(s as u32);
            }
        }
        while let Some(at) = queue.pop_front() {
            for &from in adjacency.preds(at) {
                if toward[from as usize] == NONE {
                    toward[from as usize] = at;
                    queue.push_back(from);
                }
            }
        }
        Avoid {
            comp,
            fair_cycle,
            toward,
            stops,
        }
    }

    /// The lasso from `start`: the BFS path to it, a shortest walk inside `N` to the nearest
    /// end, and for a component a loop that covers every fair instance and returns.
    fn lasso(&self, fair: &FairSteps, avoid: &Avoid, start: u32) -> Lasso {
        let (first, walk, tail) = self.walk(fair, avoid, start);
        let mut states = vec![self.states.at(first as usize).into_state()];
        let mut actions = Vec::with_capacity(walk.len());
        let steps = match tail {
            Loop::Stutter => walk.len(),
            Loop::Back { .. } => walk.len() - 1,
        };
        for (k, e) in walk.iter().enumerate() {
            actions.push(self.bindings[e.binding as usize].0);
            if k < steps {
                states.push(self.states.at(e.to as usize).into_state());
            }
        }
        Lasso {
            states,
            actions,
            tail,
        }
    }

    /// The lasso from `start` as a walk of edges from its first state: with `Loop::Back`,
    /// the last edge returns to the state the loop starts at.
    pub(super) fn walk(
        &self,
        fair: &FairSteps,
        avoid: &Avoid,
        start: u32,
    ) -> (u32, Vec<Edge>, Loop) {
        let mut walk: Vec<Edge> = self.path_to(start);
        let mut at = start;
        while avoid.toward[at as usize] != at {
            let to = avoid.toward[at as usize];
            walk.push(self.edge_between(at, to));
            at = to;
        }
        let end = at;
        let first = walk.first().map_or(start, |e| e.from);
        let tail = if avoid.stops[end as usize] {
            Loop::Stutter
        } else {
            debug_assert!(avoid.fair_cycle[avoid.comp[end as usize] as usize]);
            let entry = walk.len() + 1;
            walk.extend(self.fair_loop(fair, avoid, end));
            Loop::Back { to: entry }
        };
        (first, walk, tail)
    }

    /// A walk inside `end`'s component from `end` back to it that, for each fair instance,
    /// takes a move of it or passes a state where it is disabled: each time to the nearest
    /// state or move that covers an instance not yet covered, then back by a shortest walk.
    fn fair_loop(&self, fair: &FairSteps, avoid: &Avoid, end: u32) -> Vec<Edge> {
        let c = avoid.comp[end as usize];
        let w = fair.words;
        let mut all = vec![0u64; w];
        (0..fair.count as u32).for_each(|a| set(&mut all, a));
        let mut covered = vec![0u64; w];
        let visit = |covered: &mut [u64], s: u32| {
            for (cv, en) in covered.iter_mut().zip(fair.enabled(s)) {
                *cv |= !en;
            }
        };
        visit(&mut covered, end);
        let done = |covered: &[u64]| covered.iter().zip(&all).all(|(cv, a)| cv & a == *a);
        let mut walk: Vec<Edge> = Vec::new();
        let mut at = end;
        while !done(&covered) {
            let path = self.walk_in(avoid, c, at, |target| match target {
                Target::State(s) => fair
                    .enabled(s)
                    .iter()
                    .zip(&covered)
                    .zip(&all)
                    .any(|((en, cv), a)| !en & !cv & a != 0),
                Target::Edge(e) => {
                    let a = fair.instance[e.binding as usize];
                    a != NONE && covered[a as usize / 64] & (1 << (a % 64)) == 0
                }
            });
            for e in &path {
                set(&mut covered, fair.instance[e.binding as usize]);
                visit(&mut covered, e.to);
            }
            at = path
                .last()
                .expect("a fair component covers every instance")
                .to;
            walk.extend(path);
        }
        if walk.is_empty() {
            walk = self.walk_in(avoid, c, end, |target| matches!(target, Target::State(_)));
            at = walk[0].to;
        }
        if at != end {
            walk.extend(self.walk_in(
                avoid,
                c,
                at,
                |target| matches!(target, Target::State(s) if s == end),
            ));
        }
        walk
    }

    /// A shortest walk inside component `c` from `from` to the first state or move `wanted`
    /// accepts, the move included. `from` itself is not offered.
    fn walk_in(
        &self,
        avoid: &Avoid,
        c: u32,
        from: u32,
        wanted: impl Fn(Target) -> bool,
    ) -> Vec<Edge> {
        let mut via: FastMap<u32, Option<Edge>> = FastMap::default();
        via.insert(from, None);
        let mut queue = VecDeque::from([from]);
        let path_to = |via: &FastMap<u32, Option<Edge>>, mut at: u32, last: Option<Edge>| {
            let mut path: Vec<Edge> = last.into_iter().collect();
            while let Some(Some(e)) = via.get(&at) {
                path.push(*e);
                at = e.from;
            }
            path.reverse();
            path
        };
        while let Some(at) = queue.pop_front() {
            if at != from && wanted(Target::State(at)) {
                return path_to(&via, at, None);
            }
            for e in self.edges_from(at) {
                // A component of `N` holds only states of `N`.
                if e.to == e.from || avoid.comp[e.to as usize] != c {
                    continue;
                }
                if wanted(Target::Edge(&e)) {
                    return path_to(&via, at, Some(e));
                }
                if let Entry::Vacant(slot) = via.entry(e.to) {
                    slot.insert(Some(e));
                    queue.push_back(e.to);
                }
            }
        }
        panic!("no walk inside the component reaches what it covers")
    }

    /// The moves from `at`, in the order BFS found them.
    fn edges_from(&self, at: u32) -> super::EdgeIter<'_> {
        self.edges().from_state(at)
    }

    /// The first move from `from` to `to`.
    fn edge_between(&self, from: u32, to: u32) -> Edge {
        self.edges_from(from)
            .find(|e| e.to == to)
            .expect("the adjacency holds only recorded moves")
    }
}

#[cfg(feature = "shadow")]
impl ComponentRun {
    /// Per binding of `properties()[i]`, in `binder_slots` order: per state, whether the goal
    /// fails there, and whether it can avoid the goal forever under `fairness`. What the twin
    /// of `OrbitRun::decide_liveness` holds the pairs to.
    pub(crate) fn liveness_reference(
        &self,
        i: usize,
        fairness: Fairness,
    ) -> Vec<(Vec<bool>, Vec<bool>)> {
        let ts = &self.system;
        let p = &ts.properties()[i];
        let goal = match &p.formula {
            Ltl::Eventually(g) | Ltl::LeadsTo(_, g) => g,
            other => panic!("liveness_reference on {:?}", other.kind()),
        };
        let fair = self.fair_steps(i);
        ts.binder_slots(&p.binders)
            .map(|b| {
                let avoid = self.avoid(&fair, goal, &b, StopRule::Fair(fairness));
                let in_n = self
                    .states
                    .iter()
                    .map(|s| !ts.holds_bound(goal, &s, &b))
                    .collect();
                (in_n, avoid.toward.iter().map(|&t| t != NONE).collect())
            })
            .collect()
    }
}

/// Sets bit `a` of `bits`, unless `a` is `NONE`.
fn set(bits: &mut [u64], a: u32) {
    if a != NONE {
        bits[a as usize / 64] |= 1 << (a % 64);
    }
}

/// What a walk inside a component looks for.
#[derive(Clone, Copy)]
enum Target<'a> {
    State(u32),
    Edge(&'a Edge),
}
