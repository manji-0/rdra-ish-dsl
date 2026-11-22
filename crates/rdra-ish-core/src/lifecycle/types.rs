//! The transition system's data. Everything here is plain data that `build` fills in and that
//! the explorer and a printer read; no item here reads the `SemanticModel`.

use std::num::NonZeroU8;

use crate::location::LocatedSpan;
use crate::model::{ApiKey, BucKey, ConditionKey, EntityKey, EventKey, FkId, UseCaseKey};

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
    Raised {
        entity: EntId,
        /// The event id.
        event: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    pub entity: EntId,
    pub role: ParamRole,
    /// The invoked API whose `creates` makes the use case create this instance; `None` for a
    /// direct `creates` or any other role.
    pub via_api: Option<ApiKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    /// The var a `GuardWhy::Copy` guard pins in the successor. The guard constrains nothing
    /// else, so a system without that var drops it.
    pub fn copy_target(&self) -> Option<VarId> {
        match (&self.why, &self.pred) {
            (
                GuardWhy::Copy,
                Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Next(_),
                    },
                    ..,
                ),
            ) => Some(*var),
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
                Pred::Not(body) | Pred::Forall { body, .. } | Pred::Exists { body, .. } => {
                    stack.push(body)
                }
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

    /// Whether some slot reference points into the successor.
    /// The explorer asks this of every guard in every state, so it walks the tree in place:
    /// `terms` keeps a stack on the heap.
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
            Pred::Not(body) | Pred::Forall { body, .. } | Pred::Exists { body, .. } => {
                body.reads_next()
            }
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

/// One `transitions` arm of an `Effect::Transition`.
#[derive(Clone, Debug)]
pub struct Arm {
    pub from: Value,
    pub to: Value,
    /// The `transitions` line that declares it.
    pub site: Site,
    /// Its `.when` conditions, key and id: any of them enables it. Empty when unguarded.
    pub conds: Vec<(ConditionKey, String)>,
}

/// One value of an `Effect::Choose`.
#[derive(Clone, Debug)]
pub struct Choice {
    pub value: Value,
    /// The `sets(.., col == present)`, copy or `chooses` line that asks for the value. A
    /// creation's own choice of a variation value has none.
    pub site: Option<Site>,
    /// The `.when` conditions of a `chooses(.., value)` or guarded `sets` line, as
    /// `Arm::conds`. Empty when unguarded.
    pub conds: Vec<(ConditionKey, String)>,
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
}

/// A state constraint, closed over present slots of its binders.
#[derive(Clone, Debug)]
pub struct Invariant {
    pub name: String,
    /// Universally bound entities, outermost first.
    pub binders: Vec<EntId>,
    pub body: Pred,
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
    /// A 1:1 key: no two present children belong to one parent slot.
    Unique,
    /// A child-side range `1..*` / `1..1`: every present parent slot has a present child.
    MinCard,
    /// A column's `@check(expr)`: no present slot makes the expression false.
    Check,
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
            InvText::Unique { .. } => InvKind::Unique,
            InvText::MinCard { .. } => InvKind::MinCard,
            InvText::Check(_) => InvKind::Check,
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
    /// `relate(child, parent, 1..*:1)`, by the key's role.
    MinCard {
        child: String,
        parent: String,
        role: String,
    },
}

/// `after(UC).assert(..)`: a post-state predicate checked after the last action of the use
/// case, so once per run of a split one, on the run's rows.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct AfterStep {
    pub act: ActId,
    /// Scope entities the action does not bind, universally bound in the post-state.
    pub binders: Vec<EntId>,
    /// `At::Param` reads the action's params in the post-state.
    pub post: Pred,
}

/// A temporal property. The explorer does not check it; `explain_property` evaluates it on a
/// lasso another engine found.
#[derive(Clone, Debug)]
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
    /// Every predicate the property reads: the link, then the formula's operands.
    pub fn preds(&self) -> impl Iterator<Item = &Pred> {
        std::iter::once(&self.link).chain(self.formula.preds())
    }

    /// Whether `.fair` names the use case of `action`.
    pub fn is_fair(&self, action: &Action) -> bool {
        matches!(&action.origin, ActionOrigin::UseCase { uc, .. } if self.fair.contains(uc))
    }
}

#[derive(Clone, Debug)]
pub enum Ltl {
    Always(Pred),
    Eventually(Pred),
    LeadsTo(Pred, Pred),
}

impl Ltl {
    /// The operands, left to right.
    pub fn preds(&self) -> impl Iterator<Item = &Pred> {
        match self {
            Ltl::Always(x) | Ltl::Eventually(x) => std::iter::once(x).chain(None),
            Ltl::LeadsTo(x, y) => std::iter::once(x).chain(Some(y)),
        }
    }

    /// The same operator over `f` of each operand.
    pub fn map(&self, mut f: impl FnMut(&Pred) -> Pred) -> Ltl {
        match self {
            Ltl::Always(x) => Ltl::Always(f(x)),
            Ltl::Eventually(x) => Ltl::Eventually(f(x)),
            Ltl::LeadsTo(x, y) => Ltl::LeadsTo(f(x), f(y)),
        }
    }

    pub fn kind(&self) -> LtlKind {
        match self {
            Ltl::Always(_) => LtlKind::Always,
            Ltl::Eventually(_) => LtlKind::Eventually,
            Ltl::LeadsTo(..) => LtlKind::LeadsTo,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LtlKind {
    Always,
    Eventually,
    LeadsTo,
}
