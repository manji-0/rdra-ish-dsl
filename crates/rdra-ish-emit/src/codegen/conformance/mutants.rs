//! The self-check of `check --code=run` (#439): traces that are wrong on purpose. Each mutant
//! is one trace of the real traces cut after one row, with that row changed so that the model
//! does not do what it says. A service that conforms fails each at that row; a run that passes
//! one did not run the harness on the traces it was given, swallowed its failure, or (for
//! [`MutantKind::AbsentRow`]) runs a call on a row it never looked up. The user's code is not
//! changed (docs/design/code-check.md, decision 1).
//!
//! Only the last row is changed, and only into something the model never does, so no mutant
//! is equivalent to the real traces: a conforming service never passes one.

use std::collections::BTreeMap;

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
#[derive(Clone, Debug, Default)]
pub struct Mutants {
    pub mutants: Vec<Mutant>,
    pub none: Vec<(MutantKind, &'static str)>,
}

/// A row of a trace: its line in the traces (0-based), its text, and the state the trace is
/// in before it.
struct Row<'a> {
    line: usize,
    text: &'a str,
    at: &'a str,
}

/// Up to `per_kind` mutants of each kind from `traces`, picked by `seed`: the same traces
/// and seed give the same mutants. `one_row` names the actions whose call takes one existing
/// row, first; `absent` is an id no row has. `guards` are the refused calls a condition
/// rejects ([`super::Traces::guards`]): with `per_kind` above 0, one guard-removed mutant per
/// condition.
pub fn mutants(
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

    // Each candidate: the trace's first line, the row, and the row it becomes.
    let mut candidates: BTreeMap<MutantKind, Vec<(usize, usize, String)>> = BTreeMap::new();
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
    // Per condition, the rows it rejects: the row of the traces, in the state it names.
    let mut guarded: BTreeMap<&str, Vec<(usize, usize, String)>> = BTreeMap::new();
    for (start, row) in &rows {
        for g in guards
            .iter()
            .filter(|g| g.state == row.at && g.row == row.text)
        {
            let call: Vec<&str> = row.text.split_whitespace().skip(2).collect();
            guarded.entry(&g.condition).or_default().push((
                *start,
                row.line,
                format!("C {} {}", row.at, call.join(" ")),
            ));
        }
    }
    for (start, row) in &rows {
        let toks: Vec<&str> = row.text.split_whitespace().collect();
        match toks.as_slice() {
            ["X", _kind, call @ ..] => candidates
                .entry(MutantKind::RunRefused)
                .or_default()
                .push((*start, row.line, format!("C {} {}", row.at, call.join(" ")))),
            ["C", to, call @ ..] => {
                let push = |c: &mut BTreeMap<MutantKind, Vec<_>>, k, text| {
                    c.entry(k).or_default().push((*start, row.line, text));
                };
                push(
                    &mut candidates,
                    MutantKind::RefuseRun,
                    format!("X - {}", call.join(" ")),
                );
                let rows_of = states.get(to);
                let other: Vec<&str> = states
                    .iter()
                    .filter(|(_, r)| Some(*r) != rows_of)
                    .map(|(id, _)| *id)
                    .collect();
                if !other.is_empty() {
                    let pick = other[rng.below(other.len())];
                    push(
                        &mut candidates,
                        MutantKind::WrongState,
                        format!("C {pick} {}", call.join(" ")),
                    );
                }
                if let [action, _id, rest @ ..] = call {
                    if one_row.iter().any(|a| a == action) {
                        let rest: String = rest.iter().map(|t| format!(" {t}")).collect();
                        push(
                            &mut candidates,
                            MutantKind::AbsentRow,
                            format!("C {to} {action} {absent}{rest}"),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = Mutants::default();
    let cut = |start: usize, line: usize, now: String, kind, guard: Option<String>| {
        let mut text = String::new();
        for l in header.iter().chain(&lines[start..line]) {
            text.push_str(l);
            text.push('\n');
        }
        text.push_str(&now);
        text.push('\n');
        Mutant {
            kind,
            line: header.len() + (line - start) + 1,
            text,
            was: (line + 1, lines[line].to_string()),
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
                    let (start, line, now) = pool[rng.below(pool.len())].clone();
                    out.mutants
                        .push(cut(start, line, now, kind, Some(condition.to_string())));
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
            let (start, line, now) = pool.swap_remove(rng.below(pool.len()));
            out.mutants.push(cut(start, line, now, kind, None));
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
