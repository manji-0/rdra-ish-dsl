//! The twin of `run_orbits`: the plain run of the same system, when it stays under
//! `SHADOW_CAP` states. A `shadow` build holds a run up to symmetry to it:
//!
//! 1. the canonical forms of the plain run's states are the stored states;
//! 2. for every plain state `s`, the canonical forms of the successors of `s` are those of
//!    the successors of its stored form, which checks the bisimulation itself: a group that
//!    moves something a guard reads fails here, though the reached set says nothing;
//! 3. the two runs end alike and report the same violated checks.

use std::collections::BTreeSet;

use super::OrbitRun;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::explore::symmetry::Canon;
use crate::lifecycle::explore::{Caps, ComponentRun, Threads};
use crate::lifecycle::expr::StateSlice;
use crate::shadow::{compared, skipped, Twin};

/// The most states of the plain run the twin explores.
const SHADOW_CAP: usize = 200_000;

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
        skipped(twin);
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
        let mine: BTreeSet<Vec<u8>> = ts.moves(s).map(|m| form(&m.next.0)).collect();
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
        skipped(twin);
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
    for r in 0..orbits.run.states.len() as u32 {
        let moves: Vec<_> = ts.moves(&orbits.run.states[r as usize]).collect();
        let first = edges.range_of(r).start;
        for (k, e) in edges.range_of(r).zip(edges.from_state(r)) {
            g.apply(perms.get(perms.id(k)), &moves[k - first].next.0, &mut out);
            assert!(
                out[..] == orbits.run.states[e.to as usize].0[..],
                "{}",
                name(&format!(
                    "edge {k}'s permutation does not take its move to its target"
                ))
            );
        }
    }
    // (2) The pairs against the plain bindings.
    let reference = plain.liveness_reference(i, fairness);
    let stored: FastMap<&[u8], u32> = (orbits.run.states.iter().enumerate())
        .map(|(r, s)| (&s.0[..], r as u32))
        .collect();
    let index: FastMap<&[u8], u32> = (product.bindings.iter().enumerate())
        .map(|(k, b)| (&b[..], k as u32))
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
            has |=
                avoid[at] && ts.triggers(p, b, s) && cause.is_none_or(|x| ts.holds_bound(x, s, b));
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
