//! The transition system's data. Everything here is plain data that `build` fills in and that
//! the explorer and a printer read; no item here reads the `SemanticModel`.

use std::num::NonZeroU8;

use crate::location::LocatedSpan;
use crate::model::{BucKey, ConditionKey, EntityKey, FkId, UseCaseKey};

/// Bounds are an input to the semantics, not a printer option. Every report states them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Slots per entity.
    pub instances: NonZeroU8,
    /// Int domain is `0..=int_max`; `now` ticks inside it.
    pub int_max: u8,
}

impl Bounds {
    /// One abstract instance per entity, as `states` has always reported.
    pub const STATES: Bounds = Bounds {
        instances: NonZeroU8::MIN,
        int_max: 5,
    };
    /// Two instances per entity, the TLA export default.
    pub const VERIFY: Bounds = Bounds {
        instances: match NonZeroU8::new(2) {
            Some(n) => n,
            None => unreachable!(),
        },
        int_max: 5,
    };

    pub fn slots(self) -> usize {
        usize::from(self.instances.get())
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

#[derive(Clone, Debug)]
pub struct EntityDecl {
    pub key: EntityKey,
    pub id: String,
    pub label: String,
    pub birth: Birth,
    pub exists: VarId,
    /// View axes: state columns in declaration order, then proposition vars.
    pub columns: Vec<VarId>,
    /// One per relate where this entity holds the foreign key.
    pub owners: Vec<VarId>,
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub kind: VarKind,
    pub domain: Domain,
    /// Value of an absent or deleted slot, and the creation default.
    pub base: Value,
    pub start: Start,
    pub site: Site,
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
        /// PostgreSQL type a `sets(.., present:<type>)` declared, for display only.
        pg_type: Option<String>,
    },
    /// A comparison proposition that `sets(UC, E, cmp, bool)` drives.
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
    Now,
}

/// Finite by construction: every domain has a size under `Bounds`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Domain {
    /// absent | present | deleted
    Existence,
    /// Declared values in declared casing and order.
    Enum(Vec<String>),
    Bool,
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
    pub fn size(&self, bounds: Bounds) -> usize {
        let ints = usize::from(bounds.int_max) + 1;
        match self {
            Domain::Existence => 3,
            Domain::Enum(values) => values.len(),
            Domain::Bool | Domain::Nullable => 2,
            Domain::Int => ints,
            Domain::NullableInt => ints + 1,
            Domain::Instance { optional, .. } => bounds.slots() + usize::from(*optional),
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
    pub fn of_number(&self, n: i64, bounds: Bounds) -> Option<Value> {
        let max = i64::from(bounds.int_max);
        if !(0..=max).contains(&n) {
            return None;
        }
        match self {
            Domain::Int => Some(Value(n as u8)),
            Domain::NullableInt => Some(Value(n as u8 + 1)),
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
}

/// A value is a domain index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Value(pub u8);

impl Value {
    pub const ABSENT: Value = Value(0);
    pub const PRESENT: Value = Value(1);
    pub const DELETED: Value = Value(2);
    pub const FALSE: Value = Value(0);
    pub const TRUE: Value = Value(1);
    pub const NULL: Value = Value(0);
}

/// Which model fact an item of the system came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelRef {
    /// Index into `SemanticModel::state_transitions`.
    Transition(usize),
    ColumnEffect(usize),
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug)]
pub struct Action {
    /// The use case id, `<id>_<n>` for the n-th transaction group of a split use case, or
    /// `Tick`.
    pub name: String,
    pub origin: ActionOrigin,
    /// At most one param per entity.
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
    },
    /// The `now` tick; exists only when a predicate reads `now`.
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    pub entity: EntId,
    pub role: ParamRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamRole {
    /// The lowest-index absent slot.
    New,
    /// Any present slot.
    Existing,
    /// A present slot that a `New` child is created under; the action does not write it.
    Parent,
}

/// A guard holds on the pre-state, or, when it reads `At::Next`, on the pre-state and the
/// successor together: such a guard filters the successors after the effects.
#[derive(Clone, Debug)]
pub struct Guard {
    pub pred: Pred,
    pub why: GuardWhy,
    pub site: Site,
}

impl Guard {
    pub fn reads_successor(&self) -> bool {
        self.pred.reads_next()
    }
}

impl Pred {
    /// Whether some slot reference points into the successor.
    pub fn reads_next(&self) -> bool {
        let term = |t: &Term| {
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
            Pred::Cmp(a, _, b) => term(a) || term(b),
            Pred::Not(p) => p.reads_next(),
            Pred::And(ps) | Pred::Or(ps) => ps.iter().any(Pred::reads_next),
            Pred::Forall { body, .. } | Pred::Exists { body, .. } => body.reads_next(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardWhy {
    /// `transitions(E.col, UC, from -> to)`: the column is one of the `from` values.
    TransitionFrom,
    /// The child param belongs to the parent param, or is unlinked before a created parent
    /// links it.
    Owner,
    /// `now` has room to tick.
    Clock,
    /// `holds(Cond, ..)` of a condition that `applies_to` the use case.
    Condition { cond: ConditionKey },
}

#[derive(Clone, Debug)]
pub enum Effect {
    /// exists := present; the slot's columns are already at base.
    Create { param: u8 },
    /// exists := deleted; columns and owners reset to base.
    Delete { param: u8 },
    Assign {
        var: VarId,
        param: u8,
        rhs: Rhs,
        site: Site,
    },
    /// var := the `to` of the arm whose `from` the pre-state holds.
    Transition {
        var: VarId,
        param: u8,
        arms: Vec<(Value, Value)>,
        sites: Vec<Site>,
    },
    /// One successor per value.
    Choose {
        var: VarId,
        param: u8,
        values: Vec<Value>,
    },
    /// now := now + 1 while now < int_max.
    Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rhs {
    Lit(Value),
    /// The slot index of that param (owner assignment).
    Param(u8),
}

/// Where a slot reference points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// An action parameter.
    Param(u8),
    /// The n-th binder of the enclosing invariant, assert, or property, outermost first.
    Bound(u8),
    /// An action parameter's slot in the successor. A condition atom on a created param reads
    /// it, so the condition constrains what the action creates.
    Next(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pred {
    True,
    False,
    Cmp(Term, CmpOp, Term),
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
}

/// A state constraint, closed over present slots of its binders.
#[derive(Clone, Debug)]
pub struct Invariant {
    pub name: String,
    pub kind: InvKind,
    /// Universally bound entities, outermost first.
    pub binders: Vec<EntId>,
    pub body: Pred,
    /// The constraint as the author wrote it, for diagnostics.
    pub text: InvText,
    pub site: Site,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvKind {
    Forbidden,
    Invariant,
    Required,
    Exclusive,
    CrossForbidden,
    CrossInvariant,
    Quantifier,
    /// A 1:1 key: no two present children belong to one parent slot.
    Unique,
    /// A child-side range `1..*` / `1..1`: every present parent slot has a present child.
    MinCard,
    /// A column's `@check(expr)`: no present slot makes the expression false.
    Check,
}

/// The parts of a constraint that a violation message names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvText {
    Conditions(String),
    WhenThen {
        guards: String,
        requireds: String,
    },
    Cross {
        entities: String,
        conditions: String,
    },
    CrossWhenThen {
        entities: String,
        guards: String,
        requireds: String,
    },
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
    /// `relate(child, parent, 1..*:1)`, by the key's role.
    MinCard {
        child: String,
        parent: String,
        role: String,
    },
}

/// `after(UC).assert(..)`: a post-state predicate checked after every action of the use case,
/// so after each transaction group of a split one.
#[derive(Clone, Debug)]
pub struct AfterAssert {
    pub name: String,
    /// The use case id.
    pub anchor: String,
    pub steps: Vec<AfterStep>,
    pub requireds: String,
    pub site: Site,
}

/// The assert as one action's post-state reads it.
#[derive(Clone, Debug)]
pub struct AfterStep {
    pub act: ActId,
    /// Scope entities the action does not bind, universally bound in the post-state.
    pub binders: Vec<EntId>,
    /// `At::Param` reads the action's params in the post-state.
    pub post: Pred,
}

/// A temporal property. Data only: the explorer does not evaluate it.
#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    /// Entities the formula is quantified over per instance, outermost first.
    pub binders: Vec<EntId>,
    pub formula: Ltl,
    pub site: Site,
}

#[derive(Clone, Debug)]
pub enum Ltl {
    Always(Pred),
    Eventually(Pred),
    LeadsTo(Pred, Pred),
}
