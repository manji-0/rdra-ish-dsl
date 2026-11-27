//! `SemanticModel` -> `TransitionSystem`. The only reader of the model's lifecycle facts:
//! columns, `transitions`, `sets`, CRUD edges, relates, constraints, asserts, and properties.

use crate::lifecycle::narrow::id8;
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
    Bounds, CmpOp, Domain, Effect, EntId, Guard, Param, ParamRole, Pred, Site, Start, Term, Value,
    VarDecl, VarId, VarKind,
};
use super::{Built, Scope, TransitionSystem};
use crate::model::{ConditionKey, EntityKey, FkId, SemanticModel, UseCaseKey};
use naming::Named;

pub(super) fn build(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Built {
    let mut b = Builder::new(model, bounds);
    b.entities();
    b.clock();
    b.actions(scope);
    b.invariants();
    b.overflow_rules();
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
    /// Each diagnostic, with the entities of the actions it altered.
    diags: Vec<(Diag, Vec<EntityKey>)>,
    /// The entities of the use case action being built, if any.
    acting: Vec<EntityKey>,
    /// The entity each `holds` body on a split use case reads before the later transaction
    /// group that writes it (`GuardReadsLaterStep`), for the guard's `Runs_` row.
    later_reads: HashMap<(UseCaseKey, ConditionKey), EntityKey>,
    /// `EventOverflow_<E>_<Ev>_<UC>` of each counted mark, which follow the model's rules.
    overflows: Vec<super::types::Invariant>,
}

impl<'m> Builder<'m> {
    fn new(model: &'m SemanticModel, bounds: Bounds) -> Self {
        Builder {
            model,
            bounds: bounds.clone(),
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
            acting: Vec::new(),
            later_reads: HashMap::new(),
            overflows: Vec::new(),
        }
    }

    fn finish(mut self, scope: &Scope) -> Built {
        self.ts.layout();
        self.one_write_per_slot();
        self.goals_met_by_delete();
        let bits = super::explore::packed_bits(&self.ts);
        if let Some(d) = too_wide(bits, super::explore::MOST_PACKED_BITS) {
            self.diag(Vec::new(), d);
        }
        let model = self.model;
        let mut inactive: Vec<UseCaseKey> = self
            .active_ucs
            .iter()
            .copied()
            .filter(|uc| !self.uc_actions.contains_key(uc) && !scope.without.contains(uc))
            .collect();
        inactive.sort_by(|a, b| model.use_cases[*a].id.cmp(&model.use_cases[*b].id));
        let diags = self
            .diags
            .into_iter()
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
}

/// `StateTooWide` when a packed state of `bits` bits passes `most`, the bits a packed state
/// addresses: the build stops there, as `DomainTooLarge` stops a column past a byte, rather
/// than any engine packing two cells into one place (P50).
fn too_wide(bits: u64, most: u64) -> Option<StateDiag> {
    (bits > most).then_some(StateDiag::StateTooWide { bits, most })
}

impl<'m> Builder<'m> {
    /// An action that writes one slot twice fails the build with `ConflictingEffects`
    /// (`Action::slots_written_twice`): the engines would not agree on its step. No builder
    /// path makes one today; this keeps a future one from reaching the engines.
    fn one_write_per_slot(&mut self) {
        let found: Vec<(String, VarId, Site, Vec<EntityKey>)> = (self.ts.actions.iter())
            .flat_map(|a| {
                let entities: Vec<EntityKey> = (a.params.iter())
                    .map(|p| self.ts.entity(p.entity).key)
                    .collect();
                (a.slots_written_twice().into_iter())
                    .map(move |(var, _)| (a.name.clone(), var, a.site.clone(), entities.clone()))
            })
            .collect();
        for (usecase, var, site, entities) in found {
            let column = self.ts.var_name(var);
            self.diag(
                entities,
                StateDiag::ConflictingEffects {
                    usecase,
                    column,
                    sites: vec![site],
                },
            );
        }
    }

    /// Records a diagnostic once. One raised while a use case action is built may have
    /// dropped or changed a guard or an effect of it, so it also involves every entity that
    /// action binds: a selection that reads the action does not defer it.
    fn diag(&mut self, entities: Vec<EntityKey>, diag: StateDiag) {
        let d = Diag { entities, diag };
        let at = match self.diags.iter().position(|(x, _)| *x == d) {
            Some(at) => at,
            None => {
                self.diags.push((d, Vec::new()));
                self.diags.len() - 1
            }
        };
        let altered = &mut self.diags[at].1;
        for k in &self.acting {
            if !altered.contains(k) {
                altered.push(*k);
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
            ends: Vec::new(),
        });
        id
    }
}

/// Why a copy has no source row: the reason for `UnboundCopySource`, or the keys that could
/// lead to it.
enum CopyMiss {
    Unbound(String),
    Ambiguous(Vec<FkId>),
}

enum EffectRhs {
    Value(Value),
    /// `sets(.., col == present)` on a nullable Int, Enum, or Bool column: any non-null value.
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
        id8(self.params.len() - 1)
    }

    /// The param of a row of `entity` the action writes.
    fn param_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_written())
            .map(|i| id8(i))
    }

    /// The param of the action's row of `entity`: one it writes, or one an earlier step of
    /// its run wrote.
    fn row_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_row())
            .map(|i| id8(i))
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
        Effect::Choose { values, .. } if values.iter().all(|c| c.site.is_none()) => {
            a.effects.remove(at);
            None
        }
        Effect::Assign { site, .. } => Some(site.clone()),
        Effect::Choose { values, .. } => values.first().and_then(|c| c.site.clone()),
        Effect::Transition { arms, .. } => arms.first().map(|arm| arm.site.clone()),
        _ => None,
    }
}

/// The BUC a use case's actions name in `states` provenance: of the BUCs that hold it,
/// directly or through a step of one of their flows, the first by id.
fn first_buc(model: &SemanticModel, uc: UseCaseKey) -> Option<crate::model::BucKey> {
    crate::business_flow::bucs_of_use_case(model, uc)
        .into_iter()
        .next()
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
        .and_then(|i| u8::try_from(i).ok())
}

/// Term equality of two slots whose domains `compares_with` each other, under which null
/// equals null. Every engine reads a `Pred::Cmp` of two non-numeric slots as equality of their
/// values, which numbers an Enum or Bool column with null one past the same column without
/// it, so such a pair compares value by value instead: `l = v /\ r = v` for some declared
/// `v`, which a null on either side never meets.
fn slots_eq((lt, ld): (Term, &Domain), (rt, rd): (Term, &Domain)) -> Pred {
    if ld == rd || ld.is_numeric() || ld.without_null() != rd.without_null() {
        return Pred::Cmp(lt, CmpOp::Eq, rt);
    }
    let declared = ld.names().map_or(2, <[String]>::len);
    Pred::Or(
        (0..declared)
            .map(|i| {
                Pred::And(vec![
                    Pred::Cmp(lt.clone(), CmpOp::Eq, Term::Lit(ld.of_variant(i))),
                    Pred::Cmp(rt.clone(), CmpOp::Eq, Term::Lit(rd.of_variant(i))),
                ])
            })
            .collect(),
    )
}

#[cfg(test)]
mod width_tests {
    use super::too_wide;
    use crate::lifecycle::explore::{packed_bits, MOST_PACKED_BITS};
    use crate::lifecycle::{Bounds, Scope, Severity, StateDiag, TransitionSystem};
    use std::num::NonZeroU8;

    /// The build counts a state's packed bits and stops a system one bit past the bits a
    /// packed state addresses with `StateTooWide`, an error (P50). The P50 reproduction's
    /// state, 255 rows of 34 Int columns at Int 0..255, passes bit 65,535 and stays far inside.
    #[test]
    fn a_state_past_the_packed_width_is_a_build_error() {
        let cols: String = (0..34)
            .map(|i| format!("  c{i}: Int @default(0)\n"))
            .collect();
        let sets: String = (0..34)
            .map(|i| format!("sets(Create, Row.c{i} == 1)\n"))
            .collect();
        let src = format!(
            "entity Row \"行\" {{\n  id: Int @pk\n{cols}}}\nusecase Create \"作成\"\ncreates(Create, Row)\n{sets}"
        );
        let bounds = Bounds::uniform(NonZeroU8::MAX, 255);
        let model = crate::lifecycle::tests::model_from(&src);
        let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
        let bits = packed_bits(&built.ts);
        assert!(bits > u64::from(u16::MAX), "{bits}");
        assert!(
            !(built.diags.iter()).any(|d| matches!(d.diag, StateDiag::StateTooWide { .. })),
            "{:?}",
            built.diags
        );
        assert_eq!(too_wide(bits, bits), None);
        let past = too_wide(bits, bits - 1).expect("one bit past is too wide");
        assert_eq!(past.severity(), Severity::Error);
        assert_eq!(past.name(&[]), "StateTooWide");
        assert!(past.to_string().contains(&format!("{bits} bits")), "{past}");
        assert_eq!(too_wide(MOST_PACKED_BITS, MOST_PACKED_BITS), None);
        assert!(too_wide(MOST_PACKED_BITS + 1, MOST_PACKED_BITS).is_some());
    }
}
