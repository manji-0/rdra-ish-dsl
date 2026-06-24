use super::support::*;
use crate::analysis::build_model;
use crate::analysis::instance::register_instance;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;
use rdra_ish_syntax::parse;

#[test]
fn entity_block_comment_does_not_drop_following_columns() {
    let src = r#"
usecase ActivateExample "Activate example"

entity Example "Example" {
  id: Int @pk
  // Comment between columns should not end the entity body.
  status: Enum(active, inactive)
}

sets(ActivateExample, Example, "status", "active")
"#;

    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "model errors: {errors:?}");

    let example = model
        .entities
        .iter()
        .find_map(|(_, entity)| (entity.id == "Example").then_some(entity))
        .expect("Example entity should be registered");

    let column_names: Vec<_> = example
        .columns
        .iter()
        .map(|col| col.name.as_str())
        .collect();
    assert_eq!(column_names, vec!["id", "status"]);
}

#[test]
fn register_instance_populates_each_node_store_and_symbol_table() {
    let mut model = SemanticModel::default();
    let mut diags = Vec::new();

    let cases = [
        (Kind::Actor, "ActorA"),
        (Kind::ExtSystem, "ExtA"),
        (Kind::System, "SystemA"),
        (Kind::Requirement, "ReqA"),
        (Kind::Adr, "AdrA"),
        (Kind::Nfr, "NfrA"),
        (Kind::Quality, "QualityA"),
        (Kind::Constraint, "ConstraintA"),
        (Kind::Concept, "ConceptA"),
        (Kind::DomainObject, "DomainObjectA"),
        (Kind::Aggregate, "AggregateA"),
        (Kind::ValueObject, "ValueObjectA"),
        (Kind::Business, "BusinessA"),
        (Kind::Buc, "BucA"),
        (Kind::Flow, "FlowA"),
        (Kind::Step, "StepA"),
        (Kind::UsageScene, "SceneA"),
        (Kind::UseCase, "UsecaseA"),
        (Kind::Screen, "ScreenA"),
        (Kind::Field, "FieldA"),
        (Kind::Event, "EventA"),
        (Kind::State, "StateA"),
        (Kind::Condition, "ConditionA"),
        (Kind::Variation, "VariationA"),
        (Kind::Api, "ApiA"),
        (Kind::Dto, "DtoA"),
        (Kind::Location, "LocationA"),
        (Kind::Timing, "TimingA"),
        (Kind::Medium, "MediumA"),
        (Kind::Permission, "PermissionA"),
    ];

    for (kind, id) in &cases {
        register_instance(
            &mut model,
            &instance(kind.clone(), id),
            DiagCtxt::new(0),
            &mut diags,
        );
    }

    let entity_inst = InstanceDecl {
        kind: Kind::Entity,
        id: "EntityA".to_string(),
        label: "EntityA label".to_string(),
        description: Some("EntityA description".to_string()),
        requirement: RequirementMetadata::default(),
        adr: AdrMetadata::default(),
        api: ApiMetadata::default(),
        nfr: NfrMetadata::default(),
        field: FieldMetadata::default(),
        usecase: UseCaseMetadata::default(),
        columns: vec![Column {
            name: "id".to_string(),
            col_type: ColType::Int,
            annotations: vec![Annotation::Pk],
            span: 0..0,
        }],
        span: 0..0,
    };
    register_instance(&mut model, &entity_inst, DiagCtxt::new(0), &mut diags);

    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(model.actors.len(), 1);
    assert_eq!(model.ext_systems.len(), 1);
    assert_eq!(model.systems.len(), 1);
    assert_eq!(model.requirements.len(), 1);
    assert_eq!(model.nfrs.len(), 1);
    assert_eq!(model.qualities.len(), 1);
    assert_eq!(model.constraints.len(), 1);
    assert_eq!(model.concepts.len(), 1);
    assert_eq!(model.domain_objects.len(), 1);
    assert_eq!(model.aggregates.len(), 1);
    assert_eq!(model.value_objects.len(), 1);
    assert_eq!(model.businesses.len(), 1);
    assert_eq!(model.bucs.len(), 1);
    assert_eq!(model.usage_scenes.len(), 1);
    assert_eq!(model.use_cases.len(), 1);
    assert_eq!(model.screens.len(), 1);
    assert_eq!(model.fields.len(), 1);
    assert_eq!(model.events.len(), 1);
    assert_eq!(model.entities.len(), 1);
    assert_eq!(model.states.len(), 1);
    assert_eq!(model.conditions.len(), 1);
    assert_eq!(model.variations.len(), 1);
    assert_eq!(model.apis.len(), 1);
    assert_eq!(model.dtos.len(), 1);
    assert_eq!(model.locations.len(), 1);
    assert_eq!(model.timings.len(), 1);
    assert_eq!(model.media.len(), 1);
    assert_eq!(model.permissions.len(), 1);

    let entity = model.entities.values().next().unwrap();
    assert_eq!(entity.columns.len(), 1);
    assert!(entity.columns[0].is_pk);

    for (kind, id) in &cases {
        assert!(
            model.symbols.lookup_qualified(kind, id).is_some(),
            "{id} should be present in symbol table"
        );
    }
    assert!(model
        .symbols
        .lookup_qualified(&Kind::Entity, "EntityA")
        .is_some());
}

#[test]
fn build_model_registers_screen_fields_and_column_mappings() {
    let src = r#"
screen CheckoutScreen "Checkout screen"
field ShippingAddress "Shipping address" access editable required true source actor
field OrderTotal "Order total" access readonly required true source system
entity Order "Order" {
  id: Int @pk
  shipping_address: String
  total: Money
}
contains(CheckoutScreen, ShippingAddress)
contains(CheckoutScreen, OrderTotal)
maps_field(ShippingAddress, Order, "shipping_address")
maps_field(OrderTotal, Order, "total")
"#;

    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    assert_eq!(model.fields.len(), 2);
    let shipping = model
        .fields
        .iter()
        .find_map(|(_, field)| (field.id == "ShippingAddress").then_some(field))
        .expect("ShippingAddress field should be registered");
    assert_eq!(shipping.access.as_deref(), Some("editable"));
    assert_eq!(shipping.required, Some(true));
    assert_eq!(shipping.source.as_deref(), Some("actor"));
    assert_eq!(model.field_mappings.len(), 2);
    assert!(model
        .relations
        .iter()
        .any(|rel| matches!(rel.kind, RelKind::MapsField)));
}

#[test]
fn register_instance_reports_duplicate_same_kind_but_keeps_cross_kind_names() {
    let mut model = SemanticModel::default();
    let mut diags = Vec::new();

    register_instance(
        &mut model,
        &instance(Kind::Actor, "Same"),
        DiagCtxt::new(0),
        &mut diags,
    );
    register_instance(
        &mut model,
        &instance(Kind::UseCase, "Same"),
        DiagCtxt::new(0),
        &mut diags,
    );
    register_instance(
        &mut model,
        &instance(Kind::Actor, "Same"),
        DiagCtxt::new(0),
        &mut diags,
    );

    assert_eq!(model.actors.len(), 2);
    assert_eq!(model.use_cases.len(), 1);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].error.to_string().contains("duplicate definition"));
    assert!(model
        .symbols
        .lookup_qualified(&Kind::Actor, "Same")
        .is_some());
    assert!(model
        .symbols
        .lookup_qualified(&Kind::UseCase, "Same")
        .is_some());
}
