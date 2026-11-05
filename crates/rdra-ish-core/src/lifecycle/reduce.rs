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

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use super::cone::{Cone, Seed, VarSet};
use super::explore::Caps;
use super::expr::State;
use super::types::{ActId, ActionOrigin, Effect, EntId, Rhs, Value, VarId, VarKind};
use super::TransitionSystem;

/// The most cells (`Reduction::tla_cells`) a reduced cone may print for TLC. Measured with
/// TLC's 1 GB heap (#112): the insurance-claim-ops cones of 2,428 classes print 72,840 cells
/// and `verify` runs them; the `ClaimCase` view's of 4,368 classes prints 104,832 (a 1.6 MB
/// module) and TLC run by hand on it checks 52,687 states in 21 s at a peak of 912 MB; a
/// random model's cone of 24,000 classes printed 326,400 and ran out of heap while the
/// module was processed. A cone past the budget is explored through the split without
/// inputs, or unreduced.
const TLA_CELL_BUDGET: usize = 200_000;

/// A step of a path: the action, its params, and the state it enters.
pub type Step = (ActId, Box<[u8]>, State);
/// A class move: the last action of a path that leaves the class, its params, and the class
/// it enters.
pub type ClassMove = (ActId, Box<[u8]>, u32);

/// A cone's S side quotiented, in the cone system's ids.
#[derive(Clone, Debug)]
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
    rises: HashMap<(u32, Box<[u8]>), u32>,
    /// The extended sub-lifecycle's bytes after its vars: the stubs `restrict_to` adds for
    /// parents outside it, which no action there writes.
    tail: Box<[u8]>,
    s_actions: BTreeSet<ActId>,
    skip: Vec<bool>,
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
}

impl Reduction {
    pub fn classes(&self) -> usize {
        self.reps.len()
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

    /// The inputs of S, ascending: T vars its actions read, each of which only rises.
    pub fn inputs(&self) -> &[VarId] {
        &self.inputs
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
    /// their layout, then the stubs.
    pub(crate) fn key(&self, s: &State) -> Vec<u8> {
        let mut key = Vec::new();
        for &(at, w, _) in &self.spans {
            key.extend_from_slice(&s.0[at..at + w]);
        }
        key.extend_from_slice(&self.tail);
        key
    }

    /// The input bytes of a state, in the extended sub-lifecycle's order.
    pub(crate) fn input_bytes(&self, s: &State) -> Vec<u8> {
        let mut out = Vec::new();
        for &(at, w, in_s) in &self.spans {
            if !in_s {
                out.extend_from_slice(&s.0[at..at + w]);
            }
        }
        out
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
    pub fn class(&self, s: &State) -> Option<u32> {
        self.class_of.get(self.key(s).as_slice()).copied()
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

    /// The stages of a T step's change of the inputs from `before` to `after`, each the
    /// input bytes after it. One stage: a T action restricted to the extended sub-lifecycle
    /// writes every input it writes at once.
    pub fn stages(&self, before: &[u8], after: &[u8]) -> Vec<Vec<u8>> {
        if before == after {
            Vec::new()
        } else {
            vec![after.to_vec()]
        }
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
        let mut out: Vec<(&[u8], u32)> = self
            .rises
            .iter()
            .filter(|((c, _), _)| *c == class)
            .map(|((_, k), to)| (k.as_ref(), *to))
            .collect();
        out.sort_unstable();
        out
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
    pub(crate) fn canonical_initial(&self, s: &State) -> Option<State> {
        let class = *self.init.get(self.key(s).as_slice())?;
        let mut out = s.clone();
        self.write(&mut out, &self.reps[class as usize]);
        Some(out)
    }

    /// The class moves from `s`: the last action, its params, and the successor.
    pub(crate) fn moves_from(&self, s: &State) -> Vec<Step> {
        let class = self
            .class(s)
            .expect("a reduced state holds a representative");
        self.moves[class as usize]
            .iter()
            .map(|(act, params, to)| {
                let mut s2 = s.clone();
                self.write(&mut s2, &self.reps[*to as usize]);
                (*act, params.clone(), s2)
            })
            .collect()
    }
}

/// The classes of S, with the initial states' and the moves' classes.
struct Quotient {
    reps: Vec<Box<[u8]>>,
    class_of: HashMap<Box<[u8]>, u32>,
    init: HashMap<Box<[u8]>, u32>,
    moves: Vec<Vec<ClassMove>>,
    rises: HashMap<(u32, Box<[u8]>), u32>,
    inner: Option<Arc<Reduction>>,
    states: usize,
}

/// The extended sub-lifecycle of a cone (`restrict_to(S ∪ inputs)`), with its actions sorted
/// into the S actions of the cone, whose steps are silent, and the rises: T actions that
/// write an input, kept with the guards they have on S and the inputs only.
struct Extended {
    system: TransitionSystem,
    /// Per action of `system`, the cone's action it comes from.
    acts: Vec<ActId>,
    /// Per action of `system`, whether it is a rise.
    rise: Vec<bool>,
    obs_local: VarSet,
    /// The inputs, in `system`'s ids.
    inputs_local: VarSet,
}

impl Extended {
    fn of(cone: &TransitionSystem, split: &Split) -> Self {
        Self::build(cone, &split.s, &split.inputs, &split.s_actions, &split.obs)
    }

    /// The extended sub-lifecycle of a reduced cone, as `of` built it.
    fn rebuild(cone: &TransitionSystem, r: &Reduction) -> Self {
        Self::build(
            cone,
            &r.vars.iter().copied().collect(),
            &r.inputs.iter().copied().collect(),
            &r.s_actions,
            &r.obs,
        )
    }

    fn build(
        cone: &TransitionSystem,
        s: &VarSet,
        inputs: &VarSet,
        s_actions: &BTreeSet<ActId>,
        obs: &VarSet,
    ) -> Self {
        let all: VarSet = s.union(inputs).copied().collect();
        let (system, _) = cone.restrict_to_mapped(&all, &BTreeSet::new(), false);
        let local = |v: &VarId| VarId::at(all.iter().position(|x| x == v).expect("in S ∪ inputs"));
        let acts: Vec<ActId> = system
            .actions
            .iter()
            .map(|a| {
                let at = cone
                    .actions
                    .iter()
                    .position(|b| b.source == a.source)
                    .expect("an action of the extended sub-lifecycle is an action of the cone");
                ActId::at(at)
            })
            .collect();
        let rise = acts.iter().map(|a| !s_actions.contains(a)).collect();
        Extended {
            system,
            acts,
            rise,
            obs_local: obs.iter().map(local).collect(),
            inputs_local: inputs.iter().map(local).collect(),
        }
    }

    /// The actions of `system` that are rises.
    fn rise_actions(&self) -> BTreeSet<ActId> {
        (0..self.rise.len())
            .filter(|i| self.rise[*i])
            .map(ActId::at)
            .collect()
    }
}

/// The partition of a run of the extended sub-lifecycle: the coarsest one that refines
/// `initial` (the states' `obs` and inputs), is divergence-blind stuttering stable under the
/// silent edges, and is strongly stable under the rise edges: two states of a block reach the
/// same blocks by a rise. The blocks refine the inputs, so the block a rise enters names the
/// inputs it set, and the second condition says that two equivalent states stay equivalent
/// under every rise. Returns each state's block.
fn quotient_blocks(
    n: usize,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
) -> Vec<u32> {
    let mut block = stuttering_blocks(n, silent, initial);
    if rises.is_empty() {
        return block;
    }
    loop {
        let mut signature: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); n];
        for &(x, y) in rises {
            signature[x as usize].insert(block[y as usize]);
        }
        let mut index: HashMap<(u32, &BTreeSet<u32>), u32> = HashMap::new();
        let refined: Vec<u32> = (0..n)
            .map(|i| {
                let len = index.len() as u32;
                *index.entry((block[i], &signature[i])).or_insert(len)
            })
            .collect();
        let before = block.iter().collect::<BTreeSet<_>>().len();
        if index.len() == before {
            return block;
        }
        block = stuttering_blocks(n, silent, &refined);
    }
}

impl Quotient {
    /// The quotient of the extended sub-lifecycle `ext` by stuttering equivalence with
    /// respect to `obs` and the inputs; `None` when it does not fit under `caps`, or when the
    /// quotient is the identity on an S that no inner reduction shrank, which would reduce
    /// nothing (#101). An unobserved S is one class, held at an initial S state, with no
    /// moves: nothing on T reads it, so no path through it is told from staying put, whatever
    /// its inputs do.
    fn of(ext: &Extended, caps: Caps) -> Option<Self> {
        let raw = &ext.system;
        if ext.obs_local.is_empty() {
            let initial = raw.initial_states();
            let rep = initial.first().expect("an initial state").0.clone();
            return Some(Quotient {
                class_of: HashMap::from([(rep.clone(), 0)]),
                reps: vec![rep],
                init: initial.iter().map(|st| (st.0.clone(), 0)).collect(),
                moves: vec![Vec::new()],
                rises: HashMap::new(),
                inner: None,
                states: initial.len(),
            });
        }
        // The inputs stay on the nested T side: the rises are kept, and only `obs` anchors.
        let sub = raw.reduced(&[(ext.obs_local.clone(), ext.rise_actions())], caps);
        let run = sub.clone().run(caps);
        if !run.complete {
            return None;
        }
        let n = run.states.len();
        let key_vars: VarSet = ext.obs_local.union(&ext.inputs_local).copied().collect();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = run.system.project(st, &key_vars);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let (silent, rise_edges) = split_edges(&run, &ext.rise);
        let blocks = quotient_blocks(n, &silent, &rise_edges, &initial);
        if !sub.reduction().is_some_and(|r| r.shrinks())
            && blocks.iter().collect::<BTreeSet<_>>().len() == n
        {
            return None;
        }
        let mut rep_of_block: HashMap<u32, u32> = HashMap::new();
        let mut reps: Vec<Box<[u8]>> = Vec::new();
        let mut class_of: HashMap<Box<[u8]>, u32> = HashMap::new();
        let mut class: Vec<u32> = Vec::with_capacity(n);
        for (i, st) in run.states.iter().enumerate() {
            let c = *rep_of_block.entry(blocks[i]).or_insert_with(|| {
                reps.push(st.0.clone());
                class_of.insert(st.0.clone(), reps.len() as u32 - 1);
                reps.len() as u32 - 1
            });
            class.push(c);
        }
        // Initial S states, as the cone's initial states project them, to their classes. The
        // extended sub-lifecycle canonicalizes them for its own reduction first, as the run
        // did.
        let mut init: HashMap<Box<[u8]>, u32> = HashMap::new();
        for st in raw.initial_states() {
            let canonical = match sub.reduction() {
                Some(inner) => inner
                    .canonical_initial(&st)
                    .expect("an initial S state canonicalizes"),
                None => st.clone(),
            };
            let at = run
                .states
                .iter()
                .position(|x| *x == canonical)
                .expect("an initial S state is reached");
            init.insert(st.0.clone(), class[at]);
        }
        let mut moves: Vec<Vec<ClassMove>> = vec![Vec::new(); reps.len()];
        let mut rises: HashMap<(u32, Box<[u8]>), u32> = HashMap::new();
        let mut seen: BTreeSet<(u32, ActId, Box<[u8]>, u32)> = BTreeSet::new();
        for e in run.edges() {
            let (x, y) = (e.from as usize, e.to as usize);
            let (act, params) = run.step(e);
            if ext.rise[act.index()] {
                // Every state of a block rises into the same blocks; the representative's
                // edges are enough.
                if reps[class[x] as usize] == run.states[x].0 {
                    let inputs = run.system.project(&run.states[y], &ext.inputs_local).0;
                    let was = rises.insert((class[x], inputs), class[y]);
                    assert!(was.is_none_or(|c| c == class[y]), "a rise is a function");
                }
                continue;
            }
            if class[x] == class[y] {
                continue;
            }
            let act = ext.acts[act.index()];
            if seen.insert((class[x], act, params.into(), class[y])) {
                moves[class[x] as usize].push((act, params.into(), class[y]));
            }
        }
        Some(Quotient {
            reps,
            class_of,
            init,
            moves,
            rises,
            inner: sub.reduction.clone(),
            states: n,
        })
    }
}

/// The split of a cone.
pub(super) struct Split {
    t: VarSet,
    pub(super) s: VarSet,
    t_actions: BTreeSet<ActId>,
    pub(super) s_actions: BTreeSet<ActId>,
    obs: VarSet,
    /// T vars the S actions read (#112), each monotone, with the existence of each one's
    /// entity: the inputs of S.
    pub(super) inputs: VarSet,
}

impl TransitionSystem {
    pub fn reduction(&self) -> Option<&Reduction> {
        self.reduction.as_deref()
    }

    /// Per var, whether no action lowers it (#112): the writes of the system, as edges
    /// between the var's values, form no cycle. A create takes an existence from absent to
    /// present and a delete from present to deleted; a delete resets the entity's columns
    /// and owners, which makes them cyclic; an assignment or a choice in the action that
    /// creates the row starts from base, elsewhere from any value; a transition is its arms;
    /// the clock ticks up. Such a var changes at most as often as its domain has values, so
    /// a sub-lifecycle that reads it as a free input sees finitely many input histories.
    pub(crate) fn monotone_vars(&self) -> Vec<bool> {
        let n = self.vars.len();
        let mut edges: Vec<BTreeSet<(u8, u8)>> = vec![BTreeSet::new(); n];
        let mut cyclic = vec![false; n];
        for a in &self.actions {
            let creates: BTreeSet<u8> = a
                .effects
                .iter()
                .filter_map(|e| match e {
                    Effect::Create { param } => Some(*param),
                    _ => None,
                })
                .collect();
            let entity = |p: &u8| self.entity(a.params[usize::from(*p)].entity);
            let froms = |var: VarId, param: &u8| -> Vec<u8> {
                let decl = self.var(var);
                if creates.contains(param) {
                    vec![decl.base.0]
                } else {
                    (0..decl.domain.size(self.bounds) as u8).collect()
                }
            };
            for eff in &a.effects {
                match eff {
                    Effect::Create { param } => {
                        edges[entity(param).exists.index()]
                            .insert((Value::ABSENT.0, Value::PRESENT.0));
                    }
                    Effect::Delete { param } => {
                        let d = entity(param);
                        edges[d.exists.index()].insert((Value::PRESENT.0, Value::DELETED.0));
                        for v in d.columns.iter().chain(&d.owners) {
                            cyclic[v.index()] = true;
                        }
                    }
                    Effect::Assign {
                        var, param, rhs, ..
                    } => {
                        let tos: Vec<u8> = match rhs {
                            Rhs::Lit(v) => vec![v.0],
                            Rhs::Param(_) => (0..self.bounds.slots() as u8)
                                .map(|c| self.var(*var).domain.of_slot(c).0)
                                .collect(),
                        };
                        for f in froms(*var, param) {
                            edges[var.index()].extend(tos.iter().map(|t| (f, *t)));
                        }
                    }
                    Effect::Transition { var, arms, .. } => {
                        edges[var.index()].extend(arms.iter().map(|(f, t)| (f.0, t.0)));
                    }
                    Effect::Choose {
                        var, param, values, ..
                    } => {
                        for f in froms(*var, param) {
                            edges[var.index()].extend(values.iter().map(|v| (f, v.0)));
                        }
                    }
                    Effect::Tick => {
                        if let Some(now) = self.now {
                            edges[now.index()].extend((0..self.bounds.int_max).map(|i| (i, i + 1)));
                        }
                    }
                }
            }
        }
        (0..n).map(|v| !cyclic[v] && acyclic(&edges[v])).collect()
    }

    /// The split for one anchor: T holds the anchor and the writes of the kept actions, and
    /// any action that writes a T var, or that writes into the cone while reading a T var
    /// that is not monotone, has every write in T. So S is closed up to its inputs: its
    /// writers read S and monotone T vars, and write only S. An entity's vars share a side
    /// (`own_existence`): a T entity's view is observed in full, and S is a system of whole
    /// entities, whose owner vars may name a parent in T (`restrict_to`).
    fn split_at(
        &self,
        anchor: Option<VarId>,
        keep: &BTreeSet<ActId>,
        reads: &[VarSet],
        writes: &[VarSet],
        monotone: &[bool],
    ) -> Seed {
        let own: Vec<Option<VarId>> = self.var_ids().map(|v| self.own_existence(v)).collect();
        let mut t: VarSet = anchor.into_iter().collect();
        let mut t_actions = BTreeSet::new();
        loop {
            let mut grew = false;
            for i in 0..self.actions.len() {
                let id = ActId::at(i);
                if t_actions.contains(&id) || (writes[i].is_empty() && !keep.contains(&id)) {
                    continue;
                }
                let reads_t = reads[i]
                    .iter()
                    .any(|v| t.contains(v) && !monotone[v.index()]);
                if keep.contains(&id) || !writes[i].is_disjoint(&t) || reads_t {
                    t_actions.insert(id);
                    t.extend(writes[i].iter().copied());
                    grew = true;
                }
            }
            for v in self.var_ids() {
                let Some(e) = own[v.index()] else { continue };
                grew |= (t.contains(&v) && t.insert(e)) || (t.contains(&e) && t.insert(v));
            }
            if !grew {
                return (t, t_actions);
            }
        }
    }

    /// The split of the cone for its targets' seeds and kept actions: of the splits anchored
    /// at each var the seeds read, the one with the largest S, the first on a tie. What the
    /// seeds read of S is observed, so every target's check reads T and `obs` only. With
    /// `inputs`, an S action may read monotone T vars, which become the inputs of S; without,
    /// every reader of a T var joins T.
    pub(super) fn split_with(&self, seeds: &[Seed], inputs: bool) -> Split {
        // A copy's reads count here too: an S action that copies from T keeps the copy's
        // guard through its inputs, or joins T.
        let reads: Vec<VarSet> = self
            .actions
            .iter()
            .map(|a| {
                let mut r = self.action_reads(a);
                for (_, c) in self.copy_reads(a) {
                    r.extend(c);
                }
                r
            })
            .collect();
        let writes: Vec<VarSet> = self.actions.iter().map(|a| self.action_writes(a)).collect();
        let monotone: Vec<bool> = if inputs {
            self.monotone_vars()
        } else {
            vec![false; self.vars.len()]
        };
        let keep: BTreeSet<ActId> = seeds.iter().flat_map(|(_, k)| k.iter().copied()).collect();
        let mut anchors: Vec<Option<VarId>> = seeds
            .iter()
            .flat_map(|(s, _)| s.iter().copied().map(Some))
            .collect();
        anchors.dedup();
        if anchors.is_empty() {
            anchors.push(None);
        }
        let (t, t_actions) = anchors
            .into_iter()
            .map(|a| self.split_at(a, &keep, &reads, &writes, &monotone))
            .min_by_key(|(t, _)| t.len())
            .expect("an anchor");
        let s: VarSet = self.var_ids().filter(|v| !t.contains(v)).collect();
        let s_actions: BTreeSet<ActId> = self
            .action_ids()
            .filter(|id| !t_actions.contains(id) && !writes[id.index()].is_empty())
            .collect();
        let mut obs = VarSet::new();
        for id in &t_actions {
            obs.extend(reads[id.index()].intersection(&s).copied());
        }
        for (seed, _) in seeds {
            obs.extend(seed.intersection(&s).copied());
        }
        let mut inputs = VarSet::new();
        for id in &s_actions {
            inputs.extend(reads[id.index()].intersection(&t).copied());
        }
        for v in inputs.clone() {
            inputs.extend(self.existences(v));
        }
        inputs.retain(|v| t.contains(v));
        assert!(
            inputs.iter().all(|v| monotone[v.index()]),
            "every input of S is monotone"
        );
        Split {
            t,
            s,
            t_actions,
            s_actions,
            obs,
            inputs,
        }
    }

    /// This cone system with its S side reduced, when it has one, its S side fits under
    /// `caps`, and the tables TLC would get fit under `TLA_CELL_BUDGET`; otherwise unchanged.
    /// `seeds` are what each target that shares the cone reads and keeps.
    pub fn reduced(&self, seeds: &[Seed], caps: Caps) -> TransitionSystem {
        match self.reduced_with(seeds, caps, true) {
            Ok(out) => out,
            Err(true) => self
                .reduced_with(seeds, caps, false)
                .unwrap_or_else(|_| self.clone()),
            Err(false) => self.clone(),
        }
    }

    /// One attempt at `reduced`, with or without the inputs of S. `Err(true)` says the
    /// printed tables would pass `TLA_CELL_BUDGET` under a split that had inputs, so the
    /// split without them is worth a try; `Err(false)` that nothing reduces.
    fn reduced_with(
        &self,
        seeds: &[Seed],
        caps: Caps,
        inputs: bool,
    ) -> Result<TransitionSystem, bool> {
        let split = self.split_with(seeds, inputs);
        // Every S var observed by T makes the quotient the identity, whatever the inputs do.
        if split.s.is_empty() || split.s.is_subset(&split.obs) {
            return Err(false);
        }
        let ext = Extended::of(self, &split);
        let Some(quotient) = Quotient::of(&ext, caps) else {
            return Err(false);
        };
        let Quotient {
            reps,
            class_of,
            init,
            moves,
            rises,
            inner,
            states: n,
        } = quotient;
        let all: Vec<VarId> = split.s.union(&split.inputs).copied().collect();
        let spans: Vec<(usize, usize, bool)> = all
            .iter()
            .map(|&v| {
                let width = match self.var(v).kind {
                    VarKind::Now => 1,
                    _ => self.bounds.slots(),
                };
                (self.offset(v), width, split.s.contains(&v))
            })
            .collect();
        let width: usize = spans.iter().map(|(_, w, _)| w).sum();
        let tail = ext.system.initial_states()[0].0[width..].into();
        let mut skip = vec![false; self.actions.len()];
        for a in &split.s_actions {
            skip[a.index()] = true;
        }
        let s_entity = self
            .entities
            .iter()
            .map(|e| split.s.contains(&e.exists))
            .collect();
        let reduction = Reduction {
            vars: split.s.iter().copied().collect(),
            inputs: split.inputs.iter().copied().collect(),
            all,
            spans,
            rises,
            tail,
            s_actions: split.s_actions.clone(),
            skip,
            s_entity,
            obs: split.obs.clone(),
            reps,
            class_of,
            init,
            moves,
            inner,
            states: n,
        };
        if reduction.tla_cells() > TLA_CELL_BUDGET {
            return Err(!split.inputs.is_empty());
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
        let reads_ok = |ts: &TransitionSystem, binders: &[EntId], preds: &[&super::types::Pred]| {
            let mut r = VarSet::new();
            binders.iter().for_each(|e| {
                r.insert(ts.entity(*e).exists);
            });
            preds.iter().for_each(|p| ts.pred_reads(p, &mut r));
            r.is_subset(&readable)
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
        let properties = std::mem::take(&mut self.properties);
        self.properties = properties
            .into_iter()
            .filter(|p| {
                let preds: Vec<&super::types::Pred> = match &p.formula {
                    super::types::Ltl::Always(x) | super::types::Ltl::Eventually(x) => vec![x],
                    super::types::Ltl::LeadsTo(x, y) => vec![x, y],
                };
                reads_ok(self, &p.binders, &preds)
                    && self.actions.iter().enumerate().all(|(i, a)| {
                        !matches!(&a.origin, ActionOrigin::UseCase { uc, .. } if p.fair.contains(uc))
                            || split.t_actions.contains(&ActId::at(i))
                    })
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
        let (sys, local) = self.cone_system(cone, seeds);
        if std::env::var_os("RDRA_ISH_NO_REDUCE").is_some() {
            return sys;
        }
        sys.reduced(&local, caps)
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
    pub(crate) fn plain_moves(&self, s: &State) -> Vec<Step> {
        self.moves_of(s, false)
            .map(|m| (m.act, m.params, m.next))
            .collect()
    }

    /// A reduced path, each step `(action, params, successor)` from `start`, lifted to a path
    /// of the unreduced cone: a class move becomes the S-action path that realizes it from the
    /// concrete S state the lifted path holds. Returns the start with its S part concrete and
    /// the lifted steps, grouped per input step.
    pub(crate) fn lift(&self, start: &State, steps: &[Step]) -> (State, Vec<Vec<Step>>) {
        let Some(r) = self.reduction() else {
            return (
                start.clone(),
                steps.iter().map(|s| vec![s.clone()]).collect(),
            );
        };
        let ext = Extended::rebuild(self, r);
        let mut sub = ext.system.clone();
        sub.reduction = r.inner.clone();
        let run = sub.clone().run(Caps::new(usize::MAX, usize::MAX));
        let n = run.states.len();
        let key_vars: VarSet = ext.obs_local.union(&ext.inputs_local).copied().collect();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = run.system.project(st, &key_vars);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let (silent, rise_edges) = split_edges(&run, &ext.rise);
        let blocks = quotient_blocks(n, &silent, &rise_edges, &initial);
        let at: HashMap<&State, u32> = run
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s, i as u32))
            .collect();
        let block_of_class: HashMap<u32, u32> = run
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| r.class_of.get(&s.0).map(|c| (*c, blocks[i])))
            .collect();
        let succ = Succ::new(&run, &ext.rise);
        // The concrete start: an initial S state in the start's class. The run reaches its
        // initial states first.
        let start_block = block_of_class[&r
            .class(start)
            .expect("a reduced state holds a representative")];
        let s0 = run
            .states
            .iter()
            .take(sub.initial_states().len())
            .find(|s| blocks[at[*s] as usize] == start_block)
            .expect("the start's class holds an initial S state")
            .clone();
        enum Lifted {
            /// A T step, and the rises of the extended sub-lifecycle it makes.
            T(ActId, Box<[u8]>, State, usize),
            S(usize),
        }
        let mut level: Vec<Lifted> = Vec::new();
        let mut s_steps: Vec<Step> = Vec::new();
        let mut s_at = at[&s0];
        for (act, params, post) in steps {
            if !r.skips(*act) {
                let after = run
                    .system
                    .project(&State(r.key(post).into()), &ext.inputs_local);
                let before = run
                    .system
                    .project(&run.states[s_at as usize], &ext.inputs_local);
                let stages = r.stages(&before.0, &after.0);
                for stage in &stages {
                    let e = succ
                        .rises_from(s_at)
                        .find(|e| {
                            run.system
                                .project(&run.states[e.to as usize], &ext.inputs_local)
                                .0
                                .as_ref()
                                == stage.as_slice()
                        })
                        .expect("a T step's rise is a rise of the extended sub-lifecycle");
                    let (a, p) = run.step(&e);
                    s_steps.push((a, p.into(), run.states[e.to as usize].clone()));
                    s_at = e.to;
                }
                level.push(Lifted::T(*act, params.clone(), post.clone(), stages.len()));
                continue;
            }
            let target = block_of_class[&r.class(post).expect("a class move enters a class")];
            let path = succ.shortest_path(s_at, |i| blocks[i as usize] == target);
            let mut count = 0;
            for e in path {
                let (a, p) = run.step(&e);
                s_steps.push((a, p.into(), run.states[e.to as usize].clone()));
                s_at = e.to;
                count += 1;
            }
            level.push(Lifted::S(count));
        }
        // The S path is a path of the extended sub-lifecycle under its own reduction: lift it
        // there and splice the concrete S parts back under the T steps.
        let (s_start, lifted) = sub.lift(&s0, &s_steps);
        let mut lifted = lifted.into_iter();
        let mut out_start = start.clone();
        r.write(&mut out_start, &s_start.0);
        let mut cur = out_start.clone();
        let mut out = Vec::new();
        for step in level {
            let mut group = Vec::new();
            match step {
                Lifted::T(act, params, post, rises) => {
                    // A rise's lifted step leaves S where it was; the T step below carries
                    // the inputs.
                    for _ in 0..rises {
                        let group = lifted.next().expect("one group per rise");
                        assert_eq!(group.len(), 1, "a rise lifts as itself");
                    }
                    let mut next = post;
                    r.write(&mut next, &r.key(&cur));
                    debug_assert!(
                        self.plain_moves(&cur)
                            .iter()
                            .any(|(a, p, n)| *a == act && *p == params && *n == next),
                        "a T step lifts as itself"
                    );
                    group.push((act, params, next.clone()));
                    cur = next;
                }
                Lifted::S(count) => {
                    for _ in 0..count {
                        for (a, p, s_post) in lifted.next().expect("one group per S step") {
                            let mut next = cur.clone();
                            r.write(&mut next, &s_post.0);
                            group.push((ext.acts[a.index()], p, next.clone()));
                            cur = next;
                        }
                    }
                }
            }
            out.push(group);
        }
        (out_start, out)
    }

    /// The moves of `s`, with the reduction applied when the system has one and `reduced`: the
    /// S actions are left out, the class moves come last, and a T step that sets an input of
    /// S moves the representative to the class the inputs put it in.
    pub(crate) fn moves_of<'s>(
        &'s self,
        s: &'s State,
        reduced: bool,
    ) -> impl Iterator<Item = super::explore::Move> + 's {
        let r = if reduced { self.reduction() } else { None };
        let extra: Vec<super::explore::Move> = r
            .map(|r| {
                r.moves_from(s)
                    .into_iter()
                    .map(|(act, params, next)| super::explore::Move { act, params, next })
                    .collect()
            })
            .unwrap_or_default();
        let inputs = r
            .filter(|r| !r.inputs.is_empty())
            .map(|r| (r, r.input_bytes(s)));
        self.raw_moves(s)
            .filter(move |m| r.is_none_or(|r| !r.skips(m.act)))
            .map(move |mut m| {
                if let Some((r, before)) = &inputs {
                    let after = r.input_bytes(&m.next);
                    if after != *before {
                        let class = r.class(s).expect("a reduced state holds a representative");
                        let to = r.recanon(class, before, &after).unwrap_or_else(|| {
                            panic!(
                                "a T step's rise is a rise of the extended sub-lifecycle: {} from class {class} inputs {before:?} to {after:?}, known {:?}",
                                self.actions[m.act.index()].name,
                                r.rises_from(class)
                            )
                        });
                        r.write(&mut m.next, &r.reps[to as usize]);
                    }
                }
                m
            })
            .chain(extra)
    }
}

/// The moves of a run by source state, for shortest paths over the silent edges, with the
/// rise edges kept apart.
struct Succ<'a> {
    run: &'a super::explore::ComponentRun,
    start: Vec<u32>,
    order: Vec<u32>,
    is_rise: &'a [bool],
}

impl<'a> Succ<'a> {
    fn new(run: &'a super::explore::ComponentRun, is_rise: &'a [bool]) -> Self {
        let n = run.states.len();
        let mut start = vec![0u32; n + 1];
        for e in run.edges() {
            start[e.from as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut order = vec![0u32; run.edges().len()];
        for (k, e) in run.edges().iter().enumerate() {
            order[fill[e.from as usize] as usize] = k as u32;
            fill[e.from as usize] += 1;
        }
        Succ {
            run,
            start,
            order,
            is_rise,
        }
    }

    fn all_from(&self, at: u32) -> impl Iterator<Item = super::explore::Edge> + '_ {
        self.order[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
            .iter()
            .map(|k| self.run.edges()[*k as usize])
    }

    fn is_rise(&self, e: &super::explore::Edge) -> bool {
        self.is_rise[self.run.step(e).0.index()]
    }

    /// The silent edges from `at`.
    fn edges_from(&self, at: u32) -> impl Iterator<Item = super::explore::Edge> + '_ {
        self.all_from(at).filter(move |e| !self.is_rise(e))
    }

    /// The rise edges from `at`.
    fn rises_from(&self, at: u32) -> impl Iterator<Item = super::explore::Edge> + '_ {
        self.all_from(at).filter(move |e| self.is_rise(e))
    }

    /// The BFS path over silent edges from `from` to the first state `done` accepts.
    fn shortest_path(&self, from: u32, done: impl Fn(u32) -> bool) -> Vec<super::explore::Edge> {
        if done(from) {
            return Vec::new();
        }
        let mut parent: HashMap<u32, super::explore::Edge> = HashMap::new();
        let mut queue = VecDeque::from([from]);
        while let Some(x) = queue.pop_front() {
            for e in self.edges_from(x) {
                if e.to == from || parent.contains_key(&e.to) {
                    continue;
                }
                parent.insert(e.to, e);
                if done(e.to) {
                    let mut path = vec![e];
                    let mut at = e.from;
                    while at != from {
                        let p = parent[&at];
                        path.push(p);
                        at = p.from;
                    }
                    path.reverse();
                    return path;
                }
                queue.push_back(e.to);
            }
        }
        panic!("a class move has a path of S actions behind it");
    }
}

/// Divergence-blind stuttering equivalence of a graph whose moves are all silent, refining
/// `initial`, the block of each state: the coarsest partition in which, for any two blocks
/// B and B', either every state of B reaches B' through B or none does. Returns each state's
/// block.
fn stuttering_blocks(n: usize, edges: &[(u32, u32)], initial: &[u32]) -> Vec<u32> {
    let mut block: Vec<u32> = initial.to_vec();
    let mut nblocks = block.iter().max().map_or(0, |b| *b as usize + 1);
    let inter: Vec<(u32, u32)> = edges.iter().copied().filter(|(a, b)| a != b).collect();
    let pred_pairs: Vec<(u32, u32)> = inter.iter().map(|&(a, b)| (b, a)).collect();
    let pred = Adj::new(n, &pred_pairs);
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); nblocks];
    for (i, &b) in block.iter().enumerate() {
        members[b as usize].push(i as u32);
    }
    let mut marked = vec![false; n];
    loop {
        let mut changed = false;
        let mut splitter = 0;
        while splitter < nblocks {
            let mut stack: Vec<u32> = Vec::new();
            let mut touched: Vec<u32> = Vec::new();
            for &t in &members[splitter] {
                for &p in pred.of(t) {
                    if block[p as usize] != splitter as u32 && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            while let Some(x) = stack.pop() {
                for &p in pred.of(x) {
                    if block[p as usize] == block[x as usize] && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            let mut by_block: HashMap<u32, Vec<u32>> = HashMap::new();
            for &x in &touched {
                by_block.entry(block[x as usize]).or_default().push(x);
            }
            let mut split: Vec<(u32, Vec<u32>)> = by_block.into_iter().collect();
            split.sort_unstable();
            for (b, xs) in split {
                if xs.len() < members[b as usize].len() {
                    let nb = nblocks as u32;
                    nblocks += 1;
                    for &x in &xs {
                        block[x as usize] = nb;
                    }
                    members.push(xs);
                    members[b as usize].retain(|m| block[*m as usize] == b);
                    changed = true;
                }
            }
            for &x in &touched {
                marked[x as usize] = false;
            }
            splitter += 1;
        }
        if !changed {
            return block;
        }
    }
}

/// Edges of a run by state index.
type Edges = Vec<(u32, u32)>;

/// The edges of a run of the extended sub-lifecycle, the silent ones and the rises apart.
fn split_edges(run: &super::explore::ComponentRun, is_rise: &[bool]) -> (Edges, Edges) {
    let mut silent = Vec::new();
    let mut rises = Vec::new();
    for (a, b, act) in run.graph().edges() {
        if is_rise[act.index()] {
            rises.push((a, b));
        } else {
            silent.push((a, b));
        }
    }
    (silent, rises)
}

/// Whether a var's write edges form no cycle; a self-loop is not one.
fn acyclic(edges: &BTreeSet<(u8, u8)>) -> bool {
    fn visit(v: u8, edges: &BTreeSet<(u8, u8)>, done: &mut HashMap<u8, bool>) -> bool {
        if let Some(finished) = done.get(&v) {
            return *finished;
        }
        done.insert(v, false);
        for &(a, b) in edges {
            if a == v && b != v && !visit(b, edges, done) {
                return false;
            }
        }
        done.insert(v, true);
        true
    }
    let mut done = HashMap::new();
    edges
        .iter()
        .flat_map(|(a, b)| [*a, *b])
        .all(|v| visit(v, edges, &mut done))
}

/// Compressed adjacency.
struct Adj {
    start: Vec<u32>,
    list: Vec<u32>,
}

impl Adj {
    fn new(n: usize, pairs: &[(u32, u32)]) -> Self {
        let mut start = vec![0u32; n + 1];
        for &(a, _) in pairs {
            start[a as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut list = vec![0u32; pairs.len()];
        for &(a, b) in pairs {
            list[fill[a as usize] as usize] = b;
            fill[a as usize] += 1;
        }
        Adj { start, list }
    }

    fn of(&self, at: u32) -> &[u32] {
        &self.list[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
    }
}
