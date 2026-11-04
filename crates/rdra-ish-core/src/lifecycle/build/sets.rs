//! What a use case's `sets`, `copies`, and `drives` write: the guarded effects of an action.

use super::{prior_write, variant_index, ActionDraft, Builder, CopyMiss, EffectRhs};
use crate::analysis::AppliedBody;
use crate::lifecycle::build::preds;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, ModelRef, Param, ParamRole, Pred, Rhs, Site,
    Term, Value, VarId, VarKind,
};
use crate::model::{
    CmpOpModel, CmpRhs, ConditionKey, CrossConstraintScope, EffectValue, EntityKey, FkId, Join,
    SetValue, UseCaseKey,
};

impl<'m> Builder<'m> {
    pub(super) fn sets_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        // Unguarded lines first, so a guarded line of a column they write conflicts with them.
        let lines = model
            .column_effects
            .iter()
            .enumerate()
            .filter(|(_, ce)| ce.origin == uc);
        let (guarded, unguarded): (Vec<_>, Vec<_>) = lines.partition(|(_, ce)| ce.guard.is_some());
        for (idx, ce) in unguarded.into_iter().chain(guarded) {
            let SetValue::Value(value) = &ce.value else {
                continue;
            };
            let Some(&e) = self.ent_of.get(&ce.entity) else {
                continue;
            };
            let Some(p) = a.param_of(e) else { continue };
            let Some(&var) = self.col_var.get(&(e, ce.column.clone())) else {
                continue;
            };
            let site = Site::new(ModelRef::ColumnEffect(idx), &ce.site);
            let decl = &self.ts.vars[var.index()];
            let lifecycle = matches!(
                decl.kind,
                VarKind::Column {
                    lifecycle: true,
                    ..
                }
            );
            if lifecycle && a.params[usize::from(p)].role != ParamRole::New {
                self.diag(
                    vec![ce.entity],
                    StateDiag::DoubleModeledEnum {
                        column: ce.column.clone(),
                        usecase: model.use_cases[uc].id.clone(),
                        site,
                    },
                );
                continue;
            }
            let domain = decl.domain.clone();
            let rhs = self.effect_value(&domain, value, &site, ce.entity);
            if let Some(ck) = ce.guard {
                let values = match rhs {
                    EffectRhs::Value(v) => vec![v],
                    EffectRhs::AnyNonNull => (1..domain.size(self.bounds))
                        .map(|i| Value(i as u8))
                        .collect(),
                    EffectRhs::None => continue,
                };
                self.guarded_set(a, uc, ce.entity, var, p, values, (ck, site));
                continue;
            }
            match rhs {
                EffectRhs::Value(v) => self.assign(a, uc, ce.entity, var, p, Rhs::Lit(v), site),
                EffectRhs::AnyNonNull => {
                    let values: Vec<Value> = (1..domain.size(self.bounds))
                        .map(|i| Value(i as u8))
                        .collect();
                    a.effects.push(Effect::Choose {
                        var,
                        param: p,
                        sites: vec![site; values.len()],
                        conds: Vec::new(),
                        values,
                    });
                }
                EffectRhs::None => {}
            }
        }
        for (idx, pe) in model.proposition_effects.iter().enumerate() {
            if pe.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&pe.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) =
                (a.param_of(e), self.prop_var.get(&(e, pe.prop.axis_key())))
            else {
                continue;
            };
            let v = if pe.truth { Value::TRUE } else { Value::FALSE };
            let site = Site::new(ModelRef::PropositionEffect(idx), &pe.site);
            self.check_driven_truth(uc, idx, &site);
            self.assign(a, uc, pe.entity, var, p, Rhs::Lit(v), site);
        }
    }

    /// `sets(UC, E.col == v).when(Cond)`: one guarded value of a choice that the step's
    /// `GuardWhy::ArmCondition` guard filters, as a guarded `chooses` value is. The lines of
    /// one column join one `Choose`, and a value two lines give is enabled by either. An
    /// unguarded `sets` of the column conflicts with it, and so does a copy (`copy_effects`).
    #[allow(clippy::too_many_arguments)]
    fn guarded_set(
        &mut self,
        a: &mut ActionDraft,
        uc: UseCaseKey,
        ek: EntityKey,
        var: VarId,
        p: u8,
        values: Vec<Value>,
        (ck, site): (ConditionKey, Site),
    ) {
        let cond = (ck, self.model.conditions[ck].id.clone());
        let guarded = a.effects.iter().position(|eff| {
            matches!(eff, Effect::Choose { var: v, param: q, conds, .. }
                if (*v, *q) == (var, p) && !conds.is_empty() && conds.iter().all(|c| !c.is_empty()))
        });
        let at = match guarded {
            Some(at) => at,
            None => {
                if let Some(old) = prior_write(a, var, p) {
                    let column = self.ts.var_name(var);
                    self.diag(
                        vec![ek],
                        StateDiag::ConflictingEffects {
                            usecase: self.model.use_cases[uc].id.clone(),
                            column,
                            sites: vec![old, site],
                        },
                    );
                    return;
                }
                a.effects.push(Effect::Choose {
                    var,
                    param: p,
                    values: Vec::new(),
                    sites: Vec::new(),
                    conds: Vec::new(),
                });
                a.effects.len() - 1
            }
        };
        let Effect::Choose {
            values: have,
            sites,
            conds,
            ..
        } = &mut a.effects[at]
        else {
            return;
        };
        for v in values {
            match have.iter().position(|x| *x == v) {
                Some(i) if !conds[i].contains(&cond) => conds[i].push(cond.clone()),
                Some(_) => {}
                None => {
                    have.push(v);
                    sites.push(site.clone());
                    conds.push(vec![cond.clone()]);
                }
            }
        }
    }

    /// `sets(UC, E, column == F.source)`: the step leaves in `column` the value `F.source` holds
    /// in the row of `F` it binds, read as a condition atom on `F` reads it. The copy is a
    /// choice of the column's value that a successor guard pins to the source, so the explorer
    /// and the TLA+ printer read it as they read an arm guard. It runs after the owner
    /// bindings, so the parent a created child takes is already a param.
    pub(super) fn copy_effects(
        &mut self,
        uc: UseCaseKey,
        a: &mut ActionDraft,
        groups: &[Vec<EntityKey>],
    ) {
        let model = self.model;
        for (idx, ce) in model.column_effects.iter().enumerate() {
            let SetValue::Copy(source) = &ce.value else {
                continue;
            };
            if ce.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&ce.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) = (a.param_of(e), self.col_var.get(&(e, ce.column.clone())))
            else {
                continue;
            };
            let site = Site::new(ModelRef::ColumnEffect(idx), &ce.site);
            // A state model starts at the values its creations name, so a lifecycle column
            // takes no copied value, on a creation either.
            let lifecycle = matches!(
                self.ts.vars[var.index()].kind,
                VarKind::Column {
                    lifecycle: true,
                    ..
                }
            );
            if lifecycle {
                self.diag(
                    vec![ce.entity],
                    StateDiag::DoubleModeledEnum {
                        column: ce.column.clone(),
                        usecase: model.use_cases[uc].id.clone(),
                        site,
                    },
                );
                continue;
            }
            let source_at = match self.copy_source(a, p, source.entity, &ce.by, groups) {
                Ok(at) => at,
                Err(CopyMiss::Ambiguous(candidates)) => {
                    let statement = format!(
                        "sets({}, {}.{} == {}.{})",
                        model.use_cases[uc].id,
                        model.entities[ce.entity].id,
                        ce.column,
                        model.entities[source.entity].id,
                        source.column
                    );
                    let (a, b) = (ce.entity, source.entity);
                    let diag =
                        preds::ambiguous_owner_link(model, &statement, a, b, &candidates, &site);
                    self.diag(vec![ce.entity], diag);
                    continue;
                }
                Err(CopyMiss::Unbound(reason)) => {
                    let source_id = &model.entities[source.entity].id;
                    self.diag(
                        vec![ce.entity],
                        StateDiag::UnboundCopySource {
                            usecase: model.use_cases[uc].id.clone(),
                            column: format!("{}.{}", model.entities[ce.entity].id, ce.column),
                            source: format!("{source_id}.{}", source.column),
                            reason,
                            site,
                        },
                    );
                    continue;
                }
            };
            let Some(&source_var) = self
                .ent_of
                .get(&source.entity)
                .and_then(|s| self.col_var.get(&(*s, source.column.clone())))
            else {
                continue;
            };
            let copy = Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Next(p),
                },
                CmpOp::Eq,
                Term::Slot {
                    var: source_var,
                    at: source_at,
                },
            );
            if a.guards.iter().any(|g| g.pred == copy) {
                continue;
            }
            if let Some(old) = prior_write(a, var, p) {
                let column = self.ts.var_name(var);
                self.diag(
                    vec![ce.entity],
                    StateDiag::ConflictingEffects {
                        usecase: model.use_cases[uc].id.clone(),
                        column,
                        sites: vec![old, site],
                    },
                );
                continue;
            }
            let size = self.ts.vars[var.index()].domain.size(self.bounds);
            a.effects.push(Effect::Choose {
                var,
                param: p,
                values: (0..size).map(|i| Value(i as u8)).collect(),
                sites: vec![site.clone(); size],
                conds: Vec::new(),
            });
            a.guards.push(Guard {
                pred: copy,
                why: GuardWhy::Copy,
                site,
            });
        }
    }

    /// The row of `source` a copy into param `p` reads, by the rule a condition atom follows:
    /// a param of the action (on the pre-state, or in the successor when the action creates
    /// it), else the parent `p` belongs to through the key [`SemanticModel::join_key`] picks
    /// (with the copy's `.by`), which joins the action as a `Parent` param that `p`'s owner
    /// names. Any other row is not bound to the step.
    fn copy_source(
        &mut self,
        a: &mut ActionDraft,
        p: u8,
        source: EntityKey,
        by: &[String],
        groups: &[Vec<EntityKey>],
    ) -> Result<At, CopyMiss> {
        let model = self.model;
        let Some(&s) = self.ent_of.get(&source) else {
            return Err(CopyMiss::Unbound(
                "it is published and has no state".to_string(),
            ));
        };
        if let Some(q) = self.read_param(&a.params, s, by) {
            return Ok(match a.params[usize::from(q)].role {
                ParamRole::New => At::Next(q),
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Earlier { .. } => {
                    At::Param(q)
                }
            });
        }
        let target = a.params[usize::from(p)].entity;
        let target_id = &self.ts.entities[target.index()].id;
        let source_id = &model.entities[source].id;
        let target_ek = self.ts.entities[target.index()].key;
        let key = match model.join_key(target_ek, source, by) {
            Join::Ambiguous(candidates) => return Err(CopyMiss::Ambiguous(candidates)),
            Join::Key(fk) => Some(fk),
            Join::Unrelated => None,
        };
        let owner = key
            .filter(|fk| model.foreign_key(*fk).child == target_ek)
            .and_then(|fk| Some((fk, self.owner_var(fk)?)));
        if let Some((fk, var)) = owner {
            if a.params[usize::from(p)].role == ParamRole::New {
                return Err(CopyMiss::Unbound(format!(
                    "the {target_id} it creates is not linked to a {source_id}: the relate is optional and the use case does not write {source_id}"
                )));
            }
            let q = a.param(s, ParamRole::Parent { key: fk });
            let site = self.ts.vars[var.index()].site.clone();
            a.guards.push(Guard {
                pred: Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(p),
                    },
                    CmpOp::Eq,
                    Term::Index(At::Param(q)),
                ),
                why: GuardWhy::Owner,
                site,
            });
            return Ok(At::Param(q));
        }
        let child = key.is_some();
        let later = groups.iter().flatten().any(|ek| *ek == source);
        Err(CopyMiss::Unbound(if later {
            format!("the use case writes {source_id} in a later transaction group, a step of its own; order the use case's invokes so {source_id} is written first, or relate {target_id} to {source_id} so one step writes both")
        } else if child {
            format!("several {source_id} rows may belong to one {target_id}; copy from the row the {target_id} belongs to, or from one the use case writes")
        } else {
            format!("the use case does not write {source_id}, and no relate makes it the parent of {target_id}")
        }))
    }

    /// The owner var of key `fk`, if both its entities have state.
    pub(super) fn owner_var(&self, fk: FkId) -> Option<VarId> {
        self.owners.iter().map(|(var, ..)| *var).find(|var| {
            matches!(self.ts.vars[var.index()].kind, VarKind::Owner { fk: id, .. } if id == fk)
        })
    }

    /// The param a statement with selector `by` reads for a row of entity `e`: the row of `e`
    /// the action writes or an earlier step of its run wrote, else the `Parent` param of the
    /// key the statement's join follows ([`SemanticModel::join_key`] from that key's child).
    /// One `Parent` param of `e` whose join no `.by` names is the row the action reads.
    /// Otherwise, for a key with no `Parent` param or for several such params and no `.by`
    /// naming one, `None`: the statement joins its own row of `e`.
    pub(super) fn read_param(&self, params: &[Param], e: EntId, by: &[String]) -> Option<u8> {
        if let Some(p) = params.iter().position(|p| p.entity == e && p.role.is_row()) {
            return Some(p as u8);
        }
        let model = self.model;
        let ek = self.ts.entities[e.index()].key;
        let parents: Vec<(u8, FkId)> = params
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.role {
                ParamRole::Parent { key } if p.entity == e => Some((i as u8, key)),
                _ => None,
            })
            .collect();
        let join = |fk: FkId| model.join_key(model.foreign_key(fk).child, ek, by);
        let named: Vec<u8> = parents
            .iter()
            .filter(|&&(_, fk)| join(fk) == Join::Key(fk))
            .map(|&(p, _)| p)
            .collect();
        match (parents.as_slice(), named.as_slice()) {
            (_, [p]) => Some(*p),
            ([(p, fk)], []) if matches!(join(*fk), Join::Ambiguous(_)) => Some(*p),
            _ => None,
        }
    }

    /// Whether a `holds` body the action carries reads `parent` through key `fk` from a
    /// `child` the action creates, so the creation links to the row the guard reads: the body
    /// compares the parent and its join follows `fk`, or its `.along(..)` takes a hop through
    /// `fk`.
    pub(super) fn guard_reads_parent(&self, bodies: &[AppliedBody<'m>], fk: FkId) -> bool {
        let model = self.model;
        let key = model.foreign_key(fk);
        bodies.iter().any(|b| {
            let along = match &b.body.along {
                CrossConstraintScope::RelationPath { links, .. } => links.contains(&Some(fk)),
                CrossConstraintScope::GlobalProduct => false,
            };
            let compared = b
                .body
                .conds
                .iter()
                .flat_map(preds::cond_entities)
                .any(|ek| ek == key.parent)
                && model.join_key(key.child, key.parent, &b.body.by) == Join::Key(fk);
            along || compared
        })
    }

    /// A use case that drives a comparison and also sets every Int operand of it to a literal
    /// must drive the truth those literals give. Otherwise the model contradicts itself.
    fn check_driven_truth(&mut self, uc: UseCaseKey, idx: usize, site: &Site) {
        let model = self.model;
        let pe = &model.proposition_effects[idx];
        let literal = |column: &str| {
            model.column_effects.iter().enumerate().find_map(|(i, ce)| {
                match (ce.origin == uc && ce.entity == pe.entity && ce.column == column)
                    .then(|| ce.value.literal().map(EffectValue::normalize))
                    .flatten()
                {
                    Some(EffectValue::Int(n)) => Some((*n, i)),
                    _ => None,
                }
            })
        };
        let Some((lhs, lhs_at)) = literal(&pe.prop.lhs_column) else {
            return;
        };
        let (rhs, rhs_at) = match &pe.prop.rhs {
            CmpRhs::IntLit(n) => (*n, None),
            CmpRhs::Column(c) => match literal(c) {
                Some((n, i)) => (n, Some(i)),
                None => return,
            },
            CmpRhs::Now | CmpRhs::Value(_) | CmpRhs::Values(_) => return,
        };
        let holds = match pe.prop.op {
            CmpOpModel::Lt => lhs < rhs,
            CmpOpModel::Gt => lhs > rhs,
            CmpOpModel::Le => lhs <= rhs,
            CmpOpModel::Ge => lhs >= rhs,
            CmpOpModel::Eq => lhs == rhs,
            CmpOpModel::Ne => lhs != rhs,
            // A driven comparison is never a set: `drives` rejects `in`.
            CmpOpModel::In | CmpOpModel::NotIn => return,
        };
        if holds == pe.truth {
            return;
        }
        let mut sites = vec![site.clone()];
        for i in std::iter::once(lhs_at).chain(rhs_at) {
            sites.push(Site::new(
                ModelRef::ColumnEffect(i),
                &model.column_effects[i].site,
            ));
        }
        self.diag(
            vec![pe.entity],
            StateDiag::ContradictoryDrivenComparison {
                usecase: model.use_cases[uc].id.clone(),
                proposition: pe.prop.display(),
                driven: pe.truth,
                sites,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn assign(
        &mut self,
        a: &mut ActionDraft,
        uc: UseCaseKey,
        ek: EntityKey,
        var: VarId,
        param: u8,
        rhs: Rhs,
        site: Site,
    ) {
        let existing = a.effects.iter_mut().find_map(|eff| match eff {
            Effect::Assign {
                var: v,
                param: q,
                rhs,
                site,
            } if *v == var && *q == param => Some((rhs, site)),
            _ => None,
        });
        if let Some((old_rhs, old_site)) = existing {
            if *old_rhs != rhs {
                let sites = vec![old_site.clone(), site.clone()];
                *old_rhs = rhs;
                *old_site = site;
                let column = self.ts.var_name(var);
                self.diag(
                    vec![ek],
                    StateDiag::ConflictingEffects {
                        usecase: self.model.use_cases[uc].id.clone(),
                        column,
                        sites,
                    },
                );
            }
            return;
        }
        a.effects.push(Effect::Assign {
            var,
            param,
            rhs,
            site,
        });
    }

    fn effect_value(
        &mut self,
        domain: &Domain,
        value: &EffectValue,
        site: &Site,
        ek: EntityKey,
    ) -> EffectRhs {
        match (domain, value.normalize()) {
            (Domain::Enum(values), EffectValue::EnumVariant(s)) => {
                variant_index(values, s).map_or(EffectRhs::None, |i| EffectRhs::Value(Value(i)))
            }
            (Domain::Bool, EffectValue::Bool(b)) => {
                EffectRhs::Value(if *b { Value::TRUE } else { Value::FALSE })
            }
            (Domain::Nullable, EffectValue::Null) => EffectRhs::Value(Value::NULL),
            (Domain::Nullable, EffectValue::Present | EffectValue::Int(_)) => {
                EffectRhs::Value(Value::PRESENT)
            }
            (Domain::Int | Domain::NullableInt, EffectValue::Int(n)) => {
                match domain.of_number(*n, self.bounds) {
                    Some(v) => EffectRhs::Value(v),
                    None => {
                        self.out_of_bounds(*n, site, ek);
                        EffectRhs::None
                    }
                }
            }
            (Domain::NullableInt, EffectValue::Null) => EffectRhs::Value(Value::NULL),
            (Domain::NullableInt, EffectValue::Present) => EffectRhs::AnyNonNull,
            _ => EffectRhs::None,
        }
    }

    pub(super) fn out_of_bounds(&mut self, value: i64, site: &Site, ek: EntityKey) {
        self.diag(
            vec![ek],
            StateDiag::IntLiteralOutOfBounds {
                value,
                max: self.bounds.int_max,
                site: site.clone(),
            },
        );
    }
}
