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
use super::types::{
    ActId, Action, ActionOrigin, At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, ModelRef,
    Param, ParamRole, Pred, Rhs, Site, Start, Term, Value, VarDecl, VarId, VarKind,
};
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
    /// The input bits: per group of S actions whose reads of T fold into one bit, the folds.
    /// A bit is a function of the T state (`bit_values`); in the extended sub-lifecycle it is
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

    /// The raw inputs of S, ascending: T vars its actions read, each of which only rises.
    pub fn inputs(&self) -> &[VarId] {
        &self.inputs
    }

    /// The input bits: per group, the folds of the S actions that read it.
    pub fn bits(&self) -> &[Vec<Fold>] {
        &self.bits
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
    pub(crate) fn key(&self, ts: &TransitionSystem, s: &State) -> Vec<u8> {
        let mut key = Vec::new();
        for &(at, w, _) in &self.spans {
            key.extend_from_slice(&s.0[at..at + w]);
        }
        key.extend_from_slice(&self.tail);
        key.extend(self.bit_values(ts, s));
        key
    }

    /// The input bytes of a state, in the extended sub-lifecycle's order: the raw inputs,
    /// then the bits.
    pub(crate) fn input_bytes(&self, ts: &TransitionSystem, s: &State) -> Vec<u8> {
        let mut out = Vec::new();
        for &(at, w, in_s) in &self.spans {
            if !in_s {
                out.extend_from_slice(&s.0[at..at + w]);
            }
        }
        out.extend(self.bit_values(ts, s));
        out
    }

    /// The input bits of a state: per bit group and slot of its key, whether some fold of
    /// the group holds there.
    fn bit_values(&self, ts: &TransitionSystem, s: &State) -> Vec<u8> {
        let slots = ts.bounds.slots();
        let mut out = Vec::with_capacity(self.bits.len() * slots);
        for group in &self.bits {
            for c in 0..slots as u8 {
                out.push(u8::from(group.iter().any(|f| f.holds(ts, s, c))));
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
    pub fn class(&self, ts: &TransitionSystem, s: &State) -> Option<u32> {
        self.class_of.get(self.key(ts, s).as_slice()).copied()
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
    pub(crate) fn canonical_initial(&self, ts: &TransitionSystem, s: &State) -> Option<State> {
        let class = *self.init.get(self.key(ts, s).as_slice())?;
        let mut out = s.clone();
        self.write(&mut out, &self.reps[class as usize]);
        Some(out)
    }

    /// The class moves from `s`: the last action, its params, and the successor.
    pub(crate) fn moves_from(&self, ts: &TransitionSystem, s: &State) -> Vec<Step> {
        let class = self
            .class(ts, s)
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

/// An S action's reads of T folded into one input bit (#112): the action keeps its S params
/// and guards and, in the extended sub-lifecycle, reads one bit per slot of its key in place
/// of its guards on T. For a `Param` key the bit holds when, for some binding of the other T
/// params, every folded guard holds, each T param whose binding needs a present row on one.
#[derive(Clone, Debug)]
pub struct Fold {
    /// The cone's action.
    act: ActId,
    key: FoldKey,
    /// The T params, each with whether its row must be present.
    t_params: Vec<(u8, bool)>,
    /// The folded guards, by index in the action.
    guards: Vec<usize>,
    /// For a `Linked` key: the folded node's body, with its quantifier over the key's entity
    /// stripped and its link atoms replaced by `True`, read with the key slot as `Bound(0)`.
    node: Option<Pred>,
}

/// What indexes a fold's bit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoldKey {
    /// A param of the action: the one T param a guard links to an S row or an effect
    /// references, or the one S param the folded guards name, or the acting row.
    Param(u8),
    /// A row of a T entity that a quantifier inside guard `guard` binds (the node fold): the
    /// node is a quantifier over `entity`, or an And/Or of such quantifiers, whose bodies
    /// read T except link atoms `link_var[s_param] = Index(row)`; it stands in the guard
    /// under And/Or alone, possibly more than once (the same node each time), so the
    /// quantifier can be pulled out to the action. The bit at slot `c` holds when row `c` of
    /// `entity` is present and the body holds for it; the extended sub-lifecycle binds the
    /// row as a param `q` of any slot and reads the node as `link_var[s_param] = q ∧ bit[q]`
    /// where the node stood.
    Linked {
        guard: usize,
        entity: EntId,
        link_var: VarId,
        s_param: u8,
    },
}

impl Fold {
    pub fn act(&self) -> ActId {
        self.act
    }

    pub fn key(&self) -> &FoldKey {
        &self.key
    }

    /// For a `Linked` key, the folded node's body, read with the key slot as `Bound(0)`.
    pub fn node_body(&self) -> Option<&Pred> {
        self.node.as_ref()
    }

    /// The T params and whether each must be on a present row.
    pub fn t_params(&self) -> &[(u8, bool)] {
        &self.t_params
    }

    /// The folded guards, by index in the action.
    pub fn guards(&self) -> &[usize] {
        &self.guards
    }

    /// Whether the bit holds for slot `c` of the key in `s`.
    fn holds(&self, ts: &TransitionSystem, s: &State, c: u8) -> bool {
        let a = &ts.actions[self.act.index()];
        let mut params = vec![0u8; a.params.len()];
        let key = match self.key {
            FoldKey::Param(key) => key,
            FoldKey::Linked { entity, .. } => {
                let body = self.node.as_ref().expect("a linked fold has its node");
                if !s.is_present(ts, entity, usize::from(c)) {
                    return false;
                }
                let mut env = super::expr::Env::new(&params);
                env.bound.push(c);
                return ts.eval(body, s, &mut env);
            }
        };
        params[usize::from(key)] = c;
        let free: Vec<u8> = self
            .t_params
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| *p != key)
            .collect();
        let slots = ts.bounds.slots();
        (0..slots.pow(free.len() as u32)).any(|binding| {
            let mut rest = binding;
            for p in &free {
                params[usize::from(*p)] = (rest % slots) as u8;
                rest /= slots;
            }
            let present = self.t_params.iter().all(|(p, must)| {
                !must
                    || s.is_present(
                        ts,
                        a.params[usize::from(*p)].entity,
                        usize::from(params[usize::from(*p)]),
                    )
            });
            let mut env = super::expr::Env::new(&params);
            present
                && self
                    .guards
                    .iter()
                    .all(|g| ts.eval(&a.guards[*g].pred, s, &mut env))
        })
    }
}

/// The extended sub-lifecycle of a cone (`restrict_to(S ∪ inputs)`), with its actions sorted
/// into the S actions of the cone, whose steps are silent, and the rises: T actions that
/// write a raw input, kept with the guards they have on S and the inputs only, and the flips
/// of the input bits.
struct Extended {
    system: TransitionSystem,
    /// Per action of `system`, the cone's action it comes from; `None` for a bit's flip.
    acts: Vec<Option<ActId>>,
    /// Per action of `system`, whether it is a rise.
    rise: Vec<bool>,
    obs_local: VarSet,
    /// The raw inputs and the bit vars, in `system`'s ids.
    inputs_local: VarSet,
    /// The bit groups, in the cone's ids.
    bits: Vec<Vec<Fold>>,
    variants: HashMap<ActId, Vec<ActId>>,
}

impl Extended {
    fn of(cone: &TransitionSystem, split: &Split) -> Self {
        Self::build(
            cone,
            &split.s,
            &split.inputs,
            &split.s_actions,
            &split.obs,
            &split.folds,
        )
    }

    /// The extended sub-lifecycle of a reduced cone, as `of` built it.
    fn rebuild(cone: &TransitionSystem, r: &Reduction) -> Self {
        let folds: Vec<Fold> = r.bits.iter().flatten().cloned().collect();
        Self::build(
            cone,
            &r.vars.iter().copied().collect(),
            &r.inputs.iter().copied().collect(),
            &r.s_actions,
            &r.obs,
            &folds,
        )
    }

    /// `restrict_to(S ∪ inputs)` with one bit var per group of folded S actions, each such
    /// action reading its bit in place of its guards on T (which the restriction dropped
    /// with their T params) and with its key's T param bound to any slot, the variants of
    /// one group merged into one action, and per bit a rise and a fall that flip it freely.
    fn build(
        cone: &TransitionSystem,
        s: &VarSet,
        inputs: &VarSet,
        s_actions: &BTreeSet<ActId>,
        obs: &VarSet,
        folds: &[Fold],
    ) -> Self {
        let all: VarSet = s.union(inputs).copied().collect();
        let (mut system, map) = cone.restrict_to_mapped(&all, &BTreeSet::new(), false);
        let local = |v: &VarId| VarId::at(all.iter().position(|x| x == v).expect("in S ∪ inputs"));
        let mut acts: Vec<Option<ActId>> = system
            .actions
            .iter()
            .map(|a| {
                let at = cone
                    .actions
                    .iter()
                    .position(|b| b.source == a.source)
                    .expect("an action of the extended sub-lifecycle is an action of the cone");
                Some(ActId::at(at))
            })
            .collect();
        let mut bits: Vec<Vec<Fold>> = Vec::new();
        let mut bit_vars: Vec<VarId> = Vec::new();
        let mut variants: HashMap<ActId, Vec<ActId>> = HashMap::new();
        let mut groups: HashMap<String, usize> = HashMap::new();
        let mut drop: Vec<bool> = vec![false; system.actions.len()];
        let t: VarSet = cone.var_ids().filter(|v| !s.contains(v)).collect();
        let new_bit = |system: &mut TransitionSystem, entity: EntId, name: &str| {
            let var = VarId::at(system.vars.len());
            system.vars.push(VarDecl {
                kind: VarKind::Run {
                    entity,
                    usecase: format!("in_{name}"),
                },
                domain: Domain::Bool,
                base: Value::FALSE,
                start: Start::Base,
                site: Site::bare(ModelRef::Synthesized),
            });
            var
        };
        let bit_guard = |var: VarId, key: u8| Guard {
            pred: Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Param(key),
                },
                CmpOp::Eq,
                Term::Lit(Value::TRUE),
            ),
            why: GuardWhy::Run { step: 0 },
            site: Site::bare(ModelRef::Synthesized),
        };
        for fold in folds {
            let ext_id = map.act[fold.act.index()].expect("an S action is kept");
            let renumber = |p: u8| match &map.params[fold.act.index()] {
                Some(m) => m[usize::from(p)].expect("an S param is kept"),
                None => p,
            };
            let name = system.actions[ext_id.index()].name.clone();
            let key = match &fold.key {
                FoldKey::Param(key) => *key,
                FoldKey::Linked {
                    guard,
                    entity,
                    link_var,
                    s_param,
                } => {
                    let body = fold.node.as_ref().expect("a linked fold has its node");
                    let signature = format!("{:?}", (entity, link_var, body));
                    let var = match groups.get(&signature) {
                        Some(&g) => {
                            bits[g].push(fold.clone());
                            bit_vars[g]
                        }
                        None => {
                            let var =
                                new_bit(&mut system, map.e(*entity), &format!("{name}_{guard}"));
                            groups.insert(signature, bits.len());
                            bits.push(vec![fold.clone()]);
                            bit_vars.push(var);
                            var
                        }
                    };
                    let a = &mut system.actions[ext_id.index()];
                    let q = a.params.len() as u8;
                    a.params.push(Param {
                        entity: map.e(*entity),
                        role: ParamRole::Earlier { step: 0 },
                        via_api: None,
                    });
                    let g = &cone.actions[fold.act.index()].guards[*guard];
                    let node = cone
                        .node_in(&g.pred, &t)
                        .expect("a linked fold's guard holds its node");
                    let rest = |p: &Pred| {
                        let mut p = map.pred(p);
                        if let Some(m) = &map.params[fold.act.index()] {
                            super::restrict::renumber_pred(
                                &mut p,
                                &super::restrict::renumber_by(m),
                            );
                        }
                        p
                    };
                    let mut stand_in: Vec<Pred> = node.outside.iter().map(rest).collect();
                    stand_in.push(Pred::Cmp(
                        Term::Slot {
                            var: map.v(*link_var),
                            at: At::Param(renumber(*s_param)),
                        },
                        CmpOp::Eq,
                        Term::Index(At::Param(q)),
                    ));
                    stand_in.push(bit_guard(var, q).pred);
                    let stand_in = Pred::And(stand_in);
                    let pred = cone.substitute_node(&g.pred, &t, &node, &stand_in, &rest);
                    a.guards.push(Guard {
                        pred,
                        why: g.why,
                        site: Site::bare(ModelRef::Synthesized),
                    });
                    variants.insert(fold.act, vec![fold.act]);
                    continue;
                }
            };
            let local_key = renumber(key);
            let a = &mut system.actions[ext_id.index()];
            if fold.t_params.iter().any(|(p, _)| *p == key) {
                a.params[usize::from(local_key)].role = ParamRole::Earlier { step: 0 };
            }
            let entity = a.params[usize::from(local_key)].entity;
            let signature = format!(
                "{:?}",
                (
                    &a.params,
                    a.guards.iter().map(|g| &g.pred).collect::<Vec<_>>(),
                    &a.effects,
                    local_key
                )
            );
            match groups.get(&signature) {
                Some(&g) => {
                    bits[g].push(fold.clone());
                    let first = bits[g][0].act;
                    variants
                        .get_mut(&first)
                        .expect("the group's first")
                        .push(fold.act);
                    drop[ext_id.index()] = true;
                }
                None => {
                    let var = new_bit(&mut system, entity, &name);
                    system.actions[ext_id.index()]
                        .guards
                        .push(bit_guard(var, local_key));
                    groups.insert(signature, bits.len());
                    bits.push(vec![fold.clone()]);
                    bit_vars.push(var);
                    variants.insert(fold.act, vec![fold.act]);
                }
            }
        }
        let kept: Vec<usize> = (0..system.actions.len()).filter(|i| !drop[*i]).collect();
        system.actions = kept.iter().map(|&i| system.actions[i].clone()).collect();
        acts = kept.iter().map(|&i| acts[i]).collect();
        // A bit starts where the cone starts: an entity no use case creates is present from
        // the first state, and a bit over it is raised there.
        for (g, &var) in bit_vars.iter().enumerate() {
            if bits[g].iter().any(|f| cone.fold_initial(f) == Some(true)) {
                system.vars[var.index()].base = Value::TRUE;
            }
        }
        for (k, &var) in bit_vars.iter().enumerate() {
            let VarKind::Run { entity, .. } = system.vars[var.index()].kind else {
                unreachable!("a bit is a run mark")
            };
            for (name, from, to) in [
                ("Rise", Value::FALSE, Value::TRUE),
                ("Fall", Value::TRUE, Value::FALSE),
            ] {
                system.actions.push(Action {
                    name: format!("{name}_in_{k}"),
                    source: ActId::at(cone.actions.len() + 2 * k + usize::from(to == Value::FALSE)),
                    origin: ActionOrigin::Clock,
                    site: Site::bare(ModelRef::Synthesized),
                    params: vec![Param {
                        entity,
                        role: ParamRole::Earlier { step: 0 },
                        via_api: None,
                    }],
                    guards: vec![Guard {
                        pred: Pred::Cmp(
                            Term::Slot {
                                var,
                                at: At::Param(0),
                            },
                            CmpOp::Eq,
                            Term::Lit(from),
                        ),
                        why: GuardWhy::Run { step: 0 },
                        site: Site::bare(ModelRef::Synthesized),
                    }],
                    effects: vec![Effect::Assign {
                        var,
                        param: 0,
                        rhs: Rhs::Lit(to),
                        site: Site::bare(ModelRef::Synthesized),
                    }],
                });
                acts.push(None);
            }
        }
        system.layout();
        let rise = acts
            .iter()
            .map(|a| a.is_none_or(|a| !s_actions.contains(&a)))
            .collect();
        let mut inputs_local: VarSet = inputs.iter().map(local).collect();
        inputs_local.extend(bit_vars);
        Extended {
            system,
            acts,
            rise,
            obs_local: obs.iter().map(local).collect(),
            inputs_local,
            bits,
            variants,
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
                    .canonical_initial(&sub, &st)
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
            let act = ext.acts[act.index()].expect("a silent step is an S action of the cone");
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
    /// T vars the S actions read raw (#112), each monotone, with the existence of each one's
    /// entity: the raw inputs of S.
    pub(super) inputs: VarSet,
    /// The S actions whose reads of T fold into a bit.
    pub(super) folds: Vec<Fold>,
}

/// A foldable node of a guard: the T entity its quantifier binds, the link `(S var, param)`
/// that ties the bound row to an S row, and the body with the quantifier stripped and the
/// links replaced by `True`.
struct Node {
    entity: EntId,
    link: (VarId, u8),
    body: Pred,
    /// Conjuncts of the quantifier's body that read S alone and name no bound row: they
    /// leave the quantifier and stand beside the link in the stand-in.
    outside: Vec<Pred>,
}

impl Node {
    fn same(&self, other: &Node) -> bool {
        self.entity == other.entity
            && self.link == other.link
            && self.body == other.body
            && self.outside == other.outside
    }
}

/// Whether `pred` reads a slot or index of a bound row.
fn names_bound(pred: &Pred) -> bool {
    let term = |t: &Term| {
        matches!(
            t,
            Term::Slot {
                at: At::Bound(_),
                ..
            } | Term::Index(At::Bound(_))
        )
    };
    match pred {
        Pred::True | Pred::False => false,
        Pred::Cmp(a, _, b) => term(a) || term(b),
        Pred::In(a, vs) => term(a) || vs.iter().any(term),
        Pred::Not(x) => names_bound(x),
        Pred::And(xs) | Pred::Or(xs) => xs.iter().any(names_bound),
        Pred::Forall { body, .. } | Pred::Exists { body, .. } => names_bound(body),
    }
}

/// The params a predicate names.
fn named_params(pred: &Pred) -> Vec<u8> {
    let named = std::cell::RefCell::new(Vec::new());
    let mut copy = pred.clone();
    super::restrict::renumber_pred(&mut copy, &|p: &mut u8| named.borrow_mut().push(*p));
    named.into_inner()
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
        let folds: Vec<Fold> = s_actions
            .iter()
            .filter_map(|id| self.fold_of(*id, &t))
            .filter(|f| self.fold_initial(f).is_some())
            .collect();
        let mut inputs = VarSet::new();
        for id in &s_actions {
            let fold = folds.iter().find(|f| f.act == *id);
            inputs.extend(self.raw_reads(*id, fold).intersection(&t).copied());
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
            folds,
        }
    }

    /// An S action's reads of T folded into a bit, when they can be (#112): at most one T
    /// param (a row of a T entity it binds) is referenced by an effect, and that one keys the
    /// bit; every guard that names a T param reads T only, or is a link guard that compares
    /// an S var with one T param's slot (that param is kept, bound to any slot, and keys the
    /// bit); the folded guards name at most one S param's slot, which keys the bit
    /// otherwise; no folded guard reads the successor. An action with no T param folds a
    /// node (`node_fold_of`).
    fn fold_of(&self, id: ActId, t: &VarSet) -> Option<Fold> {
        let a = &self.actions[id.index()];
        let t_params: Vec<(u8, bool)> = a
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| t.contains(&self.entity(p.entity).exists))
            .map(|(i, p)| {
                let present = matches!(
                    p.role,
                    ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered
                );
                (i as u8, present)
            })
            .collect();
        if t_params.is_empty() {
            return self.node_fold_of(id, t);
        }
        let is_t = |p: &u8| t_params.iter().any(|(q, _)| q == p);
        // A T param an effect references (an owner set to it) stays a param of the extended
        // sub-lifecycle, bound to any slot, so it must be the bit's key.
        let mut touched_t: BTreeSet<u8> = BTreeSet::new();
        for eff in &a.effects {
            let touched: Vec<u8> = match eff {
                Effect::Create { param } | Effect::Delete { param } => vec![*param],
                Effect::Assign { param, rhs, .. } => match rhs {
                    Rhs::Param(q) => vec![*param, *q],
                    Rhs::Lit(_) => vec![*param],
                },
                Effect::Transition { param, .. } | Effect::Choose { param, .. } => vec![*param],
                Effect::Tick => Vec::new(),
            };
            touched_t.extend(touched.iter().filter(|p| is_t(p)));
        }
        if touched_t.len() > 1 {
            return None;
        }
        let mut guards = Vec::new();
        let mut linked: BTreeSet<u8> = BTreeSet::new();
        let mut s_keys: BTreeSet<u8> = BTreeSet::new();
        for (gi, g) in a.guards.iter().enumerate() {
            let named = named_params(&g.pred);
            if !named.iter().any(is_t) {
                continue;
            }
            if g.reads_successor() || g.copy_target().is_some() {
                return None;
            }
            let mut r = VarSet::new();
            self.pred_reads(&g.pred, &mut r);
            if r.is_subset(t) {
                s_keys.extend(named.iter().filter(|p| !is_t(p)));
                guards.push(gi);
                continue;
            }
            let link = match &g.pred {
                Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(k),
                    },
                    CmpOp::Eq,
                    Term::Index(At::Param(p)),
                )
                | Pred::Cmp(
                    Term::Index(At::Param(p)),
                    CmpOp::Eq,
                    Term::Slot {
                        var,
                        at: At::Param(k),
                    },
                ) if !t.contains(var) && !is_t(k) && is_t(p) => Some(*p),
                _ => None,
            };
            linked.insert(link?);
        }
        if linked.len() > 1 || s_keys.len() > 1 || (!linked.is_empty() && !s_keys.is_empty()) {
            return None;
        }
        if let Some(p) = touched_t.first() {
            if !s_keys.is_empty() || linked.iter().any(|l| l != p) {
                return None;
            }
            linked.insert(*p);
        }
        let key = match (linked.first(), s_keys.first()) {
            (Some(p), _) => *p,
            (None, Some(k)) => *k,
            (None, None) => a
                .params
                .iter()
                .position(|p| p.role.is_written())
                .expect("an S action writes a row") as u8,
        };
        // A row the step creates has no slot before the step, so no bit can be keyed by it.
        if a.params[usize::from(key)].role == ParamRole::New {
            return None;
        }
        Some(Fold {
            act: id,
            key: FoldKey::Param(key),
            t_params,
            guards,
            node: None,
        })
    }

    /// The bit's value in the cone's initial states, when every initial state and every slot
    /// of the key agree; `None` otherwise, and the read stays raw (a bit var has one base).
    fn fold_initial(&self, fold: &Fold) -> Option<bool> {
        let initial = self.initial_states();
        let mut vals = initial
            .iter()
            .flat_map(|st| (0..self.bounds.slots() as u8).map(move |c| fold.holds(self, st, c)));
        let first = vals.next()?;
        vals.all(|v| v == first).then_some(first)
    }

    /// The node fold of an S action with no T params (#112): exactly one guard reads T,
    /// through one node (`FoldKey::Linked`) under And/Or alone.
    fn node_fold_of(&self, id: ActId, t: &VarSet) -> Option<Fold> {
        let a = &self.actions[id.index()];
        let mut found: Option<(usize, Node)> = None;
        for (gi, g) in a.guards.iter().enumerate() {
            let mut r = VarSet::new();
            self.pred_reads(&g.pred, &mut r);
            if r.is_disjoint(t) {
                continue;
            }
            if found.is_some() || g.copy_target().is_some() {
                return None;
            }
            found = Some((gi, self.node_in(&g.pred, t)?));
        }
        let (guard, node) = found?;
        Some(Fold {
            act: id,
            key: FoldKey::Linked {
                guard,
                entity: node.entity,
                link_var: node.link.0,
                s_param: node.link.1,
            },
            t_params: Vec::new(),
            guards: vec![guard],
            node: Some(node.body),
        })
    }

    /// The one node `pred` reads T through: every part of `pred` that reads T, under And/Or
    /// alone, is that node. `None` when T is read elsewhere or through two nodes.
    fn node_in(&self, pred: &Pred, t: &VarSet) -> Option<Node> {
        let mut found: Option<Node> = None;
        self.walk_nodes(pred, t, &mut |node| match &found {
            Some(f) if !f.same(&node) => false,
            Some(_) => true,
            None => {
                found = Some(node);
                true
            }
        })?;
        found
    }

    /// Calls `at` with each node of `pred` under And/Or; `None` when a part that reads T is no
    /// node, or `at` says no.
    fn walk_nodes(&self, pred: &Pred, t: &VarSet, at: &mut dyn FnMut(Node) -> bool) -> Option<()> {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        if r.is_disjoint(t) {
            return Some(());
        }
        if let Some(node) = self.as_node(pred, t) {
            return at(node).then_some(());
        }
        match pred {
            Pred::And(parts) | Pred::Or(parts) => {
                for part in parts {
                    self.walk_nodes(part, t, at)?;
                }
                Some(())
            }
            _ => None,
        }
    }

    /// `pred` with each occurrence of `node` replaced by `stand_in` and every other part
    /// mapped by `rest`, which the parts under And/Or that read no T go through whole.
    fn substitute_node(
        &self,
        pred: &Pred,
        t: &VarSet,
        node: &Node,
        stand_in: &Pred,
        rest: &dyn Fn(&Pred) -> Pred,
    ) -> Pred {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        if r.is_disjoint(t) {
            return rest(pred);
        }
        if let Some(found) = self.as_node(pred, t) {
            assert!(found.same(node), "the guard's nodes agree");
            return stand_in.clone();
        }
        let parts = |xs: &[Pred]| -> Vec<Pred> {
            xs.iter()
                .map(|x| self.substitute_node(x, t, node, stand_in, rest))
                .collect()
        };
        match pred {
            Pred::And(xs) => Pred::And(parts(xs)),
            Pred::Or(xs) => Pred::Or(parts(xs)),
            _ => unreachable!("a node fold's guard reads T through nodes under And/Or alone"),
        }
    }

    /// `pred` as a node: a quantifier over a T entity whose body reads T except link atoms
    /// `v[p] = Index(Bound(0))` (v an S var, p a param), all on one `(v, p)`, and no param
    /// or successor otherwise; or an And/Or of such quantifiers over one entity and link.
    fn as_node(&self, pred: &Pred, t: &VarSet) -> Option<Node> {
        match pred {
            Pred::Exists { entity, body } if t.contains(&self.entity(*entity).exists) => {
                let mut link: Option<(VarId, u8)> = None;
                let mut outside = Vec::new();
                let mut inside = Vec::new();
                let parts: Vec<&Pred> = match &**body {
                    Pred::And(parts) => parts.iter().collect(),
                    other => vec![other],
                };
                for part in parts {
                    let mut r = VarSet::new();
                    self.pred_reads(part, &mut r);
                    if r.is_disjoint(t) && !names_bound(part) {
                        outside.push(part.clone());
                    } else {
                        inside.push(self.strip_links(part, t, &mut link, true)?);
                    }
                }
                let body = match inside.len() {
                    1 => inside.pop().expect("one part"),
                    _ => Pred::And(inside),
                };
                Some(Node {
                    entity: *entity,
                    link: link?,
                    body,
                    outside,
                })
            }
            Pred::And(parts) | Pred::Or(parts) if !parts.is_empty() => {
                let nodes: Vec<Node> = parts
                    .iter()
                    .map(|p| self.as_node(p, t))
                    .collect::<Option<_>>()?;
                let first = &nodes[0];
                if nodes.iter().any(|n| {
                    n.entity != first.entity || n.link != first.link || !n.outside.is_empty()
                }) {
                    return None;
                }
                let bodies: Vec<Pred> = nodes.iter().map(|n| n.body.clone()).collect();
                Some(Node {
                    entity: first.entity,
                    link: first.link,
                    body: match pred {
                        Pred::And(_) => Pred::And(bodies),
                        _ => Pred::Or(bodies),
                    },
                    outside: Vec::new(),
                })
            }
            _ => None,
        }
    }

    /// A node's body with its link atoms replaced by `True`, recording the one link; `None`
    /// when an atom reads S, a param, or the successor, when links differ, or when a link
    /// stands anywhere but as a conjunct of the body (under Or, Not, or a universal the
    /// stand-in `link ∧ bit` would not be the node). The node is under no quantifier, so
    /// its row is `Bound(0)` at every depth of its body.
    fn strip_links(
        &self,
        pred: &Pred,
        t: &VarSet,
        link: &mut Option<(VarId, u8)>,
        conjunctive: bool,
    ) -> Option<Pred> {
        let is_link = |term: &Term, other: &Term| match (term, other) {
            (
                Term::Slot {
                    var,
                    at: At::Param(p),
                },
                Term::Index(At::Bound(0)),
            ) if !t.contains(var) => Some((*var, *p)),
            _ => None,
        };
        match pred {
            Pred::Cmp(x, CmpOp::Eq, y) => {
                if let Some(found) = is_link(x, y).or_else(|| is_link(y, x)) {
                    if !conjunctive || link.get_or_insert(found) != &found {
                        return None;
                    }
                    return Some(Pred::True);
                }
                self.t_atom(pred, t)
            }
            Pred::True | Pred::False | Pred::Cmp(..) | Pred::In(..) => self.t_atom(pred, t),
            Pred::Not(x) => Some(Pred::Not(Box::new(self.strip_links(x, t, link, false)?))),
            Pred::And(xs) => Some(Pred::And(
                xs.iter()
                    .map(|x| self.strip_links(x, t, link, conjunctive))
                    .collect::<Option<_>>()?,
            )),
            Pred::Or(xs) => Some(Pred::Or(
                xs.iter()
                    .map(|x| self.strip_links(x, t, link, false))
                    .collect::<Option<_>>()?,
            )),
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                if !t.contains(&self.entity(*entity).exists) {
                    return None;
                }
                // Under an existential the link is still a conjunct of the node (`∃b1 (L ∧
                // T)` is `L ∧ ∃b1 T` when `L` names no `b1`); under a universal it is not.
                let body = Box::new(self.strip_links(
                    body,
                    t,
                    link,
                    conjunctive && matches!(pred, Pred::Exists { .. }),
                )?);
                Some(match pred {
                    Pred::Forall { .. } => Pred::Forall {
                        entity: *entity,
                        body,
                    },
                    _ => Pred::Exists {
                        entity: *entity,
                        body,
                    },
                })
            }
        }
    }

    /// An atom that reads T alone and names no param or successor, as is.
    fn t_atom(&self, pred: &Pred, t: &VarSet) -> Option<Pred> {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        (r.is_subset(t) && named_params(pred).is_empty()).then(|| pred.clone())
    }

    /// What an S action reads outside its fold: the existence of its other `Existing` and
    /// `Parent` rows, its unfolded guards, its copies, and its transition sources.
    fn raw_reads(&self, id: ActId, fold: Option<&Fold>) -> VarSet {
        let a = &self.actions[id.index()];
        let folded_param = |p: u8| fold.is_some_and(|f| f.t_params.iter().any(|(q, _)| *q == p));
        let folded_guard = |g: usize| fold.is_some_and(|f| f.guards.contains(&g));
        let mut out = VarSet::new();
        for (i, p) in a.params.iter().enumerate() {
            if matches!(p.role, ParamRole::Existing | ParamRole::Parent { .. })
                && !folded_param(i as u8)
            {
                out.insert(self.entity(p.entity).exists);
            }
        }
        for (gi, g) in a.guards.iter().enumerate() {
            if !folded_guard(gi) {
                self.pred_reads(&g.pred, &mut out);
            }
        }
        for eff in &a.effects {
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
        // After S and the raw inputs come the stubs, then the bit vars, whose values a
        // state's `key` computes.
        let first = &ext.system.initial_states()[0].0;
        let bits_width = ext.bits.len() * self.bounds.slots();
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
            return Err(!split.inputs.is_empty() || !split.folds.is_empty());
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
            .class(self, start)
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
                    .project(&State(r.key(self, post).into()), &ext.inputs_local);
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
            let target = block_of_class[&r.class(self, post).expect("a class move enters a class")];
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
                    r.write(&mut next, &r.key(self, &cur));
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
                            // A folded or merged action binds its T params in the cone:
                            // the cone's step is the variant that leads to the same state.
                            let first = ext.acts[a.index()].expect("an S step is the cone's");
                            let step = match r.variants.get(&first) {
                                None => (first, p, next.clone()),
                                Some(variants) => self
                                    .plain_moves(&cur)
                                    .into_iter()
                                    .find(|(a2, _, n2)| variants.contains(a2) && *n2 == next)
                                    .expect("a folded step is a step of the cone"),
                            };
                            group.push(step);
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
                r.moves_from(self, s)
                    .into_iter()
                    .map(|(act, params, next)| super::explore::Move { act, params, next })
                    .collect()
            })
            .unwrap_or_default();
        let inputs = r
            .filter(|r| !r.inputs.is_empty() || !r.bits.is_empty())
            .map(|r| (r, r.input_bytes(self, s)));
        self.raw_moves(s)
            .filter(move |m| r.is_none_or(|r| !r.skips(m.act)))
            .map(move |mut m| {
                if let Some((r, before)) = &inputs {
                    let after = r.input_bytes(self, &m.next);
                    if after != *before {
                        let class = r
                            .class(self, s)
                            .expect("a reduced state holds a representative");
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
