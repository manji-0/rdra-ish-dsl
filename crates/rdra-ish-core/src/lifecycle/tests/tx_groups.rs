//! Vision rule 6: a use case whose writes span k transaction groups is k actions, and the
//! checker explores the states in between.

use super::*;
use crate::lifecycle::{ActionOrigin, Domain, Effect, GuardWhy, ParamRole, Rhs, Value, VarKind};

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
    // The APIs in invokes order: the capture first. The second step binds the payment the
    // first step captured, and the order it belongs to.
    assert_eq!(bound(&ts, "Pay_1"), vec!["Payment"]);
    assert_eq!(bound(&ts, "Pay_2"), vec!["Order", "Payment"]);
    let pay_2 = ts.actions().iter().find(|a| a.name == "Pay_2").unwrap();
    assert_eq!(pay_2.params[1].role, ParamRole::Earlier { step: 1 });
    assert!(pay_2
        .guards
        .iter()
        .any(|g| matches!(g.why, GuardWhy::Owner)));
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
sets(Logout, SessionView.status == ended)
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

fn after_violated(d: &Derived, anchor: &str) -> bool {
    d.diags.iter().any(|x| {
        matches!(
            &x.diag,
            StateDiag::AfterAssertViolated { anchor: a, .. } if a == anchor
        )
    })
}

/// `after(UC).assert` is read once per run, after its last step, on the run's rows (#83):
/// the payment step 1 captured is the last step's `Earlier` row, so the assert never sees
/// the state between the two steps, and a run whose last step leaves the assert false
/// violates it.
#[test]
fn after_assert_is_checked_after_the_last_step_of_a_run() {
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
    assert_eq!(steps, vec!["Pay_2"]);
    assert!(a.steps[0].binders.is_empty());
    assert!(!after_violated(&derive_src(&src), "Pay"));
    let broken =
        split_pay("after(Pay).assert(Order.status == pending, Payment.status == captured)");
    assert!(after_violated(&derive_src(&broken), "Pay"));
}

/// The invokes order decides which step creates the payment. Created first, it is the last
/// step's `Earlier` row; created last, the order is. Either way the assert reads the run's
/// payment, not another order's refunded one, so it holds at two instances. A `Refund` that
/// runs between the two steps refunds the run's payment, and the assert sees that.
#[test]
fn after_assert_reads_the_rows_of_the_run_in_either_step_order() {
    let pay = |first: &str, second: &str, refund: bool| {
        let refund = if refund {
            "usecase Refund \"返金\"\nupdates(Refund, Payment)\ntransitions(Refund, Payment.status, captured -> refunded)"
        } else {
            ""
        };
        format!(
            r#"
entity Order "注文" {{ id: Int @pk  status: Enum(pending, paid) @default(pending) }}
entity Payment "決済" {{ id: Int @pk  status: Enum(captured, refunded) @default(captured) }}
relate(Payment, Order, N:1)
api OrderApi "mark paid"
api PaymentApi "capture"
usecase Place "注文"
usecase Pay "支払"
creates(Place, Order)
invokes(Pay, {first})
invokes(Pay, {second})
updates(OrderApi, Order)
creates(PaymentApi, Payment)
transitions(Pay, Order.status, pending -> paid)
{refund}
after(Pay).assert(Order.status == paid, Payment.status == captured)
"#
        )
    };
    let holds = |first: &str, second: &str, refund: bool| {
        let model = model_from(&pay(first, second, refund));
        let two = derive_scoped(&model, Bounds::VERIFY, &Scope::ALL);
        !after_violated(&two, "Pay")
    };
    assert!(holds("PaymentApi", "OrderApi", false));
    assert!(holds("OrderApi", "PaymentApi", true));
    assert!(!holds("PaymentApi", "OrderApi", true));
}

/// An `after` atom on an entity the run never binds reads every present row of it, and
/// says so.
#[test]
fn an_after_atom_on_an_entity_the_run_never_binds_warns() {
    let unbound = |extra: &str| -> Vec<(String, String)> {
        TransitionSystem::build(&model_from(&split_pay(extra)), Bounds::STATES, &Scope::ALL)
            .diags
            .iter()
            .filter_map(|d| match &d.diag {
                StateDiag::UnboundAfterRead { anchor, entity, .. } => {
                    Some((anchor.clone(), entity.clone()))
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        unbound("sets(Staff, Desk.open == true)\nafter(Pay).assert(Desk.open == true)"),
        vec![("Pay".to_string(), "Desk".to_string())]
    );
    assert!(
        unbound("after(Pay).assert(Order.status == paid, Payment.status == captured)").is_empty()
    );
}

#[test]
fn a_condition_guards_the_first_step_of_the_use_case_and_an_api_body_its_own_step() {
    let conditions = |target: &str, body: &str| {
        split_pay(&format!(
            "condition Ready \"可\"\nholds(Ready, {body})\napplies_to(Ready, {target})\n\
             usecase Rush \"急ぎ\"\nsets(Rush, Order.rush == true)\nsets(Staff, Desk.open == true)"
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
    // A body applied to the use case is the precondition of its run: the first step carries
    // it, whether a later step writes the entity the body reads or no step does, and the
    // later steps run because the first did (#80).
    for body in ["Order.rush == true", "Desk.open == true"] {
        assert_eq!(
            guarded(&built(&conditions("Pay", body))),
            vec!["Pay_1"],
            "{body}"
        );
    }
    // A body applied to an API is the precondition of that API's transaction.
    assert_eq!(
        guarded(&built(&conditions("PayOrderApi", "Desk.open == true"))),
        vec!["Pay_2"]
    );
    assert_eq!(
        guarded(&built(&conditions(
            "CapturePaymentApi",
            "Desk.open == true"
        ))),
        vec!["Pay_1"]
    );
}

/// OpenSettlement moves the claim through one API and creates the settlement under it through
/// another, guarded by the claim's kind (#78, #80). The first step carries the guard, and the
/// settlement's step creates the settlement under the claim the first step moved, so no
/// settlement is ever created under a vehicle claim.
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
fn a_guard_on_the_first_step_protects_the_run() {
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
    // The settlement's step binds the claim the first step moved, and creates the settlement
    // under it; the guard was read on that row by the first step.
    assert_eq!(bound(&ts, "OpenSettlement_2"), vec!["Settlement", "Claim"]);
}

/// The run marks of a split use case: one per slot of each entity a step before the last
/// writes, on no entity only the last step writes, on no use case with one group, and never
/// an axis of `states`.
#[test]
fn a_split_use_case_marks_the_rows_its_earlier_steps_write() {
    let model = sample_model("samples/formal-verification/split_run_order.rdra");
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let marks: Vec<(String, String)> = ts
        .vars()
        .iter()
        .filter_map(|d| match &d.kind {
            VarKind::Run { entity, usecase } => {
                Some((ts.entity(*entity).id.clone(), usecase.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        marks,
        vec![("Claim".to_string(), "OpenSettlement".to_string())]
    );
    let mark = ts
        .var_ids()
        .find(|v| matches!(ts.var(*v).kind, VarKind::Run { .. }))
        .unwrap();
    assert_eq!(
        ts.var(mark).domain,
        Domain::Enum(vec!["none".to_string(), "1".to_string()])
    );
    assert!(ts
        .entities()
        .iter()
        .all(|e| !e.columns.contains(&mark) && !e.owners.contains(&mark)));
    let step = |name: &str| ts.actions().iter().find(|a| a.name == name).unwrap();
    // Step 1 takes a claim in no run and marks it; step 2 binds the marked claim as the row
    // it creates the settlement under, and ends the run.
    let step_1 = step("OpenSettlement_1");
    assert!(step_1
        .guards
        .iter()
        .any(|g| g.why == GuardWhy::Run { step: 0 }));
    let step_2 = step("OpenSettlement_2");
    assert_eq!(bound(&ts, "OpenSettlement_2"), vec!["Settlement", "Claim"]);
    assert_eq!(step_2.params[1].role, ParamRole::Earlier { step: 1 });
    assert!(step_2
        .guards
        .iter()
        .any(|g| g.why == GuardWhy::Run { step: 1 }));
    let marks_set: Vec<Value> = step_2
        .effects
        .iter()
        .filter_map(|eff| match eff {
            Effect::Assign {
                var,
                rhs: Rhs::Lit(v),
                ..
            } if *var == mark => Some(*v),
            _ => None,
        })
        .collect();
    assert_eq!(marks_set, vec![Value(0)]);
    assert!(step_2.effects.iter().any(|eff| matches!(
        eff,
        Effect::Assign {
            rhs: Rhs::Param(1),
            ..
        }
    )));
}

/// The repro of #80 at 2 instances: the settlement is created once, under the claim step 1
/// moved, so the 1:1 key holds. Unordered steps created a second settlement under a claim
/// that already had one.
#[test]
fn a_run_finishes_on_the_rows_it_started() {
    let model = sample_model("samples/formal-verification/split_run_order.rdra");
    let d = derive_scoped(&model, Bounds::VERIFY, &Scope::ALL);
    let violated: Vec<&StateDiag> = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .filter(|x| matches!(x, StateDiag::ConstraintViolated { .. }))
        .collect();
    assert!(violated.is_empty(), "{violated:?}");
    assert_eq!(
        d.rows("Claim"),
        vec![
            "kind=liability, status=open",
            "kind=vehicle, status=open",
            "kind=liability, status=negotiating",
            "kind=liability, status=closed",
            "kind=vehicle, status=closed",
        ]
    );
}

/// Between two steps of a run any other use case runs: a flag set on a negotiating claim
/// with no settlement yet, then the settlement step, reaches the forbidden pair. Without the
/// flagging use case the pair is unreachable.
const INTERLEAVED_SPLIT: &str = r#"
entity Claim "案件" { id: Int @pk  status: Enum(open, negotiating, closed) @default(open)  flagged: Bool @default(false) }
entity Settlement "示談" { id: Int @pk  status: Enum(pending, agreed) @default(pending) }
relate(Settlement, Claim, 1:1)
usecase Report "受付"
usecase OpenSettlement "示談開始"
usecase Flag "注意"
api ClaimApi "案件 API"
api SettlementApi "示談 API"
creates(Report, Claim)
invokes(OpenSettlement, ClaimApi)
updates(ClaimApi, Claim)
transitions(OpenSettlement, Claim.status, open -> negotiating)
invokes(OpenSettlement, SettlementApi)
creates(SettlementApi, Settlement)
sets(Flag, Claim.flagged == true)
condition NoSettlementYet "示談未作成"
holds(NoSettlementYet, Claim.status == negotiating).none(Settlement)
applies_to(NoSettlementYet, Flag)
forbidden(Claim, Settlement, Claim.flagged == true, Settlement.status == pending).along(Claim, Settlement)
"#;

#[test]
fn another_use_case_interleaves_between_two_steps_of_a_run() {
    let d = derive_src(INTERLEAVED_SPLIT);
    let trace = d.diags.iter().find_map(|x| match &x.diag {
        StateDiag::ConstraintViolated { trace, .. } => Some(trace),
        _ => None,
    });
    let steps: Vec<&str> = trace
        .unwrap_or_else(|| panic!("the pair is reached: {:?}", d.diags))
        .steps
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(
        steps,
        vec!["Report", "OpenSettlement_1", "Flag", "OpenSettlement_2"]
    );
    let without_flag = INTERLEAVED_SPLIT
        .replace("applies_to(NoSettlementYet, Flag)", "")
        .replace("sets(Flag, Claim.flagged == true)", "");
    let d = derive_src(&without_flag);
    assert!(
        !d.diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. })),
        "{:?}",
        d.diags
    );
}

/// A later step whose guard fails when its turn comes waits, and the row stays in the run:
/// the run is never abandoned, so the claim is stuck at negotiating, and the deadlock names
/// what blocks each way out.
#[test]
fn a_blocked_run_is_never_abandoned() {
    let src = r#"
entity Claim "案件" { id: Int @pk  status: Enum(open, negotiating, closed) @default(open)  ok: Bool @default(true) }
entity Settlement "示談" { id: Int @pk  status: Enum(pending, agreed) @default(pending) }
relate(Settlement, Claim, 1:1)
usecase Report "受付"
usecase OpenSettlement "示談開始"
usecase Veto "却下"
usecase Close "終了"
api ClaimApi "案件 API"
api SettlementApi "示談 API"
creates(Report, Claim)
invokes(OpenSettlement, ClaimApi)
updates(ClaimApi, Claim)
transitions(OpenSettlement, Claim.status, open -> negotiating)
invokes(OpenSettlement, SettlementApi)
creates(SettlementApi, Settlement)
sets(Veto, Claim.ok == false)
transitions(Close, Claim.status, negotiating -> closed)
condition Ok "可"
holds(Ok, Claim.ok == true)
applies_to(Ok, SettlementApi)
condition Settled "示談あり"
holds(Settled, Settlement.status == pending)
applies_to(Settled, Close)
"#;
    let d = derive_src(src);
    let found = d
        .diags
        .iter()
        .any(|x| matches!(&x.diag, StateDiag::Deadlock { value, .. } if value == "negotiating"));
    assert!(found, "the vetoed run deadlocks: {:?}", d.diags);
}

/// Pay marks the order paid through one API and creates its payment through another, so the
/// payment is created in the second transaction group. `{extra}` completes the model.
fn pay_then_capture(extra: &str) -> String {
    format!(
        r#"
entity Order "注文" {{ id: Int @pk  status: Enum(pending, paid) @default(pending) }}
entity Payment "決済" {{ id: Int @pk  status: Enum(captured, refunded) @default(captured) }}
relate(Payment, Order, N:1)
api OrderApi "mark paid"
api PaymentApi "capture"
usecase Place "注文確定"
usecase Pay "決済"
creates(Place, Order)
invokes(Pay, OrderApi)
invokes(Pay, PaymentApi)
updates(OrderApi, Order)
creates(PaymentApi, Payment)
transitions(Pay, Order.status, pending -> paid)
{extra}
"#
    )
}

/// A `holds` body on a split use case is read before its first transaction (#89). A body that
/// compares an entity only a later group creates reads rows that are not the run's: the
/// warning `GuardReadsLaterStep` names the API of that group to apply it to, and the guard's
/// `GuardWhy` carries the entity for the `Runs_` row. A body on what step 1 writes, one
/// applied to the later group's API, or one on a parent a later group only updates (its row
/// is the run's through the key) draws nothing.
#[test]
fn a_guard_on_what_a_later_transaction_creates_names_the_api_to_apply_it_to() {
    let later = |src: &str| {
        derive_src(src)
            .diags
            .iter()
            .filter_map(|x| match &x.diag {
                StateDiag::GuardReadsLaterStep {
                    condition,
                    entity,
                    step,
                    apis,
                    ..
                } => Some((condition.clone(), entity.clone(), *step, apis.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let captured = "condition Captured \"決済済み\"\nholds(Captured, Payment.status == captured)\n";
    let on_pay = pay_then_capture(&format!("{captured}applies_to(Captured, Pay)"));
    assert_eq!(
        later(&on_pay),
        vec![(
            "Captured".to_string(),
            "Payment".to_string(),
            2,
            vec!["PaymentApi".to_string()]
        )]
    );
    assert!(later(&pay_then_capture(&format!(
        "{captured}applies_to(Captured, PaymentApi)"
    )))
    .is_empty());
    assert!(later(&pay_then_capture(
        "condition Open \"未払い\"\nholds(Open, Order.status == pending)\napplies_to(Open, Pay)"
    ))
    .is_empty());
    assert!(later(&split_pay(
        "condition Rush \"急ぎ\"\nholds(Rush, Order.rush == true)\napplies_to(Rush, Pay)"
    ))
    .is_empty());

    let ts = built(&on_pay);
    let model = model_from(&on_pay);
    let payment = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "Payment")
        .map(|(k, _)| k);
    let first = &ts.actions()[action_names(&ts)
        .iter()
        .position(|n| n == "Pay_1")
        .expect("Pay_1")];
    assert!(
        first
            .guards
            .iter()
            .any(|g| matches!(g.why, GuardWhy::Condition { later, .. } if later == payment)),
        "{:?}",
        first.guards
    );
    let (message, severity) = derive_src(&on_pay)
        .diags
        .iter()
        .find(|x| matches!(x.diag, StateDiag::GuardReadsLaterStep { .. }))
        .map(|x| (x.diag.to_string(), x.diag.severity()))
        .expect("GuardReadsLaterStep");
    assert!(
        message.contains("applies_to(Captured, PaymentApi)"),
        "{message}"
    );
    assert_eq!(severity, crate::lifecycle::Severity::Warning);
}
