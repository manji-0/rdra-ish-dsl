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

impl SemanticModel {
    /// Values of an `Enum` or variation-typed column; `None` for scalar types.
    ///
    /// This is the single lookup every exporter uses for "what may this column hold".
    pub fn column_values<'a>(&'a self, col: &'a ModelColumn) -> Option<&'a [std::string::String]> {
        match &col.col_type {
            ColumnType::Enum(values) => Some(values),
            ColumnType::Variation(key) => self.variations.get(*key).map(|v| v.values.as_slice()),
            _ => None,
        }
    }

    /// `transitions` names this column of this entity, so it is a state axis.
    pub fn is_lifecycle_column(&self, entity: EntityKey, column: &str) -> bool {
        self.state_transitions
            .iter()
            .any(|t| t.entity == entity && t.column == column)
    }
}

/// One `combines` declaration. Condition links are read through `condition_edges`, which
/// prefers the `holds` body over these.
#[derive(Debug, Clone)]
pub struct CombinesDecl {
    pub condition: ConditionKey,
    pub target: CombinesTarget,
    pub location: Option<crate::location::LocatedSpan>,
}

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
    pub businesses: SlotMap<BusinessKey, Business>,
    pub bucs: SlotMap<BucKey, Buc>,
    pub flows: SlotMap<FlowKey, Flow>,
    pub steps: SlotMap<StepKey, Step>,
    pub use_cases: SlotMap<UseCaseKey, UseCase>,
    pub screens: SlotMap<ScreenKey, Screen>,
    pub fields: SlotMap<FieldKey, Field>,
    pub events: SlotMap<EventKey, Event>,
    pub entities: SlotMap<EntityKey, Entity>,
    pub states: SlotMap<StateKey, State>,
    pub apis: SlotMap<ApiKey, Api>,
    pub dtos: SlotMap<DtoKey, Dto>,
    pub timers: SlotMap<TimerKey, Timer>,
    pub media: SlotMap<MediumKey, Medium>,
    pub permissions: SlotMap<PermissionKey, Permission>,
    pub variations: SlotMap<VariationKey, Variation>,
    pub conditions: SlotMap<ConditionKey, Condition>,
    pub relations: Vec<Relation>,
    /// `combines(Condition, Variation | Entity.column)` declarations, in source order.
    pub combines: Vec<CombinesDecl>,
    pub boundary_coordinations: Vec<BoundaryCoordination>,
    pub business_mapping_contexts: Vec<BusinessMappingContext>,
    pub field_mappings: Vec<FieldMapping>,
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
    /// Per-file import / module visibility used when resolving references.
    pub import_scopes: crate::import_scope::ImportScopes,
    /// Declaration sites for `kind:id` lookups (LSP go-to-definition).
    pub decl_sites: crate::location::DeclIndex,
}
