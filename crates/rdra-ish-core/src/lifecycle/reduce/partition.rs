//! The quotient of the extended sub-lifecycle: stuttering partition refinement.
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use super::extended::Extended;
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::Caps;
use crate::lifecycle::expr::State;
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

/// The classes of S, with the initial states' and the moves' classes.
pub(super) struct Quotient {
    pub(super) reps: Vec<Box<[u8]>>,
    pub(super) class_of: HashMap<Box<[u8]>, u32>,
    pub(super) init: HashMap<Box<[u8]>, u32>,
    pub(super) moves: Vec<Vec<ClassMove>>,
    pub(super) rises: HashMap<(u32, Box<[u8]>), u32>,
    pub(super) inner: Option<Arc<Reduction>>,
    pub(super) states: usize,
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
            // The BFS of the cone reads a class move's params by the cone action's params.
            let params = ext.cone_params(cone, act, params);
            if seen.insert((class[x], act, params.clone(), class[y])) {
                moves[class[x] as usize].push((act, params, class[y]));
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
