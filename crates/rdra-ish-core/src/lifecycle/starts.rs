//! The start values of an entity's lifecycle column, for diagrams that draw the initial edge.

use super::{
    Action, Birth, Domain, Effect, EntId, ParamRole, Start, TransitionSystem, Value, VarId, VarKind,
};
use crate::model::EntityKey;

impl TransitionSystem {
    /// The values a fresh slot of `entity`'s enum column `column` can hold, in declared order.
    ///
    /// A created entity starts as each creating action leaves it: the assigned or chosen value,
    /// the `to` of a transition arm that fires from the base value, else the base. A
    /// pre-existing entity starts at its base (`Start::Any` columns at every value). Empty when
    /// the column is not an enum column of the system.
    pub fn column_start_values(&self, entity: EntityKey, column: &str) -> Vec<&str> {
        let Some((ent_id, ent)) = self
            .entity_ids()
            .map(|id| (id, self.entity(id)))
            .find(|(_, e)| e.key == entity)
        else {
            return Vec::new();
        };
        let Some(var) = ent.columns.iter().copied().find(
            |v| matches!(&self.var(*v).kind, VarKind::Column { column: c, .. } if c == column),
        ) else {
            return Vec::new();
        };
        let Domain::Enum(names) = &self.var(var).domain else {
            return Vec::new();
        };
        self.start_values(ent_id, var)
            .into_iter()
            .filter_map(|v| names.get(v.0 as usize).map(String::as_str))
            .collect()
    }

    /// The values a fresh slot of `ent_id` holds in `var`, sorted: for a created entity what
    /// each creating action leaves, for a pre-existing one its start.
    pub(crate) fn start_values(&self, ent_id: EntId, var: VarId) -> Vec<Value> {
        let decl = self.var(var);
        let mut starts: Vec<Value> = Vec::new();
        if self.entity(ent_id).birth == Birth::PreExisting {
            match decl.start {
                Start::Base | Start::Identity => starts.push(decl.base),
                Start::Any => {
                    starts.extend((0..decl.domain.size(self.bounds)).map(|i| Value(i as u8)))
                }
            }
        } else {
            for (action, param) in self.creations(ent_id) {
                starts.extend(Self::created_value(action, var, param, decl.base));
            }
        }
        starts.sort();
        starts.dedup();
        starts
    }

    /// Each action that creates `ent_id`, with the param it creates.
    pub(crate) fn creations(&self, ent_id: EntId) -> impl Iterator<Item = (&Action, u8)> + '_ {
        self.actions.iter().flat_map(move |action| {
            action.effects.iter().filter_map(move |effect| {
                let Effect::Create { param } = effect else {
                    return None;
                };
                let p = action.params[usize::from(*param)];
                (p.entity == ent_id && p.role == ParamRole::New).then_some((action, *param))
            })
        })
    }

    pub(crate) fn created_value(action: &Action, var: VarId, param: u8, base: Value) -> Vec<Value> {
        let mut out = Vec::new();
        for effect in &action.effects {
            match effect {
                Effect::Assign {
                    var: v,
                    param: q,
                    rhs: super::Rhs::Lit(value),
                    ..
                } if *v == var && *q == param => out.push(*value),
                Effect::Choose {
                    var: v,
                    param: q,
                    values,
                } if *v == var && *q == param => out.extend(values),
                Effect::Transition {
                    var: v,
                    param: q,
                    arms,
                    ..
                } if *v == var && *q == param => {
                    out.extend(arms.iter().filter(|(from, _)| *from == base).map(|a| a.1));
                }
                _ => {}
            }
        }
        if out.is_empty() {
            out.push(base);
        }
        out
    }
}
