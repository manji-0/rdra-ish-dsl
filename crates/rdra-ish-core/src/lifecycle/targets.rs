//! Exploration per target: each entity view and each constraint verdict is computed on the
//! cone it reads, and identical cones are explored once.

use std::collections::HashMap;
use std::fmt;

use super::cone::{Cone, Target};
use super::diag::{Diag, StateDiag, UnmetPremise};
use super::explore::{Caps, Checks, ComponentRun, StoredRun};
use super::expr::StateSlice;
use super::findings::{Progress, Runs, Taken, Witness};
use super::types::{
    ActionOrigin, AfterAssert, AfterStep, EntId, GuardWhy, Invariant, Ltl, Pred, Premise, Site,
    Term, VarKind,
};
use super::view::{entity_view, AbstractValue, EntityStateResult};
use super::{QuotientCache, TransitionSystem};
use crate::memory::{watch, MemorySize};
use crate::model::{EntityKey, SemanticModel};
use crate::parallel::{in_parallel, jobs};

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
    /// States of those models an instance can get stuck at, the ones at the bound included.
    pub deadlocks: usize,
    /// Actions no cone that keeps them runs.
    pub never_run: usize,
}

/// A progress finding a run neither confirmed nor ruled out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undecided {
    pub subject: Subject,
    /// The entity or the `can_reach` property whose cone stopped; `None` when the scope, not
    /// a cone, left it open.
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
    /// Whether an action that runs can take one of its `.when` arms.
    Arm {
        action: String,
        column: String,
        from: String,
        to: String,
    },
    /// Whether an action that runs can pick one of its `.when` values of a `chooses`.
    Choice {
        action: String,
        column: String,
        value: String,
    },
    /// Whether an instance can get stuck at a state of a state model.
    Deadlock {
        entity: String,
        column: String,
        value: String,
    },
    /// Whether a `can_reach` property holds.
    Reach { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndecidedWhy {
    /// The entity's cone stopped at this many states.
    StateCap(usize),
    /// The entity's cone stopped at this many patterns of one entity.
    PatternCap(usize),
    /// The entity's cone reached the memory limit; the str says how to raise it.
    MemoryCap(MemorySize, &'static str),
    /// `--buc` leaves use cases out, so what they would reach or enable is not ruled out.
    Scope,
    /// `--entity` explores no cone that keeps the action.
    Unselected,
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
            Subject::Arm {
                action,
                column,
                from,
                to,
            } => write!(
                f,
                "whether {action} can take {column} {from} -> {to} is not decided: "
            )?,
            Subject::Choice {
                action,
                column,
                value,
            } => write!(
                f,
                "whether {action} can choose {column} = {value} is not decided: "
            )?,
            Subject::Deadlock {
                entity,
                column,
                value,
            } => write!(
                f,
                "whether some {entity} can get stuck at {column} = {value} is not decided: "
            )?,
            Subject::Reach { name } => write!(f, "can_reach {name} is not decided: ")?,
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
            UndecidedWhy::MemoryCap(limit, raise) => write!(
                f,
                "the {cone} cone stopped at the memory limit ({limit}; raise it with {raise})"
            ),
            UndecidedWhy::Scope => write!(f, "--buc leaves use cases out of the run"),
            UndecidedWhy::Unselected => write!(f, "no cone of a selected entity keeps it"),
        }
    }
}

/// The cap or the limit a run stopped at, read off its diagnostics; `None` for a run that did
/// not stop.
fn stopped_at(run: &ComponentRun) -> Option<UndecidedWhy> {
    run.diags.iter().find_map(|d| match d.diag {
        StateDiag::StateCapReached { cap, .. } => Some(UndecidedWhy::StateCap(cap)),
        StateDiag::PatternCapReached { cap, .. } => Some(UndecidedWhy::PatternCap(cap)),
        StateDiag::MemoryCapReached { limit, raise, .. } => {
            Some(UndecidedWhy::MemoryCap(limit, raise))
        }
        _ => None,
    })
}

/// What the entity cones that keep one action say about it.
#[derive(Default)]
struct RunsSoFar {
    /// Some explored entity cone keeps the action.
    kept: bool,
    fired: bool,
    /// The first complete cone's `NeverRuns`.
    never: Option<StateDiag>,
    /// The first capped cone: its entity and its cap.
    capped: Option<(String, UndecidedWhy)>,
    /// Each `.when` arm of the action, by its `transitions` site.
    arms: Vec<ArmSoFar>,
}

/// What the entity cones that keep an action say about one of its `.when` arms.
struct ArmSoFar {
    site: Site,
    subject: Subject,
    /// Some cone took it.
    taken: bool,
    /// The first complete cone's `ArmNeverTaken`.
    never: Option<StateDiag>,
    /// The first capped cone that did not take it: its entity and its cap.
    capped: Option<(String, UndecidedWhy)>,
}

impl TransitionSystem {
    /// Explores each target on its cone; targets with the same cone share one run.
    pub fn explore_targets(&self, targets: &[Target], caps: Caps) -> TargetExploration {
        self.explore_targets_with(targets, caps, None, &[])
    }

    /// `explore_targets` with each cone reduced by its independent sub-lifecycle
    /// (`TransitionSystem::reduced`), as `verify` explores.
    pub fn explore_targets_reduced(&self, targets: &[Target], caps: Caps) -> TargetExploration {
        self.explore_targets_reduced_in(targets, caps, &QuotientCache::new(), &[])
    }

    /// `explore_targets_reduced`, sharing quotients through `cache` with the other cones
    /// reduced in it. A cone whose system is one of `every_check` evaluates every check its
    /// system holds, whatever its targets read, so that a caller can decide on its run the
    /// targets of a later pass that runs that system (#209): `verify`'s safety and property
    /// cones.
    pub fn explore_targets_reduced_in(
        &self,
        targets: &[Target],
        caps: Caps,
        cache: &QuotientCache,
        every_check: &[&TransitionSystem],
    ) -> TargetExploration {
        self.explore_targets_with(targets, caps, Some(cache), every_check)
    }

    /// `reduce` holds the quotients the cones are reduced with; `None` leaves them whole.
    fn explore_targets_with(
        &self,
        targets: &[Target],
        caps: Caps,
        reduce: Option<&QuotientCache>,
        every_check: &[&TransitionSystem],
    ) -> TargetExploration {
        let mut groups: Vec<(Cone, Vec<Target>)> = Vec::new();
        let mut index: HashMap<Cone, usize> = HashMap::new();
        let mut answered = Vec::new();
        for &target in targets {
            let cone = self.target_cone(target);
            let at = *index.entry(cone.clone()).or_insert_with(|| {
                groups.push((cone.clone(), Vec::new()));
                groups.len() - 1
            });
            groups[at].1.push(target);
            answered.push((target, at));
        }
        // Each cone is explored on its own system, so the cones run side by side; the runs
        // come back in cone order, and each is what it is alone.
        let explore = |cone: &Cone, shared: &[Target], caps: Caps| {
            // Only an entity target's view reads the patterns (`entity_views`), so a cone of
            // other targets gathers none and the pattern cap does not stop it (#251).
            let caps = match shared.iter().any(|t| matches!(t, Target::Entity(_))) {
                true => caps,
                false => caps.without_views(),
            };
            // A reduction keeps what LTL reads, not `AG EF`: a `can_reach` cone stays whole.
            let reach = shared.iter().any(|t| matches!(t, Target::Reach(_)));
            let system = match reduce.filter(|_| !reach) {
                Some(cache) => {
                    let seeds = shared.iter().map(|t| self.target_seed(*t));
                    self.reduced_cone_in(cone, seeds, caps, cache)
                }
                None => self.restrict_to(&cone.vars, &cone.keep),
            };
            // A cone of checks alone is decided once each has failed, which reads every check
            // its system holds; an entity target or a `can_reach` reads the whole graph, and
            // nothing reads the checks of such a cone but its own check targets, and the
            // caller's later pass when the system is one of `every_check`.
            let checks = shared
                .iter()
                .all(|t| matches!(t, Target::Invariant(_) | Target::AfterAssert(_)));
            match (checks, every_check.contains(&&system)) {
                (true, _) => system.run_checking(caps.until_all_violated(), Checks::All),
                (false, true) => system.run_checking(caps, Checks::All),
                (false, false) => system.run_checking(caps, self.checks_of(shared)),
            }
        };
        let ran = in_parallel(groups, jobs(), |(cone, shared)| {
            let (run, seen) = watch(|| explore(&cone, &shared, caps));
            (cone, shared, run, seen)
        });
        // Under a memory limit the result is the one of the cones run one at a time in cone
        // order, each with the room the results before it leave. A cone whose run side by
        // side met a fuller pool, or reached past that room, runs again alone with it.
        // A crowded run is dropped before the first run again, so that what the runs hold
        // stays near the limit.
        let ran: Vec<_> = ran
            .into_iter()
            .map(|(cone, shared, run, seen)| {
                let run = (!seen.crowded).then_some(run);
                (cone, shared, run, seen.peak)
            })
            .collect();
        let mut held = 0u64;
        let mut cones = Vec::with_capacity(ran.len());
        for (cone, shared, run, peak) in ran {
            let run = match (caps.memory, run) {
                (None, Some(run)) => run,
                (Some(pool), run) => {
                    let room = pool.room().saturating_sub(held);
                    let run = match run.filter(|_| peak <= room) {
                        Some(run) => run,
                        None => explore(&cone, &shared, caps.charged_to(pool.with_room(room))),
                    };
                    held += run.kept;
                    run
                }
                (None, None) => unreachable!("only a run charged to a pool is crowded"),
            };
            cones.push(ConeRun { cone, run });
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
                    self.entity_findings(e, &cones[at], &mut out, &mut runs);
                }
                // A state the cap left unexpanded has no edges, so it would look stuck: only a
                // complete graph decides a `can_reach`.
                Target::Reach(i) if !cones[at].run.complete => {
                    let name = self.reach[i].name.clone();
                    out.undecided.push(Undecided {
                        subject: Subject::Reach { name: name.clone() },
                        cone: Some(name),
                        why: stopped_at(&cones[at].run)
                            .expect("an incomplete run stopped at a cap"),
                    });
                }
                _ => out.verdicts.extend(self.verdict(target, &cones[at])),
            }
        }
        out.cones = cones;
        out.tally.entity_cones = entity_cones.len();
        // Whether an action runs is judged on entity cones only: a run of other targets alone
        // (verify's `can_reach` cones) judges none.
        let judged = match entity_cones.is_empty() {
            true => Vec::new(),
            false => self.merge_variants(runs, &mut out.verdicts, &mut out.undecided),
        };
        for (i, r) in judged {
            if r.fired {
                for arm in r.arms.into_iter().filter(|a| !a.taken) {
                    if let Some(diag) = arm.never {
                        let entities = self.written_entities(i);
                        out.verdicts.push(Diag { entities, diag });
                    } else if let Some((cone, why)) = arm.capped {
                        out.undecided.push(Undecided {
                            subject: arm.subject,
                            cone: Some(cone),
                            why,
                        });
                    }
                }
                continue;
            }
            if let Some(diag) = r.never {
                let entities = self.written_entities(i);
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
            } else if !r.kept && self.judged_by_entity_cones(i) {
                out.undecided.push(Undecided {
                    subject: Subject::Runs {
                        action: self.actions[i].name.clone(),
                    },
                    cone: None,
                    why: UndecidedWhy::Unselected,
                });
            }
        }
        out.targets = answered;
        out
    }

    /// The invariants and after-asserts among `targets`, by name: what a run of their cone
    /// reads of its checks.
    fn checks_of(&self, targets: &[Target]) -> Checks {
        Checks::Named(
            targets
                .iter()
                .filter_map(|t| self.check_name(*t))
                .map(str::to_string)
                .collect(),
        )
    }

    /// The name of an invariant or an after-assert target: the name a run evaluates it by.
    pub(super) fn check_name(&self, target: Target) -> Option<&str> {
        match target {
            Target::Invariant(i) => Some(&self.invariants[i].name),
            Target::AfterAssert(i) => Some(&self.after[i].name),
            Target::Entity(_) | Target::Reach(_) => None,
        }
    }

    /// The entities action `i` writes.
    fn written_entities(&self, i: usize) -> Vec<EntityKey> {
        self.actions[i]
            .params
            .iter()
            .filter(|p| p.role.is_written())
            .map(|p| self.entity(p.entity).key)
            .collect()
    }

    /// The `<UC>_by_<Ev>` variants of one use case step are one use case to the findings: it
    /// runs when any variant runs and takes an arm when any variant takes it, and a `NeverRuns`
    /// or an arm finding carries the use case's name. A variant that a complete cone shows
    /// never runs while another ran is a `TriggerNeverRuns` info; one that only capped cones
    /// keep is undecided under its own name. Returns each action's index with what is reported
    /// under it: a group of variants under its first.
    fn merge_variants(
        &self,
        runs: Vec<RunsSoFar>,
        verdicts: &mut Vec<Diag>,
        undecided: &mut Vec<Undecided>,
    ) -> Vec<(usize, RunsSoFar)> {
        let step_of = |i: usize| match self.actions[i].origin {
            ActionOrigin::UseCase { uc, step, .. } => Some((uc, step)),
            ActionOrigin::Clock => None,
        };
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (i, a) in self.actions.iter().enumerate() {
            let ActionOrigin::UseCase {
                trigger: Some(_), ..
            } = a.origin
            else {
                continue;
            };
            match groups.iter_mut().find(|g| step_of(g[0]) == step_of(i)) {
                Some(g) => g.push(i),
                None => groups.push(vec![i]),
            }
        }
        let mut runs: Vec<Option<RunsSoFar>> = runs.into_iter().map(Some).collect();
        for g in groups {
            let (name, _) = self.variant_of(g[0]);
            let rename = |d: &mut StateDiag| match d {
                StateDiag::NeverRuns { action, .. }
                | StateDiag::ArmNeverTaken { action, .. }
                | StateDiag::ValueNeverChosen { action, .. } => *action = name.clone(),
                _ => {}
            };
            let mut merged = RunsSoFar::default();
            let any_fired = g.iter().any(|&i| runs[i].as_ref().is_some_and(|r| r.fired));
            for &i in &g {
                let r = runs[i].take().expect("a variant is merged once");
                if any_fired && !r.fired && r.never.is_none() {
                    if let Some((cone, why)) = &r.capped {
                        undecided.push(Undecided {
                            subject: Subject::Runs {
                                action: self.actions[i].name.clone(),
                            },
                            cone: Some(cone.clone()),
                            why: *why,
                        });
                    }
                } else if any_fired && !r.fired {
                    let action = &self.actions[i];
                    verdicts.push(Diag {
                        entities: action
                            .params
                            .iter()
                            .filter(|p| p.role.is_written())
                            .map(|p| self.entity(p.entity).key)
                            .collect(),
                        diag: StateDiag::TriggerNeverRuns {
                            usecase: name.clone(),
                            event: self.variant_of(i).1,
                            site: action.site.clone(),
                        },
                    });
                }
                merged.kept |= r.kept;
                merged.fired |= r.fired;
                merged.never = merged.never.or(r.never);
                merged.capped = merged.capped.or(r.capped);
                for arm in r.arms {
                    match merged.arms.iter_mut().find(|a| a.site == arm.site) {
                        Some(m) => {
                            m.taken |= arm.taken;
                            m.never = m.never.take().or(arm.never);
                            m.capped = m.capped.take().or(arm.capped);
                        }
                        None => merged.arms.push(arm),
                    }
                }
            }
            if let Some(d) = &mut merged.never {
                rename(d);
            }
            for arm in &mut merged.arms {
                if let Some(d) = &mut arm.never {
                    rename(d);
                }
                match &mut arm.subject {
                    Subject::Arm { action, .. } | Subject::Choice { action, .. } => {
                        *action = name.clone();
                    }
                    _ => {}
                }
            }
            runs[g[0]] = Some(merged);
        }
        runs.into_iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, r?)))
            .collect()
    }

    /// The use case step's name and the event id of a variant `<step>_by_<Ev>`: the event is
    /// the one its mark guard reads.
    fn variant_of(&self, i: usize) -> (String, String) {
        let action = &self.actions[i];
        let ActionOrigin::UseCase {
            trigger: Some(event),
            ..
        } = action.origin
        else {
            unreachable!("a variant has a trigger")
        };
        let event = action
            .guards
            .iter()
            .find_map(|g| match (&g.why, &g.pred) {
                (GuardWhy::Raised { event: e }, Pred::Cmp(Term::Slot { var, .. }, _, _))
                    if *e == event =>
                {
                    match &self.var(*var).kind {
                        VarKind::Raised { event, .. } => Some(event.clone()),
                        _ => None,
                    }
                }
                _ => None,
            })
            .expect("a variant reads its event's mark");
        let name = action
            .name
            .strip_suffix(&format!("_by_{event}"))
            .expect("a variant is named <step>_by_<Ev>")
            .to_string();
        (name, event)
    }

    /// Whether the action can run is decided on the cones of the entities it binds other than
    /// as a parent, as `state_model_findings` does; the clock is not a use case.
    fn judged_by_entity_cones(&self, i: usize) -> bool {
        let action = &self.actions[i];
        matches!(action.origin, ActionOrigin::UseCase { .. })
            && action.params.iter().any(|p| p.role.is_written())
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

    /// The `NoWitness` errors and `Deadlock`s of `e`'s state models on its cone, each deadlock
    /// with its trace, and what a capped cone leaves undecided; and into `runs`, by this
    /// system's action, what the cone says about each action it keeps.
    fn entity_findings(
        &self,
        e: EntId,
        cone: &ConeRun,
        out: &mut TargetExploration,
        runs: &mut [RunsSoFar],
    ) {
        let run = &cone.run;
        debug_assert!(!run.checks_failed, "an entity cone runs to the end");
        let decl = self.entity(e);
        let local = run.system.local_entity(decl.key);
        let why = stopped_at(run);
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
                        value: AbstractValue::of(&run.system.var(w.var).domain, w.value).display(),
                    },
                    cone: Some(decl.id.clone()),
                    why: capped(),
                }),
            }
        }
        out.tally.state_models += models.len();
        for d in findings.deadlocks {
            match d.progress {
                Progress::Leaves => {}
                Progress::Stuck { at, mut diag } => {
                    if let Some(trace) = diag.trace_mut() {
                        *trace = run.trace_to(at);
                        self.annotate(&cone.cone, trace);
                    }
                    out.tally.deadlocks += 1;
                    out.verdicts.push(Diag {
                        diag: *diag,
                        entities: vec![decl.key],
                    });
                }
                Progress::NotDecided => out.undecided.push(Undecided {
                    subject: Subject::Deadlock {
                        entity: decl.id.clone(),
                        column: run.system.var_name(d.var),
                        value: run.system.value_text(d.var, d.value),
                    },
                    cone: Some(decl.id.clone()),
                    why: capped(),
                }),
            }
        }
        for a in findings.runs {
            let source = run.system.actions()[a.act.index()].source;
            let at = self
                .actions
                .iter()
                .position(|x| x.source == source)
                .expect("a cone action is an action of the system");
            let r = &mut runs[at];
            r.kept = true;
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
        for arm in findings.arms {
            let action = &run.system.actions()[arm.act.index()];
            let (source, name) = (action.source, &action.name);
            let at = self
                .actions
                .iter()
                .position(|x| x.source == source)
                .expect("a cone action is an action of the system");
            let arms = &mut runs[at].arms;
            let i = match arms.iter().position(|a| a.site == arm.site) {
                Some(i) => i,
                None => {
                    arms.push(ArmSoFar {
                        site: arm.site.clone(),
                        subject: match &arm.from {
                            Some(from) => Subject::Arm {
                                action: name.clone(),
                                column: arm.column.clone(),
                                from: from.clone(),
                                to: arm.to.clone(),
                            },
                            None => Subject::Choice {
                                action: name.clone(),
                                column: arm.column.clone(),
                                value: arm.to.clone(),
                            },
                        },
                        taken: false,
                        never: None,
                        capped: None,
                    });
                    arms.len() - 1
                }
            };
            match arm.taken {
                Taken::Yes => arms[i].taken = true,
                Taken::Never(diag) => {
                    arms[i].never.get_or_insert(*diag);
                }
                Taken::NotDecided => {
                    arms[i]
                        .capped
                        .get_or_insert_with(|| (decl.id.clone(), capped()));
                }
                Taken::Moot => {}
            }
        }
    }

    /// The diagnostic is a violation of this invariant or assert.
    pub(super) fn reports(&self, target: Target, d: &StateDiag) -> bool {
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

    /// What the target's own cone says about an invariant, assert, or `can_reach`: its
    /// violations, each trace marked when it creates more instances than the bound, and any
    /// cap that left the verdict of an invariant or an assert incomplete. A `can_reach` cone
    /// that stopped is undecided instead (`explore_targets_with`).
    fn verdict(&self, target: Target, cone: &ConeRun) -> Vec<Diag> {
        let run = &cone.run;
        debug_assert!(
            !matches!(target, Target::Entity(_)),
            "an entity target has no verdict"
        );
        let entities = self.target_entities(target);
        if let Target::Reach(i) = target {
            debug_assert!(run.complete, "a stopped can_reach cone is undecided");
            let name = &self.reach[i].name;
            let local = run
                .system
                .reach_properties()
                .iter()
                .position(|r| r.name == *name)
                .expect("the cone holds what the property reads");
            if let Some((at, mut diag)) = run.system.reach_failure(local, &run.graph()) {
                if let Some(trace) = diag.trace_mut() {
                    *trace = run.trace_to(at);
                }
                return vec![Diag { entities, diag }];
            }
            return Vec::new();
        }
        crate::shadow_assert!(
            self.check_name(target)
                .is_some_and(|name| run.checks().reads(name)),
            "the run of a check's cone evaluates the check"
        );
        let mut out: Vec<Diag> = run
            .diags
            .iter()
            .filter(|d| self.reports(target, &d.diag))
            .cloned()
            .map(|mut d| {
                if let Some(trace) = d.diag.trace_mut() {
                    self.annotate(&cone.cone, trace);
                }
                d
            })
            .collect();
        for d in run.diags.iter().filter(|d| {
            matches!(
                d.diag,
                StateDiag::PatternCapReached { .. }
                    | StateDiag::StateCapReached { .. }
                    | StateDiag::MemoryCapReached { .. }
                    | StateDiag::TimeBudgetReached { .. }
            )
        }) {
            out.push(Diag {
                entities: entities.clone(),
                diag: d.diag.clone(),
            });
        }
        out.extend(self.vacuity(target, run));
        out
    }

    /// The `VacuousRule` of an invariant or an after-assert on a run of its cone: the check
    /// holds there, the run is complete, and no reached state meets one of its premises.
    /// `run` explores the target's cone, reduced or not, at this system's bounds.
    pub fn vacuity(&self, target: Target, run: &impl StoredRun) -> Option<Diag> {
        if !run.whole() {
            return None;
        }
        let (name, label, site, unmet) = match target {
            // A `can_reach` is a witness: one that holds has the state it asks for.
            Target::Entity(_) | Target::Reach(_) => return None,
            Target::Invariant(i) => {
                let inv = &self.invariants[i];
                (
                    &inv.name,
                    &inv.label,
                    &inv.site,
                    self.unmet_of_rule(inv, run)?,
                )
            }
            Target::AfterAssert(i) => {
                let a = &self.after[i];
                (&a.name, &a.label, &a.site, Self::unmet_of_assert(a, run)?)
            }
        };
        (!unmet.is_empty()).then(|| Diag {
            entities: self.target_entities(target),
            diag: StateDiag::VacuousRule {
                name: name.clone(),
                label: label.clone(),
                unmet,
                one_instance: self.most_slots() == 1,
                site: site.clone(),
            },
        })
    }

    /// The premises of `inv` no state of `run` meets; `None` when the rule is violated there
    /// or the cone does not hold it. When no state has the rows the rule ranges over, that
    /// alone is reported.
    fn unmet_of_rule(&self, inv: &Invariant, run: &impl StoredRun) -> Option<Vec<UnmetPremise>> {
        crate::shadow_assert!(
            run.evaluated().reads(&inv.name),
            "the run evaluates the rule"
        );
        let violated = run.diagnostics().iter().any(
            |d| matches!(&d.diag, StateDiag::ConstraintViolated { name, .. } if *name == inv.name),
        );
        if violated {
            return None;
        }
        // The cone's copy of the rule reads the cone's vars.
        let sys = run.system();
        let local = sys.invariants.iter().find(|x| x.name == inv.name)?;
        let met = |pred: &Pred| {
            run.stored()
                .iter()
                .any(|s| sys.meets(&local.binders, pred, s))
        };
        let unmet = |premise: &Premise| UnmetPremise::Rows {
            rows: TransitionSystem::premise_rows(inv.binders.len(), &premise.pred)
                .iter()
                .zip(&inv.binders)
                .filter(|(asked, _)| **asked)
                .map(|(_, e)| self.entity(*e).id.clone())
                .collect(),
            text: premise.text.clone(),
        };
        // A condition the build could not express is `False` and has its own error.
        let built = |p: &&Premise| !p.pred.nodes().any(|n| matches!(n, Pred::False));
        Some(match met(&Pred::True) {
            false if !local.premises.is_empty() => vec![unmet(&Premise::rows())],
            _ => local
                .premises
                .iter()
                .filter(built)
                .filter(|p| !met(&p.pred))
                .map(unmet)
                .collect(),
        })
    }

    /// The premises of the after-assert `a` no step of `run` meets; `None` when the assert
    /// is violated there or the cone does not hold it. The assert says something once its use
    /// case takes its last step, into a state that has rows of the entities the step does not
    /// bind, when it reads any.
    fn unmet_of_assert(a: &AfterAssert, run: &impl StoredRun) -> Option<Vec<UnmetPremise>> {
        crate::shadow_assert!(
            run.evaluated().reads(&a.name),
            "the run evaluates the assert"
        );
        let violated = run.diagnostics().iter().any(
            |d| matches!(&d.diag, StateDiag::AfterAssertViolated { name, .. } if *name == a.name),
        );
        if violated {
            return None;
        }
        let sys = run.system();
        let local = sys.after.iter().find(|x| x.name == a.name)?;
        let steps = |step: &AfterStep| run.targets_of(step.act);
        let ran: Vec<(&AfterStep, Vec<&StateSlice>)> = local
            .steps
            .iter()
            .map(|step| (step, steps(step)))
            .filter(|(_, ends)| !ends.is_empty())
            .collect();
        let usecase = a.anchor.clone();
        if ran.is_empty() {
            return Some(vec![UnmetPremise::NeverRuns { usecase }]);
        }
        let with_rows = ran.iter().any(|(step, ends)| {
            ends.iter()
                .any(|s| sys.meets(&step.binders, &Pred::True, s))
        });
        Some(match with_rows {
            true => Vec::new(),
            false => {
                let rows = ran[0].0.binders.iter();
                let rows = rows.map(|e| sys.entity(*e).id.clone()).collect();
                vec![UnmetPremise::NoRowsAfter { usecase, rows }]
            }
        })
    }

    /// The `VacuousRule` of each temporal property, each on its own cone explored under
    /// `caps`: no reachable state has the rows the property ranges over, or the trigger of
    /// its `leads_to` holds for none. A cone a cap stops reports nothing. BFS evaluates no
    /// temporal formula, so a property that is violated may be reported too.
    pub fn vacuous_properties(&self, caps: Caps) -> Vec<Diag> {
        let all = (0..self.properties.len()).collect();
        let found = in_parallel(all, jobs(), |i| self.vacuous_property(i, caps));
        found.into_iter().flatten().collect()
    }

    /// The `VacuousRule` of property `i` on its own cone, explored under `caps`.
    pub fn vacuous_property(&self, i: usize, caps: Caps) -> Option<Diag> {
        let cone = self.property_cone(i);
        let run = self.restrict_to(&cone.vars, &cone.keep).run(caps);
        self.property_vacuity(i, &run)
    }

    /// The `VacuousRule` of property `i` on a run of its cone, reduced or not.
    pub fn property_vacuity(&self, i: usize, run: &impl StoredRun) -> Option<Diag> {
        let property = &self.properties[i];
        let sys = run.system();
        let local = sys.properties.iter().find(|p| p.name == property.name)?;
        if !run.whole() {
            return None;
        }
        // A formula the build could not express is `False` and has its own error.
        let unbuilt = |pred: &Pred| pred.nodes().any(|n| matches!(n, Pred::False));
        if local.preds().any(unbuilt) {
            return None;
        }
        // The formula is read per binding of every binder to a present row, as TLC reads it:
        // a trigger that holds only while another binder has no row never fires.
        let met = |pred: &Pred| {
            let absent = Pred::Not(Box::new(pred.clone()));
            run.stored().iter().any(|s| {
                sys.find_violation(&local.binders, &absent, s, &[])
                    .is_some()
            })
        };
        // Every form speaks of the rows its `.along` links, and beyond that of the states
        // where its condition holds: the trigger of a `leads_to`, what a `stable` keeps, the
        // `.when` of an `unchanged`, and what a `precedes` asks a predecessor for.
        let linked = |pred: &Pred| Pred::And(vec![local.link.clone(), pred.clone()]);
        let asks = match &local.formula {
            Ltl::Always(_) | Ltl::Eventually(_) => None,
            Ltl::LeadsTo(trigger, _) => Some(("leads_to trigger", trigger)),
            Ltl::Stable(kept) => Some(("stable condition", kept)),
            Ltl::Unchanged { when, .. } => Some(("unchanged `.when`", when)),
            Ltl::Precedes(_, later) => Some(("precedes second condition", later)),
        };
        let unmet = match asks {
            _ if !met(&local.link) => UnmetPremise::Rows {
                rows: property
                    .binders
                    .iter()
                    .map(|e| self.entity(*e).id.clone())
                    .collect(),
                text: String::new(),
            },
            Some((what, pred)) if !met(&linked(pred)) => UnmetPremise::Trigger {
                what: what.to_string(),
            },
            _ => return None,
        };
        Some(Diag {
            entities: property
                .binders
                .iter()
                .map(|e| self.entity(*e).key)
                .collect(),
            diag: StateDiag::VacuousRule {
                name: property.name.clone(),
                label: None,
                unmet: vec![unmet],
                one_instance: self.most_slots() == 1,
                site: property.site.clone(),
            },
        })
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

    /// Constraint and assert verdicts, `NoWitness` errors, `Deadlock`s, and `NeverRuns`
    /// warnings, each from its own cone and each once.
    pub fn verdicts(&self) -> &[Diag] {
        &self.verdicts
    }

    /// The states of state models, the actions, and the `can_reach` properties no cone decided.
    pub fn undecided(&self) -> &[Undecided] {
        &self.undecided
    }

    /// What the entity cones decided, counted.
    pub fn tally(&self) -> Tally {
        self.tally
    }

    /// A run whose `--buc` drops use cases rules neither a state, an action, nor a way out of
    /// a state out: its `NoWitness` errors, `Deadlock`s, and `NeverRuns` warnings become
    /// undecided.
    pub fn defer_progress(&mut self) {
        let mut kept = Vec::new();
        for d in std::mem::take(&mut self.verdicts) {
            // A use case the scope drops may reach the state that meets the premise.
            if matches!(d.diag, StateDiag::VacuousRule { .. }) {
                continue;
            }
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
                // A left-out use case may enable the guard.
                StateDiag::ArmNeverTaken {
                    action,
                    column,
                    from,
                    to,
                    ..
                } => Subject::Arm {
                    action: action.clone(),
                    column: column.clone(),
                    from: from.clone(),
                    to: to.clone(),
                },
                StateDiag::ValueNeverChosen {
                    action,
                    column,
                    value,
                    ..
                } => Subject::Choice {
                    action: action.clone(),
                    column: column.clone(),
                    value: value.clone(),
                },
                StateDiag::Deadlock {
                    entity,
                    column,
                    value,
                    ..
                } => Subject::Deadlock {
                    entity: entity.clone(),
                    column: column.clone(),
                    value: value.clone(),
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
        self.tally.deadlocks = 0;
        self.tally.never_run = 0;
    }

    /// The views of the explored entity targets, sorted by entity id, each from the entity's
    /// cone.
    pub fn entity_views(
        &self,
        ts: &TransitionSystem,
        model: &SemanticModel,
        build_diags: &[Diag],
    ) -> Vec<EntityStateResult> {
        let mut out: Vec<EntityStateResult> = (0..ts.entities().len())
            .filter(|i| {
                self.targets
                    .iter()
                    .any(|(t, _)| *t == Target::Entity(EntId::at(*i)))
            })
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
                let mut view = entity_view(model, run, local, build_diags, verdicts);
                for trace in view.patterns.iter_mut().filter_map(|p| p.witness.as_mut()) {
                    ts.annotate(&cone.cone, trace);
                }
                view
            })
            .collect();
        out.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        out
    }
}
