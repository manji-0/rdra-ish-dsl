//! Condition guards: `applies_to(Cond, UC)` with a `holds` body guards the use case's action.

use super::*;
use crate::lifecycle::{GuardWhy, InvKind, Pred};

/// A payment under a visit, and a room no relate reaches. `{body}` and `{extra}` complete it.
fn clinic(body: &str, extra: &str) -> String {
    format!(
        r#"
entity Visit "来院" {{ id: Int @pk  status: Enum(booked, arrived) }}
entity Payment "支払い" {{ id: Int @pk  status: Enum(open, paid)  method: Enum(cash, insurance) }}
entity Room "診察室" {{ id: Int @pk  state: Enum(free, busy) }}
relate(Payment, Visit, N:1)
usecase Book "予約"
usecase Arrive "到着"
usecase Open "会計開始"
usecase Pay "支払い"
usecase Prepare "準備"
usecase Occupy "入室"
creates(Book, Visit)
transitions(Visit.status, Arrive, booked -> arrived)
creates(Open, Payment)
transitions(Payment.status, Pay, open -> paid)
creates(Prepare, Room)
transitions(Room.state, Occupy, free -> busy)
condition Ready "支払い可能"
{body}
applies_to(Ready, Pay)
{extra}
"#
    )
}

fn pay_guards(model: &SemanticModel) -> Vec<Pred> {
    let built = TransitionSystem::build(model, Bounds::STATES, &Scope::All);
    let pay = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "Pay")
        .expect("Pay action");
    pay.guards
        .iter()
        .filter(|g| matches!(g.why, GuardWhy::Condition { .. }))
        .map(|g| g.pred.clone())
        .collect()
}

fn reaches_paid(d: &Derived) -> bool {
    d.rows("Payment").iter().any(|r| r.contains("status=paid"))
}

#[test]
fn a_bound_atom_reads_the_param() {
    let without = derive_src(&clinic("holds(Ready, Payment.method == insurance)", ""));
    assert!(!reaches_paid(&without), "{:?}", without.rows("Payment"));
    let model = model_from(&clinic(
        "holds(Ready, Payment.method == insurance)",
        "usecase Insure \"保険\"\nsets(Insure, Payment, method == insurance)",
    ));
    let guards = pay_guards(&model);
    assert!(
        matches!(guards.as_slice(), [Pred::And(atoms)] if atoms.len() == 1 && matches!(atoms[0], Pred::Cmp(..))),
        "{guards:?}"
    );
    let d = derive(&model);
    assert!(
        d.rows("Payment")
            .contains(&"method=insurance, status=paid".to_string()),
        "{:?}",
        d.rows("Payment")
    );
    assert!(
        !d.rows("Payment")
            .contains(&"method=cash, status=paid".to_string()),
        "{:?}",
        d.rows("Payment")
    );
}

#[test]
fn an_unbound_atom_is_an_exists_along_the_owner() {
    let src = clinic(
        "holds(Ready, Visit.status == arrived)",
        "invariant(Payment, Visit)\n  .along(Payment, Visit)\n  .when(Payment.status == paid)\n  .then(Visit.status == arrived)",
    );
    let model = model_from(&src);
    let guards = pay_guards(&model);
    let [Pred::Exists { body, .. }] = guards.as_slice() else {
        panic!("{guards:?}");
    };
    assert!(
        matches!(body.as_ref(), Pred::And(parts) if parts.len() == 2),
        "owner link and atom: {body:?}"
    );
    let d = derive(&model);
    assert!(reaches_paid(&d));
    // Two visits, one arrived: only a payment of the arrived visit can be paid.
    let d = derive_scoped(&model, Bounds::VERIFY, &Scope::All);
    let violated = d.diags.iter().any(|x| {
        matches!(
            &x.diag,
            StateDiag::ConstraintViolated {
                kind: InvKind::CrossInvariant,
                ..
            }
        )
    });
    assert!(!violated, "{:?}", d.diags);
}

#[test]
fn an_unbound_atom_with_no_relate_is_an_independent_exists() {
    let model = model_from(&clinic("holds(Ready, Room.state == busy)", ""));
    let guards = pay_guards(&model);
    let [Pred::Exists { body, .. }] = guards.as_slice() else {
        panic!("{guards:?}");
    };
    assert!(
        matches!(body.as_ref(), Pred::And(parts) if parts.len() == 1),
        "no owner link: {body:?}"
    );
    assert!(reaches_paid(&derive(&model)));
    let never_busy = model_from(
        &clinic("holds(Ready, Room.state == busy)", "")
            .replace("transitions(Room.state, Occupy, free -> busy)", ""),
    );
    assert!(!reaches_paid(&derive(&never_busy)));
}

#[test]
fn a_body_on_a_use_case_that_writes_nothing_is_unbindable() {
    let d = derive_src(&clinic(
        "holds(Ready, Visit.status == arrived)",
        "usecase Look \"照会\"\nreads(Look, Visit)\napplies_to(Ready, Look)",
    ));
    let unbindable: Vec<_> = d
        .diags
        .iter()
        .filter(|x| matches!(x.diag, StateDiag::UnbindableCondition { .. }))
        .collect();
    assert_eq!(unbindable.len(), 1, "{:?}", d.diags);
    assert!(crate::lifecycle::has_errors(&d.diags));
}

#[test]
fn a_condition_without_a_body_adds_nothing() {
    let with_trace = model_from(&clinic("", "").replace(
        "condition Ready \"支払い可能\"\n",
        "condition Ready \"支払い可能\"\ncombines(Ready, Visit.status)\n",
    ));
    assert!(pay_guards(&with_trace).is_empty());
    let d = derive(&with_trace);
    assert!(reaches_paid(&d));
    assert!(!crate::lifecycle::has_errors(&d.diags), "{:?}", d.diags);
}

#[test]
fn an_atom_on_a_created_param_constrains_what_is_created() {
    let src = r#"
variation Method "支払方法" {
  cash "現金"
  insurance "保険"
}
entity Visit "来院" { id: Int @pk  status: Enum(booked, arrived) }
entity Payment "支払い" { id: Int @pk  method: Method @default(cash) }
relate(Payment, Visit, N:1)
usecase Book "予約"
usecase Arrive "到着"
usecase Collect "徴収"
creates(Book, Visit)
transitions(Visit.status, Arrive, booked -> arrived)
creates(Collect, Payment)
condition Covered "保険適用"
holds(Covered, Payment.method == insurance, Visit.status == arrived)
applies_to(Covered, Collect)
invariant(Payment, Visit)
  .along(Payment, Visit)
  .when(Payment.method == insurance)
  .then(Visit.status == arrived)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
    let collect = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "Collect")
        .expect("Collect action");
    let cond: Vec<_> = collect
        .guards
        .iter()
        .filter(|g| matches!(g.why, GuardWhy::Condition { .. }))
        .collect();
    assert!(
        matches!(cond.as_slice(), [g] if g.reads_successor()),
        "{cond:?}"
    );
    let d = derive(&model);
    assert_eq!(d.rows("Payment"), vec!["method=insurance"]);
    assert!(!crate::lifecycle::has_errors(&d.diags), "{:?}", d.diags);
    // The parent atom reads the pre-state: at two instances only an arrived visit's payment.
    let d = derive_scoped(&model, Bounds::VERIFY, &Scope::All);
    assert!(!crate::lifecycle::has_errors(&d.diags), "{:?}", d.diags);
}
