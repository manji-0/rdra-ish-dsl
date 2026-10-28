//! Exploration per target: each entity view and each constraint verdict is computed on the
//! cone it reads, and identical cones are explored once.

use std::collections::HashMap;
use std::fmt;

use super::cone::{Cone, Target};
use super::diag::{Diag, StateDiag};
use super::explore::{Caps, ComponentRun};
use super::findings::Witness;
use super::types::EntId;
use super::view::{entity_view, AbstractValue, EntityStateResult};
use super::TransitionSystem;
use crate::model::{EntityKey, SemanticModel};

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
    undecided: Vec<Undecided>,
}

/// A state of a state model that a run neither witnessed nor ruled out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undecided {
    pub entity: String,
    pub column: String,
    pub value: String,
    pub why: UndecidedWhy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndecidedWhy {
    /// The entity's cone stopped at this many states.
    StateCap(usize),
    /// The entity's cone stopped at this many patterns of one entity.
    PatternCap(usize),
    /// `--buc` leaves use cases out, so a state they would reach is not ruled out.
    Scope,
}

impl fmt::Display for Undecided {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Undecided {
            entity,
            column,
            value,
            why,
        } = self;
        write!(f, "state {entity}.{column} = {value} is not decided: ")?;
        match why {
            UndecidedWhy::StateCap(cap) => {
                write!(
                    f,
                    "the {entity} cone stopped at the state cap ({cap} states)"
                )
            }
            UndecidedWhy::PatternCap(cap) => write!(
                f,
                "the {entity} cone stopped at the pattern cap ({cap} patterns)"
            ),
            UndecidedWhy::Scope => write!(f, "--buc leaves use cases out of the run"),
        }
    }
}

impl TransitionSystem {
    /// Explores each target on its cone; targets with the same cone share one run.
    pub fn explore_targets(&self, targets: &[Target], caps: Caps) -> TargetExploration {
        let mut cones: Vec<ConeRun> = Vec::new();
        let mut index: HashMap<Cone, usize> = HashMap::new();
        let mut answered = Vec::new();
        for &target in targets {
            let cone = self.target_cone(target);
            let at = *index.entry(cone.clone()).or_insert_with(|| {
                let run = self.restrict_to(&cone.vars, &cone.keep).run(caps);
                cones.push(ConeRun {
                    cone: cone.clone(),
                    run,
                });
                cones.len() - 1
            });
            answered.push((target, at));
        }
        let mut verdicts = Vec::new();
        let mut undecided = Vec::new();
        for &(target, at) in &answered {
            match target {
                Target::Entity(e) => {
                    self.witnesses(e, &cones[at].run, &mut verdicts, &mut undecided)
                }
                _ => verdicts.extend(self.verdict(target, &cones[at])),
            }
        }
        TargetExploration {
            cones,
            targets: answered,
            verdicts,
            undecided,
        }
    }

    /// The id of the entity `key` in this (restricted) system.
    pub(crate) fn local_entity(&self, key: EntityKey) -> EntId {
        let at = self
            .entities()
            .iter()
            .position(|d| d.key == key)
            .expect("an entity is in its own cone");
        EntId::at(at)
    }

    /// The `NoWitness` errors of `e`'s state models on its cone, and the states a capped cone
    /// leaves undecided.
    fn witnesses(
        &self,
        e: EntId,
        run: &ComponentRun,
        verdicts: &mut Vec<Diag>,
        undecided: &mut Vec<Undecided>,
    ) {
        let key = self.entity(e).key;
        let local = run.system.local_entity(key);
        let why = run.diags.iter().find_map(|d| match d.diag {
            StateDiag::StateCapReached { cap } => Some(UndecidedWhy::StateCap(cap)),
            StateDiag::PatternCapReached { cap, .. } => Some(UndecidedWhy::PatternCap(cap)),
            _ => None,
        });
        let findings = run.system.state_model_findings(local, &run.graph());
        for w in findings.witnesses {
            match w.witness {
                Witness::At(_) => {}
                Witness::Missing(diag) => verdicts.push(Diag {
                    diag: *diag,
                    entities: vec![key],
                }),
                Witness::NotDecided => undecided.push(Undecided {
                    entity: self.entity(e).id.clone(),
                    column: run.system.var_name(w.var),
                    value: AbstractValue::of(&run.system.var(w.var).domain, w.value)
                        .display_with_type(None),
                    why: why.expect("an incomplete run stopped at a cap"),
                }),
            }
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

    /// What the target's own cone says about an invariant or assert: its violations, each
    /// trace marked when it creates more instances than the bound, and any cap that left the
    /// verdict incomplete.
    fn verdict(&self, target: Target, cone: &ConeRun) -> Vec<Diag> {
        let run = &cone.run;
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
            .map(|mut d| {
                if let Some(trace) = d.diag.trace_mut() {
                    trace.over_bound = self.over_bound(&cone.cone, trace);
                }
                d
            })
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

    /// Constraint and assert verdicts and `NoWitness` errors, each from its own cone.
    pub fn verdicts(&self) -> &[Diag] {
        &self.verdicts
    }

    /// The states of state models no cone decided.
    pub fn undecided(&self) -> &[Undecided] {
        &self.undecided
    }

    /// A run whose `--buc` drops use cases cannot rule a state out: its `NoWitness` errors
    /// become undecided states.
    pub(crate) fn defer_witnesses(&mut self) {
        let (missing, kept): (Vec<Diag>, Vec<Diag>) = std::mem::take(&mut self.verdicts)
            .into_iter()
            .partition(|d| matches!(d.diag, StateDiag::NoWitness { .. }));
        self.verdicts = kept;
        self.undecided
            .extend(missing.into_iter().filter_map(|d| match d.diag {
                StateDiag::NoWitness {
                    entity,
                    column,
                    value,
                    ..
                } => Some(Undecided {
                    entity,
                    column,
                    value,
                    why: UndecidedWhy::Scope,
                }),
                _ => None,
            }));
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
                let cone = self.cone_of(Target::Entity(EntId::at(i)));
                let run = &cone.run;
                let local = run.system.local_entity(key);
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
                let mut view = entity_view(model, run, local, build_diags, Some(verdicts));
                for trace in view.patterns.iter_mut().filter_map(|p| p.witness.as_mut()) {
                    trace.over_bound = ts.over_bound(&cone.cone, trace);
                }
                view
            })
            .collect();
        out.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        out
    }
}
