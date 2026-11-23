//! `can_reach(goal)`: from every reachable state where the instance is present, some path on
//! which it stays present reaches the goal. BFS decides it on the property's cone, and the
//! cone decides what the whole system decides.

use super::cone::load;
use super::*;
use crate::lifecycle::{Inst, Target};

const UNMET: &str = "samples/formal-verification/can_reach_unmet.rdra";
const MET: &str = "samples/formal-verification/can_reach_met.rdra";

fn unreachable_goals(diags: &[Diag]) -> Vec<&StateDiag> {
    diags
        .iter()
        .map(|d| &d.diag)
        .filter(|d| matches!(d, StateDiag::GoalUnreachable { .. }))
        .collect()
}

/// Whether `can_reach` `i` fails on the whole system explored unrestricted, the reference the
/// cone's verdict must match.
fn fails_on_the_whole_system(model: &SemanticModel, bounds: Bounds, i: usize) -> bool {
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let run = ts.run(Caps::new(usize::MAX, 1 << 20));
    assert!(run.complete);
    run.system.reach_failure(i, &run.graph()).is_some()
}

/// What `verify`'s BFS says about each `can_reach` of `model`, at `bounds`.
fn reach_verdicts(model: &SemanticModel, bounds: Bounds) -> Vec<Diag> {
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let run = ts.explore_targets(&ts.reach_targets(), Caps::for_instances(1));
    run.verdicts().to_vec()
}

fn fails_on_its_cone(model: &SemanticModel, bounds: Bounds, i: usize) -> bool {
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    // A cone that frees slots reaches a superset, where `EF` may find a path the model lacks.
    assert!(!ts.cone_frees_slots(&ts.target_cone(Target::Reach(i))));
    let run = ts.explore_targets(&[Target::Reach(i)], Caps::new(usize::MAX, 1 << 20));
    !unreachable_goals(run.verdicts()).is_empty()
}

/// An Order that has lost its stock and its cancellation window moves between pending and
/// on_hold forever. Both states have a way out, so no deadlock is reported; `can_reach` is,
/// with a shortest trace into the trap and the states the Order is then confined to.
#[test]
fn a_cycle_with_no_way_out_fails_where_no_deadlock_does() {
    let report = super::super::derive_states(&load(UNMET), &Scope::ALL, Caps::for_instances(1));
    assert!(
        !report
            .diags
            .iter()
            .any(|d| matches!(d.diag, StateDiag::Deadlock { .. })),
        "{:?}",
        report.diags
    );
    let verdicts = reach_verdicts(&load(UNMET), Bounds::STATES);
    let found = unreachable_goals(&verdicts);
    let [StateDiag::GoalUnreachable {
        name,
        instance,
        confined,
        states,
        trace,
        ..
    }] = found.as_slice()
    else {
        panic!("one GoalUnreachable expected: {found:?}");
    };
    assert_eq!(name, "OrderCanFinish");
    assert_eq!(
        instance,
        &[Inst {
            entity: "Order".into(),
            slot: 1
        }]
    );
    let actions: Vec<&str> = trace.steps.iter().map(|s| s.action.as_str()).collect();
    assert_eq!(actions, ["Place", "CloseWindow", "LoseStock"]);
    assert_eq!(confined, &["status = pending", "status = on_hold"]);
    assert_eq!(*states, 2);
    assert_eq!(found[0].name(&[]), "OrderCanFinish");
}

/// With a way to restock, every reachable state can still finish, though an Order may be
/// held and resumed forever: `can_reach` asks for a path, not for progress.
#[test]
fn a_way_back_to_the_goal_passes() {
    let verdicts = reach_verdicts(&load(MET), Bounds::STATES);
    assert!(unreachable_goals(&verdicts).is_empty(), "{verdicts:?}");
}

/// The only way to `done` is a use case an event starts, and the event's raiser can never
/// run. A cone that left the event's mark out would let `Finish` run freely and pass; the
/// cone keeps it, and fails as the whole system does.
#[test]
fn a_goal_behind_an_event_that_is_never_raised_is_unreachable() {
    let model = model_from(
        r#"
entity Order "x" { id: Int @pk
  status: Enum(open, done) @default(open) }
entity Approval "x" { id: Int @pk
  status: Enum(waiting, granted) @default(waiting) }
relate(Approval, Order, N:1)
usecase Place "x"
usecase Request "x"
usecase Grant "x"
usecase Finish "x"
event EvGranted "x"
creates(Place, Order)
creates(Request, Approval)
transitions(Grant, Approval.status, waiting -> granted)
raises(Grant, EvGranted)
triggers(EvGranted, Finish)
transitions(Finish, Order.status, open -> done)
condition Never "x"
holds(Never, Approval.status == granted)
applies_to(Never, Grant)
property OrderFinishes can_reach(Order.status == done)
"#,
    );
    assert!(fails_on_the_whole_system(&model, Bounds::STATES, 0));
    assert!(fails_on_its_cone(&model, Bounds::STATES, 0));
}

/// The only way to `done` creates a Receipt, and the one Receipt slot is taken by a receipt
/// that is never removed. A cone that did not count Receipt slots would pass; the cone counts
/// them, and fails as the whole system does.
#[test]
fn a_goal_that_needs_a_slot_the_bound_never_frees_is_unreachable() {
    let model = model_from(
        r#"
entity Order "x" { id: Int @pk
  status: Enum(open, done) @default(open) }
entity Receipt "x" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "x"
usecase Issue "x"
usecase Finish "x"
creates(Place, Order)
creates(Issue, Receipt)
creates(Finish, Receipt)
transitions(Finish, Order.status, open -> done)
property OrderFinishes can_reach(Order.status == done)
"#,
    );
    assert!(fails_on_the_whole_system(&model, Bounds::STATES, 0));
    assert!(fails_on_its_cone(&model, Bounds::STATES, 0));
}

/// The instance must reach the goal itself: deleting it is not finishing it, so an Order that
/// can only be purged fails.
#[test]
fn deleting_the_instance_does_not_reach_the_goal() {
    let model = model_from(
        r#"
entity Order "x" { id: Int @pk
  status: Enum(open, done) @default(open)
  in_stock: Bool @default(true) }
usecase Place "x"
usecase Finish "x"
usecase LoseStock "x"
usecase Purge "x"
creates(Place, Order)
updates(LoseStock, Order)
deletes(Purge, Order)
transitions(Finish, Order.status, open -> done)
sets(usecase::LoseStock, Order.in_stock == false)
condition InStock "x"
holds(InStock, Order.in_stock == true)
applies_to(InStock, Finish)
property OrderFinishes can_reach(Order.status == done)
"#,
    );
    let verdicts = reach_verdicts(&model, Bounds::STATES);
    let found = unreachable_goals(&verdicts);
    let [StateDiag::GoalUnreachable { trace, .. }] = found.as_slice() else {
        panic!("one GoalUnreachable expected: {found:?}");
    };
    let actions: Vec<&str> = trace.steps.iter().map(|s| s.action.as_str()).collect();
    assert_eq!(actions, ["Place", "LoseStock"]);
}

/// On the samples, at one instance and at two, the cone's verdict is the whole system's.
#[test]
fn the_cone_decides_what_the_whole_system_decides() {
    let two = Bounds::uniform(std::num::NonZeroU8::new(2).unwrap(), Bounds::STATES.int_max);
    for (sample, fails) in [(UNMET, true), (MET, false)] {
        let model = load(sample);
        for bounds in [Bounds::STATES, two.clone()] {
            assert_eq!(
                fails_on_the_whole_system(&model, bounds.clone(), 0),
                fails,
                "{sample}"
            );
            assert_eq!(fails_on_its_cone(&model, bounds, 0), fails, "{sample}");
        }
    }
}

/// `check` and `states` verify safety only: they explore no `can_reach` cone.
#[test]
fn check_and_states_do_not_evaluate_can_reach() {
    let report = super::super::derive_states(&load(UNMET), &Scope::ALL, Caps::for_instances(1));
    assert!(unreachable_goals(&report.diags).is_empty());
}

/// `.along` binds only linked rows, as for the other forms: a captured payment's own order
/// can always be paid, while beside another order that is cancelled it cannot. At two
/// instances the scoped property holds and the global one fails, on the cone as on the whole
/// system.
#[test]
fn along_binds_only_linked_rows() {
    let src = |chain: &str| {
        format!(
            r#"
entity Order "x" {{ id: Int @pk
  status: Enum(draft, paid, cancelled) @default(draft) }}
entity Payment "x" {{ id: Int @pk
  status: Enum(pending, captured, voided) @default(pending) }}
relate(Payment, Order, N:1)
usecase Pay "x"
usecase CancelOrder "x"
transitions(Pay, Order.status, draft -> paid)
transitions(Pay, Payment.status, pending -> captured)
transitions(CancelOrder, Order.status, draft -> cancelled)
property P can_reach((not (Payment.status == captured)) or (Order.status == paid)){chain}
"#
        )
    };
    let two = Bounds::uniform(std::num::NonZeroU8::new(2).unwrap(), Bounds::STATES.int_max);
    let along = model_from(&src(".along(Payment, Order)"));
    assert!(!fails_on_the_whole_system(&along, two.clone(), 0));
    assert!(!fails_on_its_cone(&along, two.clone(), 0));
    let global = model_from(&src(".global()"));
    assert!(fails_on_the_whole_system(&global, two.clone(), 0));
    assert!(fails_on_its_cone(&global, two, 0));
}
