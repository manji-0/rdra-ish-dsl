//! The quotient of the extended sub-lifecycle: stuttering partition refinement.
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use super::extended::Extended;
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::{Adjacency, Caps};
use crate::lifecycle::expr::State;
use crate::lifecycle::types::{ActId, VarId};
use crate::lifecycle::TransitionSystem;

/// The classes of S, with the initial states' and the moves' classes.
pub(super) struct Quotient {
    pub(super) reps: Vec<Box<[u8]>>,
    pub(super) class_of: HashMap<Box<[u8]>, u32>,
    pub(super) init: HashMap<Box<[u8]>, u32>,
    pub(super) moves: Vec<Vec<ClassMove>>,
    pub(super) rises: BTreeMap<(u32, Box<[u8]>), u32>,
    pub(super) inner: Option<Arc<Reduction>>,
    pub(super) states: usize,
    /// Per class, whether it can loop inside itself (`can_loop`).
    pub(super) loops: Vec<bool>,
    pub(super) divergence: Divergence,
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
    pub(super) fn of(ext: &Extended, cone: &TransitionSystem, caps: Caps) -> Option<Self> {
        let raw = &ext.system;
        if ext.obs_local.is_empty() {
            let initial = raw.initial_states();
            let rep = initial.first().expect("an initial state").0.clone();
            return Some(Quotient {
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
        let mut rises: BTreeMap<(u32, Box<[u8]>), u32> = BTreeMap::new();
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
            // The BFS of the cone reads a class move's params by the cone action's params.
            let params = ext.cone_params(cone, act, params);
            if seen.insert((class[x], act, params.clone(), class[y])) {
                moves[class[x] as usize].push((act, params, class[y]));
            }
        }
        let (loops, divergence) = match s_cycles(ext) {
            true => divergence(&run, &sub, &silent, &rise_edges, &initial, &class),
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
        Some(Quotient {
            reps,
            class_of,
            init,
            moves,
            rises,
            inner: sub.reduction.clone(),
            states: n,
            loops,
            divergence,
        })
    }
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
            .map(|k| self.run.edges()[*k as usize])
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

/// Divergence-blind stuttering equivalence of a graph whose moves are all silent, refining
/// `initial`, the block of each state: the coarsest partition in which, for any two blocks
/// B and B', either every state of B reaches B' through B or none does. Returns each state's
/// block.
pub(super) fn stuttering_blocks(n: usize, edges: &[(u32, u32)], initial: &[u32]) -> Vec<u32> {
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
mod tests {
    use super::*;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::{Bounds, Scope};

    /// A job behind a gate behind a key, the key's status cycling or only rising.
    fn model(key_cycles: bool) -> String {
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
            let q = Quotient::of(&ext, &sys, Caps::VERIFY).expect("one class");
            assert_eq!(q.reps.len(), 1, "an unobserved sub-lifecycle is one class");
            assert_eq!(
                q.loops,
                vec![key_cycles],
                "the class can loop exactly when the key's status cycles"
            );
        }
    }
}
