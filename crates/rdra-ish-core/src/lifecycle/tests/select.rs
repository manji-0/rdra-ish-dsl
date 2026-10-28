//! `--entity` answers the selected entities' targets on their exact cones and defers a build
//! error no selected cone reads; `--buc` drops use cases and leaves progress undecided.

use super::*;
use crate::lifecycle::{Target, UndecidedWhy};

const PARTIAL: &str = "samples/formal-verification/partial_scope.rdra";

fn scope(model: &SemanticModel, entities: &[&str]) -> Scope {
    let ids: Vec<String> = entities.iter().map(|e| e.to_string()).collect();
    Scope::parse(model, &[], &ids).unwrap()
}

fn derived_column_reads(diags: &[Diag]) -> usize {
    diags
        .iter()
        .filter(|d| matches!(d.diag, StateDiag::DerivedColumnRead { .. }))
        .count()
}

/// The Audit error blocks the whole model; `--entity Order` defers it and answers Order's
/// view and invariant only.
#[test]
fn an_error_no_selected_cone_reads_is_deferred() {
    let model = sample_model(PARTIAL);
    let whole = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    assert_eq!(derived_column_reads(&whole.diags), 1);
    assert!(whole.selection.deferred.is_empty());
    assert!(whole.selection.unchecked.is_empty());

    let built = TransitionSystem::build(&model, Bounds::STATES, &scope(&model, &["Order"]));
    assert_eq!(derived_column_reads(&built.diags), 0, "{:?}", built.diags);
    assert_eq!(derived_column_reads(&built.selection.deferred), 1);
    let ts = &built.ts;
    let names: Vec<String> = built
        .selection
        .targets
        .iter()
        .map(|t| match *t {
            Target::Entity(e) => ts.entity(e).id.clone(),
            Target::Invariant(i) => ts.invariants()[i].name.clone(),
            Target::AfterAssert(_) => unreachable!(),
        })
        .collect();
    assert_eq!(names, ["Order", "Forbidden_Order_0"]);

    let report = super::super::derive_states(&model, &scope(&model, &["Order"]), 256);
    assert!(!report.has_errors(), "{:?}", report.diags);
    let views: Vec<&str> = report.views.iter().map(|v| v.entity_id.as_str()).collect();
    assert_eq!(views, ["Order"]);
}

/// A guard that reads Audit's derived column alters Pay, an Order action, so the error is
/// not deferred under `--entity Order` even though it names only Audit.
#[test]
fn an_error_that_alters_a_selected_action_blocks() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Audit "監査" { id: Int @pk  count: Int @default(0)  score: Int @derived(count * 2) }
usecase Place "注文確定"
usecase Pay "支払い"
creates(Place, Order)
updates(Pay, Order)
transitions(Order.status, Pay, pending -> paid)
condition Audited "監査済み"
holds(Audited, Audit.score > 1)
applies_to(Audited, Pay)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &scope(&model, &["Order"]));
    assert_eq!(derived_column_reads(&built.diags), 1, "{:?}", built.diags);
    assert!(built.selection.deferred.is_empty());
}

/// `--entity` keeps the progress findings it can decide: Order's states are witnessed on its
/// exact cone. Record, which no Order cone keeps, is not decided rather than dead.
#[test]
fn an_entity_scope_leaves_only_unkept_use_cases_undecided() {
    let model = sample_model(PARTIAL);
    let report = super::super::derive_states(&model, &scope(&model, &["Order"]), 256);
    let undecided: Vec<String> = report.undecided.iter().map(|u| u.to_string()).collect();
    assert_eq!(
        undecided,
        ["whether Record can run is not decided: no cone of a selected entity keeps it"]
    );
    assert!(report
        .undecided
        .iter()
        .all(|u| u.why == UndecidedWhy::Unselected));
    assert!(!report
        .diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::NeverRuns { .. })));
}

#[test]
fn an_unknown_id_is_an_error() {
    let model = sample_model(PARTIAL);
    let err = Scope::parse(&model, &[], &["Nope".to_string()]).unwrap_err();
    assert_eq!(err.to_string(), "unknown entity `Nope`");
    let err = Scope::parse(&model, &["Nope".to_string()], &[]).unwrap_err();
    assert_eq!(err.to_string(), "unknown buc `Nope`");
}
