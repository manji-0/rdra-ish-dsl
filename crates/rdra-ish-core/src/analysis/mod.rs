use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::{DiagCtxt, SourceId};
use crate::model::*;
use rdra_ish_syntax::ast::*;
mod arg_resolve;
mod comparison;
mod constraint;
mod effect;
mod instance;
mod metadata;
mod nodes;
mod predicate_process;
mod qref_util;

use instance::register_instance;
use predicate_process::process_predicate;

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let items: Vec<(SourceId, Item)> = ast.items.iter().cloned().map(|item| (0, item)).collect();
    build_model_items(&items)
}

pub fn build_model_items(items: &[(SourceId, Item)]) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel::default();
    let mut diags: Vec<Diagnostic> = vec![];

    for (source_id, item) in items {
        if let Item::Instance(inst) = item {
            register_instance(&mut model, inst, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    for (source_id, item) in items {
        if let Item::Predicate(pred) = item {
            process_predicate(&mut model, pred, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    generate_fks(&mut model, &mut diags);

    (model, diags)
}

fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let rels: Vec<_> = model
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelKind::RelateManyToOne | RelKind::RelateOneToMany))
        .map(|r| {
            (
                r.from.clone(),
                r.to.clone(),
                r.kind.clone(),
                r.options.clone(),
            )
        })
        .collect();

    for (from, to, kind, options) in rels {
        let (many_key, one_key) = match kind {
            RelKind::RelateManyToOne => {
                if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (&from, &to) {
                    (*fk, *tk)
                } else {
                    continue;
                }
            }
            RelKind::RelateOneToMany => {
                if let (NodeRef::Entity(ok), NodeRef::Entity(mk)) = (&from, &to) {
                    (*mk, *ok)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let (one_id, pk_type) = {
            let one = &model.entities[one_key];
            let pk = one.columns.iter().find(|c| c.is_pk);
            match pk {
                Some(col) => (one.id.clone(), col.col_type.clone()),
                None => {
                    push_entity_error(
                        model,
                        diags,
                        &one.id,
                        RdraError::MissingPk {
                            entity: one.id.clone(),
                        },
                    );
                    continue;
                }
            }
        };

        let fk_col_name = format!("{}_id", one_id.to_lowercase());

        let many_entity_id = model.entities[many_key].id.clone();
        if model.entities[many_key]
            .columns
            .iter()
            .any(|c| c.name == fk_col_name)
        {
            push_entity_error(
                model,
                diags,
                &many_entity_id,
                RdraError::FkConflict {
                    entity: many_entity_id.clone(),
                    col: fk_col_name.clone(),
                },
            );
            continue;
        }

        let fk_col = ModelColumn {
            name: fk_col_name,
            col_type: pk_type,
            is_pk: false,
            is_unique: false,
            is_indexed: false,
            is_nullable: options.optional,
            default_val: None,
            label: None,
            is_fk: true,
            fk_target: Some(one_id),
            fk_optional: options.optional,
            fk_on_delete: options.on_delete,
            fk_on_update: options.on_update,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
        };
        model.entities[many_key].columns.push(fk_col);
    }
}

#[cfg(test)]
mod tests {
    use super::comparison::{resolve_comparison, to_model_op, type_category};
    use super::constraint::{
        add_condition_entities_to_scope, cross_scope_semantics_from_chain, push_unique_entity,
    };
    use super::instance::register_instance;
    use super::nodes::node_kind_tag_str;
    use super::*;
    use rdra_ish_syntax::parse;

    fn instance(kind: Kind, id: &str) -> InstanceDecl {
        InstanceDecl {
            kind,
            id: id.to_string(),
            label: format!("{id} label"),
            description: Some(format!("{id} description")),
            requirement: RequirementMetadata::default(),
            adr: AdrMetadata::default(),
            api: ApiMetadata::default(),
            nfr: NfrMetadata::default(),
            field: FieldMetadata::default(),
            usecase: UseCaseMetadata::default(),
            columns: Vec::new(),
            span: 0..0,
        }
    }

    fn model_column(name: &str, col_type: ColumnType) -> ModelColumn {
        ModelColumn {
            name: name.to_string(),
            col_type,
            is_pk: false,
            is_unique: false,
            is_indexed: false,
            is_nullable: false,
            default_val: None,
            label: None,
            is_fk: false,
            fk_target: None,
            fk_optional: false,
            fk_on_delete: None,
            fk_on_update: None,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
        }
    }

    fn qref(id: &str) -> QRef {
        QRef {
            kind_qualifier: None,
            parts: vec![id.to_string()],
            span: 0..0,
        }
    }

    fn qcol(entity: &str, column: &str) -> Operand {
        Operand::QualifiedColumn(QualifiedColumnRef {
            entity: qref(entity),
            column: column.to_string(),
            span: 0..0,
        })
    }

    fn entity_key(model: &SemanticModel, id: &str) -> EntityKey {
        model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == id).then_some(key))
            .unwrap()
    }

    fn simple_entity_model(ids: &[&str]) -> SemanticModel {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();
        for id in ids {
            let inst = InstanceDecl {
                kind: Kind::Entity,
                id: (*id).to_string(),
                label: format!("{id} label"),
                description: None,
                requirement: RequirementMetadata::default(),
                adr: AdrMetadata::default(),
                api: ApiMetadata::default(),
                nfr: NfrMetadata::default(),
                field: FieldMetadata::default(),
                usecase: UseCaseMetadata::default(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        col_type: ColType::Int,
                        annotations: vec![Annotation::Pk],
                        span: 0..0,
                    },
                    Column {
                        name: "status".to_string(),
                        col_type: ColType::Enum(vec!["open".to_string(), "closed".to_string()]),
                        annotations: Vec::new(),
                        span: 0..0,
                    },
                    Column {
                        name: "amount".to_string(),
                        col_type: ColType::Decimal,
                        annotations: Vec::new(),
                        span: 0..0,
                    },
                ],
                span: 0..0,
            };
            register_instance(&mut model, &inst, DiagCtxt::new(0), &mut diags);
        }
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        model
    }

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
    fn type_category_groups_comparison_compatible_column_types() {
        assert_eq!(type_category(&ColumnType::Int), "numeric");
        assert_eq!(type_category(&ColumnType::Money), "numeric");
        assert_eq!(type_category(&ColumnType::Decimal), "numeric");
        assert_eq!(type_category(&ColumnType::Date), "temporal");
        assert_eq!(type_category(&ColumnType::DateTime), "temporal");
        assert_eq!(type_category(&ColumnType::String), "equality");
        assert_eq!(type_category(&ColumnType::Bool), "equality");
        assert_eq!(
            type_category(&ColumnType::Enum(vec!["open".to_string()])),
            "equality"
        );
    }

    #[test]
    fn to_model_op_maps_every_ast_comparison_operator() {
        assert_eq!(to_model_op(&CmpOp::Lt), CmpOpModel::Lt);
        assert_eq!(to_model_op(&CmpOp::Gt), CmpOpModel::Gt);
        assert_eq!(to_model_op(&CmpOp::Le), CmpOpModel::Le);
        assert_eq!(to_model_op(&CmpOp::Ge), CmpOpModel::Ge);
        assert_eq!(to_model_op(&CmpOp::Eq), CmpOpModel::Eq);
        assert_eq!(to_model_op(&CmpOp::Ne), CmpOpModel::Ne);
    }

    #[test]
    fn node_kind_tag_str_labels_each_node_ref_kind() {
        let mut model = SemanticModel::default();
        let mut diags = Vec::new();
        let cases = [
            (Kind::Actor, "ActorA", "actor"),
            (Kind::ExtSystem, "ExtA", "extsystem"),
            (Kind::System, "SystemA", "system"),
            (Kind::Requirement, "ReqA", "requirement"),
            (Kind::Adr, "AdrA", "adr"),
            (Kind::Nfr, "NfrA", "nfr"),
            (Kind::Quality, "QualityA", "quality"),
            (Kind::Constraint, "ConstraintA", "constraint"),
            (Kind::Concept, "ConceptA", "concept"),
            (Kind::DomainObject, "DomainObjectA", "domain_object"),
            (Kind::Aggregate, "AggregateA", "aggregate"),
            (Kind::ValueObject, "ValueObjectA", "valueobject"),
            (Kind::Business, "BusinessA", "business"),
            (Kind::Buc, "BucA", "buc"),
            (Kind::Flow, "FlowA", "flow"),
            (Kind::Step, "StepA", "step"),
            (Kind::UsageScene, "SceneA", "usagescene"),
            (Kind::UseCase, "UsecaseA", "usecase"),
            (Kind::Screen, "ScreenA", "screen"),
            (Kind::Field, "FieldA", "field"),
            (Kind::Event, "EventA", "event"),
            (Kind::State, "StateA", "state"),
            (Kind::Condition, "ConditionA", "condition"),
            (Kind::Variation, "VariationA", "variation"),
            (Kind::Api, "ApiA", "api"),
            (Kind::Dto, "DtoA", "dto"),
            (Kind::Location, "LocationA", "location"),
            (Kind::Timing, "TimingA", "timing"),
            (Kind::Medium, "MediumA", "medium"),
            (Kind::Permission, "PermissionA", "permission"),
        ];

        for (kind, id, _) in &cases {
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
            description: None,
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
        for (kind, id, expected) in &cases {
            let node = model.symbols.lookup_qualified(kind, id).unwrap();
            assert_eq!(node_kind_tag_str(node), *expected);
        }
        let entity = model
            .symbols
            .lookup_qualified(&Kind::Entity, "EntityA")
            .unwrap();
        assert_eq!(node_kind_tag_str(entity), "entity");
    }

    #[test]
    fn push_unique_entity_preserves_first_seen_scope_order() {
        let model = simple_entity_model(&["Order", "Payment"]);
        let order = entity_key(&model, "Order");
        let payment = entity_key(&model, "Payment");
        let mut scope = Vec::new();

        push_unique_entity(&mut scope, order);
        push_unique_entity(&mut scope, payment);
        push_unique_entity(&mut scope, order);

        assert_eq!(scope, vec![order, payment]);
    }

    #[test]
    fn add_condition_entities_to_scope_adds_equals_and_comparison_entities_once() {
        let model = simple_entity_model(&["Order", "Payment", "Invoice"]);
        let order = entity_key(&model, "Order");
        let payment = entity_key(&model, "Payment");
        let invoice = entity_key(&model, "Invoice");
        let conditions = vec![
            CrossEntityCondition::Equals {
                column: QualifiedModelColumnRef {
                    entity: order,
                    column: "status".to_string(),
                },
                value: EffectValue::EnumVariant("closed".to_string()),
            },
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs: QualifiedModelColumnRef {
                    entity: payment,
                    column: "amount".to_string(),
                },
                op: CmpOpModel::Gt,
                rhs: CrossCmpRhs::Column(QualifiedModelColumnRef {
                    entity: invoice,
                    column: "amount".to_string(),
                }),
            }),
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs: QualifiedModelColumnRef {
                    entity: order,
                    column: "amount".to_string(),
                },
                op: CmpOpModel::Ge,
                rhs: CrossCmpRhs::IntLit(1),
            }),
        ];
        let mut scope = vec![order];

        add_condition_entities_to_scope(&mut scope, &conditions);

        assert_eq!(scope, vec![order, payment, invoice]);
    }

    #[test]
    fn cross_scope_semantics_from_chain_returns_relation_path_for_along_chain() {
        let model = simple_entity_model(&["Order", "Payment"]);
        let mut diags = Vec::new();
        let pred = PredicateCall {
            name: "cross_invariant".to_string(),
            args: Vec::new(),
            chain: vec![ChainCall {
                name: "along".to_string(),
                args: vec![
                    PredicateArg::Ref(qref("Order")),
                    PredicateArg::Ref(qref("Payment")),
                ],
                span: 0..0,
            }],
            span: 0..0,
        };

        let semantics =
            cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let CrossConstraintScope::RelationPath(path) = semantics else {
            panic!("expected relation path scope");
        };
        assert_eq!(
            path,
            vec![entity_key(&model, "Order"), entity_key(&model, "Payment")]
        );
    }

    #[test]
    fn cross_scope_semantics_from_chain_defaults_to_global_product_without_along() {
        let model = simple_entity_model(&["Order"]);
        let mut diags = Vec::new();
        let pred = PredicateCall {
            name: "cross_forbidden".to_string(),
            args: Vec::new(),
            chain: Vec::new(),
            span: 0..0,
        };

        let semantics =
            cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert!(matches!(semantics, CrossConstraintScope::GlobalProduct));
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

    #[test]
    fn resolve_comparison_accepts_same_entity_qualified_columns_and_literals() {
        let cols = vec![
            model_column("stock", ColumnType::Int),
            model_column("selling", ColumnType::Int),
            model_column("expired_at", ColumnType::DateTime),
        ];
        let mut diags = Vec::new();

        let col_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: qcol("Stock", "stock"),
                op: CmpOp::Lt,
                rhs: qcol("Stock", "selling"),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();
        let int_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("stock".to_string()),
                op: CmpOp::Ge,
                rhs: Operand::IntLit("10".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();
        let now_prop = resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("expired_at".to_string()),
                op: CmpOp::Lt,
                rhs: Operand::Now,
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .unwrap();

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert_eq!(col_prop.axis_key(), "stock<selling");
        assert_eq!(int_prop.rhs, CmpRhs::IntLit(10));
        assert_eq!(now_prop.rhs, CmpRhs::Now);
    }

    #[test]
    fn resolve_comparison_rejects_cross_entity_and_invalid_type_comparisons() {
        let cols = vec![
            model_column("stock", ColumnType::Int),
            model_column("active", ColumnType::Bool),
            model_column("name", ColumnType::String),
        ];
        let mut diags = Vec::new();

        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: qcol("Other", "stock"),
                op: CmpOp::Lt,
                rhs: Operand::Column("stock".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());
        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("active".to_string()),
                op: CmpOp::Lt,
                rhs: Operand::Column("stock".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());
        assert!(resolve_comparison(
            &cols,
            "Stock",
            &Comparison {
                lhs: Operand::Column("name".to_string()),
                op: CmpOp::Eq,
                rhs: Operand::IntLit("1".to_string()),
                span: 0..0,
            },
            DiagCtxt::new(0),
            &mut diags,
        )
        .is_none());

        let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages.iter().any(|msg| msg.contains("type mismatch")),
            "expected cross-entity type mismatch, got {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("order comparison operator")),
            "expected ordered comparison diagnostic, got {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("comparison type mismatch")),
            "expected rhs type mismatch diagnostic, got {messages:?}"
        );
    }

    #[test]
    fn test_build_model_basic() {
        let src = r#"
actor Customer "顧客" description "商品を購入する顧客"
entity Order "注文" description "受注情報" { id: Int @pk }
entity Customer_profile "顧客情報" { id: Int @pk  name: String }
usecase Browse "商品を探す" description "商品一覧を参照する"
performs(Customer, Browse)
relate(Order, Customer_profile, "N:1")
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

        let order = model
            .entities
            .values()
            .find(|e| e.id == "Order")
            .expect("Order entity not found");
        assert_eq!(order.description.as_deref(), Some("受注情報"));

        let fk_col = order
            .columns
            .iter()
            .find(|c| c.name == "customer_profile_id")
            .expect("customer_profile_id FK column not found");

        assert!(fk_col.is_fk);
        assert_eq!(fk_col.fk_target.as_deref(), Some("Customer_profile"));
        assert_eq!(fk_col.col_type, ColumnType::Int);
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
    fn test_build_model_conceptual_elements_and_entity_mapping() {
        let src = r#"
concept PatientIdentity "Patient identity"
concept CarePlan "Care plan"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"
entity AppointmentTable "appointment table" { id: Int @pk  starts_at: DateTime }
contains(SchedulingAggregate, Appointment)
contains(SchedulingAggregate, TimeSlot)
contains(SchedulingAggregate, PatientIdentity)
maps_to(Appointment, AppointmentTable)
maps_to(TimeSlot, AppointmentTable)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.concepts.len(), 2);
        assert_eq!(model.domain_objects.len(), 1);
        assert_eq!(model.aggregates.len(), 1);
        assert_eq!(model.value_objects.len(), 1);

        assert!(model
            .concepts
            .values()
            .any(|concept| concept.id == "CarePlan"));
        assert_eq!(model.entities.len(), 1);

        let contains_count = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Contains)
            .count();
        let maps_to_count = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::MapsTo)
            .count();
        assert_eq!(contains_count, 3);
        assert_eq!(maps_to_count, 2);
        assert_eq!(model.concept_mappings.len(), 2);
        assert!(model.concept_mappings.iter().any(|mapping| {
            matches!(
                (&mapping.source, model.entities[mapping.entity].id.as_str()),
                (ConceptualRef::DomainObject(_), "AppointmentTable")
            ) && model.domain_objects.values().any(|d| d.id == "Appointment")
        }));
        assert!(model.concept_mappings.iter().any(|mapping| {
            matches!(
                (&mapping.source, model.entities[mapping.entity].id.as_str()),
                (ConceptualRef::ValueObject(_), "AppointmentTable")
            ) && model.value_objects.values().any(|v| v.id == "TimeSlot")
        }));
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
relate(Order, Customer, "N:1").optional().on_delete(set_null).on_update(cascade)
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
        assert!(customer_id.is_fk);
        assert!(customer_id.fk_optional);
        assert!(customer_id.is_nullable);
        assert_eq!(customer_id.fk_on_delete.as_deref(), Some("set_null"));
        assert_eq!(customer_id.fk_on_update.as_deref(), Some("cascade"));
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

    #[test]
    fn test_duplicate_definition_same_kind() {
        let src = r#"
actor Customer "顧客"
actor Customer "重複"
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("duplicate definition"));
    }

    #[test]
    fn test_same_name_different_kind_allowed() {
        // `actor Add` and `usecase Add` must coexist without error when
        // references are qualified.
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(actor::Add, usecase::Add)
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
        assert_eq!(model.use_cases.len(), 1);
        assert_eq!(model.relations.len(), 1);
    }

    #[test]
    fn test_ambiguous_unqualified_reference() {
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(Add, Add)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("ambiguous reference"));
    }

    #[test]
    fn test_type_mismatch() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Browse, Customer)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_nm_relation_warning() {
        let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
relate(A, B, "N:M")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
        assert!(!warnings.is_empty());
        assert!(warnings[0].error.to_string().contains("N:M relation"));
    }

    #[test]
    fn test_missing_pk_error() {
        let src = r#"
entity A "A" { name: String }
entity B "B" { id: Int @pk }
relate(B, A, "N:1")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("missing @pk"));
    }

    #[test]
    fn test_one_to_many_fk_on_to_side() {
        let src = r#"
entity Customer "顧客" { id: Int @pk }
entity Order "注文" { id: Int @pk }
relate(Customer, Order, "1:N")
"#;
        let (ast, _) = parse(src);
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

        let order = model.entities.values().find(|e| e.id == "Order").unwrap();
        let fk = order.columns.iter().find(|c| c.name == "customer_id");
        assert!(fk.is_some(), "customer_id FK not found in Order");
        assert!(fk.unwrap().is_fk);
    }

    #[test]
    fn test_api_declaration_and_invokes() {
        let src = r#"
usecase PlaceOrder "注文する"
api OrderApi "注文API" description "注文を永続化するAPI"
invokes(PlaceOrder, OrderApi)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.apis.len(), 1);
        let api = model.apis.values().next().unwrap();
        assert_eq!(api.id, "OrderApi");
        assert_eq!(api.label, "注文API");
        assert_eq!(api.description.as_deref(), Some("注文を永続化するAPI"));

        let invokes_rel = model.relations.iter().find(|r| r.kind == RelKind::Invokes);
        assert!(invokes_rel.is_some(), "Invokes relation should exist");
    }

    #[test]
    fn test_belongs_when_where_context() {
        let src = r#"
business ClinicOps "Clinic Operations"
buc BucAppointmentScheduling "Appointment Scheduling"
location FrontDesk "Front Desk"
timing AppointmentRequested "Appointment Requested"
medium FrontDeskTerminal "Front Desk Terminal"
belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .where(FrontDesk)
  .where("patient portal")
  .by(FrontDeskTerminal)
  .by("tablet")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let rel = model.relations.iter().find(|r| r.kind == RelKind::Belongs);
        assert!(rel.is_some(), "Belongs relation should still exist");

        assert_eq!(model.business_mapping_contexts.len(), 1);
        let ctx = &model.business_mapping_contexts[0];
        assert_eq!(model.bucs[ctx.buc].id, "BucAppointmentScheduling");
        assert_eq!(model.businesses[ctx.business].id, "ClinicOps");
        assert_eq!(ctx.whens.len(), 2);
        assert_eq!(ctx.wheres.len(), 2);
        assert_eq!(ctx.bys.len(), 2);

        assert!(matches!(
            &ctx.whens[0],
            BusinessMappingContextValue::Text(s) if s == "patient requests a booking"
        ));
        assert!(matches!(
            &ctx.whens[1],
            BusinessMappingContextValue::Ref(NodeRef::Timing(_))
        ));
        assert!(matches!(
            &ctx.wheres[0],
            BusinessMappingContextValue::Ref(NodeRef::Location(_))
        ));
        assert!(matches!(
            &ctx.wheres[1],
            BusinessMappingContextValue::Text(s) if s == "patient portal"
        ));
        assert!(matches!(
            &ctx.bys[0],
            BusinessMappingContextValue::Ref(NodeRef::Medium(_))
        ));
        assert!(matches!(
            &ctx.bys[1],
            BusinessMappingContextValue::Text(s) if s == "tablet"
        ));
    }

    #[test]
    fn test_actor_permission_attachment() {
        let src = r#"
actor Staff "Staff"
permission ManageSchedule "Manage Schedule"
has_permission(Staff, ManageSchedule)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.permissions.len(), 1);
        let permission = model.permissions.values().next().unwrap();
        assert_eq!(permission.id, "ManageSchedule");
        assert_eq!(permission.label, "Manage Schedule");

        let rel = model
            .relations
            .iter()
            .find(|r| r.kind == RelKind::HasPermission)
            .expect("HasPermission relation should exist");
        assert!(matches!(rel.from, NodeRef::Actor(_)));
        assert!(matches!(rel.to, NodeRef::Permission(_)));
    }

    #[test]
    fn after_assert_registers_temporal_assertion_from_equality_expr() {
        let (ast, parse_errors) = parse(
            r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" {
  id: Int @pk
  status: Enum(requested, executed) @default(requested)
}
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
        );
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.temporal_assertions.len(), 1);
        let assertion = &model.temporal_assertions[0];
        assert_eq!(model.use_cases[assertion.anchor].id, "ExecuteCertIssue");
        assert_eq!(assertion.requireds.len(), 1);
    }

    #[test]
    fn forbidden_when_none_registers_quantifier_constraint() {
        let (ast, parse_errors) = parse(
            r#"
entity ClientCertificate "Client Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity TerminalCertAssignment "Terminal Cert Assignment" {
  id: Int @pk
  status: Enum(active, inactive) @default(active)
}
forbidden_when(ClientCertificate, (status, revoked))
  .none(TerminalCertAssignment, (status, active))
"#,
        );
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(model.quantifier_constraints.len(), 1);
        let constraint = &model.quantifier_constraints[0];
        assert_eq!(model.entities[constraint.anchor].id, "ClientCertificate");
        assert_eq!(
            model.entities[constraint.related].id,
            "TerminalCertAssignment"
        );
        assert_eq!(constraint.guards.len(), 1);
        assert_eq!(constraint.related_conditions.len(), 1);
    }

    #[test]
    fn test_screen_constraint_patterns_derive_from_usecase_and_api() {
        let src = r#"
usecase BookAppointment "Book Appointment"
screen BookingScreen "Booking Screen"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
medium StaffTerminal "Staff Terminal"
medium SecureChannel "Secure Channel"
displays(BookAppointment, BookingScreen)
invokes(BookAppointment, BookingApi)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, PatientRead)
requires_medium(BookingApi, SecureChannel)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let patterns = crate::derive_screen_constraint_patterns(&model);
        assert_eq!(patterns.len(), 1);

        let pattern = &patterns[0];
        assert_eq!(model.screens[pattern.screen].id, "BookingScreen");
        assert_eq!(model.use_cases[pattern.usecase].id, "BookAppointment");
        assert_eq!(
            model.apis[pattern.api.expect("api should be part of the path")].id,
            "BookingApi"
        );

        let permission_ids: Vec<_> = pattern
            .permissions
            .iter()
            .map(|key| model.permissions[*key].id.as_str())
            .collect();
        assert_eq!(permission_ids, vec!["ScheduleWrite", "PatientRead"]);

        let medium_ids: Vec<_> = pattern
            .media
            .iter()
            .map(|key| model.media[*key].id.as_str())
            .collect();
        assert_eq!(medium_ids, vec!["StaffTerminal", "SecureChannel"]);
    }

    #[test]
    fn test_api_crud_type_check_ok() {
        let src = r#"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
creates(OrderApi, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let creates_rel = model.relations.iter().find(|r| r.kind == RelKind::Creates);
        assert!(creates_rel.is_some());
    }

    #[test]
    fn test_invokes_type_mismatch() {
        // invokes(uc, entity) は TypeMismatch になるはず
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
invokes(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty(), "type mismatch expected");
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_usecase_crud_still_allowed() {
        // 後方互換: usecase が直接 entity を creates しても OK
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
creates(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "legacy creates(uc, entity) should still work"
        );
        assert_eq!(
            model
                .relations
                .iter()
                .filter(|r| r.kind == RelKind::Creates)
                .count(),
            1
        );
    }

    #[test]
    fn test_sets_comparison_registers_proposition_effect() {
        let src = r#"
usecase Sell "販売する"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
}
updates(Sell, Stock)
sets(Sell, Stock, stock < selling, true)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.proposition_effects.len(), 1);
        let effect = &model.proposition_effects[0];
        assert_eq!(effect.prop.axis_key(), "stock<selling");
        assert!(effect.truth);
        assert!(matches!(effect.origin, NodeRef::UseCase(_)));
    }

    #[test]
    fn test_required_registers_conditions_and_comparison() {
        let src = r#"
entity Coupon "クーポン" {
  id: Int @pk
  status: Enum(usable, expired)
  expired_at: DateTime @null
}
required(Coupon, (status, usable), expired_at < now)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.required_constraints.len(), 1);
        let constraint = &model.required_constraints[0];
        assert_eq!(constraint.conditions.len(), 1);
        assert_eq!(constraint.comparisons.len(), 1);
        assert_eq!(constraint.comparisons[0].axis_key(), "expired_at<now");
    }

    #[test]
    fn test_exclusive_registers_flat_pair_conditions() {
        let src = r#"
entity Document "文書" {
  id: Int @pk
  approved: Bool
  rejected: Bool
}
exclusive(Document, approved, true, rejected, true)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.exclusive_constraints.len(), 1);
        let constraint = &model.exclusive_constraints[0];
        assert_eq!(constraint.conditions.len(), 2);
        assert_eq!(constraint.comparisons.len(), 0);
    }

    #[test]
    fn test_cross_forbidden_registers_qualified_conditions() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, cancelled)
  total: Decimal
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
  amount: Decimal
}
cross_forbidden(Order, Payment, (Order.status, cancelled), Payment.amount > Order.total)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.cross_forbidden_constraints.len(), 1);
        let constraint = &model.cross_forbidden_constraints[0];
        assert_eq!(constraint.scope.len(), 2);
        assert_eq!(constraint.conditions.len(), 2);

        let order_key = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Order").then_some(key))
            .unwrap();
        let payment_key = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Payment").then_some(key))
            .unwrap();

        assert!(matches!(
            &constraint.conditions[0],
            CrossEntityCondition::Equals { column, value }
                if column.entity == order_key
                    && column.column == "status"
                    && value == &EffectValue::EnumVariant("cancelled".to_string())
        ));
        assert!(matches!(
            &constraint.conditions[1],
            CrossEntityCondition::Comparison(CrossComparisonProp {
                lhs,
                op: CmpOpModel::Gt,
                rhs: CrossCmpRhs::Column(rhs),
            }) if lhs.entity == payment_key
                && lhs.column == "amount"
                && rhs.entity == order_key
                && rhs.column == "total"
        ));
    }

    #[test]
    fn test_cross_invariant_registers_when_then_conditions() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.cross_entity_invariants.len(), 1);
        let invariant = &model.cross_entity_invariants[0];
        assert_eq!(invariant.scope.len(), 2);
        assert_eq!(invariant.guards.len(), 1);
        assert_eq!(invariant.requireds.len(), 1);
    }

    #[test]
    fn test_cross_invariant_registers_along_scope() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let invariant = &model.cross_entity_invariants[0];
        let CrossConstraintScope::RelationPath(path) = &invariant.scope_semantics else {
            panic!(
                "expected relation-path scope, got {:?}",
                invariant.scope_semantics
            );
        };
        let path_ids: Vec<_> = path
            .iter()
            .map(|key| model.entities[*key].id.as_str())
            .collect();
        assert_eq!(path_ids, vec!["Order", "Payment"]);
    }

    #[test]
    fn test_cross_invariant_can_infer_scope_from_qualified_columns() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant()
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let invariant = &model.cross_entity_invariants[0];
        let scope_ids: Vec<_> = invariant
            .scope
            .iter()
            .map(|key| model.entities[*key].id.as_str())
            .collect();
        assert_eq!(scope_ids, vec!["Order", "Payment"]);
    }

    #[test]
    fn test_cross_invariant_requires_column_qualifier_for_multi_entity_scope() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
cross_invariant(Order, Payment)
  .when(status, paid)
  .then(Payment.status, captured)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors
                .iter()
                .any(|d| d.error.to_string().contains("needs an entity qualifier")),
            "expected qualifier diagnostic, got: {:?}",
            errors
        );
    }
}
