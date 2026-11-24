//! Which rules each guard keeps (`check --guards`). A guard here is a condition the model
//! applies to a use case: a `holds` condition that `applies_to` it, or the `.when`
//! conditions on one column's arms or values. For each guard the system is built again
//! without it, and every rule whose cone keeps the use case is explored again; a rule that
//! now fails depends on the guard. This is mutation testing of the model, with guard
//! deletion as the one mutation.

use super::cone::{Cone, Target};
use super::diag::StateDiag;
use super::explore::Caps;
use super::targets::UndecidedWhy;
use super::types::{ActId, ActionOrigin, Guard, GuardWhy, ModelRef, Site};
use super::TransitionSystem;
use crate::model::{ConditionKey, UseCaseKey};

/// One guard as the report names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardSubject {
    pub usecase: UseCaseKey,
    pub kind: GuardKind,
    /// The first guard entry it removes: `why` and `site` say what the model states.
    pub why: GuardWhy,
    pub site: Site,
}

/// What the report removes for one guard, across every action of the use case: its
/// transaction groups and its `<UC>_by_<Ev>` variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardKind {
    /// Every `holds` body of the condition that `applies_to` the use case.
    Condition(ConditionKey),
    /// The `.when` conditions of one column's arms or values, named by the line of the
    /// first guarded one: without them every arm or value is open.
    Arms(ModelRef),
}

impl GuardKind {
    fn of(guard: &Guard) -> Option<Self> {
        match guard.why {
            GuardWhy::Condition { cond, .. } => Some(GuardKind::Condition(cond)),
            GuardWhy::ArmCondition { .. } => Some(GuardKind::Arms(guard.site.fact.clone())),
            _ => None,
        }
    }
}

/// What the rules say once the guard is gone.
#[derive(Clone, Debug)]
pub struct GuardDeps {
    pub guard: GuardSubject,
    /// The rules whose cone keeps the use case, so the ones explored again, in target order.
    pub rerun: Vec<Target>,
    /// Distinct cones explored for them.
    pub cones: usize,
    /// The rules that fail without the guard: the guard protects them.
    pub protects: Vec<Target>,
    /// The rules whose cone stopped at a cap without a counterexample: not decided, so
    /// neither protected nor not.
    pub undecided: Vec<(Target, UndecidedWhy)>,
}

impl TransitionSystem {
    /// Each guard (`GuardKind`) of the system in action order, with the rules of `targets`
    /// (invariants and after-asserts; views are skipped) that fail once it is removed. Only
    /// the rules whose cone keeps an action the guard is on are explored again, one guard
    /// after another, so each mutant's cones run under `caps` alone. Every rule of `targets`
    /// must hold on this system: `check` asks only when it found no error.
    pub fn guard_dependencies(&self, targets: &[Target], caps: Caps) -> Vec<GuardDeps> {
        let rules: Vec<(Target, Cone)> = targets
            .iter()
            .filter(|t| matches!(t, Target::Invariant(_) | Target::AfterAssert(_)))
            .map(|&t| (t, self.target_cone(t)))
            .collect();
        let mut out = Vec::new();
        for subject in self.guard_subjects() {
            let acts: Vec<ActId> = self
                .action_ids()
                .filter(|&a| self.carries(a, &subject))
                .collect();
            let rerun: Vec<Target> = rules
                .iter()
                .filter(|(_, cone)| acts.iter().any(|&a| self.in_cone(a, cone)))
                .map(|(t, _)| *t)
                .collect();
            let mut deps = GuardDeps {
                guard: subject,
                rerun,
                cones: 0,
                protects: Vec::new(),
                undecided: Vec::new(),
            };
            if !deps.rerun.is_empty() {
                let mutant = self.without(&deps.guard);
                let run = mutant.explore_targets(&deps.rerun, caps);
                deps.cones = run.cones.len();
                for &(target, at) in &run.targets {
                    let cone = &run.cones[at].run;
                    debug_assert!(
                        self.check_name(target)
                            .is_some_and(|name| cone.checks().reads(name)),
                        "a rule's cone evaluates the rule"
                    );
                    if cone.diags.iter().any(|d| self.reports(target, &d.diag)) {
                        deps.protects.push(target);
                    } else if !cone.complete {
                        deps.undecided.push((target, cap_of(&cone.diags)));
                    }
                }
            }
            out.push(deps);
        }
        out
    }

    /// Each guard once, in the order of the actions and of their guards.
    fn guard_subjects(&self) -> Vec<GuardSubject> {
        let mut out: Vec<GuardSubject> = Vec::new();
        for action in &self.actions {
            let ActionOrigin::UseCase { uc, .. } = action.origin else {
                continue;
            };
            for g in &action.guards {
                let Some(kind) = GuardKind::of(g) else {
                    continue;
                };
                if !out.iter().any(|s| s.usecase == uc && s.kind == kind) {
                    out.push(GuardSubject {
                        usecase: uc,
                        kind,
                        why: g.why,
                        site: g.site.clone(),
                    });
                }
            }
        }
        out
    }

    fn is_subject(&self, act: ActId, subject: &GuardSubject, guard: &Guard) -> bool {
        matches!(self.actions[act.index()].origin,
            ActionOrigin::UseCase { uc, .. } if uc == subject.usecase)
            && GuardKind::of(guard).as_ref() == Some(&subject.kind)
    }

    /// The action carries the guard.
    fn carries(&self, act: ActId, subject: &GuardSubject) -> bool {
        self.actions[act.index()]
            .guards
            .iter()
            .any(|g| self.is_subject(act, subject, g))
    }

    /// `restrict_to(cone)` keeps the action: it writes a var of the cone, or the cone keeps it.
    fn in_cone(&self, act: ActId, cone: &Cone) -> bool {
        cone.keep.contains(&act)
            || !self
                .action_writes(&self.actions[act.index()])
                .is_disjoint(&cone.vars)
    }

    /// This system with the guard removed from every action of its use case.
    fn without(&self, subject: &GuardSubject) -> TransitionSystem {
        let mut mutant = self.clone();
        for (i, action) in mutant.actions.iter_mut().enumerate() {
            let act = ActId::at(i);
            action.guards.retain(|g| !self.is_subject(act, subject, g));
        }
        mutant
    }
}

/// The cap an incomplete run stopped at.
fn cap_of(diags: &[super::diag::Diag]) -> UndecidedWhy {
    diags
        .iter()
        .find_map(|d| match d.diag {
            StateDiag::StateCapReached { cap, .. } => Some(UndecidedWhy::StateCap(cap)),
            StateDiag::PatternCapReached { cap, .. } => Some(UndecidedWhy::PatternCap(cap)),
            StateDiag::MemoryCapReached { limit, raise, .. } => {
                Some(UndecidedWhy::MemoryCap(limit, raise))
            }
            _ => None,
        })
        .expect("an incomplete run stopped at a cap")
}
