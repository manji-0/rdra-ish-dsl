use super::support::*;
use crate::analysis::instance::register_instance;
use crate::analysis::nodes::node_kind_tag_str;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

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
