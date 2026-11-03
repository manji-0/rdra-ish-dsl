//! Diagnostics of the lifecycle layer: one stream from `build` and `explore`.

use std::fmt;

use super::lasso::Loop;
use super::trace::{Inst, Trace};
use super::types::{ActionOrigin, GuardWhy, InvKind, InvText, LtlKind, ParamRole, Site};
use crate::model::EntityKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    /// The tag `states` and `check` print: `info`, `warn`, or `error`.
    pub fn as_label(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warn",
            Severity::Error => "error",
        }
    }
}

/// A diagnostic and the entities whose views report it (empty: the whole system).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    pub entities: Vec<EntityKey>,
    pub diag: StateDiag,
}

/// What a cone that hit its state cap varies in, read off the states it reached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapGrowth {
    /// Each variable of the cone, most distinct values reached first.
    pub vars: Vec<VarSpread>,
    /// Each use case with the number of reached states it reached first, most first.
    pub usecases: Vec<(String, usize)>,
}

/// One variable of a capped cone: how many of its domain's values the reached states hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarSpread {
    /// `Entity.column`, `Entity._exists`, `Child._owner_Parent`, or `now`.
    pub name: String,
    pub reached: usize,
    pub domain: usize,
    /// `Int 0..N` for an Int column, `clock` for `now`; `None` for any other domain.
    pub range: Option<String>,
}

impl CapGrowth {
    /// How many varying variables and use cases the message names.
    const SHOWN: usize = 8;
}

impl fmt::Display for CapGrowth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let varying: Vec<&VarSpread> = self.vars.iter().filter(|v| v.reached > 1).collect();
        write!(
            f,
            "the cone has {} variables, {} of them vary",
            self.vars.len(),
            varying.len()
        )?;
        if !varying.is_empty() {
            let shown: Vec<String> = varying
                .iter()
                .take(Self::SHOWN)
                .map(|v| {
                    let range = v
                        .range
                        .as_ref()
                        .map_or(String::new(), |r| format!(" ({r})"));
                    format!("{} {}/{}{range}", v.name, v.reached, v.domain)
                })
                .collect();
            let more = varying.len().saturating_sub(Self::SHOWN);
            let more = if more > 0 {
                format!(", and {more} more")
            } else {
                String::new()
            };
            write!(f, " (values reached/domain: {}{more})", shown.join(", "))?;
        }
        if !self.usecases.is_empty() {
            let shown: Vec<String> = self
                .usecases
                .iter()
                .take(Self::SHOWN)
                .map(|(uc, n)| format!("{uc} {n}"))
                .collect();
            write!(f, "; new states first reached by: {}", shown.join(", "))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateDiag {
    /// A declared Enum value no reachable state holds.
    UnreachableEnumVariant { column: String, variant: String },
    /// One use case assigns two values to one column (the last one wins).
    ConflictingEffects {
        usecase: String,
        column: String,
        sites: Vec<Site>,
    },
    /// A use case that does not create the entity `sets` its lifecycle column, which
    /// `transitions` already drives. The `sets` is not applied.
    DoubleModeledEnum {
        column: String,
        usecase: String,
        site: Site,
    },
    /// No use case creates the entity: its slots start present at base.
    NoCreationPath,
    /// A pre-existing child's mandatory key names a parent that some use case creates, so at
    /// the start every child slot belongs to a parent slot that is still absent.
    PreExistingChildOfCreatedParent { parent: String, site: Site },
    /// A child-side range needs a child per parent, but the parent starts present and the
    /// child absent, so `MinCard_*` fails in the initial state.
    MinCardOfPreExistingParent { child: String, site: Site },
    /// A use case creates the parent of a child-side range `1..*` and no child in the same
    /// transaction group, so the parent it leaves has no child.
    MinCardNeedsCreate {
        usecase: String,
        child: String,
        site: Site,
    },
    /// The entity's view exceeded the pattern cap; the component's exploration stopped.
    PatternCapReached { cap: usize, bound: usize },
    /// The component's joint state count exceeded the cap; exploration stopped. `growth`
    /// names what the reached states vary in.
    StateCapReached { cap: usize, growth: CapGrowth },
    /// A reachable state violates a state constraint. Each constraint reports its first
    /// violation, with a shortest trace to it.
    ConstraintViolated {
        name: String,
        kind: InvKind,
        text: InvText,
        /// The rule's `.label("...")`, when it has one.
        label: Option<String>,
        pattern_desc: String,
        site: Site,
        trace: Trace,
    },
    /// A step of the anchor use case ends in a state that fails its `after(..).assert`. Each
    /// assert reports its first violation, with a shortest trace ending in that step.
    AfterAssertViolated {
        /// `AfterAssert_<anchor>_<i>`, the property name TLC reports.
        name: String,
        anchor: String,
        requireds: String,
        /// The rule's `.label("...")`, when it has one.
        label: Option<String>,
        actual: String,
        site: Site,
        trace: Trace,
    },
    /// A temporal property fails on the infinite behavior of a lasso: `trace`, then `lasso`.
    /// Only a replayed TLC counterexample builds it; BFS does not check properties.
    PropertyViolated {
        /// The property's name, which is the PROPERTY TLC checks.
        name: String,
        kind: LtlKind,
        /// The instance of each binder the formula fails for, outermost first.
        instance: Vec<Inst>,
        /// The state that shows it, 0 for the start and k for the state after step k: where
        /// `always` is false, or where the instance is present (and the `leads_to` trigger
        /// holds) and the goal never holds from then on.
        witness: usize,
        lasso: Loop,
        site: Site,
        trace: Trace,
    },
    /// A state of a state model (a `transitions` endpoint, or a value a creating use case
    /// sets) that no reachable state holds.
    NoWitness {
        entity: String,
        column: String,
        value: String,
        /// The transition arms and creations that would enter it.
        into: Vec<Entry>,
    },
    /// An action (a use case, or one transaction group of a split one) that labels no edge of
    /// any cone that keeps it: in no reachable state can it run.
    NeverRuns {
        action: String,
        origin: ActionOrigin,
        /// The use case declaration.
        site: Site,
        why: Blocked,
        /// `(child, parent)` entity ids: instances the action creates under an `.optional()`
        /// key to a parent it does not write, so they are created unlinked. Set only when `why`
        /// is `Blocked::Guards`.
        unlinked: Vec<(String, String)>,
    },
    /// A `transitions(..).when(Cond)` arm whose use case runs and whose `from` state is
    /// reached, but which no edge of any cone that keeps it takes: its guard never holds where
    /// the use case could take it.
    ArmNeverTaken {
        action: String,
        column: String,
        from: String,
        to: String,
        /// The ids of the arm's `.when` conditions.
        conditions: Vec<String>,
        /// The `transitions` declaration.
        site: Site,
    },
    /// A `chooses(.., value).when(Cond)` value whose use case runs, but which no edge of any
    /// cone that keeps it picks: its guard never holds where the use case runs.
    ValueNeverChosen {
        action: String,
        /// `Entity.column`.
        column: String,
        value: String,
        /// The ids of the value's `.when` conditions.
        conditions: Vec<String>,
        /// The `chooses` line.
        site: Site,
    },
    /// A present instance at a state of a state model that has a way out, from which no path
    /// moves it: no edge ever changes the column or deletes it. Reported once per state, at
    /// the first such reachable state, with a shortest trace to it.
    Deadlock {
        entity: String,
        column: String,
        value: String,
        /// The stuck instance in the trace's last state.
        at: Inst,
        /// Each way out of the state, and what keeps it from moving the instance.
        exits: Vec<Exit>,
        trace: Trace,
    },
    /// `after(UC).assert` names a use case that changes nothing.
    AssertOnInertUseCase { anchor: String, site: Site },
    /// `property ... .fair(UC)` names a use case that changes nothing, so its fairness
    /// constrains no step.
    FairOnInertUseCase {
        property: String,
        usecase: String,
        site: Site,
    },
    /// An Int literal outside the bounded domain `0..=max`.
    IntLiteralOutOfBounds { value: i64, max: u8, site: Site },
    /// A cross-entity constraint whose entities no relate path binds.
    UnbindableScope {
        constraint: String,
        reason: String,
        site: Site,
    },
    /// A `.has`/`.none`, a `holds` body, or a copy joins two entities that several keys
    /// join, and no `.by(role)` names one. The analysis reports the same for `.along`.
    AmbiguousOwnerLink {
        /// The statement, such as `.none(...)` or `holds(Cond, ...)`.
        statement: String,
        a: String,
        b: String,
        roles: String,
        /// The statement with the `.by(role)` that picks the first candidate.
        rewrite: String,
        site: Site,
    },
    /// `sets(UC, E.column == F.source)` where the step binds no one row of `F` to read.
    UnboundCopySource {
        usecase: String,
        column: String,
        source: String,
        reason: String,
        site: Site,
    },
    /// `applies_to(Cond, UC)` with a `holds` body, where the guard has nothing to read.
    UnbindableCondition {
        condition: String,
        usecase: String,
        reason: String,
        site: Site,
    },
    /// A use case drives a comparison to one truth while its own Int literals give the other.
    ContradictoryDrivenComparison {
        usecase: String,
        proposition: String,
        driven: bool,
        sites: Vec<Site>,
    },
    /// A constraint or a condition guard reads a column that no action writes.
    UnwrittenConstrainedVar { column: String, site: Site },
    /// A comparison the bounded domains cannot evaluate (non-numeric operands).
    UnsupportedComparison {
        comparison: String,
        reason: String,
        site: Site,
    },
    /// A lifecycle construct reads an arithmetic `@derived` column, which has no state var.
    DerivedColumnRead { column: String, site: Site },
    /// A `holds` body applied to a use case compares an entity that no key joins to a row the
    /// use case binds, so the guard holds when any row of it anywhere satisfies the body.
    UnjoinedGuardRead {
        condition: String,
        usecase: String,
        entity: String,
        /// How to join it: the `.along(..)` a key path gives, or writing the entity.
        fix: String,
        site: Site,
    },
    /// A property atom names a column no single entity owns.
    UnresolvedPropertyAtom {
        property: String,
        column: String,
        site: Site,
    },
}

/// `「label」: ` for a labelled rule, so a violation names the rule in the author's words.
pub fn label_prefix(label: &Option<String>) -> String {
    label
        .as_ref()
        .map_or_else(String::new, |label| format!("「{label}」: "))
}

impl StateDiag {
    /// The stable name findings print and expectations and engine comparisons match: the
    /// check's own name for a violated invariant or assert, which TLC reports too;
    /// `Witness_<entity>_<column>_<value>`, `Runs_<action>`, `Deadlock_<entity>_<column>_<value>`,
    /// `DerivedColumnRead_<entity>_<column>`; otherwise `<Variant>_<entities>_<key fields>`.
    /// `entities` are the ids the finding is reported under, so one finding on two entity
    /// views keeps two names. Any character outside `[A-Za-z0-9_]` becomes `_`.
    pub fn name(&self, entities: &[&str]) -> String {
        let with = |variant: &str, keys: &[&str]| {
            let mut parts = vec![variant];
            parts.extend_from_slice(entities);
            parts.extend_from_slice(keys);
            parts.join("_")
        };
        let raw = match self {
            StateDiag::ConstraintViolated { name, .. }
            | StateDiag::AfterAssertViolated { name, .. }
            | StateDiag::PropertyViolated { name, .. } => name.clone(),
            StateDiag::NoWitness {
                entity,
                column,
                value,
                ..
            } => format!("Witness_{entity}_{column}_{value}"),
            StateDiag::NeverRuns { action, .. } => format!("Runs_{action}"),
            StateDiag::ArmNeverTaken {
                action, from, to, ..
            } => format!("Arm_{action}_{from}_{to}"),
            StateDiag::ValueNeverChosen {
                action,
                column,
                value,
                ..
            } => format!("Choice_{action}_{column}_{value}"),
            StateDiag::Deadlock {
                entity,
                column,
                value,
                ..
            } => format!("Deadlock_{entity}_{column}_{value}"),
            // The column is already `<entity>.<column>`.
            StateDiag::DerivedColumnRead { column, .. } => format!("DerivedColumnRead_{column}"),
            StateDiag::UnreachableEnumVariant { column, variant } => {
                with("UnreachableEnumVariant", &[column, variant])
            }
            StateDiag::ConflictingEffects {
                usecase, column, ..
            } => with("ConflictingEffects", &[usecase, column]),
            StateDiag::DoubleModeledEnum {
                column, usecase, ..
            } => with("DoubleModeledEnum", &[usecase, column]),
            StateDiag::NoCreationPath => with("NoCreationPath", &[]),
            StateDiag::PreExistingChildOfCreatedParent { parent, .. } => {
                with("PreExistingChildOfCreatedParent", &[parent])
            }
            StateDiag::MinCardOfPreExistingParent { child, .. } => {
                with("MinCardOfPreExistingParent", &[child])
            }
            StateDiag::MinCardNeedsCreate { usecase, child, .. } => {
                with("MinCardNeedsCreate", &[usecase, child])
            }
            StateDiag::PatternCapReached { .. } => with("PatternCapReached", &[]),
            StateDiag::StateCapReached { .. } => with("StateCapReached", &[]),
            StateDiag::AssertOnInertUseCase { anchor, .. } => {
                with("AssertOnInertUseCase", &[anchor])
            }
            StateDiag::FairOnInertUseCase {
                property, usecase, ..
            } => with("FairOnInertUseCase", &[property, usecase]),
            StateDiag::IntLiteralOutOfBounds { value, .. } => {
                with("IntLiteralOutOfBounds", &[&value.to_string()])
            }
            StateDiag::UnbindableScope { constraint, .. } => with("UnbindableScope", &[constraint]),
            StateDiag::UnbindableCondition {
                condition, usecase, ..
            } => with("UnbindableCondition", &[condition, usecase]),
            StateDiag::UnboundCopySource {
                usecase, column, ..
            } => with("UnboundCopySource", &[usecase, column]),
            StateDiag::AmbiguousOwnerLink { a, b, .. } => with("AmbiguousOwnerLink", &[a, b]),
            StateDiag::ContradictoryDrivenComparison {
                usecase,
                proposition,
                ..
            } => with("ContradictoryDrivenComparison", &[usecase, proposition]),
            StateDiag::UnwrittenConstrainedVar { column, .. } => {
                with("UnwrittenConstrainedVar", &[column])
            }
            StateDiag::UnsupportedComparison { comparison, .. } => {
                with("UnsupportedComparison", &[comparison])
            }
            StateDiag::UnresolvedPropertyAtom {
                property, column, ..
            } => with("UnresolvedPropertyAtom", &[property, column]),
            StateDiag::UnjoinedGuardRead {
                condition, usecase, ..
            } => with("UnjoinedGuardRead", &[condition, usecase]),
        };
        raw.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// The counterexample of a violated constraint, assert, or property (for a property, the
    /// finite part of its lasso), or the path into a deadlock.
    pub fn trace(&self) -> Option<&Trace> {
        match self {
            StateDiag::ConstraintViolated { trace, .. }
            | StateDiag::AfterAssertViolated { trace, .. }
            | StateDiag::PropertyViolated { trace, .. }
            | StateDiag::Deadlock { trace, .. } => Some(trace),
            _ => None,
        }
    }

    pub fn trace_mut(&mut self) -> Option<&mut Trace> {
        match self {
            StateDiag::ConstraintViolated { trace, .. }
            | StateDiag::AfterAssertViolated { trace, .. }
            | StateDiag::PropertyViolated { trace, .. }
            | StateDiag::Deadlock { trace, .. } => Some(trace),
            _ => None,
        }
    }

    /// A deadlock is an error, unless some way out is blocked only by the instance bound: it
    /// needs a free slot no stuck state has. The author cannot fix that in the model.
    pub fn severity(&self) -> Severity {
        match self {
            StateDiag::Deadlock { exits, .. } if exits.iter().any(|x| x.why.only_free_slots()) => {
                Severity::Warning
            }
            StateDiag::NoCreationPath | StateDiag::MinCardNeedsCreate { .. } => Severity::Info,
            StateDiag::UnreachableEnumVariant { .. }
            | StateDiag::UnwrittenConstrainedVar { .. }
            | StateDiag::PreExistingChildOfCreatedParent { .. }
            | StateDiag::MinCardOfPreExistingParent { .. }
            | StateDiag::NeverRuns { .. }
            | StateDiag::ArmNeverTaken { .. }
            | StateDiag::ValueNeverChosen { .. }
            | StateDiag::UnjoinedGuardRead { .. } => Severity::Warning,
            StateDiag::ConflictingEffects { .. }
            | StateDiag::DoubleModeledEnum { .. }
            | StateDiag::PatternCapReached { .. }
            | StateDiag::ContradictoryDrivenComparison { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::ConstraintViolated { .. }
            | StateDiag::AfterAssertViolated { .. }
            | StateDiag::PropertyViolated { .. }
            | StateDiag::NoWitness { .. }
            | StateDiag::Deadlock { .. }
            | StateDiag::AssertOnInertUseCase { .. }
            | StateDiag::FairOnInertUseCase { .. }
            | StateDiag::IntLiteralOutOfBounds { .. }
            | StateDiag::UnbindableScope { .. }
            | StateDiag::UnbindableCondition { .. }
            | StateDiag::UnboundCopySource { .. }
            | StateDiag::AmbiguousOwnerLink { .. }
            | StateDiag::UnsupportedComparison { .. }
            | StateDiag::DerivedColumnRead { .. }
            | StateDiag::UnresolvedPropertyAtom { .. } => Severity::Error,
        }
    }
}

/// A way into a state: a `transitions` arm from `from`, or a creation (`from` is `None`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub action: String,
    pub from: Option<String>,
    /// The arm, the `sets`, or the creating use case.
    pub site: Site,
}

/// A way out of a state of a state model: a `transitions` arm from it, or a deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub action: String,
    pub origin: ActionOrigin,
    /// The state the arm enters; `None` for a deletion.
    pub to: Option<String>,
    /// The arm, or the use case declaration for a deletion.
    pub site: Site,
    /// What keeps it from moving the stuck instance, read off the states it stays stuck in.
    pub why: Blocked,
}

/// An instance an action needs: the entity id, the param's role, and for a creation through
/// an invoked API's `creates`, that API's id, which is why the use case creates it at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    pub entity: String,
    pub role: ParamRole,
    pub via_api: Option<String>,
}

/// `; <who> creates <Entity> via api '<Api>'` for each need created through an API, joined
/// with `and`; empty when none is. It says why a use case with no `creates` of its own needs
/// a free slot.
pub fn created_via_apis(who: &str, needs: &[Need]) -> String {
    let vias: Vec<String> = needs
        .iter()
        .filter(|n| n.role == ParamRole::New)
        .filter_map(|n| Some(format!("{} via api '{}'", n.entity, n.via_api.as_ref()?)))
        .collect();
    match vias.is_empty() {
        true => String::new(),
        false => format!("; {who} creates {}", vias.join(" and ")),
    }
}

/// Why an action never runs, read off the reachable states of a cone that keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// No reachable state has an instance for these params: a free slot for a `New` one, a
    /// present instance otherwise.
    NoInstance(Vec<Need>),
    /// Each param finds an instance in some reachable state, but no state has all of these at
    /// once.
    Apart(Vec<Need>),
    /// These guards hold in no reachable state, under any binding of the params.
    Guards(Vec<(GuardWhy, Site)>),
    /// Each guard holds in some reachable state, but never all of them in one.
    Together,
    /// In some reachable state its guards hold and every other instance is there, but these
    /// instances it creates have no free slot within the instance bound.
    Crowded(Vec<Need>),
}

impl Blocked {
    /// Only free slots are missing: every param it lacks is a `New` one.
    pub fn only_free_slots(&self) -> bool {
        match self {
            Blocked::NoInstance(params) => params.iter().all(|n| n.role == ParamRole::New),
            Blocked::Crowded(_) => true,
            Blocked::Apart(_) | Blocked::Guards(_) | Blocked::Together => false,
        }
    }
}

pub fn has_errors<'a>(diags: impl IntoIterator<Item = &'a Diag>) -> bool {
    diags
        .into_iter()
        .any(|d| d.diag.severity() == Severity::Error)
}

impl fmt::Display for StateDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateDiag::UnreachableEnumVariant { column, variant } => {
                write!(f, "{column}={variant} is unreachable")
            }
            StateDiag::ConflictingEffects {
                usecase, column, ..
            } => write!(f, "{usecase} assigns conflicting values to {column}"),
            StateDiag::DoubleModeledEnum {
                column, usecase, ..
            } => write!(
                f,
                "{usecase} sets {column}, which transitions already drive; model it as a transition"
            ),
            StateDiag::NoCreationPath => {
                write!(f, "no use case creates this entity; it starts present")
            }
            StateDiag::PreExistingChildOfCreatedParent { parent, .. } => write!(
                f,
                "this entity starts present, but its parent {parent} starts absent; it starts owned by a {parent} that does not exist yet"
            ),
            StateDiag::MinCardOfPreExistingParent { child, .. } => write!(
                f,
                "this entity starts present and needs a {child}, but every {child} starts absent; its minimum-children invariant fails at the start"
            ),
            StateDiag::MinCardNeedsCreate { usecase, child, .. } => write!(
                f,
                "{usecase} creates this entity without a {child} in the same transaction; its minimum-children invariant fails after {usecase} unless the use case also creates a {child}"
            ),
            StateDiag::PatternCapReached { cap, bound } => {
                write!(
                    f,
                    "pattern cap {cap} reached (bound {bound}); results are incomplete"
                )
            }
            StateDiag::StateCapReached { cap, growth } => {
                write!(f, "state cap {cap} reached; results are incomplete: {growth}")
            }
            StateDiag::ConstraintViolated {
                name,
                label,
                pattern_desc,
                ..
            } => write!(
                f,
                "{}{name} is violated by a reachable state: {pattern_desc}",
                label_prefix(label)
            ),
            StateDiag::AfterAssertViolated {
                anchor,
                requireds,
                label,
                actual,
                ..
            } => write!(
                f,
                "{}after({anchor}).assert({requireds}) fails: {actual}",
                label_prefix(label)
            ),
            StateDiag::PropertyViolated {
                name,
                kind,
                instance,
                witness,
                ..
            } => {
                let inst = instance
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let at = match witness {
                    0 => "at the start".to_string(),
                    k => format!("after step {k}"),
                };
                write!(f, "property {name} is violated: ")?;
                match kind {
                    LtlKind::Always => write!(f, "its always condition is false for {inst} {at}"),
                    LtlKind::Eventually => write!(
                        f,
                        "{inst} exists {at}, and its eventually condition never holds from then on"
                    ),
                    LtlKind::LeadsTo => write!(
                        f,
                        "its leads_to trigger holds for {inst} {at}, and its goal never holds from then on"
                    ),
                }
            }
            StateDiag::NoWitness {
                entity,
                column,
                value,
                ..
            } => write!(
                f,
                "state {entity}.{column} = {value} has no reachability witness: no sequence of use cases from the start reaches it"
            ),
            StateDiag::NeverRuns { action, why, .. } => {
                write!(f, "{action} can never run: ")?;
                match why {
                    Blocked::NoInstance(needs) => write!(
                        f,
                        "no reachable state has an instance it needs{}",
                        created_via_apis("it", needs)
                    ),
                    Blocked::Apart(_) => write!(
                        f,
                        "no reachable state has all the instances it needs at once"
                    ),
                    Blocked::Guards(_) => write!(f, "in no reachable state do its guards hold"),
                    Blocked::Together => write!(
                        f,
                        "each of its guards holds in some reachable state, but never all of them in one"
                    ),
                    Blocked::Crowded(needs) => write!(
                        f,
                        "whenever its guards hold, the instance bound leaves no free slot for an instance it creates{}",
                        created_via_apis("it", needs)
                    ),
                }
            }
            StateDiag::ArmNeverTaken {
                action,
                column,
                from,
                to,
                conditions,
                ..
            } => write!(
                f,
                "{action} never takes {column} {from} -> {to}: wherever it could, its guard {} does not hold",
                conditions.join(" or ")
            ),
            StateDiag::ValueNeverChosen {
                action,
                column,
                value,
                conditions,
                ..
            } => write!(
                f,
                "{action} never chooses {column} = {value}: wherever it could, its guard {} does not hold",
                conditions.join(" or ")
            ),
            StateDiag::Deadlock {
                column,
                value,
                at,
                exits,
                ..
            } => {
                let bound = exits.iter().find_map(|x| match &x.why {
                    Blocked::NoInstance(needs) if x.why.only_free_slots() => {
                        Some((&x.action, needs))
                    }
                    Blocked::Crowded(needs) => Some((&x.action, needs)),
                    _ => None,
                });
                match bound {
                    Some((action, needs)) => {
                        let slots: Vec<&str> = needs.iter().map(|n| n.entity.as_str()).collect();
                        write!(
                            f,
                            "deadlock at the bound: {at} stays at {column} = {value} forever; {action} needs a free {} slot within the instance bound{}",
                            slots.join(" and "),
                            created_via_apis(action, needs)
                        )
                    }
                    None => write!(
                        f,
                        "deadlock: {at} stays at {column} = {value} forever, and {value} is not an end state"
                    ),
                }
            }
            StateDiag::AssertOnInertUseCase { anchor, .. } => {
                write!(f, "after({anchor}).assert names a use case with no effect")
            }
            StateDiag::FairOnInertUseCase {
                property, usecase, ..
            } => write!(
                f,
                "property {property}: .fair({usecase}) names a use case with no effect, so its fairness constrains no step"
            ),
            StateDiag::IntLiteralOutOfBounds { value, max, .. } => {
                write!(
                    f,
                    "Int literal {value} is outside the bounded domain 0..{max}"
                )
            }
            StateDiag::UnbindableScope {
                constraint, reason, ..
            } => write!(f, "{constraint}: {reason}"),
            StateDiag::UnbindableCondition {
                condition,
                usecase,
                reason,
                ..
            } => write!(f, "condition {condition} on {usecase}: {reason}"),
            StateDiag::AmbiguousOwnerLink {
                statement,
                a,
                b,
                roles,
                rewrite,
                ..
            } => write!(
                f,
                "{statement} cannot tell which key joins {a} and {b}: roles {roles}; name one with `{rewrite}`"
            ),
            StateDiag::UnboundCopySource {
                usecase,
                column,
                source,
                reason,
                ..
            } => write!(
                f,
                "{usecase} copies {source} into {column}, but binds no one row of it: {reason}"
            ),
            StateDiag::ContradictoryDrivenComparison {
                usecase,
                proposition,
                driven,
                ..
            } => write!(
                f,
                "{usecase} drives {proposition} to {driven}, but its own Int values make it {}",
                !driven
            ),
            StateDiag::UnwrittenConstrainedVar { column, .. } => {
                write!(f, "{column} is constrained but no use case writes it")
            }
            StateDiag::UnsupportedComparison {
                comparison, reason, ..
            } => write!(f, "{comparison}: {reason}"),
            StateDiag::DerivedColumnRead { column, .. } => write!(
                f,
                "{column} is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead"
            ),
            StateDiag::UnresolvedPropertyAtom {
                property, column, ..
            } => write!(
                f,
                "property {property}: no single entity owns column {column}"
            ),
            StateDiag::UnjoinedGuardRead {
                condition,
                usecase,
                entity,
                fix,
                ..
            } => write!(
                f,
                "holds({condition}, ...) on {usecase} reads {entity}, which no key joins to a row {usecase} binds, so any {entity} row satisfies it; {fix}"
            ),
        }
    }
}
