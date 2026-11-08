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

const FN_SRC: &str = r#"
entity Customer "C" { id: Int @pk }
entity Order "O" {
  id: Int @pk
  status: Enum(requested, pending, paid, cancelled) @default(requested)
  paid_at: DateTime @null
  qty: Int @default("1")
}
relate(Order, Customer, N:1)
usecase Place "Place"
usecase Pay "Pay"
usecase Cancel "Cancel"
creates(Place, Order)
transitions(Place, Order.status, requested -> pending)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.paid_at == present)
transitions(Cancel, Order.status, (pending, paid) -> cancelled)
condition Small "small"
holds(Small, Order.qty <= 2)
applies_to(Small, Cancel)
invariant(Order).when(status == paid).then(paid_at == present)
forbidden(Order, status == pending, paid_at == present)
"#;

fn functions_of(src: &str) -> DomainModel {
    let model = model_from(src);
    build(&model, true, &|_| None).unwrap()
}

/// A created row that a transition of the same use case moves ends where the arm from its
/// start state goes, and takes its key from the parent it is created under.
#[test]
fn a_created_row_takes_the_arm_from_its_start_state() {
    let domain = functions_of(FN_SRC);
    let place = domain.fns.iter().find(|f| f.name == "Place").unwrap();
    let ir::OutKind::Row {
        ty, tag, fields, ..
    } = &place.outputs[0].kind
    else {
        panic!("Place writes a row");
    };
    assert_eq!(ty, "OrderPending");
    assert_eq!(tag.as_ref().unwrap().1.wire, "pending");
    let key = fields
        .iter()
        .find(|(f, _)| f.wire == "customer_id")
        .unwrap();
    assert!(matches!(key.1, ir::Assigned::ParentId { .. }), "{key:?}");
    let qty = fields.iter().find(|(f, _)| f.wire == "qty").unwrap();
    assert!(
        matches!(
            &qty.1,
            ir::Assigned::Lit {
                lit: ir::Lit::Int(1),
                ..
            }
        ),
        "{qty:?}"
    );
}

/// One function per source state, a condition as a `Rejected` guard, and a value the model
/// leaves open as an input.
#[test]
fn one_function_per_source_state() {
    let domain = functions_of(FN_SRC);
    let names: Vec<&str> = domain.fns.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"CancelFromPending"), "{names:?}");
    assert!(names.contains(&"CancelFromPaid"), "{names:?}");
    let from_paid = domain
        .fns
        .iter()
        .find(|f| f.name == "CancelFromPaid")
        .unwrap();
    assert_eq!(from_paid.rows[0].ty, "OrderPaid");
    assert_eq!(from_paid.conditions.len(), 1);
    assert_eq!(from_paid.conditions[0].id, "Small");
    // paid keeps paid_at present; cancelled leaves it nullable, so it is wrapped.
    let ir::OutKind::Row { fields, .. } = &from_paid.outputs[0].kind else {
        panic!("Cancel writes a row");
    };
    let paid_at = fields.iter().find(|(f, _)| f.wire == "paid_at").unwrap();
    assert!(
        matches!(
            &paid_at.1,
            ir::Assigned::From {
                adapt: ir::Adapt::Wrap,
                ..
            }
        ),
        "{paid_at:?}"
    );
    let pay = domain.fns.iter().find(|f| f.name == "Pay").unwrap();
    assert_eq!(pay.inputs.len(), 1, "paid_at is the caller's value");
    assert!(domain.skipped.is_empty(), "{:?}", domain.skipped);
}

/// A combination the verified rules rule out gets no function: Cancel from a paid row would
/// leave a cancelled row with a payment time, which a rule forbids.
#[test]
fn a_combination_a_rule_rules_out_gets_no_function() {
    let src = FN_SRC.replace(
        "forbidden(Order, status == pending, paid_at == present)",
        "forbidden(Order, status == pending, paid_at == present)\nforbidden(Order, status == cancelled, paid_at == present)",
    );
    let domain = functions_of(&src);
    let names: Vec<&str> = domain.fns.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"CancelFromPending"), "{names:?}");
    assert!(!names.contains(&"CancelFromPaid"), "{names:?}");
}

/// A refinement every reachable row agrees with and no rule states is a candidate rule; a
/// column no use case writes is reported as one, not once per state.
#[test]
fn reachable_refinements_no_rule_states_are_candidates() {
    let src = r#"
entity Order "O" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  paid_at: DateTime @null
  note: String @null
}
usecase Place "Place"
usecase Pay "Pay"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.paid_at == present)
"#;
    let model = model_from(src);
    let domain = build(&model, true, &|_| None).unwrap();
    let c = &domain.candidates;
    assert!(
        c.contains(&"invariant(Order).when(status == paid).then(paid_at == present)".to_string()),
        "{c:?}"
    );
    assert!(
        c.contains(&"forbidden(Order, status == pending, paid_at == present)".to_string()),
        "{c:?}"
    );
    assert!(
        c.iter()
            .any(|x| x.starts_with("Order.note is null in every reachable state")),
        "{c:?}"
    );
    assert!(!c.iter().any(|x| x.contains("note ==")), "{c:?}");
}

/// A raise carries the id of the row it marks; the use case the event triggers takes the
/// event and checks that its row is the event's row.
#[test]
fn an_event_carries_its_row_and_its_handler_takes_it() {
    let src = r#"
entity Order "O" {
  id: Int @pk
  status: Enum(open, shipped) @default(open)
}
entity Shipment "S" { id: Int @pk }
relate(Shipment, Order, N:1)
usecase Place "Place"
usecase Ship "Ship"
usecase Dispatch "Dispatch"
event EvShipped "shipped"
creates(Place, Order)
transitions(Ship, Order.status, open -> shipped)
raises(Ship, EvShipped)
triggers(EvShipped, Dispatch)
creates(Dispatch, Shipment)
"#;
    let model = model_from(src);
    let domain = build(&model, true, &|_| None).unwrap();
    let event = domain
        .events
        .iter()
        .find(|e| e.name == "EvShipped")
        .unwrap();
    assert_eq!(event.fields.len(), 1, "{:?}", event.fields);
    assert_eq!(event.fields[0].name, "order_id");
    let ship = domain.fns.iter().find(|f| f.action == "Ship").unwrap();
    assert!(matches!(
        ship.raises[0].ids[0].1,
        ir::MarkSource::Row { .. }
    ));
    let dispatch: Vec<_> = domain
        .fns
        .iter()
        .filter(|f| f.action == "Dispatch")
        .collect();
    assert!(!dispatch.is_empty(), "{:?}", domain.skipped);
    for f in dispatch {
        assert_eq!(f.event_in.as_ref().unwrap().ty, "EvShipped");
        assert!(f.checks.iter().any(
            |c| matches!(c, ir::ArgCheck::EventRow { event_field, .. } if event_field == "order_id")
        ));
    }
}
