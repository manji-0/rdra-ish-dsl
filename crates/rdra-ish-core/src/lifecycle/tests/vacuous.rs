//! Rules that hold vacuously: `StateDiag::VacuousRule`.

use std::collections::BTreeMap;

use super::model_from;
use std::num::NonZeroU8;

use crate::lifecycle::{
    Bounds, Caps, Diag, RunEnd, Scope, StateDiag, Target, TargetExploration, TransitionSystem,
    UnmetPremise,
};

/// The premises no reachable state meets, per rule that holds vacuously.
fn vacuous(src: &str, bounds: Bounds, caps: Caps) -> BTreeMap<String, Vec<String>> {
    let ts = TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts;
    texts(ts.explore_targets(&ts.targets(), caps).verdicts())
}

fn texts(verdicts: &[Diag]) -> BTreeMap<String, Vec<String>> {
    verdicts
        .iter()
        .filter_map(|d| match &d.diag {
            StateDiag::VacuousRule { name, unmet, .. } => {
                let text = |u: &UnmetPremise| match u {
                    UnmetPremise::Rows { text, .. } => text.clone(),
                    other => other.to_string(),
                };
                Some((name.clone(), unmet.iter().map(text).collect()))
            }
            _ => None,
        })
        .collect()
}

const ORDERS: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid, void) @default(open)
  flagged: Bool @default(false)
}
usecase Place "注文"
usecase Pay "支払"
usecase Flag "印"
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
sets(Flag, Order.flagged == true)
"#;

/// No order is ever `void`, so a rule about void orders says nothing: the guards of a
/// `.when` never hold, and a `forbidden` pair names a value no row takes.
#[test]
fn a_rule_about_a_state_no_row_reaches_holds_vacuously() {
    let src = format!(
        "{ORDERS}invariant(Order).when(status == void).then(flagged == true)\n\
         forbidden(Order, status == void, flagged == true)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::for_instances(1));
    let void = vec!["status=void".to_string()];
    assert_eq!(
        found,
        BTreeMap::from([
            ("Forbidden_Order_0".to_string(), void.clone()),
            ("Inv_Order_0".to_string(), void),
        ])
    );
}

/// What a `forbidden` forbids is unreachable when it holds: that is its claim, not a
/// premise. A pair whose values each some row takes, never together, is not vacuous, and
/// neither is a rule of one condition, nor a rule whose guards hold somewhere.
#[test]
fn a_rule_whose_premises_some_state_meets_is_not_vacuous() {
    // Flag needs an open order and Pay an unflagged one, so no paid order is flagged.
    let src = format!(
        "{ORDERS}condition Open \"未払\"\nholds(Open, Order.status == open)\napplies_to(Open, Flag)\n\
         condition Plain \"印なし\"\nholds(Plain, Order.flagged == false)\napplies_to(Plain, Pay)\n\
         forbidden(Order, status == paid, flagged == true)\n\
         forbidden(Order, status == void)\n\
         invariant(Order).when(status == paid).then(flagged == false)\n"
    );
    assert_eq!(
        vacuous(&src, Bounds::STATES, Caps::for_instances(1)),
        BTreeMap::new()
    );
}

/// A rule over an entity no use case can create ranges over no row at all; that alone is
/// reported, not each of its conditions.
#[test]
fn a_rule_over_rows_that_never_exist_holds_vacuously() {
    let src = format!(
        "{ORDERS}entity Ghost \"幽霊\" {{ id: Int @pk  seen: Bool @default(false)  loud: Bool @default(false) }}\n\
         usecase Summon \"召喚\"\ncreates(Summon, Ghost)\nsets(Summon, Ghost.seen == true)\n\
         condition Never \"起きない\"\nholds(Never, Order.status == void)\napplies_to(Never, Summon)\n\
         forbidden(Ghost, seen == true, loud == true)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::for_instances(1));
    assert_eq!(
        found.get("Forbidden_Ghost_0"),
        Some(&vec![String::new()]),
        "{found:?}"
    );
}

/// A violated rule is not vacuous, and a run a cap stops has not seen every state, so it
/// reports no vacuity.
#[test]
fn a_violated_rule_and_a_capped_run_report_no_vacuity() {
    let violated = format!("{ORDERS}forbidden(Order, status == paid, flagged == true)\n");
    assert_eq!(
        vacuous(&violated, Bounds::STATES, Caps::for_instances(1)),
        BTreeMap::new()
    );
    let capped = format!("{ORDERS}forbidden(Order, status == void, flagged == true)\n");
    assert_eq!(
        vacuous(&capped, Bounds::VERIFY, Caps::new(256, 3)),
        BTreeMap::new()
    );
    assert_eq!(
        vacuous(&capped, Bounds::VERIFY, Caps::VERIFY).len(),
        1,
        "the same rule is vacuous once the run completes"
    );
}

/// A rule over two entities meets a condition on one of them with a row of that entity
/// alone. No cancelled order keeps a payment here, since Cancel deletes it, but cancelled
/// orders exist and captured payments exist: the rule claims they never meet.
#[test]
fn a_condition_on_one_entity_needs_no_row_of_the_other() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Cancel "キャンセル"
creates(Place, Order)
creates(Place, Payment)
updates(Pay, Order)
updates(Pay, Payment)
updates(Cancel, Order)
deletes(Cancel, Payment)
transitions(Pay, Order.status, pending -> paid)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Pay, Payment.status, pending -> captured)
relate(Payment, Order, N:1)
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)
"#;
    assert_eq!(
        vacuous(src, Bounds::STATES, Caps::for_instances(1)),
        BTreeMap::new()
    );
}

/// A `.none(...)` says something only of rows of the related entity: with no such row ever,
/// it holds vacuously, as the `forbidden` that says the same does. A `.has(...)` asks for a
/// row, so it has no such premise.
#[test]
fn a_none_over_rows_that_never_exist_holds_vacuously() {
    let src = format!(
        "{ORDERS}entity Line \"明細\" {{ id: Int @pk  kind: Enum(plain, gift) @default(plain) }}\n\
         relate(Line, Order, N:1)\n\
         usecase AddLine \"明細追加\"\ncreates(AddLine, Line)\n\
         condition Never \"起きない\"\nholds(Never, Order.status == void)\napplies_to(Never, AddLine)\n\
         invariant(Order).none(Line.kind == gift)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::for_instances(1));
    assert_eq!(
        found.get("Quantifier_Order_0"),
        Some(&vec!["some Line row beside it".to_string()]),
        "{found:?}"
    );
}

/// The finding names the rows each premise asks for: a condition on one entity of a rule
/// over two asks for a row of that entity alone. It says when the run had one instance per
/// entity, where a state that needs a second row of some entity is not reached.
#[test]
fn the_finding_names_the_rows_each_premise_asks_for() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, void) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文確定"
usecase Pay "決済確定"
creates(Place, Order)
creates(Place, Payment)
transitions(Pay, Payment.status, pending -> captured)
relate(Payment, Order, N:1)
forbidden(Order, Payment, Order.status == void, Payment.status == captured).along(Order, Payment)
"#;
    for (bounds, note) in [(Bounds::STATES, true), (Bounds::VERIFY, false)] {
        let ts = TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts;
        let run = ts.explore_targets(&ts.targets(), Caps::VERIFY);
        let message = run
            .verdicts()
            .iter()
            .find(|d| matches!(d.diag, StateDiag::VacuousRule { .. }))
            .expect("the rule is vacuous: no order is void")
            .diag
            .to_string();
        assert!(
            message.starts_with(
                "MultiForbidden_Order_0 holds vacuously: no reachable state has a row of Order with Order.status=void"
            ),
            "{message}"
        );
        assert_eq!(message.contains("--instance-count 2"), note, "{message}");
    }
}

/// A scope that drops use cases may drop the one that reaches the premise, so its run
/// reports no vacuity (`TargetExploration::defer_progress`).
#[test]
fn a_run_whose_scope_drops_use_cases_reports_no_vacuity() {
    let src = format!("{ORDERS}forbidden(Order, status == void, flagged == true)\n");
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let mut run = ts.explore_targets(&ts.targets(), Caps::for_instances(1));
    assert_eq!(texts(run.verdicts()).len(), 1);
    run.defer_progress();
    assert_eq!(texts(run.verdicts()), BTreeMap::new());
}

const SHIPPING: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid, shipped, void) @default(open) }
entity Desk "窓口" { id: Int @pk }
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Open "開設"
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
transitions(Ship, Order.status, void -> shipped)
creates(Open, Desk)
condition Never "起きない"
holds(Never, Order.status == void)
applies_to(Never, Open)
"#;

/// An `after` assertion says something once its use case runs. Ship needs a void order and
/// none is ever void, so its assertion holds vacuously; Pay runs, so its assertion does not.
/// An assertion that reads an entity its step does not bind needs a row of it after the step:
/// no Desk ever exists.
#[test]
fn an_assert_after_a_use_case_that_never_runs_holds_vacuously() {
    let src = format!(
        "{SHIPPING}after(Ship).assert(Order.status == shipped)\n\
         after(Pay).assert(Order.status == paid)\n\
         after(Pay).assert(Desk.id == 1)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::for_instances(1));
    assert_eq!(
        found,
        BTreeMap::from([
            (
                "AfterAssert_Ship_0".to_string(),
                vec!["Ship never runs to its last step".to_string()]
            ),
            (
                "AfterAssert_Pay_2".to_string(),
                vec!["no run of Pay ends in a state with a row of Desk".to_string()]
            ),
        ])
    );
}

/// A `leads_to` whose trigger no reachable state meets holds vacuously, and so does any
/// property over rows that never exist. A property whose trigger is reached is not vacuous,
/// whatever its verdict: BFS evaluates no temporal formula.
#[test]
fn a_leads_to_whose_trigger_never_holds_holds_vacuously() {
    let src = format!(
        "{SHIPPING}property VoidShips \"無効は出荷\"\n  leads_to(Order.status == void, Order.status == shipped)\n\
         property OpenPays \"未払は支払\"\n  leads_to(Order.status == open, Order.status == paid)\n\
         property DeskStays \"窓口は残る\"\n  always(Desk.id == 1)\n"
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let found = texts(&ts.vacuous_properties(Caps::for_instances(1)));
    assert_eq!(
        found,
        BTreeMap::from([
            (
                "VoidShips".to_string(),
                vec!["its leads_to trigger holds in no reachable state".to_string()]
            ),
            ("DeskStays".to_string(), vec![String::new()]),
        ])
    );
    assert_eq!(
        texts(&ts.vacuous_properties(Caps::new(256, 1))),
        BTreeMap::new()
    );
}

/// A property over two entities is read per pair of present rows, as TLC reads it. A Desk
/// opens only once the one Order is paid, so no state has an open order beside a desk: the
/// trigger never fires, though open orders exist.
#[test]
fn a_trigger_needs_a_row_of_every_entity_the_property_ranges_over() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Desk "窓口" { id: Int @pk  busy: Bool @default(false) }
usecase Place "注文"
usecase Pay "支払"
usecase Open "開設"
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
creates(Open, Desk)
condition Paid "済"
holds(Paid, Order.status == paid)
applies_to(Paid, Open)
property TwoLead "両方"
  leads_to(Order.status == open, Desk.busy == true)
"#;
    let ts = TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts;
    let found = texts(&ts.vacuous_properties(Caps::for_instances(1)));
    assert_eq!(
        found.get("TwoLead"),
        Some(&vec![
            "its leads_to trigger holds in no reachable state".to_string()
        ]),
        "{found:?}"
    );
}

/// The forms BFS decides say something only where their condition holds: what a `stable`
/// keeps, and what a `precedes` asks a predecessor for. No order is ever void, so both hold
/// vacuously; the same forms over a reached state do not.
#[test]
fn a_stable_or_precedes_over_a_state_no_row_reaches_holds_vacuously() {
    let src = format!(
        "{SHIPPING}property VoidStays \"無効は無効\"\n  stable(Order.status == void)\n\
         property PaidFirst \"支払が先\"\n  precedes(Order.status == paid, Order.status == void)\n\
         property PaidStays \"支払は支払\"\n  stable(Order.status == paid)\n\
         property OpenFirst \"未払が先\"\n  precedes(Order.status == open, Order.status == paid)\n"
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let found = texts(&ts.vacuous_properties(Caps::for_instances(1)));
    assert_eq!(
        found,
        BTreeMap::from([
            (
                "VoidStays".to_string(),
                vec!["its stable condition holds in no reachable state".to_string()]
            ),
            (
                "PaidFirst".to_string(),
                vec!["its precedes second condition holds in no reachable state".to_string()]
            ),
        ])
    );
}

/// A cone of checks alone stops once every check on it has failed, but one rule that holds
/// keeps it going to the end: Inv_Order_0 shares Forbidden_Order_0's cone, which Pay and
/// Flag break, and is reported vacuous as on a run that never stops.
#[test]
fn a_violated_rule_does_not_hide_the_vacuity_of_a_rule_on_its_cone() {
    let src = format!(
        "{ORDERS}forbidden(Order, status == paid, flagged == true)\n\
         invariant(Order).when(status == void).then(flagged == true)\n"
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let targets = ts.targets();
    let run = ts.explore_targets(&targets, Caps::for_instances(1));
    let checks: Vec<Target> = targets
        .iter()
        .copied()
        .filter(|t| matches!(t, Target::Invariant(_)))
        .collect();
    assert_eq!(checks.len(), 2);
    let cone = run.cone_of(checks[0]);
    assert_eq!(cone.cone, run.cone_of(checks[1]).cone, "one cone");
    assert_eq!(cone.run.end(), RunEnd::Complete);
    assert_eq!(
        texts(run.verdicts()),
        BTreeMap::from([("Inv_Order_0".to_string(), vec!["status=void".to_string()])])
    );
}

const TAGS: &str = r#"
entity Tag "札" { id: Int @pk  mark: Enum(a, b) @default(a) }
usecase Make "作成"
creates(Make, Tag)
after(Make).assert(Tag.mark == b)
"#;

/// The first Make breaks the assert. On its own the cone stops there, with the
/// counterexample of the run to the end; when the cone is also the entity's, the entity's
/// view needs every state, and it runs to the end.
#[test]
fn a_cone_that_also_answers_an_entity_target_runs_to_the_end() {
    let bounds = Bounds {
        instances: NonZeroU8::new(3).expect("3 instances"),
        int_max: 3,
    };
    let ts = TransitionSystem::build(&model_from(TAGS), bounds, &Scope::ALL).ts;
    let targets = ts.targets();
    let assert = *targets
        .iter()
        .find(|t| matches!(t, Target::AfterAssert(_)))
        .expect("the assert");
    let entity = *targets
        .iter()
        .find(|t| matches!(t, Target::Entity(_)))
        .expect("the entity");
    let caps = Caps::for_instances(3);
    let alone = ts.explore_targets(&[assert], caps);
    let shared = ts.explore_targets(&[assert, entity], caps);
    assert_eq!(shared.cones.len(), 1, "the assert's cone is the entity's");
    let (stopped, whole) = (&alone.cone_of(assert).run, &shared.cone_of(assert).run);
    assert_eq!(stopped.end(), RunEnd::ChecksFailed);
    assert_eq!(whole.end(), RunEnd::Complete);
    assert!(stopped.states.len() < whole.states.len());
    let violations = |run: &TargetExploration| -> Vec<Diag> {
        run.verdicts()
            .iter()
            .filter(|d| matches!(d.diag, StateDiag::AfterAssertViolated { .. }))
            .cloned()
            .collect()
    };
    assert_eq!(violations(&alone).len(), 1);
    assert_eq!(violations(&alone), violations(&shared));
}

/// A property's cone runs to the end whatever its rules do: Forbidden_Order_0 fails on it,
/// and VoidShips is still found vacuous.
#[test]
fn a_property_cone_with_a_violated_rule_still_reports_vacuity() {
    let src = format!(
        "{SHIPPING}forbidden(Order, status == paid)\n\
         property VoidShips \"無効は出荷\"\n  leads_to(Order.status == void, Order.status == shipped)\n"
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    assert_eq!(
        texts(&ts.vacuous_properties(Caps::for_instances(1))),
        BTreeMap::from([(
            "VoidShips".to_string(),
            vec!["its leads_to trigger holds in no reachable state".to_string()]
        )])
    );
}
