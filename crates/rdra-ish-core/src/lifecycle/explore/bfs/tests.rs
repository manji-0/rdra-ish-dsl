//! What the expanding thread reads off each move (`Probes`, #296 item 3.1) against a scan of
//! the chunk's moves.

use std::collections::HashMap;
use std::num::NonZeroU8;

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::tests::model_from;
use crate::lifecycle::{Bounds, Caps, Scope, TransitionSystem};

use super::super::arena::{hash, Seen};
use super::super::probe::{Numbering, Probe, Shared};
use super::super::symmetry::Group;
use super::super::tests::DESKS;
use super::{expand, Expander, Scratch, CHUNK};

/// Every move of every chunk of a run, plain and up to symmetry: a move is a `Loop` exactly
/// when its successor is its source, an `OrbitSelf` exactly when the canonical form is; any
/// other move carries its key packed as the store packs it and that packing's hash, and is a
/// `SameAs(k)` exactly when `k` is the first earlier move of the chunk with the same key, else
/// a `Miss`. Probed against a table of every state of the run, each such move is a `Hit` of
/// its key's number instead. Distinct bindings get distinct numbers. Each kind occurs.
#[test]
fn a_chunks_probes_are_what_a_scan_of_its_moves_finds() {
    let bounds = Bounds::uniform(NonZeroU8::new(3).expect("3 instances"), 5);
    let ts = TransitionSystem::build(&model_from(DESKS), bounds, &Scope::ALL).ts;
    let run = ts.clone().run(Caps::VERIFY);
    let states = &run.states;
    let numbering = Numbering::of(&ts);
    assert!(numbering.len() > 0);
    let group = Group::of(&ts);
    let mut seen = Seen::new(true);
    let mut number = HashMap::new();
    for i in 0..states.len() {
        let mut packed = Vec::new();
        states.pack(&states.at(i), &mut packed);
        seen.insert_hashed(states, i as u32, hash(&packed));
        number.insert(packed, i as u32);
    }
    let shared = Shared {
        table: seen.current().expect("a shared table"),
        rows: states.publish(&mut None),
    };
    let cases = [(None, false), (Some(&group), false), (None, true)];
    for (group, probed) in cases {
        let expander = Expander {
            ts: &ts,
            group,
            numbering: &numbering,
        };
        let mut scratch = Scratch::default();
        let mut kinds = HashMap::new();
        let mut numbers: HashMap<u32, (_, Box<[u8]>)> = HashMap::new();
        for start in (0..states.len()).step_by(CHUNK) {
            let range = start..(start + CHUNK).min(states.len());
            let shared = probed.then(|| shared.clone());
            let chunk = expand(expander, states, range.clone(), &mut scratch, shared);
            let (moves, probes) = (&chunk.moves, &chunk.probes);
            // Per key of the chunk so far: the first move that reached it.
            let mut first: HashMap<Vec<u8>, u32> = HashMap::new();
            let mut from = 0;
            for (at, &end) in range.zip(&chunk.ends) {
                let source = states.at(at);
                for i in from..end {
                    let (act, params, next) = moves.get(i);
                    let n = probes.number(i);
                    let named = numbers.entry(n).or_insert((act, params.into()));
                    assert_eq!((named.0, &named.1[..]), (act, params), "number {n}");
                    let probe = probes.probe(i);
                    *kinds
                        .entry(std::mem::discriminant(&probe))
                        .or_insert(0usize) += 1;
                    if next == &source.0[..] {
                        assert_eq!(probe, Probe::Loop, "move {i}");
                        continue;
                    }
                    let key = match group {
                        Some(_) => moves.canonical(i).0,
                        None => next,
                    };
                    if group.is_some() && key == &source.0[..] {
                        assert_eq!(probe, Probe::OrbitSelf, "move {i}");
                        continue;
                    }
                    let mut want = Vec::new();
                    states.pack(StateSlice::new(key), &mut want);
                    let (packed, h) = probes.packed(i);
                    assert_eq!(packed, &want[..], "move {i}");
                    assert_eq!(h, hash(packed), "move {i}");
                    let i = u32::try_from(i).expect("moves fit in u32");
                    if probed {
                        assert_eq!(probe, Probe::Hit(number[&want]), "move {i}");
                        continue;
                    }
                    match *first.entry(want).or_insert(i) {
                        k if k == i => assert_eq!(probe, Probe::Miss, "move {i}"),
                        k => assert_eq!(probe, Probe::SameAs(k), "move {i}"),
                    }
                }
                from = end;
            }
        }
        // Loops and misses or hits, and same-as moves; orbit self-edges under symmetry.
        let want_kinds = [3, 4, 2][usize::from(group.is_some()) + 2 * usize::from(probed)];
        assert_eq!(kinds.len(), want_kinds, "{kinds:?}");
    }
}
