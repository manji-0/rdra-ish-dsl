//! Lifting a reduced path back to a path of the unreduced cone.
use crate::lifecycle::narrow::id32;
use std::collections::{HashMap, VecDeque};

use super::extended::Extended;
use super::partition::{partition, Succ};
use super::*;
use crate::lifecycle::explore::{Adjacency, Caps, Spurious};
use crate::lifecycle::expr::State;
use crate::lifecycle::lasso::Loop;
use crate::lifecycle::trace::ReplayError;
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

impl TransitionSystem {
    /// A reduced path, each step `(action, params, successor)` from `start`, lifted to a path
    /// of the unreduced cone: a class move becomes the S-action path that realizes it from the
    /// concrete S state the lifted path holds. Returns the start with its S part concrete and
    /// the lifted steps, grouped per input step. Every lifted step is checked against the
    /// unreduced cone's step relation in every build: a step that is not one of its moves is
    /// the `ReplayError` of that step, a fault of the reduction, never a panic (claim A16 of
    /// docs/theory/claims.md).
    pub(crate) fn lift(
        &self,
        start: &State,
        steps: &[Step],
    ) -> Result<(State, Vec<Vec<Step>>), ReplayError> {
        let Some(r) = self.reduction() else {
            return Ok((
                start.clone(),
                steps.iter().map(|s| vec![s.clone()]).collect(),
            ));
        };
        let ext = Extended::rebuild(self, r);
        let mut sub = ext.system.clone();
        sub.reduction = r.inner.clone();
        // S up to `H` when the quotient was (#212 phase 2): a plain run of it stopped at the
        // memory limit. Its stored graph stands for S's as `Classes::of` read it, and the S
        // path found on it is lifted to concrete S states before it is spliced in.
        let orbits = r.up_to_h.then(|| {
            sub.clone().run_orbits_fixing(
                Caps::new(usize::MAX, usize::MAX),
                crate::lifecycle::explore::Checks::none(),
                &ext.fixed(),
            )
        });
        let plain;
        let run = match &orbits {
            Some(orbits) => orbits.quotient_graph(),
            None => {
                plain = sub.clone().run(Caps::new(usize::MAX, usize::MAX));
                &plain
            }
        };
        let n = run.states.len();
        let blocks = partition(&ext, &run.system, run).blocks;
        let block_of_class: HashMap<u32, u32> = r.rep_blocks(run, &blocks).collect();
        let succ = Succ::new(run, &ext.rise);
        // The concrete start: an initial S state in the start's class. The run reaches its
        // initial states first.
        // A start with no class, or whose class holds no initial S state, is no initial state
        // of the cone: the path's fault, never a panic (claim A16).
        let start_block = *(r.class(self, start))
            .and_then(|c| block_of_class.get(&c))
            .ok_or(ReplayError::NotInitial)?;
        // A run keeps each state once, so a state's index is where it sits.
        let s0_at = (0..sub.initial_states().len().min(n))
            .find(|&i| blocks[i] == start_block)
            .ok_or(ReplayError::NotInitial)?;
        let s0 = run.states.at(s0_at).into_state();
        enum Lifted {
            /// A T step, and the rises of the extended sub-lifecycle it makes.
            T(ActId, Box<[u8]>, State, usize),
            S(usize),
        }
        let mut level: Vec<Lifted> = Vec::new();
        let mut s_steps: Vec<Step> = Vec::new();
        let mut s_at = id32(s0_at);
        // The stored edges the S path takes, for its lift up to `H`.
        let s_first = s_at;
        let mut s_edges = Vec::new();
        for (k, (act, params, post)) in steps.iter().enumerate() {
            // A step the lift cannot follow is state `k + 2` of the reduced path.
            let fails = ReplayError::NoMove {
                state: k + 2,
                action: *act,
            };
            if !r.skips(*act) {
                let after = run
                    .system
                    .project(&State(r.key(self, post).into()), &ext.inputs_local);
                let before = run
                    .system
                    .project(&run.states.at(s_at as usize), &ext.inputs_local);
                let stages = r.stages(&before.0, &after.0);
                for stage in &stages {
                    let e = succ
                        .rises_from(s_at)
                        .find(|e| {
                            run.system
                                .project(&run.states.at(e.to as usize), &ext.inputs_local)
                                .0
                                .as_ref()
                                == stage.as_slice()
                        })
                        .ok_or(fails.clone())?;
                    let (a, p) = run.step(&e);
                    s_steps.push((a, p.into(), run.states.at(e.to as usize).into_state()));
                    s_at = e.to;
                    s_edges.push(e);
                }
                level.push(Lifted::T(*act, params.clone(), post.clone(), stages.len()));
                continue;
            }
            let target = *(r.class(self, post))
                .and_then(|c| block_of_class.get(&c))
                .ok_or(fails.clone())?;
            // A class move with no S path behind it fails like one whose target has no block.
            let path = succ
                .shortest_path(s_at, |i| blocks[i as usize] == target)
                .ok_or(fails)?;
            let mut count = 0;
            for e in path {
                let (a, p) = run.step(&e);
                s_steps.push((a, p.into(), run.states.at(e.to as usize).into_state()));
                s_at = e.to;
                s_edges.push(e);
                count += 1;
            }
            level.push(Lifted::S(count));
        }
        // Up to `H` the S path runs through stored forms: lift it to concrete S states first.
        // `H` keeps the inputs, so a rise's concrete step sets the inputs its stored edge does.
        if let Some(orbits) = &orbits {
            let (first, concrete) = orbits.lift_path(s_first, &s_edges)?;
            if first != s0 {
                return Err(ReplayError::NotInitial);
            }
            s_steps = concrete;
        }
        // The S path is a path of the extended sub-lifecycle under its own reduction: lift it
        // there and splice the concrete S parts back under the T steps.
        let (s_start, lifted) = sub.lift(&s0, &s_steps)?;
        let mut lifted = lifted.into_iter();
        let mut out_start = start.clone();
        r.write(&mut out_start, &s_start.0);
        let mut cur = out_start.clone();
        let mut out = Vec::new();
        // The lifted steps so far: a step that fails is state `done + 2` of the lifted path.
        let mut done = 0usize;
        let is_move = |cur: &State, step: &Step| {
            self.plain_moves(cur)
                .iter()
                .any(|(a, p, n)| *a == step.0 && *p == step.1 && *n == step.2)
        };
        for step in level {
            let mut group = Vec::new();
            match step {
                Lifted::T(act, params, post, rises) => {
                    // A rise's lifted step leaves S where it was; the T step below carries
                    // the inputs.
                    // A rise lifts as itself: one group of one step.
                    for _ in 0..rises {
                        if lifted.next().is_none_or(|group| group.len() != 1) {
                            return Err(ReplayError::Length);
                        }
                    }
                    let mut next = post;
                    r.write(&mut next, &r.key(self, &cur));
                    // A T step lifts as itself.
                    let step = (act, params, next.clone());
                    if !is_move(&cur, &step) {
                        return Err(ReplayError::NoMove {
                            state: done + 2,
                            action: act,
                        });
                    }
                    group.push(step);
                    done += 1;
                    cur = next;
                }
                Lifted::S(count) => {
                    for _ in 0..count {
                        for (a, p, s_post) in lifted.next().ok_or(ReplayError::Length)? {
                            let mut next = cur.clone();
                            r.write(&mut next, &s_post.0);
                            // A folded or merged action binds its T params in the cone:
                            // the cone's step is the variant that leads to the same state.
                            let Some(first) = ext.acts[a.index()] else {
                                return Err(ReplayError::NoMove {
                                    state: done + 2,
                                    action: a,
                                });
                            };
                            let fails = ReplayError::NoMove {
                                state: done + 2,
                                action: first,
                            };
                            let step = if r.in_cone_params(self, first, &p) {
                                (first, p, next.clone())
                            } else {
                                self.plain_moves(&cur)
                                    .into_iter()
                                    .find(|(a2, _, n2)| r.stands_for(first, *a2) && *n2 == next)
                                    .ok_or(fails.clone())?
                            };
                            if !is_move(&cur, &step) {
                                return Err(fails);
                            }
                            group.push(step);
                            done += 1;
                            cur = next;
                        }
                    }
                }
            }
            out.push(group);
        }
        Ok((out_start, out))
    }
}

/// A lasso of the unreduced cone: its states, the action of each step (with `Loop::Back`, one
/// more, which returns to state `to`), and how it goes on.
pub(crate) type ConeLasso = (Vec<State>, Vec<ActId>, Loop);

/// The most copies of a reduced loop lifted before the lift gives up on closing it.
const MAX_COPIES: usize = 1 << 12;
/// The most states the walk from a stop's concrete state visits.
const MAX_WALK: usize = 1 << 18;

impl TransitionSystem {
    /// A reduced lasso that loops, lifted to the unreduced cone: the prefix, then the loop
    /// repeated until the concrete state at the end of a copy is one at the end of an earlier
    /// copy. The lift of a copy is a function of the concrete state it starts from, so the
    /// lifted loop closes after at most as many copies as there are concrete states at the
    /// loop's start.
    pub(crate) fn lift_cycle(
        &self,
        start: &State,
        prefix: &[Step],
        cycle: &[Step],
    ) -> Result<ConeLasso, Spurious> {
        let mut copies = 4;
        while copies <= MAX_COPIES {
            let mut steps = prefix.to_vec();
            (0..copies).for_each(|_| steps.extend_from_slice(cycle));
            // A lift that fails is no lasso of the cone: the reduced one is spurious.
            let (first, groups) = self.lift(start, &steps).map_err(|_| Spurious::Fails)?;
            let mut states = vec![first];
            let mut actions = Vec::new();
            let mut ends = Vec::new();
            for (k, group) in groups.into_iter().enumerate() {
                if k == prefix.len() {
                    ends.push(states.len() - 1);
                }
                for (act, _, post) in group {
                    actions.push(act);
                    states.push(post);
                }
                if k + 1 > prefix.len() && (k + 1 - prefix.len()).is_multiple_of(cycle.len()) {
                    ends.push(states.len() - 1);
                }
            }
            let mut seen: HashMap<&State, usize> = HashMap::new();
            for &end in &ends {
                if let Some(&back) = seen.get(&states[end]) {
                    let states = states[..end].to_vec();
                    let actions = actions[..end].to_vec();
                    return Ok((states, actions, Loop::Back { to: back + 1 }));
                }
                seen.insert(&states[end], end);
            }
            copies *= 2;
        }
        Err(Spurious::Unclosed)
    }

    /// A reduced lasso that stops, lifted to the unreduced cone `cone`: the prefix, then a
    /// walk of S steps from the concrete state it ends at that keep T and `obs` where they
    /// are, to a state with no move (a stop) or around a cycle (a loop inside the class).
    pub(crate) fn lift_stop(
        &self,
        cone: &TransitionSystem,
        start: &State,
        prefix: &[Step],
    ) -> Result<ConeLasso, Spurious> {
        // Unchecked invariant: `lift_stop` is called only on a reduced cone's lasso
        // (`decide_liveness_reduced`); it is about the caller, not about a path.
        let r = self.reduction().expect("a reduced cone");
        let (first, steps) = self.lifted(start, prefix).map_err(|_| Spurious::Fails)?;
        let mut states = vec![first];
        let mut actions = Vec::new();
        for (act, _, post) in steps {
            actions.push(act);
            states.push(post);
        }
        // Unchecked invariant: `states` starts with the lifted start, so it is never empty.
        let x = states.last().expect("a lasso has a state").clone();
        let obs = cone.project(&x, r.obs());
        // The walk's graph: its states in the order found, the parent of each on a shortest
        // walk, and the moves it keeps.
        let mut at: HashMap<State, u32> = HashMap::from([(x.clone(), 0)]);
        let mut order = vec![x];
        let mut parent: Vec<Option<(u32, ActId)>> = vec![None];
        let mut edges: Vec<(u32, u32, ActId)> = Vec::new();
        let walk_to = |order: &[State], parent: &[Option<(u32, ActId)>], mut i: u32| {
            let mut walk = Vec::new();
            while let Some((p, act)) = parent[i as usize] {
                walk.push((act, order[i as usize].clone()));
                i = p;
            }
            walk.reverse();
            walk
        };
        let mut k = 0;
        while k < order.len() {
            let s = order[k].clone();
            let moves: Vec<_> = cone.moves(&s).filter(|m| m.next != s).collect();
            if moves.is_empty() {
                for (act, s) in walk_to(&order, &parent, id32(k)) {
                    actions.push(act);
                    states.push(s);
                }
                return Ok((states, actions, Loop::Stutter));
            }
            for m in moves {
                if !r.skips(m.act) || cone.project(&m.next, r.obs()) != obs {
                    continue;
                }
                let next = id32(order.len());
                let to = *at.entry(m.next.clone()).or_insert_with(|| {
                    order.push(m.next.clone());
                    parent.push(Some((id32(k), m.act)));
                    next
                });
                edges.push((id32(k), to, m.act));
                if order.len() > MAX_WALK {
                    return Err(Spurious::NoLoop);
                }
            }
            k += 1;
        }
        // Every state of the walk has a move: a cycle, entered at its first state found.
        let n = order.len();
        let adjacency = Adjacency::new(n, || edges.iter().map(|&(a, b, _)| (a, b)));
        let (comp, count) = adjacency.components();
        let mut size = vec![0u32; count];
        comp.iter().for_each(|c| size[*c as usize] += 1);
        let Some(entry) = (0..n).find(|&i| size[comp[i] as usize] >= 2) else {
            return Err(Spurious::NoLoop);
        };
        for (act, s) in walk_to(&order, &parent, id32(entry)) {
            actions.push(act);
            states.push(s);
        }
        let to = states.len();
        // A shortest walk inside the component from the entry back to it.
        let c = comp[entry];
        let mut via: Vec<Option<(u32, ActId)>> = vec![None; n];
        let mut queue = VecDeque::from([id32(entry)]);
        let mut back = None;
        'search: while let Some(u) = queue.pop_front() {
            for &(a, b, act) in edges.iter().filter(|e| e.0 == u) {
                if comp[b as usize] != c || a == b {
                    continue;
                }
                if b as usize == entry {
                    back = Some((u, act));
                    break 'search;
                }
                if via[b as usize].is_none() {
                    via[b as usize] = Some((u, act));
                    queue.push_back(b);
                }
            }
        }
        // Unchecked invariants of the walk's own graph, not of a path it was given: a strongly
        // connected component of two or more states has a cycle through `entry`, which the
        // search finds, and every state it reaches has the parent it was reached through.
        let (last, back_act) = back.expect("a component of two states has a cycle");
        let mut cycle = Vec::new();
        let mut i = last;
        while i as usize != entry {
            let (p, act) = via[i as usize].expect("the walk's parent");
            cycle.push((act, order[i as usize].clone()));
            i = p;
        }
        for (act, s) in cycle.into_iter().rev() {
            actions.push(act);
            states.push(s);
        }
        actions.push(back_act);
        Ok((states, actions, Loop::Back { to }))
    }
}
