//! A test build's record of the flat runs of `X` that `Classes::of_graph` partitions, for the
//! Lean checker of proofs/lean (`PartitionCert.lean`, `scripts/check-lean-certificates.sh`).
//! The ignored test `dump_partition_certificates` turns it on; every other build and test
//! records nothing.
//!
//! A record is one run of the extended sub-lifecycle `X` with no inner reduction and not up
//! to `H`, numbered as the run discovered its states, in lines of numbers:
//!
//! - `c NAME`: what the test was reducing;
//! - `n N K`: the states and the classes;
//! - `o`, `s`, `i`: per state, its `obs`, its `S` (every var of `X` outside the inputs) and
//!   its input bytes (the raw inputs and the bits), each value numbered as it first shows;
//! - `b`: per state, its class, the block of the certified partition numbered as `of_graph`
//!   numbers it;
//! - `r`: per class, its representative's state (`Classes::reps`);
//! - `z`: the run's initial states;
//! - `e`, `u`: the silent edges and the rises (`split_edges`), each as its two states.
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;

use super::super::extended::Extended;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::ComponentRun;
use crate::lifecycle::expr::State;
use crate::lifecycle::TransitionSystem;

/// What a dump holds: the records, and the runs of `X` it left out, by reason.
#[derive(Default)]
pub(crate) struct Dump {
    /// The name the next records take.
    pub(crate) name: String,
    pub(crate) records: Vec<String>,
    /// The runs left out, by reason: `nested` (a run of `X` under its own inner reduction,
    /// whose states are inner classes), `up to H` (a run whose states are orbits),
    /// `identity` (a partition `of_graph` turns down before it numbers classes, #101),
    /// `stopped` (a run of `X` a cap or a limit stopped, which `Classes::of` never
    /// partitions) and `unobserved` (an `X` with no `obs`, one class that is never
    /// explored).
    pub(crate) left_out: std::collections::BTreeMap<&'static str, usize>,
}

thread_local! {
    static DUMP: RefCell<Option<Dump>> = const { RefCell::new(None) };
}

/// Runs `f` with a dump on and returns what it recorded.
pub(crate) fn recording<T>(f: impl FnOnce() -> T) -> (T, Dump) {
    DUMP.with(|d| *d.borrow_mut() = Some(Dump::default()));
    let out = f();
    let dump = DUMP
        .with(|d| d.borrow_mut().take())
        .expect("the dump is on");
    (out, dump)
}

/// Names the records that follow.
pub(crate) fn name(name: &str) {
    DUMP.with(|d| {
        if let Some(d) = d.borrow_mut().as_mut() {
            d.name = name.to_string();
        }
    });
}

/// Counts a run of `X` left out for `why` (`Dump::left_out`).
pub(in crate::lifecycle::reduce) fn left_out(why: &'static str) {
    DUMP.with(|d| {
        if let Some(d) = d.borrow_mut().as_mut() {
            *d.left_out.entry(why).or_default() += 1;
        }
    });
}

/// Records the run `of_graph` partitioned, when a dump is on and the run is flat.
#[allow(clippy::too_many_arguments)]
pub(super) fn record(
    ext: &Extended,
    sub: &TransitionSystem,
    run: &ComponentRun,
    up_to_h: bool,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    class: &[u32],
    reps: &[u32],
) {
    DUMP.with(|d| {
        let mut d = d.borrow_mut();
        let Some(d) = d.as_mut() else { return };
        let n = run.states.len();
        let why = match () {
            _ if sub.reduction().is_some() => "nested",
            _ if up_to_h => "up to H",
            _ => "",
        };
        if !why.is_empty() {
            *d.left_out.entry(why).or_default() += 1;
            return;
        }
        let s_vars: VarSet = sub
            .var_ids()
            .filter(|v| !ext.inputs_local.contains(v))
            .collect();
        let mut out = String::new();
        let _ = writeln!(out, "c {}", d.name);
        let _ = writeln!(out, "n {n} {}", reps.len());
        for (tag, vars) in [
            ("o", &ext.obs_local),
            ("s", &s_vars),
            ("i", &ext.inputs_local),
        ] {
            let mut index: HashMap<State, u32> = HashMap::new();
            let values: Vec<u32> = run
                .states
                .iter()
                .map(|st| {
                    let len = u32::try_from(index.len()).expect("fewer values than states");
                    *index.entry(sub.project(&st, vars)).or_insert(len)
                })
                .collect();
            line(&mut out, tag, values.iter().copied());
        }
        line(&mut out, "b", class.iter().copied());
        line(&mut out, "r", reps.iter().copied());
        let initial = sub.initial_states();
        let at: Vec<u32> = (run.states.iter().enumerate())
            .filter(|(_, st)| initial.iter().any(|x| *st == *x))
            .map(|(i, _)| u32::try_from(i).expect("fewer states than u32"))
            .collect();
        line(&mut out, "z", at.into_iter());
        line(&mut out, "e", silent.iter().flat_map(|&(a, b)| [a, b]));
        line(&mut out, "u", rises.iter().flat_map(|&(a, b)| [a, b]));
        d.records.push(out);
    });
}

fn line(out: &mut String, tag: &str, values: impl Iterator<Item = u32>) {
    out.push_str(tag);
    for v in values {
        let _ = write!(out, " {v}");
    }
    out.push('\n');
}
