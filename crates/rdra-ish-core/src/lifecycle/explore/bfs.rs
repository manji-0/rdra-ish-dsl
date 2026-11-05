//! The BFS itself: interning bindings, admitting states, and checking each move.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::lifecycle::diag::{CapGrowth, Diag, StateDiag, VarSpread};
use crate::lifecycle::expr::State;
use crate::lifecycle::types::{ActId, Domain, EntId, Value, VarKind};
use crate::lifecycle::view::pattern_bound;
use crate::lifecycle::TransitionSystem;

use super::{Bfs, Bindings, Caps, Edge, EntityView, Move, Reach};

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

impl<'t> Bfs<'t> {
    pub(super) fn new(ts: &'t TransitionSystem, caps: Caps) -> Self {
        Bfs {
            ts,
            caps,
            states: Vec::new(),
            seen: HashMap::new(),
            complete: true,
            diags: Vec::new(),
            views: ts.entities.iter().map(|_| EntityView::default()).collect(),
            violated: HashSet::new(),
            violations: Vec::new(),
            parents: Vec::new(),
            edges: Vec::new(),
            bindings: Bindings::default(),
        }
    }

    pub(super) fn go(&mut self) {
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
                    growth: self.growth(),
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

    /// What the reached states vary in: the distinct values of each variable, and the use
    /// case of the edge that first reached each state.
    fn growth(&self) -> CapGrowth {
        let ts = self.ts;
        let mut vars: Vec<VarSpread> = ts
            .var_ids()
            .map(|v| {
                let decl = ts.var(v);
                let slots = match decl.kind {
                    VarKind::Now => 1,
                    _ => ts.bounds.slots(),
                };
                let reached: BTreeSet<Value> = self
                    .states
                    .iter()
                    .flat_map(|s| (0..slots).map(move |slot| s.get(ts, v, slot)))
                    .collect();
                let (name, range) = match &decl.kind {
                    VarKind::Now => ("now".to_string(), Some("clock".to_string())),
                    VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => {
                        let range = matches!(decl.domain, Domain::Int | Domain::NullableInt)
                            .then(|| format!("Int 0..{}", ts.bounds.int_max));
                        let name = format!("{}.{}", ts.entity(*entity).id, ts.var_name(v));
                        (name, range)
                    }
                    VarKind::Exists(_)
                    | VarKind::Owner { .. }
                    | VarKind::Run { .. }
                    | VarKind::Raised { .. } => (ts.var_name(v), None),
                };
                VarSpread {
                    name,
                    reached: reached.len(),
                    domain: decl.domain.size(ts.bounds),
                    range,
                }
            })
            .collect();
        vars.sort_by(|a, b| b.reached.cmp(&a.reached).then(a.name.cmp(&b.name)));
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for edge in self.parents.iter().flatten() {
            let (act, _) = self.bindings.list[self.edges[*edge as usize].binding as usize];
            *counts
                .entry(ts.actions[act.index()].name.as_str())
                .or_default() += 1;
        }
        let mut usecases: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(name, n)| (name.to_string(), n))
            .collect();
        usecases.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        CapGrowth { vars, usecases }
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
            let in_s = ts.reduction().is_some_and(|r| r.is_s_entity(param.entity));
            if !param.role.is_written() || in_s {
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
                self.violation(&inv.name, d, Reach::State { at, inv: i });
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
                    after: i,
                };
                self.violation(&a.name, d, reach);
            }
        }
    }
}
