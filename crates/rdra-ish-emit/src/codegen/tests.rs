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
  method: PaymentMethod
  delivered_at: DateTime @null
  type: String @null
}
entity OrderLine "Order Line" {
  id: Int @pk
  qty: Int
}
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
    let domain = build(&model, &|_| None).unwrap();
    let order = domain.records.iter().find(|r| r.name == "Order").unwrap();
    let key = order
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
    let err = build(&model, &|_| None).unwrap_err().to_string();
    assert!(err.contains("OrderStatus"), "{err}");
    assert!(err.contains("entity OrderStatus"), "{err}");
}
