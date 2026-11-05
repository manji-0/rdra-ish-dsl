//! A path from an initial state in model terms: the actions with their bound instances, and
//! what each step changed with the site of the effect that wrote it.

use std::collections::BTreeMap;
use std::fmt;

use super::cone::{Cone, Target};
use super::diag::StateDiag;
use super::expr::State;
use super::reduce::Step;
use super::types::{
    ActId, ActionOrigin, Domain, Effect, EntId, ParamRole, Site, Value, VarId, VarKind,
};
use super::view::AbstractValue;
use super::TransitionSystem;

/// A trace from another engine matched step by step against `moves`, the step relation BFS
/// uses: its states and the move each step took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    pub(super) states: Vec<State>,
    pub(super) moves: Vec<(ActId, Box<[u8]>)>,
}

/// Where a trace leaves the step relation. `state` counts from 1, as TLC numbers its states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Empty,
    /// Not one action per step.
    Length,
    /// The first state is not an initial state.
    NotInitial,
    /// No move of `action` reaches `states[state - 1]` from the state before it.
    NoMove {
        state: usize,
        action: ActId,
    },
    /// A lasso's back edge: no move of `action` takes the last state to state `to`, or the
    /// trace has no state `to`.
    NoBackMove {
        to: usize,
        action: ActId,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    /// The instances the initial state already holds (pre-existing entities), as creations.
    pub start: Vec<Change>,
    pub steps: Vec<TraceStep>,
    /// Entities the trace creates more instances of than the bound allows. The cone that
    /// found it does not count their slots (`cone_frees_slots`), so the whole model at this
    /// bound may not reach the same state.
    pub over_bound: Vec<String>,
    /// Use cases the trace runs without their event, as `(use case, event)` in step order
    /// without repeats. The cone that found it left the event's mark out (`cone`), so the
    /// step ran without the event, which the whole model may not allow.
    pub free_triggers: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    /// `Place`, `Pay_1`, or `Tick`.
    pub action: String,
    pub origin: ActionOrigin,
    /// The use case declaration; `Synthesized` for `Tick`.
    pub site: Site,
    /// Every param, in param order.
    pub bound: Vec<Inst>,
    pub changes: Vec<Change>,
}

/// `Order#1`: the 1-based slot, so the label matches TLC's `Order_exists[1]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inst {
    pub entity: String,
    pub slot: u8,
}

impl fmt::Display for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.entity, self.slot)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The instance became present, holding these `(field, value)`s.
    Create {
        at: Inst,
        values: Vec<(String, String)>,
    },
    Delete {
        at: Inst,
    },
    /// A column or owner of an instance present before and after the step.
    Set {
        at: Inst,
        field: String,
        from: String,
        to: String,
        /// The `transitions` arm or `sets` that wrote it; `None` for an actor's choice.
        cause: Option<Site>,
    },
    Tick {
        from: String,
        to: String,
    },
}

impl TransitionSystem {
    /// Every var at its base value: no instance present, the clock at 0.
    pub(crate) fn base_state(&self) -> State {
        self.state_from(|var, _| Some(self.var(var).base))
            .expect("every var has a base")
    }

    /// The state holding `value(var, slot)` in each slot (`now` has slot 0 only); `None` when
    /// some slot has no value. The caller keeps each value inside its var's domain.
    pub fn state_from(
        &self,
        mut value: impl FnMut(VarId, usize) -> Option<Value>,
    ) -> Option<State> {
        let mut state = State(vec![0u8; self.width].into_boxed_slice());
        for var in self.var_ids() {
            let slots = if self.var(var).kind == VarKind::Now {
                1
            } else {
                self.bounds.slots()
            };
            for slot in 0..slots {
                state.set(self, var, slot, value(var, slot)?);
            }
        }
        Some(state)
    }

    pub(crate) fn inst(&self, e: EntId, slot: usize) -> Inst {
        Inst {
            entity: self.entity(e).id.clone(),
            slot: slot as u8 + 1,
        }
    }

    fn field_name(&self, var: VarId) -> String {
        match &self.var(var).kind {
            VarKind::Column { column, .. } => column.clone(),
            VarKind::Prop { key, .. } => key.clone(),
            VarKind::Owner { parent, role, .. } => {
                format!(
                    "owner {}",
                    role.as_deref().unwrap_or(&self.entity(*parent).id)
                )
            }
            VarKind::Run { usecase, .. } => format!("run of {usecase}"),
            VarKind::Raised { event, .. } => format!("raised {event}"),
            VarKind::Exists(_) | VarKind::Now => self.var_name(var),
        }
    }

    pub(crate) fn value_text(&self, var: VarId, value: Value) -> String {
        match &self.var(var).domain {
            d @ Domain::Instance { parent, .. } => match d.slot(value) {
                Some(slot) => self.inst(*parent, usize::from(slot)).to_string(),
                None => "null".to_string(),
            },
            d => AbstractValue::of(d, value).display_with_type(None),
        }
    }

    /// What differs between `pre` and `post`, instance by instance in entity order. `cause`
    /// names the effect that wrote a `(var, slot)`.
    fn changes(
        &self,
        pre: &State,
        post: &State,
        cause: impl Fn(VarId, usize) -> Option<Site>,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        for (i, decl) in self.entities.iter().enumerate() {
            let e = EntId::at(i);
            let fields = || decl.columns.iter().chain(&decl.owners).copied();
            for slot in 0..self.bounds.slots() {
                let at = || self.inst(e, slot);
                match (
                    pre.is_present(self, e, slot),
                    post.is_present(self, e, slot),
                ) {
                    (false, true) => out.push(Change::Create {
                        at: at(),
                        values: fields()
                            .map(|v| {
                                (
                                    self.field_name(v),
                                    self.value_text(v, post.get(self, v, slot)),
                                )
                            })
                            .collect(),
                    }),
                    (true, false) => out.push(Change::Delete { at: at() }),
                    (true, true) => {
                        for v in fields() {
                            let (a, b) = (pre.get(self, v, slot), post.get(self, v, slot));
                            if a != b {
                                out.push(Change::Set {
                                    at: at(),
                                    field: self.field_name(v),
                                    from: self.value_text(v, a),
                                    to: self.value_text(v, b),
                                    cause: cause(v, slot),
                                });
                            }
                        }
                    }
                    (false, false) => {}
                }
            }
        }
        if let Some(now) = self.now {
            let (a, b) = (pre.get(self, now, 0), post.get(self, now, 0));
            if a != b {
                out.push(Change::Tick {
                    from: a.0.to_string(),
                    to: b.0.to_string(),
                });
            }
        }
        out
    }

    /// The pre-existing instances of an initial state.
    pub(crate) fn trace_start(&self, initial: &State) -> Vec<Change> {
        self.changes(&self.base_state(), initial, |_, _| None)
    }

    /// One step of a trace: `act` with `params` moves `pre` to `post`.
    pub(crate) fn trace_step(
        &self,
        act: ActId,
        params: &[u8],
        pre: &State,
        post: &State,
    ) -> TraceStep {
        let action = &self.actions[act.index()];
        let slot = |p: &u8| usize::from(params[usize::from(*p)]);
        let cause = |var: VarId, at: usize| {
            action.effects.iter().rev().find_map(|eff| match eff {
                Effect::Assign {
                    var: v,
                    param,
                    site,
                    ..
                } if *v == var && slot(param) == at => Some(site.clone()),
                Effect::Transition {
                    var: v,
                    param,
                    arms,
                    sites,
                    ..
                } if *v == var && slot(param) == at => {
                    // Of a choice, the arm the step took.
                    let arm = (pre.get(self, var, at), post.get(self, var, at));
                    arms.iter()
                        .position(|a| *a == arm)
                        .map(|i| sites[i].clone())
                }
                Effect::Choose {
                    var: v,
                    param,
                    values,
                    sites,
                    ..
                } if *v == var && slot(param) == at => {
                    // The line of the value the step picked.
                    let value = post.get(self, var, at);
                    values
                        .iter()
                        .position(|x| *x == value)
                        .and_then(|i| sites.get(i).cloned())
                }
                _ => None,
            })
        };
        TraceStep {
            action: action.name.clone(),
            origin: action.origin.clone(),
            site: action.site.clone(),
            bound: action
                .params
                .iter()
                .zip(params)
                .map(|(p, s)| self.inst(p.entity, usize::from(*s)))
                .collect(),
            changes: self.changes(pre, post, cause),
        }
    }

    /// Sets what the cone cannot vouch for in `trace`: `over_bound` and `free_triggers`.
    pub fn annotate(&self, cone: &Cone, trace: &mut Trace) {
        trace.over_bound = self.over_bound(cone, trace);
        trace.free_triggers = self.free_triggers(cone, trace);
    }

    /// The `(use case, event)` of each step whose event's mark the cone leaves out, in step
    /// order without repeats.
    fn free_triggers(&self, cone: &Cone, trace: &Trace) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for step in &trace.steps {
            let Some(action) = self.actions.iter().find(|a| a.name == step.action) else {
                continue;
            };
            for (mark, _) in self.mark_reads(action) {
                if cone.vars.contains(&mark) {
                    continue;
                }
                let VarKind::Raised { event, .. } = &self.var(mark).kind else {
                    unreachable!("a mark is a Raised var")
                };
                let usecase = action
                    .name
                    .strip_suffix(&format!("_by_{event}"))
                    .unwrap_or(&action.name);
                let pair = (usecase.to_string(), event.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        out
    }

    /// Entities whose slots the cone does not count and that the trace creates more often than
    /// the bound allows, in id order.
    fn over_bound(&self, cone: &Cone, trace: &Trace) -> Vec<String> {
        if !self.cone_frees_slots(cone) {
            return Vec::new();
        }
        let mut creates: BTreeMap<String, usize> = BTreeMap::new();
        for step in &trace.steps {
            let Some(action) = self.actions.iter().find(|a| a.name == step.action) else {
                continue;
            };
            for p in &action.params {
                let decl = self.entity(p.entity);
                if p.role == ParamRole::New && !cone.vars.contains(&decl.exists) {
                    *creates.entry(decl.id.clone()).or_default() += 1;
                }
            }
        }
        creates
            .into_iter()
            .filter(|(_, n)| *n > self.bounds.slots())
            .map(|(id, _)| id)
            .collect()
    }
}

impl TransitionSystem {
    /// Matches each step `states[k] -> states[k + 1]` with a move of `actions[k]`. When several
    /// bindings of that action reach the same successor they are symmetric, and the replay
    /// takes the first in `moves` order, the lowest one, as BFS does.
    pub fn replay(&self, states: Vec<State>, actions: &[ActId]) -> Result<Replay, ReplayError> {
        let first = states.first().ok_or(ReplayError::Empty)?;
        if !self.initial_states().contains(first) {
            return Err(ReplayError::NotInitial);
        }
        if actions.len() + 1 != states.len() {
            return Err(ReplayError::Length);
        }
        let mut moves = Vec::with_capacity(actions.len());
        for (k, (pair, &action)) in states.windows(2).zip(actions).enumerate() {
            let m = self
                .moves(&pair[0])
                .find(|m| m.act == action && m.next == pair[1])
                .ok_or(ReplayError::NoMove {
                    state: k + 2,
                    action,
                })?;
            moves.push((m.act, m.params));
        }
        Ok(Replay { states, moves })
    }

    /// The replayed path in model terms, lifted on a reduced system.
    pub(crate) fn replay_trace(&self, replay: &Replay) -> Trace {
        let (start, steps) = self.lifted_replay(replay);
        self.trace_from(&start, &steps)
    }

    /// A path from `start` in model terms.
    pub(crate) fn trace_from(&self, start: &State, steps: &[Step]) -> Trace {
        let mut pre = start;
        Trace {
            start: self.trace_start(start),
            steps: steps
                .iter()
                .map(|(act, params, post)| {
                    let step = self.trace_step(*act, params, pre, post);
                    pre = post;
                    step
                })
                .collect(),
            over_bound: Vec::new(),
            free_triggers: Vec::new(),
        }
    }

    /// The path a printed counterexample of `replay` shows: on a reduced system, the replay
    /// lifted to a path of the system without its reduction, as its states and the action of
    /// each step; otherwise the replay itself.
    pub fn lifted_path(&self, replay: &Replay) -> (Vec<State>, Vec<ActId>) {
        let (start, steps) = self.lifted_replay(replay);
        let actions = steps.iter().map(|s| s.0).collect();
        let states = std::iter::once(start).chain(steps.into_iter().map(|s| s.2));
        (states.collect(), actions)
    }

    /// The replay as a path from its first state, lifted (`lift`) and flattened.
    fn lifted_replay(&self, replay: &Replay) -> (State, Vec<Step>) {
        let steps: Vec<Step> = replay
            .moves
            .iter()
            .zip(replay.states.iter().skip(1))
            .map(|((act, params), post)| (*act, params.clone(), post.clone()))
            .collect();
        self.lifted(&replay.states[0], &steps)
    }

    /// `lift`, flattened.
    pub(crate) fn lifted(&self, start: &State, steps: &[Step]) -> (State, Vec<Step>) {
        let (start, groups) = self.lift(start, steps);
        (start, groups.into_iter().flatten().collect())
    }

    /// The violation of `target` the replayed trace ends in, built by the code BFS reports
    /// with, so its text does not depend on the engine that found the trace. `None` when the
    /// last state (for an after-assert, the last step) does not violate it. On a reduced
    /// system the trace is lifted first and the violation read on the lifted last state.
    pub fn explain(&self, target: Target, replay: &Replay) -> Option<StateDiag> {
        let (start, steps) = self.lifted_replay(replay);
        let last = steps.last().map_or(&start, |s| &s.2);
        let mut d = match target {
            Target::Entity(_) => return None,
            Target::Invariant(i) => self.invariant_violation(i, last)?,
            Target::AfterAssert(i) => {
                let (act, params, _) = steps.last()?;
                self.after_violation(i, *act, params, last)?
            }
        };
        if let Some(t) = d.diag.trace_mut() {
            *t = self.trace_from(&start, &steps);
        }
        Some(d.diag)
    }
}
