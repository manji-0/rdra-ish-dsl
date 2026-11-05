use super::*;
use crate::build_model;
use rdra_ish_syntax::parse;

fn findings(src: &str, rule: &str) -> Vec<LintIssue> {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    collect_findings(&model)
        .into_iter()
        .filter(|issue| issue.rule == rule)
        .collect()
}

const ORDER: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending)  qty: Int  left: Int @null }
usecase Place "注文"
usecase Pay "支払"
usecase Remind "督促"
creates(Place, Order)
updates(Pay, Order)
transitions(Pay, Order.status, pending -> paid)
"#;

/// `check` and `verify` read an Int column in 0..5 (#89): a comparison no value there
/// satisfies, and one every value satisfies on a column that is not `@null`, are lint
/// warnings; one some value fails is not flagged, and neither is `left >= 0`, which null
/// fails.
#[test]
fn a_comparison_the_int_range_decides_is_a_warning() {
    let src = format!(
        "{ORDER}forbidden(Order, qty > 5)\nrequired(Order, qty <= 5)\nforbidden(Order, qty > 2, status == paid)\nforbidden(Order, left >= 0, status == paid)\ncondition Empty \"空\"\nholds(Empty, Order.qty < 0)\napplies_to(Empty, Pay)\n"
    );
    let unsat: Vec<String> = findings(&src, "comparison-unsatisfiable-in-range")
        .into_iter()
        .inspect(|i| assert_eq!(i.severity, LintSeverity::Warning))
        .map(|i| i.message)
        .collect();
    assert_eq!(unsat.len(), 2, "{unsat:?}");
    assert!(
        unsat[0].starts_with("Order.qty > 5 holds for no value in 0..5"),
        "{unsat:?}"
    );
    assert!(
        unsat[1].starts_with("Order.qty < 0 holds for no value"),
        "{unsat:?}"
    );
    let trivial: Vec<String> = findings(&src, "comparison-trivial-in-range")
        .into_iter()
        .inspect(|i| assert_eq!(i.severity, LintSeverity::Warning))
        .map(|i| i.message)
        .collect();
    assert_eq!(trivial.len(), 1, "{trivial:?}");
    assert!(
        trivial[0].starts_with("Order.qty <= 5 holds for every value"),
        "{trivial:?}"
    );
}

/// A use case only an event starts runs, once the event was raised, in every state that
/// follows (#89, #112): with no guard and no from-state it is the info
/// `triggered-usecase-unguarded`. A guard, a from-state, or an actor that performs it clears
/// the info.
#[test]
fn a_triggered_use_case_with_no_guard_is_info() {
    let triggered = format!(
        "{ORDER}event Paid \"支払済み\"\nraises(Pay, Paid)\ntriggers(Paid, Remind)\nupdates(Remind, Order)\nsets(Remind, Order.qty == 1)\n"
    );
    let ids = |src: &str| -> Vec<String> {
        findings(src, "triggered-usecase-unguarded")
            .into_iter()
            .inspect(|i| assert_eq!(i.severity, LintSeverity::Info))
            .map(|i| i.subject_id)
            .collect()
    };
    assert_eq!(ids(&triggered), vec!["Remind".to_string()]);
    assert!(ids(&format!(
        "{triggered}condition IsPaid \"支払済み\"\nholds(IsPaid, Order.status == paid)\napplies_to(IsPaid, Remind)\n"
    ))
    .is_empty());
    assert!(ids(&format!(
        "{triggered}actor Clerk \"係\"\nperforms(Clerk, Remind)\n"
    ))
    .is_empty());
}

/// A condition with no `holds` body guards nothing in `check` (#89), so applying it to a
/// use case is the info `condition-applied-without-body`.
#[test]
fn a_bodiless_condition_applied_to_a_use_case_is_info() {
    let src = format!("{ORDER}condition Bare \"本文なし\"\napplies_to(Bare, Pay)\ncondition Kept \"本文あり\"\nholds(Kept, Order.status == pending)\napplies_to(Kept, Pay)\ncondition Traced \"追跡\"\ncombines(Traced, Order.status)\napplies_to(Traced, Pay)\n");
    let found = findings(&src, "condition-applied-without-body");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].subject_id, "Bare");
    assert_eq!(found[0].severity, LintSeverity::Info);
}
