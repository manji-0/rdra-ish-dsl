//! Reduction of a cone by an independent sub-lifecycle (#74). A cone's vars split into a
//! target side T, which holds what the cone's verdicts read and every var an action writes
//! when it writes or reads T, and the rest S, written only by actions that read S and T vars
//! that only rise (#112, the inputs of S); an entity's vars share a side. T reads S through
//! the vars `obs`. S with its inputs is a transition system of its own, the extended
//! sub-lifecycle (`restrict_to` stubs a parent of its entities that is in T), in which the S
//! actions are silent and a T action that writes an input is a rise, kept with the guards it
//! has on S and the inputs. Its states are quotiented by the coarsest partition that is
//! divergence-blind stuttering stable under the silent edges with respect to `obs` and the
//! inputs, and strongly stable under the rises: two states are equivalent when they agree on
//! `obs` and the inputs, every class a silent path reaches from one is reached from the other,
//! and both rise into the same classes. The reduced cone keeps one representative per class in
//! the state, runs T-actions on it as they are, moves the class along a rise when a T step
//! changes the inputs, and replaces the S-actions by class moves: from a class, by the last
//! action of a path that leaves it, to the class the path enters. The reduced cone is
//! stuttering equivalent to the cone with respect to T and `obs`, so it reaches the same
//! T-side states, the same findings on T's entities, and the same verdicts of the checks that
//! read T and `obs`; a check that reads more of S is dropped and decided on its own cone
//! (docs/formal-verification.md, "Reduction by an independent sub-lifecycle").
//!
//! A reduced trace is lifted back to a real one: a class move becomes the path of S-actions
//! that realizes it from the concrete S state the trace holds, so a printed counterexample can
//! be longer than the reduced one and name another use case on an S step. S is explored with
//! the same reduction applied to it, so a reduction nests.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use super::cone::{Cone, Seed, VarSet};
use super::explore::Caps;
use super::expr::{State, StateSlice};
use super::types::{ActId, EntId, Pred, Value, VarId};
use super::TransitionSystem;

mod cache;
mod extended;
mod fold;
mod lift;
mod partition;
#[cfg(feature = "shadow")]
mod reference;
mod split;

pub use cache::QuotientCache;
use extended::Extended;
pub use fold::{Fold, FoldKey};
pub use partition::Divergence;
use partition::{NoQuotient, Quotient};
use split::Split;

/// The most cells (`Reduction::tla_cells`) a reduced cone may print for TLC. Measured with
/// TLC's 1 GB heap (#112): the insurance-claim-ops cones of 2,428 classes print 72,840 cells
/// and `verify` runs them; the `ClaimCase` view's of 4,368 classes prints 104,832 (a 1.6 MB
/// module) and TLC run by hand on it checks 52,687 states in 21 s at a peak of 912 MB; a
/// random model's cone of 24,000 classes printed 326,400 and ran out of heap while the
/// module was processed. A cone past the budget is explored through the split without
/// inputs, or unreduced.
const TLA_CELL_BUDGET: usize = 200_000;

/// Why one attempt at `reduced` left the cone unreduced.
enum Unreduced {
    /// The printed tables would pass `TLA_CELL_BUDGET` under a split that had inputs, so the
    /// split without them is worth a try.
    OverBudgetWithInputs,
    /// Nothing reduces.
    Nothing,
}

/// A step of a path: the action, its params, and the state it enters.
pub type Step = (ActId, Box<[u8]>, State);
/// A class move: the last action of a path that leaves the class, its params, and the class
/// it enters.
pub type ClassMove = (ActId, Box<[u8]>, u32);

/// The buffers a reduced step reuses from state to state (`moves_of_into`): the key of the
/// state's class, and its input bytes before and after a step.
#[derive(Default)]
pub(crate) struct StepBuffers {
    key: Vec<u8>,
    before: Vec<u8>,
    after: Vec<u8>,
}

/// A cone's S side quotiented, in the cone system's ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction {
    /// S, ascending.
    vars: Vec<VarId>,
    /// The inputs of S: T vars the S actions read, which only rise.
    inputs: Vec<VarId>,
    /// S ∪ inputs, ascending: the vars of the extended sub-lifecycle in its layout.
    all: Vec<VarId>,
    /// `(offset, width, in S)` of each var of `all` in the state.
    spans: Vec<(usize, usize, bool)>,
    /// Per class and the input bytes a T step leaves, the class the representative is in
    /// under those inputs.
    rises: BTreeMap<(u32, Box<[u8]>), u32>,
    /// The input bits: per group of S actions whose reads of T fold into one bit, the folds.
    /// A bit is a function of the T state (`push_bits`); in the extended sub-lifecycle it is
    /// a var that flips freely.
    bits: Vec<Vec<Fold>>,
    /// Per S action the extended sub-lifecycle keeps, the cone's actions it stands for: the
    /// variants of one use case step that differ only in what they read of T.
    variants: HashMap<ActId, Vec<ActId>>,
    /// The extended sub-lifecycle's bytes after its vars: the stubs `restrict_to` adds for
    /// parents outside it, which no action there writes.
    tail: Box<[u8]>,
    s_actions: BTreeSet<ActId>,
    skip: Vec<bool>,
    /// Per action of the cone: it writes no raw input and no var a bit reads, so no step of
    /// it changes the inputs and the class stays where it is.
    steady: Vec<bool>,
    s_entity: Vec<bool>,
    /// S vars the T-side actions read.
    obs: VarSet,
    /// The representative of each class: its bytes as the extended sub-lifecycle's state.
    reps: Vec<Box<[u8]>>,
    class_of: HashMap<Box<[u8]>, u32>,
    /// Each initial S state's class.
    init: HashMap<Box<[u8]>, u32>,
    /// Per class, the class moves: the last action, its params, and the class entered.
    moves: Vec<Vec<ClassMove>>,
    /// The reduction the extended sub-lifecycle was explored with, in its ids.
    inner: Option<Arc<Reduction>>,
    /// Extended S states explored.
    pub states: usize,
    /// Per class, whether a behavior can step inside it forever (`can_loop`).
    loops: Vec<bool>,
    /// The divergence marks, counted.
    pub divergence: Divergence,
}

impl Reduction {
    pub fn classes(&self) -> usize {
        self.reps.len()
    }

    /// Whether a behavior of the cone may step inside `class` forever while T stands still
    /// (docs/formal-verification.md, "WF_vars(Next) on a reduced cone"). An over-approximation:
    /// a class this marks may hold no such loop among the states the cone reaches.
    pub fn can_loop(&self, class: u32) -> bool {
        self.loops[class as usize]
    }

    /// Whether this reduction, or one nested in it, has fewer classes than states. A quotient
    /// that is the identity at every level reduces nothing (#101), whatever its depth: an
    /// identity inner reduction does not make the outer one worthwhile.
    fn shrinks(&self) -> bool {
        self.reps.len() < self.states || self.inner.as_ref().is_some_and(|i| i.shrinks())
    }

    /// The cells the TLA+ printer writes for this reduction: one per class for each observed
    /// var and each S action, and one per rise. `TLA_CELL_BUDGET` bounds it.
    pub fn tla_cells(&self) -> usize {
        self.reps.len() * (self.obs.len() + self.s_actions.len()) + self.rises.len()
    }

    /// The S vars, ascending.
    pub fn vars(&self) -> &[VarId] {
        &self.vars
    }

    /// The raw inputs of S, ascending: T vars its actions read, each of which only rises.
    pub fn inputs(&self) -> &[VarId] {
        &self.inputs
    }

    /// Whether no step of `act` changes the inputs: it writes no raw input and no var a bit
    /// reads. Such a step leaves the class alone, so neither engine recomputes it.
    pub fn is_steady(&self, act: ActId) -> bool {
        self.steady[act.index()]
    }

    /// The input bits: per group, the folds of the S actions that read it.
    pub fn bits(&self) -> &[Vec<Fold>] {
        &self.bits
    }

    /// Per bit group, the slots of its key's entity in `ts`, the cone: one bit per slot.
    pub fn bit_slots(&self, ts: &TransitionSystem) -> Vec<usize> {
        self.bits
            .iter()
            .map(|group| ts.slots(group[0].key_entity(ts)))
            .collect()
    }

    /// The vars of the extended sub-lifecycle in its layout: S and the inputs, ascending; a
    /// var of a nested reduction indexes them.
    pub fn sub_vars(&self) -> &[VarId] {
        &self.all
    }

    /// The reduction the extended sub-lifecycle was explored with, in its ids.
    pub fn inner(&self) -> Option<&Reduction> {
        self.inner.as_deref()
    }

    pub fn obs(&self) -> &VarSet {
        &self.obs
    }

    pub fn s_actions(&self) -> &BTreeSet<ActId> {
        &self.s_actions
    }

    pub(crate) fn skips(&self, act: ActId) -> bool {
        self.skip[act.index()]
    }

    /// The entity's existence is in S: its view is not this cone's to report.
    pub(crate) fn is_s_entity(&self, e: EntId) -> bool {
        self.s_entity[e.index()]
    }

    /// The S and input bytes of a state, as the extended sub-lifecycle's state: its vars in
    /// their layout, the stubs, then the input bits.
    pub(crate) fn key(&self, ts: &TransitionSystem, s: &StateSlice) -> Vec<u8> {
        let mut key = Vec::new();
        self.key_into(ts, s, &mut key);
        key
    }

    /// `key` written into `out`, which keeps its allocation.
    fn key_into(&self, ts: &TransitionSystem, s: &StateSlice, out: &mut Vec<u8>) {
        out.clear();
        for &(at, w, _) in &self.spans {
            out.extend_from_slice(&s.0[at..at + w]);
        }
        out.extend_from_slice(&self.tail);
        self.push_bits(ts, s, out);
    }

    /// The input bytes of a state, in the extended sub-lifecycle's order: the raw inputs,
    /// then the bits.
    pub(crate) fn input_bytes(&self, ts: &TransitionSystem, s: &StateSlice) -> Vec<u8> {
        let mut out = Vec::new();
        self.input_bytes_into(ts, s, &mut out);
        out
    }

    /// `input_bytes` written into `out`, which keeps its allocation.
    pub(crate) fn input_bytes_into(
        &self,
        ts: &TransitionSystem,
        s: &StateSlice,
        out: &mut Vec<u8>,
    ) {
        out.clear();
        for &(at, w, in_s) in &self.spans {
            if !in_s {
                out.extend_from_slice(&s.0[at..at + w]);
            }
        }
        self.push_bits(ts, s, out);
    }

    /// Appends the input bits of a state to `out`: per bit group and slot of its key, whether
    /// some fold of the group holds there.
    fn push_bits(&self, ts: &TransitionSystem, s: &StateSlice, out: &mut Vec<u8>) {
        for group in &self.bits {
            for c in 0..ts.slots(group[0].key_entity(ts)) as u8 {
                out.push(u8::from(group.iter().any(|f| f.holds(ts, s, c))));
            }
        }
    }

    /// Writes the S part of `bytes`, an extended sub-lifecycle state, into `s`.
    fn write(&self, s: &mut State, bytes: &[u8]) {
        let mut i = 0;
        for &(at, w, in_s) in &self.spans {
            if in_s {
                s.0[at..at + w].copy_from_slice(&bytes[i..i + w]);
            }
            i += w;
        }
    }

    /// The class of a state whose S part is a representative.
    pub fn class(&self, ts: &TransitionSystem, s: &StateSlice) -> Option<u32> {
        self.class_into(ts, s, &mut Vec::new())
    }

    /// `class`, with the key built in `key`, which keeps its allocation.
    fn class_into(&self, ts: &TransitionSystem, s: &StateSlice, key: &mut Vec<u8>) -> Option<u32> {
        self.key_into(ts, s, key);
        self.class_of.get(key.as_slice()).copied()
    }

    /// The representative's bytes, as the extended sub-lifecycle's state.
    pub fn rep(&self, class: u32) -> &[u8] {
        &self.reps[class as usize]
    }

    /// The value of a slot of a var of the extended sub-lifecycle in a class's
    /// representative; `None` for a var outside it.
    pub fn rep_value(&self, class: u32, var: VarId, slot: usize) -> Option<Value> {
        let k = self.all.iter().position(|v| *v == var)?;
        let at: usize = self.spans[..k].iter().map(|(_, w, _)| w).sum();
        Some(Value(self.reps[class as usize][at + slot]))
    }

    /// The class a T step moves `class` to when it leaves the inputs at `inputs`.
    pub fn rise(&self, class: u32, inputs: &[u8]) -> Option<u32> {
        self.rises.get(&(class, inputs.into())).copied()
    }

    /// The stages of a T step's change of the inputs from `before` to `after`: first the raw
    /// inputs, then each bit slot that flips, one at a time, each stage the input bytes after
    /// it. The extended sub-lifecycle changes the inputs by one action at a time (a restricted
    /// T action writes the raw inputs, a flip one bit slot), so a T step that changes several
    /// moves the class by one rise per stage, in this order in both engines.
    pub fn stages(&self, before: &[u8], after: &[u8]) -> Vec<Vec<u8>> {
        let raw: usize = self
            .spans
            .iter()
            .filter(|(_, _, in_s)| !in_s)
            .map(|(_, w, _)| w)
            .sum();
        let mut stages = Vec::new();
        let mut cur = before.to_vec();
        if cur[..raw] != after[..raw] {
            cur[..raw].copy_from_slice(&after[..raw]);
            stages.push(cur.clone());
        }
        for k in raw..after.len() {
            if cur[k] != after[k] {
                cur[k] = after[k];
                stages.push(cur.clone());
            }
        }
        stages
    }

    /// The class a T step moves `class` to when it changes the inputs from `before` to
    /// `after`, stage by stage.
    pub fn recanon(&self, class: u32, before: &[u8], after: &[u8]) -> Option<u32> {
        let mut c = class;
        for stage in self.stages(before, after) {
            c = self.rise(c, &stage)?;
        }
        Some(c)
    }

    /// The rises from a class, sorted: the input bytes a T step leaves and the class they put
    /// the representative in.
    pub fn rises_from(&self, class: u32) -> Vec<(&[u8], u32)> {
        self.rises
            .range((class, Box::default())..)
            .take_while(|((c, _), _)| *c == class)
            .map(|((_, k), to)| (k.as_ref(), *to))
            .collect()
    }

    /// Whether the var is in S.
    pub fn is_s_var(&self, var: VarId) -> bool {
        self.vars.contains(&var)
    }

    /// Classes of the initial S states, ascending.
    pub fn initial_classes(&self) -> Vec<u32> {
        let set: BTreeSet<u32> = self.init.values().copied().collect();
        set.into_iter().collect()
    }

    /// Each class's moves, by class.
    pub fn class_moves(&self) -> &[Vec<ClassMove>] {
        &self.moves
    }

    /// The state with its S part replaced by the representative of the class it starts in;
    /// `None` when the S part is no initial S state.
    pub(crate) fn canonical_initial(&self, ts: &TransitionSystem, s: &StateSlice) -> Option<State> {
        let class = *self.init.get(self.key(ts, s).as_slice())?;
        let mut out = s.to_owned();
        self.write(&mut out, &self.reps[class as usize]);
        Some(out)
    }

    /// Whether a class move's `params` bind the cone action `act` as it is: the extended
    /// sub-lifecycle kept its params and did not fold it.
    fn in_cone_params(&self, ts: &TransitionSystem, act: ActId, params: &[u8]) -> bool {
        !self.variants.contains_key(&act) && ts.actions[act.index()].params.len() == params.len()
    }

    /// Whether cone action `a2` realizes the extended sub-lifecycle's step of `act`.
    fn stands_for(&self, act: ActId, a2: ActId) -> bool {
        a2 == act || self.variants.get(&act).is_some_and(|v| v.contains(&a2))
    }
}

impl TransitionSystem {
    pub fn reduction(&self) -> Option<&Reduction> {
        self.reduction.as_deref()
    }

    /// This cone system with its S side reduced, when it has one, its S side fits under
    /// `caps`, and the tables TLC would get fit under `TLA_CELL_BUDGET`; otherwise unchanged.
    /// `seeds` are what each target that shares the cone reads and keeps.
    pub fn reduced(&self, seeds: &[Seed], caps: Caps) -> TransitionSystem {
        self.reduced_in(seeds, caps, &QuotientCache::new()).0
    }

    /// `reduced`, with the quotients `cache` holds; the flag says whether no cap and no limit
    /// stopped a run of the reduction, nested ones included.
    pub(super) fn reduced_in(
        &self,
        seeds: &[Seed],
        caps: Caps,
        cache: &QuotientCache,
    ) -> (TransitionSystem, bool) {
        let mut clean = true;
        let out = match self.reduced_with(seeds, caps, true, cache, &mut clean) {
            Ok(out) => out,
            Err(Unreduced::OverBudgetWithInputs) => self
                .reduced_with(seeds, caps, false, cache, &mut clean)
                .unwrap_or_else(|_| self.clone()),
            Err(Unreduced::Nothing) => self.clone(),
        };
        (out, clean)
    }

    /// One attempt at `reduced`, with or without the inputs of S. `clean` turns false when a
    /// cap or a limit stopped a run of the quotient.
    fn reduced_with(
        &self,
        seeds: &[Seed],
        caps: Caps,
        inputs: bool,
        cache: &QuotientCache,
        clean: &mut bool,
    ) -> Result<TransitionSystem, Unreduced> {
        let split = self.split_with(seeds, inputs);
        // Every S var observed by T makes the quotient the identity, whatever the inputs do.
        if split.s.is_empty() || split.s.is_subset(&split.obs) {
            return Err(Unreduced::Nothing);
        }
        let ext = Extended::of(self, &split);
        // Past the budget, the split without inputs is worth a try when this one had any.
        let over_budget = || match split.inputs.is_empty() && split.folds.is_empty() {
            true => Unreduced::Nothing,
            false => Unreduced::OverBudgetWithInputs,
        };
        let (quotient, quotient_clean) = Quotient::of(&ext, self, caps, cache);
        *clean &= quotient_clean;
        let Quotient { classes, moves } = match quotient {
            Ok(quotient) => quotient,
            Err(NoQuotient::Nothing) => return Err(Unreduced::Nothing),
            Err(NoQuotient::TooLarge) => return Err(over_budget()),
        };
        let n = classes.states;
        let (reps, class_of, init) = (
            classes.reps.clone(),
            classes.class_of.clone(),
            classes.init.clone(),
        );
        let (rises, inner, loops, divergence) = (
            classes.rises.clone(),
            classes.inner.clone(),
            classes.loops.clone(),
            classes.divergence,
        );
        let all: Vec<VarId> = split.s.union(&split.inputs).copied().collect();
        let spans: Vec<(usize, usize, bool)> = all
            .iter()
            .map(|&v| {
                let width = self.slots_of(&self.var(v).kind);
                (self.offset(v), width, split.s.contains(&v))
            })
            .collect();
        let width: usize = spans.iter().map(|(_, w, _)| w).sum();
        // After S and the raw inputs come the stubs, then the bit vars, whose values a
        // state's `key` computes.
        let first = &ext.system.initial_states()[0].0;
        let bits_width: usize = ext
            .bits
            .iter()
            .map(|group| self.slots(group[0].key_entity(self)))
            .sum();
        let tail = first[width..first.len() - bits_width].into();
        let mut skip = vec![false; self.actions.len()];
        for a in &split.s_actions {
            skip[a.index()] = true;
        }
        let s_entity = self
            .entities
            .iter()
            .map(|e| split.s.contains(&e.exists))
            .collect();
        let mut input_reads: VarSet = split.inputs.iter().copied().collect();
        for fold in ext.bits.iter().flatten() {
            input_reads.extend(fold.reads(self));
        }
        let steady = self
            .actions
            .iter()
            .map(|a| self.action_writes(a).is_disjoint(&input_reads))
            .collect();
        let reduction = Reduction {
            vars: split.s.iter().copied().collect(),
            inputs: split.inputs.iter().copied().collect(),
            all,
            spans,
            rises,
            bits: ext.bits,
            variants: ext.variants,
            tail,
            s_actions: split.s_actions.clone(),
            skip,
            steady,
            s_entity,
            obs: split.obs.clone(),
            reps,
            class_of,
            init,
            moves,
            inner,
            states: n,
            loops,
            divergence,
        };
        if reduction.tla_cells() > TLA_CELL_BUDGET {
            return Err(over_budget());
        }
        let mut out = self.clone();
        out.reduction = Some(Arc::new(reduction));
        out.drop_unreadable_checks(&split);
        Ok(out)
    }

    /// Drops the checks the reduced cone cannot evaluate: an invariant or after-assert that
    /// reads S outside `obs`, an after-assert on an S action, and a property whose fairness
    /// names an S action. Each has a cone of its own that decides it.
    fn drop_unreadable_checks(&mut self, split: &Split) {
        let readable: VarSet = split.t.union(&split.obs).copied().collect();
        let reads_ok = |ts: &TransitionSystem, binders: &[EntId], preds: &[&Pred]| {
            ts.check_reads(binders, preds.iter().copied())
                .is_subset(&readable)
        };
        let invariants = std::mem::take(&mut self.invariants);
        self.invariants = invariants
            .into_iter()
            .filter(|inv| reads_ok(self, &inv.binders, &[&inv.body]))
            .collect();
        let after = std::mem::take(&mut self.after);
        self.after = after
            .into_iter()
            .filter(|a| {
                a.steps.iter().all(|step| {
                    split.t_actions.contains(&step.act)
                        && reads_ok(self, &step.binders, &[&step.post])
                })
            })
            .collect();
        // A reduction keeps stuttering equivalence, which says nothing of `AG EF`, so a reduced
        // system decides no `can_reach` (`explore_targets_with` reduces no cone of one).
        self.reach.clear();
        let properties = std::mem::take(&mut self.properties);
        self.properties = properties
            .into_iter()
            .filter(|p| {
                self.property_reads(p).is_subset(&readable)
                    && self
                        .actions
                        .iter()
                        .enumerate()
                        .all(|(i, a)| !p.is_fair(a) || split.t_actions.contains(&ActId::at(i)))
            })
            .collect();
    }

    /// The cone as a system, reduced under `caps` by what its targets read and keep: the
    /// seeds and keeps of every target that shares the cone, united. With `RDRA_ISH_NO_REDUCE`
    /// set the cone stays unreduced, for the differential test `scripts/reduction-diff.py`,
    /// which runs `verify` both ways and compares the verdicts.
    pub fn reduced_cone(
        &self,
        cone: &Cone,
        seeds: impl IntoIterator<Item = Seed>,
        caps: Caps,
    ) -> TransitionSystem {
        self.reduced_cone_in(cone, seeds, caps, &QuotientCache::new())
    }

    /// `reduced_cone`, sharing quotients through `cache` with the other cones reduced in it.
    pub fn reduced_cone_in(
        &self,
        cone: &Cone,
        seeds: impl IntoIterator<Item = Seed>,
        caps: Caps,
        cache: &QuotientCache,
    ) -> TransitionSystem {
        let (sys, local) = self.cone_system(cone, seeds);
        if std::env::var_os("RDRA_ISH_NO_REDUCE").is_some() {
            return sys;
        }
        sys.reduced_in(&local, caps, cache).0
    }

    /// The cone as a system, and the seeds in its ids.
    pub(super) fn cone_system(
        &self,
        cone: &Cone,
        seeds: impl IntoIterator<Item = Seed>,
    ) -> (TransitionSystem, Vec<Seed>) {
        let sys = self.restrict_to(&cone.vars, &cone.keep);
        let local: Vec<Seed> = seeds
            .into_iter()
            .map(|(seed, keep)| {
                let seed_local: VarSet = seed
                    .iter()
                    .filter_map(|v| cone.vars.iter().position(|x| x == v))
                    .map(VarId::at)
                    .collect();
                let keep_local: BTreeSet<ActId> = sys
                    .action_ids()
                    .filter(|a| {
                        let source = sys.actions[a.index()].source;
                        keep.iter()
                            .any(|k| self.actions[k.index()].source == source)
                    })
                    .collect();
                (seed_local, keep_local)
            })
            .collect();
        (sys, local)
    }

    /// The moves of `s` without the reduction: the real step relation on a reduced state.
    pub(crate) fn plain_moves(&self, s: &StateSlice) -> Vec<Step> {
        let mut out = super::explore::Moves::default();
        self.moves_of_into(s, false, &mut s.to_owned(), &mut out);
        out.into_moves()
            .into_iter()
            .map(|m| (m.act, m.params, m.next))
            .collect()
    }

    /// The moves of `s`, with the reduction applied when the system has one and `reduced`: the
    /// S actions are left out, the class moves come last, and a T step that sets an input of
    /// S moves the representative to the class the inputs put it in. The class of `s` is
    /// looked up once, when a step or the class moves first need it, in buffers `out` keeps.
    pub(crate) fn moves_of_into(
        &self,
        s: &StateSlice,
        reduced: bool,
        scratch: &mut State,
        out: &mut super::explore::Moves,
    ) {
        let r = self.reduction().filter(|_| reduced);
        let mut buffers = std::mem::take(&mut out.reduced);
        let StepBuffers { key, before, after } = &mut buffers;
        let rises = r.filter(|r| !r.inputs.is_empty() || !r.bits.is_empty());
        if let Some(r) = rises {
            r.input_bytes_into(self, s, before);
        }
        let mut class: Option<u32> = None;
        let class_of = |r: &Reduction, key: &mut Vec<u8>| {
            r.class_into(self, s, key)
                .expect("a reduced state holds a representative")
        };
        self.each_raw_move(
            s,
            |act| r.is_some_and(|r| r.skips(act)),
            scratch,
            |act, params, next| {
                // A steady action's step leaves the inputs as they were, so the class too.
                crate::shadow_assert!(rises
                    .is_none_or(|r| !r.is_steady(act) || r.input_bytes(self, next) == *before));
                if let Some(r) = rises.filter(|r| !r.is_steady(act)) {
                    r.input_bytes_into(self, next, after);
                    if after != before {
                        let class = *class.get_or_insert_with(|| class_of(r, key));
                        let to = r.recanon(class, before, after).unwrap_or_else(|| {
                            panic!(
                                "a T step's rise is a rise of the extended sub-lifecycle: {} from class {class} inputs {before:?} to {after:?}, known {:?}",
                                self.actions[act.index()].name,
                                r.rises_from(class)
                            )
                        });
                        r.write(next, &r.reps[to as usize]);
                    }
                }
                out.push(act, params, next);
            },
        );
        if let Some(r) = r {
            #[cfg(feature = "shadow")]
            let from = out.len();
            let class = class.unwrap_or_else(|| class_of(r, key));
            for (act, params, to) in &r.moves[class as usize] {
                scratch.0.copy_from_slice(&s.0);
                r.write(scratch, &r.reps[*to as usize]);
                out.push(*act, params, scratch);
            }
            #[cfg(feature = "shadow")]
            reference::check_class_moves(r, self, s, out, from);
        }
        out.reduced = buffers;
    }
}
