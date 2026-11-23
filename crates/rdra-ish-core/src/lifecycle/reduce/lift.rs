//! Lifting a reduced path back to a path of the unreduced cone.
use std::collections::{HashMap, VecDeque};

use super::extended::Extended;
use super::partition::{quotient_blocks, split_edges, Succ};
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::{Adjacency, Caps, Spurious};
use crate::lifecycle::expr::State;
use crate::lifecycle::lasso::Loop;
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

impl TransitionSystem {
    /// A reduced path, each step `(action, params, successor)` from `start`, lifted to a path
    /// of the unreduced cone: a class move becomes the S-action path that realizes it from the
    /// concrete S state the lifted path holds. Returns the start with its S part concrete and
    /// the lifted steps, grouped per input step.
    pub(crate) fn lift(&self, start: &State, steps: &[Step]) -> (State, Vec<Vec<Step>>) {
        let Some(r) = self.reduction() else {
            return (
                start.clone(),
                steps.iter().map(|s| vec![s.clone()]).collect(),
            );
        };
        let ext = Extended::rebuild(self, r);
        let mut sub = ext.system.clone();
        sub.reduction = r.inner.clone();
        let run = sub.clone().run(Caps::new(usize::MAX, usize::MAX));
        let n = run.states.len();
        let key_vars: VarSet = ext.obs_local.union(&ext.inputs_local).copied().collect();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = run.system.project(st, &key_vars);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let (silent, rise_edges) = split_edges(&run, &ext.rise);
        let blocks = quotient_blocks(n, &silent, &rise_edges, &initial);
        let at: HashMap<&State, u32> = run
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s, i as u32))
            .collect();
        let block_of_class: HashMap<u32, u32> = run
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| r.class_of.get(&s.0).map(|c| (*c, blocks[i])))
            .collect();
        let succ = Succ::new(&run, &ext.rise);
        // The concrete start: an initial S state in the start's class. The run reaches its
        // initial states first.
        let start_block = block_of_class[&r
            .class(self, start)
            .expect("a reduced state holds a representative")];
        let s0 = run
            .states
            .iter()
            .take(sub.initial_states().len())
            .find(|s| blocks[at[*s] as usize] == start_block)
            .expect("the start's class holds an initial S state")
            .clone();
        enum Lifted {
            /// A T step, and the rises of the extended sub-lifecycle it makes.
            T(ActId, Box<[u8]>, State, usize),
            S(usize),
        }
        let mut level: Vec<Lifted> = Vec::new();
        let mut s_steps: Vec<Step> = Vec::new();
        let mut s_at = at[&s0];
        for (act, params, post) in steps {
            if !r.skips(*act) {
                let after = run
                    .system
                    .project(&State(r.key(self, post).into()), &ext.inputs_local);
                let before = run
                    .system
                    .project(&run.states[s_at as usize], &ext.inputs_local);
                let stages = r.stages(&before.0, &after.0);
                for stage in &stages {
                    let e = succ
                        .rises_from(s_at)
                        .find(|e| {
                            run.system
                                .project(&run.states[e.to as usize], &ext.inputs_local)
                                .0
                                .as_ref()
                                == stage.as_slice()
                        })
                        .expect("a T step's rise is a rise of the extended sub-lifecycle");
                    let (a, p) = run.step(&e);
                    s_steps.push((a, p.into(), run.states[e.to as usize].clone()));
                    s_at = e.to;
                }
                level.push(Lifted::T(*act, params.clone(), post.clone(), stages.len()));
                continue;
            }
            let target = block_of_class[&r.class(self, post).expect("a class move enters a class")];
            let path = succ.shortest_path(s_at, |i| blocks[i as usize] == target);
            let mut count = 0;
            for e in path {
                let (a, p) = run.step(&e);
                s_steps.push((a, p.into(), run.states[e.to as usize].clone()));
                s_at = e.to;
                count += 1;
            }
            level.push(Lifted::S(count));
        }
        // The S path is a path of the extended sub-lifecycle under its own reduction: lift it
        // there and splice the concrete S parts back under the T steps.
        let (s_start, lifted) = sub.lift(&s0, &s_steps);
        let mut lifted = lifted.into_iter();
        let mut out_start = start.clone();
        r.write(&mut out_start, &s_start.0);
        let mut cur = out_start.clone();
        let mut out = Vec::new();
        for step in level {
            let mut group = Vec::new();
            match step {
                Lifted::T(act, params, post, rises) => {
                    // A rise's lifted step leaves S where it was; the T step below carries
                    // the inputs.
                    for _ in 0..rises {
                        let group = lifted.next().expect("one group per rise");
                        assert_eq!(group.len(), 1, "a rise lifts as itself");
                    }
                    let mut next = post;
                    r.write(&mut next, &r.key(self, &cur));
                    debug_assert!(
                        self.plain_moves(&cur)
                            .iter()
                            .any(|(a, p, n)| *a == act && *p == params && *n == next),
                        "a T step lifts as itself"
                    );
                    group.push((act, params, next.clone()));
                    cur = next;
                }
                Lifted::S(count) => {
                    for _ in 0..count {
                        for (a, p, s_post) in lifted.next().expect("one group per S step") {
                            let mut next = cur.clone();
                            r.write(&mut next, &s_post.0);
                            // A folded or merged action binds its T params in the cone:
                            // the cone's step is the variant that leads to the same state.
                            let first = ext.acts[a.index()].expect("an S step is the cone's");
                            let step = if r.in_cone_params(self, first, &p) {
                                (first, p, next.clone())
                            } else {
                                self.plain_moves(&cur)
                                    .into_iter()
                                    .find(|(a2, _, n2)| r.stands_for(first, *a2) && *n2 == next)
                                    .expect("a folded step is a step of the cone")
                            };
                            group.push(step);
                            cur = next;
                        }
                    }
                }
            }
            out.push(group);
        }
        (out_start, out)
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
            let (first, groups) = self.lift(start, &steps);
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
        let r = self.reduction().expect("a reduced cone");
        let (first, steps) = self.lifted(start, prefix);
        let mut states = vec![first];
        let mut actions = Vec::new();
        for (act, _, post) in steps {
            actions.push(act);
            states.push(post);
        }
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
                for (act, s) in walk_to(&order, &parent, k as u32) {
                    actions.push(act);
                    states.push(s);
                }
                return Ok((states, actions, Loop::Stutter));
            }
            for m in moves {
                if !r.skips(m.act) || cone.project(&m.next, r.obs()) != obs {
                    continue;
                }
                let next = order.len() as u32;
                let to = *at.entry(m.next.clone()).or_insert_with(|| {
                    order.push(m.next.clone());
                    parent.push(Some((k as u32, m.act)));
                    next
                });
                edges.push((k as u32, to, m.act));
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
        for (act, s) in walk_to(&order, &parent, entry as u32) {
            actions.push(act);
            states.push(s);
        }
        let to = states.len();
        // A shortest walk inside the component from the entry back to it.
        let c = comp[entry];
        let mut via: Vec<Option<(u32, ActId)>> = vec![None; n];
        let mut queue = VecDeque::from([entry as u32]);
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
