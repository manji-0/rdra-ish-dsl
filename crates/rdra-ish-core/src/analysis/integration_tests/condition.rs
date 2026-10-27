use crate::analysis::{condition_body_display, condition_edges, ConditionEdge};
use crate::diagnostics::RdraError;
use crate::model::{NodeRef, RelKind, SemanticModel, TypedPredicate};
use rdra_ish_syntax::parse;

fn build(src: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    crate::analysis::build_model(&ast)
}

fn errors(diags: &[crate::Diagnostic]) -> Vec<&RdraError> {
    diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| &d.error)
        .collect()
}

const BASE: &str = r#"
variation PaymentMethod "Payment Method" {
  cash "Cash"
  insurance "Insurance"
}
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(booked, checkedin) @default(booked)
  note: Enum(a, b) @default(a)
}
entity Payment "Payment" {
  id: Int @pk
  method: PaymentMethod @default(cash)
  amount: Int
}
usecase CheckIn "Check in"
usecase Collect "Collect"
transitions(Appointment.status, CheckIn, booked -> checkedin)
"#;

fn with(extra: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    build(&format!("{BASE}{extra}"))
}

#[test]
fn combines_derives_variation_and_state_model_edges() {
    let (model, diags) = with(
        "condition CopayDue \"Copay due\"\ncombines(CopayDue, PaymentMethod)\ncombines(CopayDue, Appointment.status)\napplies_to(CopayDue, Collect)\n",
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let (ck, cond) = model.conditions.iter().next().unwrap();
    assert!(cond.body.is_none());
    let edges = condition_edges(&model, ck);
    assert_eq!(edges.len(), 2, "{edges:?}");
    assert!(matches!(edges[0], ConditionEdge::Variation(_)));
    assert!(matches!(&edges[1], ConditionEdge::StateModel { column, .. } if column == "status"));
    let combines = model
        .relations
        .iter()
        .filter(|r| r.kind == RelKind::Combines && r.from == NodeRef::Condition(ck))
        .count();
    assert_eq!(combines, 2);
    assert!(model
        .relations
        .iter()
        .any(|r| r.kind == RelKind::AppliesTo && r.from == NodeRef::Condition(ck)));
    assert!(model
        .typed_predicates
        .iter()
        .any(|p| matches!(p, TypedPredicate::AppliesTo { .. })));
}

#[test]
fn holds_stores_the_body_and_derives_the_same_edges() {
    let (model, diags) = with(
        "condition Insured \"Insured\"\nholds(Insured, Payment.method == insurance, Appointment.status == checkedin)\napplies_to(Insured, Collect)\n",
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let (ck, cond) = model.conditions.iter().next().unwrap();
    assert_eq!(cond.body.as_ref().map(Vec::len), Some(2));
    assert_eq!(
        condition_body_display(&model, ck).as_deref(),
        Some("Payment.method == insurance, Appointment.status == checkedin")
    );
    let edges = condition_edges(&model, ck);
    assert!(matches!(edges[0], ConditionEdge::Variation(_)));
    assert!(matches!(&edges[1], ConditionEdge::StateModel { column, .. } if column == "status"));
}

#[test]
fn a_body_keeps_non_rdra_operands_without_edges() {
    let (model, diags) = with("condition Big \"Big\"\nholds(Big, Payment.amount > 100)\n");
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let (ck, _) = model.conditions.iter().next().unwrap();
    assert!(condition_edges(&model, ck).is_empty());
}

#[test]
fn combines_after_holds_is_rejected() {
    let (_, diags) =
        with("condition C \"C\"\nholds(C, Payment.method == cash)\ncombines(C, PaymentMethod)\n");
    let errs = errors(&diags);
    assert_eq!(
        errs.iter()
            .filter(|e| matches!(e, RdraError::ConditionCombinesWithBody { condition } if condition == "C"))
            .count(),
        1,
        "{errs:?}"
    );
}

#[test]
fn holds_after_combines_is_rejected() {
    let (model, diags) =
        with("condition C \"C\"\ncombines(C, PaymentMethod)\nholds(C, Payment.method == cash)\n");
    let errs = errors(&diags);
    assert_eq!(
        errs.iter()
            .filter(|e| matches!(e, RdraError::ConditionCombinesWithBody { condition } if condition == "C"))
            .count(),
        1,
        "{errs:?}"
    );
    assert!(model.conditions.values().next().unwrap().body.is_none());
}

#[test]
fn a_second_holds_is_rejected() {
    let (model, diags) = with(
        "condition C \"C\"\nholds(C, Payment.method == cash)\nholds(C, Payment.method == insurance)\n",
    );
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::ConditionSecondBody { condition } if condition == "C")),
        "{diags:?}"
    );
    assert_eq!(
        condition_body_display(&model, model.conditions.keys().next().unwrap()).as_deref(),
        Some("Payment.method == cash")
    );
}

#[test]
fn combines_on_a_non_lifecycle_column_is_rejected_wherever_transitions_sit() {
    let (_, diags) = with("condition C \"C\"\ncombines(C, Appointment.note)\n");
    assert!(
        errors(&diags).iter().any(|e| matches!(e,
            RdraError::ConditionCombinesAttribute { condition, entity, col }
                if condition == "C" && entity == "Appointment" && col == "note")),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\ncombines(C, Payment.method)\n");
    assert!(
        errors(&diags).iter().any(
            |e| matches!(e, RdraError::ConditionCombinesAttribute { col, .. } if col == "method")
        ),
        "{diags:?}"
    );

    let (model, diags) = build(
        r#"
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(booked, checkedin) @default(booked)
}
usecase CheckIn "Check in"
condition C "C"
combines(C, Appointment.status)
transitions(Appointment.status, CheckIn, booked -> checkedin)
"#,
    );
    assert!(
        !errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::ConditionCombinesAttribute { .. })),
        "a transitions declared after combines still makes the column a lifecycle: {diags:?}"
    );
    assert_eq!(
        condition_edges(&model, model.conditions.keys().next().unwrap()).len(),
        1
    );
}

#[test]
fn combines_with_an_unknown_column_or_symbol_is_reported() {
    let (_, diags) = with("condition C \"C\"\ncombines(C, Appointment.nope)\n");
    assert!(
        errors(&diags).iter().any(
            |e| matches!(e, RdraError::UnknownColumn { entity, col } if entity == "Appointment" && col == "nope")
        ),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\ncombines(C, Ghost)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::UndefinedSymbol { id } if id == "Ghost")),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\ncombines(C, Collect)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::TypeMismatch { pred, .. } if pred == "combines")),
        "{diags:?}"
    );
}

#[test]
fn a_two_part_target_naming_a_variation_and_a_column_is_ambiguous() {
    // Without import scopes `a.b` resolves `b` in any module, so a variation named like a
    // column of an entity that shares the first part makes the target ambiguous.
    let (_, diags) = build(
        r#"
variation status "Status" { open closed }
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(booked, done) @default(booked)
}
usecase Finish "Finish"
transitions(Appointment.status, Finish, booked -> done)
condition C "C"
combines(C, Appointment.status)
"#,
    );
    assert!(
        errors(&diags).iter().any(|e| matches!(e,
            RdraError::AmbiguousCombinesTarget { variation, entity, .. }
                if variation == "status" && entity == "Appointment")),
        "{diags:?}"
    );
}

#[test]
fn body_values_are_checked_against_the_column() {
    let (_, diags) = with("condition C \"C\"\nholds(C, Payment.method == barter)\n");
    assert_eq!(
        errors(&diags).len(),
        1,
        "one root cause, one error: {diags:?}"
    );
    assert!(
        errors(&diags).iter().any(
            |e| matches!(e, RdraError::InvalidVariationValue { variation, value, .. } if variation == "PaymentMethod" && value == "barter")
        ),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\nholds(C, Appointment.status == gone)\n");
    assert_eq!(
        errors(&diags).len(),
        1,
        "one root cause, one error: {diags:?}"
    );
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::InvalidEnumVariant { value, .. } if value == "gone")),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\nholds(C, Ghost.x == 1)\n");
    assert_eq!(errors(&diags).len(), 1, "{diags:?}");
    assert!(
        matches!(errors(&diags)[0], RdraError::UndefinedSymbol { id } if id == "Ghost"),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\nholds(C, Payment.nope == 1)\n");
    assert_eq!(errors(&diags).len(), 1, "{diags:?}");
    let (_, diags) = with("condition C \"C\"\nholds(C, method == cash)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::CrossConstraintColumnNeedsEntity { .. })),
        "a bare column cannot say which entity it belongs to: {diags:?}"
    );
}

#[test]
fn holds_needs_a_comparison_after_the_condition() {
    let (_, diags) = with("condition C \"C\"\nholds(C)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::WrongArity { name, .. } if name == "holds")),
        "{diags:?}"
    );
    let (_, diags) = with("condition C \"C\"\nholds(C, Payment)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::TypeMismatch { pred, .. } if pred == "holds")),
        "{diags:?}"
    );
}

#[test]
fn applies_to_takes_a_condition_or_an_nfr_and_rejects_other_sources() {
    let (_, diags) = with(
        "condition C \"C\"\nnfr Fast \"Fast\"\napplies_to(C, Collect)\napplies_to(Fast, Collect)\n",
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let (_, diags) = with("applies_to(Collect, CheckIn)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::TypeMismatch { pred, .. } if pred == "applies_to")),
        "{diags:?}"
    );
}

#[test]
fn condition_takes_no_chain_methods() {
    let (_, diags) = with("condition C \"C\"\ncombines(C, PaymentMethod).when(x)\n");
    assert!(
        errors(&diags)
            .iter()
            .any(|e| matches!(e, RdraError::UnknownChainMethod { pred, .. } if pred == "combines")),
        "{diags:?}"
    );
}
