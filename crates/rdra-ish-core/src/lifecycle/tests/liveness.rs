//! `eventually` and `leads_to` decided on an explored graph under weak fairness: stops and
//! cycles, enabledness read on every move, self-loops, capped graphs, `.along`, and deleted
//! rows. Every counterexample certifies itself (`decide_liveness` returns no `LassoFault`).

use super::*;
use crate::lifecycle::{Fairness, LassoReplay, Loop, PropertyVerdict, Unfair};

/// A verdict in short: `None` when the property holds, else the actions of the lasso's trace,
/// its back edge last, and how it loops.
type Lasso = Option<(Vec<String>, Loop)>;

fn decide(src: &str, name: &str, fairness: Fairness) -> Lasso {
    decide_under(src, name, fairness, Caps::VERIFY).expect("a complete graph decides")
}

/// `decide` under `caps`; `Err(())` when the property is not decided.
fn decide_under(src: &str, name: &str, fairness: Fairness, caps: Caps) -> Result<Lasso, ()> {
    let ts = system_from(src, Bounds::VERIFY);
    let i = ts
        .properties()
        .iter()
        .position(|p| p.name == name)
        .unwrap_or_else(|| panic!("no property {name}"));
    let verdict = ts
        .run(caps)
        .decide_liveness(i, fairness)
        .unwrap_or_else(|fault| panic!("{name}: the lasso fails its own check: {fault:?}"));
    match verdict {
        PropertyVerdict::Holds => Ok(None),
        PropertyVerdict::Undecided => Err(()),
        PropertyVerdict::Violated(diag) => match *diag {
            StateDiag::PropertyViolated { lasso, trace, .. } => Ok(Some((
                trace.steps.iter().map(|s| s.action.clone()).collect(),
                lasso.expect("a liveness counterexample is a lasso"),
            ))),
            other => panic!("{name}: {other:?}"),
        },
    }
}

const SHIP: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped)
property PaidShipsFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// Without `.fair` a behavior may stop at a paid order; `WF_vars(Next)` forbids stopping
/// where a step can run, and `.fair(Ship)` forbids stopping where `Ship` can.
#[test]
fn a_stop_is_fair_only_where_nothing_fair_can_run() {
    assert_eq!(
        decide(SHIP, "PaidShips", Fairness::Plain),
        Some((names(&["Place", "Pay"]), Loop::Stutter))
    );
    assert_eq!(decide(SHIP, "PaidShips", Fairness::WfNext), None);
    assert_eq!(decide(SHIP, "PaidShipsFair", Fairness::Plain), None);
}

const UNPAY: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Unpay "支払取消"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Unpay, Order.status, paid -> pending)
transitions(Ship, Order.status, paid -> shipped)
property PaidShipsFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// Paying and unpaying forever passes a state where `Ship` is disabled, so the cycle is fair
/// to it, under either fairness: the lasso loops back to the paid order.
#[test]
fn a_cycle_that_passes_a_disabled_state_is_fair() {
    let cycle = Some((
        names(&["Place", "Pay", "Unpay", "Pay"]),
        Loop::Back { to: 3 },
    ));
    assert_eq!(decide(UNPAY, "PaidShipsFair", Fairness::Plain), cycle);
    assert_eq!(decide(UNPAY, "PaidShipsFair", Fairness::WfNext), cycle);
}

const HOLD: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, held, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Hold "保留"
usecase Release "保留解除"
usecase Ship "出荷"
usecase ShipPaid "支払済出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Hold, Order.status, paid -> held)
transitions(Release, Order.status, held -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Ship, Order.status, held -> shipped)
transitions(ShipPaid, Order.status, paid -> shipped)
property ShipsFromBoth leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
property ShipsFromPaid leads_to(Order.status == paid, Order.status == shipped).fair(ShipPaid)
"#;

/// `Ship` is enabled at both states of the hold cycle and its only step leaves the cycle for
/// the goal: enabledness reads that step, so no fair behavior holds forever. `ShipPaid` is
/// disabled while held, so holding and releasing forever is fair to it.
#[test]
fn an_instance_whose_step_leaves_the_cycle_is_still_enabled() {
    assert_eq!(decide(HOLD, "ShipsFromBoth", Fairness::Plain), None);
    assert_eq!(
        decide(HOLD, "ShipsFromPaid", Fairness::Plain),
        Some((
            names(&["Place", "Pay", "Hold", "Release"]),
            Loop::Back { to: 3 }
        ))
    );
}

const MARK: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, shipped) @default(pending)
  flag: Bool @default(false)
}
usecase Place "注文"
usecase Mark "印付け"
creates(Place, Order)
sets(Mark, Order.flag == true)
property Ships eventually(Order.status == shipped).fair(Mark)
property MarkedShips leads_to(Order.flag == true, Order.status == shipped)
"#;

/// Once the flag is set, `Mark` runs and changes nothing: that is neither a step of `Mark`
/// nor makes it enabled, so the marked order may stop under `.fair(Mark)` and under
/// `WF_vars(Next)`.
#[test]
fn a_self_loop_is_neither_a_step_nor_enabling() {
    assert_eq!(
        decide(MARK, "Ships", Fairness::Plain),
        Some((names(&["Place", "Mark"]), Loop::Stutter)),
        "the unmarked order may not stop, since Mark can still set the flag"
    );
    // The first marked order may still place the second, so it stops once both are marked.
    assert_eq!(
        decide(MARK, "MarkedShips", Fairness::WfNext),
        Some((names(&["Place", "Mark", "Place", "Mark"]), Loop::Stutter))
    );
}

/// A graph the cap stopped decides a counterexample it reached, and nothing else. BFS reaches
/// the paid order third and expands it fourth, of 13 states.
#[test]
fn a_capped_graph_decides_only_a_counterexample() {
    let capped = Caps::new(Caps::for_instances(1).patterns, 6);
    assert_eq!(decide(SHIP, "PaidShipsFair", Fairness::Plain), None);
    assert_eq!(
        decide_under(SHIP, "PaidShipsFair", Fairness::Plain, capped),
        Err(())
    );
    assert_eq!(
        decide_under(SHIP, "PaidShips", Fairness::Plain, capped),
        Ok(Some((names(&["Place", "Pay"]), Loop::Stutter)))
    );
    // The cap stops the run before it expands the paid order, or while it does: that order
    // may have moves the run did not record, so it is no stop.
    for cap in [3, 5] {
        let tight = Caps::new(Caps::for_instances(1).patterns, cap);
        assert_eq!(
            decide_under(SHIP, "PaidShips", Fairness::Plain, tight),
            Err(()),
            "cap {cap}"
        );
    }
}

const RELINK: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(draft, paid) @default(draft) }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, N:1)
usecase Capture "確定"
usecase PayOrder "入金"
usecase Move "付け替え"
transitions(Capture, Payment.status, pending -> captured)
transitions(PayOrder, Order.status, draft -> paid)
relinks(Move, Payment, Order)
property CapturedPaid
  leads_to(Payment.status == captured, Order.status == paid).along(Payment, Order).fair(PayOrder)
property CapturedPaidUnfair
  leads_to(Payment.status == captured, Order.status == paid).along(Payment, Order)
"#;

/// The goal reads the order bound at the trigger, not the order the key names later: moving
/// the payment does not change what it owes, and `PayOrder` is fair to every order.
#[test]
fn a_moved_key_keeps_the_obligation_of_its_trigger() {
    assert_eq!(decide(RELINK, "CapturedPaid", Fairness::Plain), None);
    let Some((actions, Loop::Stutter)) = decide(RELINK, "CapturedPaidUnfair", Fairness::Plain)
    else {
        panic!("without .fair a captured payment's order may stay unpaid");
    };
    assert_eq!(actions, names(&["Capture"]));
}

const DROP: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, shipped) @default(pending) }
usecase Place "注文"
usecase Ship "出荷"
usecase Drop "削除"
creates(Place, Order)
transitions(Ship, Order.status, pending -> shipped)
deletes(Drop, Order)
property Ships eventually(Order.status == shipped).fair(Ship, Drop)
"#;

/// A row deleted after its trigger still owes its goal, read on its base values, so a fair
/// `Drop` ends the order unshipped.
#[test]
fn a_deleted_row_still_owes_its_goal() {
    assert_eq!(
        decide(DROP, "Ships", Fairness::Plain),
        Some((names(&["Place", "Drop"]), Loop::Stutter))
    );
}

/// The lasso that runs each use case in turn, its first move to another state each time,
/// then `tail`; with `Loop::Back` the last step returns to the state it names.
fn lasso_of(ts: &TransitionSystem, steps: &[&str], tail: Loop) -> LassoReplay {
    let mut states = vec![ts.initial_states()[0].clone()];
    let mut acts = Vec::new();
    for name in steps {
        let at = states.last().expect("a state").clone();
        let m = ts
            .moves(&at)
            .find(|m| ts.actions()[m.act.index()].name == *name && m.next != at)
            .unwrap_or_else(|| panic!("{name} does not run"));
        acts.push(m.act);
        states.push(m.next);
    }
    if let Loop::Back { .. } = tail {
        states.pop();
    }
    ts.replay_lasso(states, &acts, tail)
        .expect("the lasso replays")
}

/// The fairness check reads `moves`, not the search: it rejects a stop where a fair step can
/// run, a loop that starves an instance enabled all along it, and under `WF_vars(Next)` a stop
/// where any step can run.
#[test]
fn the_fairness_check_rejects_an_unfair_lasso() {
    let ts = system_from(SHIP, Bounds::VERIFY);
    let (plain, fair) = (0, 1);
    let paid = lasso_of(&ts, &["Place", "Pay"], Loop::Stutter);
    assert!(matches!(
        ts.unfair_loop(fair, &paid, Fairness::Plain),
        Some(Unfair::Starved { .. })
    ));
    assert_eq!(ts.unfair_loop(plain, &paid, Fairness::Plain), None);
    assert_eq!(
        ts.unfair_loop(plain, &paid, Fairness::WfNext),
        Some(Unfair::Stops)
    );

    let ts = system_from(HOLD, Bounds::VERIFY);
    let (both, paid_only) = (0, 1);
    let held = lasso_of(
        &ts,
        &["Place", "Pay", "Hold", "Release"],
        Loop::Back { to: 3 },
    );
    assert!(matches!(
        ts.unfair_loop(both, &held, Fairness::Plain),
        Some(Unfair::Starved { .. })
    ));
    assert_eq!(ts.unfair_loop(paid_only, &held, Fairness::Plain), None);
}
