use super::*;

fn parse_ok(src: &str) -> Ast {
    let (ast, errors) = parse(src);
    if !errors.is_empty() {
        panic!("parse errors: {:?}", errors);
    }
    ast
}

#[test]
fn test_parse_instance_decl() {
    let ast = parse_ok(r#"actor Customer "顧客" description "商品を購入する顧客""#);
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_requirement_metadata() {
    let ast = parse_ok(
        r#"requirement ReqCheckout "Checkout must be reliable"
description "The checkout flow must preserve customer intent."
priority "must"
source "Customer interview"
stakeholder "Store Operations"
owner "Product Owner"
acceptance_criteria "A payment timeout leaves the cart recoverable."
acceptance_criteria "A submitted order receives a stable order number."
status "proposed"
risk "high"
rationale "Checkout failures directly block revenue.""#,
    );

    let Item::Instance(inst) = &ast.items[0] else {
        panic!("expected instance declaration");
    };
    assert_eq!(inst.kind, Kind::Requirement);
    assert_eq!(inst.requirement.priority.as_deref(), Some("must"));
    assert_eq!(inst.requirement.sources, vec!["Customer interview"]);
    assert_eq!(inst.requirement.stakeholders, vec!["Store Operations"]);
    assert_eq!(inst.requirement.owner.as_deref(), Some("Product Owner"));
    assert_eq!(
        inst.requirement.acceptance_criteria,
        vec![
            "A payment timeout leaves the cart recoverable.",
            "A submitted order receives a stable order number."
        ]
    );
    assert_eq!(inst.requirement.status.as_deref(), Some("proposed"));
    assert_eq!(inst.requirement.risk.as_deref(), Some("high"));
    assert_eq!(
        inst.requirement.rationale.as_deref(),
        Some("Checkout failures directly block revenue.")
    );
}

#[test]
fn test_parse_adr_metadata() {
    let ast = parse_ok(
        r#"adr AdrOutbox "Use transactional outbox"
adr_status accepted
context "External subscribers need customer changes."
decision "Publish customer changes through a transactional outbox."
consequence "Delivery becomes eventually consistent."
accepted "Transactional outbox"
rejected "Synchronous callback"
reason "Avoid coupling write latency to external subscribers.""#,
    );

    let Item::Instance(inst) = &ast.items[0] else {
        panic!("expected instance declaration");
    };
    assert_eq!(inst.kind, Kind::Adr);
    assert_eq!(inst.adr.status.as_deref(), Some("accepted"));
    assert_eq!(
        inst.adr.context,
        vec!["External subscribers need customer changes."]
    );
    assert_eq!(
        inst.adr.decision.as_deref(),
        Some("Publish customer changes through a transactional outbox.")
    );
    assert_eq!(
        inst.adr.consequences,
        vec!["Delivery becomes eventually consistent."]
    );
    assert_eq!(inst.adr.accepted_options, vec!["Transactional outbox"]);
    assert_eq!(inst.adr.rejected_options, vec!["Synchronous callback"]);
    assert_eq!(
        inst.adr.reasons,
        vec!["Avoid coupling write latency to external subscribers."]
    );
}

#[test]
fn test_parse_usecase_metadata() {
    let ast = parse_ok(
        r#"usecase CapturePayment "Capture payment"
precondition "Order is authorized."
guard "Provider is available."
postcondition "Payment is captured."
alternative_flow "Customer changes payment method."
business_error "Authorization expires.""#,
    );

    let Item::Instance(uc) = &ast.items[0] else {
        panic!("expected usecase instance");
    };
    assert_eq!(uc.kind, Kind::UseCase);
    assert_eq!(uc.usecase.preconditions, vec!["Order is authorized."]);
    assert_eq!(uc.usecase.guards, vec!["Provider is available."]);
    assert_eq!(uc.usecase.postconditions, vec!["Payment is captured."]);
    assert_eq!(
        uc.usecase.alternatives,
        vec!["Customer changes payment method."]
    );
    assert_eq!(uc.usecase.errors, vec!["Authorization expires."]);
}

#[test]
fn test_parse_business_flow_elements() {
    let ast = parse_ok(
        r#"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
step AuthorizePayment "Authorize payment"
contains(CheckoutFlow, ReviewCart)
precedes(ReviewCart, AuthorizePayment)
branches(ReviewCart, AuthorizePayment)
excepts(AuthorizePayment, ReviewCart)
repeats(ReviewCart, ReviewCart)
covers(AuthorizePayment, usecase::CapturePayment)
"#,
    );

    assert_eq!(ast.items.len(), 9);
    let Item::Instance(flow) = &ast.items[0] else {
        panic!("expected flow instance");
    };
    assert_eq!(flow.kind, Kind::Flow);
    let Item::Instance(step) = &ast.items[1] else {
        panic!("expected step instance");
    };
    assert_eq!(step.kind, Kind::Step);
}

#[test]
fn test_parse_api_contract_elements() {
    let ast = parse_ok(
        r#"
api CreateOrder "Create order"
  method POST
  path "/orders"
  idempotency "idempotent"
  mode sync
  auth bearer
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
dto OrderResponse "Order response" {
  order_id: Int
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
"#,
    );

    let Item::Instance(api) = &ast.items[0] else {
        panic!("expected api instance");
    };
    assert_eq!(api.kind, Kind::Api);
    assert_eq!(api.api.method.as_deref(), Some("POST"));
    assert_eq!(api.api.path.as_deref(), Some("/orders"));
    assert_eq!(api.api.idempotency.as_deref(), Some("idempotent"));
    assert_eq!(api.api.mode.as_deref(), Some("sync"));
    assert_eq!(api.api.auth_scheme.as_deref(), Some("bearer"));

    let Item::Instance(dto) = &ast.items[1] else {
        panic!("expected dto instance");
    };
    assert_eq!(dto.kind, Kind::Dto);
    assert_eq!(dto.columns.len(), 2);
}

#[test]
fn test_parse_goal_with_its_measure() {
    let ast = parse_ok(
        r#"
goal ShortWait "受付から診察までの待ち時間を短くする"
  description "待ちが長いと患者が帰ってしまう。"
  metric "中央値"
  baseline "45分"
  target "30分"
  window "月次"
goal Bare "Bare"
"#,
    );
    let Item::Instance(goal) = &ast.items[0] else {
        panic!("expected goal instance");
    };
    assert_eq!(goal.kind, Kind::Goal);
    assert_eq!(
        goal.description.as_deref(),
        Some("待ちが長いと患者が帰ってしまう。")
    );
    assert_eq!(goal.nfr.metric.as_deref(), Some("中央値"));
    assert_eq!(goal.nfr.baseline.as_deref(), Some("45分"));
    assert_eq!(goal.nfr.target.as_deref(), Some("30分"));
    assert_eq!(goal.nfr.window.as_deref(), Some("月次"));
    let Item::Instance(bare) = &ast.items[1] else {
        panic!("expected goal instance");
    };
    assert_eq!(bare.nfr, NfrMetadata::default());
}

#[test]
fn a_goal_takes_no_slo_and_only_a_goal_takes_a_baseline() {
    let (_, errors) = parse("goal G \"g\"\n  slo \"99%\"\n");
    assert!(!errors.is_empty(), "a goal with slo must not parse");
    let (_, errors) = parse("nfr N \"n\"\n  baseline \"1\"\n");
    assert!(!errors.is_empty(), "an nfr with baseline must not parse");
}

#[test]
fn a_column_may_be_named_goal() {
    let ast = parse_ok("entity CarePlan \"c\" {\n  id: Int @pk\n  goal: String\n}\n");
    let Item::Instance(entity) = &ast.items[0] else {
        panic!("expected entity");
    };
    assert_eq!(entity.columns[1].name, "goal");
}

#[test]
fn a_kind_keyword_is_a_variation_or_enum_value() {
    let ast = parse_ok(
        "variation Method \"m\" {\n  site_visit \"s\"\n  field \"f\"\n}\n\
         entity E \"e\" {\n  id: Int @pk\n  s: Enum(a, field, screen)\n}\n\
         forbidden(E, E.s == field)\n",
    );
    let Item::Instance(method) = &ast.items[0] else {
        panic!("expected variation");
    };
    assert_eq!(method.variation_values[1].value, "field");
}

#[test]
fn a_keyword_named_column_reads_in_an_expression() {
    for word in ["goal", "condition", "variation"] {
        parse_ok(&format!(
            "forbidden(E, {word} == present)\nforbidden(E, E.{word} == present)\n"
        ));
    }
}

#[test]
fn test_parse_non_functional_elements() {
    let ast = parse_ok(
        r#"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
quality Performance "Performance"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
applies_to(CheckoutLatency, api::CheckoutApi)
qualifies(CheckoutLatency, Performance)
constrains(AuditRetention, system::CoreSystem)
"#,
    );

    let Item::Instance(nfr) = &ast.items[0] else {
        panic!("expected nfr instance");
    };
    assert_eq!(nfr.kind, Kind::Nfr);
    assert_eq!(nfr.nfr.metric.as_deref(), Some("p95_latency_ms"));
    assert_eq!(nfr.nfr.target.as_deref(), Some("<=300"));
    assert_eq!(nfr.nfr.window.as_deref(), Some("5m"));
    assert_eq!(nfr.nfr.slo.as_deref(), Some("99.9%"));

    let Item::Instance(quality) = &ast.items[1] else {
        panic!("expected quality instance");
    };
    assert_eq!(quality.kind, Kind::Quality);

    let Item::Instance(constraint) = &ast.items[2] else {
        panic!("expected constraint instance");
    };
    assert_eq!(constraint.kind, Kind::Constraint);
    assert_eq!(constraint.nfr.retention.as_deref(), Some("7y"));
    assert_eq!(constraint.nfr.privacy.as_deref(), Some("restricted"));
}

#[test]
fn test_parse_screen_field_metadata() {
    let ast = parse_ok(
        r#"
field ShippingAddress "Shipping address"
  access editable
  required true
  source actor
field OrderTotal "Order total"
  access readonly
  derived system
contains(CheckoutScreen, ShippingAddress)
maps_field(ShippingAddress, Order.shipping_address)
"#,
    );

    let Item::Instance(field) = &ast.items[0] else {
        panic!("expected field instance");
    };
    assert_eq!(field.kind, Kind::Field);
    assert_eq!(field.field.access.as_deref(), Some("editable"));
    assert_eq!(field.field.required, Some(true));
    assert_eq!(field.field.source.as_deref(), Some("actor"));
    let Item::Instance(derived_field) = &ast.items[1] else {
        panic!("expected derived field instance");
    };
    assert_eq!(derived_field.field.access.as_deref(), Some("readonly"));
    assert_eq!(derived_field.field.source.as_deref(), Some("system"));
}

#[test]
fn test_parse_entity() {
    let src = r#"
entity Product "商品" {
  id:  Int    @pk
  sku: String @unique @index
  name: String @label("商品名")
  price: Decimal @check(price >= 0)
  store_id: Int @index(status, store_id) @unique(sku, store_id)
  status: Enum(active, discontinued) @default(active)
  note: String @null
  tenant_id: Int @tenant
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  display_price: Money @derived(price * tax_rate)
}
"#;
    let ast = parse_ok(src);
    insta::assert_debug_snapshot!(ast);
}

fn column_annotations(src: &str) -> Vec<Annotation> {
    let ast = parse_ok(src);
    let Item::Instance(entity) = &ast.items[0] else {
        panic!("expected an entity");
    };
    entity.columns[0].annotations.clone()
}

#[test]
fn check_and_derived_take_typed_expressions() {
    let check = column_annotations("entity E \"E\" { qty: Int @check(qty >= 1 and qty <= 5) }");
    assert!(
        matches!(
            &check[0],
            Annotation::Check(ColumnExpr::Logical(Expr::And(..)))
        ),
        "{check:?}"
    );
    let derived = column_annotations("entity E \"E\" { net: Int @derived(total - 2) }");
    let Annotation::Derived(ColumnExpr::Arith(arith)) = &derived[0] else {
        panic!("{derived:?}");
    };
    assert_eq!(arith.lhs, Operand::Column("total".to_string()));
    assert_eq!(arith.op, ArithOp::Sub);
    assert_eq!(arith.rhs, Operand::IntLit("2".to_string()));
}

#[test]
fn a_quoted_check_is_an_error_naming_the_unquoted_form() {
    let (_, errors) = parse("entity E \"E\" { qty: Int @check(\"qty >= 1\") }");
    let messages: Vec<String> = errors.iter().map(crate::format_parse_error).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("write `@check(qty >= 1)`")),
        "{messages:?}"
    );
    let (_, errors) = parse("entity E \"E\" { net: Int @derived(\"a - b\") }");
    let messages: Vec<String> = errors.iter().map(crate::format_parse_error).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("write `@derived(a - b)`")),
        "{messages:?}"
    );
}

#[test]
fn arith_takes_one_operator_over_columns_and_integers() {
    for src in [
        "entity E \"E\" { n: Int @derived(a + b + c) }",
        "entity E \"E\" { n: Int @derived(now - a) }",
        "entity E \"E\" { n: Int @derived(E.a * 2) }",
    ] {
        assert!(!parse(src).1.is_empty(), "{src} should not parse");
    }
}

#[test]
fn test_parse_predicate() {
    let ast = parse_ok("performs(Customer, Browse)");
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_relate() {
    let ast = parse_ok(
        r#"relate(Order, Customer, N:1).optional().on_delete(set_null).on_update(cascade)"#,
    );
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_navigates_via() {
    let ast = parse_ok("navigates(CheckInScreen, EligibilityScreen).via(CheckInPatient)");
    let Item::Predicate(pred) = &ast.items[0] else {
        panic!("expected predicate");
    };
    assert_eq!(pred.name, "navigates");
    assert_eq!(pred.args.len(), 2);
    assert_eq!(pred.chain.len(), 1);
    assert_eq!(pred.chain[0].name, "via");
    assert!(matches!(
        pred.chain[0].args.first(),
        Some(PredicateArg::Ref(q)) if q.parts == ["CheckInPatient"]
    ));
}

#[test]
fn test_parse_relate_nm_via() {
    let ast = parse_ok(r#"relate(Order, Product, N:M).via(OrderProduct)"#);
    let Item::Predicate(pred) = &ast.items[0] else {
        panic!("expected predicate");
    };
    assert_eq!(pred.name, "relate");
    assert!(matches!(pred.args.get(2), Some(PredicateArg::Card(c)) if c == "N:M"));
    assert_eq!(pred.chain.len(), 1);
    assert_eq!(pred.chain[0].name, "via");
    assert!(matches!(
        pred.chain[0].args.first(),
        Some(PredicateArg::Ref(q)) if q.parts == ["OrderProduct"]
    ));
}

#[test]
fn test_parse_import() {
    let src = r#"
import shared.actors
import shared.entities as e
import shared.actors.{Customer, Staff}
import shared.actors.{Customer as C}
"#;
    let ast = parse_ok(src);
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_module() {
    let ast = parse_ok("module shared.actors");
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_module_path_with_keyword_segment() {
    let ast = parse_ok("module buc.purchase");
    assert_eq!(ast.items.len(), 1);
    let Item::Module(name, _) = &ast.items[0] else {
        panic!("expected module declaration");
    };
    assert_eq!(name.0, vec!["buc", "purchase"]);
}

#[test]
fn test_parse_full_snippet() {
    let src = r#"
// コメント
module shared.actors

import shared.entities

actor   Customer "顧客"
usecase Browse   "商品を探す"

entity  Order "注文" {
  id: Int @pk
  total: Money
  ordered_at: DateTime
}

performs(Customer, Browse)
relate(Order, Customer, N:1)
"#;
    let ast = parse_ok(src);
    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_parse_inline_comments() {
    let src = r#"
module shop // module comment
actor Customer "顧客" /* actor comment */
usecase Browse "商品を探す" // usecase comment
entity Product "商品" { // columns
  id: Int @pk // primary key
  name: String /* display name */
  // status column
  status: Enum(active, discontinued) @default(active) // default status
}
performs(Customer, Browse) /* predicate comment */
"#;
    let ast = parse_ok(src);
    assert_eq!(ast.items.len(), 5);
}

#[test]
fn test_parse_equals_forbidden() {
    let ast = parse_ok(r#"forbidden(Order, status == cancelled)"#);
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "forbidden");
    assert_eq!(pred.args.len(), 2);
    assert!(matches!(&pred.args[1], PredicateArg::Expr(Expr::Cmp(_))));
    assert!(pred.chain.is_empty());
}

#[test]
fn a_chain_call_reads_actor_and_system_as_bare_words() {
    let ast = parse_ok("exposes(S, U).caller(system)\nexposes(S, V).caller(actor).append()");
    let words: Vec<String> = ast
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Predicate(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.chain.iter())
        .filter(|c| c.name == "caller")
        .map(|c| match &c.args[0] {
            PredicateArg::Ref(q) => q.parts.join("."),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(words, ["system", "actor"]);
}

#[test]
fn a_kind_qualified_ref_in_a_chain_argument_still_parses() {
    for (src, kind, id) in [
        (
            "forbidden(Order, status == b).role(actor::Customer)",
            Kind::Actor,
            "Customer",
        ),
        (
            "relate(Line, Order, N:1).via(system::Shop)",
            Kind::System,
            "Shop",
        ),
    ] {
        let ast = parse_ok(src);
        let Item::Predicate(pred) = &ast.items[0] else {
            panic!("{src}")
        };
        let PredicateArg::Ref(q) = &pred.chain[0].args[0] else {
            panic!("{src}: {:?}", pred.chain[0].args)
        };
        assert_eq!(q.kind_qualifier, Some(kind), "{src}");
        assert_eq!(q.parts, [id], "{src}");
    }
}

#[test]
fn a_bare_kind_word_outside_caller_is_a_syntax_error() {
    for src in [
        "relate(Line, Order, N:1).role(actor)",
        "forbidden(Order, status == b).role(system)",
        "performs(actor, U)",
    ] {
        let (_, errors) = parse(src);
        assert!(!errors.is_empty(), "must reject: {src}");
    }
}

#[test]
fn test_parse_chained_invariant() {
    let ast =
        parse_ok(r#"invariant(Order).when(status == delivered).then(delivered_at == present)"#);
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "invariant");
    assert_eq!(pred.args.len(), 1);
    assert_eq!(pred.chain.len(), 2);
    assert_eq!(pred.chain[0].name, "when");
    assert_eq!(pred.chain[0].args.len(), 1);
    assert!(matches!(
        &pred.chain[0].args[0],
        PredicateArg::Expr(Expr::Cmp(_))
    ));
    assert_eq!(pred.chain[1].name, "then");
    assert_eq!(pred.chain[1].args.len(), 1);
}

#[test]
fn test_parse_cross_forbidden_with_along_chain() {
    let ast = parse_ok(
        r#"forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)"#,
    );
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "forbidden");
    assert_eq!(pred.args.len(), 4);
    assert_eq!(pred.chain.len(), 1);
    assert_eq!(pred.chain[0].name, "along");
    assert_eq!(pred.chain[0].args.len(), 2);
}

#[test]
fn test_parse_multi_chain_invariant() {
    let ast = parse_ok(
        r#"invariant(Order).when(status == delivered).when(refunded == false).then(refund_id == null)"#,
    );
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.chain.len(), 3);
    assert_eq!(pred.chain[0].name, "when");
    assert_eq!(pred.chain[1].name, "when");
    assert_eq!(pred.chain[2].name, "then");
}

#[test]
fn test_parse_typed_qref() {
    let src = r#"
actor    Add "追加"
usecase  Add "追加する"
performs(actor::Add, usecase::Add)
"#;
    let ast = parse_ok(src);
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "performs");
    if let PredicateArg::Ref(qref) = &pred.args[0] {
        assert_eq!(qref.kind_qualifier, Some(Kind::Actor));
        assert_eq!(qref.parts, vec!["Add"]);
    } else {
        panic!("expected Ref arg");
    }
    if let PredicateArg::Ref(qref) = &pred.args[1] {
        assert_eq!(qref.kind_qualifier, Some(Kind::UseCase));
        assert_eq!(qref.parts, vec!["Add"]);
    } else {
        panic!("expected Ref arg");
    }
}

// ── 比較式（Expr）のパーステスト ──────────────────────────────────────────

/// 既存の呼び出し `performs(A, B)` が Expr ではなく Ref になること（後退しないことの確認）
#[test]
fn test_existing_call_unaffected_by_expr() {
    let ast = parse_ok("performs(Customer, Browse)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "performs");
    assert!(
        matches!(&pred.args[0], PredicateArg::Ref(_)),
        "first arg should be Ref"
    );
    assert!(
        matches!(&pred.args[1], PredicateArg::Ref(_)),
        "second arg should be Ref"
    );
}

#[test]
fn test_parse_relate_card_and_transition() {
    let relate = parse_ok("relate(Payment, Order, N:1)");
    let pred = relate
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert!(matches!(&pred.args[2], PredicateArg::Card(c) if c == "N:1"));

    for card in ["1..*:1", "0..1:1", "1:1..*", "2..5:1"] {
        let ast = parse_ok(&format!("relate(Line, Order, {card})"));
        let Some(Item::Predicate(pred)) = ast.items.first() else {
            panic!("no predicate for {card}");
        };
        assert!(matches!(&pred.args[2], PredicateArg::Card(c) if c == card));
    }
    let ast = parse_ok("/* a * b */ relate(Line, Order, 1 .. * : 1) // *");
    let Some(Item::Predicate(pred)) = ast.items.first() else {
        panic!("no predicate");
    };
    assert!(matches!(&pred.args[2], PredicateArg::Card(c) if c == "1..*:1"));

    let trans = parse_ok("transitions(CapturePay, Order.status, pending -> paid)");
    let pred = trans
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert!(matches!(
        &pred.args[2],
        PredicateArg::Transition { from, to } if from == &["pending"] && to == "paid"
    ));
}

#[test]
fn a_transition_lists_its_sources_in_parentheses() {
    let ast = parse_ok("transitions(Expire, Order.status, (pending, held) -> cancelled)");
    let Some(Item::Predicate(pred)) = ast.items.first() else {
        panic!("no predicate");
    };
    assert!(matches!(
        &pred.args[2],
        PredicateArg::Transition { from, to } if from == &["pending", "held"] && to == "cancelled"
    ));
    let ast = parse_ok("transitions(Expire, Order.status, (pending) -> cancelled)");
    let Some(Item::Predicate(pred)) = ast.items.first() else {
        panic!("no predicate");
    };
    assert!(matches!(
        &pred.args[2],
        PredicateArg::Transition { from, .. } if from == &["pending"]
    ));
    for src in [
        "transitions(Expire, Order.status, () -> cancelled)",
        "transitions(Expire, Order.status, * -> cancelled)",
    ] {
        let (_, errors) = parse(src);
        assert!(!errors.is_empty(), "{src} parsed");
    }
}

#[test]
fn test_parse_invariant_none_quantifier() {
    let ast = parse_ok("invariant(Cert).when(status == revoked).none(Assign.status == active)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "invariant");
    assert_eq!(pred.chain.len(), 2);
    assert_eq!(pred.chain[1].name, "none");
}

/// 比較式 `stock < selling` が `PredicateArg::Expr(Cmp)` としてパースされること
#[test]
fn test_parse_comparison_col_col() {
    let ast = parse_ok("forbidden(Stock, stock < selling)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.name, "forbidden");
    assert_eq!(pred.args.len(), 2);
    if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
        assert_eq!(cmp.lhs, Operand::Column("stock".to_string()));
        assert_eq!(cmp.op, CmpOp::Lt);
        assert_eq!(cmp.rhs, Operand::Column("selling".to_string()));
    } else {
        panic!("expected Expr(Cmp), got {:?}", pred.args[1]);
    }
}

/// クロスエンティティ比較式 `Order.total > Payment.amount` がパースされること
#[test]
fn test_parse_comparison_qualified_columns() {
    let ast = parse_ok("forbidden(Order, Payment, Order.total > Payment.amount)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[2] {
        if let Operand::QualifiedColumn(lhs) = &cmp.lhs {
            assert_eq!(lhs.entity.parts, vec!["Order"]);
            assert_eq!(lhs.column, "total");
        } else {
            panic!("expected qualified lhs");
        }
        assert_eq!(cmp.op, CmpOp::Gt);
        if let Operand::QualifiedColumn(rhs) = &cmp.rhs {
            assert_eq!(rhs.entity.parts, vec!["Payment"]);
            assert_eq!(rhs.column, "amount");
        } else {
            panic!("expected qualified rhs");
        }
    } else {
        panic!("expected Expr(Cmp), got {:?}", pred.args[2]);
    }
}

/// 比較式 `stock >= 0` (整数リテラル右辺) がパースされること
#[test]
fn test_parse_comparison_col_intlit() {
    let ast = parse_ok("forbidden(Stock, stock >= 0)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
        assert_eq!(cmp.lhs, Operand::Column("stock".to_string()));
        assert_eq!(cmp.op, CmpOp::Ge);
        assert_eq!(cmp.rhs, Operand::IntLit("0".to_string()));
    } else {
        panic!("expected Expr(Cmp) with IntLit rhs");
    }
}

/// 比較式 `expired_at < now` (組み込み now) がパースされること
#[test]
fn test_parse_comparison_col_now() {
    let ast = parse_ok("forbidden(Coupon, expired_at < now)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
        assert_eq!(cmp.lhs, Operand::Column("expired_at".to_string()));
        assert_eq!(cmp.op, CmpOp::Lt);
        assert_eq!(cmp.rhs, Operand::Now);
    } else {
        panic!("expected Expr(Cmp) with Now rhs");
    }
}

/// invariant の `.when(expr).then(expr)` 形式
#[test]
fn test_parse_invariant_with_comparison_chain() {
    let ast = parse_ok("invariant(Order).when(expired_at < now).then(status == expired)");
    let pred = ast
        .items
        .iter()
        .find_map(|i| {
            if let Item::Predicate(p) = i {
                Some(p)
            } else {
                None
            }
        })
        .expect("predicate not found");
    assert_eq!(pred.chain.len(), 2);
    assert_eq!(pred.chain[0].name, "when");
    assert_eq!(pred.chain[0].args.len(), 1);
    assert!(
        matches!(&pred.chain[0].args[0], PredicateArg::Expr(Expr::Cmp(_))),
        "when arg should be Expr(Cmp)"
    );
    assert_eq!(pred.chain[1].name, "then");
    assert_eq!(pred.chain[1].args.len(), 1);
    assert!(matches!(
        &pred.chain[1].args[0],
        PredicateArg::Expr(Expr::Cmp(_))
    ));
}

/// 全比較演算子トークンが正しくパースされること
#[test]
fn test_parse_all_cmp_ops() {
    let cases = [
        ("a < b", CmpOp::Lt),
        ("a > b", CmpOp::Gt),
        ("a <= b", CmpOp::Le),
        ("a >= b", CmpOp::Ge),
        ("a == b", CmpOp::Eq),
        ("a != b", CmpOp::Ne),
    ];
    for (expr_str, expected_op) in cases {
        let src = format!("forbidden(E, {})", expr_str);
        let ast = parse_ok(&src);
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = &pred.args[1] {
            assert_eq!(cmp.op, expected_op, "failed for: {}", expr_str);
        } else {
            panic!("expected Expr(Cmp) for: {}", expr_str);
        }
    }
}

#[test]
fn test_parse_property_label_optional() {
    let ast = parse_ok(
        r#"property StockOk
  always(Item.stock >= Item.selling)"#,
    );
    let Item::Property(prop) = &ast.items[0] else {
        panic!("expected Property");
    };
    assert_eq!(prop.id, "StockOk");
    assert!(prop.label.is_none());
    assert!(matches!(&prop.formula, AstTemporalFormula::Always(_)));
}

#[test]
fn test_parse_property_leads_to() {
    let ast = parse_ok(
        r#"property PaidLeadsToShipped "paid eventually ships"
  leads_to(Order.status == paid, Order.status == shipped)
"#,
    );
    let Item::Property(prop) = &ast.items[0] else {
        panic!("expected Property");
    };
    assert_eq!(prop.id, "PaidLeadsToShipped");
    assert!(matches!(&prop.formula, AstTemporalFormula::LeadsTo { .. }));
}

#[test]
fn test_parse_property_fair_chain() {
    let ast = parse_ok(
        r#"property GrantEnds
  leads_to(A.status == granted, A.status == registered).fair(Pay, Register)
"#,
    );
    let Item::Property(prop) = &ast.items[0] else {
        panic!("expected Property");
    };
    assert_eq!(prop.chain.len(), 1);
    assert_eq!(prop.chain[0].name, "fair");
    assert_eq!(prop.chain[0].args.len(), 2);
}

#[test]
fn test_parse_property_always_with_or() {
    let ast = parse_ok(
        r#"property EventuallyTerminal "terminal"
  eventually(Order.status == delivered or Order.status == cancelled)
"#,
    );
    let Item::Property(prop) = &ast.items[0] else {
        panic!("expected Property");
    };
    assert!(matches!(
        &prop.formula,
        AstTemporalFormula::Eventually(Expr::Or(_, _))
    ));
}

#[test]
fn test_parse_property_tla_or_alias() {
    let ast = parse_ok(
        r#"property EventuallyTerminal "terminal"
  eventually(Order.status == delivered \/ Order.status == cancelled)
"#,
    );
    let Item::Property(prop) = &ast.items[0] else {
        panic!("expected Property");
    };
    assert!(matches!(
        &prop.formula,
        AstTemporalFormula::Eventually(Expr::Or(_, _))
    ));
}

#[test]
fn empty_enum_is_parse_error() {
    let (_ast, errors) = parse("entity E \"e\" { status: Enum() }");
    assert!(!errors.is_empty(), "Enum() must be rejected at parse time");
    let msg = format!("{:?}", errors);
    assert!(
        msg.contains("Enum()") || msg.contains("variant"),
        "unexpected errors: {msg}"
    );
}

#[test]
fn enum_values_take_optional_labels() {
    let (ast, errors) = parse("entity E \"e\" { status: Enum(open \"受付中\", closed) }");
    assert!(errors.is_empty(), "{errors:?}");
    let Item::Instance(inst) = &ast.items[0] else {
        panic!("expected instance");
    };
    assert_eq!(
        inst.columns[0].col_type,
        ColType::Enum(vec![
            EnumValue {
                value: "open".to_string(),
                label: Some("受付中".to_string()),
            },
            EnumValue {
                value: "closed".to_string(),
                label: None,
            },
        ])
    );
}

#[test]
fn variation_block_lists_values_with_optional_labels() {
    let (ast, errors) =
        parse("variation PaymentMethod \"Payment Method\" {\n  cash \"Cash\"\n  card\n}\n");
    assert!(errors.is_empty(), "{errors:?}");
    let Item::Instance(inst) = &ast.items[0] else {
        panic!("expected instance");
    };
    assert_eq!(inst.kind.name(), "variation");
    let values: Vec<_> = inst
        .variation_values
        .iter()
        .map(|v| (v.value.as_str(), v.label.as_deref()))
        .collect();
    assert_eq!(values, vec![("cash", Some("Cash")), ("card", None)]);
    assert!(inst.columns.is_empty());
}

#[test]
fn variation_without_values_is_parse_error() {
    for src in [
        "variation V \"V\"\n",
        "variation V \"V\" {}\n",
        "variation V \"V\" { a: Int }\n",
        "variation V \"V\" { a\n a }\n",
        "entity E \"E\" { a \"A\" }\n",
    ] {
        let (_ast, errors) = parse(src);
        assert!(!errors.is_empty(), "must reject: {src}");
    }
}

#[test]
fn column_type_position_accepts_a_variation_name() {
    let (ast, errors) = parse("entity E \"E\" { method: PaymentMethod @default(card) }\n");
    assert!(errors.is_empty(), "{errors:?}");
    let Item::Instance(inst) = &ast.items[0] else {
        panic!("expected instance");
    };
    assert_eq!(
        inst.columns[0].col_type,
        ColType::Named("PaymentMethod".into())
    );
}

#[test]
fn variation_stays_usable_as_column_and_path_name() {
    let (_, errors) = parse("entity E \"E\" { variation: String }\n");
    assert!(errors.is_empty(), "{errors:?}");
    let (_, errors) = parse("module variation.shared\nimport variation.shared\n");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn condition_is_a_kind_and_stays_usable_as_column_and_path_name() {
    let (ast, errors) = parse("condition CopayDue \"A copay is due\"\n");
    assert!(errors.is_empty(), "{errors:?}");
    let inst = ast.items.iter().find_map(|i| match i {
        Item::Instance(inst) => Some(inst),
        _ => None,
    });
    assert_eq!(inst.map(|i| i.kind.name()), Some("condition"));
    let (_, errors) = parse("entity E \"E\" { condition: String }\n");
    assert!(errors.is_empty(), "{errors:?}");
    let (_, errors) = parse("module condition.shared\nimport condition.shared\n");
    assert!(errors.is_empty(), "{errors:?}");
}

fn first_cmp(src: &str) -> Comparison {
    let ast = parse_ok(src);
    let pred = ast
        .items
        .iter()
        .find_map(|i| match i {
            Item::Predicate(p) => Some(p),
            _ => None,
        })
        .expect("predicate not found");
    pred.args
        .iter()
        .chain(pred.chain.iter().flat_map(|c| c.args.iter()))
        .find_map(|a| match a {
            PredicateArg::Expr(Expr::Cmp(cmp)) => Some(cmp.clone()),
            _ => None,
        })
        .expect("comparison not found")
}

#[test]
fn parses_set_membership() {
    let cmp = first_cmp("forbidden(E, status in (a, b))");
    assert_eq!(cmp.lhs, Operand::Column("status".into()));
    assert_eq!(cmp.op, CmpOp::In);
    assert_eq!(cmp.rhs, Operand::Values(vec!["a".into(), "b".into()]));

    let cmp = first_cmp("holds(C, E.status not in (a))");
    assert!(matches!(cmp.lhs, Operand::QualifiedColumn(_)));
    assert_eq!(cmp.op, CmpOp::NotIn);
    assert_eq!(cmp.rhs, Operand::Values(vec!["a".into()]));

    let cmp = first_cmp("invariant(E).when(qty in (1, 2)).then(x == y)");
    assert_eq!(cmp.rhs, Operand::Values(vec!["1".into(), "2".into()]));
}

#[test]
fn an_empty_or_unclosed_set_does_not_parse() {
    for src in [
        "forbidden(E, status in ())",
        "forbidden(E, status in (a,))",
        "forbidden(E, status in a)",
        "forbidden(E, status not (a))",
        "forbidden(E, status == (a, b))",
    ] {
        let (_, errors) = parse(src);
        assert!(!errors.is_empty(), "{src}");
    }
}

#[test]
fn an_empty_set_names_the_missing_values() {
    for src in [
        "forbidden(E, status in ())",
        "invariant(E).when(status not in ()).then(x == y)",
        "property P always(E.status in ())",
        "entity E \"E\" { id: Int @pk  s: Enum(a) @check(s in ()) }",
    ] {
        let (_, errors) = parse(src);
        assert_eq!(errors.len(), 1, "{src}: {errors:?}");
        assert!(
            format!("{:?}", errors[0]).contains("takes at least one value"),
            "{src}: {errors:?}"
        );
    }
}

#[test]
fn set_membership_reads_in_logical_expressions() {
    let ast = parse_ok("property P always(not E.status in (a) or E.status not in (b, c))");
    let Some(Item::Property(prop)) = ast.items.first() else {
        panic!("expected property");
    };
    let AstTemporalFormula::Always(Expr::Or(a, b)) = &prop.formula else {
        panic!("expected always(or)");
    };
    assert!(
        matches!(&**a, Expr::Not(inner) if matches!(&**inner, Expr::Cmp(c) if c.op == CmpOp::In))
    );
    assert!(matches!(&**b, Expr::Cmp(c) if c.op == CmpOp::NotIn));
    let (_, errors) = parse("entity E \"E\" { id: Int @pk  s: Enum(a, b) @check(s in (a, b)) }");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn in_stays_usable_as_a_column_name() {
    let (_, errors) = parse("entity E \"E\" { id: Int @pk  in: Int }\n");
    assert!(errors.is_empty(), "{errors:?}");
    let cmp = first_cmp("forbidden(E, in > 3)");
    assert_eq!(cmp.lhs, Operand::Column("in".into()));
    let cmp = first_cmp("forbidden(E, in in (1, 2))");
    assert_eq!(cmp.lhs, Operand::Column("in".into()));
    assert_eq!(cmp.op, CmpOp::In);
    let cmp = first_cmp("forbidden(E, x == in)");
    assert_eq!(cmp.rhs, Operand::Column("in".into()));
    let cmp = first_cmp("forbidden(E, E.in not in (a))");
    assert_eq!(cmp.op, CmpOp::NotIn);
}

#[test]
fn lex_errors_are_reported() {
    let (_ast, errors) = parse("actor Customer \"ok\" #broken");
    assert!(
        !errors.is_empty(),
        "unknown characters must not be silently dropped"
    );
}
