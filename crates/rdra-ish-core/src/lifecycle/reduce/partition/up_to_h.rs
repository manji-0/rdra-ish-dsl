//! The twin of `Classes::of` on a run of S up to `H` (#212 phase 2): the classes of the plain
//! run of the same S, when it stays under `SHADOW_CAP` states. A `shadow` build holds the
//! classes up to `H` to them, through the map that sends a plain state's class to the class
//! of its canonical form:
//!
//! 1. the map is a bijection: the two partitions are the same sets of S states;
//! 2. it keeps which classes can loop, the initial states' classes, the rises, and the class
//!    moves by action (their params are read in other representatives, so they are left out).

use std::collections::{BTreeSet, HashMap};

use super::super::extended::Extended;
use super::{Classes, Classified, Ended};
use crate::lifecycle::explore::{Caps, Checks, OrbitRun};
use crate::lifecycle::TransitionSystem;

/// Counts a call the twin compared or skipped, in a `shadow` build.
fn count(compared: bool) {
    #[cfg(feature = "shadow")]
    match compared {
        true => crate::shadow::compared(crate::shadow::Twin::SUpToH),
        false => crate::shadow::skipped(crate::shadow::Twin::SUpToH),
    }
    #[cfg(not(feature = "shadow"))]
    let _ = compared;
}

/// The most states of the plain run of S the twin explores.
const SHADOW_CAP: usize = 200_000;

/// Holds `h`, the classes `Classes::of` read off `orbits`, a run of `sub` up to `H` whose
/// stored states have the classes `h_class`, to the classes of the plain run of `sub`.
pub(super) fn check(
    ext: &Extended,
    sub: &TransitionSystem,
    caps: Caps,
    orbits: &OrbitRun,
    h: &Classes,
    h_class: &[u32],
) {
    let plain_caps = Caps::new(caps.patterns, SHADOW_CAP).without_views();
    let run = sub.clone().run_checking(plain_caps, Checks::none());
    if !run.complete {
        count(false);
        return;
    }
    let (Classified::Classes(p), _, p_class) =
        Classes::of_graph(ext, sub, &run, Ended::Clean, None)
    else {
        // The plain quotient reduces nothing or is too large: there is nothing to compare.
        count(false);
        return;
    };
    let name = |what: &str| {
        format!(
            "shadow: Classes::of up to H ({} classes): {what}",
            h.reps.len()
        )
    };
    let stored: HashMap<&[u8], usize> = (orbits.states().iter())
        .enumerate()
        .map(|(i, s)| (&s.0[..], i))
        .collect();
    // (1) Each plain class goes to one class up to `H`, and no two to the same one.
    let mut to_h: Vec<Option<u32>> = vec![None; p.reps.len()];
    let key_vars: crate::lifecycle::cone::VarSet =
        ext.obs_local.union(&ext.inputs_local).copied().collect();
    for (i, s) in run.states.iter().enumerate() {
        let form = orbits.canonical(s);
        assert!(
            sub.project(s, &key_vars) == sub.project(&form, &key_vars),
            "{}",
            name("a state's form moves what the quotient reads")
        );
        let at = *stored
            .get(&form.0[..])
            .unwrap_or_else(|| panic!("{}", name("a plain state's form is not stored")));
        let (pc, hc) = (p_class[i] as usize, h_class[at]);
        match to_h[pc] {
            None => to_h[pc] = Some(hc),
            Some(was) => assert_eq!(was, hc, "{}", name("a plain class splits")),
        }
    }
    let to_h: Vec<u32> = to_h
        .into_iter()
        .map(|c| c.expect("every plain class holds a state"))
        .collect();
    assert_eq!(
        to_h.iter().collect::<BTreeSet<_>>().len(),
        h.reps.len(),
        "{}",
        name(&format!(
            "{} plain classes merge into {}",
            p.reps.len(),
            h.reps.len()
        ))
    );
    assert_eq!(
        p.reps.len(),
        h.reps.len(),
        "{}",
        name("the class counts differ")
    );
    let map = |c: u32| to_h[c as usize];
    // (2) What the reduced cone reads of the classes.
    for (c, &l) in p.loops.iter().enumerate() {
        assert_eq!(
            l,
            h.loops[map(c as u32) as usize],
            "{}",
            name("a class loops in one run only")
        );
    }
    for (s, c) in &p.init {
        assert_eq!(
            h.init.get(s).copied(),
            Some(map(*c)),
            "{}",
            name("an initial S state starts in another class")
        );
    }
    let p_rises: BTreeSet<(u32, &[u8], u32)> = (p.rises.iter())
        .map(|((c, i), d)| (map(*c), &i[..], map(*d)))
        .collect();
    let h_rises: BTreeSet<(u32, &[u8], u32)> =
        h.rises.iter().map(|((c, i), d)| (*c, &i[..], *d)).collect();
    assert!(p_rises == h_rises, "{}", name("the rises differ"));
    let moves =
        |m: &[Vec<super::ClassMove>], f: &dyn Fn(u32) -> u32| -> BTreeSet<(u32, usize, u32)> {
            m.iter()
                .enumerate()
                .flat_map(|(x, ms)| {
                    ms.iter()
                        .map(move |(act, _, y)| (f(x as u32), act.index(), f(*y)))
                })
                .collect()
        };
    assert!(
        moves(&p.moves, &map) == moves(&h.moves, &|c| c),
        "{}",
        name("the class moves differ")
    );
    count(true);
}
