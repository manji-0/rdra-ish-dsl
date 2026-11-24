//! The `perf-run` line of a `perf` build (`crate::perf`): what one BFS run cost and what its
//! store holds; and the guard counts of #300: the `perf-guards` line of a system, and the
//! `perf-evals` line of what the move generator left to `eval`.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::OnceLock;
use std::time::Instant;

use super::ComponentRun;
use crate::lifecycle::types::{At, Pred, Term, VarId};
use crate::lifecycle::TransitionSystem;
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
    // A run without an edge list (`Caps::stop_when_all_violated`) counts its moves only.
    let loops = match run.has_edges() {
        true => run
            .edges()
            .iter()
            .filter(|e| e.from == e.to)
            .count()
            .to_string(),
        false => "-".to_string(),
    };
    let mut out = format!(
        "perf-run pass={} label={} reduced={} early={early} complete={} graph={} vars={} cells={cells} width={} bits_exact={bits_exact:.1} bits_packed={bits_packed} log2n={:.1} states={n} edges={} loops={loops} bindings={} actions={} inv={invariants} after={after} kept={} peak={} wall_ms={wall_ms:.1} thread_cpu_ms={thread_ms:.1} proc_cpu_ms={process_ms:.1}",
        pass_name(),
        label_name(),
        ts.reduction().is_some(),
        run.complete,
        run.has_edges(),
        ts.var_ids().count(),
        run.states.width(),
        (n.max(1) as f64).log2(),
        run.moves(),
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
    for i in 0..run.states.len() {
        if let Some(e) = run.parent(i as u32) {
            let (a, b) = (&run.states[e.from as usize], &run.states[i]);
            delta += a.0.iter().zip(b.0.iter()).filter(|(x, y)| x != y).count() as u64;
            with_parent += 1;
        }
    }
    let fx = |h: u64, w: u64| (h.rotate_left(5) ^ w).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    let shash = (run.states.iter())
        .flat_map(|s| s.0.iter())
        .fold(0u64, |h, b| fx(h, u64::from(*b)));
    // A run without an edge list has no graph to hash.
    let (pairs, ehash, bhash) = match run.has_edges() {
        false => ("-".to_string(), "-".to_string(), "-".to_string()),
        true => {
            let edges = run.edges();
            let mut pairs: Vec<u64> = (edges.iter())
                .filter(|e| e.from != e.to)
                .map(|e| (u64::from(e.from) << 32) | u64::from(e.to))
                .collect();
            pairs.sort_unstable();
            pairs.dedup();
            let ehash =
                (edges.iter()).fold(0u64, |h, e| fx(fx(h, u64::from(e.from)), u64::from(e.to)));
            let bhash = edges.iter().fold(0u64, |h, e| {
                let (act, params) = &run.bindings[e.binding as usize];
                (params.iter()).fold(fx(h, act.index() as u64), |h, b| fx(h, u64::from(*b)))
            });
            (
                pairs.len().to_string(),
                format!("{ehash:016x}"),
                format!("{bhash:016x}"),
            )
        }
    };
    format!(
        " bits_reached={bits_reached} const_cols={const_cols} parent_delta={:.2} distinct_pairs={pairs} shash={shash:016x} ehash={ehash} bhash={bhash}",
        delta as f64 / with_parent.max(1) as f64,
    )
}

/// What a guard conjunct the move generator leaves to `eval` reads (#300), or a post-guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Two vars of one param's row in the pre-state, and literals.
    TwoVars,
    /// Three vars or more of one param's row.
    MoreVars,
    /// Two params' rows, one var or slot index of each (a link `row.owner = other` reads the
    /// owner var of one and the slot index of the other): what an index across params would
    /// answer.
    TwoParams,
    /// More vars of two params, or three params and more.
    MoreParams,
    /// A quantifier or a count over an entity's rows.
    Quantifier,
    /// Anything else: `now`, a bound row's slot or index, a successor's slot, no slot at all.
    Other,
    /// A guard read on the pre-state and the successor together.
    Post,
}

const KINDS: [(Kind, &str); 7] = [
    (Kind::TwoVars, "two_vars"),
    (Kind::MoreVars, "more_vars"),
    (Kind::TwoParams, "two_params"),
    (Kind::MoreParams, "more_params"),
    (Kind::Quantifier, "quantifier"),
    (Kind::Other, "other"),
    (Kind::Post, "post"),
];

static EXPANDED: AtomicU64 = AtomicU64::new(0);
static EVALS: [AtomicU64; KINDS.len()] = [const { AtomicU64::new(0) }; KINDS.len()];

impl Kind {
    /// The kind of a pre-state guard conjunct `one_row` does not take.
    pub(crate) fn of(pred: &Pred) -> Kind {
        let mut reads = BTreeSet::new();
        let (mut quantifier, mut other) = (false, false);
        walk(pred, &mut reads, &mut quantifier, &mut other);
        let params: BTreeSet<u8> = reads.iter().map(|(p, _)| *p).collect();
        match (quantifier, other, params.len(), reads.len()) {
            (true, _, _, _) => Kind::Quantifier,
            (false, true, _, _) | (false, false, 0, _) => Kind::Other,
            (false, false, 1, 2) => Kind::TwoVars,
            (false, false, 1, _) => Kind::MoreVars,
            (false, false, 2, 2) => Kind::TwoParams,
            (false, false, _, _) => Kind::MoreParams,
        }
    }
}

/// The `(param, var)` slots `pred` reads (`None` for a param's slot index), and whether it
/// holds a quantifier or count, or reads anything else than params' rows and literals.
fn walk(
    pred: &Pred,
    reads: &mut BTreeSet<(u8, Option<VarId>)>,
    quantifier: &mut bool,
    other: &mut bool,
) {
    let mut term = |t: &Term| match *t {
        Term::Slot {
            var,
            at: At::Param(p),
        } => {
            reads.insert((p, Some(var)));
        }
        Term::Index(At::Param(p)) => {
            reads.insert((p, None));
        }
        Term::Slot { .. } | Term::Index(_) | Term::Now => *other = true,
        Term::Lit(_) | Term::Int(_) => {}
    };
    match pred {
        Pred::True | Pred::False => {}
        Pred::Cmp(a, _, b) => {
            term(a);
            term(b);
        }
        Pred::In(a, values) => {
            term(a);
            values.iter().for_each(term);
        }
        Pred::Not(body) => walk(body, reads, quantifier, other),
        Pred::And(parts) | Pred::Or(parts) => {
            for p in parts {
                walk(p, reads, quantifier, other);
            }
        }
        Pred::Forall { .. } | Pred::Exists { .. } | Pred::Count { .. } => *quantifier = true,
    }
}

/// `RDRA_PERF_EVAL_TWICE`: each guard the move generator leaves to `eval` is evaluated a
/// second time, so the instructions a run gains are what those evaluations cost.
fn twice() -> bool {
    static TWICE: OnceLock<bool> = OnceLock::new();
    *TWICE.get_or_init(|| std::env::var_os("RDRA_PERF_EVAL_TWICE").is_some())
}

/// Counts one evaluation of a guard of `kind`, and returns what `eval` says.
pub(crate) fn evaluated(kind: Kind, mut eval: impl FnMut() -> bool) -> bool {
    EVALS[kind as usize].fetch_add(1, Relaxed);
    if twice() {
        std::hint::black_box(eval());
    }
    eval()
}

/// Counts one state whose moves are generated.
pub(crate) fn expanded() {
    EXPANDED.fetch_add(1, Relaxed);
}

/// Prints the `perf-evals` line: the states whose moves were generated, and the guards the
/// move generator left to `eval`, evaluated by kind.
pub fn evals_line() {
    if level() == 0 {
        return;
    }
    let counts: Vec<String> = (KINDS.iter())
        .map(|(kind, name)| format!("{name}={}", EVALS[*kind as usize].load(Relaxed)))
        .collect();
    eprintln!(
        "perf-evals expanded={} {}",
        EXPANDED.load(Relaxed),
        counts.join(" ")
    );
}

impl TransitionSystem {
    /// Prints the `perf-guards` line of this system (#300): its pre-state guard conjuncts that
    /// #253 turns into byte tests, those it leaves to `eval` by kind, and its post-guards.
    pub fn perf_guards_line(&self) {
        if level() == 0 {
            return;
        }
        let (mut conjuncts, mut tests, mut post) = (0, 0, 0);
        let mut left = [0usize; KINDS.len()];
        for action in &self.actions {
            for guard in &action.guards {
                if guard.reads_successor() {
                    post += 1;
                    continue;
                }
                for part in super::plan::conjuncts(&guard.pred) {
                    conjuncts += 1;
                    match super::plan::one_row(part) {
                        Some(_) => tests += 1,
                        None => left[Kind::of(part) as usize] += 1,
                    }
                }
            }
        }
        let left: Vec<String> = (KINDS.iter())
            .filter(|(kind, _)| *kind != Kind::Post)
            .map(|(kind, name)| format!("{name}={}", left[*kind as usize]))
            .collect();
        eprintln!(
            "perf-guards actions={} conjuncts={conjuncts} byte_tests={tests} {} post_guards={post}",
            self.actions.len(),
            left.join(" ")
        );
    }
}
