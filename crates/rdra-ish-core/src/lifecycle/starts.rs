//! The start values of an entity's lifecycle column, for diagrams that draw the initial edge.

use super::{Birth, Domain, Effect, ParamRole, Start, TransitionSystem, Value, VarKind};
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
        let decl = self.var(var);
        let Domain::Enum(names) = &decl.domain else {
            return Vec::new();
        };
        let mut starts: Vec<Value> = Vec::new();
        if ent.birth == Birth::PreExisting {
            match decl.start {
                Start::Base | Start::Identity => starts.push(decl.base),
                Start::Any => starts.extend((0..names.len()).map(|i| Value(i as u8))),
            }
        } else {
            for action in &self.actions {
                for effect in &action.effects {
                    let Effect::Create { param } = effect else {
                        continue;
                    };
                    let p = action.params[usize::from(*param)];
                    if p.entity != ent_id || p.role != ParamRole::New {
                        continue;
                    }
                    starts.extend(Self::created_value(action, var, *param, decl.base));
                }
            }
        }
        starts.sort();
        starts.dedup();
        starts
            .into_iter()
            .filter_map(|v| names.get(v.0 as usize).map(String::as_str))
            .collect()
    }

    fn created_value(
        action: &super::Action,
        var: super::VarId,
        param: u8,
        base: Value,
    ) -> Vec<Value> {
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
