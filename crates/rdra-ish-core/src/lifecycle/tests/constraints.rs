//! Constraints, after-asserts, and quantifiers checked on reached states.

use super::*;
use crate::lifecycle::InvKind;

fn violated(d: &Derived, id: &str, kind: InvKind) -> bool {
    d.has(
        id,
        |x| matches!(x, StateDiag::ConstraintViolated { kind: k, .. } if *k == kind),
    )
}

fn witnesses(d: &Derived, id: &str) -> Vec<String> {
    d.entity(id)
        .diagnostics
        .iter()
        .filter_map(|x| match x {
            StateDiag::ConstraintViolated { pattern_desc, .. } => Some(pattern_desc.clone()),
            _ => None,
        })
        .collect()
}

const ORDER: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending)  note: String @null }
usecase Place "注文"
usecase Pay "支払"
usecase Cancel "取消"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Cancel, Order.status, pending -> cancelled)
"#;

#[test]
fn forbidden_reachable_state_is_reported_with_its_pattern() {
    let d = derive_src(&format!("{ORDER}\nforbidden(Order, status == cancelled)"));
    assert!(violated(&d, "Order", InvKind::Forbidden));
    // The verdict comes from the forbidden's own cone, which `note` is not in.
    assert_eq!(witnesses(&d, "Order"), vec!["status=cancelled"]);
}

#[test]
fn forbidden_conditions_are_a_conjunction() {
    let d = derive_src(&format!(
        "{ORDER}\nforbidden(Order, status == cancelled, note == present)"
    ));
    assert!(!violated(&d, "Order", InvKind::Forbidden));
}

#[test]
fn forbidden_unreachable_state_passes() {
    let d = derive_src(
        r#"
entity Item "I" { id: Int @pk  status: Enum(active, inactive) @default(active) }
usecase Create "作成"
creates(Create, Item)
forbidden(Item, status == inactive)
"#,
    );
    assert!(!violated(&d, "Item", InvKind::Forbidden));
}

#[test]
fn correlated_effects_of_one_use_case_stay_correlated() {
    let d = derive_src(
        r#"
entity DispenseReception "調剤受付" {
  id: Int @pk
  validity: Enum(active, cancelled) @default(active)
  progress: Enum(open, completed) @default(open)
  correction: Enum(none, correcting) @default(none)
  recalc: Bool
}
usecase ReceiveDispense "受付"
usecase CompleteAccounting "会計確定"
usecase CancelReception "取消"
usecase StartCorrection "訂正開始"
creates(ReceiveDispense, DispenseReception)
sets(CompleteAccounting, DispenseReception.validity == active)
sets(CompleteAccounting, DispenseReception.progress == completed)
sets(CompleteAccounting, DispenseReception.correction == none)
sets(CompleteAccounting, DispenseReception.recalc == false)
sets(CancelReception, DispenseReception.validity == cancelled)
sets(CancelReception, DispenseReception.progress == open)
sets(CancelReception, DispenseReception.correction == none)
sets(CancelReception, DispenseReception.recalc == false)
sets(StartCorrection, DispenseReception.validity == active)
sets(StartCorrection, DispenseReception.progress == open)
sets(StartCorrection, DispenseReception.correction == correcting)
sets(StartCorrection, DispenseReception.recalc == true)
forbidden(DispenseReception, validity == cancelled, progress == completed)
forbidden(DispenseReception, validity == cancelled, correction == correcting)
forbidden(DispenseReception, progress == completed, correction == correcting)
"#,
    );
    assert!(
        !violated(&d, "DispenseReception", InvKind::Forbidden),
        "{:?}",
        d.diags
    );
}

#[test]
fn invariant_when_then_is_checked() {
    let d = derive_src(&format!(
        "{ORDER}\ninvariant(Order).when(status == paid).then(note == present)"
    ));
    assert!(violated(&d, "Order", InvKind::Invariant));
    assert_eq!(witnesses(&d, "Order"), vec!["note=null, status=paid"]);
    let d = derive_src(&format!(
        "{ORDER}\nsets(Pay, Order.note == present)\ninvariant(Order).when(status == paid).then(note == present)"
    ));
    assert!(!violated(&d, "Order", InvKind::Invariant));
}

#[test]
fn required_and_exclusive_are_checked() {
    let d = derive_src(&format!("{ORDER}\nrequired(Order, note == null)"));
    assert!(!violated(&d, "Order", InvKind::Required));
    let d = derive_src(&format!("{ORDER}\nrequired(Order, status == pending)"));
    assert!(violated(&d, "Order", InvKind::Required));
    let d = derive_src(&format!(
        "{ORDER}\nsets(Cancel, Order.note == present)\nexclusive(Order, status == cancelled, note == present)"
    ));
    assert!(violated(&d, "Order", InvKind::Exclusive));
}

#[test]
fn driven_comparison_reads_its_proposition() {
    let d = derive_src(
        r#"
entity Item "I" { id: Int @pk  stock: Int  selling: Int  status: Enum(on, off) }
usecase Make "作る"
usecase Sell "売る"
creates(Make, Item)
drives(Make, Item.stock < selling, false)
drives(Sell, Item.stock < selling, true)
forbidden(Item, status == on, stock < selling)
"#,
    );
    let item = d.entity("Item");
    let axes: Vec<&str> = item.axes.iter().map(|a| a.column.as_str()).collect();
    assert_eq!(axes, vec!["status", "__cmp:stock<selling"]);
    assert!(violated(&d, "Item", InvKind::Forbidden));
}

#[test]
fn driven_comparison_contradicting_the_same_use_case_ints_is_an_error() {
    let src = r#"
entity Item "Item" {
  id: Int @pk
  stock: Int
  selling: Int
  status: Enum(active, archived) @default(active)
}
usecase Sell "Sell"
creates(Sell, Item)
updates(Sell, Item)
sets(Sell, Item.stock == 0)
sets(Sell, Item.selling == 1)
drives(Sell, Item.stock < selling, DRIVEN)
forbidden(Item, stock < selling)
"#;
    let d = derive_src(&src.replace("DRIVEN", "false"));
    let found: Vec<&StateDiag> = d
        .entity("Item")
        .diagnostics
        .iter()
        .filter(|x| matches!(x, StateDiag::ContradictoryDrivenComparison { .. }))
        .collect();
    assert!(
        matches!(
            found.as_slice(),
            [StateDiag::ContradictoryDrivenComparison { driven: false, sites, .. }] if sites.len() == 3
        ),
        "the drive and both Int sets are named: {:?}",
        d.entity("Item").diagnostics
    );
    assert!(crate::lifecycle::has_errors(&d.diags), "fail-closed");
    let agreeing = derive_src(&src.replace("DRIVEN", "true"));
    assert!(!agreeing
        .entity("Item")
        .diagnostics
        .iter()
        .any(|x| matches!(x, StateDiag::ContradictoryDrivenComparison { .. })));
}

#[test]
fn undriven_comparison_is_evaluated_over_the_int_domain() {
    let d = derive_src(
        r#"
entity Item "I" { id: Int @pk  stock: Int @default(2)  selling: Int @default(1) }
usecase Make "作る"
usecase Sell "売る"
creates(Make, Item)
sets(Sell, Item.stock == 0)
forbidden(Item, stock < selling)
"#,
    );
    assert!(violated(&d, "Item", InvKind::Forbidden));
    assert_eq!(witnesses(&d, "Item"), vec!["selling=1, stock=0"]);
}

#[test]
fn int_and_nullable_int_columns_compare_by_their_own_numbers() {
    let d = derive_src(
        r#"
entity Item "I" { id: Int @pk  stock: Int @default(2)  cap: Int @null }
usecase Make "作る"
usecase Cap "上限"
creates(Make, Item)
sets(Cap, Item.cap == 2)
forbidden(Item, stock < cap)
"#,
    );
    assert!(!violated(&d, "Item", InvKind::Forbidden));
}

#[test]
fn cross_forbidden_is_reported_on_each_entity() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Payment "P" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文"
usecase Capture "確定"
creates(Place, Order)
creates(Place, Payment)
transitions(Capture, Payment.status, pending -> captured)
forbidden(Order, Payment, Order.status == open, Payment.status == captured)
"#,
    );
    for id in ["Order", "Payment"] {
        assert!(violated(&d, id, InvKind::CrossForbidden), "{id}");
    }
    assert_eq!(
        witnesses(&d, "Order"),
        vec!["Order(status=open); Payment(status=captured)"]
    );
}

#[test]
fn global_cross_invariant_on_related_entities_is_evaluated() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Payment "P" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
relate(Payment, Order, N:1)
creates(Place, Order)
creates(Place, Payment)
updates(Pay, Order)
updates(Pay, Payment)
transitions(Pay, Order.status, open -> paid)
transitions(Pay, Payment.status, pending -> captured)
invariant(Order, Payment).when(Order.status == paid).then(Payment.status == captured)
"#,
    );
    assert!(
        !violated(&d, "Order", InvKind::CrossInvariant),
        "{:?}",
        d.diags
    );
}

#[test]
fn relation_scoped_invariant_sees_the_created_link() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(paid) }
entity Payment "支払い" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase RegisterPayment "登録"
creates(RegisterPayment, Order)
creates(RegisterPayment, Payment)
relate(Payment, Order, 1:1)
invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#,
    );
    for id in ["Order", "Payment"] {
        assert!(violated(&d, id, InvKind::CrossInvariant), "{id}");
    }
}

#[test]
fn relation_scope_without_a_relate_is_unbindable() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(open, paid) }
entity Payment "P" { id: Int @pk  status: Enum(pending, captured) }
usecase Place "注文"
creates(Place, Order)
creates(Place, Payment)
invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#,
    );
    assert!(d.has("Order", |x| matches!(x, StateDiag::UnbindableScope { .. })));
}

#[test]
fn false_state_condition_makes_a_cross_constraint_hold() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(open)  total: Decimal }
entity Payment "支払い" { id: Int @pk  amount: Decimal }
forbidden(Order, Payment, Payment.amount > Order.total, Order.status == paid)
"#,
    );
    assert!(!crate::lifecycle::has_errors(&d.diags), "{:?}", d.diags);
}

#[test]
fn cross_order_payment_sample_holds_under_atomic_pay() {
    let d = derive(&sample_model(
        "samples/formal-verification/cross_order_payment.rdra",
    ));
    assert!(
        !violated(&d, "Order", InvKind::CrossForbidden),
        "{:?}",
        d.diags
    );
}

#[test]
fn cross_order_payment_split_sample_reaches_the_in_between_state() {
    let d = derive(&sample_model(
        "samples/formal-verification/cross_order_payment_split.rdra",
    ));
    assert!(
        violated(&d, "Order", InvKind::CrossForbidden),
        "{:?}",
        d.diags
    );
}

#[test]
fn untriggered_sets_sample_reaches_its_forbidden_state() {
    let d = derive(&sample_model(
        "samples/formal-verification/untriggered_sets.rdra",
    ));
    assert!(violated(&d, "Order", InvKind::Forbidden));
    assert!(d.has("Order", |x| matches!(
        x,
        StateDiag::UnreachableEnumVariant { variant, .. } if variant == "draft"
    )));
}

#[test]
fn after_assert_passes_on_the_transition_it_names() {
    let d = derive_src(
        r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
creates(ExecuteCertIssue, CertificateOrder)
transitions(ExecuteCertIssue, CertificateOrder.status, requested -> executed)
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
    );
    assert!(!d.has("CertificateOrder", |x| matches!(
        x,
        StateDiag::AfterAssertViolated { .. }
    )));
    assert_eq!(d.rows("CertificateOrder"), vec!["status=executed"]);
}

#[test]
fn after_assert_fails_when_the_step_leaves_the_column() {
    let d = derive_src(
        r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
creates(ExecuteCertIssue, CertificateOrder)
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
    );
    assert!(d.has("CertificateOrder", |x| matches!(
        x,
        StateDiag::AfterAssertViolated { actual, .. } if actual == "CertificateOrder(status=requested)"
    )));
}

#[test]
fn after_assert_is_checked_on_a_step_that_writes_only_what_it_does_not_read() {
    // IssueCert writes only the certificate and the assert reads only the order.
    let d = derive_src(
        r#"
entity CertificateOrder "依頼" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
entity ClientCertificate "証明書" { id: Int @pk }
usecase RequestCert "依頼"
usecase ExecuteCertIssue "発行実行"
usecase IssueCert "証明書作成"
creates(RequestCert, CertificateOrder)
transitions(ExecuteCertIssue, CertificateOrder.status, requested -> executed)
creates(IssueCert, ClientCertificate)
relate(ClientCertificate, CertificateOrder, N:1)
after(IssueCert).assert(CertificateOrder.status == executed)
"#,
    );
    assert!(
        d.has("CertificateOrder", |x| matches!(
            x,
            StateDiag::AfterAssertViolated { actual, .. } if actual.contains("CertificateOrder(status=requested)")
        )),
        "IssueCert can run before ExecuteCertIssue: {:?}",
        d.entity("CertificateOrder").diagnostics
    );
}

#[test]
fn after_assert_on_a_use_case_without_effects_is_an_error() {
    let d = derive_src(
        r#"
usecase Look "見る"
entity Doc "D" { id: Int @pk  status: Enum(a, b) }
after(Look).assert(Doc.status == a)
"#,
    );
    assert!(d.has("Doc", |x| matches!(
        x,
        StateDiag::AssertOnInertUseCase { .. }
    )));
}

#[test]
fn fairness_on_a_use_case_without_effects_is_an_error() {
    let d = derive_src(
        r#"
usecase Look "見る"
usecase Close "閉じる"
entity Doc "D" { id: Int @pk  status: Enum(a, b) }
transitions(Close, Doc.status, a -> b)
property Closes leads_to(Doc.status == a, Doc.status == b).fair(Close, Look)
"#,
    );
    assert!(d.has("Doc", |x| matches!(
        x,
        StateDiag::FairOnInertUseCase { property, usecase, .. }
            if property == "Closes" && usecase == "Look"
    )));
}

#[test]
fn order_after_assert_holds() {
    let d = derive(&sample_model("samples/formal-verification/order.rdra"));
    assert_eq!(d.diags.len(), 0, "{:?}", d.diags);
    let d = derive(&sample_model(
        "samples/formal-verification/after_assert_unmet.rdra",
    ));
    assert!(d
        .diags
        .iter()
        .any(|x| matches!(x.diag, StateDiag::AfterAssertViolated { .. })));
}

#[test]
fn quantifier_none_finds_a_revoked_cert_with_an_active_assignment() {
    let d = derive(&sample_model(
        "samples/formal-verification/quantifier_none.rdra",
    ));
    assert!(violated(&d, "Cert", InvKind::Quantifier), "{:?}", d.diags);
}

/// The related entity is the anchor itself, with no relate between them: the conditions read
/// every other row, not the anchor row (#63).
#[test]
fn quantifier_over_the_anchor_entity_reads_the_quantified_row() {
    let model = model_from(
        r#"
entity Task "作業" { id: Int @pk  status: Enum(open, done) @default(open) }
usecase Add "追加"
usecase Finish "完了"
creates(Add, Task)
transitions(Finish, Task.status, open -> done)
invariant(Task).when(status == done).none(Task.status == open)
"#,
    );
    let d = derive_scoped(&model, Bounds::VERIFY, &Scope::ALL);
    let found = d.diags.iter().any(|x| {
        matches!(
            x.diag,
            StateDiag::ConstraintViolated {
                kind: InvKind::Quantifier,
                ..
            }
        )
    });
    assert!(found, "{:?}", d.diags);
}

#[test]
fn quantifier_none_holds_when_the_related_condition_is_unreachable() {
    let d = derive_src(
        r#"
usecase RevokeCert "Revoke Cert"
usecase AssignTerminal "Assign Terminal"
entity ClientCertificate "Client Certificate" { id: Int @pk  status: Enum(active, revoked) @default(active) }
entity TerminalCertAssignment "Terminal Cert Assignment" { id: Int @pk  status: Enum(active, inactive) @default(inactive) }
creates(RevokeCert, ClientCertificate)
creates(AssignTerminal, TerminalCertAssignment)
sets(RevokeCert, ClientCertificate.status == revoked)
invariant(ClientCertificate).when(ClientCertificate.status == revoked)
  .none(TerminalCertAssignment.status == active)
"#,
    );
    assert!(
        !violated(&d, "ClientCertificate", InvKind::Quantifier),
        "{:?}",
        d.diags
    );
}

#[test]
fn invariant_guards_are_a_conjunction() {
    let d = derive_src(
        r#"
entity Order "注文" {
  id:        Int @pk
  status:    Enum(pending, delivered) @default(pending)
  refunded:  Bool
  refund_id: String @null
}
usecase Place   "注文確定"
usecase Deliver "配達"
usecase Refund  "返金"
creates(Place,   Order)
updates(Deliver, Order)
updates(Refund,  Order)
transitions(Deliver, Order.status, pending -> delivered)
sets(usecase::Refund, Order.refunded == true)
sets(usecase::Refund, Order.refund_id == present)
invariant(Order)
  .when(status == delivered)
  .when(refunded == false)
  .then(refund_id == null)
"#,
    );
    assert!(!violated(&d, "Order", InvKind::Invariant), "{:?}", d.diags);
}

#[test]
fn global_cross_forbidden_on_related_pre_existing_entities_is_evaluated() {
    let d = derive_src(
        r#"
entity Terminal "端末" { id: Int @pk  status: Enum(active, deregistered) @default(deregistered) }
entity ClientCertificate "証明書" { id: Int @pk  status: Enum(active, revoked) @default(active) }
relate(Terminal, ClientCertificate, 1:N)
forbidden(Terminal, ClientCertificate,
  Terminal.status == deregistered,
  ClientCertificate.status == active)
"#,
    );
    for id in ["Terminal", "ClientCertificate"] {
        assert!(violated(&d, id, InvKind::CrossForbidden), "{id}");
    }
}

#[test]
fn cross_comparison_of_numeric_columns_is_evaluated() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  total: Decimal }
entity Payment "支払い" { id: Int @pk  amount: Decimal }
forbidden(Order, Payment, Payment.amount > Order.total)
"#,
    );
    for id in ["Order", "Payment"] {
        assert!(!violated(&d, id, InvKind::CrossForbidden), "{id}");
        assert!(
            !d.entity(id)
                .diagnostics
                .iter()
                .any(|x| x.severity() == crate::lifecycle::Severity::Error),
            "both start at 0, so the comparison is false: {:?}",
            d.entity(id).diagnostics
        );
    }
}

#[test]
fn relation_scoped_constraints_see_the_link_one_creation_makes() {
    let forbidden = derive_src(
        r#"
entity Terminal "端末" { id: Int @pk  status: Enum(active, deregistered) @default(deregistered) }
entity ClientCertificate "証明書" { id: Int @pk  status: Enum(active, suspended) @default(active) }
usecase RegisterTerminalCertificate "登録"
creates(RegisterTerminalCertificate, Terminal)
creates(RegisterTerminalCertificate, ClientCertificate)
relate(Terminal, ClientCertificate, 1:N)
forbidden(Terminal, ClientCertificate,
  Terminal.status == deregistered,
  ClientCertificate.status == active)
  .along(Terminal, ClientCertificate)
"#,
    );
    for id in ["Terminal", "ClientCertificate"] {
        assert!(violated(&forbidden, id, InvKind::CrossForbidden), "{id}");
    }
    let n_to_one = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(draft, submitted) @default(submitted) }
entity LineItem "明細" { id: Int @pk  status: Enum(pending, allocated) @default(pending) }
usecase SubmitOrder "注文確定"
creates(SubmitOrder, Order)
creates(SubmitOrder, LineItem)
relate(LineItem, Order, N:1)
invariant(Order, LineItem)
  .along(Order, LineItem)
  .when(Order.status == submitted)
  .then(LineItem.status == allocated)
"#,
    );
    for id in ["Order", "LineItem"] {
        assert!(violated(&n_to_one, id, InvKind::CrossInvariant), "{id}");
    }
}

#[test]
fn relation_scoped_constraints_hold_on_unlinked_pre_existing_rows() {
    let invariant = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(paid) }
entity Payment "支払い" { id: Int @pk  status: Enum(pending, captured) @default(captured) }
relate(Payment, Order, 1:1)
invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#,
    );
    assert!(!violated(&invariant, "Order", InvKind::CrossInvariant));
    let forbidden = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Payment "支払い" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, 1:1)
forbidden(Order, Payment,
  Order.status == paid,
  Payment.status == pending)
  .along(Order, Payment)
"#,
    );
    assert!(!violated(&forbidden, "Order", InvKind::CrossForbidden));
}

#[test]
fn a_creating_use_case_drives_its_proposition_at_creation() {
    let d = derive_src(
        r#"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
  status: Enum(draft, on_sale) @default(draft)
}
usecase ListItem "出品"
creates(ListItem, Stock)
sets(ListItem, Stock.status == on_sale)
drives(ListItem, Stock.stock < selling, true)
invariant(Stock)
  .when(status == on_sale)
  .then(stock < selling)
"#,
    );
    assert_eq!(
        d.rows("Stock"),
        vec!["__cmp:stock<selling=true, status=on_sale"]
    );
    assert!(!violated(&d, "Stock", InvKind::Invariant));
}

const CART: &str = r#"
entity Line "明細" { id: Int @pk  qty: Int @default(1) @check(qty >= 1) }
usecase Add "追加"
usecase Clear "クリア"
creates(Add, Line)
"#;

#[test]
fn a_check_is_an_invariant_a_use_case_can_break() {
    let d = derive_src(&format!("{CART}\nsets(Clear, Line.qty == 0)"));
    assert!(violated(&d, "Line", InvKind::Check));
    assert_eq!(witnesses(&d, "Line"), vec!["qty=0"]);
    let d = derive_src(&format!("{CART}\nsets(Clear, Line.qty == 3)"));
    assert!(!violated(&d, "Line", InvKind::Check));
}

#[test]
fn a_check_on_a_created_entity_starts_at_the_base_value() {
    let d = derive_src(
        r#"
entity Line "明細" { id: Int @pk  qty: Int @check(qty >= 1) }
usecase Add "追加"
creates(Add, Line)
"#,
    );
    assert!(violated(&d, "Line", InvKind::Check));
}

#[test]
/// A `@check` is two-valued like every other rule (#84): null satisfies no comparison but
/// `!=`, so a check that admits null says `qty == null or ...`.
fn a_null_operand_fails_a_check_unless_the_check_admits_null() {
    let src = |expr: &str, value: &str| {
        format!(
            r#"
entity Line "明細" {{ id: Int @pk  qty: Int @null @default(2) @check({expr}) }}
usecase Add "追加"
usecase Set "設定"
creates(Add, Line)
sets(Set, Line.qty == {value})
"#
        )
    };
    let bounded = "qty >= 1 and qty <= 4";
    assert!(violated(
        &derive_src(&src(bounded, "null")),
        "Line",
        InvKind::Check
    ));
    assert!(!violated(
        &derive_src(&src(bounded, "2")),
        "Line",
        InvKind::Check
    ));
    assert!(violated(
        &derive_src(&src(bounded, "5")),
        "Line",
        InvKind::Check
    ));
    let admits = "qty == null or (qty >= 1 and qty <= 4)";
    assert!(!violated(
        &derive_src(&src(admits, "null")),
        "Line",
        InvKind::Check
    ));
    assert!(violated(
        &derive_src(&src(admits, "5")),
        "Line",
        InvKind::Check
    ));
    assert!(!violated(
        &derive_src(&src("qty != 3", "null")),
        "Line",
        InvKind::Check
    ));
    assert!(violated(
        &derive_src(&src("not (qty != 3)", "null")),
        "Line",
        InvKind::Check
    ));
}

#[test]
fn a_logical_derived_column_is_its_expression_and_no_axis() {
    let src = |qty: u8| {
        format!(
            r#"
entity Line "明細" {{ id: Int @pk  qty: Int @default(1)  bulk: Bool @derived(qty >= 3) }}
usecase Add "追加"
usecase Set "設定"
creates(Add, Line)
sets(Set, Line.qty == {qty})
forbidden(Line, bulk == true)
"#
        )
    };
    let d = derive_src(&src(4));
    assert!(violated(&d, "Line", InvKind::Forbidden));
    assert!(
        d.rows("Line").iter().all(|r| !r.contains("bulk")),
        "{:?}",
        d.rows("Line")
    );
    assert!(!violated(&derive_src(&src(2)), "Line", InvKind::Forbidden));
}

#[test]
fn reading_an_arithmetic_derived_column_is_derived_column_read() {
    let d = derive_src(
        r#"
entity Line "明細" { id: Int @pk  qty: Int  price: Int  total: Int @derived(qty * price) }
usecase Add "追加"
creates(Add, Line)
forbidden(Line, total > 3)
"#,
    );
    assert!(
        d.has(
            "Line",
            |x| matches!(x, StateDiag::DerivedColumnRead { column, .. } if column == "Line.total")
        ),
        "{:?}",
        d.diags
    );
}

/// A comparison prints as written in a violation, `status != paid` or `status in (a, b)`,
/// not `status != paid=true` (#61).
#[test]
fn a_violated_rule_prints_each_comparison_as_written() {
    let d = derive_src(&format!(
        r#"{ORDER}
entity Ticket "Ticket" {{ id: Int @pk  status: Enum(open, done) @default(open) }}
relate(Ticket, Order, N:1)
creates(Place, Ticket)
invariant(Order).when(status != pending).then(status != paid, status != cancelled)
forbidden(Order, status in (paid, cancelled), note == null)
forbidden(Order, Ticket, Order.status not in (pending), Ticket.status == open)
"#
    ));
    let texts: Vec<String> = d
        .diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { text, .. } => Some(format!("{text:?}")),
            _ => None,
        })
        .collect();
    for want in [
        "guards: \"status != pending\", requireds: \"status != paid AND status != cancelled\"",
        "\"note=null AND status in (paid, cancelled)\"",
        "\"Order.status not in (pending) AND Ticket.status=open\"",
    ] {
        assert!(texts.iter().any(|t| t.contains(want)), "{want}: {texts:#?}");
    }
    assert!(texts.iter().all(|t| !t.contains("=true")), "{texts:#?}");
}

#[test]
fn a_violated_rule_names_its_label() {
    let d = derive_src(&format!(
        "{ORDER}\nforbidden(Order, status == cancelled).label(\"取消された注文は残さない\")"
    ));
    assert!(d.has("Order", |x| matches!(
        x,
        StateDiag::ConstraintViolated { name, label, .. }
            if name == "Forbidden_Order_0" && label.as_deref() == Some("取消された注文は残さない")
                && x.to_string().starts_with("「取消された注文は残さない」: Forbidden_Order_0 is violated")
    )));
}

#[test]
fn a_violated_rule_without_label_prints_no_prefix() {
    let d = derive_src(&format!("{ORDER}\nforbidden(Order, status == cancelled)"));
    assert!(d.has("Order", |x| matches!(
        x,
        StateDiag::ConstraintViolated { label: None, .. }
    ) && x
        .to_string()
        .starts_with("Forbidden_Order_0 is violated")));
}

#[test]
fn a_violated_after_assert_names_its_label() {
    let d = derive_src(
        r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
creates(ExecuteCertIssue, CertificateOrder)
after(ExecuteCertIssue).assert(CertificateOrder.status == executed).label("発行後は実行済")
"#,
    );
    assert!(d.has("CertificateOrder", |x| matches!(
        x,
        StateDiag::AfterAssertViolated { label, .. } if label.as_deref() == Some("発行後は実行済")
    ) && x
        .to_string()
        .starts_with("「発行後は実行済」: after(")));
}

/// A `Date` column takes a number, a point on the clock: the creation places the deadline at
/// 2, `expires_at < now` holds once the clock has ticked past it, and the rule that an active
/// coupon is not past its deadline is reachable through `Tick` alone.
#[test]
fn a_date_sets_places_the_deadline_on_the_clock() {
    const COUPON: &str = r#"
entity Coupon "クーポン" { id: Int @pk  status: Enum(active, expired) @default(active)  expires_at: Date }
usecase Issue "発行"
usecase Expire "失効"
creates(Issue, Coupon)
sets(Issue, Coupon.expires_at == 2)
transitions(Expire, Coupon.status, active -> expired)
condition PastDeadline "期限を過ぎた"
holds(PastDeadline, Coupon.expires_at < now)
applies_to(PastDeadline, Expire)
"#;
    let d = derive_src(&format!(
        "{COUPON}\nforbidden(Coupon, status == active, expires_at < now)"
    ));
    assert!(violated(&d, "Coupon", InvKind::Forbidden));
    assert_eq!(witnesses(&d, "Coupon"), vec!["expires_at=2, status=active"]);
    assert!(d
        .rows("Coupon")
        .contains(&"expires_at=2, status=expired".to_string()));

    // Expire runs only after the deadline: before it, the coupon is active and in date.
    let d = derive_src(&format!(
        "{COUPON}\nforbidden(Coupon, status == expired, expires_at >= now)"
    ));
    assert!(!violated(&d, "Coupon", InvKind::Forbidden));
}
