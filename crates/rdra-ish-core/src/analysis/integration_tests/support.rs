use crate::analysis::instance::register_instance;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

pub(super) fn instance(kind: Kind, id: &str) -> InstanceDecl {
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

pub(super) fn model_column(name: &str, col_type: ColumnType) -> ModelColumn {
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

pub(super) fn qref(id: &str) -> QRef {
    QRef {
        kind_qualifier: None,
        parts: vec![id.to_string()],
        span: 0..0,
    }
}

pub(super) fn qcol(entity: &str, column: &str) -> Operand {
    Operand::QualifiedColumn(QualifiedColumnRef {
        entity: qref(entity),
        column: column.to_string(),
        span: 0..0,
    })
}

pub(super) fn entity_key(model: &SemanticModel, id: &str) -> EntityKey {
    model
        .entities
        .iter()
        .find_map(|(key, entity)| (entity.id == id).then_some(key))
        .unwrap()
}

pub(super) fn simple_entity_model(ids: &[&str]) -> SemanticModel {
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
