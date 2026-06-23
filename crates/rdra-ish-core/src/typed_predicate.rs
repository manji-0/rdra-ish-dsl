//! 述語呼び出しの型付き discriminated union 構築・集約 API。

use crate::model::{
    ActorKey, AdrKey, ApiKey, AppliesToTarget, BucKey, BusinessKey, Cardinality, ConceptualRef,
    ConstrainsTarget, ConstraintKey, ContainedRef, ContainerRef, CoversTarget, DecidesTarget,
    DtoKey, EntityKey, EntityTouchpoint, EventKey, ExtSystemKey, FieldKey, MediumKey, NfrKey,
    NfrOrConstraint, NodeRef, PerformTarget, PermissionKey, QualityKey, RequirementKey, ScreenKey,
    SemanticModel, StateKey, StepKey, SystemKey, TriggerTarget, TypedPredicate, UseCaseKey,
};
use rdra_ish_syntax::ast::{PredicateArg, PredicateCall};

/// 述語名と解決済み引数から型付き述語を構築する。
/// 引数の型が期待と一致しない場合は `None`。
/// `sets` / 制約述語 / `cross_*` / `after` は専用ハンドラで登録する。
pub fn build_typed_predicate(
    name: &str,
    resolved: &[Option<NodeRef>],
    pred: &PredicateCall,
) -> Option<TypedPredicate> {
    match name {
        "outbox" => Some(TypedPredicate::Outbox {
            event: as_event(resolved.first()?.as_ref()?)?,
        }),
        "coordinates" => Some(TypedPredicate::Coordinates {
            usecase: as_usecase(resolved.first()?.as_ref()?)?,
            left: as_entity(resolved.get(1)?.as_ref()?)?,
            right: as_entity(resolved.get(2)?.as_ref()?)?,
        }),
        "transitions" => Some(TypedPredicate::Transitions {
            event: as_event(resolved.first()?.as_ref()?)?,
            from: as_state(resolved.get(1)?.as_ref()?)?,
            to: as_state(resolved.get(2)?.as_ref()?)?,
        }),
        "maps_field" => {
            let (from, to) = pair(resolved)?;
            let column = pred.args.get(2).and_then(arg_as_str)?.to_string();
            Some(TypedPredicate::MapsField {
                field: as_field(from)?,
                entity: as_entity(to)?,
                column,
            })
        }
        "relate" => {
            let (from, to) = pair(resolved)?;
            let card = pred.args.get(2).and_then(arg_as_str)?;
            Some(TypedPredicate::Relate {
                from: as_entity(from)?,
                to: as_entity(to)?,
                cardinality: Cardinality::from_literal(card)?,
            })
        }
        _ => {
            let (from, to) = pair(resolved)?;
            match name {
                "performs" => Some(TypedPredicate::Performs {
                    actor: as_actor(from)?,
                    target: PerformTarget::from_node_ref(to)?,
                }),
                "uses" => Some(TypedPredicate::Uses {
                    actor: as_actor(from)?,
                    ext_system: as_ext_system(to)?,
                }),
                "reads" => Some(TypedPredicate::Reads {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "writes" => Some(TypedPredicate::Writes {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "creates" => Some(TypedPredicate::Creates {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "updates" => Some(TypedPredicate::Updates {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "deletes" => Some(TypedPredicate::Deletes {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "invokes" => Some(TypedPredicate::Invokes {
                    usecase: as_usecase(from)?,
                    api: as_api(to)?,
                }),
                "request" => Some(TypedPredicate::Request {
                    api: as_api(from)?,
                    dto: as_dto(to)?,
                }),
                "response" => Some(TypedPredicate::Response {
                    api: as_api(from)?,
                    dto: as_dto(to)?,
                }),
                "error_response" => Some(TypedPredicate::ErrorResponse {
                    api: as_api(from)?,
                    dto: as_dto(to)?,
                }),
                "applies_to" => Some(TypedPredicate::AppliesTo {
                    nfr: as_nfr(from)?,
                    target: AppliesToTarget::from_node_ref(to)?,
                }),
                "qualifies" => Some(TypedPredicate::Qualifies {
                    source: NfrOrConstraint::from_node_ref(from)?,
                    quality: as_quality(to)?,
                }),
                "constrains" => Some(TypedPredicate::Constrains {
                    constraint: as_constraint(from)?,
                    target: ConstrainsTarget::from_node_ref(to)?,
                }),
                "owns" => Some(TypedPredicate::Owns {
                    system: as_system(from)?,
                    entity: as_entity(to)?,
                }),
                "displays" => Some(TypedPredicate::Displays {
                    usecase: as_usecase(from)?,
                    screen: as_screen(to)?,
                }),
                "shows" => Some(TypedPredicate::Shows {
                    screen: as_screen(from)?,
                    entity: as_entity(to)?,
                }),
                "raises" => Some(TypedPredicate::Raises {
                    usecase: as_usecase(from)?,
                    event: as_event(to)?,
                }),
                "triggers" => Some(TypedPredicate::Triggers {
                    event: as_event(from)?,
                    target: TriggerTarget::from_node_ref(to)?,
                }),
                "contains" => Some(TypedPredicate::Contains {
                    container: ContainerRef::from_node_ref(from)?,
                    contained: ContainedRef::from_node_ref(to)?,
                }),
                "precedes" => Some(TypedPredicate::Precedes {
                    from: as_step(from)?,
                    to: as_step(to)?,
                }),
                "branches" => Some(TypedPredicate::Branches {
                    from: as_step(from)?,
                    to: as_step(to)?,
                }),
                "excepts" => Some(TypedPredicate::Excepts {
                    from: as_step(from)?,
                    to: as_step(to)?,
                }),
                "repeats" => Some(TypedPredicate::Repeats {
                    from: as_step(from)?,
                    to: as_step(to)?,
                }),
                "covers" => Some(TypedPredicate::Covers {
                    step: as_step(from)?,
                    target: CoversTarget::from_node_ref(to)?,
                }),
                "compensates" => Some(TypedPredicate::Compensates {
                    from: as_usecase(from)?,
                    to: as_usecase(to)?,
                }),
                "maps_to" => Some(TypedPredicate::MapsTo {
                    source: ConceptualRef::from_node_ref(from)?,
                    entity: as_entity(to)?,
                }),
                "belongs" => Some(TypedPredicate::Belongs {
                    buc: as_buc(from)?,
                    business: as_business(to)?,
                }),
                "has_permission" => Some(TypedPredicate::HasPermission {
                    actor: as_actor(from)?,
                    permission: as_permission(to)?,
                }),
                "requires_permission" => Some(TypedPredicate::RequiresPermission {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    permission: as_permission(to)?,
                }),
                "requires_medium" => Some(TypedPredicate::RequiresMedium {
                    origin: EntityTouchpoint::from_node_ref(from)?,
                    medium: as_medium(to)?,
                }),
                "motivates" => Some(TypedPredicate::Motivates {
                    requirement: as_requirement(from)?,
                    buc: as_buc(to)?,
                }),
                "decides" => Some(TypedPredicate::Decides {
                    adr: as_adr(from)?,
                    target: DecidesTarget::from_node_ref(to)?,
                }),
                _ => None,
            }
        }
    }
}

/// モデル内の型付き述語一覧（述語名・内容の辞書順）。
pub fn collect_typed_predicates(model: &SemanticModel) -> Vec<TypedPredicate> {
    let mut predicates = model.typed_predicates.clone();
    predicates.sort_by_key(typed_predicate_sort_key);
    predicates
}

pub fn typed_predicate_name(pred: &TypedPredicate) -> &'static str {
    match pred {
        TypedPredicate::Performs { .. } => "performs",
        TypedPredicate::Uses { .. } => "uses",
        TypedPredicate::Reads { .. } => "reads",
        TypedPredicate::Writes { .. } => "writes",
        TypedPredicate::Creates { .. } => "creates",
        TypedPredicate::Updates { .. } => "updates",
        TypedPredicate::Deletes { .. } => "deletes",
        TypedPredicate::Invokes { .. } => "invokes",
        TypedPredicate::Request { .. } => "request",
        TypedPredicate::Response { .. } => "response",
        TypedPredicate::ErrorResponse { .. } => "error_response",
        TypedPredicate::AppliesTo { .. } => "applies_to",
        TypedPredicate::Qualifies { .. } => "qualifies",
        TypedPredicate::Constrains { .. } => "constrains",
        TypedPredicate::Owns { .. } => "owns",
        TypedPredicate::Displays { .. } => "displays",
        TypedPredicate::Shows { .. } => "shows",
        TypedPredicate::Raises { .. } => "raises",
        TypedPredicate::Triggers { .. } => "triggers",
        TypedPredicate::Contains { .. } => "contains",
        TypedPredicate::Precedes { .. } => "precedes",
        TypedPredicate::Branches { .. } => "branches",
        TypedPredicate::Excepts { .. } => "excepts",
        TypedPredicate::Repeats { .. } => "repeats",
        TypedPredicate::Covers { .. } => "covers",
        TypedPredicate::Compensates { .. } => "compensates",
        TypedPredicate::MapsTo { .. } => "maps_to",
        TypedPredicate::Coordinates { .. } => "coordinates",
        TypedPredicate::Belongs { .. } => "belongs",
        TypedPredicate::HasPermission { .. } => "has_permission",
        TypedPredicate::RequiresPermission { .. } => "requires_permission",
        TypedPredicate::RequiresMedium { .. } => "requires_medium",
        TypedPredicate::Motivates { .. } => "motivates",
        TypedPredicate::Decides { .. } => "decides",
        TypedPredicate::Transitions { .. } => "transitions",
        TypedPredicate::Outbox { .. } => "outbox",
        TypedPredicate::MapsField { .. } => "maps_field",
        TypedPredicate::Relate { .. } => "relate",
        TypedPredicate::SetsColumn { .. } => "sets",
        TypedPredicate::SetsProposition { .. } => "sets",
        TypedPredicate::After { .. } => "after",
        TypedPredicate::Forbidden { .. } => "forbidden",
        TypedPredicate::Required { .. } => "required",
        TypedPredicate::Exclusive { .. } => "exclusive",
        TypedPredicate::Invariant { .. } => "invariant",
        TypedPredicate::ForbiddenWhen { .. } => "forbidden_when",
        TypedPredicate::CrossForbidden { .. } => "cross_forbidden",
        TypedPredicate::CrossInvariant { .. } => "cross_invariant",
    }
}

fn typed_predicate_sort_key(pred: &TypedPredicate) -> String {
    format!("{}:{pred:?}", typed_predicate_name(pred))
}

fn pair(resolved: &[Option<NodeRef>]) -> Option<(&NodeRef, &NodeRef)> {
    match (resolved.first(), resolved.get(1)) {
        (Some(Some(a)), Some(Some(b))) => Some((a, b)),
        _ => None,
    }
}

fn arg_as_str(arg: &PredicateArg) -> Option<&str> {
    match arg {
        PredicateArg::Lit(s) => Some(s.as_str()),
        _ => None,
    }
}

macro_rules! node_extractor {
    ($fn_name:ident, $variant:ident, $ty:ty) => {
        fn $fn_name(node: &NodeRef) -> Option<$ty> {
            match node {
                NodeRef::$variant(k) => Some(*k),
                _ => None,
            }
        }
    };
}

node_extractor!(as_actor, Actor, ActorKey);
node_extractor!(as_ext_system, ExtSystem, ExtSystemKey);
node_extractor!(as_usecase, UseCase, UseCaseKey);
node_extractor!(as_api, Api, ApiKey);
node_extractor!(as_entity, Entity, EntityKey);
node_extractor!(as_screen, Screen, ScreenKey);
node_extractor!(as_event, Event, EventKey);
node_extractor!(as_step, Step, StepKey);
node_extractor!(as_buc, Buc, BucKey);
node_extractor!(as_business, Business, BusinessKey);
node_extractor!(as_permission, Permission, PermissionKey);
node_extractor!(as_medium, Medium, MediumKey);
node_extractor!(as_requirement, Requirement, RequirementKey);
node_extractor!(as_adr, Adr, AdrKey);
node_extractor!(as_nfr, Nfr, NfrKey);
node_extractor!(as_quality, Quality, QualityKey);
node_extractor!(as_constraint, Constraint, ConstraintKey);
node_extractor!(as_system, System, SystemKey);
node_extractor!(as_field, Field, FieldKey);
node_extractor!(as_dto, Dto, DtoKey);
node_extractor!(as_state, State, StateKey);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        model
    }

    #[test]
    fn typed_predicates_capture_relations_and_lifecycle() {
        let model = model_from(
            r#"
actor Staff "Staff"
usecase Book "Book"
entity Appointment "Appointment" { id: Int @pk }
domain_object Appt "Appointment"
event EvBook "Booked"
state Draft "Draft"
state Booked "Booked"
performs(Staff, Book)
creates(Book, Appointment)
raises(Book, EvBook)
maps_to(Appt, Appointment)
transitions(EvBook, Draft, Booked)
"#,
        );

        let predicates = collect_typed_predicates(&model);
        assert!(predicates
            .iter()
            .any(|p| matches!(p, TypedPredicate::Performs { .. })));
        assert!(predicates
            .iter()
            .any(|p| matches!(p, TypedPredicate::Creates { .. })));
        assert!(predicates
            .iter()
            .any(|p| matches!(p, TypedPredicate::Raises { .. })));
        assert!(predicates
            .iter()
            .any(|p| matches!(p, TypedPredicate::MapsTo { .. })));
        assert!(predicates
            .iter()
            .any(|p| matches!(p, TypedPredicate::Transitions { .. })));
        assert_eq!(predicates.len(), model.typed_predicates.len());
    }

    #[test]
    fn build_typed_predicate_rejects_mismatched_kinds() {
        let pred = PredicateCall {
            name: "performs".to_string(),
            args: vec![],
            chain: vec![],
            span: 0..0,
        };
        assert!(build_typed_predicate("performs", &[], &pred).is_none());
    }
}
