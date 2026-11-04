//! What a use case's `transitions` and `chooses` do: arm guards, chosen columns, and creation choices.

use super::{variant_index, ActionDraft, Builder, GuardedArm};
use crate::lifecycle::types::{
    At, CmpOp, Domain, Effect, Guard, GuardWhy, ModelRef, Param, ParamRole, Pred, Site, Term,
    Value, VarKind,
};
use crate::model::{ConditionKey, UseCaseKey};

impl Builder<'_> {
    /// One `Transition` effect per column: its arms are the use case's `from -> to` pairs,
    /// and its guard is that the column holds one of the `from` values. Arms that share a
    /// `from` are a choice: one run takes one of them.
    pub(super) fn transition_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        let first = a.effects.len();
        for (idx, st) in model.state_transitions.iter().enumerate() {
            if st.usecase != uc {
                continue;
            }
            let e = self.ent_of[&st.entity];
            let (Some(p), Some(&var)) = (a.param_of(e), self.col_var.get(&(e, st.column.clone())))
            else {
                continue;
            };
            let Domain::Enum(values) = &self.ts.vars[var.index()].domain else {
                continue;
            };
            let (Some(from), Some(to)) = (
                variant_index(values, &st.from),
                variant_index(values, &st.to),
            ) else {
                continue;
            };
            let arm = (Value(from), Value(to));
            let site = Site::new(ModelRef::Transition(idx), &st.site);
            let guard: Vec<(ConditionKey, String)> = st
                .guard
                .map(|ck| (ck, model.conditions[ck].id.clone()))
                .into_iter()
                .collect();
            let existing = a.effects[first..].iter_mut().find_map(|eff| match eff {
                Effect::Transition {
                    var: v,
                    arms,
                    sites,
                    conds,
                    ..
                } if *v == var => Some((arms, sites, conds)),
                _ => None,
            });
            match existing {
                Some((arms, sites, conds)) => match arms.iter().position(|x| *x == arm) {
                    // The same arm declared again: either declaration enables it.
                    Some(i) if conds[i].is_empty() => {}
                    Some(i) if guard.is_empty() => conds[i].clear(),
                    Some(i) => {
                        for c in guard {
                            if !conds[i].contains(&c) {
                                conds[i].push(c);
                            }
                        }
                    }
                    None => {
                        arms.push(arm);
                        sites.push(site);
                        conds.push(guard);
                    }
                },
                None => a.effects.push(Effect::Transition {
                    var,
                    param: p,
                    arms: vec![arm],
                    sites: vec![site],
                    conds: vec![guard],
                }),
            }
        }
        for eff in &a.effects[first..] {
            let Effect::Transition {
                var,
                param,
                arms,
                sites,
                ..
            } = eff
            else {
                continue;
            };
            let at = At::Param(*param);
            let mut froms: Vec<Value> = Vec::new();
            for (from, _) in arms {
                if !froms.contains(from) {
                    froms.push(*from);
                }
            }
            let pred = Pred::Or(
                froms
                    .into_iter()
                    .map(|from| Pred::Cmp(Term::Slot { var: *var, at }, CmpOp::Eq, Term::Lit(from)))
                    .collect(),
            );
            a.guards.push(Guard {
                pred,
                why: GuardWhy::TransitionFrom,
                site: sites[0].clone(),
            });
        }
    }

    /// One successor guard per column with a `.when(Cond)` arm or chosen value: the step takes
    /// an arm whose guard held on the pre-state, `\/ (col = from /\ col' = to /\ guard)` over
    /// the `transitions` arms, `\/ (col' = v /\ guard)` over the `chooses` values (an unguarded
    /// arm's or value's guard is true). It filters the choice the `Transition` or `Choose`
    /// effect forks, so the use case runs when some arm or value is enabled, and a body on a
    /// created param reads it after the step, as a use case's condition does.
    pub(super) fn arm_guards(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        let mut guards = Vec::new();
        for eff in &a.effects {
            // Each arm or value: its comparisons on the column, its site, its conditions.
            let arms: Vec<GuardedArm> = match eff {
                Effect::Transition {
                    var,
                    param,
                    arms,
                    sites,
                    conds,
                } => {
                    let slot = |at| Term::Slot { var: *var, at };
                    arms.iter()
                        .zip(sites)
                        .zip(conds)
                        .map(|(((from, to), site), conds)| {
                            let parts = vec![
                                Pred::Cmp(slot(At::Param(*param)), CmpOp::Eq, Term::Lit(*from)),
                                Pred::Cmp(slot(At::Next(*param)), CmpOp::Eq, Term::Lit(*to)),
                            ];
                            (parts, site, conds)
                        })
                        .collect()
                }
                Effect::Choose {
                    var,
                    param,
                    values,
                    sites,
                    conds,
                } => {
                    let next = Term::Slot {
                        var: *var,
                        at: At::Next(*param),
                    };
                    values
                        .iter()
                        .zip(sites)
                        .zip(conds)
                        .map(|((v, site), conds)| {
                            let parts = vec![Pred::Cmp(next.clone(), CmpOp::Eq, Term::Lit(*v))];
                            (parts, site, conds)
                        })
                        .collect()
                }
                _ => continue,
            };
            let Some(first) = arms.iter().position(|(_, _, c)| !c.is_empty()) else {
                continue;
            };
            let mut any = Vec::new();
            for (mut parts, site, conds) in arms.iter().cloned() {
                if !conds.is_empty() {
                    let bodies = conds
                        .iter()
                        .filter(|(ck, _)| model.conditions[*ck].body.is_some())
                        .map(|(ck, _)| self.body_pred(uc, *ck, &a.params, site))
                        .collect();
                    parts.push(Pred::Or(bodies));
                }
                any.push(Pred::And(parts));
            }
            guards.push(Guard {
                pred: Pred::Or(any),
                why: GuardWhy::ArmCondition {
                    cond: arms[first].2[0].0,
                },
                site: arms[first].1.clone(),
            });
        }
        a.guards.extend(guards);
    }

    /// `chooses(UC, E.col)`: the actor picks the variation column's value again, one successor
    /// per value, as on a creation; `chooses(UC, E.col, v)` lines list the values. A value
    /// listed twice is enabled by either line, as a `transitions` arm declared twice: an
    /// unguarded line clears its guards, guarded lines join theirs. On a created param it is
    /// the creation's own choice.
    pub(super) fn chosen_columns(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        for (idx, choice) in model.column_choices.iter().enumerate() {
            if choice.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&choice.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) =
                (a.param_of(e), self.col_var.get(&(e, choice.column.clone())))
            else {
                continue;
            };
            if a.writes(var, p) {
                continue;
            }
            let fact = ModelRef::ColumnChoice(idx);
            let mut values = Vec::new();
            let mut sites = Vec::new();
            let mut conds: Vec<Vec<(ConditionKey, String)>> = Vec::new();
            if choice.values.is_empty() {
                let size = self.ts.vars[var.index()].domain.size(self.bounds);
                values = (0..size).map(|i| Value(i as u8)).collect();
                sites = vec![Site::new(fact.clone(), &choice.site); values.len()];
                conds = vec![Vec::new(); values.len()];
            }
            let names = match &self.ts.vars[var.index()].domain {
                Domain::Enum(names) => names.as_slice(),
                _ => &[],
            };
            for chosen in &choice.values {
                let Some(v) = variant_index(names, &chosen.value).map(Value) else {
                    continue;
                };
                let guard: Vec<(ConditionKey, String)> = chosen
                    .guard
                    .map(|ck| (ck, model.conditions[ck].id.clone()))
                    .into_iter()
                    .collect();
                match values.iter().position(|x| *x == v) {
                    Some(i) if conds[i].is_empty() => {}
                    Some(i) if guard.is_empty() => conds[i].clear(),
                    Some(i) => {
                        for c in guard {
                            if !conds[i].contains(&c) {
                                conds[i].push(c);
                            }
                        }
                    }
                    None => {
                        values.push(v);
                        sites.push(Site::new(fact.clone(), &chosen.site));
                        conds.push(guard);
                    }
                }
            }
            a.effects.push(Effect::Choose {
                var,
                param: p,
                values,
                sites,
                conds,
            });
        }
    }

    /// Vision rule 5: an actor chooses a variation column's value, so a created instance takes
    /// any value unless the creating use case sets it. `@default` does not pin the choice.
    pub(super) fn creation_choices(&mut self, a: &mut ActionDraft) {
        for p in 0..a.params.len() {
            let Param { entity, role, .. } = a.params[p];
            if role != ParamRole::New {
                continue;
            }
            for &var in &self.ts.entities[entity.index()].columns {
                let VarKind::Column {
                    variation: true, ..
                } = &self.ts.vars[var.index()].kind
                else {
                    continue;
                };
                if a.writes(var, p as u8) {
                    continue;
                }
                let size = self.ts.vars[var.index()].domain.size(self.bounds);
                a.effects.push(Effect::Choose {
                    var,
                    param: p as u8,
                    values: (0..size).map(|i| Value(i as u8)).collect(),
                    sites: Vec::new(),
                    conds: Vec::new(),
                });
            }
        }
    }
}
