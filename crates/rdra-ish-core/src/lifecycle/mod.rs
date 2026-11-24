//! The lifecycle layer: one transition system that both `states` and the TLA export read
//! (docs/vision.md, "The lifecycle layer has one semantics").
//!
//! `TransitionSystem::build` reads the model under explicit `Bounds` and returns the system
//! with its diagnostics. `explore_targets` runs one BFS per target over that target's cone (the
//! vars the target reads, closed under what writes them), and `TargetExploration::entity_views`
//! projects the reached states onto per-entity views shaped like the `states` output.

mod build;
mod cone;
mod diag;
mod explore;
mod expr;
mod findings;
mod guards;
mod lasso;
pub mod prove;
mod reach;
mod reduce;
mod restrict;
mod select;
mod starts;
mod states;
mod targets;
mod trace;
mod types;
mod view;

#[cfg(test)]
mod tests;

pub use cone::{Cone, Seed, Target, VarSet};
pub use diag::{
    created_via_apis, has_errors, label_prefix, Blocked, CapGrowth, Diag, Entry, Exit, Need,
    Severity, StateDiag, UnmetPremise, VarSpread,
};
pub use explore::{
    store_bytes, Adjacency, Caps, ComponentRun, Edge, Estimate, Graph, Held, LabelledEdges,
    LassoFault, Layers, PropertyVerdict, ReducedVerdict, RunEnd, Spurious, States,
};
pub use expr::{State, StateSlice};
pub use findings::{
    ActionRuns, ArmTaken, ModelState, Progress, Runs, StateModelFindings, Taken, ValueProgress,
    ValueWitness, Witness,
};
pub use guards::{GuardDeps, GuardKind, GuardSubject};
pub use lasso::{Fairness, LassoReplay, Loop, Unfair};
pub use reduce::{ClassMove, Divergence, Fold, FoldKey, QuotientCache, Reduction, Step};
pub use select::{NotChecked, Scope, Selection, UnknownId, Without};
pub use states::{derive_states, derive_states_built, BoundsLine, FindingTally, StatesReport};
pub use targets::{ConeRun, Subject, Tally, TargetExploration, Undecided, UndecidedWhy};
pub use trace::{Change, Inst, Replay, ReplayError, Trace, TraceStep};
pub use types::{
    ActId, Action, ActionOrigin, AfterAssert, AfterStep, Arm, At, Birth, Bounds, Choice, CmpOp,
    Domain, Effect, EntId, EntityDecl, Guard, GuardWhy, InvKind, InvText, Invariant, Ltl, LtlKind,
    ModelRef, Param, ParamRole, Pred, Property, Reach, Rhs, Site, Start, Term, Value, VarDecl,
    VarId, VarKind,
};
pub use view::{
    AbstractValue, AxisKind, EntityStateResult, Provenance, ReachablePattern, StateAxis,
    StatePattern,
};

use crate::model::SemanticModel;

/// A built system and what the build found. `build` never fails: callers gate on
/// `has_errors(&built.diags)`. A scope's `--buc` decides which use cases the system has;
/// birth is decided on the whole model either way, so a scope never changes how an entity
/// starts.
pub struct Built {
    pub ts: TransitionSystem,
    /// Blocking: an error here stops exports and `verify`.
    pub diags: Vec<Diag>,
    /// The targets the scope answers; errors it defers are here, not in `diags`.
    pub selection: Selection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionSystem {
    pub bounds: Bounds,
    entities: Vec<EntityDecl>,
    vars: Vec<VarDecl>,
    actions: Vec<Action>,
    invariants: Vec<Invariant>,
    after: Vec<AfterAssert>,
    properties: Vec<Property>,
    reach: Vec<Reach>,
    now: Option<VarId>,
    offsets: Vec<usize>,
    width: usize,
    /// The id of each API a `New` param is created through (`Param::via_api`).
    api_ids: std::collections::HashMap<crate::model::ApiKey, String>,
    /// The S side of a reduced cone (`reduced`).
    reduction: Option<std::sync::Arc<Reduction>>,
    /// What the move generator reads of each action's guards (`explore::Plans`).
    plans: explore::Plans,
}

impl TransitionSystem {
    pub fn build(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Built {
        build::build(model, bounds, scope)
    }

    fn empty(bounds: Bounds) -> Self {
        TransitionSystem {
            bounds,
            entities: Vec::new(),
            vars: Vec::new(),
            actions: Vec::new(),
            invariants: Vec::new(),
            after: Vec::new(),
            properties: Vec::new(),
            reach: Vec::new(),
            now: None,
            offsets: Vec::new(),
            width: 0,
            api_ids: std::collections::HashMap::new(),
            reduction: None,
            plans: explore::Plans::default(),
        }
    }

    /// Assigns each var its slots in the flat state.
    fn layout(&mut self) {
        self.plans = explore::Plans::default();
        self.offsets.clear();
        let mut at = 0;
        for var in &self.vars {
            self.offsets.push(at);
            at += self.slots_of(&var.kind);
        }
        self.width = at;
    }

    pub(crate) fn offset(&self, var: VarId) -> usize {
        self.offsets[var.index()]
    }

    /// How many state bytes a var of this kind takes: `now` is one slot wide, the rest one
    /// per slot of their entity.
    pub fn slots_of(&self, kind: &VarKind) -> usize {
        match kind.entity() {
            None => 1,
            Some(e) => self.slots(e),
        }
    }

    /// The slots of entity `e` under the system's bounds.
    pub fn slots(&self, e: EntId) -> usize {
        usize::from(self.entity(e).slots.get())
    }

    /// The largest slot count of any entity, or 1 without entities.
    pub fn most_slots(&self) -> usize {
        self.entity_ids().map(|e| self.slots(e)).max().unwrap_or(1)
    }

    pub fn entities(&self) -> &[EntityDecl] {
        &self.entities
    }

    pub fn entity(&self, id: EntId) -> &EntityDecl {
        &self.entities[id.index()]
    }

    /// The id of an API a creation goes through.
    pub fn api_id(&self, api: crate::model::ApiKey) -> String {
        self.api_ids.get(&api).cloned().unwrap_or_default()
    }

    pub fn vars(&self) -> &[VarDecl] {
        &self.vars
    }

    pub fn var(&self, id: VarId) -> &VarDecl {
        &self.vars[id.index()]
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    pub fn after_asserts(&self) -> &[AfterAssert] {
        &self.after
    }

    pub fn properties(&self) -> &[Property] {
        &self.properties
    }

    /// The `can_reach` properties, which BFS decides and TLC does not check.
    pub fn reach_properties(&self) -> &[Reach] {
        &self.reach
    }

    pub fn now(&self) -> Option<VarId> {
        self.now
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = EntId> {
        (0..self.entities.len()).map(EntId::at)
    }

    pub fn var_ids(&self) -> impl Iterator<Item = VarId> {
        (0..self.vars.len()).map(VarId::at)
    }

    pub fn action_ids(&self) -> impl Iterator<Item = ActId> {
        (0..self.actions.len()).map(ActId::at)
    }

    /// Column name of a column or proposition var, as views label it.
    pub fn var_name(&self, var: VarId) -> String {
        match &self.var(var).kind {
            VarKind::Column { column, .. } => column.clone(),
            VarKind::Prop { key, .. } => format!("__cmp:{key}"),
            VarKind::Exists(e) => format!("{}._exists", self.entity(*e).id),
            VarKind::Owner {
                child,
                parent,
                role,
                ..
            } => {
                let name = role.as_deref().unwrap_or(&self.entity(*parent).id);
                format!("{}._owner_{name}", self.entity(*child).id)
            }
            VarKind::Run { entity, usecase } => {
                format!("{}._run_{usecase}", self.entity(*entity).id)
            }
            VarKind::Raised {
                entity,
                event,
                handler: None,
            } => {
                format!("{}._raised_{event}", self.entity(*entity).id)
            }
            VarKind::Raised {
                entity,
                event,
                handler: Some(uc),
            } => {
                format!("{}._pending_{event}_{uc}", self.entity(*entity).id)
            }
            VarKind::Now => "now".to_string(),
        }
    }
}
