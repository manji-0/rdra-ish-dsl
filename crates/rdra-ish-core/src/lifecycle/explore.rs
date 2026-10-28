//! BFS over one component's flat states. Guards, invariants, and after-asserts are checked on
//! the fly; per-entity view statistics are gathered from the edges as they are found. The run
//! keeps every edge and, per state, the edge that first reached it.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::diag::{Diag, StateDiag};
use super::expr::{Env, State};
use super::restrict::Component;
use super::trace::Trace;
use super::types::{ActId, Action, Effect, EntId, InvKind, ParamRole, Rhs, Start, Value, VarId};
use super::view::{describe_slot, pattern_bound};
use super::TransitionSystem;

/// Where exploration of a component stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// Distinct view patterns per entity (`--max-patterns`).
    pub patterns: usize,
    /// Joint states per component.
    pub states: usize,
}

impl Caps {
    pub const DEFAULT_PATTERNS: usize = 256;

    pub const fn new(patterns: usize, states: usize) -> Self {
        Caps { patterns, states }
    }
}

/// One BFS per component.
pub struct Exploration {
    pub runs: Vec<ComponentRun>,
}

pub struct ComponentRun {
    pub component: Component,
    /// The component as a standalone system; `states` and the views index into it.
    pub system: TransitionSystem,
    /// Reached states in discovery order.
    pub states: Vec<State>,
    /// False when a cap stopped the BFS.
    pub complete: bool,
    pub diags: Vec<Diag>,
    pub(crate) views: Vec<EntityView>,
    /// Parallel to `states`: the index in `edges` of the edge that first reached the state;
    /// `None` for an initial state.
    parents: Vec<Option<u32>>,
    /// Every move from an expanded state, self-loops and repeats included, in discovery order.
    edges: Vec<Edge>,
    /// `(action, params)` of each distinct binding an edge names.
    bindings: Vec<(ActId, Box<[u8]>)>,
}

/// One move of the run: `states[from]` steps to `states[to]` under one `(action, params)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    binding: u32,
}

/// One step: an action, its bound slots, and the successor. BFS enumerates this, and nothing
/// else evaluates guards and effects.
pub(crate) struct Move {
    pub act: ActId,
    pub params: Box<[u8]>,
    pub next: State,
}

/// What both engines produce for one cone: the reached states in discovery order, initial
/// ones first, and the moves between them. Findings read only this.
pub struct Graph<'a> {
    pub states: &'a [State],
    /// False when a cap stopped the exploration, so a state it did not reach may be reachable.
    pub complete: bool,
    edges: &'a [Edge],
    bindings: &'a [(ActId, Box<[u8]>)],
}

impl Graph<'_> {
    /// Every move as `(from, to, action)`, self-loops and repeats included.
    pub fn edges(&self) -> impl Iterator<Item = (u32, u32, ActId)> + '_ {
        self.edges
            .iter()
            .map(|e| (e.from, e.to, self.bindings[e.binding as usize].0))
    }
}

impl ComponentRun {
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn graph(&self) -> Graph<'_> {
        Graph {
            states: &self.states,
            complete: self.complete,
            edges: &self.edges,
            bindings: &self.bindings,
        }
    }

    /// The action and its params on an edge.
    pub fn step(&self, edge: &Edge) -> (ActId, &[u8]) {
        let (act, params) = &self.bindings[edge.binding as usize];
        (*act, params)
    }

    /// The edge that first reached `states[at]`; `None` for an initial state.
    pub fn parent(&self, at: u32) -> Option<&Edge> {
        self.parents[at as usize].map(|e| &self.edges[e as usize])
    }

    /// The BFS path from an initial state to `states[at]`, first edge first.
    pub fn path_to(&self, mut at: u32) -> Vec<Edge> {
        let mut path = Vec::new();
        while let Some(edge) = self.parent(at) {
            path.push(*edge);
            at = edge.from;
        }
        path.reverse();
        path
    }

    /// The BFS path to `states[at]` in model terms: a shortest one, since BFS reaches states
    /// in depth order.
    pub fn trace_to(&self, at: u32) -> Trace {
        let path = self.path_to(at);
        let start = path.first().map_or(at, |e| e.from);
        let ts = &self.system;
        Trace {
            start: ts.trace_start(&self.states[start as usize]),
            steps: path
                .iter()
                .map(|e| {
                    let (act, params) = self.step(e);
                    let (pre, post) = (&self.states[e.from as usize], &self.states[e.to as usize]);
                    ts.trace_step(act, params, pre, post)
                })
                .collect(),
            over_bound: Vec::new(),
        }
    }

    /// The trace of a violation: the path to the violating state, or for an after-assert the
    /// path to the step's source and the step itself. The step need not be on the BFS path to
    /// its target, which another path may reach first.
    fn trace_of(&self, reach: &Reach) -> Trace {
        match reach {
            Reach::State(at) => self.trace_to(*at),
            Reach::Step {
                from,
                act,
                params,
                to,
            } => {
                let mut trace = self.trace_to(*from);
                let pre = &self.states[*from as usize];
                trace
                    .steps
                    .push(self.system.trace_step(*act, params, pre, to));
                trace
            }
        }
    }
}

/// Where a violation was seen.
enum Reach {
    /// `states[at]` violates an invariant.
    State(u32),
    /// The step from `states[from]` to `to` fails an after-assert.
    Step {
        from: u32,
        act: ActId,
        params: Box<[u8]>,
        to: State,
    },
}

/// What the edges said about one entity's view patterns, indexed by pattern id.
#[derive(Default)]
pub(crate) struct EntityView {
    pub patterns: Vec<Box<[u8]>>,
    index: HashMap<Box<[u8]>, usize>,
    pub initial: Vec<bool>,
    /// Some enabled action moves a slot off this pattern.
    pub changes: Vec<bool>,
    /// Some enabled action deletes a slot in this pattern.
    pub deletable: Vec<bool>,
    /// Actions that create, delete, or touch a slot in this pattern without moving it.
    touch: Vec<BTreeSet<ActId>>,
    /// (from pattern, action, to pattern) for every move between patterns.
    moves: BTreeSet<(usize, ActId, usize)>,
    pub truncated: bool,
}

impl EntityView {
    pub(crate) fn id(&self, key: &[u8]) -> Option<usize> {
        self.index.get(key).copied()
    }

    fn intern(&mut self, key: Box<[u8]>) -> usize {
        if let Some(id) = self.index.get(&key) {
            return *id;
        }
        let id = self.patterns.len();
        self.index.insert(key.clone(), id);
        self.patterns.push(key);
        self.initial.push(false);
        self.changes.push(false);
        self.deletable.push(false);
        self.touch.push(BTreeSet::new());
        id
    }

    /// The actions on some path to each pattern: the least set closed under the moves.
    /// It depends only on which moves exist, not on the order BFS found them.
    pub fn via(&self) -> Vec<BTreeSet<ActId>> {
        let mut via = self.touch.clone();
        loop {
            let mut grew = false;
            for &(from, act, to) in &self.moves {
                let mut add = via[from].clone();
                add.insert(act);
                if !add.is_subset(&via[to]) {
                    via[to].extend(add);
                    grew = true;
                }
            }
            if !grew {
                return via;
            }
        }
    }
}

impl TransitionSystem {
    /// Explores every component separately.
    pub fn explore(&self, caps: Caps) -> Exploration {
        let runs = self
            .components()
            .into_iter()
            .map(|component| {
                let system = self.restrict(&component);
                let mut run = system.run(caps);
                run.component = component;
                run
            })
            .collect();
        Exploration { runs }
    }

    /// Explores this system as one component.
    pub fn run(self, caps: Caps) -> ComponentRun {
        let component = Component {
            entities: (0..self.entities.len()).map(EntId::at).collect(),
            now: self.now.is_some(),
        };
        let mut bfs = Bfs::new(&self, caps);
        bfs.go();
        let Bfs {
            states,
            complete,
            diags,
            views,
            parents,
            edges,
            bindings,
            violations,
            ..
        } = bfs;
        let mut run = ComponentRun {
            component,
            system: self,
            states,
            complete,
            diags,
            views,
            parents,
            edges,
            bindings: bindings.list,
        };
        for (at, reach) in &violations {
            let trace = run.trace_of(reach);
            if let Some(t) = run.diags[*at].diag.trace_mut() {
                *t = trace;
            }
        }
        run
    }

    pub(crate) fn pattern_key(&self, state: &State, e: EntId, slot: usize) -> Box<[u8]> {
        self.entity(e)
            .columns
            .iter()
            .map(|v| state.get(self, *v, slot).0)
            .collect()
    }

    pub(crate) fn initial_states(&self) -> Vec<State> {
        let mut base = self.base_state();
        let mut free: Vec<(VarId, usize, usize)> = Vec::new();
        for decl in &self.entities {
            if decl.birth == super::types::Birth::PreExisting {
                for slot in 0..self.bounds.slots() {
                    base.set(self, decl.exists, slot, Value::PRESENT);
                    for &var in decl.columns.iter().chain(&decl.owners) {
                        let d = self.var(var);
                        match d.start {
                            Start::Base => {}
                            Start::Any => free.push((var, slot, d.domain.size(self.bounds))),
                            Start::Identity => {
                                base.set(self, var, slot, d.domain.of_slot(slot as u8));
                            }
                        }
                    }
                }
            }
        }
        let mut out = vec![base];
        for (var, slot, size) in free {
            out = out
                .into_iter()
                .flat_map(|s| {
                    (0..size).map(move |v| {
                        let mut s = s.clone();
                        s.set(self, var, slot, Value(v as u8));
                        s
                    })
                })
                .collect();
        }
        out
    }

    /// Slot choices for each param: the lowest absent slot for `New`, every present slot
    /// otherwise.
    fn bindings(&self, action: &Action, state: &State) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = vec![Vec::new()];
        for p in &action.params {
            let exists = self.entity(p.entity).exists;
            let slots: Vec<u8> = match p.role {
                ParamRole::New => (0..self.bounds.slots())
                    .find(|&s| state.get(self, exists, s) == Value::ABSENT)
                    .map(|s| s as u8)
                    .into_iter()
                    .collect(),
                ParamRole::Existing | ParamRole::Parent => (0..self.bounds.slots())
                    .filter(|&s| state.get(self, exists, s) == Value::PRESENT)
                    .map(|s| s as u8)
                    .collect(),
            };
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    slots.iter().map(move |s| {
                        let mut b = prefix.clone();
                        b.push(*s);
                        b
                    })
                })
                .collect();
            if out.is_empty() {
                break;
            }
        }
        out
    }

    fn successors(&self, action: &Action, params: &[u8], s: &State) -> Vec<State> {
        let slot = |p: u8| usize::from(params[usize::from(p)]);
        let mut next = s.clone();
        let mut deleted: Vec<u8> = Vec::new();
        let mut choices: Vec<(VarId, u8, &[Value])> = Vec::new();
        for eff in &action.effects {
            match eff {
                Effect::Create { param } => {
                    let e = action.params[usize::from(*param)].entity;
                    next.set(self, self.entity(e).exists, slot(*param), Value::PRESENT);
                }
                Effect::Delete { param } => deleted.push(*param),
                Effect::Assign {
                    var, param, rhs, ..
                } => {
                    let v = match rhs {
                        Rhs::Lit(v) => *v,
                        Rhs::Param(q) => self.var(*var).domain.of_slot(params[usize::from(*q)]),
                    };
                    next.set(self, *var, slot(*param), v);
                }
                Effect::Transition {
                    var, param, arms, ..
                } => {
                    let from = s.get(self, *var, slot(*param));
                    if let Some((_, to)) = arms.iter().find(|(f, _)| *f == from) {
                        next.set(self, *var, slot(*param), *to);
                    }
                }
                Effect::Choose { var, param, values } => choices.push((*var, *param, values)),
                Effect::Tick => {
                    let now = self.now.expect("tick without a clock");
                    let v = s.get(self, now, 0);
                    next.set(self, now, 0, Value(v.0 + 1));
                }
            }
        }
        for &p in &deleted {
            let e = action.params[usize::from(p)].entity;
            let decl = self.entity(e);
            for &var in decl.columns.iter().chain(decl.owners.iter()) {
                next.set(self, var, slot(p), self.var(var).base);
            }
            next.set(self, decl.exists, slot(p), Value::DELETED);
        }
        choices.retain(|(_, p, _)| !deleted.contains(p));
        let mut out = vec![next];
        for (var, p, values) in choices {
            let sl = slot(p);
            out = out
                .into_iter()
                .flat_map(|st| {
                    values.iter().map(move |v| {
                        let mut st = st.clone();
                        st.set(self, var, sl, *v);
                        st
                    })
                })
                .collect();
        }
        out
    }

    /// Every move from `s`, in BFS order: actions by index, then `bindings`, then the
    /// successors of each binding whose pre-guards hold, keeping those whose post-guards hold.
    pub(crate) fn moves<'s>(&'s self, s: &'s State) -> impl Iterator<Item = Move> + 's {
        self.actions
            .iter()
            .enumerate()
            .flat_map(move |(ai, action)| {
                self.bindings(action, s)
                    .into_iter()
                    .filter(move |params| {
                        let mut env = Env::new(params);
                        action
                            .guards
                            .iter()
                            .filter(|g| !g.reads_successor())
                            .all(|g| self.eval(&g.pred, s, &mut env))
                    })
                    .flat_map(move |params| {
                        let params: Box<[u8]> = params.into();
                        self.successors(action, &params, s)
                            .into_iter()
                            .filter(|next| {
                                let mut env = Env {
                                    next: Some(next),
                                    ..Env::new(&params)
                                };
                                action
                                    .guards
                                    .iter()
                                    .filter(|g| g.reads_successor())
                                    .all(|g| self.eval(&g.pred, s, &mut env))
                            })
                            .map(|next| Move {
                                act: ActId::at(ai),
                                params: params.clone(),
                                next,
                            })
                            .collect::<Vec<_>>()
                    })
            })
    }
}

impl TransitionSystem {
    /// The violation of `invariants()[i]` in `s`, with an empty trace; `None` when it holds.
    /// BFS and the replay of a TLC trace both build the diagnostic here.
    pub(crate) fn invariant_violation(&self, i: usize, s: &State) -> Option<Diag> {
        let inv = &self.invariants[i];
        let binding = self.find_violation(&inv.binders, &inv.body, s, &[])?;
        let local = matches!(
            inv.kind,
            InvKind::Forbidden
                | InvKind::Invariant
                | InvKind::Required
                | InvKind::Exclusive
                | InvKind::Check
        );
        let pattern_desc = if local {
            describe_slot(self, s, inv.binders[0], usize::from(binding[0]), false)
        } else {
            inv.binders
                .iter()
                .zip(&binding)
                .map(|(e, slot)| describe_slot(self, s, *e, usize::from(*slot), true))
                .collect::<Vec<_>>()
                .join("; ")
        };
        Some(Diag {
            entities: inv.binders.iter().map(|e| self.entity(*e).key).collect(),
            diag: StateDiag::ConstraintViolated {
                name: inv.name.clone(),
                kind: inv.kind,
                text: inv.text.clone(),
                pattern_desc,
                site: inv.site.clone(),
                trace: Trace::default(),
            },
        })
    }

    /// The violation of `after_asserts()[i]` by the step `act` with `params` into `s2`, with
    /// an empty trace; `None` when the step does not trigger it or it holds.
    pub(crate) fn after_violation(
        &self,
        i: usize,
        act: ActId,
        params: &[u8],
        s2: &State,
    ) -> Option<Diag> {
        let a = &self.after[i];
        let action = &self.actions[act.index()];
        a.steps
            .iter()
            .filter(|step| step.act == act)
            .find_map(|step| {
                let binding = self.find_violation(&step.binders, &step.post, s2, params)?;
                let mut parts: Vec<String> = Vec::new();
                let mut entities = Vec::new();
                for (p, param) in action.params.iter().enumerate() {
                    if param.role == ParamRole::Parent {
                        continue;
                    }
                    parts.push(describe_slot(
                        self,
                        s2,
                        param.entity,
                        usize::from(params[p]),
                        true,
                    ));
                    entities.push(self.entity(param.entity).key);
                }
                for (e, slot) in step.binders.iter().zip(&binding) {
                    parts.push(describe_slot(self, s2, *e, usize::from(*slot), true));
                    entities.push(self.entity(*e).key);
                }
                Some(Diag {
                    entities,
                    diag: StateDiag::AfterAssertViolated {
                        anchor: a.anchor.clone(),
                        requireds: a.requireds.clone(),
                        actual: parts.join("; "),
                        site: a.site.clone(),
                        trace: Trace::default(),
                    },
                })
            })
    }
}

/// Interns each `(action, params)` an edge names, so an edge stores one index.
#[derive(Default)]
struct Bindings {
    list: Vec<(ActId, Box<[u8]>)>,
    /// Per action: its params to their index in `list`.
    index: Vec<HashMap<Box<[u8]>, u32>>,
}

impl Bindings {
    fn id(&mut self, act: ActId, params: &[u8]) -> u32 {
        if self.index.len() <= act.index() {
            self.index.resize_with(act.index() + 1, HashMap::new);
        }
        if let Some(id) = self.index[act.index()].get(params) {
            return *id;
        }
        let id = u32::try_from(self.list.len()).expect("bindings fit in u32");
        self.list.push((act, params.into()));
        self.index[act.index()].insert(params.into(), id);
        id
    }
}

struct Bfs<'t> {
    ts: &'t TransitionSystem,
    caps: Caps,
    states: Vec<State>,
    seen: HashMap<State, u32>,
    complete: bool,
    diags: Vec<Diag>,
    views: Vec<EntityView>,
    /// Per action, per param: the action writes that param's existence or view columns.
    writes: Vec<Vec<bool>>,
    /// Invariants and asserts with a reported violation: each reports its first one only.
    violated: HashSet<String>,
    /// Per reported violation: its index in `diags` and where it was seen.
    violations: Vec<(usize, Reach)>,
    parents: Vec<Option<u32>>,
    edges: Vec<Edge>,
    bindings: Bindings,
}

impl<'t> Bfs<'t> {
    fn new(ts: &'t TransitionSystem, caps: Caps) -> Self {
        let writes = ts
            .actions
            .iter()
            .map(|a| {
                (0..a.params.len())
                    .map(|p| {
                        let columns = &ts.entity(a.params[p].entity).columns;
                        a.effects.iter().any(|eff| match eff {
                            Effect::Create { param } | Effect::Delete { param } => {
                                usize::from(*param) == p
                            }
                            Effect::Assign { var, param, .. }
                            | Effect::Transition { var, param, .. }
                            | Effect::Choose { var, param, .. } => {
                                usize::from(*param) == p && columns.contains(var)
                            }
                            Effect::Tick => false,
                        })
                    })
                    .collect()
            })
            .collect();
        Bfs {
            ts,
            caps,
            states: Vec::new(),
            seen: HashMap::new(),
            complete: true,
            diags: Vec::new(),
            views: ts.entities.iter().map(|_| EntityView::default()).collect(),
            writes,
            violated: HashSet::new(),
            violations: Vec::new(),
            parents: Vec::new(),
            edges: Vec::new(),
            bindings: Bindings::default(),
        }
    }

    fn go(&mut self) {
        let ts = self.ts;
        for s in ts.initial_states() {
            for (i, _) in ts.entities.iter().enumerate() {
                let e = EntId::at(i);
                for slot in 0..ts.bounds.slots() {
                    if s.is_present(ts, e, slot) {
                        let key = ts.pattern_key(&s, e, slot);
                        let Some(id) = self.intern(e, key) else {
                            return;
                        };
                        self.views[i].initial[id] = true;
                    }
                }
            }
            if !self.admit(s, None) {
                return;
            }
        }
        let mut next = 0;
        while next < self.states.len() {
            let from = next as u32;
            let s = self.states[next].clone();
            next += 1;
            for Move {
                act,
                params,
                next: s2,
            } in ts.moves(&s)
            {
                if !self.observe(act, &params, &s, &s2) {
                    return;
                }
                let binding = self.bindings.id(act, &params);
                let edge = self.edges.len() as u32;
                if s2 == s {
                    self.edges.push(Edge {
                        from,
                        to: from,
                        binding,
                    });
                    continue;
                }
                self.check_after(from, act, &params, &s2);
                let to = match self.seen.get(&s2) {
                    Some(&to) => to,
                    None => {
                        let to = self.states.len() as u32;
                        if !self.admit(s2, Some(edge)) {
                            return;
                        }
                        to
                    }
                };
                self.edges.push(Edge { from, to, binding });
            }
        }
    }

    /// Adds a new state; false when the state cap stops the run. `parent` is the index in
    /// `edges` that the edge reaching it takes.
    fn admit(&mut self, s: State, parent: Option<u32>) -> bool {
        if self.seen.contains_key(&s) {
            return true;
        }
        if self.states.len() >= self.caps.states {
            self.complete = false;
            let entities = self.ts.entities.iter().map(|e| e.key).collect();
            self.diags.push(Diag {
                entities,
                diag: StateDiag::StateCapReached {
                    cap: self.caps.states,
                },
            });
            return false;
        }
        self.check_invariants(&s, self.states.len() as u32);
        self.seen.insert(s.clone(), self.states.len() as u32);
        self.states.push(s);
        self.parents.push(parent);
        true
    }

    /// Interns a view pattern; `None` when the pattern cap stops the run.
    fn intern(&mut self, e: EntId, key: Box<[u8]>) -> Option<usize> {
        let view = &mut self.views[e.index()];
        if view.id(&key).is_none() && view.patterns.len() >= self.caps.patterns {
            view.truncated = true;
            self.complete = false;
            let bound = pattern_bound(self.ts, e);
            self.diags.push(Diag {
                entities: vec![self.ts.entity(e).key],
                diag: StateDiag::PatternCapReached {
                    cap: self.caps.patterns,
                    bound,
                },
            });
            return None;
        }
        Some(view.intern(key))
    }

    /// Records what one edge says about each written slot's pattern.
    fn observe(&mut self, act: ActId, params: &[u8], s: &State, s2: &State) -> bool {
        let ts = self.ts;
        let action = &ts.actions[act.index()];
        for (p, param) in action.params.iter().enumerate() {
            if param.role == ParamRole::Parent {
                continue;
            }
            let (e, slot) = (param.entity, usize::from(params[p]));
            let (pre, post) = (s.is_present(ts, e, slot), s2.is_present(ts, e, slot));
            let pre_id = pre.then(|| {
                let key = ts.pattern_key(s, e, slot);
                self.views[e.index()]
                    .id(&key)
                    .expect("pre-state pattern interned")
            });
            match (pre_id, post) {
                (None, true) => {
                    let Some(id) = self.intern(e, ts.pattern_key(s2, e, slot)) else {
                        return false;
                    };
                    let view = &mut self.views[e.index()];
                    view.initial[id] = true;
                    view.touch[id].insert(act);
                }
                (Some(id), false) => {
                    let view = &mut self.views[e.index()];
                    view.deletable[id] = true;
                    view.touch[id].insert(act);
                }
                (Some(id), true) => {
                    let Some(id2) = self.intern(e, ts.pattern_key(s2, e, slot)) else {
                        return false;
                    };
                    let view = &mut self.views[e.index()];
                    if id2 != id {
                        view.changes[id] = true;
                        view.moves.insert((id, act, id2));
                    } else if !self.writes[act.index()][p] {
                        view.touch[id].insert(act);
                    }
                }
                (None, false) => {}
            }
        }
        true
    }

    /// Reports the violation unless its rule already has one; the trace is filled in when the
    /// run ends.
    fn violation(&mut self, name: &str, diag: Diag, reach: Reach) {
        if self.violated.insert(name.to_string()) {
            self.violations.push((self.diags.len(), reach));
            self.diags.push(diag);
        }
    }

    fn check_invariants(&mut self, s: &State, at: u32) {
        let ts = self.ts;
        for (i, inv) in ts.invariants.iter().enumerate() {
            if self.violated.contains(&inv.name) {
                continue;
            }
            if let Some(d) = ts.invariant_violation(i, s) {
                self.violation(&inv.name, d, Reach::State(at));
            }
        }
    }

    fn check_after(&mut self, from: u32, act: ActId, params: &[u8], s2: &State) {
        let ts = self.ts;
        for (i, a) in ts.after.iter().enumerate() {
            if self.violated.contains(&a.name) {
                continue;
            }
            if let Some(d) = ts.after_violation(i, act, params, s2) {
                let reach = Reach::Step {
                    from,
                    act,
                    params: params.into(),
                    to: s2.clone(),
                };
                self.violation(&a.name, d, reach);
            }
        }
    }
}
