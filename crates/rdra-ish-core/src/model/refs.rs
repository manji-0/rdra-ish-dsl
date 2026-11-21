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
    Goal(GoalKey),
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

impl NodeRef {
    /// Kind name as the DSL and every output write it (`"actor"`, `"usecase"`, ...).
    pub fn kind_tag(&self) -> &'static str {
        match self {
            NodeRef::Actor(_) => "actor",
            NodeRef::ExtSystem(_) => "extsystem",
            NodeRef::System(_) => "system",
            NodeRef::Requirement(_) => "requirement",
            NodeRef::Adr(_) => "adr",
            NodeRef::Goal(_) => "goal",
            NodeRef::Nfr(_) => "nfr",
            NodeRef::Quality(_) => "quality",
            NodeRef::Constraint(_) => "constraint",
            NodeRef::Business(_) => "business",
            NodeRef::Buc(_) => "buc",
            NodeRef::Flow(_) => "flow",
            NodeRef::Step(_) => "step",
            NodeRef::UseCase(_) => "usecase",
            NodeRef::Screen(_) => "screen",
            NodeRef::Field(_) => "field",
            NodeRef::Event(_) => "event",
            NodeRef::Entity(_) => "entity",
            NodeRef::State(_) => "state",
            NodeRef::Api(_) => "api",
            NodeRef::Dto(_) => "dto",
            NodeRef::Timer(_) => "timer",
            NodeRef::Medium(_) => "medium",
            NodeRef::Permission(_) => "permission",
            NodeRef::Variation(_) => "variation",
            NodeRef::Condition(_) => "condition",
        }
    }
}

/// 述語の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelKind {
    Performs,
    Reads,
    Creates,
    Updates,
    Deletes,
    Displays,
    Shows,
    /// screen → screen; `via` holds the use case that causes the move.
    Navigates,
    Raises,
    Triggers,
    Contains,
    Belongs,
    HasPermission,
    RequiresPermission,
    RequiresMedium,
    Motivates,
    Achieves,
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
    /// 条件 to the use case one of whose `transitions` arms it guards with `.when(Cond)`.
    /// Derived from `StateTransition::guard`.
    Guards,
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
    /// Junction entity for `relate(A, B, N:M).via(Junction)`, or the use case of
    /// `navigates(Screen, Screen).via(UseCase)`.
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
    /// `.references(a, b)`: the parent's unique columns the key refers to. Empty is the
    /// parent's primary key.
    pub references: Vec<String>,
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

/// `maps_field(Dto.field, Entity.column)`: a DTO field that carries a column's value.
#[derive(Debug, Clone)]
pub struct DtoMapping {
    pub dto: DtoKey,
    pub field: std::string::String,
    pub entity: EntityKey,
    pub column: std::string::String,
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

/// What a condition combines: the target of `combines`, and each link `condition_edges` derives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombinesTarget {
    Variation(VariationKey),
    /// A lifecycle column of an entity (RDRA 状態モデル).
    StateModel {
        entity: EntityKey,
        column: std::string::String,
    },
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
    /// For an inline `Enum`, `value_labels[i]` is the optional label of its `i`-th value.
    pub value_labels: Vec<Option<std::string::String>>,
    /// `@end(..)`: values a row may stay at for good, so staying there is no deadlock.
    pub end_values: Vec<std::string::String>,
    /// `String(255)`, `Decimal(10, 2)`, `DateTime(tz)`: what the schema stores. The lifecycle
    /// does not read it.
    pub detail: TypeDetail,
    /// `@generated(..)`: how a key's value is made.
    pub generated: Option<IdGeneration>,
    /// `@default(now())` / `@default(uuid())`: a default the database computes.
    pub default_fn: Option<DefaultFn>,
    /// `@on_update(now())`: the value each update of the row writes.
    pub on_update: Option<DefaultFn>,
    /// `@version`: the row's version, which each update checks and advances.
    pub is_version: bool,
}

/// The storage detail a column type's arguments give.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeDetail {
    /// `String(n)`: at most `n` characters.
    pub length: Option<u32>,
    /// `Decimal(p, s)` / `Money(p, s)`: `p` digits, `s` of them after the point.
    pub precision: Option<u32>,
    pub scale: Option<u32>,
    /// `DateTime(tz)` is `Some(true)`, `DateTime(local)` is `Some(false)`.
    pub time_zone: Option<bool>,
}

/// `@generated(..)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdGeneration {
    /// The database numbers the rows.
    Identity,
    /// A random UUID.
    Uuid,
    /// A ULID, which the application makes.
    Ulid,
}

impl IdGeneration {
    pub fn as_str(self) -> &'static str {
        match self {
            IdGeneration::Identity => "identity",
            IdGeneration::Uuid => "uuid",
            IdGeneration::Ulid => "ulid",
        }
    }
}

/// A function a default or an update calls: `now()` or `uuid()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultFn {
    Now,
    Uuid,
}

impl DefaultFn {
    pub fn as_str(self) -> &'static str {
        match self {
            DefaultFn::Now => "now()",
            DefaultFn::Uuid => "uuid()",
        }
    }
}

impl ModelColumn {
    /// The label an inline `Enum` gives `value`, if any.
    pub fn value_label(&self, value: &str) -> Option<&str> {
        let ColumnType::Enum(values) = &self.col_type else {
            return None;
        };
        let index = values.iter().position(|v| v == value)?;
        self.value_labels.get(index)?.as_deref()
    }

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
