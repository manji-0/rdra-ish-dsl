//! The twin of `run_orbits`: the plain run of the same system, when it stays under
//! `SHADOW_CAP` states. A `shadow` build holds a run up to symmetry to it:
//!
//! 1. the canonical forms of the plain run's states are the stored states;
//! 2. for every plain state `s`, the canonical forms of the successors of `s` are those of
//!    the successors of its stored form, which checks the bisimulation itself: a group that
//!    moves something a guard reads fails here, though the reached set says nothing;
//! 3. the two runs end alike and report the same violated checks.

use crate::lifecycle::narrow::{id32, id8};
use std::collections::BTreeSet;

use super::findings::StuckAt;
use super::OrbitRun;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::explore::symmetry::Canon;
use crate::lifecycle::explore::{Caps, ComponentRun, Threads};
use crate::lifecycle::expr::StateSlice;
use crate::shadow::{compared, skipped, Skip, Twin};

/// The most states of the plain run the twin explores.
const SHADOW_CAP: usize = 200_000;

/// Why a twin lets a call through: the run up to symmetry stopped, which leaves nothing to
/// compare, or only the plain run passed `SHADOW_CAP`.
fn why(orbits_stopped: bool) -> Skip {
    match orbits_stopped {
        true => Skip::Stopped,
        false => Skip::OverCap,
    }
}

/// The names of the violated invariants and after-asserts of a run's diagnostics.
fn violated<'a>(diags: impl Iterator<Item = &'a StateDiag>) -> BTreeSet<String> {
    diags
        .filter_map(|d| match d {
            StateDiag::ConstraintViolated { name, .. }
            | StateDiag::AfterAssertViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn check(orbits: &OrbitRun, caps: Caps) {
    let ts = orbits.system();
    // A reduced cone runs up to `G_T` (#212 phase 3): counted apart, so a run that reaches
    // only whole cones shows that it held no reduced one.
    let twin = match ts.reduction() {
        Some(_) => Twin::ReducedOrbitRun,
        None => Twin::OrbitRun,
    };
    // Neither run gathers views (`run_orbits`), so the pattern cap stops neither.
    let mut plain_caps = Caps::new(caps.patterns, SHADOW_CAP).without_views();
    plain_caps.stop_when_all_violated = caps.stop_when_all_violated;
    let plain: ComponentRun =
        ts.clone()
            .run_on(plain_caps, orbits.checks().clone(), Threads::Shared);
    let stopped = |complete: bool, failed: bool| !complete && !failed;
    if stopped(plain.complete, plain.checks_failed)
        || stopped(orbits.complete(), orbits.run.checks_failed)
    {
        skipped(
            twin,
            why(stopped(orbits.complete(), orbits.run.checks_failed)),
        );
        return;
    }
    let name = |what: &str| format!("shadow: run_orbits ({} classes): {what}", orbits.orbits());
    // (3) The same end and the same violated checks.
    assert_eq!(
        plain.end(),
        orbits.end(),
        "{}",
        name("the plain run ends otherwise")
    );
    let (a, b) = (
        violated(plain.diags.iter().map(|d| &d.diag)),
        violated(orbits.diags().iter().map(|d| &d.diag)),
    );
    assert_eq!(a, b, "{}", name("the violated checks differ"));
    if !plain.complete {
        compared(twin);
        return;
    }
    let g = &orbits.group;
    let mut canon = Canon::default();
    let mut form = |s: &[u8]| -> Vec<u8> {
        let mut out = vec![0u8; s.len()];
        canon.canonical(g, s, &mut out);
        out
    };
    // (1) The plain run's states, by canonical form, are the stored ones.
    let stored: BTreeSet<Vec<u8>> = orbits.states().iter().map(|s| s.0.to_vec()).collect();
    let forms: BTreeSet<Vec<u8>> = plain.states.iter().map(|s| form(&s.0)).collect();
    assert!(
        forms == stored,
        "{}",
        name(&format!(
            "the plain run's {} states have {} forms, the run stores {}; {} not stored, {} not reached",
            plain.states.len(),
            forms.len(),
            stored.len(),
            forms.difference(&stored).count(),
            stored.difference(&forms).count()
        ))
    );
    // (2) Each plain state steps into the classes its stored form steps into.
    for s in plain.states.iter() {
        let rep = form(&s.0);
        let mine: BTreeSet<Vec<u8>> = ts.moves(&s).map(|m| form(&m.next.0)).collect();
        let theirs: BTreeSet<Vec<u8>> = ts
            .moves(StateSlice::new(&rep))
            .map(|m| form(&m.next.0))
            .collect();
        assert!(
            mine == theirs,
            "{}",
            name(&format!(
                "state {:?} steps into {} classes, its form {rep:?} into {}: not a bisimulation",
                &s.0,
                mine.len(),
                theirs.len()
            ))
        );
    }
    compared(twin);
}

/// The twin of `OrbitRun::decide_liveness`: the plain run of the same system, when it stays
/// under `SHADOW_CAP` states, decided per binding by `liveness.rs`. A `shadow` build holds the
/// pairs to it:
///
/// 1. each edge's permutation takes the move at the edge's place among its source's moves to
///    the edge's target;
/// 2. for every plain state `s` and binding `b`, the pair of `s`'s stored form and `σ_s(b)`
///    fails the goal and can avoid it forever exactly when `(s, b)` does on the plain graph,
///    which checks that the binding and the fair instances follow the permutations;
/// 3. the two have a counterexample alike.
pub(super) fn check_liveness(
    product: &super::liveness::Product,
    search: &super::liveness::Search,
    i: usize,
    fairness: crate::lifecycle::lasso::Fairness,
) {
    use crate::fasthash::FastMap;
    use crate::lifecycle::explore::Checks;
    let orbits = product.run;
    let ts = orbits.system();
    let twin = Twin::OrbitLiveness;
    let plain: ComponentRun = ts.clone().run_on(
        Caps::new(usize::MAX, SHADOW_CAP).without_views(),
        Checks::All,
        Threads::Shared,
    );
    if !plain.complete || !orbits.complete() {
        skipped(twin, why(!orbits.complete()));
        return;
    }
    let g = &orbits.group;
    let perms = orbits.perms.as_ref().expect("an annotated run");
    let p = &ts.properties()[i];
    let name = |what: &str| {
        format!(
            "shadow: OrbitRun::decide_liveness({}, {fairness:?}) ({} classes): {what}",
            p.name,
            orbits.orbits()
        )
    };
    // (1) Each edge's permutation.
    let edges = orbits.run.edges();
    let mut out = vec![0u8; ts.width];
    for r in 0..id32(orbits.run.states.len()) {
        let moves: Vec<_> = ts.moves(&orbits.run.states.at(r as usize)).collect();
        let first = edges.range_of(r).start;
        for (k, e) in edges.range_of(r).zip(edges.from_state(r)) {
            g.apply(perms.get(perms.id(k)), &moves[k - first].next.0, &mut out);
            assert!(
                out[..] == orbits.run.states.at(e.to as usize).0[..],
                "{}",
                name(&format!(
                    "edge {k}'s permutation does not take its move to its target"
                ))
            );
        }
    }
    // (2) The pairs against the plain bindings.
    let reference = plain.liveness_reference(i, fairness);
    let stored: FastMap<Vec<u8>, u32> = (orbits.run.states.iter().enumerate())
        .map(|(r, s)| (s.0.to_vec(), id32(r)))
        .collect();
    let index: FastMap<&[u8], u32> = (product.bindings.iter().enumerate())
        .map(|(k, b)| (&b[..], id32(k)))
        .collect();
    let cause = match &p.formula {
        crate::lifecycle::types::Ltl::LeadsTo(x, _) => Some(x),
        _ => None,
    };
    let mut canon = Canon::default();
    let mut has = false;
    for (at, s) in plain.states.iter().enumerate() {
        canon.canonical(g, &s.0, &mut out);
        let r = stored[&out[..]];
        let sigma = canon.perm().to_vec();
        for (kb, b) in product.bindings.iter().enumerate() {
            let moved: Vec<u8> = (p.binders.iter().zip(b))
                .map(|(&e, &slot)| g.map_slot(&sigma, e, slot))
                .collect();
            let u = product.pair(r, index[&moved[..]]) as usize;
            let (in_n, avoid) = (&reference[kb].0, &reference[kb].1);
            assert_eq!(
                in_n[at],
                search.in_n[u],
                "{}",
                name(&format!(
                    "state {at} binding {b:?}: the goal differs on its pair"
                ))
            );
            assert_eq!(
                avoid[at],
                search.toward[u] != u32::MAX,
                "{}",
                name(&format!(
                    "state {at} binding {b:?}: the plain run can avoid the goal forever: {}, its pair: {}",
                    avoid[at],
                    !avoid[at]
                ))
            );
            has |= avoid[at]
                && ts.triggers(p, b, &s)
                && cause.is_none_or(|x| ts.holds_bound(x, &s, b));
        }
    }
    // (3) A counterexample alike.
    assert_eq!(
        has,
        search.start.is_some(),
        "{}",
        name("one run has a counterexample, the other none")
    );
    compared(twin);
}

/// The twin of `OrbitRun::state_model_findings`: the plain run of the same system, when it
/// stays under `SHADOW_CAP` states. A `shadow` build holds the findings on the pairs to it:
///
/// 1. the same witnesses, as found or not, the same runs, and the same `.when` arms taken;
/// 2. a deadlock at the same values;
/// 3. each deadlock the pairs found sits on a concrete state the plain run reaches, where the
///    plain pass finds the instance the deadlock names stuck, and the plain pass gives that
///    instance the same deadlock, its exits and what blocks each, but for the trace.
pub(super) fn check_state_models(
    orbits: &OrbitRun,
    e: crate::lifecycle::types::EntId,
    found: &crate::lifecycle::findings::StateModelFindings,
    stuck: &[StuckAt],
) {
    use crate::lifecycle::findings::{Progress, Witness};
    use crate::lifecycle::types::Value;
    let twin = Twin::OrbitStateModels;
    let ts = orbits.system();
    let plain: ComponentRun = ts.clone().run_on(
        Caps::new(usize::MAX, SHADOW_CAP).without_views(),
        orbits.checks().clone(),
        Threads::Shared,
    );
    if !plain.complete || !orbits.complete() {
        skipped(twin, why(!orbits.complete()));
        return;
    }
    let want = ts.state_model_findings(e, &plain.graph());
    let name = |what: &str| {
        format!(
            "shadow: state_model_findings of {} ({} classes): {what}",
            ts.entity(e).id,
            orbits.orbits()
        )
    };
    // (1) The witnesses, the runs and the arms.
    let kind = |w: &Witness| match w {
        Witness::At(_) => None,
        other => Some(other.clone()),
    };
    assert_eq!(
        found.witnesses.len(),
        want.witnesses.len(),
        "{}",
        name("witnesses")
    );
    for (a, b) in found.witnesses.iter().zip(&want.witnesses) {
        assert!(
            (a.var, a.value) == (b.var, b.value) && kind(&a.witness) == kind(&b.witness),
            "{}",
            name(&format!("the witness {a:?} is {b:?} on the plain run"))
        );
    }
    assert_eq!(found.runs, want.runs, "{}", name("the runs differ"));
    assert_eq!(found.arms, want.arms, "{}", name("the arms differ"));
    // (2) and (3) The deadlocks.
    assert_eq!(
        found.deadlocks.len(),
        want.deadlocks.len(),
        "{}",
        name("deadlocks")
    );
    let g = plain.graph();
    let adjacency = g.adjacency();
    let exists = ts.offset(ts.entity(e).exists);
    let untraced = |d: &StateDiag| {
        let mut d = d.clone();
        if let Some(t) = d.trace_mut() {
            *t = Default::default();
        }
        d
    };
    for ((a, b), at) in found.deadlocks.iter().zip(&want.deadlocks).zip(stuck) {
        assert_eq!(
            (a.var, a.value),
            (b.var, b.value),
            "{}",
            name("deadlock values")
        );
        match (&a.progress, &b.progress, at) {
            (Progress::Leaves, Progress::Leaves, _) => {}
            (Progress::Stuck { diag, .. }, Progress::Stuck { .. }, Some((at, k))) => {
                let (start, steps, tau) = orbits.lift_to(*at);
                let last = steps.last().map_or(&start, |s| &s.2);
                let named = usize::from(orbits.group.map_slot(&tau, e, id8(*k)));
                let p = (0..plain.states.len())
                    .find(|&p| plain.states.at(p) == *last)
                    .unwrap_or_else(|| panic!("{}", name("a deadlock's state is not reached")));
                let leave = ts.can_leave(e, a.var, named, &g, &adjacency);
                let s = plain.states.at(p);
                assert!(
                    s.0[exists + named] == Value::PRESENT.0
                        && s.0[ts.offset(a.var) + named] == a.value.0
                        && !leave[p],
                    "{}",
                    name(&format!(
                        "the deadlock at {:?} = {:?} names slot {named} of plain state {p}, which can leave",
                        a.var, a.value
                    ))
                );
                let held: Vec<_> = adjacency
                    .forward_from(id32(p), |_| true)
                    .into_iter()
                    .map(|i| (plain.states.at(i as usize), id8(named)))
                    .collect();
                let exits = ts.exits(e, a.var, a.value);
                let want = ts.deadlock(e, a.var, a.value, named, &exits, &held);
                assert_eq!(
                    untraced(diag),
                    untraced(&want),
                    "{}",
                    name("the deadlock differs from the plain pass's for its instance")
                );
            }
            (x, y, _) => panic!(
                "{}",
                name(&format!(
                    "the deadlock at {:?} is {x:?}, on the plain run {y:?}",
                    a.var
                ))
            ),
        }
    }
    compared(twin);
}
