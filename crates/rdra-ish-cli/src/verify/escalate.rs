//! `verify --prove`'s bound escalation (#421): a state rule the proof left unproved and BFS
//! passed at the run's instances is checked again on its own cone at one and two instances
//! more per entity. A counterexample there is a real one, since BFS reaches only reachable
//! states, so it is an error, printed with its trace and the instances it needed.
//!
//! Each run may hold `GROWTH` times the store the rule's cone held at the run's bounds, and
//! never more than BFS's pool has left (owner decision, #421: managed by memory, with no state
//! count). A run stopped there, by the memory limit, or by the state cap decides nothing.
//!
//! The runs go one at a time, after every other BFS run and the TLC cone runs, so what stops
//! one does not depend on the schedule. Their verdicts stay out of the safety cones', which
//! TLC is compared with at the run's bounds.

use std::collections::BTreeSet;
use std::num::NonZeroU8;

use super::bfs::BfsCone;
use super::jobs::{Job, JobKind};
use crate::state_findings::{render_violation, Finding, TraceSource};
use rdra_ish_core::lifecycle::{
    has_errors, label_prefix, Bounds, Caps, Cone, RunEnd, Scope, StateDiag, Target,
    TransitionSystem,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};

/// How many instances past the run's the escalation tries.
const STEPS: u8 = 2;

/// How many times the store its cone held at the run's bounds a rule's run at more instances
/// may hold. On the engine corpus's escalated cases one instance more multiplied the store by
/// 1.1 to 6.4 and two more by 1.4 to 10.1, so 10 lets all but one run finish
/// (`safety_count_guard_order` at 4 instances, 10.1). insurance-claim-ops with its lemma
/// dropped holds 34 MB at 2 instances; at 3 its run stops at 340 MB after about 40 s, where the
/// memory limit alone let it run past 8 GB.
const GROWTH: u64 = 10;

/// Escalates each rule of `unproved` that its safety cone of `jobs` passed (`cones`), prints
/// one `escalated:` line per rule, and an error per counterexample. Returns how many rules a
/// counterexample broke.
pub(super) fn escalate(
    (program, model): (&ResolvedProgram, &SemanticModel),
    (ts, bounds, scope): (&TransitionSystem, &Bounds, &Scope),
    (jobs, cones): (&[Job], &[Option<BfsCone>]),
    unproved: &BTreeSet<usize>,
    caps: Caps,
) -> usize {
    let passed: Vec<(usize, (usize, u64))> = unproved
        .iter()
        .filter_map(|&i| Some((i, passed_at(ts, jobs, cones, i)?)))
        .collect();
    if passed.is_empty() {
        return 0;
    }
    // One build per bound, shared by the rules; `None` once a bound does not build as `ts`.
    let systems: Vec<Option<TransitionSystem>> = (1..=STEPS)
        .map(|k| grown(bounds, k).and_then(|b| same_system(model, b, scope, ts)))
        .collect();
    let mut broken = 0;
    for (i, (at, peak)) in passed {
        let inv = &ts.invariants()[i];
        let mut said = Vec::new();
        for system in systems.iter().map_while(Option::as_ref) {
            let caps = match caps.memory {
                Some(pool) => caps.charged_to(pool.with_room(pool.left().min(GROWTH * peak))),
                None => caps,
            };
            // The cone of one rule stops once the rule fails (`explore_cone`); the caps stay
            // plain, since the sub-lifecycle a reduction runs keeps its whole graph.
            let run = system.explore_targets_reduced(&[Target::Invariant(i)], caps);
            let n = cone_slots(system, &run.cone_of(Target::Invariant(i)).cone);
            let found = run.verdicts().iter().find(|d| {
                matches!(&d.diag, StateDiag::ConstraintViolated { name, site, .. }
                    if *name == inv.name && *site == inv.site)
            });
            if let Some(d) = found {
                said.push(format!("counterexample at {n} instances"));
                let text = render_violation(program, model, &d.diag, TraceSource::BfsAt(n))
                    .expect("a violation renders");
                eprintln!("{}", Finding::new(&d.diag, &[], text));
                broken += 1;
                break;
            }
            let complete = run.cone_of(Target::Invariant(i)).run.complete;
            if !complete {
                said.push(format!("not decided at {n} instances"));
                break;
            }
            said.push(format!("ok at {n} instances"));
        }
        if said.is_empty() {
            continue;
        }
        println!(
            "escalated: {}{}: {} (BFS on its cone; ok at {at})",
            label_prefix(&inv.label),
            inv.name,
            said.join(", ")
        );
    }
    broken
}

/// The most instances of an entity in `cone` of `ts`.
fn cone_slots(ts: &TransitionSystem, cone: &Cone) -> usize {
    ts.restrict_to(&cone.vars, &cone.keep).most_slots()
}

/// The instances of rule `i`'s safety cone of `ts` (`cone_slots`) and the store its run held
/// (`BfsCone::Ran::peak`), when BFS passed it: a run to the end that did not violate it, or a
/// cone covered by one that passed. `None` when no cone decided it, or when the run that did
/// was up to symmetry, which measures no store to grow from.
fn passed_at(
    ts: &TransitionSystem,
    jobs: &[Job],
    cones: &[Option<BfsCone>],
    i: usize,
) -> Option<(usize, u64)> {
    let inv = &ts.invariants()[i];
    let at = jobs.iter().position(|job| {
        matches!(&job.kind, JobKind::Cone(system) if system.targets.contains(&Target::Invariant(i)))
    })?;
    let JobKind::Cone(system) = &jobs[at].kind else {
        unreachable!("a safety cone's job");
    };
    let slots = cone_slots(ts, &system.cone);
    let peak = match cones[at].as_ref()? {
        BfsCone::Ran {
            found,
            end: RunEnd::Complete,
            peak,
            ..
        } => {
            let violated = found.iter().any(|d| {
                matches!(&d.diag, StateDiag::ConstraintViolated { name, site, .. }
                    if *name == inv.name && *site == inv.site)
            });
            (!violated).then_some(*peak)?
        }
        BfsCone::Covered(by) => match cones[*by].as_ref()? {
            BfsCone::Ran {
                found,
                end: RunEnd::Complete,
                peak,
                ..
            } if found.is_empty() => *peak,
            _ => return None,
        },
        BfsCone::Ran { .. } | BfsCone::Proved { .. } => return None,
    };
    (peak > 0).then_some((slots, peak))
}

/// `bounds` with `k` more instances of every entity, the ones `--instances` names included;
/// `None` past 255.
fn grown(bounds: &Bounds, k: u8) -> Option<Bounds> {
    let add = |n: NonZeroU8| n.checked_add(k);
    let mut out = Bounds::uniform(add(bounds.instances)?, bounds.int_max);
    for (id, n) in &bounds.per_entity {
        out.per_entity.insert(id.clone(), add(*n)?);
    }
    Some(out)
}

/// `model` built at `bounds`, when it builds without errors into the system `ts` is at other
/// bounds: the same entities, vars, and rules, so `ts`'s indices are its own.
fn same_system(
    model: &SemanticModel,
    bounds: Bounds,
    scope: &Scope,
    ts: &TransitionSystem,
) -> Option<TransitionSystem> {
    let built = TransitionSystem::build(model, bounds, scope);
    let other = built.ts;
    let same = !has_errors(&built.diags)
        && other.entity_ids().count() == ts.entity_ids().count()
        && ts
            .entity_ids()
            .all(|e| other.entity(e).id == ts.entity(e).id)
        && other.var_ids().count() == ts.var_ids().count()
        && other.invariants().len() == ts.invariants().len()
        && (ts.invariants().iter().zip(other.invariants()))
            .all(|(a, b)| a.name == b.name && a.site == b.site);
    same.then_some(other)
}

#[cfg(test)]
mod tests {
    use super::grown;
    use rdra_ish_core::lifecycle::Bounds;
    use std::num::NonZeroU8;

    fn n(k: u8) -> NonZeroU8 {
        NonZeroU8::new(k).expect("not zero")
    }

    /// Every count grows, the ones `--instances` names too, and none past 255.
    #[test]
    fn grown_bounds_add_to_every_count_and_stop_at_255() {
        let mut bounds = Bounds::uniform(n(1), 5);
        bounds.per_entity.insert("Loan".to_string(), n(3));
        let up = grown(&bounds, 2).expect("fits");
        assert_eq!((up.instances, up.int_max), (n(3), 5));
        assert_eq!(up.per_entity["Loan"], n(5));
        bounds.per_entity.insert("Room".to_string(), n(254));
        assert!(grown(&bounds, 1).is_some());
        assert!(grown(&bounds, 2).is_none());
    }
}
