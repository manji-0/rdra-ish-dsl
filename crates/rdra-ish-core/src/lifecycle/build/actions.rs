//! The actions of a system: one per use case transaction step, named apart, with their run marks.

use std::collections::HashSet;

use super::{first_buc, ActionDraft, Builder};
use crate::analysis::{applied_bodies, AppliedBody};
use crate::lifecycle::types::{
    Action, ActionOrigin, At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, ModelRef, Param,
    ParamRole, Pred, Rhs, Site, Term, Value, VarKind,
};
use crate::lifecycle::Scope;
use crate::model::{ApiKey, EntityKey, NodeRef, UseCaseKey};
use crate::resolver::reachable_from_bucs;
use crate::tx::{infer_usecase_transactions, UcWrite, WriteKind};

impl<'m> Builder<'m> {
    pub(super) fn actions(&mut self, scope: &Scope) {
        let model = self.model;
        let in_scope: Option<HashSet<NodeRef>> =
            (!scope.bucs.is_empty()).then(|| reachable_from_bucs(model, &scope.bucs));
        let mut txs = infer_usecase_transactions(model);
        self.active_ucs = txs.iter().map(|utx| utx.usecase).collect();
        // Transitions first in declaration order, then the rest by use case id.
        let first_transition = |uc: UseCaseKey| {
            model
                .state_transitions
                .iter()
                .position(|t| t.usecase == uc)
                .unwrap_or(usize::MAX)
        };
        txs.sort_by_key(|utx| {
            (
                first_transition(utx.usecase),
                model.use_cases[utx.usecase].id.clone(),
            )
        });
        for utx in &txs {
            let uc = utx.usecase;
            if in_scope
                .as_ref()
                .is_some_and(|r| !r.contains(&NodeRef::UseCase(uc)))
            {
                continue;
            }
            let groups: Vec<Vec<UcWrite>> = utx
                .groups()
                .map(|g| {
                    g.ordered_writes
                        .iter()
                        .filter(|w| self.ent_of.contains_key(&w.entity))
                        .cloned()
                        .collect()
                })
                .filter(|writes: &Vec<UcWrite>| !writes.is_empty())
                .collect();
            let entities: Vec<Vec<EntityKey>> = groups
                .iter()
                .map(|writes| writes.iter().map(|w| w.entity).collect())
                .collect();
            let id = &model.use_cases[uc].id;
            let written_apis: HashSet<ApiKey> = utx.writes().filter_map(|w| w.via_api).collect();
            self.run_mark_vars(uc, &entities);
            self.later_step_reads(uc, &groups, &written_apis);
            for (g, writes) in groups.iter().enumerate() {
                let step = (entities.len() > 1).then(|| u8::try_from(g + 1).expect("few groups"));
                let name = match step {
                    None => id.clone(),
                    Some(n) => format!("{id}_{n}"),
                };
                let bodies = self.step_bodies(uc, writes, g, &written_apis);
                self.acting = entities[g].clone();
                let action = self.usecase_action(uc, name, writes, &entities, g, &bodies);
                self.acting.clear();
                self.uc_actions
                    .entry(uc)
                    .or_default()
                    .push(crate::lifecycle::types::ActId::at(self.ts.actions.len()));
                self.ts.actions.push(action);
            }
        }
        if let Some(tick) = self.tick() {
            self.ts.actions.push(tick);
        }
        for (i, a) in self.ts.actions.iter_mut().enumerate() {
            a.source = crate::lifecycle::types::ActId::at(i);
        }
        self.name_generated_actions_apart();
        self.inert_conditions();
    }

    /// A generated action name, `<UC>_<n>` for a step of a split use case or `Tick` for the
    /// clock, that is a use case's id or an earlier generated name takes the first free
    /// `<name>_<k>`, k from 2, so action names are unique and a model whose generated names
    /// collide with nothing keeps every name.
    fn name_generated_actions_apart(&mut self) {
        let model = self.model;
        let generated = |a: &Action| match &a.origin {
            ActionOrigin::Clock => true,
            ActionOrigin::UseCase { uc, .. } => a.name != model.use_cases[*uc].id,
        };
        let mut taken: HashSet<String> = self
            .ts
            .actions
            .iter()
            .filter(|a| !generated(a))
            .map(|a| a.name.clone())
            .collect();
        for a in &mut self.ts.actions {
            if !generated(a) {
                continue;
            }
            let mut name = a.name.clone();
            let mut k = 2;
            while taken.contains(&name) {
                name = format!("{}_{k}", a.name);
                k += 1;
            }
            taken.insert(name.clone());
            a.name = name;
        }
    }

    /// An invariant or after-assert name (`Check_<E>_<column>_<n>`, `Unique_<Child>_<role>`,
    /// ...) that a temporal property's id or an earlier check's name already spells takes the
    /// first free `<name>_<k>`, k from 2, that no other check is named. Entity, column, and
    /// role ids may hold `_`, so two generated names can coincide; a model whose names collide
    /// with nothing keeps every name. A use case's id is not avoided: the TLA+ module renames
    /// the action instead, as action names are not promised (docs/compatibility.md).
    pub(super) fn name_generated_checks_apart(&mut self) {
        let mut taken: HashSet<String> = self
            .model
            .temporal_properties
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let names: Vec<&mut String> = self
            .ts
            .invariants
            .iter_mut()
            .map(|i| &mut i.name)
            .chain(self.ts.after.iter_mut().map(|a| &mut a.name))
            .collect();
        let wanted: HashSet<String> = names.iter().map(|n| n.to_string()).collect();
        for name in names {
            let base = name.clone();
            let mut k = 2;
            while !taken.insert(name.clone()) {
                loop {
                    *name = format!("{base}_{k}");
                    k += 1;
                    if !wanted.contains(name.as_str()) {
                        break;
                    }
                }
            }
        }
    }

    /// One run mark per slot of each entity a step before the last of a split use case
    /// writes (docs/state-derivation.md, "Runs of a split use case"). `groups` lists every
    /// group's entities, in transaction order.
    fn run_mark_vars(&mut self, uc: UseCaseKey, groups: &[Vec<EntityKey>]) {
        let Some((_, earlier)) = groups.split_last() else {
            return;
        };
        let model = self.model;
        let names: Vec<String> = std::iter::once("none".to_string())
            .chain((1..groups.len()).map(|i| i.to_string()))
            .collect();
        let id = &model.use_cases[uc].id;
        let site = Site::new(
            ModelRef::UseCase(uc),
            &model.decl_sites.located("usecase", id),
        );
        for &ek in earlier.iter().flatten() {
            let e = self.ent_of[&ek];
            let var = self.push_var(
                VarKind::Run {
                    entity: e,
                    usecase: id.clone(),
                },
                Domain::Enum(names.clone()),
                Value(0),
                site.clone(),
            );
            self.run_marks.insert((uc, e), var);
        }
    }

    /// The `holds` bodies step `step` (0-based) of the use case carries: the bodies applied
    /// to the use case on step 1, the precondition of the run, and a body applied to an API
    /// on the step of that API's group, or on step 1 when the API is in `written_apis`, the
    /// APIs the use case writes through, for no group.
    pub(super) fn step_bodies(
        &self,
        uc: UseCaseKey,
        writes: &[UcWrite],
        step: usize,
        written_apis: &HashSet<ApiKey>,
    ) -> Vec<AppliedBody<'m>> {
        applied_bodies(self.model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .filter(|b| match b.api {
                Some(api) if written_apis.contains(&api) => {
                    writes.iter().any(|w| w.via_api == Some(api))
                }
                _ => step == 0,
            })
            .collect()
    }

    /// One atomic action for one transaction group of the use case: every effect on every
    /// entity of the group in one step. `groups` lists every group's entities, in transaction
    /// order, `step` is this group's index in it, and `bodies` the `holds` bodies this step
    /// carries.
    fn usecase_action(
        &mut self,
        uc: UseCaseKey,
        name: String,
        writes: &[UcWrite],
        groups: &[Vec<EntityKey>],
        step: usize,
        bodies: &[AppliedBody<'m>],
    ) -> Action {
        let model = self.model;
        let mut a = ActionDraft::default();
        let mut sorted: Vec<(EntId, WriteKind, Option<ApiKey>)> = writes
            .iter()
            .map(|w| (self.ent_of[&w.entity], w.kind.clone(), w.via_api))
            .collect();
        sorted.sort_by_key(|(e, _, _)| *e);
        for (e, kind, via_api) in sorted {
            let role = match kind {
                WriteKind::Creates => ParamRole::New,
                WriteKind::Updates | WriteKind::Deletes => ParamRole::Existing,
            };
            let p = a.param(e, role);
            if let (ParamRole::New, Some(api)) = (role, via_api) {
                a.params[usize::from(p)].via_api = Some(api);
                self.ts
                    .api_ids
                    .entry(api)
                    .or_insert_with(|| model.apis[api].id.clone());
            }
            match kind {
                WriteKind::Creates => a.effects.push(Effect::Create { param: p }),
                WriteKind::Deletes => a.effects.push(Effect::Delete { param: p }),
                WriteKind::Updates => {}
            }
        }
        let ent_of = &self.ent_of;
        let mut earlier: Vec<(EntId, u8)> = groups[..step]
            .iter()
            .enumerate()
            .flat_map(|(j, ents)| ents.iter().map(move |ek| (ent_of[ek], j as u8 + 1)))
            .collect();
        earlier.sort();
        for (e, wrote) in earlier {
            a.param(e, ParamRole::Earlier { step: wrote });
        }
        self.sets_effects(uc, &mut a);
        self.chosen_columns(uc, &mut a);
        self.transition_effects(uc, &mut a);
        self.creation_choices(&mut a);
        self.owner_bindings(uc, bodies, &mut a);
        self.link_effects(uc, &mut a);
        self.copy_effects(uc, &mut a, groups);
        self.run_marks(uc, &mut a, step, groups.len());
        let guards = self.condition_guards(bodies, &a.params);
        a.guards.extend(guards);
        self.arm_guards(uc, &mut a);
        Action {
            name,
            source: crate::lifecycle::types::ActId::at(0),
            origin: ActionOrigin::UseCase {
                uc,
                buc: first_buc(model, uc),
                step: (groups.len() > 1).then(|| u8::try_from(step + 1).expect("few groups")),
            },
            site: Site::new(
                ModelRef::UseCase(uc),
                &model.decl_sites.located("usecase", &model.use_cases[uc].id),
            ),
            params: a.params,
            guards: a.guards,
            effects: a.effects,
        }
    }

    /// The run guards and mark effects of step `step` (0-based) of `k`: a written row of a
    /// step before the last is in no run (`none`) and takes this step's number; an `Earlier`
    /// row carries the previous step's number and takes this step's, or `none` at the last
    /// step. A created row's mark is at base already.
    fn run_marks(&mut self, uc: UseCaseKey, a: &mut ActionDraft, step: usize, k: usize) {
        let this = step as u8 + 1;
        let next = if usize::from(this) < k {
            Value(this)
        } else {
            Value(0)
        };
        for p in 0..a.params.len() {
            let Param { entity, role, .. } = a.params[p];
            let Some(&var) = self.run_marks.get(&(uc, entity)) else {
                continue;
            };
            let expected = match role {
                ParamRole::New => None,
                ParamRole::Existing => Some(0),
                ParamRole::Earlier { .. } => Some(this - 1),
                ParamRole::Parent { .. } => continue,
            };
            let site = self.ts.vars[var.index()].site.clone();
            if let Some(mark) = expected {
                a.guards.push(Guard {
                    pred: Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(p as u8),
                        },
                        CmpOp::Eq,
                        Term::Lit(Value(mark)),
                    ),
                    why: GuardWhy::Run { step: mark },
                    site: site.clone(),
                });
            }
            a.effects.push(Effect::Assign {
                var,
                param: p as u8,
                rhs: Rhs::Lit(next),
                site,
            });
        }
    }
}
