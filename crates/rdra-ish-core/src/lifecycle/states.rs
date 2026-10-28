//! What `states`, `check`, and the generator gates read: the system at `Bounds::STATES`, each
//! entity view and each constraint verdict computed on its own target cone.

use super::diag::{Diag, Severity, StateDiag};
use super::explore::Caps;
use super::view::EntityStateResult;
use super::{Bounds, Scope, TransitionSystem};
use crate::model::SemanticModel;

impl Caps {
    /// `--max-patterns` per entity view, and 16 joint states per allowed pattern for each
    /// target cone. The largest cone of any sample reaches 184 states, under the 4096 default.
    pub const fn per_cone(patterns: usize) -> Self {
        Caps::new(patterns, patterns.saturating_mul(16))
    }
}

pub struct StatesReport {
    pub bounds: Bounds,
    pub caps: Caps,
    /// Sorted by entity id; each carries the diagnostics that name its entity.
    pub views: Vec<EntityStateResult>,
    /// Every diagnostic of the build and the cone runs' verdicts.
    pub diags: Vec<Diag>,
}

impl StatesReport {
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
    let run = built.ts.explore_targets(caps);
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
