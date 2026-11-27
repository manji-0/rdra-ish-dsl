//! What a use case's `transitions` and `chooses` do: arm guards, chosen columns, and creation choices.

use super::{ActionDraft, Builder, GuardedArm};
use crate::lifecycle::types::{
    Arm, At, Choice, CmpOp, Effect, Guard, GuardWhy, ModelRef, Param, ParamRole, Pred, Site, Term,
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
            let domain = &self.ts.vars[var.index()].domain;
            let (Some(from), Some(to)) = (domain.of_name(&st.from), domain.of_name(&st.to)) else {
                continue;
            };
            let site = Site::new(ModelRef::Transition(idx), &st.site);
            let guard: Vec<(ConditionKey, String)> = st
                .guard
                .map(|ck| (ck, model.conditions[ck].id.clone()))
                .into_iter()
                .collect();
            let existing = a.effects[first..].iter_mut().find_map(|eff| match eff {
                Effect::Transition { var: v, arms, .. } if *v == var => Some(arms),
                _ => None,
            });
            let arm = Arm {
                from,
                to,
                site,
                conds: guard,
            };
            match existing {
                Some(arms) => match arms.iter_mut().find(|x| (x.from, x.to) == (from, to)) {
                    // The same arm declared again: either declaration enables it.
                    Some(c) => join_conds(&mut c.conds, arm.conds),
                    None => arms.push(arm),
                },
                None => a.effects.push(Effect::Transition {
                    var,
                    param: p,
                    arms: vec![arm],
                }),
            }
        }
        for eff in &a.effects[first..] {
            let Effect::Transition { var, param, arms } = eff else {
                continue;
            };
            let at = At::Param(*param);
            let mut froms: Vec<Value> = Vec::new();
            for arm in arms {
                if !froms.contains(&arm.from) {
                    froms.push(arm.from);
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
                site: arms[0].site.clone(),
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
                Effect::Transition { var, param, arms } => {
                    let slot = |at| Term::Slot { var: *var, at };
                    arms.iter()
                        .map(|arm| {
                            let parts = vec![
                                Pred::Cmp(slot(At::Param(*param)), CmpOp::Eq, Term::Lit(arm.from)),
                                Pred::Cmp(slot(At::Next(*param)), CmpOp::Eq, Term::Lit(arm.to)),
                            ];
                            (parts, &arm.site, &arm.conds)
                        })
                        .collect()
                }
                Effect::Choose { var, param, values } => {
                    let next = Term::Slot {
                        var: *var,
                        at: At::Next(*param),
                    };
                    // A creation's own choice has no line, so no `.when` guards it.
                    values
                        .iter()
                        .filter_map(|c| {
                            let parts =
                                vec![Pred::Cmp(next.clone(), CmpOp::Eq, Term::Lit(c.value))];
                            Some((parts, c.site.as_ref()?, &c.conds))
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
            let mut values: Vec<Choice> = Vec::new();
            if choice.values.is_empty() {
                let size = self.ts.vars[var.index()].domain.size(&self.ts);
                let site = Site::new(fact.clone(), &choice.site);
                values = (0..size)
                    .map(|i| Choice {
                        value: Value::at(i),
                        site: Some(site.clone()),
                        conds: Vec::new(),
                    })
                    .collect();
            }
            let domain = self.ts.vars[var.index()].domain.clone();
            for chosen in &choice.values {
                let Some(v) = domain.of_name(&chosen.value) else {
                    continue;
                };
                let guard: Vec<(ConditionKey, String)> = chosen
                    .guard
                    .map(|ck| (ck, model.conditions[ck].id.clone()))
                    .into_iter()
                    .collect();
                match values.iter_mut().find(|x| x.value == v) {
                    Some(c) => join_conds(&mut c.conds, guard),
                    None => values.push(Choice {
                        value: v,
                        site: Some(Site::new(fact.clone(), &chosen.site)),
                        conds: guard,
                    }),
                }
            }
            a.effects.push(Effect::Choose {
                var,
                param: p,
                values,
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
                let size = self.ts.vars[var.index()].domain.size(&self.ts);
                a.effects.push(Effect::Choose {
                    var,
                    param: p as u8,
                    values: (0..size)
                        .map(|i| Choice {
                            value: Value::at(i),
                            site: None,
                            conds: Vec::new(),
                        })
                        .collect(),
                });
            }
        }
    }
}

/// An arm or value declared again with `guard`: either line enables it, so an unguarded line
/// clears the conditions and guarded lines join theirs.
fn join_conds(have: &mut Vec<(ConditionKey, String)>, guard: Vec<(ConditionKey, String)>) {
    if have.is_empty() {
        return;
    }
    if guard.is_empty() {
        have.clear();
        return;
    }
    for c in guard {
        if !have.contains(&c) {
            have.push(c);
        }
    }
}
