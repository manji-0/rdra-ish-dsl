//! The self-check of `check --code=run` (#439): traces that are wrong on purpose. Each mutant
//! is one trace of the real traces cut after one row, with that row changed so that the model
//! does not do what it says. A service that conforms fails each at that row; a run that passes
//! one did not run the harness on the traces it was given, swallowed its failure, or (for
//! [`MutantKind::AbsentRow`]) runs a call on a row it never looked up. The user's code is not
//! changed (docs/design/code-check.md, decision 1).
//!
//! Only the last row is changed, and only into something the model never does, so no mutant
//! is equivalent to the real traces: a conforming service never passes one.

use std::collections::{BTreeMap, HashMap};

/// How a mutant's last row differs from the row of the real traces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MutantKind {
    /// `X <kind> call` became `C <state> call`: a call the model refuses, expected to run.
    RunRefused,
    /// `C <state> call` became `X - call` (any kind): a call the model runs, expected to be refused.
    RefuseRun,
    /// `C <state> call` expects another state, one whose rows differ.
    WrongState,
    /// `C <state> call` names a row that does not exist and still expects the state: a
    /// service that looks the row up refuses it; one that ignores the id runs it.
    AbsentRow,
    /// `X Rejected call` that one condition rejects became `C <state> call`: the model with
    /// that guard removed runs it. One per condition.
    GuardRemoved,
}

impl MutantKind {
    pub const ALL: [MutantKind; 5] = [
        MutantKind::RunRefused,
        MutantKind::RefuseRun,
        MutantKind::WrongState,
        MutantKind::AbsentRow,
        MutantKind::GuardRemoved,
    ];

    /// The kind's name in the output.
    pub fn name(self) -> &'static str {
        match self {
            MutantKind::RunRefused => "refused-call-runs",
            MutantKind::RefuseRun => "call-refused",
            MutantKind::WrongState => "wrong-state",
            MutantKind::AbsentRow => "absent-row",
            MutantKind::GuardRemoved => "guard-removed",
        }
    }

    /// Why the traces give no mutant of this kind.
    fn none_because(self) -> &'static str {
        match self {
            MutantKind::RunRefused => "the traces try no refused call",
            MutantKind::RefuseRun | MutantKind::AbsentRow => "the traces make no such call",
            MutantKind::WrongState => "no two states of the traces differ",
            MutantKind::GuardRemoved => "the traces try no call a condition rejects",
        }
    }
}

/// One mutant: the traces text, and the line of the changed row in it (1-based).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutant {
    pub kind: MutantKind,
    pub text: String,
    pub line: usize,
    /// The row of the real traces, with its line, and what it became.
    pub was: (usize, String),
    pub now: String,
    /// For [`MutantKind::GuardRemoved`]: the condition removed.
    pub guard: Option<String>,
}

/// The mutants a self-check runs, and the kinds the traces give none of, with why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mutants {
    pub mutants: Vec<Mutant>,
    pub none: Vec<(MutantKind, &'static str)>,
}

#[cfg(any(test, feature = "shadow"))]
mod reference;

/// A row of a trace: its line in the traces (0-based), its text, and the state the trace is
/// in before it.
struct Row<'a> {
    line: usize,
    text: &'a str,
    at: &'a str,
}

/// A candidate row, kept as references: its text is built only for the rows picked. The
/// trace's first line, the row's line, and the state the new row names (`RunRefused`: the
/// state it is tried in; `WrongState`: the state drawn; `AbsentRow`: the state it reaches).
#[derive(Clone, Copy)]
struct Candidate<'a> {
    start: usize,
    line: usize,
    state: &'a str,
}

impl Candidate<'_> {
    /// The row it becomes, as [`reference::mutants`] spells it.
    fn now(&self, kind: MutantKind, row: &str, absent: &str) -> String {
        let toks: Vec<&str> = row.split_whitespace().collect();
        let call = toks.get(2..).unwrap_or_default();
        match kind {
            MutantKind::RefuseRun => format!("X - {}", call.join(" ")),
            MutantKind::AbsentRow => {
                let (action, rest) = (call[0], &call[2..]);
                let rest: String = rest.iter().map(|t| format!(" {t}")).collect();
                format!("C {} {action} {absent}{rest}", self.state)
            }
            MutantKind::RunRefused | MutantKind::WrongState | MutantKind::GuardRemoved => {
                format!("C {} {}", self.state, call.join(" "))
            }
        }
    }
}

/// The states of the traces' header in id order, grouped by the rows they hold, so that a
/// state whose rows differ from a given one is drawn without looking at the others.
struct StateClasses<'a> {
    ids: Vec<&'a str>,
    index: HashMap<&'a str, usize>,
    /// Per state, its class: the states that hold the same rows.
    class: Vec<usize>,
    /// Per class, the positions of its states in `ids`, ascending.
    members: Vec<Vec<usize>>,
}

impl<'a> StateClasses<'a> {
    fn new(states: &BTreeMap<&'a str, Vec<&'a str>>) -> Self {
        let mut by_rows: HashMap<&[&str], usize> = HashMap::new();
        let mut out = StateClasses {
            ids: Vec::with_capacity(states.len()),
            index: HashMap::with_capacity(states.len()),
            class: Vec::with_capacity(states.len()),
            members: Vec::new(),
        };
        for (i, (id, rows)) in states.iter().enumerate() {
            let next = out.members.len();
            let c = *by_rows.entry(rows.as_slice()).or_insert(next);
            if c == next {
                out.members.push(Vec::new());
            }
            out.members[c].push(i);
            out.ids.push(id);
            out.index.insert(id, i);
            out.class.push(c);
        }
        out
    }

    /// The states whose rows differ from `to`'s, in id order, as a count and an accessor:
    /// every state when `to` is no state of the header.
    fn others(&self, to: &str) -> (usize, &[usize]) {
        match self.index.get(to) {
            Some(&i) => {
                let members = &self.members[self.class[i]];
                (self.ids.len() - members.len(), members)
            }
            None => (self.ids.len(), &[]),
        }
    }

    /// The `k`-th state (0-based, in id order) not among `skip`, the ascending positions of a
    /// class.
    fn nth_outside(&self, skip: &[usize], k: usize) -> &'a str {
        // The least `x` with `k + 1` positions outside `skip` in `0..=x`.
        let (mut lo, mut hi) = (k, k + skip.len());
        while lo < hi {
            let x = (lo + hi) / 2;
            let outside = x + 1 - skip.partition_point(|&m| m <= x);
            if outside > k {
                hi = x;
            } else {
                lo = x + 1;
            }
        }
        self.ids[lo]
    }
}

/// Up to `per_kind` mutants of each kind from `traces`, picked by `seed`: the same traces
/// and seed give the same mutants. `one_row` names the actions whose call takes one existing
/// row, first; `absent` is an id no row has. `guards` are the refused calls a condition
/// rejects ([`super::Traces::guards`]): with `per_kind` above 0, one guard-removed mutant per
/// condition.
///
/// It reads each row once and builds the text only of the rows it picks, so it takes linear
/// time in the traces (#439: building every candidate's text, and listing the states that
/// differ for every row, took minutes at the default cap). It picks what
/// [`reference::mutants`] picks, which a `shadow` build checks on every call.
pub fn mutants(
    traces: &str,
    one_row: &[String],
    absent: &str,
    guards: &[super::Guarded],
    per_kind: usize,
    seed: u64,
) -> Mutants {
    let out = fast(traces, one_row, absent, guards, per_kind, seed);
    #[cfg(feature = "shadow")]
    {
        let twin = reference::mutants(traces, one_row, absent, guards, per_kind, seed);
        assert!(
            out == twin,
            "shadow: mutants (seed {seed}, {per_kind} per kind) picks other mutants than its reference"
        );
    }
    out
}

fn fast(
    traces: &str,
    one_row: &[String],
    absent: &str,
    guards: &[super::Guarded],
    per_kind: usize,
    seed: u64,
) -> Mutants {
    let lines: Vec<&str> = traces.lines().collect();
    let first_trace = lines
        .iter()
        .position(|l| l.starts_with("T "))
        .unwrap_or(lines.len());
    let header = &lines[..first_trace];

    // The rows each state holds, to tell two states apart.
    let mut states: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut defining = None;
    for line in header {
        if let Some(id) = line.strip_prefix("S ") {
            states.insert(id.trim(), Vec::new());
            defining = Some(id.trim());
        } else if let (Some(row), Some(id)) = (line.strip_prefix("A "), defining) {
            states.entry(id).or_default().push(row);
        }
    }
    let classes = StateClasses::new(&states);

    let mut candidates: BTreeMap<MutantKind, Vec<Candidate>> = BTreeMap::new();
    let mut start = first_trace;
    let mut at = "";
    let mut rows: Vec<(usize, Row)> = Vec::new();
    for (n, line) in lines.iter().enumerate().skip(first_trace) {
        let mut toks = line.split_whitespace();
        let kind = toks.next().unwrap_or_default();
        if kind == "T" {
            start = n;
            at = toks.next().unwrap_or_default();
            continue;
        }
        rows.push((
            start,
            Row {
                line: n,
                text: line,
                at,
            },
        ));
        if matches!(kind, "C" | "I") {
            at = toks.next().unwrap_or_default();
        }
    }
    let mut rng = SplitMix(seed);
    // The conditions by the refused row they reject, in the state it is tried in.
    let mut rejecting: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    for g in guards {
        rejecting
            .entry((g.state.as_str(), g.row.as_str()))
            .or_default()
            .push(&g.condition);
    }
    // Per condition, the rows it rejects: the row of the traces, in the state it names.
    let mut guarded: BTreeMap<&str, Vec<Candidate>> = BTreeMap::new();
    if !rejecting.is_empty() {
        for (start, row) in &rows {
            for condition in rejecting.get(&(row.at, row.text)).into_iter().flatten() {
                guarded.entry(condition).or_default().push(Candidate {
                    start: *start,
                    line: row.line,
                    state: row.at,
                });
            }
        }
    }
    for (start, row) in &rows {
        let mut toks = row.text.split_whitespace();
        let (start, line) = (*start, row.line);
        match toks.next() {
            Some("X") if toks.next().is_some() => candidates
                .entry(MutantKind::RunRefused)
                .or_default()
                .push(Candidate {
                    start,
                    line,
                    state: row.at,
                }),
            Some("C") => {
                let Some(to) = toks.next() else { continue };
                let mut push = |k, state| {
                    candidates
                        .entry(k)
                        .or_default()
                        .push(Candidate { start, line, state });
                };
                push(MutantKind::RefuseRun, "");
                let (count, skip) = classes.others(to);
                if count > 0 {
                    let pick = classes.nth_outside(skip, rng.below(count));
                    push(MutantKind::WrongState, pick);
                }
                if let (Some(action), Some(_id)) = (toks.next(), toks.next()) {
                    if one_row.iter().any(|a| a == action) {
                        push(MutantKind::AbsentRow, to);
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = Mutants::default();
    let cut = |c: Candidate, kind, guard: Option<String>| {
        let now = c.now(kind, lines[c.line], absent);
        let mut text = String::new();
        for l in header.iter().chain(&lines[c.start..c.line]) {
            text.push_str(l);
            text.push('\n');
        }
        text.push_str(&now);
        text.push('\n');
        Mutant {
            kind,
            line: header.len() + (c.line - c.start) + 1,
            text,
            was: (c.line + 1, lines[c.line].to_string()),
            now,
            guard,
        }
    };
    for kind in MutantKind::ALL {
        if kind == MutantKind::GuardRemoved {
            if guarded.is_empty() {
                out.none.push((kind, kind.none_because()));
            } else if per_kind > 0 {
                for (condition, pool) in &guarded {
                    let c = pool[rng.below(pool.len())];
                    out.mutants.push(cut(c, kind, Some(condition.to_string())));
                }
            }
            continue;
        }
        let mut pool = candidates.remove(&kind).unwrap_or_default();
        if pool.is_empty() {
            out.none.push((kind, kind.none_because()));
            continue;
        }
        for _ in 0..per_kind.min(pool.len()) {
            let c = pool.swap_remove(rng.below(pool.len()));
            out.mutants.push(cut(c, kind, None));
        }
    }
    out
}

/// splitmix64: a small generator whose sequence depends on its seed alone.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACES: &str = "# rdra-ish conformance traces 4
# rdra-ish codegen fingerprint: model=0 code=0
S 0
A Customer 1
S 1
A Customer 1
A Order 1 status=pending
S 2
A Customer 1
A Order 1 status=paid
S 3
A Customer 1
A Order 1 status=paid
T 0
R Customer 1
G
C 1 Place 1 1
X Rejected Cancel 1
C 2 Pay 1 x
T 0
R Customer 1
G
C 1 Place 1 1
";

    fn one_row() -> Vec<String> {
        vec!["Pay".to_string(), "Cancel".to_string()]
    }

    fn guards() -> Vec<super::super::Guarded> {
        vec![super::super::Guarded {
            condition: "Big".to_string(),
            state: "1".to_string(),
            row: "X Rejected Cancel 1".to_string(),
        }]
    }

    #[test]
    fn each_kind_changes_one_row_and_cuts_the_trace_after_it() {
        let m = mutants(TRACES, &one_row(), "2", &guards(), 9, 0);
        assert!(m.none.is_empty(), "{:?}", m.none);
        for mutant in &m.mutants {
            let lines: Vec<&str> = mutant.text.lines().collect();
            assert_eq!(lines[mutant.line - 1], mutant.now, "{mutant:?}");
            assert_eq!(lines.len(), mutant.line, "the changed row is the last");
            assert_eq!(lines[0], "# rdra-ish conformance traces 4");
            assert_eq!(TRACES.lines().nth(mutant.was.0 - 1), Some(&*mutant.was.1));
        }
        let of = |k| -> Vec<&str> {
            m.mutants
                .iter()
                .filter(|x| x.kind == k)
                .map(|x| x.now.as_str())
                .collect()
        };
        // The X row runs from the state it is tried in.
        assert_eq!(of(MutantKind::RunRefused), ["C 1 Cancel 1"]);
        // One per condition, naming it.
        assert_eq!(of(MutantKind::GuardRemoved), ["C 1 Cancel 1"]);
        assert!(m
            .mutants
            .iter()
            .all(|x| (x.kind == MutantKind::GuardRemoved) == (x.guard.as_deref() == Some("Big"))));
        let mut refused = of(MutantKind::RefuseRun);
        refused.sort();
        assert_eq!(refused, ["X - Pay 1 x", "X - Place 1 1", "X - Place 1 1"]);
        // Only a call of a one-row action names an absent row; Place creates its row.
        assert_eq!(of(MutantKind::AbsentRow), ["C 2 Pay 2 x"]);
        // Never a state with the same rows: 2 and 3 hold the same.
        let wrong = of(MutantKind::WrongState);
        assert_eq!(wrong.len(), 3, "{wrong:?}");
        for now in wrong {
            let to = now.split_whitespace().nth(1).unwrap();
            if now.contains("Pay") {
                assert!(to == "0" || to == "1", "{now}");
            } else {
                assert!(to != "1", "{now}");
            }
        }
        let x = m
            .mutants
            .iter()
            .find(|x| x.kind == MutantKind::RunRefused)
            .unwrap();
        assert!(
            x.text
                .ends_with("T 0\nR Customer 1\nG\nC 1 Place 1 1\nC 1 Cancel 1\n"),
            "{}",
            x.text
        );
        assert_eq!(x.was, (18, "X Rejected Cancel 1".to_string()));
    }

    #[test]
    fn the_seed_picks_the_mutants() {
        let a = mutants(TRACES, &one_row(), "2", &[], 1, 7);
        let b = mutants(TRACES, &one_row(), "2", &[], 1, 7);
        assert_eq!(a.mutants, b.mutants);
        assert_eq!(a.mutants.len(), 4);
        let picks = |seed| {
            mutants(TRACES, &one_row(), "2", &[], 1, seed)
                .mutants
                .into_iter()
                .map(|m| m.now)
                .collect::<Vec<_>>()
        };
        assert!(
            (0..16).any(|s| picks(s) != picks(0)),
            "some seed picks other rows"
        );
    }

    /// Traces of `seed`'s making: states of which some hold the same rows, rows that name a
    /// state the header lacks, every kind of row, and guards on some X rows.
    fn synthetic(seed: u64) -> (String, Vec<super::super::Guarded>) {
        let mut rng = SplitMix(seed);
        let mut text = String::from("# rdra-ish conformance traces 4\n");
        let states = 2 + rng.below(40);
        for s in 0..states {
            text.push_str(&format!("S {s}\n"));
            // Few distinct row sets, so classes hold several states.
            let rows = rng.below(4);
            for r in 0..rows {
                text.push_str(&format!("A Order {r} status=s{}\n", rng.below(3)));
            }
        }
        let mut guards = Vec::new();
        for t in 0..1 + rng.below(30) {
            let mut at = rng.below(states + 2).to_string();
            text.push_str(&format!("T {at}\nR Order 1\nG\n"));
            for _ in 0..rng.below(12) {
                let action = ["Pay", "Ship", "Place"][rng.below(3)];
                let id = 1 + rng.below(3);
                match rng.below(6) {
                    0 => {
                        let row = format!("X Rejected {action} {id}");
                        if rng.below(2) == 0 {
                            guards.push(super::super::Guarded {
                                condition: format!("G{}", rng.below(3)),
                                state: at.clone(),
                                row: row.clone(),
                            });
                        }
                        text.push_str(&format!("{row}\n"));
                    }
                    1 => text.push_str(&format!("X NotEnabled {action} {id}\n")),
                    2 => text.push_str(&format!("I {} {action} {id} {t}\n", rng.below(states))),
                    3 => text.push_str("C\nX\n"),
                    _ => {
                        // A state past the header's now and then.
                        let to = rng.below(states + 2).to_string();
                        let extra = if rng.below(2) == 0 { " x y" } else { "" };
                        text.push_str(&format!("C {to} {action} {id}{extra}\n"));
                        at = to;
                    }
                }
            }
        }
        (text, guards)
    }

    #[test]
    fn the_fast_picks_are_the_reference_picks() {
        let one = one_row();
        for seed in 0..300 {
            let (traces, guards) = synthetic(seed);
            for per_kind in [0, 1, 3] {
                for pick in [0, seed, u64::MAX - seed] {
                    let fast = fast(&traces, &one, "9", &guards, per_kind, pick);
                    let twin = reference::mutants(&traces, &one, "9", &guards, per_kind, pick);
                    assert_eq!(
                        fast, twin,
                        "traces {seed}, {per_kind} per kind, seed {pick}:\n{traces}"
                    );
                }
            }
        }
        let fast = fast(TRACES, &one, "2", &guards(), 9, 0);
        assert_eq!(fast, reference::mutants(TRACES, &one, "2", &guards(), 9, 0));
    }

    /// Times both on the traces file `RDRA_MUTANT_TRACES` names, at one mutant per kind:
    /// `cargo test --release -p rdra-ish-emit --lib mutants_timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mutants_timing() {
        let Ok(path) = std::env::var("RDRA_MUTANT_TRACES") else {
            return;
        };
        let traces = std::fs::read_to_string(&path).unwrap();
        // Every action a call names counts as taking one row: the most candidates.
        let mut one: Vec<String> = traces
            .lines()
            .filter(|l| l.starts_with("C "))
            .filter_map(|l| l.split_whitespace().nth(2).map(str::to_string))
            .collect();
        one.sort();
        one.dedup();
        let t = std::time::Instant::now();
        let a = fast(&traces, &one, "9", &[], 1, 0);
        let fast_time = t.elapsed();
        let t = std::time::Instant::now();
        let b = reference::mutants(&traces, &one, "9", &[], 1, 0);
        let reference_time = t.elapsed();
        assert_eq!(a, b);
        eprintln!(
            "{path}: {} lines; fast {fast_time:?}, reference {reference_time:?}",
            traces.lines().count()
        );
    }

    /// Traces whose BFS stopped at its cap have no X row: no mutant of that kind, and why.
    #[test]
    fn a_kind_with_no_row_says_why() {
        let capped: String = TRACES
            .lines()
            .filter(|l| !l.starts_with("X "))
            .map(|l| format!("{l}\n"))
            .collect();
        let m = mutants(&capped, &[], "2", &[], 1, 0);
        assert_eq!(
            m.none,
            [
                (MutantKind::RunRefused, "the traces try no refused call"),
                (MutantKind::AbsentRow, "the traces make no such call"),
                (
                    MutantKind::GuardRemoved,
                    "the traces try no call a condition rejects"
                ),
            ]
        );
    }
}
