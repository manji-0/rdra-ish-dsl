//! Exact cone-of-influence reduction. A var set S is closed when every action that writes a
//! var in S has all the vars it reads in S, where a copy's guard counts as read only when the
//! var it pins is in S. Then the reachable states of `restrict_to(S)` are the reachable states
//! of the whole system projected onto S, so a view or verdict that reads only S can be
//! computed on the smaller system. The one exception is an action that also creates an entity
//! outside S: `restrict_to` drops that creation, the entity never runs out of slots, and the
//! cone reaches a superset (`cone_frees_slots`; the `bounds:` line says such entities are
//! unbounded).

use std::collections::BTreeSet;

use super::expr::State;
use super::types::{
    ActId, Action, ActionOrigin, Effect, EntId, Ltl, ParamRole, Pred, Term, VarId, VarKind,
};
use super::TransitionSystem;

pub type VarSet = BTreeSet<VarId>;

/// What a view or verdict reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// The per-entity view: existence and state columns.
    Entity(EntId),
    /// Index into `invariants()`.
    Invariant(usize),
    /// Index into `after_asserts()`.
    AfterAssert(usize),
}

/// A closed var set and the actions that must stay even when they write nothing in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cone {
    pub vars: VarSet,
    pub keep: BTreeSet<ActId>,
}

impl TransitionSystem {
    pub(crate) fn pred_reads(&self, pred: &Pred, out: &mut VarSet) {
        let term = |t: &Term, out: &mut VarSet| match t {
            Term::Slot { var, .. } => {
                out.insert(*var);
            }
            Term::Now => {
                out.extend(self.now);
            }
            Term::Lit(_) | Term::Int(_) | Term::Index(_) => {}
        };
        match pred {
            Pred::True | Pred::False => {}
            Pred::Cmp(a, _, b) => {
                term(a, out);
                term(b, out);
            }
            Pred::In(a, vs) => {
                term(a, out);
                vs.iter().for_each(|v| term(v, out));
            }
            Pred::Not(p) => self.pred_reads(p, out),
            Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| self.pred_reads(p, out)),
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                out.insert(self.entity(*entity).exists);
                self.pred_reads(body, out);
            }
        }
    }

    /// Guards, slot selection, transition sources, and the clock. Selecting a present slot
    /// reads its entity's existence; taking a new slot does not count as a read, so an entity
    /// a target never reads is never out of slots (`restrict_to` drops that param). A copy's
    /// guard is read only with the var it pins (`copy_reads`).
    pub(crate) fn action_reads(&self, action: &Action) -> VarSet {
        let mut out = VarSet::new();
        for p in action.params.iter().filter(|p| p.role != ParamRole::New) {
            out.insert(self.entity(p.entity).exists);
        }
        for g in action.guards.iter().filter(|g| g.copy_target().is_none()) {
            self.pred_reads(&g.pred, &mut out);
        }
        for eff in &action.effects {
            match eff {
                Effect::Transition { var, .. } => {
                    out.insert(*var);
                }
                Effect::Tick => out.extend(self.now),
                _ => {}
            }
        }
        out
    }

    pub(crate) fn effect_writes(&self, action: &Action, eff: &Effect) -> Vec<VarId> {
        let entity = |p: &u8| self.entity(action.params[usize::from(*p)].entity);
        match eff {
            Effect::Create { param } => vec![entity(param).exists],
            Effect::Delete { param } => {
                let d = entity(param);
                let mut v = vec![d.exists];
                v.extend(d.columns.iter().chain(d.owners.iter()));
                v
            }
            Effect::Assign { var, .. }
            | Effect::Transition { var, .. }
            | Effect::Choose { var, .. } => vec![*var],
            Effect::Tick => self.now.into_iter().collect(),
        }
    }

    fn action_writes(&self, action: &Action) -> VarSet {
        action
            .effects
            .iter()
            .flat_map(|e| self.effect_writes(action, e))
            .collect()
    }

    /// Each copy guard of the action: the var it pins and what it reads. Without the pinned var
    /// the guard always holds for some value of it, so the copy is read only with that var.
    fn copy_reads(&self, action: &Action) -> Vec<(VarId, VarSet)> {
        action
            .guards
            .iter()
            .filter_map(|g| {
                let target = g.copy_target()?;
                let mut reads = VarSet::new();
                self.pred_reads(&g.pred, &mut reads);
                Some((target, reads))
            })
            .collect()
    }

    /// The smallest closed set holding `seed` and everything the `keep` actions read.
    pub fn cone(&self, seed: &VarSet, keep: &BTreeSet<ActId>) -> VarSet {
        let reads: Vec<VarSet> = self.actions.iter().map(|a| self.action_reads(a)).collect();
        let copies: Vec<(VarId, VarSet)> = self
            .actions
            .iter()
            .flat_map(|a| self.copy_reads(a))
            .collect();
        let writes: Vec<VarSet> = self.actions.iter().map(|a| self.action_writes(a)).collect();
        let mut s = seed.clone();
        let mut taken = vec![false; self.actions.len()];
        loop {
            let mut grew = false;
            for i in 0..self.actions.len() {
                let relevant = keep.contains(&ActId::at(i)) || !writes[i].is_disjoint(&s);
                if relevant && !taken[i] {
                    taken[i] = true;
                    grew = true;
                    s.extend(reads[i].iter().copied());
                }
            }
            for (target, r) in &copies {
                if s.contains(target) && !r.is_subset(&s) {
                    s.extend(r.iter().copied());
                    grew = true;
                }
            }
            // A slot's vars mean nothing without its existence, and an owner var's domain
            // names its parent's slots.
            let existences: Vec<VarId> = s
                .iter()
                .flat_map(|v| match self.var(*v).kind {
                    VarKind::Owner { child, parent, .. } => {
                        vec![self.entity(child).exists, self.entity(parent).exists]
                    }
                    VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => {
                        vec![self.entity(entity).exists]
                    }
                    VarKind::Exists(_) | VarKind::Now => Vec::new(),
                })
                .collect();
            for e in existences {
                grew |= s.insert(e);
            }
            if !grew {
                return s;
            }
        }
    }

    /// The cone a target reads.
    pub fn target_cone(&self, target: Target) -> Cone {
        let mut seed = VarSet::new();
        let mut keep = BTreeSet::new();
        match target {
            Target::Entity(e) => {
                let d = self.entity(e);
                seed.insert(d.exists);
                seed.extend(d.columns.iter().copied());
                // Actions that touch the entity without changing it stay, with every guard,
                // so this cone decides whether they run (`NeverRuns`, findings.rs).
                for (i, a) in self.actions.iter().enumerate() {
                    if a.params
                        .iter()
                        .any(|p| p.entity == e && p.role != ParamRole::Parent)
                    {
                        keep.insert(ActId::at(i));
                    }
                }
            }
            Target::Invariant(i) => {
                let inv = &self.invariants[i];
                for e in &inv.binders {
                    seed.insert(self.entity(*e).exists);
                }
                self.pred_reads(&inv.body, &mut seed);
            }
            Target::AfterAssert(i) => {
                for step in &self.after[i].steps {
                    for e in &step.binders {
                        seed.insert(self.entity(*e).exists);
                    }
                    self.pred_reads(&step.post, &mut seed);
                    // A step is checked only when it changes the state, so the cone keeps
                    // everything the step writes: without it a real step would stutter.
                    seed.extend(self.action_writes(&self.actions[step.act.index()]));
                    keep.insert(step.act);
                }
            }
        }
        Cone {
            vars: self.cone(&seed, &keep),
            keep,
        }
    }

    /// The cone a temporal property is checked on: what its formula reads, and each action of
    /// its `.fair` use cases whole. A fair action's guards, param existences, transition
    /// sources, and the slots it creates decide `ENABLED` of the action, and its writes decide
    /// whether a step of it changes the cone; with all of them inside, weak fairness on the
    /// action restricted to the cone is weak fairness on the action in the whole model. Every
    /// other action outside the cone stutters for it (docs/formal-verification.md, "Temporal
    /// properties on a cone").
    pub fn property_cone(&self, i: usize) -> Cone {
        let p = &self.properties[i];
        let mut seed = VarSet::new();
        let mut keep = BTreeSet::new();
        for e in &p.binders {
            seed.insert(self.entity(*e).exists);
        }
        match &p.formula {
            Ltl::Always(x) | Ltl::Eventually(x) => self.pred_reads(x, &mut seed),
            Ltl::LeadsTo(x, y) => {
                self.pred_reads(x, &mut seed);
                self.pred_reads(y, &mut seed);
            }
        }
        for (k, a) in self.actions.iter().enumerate() {
            if !matches!(&a.origin, ActionOrigin::UseCase { uc, .. } if p.fair.contains(uc)) {
                continue;
            }
            keep.insert(ActId::at(k));
            seed.extend(self.action_writes(a));
            for param in a.params.iter().filter(|q| q.role == ParamRole::New) {
                seed.insert(self.entity(param.entity).exists);
            }
        }
        Cone {
            vars: self.cone(&seed, &keep),
            keep,
        }
    }

    /// Every var an action writes is in `vars`: every step that changes the whole system
    /// changes the cone, so `WF_vars(Next)` on the cone is `WF_vars(Next)` on the whole
    /// system.
    pub fn cone_keeps_every_write(&self, vars: &VarSet) -> bool {
        self.actions
            .iter()
            .all(|a| self.action_writes(a).is_subset(vars))
    }

    /// Some action of the cone takes a new slot of an entity outside it. That entity is
    /// unbounded in the cone, so the cone reaches a superset of the whole system's projection.
    pub fn cone_frees_slots(&self, cone: &Cone) -> bool {
        self.actions.iter().enumerate().any(|(i, a)| {
            let inside = cone.keep.contains(&ActId::at(i))
                || self.action_writes(a).iter().any(|v| cone.vars.contains(v));
            inside
                && a.params.iter().any(|p| {
                    p.role == ParamRole::New && !cone.vars.contains(&self.entity(p.entity).exists)
                })
        })
    }

    pub fn targets(&self) -> Vec<Target> {
        (0..self.entities.len())
            .map(|i| Target::Entity(EntId::at(i)))
            .chain((0..self.invariants.len()).map(Target::Invariant))
            .chain((0..self.after.len()).map(Target::AfterAssert))
            .collect()
    }

    /// A state of this system projected onto `vars`, in the layout of `restrict_to(vars, ..)`.
    pub fn project(&self, state: &State, vars: &VarSet) -> State {
        let mut out = Vec::new();
        for &v in vars {
            let width = match self.var(v).kind {
                VarKind::Now => 1,
                _ => self.bounds.slots(),
            };
            let at = self.offset(v);
            out.extend_from_slice(&state.0[at..at + width]);
        }
        State(out.into_boxed_slice())
    }
}
