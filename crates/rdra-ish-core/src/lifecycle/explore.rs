//! BFS over one component's flat states. Guards, invariants, and after-asserts are checked on
//! the fly; per-entity view statistics are gathered from the edges as they are found.

use std::collections::{BTreeSet, HashMap};

use super::diag::{Diag, StateDiag};
use super::expr::{Env, State};
use super::restrict::Component;
use super::types::{ActId, Action, Effect, EntId, InvKind, ParamRole, Rhs, Start, Value, VarId};
use super::view::{describe_slot, pattern_bound};
use super::TransitionSystem;

/// Distinct witnesses kept per invariant or assert.
const WITNESS_CAP: usize = 16;

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
    pub via: Vec<BTreeSet<ActId>>,
    pub truncated: bool,
}

impl EntityView {
    fn id(&self, key: &[u8]) -> Option<usize> {
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
        self.via.push(BTreeSet::new());
        id
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
            ..
        } = bfs;
        ComponentRun {
            component,
            system: self,
            states,
            complete,
            diags,
            views,
        }
    }

    pub(crate) fn pattern_key(&self, state: &State, e: EntId, slot: usize) -> Box<[u8]> {
        self.entity(e)
            .columns
            .iter()
            .map(|v| state.get(self, *v, slot).0)
            .collect()
    }

    fn initial_states(&self) -> Vec<State> {
        let mut base = State(vec![0u8; self.width].into_boxed_slice());
        let mut free: Vec<(VarId, usize, usize)> = Vec::new();
        for (i, decl) in self.vars.iter().enumerate() {
            let var = VarId::at(i);
            let slots = if decl.kind == super::types::VarKind::Now {
                1
            } else {
                self.bounds.slots()
            };
            for slot in 0..slots {
                base.set(self, var, slot, decl.base);
            }
        }
        for decl in &self.entities {
            if decl.birth == super::types::Birth::PreExisting {
                for slot in 0..self.bounds.slots() {
                    base.set(self, decl.exists, slot, Value::PRESENT);
                    for &var in &decl.columns {
                        if self.var(var).start == Start::Any {
                            free.push((var, slot, self.var(var).domain.size(self.bounds)));
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
                        Rhs::Param(q) => Value(params[usize::from(*q)]),
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
}

struct Bfs<'t> {
    ts: &'t TransitionSystem,
    caps: Caps,
    states: Vec<State>,
    seen: HashMap<State, u32>,
    complete: bool,
    diags: Vec<Diag>,
    views: Vec<EntityView>,
    /// Per action, per param: the action writes that param's slot.
    writes: Vec<Vec<bool>>,
    witnesses: HashMap<String, usize>,
}

impl<'t> Bfs<'t> {
    fn new(ts: &'t TransitionSystem, caps: Caps) -> Self {
        let writes = ts
            .actions
            .iter()
            .map(|a| {
                (0..a.params.len())
                    .map(|p| {
                        a.effects.iter().any(|eff| match eff {
                            Effect::Create { param }
                            | Effect::Delete { param }
                            | Effect::Assign { param, .. }
                            | Effect::Transition { param, .. }
                            | Effect::Choose { param, .. } => usize::from(*param) == p,
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
            witnesses: HashMap::new(),
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
            if !self.admit(s) {
                return;
            }
        }
        let mut next = 0;
        while next < self.states.len() {
            let s = self.states[next].clone();
            next += 1;
            for (ai, action) in ts.actions.iter().enumerate() {
                for params in ts.bindings(action, &s) {
                    let mut env = Env::new(&params);
                    if !action.guards.iter().all(|g| ts.eval(&g.pred, &s, &mut env)) {
                        continue;
                    }
                    for s2 in ts.successors(action, &params, &s) {
                        if !self.observe(ActId::at(ai), &params, &s, &s2) {
                            return;
                        }
                        if s2 == s {
                            continue;
                        }
                        self.check_after(ActId::at(ai), &params, &s2);
                        if !self.seen.contains_key(&s2) && !self.admit(s2) {
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Adds a new state; false when the state cap stops the run.
    fn admit(&mut self, s: State) -> bool {
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
        self.check_invariants(&s);
        self.seen.insert(s.clone(), self.states.len() as u32);
        self.states.push(s);
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
                    view.via[id].insert(act);
                }
                (Some(id), false) => {
                    let view = &mut self.views[e.index()];
                    view.deletable[id] = true;
                    view.via[id].insert(act);
                }
                (Some(id), true) => {
                    let Some(id2) = self.intern(e, ts.pattern_key(s2, e, slot)) else {
                        return false;
                    };
                    let view = &mut self.views[e.index()];
                    if id2 != id {
                        view.changes[id] = true;
                        let mut via = view.via[id].clone();
                        via.insert(act);
                        view.via[id2].extend(via);
                    } else if !self.writes[act.index()][p] {
                        view.via[id].insert(act);
                    }
                }
                (None, false) => {}
            }
        }
        true
    }

    fn witness(&mut self, name: &str, diag: StateDiag, entities: Vec<crate::model::EntityKey>) {
        let count = self.witnesses.entry(name.to_string()).or_insert(0);
        if *count >= WITNESS_CAP {
            return;
        }
        let d = Diag { entities, diag };
        if !self.diags.contains(&d) {
            *count += 1;
            self.diags.push(d);
        }
    }

    fn check_invariants(&mut self, s: &State) {
        let ts = self.ts;
        for inv in &ts.invariants {
            let Some(binding) = ts.find_violation(&inv.binders, &inv.body, s, &[]) else {
                continue;
            };
            let local = matches!(
                inv.kind,
                InvKind::Forbidden | InvKind::Invariant | InvKind::Required | InvKind::Exclusive
            );
            let pattern_desc = if local {
                describe_slot(ts, s, inv.binders[0], usize::from(binding[0]), false)
            } else {
                inv.binders
                    .iter()
                    .zip(&binding)
                    .map(|(e, slot)| describe_slot(ts, s, *e, usize::from(*slot), true))
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let entities = inv.binders.iter().map(|e| ts.entity(*e).key).collect();
            let diag = StateDiag::ConstraintViolated {
                name: inv.name.clone(),
                kind: inv.kind,
                text: inv.text.clone(),
                pattern_desc,
                site: inv.site.clone(),
            };
            self.witness(&inv.name, diag, entities);
        }
    }

    fn check_after(&mut self, act: ActId, params: &[u8], s2: &State) {
        let ts = self.ts;
        let action = &ts.actions[act.index()];
        for a in ts.after.iter().filter(|a| a.on.contains(&act)) {
            let Some(binding) = ts.find_violation(&a.binders, &a.post, s2, params) else {
                continue;
            };
            let mut parts: Vec<String> = Vec::new();
            let mut entities = Vec::new();
            for (p, param) in action.params.iter().enumerate() {
                if param.role == ParamRole::Parent {
                    continue;
                }
                parts.push(describe_slot(
                    ts,
                    s2,
                    param.entity,
                    usize::from(params[p]),
                    true,
                ));
                entities.push(ts.entity(param.entity).key);
            }
            for (e, slot) in a.binders.iter().zip(&binding) {
                parts.push(describe_slot(ts, s2, *e, usize::from(*slot), true));
                entities.push(ts.entity(*e).key);
            }
            let diag = StateDiag::AfterAssertViolated {
                anchor: action.name.clone(),
                requireds: a.requireds.clone(),
                actual: parts.join("; "),
                site: a.site.clone(),
            };
            self.witness(&a.name, diag, entities);
        }
    }
}
