use super::keys::*;

/// NodeRef: 異種ノード間関連を一様表現
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeRef {
    Actor(ActorKey),
    ExtSystem(ExtSystemKey),
    System(SystemKey),
    Requirement(RequirementKey),
    Adr(AdrKey),
    Nfr(NfrKey),
    Quality(QualityKey),
    Constraint(ConstraintKey),
    Concept(ConceptKey),
    DomainObject(DomainObjectKey),
    Aggregate(AggregateKey),
    ValueObject(ValueObjectKey),
    Business(BusinessKey),
    Buc(BucKey),
    Flow(FlowKey),
    Step(StepKey),
    UsageScene(UsageSceneKey),
    UseCase(UseCaseKey),
    Screen(ScreenKey),
    Field(FieldKey),
    Event(EventKey),
    Entity(EntityKey),
    State(StateKey),
    Condition(ConditionKey),
    Variation(VariationKey),
    Api(ApiKey),
    Dto(DtoKey),
    Location(LocationKey),
    Timing(TimingKey),
    Medium(MediumKey),
    Permission(PermissionKey),
}

/// 述語の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelKind {
    Performs,
    Uses,
    Reads,
    Writes,
    Creates,
    Updates,
    Deletes,
    Displays,
    Shows,
    Raises,
    Triggers,
    Contains,
    Belongs,
    HasPermission,
    RequiresPermission,
    RequiresMedium,
    Motivates,
    Decides,
    Transitions,
    Invokes, // usecase → api
    Precedes,
    Branches,
    Excepts,
    Repeats,
    Covers,
    Compensates,
    Request,
    Response,
    ErrorResponse,
    AppliesTo,
    Qualifies,
    Constrains,
    MapsTo,
    MapsField,
    Owns,
    // Entity ER
    RelateOneToOne,   // 1:1
    RelateOneToMany,  // 1:N (A側が1, B側がMany)
    RelateManyToOne,  // N:1 (A側がMany, B側が1) → A に FK
    RelateManyToMany, // N:M (警告のみ)
}

/// リレーション（from, to, kind）
#[derive(Debug, Clone)]
pub struct Relation {
    pub from: NodeRef,
    pub to: NodeRef,
    pub kind: RelKind,
    pub options: RelationOptions,
}

/// Relation-level options, currently used by `relate` to shape generated FKs.
#[derive(Debug, Clone, Default)]
pub struct RelationOptions {
    pub optional: bool,
    pub on_delete: Option<std::string::String>,
    pub on_update: Option<std::string::String>,
}

/// `coordinates(usecase, entity, entity)` で宣言される、system境界越えrelationの調停責務。
#[derive(Debug, Clone)]
pub struct BoundaryCoordination {
    pub usecase: UseCaseKey,
    pub left: EntityKey,
    pub right: EntityKey,
}

/// `belongs(Buc, Business).when(...).where(...).by(...)` で宣言される、
/// Business と BUC の対応関係に付く文脈値。
#[derive(Debug, Clone)]
pub enum BusinessMappingContextValue {
    Text(std::string::String),
    Ref(NodeRef),
}

/// Business と BUC の対応関係に付く When / Where / By 文脈。
#[derive(Debug, Clone)]
pub struct BusinessMappingContext {
    pub buc: BucKey,
    pub business: BusinessKey,
    pub whens: Vec<BusinessMappingContextValue>,
    pub wheres: Vec<BusinessMappingContextValue>,
    pub bys: Vec<BusinessMappingContextValue>,
}

/// Mapping between a screen field and a logical data model column.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    pub field: FieldKey,
    pub entity: EntityKey,
    pub column: std::string::String,
}

/// 概念モデル要素への参照（`maps_to` の source 側）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConceptualRef {
    Concept(ConceptKey),
    DomainObject(DomainObjectKey),
    Aggregate(AggregateKey),
    ValueObject(ValueObjectKey),
}

impl ConceptualRef {
    pub fn as_node_ref(&self) -> NodeRef {
        match self {
            ConceptualRef::Concept(k) => NodeRef::Concept(*k),
            ConceptualRef::DomainObject(k) => NodeRef::DomainObject(*k),
            ConceptualRef::Aggregate(k) => NodeRef::Aggregate(*k),
            ConceptualRef::ValueObject(k) => NodeRef::ValueObject(*k),
        }
    }

    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Concept(k) => Some(ConceptualRef::Concept(*k)),
            NodeRef::DomainObject(k) => Some(ConceptualRef::DomainObject(*k)),
            NodeRef::Aggregate(k) => Some(ConceptualRef::Aggregate(*k)),
            NodeRef::ValueObject(k) => Some(ConceptualRef::ValueObject(*k)),
            _ => None,
        }
    }
}

/// `maps_to(Conceptual, Entity)` で宣言される概念→論理データモデル対応。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptMapping {
    pub source: ConceptualRef,
    pub entity: EntityKey,
}

/// usecase / api が entity を操作する際の起点。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityTouchpoint {
    UseCase(UseCaseKey),
    Api(ApiKey),
}

impl EntityTouchpoint {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(EntityTouchpoint::UseCase(*k)),
            NodeRef::Api(k) => Some(EntityTouchpoint::Api(*k)),
            _ => None,
        }
    }
}

/// `sets(usecase/event, entity, ...)` の起点。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataOrigin {
    UseCase(UseCaseKey),
    Event(EventKey),
}

impl DataOrigin {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(DataOrigin::UseCase(*k)),
            NodeRef::Event(k) => Some(DataOrigin::Event(*k)),
            _ => None,
        }
    }
}

/// `performs(actor, ...)` の第2引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PerformTarget {
    UseCase(UseCaseKey),
    Buc(BucKey),
}

impl PerformTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(PerformTarget::UseCase(*k)),
            NodeRef::Buc(k) => Some(PerformTarget::Buc(*k)),
            _ => None,
        }
    }
}

/// `triggers(event, ...)` の第2引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TriggerTarget {
    UseCase(UseCaseKey),
    Buc(BucKey),
}

impl TriggerTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(TriggerTarget::UseCase(*k)),
            NodeRef::Buc(k) => Some(TriggerTarget::Buc(*k)),
            _ => None,
        }
    }
}

/// `contains(container, ...)` の container 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainerRef {
    Buc(BucKey),
    System(SystemKey),
    Flow(FlowKey),
    Aggregate(AggregateKey),
    Screen(ScreenKey),
}

impl ContainerRef {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Buc(k) => Some(ContainerRef::Buc(*k)),
            NodeRef::System(k) => Some(ContainerRef::System(*k)),
            NodeRef::Flow(k) => Some(ContainerRef::Flow(*k)),
            NodeRef::Aggregate(k) => Some(ContainerRef::Aggregate(*k)),
            NodeRef::Screen(k) => Some(ContainerRef::Screen(*k)),
            _ => None,
        }
    }
}

/// `contains(..., contained)` の contained 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainedRef {
    UseCase(UseCaseKey),
    Api(ApiKey),
    Flow(FlowKey),
    Step(StepKey),
    Conceptual(ConceptualRef),
    Field(FieldKey),
}

impl ContainedRef {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(ContainedRef::UseCase(*k)),
            NodeRef::Api(k) => Some(ContainedRef::Api(*k)),
            NodeRef::Flow(k) => Some(ContainedRef::Flow(*k)),
            NodeRef::Step(k) => Some(ContainedRef::Step(*k)),
            NodeRef::Field(k) => Some(ContainedRef::Field(*k)),
            other => ConceptualRef::from_node_ref(other).map(ContainedRef::Conceptual),
        }
    }
}

/// `applies_to(nfr, ...)` の target 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppliesToTarget {
    UseCase(UseCaseKey),
    Api(ApiKey),
    System(SystemKey),
}

impl AppliesToTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(AppliesToTarget::UseCase(*k)),
            NodeRef::Api(k) => Some(AppliesToTarget::Api(*k)),
            NodeRef::System(k) => Some(AppliesToTarget::System(*k)),
            _ => None,
        }
    }
}

/// `qualifies(...)` の source 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NfrOrConstraint {
    Nfr(NfrKey),
    Constraint(ConstraintKey),
}

impl NfrOrConstraint {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Nfr(k) => Some(NfrOrConstraint::Nfr(*k)),
            NodeRef::Constraint(k) => Some(NfrOrConstraint::Constraint(*k)),
            _ => None,
        }
    }
}

/// `constrains(constraint, ...)` の target 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstrainsTarget {
    UseCase(UseCaseKey),
    Api(ApiKey),
    System(SystemKey),
    Entity(EntityKey),
    Dto(DtoKey),
}

impl ConstrainsTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(ConstrainsTarget::UseCase(*k)),
            NodeRef::Api(k) => Some(ConstrainsTarget::Api(*k)),
            NodeRef::System(k) => Some(ConstrainsTarget::System(*k)),
            NodeRef::Entity(k) => Some(ConstrainsTarget::Entity(*k)),
            NodeRef::Dto(k) => Some(ConstrainsTarget::Dto(*k)),
            _ => None,
        }
    }
}

/// `covers(step, ...)` の target 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CoversTarget {
    UseCase(UseCaseKey),
    Api(ApiKey),
    Event(EventKey),
}

impl CoversTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(CoversTarget::UseCase(*k)),
            NodeRef::Api(k) => Some(CoversTarget::Api(*k)),
            NodeRef::Event(k) => Some(CoversTarget::Event(*k)),
            _ => None,
        }
    }
}

/// `decides(adr, ...)` の target 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DecidesTarget {
    Buc(BucKey),
    UseCase(UseCaseKey),
    Api(ApiKey),
    System(SystemKey),
    Entity(EntityKey),
    Requirement(RequirementKey),
    Nfr(NfrKey),
    Constraint(ConstraintKey),
    Conceptual(ConceptualRef),
    Dto(DtoKey),
}

impl DecidesTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Buc(k) => Some(DecidesTarget::Buc(*k)),
            NodeRef::UseCase(k) => Some(DecidesTarget::UseCase(*k)),
            NodeRef::Api(k) => Some(DecidesTarget::Api(*k)),
            NodeRef::System(k) => Some(DecidesTarget::System(*k)),
            NodeRef::Entity(k) => Some(DecidesTarget::Entity(*k)),
            NodeRef::Requirement(k) => Some(DecidesTarget::Requirement(*k)),
            NodeRef::Nfr(k) => Some(DecidesTarget::Nfr(*k)),
            NodeRef::Constraint(k) => Some(DecidesTarget::Constraint(*k)),
            NodeRef::Dto(k) => Some(DecidesTarget::Dto(*k)),
            other => ConceptualRef::from_node_ref(other).map(DecidesTarget::Conceptual),
        }
    }
}

/// `relate(..., "N:1")` のカーディナリティ。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

impl Cardinality {
    pub fn from_literal(card: &str) -> Option<Self> {
        match card {
            "1:1" => Some(Cardinality::OneToOne),
            "1:N" => Some(Cardinality::OneToMany),
            "N:1" => Some(Cardinality::ManyToOne),
            "N:M" => Some(Cardinality::ManyToMany),
            _ => None,
        }
    }
}

/// カラム型
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnType {
    Int,
    String,
    Money,
    DateTime,
    Date,
    Bool,
    Decimal,
    Enum(Vec<std::string::String>),
}

/// カラム
#[derive(Debug, Clone)]
pub struct ModelColumn {
    pub name: std::string::String,
    pub col_type: ColumnType,
    pub is_pk: bool,
    pub is_unique: bool,
    pub is_indexed: bool,
    pub is_nullable: bool,
    pub default_val: Option<std::string::String>,
    pub label: Option<std::string::String>,
    pub is_fk: bool,
    pub fk_target: Option<std::string::String>,
    pub fk_optional: bool,
    pub fk_on_delete: Option<std::string::String>,
    pub fk_on_update: Option<std::string::String>,
    pub check_constraints: Vec<std::string::String>,
    pub is_soft_delete: bool,
    pub is_history: bool,
    pub is_tenant_scope: bool,
    pub derived_expr: Option<std::string::String>,
}
