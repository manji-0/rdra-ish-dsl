//! The lifecycle layer: one transition system that both `states` and the TLA export read
//! (docs/vision.md, "The lifecycle layer has one semantics").
//!
//! `TransitionSystem::build` reads the model under explicit `Bounds` and returns the system
//! with its diagnostics. `explore` runs a BFS per independent component (`components`,
//! `restrict`), and `Exploration::entity_views` projects the reached states onto per-entity
//! views shaped like the `states` output.

mod build;
mod cone;
mod diag;
mod explore;
mod expr;
mod restrict;
mod starts;
mod states;
mod targets;
mod trace;
mod types;
mod view;

#[cfg(test)]
mod tests;

pub use cone::{Cone, Target, VarSet};
pub use diag::{has_errors, Diag, Severity, StateDiag};
pub use explore::{Caps, ComponentRun, Edge, Exploration};
pub use expr::State;
pub use restrict::Component;
pub use states::{derive_states, StatesReport};
pub use targets::{ConeRun, TargetExploration};
pub use trace::{Change, Inst, Replay, ReplayError, Trace, TraceStep};
pub use types::{
    ActId, Action, ActionOrigin, AfterAssert, AfterStep, At, Birth, Bounds, CmpOp, Domain, Effect,
    EntId, EntityDecl, Guard, GuardWhy, InvKind, InvText, Invariant, Ltl, ModelRef, Param,
    ParamRole, Pred, Property, Rhs, Site, Start, Term, Value, VarDecl, VarId, VarKind,
};
pub use view::{
    AbstractValue, AxisKind, EntityStateResult, Provenance, ReachablePattern, StateAxis,
    StatePattern,
};

use crate::model::SemanticModel;

/// Which use cases the system includes. Birth is decided on the whole model either way, so a
/// scope never changes how an entity starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    /// Use cases reachable from these BUC ids.
    Bucs(Vec<String>),
}

/// A built system and what the build found. `build` never fails: callers gate on
/// `has_errors(&built.diags)`.
pub struct Built {
    pub ts: TransitionSystem,
    pub diags: Vec<Diag>,
}

#[derive(Clone, Debug)]
pub struct TransitionSystem {
    pub bounds: Bounds,
    entities: Vec<EntityDecl>,
    vars: Vec<VarDecl>,
    actions: Vec<Action>,
    invariants: Vec<Invariant>,
    after: Vec<AfterAssert>,
    properties: Vec<Property>,
    now: Option<VarId>,
    offsets: Vec<usize>,
    width: usize,
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
            now: None,
            offsets: Vec::new(),
            width: 0,
        }
    }

    /// Assigns each var its slots in the flat state.
    fn layout(&mut self) {
        self.offsets.clear();
        let mut at = 0;
        for var in &self.vars {
            self.offsets.push(at);
            at += match var.kind {
                VarKind::Now => 1,
                _ => self.bounds.slots(),
            };
        }
        self.width = at;
    }

    pub(crate) fn offset(&self, var: VarId) -> usize {
        self.offsets[var.index()]
    }

    pub fn entities(&self) -> &[EntityDecl] {
        &self.entities
    }

    pub fn entity(&self, id: EntId) -> &EntityDecl {
        &self.entities[id.index()]
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
            VarKind::Now => "now".to_string(),
        }
    }
}
