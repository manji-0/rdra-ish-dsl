//! `eventually` and `leads_to` decided on a run up to instance symmetry (#212 phase 4,
//! docs/design/instance-symmetry.md, "Soundness per check"), by the fair components of
//! `liveness.rs` read on the annotated quotient.
//!
//! A plain run decides a binding `b` on its own graph. On the stored graph the binding moves:
//! an edge from `r` whose concrete successor `t` is stored as `r' = σ(t)` takes the pair
//! `(r, b)` to `(r', σ(b))`. The search runs on those pairs, at most the stored states times
//! the bindings. A pair stands for every concrete pair `(τ(r), τ(b))`, and a path of pairs
//! lifts from any of them, so the pairs where the goal fails, their moves, and their strongly
//! connected components are the plain ones read through the group.
//!
//! A fair instance is keyed by its action's param slots, and an edge moves those slots too.
//! An instance is followed along the edges of a component as the binding is: a *thread* is a
//! (pair, instance) node, and an edge inside the component takes `(u, x)` to `(v, σ(x))`.
//! The threads over one component fall apart into pieces that each stand for one concrete
//! instance over one concrete component, and no edge joins two pieces. So a component admits
//! a fair cycle when it has a cycle, is expanded, and every piece of its threads either takes
//! its instance on some edge or meets a pair where the instance is not enabled.
//!
//! The counterexample is built on the pairs: the BFS path, a shortest walk to the nearest end,
//! and for a component a loop that, from its first pair, covers every instance enabled there.
//! The loop's permutations compose to some `π`, so it closes concretely after at most the
//! order of `π` copies; each copy covers every instance again. The concrete lasso then
//! certifies itself as a plain one does: it replays, fails the property, and is fair.

use crate::lifecycle::narrow::id32;
use std::collections::VecDeque;

use crate::fasthash::FastMap;
use crate::lifecycle::expr::State;
use crate::lifecycle::lasso::{Fairness, Loop};
use crate::lifecycle::reduce::Step;
use crate::lifecycle::types::{ActId, Ltl, Pred};
use crate::lifecycle::ReplayError;

use super::super::graph::Adjacency;
use super::super::{LassoFault, PropertyVerdict};
use super::{concrete, EdgePerms, OrbitRun};

const NONE: u32 = u32::MAX;

/// Per thread a walk reached: the thread and edge it came from.
type ThreadVia = FastMap<(u32, u32), Option<(u32, u32, usize)>>;

/// The most copies of a loop the lift tries before it gives up: far more than the order of
/// any permutation of the slots a bound allows in practice.
const MOST_COPIES: usize = 10_000;

/// The stored graph of an annotated run, times the bindings of one property.
pub(super) struct Product<'r> {
    pub(super) run: &'r OrbitRun,
    perms: &'r EdgePerms,
    /// The property's bindings, `binder_slots` order.
    pub(super) bindings: Vec<Vec<u8>>,
    /// Per permutation id: binding `k` to the binding `σ` takes it to.
    bmap: Vec<Vec<u32>>,
    /// Per binding id of the run: the fair instance its moves are steps of, or `NONE`.
    instance: Vec<u32>,
    /// The fair instances by id: the action and its keying slots.
    keys: Vec<(ActId, Box<[u8]>)>,
    /// Per permutation id, `keys.len()` entries: instance `x` to the one `σ` takes it to,
    /// `NONE` when that one labels no edge of the run.
    imap: Vec<u32>,
    /// Per stored state, `en[en_start[r]..en_start[r + 1]]`: the fair instances with a move
    /// to another concrete state, ascending.
    en_start: Vec<u32>,
    en: Vec<u32>,
    /// Per stored state: some move goes to another concrete state.
    moves_on: Vec<bool>,
}

/// What the search found on the pairs.
pub(super) struct Search {
    /// Per pair: the goal fails.
    pub(super) in_n: Vec<bool>,
    comp: Vec<u32>,
    /// Per component: it admits a fair cycle.
    fair_cycle: Vec<bool>,
    /// Per pair: the next pair toward the nearest fair end, itself at an end, `NONE` when it
    /// cannot avoid the goal forever or is not in `N`.
    pub(super) toward: Vec<u32>,
    stops: Vec<bool>,
    /// The triggered pair the counterexample starts from: the first stored state, then the
    /// first binding, as the plain search picks.
    pub(super) start: Option<u32>,
}

impl<'r> Product<'r> {
    pub(super) fn new(run: &'r OrbitRun, i: usize) -> Self {
        // Unchecked internal invariant (P20): a precondition of `decide_liveness`, whose
        // callers run it on `run_annotated_orbits` only.
        let perms = run
            .perms
            .as_ref()
            .expect("a run up to symmetry that a liveness reader reads keeps its permutations");
        let ts = run.system();
        let g = &run.group;
        let p = &ts.properties()[i];
        let bindings: Vec<Vec<u8>> = ts.binder_slots(&p.binders).collect();
        let index: FastMap<&[u8], u32> = (bindings.iter().enumerate())
            .map(|(k, b)| (&b[..], id32(k)))
            .collect();
        let bmap: Vec<Vec<u32>> = (0..id32(perms.len()))
            .map(|id| {
                let perm = perms.get(id);
                bindings
                    .iter()
                    .map(|b| {
                        let to: Vec<u8> = (p.binders.iter().zip(b))
                            .map(|(&e, &slot)| g.map_slot(perm, e, slot))
                            .collect();
                        index[&to[..]]
                    })
                    .collect()
            })
            .collect();
        let mut key_index: FastMap<(ActId, Box<[u8]>), u32> = FastMap::default();
        let mut keys = Vec::new();
        let instance: Vec<u32> = (run.run.bindings.iter())
            .map(|(act, params)| {
                let Some(slots) = p.fair_instance(&ts.actions()[act.index()], params) else {
                    return NONE;
                };
                *key_index.entry((*act, slots.clone())).or_insert_with(|| {
                    keys.push((*act, slots));
                    id32(keys.len()) - 1
                })
            })
            .collect();
        // An instance's slots move as its action's params do; a class move of a reduced cone
        // keeps its params, which name only rows `G_T` leaves where they are (`concrete`).
        let imap: Vec<u32> = (0..id32(perms.len()))
            .flat_map(|id| {
                let perm = perms.get(id);
                let (keys, key_index) = (&keys, &key_index);
                keys.iter().map(move |(act, slots)| {
                    if ts.reduction().is_some_and(|r| r.skips(*act)) {
                        return key_index[&(*act, slots.clone())];
                    }
                    let ents = (ts.actions()[act.index()].params.iter())
                        .filter(|param| param.role.keys_fair_instance())
                        .map(|param| param.entity);
                    let to: Box<[u8]> = ents
                        .zip(slots.iter())
                        .map(|(e, &slot)| g.map_slot(perm, e, slot))
                        .collect();
                    key_index.get(&(*act, to)).copied().unwrap_or(NONE)
                })
            })
            .collect();
        let n = run.run.states.len();
        let edges = run.run.edges();
        let mut en_start = Vec::with_capacity(n + 1);
        let mut en = Vec::new();
        let mut moves_on = vec![false; n];
        en_start.push(0);
        for r in 0..id32(n) {
            let from = en.len();
            for (k, e) in edges.range_of(r).zip(edges.from_state(r)) {
                if !run.is_real(k, &e) {
                    continue;
                }
                moves_on[r as usize] = true;
                let x = instance[e.binding as usize];
                if x != NONE {
                    en.push(x);
                }
            }
            en[from..].sort_unstable();
            let mut kept = from;
            for j in from..en.len() {
                if kept == from || en[kept - 1] != en[j] {
                    en[kept] = en[j];
                    kept += 1;
                }
            }
            en.truncate(kept);
            en_start.push(u32::try_from(en.len()).expect("instances fit in u32"));
        }
        Product {
            run,
            perms,
            bindings,
            bmap,
            instance,
            keys,
            imap,
            en_start,
            en,
            moves_on,
        }
    }

    fn nb(&self) -> usize {
        self.bindings.len()
    }

    /// The pair of stored state `r` and binding `k`.
    pub(super) fn pair(&self, r: u32, k: u32) -> u32 {
        u32::try_from(r as usize * self.nb() + k as usize).expect("pairs fit in u32")
    }

    fn state_of(&self, u: u32) -> u32 {
        id32(u as usize / self.nb())
    }

    fn binding_of(&self, u: u32) -> u32 {
        id32(u as usize % self.nb())
    }

    /// The pair edge `k`, which leaves `state_of(u)`, takes `u` to.
    fn target(&self, u: u32, k: usize) -> u32 {
        let e = self.run.run.edges().get(k);
        let b = self.bmap[self.perms.id(k) as usize][self.binding_of(u) as usize];
        self.pair(e.to, b)
    }

    /// The fair instances enabled at stored state `r`.
    fn enabled(&self, r: u32) -> &[u32] {
        &self.en[self.en_start[r as usize] as usize..self.en_start[r as usize + 1] as usize]
    }

    fn is_enabled(&self, r: u32, x: u32) -> bool {
        x != NONE && self.enabled(r).binary_search(&x).is_ok()
    }

    /// The instance edge `k`'s permutation takes `x` to.
    fn moved(&self, k: usize, x: u32) -> u32 {
        self.imap[self.perms.id(k) as usize * self.keys.len() + x as usize]
    }

    /// The edges that leave `state_of(u)` and are real moves, with the pair each reaches.
    fn real_edges(&self, u: u32) -> impl Iterator<Item = (usize, u32)> + '_ {
        let edges = self.run.run.edges();
        let r = self.state_of(u);
        (edges.range_of(r).zip(edges.from_state(r)))
            .filter(|(k, e)| self.run.is_real(*k, e))
            .map(move |(k, _)| (k, self.target(u, k)))
    }

    /// `N`, its components, the fair ends, and the backward walk to them, for property `i`
    /// under `fairness`.
    pub(super) fn search(&self, i: usize, fairness: Fairness) -> Search {
        let ts = self.run.system();
        let p = &ts.properties()[i];
        let (cause, goal): (Option<&Pred>, &Pred) = match &p.formula {
            Ltl::Eventually(g) => (None, g),
            Ltl::LeadsTo(x, g) => (Some(x), g),
            other => panic!("decide_liveness on {:?}, not a liveness form", other.kind()),
        };
        let states = &self.run.run.states;
        let n = states.len();
        let nb = self.nb();
        let pairs = n * nb;
        let expanded = self.run.run.expanded();
        let mut in_n = vec![false; pairs];
        for r in 0..n {
            for (k, b) in self.bindings.iter().enumerate() {
                in_n[r * nb + k] = !ts.holds_bound(goal, &states.at(r), b);
            }
        }
        let edges = self.run.run.edges();
        let inside = |u: u32, v: u32| in_n[u as usize] && in_n[v as usize];
        // Every pair move inside `N`, a real move each: a concrete self-loop is a loop of
        // pairs, which `Adjacency` leaves out, and an orbit self-edge that keeps the binding is
        // one too, kept apart as `self_loop`.
        let adjacency = Adjacency::new(pairs, || {
            (0..id32(n)).flat_map(move |r| {
                (edges.range_of(r).zip(edges.from_state(r)))
                    .filter(move |(k, e)| self.run.is_real(*k, e))
                    .flat_map(move |(k, _)| {
                        (0..id32(nb)).filter_map(move |b| {
                            let u = self.pair(r, b);
                            let v = self.target(u, k);
                            inside(u, v).then_some((u, v))
                        })
                    })
            })
        });
        let mut self_loop = vec![false; pairs];
        for r in 0..id32(n) {
            for b in 0..id32(nb) {
                let u = self.pair(r, b);
                if in_n[u as usize] && self.real_edges(u).any(|(_, v)| v == u) {
                    self_loop[u as usize] = true;
                }
            }
        }
        let (comp, count) = adjacency.components();
        let mut size = vec![0u32; count];
        let mut open = vec![false; count];
        let mut looped = vec![false; count];
        for u in (0..pairs).filter(|&u| in_n[u]) {
            let c = comp[u] as usize;
            size[c] += 1;
            open[c] |= self.state_of(id32(u)) as usize >= expanded;
            looped[c] |= self_loop[u];
        }
        let candidate: Vec<bool> = (0..count)
            .map(|c| (size[c] >= 2 || looped[c]) && !open[c])
            .collect();
        let fair_cycle = self.threads_covered(&in_n, &comp, &candidate);
        let stops: Vec<bool> = (0..pairs)
            .map(|u| {
                let r = self.state_of(id32(u));
                in_n[u]
                    && (r as usize) < expanded
                    && match fairness {
                        Fairness::Plain => self.enabled(r).is_empty(),
                        Fairness::WfNext => !self.moves_on[r as usize],
                    }
            })
            .collect();
        let mut toward = vec![NONE; pairs];
        let mut queue = VecDeque::new();
        for u in 0..pairs {
            if stops[u] || (in_n[u] && fair_cycle[comp[u] as usize]) {
                toward[u] = id32(u);
                queue.push_back(id32(u));
            }
        }
        while let Some(at) = queue.pop_front() {
            for &from in adjacency.preds(at) {
                if toward[from as usize] == NONE {
                    toward[from as usize] = at;
                    queue.push_back(from);
                }
            }
        }
        drop(adjacency);
        let start = (0..id32(pairs)).find(|&u| {
            let (s, b) = (
                &states.at(self.state_of(u) as usize),
                &self.bindings[self.binding_of(u) as usize],
            );
            toward[u as usize] != NONE
                && ts.triggers(p, b, s)
                && cause.is_none_or(|x| ts.holds_bound(x, s, b))
        });
        Search {
            in_n,
            comp,
            fair_cycle,
            toward,
            stops,
            start,
        }
    }

    /// Per component, whether every piece of its threads is covered, for the `candidate`
    /// components: those with a cycle that the run expanded whole.
    fn threads_covered(&self, in_n: &[bool], comp: &[u32], candidate: &[bool]) -> Vec<bool> {
        let count = candidate.len();
        // The members of each candidate component, by a counting sort.
        let mut start = vec![0u32; count + 1];
        for u in (0..in_n.len()).filter(|&u| in_n[u] && candidate[comp[u] as usize]) {
            start[comp[u] as usize + 1] += 1;
        }
        for c in 0..count {
            start[c + 1] += start[c];
        }
        let mut fill = start.clone();
        let mut members = vec![0u32; start[count] as usize];
        for u in (0..in_n.len()).filter(|&u| in_n[u] && candidate[comp[u] as usize]) {
            let c = comp[u] as usize;
            members[fill[c] as usize] = id32(u);
            fill[c] += 1;
        }
        drop(fill);
        let mut local: FastMap<u32, u32> = FastMap::default();
        let mut base: Vec<u32> = Vec::new();
        let mut covered: Vec<bool> = Vec::new();
        let mut parent: Vec<u32> = Vec::new();
        (0..count)
            .map(|c| {
                if !candidate[c] {
                    return false;
                }
                let of = &members[start[c] as usize..start[c + 1] as usize];
                local.clear();
                base.clear();
                let mut threads = 0u32;
                for (at, &u) in of.iter().enumerate() {
                    local.insert(u, id32(at));
                    base.push(threads);
                    threads += id32(self.enabled(self.state_of(u)).len());
                }
                covered.clear();
                covered.resize(threads as usize, false);
                parent.clear();
                parent.extend(0..threads);
                let thread = |u: u32, x: u32| -> u32 {
                    let r = self.state_of(u);
                    let at = self
                        .enabled(r)
                        .binary_search(&x)
                        .expect("an enabled instance");
                    base[local[&u] as usize] + id32(at)
                };
                for &u in of {
                    let r = self.state_of(u);
                    for (k, v) in self.real_edges(u) {
                        if !in_n[v as usize] || comp[v as usize] != id32(c) {
                            continue;
                        }
                        let rv = self.state_of(v);
                        let taken = self.instance[self.run.run.edges().get(k).binding as usize];
                        if taken != NONE {
                            covered[thread(u, taken) as usize] = true;
                        }
                        for &x in self.enabled(r) {
                            let y = self.moved(k, x);
                            match self.is_enabled(rv, y) {
                                true => union(&mut parent, thread(u, x), thread(v, y)),
                                false => covered[thread(u, x) as usize] = true,
                            }
                        }
                        // An instance enabled at the target whose source here is disabled:
                        // the piece meets a pair where it is not enabled.
                        for &y in self.enabled(rv) {
                            let back = (self.enabled(r).iter()).any(|&x| self.moved(k, x) == y);
                            if !back {
                                covered[thread(v, y) as usize] = true;
                            }
                        }
                    }
                }
                let mut root_covered = vec![false; threads as usize];
                for t in 0..threads {
                    if covered[t as usize] {
                        root_covered[find(&mut parent, t) as usize] = true;
                    }
                }
                (0..threads).all(|t| root_covered[find(&mut parent, t) as usize])
            })
            .collect()
    }

    /// The counterexample from `search.start` on the pairs, lifted to concrete states and
    /// certified on the run's system as a plain lasso is.
    pub(super) fn lasso(
        &self,
        i: usize,
        fairness: Fairness,
        search: &Search,
        start: u32,
    ) -> Result<PropertyVerdict, LassoFault> {
        let run = &self.run.run;
        // The BFS path to the start's state, its binding read back to the initial state.
        // Unchecked internal invariant (P20): `path_to` returns the parent edges the BFS
        // stored, each one of its source's edges, so the look-up finds it.
        let path: Vec<usize> = (run.path_to(self.state_of(start)).iter())
            .map(|e| {
                let range = run.edges().range_of(e.from);
                (range.clone().zip(run.edges().from_state(e.from)))
                    .find(|(_, f)| f == e)
                    .map(|(k, _)| k)
                    .expect("a parent edge is an edge of its source")
            })
            .collect();
        let mut walk = path;
        let mut at = start;
        // Unchecked internal invariant (P20): `search` set `toward[at]` from one of `at`'s
        // real edges, so the walk finds that edge.
        while search.toward[at as usize] != at {
            let to = search.toward[at as usize];
            let (k, _) = self
                .real_edges(at)
                .find(|&(_, v)| v == to)
                .expect("the backward walk follows real moves");
            walk.push(k);
            at = to;
        }
        let first = walk
            .first()
            .map_or(self.state_of(start), |&k| run.edges().get(k).from);
        let entry = walk.len();
        let cycle = match search.stops[at as usize] {
            true => None,
            false => {
                debug_assert!(search.fair_cycle[search.comp[at as usize] as usize]);
                Some(self.fair_loop(search, at))
            }
        };
        let (states, actions, tail) = {
            let (start_state, mut steps, tau) = self
                .lift(first, &walk, None, 0)
                .map_err(LassoFault::Replay)?;
            match cycle {
                None => {
                    let mut states = vec![start_state];
                    states.extend(steps.iter().map(|s| s.2.clone()));
                    let actions = steps.iter().map(|s| s.0).collect::<Vec<_>>();
                    (states, actions, Loop::Stutter)
                }
                Some(cycle) => {
                    let back = steps.last().map_or(start_state.clone(), |s| s.2.clone());
                    let mut tau = tau;
                    let mut copies = 0;
                    loop {
                        let (_, more, next) = self
                            .lift(first, &cycle, Some(tau), steps.len())
                            .map_err(LassoFault::Replay)?;
                        tau = next;
                        steps.extend(more);
                        copies += 1;
                        if steps.last().is_some_and(|s| s.2 == back) {
                            break;
                        }
                        if copies == MOST_COPIES {
                            return Err(LassoFault::Unclosed);
                        }
                    }
                    let mut states = vec![start_state];
                    let actions = steps.iter().map(|s| s.0).collect::<Vec<_>>();
                    steps.pop();
                    states.extend(steps.iter().map(|s| s.2.clone()));
                    (states, actions, Loop::Back { to: entry + 1 })
                }
            }
        };
        let ts = self.run.system();
        let replay = ts
            .replay_lasso(states, &actions, tail)
            .map_err(LassoFault::Replay)?;
        let diag = ts
            .explain_property(i, &replay)
            .ok_or(LassoFault::Satisfies)?;
        if let Some(unfair) = ts.unfair_loop(i, &replay, fairness) {
            return Err(LassoFault::Unfair(unfair));
        }
        Ok(PropertyVerdict::Violated(Box::new(diag)))
    }

    /// The edges `ks`, a walk of stored edges from stored state `first`, lifted to concrete
    /// states from `τ` (the identity when `None`): each edge's concrete successor is the move
    /// at its place among its source's moves, read through `τ`, and `τ` becomes `τ ∘ σ⁻¹`.
    /// Also returns the first concrete state and the last `τ`. `before` is the number of steps
    /// the lasso has before these, so a fault names the lasso's state. An edge that is not the
    /// move at its place, or whose permutation does not take that move to its target, is a
    /// `ReplayError::NoMove` (P20), as `orbit::lift_path` reports a stored edge it cannot find.
    fn lift(
        &self,
        first: u32,
        ks: &[usize],
        tau: Option<Vec<u8>>,
        before: usize,
    ) -> Result<(State, Vec<Step>, Vec<u8>), ReplayError> {
        let (run, g) = (&self.run.run, &self.run.group);
        let ts = &run.system;
        let mut tau = tau.unwrap_or_else(|| g.identity());
        let start = {
            let mut out = vec![0u8; ts.width];
            g.apply(&tau, &run.states.at(first as usize).0, &mut out);
            State(out.into_boxed_slice())
        };
        let mut steps = Vec::with_capacity(ks.len());
        let mut stored = vec![0u8; ts.width];
        for &k in ks {
            let e = run.edges().get(k);
            let (act, params) = run.step(&e);
            let no_move = ReplayError::NoMove {
                state: before + steps.len() + 2,
                action: act,
            };
            let place = k - run.edges().range_of(e.from).start;
            let m = (ts.moves(&run.states.at(e.from as usize)).nth(place))
                .filter(|m| m.act == act && *m.params == *params)
                .ok_or_else(|| no_move.clone())?;
            let sigma = self.perms.get(self.perms.id(k));
            g.apply(sigma, &m.next.0, &mut stored);
            if stored[..] != run.states.at(e.to as usize).0[..] {
                return Err(no_move);
            }
            steps.push(concrete(ts, g, &tau, act, params, &m.next));
            tau = g.compose(&tau, &g.inverse(sigma));
        }
        Ok((start, steps, tau))
    }

    /// A walk of edges inside `end`'s component from `end` back to it that covers, for every
    /// instance enabled at `end`, a move of it or a pair where it is not enabled: each time
    /// along the shortest walk of threads that covers the first instance not yet covered,
    /// then back by a shortest walk.
    fn fair_loop(&self, search: &Search, end: u32) -> Vec<usize> {
        let c = search.comp[end as usize];
        let within = |v: u32| search.in_n[v as usize] && search.comp[v as usize] == c;
        let mut open: Vec<u32> = self.enabled(self.state_of(end)).to_vec();
        let mut walk = Vec::new();
        let mut at = end;
        while let Some(&x) = open.first() {
            let path = self.thread_walk(&within, at, x);
            for &k in &path {
                let taken = self.instance[self.run.run.edges().get(k).binding as usize];
                let to = self.run.run.edges().get(k).to;
                open = (open.iter())
                    .filter(|&&y| y != taken)
                    .map(|&y| self.moved(k, y))
                    .filter(|&y| self.is_enabled(to, y))
                    .collect();
                at = self.target(at, k);
            }
            walk.extend(path);
        }
        if walk.is_empty() || at != end {
            walk.extend(self.walk_in(&within, at, end, walk.is_empty()));
        }
        walk
    }

    /// A shortest walk of threads from `(from, x)` inside the component to the first edge that
    /// takes the thread's instance or reaches a pair where it is not enabled, that edge
    /// included.
    fn thread_walk(&self, within: &impl Fn(u32) -> bool, from: u32, x: u32) -> Vec<usize> {
        let mut via: ThreadVia = FastMap::default();
        via.insert((from, x), None);
        let mut queue = VecDeque::from([(from, x)]);
        let path_to = |via: &ThreadVia, mut at: (u32, u32), last: usize| {
            let mut path = vec![last];
            while let Some(Some((u, y, k))) = via.get(&at) {
                path.push(*k);
                at = (*u, *y);
            }
            path.reverse();
            path
        };
        while let Some((u, y)) = queue.pop_front() {
            for (k, v) in self.real_edges(u) {
                if !within(v) {
                    continue;
                }
                let taken = self.instance[self.run.run.edges().get(k).binding as usize];
                let moved = self.moved(k, y);
                if taken == y || !self.is_enabled(self.state_of(v), moved) {
                    return path_to(&via, (u, y), k);
                }
                if let std::collections::hash_map::Entry::Vacant(slot) = via.entry((v, moved)) {
                    slot.insert(Some((u, y, k)));
                    queue.push_back((v, moved));
                }
            }
        }
        // Unchecked internal invariant (P20): `fair_loop` asks only for a thread enabled in
        // a component `search` marked fair, and a fair component has, for every thread, an
        // edge that takes it or a pair where it is not enabled.
        panic!("a fair component covers every thread")
    }

    /// A shortest walk of edges inside the component from `from` to `to`; with `nonempty`, at
    /// least one edge even when `from == to`.
    fn walk_in(
        &self,
        within: &impl Fn(u32) -> bool,
        from: u32,
        to: u32,
        nonempty: bool,
    ) -> Vec<usize> {
        if from == to && !nonempty {
            return Vec::new();
        }
        let mut via: FastMap<u32, Option<(u32, usize)>> = FastMap::default();
        via.insert(from, None);
        let mut queue = VecDeque::from([from]);
        let path_to = |via: &FastMap<u32, Option<(u32, usize)>>, mut at: u32, last: usize| {
            let mut path = vec![last];
            while let Some(Some((u, k))) = via.get(&at) {
                path.push(*k);
                at = *u;
            }
            path.reverse();
            path
        };
        while let Some(u) = queue.pop_front() {
            for (k, v) in self.real_edges(u) {
                if !within(v) {
                    continue;
                }
                if v == to {
                    return path_to(&via, u, k);
                }
                if let std::collections::hash_map::Entry::Vacant(slot) = via.entry(v) {
                    slot.insert(Some((u, k)));
                    queue.push_back(v);
                }
            }
        }
        // Unchecked internal invariant (P20): `from` and `to` are pairs of one strongly
        // connected component, so a walk inside it joins them.
        panic!("a component's pairs reach each other")
    }
}

fn find(parent: &mut [u32], x: u32) -> u32 {
    let mut x = x;
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (a, b) = (find(parent, a), find(parent, b));
    if a != b {
        parent[a.max(b) as usize] = a.min(b);
    }
}

impl OrbitRun {
    /// Whether edge `k`, `e`, is a real move: to another stored state, or an orbit self-edge.
    /// A concrete self-loop is a stutter.
    pub(super) fn is_real(&self, k: usize, e: &super::super::Edge) -> bool {
        e.from != e.to || self.orbit_self.binary_search(&id32(k)).is_ok()
    }

    /// `properties()[i]`, an `eventually` or a `leads_to`, decided on this run's stored graph
    /// under `fairness`, as `ComponentRun::decide_liveness` decides it on a plain run's: the
    /// same verdict, since the search runs on (stored state, binding) pairs through each
    /// edge's permutation. A counterexample is lifted to concrete states and must replay,
    /// fail the property, and be fair; one that does not is a `LassoFault`. Needs a run that
    /// kept its permutations (`run_annotated_orbits`).
    pub fn decide_liveness(
        &self,
        i: usize,
        fairness: Fairness,
    ) -> Result<PropertyVerdict, LassoFault> {
        let product = Product::new(self, i);
        let search = product.search(i, fairness);
        #[cfg(feature = "shadow")]
        super::reference::check_liveness(&product, &search, i, fairness);
        let Some(start) = search.start else {
            return Ok(match self.run.complete {
                true => PropertyVerdict::Holds,
                false => PropertyVerdict::Undecided,
            });
        };
        product.lasso(i, fairness, &search, start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::tests::system_from;
    use crate::lifecycle::{Bounds, Caps};

    const SHIP: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped)
"#;

    /// A stored edge whose permutation does not take the move at its place to its target is
    /// a `ReplayError` of the lasso's lift, not a panic (P20): every edge lifts under the
    /// run's own permutations, and some edge does not under the identity in their place.
    #[test]
    fn an_edge_its_permutation_does_not_explain_is_a_replay_error() {
        let ts = system_from(SHIP, Bounds::VERIFY);
        let i = (ts.properties().iter())
            .position(|p| p.name == "PaidShips")
            .expect("the property");
        let run = ts.run_annotated_orbits(Caps::VERIFY);
        let product = Product::new(&run, i);
        let edges = run.perms.as_ref().expect("an annotated run").edges();
        for k in 0..edges {
            let from = run.run.edges().get(k).from;
            assert!(product.lift(from, &[k], None, 0).is_ok(), "edge {k} lifts");
        }
        let mut identity = EdgePerms::new(&run.group);
        (0..edges).for_each(|_| identity.push_identity());
        let wrong = Product {
            perms: &identity,
            ..Product::new(&run, i)
        };
        let faults: Vec<ReplayError> = (0..edges)
            .filter_map(|k| {
                let from = run.run.edges().get(k).from;
                wrong.lift(from, &[k], None, 3).err()
            })
            .collect();
        assert!(!faults.is_empty(), "some edge moves its successor");
        assert!(faults
            .iter()
            .all(|f| matches!(f, ReplayError::NoMove { state: 5, .. })));
    }
}
