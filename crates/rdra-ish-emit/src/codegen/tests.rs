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
    assert!(!domain.harness);
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
    assert!(!rust.contains("ConformancePort"));
    let go = generated(Lang::Go);
    assert!(!go.contains("ConformancePort"));
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
        .position(|i| i.ty == ir::FieldType::Rows("AssignmentRow".to_string()))
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
