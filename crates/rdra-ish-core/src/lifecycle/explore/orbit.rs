//! A run up to instance symmetry (#212, docs/design/instance-symmetry.md): BFS stores one
//! state per class of states that a permutation of rows maps onto each other, its canonical
//! form, and reports the verdicts a plain run gives.
//!
//! A stored state need not be reachable: the class of a reachable state may hold its
//! permutation that no run reaches (the `FirstAbsent` asymmetry), and that may be the form
//! stored. Every check this run decides has the same value on every member of a class, so
//! the verdicts stand; but nothing prints a stored state as it is. A counterexample is
//! lifted forward to concrete states (`lift`) and replays through the step relation before
//! it is reported.
//!
//! `OrbitRun` is a type of its own, not a `ComponentRun`: it has no `graph()` and no
//! `adjacency()`, so a reader that compares two stored states slot by slot across an edge,
//! or follows one binding along a path, does not compile against it. It exposes what is
//! sound on it: the diagnostics, the lifted traces, the counts, the stored states for state
//! predicates, and the edges' labels.

use super::symmetry::{Canon, Group, LeftOut, Orbits};
use super::{
    Caps, Checks, ComponentRun, Edge, Ends, PropertyVerdict, Reach, RunEnd, States, Threads,
};
use crate::lifecycle::diag::{Diag, StateDiag};
use crate::lifecycle::expr::{State, StateSlice};
use crate::lifecycle::narrow::id32;
use crate::lifecycle::reduce::Step;
use crate::lifecycle::types::{ActId, EntId, Ltl};
use crate::lifecycle::{LassoFault, ReplayError, TransitionSystem};

mod findings;
mod liveness;
#[cfg(feature = "shadow")]
mod reference;

/// Each edge's permutation `σ`, which takes the edge's concrete successor to its stored
/// target (docs/design/instance-symmetry.md, "Soundness per check": the annotated
/// quotient). The permutations are kept once each, and an edge names its own by index.
pub(crate) struct EdgePerms {
    table: Vec<Box<[u8]>>,
    index: crate::fasthash::FastMap<Box<[u8]>, u32>,
    /// Per edge, in the order of the run's edges: its permutation in `table`.
    of: Vec<u32>,
}

impl EdgePerms {
    /// No edge yet; the identity of `g` is permutation 0.
    pub(crate) fn new(g: &Group) -> Self {
        let mut perms = EdgePerms {
            table: Vec::new(),
            index: Default::default(),
            of: Vec::new(),
        };
        perms.intern(&g.identity());
        perms
    }

    fn intern(&mut self, perm: &[u8]) -> u32 {
        if let Some(&id) = self.index.get(perm) {
            return id;
        }
        let id = u32::try_from(self.table.len()).expect("permutations fit in u32");
        self.table.push(perm.into());
        self.index.insert(perm.into(), id);
        id
    }

    /// The next edge's permutation.
    pub(crate) fn push(&mut self, perm: &[u8]) {
        let id = self.intern(perm);
        self.of.push(id);
    }

    /// Keeps the first `edges` permutations: those of the edges a run that a cap stopped
    /// recorded.
    pub(crate) fn truncate(&mut self, edges: usize) {
        self.of.truncate(edges);
    }

    /// The next edge is a concrete self-loop.
    pub(crate) fn push_identity(&mut self) {
        self.of.push(0);
    }

    /// What the permutations hold, in the bytes `Caps::memory` counts: one id per edge, and
    /// each distinct permutation with its index entry.
    pub(crate) fn bytes(&self) -> u64 {
        let cap = self.of.capacity().max(self.of.len() + 1);
        let per = self.table.first().map_or(0, |p| p.len()) as u64;
        (cap * size_of::<u32>()) as u64 + self.table.len() as u64 * (2 * per + 48)
    }

    /// Edge `k`'s permutation, by its id.
    pub(crate) fn id(&self, k: usize) -> u32 {
        self.of[k]
    }

    /// Permutation `id`.
    pub(crate) fn get(&self, id: u32) -> &[u8] {
        &self.table[id as usize]
    }

    /// How many distinct permutations the edges name.
    pub(crate) fn len(&self) -> usize {
        self.table.len()
    }

    /// How many edges have a permutation.
    pub(crate) fn edges(&self) -> usize {
        self.of.len()
    }
}

/// A run of a system up to instance symmetry.
pub struct OrbitRun {
    /// The run over the stored forms. Private: its graph holds representatives, which no
    /// reader outside this file may walk.
    run: ComponentRun,
    group: Group,
    /// The index in the run's edges of each orbit self-edge: a move into the class of its
    /// own source, stored as a loop. Phase 2's divergence pass reads it: a silent one makes
    /// its class loop.
    orbit_self: Vec<u32>,
    orbits: Orbits,
    /// Each edge's permutation, on a run made for a reader that follows a binding along the
    /// edges (`run_annotated_orbits`); `None` otherwise.
    perms: Option<EdgePerms>,
}

/// One edge as the label readers see it: the stored states it joins, its action, and its
/// params read in its source's slots.
#[derive(Clone, Copy, Debug)]
pub struct LabelledEdge<'a> {
    pub from: u32,
    pub to: u32,
    pub act: ActId,
    pub params: &'a [u8],
    /// A move into the class of its own source: `to == from`, but a real step.
    pub orbit_self: bool,
}

impl TransitionSystem {
    /// Explores this system up to instance symmetry under `caps`, evaluating every invariant
    /// and after-assert. On a #74-reduced cone the group is `G_T` (#212 phase 3): it moves
    /// T rows alone and leaves the class representative where it is. The run
    /// gathers no entity view, whatever `caps` asks: an `OrbitRun` has no view to read, and a
    /// view gathered from representatives would miss the patterns of the other states of their
    /// classes. So the pattern cap does not stop it (#251).
    pub fn run_orbits(self, caps: Caps) -> OrbitRun {
        let group = Group::of(&self);
        self.run_orbits_in(caps, Checks::All, group)
    }

    /// `run_orbits` with each edge's permutation kept (the annotated quotient of
    /// docs/design/instance-symmetry.md), so that a reader may follow one binding along the
    /// edges: `OrbitRun::decide_liveness` (#212 phase 4). The permutations are charged to
    /// `caps.memory`, so only such a reader's run keeps them, and every other run up to
    /// symmetry stops where it did.
    pub fn run_annotated_orbits(self, caps: Caps) -> OrbitRun {
        assert!(
            !caps.stop_when_all_violated,
            "a run whose graph is read keeps its edges"
        );
        let group = Group::of(&self);
        self.run_orbits_annotating(caps, Checks::All, group, true)
    }

    /// Whether a run up to instance symmetry could store fewer states than a plain run: its
    /// group (`G_T` on a reduced cone) moves some rows. `verify` runs a cone whose group moves
    /// nothing plainly, so it costs and prints what it does without `--symmetry`
    /// (docs/design/instance-symmetry.md, "Always by default", 2(a)).
    pub fn has_instance_symmetry(&self) -> bool {
        !Group::of(self).is_trivial()
    }

    /// `run_orbits` with the classes of `fixed` left out of the group as well, evaluating
    /// `checks`: the run of the sub-lifecycle of a reduced cone up to `H` (`Group::fixing`,
    /// #212 phase 2), which evaluates none.
    pub(crate) fn run_orbits_fixing(self, caps: Caps, checks: Checks, fixed: &[EntId]) -> OrbitRun {
        let group = Group::fixing(&self, fixed);
        self.run_orbits_in(caps, checks, group)
    }

    fn run_orbits_in(self, caps: Caps, checks: Checks, group: Group) -> OrbitRun {
        self.run_orbits_annotating(caps, checks, group, false)
    }

    fn run_orbits_annotating(
        self,
        caps: Caps,
        checks: Checks,
        group: Group,
        annotate: bool,
    ) -> OrbitRun {
        group.g_t_stats(&self);
        let (mut run, violations, parts) = self.bfs_on(
            caps.without_views(),
            checks,
            Threads::Shared,
            Some(&group),
            annotate,
        );
        crate::shadow_assert!(
            !run.gathered_views() && run.views.iter().all(|v| v.patterns.is_empty()),
            "a run up to symmetry gathers no entity view"
        );
        let mut canon = Canon::default();
        for (at, reach) in &violations {
            let (start, steps) = match reach {
                Reach::State { at, .. } => lift(&run, &group, &mut canon, *at, None),
                Reach::Step {
                    from,
                    act,
                    params,
                    to,
                    ..
                } => lift(
                    &run,
                    &group,
                    &mut canon,
                    *from,
                    Some((*act, &params[..], &**to)),
                ),
            };
            let ts = &run.system;
            if let Err(e) = replays(ts, &start, &steps) {
                let reduced = &run.diags[*at].diag;
                run.diags[*at].diag = super::faulted(reduced, LassoFault::Replay(e));
                continue;
            }
            // On a reduced cone the lifted path still holds class representatives on the S
            // side: #74's lift comes after the symmetry's, which was applied last.
            let (start, steps) = ts.lifted(&start, &steps);
            let last = steps.last().map_or(&start, |s| &s.2);
            // The diagnostic names the binding the concrete last state violates.
            let rebuilt = match reach {
                Reach::State { inv, .. } => ts.invariant_violation(*inv, last),
                Reach::Step { after, .. } => {
                    let (act, params, _) = steps.last().expect("a step");
                    ts.after_violation(*after, *act, params, last)
                }
            };
            // A lifted state that does not violate what its class does is a fault (P52).
            let mut d = super::lifted(rebuilt.map(|d| d.diag), &run.diags[*at].diag);
            if let Some(t) = d.trace_mut() {
                *t = ts.trace_from(&start, &steps);
            }
            run.diags[*at].diag = d;
        }
        let out = OrbitRun {
            run,
            group,
            orbit_self: parts.orbit_self,
            orbits: parts.orbits,
            perms: parts.perms,
        };
        crate::shadow_assert!(
            out.perms
                .as_ref()
                .is_none_or(|p| p.edges() as u64 == out.run.moves()),
            "an annotated run keeps one permutation per edge"
        );
        #[cfg(feature = "shadow")]
        reference::check(&out, caps);
        out
    }
}

/// The stored path to `states[at]`, then `extra` (a step from `states[at]` into a concrete
/// successor), lifted to concrete states (docs/design/instance-symmetry.md,
/// "Counterexamples"). Each stored edge is recomputed from its source as the first move of
/// its action and params whose successor `t` has the edge's target as canonical form, and
/// `σ` takes `t` there; the concrete step is the move under `τ` of the params into `τ(t)`,
/// and `τ` becomes `τ ∘ σ⁻¹`. The first stored state is an initial state, since the initial
/// states are closed under the group.
fn lift(
    run: &ComponentRun,
    g: &Group,
    canon: &mut Canon,
    at: u32,
    extra: Option<(ActId, &[u8], &StateSlice)>,
) -> (State, Vec<Step>) {
    let path: Vec<Edge> = run.path_to(at);
    let first = path.first().map_or(at, |e| e.from);
    let (start, mut steps, tau) = lift_path(run, g, canon, first, &path);
    if let Some((act, params, to)) = extra {
        steps.push(concrete(&run.system, g, &tau, act, params, to));
    }
    (start, steps)
}

/// `path`, stored edges from `states[first]` on, lifted to concrete states as `lift` does,
/// with the permutation `τ` the last concrete state is read through.
fn lift_path(
    run: &ComponentRun,
    g: &Group,
    canon: &mut Canon,
    first: u32,
    path: &[Edge],
) -> (State, Vec<Step>, Vec<u8>) {
    let ts = &run.system;
    let start = run.states.at(first as usize).into_state();
    let mut tau = g.identity();
    let mut out = vec![0u8; ts.width];
    let mut steps = Vec::with_capacity(path.len() + 1);
    for e in path {
        let (act, params) = run.step(e);
        let target = &run.states.at(e.to as usize);
        let t = ts
            .moves(&run.states.at(e.from as usize))
            .find(|m| {
                m.act == act && *m.params == *params && {
                    canon.canonical(g, &m.next.0, &mut out);
                    out[..] == target.0
                }
            })
            .expect("symmetry: a stored edge is a move of its source");
        let sigma = canon.perm().to_vec();
        steps.push(concrete(ts, g, &tau, act, params, &t.next));
        tau = g.compose(&tau, &g.inverse(&sigma));
    }
    (start, steps, tau)
}

/// The step of `act` under `params` into `t`, read through `tau`. A class move of a reduced
/// cone keeps its params: they may bind the extended sub-lifecycle's action rather than the
/// cone's, and every row they name is one `G_T` leaves where it is (`Reduction::fixed_why`).
fn concrete(
    ts: &TransitionSystem,
    g: &Group,
    tau: &[u8],
    act: ActId,
    params: &[u8],
    t: &StateSlice,
) -> Step {
    let action = &ts.actions()[act.index()];
    let params: Box<[u8]> = match ts.reduction().is_some_and(|r| r.skips(act)) {
        true => params.into(),
        false => action
            .params
            .iter()
            .zip(params)
            .map(|(p, &slot)| g.map_slot(tau, p.entity, slot))
            .collect(),
    };
    let mut next = vec![0u8; t.0.len()];
    g.apply(tau, &t.0, &mut next);
    (act, params, State(next.into_boxed_slice()))
}

/// A lifted trace replays through the step relation (agreement rule 3): one that does not
/// is a tool bug, never a finding about the model, and is reported as a fault (P20). A step
/// that replays but binds other rows than it names fails at that step.
pub(super) fn replays(
    ts: &TransitionSystem,
    start: &State,
    steps: &[Step],
) -> Result<(), ReplayError> {
    let states: Vec<State> = std::iter::once(start.clone())
        .chain(steps.iter().map(|s| s.2.clone()))
        .collect();
    let actions: Vec<ActId> = steps.iter().map(|s| s.0).collect();
    ts.replay(states, &actions)?;
    // `replay` reads the actions only; the printed trace also names each step's rows.
    let mut at = start;
    for (i, (act, params, next)) in steps.iter().enumerate() {
        let found = ts
            .moves(at)
            .any(|m| m.act == *act && *m.params == **params && m.next.0 == next.0);
        if !found {
            return Err(ReplayError::NoMove {
                state: i + 2,
                action: *act,
            });
        }
        at = next;
    }
    Ok(())
}

/// `replays`, for a path that only feeds a finding's trace: one that does not replay stops
/// the run as a tool bug.
fn replays_or_panic(ts: &TransitionSystem, start: &State, steps: &[Step]) {
    if let Err(e) = replays(ts, start, steps) {
        panic!("symmetry: a lifted path does not replay ({e:?}); a tool bug");
    }
}

impl OrbitRun {
    /// The stored graph, for `Classes::of` and the lift of a reduced trace alone (#212 phase
    /// 2): a run up to `H` keeps what the quotient reads on every edge, so its stuttering
    /// partition is the plain one's, class for class. No other reader walks it.
    pub(crate) fn quotient_graph(&self) -> &ComponentRun {
        &self.run
    }

    /// Whether the run's group moves the rows of `e`.
    pub(crate) fn moves(&self, e: EntId) -> bool {
        self.group.moves(e)
    }

    /// The canonical form of `s`, a state of the system, under the run's group.
    pub(crate) fn canonical(&self, s: &StateSlice) -> State {
        let mut out = vec![0u8; s.0.len()];
        Canon::default().canonical(&self.group, &s.0, &mut out);
        State(out.into_boxed_slice())
    }

    /// The stored edges `path` from `states[first]`, lifted to concrete states.
    pub(crate) fn lift_path(&self, first: u32, path: &[Edge]) -> (State, Vec<Step>) {
        let (start, steps, _) =
            lift_path(&self.run, &self.group, &mut Canon::default(), first, path);
        replays_or_panic(&self.run.system, &start, &steps);
        (start, steps)
    }

    /// The system explored; the stored states are its states.
    pub fn system(&self) -> &TransitionSystem {
        &self.run.system
    }

    /// The stored states, one per class, in discovery order: forms for state predicates
    /// that read every row, never states to print.
    pub fn states(&self) -> &States {
        &self.run.states
    }

    pub fn diags(&self) -> &[Diag] {
        &self.run.diags
    }

    /// The checks the run evaluated: every invariant and after-assert.
    pub fn checks(&self) -> &Checks {
        self.run.checks()
    }

    /// False when the run stopped before it reached every class.
    pub fn complete(&self) -> bool {
        self.run.complete
    }

    pub fn end(&self) -> RunEnd {
        self.run.end()
    }

    /// The classes stored.
    pub fn orbits(&self) -> usize {
        self.run.states.len()
    }

    /// The states of the plain run the stored classes stand for, `|G_r| / |Aut(r)|` each:
    /// an estimate, high where some members of a class are not reachable.
    pub fn estimate(&self) -> u128 {
        self.orbits.states
    }

    /// The stored states `BRANCH_CAP` cut, which may not be their class's least form: the
    /// orbit count is then an upper bound.
    pub fn cut(&self) -> u64 {
        self.orbits.cut
    }

    /// The classes of entities the group left out, each with why.
    pub fn left_out(&self) -> &[LeftOut] {
        self.group.left_out()
    }

    /// What the result keeps, in the bytes `Caps::memory` counts.
    pub fn kept_bytes(&self) -> u64 {
        self.run.kept_bytes()
    }

    /// The indices among `labelled_edges` of the orbit self-edges.
    pub fn orbit_self_edges(&self) -> &[u32] {
        &self.orbit_self
    }

    /// Every edge, in discovery order, as its labels: sound for a reader of which actions
    /// label an edge and of state predicates of its ends.
    /// What the edges `act` labels reach, orbit self-edges included: those of the stored
    /// graph (`ComponentRun::step_ends`).
    pub fn step_ends(&self, act: ActId, binders: &[EntId]) -> Ends {
        self.run.step_ends_with(act, binders, &self.orbit_self)
    }

    pub fn labelled_edges(&self) -> impl Iterator<Item = LabelledEdge<'_>> {
        let mut orbit_self = self.orbit_self.iter().peekable();
        self.run.edges().iter().enumerate().map(move |(k, e)| {
            let (act, params) = self.run.step(&e);
            let is_orbit = orbit_self.next_if(|&&o| o as usize == k).is_some();
            LabelledEdge {
                from: e.from,
                to: e.to,
                act,
                params,
                orbit_self: is_orbit,
            }
        })
    }

    /// `always` property `i` of the system on the stored states: the first, in the order BFS
    /// found them, where the trigger holds and the body fails for some binding, with the
    /// lifted path to it. `None` for any other form, which a run up to symmetry does not
    /// decide in this phase.
    pub fn decide_always(&self, i: usize) -> Option<Result<PropertyVerdict, LassoFault>> {
        let ts = self.system();
        let p = &ts.properties()[i];
        let Ltl::Always(x) = &p.formula else {
            return None;
        };
        let bindings: Vec<Vec<u8>> = ts.binder_slots(&p.binders).collect();
        let found = self.run.states.iter().position(|s| {
            bindings
                .iter()
                .any(|bd| ts.triggers(p, bd, &s) && !ts.holds_bound(x, &s, bd))
        });
        let Some(at) = found else {
            return Some(Ok(match self.run.complete {
                true => PropertyVerdict::Holds,
                false => PropertyVerdict::Undecided,
            }));
        };
        let (start, steps) = lift(
            &self.run,
            &self.group,
            &mut Canon::default(),
            id32(at),
            None,
        );
        if let Err(e) = replays(ts, &start, &steps) {
            return Some(Err(LassoFault::Replay(e)));
        }
        let states: Vec<State> = std::iter::once(start.clone())
            .chain(steps.iter().map(|s| s.2.clone()))
            .collect();
        // A lifted path that does not break the property its class breaks is a fault.
        let Some((instance, _)) = ts.property_failure(p, &states, states.len() - 1) else {
            return Some(Err(LassoFault::Satisfies));
        };
        let trace = ts.trace_from(&start, &steps);
        Some(Ok(PropertyVerdict::Violated(Box::new(
            StateDiag::PropertyViolated {
                name: p.name.clone(),
                kind: p.formula.kind(),
                instance: p
                    .binders
                    .iter()
                    .zip(&instance)
                    .map(|(e, slot)| ts.inst(*e, usize::from(*slot)))
                    .collect(),
                witness: trace.steps.len(),
                lasso: None,
                site: p.site.clone(),
                trace,
            },
        ))))
    }
}
