use super::*;
use rdra_ish_core::analysis::build_model;
use rdra_ish_syntax::parse;

fn model_from(src: &str) -> SemanticModel {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    model
}

const SRC: &str = r#"
variation PaymentMethod "Payment Method" {
  credit_card "Credit card"
  bank_transfer
}
entity Customer "Customer" {
  id: Int @pk
  email: String @unique
}
entity Order "Order" {
  id: Int @pk
  total: Money @check(total >= 0)
  status: Enum(pending "Pending", paid, cancelled) @default(pending)
  paid_at: DateTime @null
  method: PaymentMethod
  delivered_at: DateTime @null
  type: String @null
}
entity OrderLine "Order Line" {
  id: Int @pk
  qty: Int
}
usecase Place "Place"
usecase Pay "Pay"
usecase CancelOrder "Cancel"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.paid_at == present)
transitions(CancelOrder, Order.status, pending -> cancelled)
invariant(Order).when(status == paid).then(paid_at == present)
forbidden(Order, status == pending, paid_at == present)
relate(Order, Customer, N:1)
relate(OrderLine, Order, 1..*:1)
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
"#;

fn generated(lang: Lang) -> String {
    let model = model_from(SRC);
    let options = Options {
        lang,
        package: "domain".to_string(),
        model_path: "order.rdra".to_string(),
        version: "x.y.z".to_string(),
        check: CheckStatus::Passed {
            bounds: "1 instance per entity, Int 0..5".to_string(),
        },
    };
    generate(&model, &options, &|_| None).expect("generates")
}

#[test]
fn typescript_snapshot() {
    insta::assert_snapshot!(generated(Lang::TypeScript));
}

#[test]
fn rust_snapshot() {
    insta::assert_snapshot!(generated(Lang::Rust));
}

#[test]
fn go_snapshot() {
    insta::assert_snapshot!(generated(Lang::Go));
}

/// A key column takes its parent's id type, and the obligations name the invariants the
/// verification checks.
#[test]
fn keys_and_obligations() {
    let model = model_from(SRC);
    let domain = build(&model, true, &|_| None).unwrap();
    let order = domain.sums.iter().find(|r| r.name == "Order").unwrap();
    let key = order.variants[0]
        .fields
        .iter()
        .find(|f| f.wire.contains("customer"))
        .unwrap();
    assert_eq!(key.ty, FieldType::Id("CustomerId".to_string()));
    let names: Vec<_> = domain
        .obligations
        .iter()
        .filter_map(|o| o.invariant.as_deref())
        .collect();
    assert!(names.contains(&"Check_Order_total_0"), "{names:?}");
    assert!(names.contains(&"MinCard_OrderLine_Order"), "{names:?}");
}

/// Two declarations that generate one type name are an error naming both.
#[test]
fn a_name_collision_is_an_error() {
    let model = model_from(
        r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, closed)
}
entity OrderStatus "Order Status" {
  id: Int @pk
}
"#,
    );
    let err = build(&model, true, &|_| None).unwrap_err().to_string();
    assert!(err.contains("OrderStatus"), "{err}");
    assert!(err.contains("entity OrderStatus"), "{err}");
}

/// The declared refinements of the sample: `paid_at` present in `paid` by the invariant,
/// null in `pending` by the forbidden rule, and nullable in `cancelled`, where no rule says.
#[test]
fn rules_refine_the_states_they_name() {
    let model = model_from(SRC);
    let domain = build(&model, true, &|_| None).unwrap();
    let order = domain.sums.iter().find(|s| s.name == "Order").unwrap();
    let variant = |name: &str| order.variants.iter().find(|v| v.name == name).unwrap();
    let paid_at = |v: &ir::VariantDecl| v.fields.iter().find(|f| f.wire == "paid_at").cloned();
    assert!(!paid_at(variant("OrderPaid")).unwrap().nullable);
    assert!(paid_at(variant("OrderPending")).is_none());
    assert!(variant("OrderPending")
        .always_null
        .iter()
        .any(|f| f.wire == "paid_at"));
    assert!(paid_at(variant("OrderCancelled")).unwrap().nullable);

    // A model `check` rejects refines nothing.
    let unrefined = build(&model, false, &|_| None).unwrap();
    let order = unrefined.sums.iter().find(|s| s.name == "Order").unwrap();
    assert!(order
        .variants
        .iter()
        .all(|v| v.always_null.is_empty()
            && v.fields.iter().all(|f| f.wire != "paid_at" || f.nullable)));
}

/// A rule with another atom, or with `.when(Cond)`, states a fact about a combination and
/// refines no state; two rules that refine one state both ways leave it nullable.
#[test]
fn only_single_state_rules_refine() {
    let model = model_from(
        r#"
entity Ticket "Ticket" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  urgent: Bool @default(false)
  closed_at: DateTime @null
  note: String @null
}
usecase Open "Open"
usecase Close "Close"
creates(Open, Ticket)
transitions(Close, Ticket.status, open -> closed)
sets(Close, Ticket.closed_at == present)
condition Urgent "Urgent"
holds(Urgent, Ticket.urgent == true)
forbidden(Ticket, status == open, urgent == true, closed_at == present)
invariant(Ticket).when(status == closed).then(closed_at == present).when(Urgent)
invariant(Ticket).when(status == open).then(note == present)
forbidden(Ticket, status == open, note == present)
"#,
    );
    let refinements = |m: &SemanticModel| {
        let (key, entity) = m.entities.iter().next().unwrap();
        states::refinements(m, key, entity, "status", &["open".into(), "closed".into()])
    };
    let found = refinements(&model);
    assert!(found.is_empty(), "{found:?}");
}
