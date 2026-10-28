//! What a cone's graph says about an entity's state models. A pure function of the system and
//! the graph, so any engine that produces the graph gets the same findings.

use super::diag::{Entry, StateDiag};
use super::explore::Graph;
use super::types::{Domain, Effect, EntId, Value, VarId, VarKind};
use super::TransitionSystem;

/// What the graph says about one state of a state model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Witness {
    /// `states[at]` is the first state in discovery order where a present slot holds it.
    At(u32),
    /// No state of a complete graph holds it: a `NoWitness` error.
    Missing(Box<StateDiag>),
    /// No state holds it, but a cap stopped the exploration, so it may still be reachable.
    NotDecided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueWitness {
    pub var: VarId,
    pub value: Value,
    pub witness: Witness,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateModelFindings {
    /// Each state of the entity's state models, column by column in declared value order.
    pub witnesses: Vec<ValueWitness>,
}

impl TransitionSystem {
    /// The states of `e`'s state models: on each lifecycle column (variation columns are the
    /// actor's choice), the `transitions` endpoints and the values a fresh slot starts at.
    pub(crate) fn state_model_values(&self, e: EntId) -> Vec<(VarId, Value)> {
        let mut out = Vec::new();
        for &var in &self.entity(e).columns {
            let decl = self.var(var);
            let lifecycle = matches!(
                decl.kind,
                VarKind::Column {
                    lifecycle: true,
                    variation: false,
                    ..
                }
            );
            if !lifecycle || !matches!(decl.domain, Domain::Enum(_)) {
                continue;
            }
            let mut values = self.start_values(e, var);
            for action in &self.actions {
                for effect in &action.effects {
                    if let Effect::Transition { var: v, arms, .. } = effect {
                        if *v == var {
                            values.extend(arms.iter().flat_map(|(from, to)| [*from, *to]));
                        }
                    }
                }
            }
            values.sort();
            values.dedup();
            out.extend(values.into_iter().map(|v| (var, v)));
        }
        out
    }

    /// Witnesses of `e`'s state models on `g`, a graph of this system.
    pub fn state_model_findings(&self, e: EntId, g: &Graph) -> StateModelFindings {
        let targets = self.state_model_values(e);
        let mut first: Vec<Option<u32>> = vec![None; targets.len()];
        let mut open = targets.len();
        for (at, s) in g.states.iter().enumerate() {
            if open == 0 {
                break;
            }
            for slot in 0..self.bounds.slots() {
                if !s.is_present(self, e, slot) {
                    continue;
                }
                for (i, (var, value)) in targets.iter().enumerate() {
                    if first[i].is_none() && s.get(self, *var, slot) == *value {
                        first[i] = Some(at as u32);
                        open -= 1;
                    }
                }
            }
        }
        let witnesses = targets
            .into_iter()
            .zip(first)
            .map(|((var, value), at)| ValueWitness {
                var,
                value,
                witness: match at {
                    Some(at) => Witness::At(at),
                    None if g.complete => {
                        Witness::Missing(Box::new(self.no_witness(e, var, value)))
                    }
                    None => Witness::NotDecided,
                },
            })
            .collect();
        StateModelFindings { witnesses }
    }

    fn no_witness(&self, e: EntId, var: VarId, value: Value) -> StateDiag {
        let text = |v: Value| match &self.var(var).domain {
            Domain::Enum(names) => names[usize::from(v.0)].clone(),
            _ => v.0.to_string(),
        };
        let mut into = Vec::new();
        for action in &self.actions {
            for effect in &action.effects {
                if let Effect::Transition {
                    var: v,
                    arms,
                    sites,
                    ..
                } = effect
                {
                    if *v != var {
                        continue;
                    }
                    for ((from, to), site) in arms.iter().zip(sites) {
                        if *to == value && *from != value {
                            into.push(Entry {
                                action: action.name.clone(),
                                from: Some(text(*from)),
                                site: site.clone(),
                            });
                        }
                    }
                }
            }
        }
        let base = self.var(var).base;
        for (action, param) in self.creations(e) {
            if !Self::created_value(action, var, param, base).contains(&value) {
                continue;
            }
            let assigned = action.effects.iter().find_map(|effect| match effect {
                Effect::Assign {
                    var: v,
                    param: p,
                    site,
                    ..
                } if *v == var && *p == param => Some(site.clone()),
                _ => None,
            });
            into.push(Entry {
                action: action.name.clone(),
                from: None,
                site: assigned.unwrap_or_else(|| action.site.clone()),
            });
        }
        let VarKind::Column { column, .. } = &self.var(var).kind else {
            unreachable!("a state model is on a column");
        };
        StateDiag::NoWitness {
            entity: self.entity(e).id.clone(),
            column: column.clone(),
            value: text(value),
            into,
        }
    }
}
