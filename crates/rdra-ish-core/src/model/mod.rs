mod comparison;
mod constraints;
mod effects;
mod instances;
mod keys;
mod refs;
mod symbol;
mod typed;

pub use comparison::*;
pub use constraints::*;
pub use effects::*;
pub use instances::*;
pub use keys::*;
pub use refs::*;
pub use symbol::*;
pub use typed::*;

use slotmap::SlotMap;
use std::collections::HashSet;

/// セマンティックモデル
#[derive(Debug, Default)]
pub struct SemanticModel {
    pub actors: SlotMap<ActorKey, Actor>,
    pub ext_systems: SlotMap<ExtSystemKey, ExtSystem>,
    pub systems: SlotMap<SystemKey, System>,
    pub requirements: SlotMap<RequirementKey, Requirement>,
    pub adrs: SlotMap<AdrKey, Adr>,
    pub nfrs: SlotMap<NfrKey, Nfr>,
    pub qualities: SlotMap<QualityKey, Quality>,
    pub constraints: SlotMap<ConstraintKey, Constraint>,
    pub concepts: SlotMap<ConceptKey, Concept>,
    pub domain_objects: SlotMap<DomainObjectKey, DomainObject>,
    pub aggregates: SlotMap<AggregateKey, Aggregate>,
    pub value_objects: SlotMap<ValueObjectKey, ValueObject>,
    pub businesses: SlotMap<BusinessKey, Business>,
    pub bucs: SlotMap<BucKey, Buc>,
    pub flows: SlotMap<FlowKey, Flow>,
    pub steps: SlotMap<StepKey, Step>,
    pub usage_scenes: SlotMap<UsageSceneKey, UsageScene>,
    pub use_cases: SlotMap<UseCaseKey, UseCase>,
    pub screens: SlotMap<ScreenKey, Screen>,
    pub fields: SlotMap<FieldKey, Field>,
    pub events: SlotMap<EventKey, Event>,
    pub entities: SlotMap<EntityKey, Entity>,
    pub states: SlotMap<StateKey, State>,
    pub conditions: SlotMap<ConditionKey, Condition>,
    pub variations: SlotMap<VariationKey, Variation>,
    pub apis: SlotMap<ApiKey, Api>,
    pub dtos: SlotMap<DtoKey, Dto>,
    pub locations: SlotMap<LocationKey, Location>,
    pub timings: SlotMap<TimingKey, Timing>,
    pub media: SlotMap<MediumKey, Medium>,
    pub permissions: SlotMap<PermissionKey, Permission>,
    pub relations: Vec<Relation>,
    pub boundary_coordinations: Vec<BoundaryCoordination>,
    pub business_mapping_contexts: Vec<BusinessMappingContext>,
    pub field_mappings: Vec<FieldMapping>,
    /// `maps_to(Conceptual, Entity)` で宣言される概念→論理データモデル対応。
    pub concept_mappings: Vec<ConceptMapping>,
    /// 解析済み述語の型付き表現。
    pub typed_predicates: Vec<TypedPredicate>,
    /// Events intentionally published outside the local model boundary.
    pub outbox_events: HashSet<EventKey>,
    pub state_transitions: Vec<StateTransition>,
    pub column_effects: Vec<ColumnEffect>,
    /// `sets(origin, entity, <comparison_expr>, bool)` で宣言された比較命題の真偽効果
    pub proposition_effects: Vec<PropositionEffect>,
    /// `forbidden(...)` 述語で宣言された禁止状態制約
    pub forbidden_constraints: Vec<ForbiddenConstraint>,
    /// `invariant(...)` 述語で宣言された不変条件制約
    pub entity_invariants: Vec<EntityInvariant>,
    /// `required(...)` 述語で宣言された常時成立制約
    pub required_constraints: Vec<RequiredConstraint>,
    /// `exclusive(...)` 述語で宣言された相互排他制約
    pub exclusive_constraints: Vec<ExclusiveConstraint>,
    /// `forbidden(...)` 述語で宣言されたクロスエンティティ禁止制約
    pub cross_forbidden_constraints: Vec<CrossForbiddenConstraint>,
    /// `invariant(...)` 述語で宣言されたクロスエンティティ不変条件
    pub cross_entity_invariants: Vec<CrossEntityInvariant>,
    /// `after(UseCase).assert(...)` 述語で宣言された時相アンカー制約
    pub temporal_assertions: Vec<TemporalAssertion>,
    /// `has` / `none` チェーンで宣言された to-many 量化制約
    pub quantifier_constraints: Vec<QuantifierConstraint>,
    /// `property Id "..." leads_to/always/eventually(...)` 時相プロパティ
    pub temporal_properties: Vec<TemporalProperty>,
    pub symbols: SymbolTable,
    /// Declaration sites for `kind:id` lookups (LSP go-to-definition).
    pub decl_sites: crate::location::DeclIndex,
}
