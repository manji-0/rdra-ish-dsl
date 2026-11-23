//! What `states`, `check`, and the generator gates read: the system at `Bounds::STATES`, each
//! entity view and each constraint verdict computed on its own target cone.

use std::fmt;

use super::diag::{Diag, Severity, StateDiag};
use super::explore::Caps;
use super::select::{NotChecked, Scope, Selection};
use super::targets::Undecided;
use super::view::EntityStateResult;
use super::{Bounds, TransitionSystem};
use crate::model::{EntityKey, SemanticModel};

impl Caps {
    /// What the generators run at, with one instance per entity: `for_instances(1)`, 2^10
    /// patterns and 2^16 states. `check` and `states` scale theirs with `--instance-count`.
    /// `examples/lifecycle_measure.rs` prints the largest cone of each sample.
    pub const STATES: Caps = Caps::for_instances(1);

    /// For BFS at `Bounds::VERIFY`: 2^23 states per cone. A cone past it leaves its state-model
    /// findings undecided; `engine_equivalence`'s `VERIFY_FALLBACK` names the sample cones
    /// over 300,000 states there.
    pub const VERIFY: Caps = Caps::new(256, 1 << 23);
}

/// The one bounds line every command prints: `check` and `states` on stderr, `verify` on
/// stdout. A result holds within these bounds and says nothing outside them; under a scope
/// it holds only within the scope, which a second line states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundsLine {
    pub bounds: Bounds,
    /// BFS caps; `None` when the command runs no BFS.
    pub caps: Option<Caps>,
    /// Also name the TLA+ constants the bounds map to.
    pub tla: bool,
    /// The `scope:` line (`Scope::line`); `None` for a whole run.
    pub scope: Option<String>,
}

impl BoundsLine {
    /// What `states`, `check`, and the generators run at.
    pub fn states(caps: Caps) -> Self {
        BoundsLine {
            bounds: Bounds::STATES,
            caps: Some(caps),
            tla: false,
            scope: None,
        }
    }

    /// What `verify` runs at: TLC has no state cap; `caps` bound the BFS of the entity cones.
    pub fn verify(bounds: Bounds, caps: Option<Caps>) -> Self {
        BoundsLine {
            bounds,
            caps,
            tla: true,
            scope: None,
        }
    }

    pub fn with_scope(self, scope: Option<String>) -> Self {
        BoundsLine { scope, ..self }
    }

    /// The final line of a run with no error: `OK: no errors within ...`, with `in the
    /// selected scope` when a scope is set.
    pub fn ok(&self) -> String {
        let scoped = if self.scope.is_some() {
            " in the selected scope,"
        } else {
            ""
        };
        format!("OK: no errors{scoped} within {}", Within(self.bounds))
    }

    /// The final line of a run with an error: `FAIL: 3 errors, 5 warnings within ...`, with
    /// `in the selected scope` when a scope is set.
    pub fn fail(&self, errors: usize, warnings: usize) -> String {
        let scoped = if self.scope.is_some() {
            " in the selected scope,"
        } else {
            ""
        };
        format!(
            "FAIL: {}{scoped} within {}",
            FindingTally { errors, warnings },
            Within(self.bounds)
        )
    }
}

/// `3 errors, 1 warning`: what a failed run ends with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindingTally {
    pub errors: usize,
    pub warnings: usize,
}

impl FindingTally {
    /// Counts an error or a warning; an info finding is neither.
    pub fn count(&mut self, severity: Severity) {
        match severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            Severity::Info => {}
        }
    }
}

impl fmt::Display for FindingTally {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        write!(
            f,
            "{} error{}, {} warning{}",
            self.errors,
            plural(self.errors),
            self.warnings,
            plural(self.warnings)
        )
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
        write!(f, "; entities a check does not read are unbounded")?;
        if let Some(scope) = &self.scope {
            write!(f, "\n{scope}")?;
        }
        Ok(())
    }
}

pub struct StatesReport {
    pub bounds: Bounds,
    pub caps: Caps,
    pub scope: Scope,
    /// The targets the scope answered; `views` holds the selected entities only.
    pub selection: Selection,
    /// Sorted by entity id; each carries the diagnostics that name its entity.
    pub views: Vec<EntityStateResult>,
    /// Every diagnostic of the build and the cone runs' verdicts.
    pub diags: Vec<Diag>,
    /// States of state models the run did not decide.
    pub undecided: Vec<Undecided>,
}

impl StatesReport {
    /// The bounds and the scope the views were derived under.
    pub fn bounds_line(&self, model: &SemanticModel) -> BoundsLine {
        BoundsLine {
            bounds: self.bounds,
            caps: Some(self.caps),
            tla: false,
            scope: self.scope.line(model, &self.selection),
        }
    }

    /// What the scope left out and what the run did not decide.
    pub fn not_checked<'a>(
        &'a self,
        ts: &'a TransitionSystem,
        model: &'a SemanticModel,
    ) -> NotChecked<'a> {
        NotChecked {
            ts,
            model,
            scope: &self.scope,
            selection: &self.selection,
            undecided: &self.undecided,
            temporal: 0,
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

    /// Under `--entity`, the errors and never-run use cases that name only entities whose
    /// views the scope leaves out, with those entities' ids. A selected cone reads them, so
    /// they count; no view prints them.
    pub fn outside_views<'a>(
        &'a self,
        model: &'a SemanticModel,
    ) -> impl Iterator<Item = (Vec<&'a str>, &'a StateDiag)> + 'a {
        let viewed = move |k: &EntityKey| {
            let id = &model.entities[*k].id;
            self.views.iter().any(|v| v.entity_id == *id)
        };
        self.diags
            .iter()
            .filter(move |d| {
                !d.entities.is_empty()
                    && !d.entities.iter().any(viewed)
                    && (d.diag.severity() == Severity::Error
                        || matches!(d.diag, StateDiag::NeverRuns { .. }))
            })
            .map(move |d| {
                let ids = d
                    .entities
                    .iter()
                    .map(|k| model.entities[*k].id.as_str())
                    .collect();
                (ids, &d.diag)
            })
    }
}

/// Builds the system for `scope` at `Bounds::STATES` and explores each selected target's
/// cone. A `--buc` that drops use cases leaves the progress findings undecided; `--entity`
/// does not, because each selected cone is exact.
pub fn derive_states(model: &SemanticModel, scope: &Scope, caps: Caps) -> StatesReport {
    derive_states_built(model, scope, Bounds::STATES, caps).0
}

/// `derive_states` at `bounds`, and the system it explored, which names what the scope left out.
pub fn derive_states_built(
    model: &SemanticModel,
    scope: &Scope,
    bounds: Bounds,
    caps: Caps,
) -> (StatesReport, TransitionSystem) {
    let built = TransitionSystem::build(model, bounds, scope);
    let mut run = built.ts.explore_targets(&built.selection.targets, caps);
    if !scope.bucs.is_empty() {
        run.defer_progress();
    }
    let views = run.entity_views(&built.ts, model, &built.diags);
    let mut diags = built.diags;
    for d in run.verdicts() {
        if !diags.contains(d) {
            diags.push(d.clone());
        }
    }
    // BFS evaluates no temporal formula, but it can say that one holds vacuously. A scope
    // drops use cases or cones a property may need, so only the whole model is asked.
    if scope.is_whole() {
        diags.extend(built.ts.vacuous_properties(caps));
    }
    let report = StatesReport {
        bounds,
        caps,
        scope: scope.clone(),
        selection: built.selection,
        views,
        diags,
        undecided: run.undecided().to_vec(),
    };
    (report, built.ts)
}
