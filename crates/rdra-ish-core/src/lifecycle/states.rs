//! What `states`, `check`, and the generator gates read: the system at `Bounds::STATES`, each
//! entity view and each constraint verdict computed on its own target cone.

use std::fmt;

use super::diag::{Diag, Severity, StateDiag};
use super::explore::Caps;
use super::targets::Undecided;
use super::view::EntityStateResult;
use super::{Bounds, Scope, TransitionSystem};
use crate::model::SemanticModel;

impl Caps {
    /// `--max-patterns` per entity view, and 16 joint states per allowed pattern for each
    /// target cone. The largest cone of any sample reaches 184 states, under the 4096 default.
    pub const fn per_cone(patterns: usize) -> Self {
        Caps::new(patterns, patterns.saturating_mul(16))
    }

    /// For BFS at `Bounds::VERIFY`: 2^21 states per cone. The largest sample cone, clinic-ops
    /// `AccountBalance`, reaches 1,707,212.
    pub const VERIFY: Caps = Caps::new(Caps::DEFAULT_PATTERNS, 1 << 21);
}

/// The one bounds line every command prints: `check` and `states` on stderr, `verify` on
/// stdout. A result holds within these bounds and says nothing outside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundsLine {
    pub bounds: Bounds,
    /// BFS caps; `None` when the command runs no BFS.
    pub caps: Option<Caps>,
    /// Also name the TLA+ constants the bounds map to.
    pub tla: bool,
}

impl BoundsLine {
    /// What `states`, `check`, and the generators run at.
    pub fn states(max_patterns: usize) -> Self {
        BoundsLine {
            bounds: Bounds::STATES,
            caps: Some(Caps::per_cone(max_patterns)),
            tla: false,
        }
    }

    /// What `verify` runs at: TLC has no state cap; `caps` bound the BFS of the entity cones.
    pub fn verify(bounds: Bounds, caps: Option<Caps>) -> Self {
        BoundsLine {
            bounds,
            caps,
            tla: true,
        }
    }

    /// The bounds alone, as the final `OK: no errors within ...` line states them.
    pub fn within(&self) -> impl fmt::Display + '_ {
        Within(self.bounds)
    }
}

struct Within(Bounds);

impl fmt::Display for Within {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0.instances.get();
        let s = if n == 1 { "" } else { "s" };
        write!(f, "{n} instance{s} per entity, Int 0..{}", self.0.int_max)
    }
}

impl fmt::Display for BoundsLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bounds: {}", Within(self.bounds))?;
        if self.tla {
            write!(
                f,
                " (TLA+ InstanceCount={}, IntRange=0..{})",
                self.bounds.instances, self.bounds.int_max
            )?;
        }
        if let Some(caps) = self.caps {
            let bfs = if self.tla { "BFS " } else { "" };
            write!(f, "; {} states per {bfs}cone", caps.states)?;
        }
        write!(f, "; entities a check does not read are unbounded")
    }
}

pub struct StatesReport {
    pub bounds: Bounds,
    pub caps: Caps,
    /// Sorted by entity id; each carries the diagnostics that name its entity.
    pub views: Vec<EntityStateResult>,
    /// Every diagnostic of the build and the cone runs' verdicts.
    pub diags: Vec<Diag>,
    /// States of state models the run did not decide.
    pub undecided: Vec<Undecided>,
}

impl StatesReport {
    /// The bounds the views were derived under.
    pub fn bounds_line(&self) -> BoundsLine {
        BoundsLine {
            bounds: self.bounds,
            caps: Some(self.caps),
            tla: false,
        }
    }

    /// Any Error diagnostic, on a view or naming no entity.
    pub fn has_errors(&self) -> bool {
        super::has_errors(&self.diags)
            || self
                .views
                .iter()
                .flat_map(|v| &v.diagnostics)
                .any(|d| d.severity() == Severity::Error)
    }

    /// Diagnostics that name no entity, so no view reports them.
    pub fn unattributed(&self) -> impl Iterator<Item = &StateDiag> {
        self.diags
            .iter()
            .filter(|d| d.entities.is_empty())
            .map(|d| &d.diag)
    }
}

/// Builds the system for `scope` at `Bounds::STATES` and explores each target's cone.
pub fn derive_states(model: &SemanticModel, scope: &Scope, max_patterns: usize) -> StatesReport {
    let bounds = Bounds::STATES;
    let caps = Caps::per_cone(max_patterns);
    let built = TransitionSystem::build(model, bounds, scope);
    let mut run = built.ts.explore_targets(&built.ts.targets(), caps);
    if *scope != Scope::All {
        run.defer_progress();
    }
    let views = run.entity_views(&built.ts, model, &built.diags);
    let mut diags = built.diags;
    for d in run.verdicts() {
        if !diags.contains(d) {
            diags.push(d.clone());
        }
    }
    StatesReport {
        bounds,
        caps,
        views,
        diags,
        undecided: run.undecided().to_vec(),
    }
}

impl Scope {
    /// `--buc` as a scope: no ids is the whole model.
    pub fn from_bucs(bucs: &[String]) -> Self {
        if bucs.is_empty() {
            Scope::All
        } else {
            Scope::Bucs(bucs.to_vec())
        }
    }
}
