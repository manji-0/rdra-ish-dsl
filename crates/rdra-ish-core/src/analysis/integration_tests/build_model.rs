use crate::analysis::build_model;
use crate::analysis::nodes::node_kind_tag_str;
use crate::model::*;
use rdra_ish_syntax::parse;

#[test]
fn transition_and_sets_facts_carry_their_predicate_site() {
    let src = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
  note: DateTime @null
}
usecase Pay "Pay"
transitions(Order.status, Pay, open -> paid)
sets(Pay, Order, note == present)
"#;
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");

    let transition = model.state_transitions.first().expect("a transition");
    let site = transition.site.as_ref().expect("transition site");
    assert!(src[site.span.clone()].starts_with("transitions("));

    let effect = model.column_effects.first().expect("a sets effect");
    let site = effect.site.as_ref().expect("sets site");
    assert!(src[site.span.clone()].starts_with("sets("));
}

#[test]
fn test_build_model_basic() {
    let src = r#"
actor Customer "顧客" description "商品を購入する顧客"
entity Order "注文" description "受注情報" { id: Int @pk }
entity Customer_profile "顧客情報" { id: Int @pk  name: String }
usecase Browse "商品を探す" description "商品一覧を参照する"
performs(Customer, Browse)
relate(Order, Customer_profile, N:1)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "unexpected errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    assert_eq!(model.actors.len(), 1);
    let actor = model.actors.values().next().unwrap();
    assert_eq!(actor.id, "Customer");
    assert_eq!(actor.label, "顧客");
    assert_eq!(actor.description.as_deref(), Some("商品を購入する顧客"));
    let use_case = model.use_cases.values().next().unwrap();
    assert_eq!(use_case.description.as_deref(), Some("商品一覧を参照する"));

    let (order_key, order) = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "Order")
        .expect("Order entity not found");
    assert_eq!(order.description.as_deref(), Some("受注情報"));

    let fk_col = order
        .columns
        .iter()
        .find(|c| c.name == "customer_profile_id")
        .expect("customer_profile_id FK column not found");

    let (_, fk) = model
        .column_fk(order_key, &fk_col.name)
        .expect("customer_profile_id is a key column");
    assert_eq!(model.entities[fk.parent].id, "Customer_profile");
    assert_eq!(fk_col.col_type, ColumnType::Int);
}

#[test]
fn one_to_one_key_is_a_unique_column_on_the_left_end() {
    let src = r#"
entity Order "Order" { id: Int @pk }
entity Payment "Payment" { id: Int @pk }
entity Receipt "Receipt" { id: Int @pk }
relate(Payment, Order, 1:1)
relate(Receipt, Order, 1:1).optional()
"#;
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "{diags:?}");

    for (child, nullable) in [("Payment", false), ("Receipt", true)] {
        let (child_key, entity) = model.entities.iter().find(|(_, e)| e.id == child).unwrap();
        let column = entity
            .columns
            .iter()
            .find(|c| c.name == "order_id")
            .unwrap_or_else(|| panic!("{child} holds order_id"));
        assert!(column.is_unique, "{child}.order_id is unique");
        assert_eq!(column.is_nullable, nullable, "{child}.order_id nullability");
        let (_, fk) = model.column_fk(child_key, "order_id").unwrap();
        assert!(fk.unique());
        assert_eq!(model.entities[fk.parent].id, "Order");
        assert_eq!(fk.columns, vec![("order_id".to_string(), "id".to_string())]);
    }
    let order = model.entities.values().find(|e| e.id == "Order").unwrap();
    assert!(!order.columns.iter().any(|c| c.name.ends_with("_id")));
}

#[test]
fn test_build_model_requirement_metadata() {
    let src = r#"
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority "must"
  source "Customer interview"
  source "Incident review"
  stakeholder "Store Operations"
  owner "Product Owner"
  acceptance criteria "A payment timeout leaves the cart recoverable."
  status "proposed"
  risk "high"
  rationale "Checkout failures directly block revenue."
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let requirement = model.requirements.values().next().unwrap();
    assert_eq!(requirement.priority.as_deref(), Some("must"));
    assert_eq!(
        requirement.sources,
        vec![
            "Customer interview".to_string(),
            "Incident review".to_string()
        ]
    );
    assert_eq!(
        requirement.stakeholders,
        vec!["Store Operations".to_string()]
    );
    assert_eq!(requirement.owner.as_deref(), Some("Product Owner"));
    assert_eq!(
        requirement.acceptance_criteria,
        vec!["A payment timeout leaves the cart recoverable.".to_string()]
    );
    assert_eq!(requirement.status.as_deref(), Some("proposed"));
    assert_eq!(requirement.risk.as_deref(), Some("high"));
    assert_eq!(
        requirement.rationale.as_deref(),
        Some("Checkout failures directly block revenue.")
    );
}

#[test]
fn test_build_model_adr_metadata_and_decision_links() {
    let src = r#"
adr AdrOutbox "Use transactional outbox"
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."
system CustomerSystem "Customer System"
entity Customer "Customer" { id: Int @pk }
api PublishCustomerChanged "Publish customer changed"
decides(AdrOutbox, CustomerSystem)
decides(AdrOutbox, Customer)
decides(AdrOutbox, PublishCustomerChanged)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let (adr_key, adr) = model.adrs.iter().next().unwrap();
    assert_eq!(adr.status.as_deref(), Some("accepted"));
    assert_eq!(
        adr.decision.as_deref(),
        Some("Publish customer changes through a transactional outbox.")
    );
    assert_eq!(adr.accepted_options, vec!["Transactional outbox"]);
    assert_eq!(adr.rejected_options, vec!["Synchronous callback"]);
    assert_eq!(adr.reasons.len(), 1);

    let target_kinds: Vec<_> = model
        .relations
        .iter()
        .filter(|relation| {
            relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr_key)
        })
        .map(|relation| node_kind_tag_str(&relation.to))
        .collect();
    assert_eq!(target_kinds, vec!["system", "entity", "api"]);
}

#[test]
fn test_build_model_business_flow_relations() {
    let src = r#"
buc BucCheckout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
step AuthorizePayment "Authorize payment"
step PaymentFailed "Payment failed"
usecase CapturePayment "Capture payment"
api PaymentApi "Payment API"
event PaymentRejected "Payment rejected"
contains(BucCheckout, CheckoutFlow)
contains(CheckoutFlow, ReviewCart)
contains(CheckoutFlow, AuthorizePayment)
precedes(ReviewCart, AuthorizePayment)
branches(ReviewCart, PaymentFailed)
excepts(AuthorizePayment, PaymentFailed)
repeats(PaymentFailed, ReviewCart)
covers(AuthorizePayment, CapturePayment)
covers(AuthorizePayment, PaymentApi)
covers(PaymentFailed, PaymentRejected)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    assert_eq!(model.flows.len(), 1);
    assert_eq!(model.steps.len(), 3);
    for kind in [
        RelKind::Precedes,
        RelKind::Branches,
        RelKind::Excepts,
        RelKind::Repeats,
        RelKind::Covers,
    ] {
        assert!(
            model.relations.iter().any(|rel| rel.kind == kind),
            "missing {kind:?} relation"
        );
    }
}

#[test]
fn test_build_model_usecase_metadata_and_compensation() {
    let src = r#"
usecase CapturePayment "Capture payment"
  precondition "Order is authorized."
  guard "Provider is available."
  postcondition "Payment is captured."
  alternative "Customer changes payment method."
  error "Authorization expires."
usecase RefundPayment "Refund payment"
compensates(RefundPayment, CapturePayment)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let capture = model
        .use_cases
        .values()
        .find(|usecase| usecase.id == "CapturePayment")
        .unwrap();
    assert_eq!(capture.preconditions, vec!["Order is authorized."]);
    assert_eq!(capture.guards, vec!["Provider is available."]);
    assert_eq!(capture.postconditions, vec!["Payment is captured."]);
    assert_eq!(
        capture.alternatives,
        vec!["Customer changes payment method."]
    );
    assert_eq!(capture.errors, vec!["Authorization expires."]);
    assert!(
        model
            .relations
            .iter()
            .any(|relation| relation.kind == RelKind::Compensates),
        "compensates should become a relation"
    );
}

#[test]
fn test_build_model_api_contract_metadata_and_dto_relations() {
    let src = r#"
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
dto OrderResponse "Order response" { order_id: Int }
dto ErrorResponse "Error response" { code: String  message: String }
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let api = model.apis.values().next().unwrap();
    assert_eq!(api.method.as_deref(), Some("POST"));
    assert_eq!(api.path.as_deref(), Some("/orders"));
    assert_eq!(api.idempotency.as_deref(), Some("idempotent"));
    assert_eq!(api.mode.as_deref(), Some("sync"));
    assert_eq!(api.auth_scheme.as_deref(), Some("bearer"));

    let request_dto = model
        .dtos
        .values()
        .find(|dto| dto.id == "CreateOrderRequest")
        .unwrap();
    assert_eq!(request_dto.fields.len(), 2);
    assert!(request_dto
        .fields
        .iter()
        .any(|field| field.name == "note" && field.is_nullable));

    for kind in [RelKind::Request, RelKind::Response, RelKind::ErrorResponse] {
        assert!(
            model.relations.iter().any(|rel| rel.kind == kind),
            "missing {kind:?} relation"
        );
    }
}

#[test]
fn test_build_model_non_functional_elements_and_relations() {
    let src = r#"
system CoreSystem "Core system"
usecase Checkout "Checkout"
api CheckoutApi "Checkout API"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable
quality Performance "Performance"
quality Availability "Availability"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
applies_to(CheckoutLatency, Checkout)
applies_to(CheckoutLatency, CheckoutApi)
applies_to(CheckoutLatency, CoreSystem)
qualifies(CheckoutLatency, Performance)
qualifies(AuditRetention, Availability)
constrains(AuditRetention, CoreSystem)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let nfr = model.nfrs.values().next().unwrap();
    assert_eq!(nfr.metric.as_deref(), Some("p95_latency_ms"));
    assert_eq!(nfr.target.as_deref(), Some("<=300"));
    assert_eq!(nfr.window.as_deref(), Some("5m"));
    assert_eq!(nfr.slo.as_deref(), Some("99.9%"));
    assert_eq!(nfr.availability.as_deref(), Some("multi_az"));
    assert_eq!(nfr.resilience.as_deref(), Some("retryable"));

    let constraint = model.constraints.values().next().unwrap();
    assert_eq!(constraint.audit.as_deref(), Some("enabled"));
    assert_eq!(constraint.logging.as_deref(), Some("structured"));
    assert_eq!(constraint.retention.as_deref(), Some("7y"));
    assert_eq!(constraint.privacy.as_deref(), Some("restricted"));

    for kind in [RelKind::AppliesTo, RelKind::Qualifies, RelKind::Constrains] {
        assert!(
            model.relations.iter().any(|rel| rel.kind == kind),
            "missing {kind:?} relation"
        );
    }
}

#[test]
fn test_build_model_system_ownership_relation() {
    let src = r#"
system StoreSystem "Store system"
entity Store "Store" { id: Int @pk }
owns(StoreSystem, Store)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert!(
        model.relations.iter().any(|rel| rel.kind == RelKind::Owns),
        "owns(System, Entity) should become an Owns relation"
    );
}

#[test]
fn test_build_model_index_and_composite_unique_annotations() {
    let src = r#"
entity Product "Product" {
  id: Int @pk
  sku: String @unique @index
  store_id: Int @index(status, store_id) @unique(sku, store_id)
  status: Enum(active, discontinued) @default(active)
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let product = model.entities.values().next().unwrap();
    let sku = product
        .columns
        .iter()
        .find(|column| column.name == "sku")
        .unwrap();
    assert!(sku.is_unique);
    assert!(sku.is_indexed);
    assert_eq!(
        product.unique_constraints,
        vec![
            vec!["sku".to_string()],
            vec!["sku".to_string(), "store_id".to_string()]
        ]
    );
    assert_eq!(
        product.indexes,
        vec![
            vec!["sku".to_string()],
            vec!["status".to_string(), "store_id".to_string()]
        ]
    );
}

#[test]
fn test_build_model_data_modeling_annotations_and_fk_options() {
    let src = r#"
entity Customer "Customer" {
  id: Int @pk
}
entity Order "Order" {
  id: Int @pk
  tenant_id: Int @tenant
  total: Money @check("total >= 0")
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  net_total: Money @derived("total - discount")
}
relate(Order, Customer, N:1).optional().on_delete(set_null).on_update(cascade)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let order = model
        .entities
        .iter()
        .find_map(|(_, entity)| (entity.id == "Order").then_some(entity))
        .unwrap();
    let tenant_id = order
        .columns
        .iter()
        .find(|column| column.name == "tenant_id")
        .unwrap();
    assert!(tenant_id.is_tenant_scope);
    let total = order
        .columns
        .iter()
        .find(|column| column.name == "total")
        .unwrap();
    assert_eq!(total.check_constraints, vec!["total >= 0"]);
    let deleted_at = order
        .columns
        .iter()
        .find(|column| column.name == "deleted_at")
        .unwrap();
    assert!(deleted_at.is_soft_delete);
    let valid_from = order
        .columns
        .iter()
        .find(|column| column.name == "valid_from")
        .unwrap();
    assert!(valid_from.is_history);
    let net_total = order
        .columns
        .iter()
        .find(|column| column.name == "net_total")
        .unwrap();
    assert_eq!(net_total.derived_expr.as_deref(), Some("total - discount"));
    let customer_id = order
        .columns
        .iter()
        .find(|column| column.name == "customer_id")
        .unwrap();
    let order_key = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "Order")
        .map(|(k, _)| k)
        .unwrap();
    let (_, fk) = model.column_fk(order_key, &customer_id.name).unwrap();
    assert!(fk.optional());
    assert!(customer_id.is_nullable);
    assert_eq!(fk.on_delete, Some(RefAction::SetNull));
    assert_eq!(fk.on_update, Some(RefAction::Cascade));
}

#[test]
fn test_build_model_rejects_requirement_metadata_on_other_kinds() {
    let src = r#"actor Customer "Customer" priority "must""#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (_, diags) = build_model(&ast);
    let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("requirement metadata is only valid")),
        "expected requirement metadata target diagnostic, got {messages:?}"
    );
}

#[test]
fn test_build_model_rejects_api_metadata_on_other_kinds() {
    let src = r#"usecase PlaceOrder "Place order" method POST"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (_, diags) = build_model(&ast);
    let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("api metadata is only valid")),
        "expected api metadata target diagnostic, got {messages:?}"
    );
}

#[test]
fn test_build_model_rejects_nfr_metadata_on_invalid_kinds() {
    let src = r#"quality Performance "Performance" metric p95_latency_ms"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (_, diags) = build_model(&ast);
    let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("non-functional metadata is only valid")),
        "expected nfr metadata target diagnostic, got {messages:?}"
    );
}
