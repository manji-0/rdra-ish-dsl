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
            Target::AfterAssert(_) | Target::StepRule(_) => unreachable!(),
        })
        .collect();
    assert_eq!(names, ["Order", "Forbidden_Order_0"]);

    let report = super::super::derive_states(&model, &scope(&model, &["Order"]), Caps::STATES);
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
transitions(Pay, Order.status, pending -> paid)
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
    let report = super::super::derive_states(&model, &scope(&model, &["Order"]), Caps::STATES);
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

/// Issue #115: an operator's `Delete` leaves every state, which hides that a staff member
/// whose key no longer matches cannot leave `pending` by their own use cases.
const SELF_SERVICE: &str = r#"
actor Staff "Staff"
actor Ops "Ops"
entity Account "Account" {
  id: Int @pk
  status: Enum(pending, active, deleted) @default(pending)
}
entity Browser "Browser" {
  id: Int @pk
  key_ok: Bool @default(true)
}
relate(Account, Browser, N:1)
buc BucSelf "Self-service"
buc BucOps "Ops"
usecase Create "Create"
usecase Activate "Activate"
usecase LoseKey "Lose key"
usecase Delete "Delete"
contains(BucSelf, Create)
contains(BucSelf, Activate)
contains(BucSelf, LoseKey)
contains(BucOps, Delete)
performs(Staff, BucSelf)
performs(Ops, BucOps)
creates(Create, Account)
updates(Activate, Account)
condition KeyOk "key matches"
holds(KeyOk, Browser.key_ok == true).along(Account, Browser)
applies_to(KeyOk, Activate)
transitions(Activate, Account.status, pending -> active)
updates(LoseKey, Browser)
sets(LoseKey, Browser.key_ok == false)
updates(Delete, Account)
transitions(Delete, Account.status, (pending, active) -> deleted)
"#;

fn without(model: &SemanticModel, w: crate::lifecycle::Without) -> Scope {
    Scope::parse(model, &[], &[])
        .unwrap()
        .without(model, &w)
        .unwrap()
}

fn deadlock_names(model: &SemanticModel, scope: &Scope) -> Vec<String> {
    let report = super::super::derive_states(model, scope, Caps::STATES);
    report
        .diags
        .iter()
        .filter(|d| matches!(d.diag, StateDiag::Deadlock { .. }))
        .map(|d| d.diag.name(&[]))
        .collect()
}

#[test]
fn leaving_out_use_cases_decides_the_deadlocks_they_hid() {
    let model = model_from(SELF_SERVICE);
    assert!(deadlock_names(&model, &Scope::ALL).is_empty());
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (ucs, bucs, actors) = (ids(&["Delete"]), ids(&["BucOps"]), ids(&["Staff"]));
    for w in [
        crate::lifecycle::Without {
            usecases: &ucs,
            ..Default::default()
        },
        crate::lifecycle::Without {
            bucs: &bucs,
            ..Default::default()
        },
        crate::lifecycle::Without {
            actors: &actors,
            ..Default::default()
        },
    ] {
        let scope = without(&model, w);
        let left_out: Vec<&str> = scope
            .without
            .iter()
            .map(|k| model.use_cases[*k].id.as_str())
            .collect();
        assert_eq!(left_out, ["Delete"]);
        assert_eq!(
            deadlock_names(&model, &scope),
            ["Deadlock_Account_status_pending"]
        );
        let built = TransitionSystem::build(&model, Bounds::STATES, &scope);
        assert!(built.selection.inactive.is_empty());
        let line = scope.line(&model, &built.selection).unwrap();
        assert!(
            line.ends_with("(1 use case assumed not to run: Delete)"),
            "{line}"
        );
    }
}

/// A use case two BUCs reach stays when only one of them is left out, and a use case no
/// actor performs stays under `--actor`.
#[test]
fn a_shared_or_unperformed_use_case_stays() {
    let src = format!("{SELF_SERVICE}contains(BucOps, Activate)\nusecase Sweep \"Sweep\"\nupdates(Sweep, Browser)\nsets(Sweep, Browser.key_ok == true)\n");
    let model = model_from(&src);
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (bucs, actors) = (ids(&["BucOps"]), ids(&["Ops"]));
    let by_buc = without(
        &model,
        crate::lifecycle::Without {
            bucs: &bucs,
            ..Default::default()
        },
    );
    let names = |s: &Scope| -> Vec<String> {
        s.without
            .iter()
            .map(|k| model.use_cases[*k].id.clone())
            .collect()
    };
    assert_eq!(names(&by_buc), ["Delete"]);
    let by_actor = without(
        &model,
        crate::lifecycle::Without {
            actors: &actors,
            ..Default::default()
        },
    );
    assert_eq!(names(&by_actor), ["Create", "LoseKey"]);
}

#[test]
fn an_unknown_left_out_id_is_an_error() {
    let model = model_from(SELF_SERVICE);
    let ids = vec!["Nope".to_string()];
    let err = Scope::ALL
        .clone()
        .without(
            &model,
            &crate::lifecycle::Without {
                actors: &ids,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "unknown actor `Nope`");
}
