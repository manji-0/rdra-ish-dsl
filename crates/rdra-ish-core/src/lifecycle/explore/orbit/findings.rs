//! The state-model findings of an entity cone on the annotated quotient (#212 phase 4;
//! docs/design/instance-symmetry.md, "Soundness per check").
//!
//! The witnesses, the runs and the never-runs read stored states and action labels, as on a
//! plain run. A `.when` arm compares a slot across an edge, so it reads the edge's concrete
//! successor: the stored target taken back through the edge's permutation. The deadlock pass
//! follows one slot along paths, so it runs on (stored state, slot) pairs: a move keeps the
//! slots of its source, and the edge's permutation `σ` takes slot `j` of the concrete
//! successor to slot `σ(j)` of the stored target. A deadlock's trace is the lifted path to its
//! pair, and the deadlock names the slot the concrete instance holds at its end.

use crate::lifecycle::narrow::{id32, id8};
use crate::lifecycle::{LassoFault, ReplayError, StateDiag};
use std::collections::HashSet;

use super::{lift_path, replays, EdgePerms, OrbitRun};
use crate::lifecycle::explore::symmetry::Canon;
use crate::lifecycle::expr::State;
use crate::lifecycle::findings::{ExitArm, Progress, StateModelFindings, ValueProgress};
use crate::lifecycle::reduce::Step;
use crate::lifecycle::trace::Trace;
use crate::lifecycle::types::{EntId, Value, VarId};
use crate::lifecycle::TransitionSystem;

/// Where a deadlock the pass found sits: its stored state and the slot of `e` it keeps the
/// stuck instance on. The twin holds it to the plain run (`reference::check_state_models`).
pub(super) type StuckAt = Option<(u32, usize)>;

impl OrbitRun {
    /// `TransitionSystem::state_model_findings` of `e` on this run, which keeps its edges'
    /// permutations (`run_annotated_orbits`): the same findings a plain run of the system
    /// gives, but for the deadlocks' traces and slots, which may name other rows.
    pub(crate) fn state_model_findings(&self, e: EntId) -> StateModelFindings {
        let run = &self.run;
        let ts = &run.system;
        let g = run.graph();
        let targets = ts.state_model_values(e);
        let (deadlocks, _stuck) = self.deadlocks(e, &targets);
        let out = StateModelFindings {
            witnesses: ts.witnesses(e, &targets, &run.states, run.complete),
            runs: ts.runs(e, &g),
            deadlocks,
            arms: ts.guarded_arms_by(e, &g, |k, to| self.successor(k, to)),
        };
        #[cfg(feature = "shadow")]
        super::reference::check_state_models(self, e, &out, &_stuck);
        out
    }

    fn perms(&self) -> &EdgePerms {
        self.perms
            .as_ref()
            .expect("the state-model findings read a run that keeps its edges' permutations")
    }

    /// Edge `k`'s concrete successor: the stored `states[to]` taken back through the edge's
    /// permutation.
    fn successor(&self, k: usize, to: u32) -> State {
        let target = self.run.states.at(to as usize);
        let id = self.perms().id(k);
        if id == 0 {
            return target.into_state();
        }
        let back = self.group.inverse(self.perms().get(id));
        let mut out = vec![0u8; target.0.len()];
        self.group.apply(&back, &target.0, &mut out);
        State(out.into_boxed_slice())
    }

    /// `TransitionSystem::deadlocks` on (stored state, slot) pairs, with where each deadlock
    /// sits.
    fn deadlocks(
        &self,
        e: EntId,
        targets: &[(VarId, Value)],
    ) -> (Vec<ValueProgress>, Vec<StuckAt>) {
        let run = &self.run;
        let ts = &run.system;
        let open: Vec<(VarId, Value, Vec<ExitArm>)> = targets
            .iter()
            .filter(|&&(var, value)| !ts.var(var).ends.contains(&value))
            .map(|&(var, value)| (var, value, ts.exits(e, var, value)))
            .filter(|(_, _, exits)| TransitionSystem::has_way_on(exits))
            .collect();
        if !run.complete {
            let out = open
                .into_iter()
                .map(|(var, value, _)| ValueProgress {
                    var,
                    value,
                    progress: Progress::NotDecided,
                })
                .collect();
            return (out, Vec::new());
        }
        if open.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let pairs = Pairs::new(self, e);
        let (n, slots) = (run.states.len(), pairs.slots);
        let present = |at: usize, k: usize| pairs.present(at, k);
        let mut out = Vec::new();
        let mut where_stuck = Vec::new();
        let mut columns: Vec<(VarId, Vec<bool>)> = Vec::new();
        for (var, value, exits) in open {
            if !columns.iter().any(|(v, _)| *v == var) {
                columns.push((var, pairs.can_leave(var)));
            }
            let (_, leave) = columns.iter().find(|(v, _)| *v == var).expect("just added");
            let cell = ts.offset(var);
            let stuck = |at: usize, k: usize| {
                present(at, k) && run.states.cell(at, cell + k) == value.0 && !leave[at * slots + k]
            };
            let first = (0..slots)
                .filter_map(|k| (0..n).find(|&at| stuck(at, k)).map(|at| (at, k)))
                .min();
            let progress = match first {
                None => Progress::Leaves,
                Some((at, k)) => {
                    let held: Vec<_> = (pairs.forward(id32(at), id8(k)).into_iter())
                        .map(|(r, j)| (run.states.at(r as usize), j))
                        .collect();
                    // A trace that fails to replay or lift is a fault in the deadlock's place.
                    let diag = match self.trace_to_slot(e, id32(at), k) {
                        Ok((trace, named)) => {
                            let mut diag = ts.deadlock(e, var, value, named, &exits, &held);
                            *diag.trace_mut().expect("a deadlock has a trace") = trace;
                            diag
                        }
                        Err(fault) => {
                            let diag = ts.deadlock(e, var, value, k, &exits, &held);
                            StateDiag::TraceFault {
                                name: diag.name(&[]),
                                fault: LassoFault::Replay(fault),
                            }
                        }
                    };
                    Progress::Stuck {
                        at: id32(at),
                        diag: Box::new(diag),
                    }
                }
            };
            where_stuck.push(first.map(|(at, k)| (id32(at), k)));
            out.push(ValueProgress {
                var,
                value,
                progress,
            });
        }
        (out, where_stuck)
    }

    /// The stored path to `states[at]` lifted to concrete states, with the permutation `τ`
    /// that reads its last state: the concrete last state is `τ(states[at])`.
    pub(super) fn lift_to(&self, at: u32) -> Result<(State, Vec<Step>, Vec<u8>), ReplayError> {
        let path = self.run.path_to(at);
        let first = path.first().map_or(at, |e| e.from);
        let (start, steps, tau) =
            lift_path(&self.run, &self.group, &mut Canon::default(), first, &path)?;
        replays(&self.run.system, &start, &steps)?;
        Ok((start, steps, tau))
    }

    /// The trace to the pair (`states[at]`, slot `k` of `e`), in model terms, and the slot
    /// of `e` the concrete instance holds at its end.
    fn trace_to_slot(&self, e: EntId, at: u32, k: usize) -> Result<(Trace, usize), ReplayError> {
        let ts = &self.run.system;
        let (start, steps, tau) = self.lift_to(at)?;
        let named = usize::from(self.group.map_slot(&tau, e, id8(k)));
        // On a reduced cone the lifted path still holds class representatives on the S side:
        // #74's lift comes after the symmetry's, which was applied last.
        let (start, steps) = ts.lifted(&start, &steps)?;
        Ok((ts.trace_from(&start, &steps), named))
    }
}

/// The pairs (stored state, slot of `e`) of a complete annotated run, and the moves between
/// them: an edge from `r` takes `(r, j)` to `(to, σ(j))`.
struct Pairs<'a> {
    orbits: &'a OrbitRun,
    e: EntId,
    slots: usize,
    /// Per distinct permutation of the run: the slot of `e` each slot goes to.
    to: Vec<Vec<u8>>,
    /// Per distinct permutation: the slot each slot comes from.
    from: Vec<Vec<u8>>,
    /// Per stored state, the edges into it, as (source, permutation id), from `preds_at`.
    preds: Vec<(u32, u32)>,
    preds_at: Vec<u32>,
}

impl<'a> Pairs<'a> {
    fn new(orbits: &'a OrbitRun, e: EntId) -> Self {
        let (run, g, perms) = (&orbits.run, &orbits.group, orbits.perms());
        let slots = run.system.slots(e);
        let map = |p: &[u8]| -> Vec<u8> { (0..slots).map(|j| g.map_slot(p, e, id8(j))).collect() };
        let to: Vec<Vec<u8>> = (0..id32(perms.len()))
            .map(|id| map(perms.get(id)))
            .collect();
        let from: Vec<Vec<u8>> = (0..id32(perms.len()))
            .map(|id| map(&g.inverse(perms.get(id))))
            .collect();
        let n = run.states.len();
        let mut preds_at = vec![0u32; n + 1];
        for edge in run.edges().iter() {
            preds_at[edge.to as usize + 1] += 1;
        }
        for i in 0..n {
            preds_at[i + 1] += preds_at[i];
        }
        let mut fill = preds_at.clone();
        let mut preds = vec![(0u32, 0u32); preds_at[n] as usize];
        for (k, edge) in run.edges().iter().enumerate() {
            let at = &mut fill[edge.to as usize];
            preds[*at as usize] = (edge.from, perms.id(k));
            *at += 1;
        }
        Pairs {
            orbits,
            e,
            slots,
            to,
            from,
            preds,
            preds_at,
        }
    }

    fn present(&self, at: usize, k: usize) -> bool {
        let ts = &self.orbits.run.system;
        let exists = ts.offset(ts.entity(self.e).exists);
        self.orbits.run.states.cell(at, exists + k) == Value::PRESENT.0
    }

    /// `TransitionSystem::can_leave` on pairs, every slot at once: whether a path from
    /// `(at, k)` changes `var` at the instance's slot or deletes it, by `at * slots + k`.
    fn can_leave(&self, var: VarId) -> Vec<bool> {
        let run = &self.orbits.run;
        let (perms, slots) = (self.orbits.perms(), self.slots);
        let cell = run.system.offset(var);
        let value = |at: u32, k: usize| run.states.cell(at as usize, cell + k);
        let mut leave = vec![false; run.states.len() * slots];
        let mut stack = Vec::new();
        for (k, edge) in run.edges().iter().enumerate() {
            let to = &self.to[perms.id(k) as usize];
            for (j, &t) in to.iter().enumerate() {
                let at = edge.from as usize * slots + j;
                if leave[at] || !self.present(edge.from as usize, j) {
                    continue;
                }
                let t = usize::from(t);
                if !self.present(edge.to as usize, t) || value(edge.to, t) != value(edge.from, j) {
                    leave[at] = true;
                    stack.push(at);
                }
            }
        }
        while let Some(at) = stack.pop() {
            let (r, i) = (at / slots, at % slots);
            let edges = self.preds_at[r] as usize..self.preds_at[r + 1] as usize;
            for &(p, id) in &self.preds[edges] {
                let j = usize::from(self.from[id as usize][i]);
                let before = p as usize * slots + j;
                if !leave[before] && self.present(p as usize, j) {
                    leave[before] = true;
                    stack.push(before);
                }
            }
        }
        leave
    }

    /// `(at, k)` and every pair a path from it reaches, in the order a breadth-first walk
    /// finds them.
    fn forward(&self, at: u32, k: u8) -> Vec<(u32, u8)> {
        let run = &self.orbits.run;
        let perms = self.orbits.perms();
        let edges = run.edges();
        let mut seen = HashSet::from([(at, k)]);
        let mut out = vec![(at, k)];
        let mut next = 0;
        while next < out.len() {
            let (r, j) = out[next];
            for i in edges.range_of(r) {
                let to = edges.get(i).to;
                let pair = (to, self.to[perms.id(i) as usize][usize::from(j)]);
                if seen.insert(pair) {
                    out.push(pair);
                }
            }
            next += 1;
        }
        out
    }
}
