use super::*;
use crate::build_model;
use rdra_ish_syntax::parse;

fn model_from(src: &str) -> SemanticModel {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    model
}

#[test]
fn a_deprecated_form_is_a_deprecated_syntax_warning() {
    let issue = diagnostic_to_lint_issue(&Diagnostic::warning(RdraError::DeprecatedSyntax {
        path: "m.rdra".into(),
        msg: "write `new(...)`".into(),
    }));
    assert_eq!(issue.rule, "deprecated-syntax");
    assert_eq!(issue.severity, LintSeverity::Warning);
    assert!(
        issue.hint.contains("rdra-ish fmt --write"),
        "{}",
        issue.hint
    );
}

#[test]
fn a_premature_buc_empty_is_info_and_not_published_to_the_editor() {
    let model = model_from(
        r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
"#,
    );
    let issue = collect_findings(&model)
        .into_iter()
        .find(|issue| issue.rule == "buc-empty")
        .unwrap();
    assert_eq!(issue.severity, LintSeverity::Info);
    assert!(
        issue
            .hint
            .starts_with("expected before stage 1 BUC skeleton; "),
        "{}",
        issue.hint
    );
    let published: Vec<String> = lint_review_diagnostics(&model)
        .iter()
        .map(|d| d.error.to_string())
        .collect();
    assert!(
        published.iter().all(|m| !m.contains("buc-empty")),
        "{published:?}"
    );
}

fn info_rules(model: &SemanticModel, rule: &str) -> Vec<String> {
    collect_findings(model)
        .into_iter()
        .filter(|issue| issue.rule == rule)
        .inspect(|issue| assert_eq!(issue.severity, LintSeverity::Info))
        .map(|issue| issue.subject_id)
        .collect()
}

#[test]
fn a_requirement_that_achieves_no_goal_is_info_unless_it_is_out_of_scope() {
    let model = model_from(
        r#"
goal ShortWait "Short wait"
  metric "median wait"
  target "30 min"
requirement ReqLinked "Linked"
requirement ReqLoose "Loose"
requirement ReqNonGoal "Online consultation"
  status "out-of-scope"
  rationale "Visits are in person."
achieves(ReqLinked, ShortWait)
"#,
    );
    assert_eq!(info_rules(&model, "requirement-no-goal"), ["ReqLoose"]);
}

#[test]
fn a_screen_no_transition_and_no_business_order_reaches_is_info() {
    let model = model_from(
        r#"
actor Clerk "Clerk"
usecase Find "Find"
usecase Pick "Pick"
usecase Report "Report"
usecase Print "Print"
screen SearchScreen "Search"
screen PickScreen "Pick"
screen ReportScreen "Report"
screen PrintScreen "Print"
screen KioskScreen "Kiosk"
displays(Find, SearchScreen)
displays(Pick, PickScreen)
displays(Report, ReportScreen)
displays(Print, PrintScreen)
navigates(ReportScreen, PrintScreen).via(Print)
flow PickFlow "Pick"
step FindStep "Find"
step PickStep "Pick"
contains(PickFlow, FindStep)
contains(PickFlow, PickStep)
covers(FindStep, Find)
covers(PickStep, Pick)
precedes(FindStep, PickStep)
performs(Clerk, FindStep)
performs(Clerk, PickStep)
"#,
    );
    assert_eq!(info_rules(&model, "screen-no-transition"), ["KioskScreen"]);
}

#[test]
fn a_goal_without_a_metric_or_a_target_is_unmeasured() {
    let model = model_from(
        r#"
goal Measured "Measured"
  metric "median wait"
  target "30 min"
goal NoTarget "No target"
  metric "median wait"
goal NoMetric "No metric"
  target "30 min"
goal Bare "Bare"
"#,
    );
    let mut unmeasured = info_rules(&model, "goal-unmeasured");
    unmeasured.sort();
    assert_eq!(unmeasured, ["Bare", "NoMetric", "NoTarget"]);
}

#[test]
fn buc_empty_on_an_event_started_buc_stays_a_warning() {
    let model = model_from(
        r#"
business Commerce "Commerce"
buc BucShip "Ship"
event OrderPaid "Order paid"
belongs(BucShip, Commerce)
triggers(OrderPaid, BucShip)
"#,
    );
    let issue = collect_findings(&model)
        .into_iter()
        .find(|issue| issue.rule == "buc-empty")
        .unwrap();
    assert_eq!(issue.severity, LintSeverity::Warning);
}

#[test]
fn lint_findings_attach_declaration_locations() {
    let model = model_from(
        r#"
actor customer "Customer"
buc Checkout "Checkout"
api CreateOrder "Create order"
"#,
    );

    let findings: Vec<_> = collect_findings(&model)
        .into_iter()
        .filter(|issue| issue.rule == "orphan-node" || issue.rule == "naming-id")
        .collect();
    assert!(!findings.is_empty());
    for issue in &findings {
        assert!(
            issue.location.is_some(),
            "expected location for {:?}",
            issue.rule
        );
    }
}

#[test]
fn lint_leaves_states_derived_from_transitions_alone() {
    let model = model_from(
        r#"
usecase Place "Place"
entity Order "Order" {
  Id: Int @pk
  status: Enum(pending, paid)
}
transitions(Place, Order.status, pending -> paid)
"#,
    );

    assert_eq!(model.states.len(), 2);
    let on_states: Vec<_> = collect_findings(&model)
        .into_iter()
        .filter(|issue| issue.subject_kind == "state")
        .collect();
    assert!(on_states.is_empty(), "{on_states:?}");
}

#[test]
fn lint_flags_a_screen_field_column_no_use_case_sets() {
    let src = |sets: &str| {
        format!(
            r#"
usecase Close "Close"
screen S "S"
field DoneOn "Done on"
contains(S, DoneOn)
entity Claim "Claim" {{
  id: Int @pk
  done_on: Date @null
}}
updates(Close, Claim)
maps_field(DoneOn, Claim.done_on)
{sets}
"#
        )
    };
    let unwritten = |model: &SemanticModel| {
        collect_findings(model)
            .into_iter()
            .filter(|issue| issue.rule == "field-column-unwritten")
            .count()
    };
    assert_eq!(unwritten(&model_from(&src(""))), 1);
    assert_eq!(
        unwritten(&model_from(&src("sets(Close, Claim.done_on == present)"))),
        0
    );
}

#[test]
fn lint_reports_coverage_readiness_and_review_findings() {
    let model = model_from(
        r#"
actor customer "Customer"
buc Checkout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
api CreateOrder "Create order" method POST
dto CreateOrderRequest "Create order request"
field ShippingAddress "Shipping address" access editable source actor
entity Order "Order" {
  Id: Int @pk
  total: Money
}
"#,
    );

    let issues = lint_issues(&model, &[]);
    let rules: Vec<_> = issues.iter().map(|issue| issue.rule).collect();
    assert!(rules.contains(&"coverage-score"));
    assert!(rules.contains(&"stage-readiness"));
    assert!(rules.contains(&"naming-id"));
    assert!(rules.contains(&"api-contract-incomplete"));
    assert!(rules.contains(&"field-unmapped"));
    assert!(rules.contains(&"naming-column"));
}

#[test]
fn coverage_score_is_zero_for_empty_model() {
    let model = model_from("");
    let issues = lint_issues(&model, &[]);
    let score = issues
        .iter()
        .find(|i| i.rule == "coverage-score")
        .expect("coverage-score");
    assert!(
        score.message.starts_with("coverage score: 0/100"),
        "empty model must not score high: {}",
        score.message
    );
    assert!(score.message.contains("empty model"));
}

#[test]
fn coverage_score_penalizes_missing_layers_for_tiny_models() {
    let model = model_from(r#"actor Solo "Solo""#);
    let issues = lint_issues(&model, &[]);
    let score = issues
        .iter()
        .find(|i| i.rule == "coverage-score")
        .expect("coverage-score");
    // orphan warning scaled by elements + 6 missing layers × -8 = large penalty
    let numeric: i32 = score
        .message
        .strip_prefix("coverage score: ")
        .and_then(|s| s.split('/').next())
        .and_then(|n| n.parse().ok())
        .expect("parse score");
    assert!(
        numeric <= 50,
        "tiny model score should stay low, got {} from {}",
        numeric,
        score.message
    );
    assert!(score.message.contains("missing layers"));
    assert!(score.message.contains("scaled"));
}

fn enum_attribute_columns(model: &SemanticModel) -> Vec<String> {
    collect_findings(model)
        .into_iter()
        .filter(|issue| issue.rule == "enum-attribute")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn enum_attribute_flags_inline_enum_nothing_moves() {
    let model = model_from(
        r#"
usecase Place "Place"
usecase Pay "Pay"
entity Order "Order" {
  Id: Int @pk
  status: Enum(pending, paid) @default(pending)
  channel: Enum(web, phone)
  priority: Enum(low, high)
  method: PaymentMethod
}
variation PaymentMethod "Payment Method" {
  cash
  card
}
transitions(Pay, Order.status, pending -> paid)
sets(Place, Order.priority == high)
"#,
    );
    let flagged = enum_attribute_columns(&model);
    assert_eq!(flagged.len(), 1, "{flagged:?}");
    assert!(flagged[0].contains("Order.channel"), "{flagged:?}");
    let issue = collect_findings(&model)
        .into_iter()
        .find(|issue| issue.rule == "enum-attribute")
        .unwrap();
    assert_eq!(issue.severity, LintSeverity::Warning);
    assert!(issue.hint.contains("variation"), "{}", issue.hint);
}

#[test]
fn exposes_and_shares_connect_the_system_and_the_element() {
    let model = model_from(
        r#"
system S "S"
usecase U "U"
entity Beat "Beat" { id: Int @pk }
entity Log "Log" { id: Int @pk }
entity Note "Note" { id: Int @pk }
creates(U, Beat)
creates(U, Log)
exposes(S, Log).append()
shares(S, Note)
"#,
    );
    let orphans: Vec<String> = collect_findings(&model)
        .into_iter()
        .filter(|i| i.rule == "orphan-node")
        .map(|i| i.subject_id)
        .collect();
    assert!(orphans.is_empty(), "{orphans:?}");
}

#[test]
fn variation_typing_a_column_of_a_reached_entity_is_anchored() {
    let model = model_from(
        r#"
buc B "B"
usecase U "U"
contains(B, U)
api Ap "Ap"
invokes(U, Ap)
variation Used "Used" {
  a
}
variation Unused "Unused" {
  a
}
entity E "E" {
  Id: Int @pk
  kind: Used
}
reads(Ap, E)
"#,
    );
    let findings = collect_findings(&model);
    assert!(!findings
        .iter()
        .any(|i| i.rule == "orphan-node" && i.subject_kind == "variation"));
    let unconnected: Vec<_> = findings
        .into_iter()
        .filter(|i| i.rule == "rdra-variation-no-buc")
        .map(|i| i.subject_id)
        .collect();
    assert_eq!(unconnected, vec!["Unused".to_string()]);
}

#[test]
fn condition_body_without_a_variation_or_lifecycle_operand_has_no_rdra_anchor() {
    let model = model_from(
        r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  Id: Int @pk
  method: Method
  amount: Int
  status: Enum(open, paid) @default(open)
}
usecase Settle "Settle"
transitions(Settle, Pay.status, open -> paid)
condition Big "Big"
holds(Big, Pay.amount > 100)
condition ByCard "By card"
holds(ByCard, Pay.method == card)
condition Paid "Paid"
holds(Paid, Pay.status == paid)
condition Traceable "Traceable"
combines(Traceable, Method)
"#,
    );
    let flagged: Vec<_> = collect_findings(&model)
        .into_iter()
        .filter(|i| i.rule == "condition-no-rdra-anchor")
        .map(|i| i.subject_id)
        .collect();
    assert_eq!(flagged, vec!["Big".to_string()]);
}

/// A body that guards a use case is a rule the use case follows, whatever columns it
/// reads: `applies_to` a use case or an API, or `.when` on a `transitions` arm or a
/// `chooses` value. `applies_to` a system adds no guard, so it does not count.
#[test]
fn a_condition_body_that_guards_a_use_case_needs_no_rdra_anchor() {
    let model = model_from(
        r#"
system Shop "Shop"
entity Pay "Pay" {
  Id: Int @pk
  amount: Int
  flagged: Bool @default(false)
  status: Enum(open, paid, held) @default(open)
}
usecase Settle "Settle"
usecase Hold "Hold"
usecase Review "Review"
api PayApi "Pay API"
invokes(Review, PayApi)
updates(PayApi, Pay)
transitions(Settle, Pay.status, open -> paid)
transitions(Hold, Pay.status, open -> held).when(Large)
sets(Review, Pay.flagged == true)
condition Big "Big"
holds(Big, Pay.amount > 100)
applies_to(Big, Settle)
condition Large "Large"
holds(Large, Pay.amount > 500)
condition Suspicious "Suspicious"
holds(Suspicious, Pay.flagged == false)
applies_to(Suspicious, PayApi)
condition SystemWide "System wide"
holds(SystemWide, Pay.amount > 1)
applies_to(SystemWide, Shop)
condition Unused "Unused"
holds(Unused, Pay.amount > 2)
"#,
    );
    let mut flagged: Vec<_> = collect_findings(&model)
        .into_iter()
        .filter(|i| i.rule == "condition-no-rdra-anchor")
        .map(|i| i.subject_id)
        .collect();
    flagged.sort();
    assert_eq!(flagged, ["SystemWide", "Unused"]);
}

#[test]
fn flow_cycle_detects_precedes_loop() {
    let model = model_from(
        r#"
step A "A"
step B "B"
precedes(A, B)
precedes(B, A)
"#,
    );
    let issues = lint_issues(&model, &[]);
    let cycles: Vec<_> = issues.iter().filter(|i| i.rule == "flow-cycle").collect();
    assert!(
        cycles.len() >= 2,
        "expected flow-cycle on both steps, got: {:?}",
        issues
            .iter()
            .map(|i| (i.rule, i.subject_id.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(cycles.iter().any(|i| i.subject_id == "A"));
    assert!(cycles.iter().any(|i| i.subject_id == "B"));
}
