//! Temporal properties on hand-built lassos: each kind under a stutter and a back edge, read
//! the way the printed TLA formula reads them.

use super::*;
use crate::lifecycle::{ActId, Inst, Loop, LtlKind, ReplayError, State};

const ORDER: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped, cancelled) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Unpay "支払取消"
usecase Ship "出荷"
usecase Return "返品"
usecase Cancel "取消"
usecase Reopen "再開"
usecase Drop "削除"
creates(Place, Order)
deletes(Drop, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Unpay, paid -> pending)
transitions(Order.status, Ship, paid -> shipped)
transitions(Order.status, Return, shipped -> paid)
transitions(Order.status, Cancel, pending -> cancelled)
transitions(Order.status, Reopen, cancelled -> pending)
property NeverShipped
  always(Order.status != shipped)
property EventuallyShipped
  eventually(Order.status == shipped)
property PaidLeadsToShipped
  leads_to(Order.status == paid, Order.status == shipped)
property PaidLeadsToPending
  leads_to(Order.status == paid, Order.status == pending)
property NeverPending
  always(Order.status != pending)
"#;

const NEVER_SHIPPED: usize = 0;
const EVENTUALLY_SHIPPED: usize = 1;
const PAID_LEADS_TO_SHIPPED: usize = 2;
const PAID_LEADS_TO_PENDING: usize = 3;
const NEVER_PENDING: usize = 4;

fn system(bounds: Bounds) -> TransitionSystem {
    TransitionSystem::build(&model_from(ORDER), bounds, &Scope::ALL).ts
}

/// The states and actions of the path that takes each `(use case, slot)` step from the
/// initial state, the slot being the one instance the use case binds.
fn walk(ts: &TransitionSystem, steps: &[(&str, u8)]) -> (Vec<State>, Vec<ActId>) {
    let mut states = vec![ts.initial_states()[0].clone()];
    let mut acts = Vec::new();
    for &(name, slot) in steps {
        let m = ts
            .moves(states.last().unwrap())
            .find(|m| ts.actions()[m.act.index()].name == name && *m.params == [slot])
            .unwrap_or_else(|| panic!("{name} [{slot}] does not run"));
        acts.push(m.act);
        states.push(m.next);
    }
    (states, acts)
}

/// `steps`, then nothing changes.
fn stutter(ts: &TransitionSystem, steps: &[(&str, u8)]) -> crate::lifecycle::LassoReplay {
    let (states, acts) = walk(ts, steps);
    ts.replay_lasso(states, &acts, Loop::Stutter)
        .expect("the path replays")
}

/// `steps`, whose last one returns to state `to` (from 1), then that loop forever.
fn back(ts: &TransitionSystem, steps: &[(&str, u8)], to: usize) -> crate::lifecycle::LassoReplay {
    let (mut states, acts) = walk(ts, steps);
    let target = states.pop().unwrap();
    assert_eq!(
        target,
        states[to - 1],
        "the last step returns to state {to}"
    );
    ts.replay_lasso(states, &acts, Loop::Back { to })
        .expect("the lasso replays")
}

/// A violation's kind, instance, witness, loop, and its steps' actions.
type Verdict = (LtlKind, Vec<String>, usize, Loop, Vec<String>);

fn verdict(d: Option<StateDiag>) -> Option<Verdict> {
    match d? {
        StateDiag::PropertyViolated {
            kind,
            instance,
            witness,
            lasso,
            trace,
            ..
        } => Some((
            kind,
            instance.iter().map(Inst::to_string).collect(),
            witness,
            lasso,
            trace.steps.iter().map(|s| s.action.clone()).collect(),
        )),
        other => panic!("not a property violation: {other:?}"),
    }
}

fn names(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

#[test]
fn always_fails_where_its_condition_is_false() {
    let ts = system(Bounds::STATES);
    let l = stutter(&ts, &[("Place", 0), ("Pay", 0), ("Ship", 0)]);
    assert_eq!(
        verdict(ts.explain_property(NEVER_SHIPPED, &l)),
        Some((
            LtlKind::Always,
            names(&["Order#1"]),
            3,
            Loop::Stutter,
            names(&["Place", "Pay", "Ship"])
        ))
    );
    let l = back(
        &ts,
        &[("Place", 0), ("Pay", 0), ("Ship", 0), ("Return", 0)],
        3,
    );
    assert_eq!(
        verdict(ts.explain_property(NEVER_SHIPPED, &l)),
        Some((
            LtlKind::Always,
            names(&["Order#1"]),
            3,
            Loop::Back { to: 3 },
            names(&["Place", "Pay", "Ship", "Return"])
        )),
        "the back edge is the last step"
    );
}

#[test]
fn always_reads_an_instance_only_while_it_is_present() {
    let ts = system(Bounds::STATES);
    // The absent slot of the start state already holds pending.
    let l = stutter(&ts, &[("Place", 0)]);
    let (_, _, witness, _, _) = verdict(ts.explain_property(NEVER_PENDING, &l)).unwrap();
    assert_eq!(witness, 1, "not at the start, where Order#1 is absent");
}

#[test]
fn eventually_fails_when_its_condition_never_comes() {
    let ts = system(Bounds::STATES);
    let l = stutter(&ts, &[("Place", 0)]);
    assert_eq!(
        verdict(ts.explain_property(EVENTUALLY_SHIPPED, &l)),
        Some((
            LtlKind::Eventually,
            names(&["Order#1"]),
            1,
            Loop::Stutter,
            names(&["Place"])
        ))
    );
    let l = back(&ts, &[("Place", 0), ("Cancel", 0), ("Reopen", 0)], 2);
    assert_eq!(
        verdict(ts.explain_property(EVENTUALLY_SHIPPED, &l)),
        Some((
            LtlKind::Eventually,
            names(&["Order#1"]),
            1,
            Loop::Back { to: 2 },
            names(&["Place", "Cancel", "Reopen"])
        ))
    );
    let shipped = stutter(&ts, &[("Place", 0), ("Pay", 0), ("Ship", 0)]);
    assert_eq!(ts.explain_property(EVENTUALLY_SHIPPED, &shipped), None);
}

#[test]
fn eventually_leaves_an_instance_once_it_is_deleted() {
    let ts = system(Bounds::STATES);
    // The dropped slot is back at its base, pending, forever; it is no longer present, so
    // the formula does not ask it to ship again.
    let l = stutter(&ts, &[("Place", 0), ("Pay", 0), ("Ship", 0), ("Drop", 0)]);
    assert_eq!(ts.explain_property(EVENTUALLY_SHIPPED, &l), None);
    let l = stutter(&ts, &[("Place", 0), ("Drop", 0)]);
    let (_, _, witness, _, _) = verdict(ts.explain_property(EVENTUALLY_SHIPPED, &l)).unwrap();
    assert_eq!(witness, 1, "present after Place, and never shipped");
}

#[test]
fn leads_to_fails_when_the_goal_never_follows_the_trigger() {
    let ts = system(Bounds::STATES);
    let l = stutter(&ts, &[("Place", 0), ("Pay", 0)]);
    assert_eq!(
        verdict(ts.explain_property(PAID_LEADS_TO_SHIPPED, &l)),
        Some((
            LtlKind::LeadsTo,
            names(&["Order#1"]),
            2,
            Loop::Stutter,
            names(&["Place", "Pay"])
        ))
    );
    // Pay and Unpay forever: the loop enters paid again but never shipped.
    let l = back(
        &ts,
        &[("Place", 0), ("Pay", 0), ("Unpay", 0), ("Pay", 0)],
        3,
    );
    assert_eq!(
        verdict(ts.explain_property(PAID_LEADS_TO_SHIPPED, &l)),
        Some((
            LtlKind::LeadsTo,
            names(&["Order#1"]),
            2,
            Loop::Back { to: 3 },
            names(&["Place", "Pay", "Unpay", "Pay"])
        ))
    );
    // Ship and Return forever: every paid is followed by shipped inside the loop.
    let l = back(
        &ts,
        &[("Place", 0), ("Pay", 0), ("Ship", 0), ("Return", 0)],
        3,
    );
    assert_eq!(ts.explain_property(PAID_LEADS_TO_SHIPPED, &l), None);
    // The loop starts at shipped, so the paid that ends the finite part is followed by the
    // shipped before it.
    let l = back(
        &ts,
        &[
            ("Place", 0),
            ("Pay", 0),
            ("Ship", 0),
            ("Return", 0),
            ("Ship", 0),
        ],
        4,
    );
    assert_eq!(ts.explain_property(PAID_LEADS_TO_SHIPPED, &l), None);
}

#[test]
fn a_leads_to_goal_holds_on_a_deleted_instance() {
    // `(present /\ x) ~> y` guards only the trigger: the dropped slot is back at its base,
    // pending, so the goal holds, as TLC reads it.
    let ts = system(Bounds::STATES);
    let l = stutter(&ts, &[("Place", 0), ("Pay", 0), ("Drop", 0)]);
    assert_eq!(ts.explain_property(PAID_LEADS_TO_PENDING, &l), None);
    let kept = stutter(&ts, &[("Place", 0), ("Pay", 0)]);
    assert!(ts.explain_property(PAID_LEADS_TO_PENDING, &kept).is_some());
}

#[test]
fn the_instance_is_the_first_binding_that_fails() {
    let ts = system(Bounds::VERIFY);
    let l = stutter(&ts, &[("Place", 0), ("Place", 1), ("Pay", 1)]);
    let (_, instance, witness, _, _) =
        verdict(ts.explain_property(PAID_LEADS_TO_SHIPPED, &l)).unwrap();
    assert_eq!((instance, witness), (names(&["Order#2"]), 3));
}

#[test]
fn a_back_edge_no_move_takes_does_not_replay() {
    let ts = system(Bounds::STATES);
    // State 4 is pending again; Cancel takes it back to state 3, Reopen does not.
    let (states, acts) = walk(&ts, &[("Place", 0), ("Cancel", 0), ("Reopen", 0)]);
    let (cancel, reopen) = (acts[1], acts[2]);
    let with = |edge: ActId| [&acts[..], &[edge]].concat();
    assert!(ts
        .replay_lasso(states.clone(), &with(cancel), Loop::Back { to: 3 })
        .is_ok());
    assert_eq!(
        ts.replay_lasso(states.clone(), &with(reopen), Loop::Back { to: 3 }),
        Err(ReplayError::NoBackMove {
            to: 3,
            action: reopen
        })
    );
    assert_eq!(
        ts.replay_lasso(states.clone(), &with(cancel), Loop::Back { to: 9 }),
        Err(ReplayError::NoBackMove {
            to: 9,
            action: cancel
        }),
        "no state 9"
    );
    assert_eq!(
        ts.replay_lasso(states, &acts, Loop::Back { to: 3 }),
        Err(ReplayError::Length),
        "a back edge needs its own action"
    );
}
