//! The `perf-run` line of a `perf` build (`crate::perf`): what one BFS run cost and what its
//! store holds.

use std::time::Instant;

use super::ComponentRun;
use crate::perf::{cpu_ns, label_name, level, pass_name};

/// When a run started: wall, thread CPU and process CPU.
pub(super) struct Clock(Instant, u64, u64);

impl Clock {
    pub(super) fn start() -> Clock {
        Clock(Instant::now(), cpu_ns(true), cpu_ns(false))
    }
}

/// Prints the line of `run`, which started at `clock`; `early` is `Caps::stop_when_all_violated`.
pub(super) fn line(run: &ComponentRun, clock: Clock, early: bool) {
    if level() == 0 {
        return;
    }
    let wall_ms = clock.0.elapsed().as_secs_f64() * 1e3;
    let thread_ms = cpu_ns(true).saturating_sub(clock.1) as f64 / 1e6;
    let process_ms = cpu_ns(false).saturating_sub(clock.2) as f64 / 1e6;
    let ts = &run.system;
    let (mut bits_exact, mut bits_packed, mut cells) = (0f64, 0u64, 0usize);
    for v in ts.var_ids() {
        let decl = ts.var(v);
        let slots = ts.slots_of(&decl.kind);
        let size = decl.domain.size(ts).max(1);
        bits_exact += slots as f64 * (size as f64).log2();
        bits_packed += slots as u64 * u64::from(usize::BITS - (size - 1).leading_zeros());
        cells += slots;
    }
    let invariants = (ts.invariants().iter())
        .filter(|i| run.checks().reads(&i.name))
        .count();
    let after = (ts.after_asserts().iter())
        .filter(|a| run.checks().reads(&a.name))
        .count();
    let n = run.states.len();
    let loops = run.edges.iter().filter(|e| e.from == e.to).count();
    let mut out = format!(
        "perf-run pass={} label={} reduced={} early={early} complete={} vars={} cells={cells} width={} bits_exact={bits_exact:.1} bits_packed={bits_packed} log2n={:.1} states={n} edges={} loops={loops} bindings={} actions={} inv={invariants} after={after} kept={} peak={} wall_ms={wall_ms:.1} thread_cpu_ms={thread_ms:.1} proc_cpu_ms={process_ms:.1}",
        pass_name(),
        label_name(),
        ts.reduction().is_some(),
        run.complete,
        ts.var_ids().count(),
        run.states.width(),
        (n.max(1) as f64).log2(),
        run.edges.len(),
        run.bindings.len(),
        ts.actions().len(),
        run.kept,
        run.peak,
    );
    if level() >= 2 {
        out.push_str(&costly(run));
    }
    eprintln!("{out}");
}

/// The columns that read every state or edge of `run`.
fn costly(run: &ComponentRun) -> String {
    let w = run.states.width();
    let mut seen = vec![[false; 256]; w];
    for s in run.states.iter() {
        for (c, b) in s.0.iter().enumerate() {
            seen[c][usize::from(*b)] = true;
        }
    }
    let distinct = |c: &[bool; 256]| c.iter().filter(|x| **x).count();
    let bits_reached: u32 = (seen.iter())
        .map(|c| usize::BITS - (distinct(c).max(1) - 1).leading_zeros())
        .sum();
    let const_cols = seen.iter().filter(|c| distinct(c) <= 1).count();
    let (mut delta, mut with_parent) = (0u64, 0u64);
    for (i, p) in run.parents.iter().enumerate() {
        if let Some(e) = p {
            let from = run.edges[*e as usize].from as usize;
            let (a, b) = (&run.states[from], &run.states[i]);
            delta += a.0.iter().zip(b.0.iter()).filter(|(x, y)| x != y).count() as u64;
            with_parent += 1;
        }
    }
    let mut pairs: Vec<u64> = (run.edges.iter())
        .filter(|e| e.from != e.to)
        .map(|e| (u64::from(e.from) << 32) | u64::from(e.to))
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    let fx = |h: u64, w: u64| (h.rotate_left(5) ^ w).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    let shash = (run.states.iter())
        .flat_map(|s| s.0.iter())
        .fold(0u64, |h, b| fx(h, u64::from(*b)));
    let ehash = (run.edges.iter()).fold(0u64, |h, e| fx(fx(h, u64::from(e.from)), u64::from(e.to)));
    let bhash = run.edges.iter().fold(0u64, |h, e| {
        let (act, params) = &run.bindings[e.binding as usize];
        (params.iter()).fold(fx(h, act.index() as u64), |h, b| fx(h, u64::from(*b)))
    });
    format!(
        " bits_reached={bits_reached} const_cols={const_cols} parent_delta={:.2} distinct_pairs={} shash={shash:016x} ehash={ehash:016x} bhash={bhash:016x}",
        delta as f64 / with_parent.max(1) as f64,
        pairs.len(),
    )
}
