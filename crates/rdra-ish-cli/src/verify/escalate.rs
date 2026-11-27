//! `verify --prove`'s bound escalation (#421): a state rule the proof left unproved and BFS
//! passed at the run's instances is checked again on its own cone at one and two instances
//! more per entity, within the memory BFS has left. A counterexample there is a real one,
//! since BFS reaches only reachable states, so it is an error, printed with its trace and the
//! instances it needed. A run stopped by the memory limit or the state cap decides nothing.
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
    has_errors, label_prefix, Bounds, Caps, RunEnd, Scope, StateDiag, Target, TransitionSystem,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};

/// How many instances past the run's the escalation tries.
const STEPS: u8 = 2;

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
    let passed: Vec<(usize, usize)> = unproved
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
    for (i, at) in passed {
        let inv = &ts.invariants()[i];
        let mut said = Vec::new();
        for system in systems.iter().map_while(Option::as_ref) {
            let n = system.most_slots();
            let run =
                system.explore_targets_reduced(&[Target::Invariant(i)], caps.until_all_violated());
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

/// The instances of the safety cone that passed rule `i` of `ts`: a run to the end that did
/// not violate it, or a cone covered by one that passed. `None` when no cone decided it.
fn passed_at(
    ts: &TransitionSystem,
    jobs: &[Job],
    cones: &[Option<BfsCone>],
    i: usize,
) -> Option<usize> {
    let inv = &ts.invariants()[i];
    let at = jobs.iter().position(|job| {
        matches!(&job.kind, JobKind::Cone(system) if system.targets.contains(&Target::Invariant(i)))
    })?;
    let slots = |at: usize| match &jobs[at].kind {
        JobKind::Cone(system) => system.system.most_slots(),
        _ => unreachable!("a safety cone's job"),
    };
    match cones[at].as_ref()? {
        BfsCone::Ran {
            found,
            end: RunEnd::Complete,
            ..
        } => {
            let violated = found.iter().any(|d| {
                matches!(&d.diag, StateDiag::ConstraintViolated { name, site, .. }
                    if *name == inv.name && *site == inv.site)
            });
            (!violated).then(|| slots(at))
        }
        BfsCone::Covered(by) => cones[*by]
            .as_ref()
            .is_some_and(BfsCone::passed)
            .then(|| slots(*by)),
        BfsCone::Ran { .. } | BfsCone::Proved { .. } => None,
    }
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
