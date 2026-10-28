use super::comparison::{ColumnCheck, DerivedExpr};
use super::foreign_key::RefAction;
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
    Business(BusinessKey),
    Buc(BucKey),
    Flow(FlowKey),
    Step(StepKey),
    UseCase(UseCaseKey),
    Screen(ScreenKey),
    Field(FieldKey),
    Event(EventKey),
    Entity(EntityKey),
    State(StateKey),
    Api(ApiKey),
    Dto(DtoKey),
    Timer(TimerKey),
    Medium(MediumKey),
    Permission(PermissionKey),
    Variation(VariationKey),
    Condition(ConditionKey),
}

/// 述語の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelKind {
    Performs,
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
    /// 条件 to バリエーション / 状態モデル。Derived from `condition_edges`.
    Combines,
    Qualifies,
    Constrains,
    MapsField,
    Owns,
    // Entity ER
    RelateOneToOne,   // 1:1
    RelateOneToMany,  // 1:N A側が1 == B側がMany
    RelateManyToOne,  // N:1 A側がMany == B側が1 → A に FK
    RelateManyToMany, // N:M — requires `.via(Junction)` for FK generation
}

/// リレーション（from, to, kind）
#[derive(Debug, Clone)]
pub struct Relation {
    pub from: NodeRef,
    pub to: NodeRef,
    pub kind: RelKind,
    /// Read by `analysis::fk` only; consumers read the resulting `ForeignKey`.
    pub(crate) options: RelationOptions,
    /// Junction entity for `relate(A, B, N:M).via(Junction)`.
    pub via: Option<NodeRef>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}

/// What a `relate` says about its key: `.role(..)`, `.key(..)`, `.optional()`,
/// `.on_delete(..)`, `.on_update(..)`, and the low end of a child-side range (`1..*:1`).
#[derive(Debug, Clone, Default)]
pub struct RelationOptions {
    pub role: Option<String>,
    /// Author columns of the child that hold the key, in parent key order. Empty generates them.
    pub key: Vec<String>,
    pub optional: bool,
    /// Children per parent at least: `One` for `1..*` / `1..1` on the child side.
    pub children_min: super::foreign_key::Min,
    pub on_delete: Option<RefAction>,
    pub on_update: Option<RefAction>,
}

/// `coordinates(usecase, entity, entity)` で宣言される、system境界越えrelationの調停責務。
#[derive(Debug, Clone)]
pub struct BoundaryCoordination {
    pub usecase: UseCaseKey,
    pub left: EntityKey,
    pub right: EntityKey,
}

/// `belongs(Buc, Business).by(...)` で宣言される、
/// Business と BUC の対応関係に付く文脈値。
#[derive(Debug, Clone)]
pub enum BusinessMappingContextValue {
    Text(std::string::String),
    Ref(NodeRef),
}

/// Business と BUC の対応関係に付く By 文脈。
#[derive(Debug, Clone)]
pub struct BusinessMappingContext {
    pub buc: BucKey,
    pub business: BusinessKey,
    pub bys: Vec<BusinessMappingContextValue>,
}

/// Mapping between a screen field and a logical data model column.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    pub field: FieldKey,
    pub entity: EntityKey,
    pub column: std::string::String,
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

/// `performs(actor, ...)` の第2引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PerformTarget {
    UseCase(UseCaseKey),
    Buc(BucKey),
    Step(StepKey),
}

impl PerformTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(PerformTarget::UseCase(*k)),
            NodeRef::Buc(k) => Some(PerformTarget::Buc(*k)),
            NodeRef::Step(k) => Some(PerformTarget::Step(*k)),
            _ => None,
        }
    }
}

/// `raises(raiser, event)` の第1引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Raiser {
    UseCase(UseCaseKey),
    ExtSystem(ExtSystemKey),
}

impl Raiser {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(Raiser::UseCase(*k)),
            NodeRef::ExtSystem(k) => Some(Raiser::ExtSystem(*k)),
            _ => None,
        }
    }
}

/// `triggers(source, ...)` の第1引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TriggerSource {
    Event(EventKey),
    Timer(TimerKey),
}

impl TriggerSource {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Event(k) => Some(TriggerSource::Event(*k)),
            NodeRef::Timer(k) => Some(TriggerSource::Timer(*k)),
            _ => None,
        }
    }
}

/// `triggers(..., target)` の第2引数。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TriggerTarget {
    UseCase(UseCaseKey),
    Buc(BucKey),
    ExtSystem(ExtSystemKey),
}

impl TriggerTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(TriggerTarget::UseCase(*k)),
            NodeRef::Buc(k) => Some(TriggerTarget::Buc(*k)),
            NodeRef::ExtSystem(k) => Some(TriggerTarget::ExtSystem(*k)),
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
    Screen(ScreenKey),
}

impl ContainerRef {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Buc(k) => Some(ContainerRef::Buc(*k)),
            NodeRef::System(k) => Some(ContainerRef::System(*k)),
            NodeRef::Flow(k) => Some(ContainerRef::Flow(*k)),
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
            _ => None,
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

/// `applies_to(source, ...)` の source 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppliesSource {
    Nfr(NfrKey),
    Condition(ConditionKey),
}

impl AppliesSource {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::Nfr(k) => Some(AppliesSource::Nfr(*k)),
            NodeRef::Condition(k) => Some(AppliesSource::Condition(*k)),
            _ => None,
        }
    }
}

/// `combines(condition, target)` の target 側。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombinesTarget {
    Variation(VariationKey),
    /// A lifecycle column of an entity (RDRA 状態モデル).
    StateModel {
        entity: EntityKey,
        column: std::string::String,
    },
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
    Timer(TimerKey),
}

impl CoversTarget {
    pub fn from_node_ref(node: &NodeRef) -> Option<Self> {
        match node {
            NodeRef::UseCase(k) => Some(CoversTarget::UseCase(*k)),
            NodeRef::Api(k) => Some(CoversTarget::Api(*k)),
            NodeRef::Event(k) => Some(CoversTarget::Event(*k)),
            NodeRef::Timer(k) => Some(CoversTarget::Timer(*k)),
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
    /// Inline value set. With `transitions` it is a state axis.
    Enum(Vec<std::string::String>),
    /// Attribute column typed by a `variation`. Never a lifecycle column, never a state axis.
    Variation(VariationKey),
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
    pub check_constraints: Vec<ColumnCheck>,
    pub is_soft_delete: bool,
    pub is_history: bool,
    pub is_tenant_scope: bool,
    pub derived_expr: Option<DerivedExpr>,
}

impl ModelColumn {
    /// The `@check` expressions in canonical text, `|`-joined (CSV cells).
    pub fn checks_display(&self) -> std::string::String {
        self.check_constraints
            .iter()
            .map(|c| c.expr.display())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// The `@derived` expression in canonical text, or empty.
    pub fn derived_display(&self) -> std::string::String {
        self.derived_expr
            .as_ref()
            .map(DerivedExpr::display)
            .unwrap_or_default()
    }
}
