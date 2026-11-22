use super::support::*;
use crate::analysis::build_model;
use crate::analysis::instance::register_instance;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::RdraError;
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

sets(ActivateExample, Example.status == active)
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
        (Kind::Business, "BusinessA"),
        (Kind::Buc, "BucA"),
        (Kind::Flow, "FlowA"),
        (Kind::Step, "StepA"),
        (Kind::UseCase, "UsecaseA"),
        (Kind::Screen, "ScreenA"),
        (Kind::Field, "FieldA"),
        (Kind::Event, "EventA"),
        (Kind::Api, "ApiA"),
        (Kind::Dto, "DtoA"),
        (Kind::Timer, "TimingA"),
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
        table: None,
        rows: None,
        bare_values: Vec::new(),
        columns: vec![Column {
            name: "id".to_string(),
            col_type: ColType::Int,
            type_args: Vec::new(),
            annotations: vec![Annotation::Pk],
            span: 0..0,
        }],
        variation_values: Vec::new(),
        span: 0..0,
    };
    register_instance(&mut model, &entity_inst, DiagCtxt::new(0), &mut diags);

    let unexpected = &diags;
    assert!(
        unexpected.is_empty(),
        "unexpected diagnostics: {unexpected:?}"
    );
    assert_eq!(model.actors.len(), 1);
    assert_eq!(model.ext_systems.len(), 1);
    assert_eq!(model.systems.len(), 1);
    assert_eq!(model.requirements.len(), 1);
    assert_eq!(model.nfrs.len(), 1);
    assert_eq!(model.qualities.len(), 1);
    assert_eq!(model.constraints.len(), 1);
    assert_eq!(model.businesses.len(), 1);
    assert_eq!(model.bucs.len(), 1);
    assert_eq!(model.use_cases.len(), 1);
    assert_eq!(model.screens.len(), 1);
    assert_eq!(model.fields.len(), 1);
    assert_eq!(model.events.len(), 1);
    assert_eq!(model.entities.len(), 1);
    assert_eq!(model.apis.len(), 1);
    assert_eq!(model.dtos.len(), 1);
    assert_eq!(model.timers.len(), 1);
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
maps_field(ShippingAddress, Order.shipping_address)
maps_field(OrderTotal, Order.total)
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
fn maps_field_checks_the_entity_and_the_column_it_names() {
    let base = "field F \"F\"\nentity Order \"Order\" {\n  id: Int @pk\n  total: Money\n}\n";
    let errors = |line: &str| {
        let (ast, parse_errors) = parse(&format!("{base}{line}\n"));
        assert!(parse_errors.is_empty(), "{line}: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(model.field_mappings.is_empty(), "{line}");
        diags.into_iter().map(|d| d.error).collect::<Vec<_>>()
    };
    assert!(matches!(
        errors("maps_field(F, Order.ghost)").as_slice(),
        [RdraError::UnknownColumn { entity, col }] if entity == "Order" && col == "ghost"
    ));
    assert!(matches!(
        errors("maps_field(F, Ghost.total)").as_slice(),
        [RdraError::UndefinedSymbol { id, .. }] if id == "Ghost"
    ));
    assert!(matches!(
        errors("maps_field(F, total)").as_slice(),
        [RdraError::TypeMismatch { pred, expected, .. }] if pred == "maps_field" && expected == "Entity.column"
    ));
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

#[test]
fn duplicate_entity_columns_are_errors() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid)
  status: Bool
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (_model, diags) = build_model(&ast);
    assert!(
        diags.iter().any(|d| matches!(
            &d.error,
            crate::diagnostics::RdraError::DuplicateColumn { col, .. } if col == "status"
        )),
        "expected DuplicateColumn, got: {:?}",
        diags.iter().map(|d| &d.error).collect::<Vec<_>>()
    );
}

#[test]
fn composite_pk_marks_all_key_columns() {
    let src = r#"
entity Line "明細" {
  order_id: Int @pk(order_id, line_no)
  line_no: Int
  qty: Int
}
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    let entity = model.entities.values().next().unwrap();
    assert_eq!(entity.primary_key, vec!["order_id", "line_no"]);
    assert!(entity
        .columns
        .iter()
        .any(|c| c.name == "order_id" && c.is_pk));
    assert!(entity
        .columns
        .iter()
        .any(|c| c.name == "line_no" && c.is_pk));
    assert!(entity.columns.iter().any(|c| c.name == "qty" && !c.is_pk));
}

fn end_values_of(src: &str) -> (Vec<String>, Vec<String>) {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    let ends = model
        .entities
        .iter()
        .flat_map(|(_, e)| e.columns.iter())
        .flat_map(|c| c.end_values.clone())
        .collect();
    (ends, errors)
}

#[test]
fn end_names_values_of_an_enum_or_variation_column() {
    let (ends, errors) = end_values_of(
        "entity E \"E\" {\n  status: Enum(free, used) @end(free, free)\n  kind: K @end(b)\n}\nvariation K \"K\" {\n  a\n  b\n}\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(ends, ["free", "b"]);
}

#[test]
fn end_rejects_an_unknown_value_and_a_column_without_states() {
    let (_, errors) = end_values_of("entity E \"E\" {\n  status: Enum(free, used) @end(gone)\n}\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0]
            .starts_with("`@end(gone)` on column 'status' of 'E': 'gone' is not one of its values"),
        "{errors:?}"
    );
    assert!(
        errors[0].contains("declared values are: free, used"),
        "{errors:?}"
    );

    let (_, errors) = end_values_of("entity E \"E\" {\n  n: Int @end(free)\n}\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("only an `Enum` or variation column"),
        "{errors:?}"
    );
}

#[test]
fn global_and_along_on_one_rule_is_an_error() {
    let src = r#"
entity A "A" { id: Int @pk  s: Enum(x, y) }
entity B "B" { id: Int @pk  t: Enum(x, y) }
relate(B, A, N:1)
forbidden(A, B, A.s == x, B.t == y).along(A, B).global()
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(errors[0].error, RdraError::GlobalWithAlong { .. }),
        "{errors:?}"
    );
}

fn column_of(src: &str, name: &str) -> (Option<ModelColumn>, Vec<String>) {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    let col = model
        .entities
        .iter()
        .flat_map(|(_, e)| e.columns.iter())
        .find(|c| c.name == name)
        .cloned();
    (col, errors)
}

#[test]
fn type_arguments_set_the_column_detail() {
    let src = "entity E \"E\" {\n  id: String @pk @generated(uuid)\n  name: String(255)\n  total: Decimal(10, 2)\n  fee: Money(12)\n  at: DateTime(local) @default(now()) @on_update(now())\n}\nentity C \"C\" { id: Int @pk }\nrelate(C, E, N:1)\n";
    let (name, errors) = column_of(src, "name");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(name.unwrap().detail.length, Some(255));
    let total = column_of(src, "total").0.unwrap().detail;
    assert_eq!((total.precision, total.scale), (Some(10), Some(2)));
    let fee = column_of(src, "fee").0.unwrap().detail;
    assert_eq!((fee.precision, fee.scale), (Some(12), Some(0)));
    let at = column_of(src, "at").0.unwrap();
    assert_eq!(at.detail.time_zone, Some(false));
    assert_eq!(at.default_fn, Some(DefaultFn::Now));
    assert_eq!(at.on_update, Some(DefaultFn::Now));
    assert_eq!(
        column_of(src, "id").0.unwrap().generated,
        Some(IdGeneration::Uuid)
    );
}

#[test]
fn a_type_argument_or_function_the_column_cannot_take_is_an_error() {
    for (col, want) in [
        (
            "x: Int(4)",
            "only `String`, `Decimal`, `Money`, and `DateTime` take arguments",
        ),
        ("x: String(0)", "positive length"),
        ("x: Decimal(2, 3)", "no larger than the precision"),
        ("x: DateTime(utc)", "`DateTime` takes `tz` or `local`"),
        (
            "x: Int @default(now())",
            "takes a `Date` or `DateTime` column",
        ),
        ("x: String @on_update(uuid())", "takes `now()`"),
        (
            "x: String @pk @generated(identity)",
            "needs an `Int` column",
        ),
        ("x: Int @pk @generated(serial)", "is not one of"),
    ] {
        let (_, errors) = column_of(&format!("entity E \"E\" {{\n  {col}\n}}\n"), "x");
        assert_eq!(errors.len(), 1, "{col}: {errors:?}");
        assert!(errors[0].contains(want), "{col}: {errors:?}");
    }
}

fn model_errors(src: &str) -> (SemanticModel, Vec<String>) {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    (model, errors)
}

#[test]
fn references_needs_a_unique_set_of_the_parent() {
    let src = |sku: &str| {
        format!("entity P \"P\" {{\n  id: Int @pk\n  {sku}\n}}\nentity C \"C\" {{ id: Int @pk }}\nrelate(C, P, N:1).references(sku)\n")
    };
    let (model, errors) = model_errors(&src("sku: String @unique"));
    assert!(errors.is_empty(), "{errors:?}");
    let fk = model.foreign_keys().next().unwrap().1;
    assert_eq!(fk.columns, [("p_sku".to_string(), "sku".to_string())]);
    let (_, errors) = model_errors(&src("sku: String"));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("`.references(sku)` names columns of 'P'"),
        "{errors:?}"
    );
}

#[test]
fn table_names_the_entity_and_version_is_one_int_column() {
    let (model, errors) =
        model_errors("entity E \"E\" table \"es\" {\n  id: Int @pk\n  v: Int @version\n}\n");
    assert!(errors.is_empty(), "{errors:?}");
    let e = model.entities.values().next().unwrap();
    assert_eq!(e.table_name(), "es");
    assert!(e.columns.iter().any(|c| c.name == "v" && c.is_version));
    for (cols, want) in [
        ("v: String @version", "needs an `Int` column"),
        (
            "v: Int @version\n  w: Int @version",
            "is the second on the entity, after 'v'",
        ),
    ] {
        let (_, errors) =
            model_errors(&format!("entity E \"E\" {{\n  id: Int @pk\n  {cols}\n}}\n"));
        assert_eq!(errors.len(), 1, "{cols}: {errors:?}");
        assert!(errors[0].contains(want), "{cols}: {errors:?}");
    }
}
