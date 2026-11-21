//! Lifting a reduced path back to a path of the unreduced cone.
use std::collections::HashMap;

use super::extended::Extended;
use super::partition::{quotient_blocks, split_edges, Succ};
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::Caps;
use crate::lifecycle::expr::State;
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
