//! Exact cone-of-influence reduction. A var set S is closed when every action that writes a
//! var in S has all the vars it reads in S, where a copy's guard counts as read only when the
//! var it pins is in S. Then the reachable states of `restrict_to(S)` are the reachable states
//! of the whole system projected onto S, so a view or verdict that reads only S can be
//! computed on the smaller system. The one exception is an action that also creates an entity
//! outside S: `restrict_to` drops that creation, the entity never runs out of slots, and the
//! cone reaches a superset (`cone_frees_slots`; the `bounds:` line says such entities are
//! unbounded).

use crate::lifecycle::narrow::id8;
use std::collections::BTreeSet;

use super::cone_check::Exactness;
use super::expr::{State, StateSlice};
use super::types::{
    ActId, Action, At, Effect, EntId, Guard, GuardWhy, ParamRole, Pred, Property, Term, VarId,
    VarKind,
};
use super::TransitionSystem;

/// The params a predicate names, in a slot or an index, in the pre-state or the successor.
pub(crate) fn named_params(pred: &Pred) -> Vec<u8> {
    let mut out = Vec::new();
    for t in pred.terms() {
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
    /// Index into `reach_properties()`.
    Reach(usize),
}

/// A closed var set and the actions that must stay even when they write nothing in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cone {
    pub vars: VarSet,
    pub keep: BTreeSet<ActId>,
}

impl TransitionSystem {
    /// What a check reads: the existence of each entity it binds, and every var of `preds`.
    pub(crate) fn check_reads<'p>(
        &self,
        binders: &[EntId],
        preds: impl IntoIterator<Item = &'p Pred>,
    ) -> VarSet {
        let mut reads = VarSet::new();
        for e in binders {
            reads.insert(self.entity(*e).exists);
        }
        for p in preds {
            self.pred_reads(p, &mut reads);
        }
        reads
    }

    /// What a property reads: `check_reads` of its link and operands, and the columns of
    /// `unchanged`.
    pub(crate) fn property_reads(&self, p: &Property) -> VarSet {
        let mut reads = self.check_reads(&p.binders, p.preds());
        for t in p.formula.columns() {
            self.term_reads(t, &mut reads);
        }
        reads
    }

    fn term_reads(&self, t: &Term, out: &mut impl Extend<VarId>) {
        match t {
            Term::Slot { var, .. } => out.extend([*var]),
            Term::Now => out.extend(self.now),
            Term::Lit(_) | Term::Int(_) | Term::Index(_) => {}
        }
    }

    /// The vars a predicate reads, into any set: a `VarSet`, or a Kani harness's mask. The
    /// existence of each entity a quantifier ranges over, and what each term reads. It
    /// recurses over the tree, as `Pred::each_term` does, so a Kani harness can walk it.
    pub(crate) fn pred_reads(&self, pred: &Pred, out: &mut impl Extend<VarId>) {
        match pred {
            Pred::Forall { entity, body }
            | Pred::Exists { entity, body }
            | Pred::Count { entity, body, .. } => {
                out.extend([self.entity(*entity).exists]);
                self.pred_reads(body, out);
            }
            Pred::Not(body) => self.pred_reads(body, out),
            Pred::And(parts) | Pred::Or(parts) => {
                parts.iter().for_each(|p| self.pred_reads(p, out));
            }
            Pred::True | Pred::False | Pred::Cmp(..) | Pred::In(..) => {
                pred.each_term(&mut |t| self.term_reads(t, out));
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
                let p = id8(p);
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

    /// Whether `v` is the counter of a counted edge (`triggers(Ev, UC).counted(k)`), which is
    /// not monotone and so is never havocked or taken as an input of a sub-lifecycle
    /// (docs/design/counted-events.md).
    pub(crate) fn is_counted(&self, v: VarId) -> bool {
        matches!(
            self.var(v).kind,
            VarKind::Raised {
                handler: Some(_),
                ..
            }
        )
    }

    /// The smallest closed set holding `seed` and everything the `keep` actions read, except
    /// that a latch is read exactly only by a `keep` action or when every action that sets it
    /// is in the set by its own writes; otherwise it stays out, with the raiser's lifecycle,
    /// and `restrict_to` drops the guards on it (docs/state-derivation.md, "Triggers"). A
    /// counter is always read exactly: dropping its guard would drop the handler's decrement
    /// with it, and the handler's and the raisers' transitions read it anyway.
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
                        || self.is_counted(*mark)
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
    ///
    /// An entity's findings say that a state is reached, that a use case runs, or that a
    /// state can be left: a run the cone adds can make one of them hold where the whole
    /// system does not. So an entity's cone that would leave a raised mark out, and run the
    /// handler without its event, is its exact cone instead (`exact_target_cone`, claim A1 of
    /// docs/theory/claims.md). A check's cone stays small: only its `ok` is read off a
    /// superset, and a counterexample it cannot vouch for is decided on the exact cone
    /// (`recheck`).
    pub fn target_cone(&self, target: Target) -> Cone {
        let (seed, keep) = self.target_seed(target);
        let cone = Cone {
            vars: self.cone(&seed, &keep),
            keep,
        };
        let need = match target {
            Target::Entity(_) if self.cone_leaves_marks(&cone) => {
                return self.exact_target_cone(target, false)
            }
            Target::Entity(_) => Exactness {
                marks: true,
                slots: false,
            },
            Target::Reach(_) => Exactness {
                marks: true,
                slots: true,
            },
            Target::Invariant(_) | Target::AfterAssert(_) => Exactness::NONE,
        };
        self.certified(&seed, cone, need)
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
            Target::Reach(i) => return self.reach_seed(i),
        }
        (seed, keep)
    }

    /// What a `can_reach` reads, grown until its cone is exact. `EF` is not monotone in the
    /// transitions: a step the cone allows and the whole model does not (a creation of an
    /// entity whose slots the cone does not count, or a use case run without its event's
    /// mark) could reach the goal where the whole model cannot. So every action that writes
    /// into the cone is kept, which brings in the marks it reads, and the existence of each
    /// entity it creates is read; with no such step left out, a path of the cone is a path of
    /// the whole model and the other way round.
    fn reach_seed(&self, i: usize) -> Seed {
        let r = &self.reach[i];
        let mut seed = self.check_reads(&r.binders, r.preds());
        let keep = self.keep_writers(&mut seed, true);
        (seed, keep)
    }

    /// The smallest set holding `seed` that decides whether a step can still change it: every
    /// action that writes into it is kept, so each mark such an action reads is in it, with
    /// what the mark brings, and so is every other var it reads. A step that leaves the set as
    /// it was then never enables a step that writes it: it changes nothing such a step reads,
    /// and a slot it takes is one fewer for a creation. `cone` alone leaves a mark out when
    /// the raiser is not in by its own writes, and the raiser's step, which changes nothing
    /// of the set, then enables a handler's step that does (#261).
    pub fn closed_cone(&self, seed: &VarSet) -> VarSet {
        let mut seed = seed.clone();
        let keep = self.keep_writers(&mut seed, false);
        self.cone(&seed, &keep)
    }

    /// Every action that writes into the cone of `seed` with them kept; with `new_slots`,
    /// `seed` also grows by the existence of each entity such an action creates.
    fn keep_writers(&self, seed: &mut VarSet, new_slots: bool) -> BTreeSet<ActId> {
        let mut keep = BTreeSet::new();
        loop {
            let vars = self.cone(seed, &keep);
            let mut grew = false;
            for (k, a) in self.actions.iter().enumerate() {
                if !keep.contains(&ActId::at(k)) && self.action_writes(a).is_disjoint(&vars) {
                    continue;
                }
                grew |= keep.insert(ActId::at(k));
                if new_slots {
                    for p in a.params.iter().filter(|p| p.role == ParamRole::New) {
                        grew |= seed.insert(self.entity(p.entity).exists);
                    }
                }
            }
            if !grew {
                return keep;
            }
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
        let (seed, keep) = self.property_seed(i);
        let cone = Cone {
            vars: self.cone(&seed, &keep),
            keep,
        };
        self.certified(&seed, cone, Exactness::NONE)
    }

    /// What a property reads directly, with its fair actions' writes and new slots, and those
    /// actions as kept.
    pub fn property_seed(&self, i: usize) -> Seed {
        let p = &self.properties[i];
        let mut seed = self.property_reads(p);
        let mut keep = BTreeSet::new();
        for (k, a) in self.actions.iter().enumerate() {
            if !p.is_fair(a) {
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

    /// Some action of the cone reads a raised mark the cone leaves out (`cone`). The action
    /// runs there without its event, so the cone reaches states the event order forbids at
    /// every bound: its `ok` holds, and its counterexample may be no run of the whole system.
    pub fn cone_leaves_marks(&self, cone: &Cone) -> bool {
        self.actions.iter().enumerate().any(|(i, a)| {
            let inside = cone.keep.contains(&ActId::at(i))
                || self.action_writes(a).iter().any(|v| cone.vars.contains(v));
            inside
                && self
                    .mark_reads(a)
                    .iter()
                    .any(|(mark, _)| !cone.vars.contains(mark))
        })
    }

    /// The cone of `target` grown until a counterexample on it is a run of the whole system:
    /// every action that writes into it is kept, so each mark it reads is in it
    /// (`closed_cone`); with `slots`, the existence of each entity such an action creates is
    /// in it too, so no entity runs out of slots later than in the whole system
    /// (`cone_frees_slots`). A check whose cone counterexample the cone cannot vouch for is
    /// decided again on it (`recheck`).
    pub fn exact_target_cone(&self, target: Target, slots: bool) -> Cone {
        let (mut seed, mut keep) = self.target_seed(target);
        keep.extend(self.keep_writers(&mut seed, slots));
        let cone = Cone {
            vars: self.cone(&seed, &keep),
            keep,
        };
        debug_assert!(
            !self.cone_leaves_marks(&cone),
            "an exact cone reads every mark"
        );
        debug_assert!(
            !slots || !self.cone_frees_slots(&cone),
            "and counts every slot"
        );
        self.certified(&seed, cone, Exactness { marks: true, slots })
    }

    /// The targets `check` and `states` explore: views and safety checks. A `can_reach` is a
    /// temporal property, which only `verify` decides (`reach_targets`).
    pub fn targets(&self) -> Vec<Target> {
        (0..self.entities.len())
            .map(|i| Target::Entity(EntId::at(i)))
            .chain((0..self.invariants.len()).map(Target::Invariant))
            .chain((0..self.after.len()).map(Target::AfterAssert))
            .collect()
    }

    /// One target per `can_reach` property, which `verify` explores beside the entity cones.
    pub fn reach_targets(&self) -> Vec<Target> {
        (0..self.reach.len()).map(Target::Reach).collect()
    }

    /// A state of this system projected onto `vars`, in the layout of `restrict_to(vars, ..)`.
    pub fn project(&self, state: &StateSlice, vars: &VarSet) -> State {
        let mut out = Vec::new();
        for &v in vars {
            let width = self.slots_of(&self.var(v).kind);
            let at = self.offset(v);
            out.extend_from_slice(&state.0[at..at + width]);
        }
        State(out.into_boxed_slice())
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`): `pred_reads`, the read set of a predicate
/// that the cone and the split build `plain_reads`, `mark_reads` and `copy_reads` from, covers
/// what `eval` reads. On a fixed system, with symbolic states, successors, bindings and leaves.
///
/// The system has an entity `E` (existence, an Int column, a Bool column, an optional owner
/// of `P`), an entity `P` (existence alone), two slots each, and a clock: 11 state bytes. A
/// predicate is well typed, as the build makes it: a slot term reads a var of the entity of
/// the param or binder it names, and the clock only through `Term::Now`; a binding puts each
/// param on a slot of its entity. Without that a term could read past its var into the next
/// one, which no read set names.
#[cfg(kani)]
mod proofs {
    use super::*;
    use crate::lifecycle::expr::Env;
    use crate::lifecycle::types::{
        Birth, Bounds, CmpOp, Domain, EntityDecl, ModelRef, Site, Start, Value, VarDecl,
    };
    use std::num::NonZeroU8;

    const WIDTH: usize = 11;
    /// The var each state byte belongs to.
    const BYTE_VAR: [usize; WIDTH] = [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5];
    const E: EntId = EntId(0);
    const P: EntId = EntId(1);
    const EXISTS_E: VarId = VarId(0);
    const EXISTS_P: VarId = VarId(1);
    const INT_E: VarId = VarId(2);
    const BOOL_E: VarId = VarId(3);
    const OWNER_E: VarId = VarId(4);
    const NOW: VarId = VarId(5);
    /// The state byte of the clock.
    const CLOCK: usize = 10;
    /// The `int_max` of the bounds, which no harness here reads.
    const INT_MAX: u8 = 3;

    fn site() -> Site {
        Site::bare(ModelRef::Synthesized)
    }

    fn system() -> TransitionSystem {
        let two = NonZeroU8::new(2).unwrap();
        let mut ts = TransitionSystem::empty(Bounds::uniform(two, INT_MAX));
        let var = |kind, domain| VarDecl {
            kind,
            domain,
            base: Value(0),
            start: Start::Base,
            site: site(),
            ends: Vec::new(),
        };
        let column = |entity| VarKind::Column {
            entity,
            column: String::new(),
            lifecycle: true,
            variation: false,
        };
        ts.vars = vec![
            var(VarKind::Exists(E), Domain::Existence),
            var(VarKind::Exists(P), Domain::Existence),
            var(column(E), Domain::Int),
            var(column(E), Domain::Bool),
            var(
                VarKind::Owner {
                    child: E,
                    parent: P,
                    fk: crate::model::FkId::at(0),
                    role: None,
                },
                Domain::Instance {
                    parent: P,
                    optional: true,
                },
            ),
            var(VarKind::Now, Domain::Int),
        ];
        let entity = |exists, columns, owners| EntityDecl {
            key: Default::default(),
            id: String::new(),
            label: String::new(),
            birth: Birth::Created,
            slots: two,
            exists,
            columns,
            owners,
        };
        ts.entities = vec![
            entity(EXISTS_E, vec![INT_E, BOOL_E], vec![OWNER_E]),
            entity(EXISTS_P, Vec::new(), Vec::new()),
        ];
        ts.now = Some(NOW);
        // The offsets `layout` gives two slots a var and one for the clock. `layout` itself
        // drops the system's `Plans`, whose drop glue CBMC cannot bound.
        ts.offsets = vec![0, 2, 4, 6, 8, CLOCK];
        ts.width = WIDTH;
        ts
    }

    /// The vars of an entity's slots.
    fn vars_of(e: EntId) -> &'static [VarId] {
        if e == E {
            &[EXISTS_E, INT_E, BOOL_E, OWNER_E]
        } else {
            &[EXISTS_P]
        }
    }

    fn any_op() -> CmpOp {
        match kani::any::<u8>() % 6 {
            0 => CmpOp::Eq,
            1 => CmpOp::Ne,
            2 => CmpOp::Lt,
            3 => CmpOp::Le,
            4 => CmpOp::Gt,
            _ => CmpOp::Ge,
        }
    }

    /// A slot of either entity.
    fn any_slot() -> u8 {
        let s: u8 = kani::any();
        kani::assume(s < 2);
        s
    }

    /// Two param slots, each on a slot of its param's entity.
    fn any_binding() -> [u8; 2] {
        [any_slot(), any_slot()]
    }

    /// The entities of a predicate's params and of the binders around it, outermost first.
    struct Scope<'a> {
        params: &'a [EntId],
        bound: Vec<EntId>,
    }

    /// Any var of `e`.
    fn any_var_of(e: EntId) -> VarId {
        let vars = vars_of(e);
        let k: usize = kani::any();
        kani::assume(k < vars.len());
        vars[k]
    }

    /// A param, its successor row, or a binder of the scope, as an `At`, with its entity.
    fn any_at(scope: &Scope) -> (At, EntId) {
        if !scope.bound.is_empty() && kani::any::<bool>() {
            let b: usize = kani::any();
            kani::assume(b < scope.bound.len());
            (At::Bound(id8(b)), scope.bound[b])
        } else {
            let p: usize = kani::any();
            kani::assume(p < scope.params.len());
            let at = if kani::any() {
                At::Param(id8(p))
            } else {
                At::Next(id8(p))
            };
            (at, scope.params[p])
        }
    }

    /// Any well-typed term: a slot of a param, of its successor row, or of a binder; a
    /// literal; an integer; a slot index; the clock.
    fn any_term(scope: &Scope) -> Term {
        match kani::any::<u8>() % 5 {
            0 => {
                let (at, e) = any_at(scope);
                Term::Slot {
                    var: any_var_of(e),
                    at,
                }
            }
            1 => Term::Lit(Value(kani::any())),
            2 => Term::Int(i64::from(kani::any::<i8>())),
            3 => Term::Index(any_at(scope).0),
            _ => Term::Now,
        }
    }

    /// A comparison of any two terms.
    fn any_cmp(scope: &Scope) -> Pred {
        Pred::Cmp(any_term(scope), any_op(), any_term(scope))
    }

    /// A membership of any term in no term or one. Each list is a literal of its length: a
    /// length CBMC cannot fold to a constant has it unwind every walk over the list.
    fn any_in(scope: &Scope, one: bool) -> Pred {
        let values = if one { vec![any_term(scope)] } else { vec![] };
        Pred::In(any_term(scope), values)
    }

    /// A set of the system's vars, one flag per var. `pred_reads` writes into it in place of a
    /// `VarSet`, whose B-tree CBMC would have to unwind.
    #[derive(Clone, Copy, Default)]
    struct Mask([bool; 6]);

    impl Extend<VarId> for Mask {
        fn extend<I: IntoIterator<Item = VarId>>(&mut self, vars: I) {
            for v in vars {
                self.0[v.index()] = true;
            }
        }
    }

    impl Mask {
        /// Whether the var of state byte `i` is in the set.
        fn holds_byte(&self, i: usize) -> bool {
            self.0[BYTE_VAR[i]]
        }
    }

    /// Calls `f` with each state byte, unrolled: a loop over the bytes would set the unwind
    /// bound of every harness past the width, and CBMC would unwind every loop of the code
    /// under proof that far.
    fn each_byte(mut f: impl FnMut(usize)) {
        f(0);
        f(1);
        f(2);
        f(3);
        f(4);
        f(5);
        f(6);
        f(7);
        f(8);
        f(9);
        f(10);
    }

    /// Whether two states differ in some byte.
    fn differ(a: &[u8], b: &[u8]) -> bool {
        let mut differ = false;
        each_byte(|i| differ |= a[i] != b[i]);
        differ
    }

    /// A state that agrees with `s` on the vars of `keep` and is anything elsewhere.
    fn agreeing(s: &[u8; WIDTH], keep: &Mask) -> [u8; WIDTH] {
        let mut out: [u8; WIDTH] = kani::any();
        each_byte(|i| {
            if keep.holds_byte(i) {
                out[i] = s[i];
            }
        });
        out
    }

    /// Read locality of a predicate: two states, and two successors, that agree on the vars
    /// `pred_reads` returns give the predicate `make` builds the same value under any binding.
    /// The predicate has a param of each entity and sits under one binder of `E`, as an
    /// invariant's body does (`find_violation`), so a quantifier's body reads two binders.
    ///
    /// `make` builds one shape with symbolic leaves, over entities it names, with at most one
    /// box or list above the leaves. CBMC unwinds to the bound every walk over a list whose
    /// length it cannot fold to a constant: a symbolic shape's parts, a symbolic entity's
    /// slots, a box inside a box. The system and the predicate are forgotten, not dropped:
    /// their drop glue recurses without a bound CBMC can see.
    fn reads_only_its_read_set(make: impl Fn(&mut Scope) -> Pred) {
        let ts = system();
        let params = [E, P];
        let mut scope = Scope {
            params: &params,
            bound: vec![E],
        };
        let pred = make(&mut scope);
        let mut keep = Mask::default();
        ts.pred_reads(&pred, &mut keep);
        let (s, n): ([u8; WIDTH], [u8; WIDTH]) = (kani::any(), kani::any());
        let (s2, n2) = (agreeing(&s, &keep), agreeing(&n, &keep));
        let binding = any_binding();
        let outer = any_slot();
        let eval = |s: &[u8; WIDTH], n: &[u8; WIDTH]| {
            let mut env = Env {
                next: Some(StateSlice::new(n)),
                bound: vec![outer],
                ..Env::new(&binding)
            };
            ts.eval(&pred, StateSlice::new(s), &mut env)
        };
        let (a, b) = (eval(&s, &n), eval(&s2, &n2));
        kani::cover!(differ(&s, &s2) && a);
        kani::cover!(differ(&s, &s2) && !a);
        assert_eq!(a, b);
        std::mem::forget((pred, ts));
    }

    /// The body of a quantifier over `entity`: `body` with it as the next binder.
    fn quantified(
        scope: &mut Scope,
        entity: EntId,
        body: impl Fn(&mut Scope) -> Pred,
    ) -> (EntId, Box<Pred>) {
        scope.bound.push(entity);
        let body = Box::new(body(scope));
        scope.bound.pop();
        (entity, body)
    }

    /// The atoms: `True`, `False`, a comparison, and a membership in no term or one. A term is
    /// any of a slot of a param, of its successor row or of the binder, a literal, an integer,
    /// a slot index, the clock.
    #[kani::proof]
    #[kani::unwind(2)]
    fn an_atom_reads_only_its_read_set() {
        reads_only_its_read_set(|_| Pred::True);
        reads_only_its_read_set(|_| Pred::False);
        reads_only_its_read_set(|s| any_cmp(s));
        reads_only_its_read_set(|s| any_in(s, false));
        reads_only_its_read_set(|s| any_in(s, true));
    }

    /// The connectives over a comparison: `Not`, and `And` and `Or` of none or one. A list of
    /// two predicates needs an unwind bound of 3, at which CBMC does not finish in minutes.
    #[kani::proof]
    #[kani::unwind(2)]
    fn a_connective_reads_only_its_read_set() {
        reads_only_its_read_set(|s| Pred::Not(Box::new(any_cmp(s))));
        reads_only_its_read_set(|s| Pred::And(vec![any_cmp(s)]));
        reads_only_its_read_set(|s| Pred::Or(vec![any_cmp(s)]));
        reads_only_its_read_set(|_| Pred::And(Vec::new()));
        reads_only_its_read_set(|_| Pred::Or(Vec::new()));
    }

    /// The quantifiers over a comparison that may read its binder and the outer one: `Forall`
    /// over `E`, `Exists` over `P`, `Count` over `E` with any operator and bound.
    #[kani::proof]
    #[kani::unwind(3)]
    fn a_quantifier_reads_only_its_read_set() {
        reads_only_its_read_set(|s| {
            let (entity, body) = quantified(s, E, |s| any_cmp(s));
            Pred::Forall { entity, body }
        });
        reads_only_its_read_set(|s| {
            let (entity, body) = quantified(s, P, |s| any_cmp(s));
            Pred::Exists { entity, body }
        });
        reads_only_its_read_set(|s| {
            let (entity, body) = quantified(s, E, |s| any_cmp(s));
            Pred::Count {
                entity,
                body,
                op: any_op(),
                n: kani::any::<u8>() % 3,
            }
        });
    }
}
