//! `can_reach(goal)` decided on an explored graph. Per binding of its binders, one backward
//! pass from the states where the instance is present and the goal holds, through states
//! where it is present, marks every state from which some path reaches the goal. A present
//! state the pass does not mark is one the instance cannot finish from. Each pass is linear
//! in the graph's states and edges.

use super::cone::VarSet;
use super::diag::StateDiag;
use super::explore::Graph;
use super::expr::{Env, StateSlice};
use super::trace::Trace;
use super::types::{EntId, Pred, VarKind};
use super::TransitionSystem;

impl TransitionSystem {
    /// The violation of `reach_properties()[i]` on `graph`, a complete graph of this system:
    /// the first state in discovery order, so the end of a shortest trace, where some
    /// instance is present and no path reaches the goal, with the index of that state. The
    /// trace is left empty for the caller, which holds the run.
    pub(crate) fn reach_failure(&self, i: usize, graph: &Graph) -> Option<(u32, StateDiag)> {
        debug_assert!(graph.complete, "a capped graph decides no can_reach");
        let r = &self.reach[i];
        let n = graph.states.len();
        let adjacency = graph.adjacency();
        let mut worst: Option<(u32, Vec<u8>, Vec<bool>)> = None;
        for binding in self.binder_slots(&r.binders) {
            let present: Vec<bool> = graph
                .states
                .iter()
                .map(|s| self.all_present(&r.binders, &binding, &s))
                .collect();
            // `.along`: only linked rows trigger; the path need only keep them present.
            let trigger: Vec<bool> = graph
                .states
                .iter()
                .zip(&present)
                .map(|(s, p)| *p && self.holds_for(&r.link, &binding, &s))
                .collect();
            let mut can = vec![false; n];
            let mut goals = Vec::new();
            for (at, s) in graph.states.iter().enumerate() {
                if present[at] && self.holds_for(&r.goal, &binding, &s) {
                    can[at] = true;
                    goals.push(at as u32);
                }
            }
            adjacency.mark_backward(&mut can, goals, |from| present[from as usize]);
            let Some(stuck) = (0..n).find(|&at| trigger[at] && !can[at]) else {
                continue;
            };
            if worst.as_ref().is_none_or(|(at, ..)| stuck < *at as usize) {
                worst = Some((stuck as u32, binding, present));
            }
        }
        let (at, binding, present) = worst?;
        // The states reachable from `at` through states where the instance is present.
        let confined = adjacency.forward_from(at, |to| present[to as usize]);
        let diag = StateDiag::GoalUnreachable {
            name: r.name.clone(),
            instance: r
                .binders
                .iter()
                .zip(&binding)
                .map(|(e, slot)| self.inst(*e, usize::from(*slot)))
                .collect(),
            confined: self.goal_readings(
                r,
                &binding,
                confined.iter().map(|&s| graph.states.at(s as usize)),
            ),
            states: confined.len(),
            site: r.site.clone(),
            trace: Trace::default(),
        };
        Some((at, diag))
    }

    fn all_present(&self, binders: &[EntId], binding: &[u8], s: &StateSlice) -> bool {
        binders
            .iter()
            .zip(binding)
            .all(|(e, slot)| s.is_present(self, *e, usize::from(*slot)))
    }

    fn holds_for(&self, pred: &Pred, binding: &[u8], s: &StateSlice) -> bool {
        let mut env = Env::new(&[]);
        env.bound = binding.to_vec();
        self.eval(pred, s, &mut env)
    }

    /// What the goal reads of the bound instances in each of `states`, one line per distinct
    /// reading in the order first seen: `status = on_hold`, or `Order#1.status = on_hold`
    /// with more than one binder.
    fn goal_readings(
        &self,
        r: &super::types::Reach,
        binding: &[u8],
        states: impl Iterator<Item = impl std::borrow::Borrow<StateSlice>>,
    ) -> Vec<String> {
        let mut reads = VarSet::new();
        self.pred_reads(&r.goal, &mut reads);
        let fields: Vec<(String, usize, super::types::VarId)> = r
            .binders
            .iter()
            .zip(binding)
            .flat_map(|(e, slot)| {
                let prefix = match r.binders.len() {
                    1 => String::new(),
                    _ => format!("{}.", self.inst(*e, usize::from(*slot))),
                };
                reads
                    .iter()
                    .filter(move |v| match self.var(**v).kind {
                        VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => {
                            entity == *e
                        }
                        _ => false,
                    })
                    .map(move |v| (prefix.clone(), usize::from(*slot), *v))
            })
            .collect();
        let mut lines: Vec<String> = Vec::new();
        for s in states {
            let s: &StateSlice = s.borrow();
            let line = fields
                .iter()
                .map(|(prefix, slot, v)| {
                    let value = self.value_text(*v, s.get(self, *v, *slot));
                    format!("{prefix}{} = {value}", self.var_name(*v))
                })
                .collect::<Vec<_>>()
                .join(", ");
            if !line.is_empty() && !lines.contains(&line) {
                lines.push(line);
            }
        }
        lines
    }
}
