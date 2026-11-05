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
    ActId, Action, ActionOrigin, At, Effect, EntId, Guard, GuardWhy, Ltl, ParamRole, Pred, Term,
    VarId, VarKind,
};
use super::TransitionSystem;

/// The params a predicate names, in a slot or an index, in the pre-state or the successor.
pub(crate) fn named_params(pred: &Pred) -> Vec<u8> {
    fn term(t: &Term, out: &mut Vec<u8>) {
        if let Term::Slot {
            at: At::Param(p) | At::Next(p),
            ..
        }
        | Term::Index(At::Param(p) | At::Next(p)) = t
        {
            if !out.contains(p) {
                out.push(*p);
            }
        }
    }
    fn walk(pred: &Pred, out: &mut Vec<u8>) {
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
            Pred::Not(p) => walk(p, out),
            Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| walk(p, out)),
            Pred::Forall { body, .. } | Pred::Exists { body, .. } => walk(body, out),
        }
    }
    let mut out = Vec::new();
    walk(pred, &mut out);
    out
}

pub type VarSet = BTreeSet<VarId>;
/// What a target reads and the actions it keeps: the seed of its cone.
pub type Seed = (VarSet, BTreeSet<ActId>);

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
    /// a target never reads is never out of slots (`restrict_to` drops that param), and an
    /// `Earlier` slot is selected by its run mark, a guard. A copy's guard is read only with
    /// the var it pins (`copy_reads`).
    pub(crate) fn action_reads(&self, action: &Action) -> VarSet {
        let mut out = self.plain_reads(action);
        for (mark, reads) in self.mark_reads(action) {
            out.insert(mark);
            out.extend(reads);
        }
        out
    }

    /// The `Triggered` params of an action, each with the mark its guard reads.
    pub(crate) fn triggered_params(&self, action: &Action) -> Vec<(u8, VarId)> {
        action
            .params
            .iter()
            .enumerate()
            .filter(|(_, param)| param.role == ParamRole::Triggered)
            .filter_map(|(p, _)| {
                let p = p as u8;
                let mark = action.guards.iter().find_map(|g| match (&g.why, &g.pred) {
                    (
                        GuardWhy::Raised { .. },
                        Pred::Cmp(
                            Term::Slot {
                                var,
                                at: At::Param(q),
                            },
                            ..,
                        ),
                    ) if *q == p => Some(*var),
                    _ => None,
                })?;
                Some((p, mark))
            })
            .collect()
    }

    /// The mark a guard belongs to: the one it reads, or the one that selects the `Triggered`
    /// param it names (its link).
    fn guard_mark(&self, action: &Action, g: &Guard) -> Option<VarId> {
        if let GuardWhy::Raised { .. } = g.why {
            let mut reads = VarSet::new();
            self.pred_reads(&g.pred, &mut reads);
            return reads.into_iter().next();
        }
        let triggered = self.triggered_params(action);
        if triggered.is_empty() {
            return None;
        }
        let named = named_params(&g.pred);
        triggered
            .into_iter()
            .find(|(p, _)| named.contains(p))
            .map(|(_, mark)| mark)
    }

    /// The reads other than those a mark brings (`mark_reads`).
    pub(crate) fn plain_reads(&self, action: &Action) -> VarSet {
        let mut out = VarSet::new();
        for p in action
            .params
            .iter()
            .filter(|p| matches!(p.role, ParamRole::Existing | ParamRole::Parent { .. }))
        {
            out.insert(self.entity(p.entity).exists);
        }
        for g in action
            .guards
            .iter()
            .filter(|g| g.copy_target().is_none() && self.guard_mark(action, g).is_none())
        {
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

    /// Per mark the action reads, what reading it brings besides the mark: the existence of
    /// a `Triggered` param it selects and what that param's link guard reads. A cone that
    /// leaves the mark out leaves these out with it (`cone`, `restrict_to`).
    pub(crate) fn mark_reads(&self, action: &Action) -> Vec<(VarId, VarSet)> {
        let mut out: Vec<(VarId, VarSet)> = Vec::new();
        let mut add = |mark: VarId, reads: VarSet| match out.iter_mut().find(|(m, _)| *m == mark) {
            Some((_, set)) => set.extend(reads),
            None => out.push((mark, reads)),
        };
        for (p, mark) in self.triggered_params(action) {
            let exists = self.entity(action.params[usize::from(p)].entity).exists;
            add(mark, VarSet::from([exists]));
        }
        for g in action.guards.iter().filter(|g| g.copy_target().is_none()) {
            let Some(mark) = self.guard_mark(action, g) else {
                continue;
            };
            let mut reads = VarSet::new();
            self.pred_reads(&g.pred, &mut reads);
            reads.remove(&mark);
            add(mark, reads);
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

    pub(crate) fn action_writes(&self, action: &Action) -> VarSet {
        action
            .effects
            .iter()
            .flat_map(|e| self.effect_writes(action, e))
            .collect()
    }

    /// Each copy guard of the action: the var it pins and what it reads. Without the pinned var
    /// the guard always holds for some value of it, so the copy is read only with that var.
    pub(crate) fn copy_reads(&self, action: &Action) -> Vec<(VarId, VarSet)> {
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

    /// The smallest closed set holding `seed` and everything the `keep` actions read, except
    /// that a mark is read exactly only by a `keep` action or when every action that sets it
    /// is in the set by its own writes; otherwise it stays out, with the raiser's lifecycle,
    /// and `restrict_to` drops the guards on it (docs/state-derivation.md, "Triggers").
    pub fn cone(&self, seed: &VarSet, keep: &BTreeSet<ActId>) -> VarSet {
        let reads: Vec<VarSet> = self.actions.iter().map(|a| self.plain_reads(a)).collect();
        let marks: Vec<Vec<(VarId, VarSet)>> =
            self.actions.iter().map(|a| self.mark_reads(a)).collect();
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
            for i in (0..self.actions.len()).filter(|&i| taken[i]) {
                for (mark, r) in &marks[i] {
                    let exact = keep.contains(&ActId::at(i))
                        || (0..self.actions.len())
                            .filter(|&j| writes[j].contains(mark))
                            .all(|j| taken[j]);
                    if exact {
                        grew |= s.insert(*mark);
                        for v in r {
                            grew |= s.insert(*v);
                        }
                    }
                }
            }
            for (target, r) in &copies {
                if s.contains(target) && !r.is_subset(&s) {
                    s.extend(r.iter().copied());
                    grew = true;
                }
            }
            let existences: Vec<VarId> = s.iter().flat_map(|v| self.existences(*v)).collect();
            for e in existences {
                grew |= s.insert(e);
            }
            if !grew {
                return s;
            }
        }
    }

    /// The existence var of the entity a var belongs to; `None` for an existence or the clock.
    pub(crate) fn own_existence(&self, v: VarId) -> Option<VarId> {
        match self.var(v).kind {
            VarKind::Owner { child: entity, .. }
            | VarKind::Column { entity, .. }
            | VarKind::Prop { entity, .. }
            | VarKind::Run { entity, .. }
            | VarKind::Raised { entity, .. } => Some(self.entity(entity).exists),
            VarKind::Exists(_) | VarKind::Now => None,
        }
    }

    /// The existence vars a var means nothing without: a slot's vars need its existence, and
    /// an owner var's domain names its parent's slots.
    pub(crate) fn existences(&self, v: VarId) -> Vec<VarId> {
        match self.var(v).kind {
            VarKind::Owner { child, parent, .. } => {
                vec![self.entity(child).exists, self.entity(parent).exists]
            }
            VarKind::Column { entity, .. }
            | VarKind::Prop { entity, .. }
            | VarKind::Run { entity, .. }
            | VarKind::Raised { entity, .. } => vec![self.entity(entity).exists],
            VarKind::Exists(_) | VarKind::Now => Vec::new(),
        }
    }

    /// The cone a target reads.
    pub fn target_cone(&self, target: Target) -> Cone {
        let (seed, keep) = self.target_seed(target);
        Cone {
            vars: self.cone(&seed, &keep),
            keep,
        }
    }

    /// What a target reads directly, and the actions its cone keeps whatever they write.
    pub fn target_seed(&self, target: Target) -> Seed {
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
                        .any(|p| p.entity == e && p.role.is_written())
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
        (seed, keep)
    }

    /// The cone a temporal property is checked on: what its formula reads, and each action of
    /// its `.fair` use cases whole. A fair action's guards, param existences, transition
    /// sources, and the slots it creates decide `ENABLED` of the action, and its writes decide
    /// whether a step of it changes the cone; with all of them inside, weak fairness on the
    /// action restricted to the cone is weak fairness on the action in the whole model. Every
    /// other action outside the cone stutters for it (docs/formal-verification.md, "Temporal
    /// properties on a cone").
    pub fn property_cone(&self, i: usize) -> Cone {
        let (seed, keep) = self.property_seed(i);
        Cone {
            vars: self.cone(&seed, &keep),
            keep,
        }
    }

    /// What a property reads directly, with its fair actions' writes and new slots, and those
    /// actions as kept.
    pub fn property_seed(&self, i: usize) -> Seed {
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
        (seed, keep)
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
