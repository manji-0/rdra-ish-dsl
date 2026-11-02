//! Vision rule 6: a use case whose writes span k transaction groups is k actions, and the
//! checker explores the states in between.

use super::*;
use crate::lifecycle::{ActionOrigin, GuardWhy};

/// Pay updates Order and Payment through one API each, so it spans two groups. `{extra}`
/// completes the model.
fn split_pay(extra: &str) -> String {
    format!(
        r#"
entity Order "注文" {{ id: Int @pk  status: Enum(pending, paid) @default(pending)  rush: Bool }}
entity Payment "決済" {{ id: Int @pk  status: Enum(pending, captured) @default(pending) }}
entity Desk "窓口" {{ id: Int @pk  open: Bool }}
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Staff "開局"
api CapturePaymentApi "Capture"
api PayOrderApi "Pay order"
creates(Place, Order)
creates(Place, Payment)
creates(Staff, Desk)
invokes(Pay, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Pay, PayOrderApi)
updates(PayOrderApi, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Pay, Payment.status, pending -> captured)
{extra}
"#
    )
}

fn built(src: &str) -> TransitionSystem {
    TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts
}

fn action_names(ts: &TransitionSystem) -> Vec<String> {
    ts.actions().iter().map(|a| a.name.clone()).collect()
}

/// The entity ids an action binds, by name.
fn bound(ts: &TransitionSystem, name: &str) -> Vec<String> {
    let act = ts.actions().iter().find(|a| a.name == name).unwrap();
    act.params
        .iter()
        .map(|p| ts.entity(p.entity).id.clone())
        .collect()
}

#[test]
fn a_use_case_spanning_two_groups_is_one_action_per_group_in_tx_order() {
    let model = model_from(&split_pay(""));
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    assert_eq!(action_names(&ts), vec!["Pay_1", "Pay_2", "Place", "Staff"]);
    assert_eq!(bound(&ts, "Pay_1"), vec!["Payment"]);
    assert_eq!(bound(&ts, "Pay_2"), vec!["Order"]);
    // Place creates both through one FK-connected group: one atomic action.
    assert_eq!(bound(&ts, "Place"), vec!["Order", "Payment"]);
    let (pay, _) = model.use_cases.iter().find(|(_, u)| u.id == "Pay").unwrap();
    for name in ["Pay_1", "Pay_2"] {
        let act = ts.actions().iter().find(|a| a.name == name).unwrap();
        assert!(matches!(act.origin, ActionOrigin::UseCase { uc, .. } if uc == pay));
    }
}

const PUBLISHED_WRITE: &str = r#"
system Auth "Auth"
entity Session "Session" { id: Int @pk  status: Enum(active, ended) @default(active) }
entity SessionView "Session (published)" { id: Int @pk  status: Enum(active, ended) }
usecase Open "Open"
usecase Logout "Logout"
creates(Open, Session)
creates(Open, SessionView)
transitions(Logout, Session.status, active -> ended)
updates(Logout, SessionView)
sets(Logout, SessionView, status == ended)
exposes(Auth, SessionView).upsert()
"#;

#[test]
fn a_published_write_joins_the_use_cases_one_internal_group_and_leaves_the_lifecycle() {
    let model = model_from(PUBLISHED_WRITE);
    let txs = crate::tx::infer_usecase_transactions(&model);
    let logout = txs
        .iter()
        .find(|t| model.use_cases[t.usecase].id == "Logout")
        .unwrap();
    let groups: Vec<Vec<String>> = logout
        .groups()
        .map(|g| {
            g.ordered_writes
                .iter()
                .map(|w| model.entities[w.entity].id.clone())
                .collect()
        })
        .collect();
    assert_eq!(groups, vec![vec!["Session", "SessionView"]]);
    assert!(crate::tx::tx_diagnostics(&model, &txs).is_empty());

    let ts = built(PUBLISHED_WRITE);
    assert_eq!(action_names(&ts), vec!["Logout", "Open"]);
    assert_eq!(bound(&ts, "Logout"), vec!["Session"]);
    let entities: Vec<&str> = ts.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(entities, vec!["Session"]);
}

#[test]
fn a_published_write_through_its_own_api_is_that_apis_group() {
    let (ast, _) = rdra_ish_syntax::parse(
        r#"
system Auth "Auth"
entity Session "Session" { id: Int @pk }
entity SessionView "Session (published)" { id: Int @pk }
usecase Login "Login"
api SessionApi "Session API" method POST path "/sessions"
api ViewApi "View API" method PUT path "/session-views"
contains(Auth, SessionApi)
contains(Auth, ViewApi)
invokes(Login, SessionApi)
invokes(Login, ViewApi)
creates(SessionApi, Session)
creates(ViewApi, SessionView)
exposes(Auth, SessionView).upsert()
"#,
    );
    let (model, diags) = crate::analysis::build_model(&ast);
    let errors: Vec<String> = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("PublishedNotInSourceTx:"),
        "{}",
        errors[0]
    );
    let txs = crate::tx::infer_usecase_transactions(&model);
    let groups: Vec<Vec<String>> = txs[0]
        .groups()
        .map(|g| {
            g.ordered_writes
                .iter()
                .map(|w| model.entities[w.entity].id.clone())
                .collect()
        })
        .collect();
    assert_eq!(groups, vec![vec!["Session"], vec!["SessionView"]]);
}

#[test]
fn provenance_names_the_use_case_not_the_group_step() {
    let d = derive_src(&split_pay(""));
    let via: Vec<String> = d
        .entity("Order")
        .patterns
        .iter()
        .flat_map(|p| p.provenance.via.iter().map(|(_, uc)| uc.clone()))
        .collect();
    assert!(via.contains(&"Pay".to_string()), "{via:?}");
    assert!(via.iter().all(|uc| !uc.starts_with("Pay_")), "{via:?}");
}

#[test]
fn after_assert_is_checked_after_each_group_step() {
    let src = split_pay("after(Pay).assert(Order.status == paid, Payment.status == captured)");
    let ts = built(&src);
    let [a] = ts.after_asserts() else {
        panic!("one assert")
    };
    let steps: Vec<String> = a
        .steps
        .iter()
        .map(|s| ts.actions()[s.act.index()].name.clone())
        .collect();
    assert_eq!(steps, vec!["Pay_1", "Pay_2"]);
    // Each step binds its own group's entity; the other group's is universally bound.
    let binders: Vec<Vec<String>> = a
        .steps
        .iter()
        .map(|s| s.binders.iter().map(|e| ts.entity(*e).id.clone()).collect())
        .collect();
    assert_eq!(binders, vec![vec!["Order"], vec!["Payment"]]);
    // After Pay_1 alone the Order is still pending: the assert sees the in-between state.
    let d = derive_src(&src);
    assert!(
        d.diags.iter().any(|x| matches!(
            &x.diag,
            StateDiag::AfterAssertViolated { anchor, .. } if anchor == "Pay"
        )),
        "{:?}",
        d.diags
    );
}

#[test]
fn a_condition_guards_every_group_of_the_use_case() {
    let conditions = |body: &str| {
        split_pay(&format!(
            "condition Ready \"可\"\nholds(Ready, {body})\napplies_to(Ready, Pay)\n\
             usecase Rush \"急ぎ\"\nsets(Rush, Order, rush == true)\nsets(Staff, Desk, open == true)"
        ))
    };
    let guarded = |ts: &TransitionSystem| -> Vec<String> {
        ts.actions()
            .iter()
            .filter(|a| {
                a.guards
                    .iter()
                    .any(|g| matches!(g.why, GuardWhy::Condition { .. }))
            })
            .map(|a| a.name.clone())
            .collect()
    };
    // A body is a precondition of the use case, so each group's step carries it, whether a
    // group writes the entity the body reads or no group does (#78).
    for body in ["Order.rush == true", "Desk.open == true"] {
        assert_eq!(
            guarded(&built(&conditions(body))),
            vec!["Pay_1", "Pay_2"],
            "{body}"
        );
    }
}

/// OpenSettlement moves the claim through one API and creates the settlement under it through
/// another, guarded by the claim's kind (#78). Each group's step carries the guard, and the
/// settlement's step reads it on the claim the settlement is created under, so no settlement
/// is ever created under a vehicle claim.
const GUARDED_SPLIT: &str = r#"
variation Kind "種目" {
  liability "賠償"
  vehicle "車両"
}
entity Claim "案件" { id: Int @pk  kind: Kind  status: Enum(open, negotiating, closed) @default(open) }
entity Settlement "示談" { id: Int @pk  status: Enum(pending, agreed) @default(pending) }
relate(Settlement, Claim, N:1)
usecase Report "受付"
usecase Close "終了"
usecase OpenSettlement "示談開始"
api ClaimApi "案件 API"
api SettlementApi "示談 API"
creates(Report, Claim)
transitions(Close, Claim.status, open -> closed)
invokes(OpenSettlement, ClaimApi)
updates(ClaimApi, Claim)
transitions(OpenSettlement, Claim.status, open -> negotiating)
invokes(OpenSettlement, SettlementApi)
creates(SettlementApi, Settlement)
condition Eligible "賠償責任の事故"
holds(Eligible, Claim.kind == liability)
forbidden(Claim, Settlement, Claim.kind == vehicle, Settlement.status == pending).along(Claim, Settlement)
"#;

#[test]
fn a_guard_holds_for_every_group_of_a_split_use_case() {
    let violated = |src: &str| {
        derive_src(src)
            .diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. }))
    };
    assert!(violated(GUARDED_SPLIT), "the rule needs the guard");
    let guarded = format!("{GUARDED_SPLIT}applies_to(Eligible, OpenSettlement)\n");
    assert!(!violated(&guarded));
    let ts = built(&guarded);
    // The settlement's step binds the claim it creates the settlement under, and the guard
    // reads that row.
    assert_eq!(bound(&ts, "OpenSettlement_2"), vec!["Settlement", "Claim"]);
}
