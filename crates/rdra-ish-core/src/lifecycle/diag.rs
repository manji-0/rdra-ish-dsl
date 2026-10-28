//! Diagnostics of the lifecycle layer: one stream from `build` and `explore`.

use std::fmt;

use super::types::{InvKind, InvText, Site};
use crate::model::EntityKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A diagnostic and the entities whose views report it (empty: the whole system).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    pub entities: Vec<EntityKey>,
    pub diag: StateDiag,
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
    /// The entity's view exceeded the pattern cap; the component's exploration stopped.
    PatternCapReached { cap: usize, bound: usize },
    /// The component's joint state count exceeded the cap; exploration stopped.
    StateCapReached { cap: usize },
    /// A reachable state violates a state constraint.
    ConstraintViolated {
        name: String,
        kind: InvKind,
        text: InvText,
        pattern_desc: String,
        site: Site,
    },
    /// A step of the anchor use case ends in a state that fails its `after(..).assert`.
    AfterAssertViolated {
        anchor: String,
        requireds: String,
        actual: String,
        site: Site,
    },
    /// `after(UC).assert` names a use case that changes nothing.
    AssertOnInertUseCase { anchor: String, site: Site },
    /// An Int literal outside the bounded domain `0..=max`.
    IntLiteralOutOfBounds { value: i64, max: u8, site: Site },
    /// A cross-entity constraint whose entities no relate path binds.
    UnbindableScope {
        constraint: String,
        reason: String,
        site: Site,
    },
    /// A constraint reads a column that no action writes.
    UnwrittenConstrainedVar { column: String, site: Site },
    /// A comparison the bounded domains cannot evaluate (non-numeric operands).
    UnsupportedComparison {
        comparison: String,
        reason: String,
        site: Site,
    },
    /// A property atom names a column no single entity owns.
    UnresolvedPropertyAtom {
        property: String,
        column: String,
        site: Site,
    },
}

impl StateDiag {
    pub fn severity(&self) -> Severity {
        match self {
            StateDiag::NoCreationPath => Severity::Info,
            StateDiag::UnreachableEnumVariant { .. }
            | StateDiag::UnwrittenConstrainedVar { .. } => Severity::Warning,
            StateDiag::ConflictingEffects { .. }
            | StateDiag::DoubleModeledEnum { .. }
            | StateDiag::PatternCapReached { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::ConstraintViolated { .. }
            | StateDiag::AfterAssertViolated { .. }
            | StateDiag::AssertOnInertUseCase { .. }
            | StateDiag::IntLiteralOutOfBounds { .. }
            | StateDiag::UnbindableScope { .. }
            | StateDiag::UnsupportedComparison { .. }
            | StateDiag::UnresolvedPropertyAtom { .. } => Severity::Error,
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
            StateDiag::PatternCapReached { cap, bound } => {
                write!(
                    f,
                    "pattern cap {cap} reached (bound {bound}); results are incomplete"
                )
            }
            StateDiag::StateCapReached { cap } => {
                write!(f, "state cap {cap} reached; results are incomplete")
            }
            StateDiag::ConstraintViolated {
                name, pattern_desc, ..
            } => write!(f, "{name} is violated by a reachable state: {pattern_desc}"),
            StateDiag::AfterAssertViolated {
                anchor,
                requireds,
                actual,
                ..
            } => write!(f, "after({anchor}).assert({requireds}) fails: {actual}"),
            StateDiag::AssertOnInertUseCase { anchor, .. } => {
                write!(f, "after({anchor}).assert names a use case with no effect")
            }
            StateDiag::IntLiteralOutOfBounds { value, max, .. } => {
                write!(
                    f,
                    "Int literal {value} is outside the bounded domain 0..{max}"
                )
            }
            StateDiag::UnbindableScope {
                constraint, reason, ..
            } => write!(f, "{constraint}: {reason}"),
            StateDiag::UnwrittenConstrainedVar { column, .. } => {
                write!(f, "{column} is constrained but no use case writes it")
            }
            StateDiag::UnsupportedComparison {
                comparison, reason, ..
            } => write!(f, "{comparison}: {reason}"),
            StateDiag::UnresolvedPropertyAtom {
                property, column, ..
            } => write!(
                f,
                "property {property}: no single entity owns column {column}"
            ),
        }
    }
}
