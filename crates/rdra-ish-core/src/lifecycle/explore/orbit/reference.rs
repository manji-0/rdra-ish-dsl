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
use crate::lifecycle::explore::{Caps, Checks, ComponentRun, Threads};
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
    // Neither run gathers views (`run_orbits`), so the pattern cap stops neither.
    let mut plain_caps = Caps::new(caps.patterns, SHADOW_CAP).without_views();
    plain_caps.stop_when_all_violated = caps.stop_when_all_violated;
    let plain: ComponentRun = ts.clone().run_on(plain_caps, Checks::All, Threads::Shared);
    let stopped = |complete: bool, failed: bool| !complete && !failed;
    if stopped(plain.complete, plain.checks_failed)
        || stopped(orbits.complete(), orbits.run.checks_failed)
    {
        skipped(Twin::OrbitRun);
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
        compared(Twin::OrbitRun);
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
    compared(Twin::OrbitRun);
}
