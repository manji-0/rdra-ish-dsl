//! `SemanticModel` -> `TransitionSystem`. The only reader of the model's lifecycle facts:
//! columns, `transitions`, `sets`, CRUD edges, relates, constraints, asserts, and properties.

mod actions;
mod events;
mod links;
mod naming;
mod preds;
mod sets;
mod transitions;
mod vars;

use std::collections::{HashMap, HashSet};

use super::diag::{Diag, StateDiag};
use super::types::{
    Bounds, Domain, Effect, EntId, Guard, Param, ParamRole, Pred, Site, Start, Value, VarDecl,
    VarId, VarKind,
};
use super::{Built, Scope, TransitionSystem};
use crate::model::{ConditionKey, EntityKey, FkId, NodeRef, RelKind, SemanticModel, UseCaseKey};
use naming::Named;

pub(super) fn build(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Built {
    let mut b = Builder::new(model, bounds);
    b.entities();
    b.clock();
    b.actions(scope);
    b.invariants();
    b.after_asserts();
    b.properties();
    b.name_generated_checks_apart();
    b.unwritten_vars();
    b.finish(scope)
}

struct Builder<'m> {
    model: &'m SemanticModel,
    bounds: Bounds,
    named: Named,
    ts: TransitionSystem,
    ent_of: HashMap<EntityKey, EntId>,
    col_var: HashMap<(EntId, String), VarId>,
    prop_var: HashMap<(EntId, String), VarId>,
    /// (owner var, child, parent)
    owners: Vec<(VarId, EntId, EntId)>,
    /// The run mark of each entity a step before the last of a split use case writes.
    run_marks: HashMap<(UseCaseKey, EntId), VarId>,
    /// Use cases whose actions exist, each with its actions in transaction order.
    uc_actions: HashMap<UseCaseKey, Vec<super::types::ActId>>,
    /// Use cases with any write in the full model (before the scope filter).
    active_ucs: HashSet<UseCaseKey>,
    diags: Vec<Diag>,
    /// Parallel to `diags`: the entities of the actions each diagnostic altered.
    altered: Vec<Vec<EntityKey>>,
    /// The entities of the use case action being built, if any.
    acting: Vec<EntityKey>,
    /// The entity each `holds` body on a split use case reads before the later transaction
    /// group that writes it (`GuardReadsLaterStep`), for the guard's `Runs_` row.
    later_reads: HashMap<(UseCaseKey, ConditionKey), EntityKey>,
}

impl<'m> Builder<'m> {
    fn new(model: &'m SemanticModel, bounds: Bounds) -> Self {
        Builder {
            model,
            bounds,
            named: Named::collect(model),
            ts: TransitionSystem::empty(bounds),
            ent_of: HashMap::new(),
            col_var: HashMap::new(),
            prop_var: HashMap::new(),
            owners: Vec::new(),
            run_marks: HashMap::new(),
            uc_actions: HashMap::new(),
            active_ucs: HashSet::new(),
            diags: Vec::new(),
            altered: Vec::new(),
            acting: Vec::new(),
            later_reads: HashMap::new(),
        }
    }

    fn finish(mut self, scope: &Scope) -> Built {
        self.ts.layout();
        let model = self.model;
        let mut inactive: Vec<UseCaseKey> = self
            .active_ucs
            .iter()
            .copied()
            .filter(|uc| !self.uc_actions.contains_key(uc))
            .collect();
        inactive.sort_by(|a, b| model.use_cases[*a].id.cmp(&model.use_cases[*b].id));
        let diags = self
            .diags
            .into_iter()
            .zip(self.altered)
            .map(|(d, mut involves)| {
                for k in &d.entities {
                    if !involves.contains(k) {
                        involves.push(*k);
                    }
                }
                (d, involves)
            })
            .collect();
        let (diags, selection) = self.ts.select(scope, diags, inactive);
        Built {
            ts: self.ts,
            diags,
            selection,
        }
    }

    /// Records a diagnostic once. One raised while a use case action is built may have
    /// dropped or changed a guard or an effect of it, so it also involves every entity that
    /// action binds: a selection that reads the action does not defer it.
    fn diag(&mut self, entities: Vec<EntityKey>, diag: StateDiag) {
        let d = Diag { entities, diag };
        let at = match self.diags.iter().position(|x| *x == d) {
            Some(at) => at,
            None => {
                self.diags.push(d);
                self.altered.push(Vec::new());
                self.diags.len() - 1
            }
        };
        for k in &self.acting {
            if !self.altered[at].contains(k) {
                self.altered[at].push(*k);
            }
        }
    }

    fn push_var(&mut self, kind: VarKind, domain: Domain, base: Value, site: Site) -> VarId {
        let id = VarId::at(self.ts.vars.len());
        self.ts.vars.push(VarDecl {
            kind,
            domain,
            base,
            start: Start::Base,
            site,
        });
        id
    }

    // ── R-var and R-start ────────────────────────────────────────────────────
}

/// Why a copy has no source row: the reason for `UnboundCopySource`, or the keys that could
/// lead to it.
enum CopyMiss {
    Unbound(String),
    Ambiguous(Vec<FkId>),
}

enum EffectRhs {
    Value(Value),
    /// `sets(.., col == present)` on a nullable Int column: any non-null value.
    AnyNonNull,
    None,
}

#[derive(Default)]
struct ActionDraft {
    params: Vec<Param>,
    guards: Vec<Guard>,
    effects: Vec<Effect>,
}

impl ActionDraft {
    fn param(&mut self, entity: EntId, role: ParamRole) -> u8 {
        self.params.push(Param {
            entity,
            role,
            via_api: None,
        });
        (self.params.len() - 1) as u8
    }

    /// The param of a row of `entity` the action writes.
    fn param_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_written())
            .map(|i| i as u8)
    }

    /// The param of the action's row of `entity`: one it writes, or one an earlier step of
    /// its run wrote.
    fn row_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_row())
            .map(|i| i as u8)
    }

    fn writes(&self, var: VarId, param: u8) -> bool {
        self.effects.iter().any(|eff| match eff {
            Effect::Assign {
                var: v, param: p, ..
            }
            | Effect::Transition {
                var: v, param: p, ..
            }
            | Effect::Choose {
                var: v, param: p, ..
            } => *v == var && *p == param,
            _ => false,
        })
    }
}

/// Before a copy into `var` of param `p`: drops a creation's free choice of the column, which
/// the copy pins, and returns the site of any other write of it in the step, which conflicts
/// with the copy.
fn prior_write(a: &mut ActionDraft, var: VarId, p: u8) -> Option<Site> {
    let at = a.effects.iter().position(|eff| match eff {
        Effect::Assign {
            var: v, param: q, ..
        }
        | Effect::Transition {
            var: v, param: q, ..
        }
        | Effect::Choose {
            var: v, param: q, ..
        } => *v == var && *q == p,
        _ => false,
    })?;
    match &a.effects[at] {
        Effect::Choose { sites, .. } if sites.is_empty() => {
            a.effects.remove(at);
            None
        }
        Effect::Assign { site, .. } => Some(site.clone()),
        Effect::Choose { sites, .. } | Effect::Transition { sites, .. } => sites.first().cloned(),
        _ => None,
    }
}

fn first_buc(model: &SemanticModel, uc: UseCaseKey) -> Option<crate::model::BucKey> {
    model
        .relations
        .iter()
        .find_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Contains, NodeRef::Buc(b), NodeRef::UseCase(u)) if *u == uc => Some(*b),
            _ => None,
        })
}

/// Declared casing wins: exact match first, then case-insensitive.
/// An arm or chosen value under `.when`: its comparisons on the column, its site, and its
/// conditions.
type GuardedArm<'a> = (Vec<Pred>, &'a Site, &'a Vec<(ConditionKey, String)>);

fn variant_index(values: &[String], name: &str) -> Option<u8> {
    values
        .iter()
        .position(|v| v == name)
        .or_else(|| values.iter().position(|v| v.eq_ignore_ascii_case(name)))
        .map(|i| i as u8)
}
