//! Exploration per target: each entity view and each constraint verdict is computed on the
//! cone it reads, and identical cones are explored once.

use std::collections::HashMap;

use super::cone::{Cone, Target};
use super::diag::{Diag, StateDiag};
use super::explore::{Caps, ComponentRun};
use super::types::EntId;
use super::view::{entity_view, EntityStateResult};
use super::TransitionSystem;
use crate::model::SemanticModel;

pub struct ConeRun {
    pub cone: Cone,
    /// `restrict_to(cone)` explored as one system.
    pub run: ComponentRun,
}

pub struct TargetExploration {
    pub cones: Vec<ConeRun>,
    /// Each target and the index of the cone run that answers it.
    pub targets: Vec<(Target, usize)>,
    verdicts: Vec<Diag>,
}

impl TransitionSystem {
    pub fn explore_targets(&self, caps: Caps) -> TargetExploration {
        let mut cones: Vec<ConeRun> = Vec::new();
        let mut index: HashMap<Cone, usize> = HashMap::new();
        let mut targets = Vec::new();
        for target in self.targets() {
            let cone = self.target_cone(target);
            let at = *index.entry(cone.clone()).or_insert_with(|| {
                let run = self.restrict_to(&cone.vars, &cone.keep).run(caps);
                cones.push(ConeRun {
                    cone: cone.clone(),
                    run,
                });
                cones.len() - 1
            });
            targets.push((target, at));
        }
        let verdicts = targets
            .iter()
            .flat_map(|(t, at)| self.verdict(*t, &cones[*at].run))
            .collect();
        TargetExploration {
            cones,
            targets,
            verdicts,
        }
    }

    /// The diagnostic is a violation of this invariant or assert.
    fn reports(&self, target: Target, d: &StateDiag) -> bool {
        match (target, d) {
            (Target::Invariant(i), StateDiag::ConstraintViolated { name, .. }) => {
                *name == self.invariants[i].name
            }
            (Target::AfterAssert(i), StateDiag::AfterAssertViolated { site, .. }) => {
                *site == self.after[i].site
            }
            _ => false,
        }
    }

    /// What the target's own cone says about an invariant or assert: its violations, and any
    /// cap that left the verdict incomplete.
    fn verdict(&self, target: Target, run: &ComponentRun) -> Vec<Diag> {
        let entities: Vec<_> = match target {
            Target::Entity(_) => return Vec::new(),
            Target::Invariant(i) => self.invariants[i]
                .binders
                .iter()
                .map(|e| self.entity(*e).key)
                .collect(),
            Target::AfterAssert(i) => {
                let mut entities = Vec::new();
                for step in &self.after[i].steps {
                    let params = self.actions[step.act.index()].params.iter();
                    for e in step.binders.iter().copied().chain(params.map(|p| p.entity)) {
                        let key = self.entity(e).key;
                        if !entities.contains(&key) {
                            entities.push(key);
                        }
                    }
                }
                entities
            }
        };
        let mut out: Vec<Diag> = run
            .diags
            .iter()
            .filter(|d| self.reports(target, &d.diag))
            .cloned()
            .collect();
        for d in run.diags.iter().filter(|d| {
            matches!(
                d.diag,
                StateDiag::PatternCapReached { .. } | StateDiag::StateCapReached { .. }
            )
        }) {
            out.push(Diag {
                entities: entities.clone(),
                diag: d.diag.clone(),
            });
        }
        out
    }
}

impl TargetExploration {
    pub fn cone_of(&self, target: Target) -> &ConeRun {
        let at = self
            .targets
            .iter()
            .find(|(t, _)| *t == target)
            .map(|(_, at)| *at)
            .expect("unknown target");
        &self.cones[at]
    }

    /// Constraint and assert verdicts, each from its own cone.
    pub fn verdicts(&self) -> &[Diag] {
        &self.verdicts
    }

    /// Per-entity views sorted by entity id, each from the entity's cone.
    pub fn entity_views(
        &self,
        ts: &TransitionSystem,
        model: &SemanticModel,
        build_diags: &[Diag],
    ) -> Vec<EntityStateResult> {
        let mut out: Vec<EntityStateResult> = (0..ts.entities().len())
            .map(|i| {
                let key = ts.entities()[i].key;
                let run = &self.cone_of(Target::Entity(EntId::at(i))).run;
                let local = run
                    .system
                    .entities()
                    .iter()
                    .position(|d| d.key == key)
                    .expect("an entity is in its own cone");
                let verdicts = self
                    .verdicts
                    .iter()
                    .filter(|d| d.entities.contains(&key))
                    .map(|d| d.diag.clone())
                    .fold(Vec::new(), |mut acc, d| {
                        if !acc.contains(&d) {
                            acc.push(d);
                        }
                        acc
                    });
                entity_view(model, run, EntId::at(local), build_diags, Some(verdicts))
            })
            .collect();
        out.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        out
    }
}
