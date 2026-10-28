//! The transition system's data. Everything here is plain data that `build` fills in and that
//! the explorer and a printer read; no item here reads the `SemanticModel`.

use std::num::NonZeroU8;

use crate::location::LocatedSpan;
use crate::model::{BucKey, EntityKey, UseCaseKey};

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
    /// Any domain value: a variation column of a pre-existing entity (vision rule 5).
    Any,
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
    /// Index of the parent slot this child slot belongs to.
    Owner {
        child: EntId,
        parent: EntId,
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
    /// A slot index of the parent entity.
    Instance(EntId),
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
            Domain::Instance(_) => bounds.slots(),
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
    /// The use case id, or `Tick`.
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
    /// One atomic action per use case. Splitting by transaction group (vision rule 6) is the
    /// next unit; it will add the group index here.
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

#[derive(Clone, Debug)]
pub struct Guard {
    pub pred: Pred,
    pub why: GuardWhy,
    pub site: Site,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardWhy {
    /// `transitions(E.col, UC, from -> to)`: the column is one of the `from` values.
    TransitionFrom,
    /// The child param belongs to the parent param.
    Owner,
    /// `now` has room to tick.
    Clock,
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
}

/// `after(UC).assert(..)`: a post-state predicate on every action of the use case.
#[derive(Clone, Debug)]
pub struct AfterAssert {
    pub name: String,
    pub anchor: UseCaseKey,
    pub on: Vec<ActId>,
    /// Scope entities the action does not bind, universally bound in the post-state.
    pub binders: Vec<EntId>,
    /// `At::Param` reads the action's params in the post-state.
    pub post: Pred,
    pub requireds: String,
    pub site: Site,
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
