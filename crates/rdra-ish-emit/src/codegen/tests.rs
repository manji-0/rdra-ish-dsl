use super::*;
use rdra_ish_core::analysis::build_model;
use rdra_ish_syntax::parse;

/// What `check` explores at by default, less the memory pool.
const CAPS: Caps = Caps::for_instances(1);

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
    generated_from(SRC, "order.rdra", lang)
}

fn generated_from(src: &str, path: &str, lang: Lang) -> String {
    let model = model_from(src);
    let options = Options {
        lang,
        package: "domain".to_string(),
        model_path: path.to_string(),
        version: "x.y.z".to_string(),
        check: CheckStatus::Passed {
            bounds: "1 instance per entity, Int 0..5".to_string(),
            caps: CAPS,
        },
        conformance: None,
        seal: false,
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

/// Names that are keywords of a target: the CLI's `codegen_builds.rs` compiles this output.
const KEYWORDS_SRC: &str =
    include_str!("../../../rdra-ish-cli/tests/fixtures/codegen/keywords.rdra");

#[test]
fn keywords_typescript_snapshot() {
    insta::assert_snapshot!(generated_from(
        KEYWORDS_SRC,
        "keywords.rdra",
        Lang::TypeScript
    ));
}

#[test]
fn keywords_rust_snapshot() {
    insta::assert_snapshot!(generated_from(KEYWORDS_SRC, "keywords.rdra", Lang::Rust));
}

#[test]
fn keywords_go_snapshot() {
    insta::assert_snapshot!(generated_from(KEYWORDS_SRC, "keywords.rdra", Lang::Go));
}

/// A model whose every action is an entry point (G15): no function, a use-case error per
/// entry. The CLI's `codegen_builds.rs` compiles this output.
const ENTRY_ONLY_SRC: &str =
    include_str!("../../../rdra-ish-cli/tests/fixtures/codegen/entry_only.rdra");

#[test]
fn entry_only_typescript_snapshot() {
    insta::assert_snapshot!(generated_from(
        ENTRY_ONLY_SRC,
        "entry_only.rdra",
        Lang::TypeScript
    ));
}

#[test]
fn entry_only_rust_snapshot() {
    insta::assert_snapshot!(generated_from(
        ENTRY_ONLY_SRC,
        "entry_only.rdra",
        Lang::Rust
    ));
}

#[test]
fn entry_only_go_snapshot() {
    insta::assert_snapshot!(generated_from(ENTRY_ONLY_SRC, "entry_only.rdra", Lang::Go));
}

/// A name the Go printer derives from a model name steps aside, with a trailing `_`, for a
/// model type that has it already; the model's type keeps its name.
#[test]
fn go_derived_names_step_aside_for_model_types() {
    let src = r#"
entity Order "O" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  qty: Int @default("1")
}
usecase Place "P"
usecase Pay "Pay"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
condition Big "big"
holds(Big, Order.qty >= 1)
applies_to(Big, Pay)
variation OrderVisitor "V" { a b }
variation MatchOrder "M" { a b }
variation OrderKindPaid "K" { a b }
variation PayErrorKind "E" { a b }
variation PayErrorRejected "R" { a b }
"#;
    let go = generated_from(src, "names.rdra", Lang::Go);
    for line in [
        "type OrderVisitor string",
        "type OrderVisitor_[R any] interface {",
        "type MatchOrder string",
        "func MatchOrder_[R any](state Order, v OrderVisitor_[R]) (R, error) {",
        "type OrderKindPaid string",
        "\treturn OrderKindPaid_\n",
        "type PayErrorKind string",
        "type PayErrorKind_ string",
        "type PayErrorRejected string",
        "\tPayErrorRejected_ PayErrorKind_ = \"Rejected\"",
        "\tPayErrorNotEnabled PayErrorKind_ = \"NotEnabled\"",
    ] {
        assert!(go.contains(line), "no {line:?} in:\n{go}");
    }
    // The other languages derive none of these names.
    let rust = generated_from(src, "names.rdra", Lang::Rust);
    assert!(!rust.contains("OrderVisitor_"), "{rust}");
}

/// A key column takes its parent's id type, and the obligations name the invariants the
/// verification checks.
#[test]
fn keys_and_obligations() {
    let model = model_from(SRC);
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
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
        .filter(|o| o.checked)
        .map(|o| o.name.as_str())
        .collect();
    assert!(names.contains(&"Check_Order_total_0"), "{names:?}");
    assert!(names.contains(&"MinCard_OrderLine_Order"), "{names:?}");
}

/// A column `kind` of a state type is `kind_` in its states, since `kind` is the
/// discriminant, and keeps its name in the row; a record keeps `kind` as it is.
#[test]
fn a_kind_column_of_a_state_type_is_escaped_in_the_domain_only() {
    let model = model_from(&format!(
        "{KEYWORDS_SRC}\nentity Note \"N\" {{\n  id: Int @pk\n  kind: Kind\n}}\n"
    ));
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
    let matched = domain.sums.iter().find(|s| s.entity == "Match").unwrap();
    assert_eq!(matched.kind, "MatchKind");
    for v in &matched.variants {
        let kind = v.fields.iter().find(|f| f.wire == "kind").unwrap();
        assert_eq!(
            (kind.name.as_str(), kind.domain_pascal().as_str()),
            ("kind_", "Kind_")
        );
    }
    let row = matched
        .row_fields
        .iter()
        .find(|f| f.wire == "kind")
        .unwrap();
    assert_eq!(row.name, "kind");
    let note = domain.records.iter().find(|r| r.id == "Note").unwrap();
    assert!(note.fields.iter().any(|f| f.name == "kind"));
}

/// The JSON contract of `--format json`: types, states, use cases, functions, obligations.
#[test]
fn json_snapshot() {
    let model = model_from(SRC);
    let options = Options {
        lang: Lang::Rust,
        package: "domain".to_string(),
        model_path: "order.rdra".to_string(),
        version: "x.y.z".to_string(),
        check: CheckStatus::Passed {
            bounds: "1 instance per entity, Int 0..5".to_string(),
            caps: CAPS,
        },
        conformance: None,
        seal: false,
    };
    insta::assert_snapshot!(generate_json(&model, &options, &|_| None).expect("generates"));
}

/// Every obligation has a name of its own, built from model identifiers: a declaration added
/// to the model, ahead of the others and with keys and rules of its own, moves none of the
/// names the model had, nor what each names.
#[test]
fn obligation_names_do_not_move() {
    let names = |src: &str| -> Vec<(String, String)> {
        build(&model_from(src), Some(CAPS), &|_| None)
            .unwrap()
            .obligations
            .into_iter()
            .map(|o| (o.name, o.text))
            .collect()
    };
    let before = names(SRC);
    let mut seen = std::collections::BTreeSet::new();
    for (name, _) in &before {
        assert!(seen.insert(name.clone()), "{name} twice: {before:?}");
    }
    assert!(
        before.iter().any(|(n, _)| n == "Ref_OrderLine_order_id"),
        "{before:?}"
    );
    let added = format!(
        r#"
entity Archive "Archive" {{
  id: Int @pk
  code: String @unique
  total: Int @check(total >= 0)
}}
relate(Archive, Customer, N:1)
relate(OrderLine, Archive, N:1)
{SRC}"#
    );
    let after = names(&added);
    for (name, text) in &before {
        assert!(
            after.iter().any(|(n, t)| n == name && t == text),
            "{name} moved or changed: {after:?}"
        );
    }
    assert!(after.len() > before.len(), "{after:?}");
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
    let err = build(&model, Some(CAPS), &|_| None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("OrderStatus"), "{err}");
    assert!(err.contains("entity OrderStatus"), "{err}");
}

/// A name a printer derives takes `_` when a model type has it, and the printer reads which
/// from `domain.derived`.
#[test]
fn a_derived_name_falls_back_to_an_underscore() {
    let mut names = Names::default();
    names
        .claim(
            "OrderVisitor".to_string(),
            "entity OrderVisitor".to_string(),
        )
        .unwrap();
    let mut domain = DomainModel::default();
    let derived = |key: &str, name: &str| DerivedName {
        key: key.to_string(),
        name: name.to_string(),
        by: format!("the visitor {name}"),
    };
    claim_derived(
        &mut names,
        &mut domain,
        vec![
            derived("go:visitor:Order", "OrderVisitor"),
            derived("go:visitor:Line", "LineVisitor"),
        ],
    )
    .unwrap();
    assert_eq!(domain.derived("go:visitor:Order"), Some("OrderVisitor_"));
    assert_eq!(domain.derived("go:visitor:Line"), Some("LineVisitor"));
    names
        .claim("Taken_".to_string(), "entity Taken_".to_string())
        .unwrap();
    names
        .claim("Taken".to_string(), "entity Taken".to_string())
        .unwrap();
    let err = claim_derived(&mut names, &mut domain, vec![derived("k", "Taken")])
        .unwrap_err()
        .to_string();
    assert!(err.contains("Taken_"), "{err}");
}

/// The port and its contracts are built whenever `check` passed: one contract per use case,
/// named `<UseCase>UseCase`, in the order of the port's methods, and the Rust output prints the contracts without the harness.
#[test]
fn contracts_are_built_and_printed_without_the_harness() {
    let model = model_from(SRC);
    let domain = build_with(&model, Some(CAPS), false, &|_| None).unwrap();
    assert!(domain.harness().is_none());
    let port = domain.port.as_ref().expect("a port");
    let contracts: Vec<(&str, &str, &str)> = port
        .contracts
        .iter()
        .map(|c| (c.name.as_str(), c.usecase.as_str(), c.error.as_str()))
        .collect();
    assert_eq!(
        contracts,
        [
            ("PayUseCase", "Pay", "PayError"),
            ("CancelOrderUseCase", "CancelOrder", "CancelOrderError"),
            ("PlaceUseCase", "Place", "PlaceError"),
        ]
    );
    for c in &port.contracts {
        assert!(c
            .methods
            .iter()
            .all(|&m| port.methods[m].usecase == c.usecase));
    }
    let rust = generated(Lang::Rust);
    assert!(rust.contains("pub trait PayUseCase {"));
    assert!(!rust.contains("trait ConformancePort"));
    let go = generated(Lang::Go);
    assert!(!go.contains("type ConformancePort"));
    // The unimplemented port comes with the contracts, without the harness's Seed and Snapshot.
    assert!(rust.contains("pub trait UnimplementedPayUseCase {}"));
    assert!(rust.contains("impl<T: UnimplementedPayUseCase> PayUseCase for T {"));
    assert!(go.contains("type UnimplementedConformancePort struct{}"));
    assert!(!go.contains("func (UnimplementedConformancePort) Seed("));
    let ts = generated(Lang::TypeScript);
    assert!(ts.contains("export class UnimplementedConformancePort {"));
    assert!(!ts.contains("  seed = "));
}

/// The declared refinements of the sample: `paid_at` present in `paid` by the invariant,
/// null in `pending` by the forbidden rule, and nullable in `cancelled`, where no rule says.
#[test]
fn rules_refine_the_states_they_name() {
    let model = model_from(SRC);
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
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
    let unrefined = build(&model, None, &|_| None).unwrap();
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
    build(&model, Some(CAPS), &|_| None).unwrap()
}

/// A logical `@derived` column is computed from what the function writes: folded from a
/// literal, read from the argument row it copies the operand from, and left an input, with
/// the reason in the doc, where the function cannot compute it.
#[test]
fn derived_columns_are_computed_or_left_an_input_with_the_reason() {
    let domain = functions_of(
        r#"
entity Item "Item" {
  id: Int @pk
  status: Enum(open, done) @default(open)
  qty: Int @default(0)
  bulk: Bool @derived(qty >= 3)
  closed: Bool @derived(status == done)
  total: Int @derived(qty * 2)
}
usecase Make "Make"
usecase Finish "Finish"
usecase Grow "Grow"
creates(Make, Item)
transitions(Finish, Item.status, open -> done)
sets(Grow, Item.qty == 4)
"#,
    );
    let field = |f: &str, wire: &str| {
        let fd = domain.fns.iter().find(|x| x.name == f).unwrap();
        let ir::OutKind::Row { fields, .. } = &fd.outputs[0].kind else {
            panic!("{f} writes a row");
        };
        (
            fd,
            fields
                .iter()
                .find(|(t, _)| t.wire == wire)
                .unwrap()
                .1
                .clone(),
        )
    };
    let lit = |b: bool| move |a: &ir::Assigned| matches!(a, ir::Assigned::Lit { lit: ir::Lit::Bool(x), .. } if *x == b);
    // Make writes qty 0 and status open: both fold.
    let (make, bulk) = field("Make", "bulk");
    assert!(lit(false)(&bulk), "{bulk:?}");
    assert!(lit(false)(&field("Make", "closed").1));
    assert!(
        !make.inputs.iter().any(|i| i.name == "item_bulk"),
        "{:?}",
        make.inputs
    );
    // Finish copies qty from the row it reads, and moves status to done.
    let (_, bulk) = field("Finish", "bulk");
    assert!(
        matches!(
            &bulk,
            ir::Assigned::Derived {
                expr: ir::Expr::IntCmp {
                    op: ir::CmpOp::Ge,
                    value: 3,
                    ..
                },
                ..
            }
        ),
        "{bulk:?}"
    );
    assert!(lit(true)(&field("Finish", "closed").1));
    // Grow writes qty 4.
    let (grow, bulk) = field("GrowFromOpen", "bulk");
    assert!(lit(true)(&bulk), "{bulk:?}");
    // An arithmetic expression is an input, and the doc says why.
    let (_, total) = field("GrowFromOpen", "total");
    assert!(matches!(total, ir::Assigned::Input { .. }), "{total:?}");
    assert!(
        grow.doc
            .iter()
            .any(|l| l.contains("`item_total` is an input") && l.contains("arithmetic")),
        "{:?}",
        grow.doc
    );
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
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
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
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
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

/// A counted event (`triggers(..).counted(k)`, #220) is a runtime queue, not a fact the
/// generator prints: its raiser and its handler are skipped with that reason, and no event
/// type is half printed for it.
#[test]
fn a_counted_event_skips_its_raiser_and_handler() {
    let src = r#"
entity Order "O" {
  id: Int @pk
  status: Enum(open, shipped) @default(open)
}
usecase Place "Place"
usecase Ship "Ship"
usecase Reopen "Reopen"
event EvShipped "shipped"
creates(Place, Order)
transitions(Ship, Order.status, open -> shipped)
raises(Ship, EvShipped)
triggers(EvShipped, Reopen).counted(2)
transitions(Reopen, Order.status, shipped -> open)
"#;
    let domain = functions_of(src);
    for action in ["Ship", "Reopen"] {
        assert!(
            !domain.fns.iter().any(|f| f.action == action),
            "{action} gets no function"
        );
        let skipped = domain
            .skipped
            .iter()
            .find(|s| s.action == action)
            .unwrap_or_else(|| panic!("{action} is skipped: {:?}", domain.skipped));
        assert!(skipped.reason.contains("counted"), "{}", skipped.reason);
    }
    assert!(
        domain
            .events
            .iter()
            .all(|e| !e.fields.is_empty() || e.name != "EvShipped"),
        "no empty EvShipped type"
    );
    assert!(domain.fns.iter().any(|f| f.action == "Place"));
}

/// A choice among arms from one state is one function per target, each guarded by its
/// arm's `.when`; a second lifecycle column is a branch on its value.
#[test]
fn choices_whens_and_second_columns() {
    let src = r#"
entity Case "C" {
  id: Int @pk
  status: Enum(open, granted, refused) @default(open)
  publication: Enum(hidden, shown) @default(hidden)
  urgent: Bool @default(false)
}
usecase Open "Open"
usecase Decide "Decide"
usecase Publish "Publish"
creates(Open, Case)
condition Urgent "urgent"
holds(Urgent, Case.urgent == true)
transitions(Decide, Case.status, open -> granted).when(Urgent)
transitions(Decide, Case.status, open -> refused)
transitions(Publish, Case.publication, hidden -> shown)
"#;
    let model = model_from(src);
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
    let names: Vec<&str> = domain.fns.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"DecideToGranted"), "{names:?}");
    assert!(names.contains(&"DecideToRefused"), "{names:?}");
    let granted = domain
        .fns
        .iter()
        .find(|f| f.name == "DecideToGranted")
        .unwrap();
    assert_eq!(granted.conditions.len(), 1);
    assert_eq!(granted.conditions[0].id, "Urgent");
    let refused = domain
        .fns
        .iter()
        .find(|f| f.name == "DecideToRefused")
        .unwrap();
    assert!(refused.conditions.is_empty());
    let run = domain
        .dispatchers
        .iter()
        .find(|d| d.name == "RunDecide")
        .unwrap();
    assert_eq!(run.choices.len(), 1, "the dispatcher takes the target");

    let publish: Vec<_> = domain
        .fns
        .iter()
        .filter(|f| f.action == "Publish")
        .collect();
    assert!(!publish.is_empty(), "{:?}", domain.skipped);
    for f in publish {
        let ir::OutKind::Row { fields, .. } = &f.outputs[0].kind else {
            panic!("Publish writes a row");
        };
        let column = fields
            .iter()
            .find(|(t, _)| t.wire == "publication")
            .unwrap();
        assert!(
            matches!(column.1, ir::Assigned::Switch { .. }),
            "{column:?}"
        );
        assert!(f.conditions.iter().any(|c| c.id == "TransitionFrom"));
    }
}

/// A `.none` condition ranges over a list of the related entity's rows, an input, and keeps
/// those the key links.
#[test]
fn a_none_condition_ranges_over_a_list_of_related_rows() {
    let src = r#"
entity Cert "Cert" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity Assignment "A" {
  id: Int @pk
  status: Enum(active, ended) @default(active)
}
relate(Assignment, Cert, N:1)
usecase Issue "Issue"
usecase Assign "Assign"
usecase End "End"
usecase Revoke "Revoke"
creates(Issue, Cert)
creates(Assign, Assignment)
transitions(End, Assignment.status, active -> ended)
transitions(Revoke, Cert.status, active -> revoked)
condition NoActive "no active assignment"
holds(NoActive, Cert.status == active).none(Assignment.status == active)
applies_to(NoActive, Revoke)
"#;
    let model = model_from(src);
    let domain = build(&model, Some(CAPS), &|_| None).unwrap();
    let revoke = domain.fns.iter().find(|f| f.action == "Revoke").unwrap();
    let list = revoke
        .inputs
        .iter()
        .position(|i| i.ty == ir::InputType::Rows("AssignmentRow".to_string()))
        .expect("the related rows are an input");
    assert_eq!(revoke.elems.len(), 1);
    let ir::Expr::Forall { input, body, .. } = &revoke.conditions[0].expr else {
        panic!("`.none` is a Forall: {:?}", revoke.conditions[0].expr);
    };
    assert_eq!(*input, list);
    let text = format!("{body:?}");
    assert!(text.contains("KeyEq"), "{text}");
}

/// A delete the key's children reject keeps its function, with the guard as the condition
/// `Restrict_<Child>_<role>` over the child rows; a line the step deletes with the Order is
/// told apart by its primary key (#199).
#[test]
fn a_restricted_delete_rejects_while_a_child_names_the_row() {
    let src = |extra: &str| {
        format!(
            r#"
entity Order "Order" {{ id: Int @pk }}
entity OrderLine "Line" {{ id: Int @pk }}
relate(OrderLine, Order, N:1)
usecase Open "Open"
usecase AddLine "Add line"
usecase Cancel "Cancel"
creates(Open, Order)
creates(AddLine, OrderLine)
deletes(Cancel, Order)
{extra}
"#
        )
    };
    for (extra, same_row) in [("", false), ("deletes(Cancel, OrderLine)", true)] {
        let domain = functions_of(&src(extra));
        let cancel = domain
            .fns
            .iter()
            .find(|f| f.action == "Cancel")
            .unwrap_or_else(|| panic!("{extra}: Cancel has a function: {:?}", domain.skipped));
        let [cond] = cancel.conditions.as_slice() else {
            panic!("{extra}: one condition: {:?}", cancel.conditions);
        };
        assert_eq!(cond.id, "Restrict_OrderLine_Order");
        let ir::Expr::Forall { body, .. } = &cond.expr else {
            panic!("{extra}: a Forall over the lines: {:?}", cond.expr);
        };
        let keys = format!("{body:?}").matches("KeyEq").count();
        assert_eq!(keys, if same_row { 2 } else { 1 }, "{extra}: {body:?}");
    }
}

/// A TypeScript name the printer derives (`parse<Row>`) that a model name already takes gets a
/// trailing `_`: here the function of the use case `ParseOrderRow` keeps `parseOrderRow`.
#[test]
fn a_derived_typescript_name_yields_to_a_model_name() {
    let src = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, done) @default(open)
}
usecase Open "Open"
usecase ParseOrderRow "Parse"
creates(Open, Order)
transitions(ParseOrderRow, Order.status, open -> done)
"#;
    let ts = generated_from(src, "derived.rdra", Lang::TypeScript);
    assert!(
        ts.contains("export function parseOrderRow_(value: unknown): Result<OrderRow, InvalidRow>"),
        "{ts}"
    );
    assert!(
        ts.contains("export function parseOrderRow(order: OrderOpen)"),
        "{ts}"
    );
}

/// The use case vocabulary #335 prints: a created row's optional key left null, a `.when`
/// on a chosen value read from the input, a `links` as the condition `Links_<Child>_<role>`,
/// a copy from the parent row, and a condition on the key of the created row, which folds
/// to the parent the step links it to.
const VOCAB_SRC: &str = r#"
variation Scope "Scope" {
  free
  limited
}
entity Owner "Owner" {
  id: Int @pk
  status: Enum(active, closed) @default(active)
}
entity Case "Case" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  first: Bool @default(true)
  scope: Scope @default(limited)
}
entity Note "Note" {
  id: Int @pk
  flag: Bool @default(false)
}
relate(Case, Owner, N:1).optional()
relate(Note, Case, N:1)
usecase AddOwner "Add owner"
usecase Open "Open"
usecase Draft "Draft"
usecase Claim "Claim"
usecase Annotate "Annotate"
creates(AddOwner, Owner)
creates(Open, Case)
transitions(Draft, Case.status, open -> drafted)
condition FirstOnly "first notice"
holds(FirstOnly, Case.first == true)
chooses(Draft, Case.scope, free).when(FirstOnly)
chooses(Draft, Case.scope, limited)
updates(Claim, Case)
links(Claim, Case, Owner)
creates(Annotate, Note)
sets(Annotate, Note.flag == Case.first)
condition OpenCase "open case"
holds(OpenCase, Case.status == open).along(Note, Case)
applies_to(OpenCase, Annotate)
"#;

#[test]
fn the_rest_of_the_use_case_vocabulary_is_printed() {
    let domain = functions_of(VOCAB_SRC);
    assert!(domain.skipped.is_empty(), "{:?}", domain.skipped);
    let f = |name: &str| {
        domain
            .fns
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name}: {:?}", domain.fns.iter().map(|f| &f.name)))
    };
    let field = |f: &ir::FnDecl, wire: &str| {
        let ir::OutKind::Row { fields, .. } = &f.outputs[0].kind else {
            panic!("{} writes a row", f.name);
        };
        fields
            .iter()
            .find(|(t, _)| t.wire == wire)
            .unwrap()
            .1
            .clone()
    };
    // Open leaves the optional key null.
    assert!(matches!(field(f("Open"), "owner_id"), ir::Assigned::Null));
    // Draft reads the chosen scope from its input.
    let draft = f("Draft");
    assert_eq!(draft.conditions[0].id, "FirstOnly");
    let text = format!("{:?}", draft.conditions[0].expr);
    assert!(text.contains("InputEnumEq"), "{text}");
    // Claim links the case: the key must be null, and becomes the owner's.
    let claim = f("ClaimFromOpen");
    assert_eq!(claim.conditions[0].id, "Links_Case_Owner");
    assert!(
        matches!(&claim.conditions[0].expr, ir::Expr::IsNull { .. }),
        "{:?}",
        claim.conditions[0].expr
    );
    assert!(matches!(
        field(claim, "owner_id"),
        ir::Assigned::ParentId { .. }
    ));
    // Annotate copies the parent's flag, and runs only under an open case: the condition on
    // the created note's case folds to the parent row's state.
    let names: Vec<&str> = domain
        .fns
        .iter()
        .filter(|f| f.action == "Annotate")
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(names, ["AnnotateFromOpen"]);
    let annotate = f("AnnotateFromOpen");
    assert!(annotate.conditions.is_empty(), "{:?}", annotate.conditions);
    assert!(
        matches!(field(annotate, "flag"), ir::Assigned::From { ref field, .. } if field.wire == "first"),
        "{:?}",
        field(annotate, "flag")
    );

    let ts = generated_from(VOCAB_SRC, "vocab.rdra", Lang::TypeScript);
    for line in [
        r#"if (!(((caseScope === "free") && (case_.first === true)) || (caseScope === "limited"))) {"#,
        "if (!(case_.owner_id === null)) {",
        "owner_id: parentOwner.id,",
        "flag: parentCase.first,",
        "owner_id: null,",
    ] {
        assert!(ts.contains(line), "{line}\n{ts}");
    }
    let rust = generated_from(VOCAB_SRC, "vocab.rdra", Lang::Rust);
    for line in [
        "if !(((case_scope == Scope::Free) && (case.first)) || (case_scope == Scope::Limited)) {",
        "if !(case.owner_id.is_none()) {",
        "flag: parent_case.first,",
    ] {
        assert!(rust.contains(line), "{line}\n{rust}");
    }
    let go = generated_from(VOCAB_SRC, "vocab.rdra", Lang::Go);
    for line in [
        "if !(((caseScope == ScopeFree) && (case_.First())) || (caseScope == ScopeLimited)) {",
        "if !(case_.OwnerId() == nil) {",
        "ownerId: ptrTo(parentOwner.Id)",
    ] {
        assert!(go.contains(line), "{line}\n{go}");
    }
}

/// The two use cases #335 left to a choice: a later step of a split use case takes the rows
/// an earlier step wrote as they are now, beside their marks, and checks the key between them
/// (Visit); a choice among arms of a second lifecycle column is one function per target, and
/// the dispatcher takes the target (Judge).
const EARLIER_ROWS_SRC: &str = r#"
entity Desk "Desk" {
  id: Int @pk
  status: Enum(idle, busy) @default(idle)
}
entity Counter "Counter" {
  id: Int @pk
}
entity Ticket "Ticket" {
  id: Int @pk
  status: Enum(waiting, called) @default(waiting)
  verdict: Enum(pending, granted, refused) @default(pending)
}
entity Stamp "Stamp" {
  id: Int @pk
}
relate(Ticket, Desk, N:1)
relate(Counter, Desk, N:1)
relate(Stamp, Counter, N:1)
usecase AddDesk "Add desk"
usecase AddCounter "Add counter"
usecase AddTicket "Add ticket"
usecase Visit "Visit"
usecase Judge "Judge"
api VisitApi "Visit API"
api StampApi "Stamp API"
creates(AddDesk, Desk)
creates(AddCounter, Counter)
creates(AddTicket, Ticket)
invokes(Visit, VisitApi)
invokes(Visit, StampApi)
updates(VisitApi, Ticket)
updates(VisitApi, Desk)
creates(StampApi, Stamp)
reads(StampApi, Counter)
transitions(Visit, Ticket.status, waiting -> called)
transitions(Visit, Desk.status, idle -> busy)
transitions(Judge, Ticket.verdict, pending -> granted)
transitions(Judge, Ticket.verdict, pending -> refused)
"#;

#[test]
fn earlier_rows_and_a_second_column_choice_are_printed() {
    let domain = functions_of(EARLIER_ROWS_SRC);
    assert!(domain.skipped.is_empty(), "{:?}", domain.skipped);
    let f = |name: &str| {
        domain
            .fns
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name}: {:?}", domain.fns.iter().map(|f| &f.name)))
    };
    // Visit_2 takes the desk and the ticket beside their marks, checks each is the row its
    // mark names, and rejects a ticket no longer on the desk.
    let visit = f("Visit2");
    let rows: Vec<&str> = visit.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(rows, ["desk", "ticket", "parent_counter"]);
    assert_eq!(visit.marks_in.len(), 2);
    let marked = visit
        .checks
        .iter()
        .filter(|c| matches!(c, ir::ArgCheck::MarkRow { .. }))
        .count();
    assert_eq!(marked, 2, "{:?}", visit.checks);
    let ids: Vec<&str> = visit.conditions.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["Key_Ticket_Desk"]);
    // Judge: one function per source state and target of `verdict`.
    let names: Vec<&str> = domain
        .fns
        .iter()
        .filter(|f| f.action == "Judge")
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "JudgeFromWaitingToGranted",
            "JudgeFromWaitingToRefused",
            "JudgeFromCalledToGranted",
            "JudgeFromCalledToRefused"
        ]
    );
    let judge = f("JudgeFromCalledToRefused");
    assert_eq!(judge.chosen.len(), 1);
    assert_eq!(judge.chosen[0].column.as_deref(), Some("verdict"));
    assert_eq!(judge.chosen[0].value.wire, "refused");
    let run = domain
        .dispatchers
        .iter()
        .find(|d| d.name == "RunJudge")
        .unwrap();
    assert_eq!(
        run.choices,
        [("ticket_verdict_to".to_string(), "TicketVerdict".to_string())]
    );

    let ts = generated_from(EARLIER_ROWS_SRC, "desk.rdra", Lang::TypeScript);
    for line in [
        "export function visit2(desk: DeskRow, ticket: TicketRow, parentCounter: Counter, deskMark: VisitDeskMark1, ticketMark: VisitTicketMark1, stampId: StampId)",
        "if (ticket.id !== ticketMark.id) {",
        "if (!(ticket.desk_id === desk.id)) {",
        r#"verdict: ("refused"),"#,
        r#"if (ticket.kind === "Called" && ticketVerdictTo === "refused") {"#,
    ] {
        assert!(ts.contains(line), "{line}\n{ts}");
    }
    let rust = generated_from(EARLIER_ROWS_SRC, "desk.rdra", Lang::Rust);
    for line in [
        "if ticket.id != ticket_mark.id {",
        "if !(ticket.desk_id == desk.id) {",
        "(Ticket::Called(ticket),) if ticket_verdict_to == TicketVerdict::Refused => judge_from_called_to_refused(ticket)",
    ] {
        assert!(rust.contains(line), "{line}\n{rust}");
    }
    let go = generated_from(EARLIER_ROWS_SRC, "desk.rdra", Lang::Go);
    for line in [
        "if ticket.Id != ticketMark.Id {",
        "if !(ticket.DeskId == desk.Id) {",
        "if ticketVerdictTo == TicketVerdictRefused {",
    ] {
        assert!(go.contains(line), "{line}\n{go}");
    }
}

/// A nullable Enum or Bool column compared with a value (#359): null satisfies no
/// comparison, so `==` takes a present value and `!=` admits null.
#[test]
fn a_comparison_on_a_nullable_enum_or_bool_column_is_printed() {
    let src = r#"
entity Item "Item" {
  id: Int @pk
  k: Enum(red, blue) @null
  f: Bool @null
}
usecase Make "Make"
usecase Paint "Paint"
usecase Clear "Clear"
creates(Make, Item)
sets(Paint, Item.k == red)
sets(Clear, Item.k == null)
condition IsRed "red"
holds(IsRed, Item.k == red)
applies_to(IsRed, Clear)
condition NotOff "not off"
holds(NotOff, Item.f != false)
applies_to(NotOff, Clear)
"#;
    let domain = functions_of(src);
    assert!(domain.skipped.is_empty(), "{:?}", domain.skipped);
    let ts = generated_from(src, "item.rdra", Lang::TypeScript);
    assert!(ts.contains(r#"if (!(item.k === "red")) {"#), "{ts}");
    assert!(ts.contains("if (!(!(item.f === false))) {"), "{ts}");
    let rust = generated_from(src, "item.rdra", Lang::Rust);
    assert!(
        rust.contains("if !(item.k == ::core::option::Option::Some(ItemK::Red)) {"),
        "{rust}"
    );
    assert!(
        rust.contains("if item.f == ::core::option::Option::Some(false) {"),
        "{rust}"
    );
    let go = generated_from(src, "item.rdra", Lang::Go);
    assert!(
        go.contains("if !(item.K != nil && *item.K == ItemKRed) {"),
        "{go}"
    );
    assert!(go.contains("if item.F != nil && !*item.F {"), "{go}");
}

/// G5 (round-2 audit): every target a function is named for (`To<Target>`) is in its
/// `chosen`, and no two cases of a dispatcher take the same states and choices, over every
/// `samples.txt` root and codegen fixture. A target whose lookup failed would be dropped from
/// `chosen` silently, and the dispatcher could not tell the function from its sibling.
#[test]
fn every_choice_a_function_is_named_for_is_chosen() {
    use std::path::{Path, PathBuf};
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut roots: Vec<PathBuf> = std::fs::read_to_string(repo.join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| repo.join(l))
        .collect();
    let mut dirs = vec![repo.join("crates/rdra-ish-cli/tests/fixtures/codegen")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("fixture dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rdra") {
                roots.push(path);
            }
        }
    }
    let (mut named, mut cases) = (0, 0);
    for root in &roots {
        let Ok(project) = rdra_ish_core::load_project(std::slice::from_ref(root), None) else {
            continue;
        };
        let Ok(domain) = build(&project.model, Some(CAPS), &|_| None) else {
            continue;
        };
        for f in domain.fns.iter().filter(|f| !f.chosen.is_empty()) {
            named += 1;
            let targets: Vec<String> = f.chosen.iter().map(|c| type_ident(&c.value.wire)).collect();
            let suffix = format!("To{}", targets.join("And"));
            assert!(
                f.name.ends_with(&suffix),
                "{}: {} is named for a target its chosen {targets:?} lacks",
                root.display(),
                f.name
            );
        }
        let key = |c: &ir::DispatchCase| {
            (
                c.states
                    .iter()
                    .map(|s| s.as_ref().map(|(_, _, v)| v.wire.clone()))
                    .collect::<Vec<_>>(),
                c.choice
                    .iter()
                    .map(|(i, v)| (*i, v.wire.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        for d in &domain.dispatchers {
            for (i, a) in d.cases.iter().enumerate() {
                cases += 1;
                for b in &d.cases[i + 1..] {
                    assert_ne!(
                        key(a),
                        key(b),
                        "{}: {} cannot tell {} from {}",
                        root.display(),
                        d.name,
                        a.function,
                        b.function
                    );
                }
            }
        }
    }
    assert!(named > 0 && cases > 0, "no function chose a target");
}

/// The Go output passes staticcheck's default checks on what the printer chooses: a Bool test
/// is `x` or `!x`, never a comparison with a constant (S1002) or `!(!x)` (SA4013), and a
/// generic helper is printed only where something calls it (U1000).
#[test]
fn go_bool_tests_and_helpers_pass_staticcheck() {
    let nullable = r#"
entity Item "Item" {
  id: Int @pk
  f: Bool @null
  g: Bool
}
usecase Make "Make"
usecase Clear "Clear"
creates(Make, Item)
condition NotOff "not off"
holds(NotOff, Item.f != false)
applies_to(NotOff, Clear)
condition GOff "g off"
holds(GOff, Item.g == false)
applies_to(GOff, Clear)
sets(Clear, Item.g == true)
"#;
    for (src, path) in [
        (SRC, "order.rdra"),
        (VOCAB_SRC, "vocab.rdra"),
        (nullable, "item.rdra"),
    ] {
        let go = generated_from(src, path, Lang::Go);
        for bad in ["== true", "== false", "!= true", "!= false", "!(!"] {
            assert!(!go.contains(bad), "{path}: {bad}\n{go}");
        }
        for helper in ["ptrTo", "copyOf", "anyOf", "allOf", "countOf"] {
            if go.contains(&format!("func {helper}[")) {
                assert!(
                    go.matches(&format!("{helper}(")).count() > 0,
                    "{path}: {helper} is printed and not called"
                );
            }
        }
    }
    assert!(generated_from(nullable, "item.rdra", Lang::Go).contains("if item.G {"));
}

/// Both Go probes list their last import group as `gofmt` sorts it, by path, whichever of the
/// service's and the domain's import paths sorts first.
#[test]
fn go_probe_imports_are_sorted_by_path() {
    let model = model_from(SRC);
    let domain = build_with(&model, Some(CAPS), true, &|_| None).unwrap();
    let group = |text: &str| -> Vec<String> {
        let block = text
            .split_once("import (\n")
            .and_then(|(_, rest)| rest.split_once("\n)\n"))
            .expect("an import block")
            .0;
        let last = block.rsplit("\n\n").next().unwrap();
        last.lines()
            .map(|l| l.rsplit('"').nth(1).expect("a quoted path").to_string())
            .collect()
    };
    for service in [
        "example.com/clinic/clinic.Service",
        "example.com/clinic/svc.Service",
    ] {
        let binding = ProbeBinding {
            service: service.to_string(),
            module: "example.com/clinic/domain".to_string(),
            conformance: Some(("svc.New()".to_string(), "traces.txt".to_string())),
            package: None,
        };
        let contract = go_port::probe(&domain, &binding).expect("a contract probe");
        let adapter = adapter::probe(Lang::Go, &domain, &binding).expect("an adapter probe");
        for text in [&contract.text, &adapter.text] {
            let paths = group(text);
            assert_eq!(paths.len(), 2, "{text}");
            let mut sorted = paths.clone();
            sorted.sort();
            assert_eq!(paths, sorted, "{text}");
        }
    }
}
