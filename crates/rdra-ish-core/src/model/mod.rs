mod comparison;
mod constraints;
mod effects;
mod foreign_key;
mod instances;
mod keys;
mod refs;
mod symbol;
mod typed;

pub use comparison::*;
pub use constraints::*;
pub use effects::*;
pub use foreign_key::*;
pub use instances::*;
pub use keys::*;
pub use refs::*;
pub use symbol::*;
pub use typed::*;

use slotmap::SlotMap;
use std::collections::HashSet;

impl SemanticModel {
    /// The declared id of `node`, or `None` when its key no longer resolves.
    pub fn node_id(&self, node: &NodeRef) -> Option<&str> {
        match node {
            NodeRef::Actor(k) => self.actors.get(*k).map(|e| e.id.as_str()),
            NodeRef::ExtSystem(k) => self.ext_systems.get(*k).map(|e| e.id.as_str()),
            NodeRef::System(k) => self.systems.get(*k).map(|e| e.id.as_str()),
            NodeRef::Requirement(k) => self.requirements.get(*k).map(|e| e.id.as_str()),
            NodeRef::Adr(k) => self.adrs.get(*k).map(|e| e.id.as_str()),
            NodeRef::Goal(k) => self.goals.get(*k).map(|e| e.id.as_str()),
            NodeRef::Nfr(k) => self.nfrs.get(*k).map(|e| e.id.as_str()),
            NodeRef::Quality(k) => self.qualities.get(*k).map(|e| e.id.as_str()),
            NodeRef::Constraint(k) => self.constraints.get(*k).map(|e| e.id.as_str()),
            NodeRef::Business(k) => self.businesses.get(*k).map(|e| e.id.as_str()),
            NodeRef::Buc(k) => self.bucs.get(*k).map(|e| e.id.as_str()),
            NodeRef::Flow(k) => self.flows.get(*k).map(|e| e.id.as_str()),
            NodeRef::Step(k) => self.steps.get(*k).map(|e| e.id.as_str()),
            NodeRef::UseCase(k) => self.use_cases.get(*k).map(|e| e.id.as_str()),
            NodeRef::Screen(k) => self.screens.get(*k).map(|e| e.id.as_str()),
            NodeRef::Field(k) => self.fields.get(*k).map(|e| e.id.as_str()),
            NodeRef::Event(k) => self.events.get(*k).map(|e| e.id.as_str()),
            NodeRef::Entity(k) => self.entities.get(*k).map(|e| e.id.as_str()),
            NodeRef::State(k) => self.states.get(*k).map(|e| e.id.as_str()),
            NodeRef::Api(k) => self.apis.get(*k).map(|e| e.id.as_str()),
            NodeRef::Dto(k) => self.dtos.get(*k).map(|e| e.id.as_str()),
            NodeRef::Timer(k) => self.timers.get(*k).map(|e| e.id.as_str()),
            NodeRef::Medium(k) => self.media.get(*k).map(|e| e.id.as_str()),
            NodeRef::Permission(k) => self.permissions.get(*k).map(|e| e.id.as_str()),
            NodeRef::Variation(k) => self.variations.get(*k).map(|e| e.id.as_str()),
            NodeRef::Condition(k) => self.conditions.get(*k).map(|e| e.id.as_str()),
        }
    }

    /// The display label of `node`, or `None` when its key no longer resolves.
    pub fn node_label(&self, node: &NodeRef) -> Option<&str> {
        match node {
            NodeRef::Actor(k) => self.actors.get(*k).map(|e| e.label.as_str()),
            NodeRef::ExtSystem(k) => self.ext_systems.get(*k).map(|e| e.label.as_str()),
            NodeRef::System(k) => self.systems.get(*k).map(|e| e.label.as_str()),
            NodeRef::Requirement(k) => self.requirements.get(*k).map(|e| e.label.as_str()),
            NodeRef::Adr(k) => self.adrs.get(*k).map(|e| e.label.as_str()),
            NodeRef::Goal(k) => self.goals.get(*k).map(|e| e.label.as_str()),
            NodeRef::Nfr(k) => self.nfrs.get(*k).map(|e| e.label.as_str()),
            NodeRef::Quality(k) => self.qualities.get(*k).map(|e| e.label.as_str()),
            NodeRef::Constraint(k) => self.constraints.get(*k).map(|e| e.label.as_str()),
            NodeRef::Business(k) => self.businesses.get(*k).map(|e| e.label.as_str()),
            NodeRef::Buc(k) => self.bucs.get(*k).map(|e| e.label.as_str()),
            NodeRef::Flow(k) => self.flows.get(*k).map(|e| e.label.as_str()),
            NodeRef::Step(k) => self.steps.get(*k).map(|e| e.label.as_str()),
            NodeRef::UseCase(k) => self.use_cases.get(*k).map(|e| e.label.as_str()),
            NodeRef::Screen(k) => self.screens.get(*k).map(|e| e.label.as_str()),
            NodeRef::Field(k) => self.fields.get(*k).map(|e| e.label.as_str()),
            NodeRef::Event(k) => self.events.get(*k).map(|e| e.label.as_str()),
            NodeRef::Entity(k) => self.entities.get(*k).map(|e| e.label.as_str()),
            NodeRef::State(k) => self.states.get(*k).map(|e| e.label.as_str()),
            NodeRef::Api(k) => self.apis.get(*k).map(|e| e.label.as_str()),
            NodeRef::Dto(k) => self.dtos.get(*k).map(|e| e.label.as_str()),
            NodeRef::Timer(k) => self.timers.get(*k).map(|e| e.label.as_str()),
            NodeRef::Medium(k) => self.media.get(*k).map(|e| e.label.as_str()),
            NodeRef::Permission(k) => self.permissions.get(*k).map(|e| e.label.as_str()),
            NodeRef::Variation(k) => self.variations.get(*k).map(|e| e.label.as_str()),
            NodeRef::Condition(k) => self.conditions.get(*k).map(|e| e.label.as_str()),
        }
    }

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

    /// How a human-facing display writes `value` of `entity.column`: the label the column's
    /// inline `Enum` or variation gives that value, or the value as written when it has none.
    pub fn value_display<'a>(&'a self, entity: EntityKey, column: &str, value: &'a str) -> &'a str {
        let Some(col) = self
            .entities
            .get(entity)
            .and_then(|e| e.columns.iter().find(|c| c.name == column))
        else {
            return value;
        };
        let label = match &col.col_type {
            ColumnType::Variation(key) => self.variations.get(*key).and_then(|variation| {
                let index = variation.values.iter().position(|v| v == value)?;
                variation.labels.get(index)?.as_deref()
            }),
            _ => col.value_label(value),
        };
        label.unwrap_or(value)
    }

    /// `transitions` names this column of this entity, so it is a state axis.
    pub fn is_lifecycle_column(&self, entity: EntityKey, column: &str) -> bool {
        self.state_transitions
            .iter()
            .any(|t| t.entity == entity && t.column == column)
    }

    /// A `property` atom's entity: the one named by id, or else the only entity with `column`.
    pub fn property_entity(&self, entity: Option<&str>, column: &str) -> Option<EntityKey> {
        match entity {
            Some(id) => self
                .entities
                .iter()
                .find(|(_, e)| e.id == id)
                .map(|(k, _)| k),
            None => {
                let mut owners = self
                    .entities
                    .iter()
                    .filter(|(_, e)| e.columns.iter().any(|c| c.name == column))
                    .map(|(k, _)| k);
                let first = owners.next()?;
                owners.next().is_none().then_some(first)
            }
        }
    }
}

/// How other models may write a published entity: `.append()` adds rows, `.upsert()` also
/// replaces them. Neither allows `deletes` or `transitions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PublishMode {
    Append,
    Upsert,
}

impl PublishMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PublishMode::Append => "append",
            PublishMode::Upsert => "upsert",
        }
    }
}

/// Who may call an exposed use case from another model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CallerKind {
    #[default]
    Actor,
    System,
    Any,
}

impl CallerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CallerKind::Actor => "actor",
            CallerKind::System => "system",
            CallerKind::Any => "any",
        }
    }
}

/// The element one `exposes` line publishes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Exposed {
    UseCase {
        usecase: UseCaseKey,
        caller: CallerKind,
    },
    Event(EventKey),
    Entity {
        entity: EntityKey,
        mode: PublishMode,
    },
}

impl Exposed {
    pub fn target(&self) -> NodeRef {
        match self {
            Exposed::UseCase { usecase, .. } => NodeRef::UseCase(*usecase),
            Exposed::Event(event) => NodeRef::Event(*event),
            Exposed::Entity { entity, .. } => NodeRef::Entity(*entity),
        }
    }
}

/// One `exposes(System, X)` line.
#[derive(Debug, Clone)]
pub struct Exposure {
    pub system: SystemKey,
    pub item: Exposed,
    pub site: Option<crate::location::LocatedSpan>,
}

/// One `shares(System, Entity)` line: the owner lets other models name an internal entity.
#[derive(Debug, Clone)]
pub struct Share {
    pub system: SystemKey,
    pub entity: EntityKey,
    pub site: Option<crate::location::LocatedSpan>,
}

/// The published surface of a model, in declaration order. Each element is exposed at most
/// once and each entity shared at most once; `DuplicateExposure` rejects a second line.
#[derive(Debug, Default)]
pub struct Publication {
    pub exposures: Vec<Exposure>,
    pub shares: Vec<Share>,
}

impl Publication {
    pub fn is_empty(&self) -> bool {
        self.exposures.is_empty() && self.shares.is_empty()
    }

    pub fn exposes(&self, target: &NodeRef) -> bool {
        self.exposures.iter().any(|x| x.item.target() == *target)
    }

    /// The publish mode of an entity, or `None` for an internal one.
    pub fn mode_of(&self, entity: EntityKey) -> Option<PublishMode> {
        self.exposures.iter().find_map(|x| match x.item {
            Exposed::Entity { entity: e, mode } if e == entity => Some(mode),
            _ => None,
        })
    }

    pub fn is_published(&self, entity: EntityKey) -> bool {
        self.mode_of(entity).is_some()
    }

    /// The caller kind of an exposed use case, or `None` for an internal one.
    pub fn caller_of(&self, usecase: UseCaseKey) -> Option<CallerKind> {
        self.exposures.iter().find_map(|x| match x.item {
            Exposed::UseCase { usecase: u, caller } if u == usecase => Some(caller),
            _ => None,
        })
    }

    pub fn is_shared(&self, entity: EntityKey) -> bool {
        self.shares.iter().any(|s| s.entity == entity)
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
    pub goals: SlotMap<GoalKey, Goal>,
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
    /// One per relate whose child holds the key, in relation order. Read through
    /// `foreign_keys()`, `column_fk()` and `unique_keys()`.
    pub(crate) foreign_keys: Vec<ForeignKey>,
    /// `(junction, key to one end, key to the other)` of each `relate(A, B, N:M).via(J)`.
    pub(crate) junction_keys: Vec<(EntityKey, FkId, FkId)>,
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
    /// `links(UseCase, Child, Parent)`: an existing child linked to an existing parent.
    pub links: Vec<Link>,
    /// `chooses(UseCase, Entity.column)`: variation columns the actor picks again.
    pub column_choices: Vec<ColumnChoice>,
    /// `drives(origin, Entity.column op rhs, bool)` で宣言された比較命題の真偽効果
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
    /// `.when(Cond)` on a `forbidden` or an `invariant`, in source order. The body of each
    /// condition is spliced into its rule after every predicate is read.
    pub rule_guards: Vec<RuleGuard>,
    /// `property Id "..." leads_to/always/eventually(...)` 時相プロパティ
    pub temporal_properties: Vec<TemporalProperty>,
    /// What `exposes` and `shares` publish to other models.
    pub publication: Publication,
    pub symbols: SymbolTable,
    /// Per-file import / module visibility used when resolving references.
    pub import_scopes: crate::import_scope::ImportScopes,
    /// Declaration sites for `kind:id` lookups (LSP go-to-definition).
    pub decl_sites: crate::location::DeclIndex,
}
