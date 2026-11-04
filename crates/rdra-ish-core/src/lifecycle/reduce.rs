//! Reduction of a cone by an independent sub-lifecycle (#74). A cone's vars split into a
//! target side T, which holds what the cone's verdicts read and every var an action writes
//! when it writes or reads T, and the rest S, written only by actions that read only S; an
//! entity's vars share a side. T reads S through the vars `obs`. S is a transition system of
//! its own (`restrict_to` stubs a parent of its entities that is in T), and its states are
//! quotiented by divergence-blind stuttering equivalence with respect to `obs`: two S states
//! are equivalent when they agree on `obs` and every class a path of S-actions reaches from
//! one is reached from the other. The reduced cone keeps one representative per class in
//! the state, runs T-actions on it as they are, and replaces the S-actions by class moves:
//! from a class, by the last action of a path that leaves it, to the class the path enters.
//! The reduced cone is stuttering equivalent to the cone with respect to T and `obs`, so it
//! reaches the same T-side states, the same findings on T's entities, and the same verdicts
//! of the checks that read T and `obs`; a check that reads more of S is dropped and decided
//! on its own cone (docs/formal-verification.md, "Reduction by an independent sub-lifecycle").
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
use super::types::{ActId, ActionOrigin, EntId, VarId, VarKind};
use super::TransitionSystem;

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
    /// `(offset, width)` of each S var in the state.
    spans: Vec<(usize, usize)>,
    /// The S subsystem's bytes after its S vars: the stubs `restrict_to` adds for parents
    /// outside S, which no S action writes.
    tail: Box<[u8]>,
    s_actions: BTreeSet<ActId>,
    skip: Vec<bool>,
    s_entity: Vec<bool>,
    /// S vars the T-side actions read.
    obs: VarSet,
    /// The representative of each class: its S bytes.
    reps: Vec<Box<[u8]>>,
    class_of: HashMap<Box<[u8]>, u32>,
    /// Each initial S state's class.
    init: HashMap<Box<[u8]>, u32>,
    /// Per class, the class moves: the last action, its params, and the class entered.
    moves: Vec<Vec<ClassMove>>,
    /// Each S subsystem action as the cone's (by `Action::source`); an S action reads only
    /// S, so its params are the same there.
    acts: Vec<ActId>,
    /// The reduction S was explored with, in the S subsystem's ids.
    inner: Option<Arc<Reduction>>,
    /// S states explored.
    pub states: usize,
}

impl Reduction {
    pub fn classes(&self) -> usize {
        self.reps.len()
    }

    /// The S vars, ascending.
    pub fn vars(&self) -> &[VarId] {
        &self.vars
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

    /// The S bytes of a state, as the S subsystem's state.
    pub(crate) fn key(&self, s: &State) -> Vec<u8> {
        let mut key = Vec::new();
        for &(at, w) in &self.spans {
            key.extend_from_slice(&s.0[at..at + w]);
        }
        key.extend_from_slice(&self.tail);
        key
    }

    fn write(&self, s: &mut State, bytes: &[u8]) {
        let mut i = 0;
        for &(at, w) in &self.spans {
            s.0[at..at + w].copy_from_slice(&bytes[i..i + w]);
            i += w;
        }
    }

    /// The class of a state whose S part is a representative.
    pub fn class(&self, s: &State) -> Option<u32> {
        self.class_of.get(self.key(s).as_slice()).copied()
    }

    /// The representative's S bytes.
    pub fn rep(&self, class: u32) -> &[u8] {
        &self.reps[class as usize]
    }

    /// The value of an S var's slot in a class's representative; `None` for a var outside S.
    pub fn rep_value(&self, class: u32, var: VarId, slot: usize) -> Option<super::types::Value> {
        let k = self.vars.iter().position(|v| *v == var)?;
        let at: usize = self.spans[..k].iter().map(|(_, w)| w).sum();
        Some(super::types::Value(self.reps[class as usize][at + slot]))
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
        let Some(class) = self.class(s) else {
            return Vec::new();
        };
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
    acts: Vec<ActId>,
    inner: Option<Arc<Reduction>>,
    states: usize,
}

impl Quotient {
    /// The quotient of `raw`, the S subsystem of `cone`, by stuttering equivalence with
    /// respect to `obs`; `None` when S does not fit under `caps`, or when the quotient is the
    /// identity on an S that no inner reduction shrank, which would reduce nothing (#101).
    /// An unobserved S is one
    /// class, held at an initial S state, with no moves: nothing on T reads it, so no path
    /// through it is told from staying put.
    fn of(
        cone: &TransitionSystem,
        raw: &TransitionSystem,
        split: &Split,
        caps: Caps,
    ) -> Option<Self> {
        let s_vars: Vec<VarId> = split.s.iter().copied().collect();
        let acts: Vec<ActId> = raw
            .actions
            .iter()
            .map(|a| {
                let at = cone
                    .actions
                    .iter()
                    .position(|b| b.source == a.source)
                    .expect("an S action is an action of the cone");
                ActId::at(at)
            })
            .collect();
        if split.obs.is_empty() {
            let initial = raw.initial_states();
            let rep = initial.first().expect("an initial state").0.clone();
            return Some(Quotient {
                class_of: HashMap::from([(rep.clone(), 0)]),
                reps: vec![rep],
                init: initial.iter().map(|st| (st.0.clone(), 0)).collect(),
                moves: vec![Vec::new()],
                acts,
                inner: None,
                states: initial.len(),
            });
        }
        let local = |v: &VarId| VarId::at(s_vars.iter().position(|x| x == v).expect("in S"));
        let obs_local: VarSet = split.obs.iter().map(local).collect();
        let sub = raw.reduced(&[(obs_local.clone(), BTreeSet::new())], caps);
        let run = sub.clone().run(caps);
        if !run.complete {
            return None;
        }
        let n = run.states.len();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = run.system.project(st, &obs_local);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let edges: Vec<(u32, u32)> = run.graph().edges().map(|(a, b, _)| (a, b)).collect();
        let blocks = stuttering_blocks(n, &edges, &initial);
        if sub.reduction().is_none() && blocks.iter().collect::<BTreeSet<_>>().len() == n {
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
        // S subsystem canonicalizes them for its own reduction first, as the run did.
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
        let mut seen: BTreeSet<(u32, ActId, Box<[u8]>, u32)> = BTreeSet::new();
        for e in run.edges() {
            let (x, y) = (e.from as usize, e.to as usize);
            if class[x] == class[y] {
                continue;
            }
            let (act, params) = run.step(e);
            let act = acts[act.index()];
            if seen.insert((class[x], act, params.into(), class[y])) {
                moves[class[x] as usize].push((act, params.into(), class[y]));
            }
        }
        Some(Quotient {
            reps,
            class_of,
            init,
            moves,
            acts,
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
    s_actions: BTreeSet<ActId>,
    obs: VarSet,
}

impl TransitionSystem {
    pub fn reduction(&self) -> Option<&Reduction> {
        self.reduction.as_deref()
    }

    /// The split for one anchor: T holds the anchor and the writes of the kept actions, and
    /// any action that writes a T var, or that writes into the cone while reading one, has
    /// every write in T. So S is closed: its writers read and write only S. An entity's vars
    /// share a side (`own_existence`): a T entity's view is observed in full, and S is a
    /// system of whole entities, whose owner vars may name a parent in T (`restrict_to`).
    fn split_at(
        &self,
        anchor: Option<VarId>,
        keep: &BTreeSet<ActId>,
        reads: &[VarSet],
        writes: &[VarSet],
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
                if keep.contains(&id) || !writes[i].is_disjoint(&t) || !reads[i].is_disjoint(&t) {
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
    /// seeds read of S is observed, so every target's check reads T and `obs` only.
    pub(super) fn split(&self, seeds: &[Seed]) -> Split {
        let reads: Vec<VarSet> = self.actions.iter().map(|a| self.action_reads(a)).collect();
        let writes: Vec<VarSet> = self.actions.iter().map(|a| self.action_writes(a)).collect();
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
            .map(|a| self.split_at(a, &keep, &reads, &writes))
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
            for (_, r) in self.copy_reads(&self.actions[id.index()]) {
                obs.extend(r.intersection(&s).copied());
            }
        }
        for (seed, _) in seeds {
            obs.extend(seed.intersection(&s).copied());
        }
        Split {
            t,
            s,
            t_actions,
            s_actions,
            obs,
        }
    }

    /// This cone system with its S side reduced, when it has one and its S side fits under
    /// `caps`; otherwise unchanged. `seeds` are what each target that shares the cone reads
    /// and keeps.
    pub fn reduced(&self, seeds: &[Seed], caps: Caps) -> TransitionSystem {
        let split = self.split(seeds);
        if split.s.is_empty() {
            return self.clone();
        }
        let s_vars: Vec<VarId> = split.s.iter().copied().collect();
        let raw = self.restrict_to(&split.s, &BTreeSet::new());
        let Some(quotient) = Quotient::of(self, &raw, &split, caps) else {
            return self.clone();
        };
        let Quotient {
            reps,
            class_of,
            init,
            moves,
            acts,
            inner,
            states: n,
        } = quotient;
        let spans: Vec<(usize, usize)> = s_vars
            .iter()
            .map(|&v| {
                let width = match self.var(v).kind {
                    VarKind::Now => 1,
                    _ => self.bounds.slots(),
                };
                (self.offset(v), width)
            })
            .collect();
        let s_width: usize = spans.iter().map(|(_, w)| w).sum();
        let tail = raw.initial_states()[0].0[s_width..].into();
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
            vars: s_vars,
            spans,
            tail,
            s_actions: split.s_actions.clone(),
            skip,
            s_entity,
            obs: split.obs.clone(),
            reps,
            class_of,
            init,
            moves,
            acts,
            inner,
            states: n,
        };
        let mut out = self.clone();
        out.reduction = Some(Arc::new(reduction));
        out.drop_unreadable_checks(&split);
        out
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

    /// The S subsystem, with the reduction S was explored under.
    pub(crate) fn sub_system(&self, r: &Reduction) -> TransitionSystem {
        let vars: VarSet = r.vars.iter().copied().collect();
        let mut sub = self.restrict_to(&vars, &BTreeSet::new());
        sub.reduction = r.inner.clone();
        sub
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
        let sub = self.sub_system(r);
        let run = sub.clone().run(Caps::new(usize::MAX, usize::MAX));
        let n = run.states.len();
        let obs_local: VarSet = r
            .obs
            .iter()
            .map(|v| VarId::at(r.vars.iter().position(|x| x == v).expect("obs in S")))
            .collect();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = run.system.project(st, &obs_local);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let edges: Vec<(u32, u32)> = run.graph().edges().map(|(a, b, _)| (a, b)).collect();
        let blocks = stuttering_blocks(n, &edges, &initial);
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
        let succ = Succ::new(&run);
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
        // Each class move becomes a path of the run from the S state the path holds.
        enum Lifted {
            T(ActId, Box<[u8]>, State),
            S(usize),
        }
        let mut level: Vec<Lifted> = Vec::new();
        let mut s_steps: Vec<Step> = Vec::new();
        let mut s_at = at[&s0];
        for (act, params, post) in steps {
            if !r.skips(*act) {
                level.push(Lifted::T(*act, params.clone(), post.clone()));
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
        // The S path is a path of the S subsystem under its own reduction: lift it there and
        // splice the concrete S parts back under the T steps.
        let (s_start, lifted) = sub.lift(&s0, &s_steps);
        let mut lifted = lifted.into_iter();
        let mut out_start = start.clone();
        r.write(&mut out_start, &s_start.0);
        let mut cur = out_start.clone();
        let mut out = Vec::new();
        for step in level {
            let mut group = Vec::new();
            match step {
                Lifted::T(act, params, post) => {
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
                            group.push((r.acts[a.index()], p, next.clone()));
                            cur = next;
                        }
                    }
                }
            }
            out.push(group);
        }
        (out_start, out)
    }

    /// The moves of `s`, with the reduction applied when the system has one and `reduced`.
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
        self.raw_moves(s)
            .filter(move |m| r.is_none_or(|r| !r.skips(m.act)))
            .chain(extra)
    }
}

/// The moves of a run by source state, for shortest paths.
struct Succ<'a> {
    run: &'a super::explore::ComponentRun,
    start: Vec<u32>,
    order: Vec<u32>,
}

impl<'a> Succ<'a> {
    fn new(run: &'a super::explore::ComponentRun) -> Self {
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
        Succ { run, start, order }
    }

    fn edges_from(&self, at: u32) -> impl Iterator<Item = super::explore::Edge> + '_ {
        self.order[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
            .iter()
            .map(|k| self.run.edges()[*k as usize])
    }

    /// The BFS path from `from` to the first state `done` accepts, as edges.
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
