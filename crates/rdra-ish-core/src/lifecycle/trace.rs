//! A path from an initial state in model terms: the actions with their bound instances, and
//! what each step changed with the site of the effect that wrote it.

use std::collections::BTreeMap;
use std::fmt;

use super::cone::Cone;
use super::expr::State;
use super::types::{
    ActId, ActionOrigin, Domain, Effect, EntId, ParamRole, Site, Value, VarId, VarKind,
};
use super::view::AbstractValue;
use super::TransitionSystem;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    /// The instances the initial state already holds (pre-existing entities), as creations.
    pub start: Vec<Change>,
    pub steps: Vec<TraceStep>,
    /// Entities the trace creates more instances of than the bound allows. The cone that
    /// found it does not count their slots (`cone_frees_slots`), so the whole model at this
    /// bound may not reach the same state.
    pub over_bound: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    /// `Place`, `Pay_1`, or `Tick`.
    pub action: String,
    pub origin: ActionOrigin,
    /// The use case declaration; `Synthesized` for `Tick`.
    pub site: Site,
    /// Every param, in param order.
    pub bound: Vec<Inst>,
    pub changes: Vec<Change>,
}

/// `Order#1`: the 1-based slot, so the label matches TLC's `Order_exists[1]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inst {
    pub entity: String,
    pub slot: u8,
}

impl fmt::Display for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.entity, self.slot)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The instance became present, holding these `(field, value)`s.
    Create {
        at: Inst,
        values: Vec<(String, String)>,
    },
    Delete {
        at: Inst,
    },
    /// A column or owner of an instance present before and after the step.
    Set {
        at: Inst,
        field: String,
        from: String,
        to: String,
        /// The `transitions` arm or `sets` that wrote it; `None` for an actor's choice.
        cause: Option<Site>,
    },
    Tick {
        from: String,
        to: String,
    },
}

impl TransitionSystem {
    /// Every var at its base value: no instance present, the clock at 0.
    pub(crate) fn base_state(&self) -> State {
        let mut base = State(vec![0u8; self.width].into_boxed_slice());
        for (i, decl) in self.vars.iter().enumerate() {
            let slots = if decl.kind == VarKind::Now {
                1
            } else {
                self.bounds.slots()
            };
            for slot in 0..slots {
                base.set(self, VarId::at(i), slot, decl.base);
            }
        }
        base
    }

    fn inst(&self, e: EntId, slot: usize) -> Inst {
        Inst {
            entity: self.entity(e).id.clone(),
            slot: slot as u8 + 1,
        }
    }

    fn field_name(&self, var: VarId) -> String {
        match &self.var(var).kind {
            VarKind::Column { column, .. } => column.clone(),
            VarKind::Prop { key, .. } => key.clone(),
            VarKind::Owner { parent, role, .. } => {
                format!(
                    "owner {}",
                    role.as_deref().unwrap_or(&self.entity(*parent).id)
                )
            }
            VarKind::Exists(_) | VarKind::Now => self.var_name(var),
        }
    }

    fn value_text(&self, var: VarId, value: Value) -> String {
        match &self.var(var).domain {
            d @ Domain::Instance { parent, .. } => match d.slot(value) {
                Some(slot) => self.inst(*parent, usize::from(slot)).to_string(),
                None => "null".to_string(),
            },
            d => AbstractValue::of(d, value).display_with_type(None),
        }
    }

    /// What differs between `pre` and `post`, instance by instance in entity order. `cause`
    /// names the effect that wrote a `(var, slot)`.
    fn changes(
        &self,
        pre: &State,
        post: &State,
        cause: impl Fn(VarId, usize) -> Option<Site>,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        for (i, decl) in self.entities.iter().enumerate() {
            let e = EntId::at(i);
            let fields = || decl.columns.iter().chain(&decl.owners).copied();
            for slot in 0..self.bounds.slots() {
                let at = || self.inst(e, slot);
                match (
                    pre.is_present(self, e, slot),
                    post.is_present(self, e, slot),
                ) {
                    (false, true) => out.push(Change::Create {
                        at: at(),
                        values: fields()
                            .map(|v| {
                                (
                                    self.field_name(v),
                                    self.value_text(v, post.get(self, v, slot)),
                                )
                            })
                            .collect(),
                    }),
                    (true, false) => out.push(Change::Delete { at: at() }),
                    (true, true) => {
                        for v in fields() {
                            let (a, b) = (pre.get(self, v, slot), post.get(self, v, slot));
                            if a != b {
                                out.push(Change::Set {
                                    at: at(),
                                    field: self.field_name(v),
                                    from: self.value_text(v, a),
                                    to: self.value_text(v, b),
                                    cause: cause(v, slot),
                                });
                            }
                        }
                    }
                    (false, false) => {}
                }
            }
        }
        if let Some(now) = self.now {
            let (a, b) = (pre.get(self, now, 0), post.get(self, now, 0));
            if a != b {
                out.push(Change::Tick {
                    from: a.0.to_string(),
                    to: b.0.to_string(),
                });
            }
        }
        out
    }

    /// The pre-existing instances of an initial state.
    pub(crate) fn trace_start(&self, initial: &State) -> Vec<Change> {
        self.changes(&self.base_state(), initial, |_, _| None)
    }

    /// One step of a trace: `act` with `params` moves `pre` to `post`.
    pub(crate) fn trace_step(
        &self,
        act: ActId,
        params: &[u8],
        pre: &State,
        post: &State,
    ) -> TraceStep {
        let action = &self.actions[act.index()];
        let slot = |p: &u8| usize::from(params[usize::from(*p)]);
        let cause = |var: VarId, at: usize| {
            action.effects.iter().rev().find_map(|eff| match eff {
                Effect::Assign {
                    var: v,
                    param,
                    site,
                    ..
                } if *v == var && slot(param) == at => Some(site.clone()),
                Effect::Transition {
                    var: v,
                    param,
                    arms,
                    sites,
                } if *v == var && slot(param) == at => {
                    let from = pre.get(self, var, at);
                    arms.iter()
                        .position(|(f, _)| *f == from)
                        .map(|i| sites[i].clone())
                }
                _ => None,
            })
        };
        TraceStep {
            action: action.name.clone(),
            origin: action.origin.clone(),
            site: action.site.clone(),
            bound: action
                .params
                .iter()
                .zip(params)
                .map(|(p, s)| self.inst(p.entity, usize::from(*s)))
                .collect(),
            changes: self.changes(pre, post, cause),
        }
    }

    /// Entities whose slots the cone does not count and that the trace creates more often than
    /// the bound allows, in id order.
    pub(crate) fn over_bound(&self, cone: &Cone, trace: &Trace) -> Vec<String> {
        if !self.cone_frees_slots(cone) {
            return Vec::new();
        }
        let mut creates: BTreeMap<String, usize> = BTreeMap::new();
        for step in &trace.steps {
            let Some(action) = self.actions.iter().find(|a| a.name == step.action) else {
                continue;
            };
            for p in &action.params {
                let decl = self.entity(p.entity);
                if p.role == ParamRole::New && !cone.vars.contains(&decl.exists) {
                    *creates.entry(decl.id.clone()).or_default() += 1;
                }
            }
        }
        creates
            .into_iter()
            .filter(|(_, n)| *n > self.bounds.slots())
            .map(|(id, _)| id)
            .collect()
    }
}
