//! Lifecycle findings, as `check` names them, attributed to the BUCs and stages they block.

use std::collections::HashMap;

use super::facts::BucFacts;
use super::stages::Rule;
use super::{FindingRef, StageId};
use crate::lifecycle::{derive_states, ActionOrigin, Caps, ModelRef, Scope, StateDiag};
use crate::model::{EntityKey, SemanticModel};

/// Which stage owns a finding. An exhaustive match: a new `StateDiag` does not compile until
/// someone decides which stage it blocks.
pub(super) fn owning_stage(model: &SemanticModel, diag: &StateDiag) -> StageId {
    match diag {
        StateDiag::ConstraintViolated { site, .. } => {
            rule_stage(model, &site.fact).unwrap_or(StageId::Lifecycle)
        }
        StateDiag::UnbindableScope { site, .. }
        | StateDiag::UnsupportedComparison { site, .. }
        | StateDiag::UnwrittenConstrainedVar { site, .. } => {
            rule_stage(model, &site.fact).unwrap_or(StageId::AdvancedRules)
        }
        StateDiag::ContradictoryDrivenComparison { .. } => StageId::AdvancedRules,
        StateDiag::UnreachableEnumVariant { .. }
        | StateDiag::ConflictingEffects { .. }
        | StateDiag::DoubleModeledEnum { .. }
        | StateDiag::NoCreationPath
        | StateDiag::PreExistingChildOfCreatedParent { .. }
        | StateDiag::MinCardOfPreExistingParent { .. }
        | StateDiag::MinCardNeedsCreate { .. }
        | StateDiag::PatternCapReached { .. }
        | StateDiag::StateCapReached { .. }
        | StateDiag::AfterAssertViolated { .. }
        | StateDiag::PropertyViolated { .. }
        | StateDiag::NoWitness { .. }
        | StateDiag::NeverRuns { .. }
        | StateDiag::Deadlock { .. }
        | StateDiag::AssertOnInertUseCase { .. }
        | StateDiag::IntLiteralOutOfBounds { .. }
        | StateDiag::UnbindableCondition { .. }
        | StateDiag::DerivedColumnRead { .. }
        | StateDiag::UnresolvedPropertyAtom { .. } => StageId::Lifecycle,
    }
}

/// The rule stage of the constraint a site names, if it names one. A `@check`, a `relate`
/// key, or a column is not a rule.
fn rule_stage(model: &SemanticModel, fact: &ModelRef) -> Option<StageId> {
    let rule = match *fact {
        ModelRef::Forbidden(i) => Rule::Forbidden(model.forbidden_constraints.get(i)?),
        ModelRef::Exclusive(i) => Rule::Exclusive(model.exclusive_constraints.get(i)?),
        ModelRef::EntityInvariant(i) => Rule::Invariant(model.entity_invariants.get(i)?),
        ModelRef::Required(i) => Rule::Required(model.required_constraints.get(i)?),
        ModelRef::CrossForbidden(_) | ModelRef::CrossInvariant(_) | ModelRef::Quantifier(_) => {
            return Some(StageId::AdvancedRules)
        }
        _ => return None,
    };
    Some(rule.tier().stage())
}

/// Every finding of a whole-model run (`check`'s scope), keyed by BUC index and stage, and
/// the findings no BUC owns.
pub(super) struct Attributed {
    pub owned: HashMap<(usize, StageId), Vec<FindingRef>>,
    pub unowned: Vec<FindingRef>,
}

pub(super) fn attribute(model: &SemanticModel, facts: &[BucFacts]) -> Attributed {
    let report = derive_states(model, &Scope::ALL, Caps::DEFAULT_PATTERNS);
    let entity_key = |id: &str| {
        model
            .entities
            .iter()
            .find(|(_, e)| e.id == id)
            .map(|(k, _)| k)
    };
    let on_views = report.views.iter().flat_map(|view| {
        let owners: Vec<EntityKey> = entity_key(&view.entity_id).into_iter().collect();
        view.diagnostics
            .iter()
            .map(move |diag| (owners.clone(), diag))
    });
    let on_system = report.diags.iter().map(|d| (d.entities.clone(), &d.diag));

    let mut attributed = Attributed {
        owned: HashMap::new(),
        unowned: Vec::new(),
    };
    let mut seen = Vec::new();
    for (owners, diag) in on_views.chain(on_system) {
        let ids: Vec<&str> = owners
            .iter()
            .map(|&key| model.entities[key].id.as_str())
            .collect();
        let finding = FindingRef {
            name: diag.name(&ids),
            severity: diag.severity(),
        };
        if seen.contains(&finding.name) {
            continue;
        }
        seen.push(finding.name.clone());
        let stage = owning_stage(model, diag);
        let bucs = owning_bucs(&owners, diag, facts);
        if bucs.is_empty() {
            attributed.unowned.push(finding);
            continue;
        }
        for buc in bucs {
            attributed
                .owned
                .entry((buc, stage))
                .or_default()
                .push(finding.clone());
        }
    }
    attributed
}

/// The BUCs that touch one of the finding's entities, or whose use case is the action that
/// never runs.
fn owning_bucs(entities: &[EntityKey], diag: &StateDiag, facts: &[BucFacts]) -> Vec<usize> {
    facts
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            entities.iter().any(|entity| f.entities.contains(entity))
                || matches!(
                    diag,
                    StateDiag::NeverRuns {
                        origin: ActionOrigin::UseCase { uc, .. },
                        ..
                    } if f.use_cases.contains(uc)
                )
        })
        .map(|(i, _)| i)
        .collect()
}
