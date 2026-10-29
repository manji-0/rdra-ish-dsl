//! A TLC counterexample in model terms: its states decoded, replayed through the step
//! relation BFS uses, and the violated check built by the code BFS reports with.
//! `tests/engine_equivalence.rs` includes this file by path beside `tlc_output.rs`.

use rdra_ish_core::lifecycle::{
    ActId, Loop, ReplayError, State, StateDiag, Target, TransitionSystem,
};
use rdra_ish_emit::tla::TlaNames;

use crate::tlc_output::{Lasso, Violation, ViolationKind};

/// The violation `ts` (the system TLC checked, printed with `names`) explains for TLC's
/// trace. An `Err` says where the trace leaves the step relation: the engines disagree.
pub fn explain_violation(
    ts: &TransitionSystem,
    names: &TlaNames,
    v: &Violation,
) -> Result<StateDiag, String> {
    let name = v.name.as_deref().ok_or("TLC names no check")?;
    let target = match v.kind {
        ViolationKind::Invariant => names.invariant_index(name).map(Target::Invariant),
        ViolationKind::ActionProperty => names.after_index(name).map(Target::AfterAssert),
        ViolationKind::Temporal | ViolationKind::Other => None,
    }
    .ok_or_else(|| format!("TLC check {name} is no invariant or after-assert of the cone"))?;
    let (states, actions) = decode(ts, names, v)?;
    let replay = ts
        .replay(states, &actions)
        .map_err(|e| replay_error(v, e))?;
    ts.explain(target, &replay)
        .ok_or_else(|| format!("the last TLC state does not violate {name}"))
}

/// The violation of `properties()[i]` that TLC's lasso shows, in a run that checks that
/// property alone. TLC does not name the property it reports, so the run says which.
pub fn explain_property(
    ts: &TransitionSystem,
    names: &TlaNames,
    i: usize,
    v: &Violation,
) -> Result<StateDiag, String> {
    let name = names.property(i);
    if v.kind != ViolationKind::Temporal {
        return Err(format!("TLC reports `{}` in the run of {name}", v.line));
    }
    let (states, mut actions) = decode(ts, names, v)?;
    let tail = match v.lasso.as_ref().ok_or("TLC printed no lasso")? {
        Lasso::Stutter => Loop::Stutter,
        Lasso::Back { to, label } => {
            actions.push(names.action_index(ts, label).ok_or_else(|| {
                format!("TLC's step back to state {to} (label {label}) names no use case")
            })?);
            Loop::Back { to: *to }
        }
    };
    let replay = ts
        .replay_lasso(states, &actions, tail)
        .map_err(|e| replay_error(v, e))?;
    let mut diag = ts
        .explain_property(i, &replay)
        .ok_or_else(|| format!("TLC's lasso does not violate {name}"))?;
    // The module renames a property whose name another identifier took; the finding carries
    // the name TLC checked.
    if let StateDiag::PropertyViolated { name: n, .. } = &mut diag {
        *n = name.to_string();
    }
    Ok(diag)
}

/// TLC's states decoded, and the action each step after the first names.
fn decode(
    ts: &TransitionSystem,
    names: &TlaNames,
    v: &Violation,
) -> Result<(Vec<State>, Vec<ActId>), String> {
    let states = v
        .trace
        .iter()
        .enumerate()
        .map(|(k, step)| {
            names
                .decode(ts, &step.state)
                .ok_or_else(|| format!("TLC state {} is no state of the cone", k + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let actions = v
        .trace
        .iter()
        .enumerate()
        .skip(1)
        .map(|(k, step)| {
            names.action_index(ts, &step.label).ok_or_else(|| {
                format!(
                    "TLC step {} (label {}) names no use case of the cone",
                    k + 1,
                    step.label
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((states, actions))
}

fn replay_error(v: &Violation, e: ReplayError) -> String {
    match e {
        ReplayError::Empty | ReplayError::Length => "TLC printed no trace".to_string(),
        ReplayError::NotInitial => "TLC state 1 is no initial state of the cone".to_string(),
        ReplayError::NoMove { state, .. } => format!(
            "TLC step {state} (label {}) matches no move of the cone",
            v.trace[state - 1].label
        ),
        ReplayError::NoBackMove { to, .. } => {
            let label = match &v.lasso {
                Some(Lasso::Back { label, .. }) => label.as_str(),
                _ => "",
            };
            format!("TLC's step back to state {to} (label {label}) matches no move of the model")
        }
    }
}
