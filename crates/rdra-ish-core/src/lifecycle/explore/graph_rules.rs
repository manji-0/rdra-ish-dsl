//! `precedes` and `can_reach`, decided on the graph a run explored: TLA+ cannot state them,
//! and the run keeps every edge.

use std::collections::VecDeque;

use crate::lifecycle::diag::{Diag, StateDiag};
use crate::lifecycle::expr::{Env, State};
use crate::lifecycle::trace::Trace;
use crate::lifecycle::types::{GraphKind, GraphRule, Pred};
use crate::lifecycle::view::describe_slot;

use super::{ComponentRun, Edge};

/// One binding's counterexample: the path to the state it ends in, and for `can_reach` the
/// number of states reachable from there.
struct Found {
    end: u32,
    path: Vec<Edge>,
    binding: Vec<u8>,
    confined: usize,
}

impl ComponentRun {
    /// The violation of the graph rule named `name` of this run's system, with a shortest
    /// trace; `None` when it holds, when the system has no such rule, or, for `can_reach`,
    /// when a cap stopped the run, since a state it did not reach may lead to the goal.
    pub fn graph_rule_violation(&self, name: &str) -> Option<Diag> {
        let ts = &self.system;
        let rule = ts.graph_rules().iter().find(|r| r.name == name)?;
        if matches!(rule.kind, GraphKind::CanReach { .. }) && !self.complete {
            return None;
        }
        let n = self.states.len();
        let mut succ: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut pred: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, e) in self.edges.iter().enumerate() {
            succ[e.from as usize].push(i as u32);
            pred[e.to as usize].push(i as u32);
        }
        let mut best: Option<Found> = None;
        for binding in bindings(rule.binders.len(), ts.bounds.slots()) {
            let found = match &rule.kind {
                GraphKind::Precedes { first, then } => {
                    self.precedes(rule, &binding, (first, then), &succ)
                }
                GraphKind::CanReach { goal } => self.can_reach(rule, &binding, goal, &succ, &pred),
            };
            if let Some(f) = found {
                if best.as_ref().is_none_or(|b| f.path.len() < b.path.len()) {
                    best = Some(f);
                }
            }
        }
        let found = best?;
        let start = match found.path.first() {
            Some(e) => e.from,
            None => found.end,
        };
        let (trace, last) = self.trace_along(start, &found.path);
        let rows = rule
            .binders
            .iter()
            .zip(&found.binding)
            .map(|(e, slot)| describe_slot(ts, &last, *e, usize::from(*slot), true))
            .collect::<Vec<_>>()
            .join("; ");
        let diag = match rule.kind {
            GraphKind::Precedes { .. } => StateDiag::PrecedenceViolated {
                name: rule.name.clone(),
                label: rule.label.clone(),
                rows,
                site: rule.site.clone(),
                trace,
            },
            GraphKind::CanReach { .. } => StateDiag::ReachabilityViolated {
                name: rule.name.clone(),
                label: rule.label.clone(),
                rows,
                confined: found.confined,
                site: rule.site.clone(),
                trace,
            },
        };
        Some(Diag {
            entities: rule.binders.iter().map(|e| ts.entity(*e).key).collect(),
            diag,
        })
    }

    fn holds(&self, pred: &Pred, binding: &[u8], at: u32) -> bool {
        let mut env = Env::new(&[]);
        env.bound = binding.to_vec();
        self.system.eval(pred, &self.states[at as usize], &mut env)
    }

    /// The rows of `binding` are present and joined by the rule's scope.
    fn triggered(&self, rule: &GraphRule, binding: &[u8], at: u32) -> bool {
        let s: &State = &self.states[at as usize];
        rule.binders
            .iter()
            .zip(binding)
            .all(|(e, slot)| s.is_present(&self.system, *e, usize::from(*slot)))
            && self.holds(&rule.scope, binding, at)
    }

    /// A shortest path from an initial state through states where the rows have not met
    /// `first` to one where they meet `then` (and so not `first`).
    fn precedes(
        &self,
        rule: &GraphRule,
        binding: &[u8],
        (first, then): (&Pred, &Pred),
        succ: &[Vec<u32>],
    ) -> Option<Found> {
        let n = self.states.len();
        let met = |at: u32| self.triggered(rule, binding, at) && self.holds(first, binding, at);
        let mut via: Vec<Option<Option<u32>>> = vec![None; n];
        let mut queue = VecDeque::new();
        for at in 0..n as u32 {
            if self.parents[at as usize].is_none() && !met(at) {
                via[at as usize] = Some(None);
                queue.push_back(at);
            }
        }
        while let Some(at) = queue.pop_front() {
            if self.triggered(rule, binding, at) && self.holds(then, binding, at) {
                let mut path = Vec::new();
                let mut cur = at;
                while let Some(Some(e)) = via[cur as usize] {
                    let edge = self.edges[e as usize];
                    path.push(edge);
                    cur = edge.from;
                }
                path.reverse();
                return Some(Found {
                    end: at,
                    path,
                    binding: binding.to_vec(),
                    confined: 0,
                });
            }
            for &e in &succ[at as usize] {
                let to = self.edges[e as usize].to;
                if via[to as usize].is_none() && !met(to) {
                    via[to as usize] = Some(Some(e));
                    queue.push_back(to);
                }
            }
        }
        None
    }

    /// The first state, in discovery order, where the rows are present and from which no path
    /// reaches one where they meet `goal`: BFS discovers states in depth order, so its path is
    /// a shortest one.
    fn can_reach(
        &self,
        rule: &GraphRule,
        binding: &[u8],
        goal: &Pred,
        succ: &[Vec<u32>],
        pred: &[Vec<u32>],
    ) -> Option<Found> {
        let n = self.states.len();
        let mut reaches = vec![false; n];
        let mut queue = VecDeque::new();
        for at in 0..n as u32 {
            if self.triggered(rule, binding, at) && self.holds(goal, binding, at) {
                reaches[at as usize] = true;
                queue.push_back(at);
            }
        }
        while let Some(at) = queue.pop_front() {
            for &e in &pred[at as usize] {
                let from = self.edges[e as usize].from;
                if !reaches[from as usize] {
                    reaches[from as usize] = true;
                    queue.push_back(from);
                }
            }
        }
        let end =
            (0..n as u32).find(|&at| !reaches[at as usize] && self.triggered(rule, binding, at))?;
        let mut seen = vec![false; n];
        seen[end as usize] = true;
        let mut queue = VecDeque::from([end]);
        let mut confined = 0;
        while let Some(at) = queue.pop_front() {
            confined += 1;
            for &e in &succ[at as usize] {
                let to = self.edges[e as usize].to;
                if !seen[to as usize] {
                    seen[to as usize] = true;
                    queue.push_back(to);
                }
            }
        }
        Some(Found {
            end,
            path: self.path_to(end),
            binding: binding.to_vec(),
            confined,
        })
    }
}

/// Every tuple of `k` slots below `slots`, the first binder slowest.
fn bindings(k: usize, slots: usize) -> impl Iterator<Item = Vec<u8>> {
    let total = slots.pow(k as u32);
    (0..total).map(move |mut i| {
        let mut out = vec![0u8; k];
        for slot in out.iter_mut().rev() {
            *slot = (i % slots) as u8;
            i /= slots;
        }
        out
    })
}

impl ComponentRun {
    /// The trace of `path` from `states[start]`, lifted and in model terms, with its last
    /// state.
    fn trace_along(&self, start: u32, path: &[Edge]) -> (Trace, State) {
        let (trace, _, last) = self.lifted_path(start, path, None);
        (trace, last)
    }
}
