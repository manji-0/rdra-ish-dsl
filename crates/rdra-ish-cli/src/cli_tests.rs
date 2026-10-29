use crate::cli::{ExportKind, ListFormat, ListKind};
use crate::export::export_artifact;
use crate::list_output::{
    format_lint_issues, list_elements, state_consistency_findings, state_diag_message,
};
use crate::load::load_model;
use rdra_ish_core::lifecycle::{InvKind, InvText, ModelRef, Site, StateDiag, Trace};
use rdra_ish_core::{format_diagnostic_message, lint_issues, SemanticModel};
use rdra_ish_emit::View;
use std::path::PathBuf;

#[test]
fn load_model_rejects_missing_input_paths() {
    let err = load_model(&[PathBuf::from("missing-input")]).unwrap_err();

    assert!(
        err.to_string().contains("not found"),
        "expected missing-path error, got: {err}"
    );
}

fn errors_fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(path)
}

#[test]
fn check_command_includes_source_location() {
    let inputs = vec![errors_fixture("tests/fixtures/errors/type_mismatch.rdra")];
    let (program, _, diags) = load_model(&inputs).expect("load model");
    let mismatch = diags
        .iter()
        .find(|d| matches!(&d.error, rdra_ish_core::RdraError::TypeMismatch { .. }))
        .expect("type mismatch diagnostic");
    assert!(
        mismatch.location.is_some(),
        "expected location on diagnostic: {:?}",
        diags
    );
    let message = format_diagnostic_message(
        Some(&program),
        mismatch.is_warning,
        mismatch.location.as_ref(),
        &mismatch.error.to_string(),
    );
    assert!(
        message.contains("type_mismatch.rdra"),
        "cli diagnostic should include file path: {message}"
    );
}

#[test]
fn table_list_reports_empty_api_result() {
    let model = SemanticModel::default();

    let output = list_elements(&model, &ListKind::Api, &ListFormat::Table).unwrap();

    assert_eq!(output, "No APIs found.\n");
}

#[test]
fn structured_empty_lists_stay_machine_readable() {
    let model = SemanticModel::default();

    let csv = list_elements(&model, &ListKind::Api, &ListFormat::Csv).unwrap();
    let json = list_elements(&model, &ListKind::Api, &ListFormat::Json).unwrap();

    assert_eq!(
        csv,
        "api_id,api_label,method,path,idempotency,mode,auth_scheme\n"
    );
    assert_eq!(json, "[]\n");
}

#[test]
fn list_api_includes_contract_metadata() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
api CreateOrder "Create order" method POST path "/orders" idempotency "idempotent" mode sync auth bearer
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Api, &ListFormat::Csv).unwrap();
    assert!(csv.contains("api_id,api_label,method,path,idempotency,mode,auth_scheme"));
    assert!(csv.contains("CreateOrder,Create order,POST,/orders,idempotent,sync,bearer"));
}

#[test]
fn lint_reports_coverage_readiness_and_review_findings() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
actor customer "Customer"
buc Checkout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
api CreateOrder "Create order" method POST
dto CreateOrderRequest "Create order request"
field ShippingAddress "Shipping address" access editable source actor
entity Order "Order" {
  Id: Int @pk
  total: Money
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let issues = lint_issues(&model, &diags);
    let csv = format_lint_issues(&issues, &ListFormat::Csv).unwrap();

    assert!(csv.contains("coverage-score"));
    assert!(csv.contains("stage-readiness"));
    assert!(csv.contains("naming-id"));
    assert!(csv.contains("api-contract-incomplete"));
    assert!(csv.contains("field-unmapped"));
    assert!(csv.contains("naming-column"));
    assert!(csv.contains("rdra-actor-no-buc"));
    assert!(csv.contains("rdra-entity-no-buc"));
    assert!(!csv.contains("orphan-node,actor"));
    let json = format_lint_issues(&issues, &ListFormat::Json).unwrap();
    assert!(json.contains("\"rdra-actor-no-buc\""));
}

#[test]
fn fmt_canonicalizes_source_and_preserves_parseability() {
    let src = r#"module shop.checkout
import shared.actors.{Customer as Buyer, Staff}
requirement ReqCheckout "Checkout reliable" priority "must" source "Interview"
adr AdrOutbox "Use outbox" adr_status accepted decision "Use transactional outbox." reason "Avoid synchronous callbacks."
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {customer_id:Int note:String @null}
invariant(Order).when(status == paid).then(total > 0)
"#;

    let formatted = rdra_ish_syntax::format_source(src).unwrap();

    assert_eq!(
        formatted,
        r#"module shop.checkout

import shared.actors.{Customer as Buyer, Staff}

requirement ReqCheckout "Checkout reliable"
  priority "must"
  source "Interview"

adr AdrOutbox "Use outbox"
  adr_status accepted
  decision "Use transactional outbox."
  reason "Avoid synchronous callbacks."

api CreateOrder "Create order"
  method POST
  path "/orders"
  auth bearer

dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}

invariant(Order).when(status == paid).then(total > 0)
"#
    );

    let (_ast, errors) = rdra_ish_syntax::parse(&formatted);
    assert!(
        errors.is_empty(),
        "formatted output should parse: {errors:?}"
    );
}

#[test]
fn list_requirement_outputs_metadata() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority "must"
  source "Customer interview"
  source "Support tickets"
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
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Requirement, &ListFormat::Csv).unwrap();
    assert!(csv.contains(
        "id,label,priority,sources,stakeholders,owner,acceptance_criteria,status,risk,rationale,description"
    ));
    assert!(csv.contains(
        "ReqCheckout,Checkout must be reliable,must,Customer interview|Support tickets,Store Operations,Product Owner,A payment timeout leaves the cart recoverable.,proposed,high,Checkout failures directly block revenue.,The checkout flow must preserve customer intent."
    ));
}

#[test]
fn list_adr_outputs_decisions_and_impacts() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
adr AdrOutbox "Use transactional outbox"
  description "Decision record for external event publication."
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."
system CustomerSystem "Customer System"
entity Customer "Customer" { id: Int @pk }
decides(AdrOutbox, CustomerSystem)
decides(AdrOutbox, Customer)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let adr_csv = list_elements(&model, &ListKind::Adr, &ListFormat::Csv).unwrap();
    assert!(adr_csv.contains(
        "id,label,status,context,decision,consequences,accepted_options,rejected_options,reasons,target_kinds,target_ids,description"
    ));
    assert!(adr_csv.contains("AdrOutbox,Use transactional outbox,accepted"));
    assert!(adr_csv.contains("entity|system"));
    assert!(adr_csv.contains("Customer|CustomerSystem"));

    let impact_csv = list_elements(&model, &ListKind::AdrImpact, &ListFormat::Csv).unwrap();
    assert!(impact_csv.contains("adr_id,adr_label,adr_status,target_kind,target_id,target_label"));
    assert!(
        impact_csv.contains("AdrOutbox,Use transactional outbox,accepted,entity,Customer,Customer")
    );
    assert!(impact_csv.contains(
        "AdrOutbox,Use transactional outbox,accepted,system,CustomerSystem,Customer System"
    ));
}

#[test]
fn list_usecase_outputs_conditions_and_alternatives() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
usecase CapturePayment "Capture payment"
  description "Captures authorized payment."
  precondition "Order is authorized."
  guard "Provider is available."
  postcondition "Payment is captured."
  alternative "Customer changes payment method."
  error "Authorization expires."
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Usecase, &ListFormat::Csv).unwrap();
    assert!(csv
        .contains("id,label,preconditions,guards,postconditions,alternatives,errors,description"));
    assert!(csv.contains(
        "CapturePayment,Capture payment,Order is authorized.,Provider is available.,Payment is captured.,Customer changes payment method.,Authorization expires.,Captures authorized payment."
    ));
}

#[test]
fn export_openapi_projects_api_contracts() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {
  customer_id: Int
}
dto OrderResponse "Order response" {
  order_id: Int
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let (json, ext) = export_artifact(&model, &ExportKind::Openapi, &View::whole()).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(ext, "openapi.json");
    assert_eq!(doc["openapi"], "3.0.3");
    assert_eq!(
        doc["paths"]["/orders"]["post"]["operationId"],
        "CreateOrder"
    );
    assert_eq!(
        doc["paths"]["/orders"]["post"]["security"],
        serde_json::json!([{ "bearer": [] }])
    );
    assert_eq!(
        doc["components"]["schemas"]["CreateOrderRequest"]["required"],
        serde_json::json!(["customer_id"])
    );
}

#[test]
fn export_dbml_projects_logical_data_model() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" { id: Int @pk  status: Enum(pending, paid) }
relate(Order, Customer, N:1).on_delete(cascade)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let (dbml, ext) = export_artifact(&model, &ExportKind::Dbml, &View::whole()).unwrap();

    assert_eq!(ext, "schema.dbml");
    assert!(dbml.contains("Table Customer"));
    assert!(dbml.contains("Enum Order_status"));
    assert!(dbml.contains("Ref: Order.customer_id > Customer.id [delete: cascade]"));
}

#[test]
fn export_asyncapi_projects_event_catalog() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
usecase SignEncounter "Sign encounter"
event EncounterSigned "Encounter signed"
raises(SignEncounter, EncounterSigned)
outbox(EncounterSigned)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let (json, ext) = export_artifact(&model, &ExportKind::Asyncapi, &View::whole()).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(ext, "asyncapi.json");
    assert_eq!(doc["asyncapi"], "3.1.0");
    assert_eq!(
        doc["operations"]["publishEncounterSigned"]["action"],
        "send"
    );
    assert_eq!(
        doc["components"]["messages"]["EncounterSigned"]["x-rdra-ish-outbox"],
        true
    );
}

#[test]
fn export_json_schema_projects_dtos_and_entities() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid)
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let (json, ext) = export_artifact(&model, &ExportKind::JsonSchema, &View::whole()).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(ext, "json-schema.json");
    assert_eq!(
        doc["$defs"]["Dto.CreateOrderRequest"]["x-rdra-ish-kind"],
        "dto"
    );
    assert_eq!(doc["$defs"]["Entity.Order"]["x-rdra-ish-kind"], "entity");
}

#[test]
fn export_er_text_formats_project_logical_data_model() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" { id: Int @pk }
relate(Order, Customer, N:1)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let (mermaid, mermaid_ext) =
        export_artifact(&model, &ExportKind::MermaidEr, &View::whole()).unwrap();
    let (plantuml, plantuml_ext) =
        export_artifact(&model, &ExportKind::PlantumlEr, &View::whole()).unwrap();

    assert_eq!(mermaid_ext, "er.mmd");
    assert!(mermaid.contains("erDiagram"));
    assert!(mermaid.contains("Order }o--|| Customer"));
    assert_eq!(plantuml_ext, "er.puml");
    assert!(plantuml.contains("@startuml"));
    assert!(plantuml.contains("Order }o--|| Customer"));
}

#[test]
fn list_dto_outputs_fields() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Dto, &ListFormat::Csv).unwrap();
    assert!(csv.contains("dto_id,dto_label,field_name,field_type,required"));
    assert!(csv.contains("CreateOrderRequest,Create order request,customer_id,Int,true"));
    assert!(csv.contains("CreateOrderRequest,Create order request,note,String,false"));
}

#[test]
fn list_field_outputs_ui_metadata_and_entity_column_mapping() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
screen CheckoutScreen "Checkout screen"
field ShippingAddress "Shipping address" access editable required true source actor
entity Order "Order" {
  id: Int @pk
  shipping_address: String
}
contains(CheckoutScreen, ShippingAddress)
maps_field(ShippingAddress, Order, "shipping_address")
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Field, &ListFormat::Csv).unwrap();
    assert!(csv.contains("field_id,field_label,access,required,source,entity_id,column_name"));
    assert!(
        csv.contains("ShippingAddress,Shipping address,editable,true,actor,Order,shipping_address")
    );
}

#[test]
fn list_entity_outputs_data_modeling_metadata() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" {
  id: Int @pk
  tenant_id: Int @tenant
  total: Money @check(total >= 0)
  discount: Money
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  net_total: Money @derived(total - discount)
}
relate(Order, Customer, N:1).optional().on_delete(set_null).on_update(cascade)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Entity, &ListFormat::Csv).unwrap();
    assert!(csv.contains("fk_optional,fk_on_delete,fk_on_update"));
    assert!(csv.contains("tenant_id,Int,false,false,false,false,,false,,,false,,"));
    assert!(csv.contains("total,Money,false,false,false,false,,false,,,false,,total >= 0"));
    assert!(csv.contains("deleted_at,DateTime,false,false,false,false,,false,,,true,,"));
    assert!(csv.contains("valid_from,DateTime,false,false,false,false,,false,,,false,,"));
    assert!(csv.contains("net_total,Money,false,false,false,false,,false,,,false,,"));
    assert!(
        csv.contains("customer_id,Int,false,false,false,true,Customer,true,set_null,cascade,true")
    );
    assert!(csv.contains("total - discount"));
}

#[test]
fn list_nfr_outputs_operational_metadata() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Nfr, &ListFormat::Csv).unwrap();
    assert!(csv.contains("id,label,metric,target,window,slo"));
    assert!(csv.contains("CheckoutLatency,Checkout latency,p95_latency_ms,<=300,5m,99.9%"));
    assert!(csv.contains("multi_az,retryable"));
}

#[test]
fn list_constraint_outputs_audit_retention_privacy_metadata() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let csv = list_elements(&model, &ListKind::Constraint, &ListFormat::Csv).unwrap();
    assert!(csv.contains("AuditRetention,Audit retention"));
    assert!(csv.contains("enabled,structured,7y,restricted"));
}

#[test]
fn table_list_reports_empty_entity_result() {
    let model = SemanticModel::default();

    let output = list_elements(&model, &ListKind::Entity, &ListFormat::Table).unwrap();

    assert_eq!(output, "No entities found.\n");
}

#[test]
fn table_list_permission_callables() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#;
    let (ast, _) = parse(src);
    let (model, _) = build_model(&ast);

    let output = list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Table).unwrap();

    assert!(output.contains("PERMISSION_ID"));
    assert!(output.contains("ScheduleWrite"));
    assert!(output.contains("BookAppointment"));
    assert!(output.contains("BookingApi"));
    assert!(output.contains("BookAppointment->BookingApi"));

    let json = list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Json).unwrap();
    assert!(json.contains("\"usecase_api_paths\""));
    assert!(json.contains("BookAppointment->BookingApi"));
}

#[test]
fn table_list_reports_empty_permission_callables() {
    let model = SemanticModel::default();

    let output = list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Table).unwrap();

    assert_eq!(output, "No permissions found.\n");
}

#[test]
fn table_list_actor_permission_audit() {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    let src = r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission LegacyAdmin "Legacy Admin"
performs(Staff, BookAppointment)
has_permission(Staff, LegacyAdmin)
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#;
    let (ast, _) = parse(src);
    let (model, _) = build_model(&ast);

    let output =
        list_elements(&model, &ListKind::ActorPermissionAudit, &ListFormat::Table).unwrap();

    assert!(output.contains("ACTOR_ID"));
    assert!(output.contains("LegacyAdmin"));
    assert!(output.contains("excess"));
    assert!(output.contains("ScheduleWrite"));
    assert!(output.contains("missing"));
    assert!(output.contains("BookAppointment->BookingApi"));
}

#[test]
fn consistency_warnings_include_permission_and_state_findings() {
    use rdra_ish_core::consistency_diagnostics;

    let src = r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(draft, booked) @default(draft)
}
performs(Staff, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#;
    let dir = std::env::temp_dir().join(format!("rdra-ish-cli-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("consistency.rdra");
    std::fs::write(&file, src).unwrap();
    let (program, model, diags) = load_model(&[file]).expect("load model");
    assert!(diags.iter().all(|diag| diag.is_warning));

    let warnings: Vec<_> = consistency_diagnostics(&model)
        .into_iter()
        .map(|d| d.error.to_string())
        .collect();
    let state_findings = state_consistency_findings(&program, &model);

    assert!(warnings
        .iter()
        .any(|warning| warning.contains("actor 'Staff' is missing permission 'ScheduleWrite'")));
    let printed: Vec<String> = state_findings.iter().map(ToString::to_string).collect();
    assert!(printed.iter().any(|line| line.starts_with(
        "info[NoCreationPath_Appointment]: state derivation for entity 'Appointment': no creates path"
    )));
    assert!(printed.iter().any(|line| line.starts_with(
        "warning[UnreachableEnumVariant_Appointment_status_booked]: state derivation for entity 'Appointment': enum variant 'status.booked' is unreachable"
    )));
}

#[test]
fn state_diag_message_formats_invariant_violation() {
    let message = state_diag_message(&StateDiag::ConstraintViolated {
        name: "Inv0".to_string(),
        kind: InvKind::Invariant,
        text: InvText::WhenThen {
            guards: "status=booked".to_string(),
            requireds: "booked_at=present".to_string(),
        },
        pattern_desc: "status=booked, booked_at=null".to_string(),
        site: Site::bare(ModelRef::EntityInvariant(0)),
        trace: Trace::default(),
    });

    assert_eq!(
        message,
        "invariant violated: when status=booked then booked_at=present is broken by status=booked, booked_at=null"
    );
}

#[test]
fn state_diag_message_formats_cross_invariant_violation() {
    let message = state_diag_message(&StateDiag::ConstraintViolated {
        name: "Cross0".to_string(),
        kind: InvKind::CrossInvariant,
        text: InvText::CrossWhenThen {
            entities: "Order, Payment".to_string(),
            guards: "Order.status=paid".to_string(),
            requireds: "Payment.status=captured".to_string(),
        },
        pattern_desc: "Order(status=paid); Payment(status=pending)".to_string(),
        site: Site::bare(ModelRef::CrossInvariant(0)),
        trace: Trace::default(),
    });

    assert_eq!(
        message,
        "multi-entity invariant violated across [Order, Payment]: when Order.status=paid then Payment.status=captured is broken by Order(status=paid); Payment(status=pending)"
    );
}

#[test]
fn state_diag_message_formats_temporal_assertion_violation() {
    let message = state_diag_message(&StateDiag::AfterAssertViolated {
        name: "AfterAssert_ExecuteCertIssue_0".to_string(),
        anchor: "ExecuteCertIssue".to_string(),
        requireds: "CertificateOrder.status=executed".to_string(),
        actual: "CertificateOrder(status=pending)".to_string(),
        site: Site::bare(ModelRef::TemporalAssertion(0)),
        trace: Trace::default(),
    });

    assert_eq!(
        message,
        "temporal assertion violated after 'ExecuteCertIssue': expected CertificateOrder.status=executed, but found CertificateOrder(status=pending)"
    );
}

#[test]
fn state_diag_message_formats_quantifier_violation() {
    let message = state_diag_message(&StateDiag::ConstraintViolated {
        name: "Quant0".to_string(),
        kind: InvKind::Quantifier,
        text: InvText::Quantifier {
            anchor: "ClientCertificate".to_string(),
            related: "TerminalCertAssignment".to_string(),
            constraint: "ClientCertificate when (status=revoked) none TerminalCertAssignment where (status=active)".to_string(),
        },
        pattern_desc: "ClientCertificate(status=revoked); TerminalCertAssignment(status=active)"
            .to_string(),
        site: Site::bare(ModelRef::Quantifier(0)),
        trace: Trace::default(),
    });

    assert!(message.starts_with(
        "to-many quantifier constraint is violated from 'ClientCertificate' to 'TerminalCertAssignment'"
    ));
    assert!(message.ends_with(
        "witnessed by ClientCertificate(status=revoked); TerminalCertAssignment(status=active)"
    ));
}

#[test]
fn clinic_ops_attribute_columns_are_variations_not_unreachable_states() {
    let inputs = vec![errors_fixture("samples/clinic-ops")];
    let (program, model, diags) = load_model(&inputs).expect("load clinic-ops");
    assert!(
        diags.iter().all(|d| d.is_warning),
        "clinic-ops must load without errors: {diags:?}"
    );

    let findings = state_consistency_findings(&program, &model);
    let unreachable_method: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("'method'") && f.message.contains("unreachable"))
        .collect();
    assert!(
        unreachable_method.is_empty(),
        "PaymentTransaction.method is chosen by an actor: {unreachable_method:?}"
    );

    let lint = lint_issues(&model, &diags);
    let enum_attributes: Vec<_> = lint
        .iter()
        .filter(|issue| issue.rule == "enum-attribute")
        .map(|issue| issue.message.as_str())
        .collect();
    assert!(enum_attributes.is_empty(), "{enum_attributes:?}");
}

#[test]
fn clinic_ops_conditions_list_their_body_and_derived_links() {
    let inputs = vec![errors_fixture("samples/clinic-ops")];
    let (_, model, diags) = load_model(&inputs).expect("load clinic-ops");
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");

    let csv = list_elements(&model, &ListKind::Condition, &ListFormat::Csv).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next(),
        Some("id,label,body,variations,state_models,applies_to")
    );
    let rows: Vec<&str> = lines.collect();
    assert_eq!(
        rows,
        vec![
            "BookedPatient,予約済みの患者,Appointment.status == apptscheduled,,Appointment.status,BatchVerifyEligibility",
            "CopayCappedAtLimit,窓口負担は負担割合で計算し、限度額情報があれば限度額までとする,,BurdenRatio|LimitCategory,Appointment.status,CalculateVisitBill",
            "OrderConfirmedByPhysician,医師が確定したオーダ,\"ClinicalOrder.physician_confirmed == true, ClinicalOrder.status == clordered\",,ClinicalOrder.status,CollectSpecimen",
            "TransferNotPossible,振替・分割の対象外（負担割合の相違・公費）,,PayerChange,ClaimStatement.status,RecordClaimReturn",
            "UnreferredFirstVisit,紹介状なしの初診で、除外に当たらない,\"Charge.kind == selectedtreatment, Appointment.referral == unreferred\",ChargeKind|ReferralCategory,,ApplySelectedTreatmentFee",
        ]
    );

    let lint = lint_issues(&model, &diags);
    let condition_findings: Vec<_> = lint
        .iter()
        .filter(|issue| issue.subject_kind == "condition")
        .map(|issue| (issue.rule, issue.subject_id.as_str()))
        .collect();
    assert!(condition_findings.is_empty(), "{condition_findings:?}");
}
