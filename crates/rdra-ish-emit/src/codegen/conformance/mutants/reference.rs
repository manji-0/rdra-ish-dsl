//! The twin of [`super::mutants`] (#439): the function as it was before the candidates were
//! picked lazily, unchanged. A `shadow` build runs both on every call and panics when they
//! differ; the tests compare them on traces of their own. See docs/design/code-check.md.

use std::collections::BTreeMap;

use super::{Mutant, MutantKind, Mutants, SplitMix};

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
pub(super) fn mutants(
    traces: &str,
    one_row: &[String],
    absent: &str,
    guards: &[super::super::Guarded],
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
