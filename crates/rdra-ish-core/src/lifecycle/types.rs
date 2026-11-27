//! The transition system's data. Everything here is plain data that `build` fills in and that
//! the explorer and a printer read; no item here reads the `SemanticModel`.

use std::collections::BTreeMap;
use std::num::NonZeroU8;

use crate::location::LocatedSpan;
use crate::model::{ApiKey, BucKey, ConditionKey, EntityKey, EventKey, FkId, UseCaseKey};

/// Bounds are an input to the semantics, not a printer option. Every report states them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Slots of every entity `per_entity` does not name.
    pub instances: NonZeroU8,
    /// Int domain is `0..=int_max`; `now` ticks inside it.
    pub int_max: u8,
    /// Slots of one entity, by entity id (`--instances Loan=4`). The build resolves them
    /// into `EntityDecl::slots`; an id no entity has is the caller's error to report.
    pub per_entity: BTreeMap<String, NonZeroU8>,
}

impl Bounds {
    /// One abstract instance per entity, as `states` has always reported.
    pub const STATES: Bounds = Bounds::uniform(NonZeroU8::MIN, 5);
    /// Two instances per entity, the TLA export default.
    pub const VERIFY: Bounds = Bounds::uniform(
        match NonZeroU8::new(2) {
            Some(n) => n,
            None => unreachable!(),
        },
        5,
    );

    /// `instances` slots of every entity.
    pub const fn uniform(instances: NonZeroU8, int_max: u8) -> Bounds {
        Bounds {
            instances,
            int_max,
            per_entity: BTreeMap::new(),
        }
    }

    /// The slots of the entity with id `id`.
    pub fn of(&self, id: &str) -> NonZeroU8 {
        self.per_entity.get(id).copied().unwrap_or(self.instances)
    }

    /// The largest slot count of any entity.
    pub fn most(&self) -> NonZeroU8 {
        self.per_entity
            .values()
            .copied()
            .fold(self.instances, std::cmp::max)
    }

    /// The same bounds at `instances` slots for every entity, without the per-entity counts.
    pub fn at(&self, instances: NonZeroU8) -> Bounds {
        Bounds::uniform(instances, self.int_max)
    }

    /// ` (Loan=4, Member=1)` after a bounds line, or nothing without per-entity counts.
    pub fn per_entity_note(&self) -> String {
        match self.per_entity.is_empty() {
            true => String::new(),
            false => format!(
                " ({})",
                self.per_entity
                    .iter()
                    .map(|(id, n)| format!("{id}={n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub(crate) u16);

        impl $name {
            pub fn index(self) -> usize {
                usize::from(self.0)
            }

            pub(crate) fn at(index: usize) -> Self {
                $name(u16::try_from(index).expect("lifecycle id overflow"))
            }

            /// The id at an index a system's own list gave out (`var_ids`, `action_ids`).
            pub fn at_index(index: usize) -> Self {
                Self::at(index)
            }
        }
    };
}

id_type!(EntId);
id_type!(VarId);
id_type!(ActId);

/// How an entity's slots start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Birth {
    /// Some use case creates it: every slot starts absent.
    Created,
    /// No use case creates it (master data): every slot starts present at base.
    PreExisting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityDecl {
    pub key: EntityKey,
    pub id: String,
    pub label: String,
    pub birth: Birth,
    /// The entity's slots under the system's bounds (`Bounds::of`).
    pub slots: NonZeroU8,
    pub exists: VarId,
    /// View axes: state columns in declaration order, then proposition vars.
    pub columns: Vec<VarId>,
    /// One per relate where this entity holds the foreign key.
    pub owners: Vec<VarId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarDecl {
    pub kind: VarKind,
    pub domain: Domain,
    /// Value of an absent or deleted slot, and the creation default.
    pub base: Value,
    pub start: Start,
    pub site: Site,
    /// `@end(..)` values of a column: a slot may stay at one for good.
    pub ends: Vec<Value>,
}

/// The value a var holds in the initial states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    Base,
    /// Any domain value: a variation column of a pre-existing entity (vision rule 5), or the
    /// owner of a pre-existing N:1 child.
    Any,
    /// The slot's own index: the owner of a pre-existing 1:1 child, so no two children share
    /// a parent at the start.
    Identity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarKind {
    Exists(EntId),
    Column {
        entity: EntId,
        column: String,
        /// `transitions` names this column.
        lifecycle: bool,
        /// Typed by a `variation`: the actor chooses the value.
        variation: bool,
    },
    /// A comparison proposition a `drives(UC, E, cmp, bool)` names.
    Prop {
        entity: EntId,
        key: String,
    },
    /// Index of the parent slot this child slot belongs to, for one foreign key.
    Owner {
        child: EntId,
        parent: EntId,
        fk: FkId,
        /// The key's `.role(..)`, which names the var in place of the parent.
        role: Option<String>,
    },
    /// The run mark of a use case split into k transaction groups, on each slot of an
    /// entity a step before the last writes: the last step of a pending run that wrote the
    /// row (`Domain::Enum` of `none`, `1`, .., `k-1`). Not a view axis, and a delete does
    /// not reset it.
    Run {
        entity: EntId,
        /// The use case id.
        usecase: String,
    },
    /// Whether `event` was raised on this row (docs/state-derivation.md, "Triggers"): the
    /// last step of every use case that raises it sets it, the first step of every use case
    /// the event triggers reads it, nothing clears it, a delete does not reset it (as `Run`),
    /// and it is not a view axis. `Domain::Bool`, base `FALSE`.
    ///
    /// With `handler`, the counter of one counted edge `triggers(Ev, UC).counted(k)`
    /// (docs/design/counted-events.md): `Domain::Enum` of `0`..`k` and `over`, base `0`; the
    /// last step of a raiser adds one, the first step of the handler takes one, and a raise
    /// at `k` moves it to `over`, which stays.
    Raised {
        entity: EntId,
        /// The event id.
        event: String,
        /// The use case id of a counted edge's handler; `None` for the latch.
        handler: Option<String>,
    },
    Now,
}

impl VarKind {
    /// The entity whose slots the var has one byte per; `None` for `now`.
    pub fn entity(&self) -> Option<EntId> {
        match *self {
            VarKind::Exists(e) => Some(e),
            VarKind::Column { entity, .. }
            | VarKind::Prop { entity, .. }
            | VarKind::Run { entity, .. }
            | VarKind::Raised { entity, .. } => Some(entity),
            VarKind::Owner { child, .. } => Some(child),
            VarKind::Now => None,
        }
    }
}

/// Finite by construction: every domain has a size under `Bounds`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Domain {
    /// absent | present | deleted
    Existence,
    /// Declared values in declared casing and order.
    Enum(Vec<String>),
    /// null | the declared values: an `Enum` column marked `@null`. As in `NullableInt`, null
    /// is value 0 and declared value `i` is value `i + 1`.
    NullableEnum(Vec<String>),
    Bool,
    /// null | false | true: a `Bool` column marked `@null`, null first as in `NullableEnum`.
    NullableBool,
    /// null | present
    Nullable,
    /// 0..=int_max
    Int,
    /// null | 0..=int_max
    NullableInt,
    /// A slot of the parent entity; `optional` adds `null` (not linked) as value 0.
    Instance {
        parent: EntId,
        optional: bool,
    },
}

impl Domain {
    /// How many values the domain has in `ts`: an owner's domain is its parent's slots.
    pub fn size(&self, ts: &super::TransitionSystem) -> usize {
        let ints = usize::from(ts.bounds.int_max) + 1;
        match self {
            Domain::Existence => 3,
            Domain::Enum(values) => values.len(),
            Domain::NullableEnum(values) => values.len() + 1,
            Domain::Bool | Domain::Nullable => 2,
            Domain::NullableBool => 3,
            Domain::Int => ints,
            Domain::NullableInt => ints + 1,
            Domain::Instance { parent, optional } => ts.slots(*parent) + usize::from(*optional),
        }
    }

    /// The integer a value stands for, for Int-typed domains.
    pub fn number(&self, value: Value) -> Option<i64> {
        match self {
            Domain::Int => Some(i64::from(value.0)),
            Domain::NullableInt => (value.0 > 0).then(|| i64::from(value.0) - 1),
            _ => None,
        }
    }

    /// The value standing for an integer, when the domain holds it under `bounds`.
    pub fn of_number(&self, n: i64, bounds: &Bounds) -> Option<Value> {
        let max = i64::from(bounds.int_max);
        if !(0..=max).contains(&n) {
            return None;
        }
        match self {
            // No value past a byte: none for the top of a nullable Int at 255, which the build
            // then reports (`DomainTooLarge`, `IntLiteralOutOfBounds`), never a wrap to null.
            Domain::Int => u8::try_from(n).ok().map(Value),
            Domain::NullableInt => u8::try_from(n + 1).ok().map(Value),
            _ => None,
        }
    }

    /// The owner value naming a parent slot.
    pub fn of_slot(&self, slot: u8) -> Value {
        match self {
            Domain::Instance { optional: true, .. } => Value(slot + 1),
            _ => Value(slot),
        }
    }

    /// The parent slot an owner value names; `None` for an unlinked optional owner.
    pub fn slot(&self, value: Value) -> Option<u8> {
        match self {
            Domain::Instance { optional: true, .. } => value.0.checked_sub(1),
            _ => Some(value.0),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Domain::Int | Domain::NullableInt)
    }

    /// The domain has a null value, `Value::NULL`: a column marked `@null`.
    pub fn has_null(&self) -> bool {
        matches!(
            self,
            Domain::Nullable | Domain::NullableInt | Domain::NullableEnum(_) | Domain::NullableBool
        )
    }

    /// The declared values of an Enum domain, with or without null.
    pub fn names(&self) -> Option<&[String]> {
        match self {
            Domain::Enum(values) | Domain::NullableEnum(values) => Some(values),
            _ => None,
        }
    }

    /// The value of declared value `i` of an Enum domain (`false` is 0 and `true` 1 of a Bool
    /// one): `i`, or `i + 1` past the null of a nullable one.
    pub fn of_variant(&self, i: usize) -> Value {
        let shift = usize::from(matches!(
            self,
            Domain::NullableEnum(_) | Domain::NullableBool
        ));
        Value::at(i + shift)
    }

    /// `of_variant`, or `None` for a declared value past a byte, which only a domain the build
    /// rejects (`DomainTooLarge`) has.
    fn variant_value(&self, i: usize) -> Option<Value> {
        let shift = usize::from(matches!(
            self,
            Domain::NullableEnum(_) | Domain::NullableBool
        ));
        u8::try_from(i + shift).ok().map(Value)
    }

    /// The declared value an Enum or Bool value stands for, as `of_variant` numbers it; `None`
    /// for null.
    pub fn variant(&self, value: Value) -> Option<usize> {
        match self {
            Domain::NullableEnum(_) | Domain::NullableBool => {
                value.0.checked_sub(1).map(usize::from)
            }
            _ => Some(usize::from(value.0)),
        }
    }

    /// The value of the declared value `name` of an Enum domain, matched exactly or else
    /// ignoring ASCII case.
    pub fn of_name(&self, name: &str) -> Option<Value> {
        let values = self.names()?;
        values
            .iter()
            .position(|v| v == name)
            .or_else(|| values.iter().position(|v| v.eq_ignore_ascii_case(name)))
            .and_then(|i| self.variant_value(i))
    }

    /// The name of an Enum value; `None` for null or a value of another domain.
    pub fn name(&self, value: Value) -> Option<&str> {
        self.names()?.get(self.variant(value)?).map(String::as_str)
    }

    /// The value standing for `b` in a Bool domain.
    pub fn of_bool(&self, b: bool) -> Value {
        self.of_variant(usize::from(b))
    }

    /// The truth a Bool value stands for; `None` for null or a value of another domain.
    pub fn truth(&self, value: Value) -> Option<bool> {
        match self {
            Domain::Bool | Domain::NullableBool => self.variant(value).map(|i| i == 1),
            _ => None,
        }
    }

    /// The domain without its null: what a column holds while it is present.
    pub fn without_null(&self) -> Domain {
        match self {
            Domain::NullableEnum(values) => Domain::Enum(values.clone()),
            Domain::NullableBool => Domain::Bool,
            Domain::NullableInt => Domain::Int,
            other => other.clone(),
        }
    }

    /// Two columns of these domains compare with `==` / `!=`: both Int-typed, or of one
    /// domain apart from null.
    pub fn compares_with(&self, other: &Domain) -> bool {
        (self.is_numeric() && other.is_numeric()) || self.without_null() == other.without_null()
    }
}

/// A value is a domain index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Value(pub u8);

impl Value {
    /// The value of index `i` of a domain. A value is a byte, and a domain of more than 256
    /// values is a build error (`DomainTooLarge`) whose column gets no var, so no index past
    /// 255 reaches here: one that does is a bug, and panics rather than wrap onto another value.
    pub fn at(i: usize) -> Value {
        match u8::try_from(i) {
            Ok(v) => Value(v),
            Err(_) => {
                panic!("value index {i} past a byte: a domain of more than 256 values was built")
            }
        }
    }

    pub const ABSENT: Value = Value(0);
    pub const PRESENT: Value = Value(1);
    pub const DELETED: Value = Value(2);
    pub const FALSE: Value = Value(0);
    pub const TRUE: Value = Value(1);
    pub const NULL: Value = Value(0);
}

/// Which model fact an item of the system came from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelRef {
    /// Index into `SemanticModel::state_transitions`.
    Transition(usize),
    ColumnEffect(usize),
    /// Index into `SemanticModel::links`.
    Link(usize),
    /// Index into `SemanticModel::column_choices`.
    ColumnChoice(usize),
    PropositionEffect(usize),
    Forbidden(usize),
    EntityInvariant(usize),
    Required(usize),
    Exclusive(usize),
    CrossForbidden(usize),
    CrossInvariant(usize),
    Quantifier(usize),
    TemporalAssertion(usize),
    TemporalProperty(usize),
    /// Index into `SemanticModel::relations`.
    Relation(usize),
    Column {
        entity: EntityKey,
        column: String,
    },
    Entity(EntityKey),
    UseCase(UseCaseKey),
    /// A condition's `holds` body.
    Condition(ConditionKey),
    /// Added by the system itself (the clock).
    Synthesized,
}

/// Provenance of a system item: the model fact and, when the source has one, its span.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Site {
    pub fact: ModelRef,
    pub span: Option<LocatedSpan>,
}

impl Site {
    pub fn new(fact: ModelRef, span: &Option<LocatedSpan>) -> Self {
        Site {
            fact,
            span: span.clone(),
        }
    }

    pub fn bare(fact: ModelRef) -> Self {
        Site { fact, span: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    /// The use case id, `<id>_<n>` for the n-th transaction group of a split use case, or
    /// `Tick`.
    pub name: String,
    /// The action's id in the system it was built in. `restrict_to` keeps it, so it names
    /// one action across a cone, its sub-lifecycle, and the whole system, where `name` may
    /// not (a use case `Go_1` beside a split use case `Go`).
    pub source: ActId,
    pub origin: ActionOrigin,
    /// The use case declaration, or `Synthesized` for `Tick`.
    pub site: Site,
    /// At most one written (`New` or `Existing`) param per entity, and at most one `Parent`
    /// param per key, so two keys to one parent read two rows.
    pub params: Vec<Param>,
    /// Conjunction, read on the pre-state.
    pub guards: Vec<Guard>,
    /// Simultaneous: read the pre-state, write the post-state.
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionOrigin {
    /// One atomic action per transaction group of the use case (vision rule 6).
    UseCase {
        uc: UseCaseKey,
        /// The first BUC that contains the use case, as provenance names it.
        buc: Option<BucKey>,
        /// The transaction group of a split use case, from 1; `None` for one of one group.
        step: Option<u8>,
        /// The tracked event this copy of the first step runs on, when several events
        /// trigger the use case (`<UC>_by_<Ev>`); `None` otherwise.
        trigger: Option<EventKey>,
    },
    /// The `now` tick; exists only when a predicate reads `now`.
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Param {
    pub entity: EntId,
    pub role: ParamRole,
    /// The invoked API whose `creates` makes the use case create this instance; `None` for a
    /// direct `creates` or any other role.
    pub via_api: Option<ApiKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamRole {
    /// The lowest-index absent slot.
    New,
    /// Any present slot.
    Existing,
    /// A present slot that a child of `key` belongs to; the action does not write it. Each
    /// key has its own, so two keys to one parent range over the parent's rows independently.
    Parent { key: FkId },
    /// A row that step `step` of the same run of this use case wrote, matched by its run mark
    /// (`VarKind::Run`) rather than by its existence. The action does not write it, reads it
    /// as the row of its entity, links it to the rows it writes, and advances its mark.
    Earlier { step: u8 },
    /// A present row of a mark entity that the use case a tracked event triggers does not
    /// otherwise bind, selected by the event's mark (`GuardWhy::Raised`) and, when one direct
    /// key joins the two entities, linked to the action's row of the other by it. The action
    /// does not write it.
    Triggered,
}

impl ParamRole {
    /// `New` or `Existing`: a row the action writes, not a parent it only reads.
    pub fn is_written(self) -> bool {
        matches!(self, ParamRole::New | ParamRole::Existing)
    }

    /// A written row, an `Earlier` one, or a `Triggered` one: the action's one row of its
    /// entity, which keys, condition atoms, copies, and after-asserts read.
    pub fn is_row(self) -> bool {
        !matches!(self, ParamRole::Parent { .. })
    }

    /// A param whose slot names the instance `.fair` is fair for: every role but `New`, whose
    /// slot `Fair_P` binds inside `WF_vars`, so the lowest absent slot moving on does not
    /// reset the instance's claim. The TLA+ printer and the liveness check both read this.
    pub fn keys_fair_instance(self) -> bool {
        self != ParamRole::New
    }

    /// What a slot's `exists` var must hold for a param of this role to bind it: absent for
    /// `New`, which takes the lowest such slot, present for a row that exists, and anything
    /// for `Earlier`, which its run mark matches whether or not the slot is present.
    pub fn binds_exists(self) -> Option<Value> {
        match self {
            ParamRole::New => Some(Value::ABSENT),
            ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                Some(Value::PRESENT)
            }
            ParamRole::Earlier { .. } => None,
        }
    }
}

/// A guard holds on the pre-state, or, when it reads `At::Next`, on the pre-state and the
/// successor together: such a guard filters the successors after the effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    pub pred: Pred,
    pub why: GuardWhy,
    pub site: Site,
}

impl Guard {
    pub fn reads_successor(&self) -> bool {
        self.pred.reads_next()
    }

    /// The var a `GuardWhy::Copy` guard pins in the successor. The guard constrains nothing
    /// else, so a system without that var drops it. The guard is `target' = source`, or, between
    /// an Enum or Bool column with null and one without, a disjunction of `target' = v /\
    /// source = v`; either way its first term is the target.
    pub fn copy_target(&self) -> Option<VarId> {
        if self.why != GuardWhy::Copy {
            return None;
        }
        match self.pred.terms().next() {
            Some(Term::Slot {
                var,
                at: At::Next(_),
            }) => Some(*var),
            _ => None,
        }
    }
}

impl Pred {
    /// Every node of the predicate, itself first, then its parts left to right.
    pub fn nodes(&self) -> impl Iterator<Item = &Pred> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let pred = stack.pop()?;
            match pred {
                Pred::True | Pred::False | Pred::Cmp(..) | Pred::In(..) => {}
                Pred::Not(body)
                | Pred::Forall { body, .. }
                | Pred::Exists { body, .. }
                | Pred::Count { body, .. } => stack.push(body),
                Pred::And(parts) | Pred::Or(parts) => stack.extend(parts.iter().rev()),
            }
            Some(pred)
        })
    }

    /// Every term of the predicate, left to right.
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.nodes().flat_map(|pred| {
            let (first, second, rest): (_, _, &[Term]) = match pred {
                Pred::Cmp(a, _, b) => (Some(a), Some(b), &[]),
                Pred::In(a, vs) => (Some(a), None, vs),
                _ => (None, None, &[]),
            };
            first.into_iter().chain(second).chain(rest)
        })
    }

    /// Whether some slot reference points into the successor. The guard scan and the paths
    /// without a plan (`pre_guards_hold`) ask this per guard and binding, so it walks the tree
    /// in place: `terms` keeps a stack on the heap.
    pub fn reads_next(&self) -> bool {
        let next = |t: &Term| {
            matches!(
                t,
                Term::Slot {
                    at: At::Next(_),
                    ..
                } | Term::Index(At::Next(_))
            )
        };
        match self {
            Pred::True | Pred::False => false,
            Pred::Cmp(a, _, b) => next(a) || next(b),
            Pred::In(a, values) => next(a) || values.iter().any(next),
            Pred::Not(body)
            | Pred::Forall { body, .. }
            | Pred::Exists { body, .. }
            | Pred::Count { body, .. } => body.reads_next(),
            Pred::And(parts) | Pred::Or(parts) => parts.iter().any(Pred::reads_next),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardWhy {
    /// `transitions(UC, E.col, from -> to)`: the column is one of the `from` values.
    TransitionFrom,
    /// The child param belongs to the parent param, or is unlinked before a created parent
    /// links it.
    Owner,
    /// `links`, `relinks` or `unlinks(UC, Child, Parent)`, by the kind of the
    /// `ModelRef::Link` at its site: the child param is unlinked on the key before a `links`,
    /// linked before a `relinks`, and linked (to the bound parent, when the step writes one)
    /// before an `unlinks`.
    Link,
    /// `now` has room to tick.
    Clock,
    /// `holds(Cond, ..)` of a condition that `applies_to` the use case. `later` is the entity
    /// it reads that only a later transaction group of the use case writes
    /// (`GuardReadsLaterStep`).
    Condition {
        cond: ConditionKey,
        later: Option<EntityKey>,
    },
    /// `transitions(..).when(Cond)` or `chooses(.., value).when(Cond)`: the successor takes an
    /// arm, or a value, of the column whose guard held on the pre-state. `cond` is the first
    /// guarded arm's or value's condition.
    ArmCondition { cond: ConditionKey },
    /// `sets(UC, E.column == F.source)`: the successor's column holds the source row's value.
    Copy,
    /// The run mark of the bound row is `step`: `0` on a row step 1 takes (in no run), `i`
    /// on an `Earlier` row step `i + 1` continues.
    Run { step: u8 },
    /// `<E>._raised_<Ev>` of the bound row is `true`: the event was raised on it, so the use
    /// case `triggers(Ev, UC)` starts may run on it.
    Raised { event: EventKey },
    /// `deletes(UC, Parent)` over a key whose `.on_delete` is `restrict`, `no_action`, or
    /// unset (the SQL default): no present child the step does not delete names the row, as
    /// the database rejects the delete otherwise.
    Restrict { fk: FkId },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Effect {
    /// exists := present; the slot's columns are already at base.
    Create { param: u8 },
    /// exists := deleted; columns reset to base, and keys keep the rows they name.
    Delete { param: u8 },
    Assign {
        var: VarId,
        param: u8,
        rhs: Rhs,
        site: Site,
    },
    /// var := the `to` of an arm whose `from` the pre-state holds. Arms that share a `from`
    /// are a choice: one successor per arm, forked like `Choose` after the other effects.
    /// A `GuardWhy::ArmCondition` guard of the action filters the successors.
    Transition {
        var: VarId,
        param: u8,
        arms: Vec<Arm>,
    },
    /// One successor per value. A `GuardWhy::ArmCondition` guard of the action filters the
    /// successors.
    Choose {
        var: VarId,
        param: u8,
        values: Vec<Choice>,
    },
    /// now := now + 1 while now < int_max.
    Tick,
}

impl Action {
    /// Each `(var, param)` slot that two of the action's effects write (`Assign`,
    /// `Transition`, or `Choose`), once. The builder makes none: an action writes each slot
    /// at most once (`ConflictingEffects`, `DoubleModeledEnum`, a `chooses` that a write
    /// already covers is dropped, a counted trigger never counts its own raise). The native
    /// step and the TLA+ printer resolve two writes of one slot differently: the native step
    /// applies them in order and forks a choice after the rest, the printer keeps the first
    /// choice and lets a later `Transition` with a choice replace an earlier one. So
    /// `TransitionSystem::build` fails a model whose actions break the rule.
    pub fn slots_written_twice(&self) -> Vec<(VarId, u8)> {
        let mut seen: Vec<(VarId, u8)> = Vec::new();
        let mut twice: Vec<(VarId, u8)> = Vec::new();
        for eff in &self.effects {
            let slot = match eff {
                Effect::Assign { var, param, .. }
                | Effect::Transition { var, param, .. }
                | Effect::Choose { var, param, .. } => (*var, *param),
                Effect::Create { .. } | Effect::Delete { .. } | Effect::Tick => continue,
            };
            if seen.contains(&slot) {
                if !twice.contains(&slot) {
                    twice.push(slot);
                }
            } else {
                seen.push(slot);
            }
        }
        twice
    }
}

/// One `transitions` arm of an `Effect::Transition`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Arm {
    pub from: Value,
    pub to: Value,
    /// The `transitions` line that declares it.
    pub site: Site,
    /// Its `.when` conditions, key and id: any of them enables it. Empty when unguarded.
    pub conds: Vec<(ConditionKey, String)>,
}

/// One value of an `Effect::Choose`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Choice {
    pub value: Value,
    /// The `sets(.., col == present)`, copy or `chooses` line that asks for the value. A
    /// creation's own choice of a variation value has none.
    pub site: Option<Site>,
    /// The `.when` conditions of a `chooses(.., value)` or guarded `sets` line, as
    /// `Arm::conds`. Empty when unguarded.
    pub conds: Vec<(ConditionKey, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rhs {
    Lit(Value),
    /// The slot index of that param (owner assignment).
    Param(u8),
}

/// Where a slot reference points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum At {
    /// An action parameter.
    Param(u8),
    /// The n-th binder of the enclosing invariant, assert, or property, outermost first.
    Bound(u8),
    /// An action parameter's slot in the successor. A condition atom on a created param reads
    /// it, so the condition constrains what the action creates.
    Next(u8),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Term {
    Slot {
        var: VarId,
        at: At,
    },
    /// A value of the other operand's domain.
    Lit(Value),
    /// An integer literal; compares numerically.
    Int(i64),
    /// The slot index of a bound entity (compared with an owner var).
    Index(At),
    Now,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pred {
    True,
    False,
    Cmp(Term, CmpOp, Term),
    /// `term` equals one of the values: `col in (a, b)`, which TLA+ writes as `\in {..}`.
    In(Term, Vec<Term>),
    Not(Box<Pred>),
    And(Vec<Pred>),
    Or(Vec<Pred>),
    /// Over present slots of one entity; the body sees it as the next `Bound`.
    Forall {
        entity: EntId,
        body: Box<Pred>,
    },
    Exists {
        entity: EntId,
        body: Box<Pred>,
    },
    /// The present slots of `entity` for which `body` holds, counted: `count op n`. The body
    /// sees the slot as the next `Bound`, as a `Forall`'s does. The build writes `Le`
    /// (`.at_most`), `Ge` (`.at_least`), and `Eq` (`.exactly`).
    Count {
        entity: EntId,
        body: Box<Pred>,
        op: CmpOp,
        n: u8,
    },
}

/// What some reachable state must show for a rule's holding to say anything: present rows
/// of the binders `pred` reads (of every binder, when it reads none) under which `pred`
/// holds. A rule with a premise no reachable state meets holds vacuously
/// (`StateDiag::VacuousRule`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Premise {
    /// Read under the rule's binders, as its body is. `True` asks only that the rows exist.
    pub pred: Pred,
    /// The conditions as the author wrote them; empty for `True`.
    pub text: String,
}

impl Premise {
    /// Only that rows of the binders exist.
    pub fn rows() -> Self {
        Premise {
            pred: Pred::True,
            text: String::new(),
        }
    }
}

/// A state constraint, closed over present slots of its binders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invariant {
    pub name: String,
    /// Universally bound entities, outermost first.
    pub binders: Vec<EntId>,
    pub body: Pred,
    /// The premises of the rule, each of which some reachable state must meet: the rows it
    /// ranges over, the guards of a `.when`, and each condition of a `forbidden` on its own.
    pub premises: Vec<Premise>,
    /// The constraint as the author wrote it, for diagnostics; its variant is the kind.
    pub text: InvText,
    /// The rule's `.label("...")`, which a violation names.
    pub label: Option<String>,
    pub site: Site,
}

/// The kind of an `InvText`, as a plain tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvKind {
    Forbidden,
    Invariant,
    Required,
    Exclusive,
    CrossForbidden,
    CrossInvariant,
    Quantifier,
    /// A 1:1 key or a unique column set: no two present rows agree on it.
    Unique,
    /// A child-side range `1..*` / `1..1`: every present parent slot has a present child.
    MinCard,
    /// A child-side range `lo..hi`, `hi` from 2 to 255: no present parent slot has more
    /// than `hi` present children.
    MaxCard,
    /// A column's `@check(expr)`: no present slot makes the expression false.
    Check,
    /// A key whose parent a step deletes and whose database rejects that delete: every
    /// present child names a present parent.
    DanglingKey,
    /// A counted edge's counter is never past its `k` (`EventOverflow_<E>_<Ev>_<UC>`).
    Overflow,
}

impl InvText {
    pub fn kind(&self) -> InvKind {
        match self {
            InvText::Forbidden(_) => InvKind::Forbidden,
            InvText::Invariant { .. } => InvKind::Invariant,
            InvText::Required(_) => InvKind::Required,
            InvText::Exclusive(_) => InvKind::Exclusive,
            InvText::CrossForbidden { .. } => InvKind::CrossForbidden,
            InvText::CrossInvariant { .. } => InvKind::CrossInvariant,
            InvText::Quantifier { .. } => InvKind::Quantifier,
            InvText::Unique { .. } | InvText::UniqueSet { .. } => InvKind::Unique,
            InvText::MinCard { .. } => InvKind::MinCard,
            InvText::MaxCard { .. } => InvKind::MaxCard,
            InvText::Check(_) => InvKind::Check,
            InvText::DanglingKey { .. } => InvKind::DanglingKey,
            InvText::Overflow { .. } => InvKind::Overflow,
        }
    }
}

/// A constraint and the parts of it that a violation message names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvText {
    Forbidden(String),
    Invariant {
        guards: String,
        requireds: String,
    },
    Required(String),
    Exclusive(String),
    CrossForbidden {
        entities: String,
        conditions: String,
    },
    CrossInvariant {
        entities: String,
        guards: String,
        requireds: String,
    },
    /// A column's `@check(expr)`: no present slot makes the expression false.
    Check(String),
    Quantifier {
        anchor: String,
        related: String,
        constraint: String,
    },
    /// `relate(child, parent, 1:1)`, by the key's role.
    Unique {
        child: String,
        parent: String,
        role: String,
    },
    /// A `@unique` column, a `@unique(a, b)`, or an N:M junction's pair of keys, by the
    /// columns of the set (`room_id, slot`).
    UniqueSet {
        entity: String,
        columns: String,
    },
    /// `relate(child, parent, 1..*:1)`, by the key's role.
    MinCard {
        child: String,
        parent: String,
        role: String,
    },
    /// A key whose parent a use case deletes and whose database rejects that delete
    /// (`restrict`, `no_action`, or no `.on_delete`), by the key's role.
    DanglingKey {
        child: String,
        parent: String,
        role: String,
    },
    /// `relate(child, parent, 0..3:1)`, by the key's role: at most `most` children.
    MaxCard {
        child: String,
        parent: String,
        role: String,
        most: u8,
    },
    /// `triggers(event, usecase).counted(cap)`: no row has more than `cap` raises pending.
    Overflow {
        event: String,
        usecase: String,
        cap: u8,
    },
}

/// `after(UC).assert(..)`: a post-state predicate checked after the last action of the use
/// case, so once per run of a split one, on the run's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AfterAssert {
    pub name: String,
    /// The use case id.
    pub anchor: String,
    pub steps: Vec<AfterStep>,
    pub requireds: String,
    /// The rule's `.label("...")`, which a violation names.
    pub label: Option<String>,
    pub site: Site,
}

/// The assert as one action's post-state reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AfterStep {
    pub act: ActId,
    /// Scope entities the action does not bind, universally bound in the post-state.
    pub binders: Vec<EntId>,
    /// `At::Param` reads the action's params in the post-state.
    pub post: Pred,
}

/// A temporal property. BFS does not check it while it explores; `explain_property`
/// evaluates it on a lasso another engine found, `ComponentRun::decide_property` decides
/// `always` and the forms `LtlKind::is_explored` names on an explored graph, and
/// `ComponentRun::decide_liveness` decides `eventually` and `leads_to` there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    /// Entities the formula is quantified over per instance, outermost first.
    pub binders: Vec<EntId>,
    /// `.along(..)`: the owner links that join the binders. It joins `present` in the
    /// formula's trigger, so the formula speaks only of linked rows. `Pred::True` without
    /// `.along`.
    pub link: Pred,
    pub formula: Ltl,
    /// `.fair(UC, ...)`: each action of these use cases is weakly fair per instance it acts
    /// on. The TLA+ printer finds the actions by `ActionOrigin`, so a restriction that drops
    /// or renumbers actions keeps the list valid.
    pub fair: Vec<UseCaseKey>,
    pub site: Site,
}

impl Property {
    /// Every predicate the property reads: the link, then the formula's operands. The
    /// columns of `unchanged` are `formula.columns()`.
    pub fn preds(&self) -> impl Iterator<Item = &Pred> {
        std::iter::once(&self.link).chain(self.formula.preds())
    }

    /// Whether `.fair` names the use case of `action`.
    pub fn is_fair(&self, action: &Action) -> bool {
        matches!(&action.origin, ActionOrigin::UseCase { uc, .. } if self.fair.contains(uc))
    }

    /// The fair instance a move of `action` bound to `params` is a step of: the slots of the
    /// params that key it (`ParamRole::keys_fair_instance`), in param order. `None` when
    /// `.fair` does not name the action's use case.
    pub fn fair_instance(&self, action: &Action, params: &[u8]) -> Option<Box<[u8]>> {
        self.is_fair(action).then(|| {
            action
                .params
                .iter()
                .zip(params)
                .filter(|(param, _)| param.role.keys_fair_instance())
                .map(|(_, slot)| *slot)
                .collect()
        })
    }
}

/// A `can_reach(goal)` property: from every reachable state where its instance is present
/// (and linked, under `.along`), some path on which it stays present reaches `goal`. That is
/// CTL `AG (present => E[present U (present /\ goal)])`, which no LTL formula states, so TLC
/// never checks it: BFS decides it on its cone's graph (`reach.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reach {
    pub name: String,
    /// Entities the goal is quantified over per instance, outermost first.
    pub binders: Vec<EntId>,
    /// `.along(..)`: the owner links that join the binders, read with `present`, so the
    /// property speaks only of linked rows. `Pred::True` without `.along`.
    pub link: Pred,
    pub goal: Pred,
    pub site: Site,
}

impl Reach {
    /// Every predicate the property reads: the link, then the goal.
    pub fn preds(&self) -> impl Iterator<Item = &Pred> {
        [&self.link, &self.goal].into_iter()
    }
}

/// A property's formula per binding; the trigger of each form is `present /\ link`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ltl {
    Always(Pred),
    Eventually(Pred),
    LeadsTo(Pred, Pred),
    /// `stable(p)`: a step from a state where the trigger and `p` hold to one where the
    /// instance is present keeps `p`; the link is read before the step only.
    Stable(Pred),
    /// `unchanged(cols).when(p)`: a step from a state where the trigger and `when` hold to
    /// one where the instance is present leaves each column as it is.
    Unchanged {
        when: Pred,
        columns: Vec<Term>,
    },
    /// `precedes(p, q)`: every state where the trigger and `q` hold has the trigger and `p`
    /// in it or in a state before it.
    Precedes(Pred, Pred),
}

impl Ltl {
    /// The operands, left to right. `unchanged`'s columns are `columns`.
    pub fn preds(&self) -> impl Iterator<Item = &Pred> {
        match self {
            Ltl::Always(x) | Ltl::Eventually(x) | Ltl::Stable(x) => std::iter::once(x).chain(None),
            Ltl::Unchanged { when, .. } => std::iter::once(when).chain(None),
            Ltl::LeadsTo(x, y) | Ltl::Precedes(x, y) => std::iter::once(x).chain(Some(y)),
        }
    }

    /// The columns `unchanged` keeps; empty for every other form.
    pub fn columns(&self) -> &[Term] {
        match self {
            Ltl::Unchanged { columns, .. } => columns,
            _ => &[],
        }
    }

    /// The same operator over `f` of each operand and `g` of each column.
    pub fn map(&self, mut f: impl FnMut(&Pred) -> Pred, g: impl FnMut(&Term) -> Term) -> Ltl {
        match self {
            Ltl::Always(x) => Ltl::Always(f(x)),
            Ltl::Eventually(x) => Ltl::Eventually(f(x)),
            Ltl::LeadsTo(x, y) => Ltl::LeadsTo(f(x), f(y)),
            Ltl::Stable(x) => Ltl::Stable(f(x)),
            Ltl::Unchanged { when, columns } => Ltl::Unchanged {
                when: f(when),
                columns: columns.iter().map(g).collect(),
            },
            Ltl::Precedes(x, y) => Ltl::Precedes(f(x), f(y)),
        }
    }

    pub fn kind(&self) -> LtlKind {
        match self {
            Ltl::Always(_) => LtlKind::Always,
            Ltl::Eventually(_) => LtlKind::Eventually,
            Ltl::LeadsTo(..) => LtlKind::LeadsTo,
            Ltl::Stable(_) => LtlKind::Stable,
            Ltl::Unchanged { .. } => LtlKind::Unchanged,
            Ltl::Precedes(..) => LtlKind::Precedes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LtlKind {
    Always,
    Eventually,
    LeadsTo,
    Stable,
    Unchanged,
    Precedes,
}

impl LtlKind {
    /// A counterexample may be a run that stops: only `eventually` and `leads_to` read
    /// `WF_vars(Next)`.
    pub fn is_liveness(self) -> bool {
        matches!(self, LtlKind::Eventually | LtlKind::LeadsTo)
    }

    /// `verify` decides it on the graph BFS explores for its cone, per edge (`stable`,
    /// `unchanged`) or per state with its latch (`precedes`), and starts no TLC run for it.
    pub fn is_explored(self) -> bool {
        matches!(
            self,
            LtlKind::Stable | LtlKind::Unchanged | LtlKind::Precedes
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nullable Enum or Bool domain puts null at value 0, as `NullableInt` does, and numbers
    /// the declared values one past it (#359).
    #[test]
    fn a_nullable_enum_or_bool_numbers_its_values_past_null() {
        let names = vec!["red".to_string(), "blue".to_string()];
        let plain = Domain::Enum(names.clone());
        let nullable = Domain::NullableEnum(names);
        assert_eq!(plain.of_name("blue"), Some(Value(1)));
        assert_eq!(nullable.of_name("blue"), Some(Value(2)));
        assert_eq!(nullable.of_name("BLUE"), Some(Value(2)));
        assert_eq!(nullable.of_name("green"), None);
        assert_eq!(nullable.name(Value::NULL), None);
        assert_eq!(nullable.name(Value(1)), Some("red"));
        assert_eq!(Domain::Bool.of_bool(true), Value::TRUE);
        assert_eq!(Domain::NullableBool.of_bool(false), Value(1));
        assert_eq!(Domain::NullableBool.of_bool(true), Value(2));
        assert_eq!(Domain::NullableBool.truth(Value::NULL), None);
        assert_eq!(Domain::NullableBool.truth(Value(2)), Some(true));
        assert!(nullable.has_null() && Domain::NullableBool.has_null());
        assert!(!plain.has_null() && !Domain::Bool.has_null());
        assert!(nullable.compares_with(&plain));
        assert!(Domain::NullableBool.compares_with(&Domain::Bool));
        assert!(!nullable.compares_with(&Domain::Bool));
        assert!(!Domain::NullableBool.compares_with(&Domain::Nullable));
    }
}
