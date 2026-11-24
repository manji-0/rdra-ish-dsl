//! The quotient of the extended sub-lifecycle: stuttering partition refinement.
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use super::cache::QuotientCache;
use super::extended::Extended;
use super::*;
use crate::fasthash::FastMap;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::{Adjacency, Caps, Checks, ComponentRun, OrbitRun};
use crate::lifecycle::expr::State;
use crate::lifecycle::types::{ActId, Domain, EntId, VarId};
use crate::lifecycle::{StateDiag, TransitionSystem};
use crate::memory::Watched;

#[cfg(any(test, feature = "shadow"))]
mod reference;
#[cfg(any(test, feature = "shadow"))]
mod up_to_h;

/// What the quotient of an observed S comes to (`Classes::of`).
#[derive(Clone, PartialEq)]
pub(super) enum Classified {
    /// A run stopped at a cap or a limit, or the quotient is the identity on an S that no
    /// inner reduction shrank (#101): it reduces nothing.
    Nothing,
    /// More classes than the tables of any cone it would reduce may print for TLC
    /// (`TLA_CELL_BUDGET`): a cone prints at least one cell per class and observed var.
    TooLarge,
    Classes(Arc<Classes>),
}

/// How the runs of one `Classes::of` ended, for `QuotientCache`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ended {
    /// No cap and no limit stopped a run.
    Clean,
    /// The run of S stopped at the memory limit, refused a request past the room of the pool
    /// it charged, after a nested reduction that no cap or limit stopped. In a pool of no
    /// more room, with room for every request the runs were granted, the runs go the same way
    /// and stop at the same request (`QuotientCache`).
    Declined,
    /// Another stop: a state or pattern cap, a deadline, a crowded pool, or a nested run that
    /// stopped.
    Stopped,
}

impl Ended {
    /// What `Classes::of` reports as its flag: whether no cap and no limit stopped a run.
    pub(super) fn is_clean(self) -> bool {
        self == Ended::Clean
    }

    /// How a computation ended whose run of S did not complete. `nested` says whether the
    /// nested reduction was clean, `seen` is what the run met, and `run` the run.
    fn stopped(nested: bool, seen: Watched, run: &ComponentRun) -> Ended {
        let memory = run
            .diags
            .iter()
            .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }));
        // A refusal within the pool's room is a crowded one: what others held decided it.
        match nested && memory && !seen.crowded {
            true => Ended::Declined,
            false => Ended::Stopped,
        }
    }
}

/// Why `Quotient::of` gives no quotient.
pub(super) enum NoQuotient {
    Nothing,
    /// `Classified::TooLarge`.
    TooLarge,
}

/// The classes of S, with the initial states' and the moves' classes, in the cone's ids.
pub(super) struct Quotient {
    pub(super) classes: Arc<Classes>,
    /// Per class, its moves with the cone's action and params.
    pub(super) moves: Vec<Vec<ClassMove>>,
}

/// What a quotient holds that does not depend on the cone it reduces: the classes, and each
/// class move with the extended sub-lifecycle's action and params. It is a function of the
/// extended sub-lifecycle, its `obs`, inputs and rises, and the caps, so cones whose
/// extended sub-lifecycles are equal share it (`QuotientCache`).
#[derive(PartialEq)]
pub(super) struct Classes {
    pub(super) reps: Vec<Box<[u8]>>,
    pub(super) class_of: HashMap<Box<[u8]>, u32>,
    pub(super) init: HashMap<Box<[u8]>, u32>,
    /// Per class, its moves in the extended sub-lifecycle's ids, each `(class, action,
    /// params, class)` once, in the order the run's edges first show it.
    pub(super) moves: Vec<Vec<ClassMove>>,
    pub(super) rises: BTreeMap<(u32, Box<[u8]>), u32>,
    pub(super) inner: Option<Arc<Reduction>>,
    pub(super) states: usize,
    /// Per class, whether it can loop inside itself (`can_loop`).
    pub(super) loops: Vec<bool>,
    pub(super) divergence: Divergence,
    /// The run of S was up to `H` (#212 phase 2): the representatives are stored forms, and
    /// a lift runs S up to `H` again to find its path.
    pub(super) up_to_h: bool,
}

/// What the divergence marks of one reduction say, for measurement: how many classes can
/// loop inside themselves, how many hold states that can and states that cannot, and, when
/// `RDRA_ISH_DIVERGENCE_STATS` is set, the classes a partition that tells the two apart
/// would have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Divergence {
    pub classes: usize,
    pub looping: usize,
    pub mixed: usize,
    pub sensitive: Option<usize>,
}

/// The partition of a run of the extended sub-lifecycle: the coarsest one that refines
/// `initial` (the states' `obs` and inputs), is divergence-blind stuttering stable under the
/// silent edges, and is strongly stable under the rise edges: two states of a block reach the
/// same blocks by a rise. The blocks refine the inputs, so the block a rise enters names the
/// inputs it set, and the second condition says that two equivalent states stay equivalent
/// under every rise. Returns each state's block.
pub(super) fn quotient_blocks(
    n: usize,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
) -> Vec<u32> {
    let stutter = Stutter::new(n, silent);
    let mut block = stutter.blocks(initial);
    if rises.is_empty() {
        return block;
    }
    let by_source = Adj::new(n, rises);
    // Each state's signature: the blocks its rises enter, ascending, end to end.
    let mut signatures: Vec<u32> = Vec::with_capacity(rises.len());
    let mut ends: Vec<usize> = Vec::with_capacity(n);
    loop {
        signatures.clear();
        ends.clear();
        for x in 0..n as u32 {
            let start = signatures.len();
            signatures.extend(by_source.of(x).iter().map(|y| block[*y as usize]));
            signatures[start..].sort_unstable();
            let mut kept = start;
            for k in start..signatures.len() {
                if k == start || signatures[k] != signatures[kept - 1] {
                    signatures[kept] = signatures[k];
                    kept += 1;
                }
            }
            signatures.truncate(kept);
            ends.push(kept);
        }
        let mut index: FastMap<(u32, &[u32]), u32> = FastMap::default();
        let refined: Vec<u32> = (0..n)
            .map(|i| {
                let from = i.checked_sub(1).map_or(0, |i| ends[i]);
                let len = index.len() as u32;
                *index
                    .entry((block[i], &signatures[from..ends[i]]))
                    .or_insert(len)
            })
            .collect();
        if index.len() == distinct(&block) {
            return block;
        }
        block = stutter.blocks(&refined);
    }
}

/// How many distinct values `block` holds.
fn distinct(block: &[u32]) -> usize {
    let mut seen = vec![false; block.iter().max().map_or(0, |b| *b as usize + 1)];
    block.iter().for_each(|b| seen[*b as usize] = true);
    seen.iter().filter(|s| **s).count()
}

impl Quotient {
    /// The quotient of the extended sub-lifecycle `ext` by stuttering equivalence with
    /// respect to `obs` and the inputs; `NoQuotient::Nothing` when it does not fit under
    /// `caps`, or when the quotient is the identity on an S that no inner reduction shrank,
    /// which would reduce nothing (#101), and `NoQuotient::TooLarge` when it has more classes
    /// than the cone's TLA+ tables may print. An unobserved S is one class, held at an initial S state, with no
    /// moves: nothing on T reads it, so no path through it is told from staying put, whatever
    /// its inputs do. The flag says whether no cap and no limit stopped a run on the way, so
    /// that the result is what the same call computes under any memory room it fits in.
    pub(super) fn of(
        ext: &Extended,
        cone: &TransitionSystem,
        caps: Caps,
        cache: &QuotientCache,
    ) -> (Result<Self, NoQuotient>, bool) {
        if ext.obs_local.is_empty() {
            let classes = Arc::new(Classes::unobserved(ext));
            let moves = vec![Vec::new()];
            return (Ok(Quotient { classes, moves }), true);
        }
        let (classified, clean) = cache.classes(ext, caps, |cache| Classes::of(ext, caps, cache));
        let clean = clean.is_clean();
        let classes = match classified {
            Classified::Nothing => return (Err(NoQuotient::Nothing), clean),
            Classified::TooLarge => return (Err(NoQuotient::TooLarge), clean),
            Classified::Classes(classes) => classes,
        };
        {
            // A class move in the cone's ids. Two of the extended sub-lifecycle's can be one
            // of the cone's, which is kept once, where it first shows.
            let mut seen: BTreeSet<(u32, ActId, Box<[u8]>, u32)> = BTreeSet::new();
            let moves = classes
                .moves
                .iter()
                .enumerate()
                .map(|(x, from)| {
                    from.iter()
                        .filter_map(|(act, params, y)| {
                            let act = ext.acts[act.index()]
                                .expect("a silent step is an S action of the cone");
                            // The BFS of the cone reads a class move's params by the cone
                            // action's params.
                            let params = ext.cone_params(cone, act, params);
                            seen.insert((x as u32, act, params.clone(), *y))
                                .then_some((act, params, *y))
                        })
                        .collect()
                })
                .collect();
            (Ok(Quotient { classes, moves }), clean)
        }
    }
}

impl Classes {
    /// The one class of an unobserved S (`Quotient::of`).
    fn unobserved(ext: &Extended) -> Self {
        let initial = ext.system.initial_states();
        let rep = initial.first().expect("an initial state").0.clone();
        Classes {
            class_of: HashMap::from([(rep.clone(), 0)]),
            reps: vec![rep],
            init: initial.iter().map(|st| (st.0.clone(), 0)).collect(),
            moves: vec![Vec::new()],
            rises: BTreeMap::new(),
            inner: None,
            states: initial.len(),
            loops: vec![s_cycles(ext)],
            divergence: Divergence {
                classes: 1,
                looping: usize::from(s_cycles(ext)),
                ..Divergence::default()
            },
            up_to_h: false,
        }
    }

    /// The classes of an observed S (`Quotient::of`), and how the runs on the way ended.
    pub(super) fn of(ext: &Extended, caps: Caps, cache: &QuotientCache) -> (Classified, Ended) {
        let raw = &ext.system;
        // The inputs stay on the nested T side: the rises are kept, and only `obs` anchors.
        let (sub, clean) =
            raw.reduced_in(&[(ext.obs_local.clone(), ext.rise_actions())], caps, cache);
        // A test or `shadow` build may force S up to `H` at once (`Caps::force_s_up_to_h`). A
        // run up to `H` never goes through `QuotientCache::run_of_s`, which holds plain runs
        // alone: neither this path nor the retry below reads a held run as one up to `H`, and
        // a run up to `H` is never held for a plain request.
        if forced(caps) && sub.reduction().is_none() {
            let ended = match clean {
                true => Ended::Clean,
                false => Ended::Stopped,
            };
            return Self::up_to_h(ext, caps, &sub, None, ended);
        }
        // The quotient reads the graph alone, so the run evaluates no check. Another key over
        // the same sub-lifecycle may have run it already (`QuotientCache::run_of_s`).
        let (run, seen) = cache.run_of_s(&sub, caps, || {
            let _label = crate::perf::label("S");
            sub.clone()
                .run_checking(caps.without_views(), Checks::none())
        });
        if !run.complete {
            let ended = Ended::stopped(clean, seen, &run);
            let memory = run
                .diags
                .iter()
                .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }));
            // A run the pool refused because others held it is no reason: alone it may fit.
            let alone = memory && !seen.crowded;
            if !(caps.s_up_to_h && alone && sub.reduction().is_none()) {
                return (Classified::Nothing, ended);
            }
            let stopped_at = Some(run.states.len());
            drop(run);
            return Self::up_to_h(ext, caps, &sub, stopped_at, Ended::Stopped);
        }
        let ended = match clean {
            true => Ended::Clean,
            false => Ended::Stopped,
        };
        let (classified, ended, _) = Self::of_graph(ext, &sub, &run, ended, None);
        (classified, ended)
    }

    /// The classes of S up to `H` (#212 phase 2, docs/design/instance-symmetry.md,
    /// "Composition with #74"), after its plain run stopped at the memory limit after
    /// `stopped_at` states under `--symmetry retry` or `always`, or at once when a test forces
    /// it. `H` leaves
    /// `ext.fixed` where they are, so every element keeps `obs` and the inputs: the run's
    /// orbits are a label-preserving bisimulation, finer than the stuttering partition, which
    /// therefore comes out of the stored graph class for class. The classes are the same sets
    /// of S states as a plain run's; their numbering and representatives are others. After a
    /// retry the result depends on the memory room, so `ended` is then `Stopped` and it is
    /// never cached.
    fn up_to_h(
        ext: &Extended,
        caps: Caps,
        sub: &TransitionSystem,
        stopped_at: Option<usize>,
        ended: Ended,
    ) -> (Classified, Ended) {
        let orbits = {
            let _label = crate::perf::label("S up to H");
            sub.clone()
                .run_orbits_fixing(caps, Checks::none(), &ext.fixed)
        };
        // `H` keeps every var the quotient reads where it is: the entity that holds it, and
        // for a key the entity it names, stay put.
        for v in ext.obs_local.iter().chain(&ext.inputs_local) {
            let d = sub.var(*v);
            let named = match d.domain {
                Domain::Instance { parent, .. } => Some(parent),
                _ => None,
            };
            for e in d.kind.entity().into_iter().chain(named) {
                assert!(
                    !orbits.moves(e),
                    "symmetry: H moves the rows of {}, which the quotient reads",
                    sub.entity(e).id
                );
            }
        }
        let stats = std::env::var_os("RDRA_ISH_S_UP_TO_H_STATS").is_some();
        if stats {
            let plain = stopped_at.map_or(String::from("no plain run"), |n| {
                format!("plain stopped at {n} states")
            });
            let left: Vec<String> = (orbits.left_out().iter())
                .map(|l| format!("{} ({:?})", l.entity, l.rule))
                .collect();
            let moving: Vec<&str> = (0..sub.entities().len())
                .map(EntId::at)
                .filter(|e| orbits.moves(*e))
                .map(|e| sub.entity(e).id.as_str())
                .collect();
            eprintln!(
                "s-up-to-h: {plain}; {} orbits (about {} states){}; left out: {}; moving: {}",
                orbits.orbits(),
                orbits.estimate(),
                if orbits.complete() { "" } else { ", stopped" },
                left.join(", "),
                moving.join(", ")
            );
        }
        if !orbits.complete() {
            return (Classified::Nothing, Ended::Stopped);
        }
        let (classified, ended, _class) =
            Self::of_graph(ext, sub, orbits.quotient_graph(), ended, Some(&orbits));
        #[cfg(any(test, feature = "shadow"))]
        if let Classified::Classes(c) = &classified {
            up_to_h::check(ext, sub, caps, &orbits, c, &_class);
        }
        let out = (classified, ended);
        if stats {
            if let (Classified::Classes(c), _) = &out {
                eprintln!("s-up-to-h: {} classes", c.reps.len());
            }
        }
        out
    }

    /// The classes read off `run`, a complete run of `sub`, the extended sub-lifecycle under
    /// its own reduction, with each of its states' class when there are classes; `orbits` is
    /// the run up to `H` whose graph `run` is, when it is one.
    fn of_graph(
        ext: &Extended,
        sub: &TransitionSystem,
        run: &ComponentRun,
        ended: Ended,
        orbits: Option<&OrbitRun>,
    ) -> (Classified, Ended, Vec<u32>) {
        let raw = &ext.system;
        let n = run.states.len();
        let key_vars: VarSet = ext.obs_local.union(&ext.inputs_local).copied().collect();
        let mut index: HashMap<State, u32> = HashMap::new();
        let initial: Vec<u32> = run
            .states
            .iter()
            .map(|st| {
                let key = sub.project(st, &key_vars);
                let len = index.len() as u32;
                *index.entry(key).or_insert(len)
            })
            .collect();
        let (silent, rise_edges) = split_edges(run, &ext.rise);
        let blocks = quotient_blocks(n, &silent, &rise_edges, &initial);
        #[cfg(any(test, feature = "shadow"))]
        reference::check(n, &silent, &rise_edges, &initial, &blocks);
        // The identity on an S no inner reduction shrank reduces nothing (#101). Up to `H` a
        // class is a union of orbits: it is the identity only when every orbit is one state.
        let trivial = orbits.is_none_or(|o| o.estimate() == n as u128);
        if trivial
            && !sub.reduction().is_some_and(|r| r.shrinks())
            && blocks.iter().collect::<BTreeSet<_>>().len() == n
        {
            return (Classified::Nothing, ended, Vec::new());
        }
        // Each block's class, numbered as the blocks first show in discovery order, so a
        // class's representative is its block's first state.
        let mut class_of_block: Vec<u32> =
            vec![u32::MAX; blocks.iter().max().map_or(0, |b| *b as usize + 1)];
        let mut reps: Vec<Box<[u8]>> = Vec::new();
        let mut class_of: HashMap<Box<[u8]>, u32> = HashMap::new();
        let mut class: Vec<u32> = Vec::with_capacity(n);
        for (i, st) in run.states.iter().enumerate() {
            let c = &mut class_of_block[blocks[i] as usize];
            if *c == u32::MAX {
                *c = reps.len() as u32;
                reps.push(st.0.into());
                class_of.insert(st.0.into(), *c);
            }
            class.push(*c);
        }
        // Every cone this quotient would reduce prints a cell per class and observed var at
        // the least, so past the budget no cone takes it: its moves and divergence marks are
        // not worth building.
        if reps.len().saturating_mul(ext.obs_local.len()) > TLA_CELL_BUDGET {
            return (Classified::TooLarge, ended, Vec::new());
        }
        // Initial S states, as the cone's initial states project them, to their classes. The
        // extended sub-lifecycle canonicalizes them for its own reduction first, as the run
        // did.
        let mut init: HashMap<Box<[u8]>, u32> = HashMap::new();
        for st in raw.initial_states() {
            let canonical = match (sub.reduction(), orbits) {
                (Some(inner), _) => inner
                    .canonical_initial(sub, &st)
                    .expect("an initial S state canonicalizes"),
                (None, Some(orbits)) => orbits.canonical(&st),
                (None, None) => st.clone(),
            };
            let at = run
                .states
                .iter()
                .position(|x| *x == canonical)
                .expect("an initial S state is reached");
            init.insert(st.0.clone(), class[at]);
        }
        let mut moves: Vec<Vec<ClassMove>> = vec![Vec::new(); reps.len()];
        let mut rises: BTreeMap<(u32, Box<[u8]>), u32> = BTreeMap::new();
        let mut seen: BTreeSet<(u32, ActId, &[u8], u32)> = BTreeSet::new();
        for e in run.edges() {
            let (x, y) = (e.from as usize, e.to as usize);
            let (act, params) = run.step(e);
            if ext.rise[act.index()] {
                // Every state of a block rises into the same blocks; the representative's
                // edges are enough.
                if *reps[class[x] as usize] == run.states[x].0 {
                    let inputs = sub.project(&run.states[y], &ext.inputs_local).0;
                    let was = rises.insert((class[x], inputs), class[y]);
                    assert!(was.is_none_or(|c| c == class[y]), "a rise is a function");
                }
                continue;
            }
            if class[x] == class[y] {
                continue;
            }
            if seen.insert((class[x], act, params, class[y])) {
                moves[class[x] as usize].push((act, params.into(), class[y]));
            }
        }
        // A silent orbit self-edge stands for a concrete cycle inside its class: a move into
        // another state of its own orbit, repeated, comes back.
        let orbit_loops: Vec<u32> = match orbits {
            Some(orbits) => (orbits.labelled_edges())
                .filter(|e| e.orbit_self && !ext.rise[e.act.index()])
                .map(|e| e.from)
                .collect(),
            None => Vec::new(),
        };
        let (loops, divergence) = match s_cycles(ext) {
            true => divergence(
                run,
                sub,
                &silent,
                &rise_edges,
                &initial,
                &class,
                &orbit_loops,
            ),
            false => {
                let classes = reps.len();
                // Nothing loops, so a partition that tells divergence apart is this one.
                let d = Divergence {
                    classes,
                    sensitive: std::env::var_os("RDRA_ISH_DIVERGENCE_STATS").map(|_| classes),
                    ..Divergence::default()
                };
                (vec![false; classes], d)
            }
        };
        let classes = Classes {
            reps,
            class_of,
            init,
            moves,
            rises,
            inner: sub.reduction.clone(),
            states: n,
            loops,
            divergence,
            up_to_h: orbits.is_some(),
        };
        (Classified::Classes(Arc::new(classes)), ended, class)
    }
}

/// Whether S runs up to `H` at once: a test-only switch (`Caps::force_s_up_to_h`, or
/// `RDRA_ISH_FORCE_S_UP_TO_H` in a `shadow` build) that lets the samples at small bounds,
/// whose S never stops at the memory limit, reach the path and its twin. A release build
/// never forces it.
fn forced(caps: Caps) -> bool {
    #[cfg(feature = "shadow")]
    if caps.s_up_to_h && std::env::var_os("RDRA_ISH_FORCE_S_UP_TO_H").is_some() {
        return true;
    }
    caps.force_s_up_to_h
}

/// Whether some S var is not monotone. When every S var is monotone, each changes finitely
/// often, so no path of S steps is infinite and no class can loop; an unobserved S, one
/// class that is never explored, can loop only otherwise.
fn s_cycles(ext: &Extended) -> bool {
    let monotone = ext.system.monotone_vars();
    (0..monotone.len()).any(|v| !monotone[v] && !ext.inputs_local.contains(&VarId::at(v)))
}

/// Which classes can loop inside themselves (docs/formal-verification.md, "WF_vars(Next) on
/// a reduced cone"): a class holds a cycle of silent edges between its states, or a state
/// whose inner class can loop. One depth-first pass over the silent edges, which keeps a
/// colour per state and the edges by source. Under `RDRA_ISH_DIVERGENCE_STATS` it also counts
/// the classes that hold both states that can reach such a loop inside the class and states
/// that cannot, and estimates the classes of a partition that tells them apart.
fn divergence(
    run: &crate::lifecycle::explore::ComponentRun,
    sub: &TransitionSystem,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
    class: &[u32],
    orbit_loops: &[u32],
) -> (Vec<bool>, Divergence) {
    let n = run.states.len();
    let classes = class.iter().max().map_or(0, |c| *c as usize + 1);
    let mut loops = vec![false; classes];
    let inner = sub
        .reduction()
        .filter(|inner| inner.loops.iter().any(|l| *l));
    let inner_loops = |i: usize| {
        inner.is_some_and(|inner| {
            inner
                .class(sub, &run.states[i])
                .is_some_and(|c| inner.can_loop(c))
        })
    };
    for i in 0..n {
        if !loops[class[i] as usize] && inner_loops(i) {
            loops[class[i] as usize] = true;
        }
    }
    for &x in orbit_loops {
        loops[class[x as usize] as usize] = true;
    }
    let succ = Adj::new(n, silent);
    let inside = |x: u32, y: u32| x != y && class[x as usize] == class[y as usize];
    // White 0, on the stack 1, done 2: an edge to a state on the stack closes a cycle.
    let mut colour = vec![0u8; n];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for root in 0..n as u32 {
        if colour[root as usize] != 0 {
            continue;
        }
        colour[root as usize] = 1;
        stack.push((root, succ.start[root as usize]));
        while let Some(top) = stack.last_mut() {
            let (u, k) = *top;
            if k == succ.start[u as usize + 1] {
                colour[u as usize] = 2;
                stack.pop();
                continue;
            }
            top.1 += 1;
            let v = succ.list[k as usize];
            if !inside(u, v) {
                continue;
            }
            match colour[v as usize] {
                0 => {
                    colour[v as usize] = 1;
                    stack.push((v, succ.start[v as usize]));
                }
                1 => loops[class[v as usize] as usize] = true,
                _ => {}
            }
        }
    }
    let looping = loops.iter().filter(|l| **l).count();
    let mut divergence = Divergence {
        classes,
        looping,
        ..Divergence::default()
    };
    if std::env::var_os("RDRA_ISH_DIVERGENCE_STATS").is_some() {
        divergence.mixed = mixed_classes(n, silent, class, &inner_loops);
        divergence.sensitive = Some(sensitive_classes(n, silent, rises, initial, &inner_loops));
    }
    (loops, divergence)
}

/// The states on a cycle of silent edges inside one block of `block`, or that hold an inner
/// class that can loop; and the in-block edges.
fn on_loop(
    n: usize,
    silent: &[(u32, u32)],
    block: &[u32],
    inner_loops: &dyn Fn(usize) -> bool,
) -> (Vec<bool>, Adjacency) {
    let inside = || {
        silent
            .iter()
            .copied()
            .filter(|&(x, y)| block[x as usize] == block[y as usize])
    };
    let adjacency = Adjacency::new(n, inside);
    let (comp, count) = adjacency.components();
    let mut size = vec![0u32; count];
    comp.iter().for_each(|c| size[*c as usize] += 1);
    let seeds = (0..n)
        .map(|i| size[comp[i] as usize] >= 2 || inner_loops(i))
        .collect();
    (seeds, adjacency)
}

/// The classes that can loop and hold a state no silent path inside the class takes to a
/// loop.
fn mixed_classes(
    n: usize,
    silent: &[(u32, u32)],
    class: &[u32],
    inner_loops: &dyn Fn(usize) -> bool,
) -> usize {
    let classes = class.iter().max().map_or(0, |c| *c as usize + 1);
    let (mut can, adjacency) = on_loop(n, silent, class, inner_loops);
    let mut loops = vec![false; classes];
    let mut queue: VecDeque<u32> = (0..n as u32).filter(|&i| can[i as usize]).collect();
    while let Some(at) = queue.pop_front() {
        loops[class[at as usize] as usize] = true;
        for &p in adjacency.preds(at) {
            if !can[p as usize] {
                can[p as usize] = true;
                queue.push_back(p);
            }
        }
    }
    let mut some_cannot = vec![false; classes];
    for i in 0..n {
        some_cannot[class[i] as usize] |= !can[i];
    }
    (0..classes).filter(|&c| loops[c] && some_cannot[c]).count()
}

/// The classes of a partition that tells divergence apart, estimated: every state on a
/// silent cycle inside its `obs` and inputs block steps to a fresh sink, which no other state
/// is like, and the partition is computed again.
fn sensitive_classes(
    n: usize,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
    inner_loops: &dyn Fn(usize) -> bool,
) -> usize {
    let (on, _) = on_loop(n, silent, initial, inner_loops);
    let sink = n as u32;
    let mut edges: Vec<(u32, u32)> = silent.to_vec();
    edges.extend((0..n as u32).filter(|&i| on[i as usize]).map(|i| (i, sink)));
    let mut first: Vec<u32> = initial.to_vec();
    first.push(initial.iter().max().map_or(0, |b| b + 1));
    let blocks = quotient_blocks(n + 1, &edges, rises, &first);
    blocks[..n].iter().collect::<BTreeSet<_>>().len()
}

/// The moves of a run by source state, for shortest paths over the silent edges, with the
/// rise edges kept apart.
pub(super) struct Succ<'a> {
    pub(super) run: &'a crate::lifecycle::explore::ComponentRun,
    pub(super) start: Vec<u32>,
    pub(super) order: Vec<u32>,
    pub(super) is_rise: &'a [bool],
}

impl<'a> Succ<'a> {
    pub(super) fn new(
        run: &'a crate::lifecycle::explore::ComponentRun,
        is_rise: &'a [bool],
    ) -> Self {
        let by_source: Vec<(u32, u32)> = run
            .edges()
            .iter()
            .enumerate()
            .map(|(k, e)| (e.from, k as u32))
            .collect();
        let Adj { start, list: order } = Adj::new(run.states.len(), &by_source);
        Succ {
            run,
            start,
            order,
            is_rise,
        }
    }

    pub(super) fn all_from(
        &self,
        at: u32,
    ) -> impl Iterator<Item = crate::lifecycle::explore::Edge> + '_ {
        self.order[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
            .iter()
            .map(|k| self.run.edges().get(*k as usize))
    }

    pub(super) fn is_rise(&self, e: &crate::lifecycle::explore::Edge) -> bool {
        self.is_rise[self.run.step(e).0.index()]
    }

    /// The silent edges from `at`.
    pub(super) fn edges_from(
        &self,
        at: u32,
    ) -> impl Iterator<Item = crate::lifecycle::explore::Edge> + '_ {
        self.all_from(at).filter(move |e| !self.is_rise(e))
    }

    /// The rise edges from `at`.
    pub(super) fn rises_from(
        &self,
        at: u32,
    ) -> impl Iterator<Item = crate::lifecycle::explore::Edge> + '_ {
        self.all_from(at).filter(move |e| self.is_rise(e))
    }

    /// The BFS path over silent edges from `from` to the first state `done` accepts.
    pub(super) fn shortest_path(
        &self,
        from: u32,
        done: impl Fn(u32) -> bool,
    ) -> Vec<crate::lifecycle::explore::Edge> {
        if done(from) {
            return Vec::new();
        }
        let mut parent: HashMap<u32, crate::lifecycle::explore::Edge> = HashMap::new();
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

/// The silent edges between distinct states, by target and by source: what
/// `Stutter::blocks` refines a partition by.
struct Stutter {
    n: usize,
    pred: Adj,
    succ: Adj,
}

impl Stutter {
    fn new(n: usize, edges: &[(u32, u32)]) -> Self {
        let inter: Vec<(u32, u32)> = edges.iter().copied().filter(|(a, b)| a != b).collect();
        let pred_pairs: Vec<(u32, u32)> = inter.iter().map(|&(a, b)| (b, a)).collect();
        Stutter {
            n,
            pred: Adj::new(n, &pred_pairs),
            succ: Adj::new(n, &inter),
        }
    }

    /// Divergence-blind stuttering equivalence refining `initial`, the block of each state:
    /// the coarsest partition in which, for any two blocks B and B', either every state of B
    /// reaches B' through B or none does. Returns each state's block.
    ///
    /// A worklist of splitters. A block C is stable under a splitter S when all or none of
    /// its states reach S through C; that reads only C, S and the edges, so it changes only
    /// when C or S splits. Every block is stable under every splitter not on the list: a
    /// splitter's pass splits each block into the states that reach it and the rest, both
    /// stable under it; the parts of a split block go on the list, as splitters of the
    /// others, and so does every block an edge from them enters, the only splitters a part
    /// can have become unstable under (a state reaches S only by an edge into S). An empty
    /// list leaves a stable partition, and a pass splits only states that are not equivalent,
    /// so it is the coarsest one, whatever order the splitters come in.
    fn blocks(&self, initial: &[u32]) -> Vec<u32> {
        let mut block: Vec<u32> = initial.to_vec();
        let nblocks = block.iter().max().map_or(0, |b| *b as usize + 1);
        let mut members: Vec<Vec<u32>> = vec![Vec::new(); nblocks];
        for (i, &b) in block.iter().enumerate() {
            members[b as usize].push(i as u32);
        }
        let mut queued = vec![true; nblocks];
        let mut work: VecDeque<u32> = (0..nblocks as u32).collect();
        let mut marked = vec![false; self.n];
        let mut stack: Vec<u32> = Vec::new();
        let mut touched: Vec<u32> = Vec::new();
        while let Some(splitter) = work.pop_front() {
            queued[splitter as usize] = false;
            touched.clear();
            for &t in &members[splitter as usize] {
                for &p in self.pred.of(t) {
                    if block[p as usize] != splitter && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            while let Some(x) = stack.pop() {
                for &p in self.pred.of(x) {
                    if block[p as usize] == block[x as usize] && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            for &x in &touched {
                marked[x as usize] = false;
            }
            touched.sort_unstable_by_key(|&x| (block[x as usize], x));
            let mut k = 0;
            while k < touched.len() {
                let b = block[touched[k] as usize];
                let end = k + touched[k..]
                    .iter()
                    .take_while(|x| block[**x as usize] == b)
                    .count();
                if end - k < members[b as usize].len() {
                    let nb = members.len() as u32;
                    for &x in &touched[k..end] {
                        block[x as usize] = nb;
                    }
                    members.push(touched[k..end].to_vec());
                    members[b as usize].retain(|m| block[*m as usize] == b);
                    queued.push(false);
                    let mut push = |c: u32| {
                        if !queued[c as usize] {
                            queued[c as usize] = true;
                            work.push_back(c);
                        }
                    };
                    push(b);
                    push(nb);
                    for &x in members[b as usize].iter().chain(&members[nb as usize]) {
                        for &y in self.succ.of(x) {
                            push(block[y as usize]);
                        }
                    }
                }
                k = end;
            }
        }
        block
    }
}

/// Edges of a run by state index.
pub(super) type Edges = Vec<(u32, u32)>;

/// The edges of a run of the extended sub-lifecycle, the silent ones and the rises apart.
pub(super) fn split_edges(
    run: &crate::lifecycle::explore::ComponentRun,
    is_rise: &[bool],
) -> (Edges, Edges) {
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

/// Compressed adjacency.
pub(super) struct Adj {
    pub(super) start: Vec<u32>,
    pub(super) list: Vec<u32>,
}

impl Adj {
    pub(super) fn new(n: usize, pairs: &[(u32, u32)]) -> Self {
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

    pub(super) fn of(&self, at: u32) -> &[u32] {
        &self.list[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::{Bounds, Scope};

    /// A job behind a gate behind a key, the key's status cycling or only rising.
    pub(in crate::lifecycle::reduce) fn model(key_cycles: bool) -> String {
        let back = match key_cycles {
            true => {
                "usecase Relock \"施錠\"\ntransitions(Relock, Key.status, unlocked -> locked)\n"
            }
            false => "",
        };
        format!(
            r#"
entity Key "鍵" {{ id: Int @pk  status: Enum(locked, unlocked) @default(locked) }}
entity Gate "門" {{ id: Int @pk  status: Enum(shut, open) @default(shut) }}
entity Job "作業" {{ id: Int @pk  status: Enum(waiting, running) @default(waiting) }}
relate(Gate, Key, N:1)
relate(Job, Gate, N:1)
usecase Unlock "解錠"
usecase Swing "開く"
usecase Submit "投入"
usecase Start "開始"
transitions(Unlock, Key.status, locked -> unlocked)
{back}transitions(Swing, Gate.status, shut -> open)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
property JobStarts eventually(Job.status == running).fair(Start)
"#
        )
    }

    /// The quotient of the job's sub-lifecycle with nothing observed: one class, never
    /// explored, that can loop exactly when some var of the sub-lifecycle does not only rise
    /// (docs/formal-verification.md, "WF_vars(Next) on a reduced cone", case 3). A property
    /// cone observes what its T side reads, so this clause runs only where a read is folded
    /// away, as in a cone of clinic-ops; the test clears `obs` to reach it directly.
    #[test]
    fn an_unobserved_sub_lifecycle_can_loop_unless_every_var_only_rises() {
        for key_cycles in [false, true] {
            let model = model_from(&model(key_cycles));
            let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
            let (sys, seeds) = ts.cone_system(&ts.property_cone(0), [ts.property_seed(0)]);
            let mut split = sys.split_with(&seeds, true);
            assert!(!split.s.is_empty(), "the job's cone has a sub-lifecycle");
            split.obs.clear();
            let ext = Extended::of(&sys, &split);
            assert!(ext.obs_local.is_empty());
            let cache = QuotientCache::new();
            let Ok(q) = Quotient::of(&ext, &sys, Caps::VERIFY, &cache).0 else {
                panic!("one class");
            };
            assert_eq!(
                q.classes.reps.len(),
                1,
                "an unobserved sub-lifecycle is one class"
            );
            assert_eq!(
                q.classes.loops,
                vec![key_cycles],
                "the class can loop exactly when the key's status cycles"
            );
        }
    }

    /// A small deterministic generator, so a failing graph can be named by its seed.
    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed >> 33
    }

    /// The worklist partition is the one the sweeps of the reference compute, on random graphs
    /// with silent edges, self-loops, cycles, and rises.
    #[test]
    fn the_worklist_partition_is_the_partition_of_repeated_sweeps() {
        for seed0 in 0..400u64 {
            let mut seed = seed0;
            let n = 1 + (lcg(&mut seed) % 40) as usize;
            let silent: Vec<(u32, u32)> = (0..lcg(&mut seed) % (3 * n as u64 + 1))
                .map(|_| {
                    (
                        (lcg(&mut seed) % n as u64) as u32,
                        (lcg(&mut seed) % n as u64) as u32,
                    )
                })
                .collect();
            let rises: Vec<(u32, u32)> = (0..lcg(&mut seed) % (n as u64 + 1))
                .map(|_| {
                    (
                        (lcg(&mut seed) % n as u64) as u32,
                        (lcg(&mut seed) % n as u64) as u32,
                    )
                })
                .collect();
            let labels = 1 + lcg(&mut seed) % 4;
            let initial: Vec<u32> = {
                let raw: Vec<u64> = (0..n).map(|_| lcg(&mut seed) % labels).collect();
                let mut index: HashMap<u64, u32> = HashMap::new();
                raw.iter()
                    .map(|l| {
                        let len = index.len() as u32;
                        *index.entry(*l).or_insert(len)
                    })
                    .collect()
            };
            let new = quotient_blocks(n, &silent, &rises, &initial);
            let old = reference::quotient_blocks(n, &silent, &rises, &initial);
            assert!(
                reference::same_partition(&new, &old),
                "seed {seed0}: {new:?} vs {old:?}"
            );
        }
    }
}
