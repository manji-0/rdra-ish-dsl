//! Exploration per target: each entity view and each constraint verdict is computed on the
//! cone it reads, and identical cones are explored once.

use std::collections::HashMap;
use std::fmt;

use super::cone::{Cone, Target};
use super::diag::{Diag, StateDiag};
use super::explore::{Caps, ComponentRun};
use super::findings::{Runs, Witness};
use super::types::{EntId, ParamRole};
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
    tally: Tally,
}

/// What the entity cones decided, counted for a summary line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Distinct cones the entity targets ran on.
    pub entity_cones: usize,
    /// Columns with a state model.
    pub state_models: usize,
    /// States of those models some reachable state holds.
    pub witnessed: usize,
    /// Actions no cone that keeps them runs.
    pub never_run: usize,
}

/// A progress finding a run neither confirmed nor ruled out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undecided {
    pub subject: Subject,
    /// The entity whose cone stopped; `None` when the scope, not a cone, left it open.
    pub cone: Option<String>,
    pub why: UndecidedWhy,
}

/// What an undecided finding is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    /// Whether a state of a state model is reachable.
    State {
        entity: String,
        column: String,
        value: String,
    },
    /// Whether an action can run.
    Runs { action: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndecidedWhy {
    /// The entity's cone stopped at this many states.
    StateCap(usize),
    /// The entity's cone stopped at this many patterns of one entity.
    PatternCap(usize),
    /// `--buc` leaves use cases out, so what they would reach or enable is not ruled out.
    Scope,
}

impl fmt::Display for Undecided {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.subject {
            Subject::State {
                entity,
                column,
                value,
            } => write!(f, "state {entity}.{column} = {value} is not decided: ")?,
            Subject::Runs { action } => write!(f, "whether {action} can run is not decided: ")?,
        }
        let cone = self.cone.as_deref().unwrap_or("its");
        match self.why {
            UndecidedWhy::StateCap(cap) => {
                write!(f, "the {cone} cone stopped at the state cap ({cap} states)")
            }
            UndecidedWhy::PatternCap(cap) => write!(
                f,
                "the {cone} cone stopped at the pattern cap ({cap} patterns)"
            ),
            UndecidedWhy::Scope => write!(f, "--buc leaves use cases out of the run"),
        }
    }
}

/// What the entity cones that keep one action say about it.
#[derive(Default)]
struct RunsSoFar {
    fired: bool,
    /// The first complete cone's `NeverRuns`.
    never: Option<StateDiag>,
    /// The first capped cone: its entity and its cap.
    capped: Option<(String, UndecidedWhy)>,
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
        let mut out = TargetExploration {
            cones: Vec::new(),
            targets: Vec::new(),
            verdicts: Vec::new(),
            undecided: Vec::new(),
            tally: Tally::default(),
        };
        let mut runs: Vec<RunsSoFar> = self.actions.iter().map(|_| RunsSoFar::default()).collect();
        let mut entity_cones = Vec::new();
        for &(target, at) in &answered {
            match target {
                Target::Entity(e) => {
                    if !entity_cones.contains(&at) {
                        entity_cones.push(at);
                    }
                    self.entity_findings(e, &cones[at].run, &mut out, &mut runs);
                }
                _ => out.verdicts.extend(self.verdict(target, &cones[at])),
            }
        }
        out.cones = cones;
        out.tally.entity_cones = entity_cones.len();
        for (i, r) in runs.into_iter().enumerate() {
            if r.fired {
                continue;
            }
            if let Some(diag) = r.never {
                let action = &self.actions[i];
                let entities = action
                    .params
                    .iter()
                    .filter(|p| p.role != ParamRole::Parent)
                    .map(|p| self.entity(p.entity).key)
                    .collect();
                out.verdicts.push(Diag { entities, diag });
                out.tally.never_run += 1;
            } else if let Some((cone, why)) = r.capped {
                out.undecided.push(Undecided {
                    subject: Subject::Runs {
                        action: self.actions[i].name.clone(),
                    },
                    cone: Some(cone),
                    why,
                });
            }
        }
        out.targets = answered;
        out
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

    /// The `NoWitness` errors of `e`'s state models on its cone and the states a capped cone
    /// leaves undecided; and into `runs`, by this system's action, what the cone says about
    /// each action it keeps.
    fn entity_findings(
        &self,
        e: EntId,
        run: &ComponentRun,
        out: &mut TargetExploration,
        runs: &mut [RunsSoFar],
    ) {
        let decl = self.entity(e);
        let local = run.system.local_entity(decl.key);
        let why = run.diags.iter().find_map(|d| match d.diag {
            StateDiag::StateCapReached { cap } => Some(UndecidedWhy::StateCap(cap)),
            StateDiag::PatternCapReached { cap, .. } => Some(UndecidedWhy::PatternCap(cap)),
            _ => None,
        });
        let capped = || why.expect("an incomplete run stopped at a cap");
        let findings = run.system.state_model_findings(local, &run.graph());
        let mut models = Vec::new();
        for w in findings.witnesses {
            if !models.contains(&w.var) {
                models.push(w.var);
            }
            match w.witness {
                Witness::At(_) => out.tally.witnessed += 1,
                Witness::Missing(diag) => out.verdicts.push(Diag {
                    diag: *diag,
                    entities: vec![decl.key],
                }),
                Witness::NotDecided => out.undecided.push(Undecided {
                    subject: Subject::State {
                        entity: decl.id.clone(),
                        column: run.system.var_name(w.var),
                        value: AbstractValue::of(&run.system.var(w.var).domain, w.value)
                            .display_with_type(None),
                    },
                    cone: Some(decl.id.clone()),
                    why: capped(),
                }),
            }
        }
        out.tally.state_models += models.len();
        for a in findings.runs {
            let name = &run.system.actions()[a.act.index()].name;
            let at = self
                .actions
                .iter()
                .position(|x| x.name == *name)
                .expect("a cone action is an action of the system");
            let r = &mut runs[at];
            match a.runs {
                Runs::Fired => r.fired = true,
                Runs::Never(diag) => {
                    r.never.get_or_insert(*diag);
                }
                Runs::NotDecided => {
                    r.capped.get_or_insert_with(|| (decl.id.clone(), capped()));
                }
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

    /// Constraint and assert verdicts, `NoWitness` errors, and `NeverRuns` warnings, each from
    /// its own cone and each once.
    pub fn verdicts(&self) -> &[Diag] {
        &self.verdicts
    }

    /// The states of state models and the actions no cone decided.
    pub fn undecided(&self) -> &[Undecided] {
        &self.undecided
    }

    /// What the entity cones decided, counted.
    pub fn tally(&self) -> Tally {
        self.tally
    }

    /// A run whose `--buc` drops use cases rules neither a state nor an action out: its
    /// `NoWitness` errors and `NeverRuns` warnings become undecided.
    pub(crate) fn defer_progress(&mut self) {
        let mut kept = Vec::new();
        for d in std::mem::take(&mut self.verdicts) {
            let subject = match &d.diag {
                StateDiag::NoWitness {
                    entity,
                    column,
                    value,
                    ..
                } => Subject::State {
                    entity: entity.clone(),
                    column: column.clone(),
                    value: value.clone(),
                },
                StateDiag::NeverRuns { action, .. } => Subject::Runs {
                    action: action.clone(),
                },
                _ => {
                    kept.push(d);
                    continue;
                }
            };
            self.undecided.push(Undecided {
                subject,
                cone: None,
                why: UndecidedWhy::Scope,
            });
        }
        self.verdicts = kept;
        self.tally.never_run = 0;
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
