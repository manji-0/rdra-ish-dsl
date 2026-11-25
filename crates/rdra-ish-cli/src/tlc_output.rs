//! TLC's text output in a canonical form: the state graph `tlc -dump dot` writes and the run log
//! (violated invariant or property names and their trace blocks).
//!
//! A state is a map from `name[slot]` (or a bare `name` for a global) to the value text as
//! TLC prints it. The TLA printer renders values the same way, so the two compare as strings.

use std::collections::{BTreeMap, BTreeSet};

pub type CanonState = BTreeMap<String, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViolationKind {
    Invariant,
    ActionProperty,
    Temporal,
    /// Any other `Error: ... violated` line.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    /// `Initial predicate`, the action operator's name, or TLC's label as printed.
    pub label: String,
    pub state: CanonState,
}

/// How a temporal counterexample goes on after its last state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lasso {
    /// `State N: Stuttering`: the last state repeats forever.
    Stutter,
    /// `Back to state K: <Action ...>`: the action `label` moves the last state back to state
    /// `to` (from 1), and the behavior repeats from there.
    Back { to: usize, label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    /// The invariant or property name, when TLC names it.
    pub name: Option<String>,
    /// TLC's error line, verbatim.
    pub line: String,
    pub trace: Vec<TraceStep>,
    /// The loop a temporal counterexample ends in; `None` for a finite trace.
    pub lasso: Option<Lasso>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Counterexample,
    ToolError,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Counterexample => "counterexample",
            Outcome::ToolError => "tool_error",
        }
    }
}

/// One TLC run's log.
#[derive(Clone, Debug, Default)]
pub struct TlcRun {
    /// In report order; with `-continue` one name may repeat.
    pub violations: Vec<Violation>,
    /// TLC said it finished model checking.
    pub completed: bool,
    pub distinct_states: Option<u64>,
    /// The run stopped on TLC 1.7.x's trace read-back failure (`readback_failed`): the
    /// violations it printed are real, but it never reached the rest of the model.
    pub readback_failed: bool,
}

impl TlcRun {
    pub fn parse(output: &str) -> Self {
        let lines: Vec<&str> = output.lines().collect();
        let mut run = TlcRun::default();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            i += 1;
            if line.contains("Model checking completed") || line.contains("No error has been found")
            {
                run.completed = true;
            }
            if let Some(n) = distinct_states(line) {
                run.distinct_states = Some(n);
            }
            let Some(kind) = violation_kind(line) else {
                continue;
            };
            let name = violation_name(line);
            let (trace, mut lasso, next) = parse_trace(&lines, i);
            i = next;
            // TLC 1.8.0's `[]S` checked as an invariant ends at the state that breaks `S`:
            // every run through that state violates the property, the one that stutters
            // there among them, which is the lasso TLC 1.7.x printed for it.
            if lasso.is_none() && located_invariant(line.trim_start_matches("Error:").trim()) {
                lasso = Some(Lasso::Stutter);
            }
            run.violations.push(Violation {
                kind,
                name,
                line: line.to_string(),
                trace,
                lasso,
            });
        }
        run.readback_failed = readback_failed(output);
        run
    }

    /// A run TLC's trace read-back stopped is a tool error, whatever it printed before (#314).
    /// Violations decide next, then the exit status and TLC's own success line.
    pub fn outcome(&self, exit_ok: bool) -> Outcome {
        if self.readback_failed {
            Outcome::ToolError
        } else if !self.violations.is_empty() {
            Outcome::Counterexample
        } else if exit_ok && self.completed {
            Outcome::Ok
        } else {
            Outcome::ToolError
        }
    }

    /// The verdict TLC printed, whatever its exit: a violation whose trace is complete (a
    /// temporal one ends in its lasso), or the success line. `None` when the log stops before
    /// either, as a killed run's may.
    pub fn verdict(&self) -> Option<Outcome> {
        let complete = |v: &Violation| v.kind != ViolationKind::Temporal || v.lasso.is_some();
        match self.violations.is_empty() {
            false if self.violations.iter().all(complete) => Some(Outcome::Counterexample),
            false => None,
            true => self.completed.then_some(Outcome::Ok),
        }
    }

    pub fn violated_names(&self) -> BTreeSet<String> {
        self.violations
            .iter()
            .filter_map(|v| v.name.clone())
            .collect()
    }
}

/// `TypeOK` is the emitter's own typing, not a model rule: its violation is a tool error.
///
/// A temporal property's violation is `Error: Temporal properties were violated.` from TLC
/// 1.7.x and `Error: Temporal property P was violated.` from 1.8.0. TLC 1.8.0 checks a
/// property `[]S` of a state predicate `S` (an `always`) as an invariant on `S`, which it
/// names by where `S` is: `Error: Invariant line 9, col 3 to line 9, col 40 of module M is
/// violated.`, with a finite trace. That is the property's violation too.
fn violation_kind(line: &str) -> Option<ViolationKind> {
    let rest = line.strip_prefix("Error:")?.trim();
    if rest.starts_with("Invariant TypeOK ") {
        None
    } else if located_invariant(rest) {
        Some(ViolationKind::Temporal)
    } else if rest.starts_with("Invariant ") && rest.ends_with("is violated.") {
        Some(ViolationKind::Invariant)
    } else if rest.starts_with("Action property ") && rest.ends_with("is violated.") {
        Some(ViolationKind::ActionProperty)
    } else if rest.starts_with("Temporal properties were violated")
        || (rest.starts_with("Temporal property ") && rest.ends_with(" was violated."))
    {
        Some(ViolationKind::Temporal)
    } else if rest.contains("violated") {
        Some(ViolationKind::Other)
    } else {
        None
    }
}

/// `Invariant line L, col C to line L, col C of module M is violated.`: TLC 1.8.0's name for
/// the state predicate of a `[]S` property it checks as an invariant.
fn located_invariant(rest: &str) -> bool {
    rest.strip_prefix("Invariant line ")
        .and_then(|r| r.strip_suffix(" is violated."))
        .is_some_and(|r| r.contains(" of module "))
}

/// The check an invariant or action property line names. A temporal violation names none,
/// as TLC 1.7.x prints it: the run checks one property, and the caller knows which.
fn violation_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Error:")?.trim();
    if located_invariant(rest) {
        return None;
    }
    let rest = rest
        .strip_prefix("Invariant ")
        .or_else(|| rest.strip_prefix("Action property "))?;
    let name = rest.strip_suffix("is violated.")?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// What a run of a `StateModel_` cfg says, read line by line without its traces: with
/// `-continue` a witnesses cfg prints a trace for almost every reached state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InvariantScan {
    /// The invariants TLC reports violated, `TypeOK` aside, in either of TLC's forms:
    /// `Error: Invariant X is violated.` and `Error: Invariant X is violated by the initial
    /// state:`.
    pub violated: BTreeSet<String>,
    /// `TypeOK` failed: the printer's typing, so a tool error.
    pub type_ok: bool,
    /// TLC said it finished model checking.
    pub completed: bool,
    pub distinct_states: Option<u64>,
}

impl InvariantScan {
    pub fn line(&mut self, line: &str) {
        let line = line.trim();
        if line.contains("Model checking completed") || line.contains("No error has been found") {
            self.completed = true;
        }
        if let Some(n) = distinct_states(line) {
            self.distinct_states = Some(n);
        }
        let Some(rest) = line.strip_prefix("Error: Invariant ") else {
            return;
        };
        let Some((name, _)) = rest.split_once(" is violated") else {
            return;
        };
        match name.trim() {
            "TypeOK" => self.type_ok = true,
            "" => {}
            name => {
                self.violated.insert(name.to_string());
            }
        }
    }

    pub fn parse(output: &str) -> Self {
        let mut scan = InvariantScan::default();
        for line in output.lines() {
            scan.line(line);
        }
        scan
    }
}

fn distinct_states(line: &str) -> Option<u64> {
    let (head, _) = line.split_once(" distinct states found")?;
    head.rsplit(", ")
        .next()?
        .trim()
        .replace(',', "")
        .parse()
        .ok()
}

/// One of TLC's `Progress(..)` lines, printed about once a minute while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlcProgress {
    pub distinct: u64,
    pub queued: u64,
}

pub fn progress(line: &str) -> Option<TlcProgress> {
    let line = line.trim();
    if !line.starts_with("Progress(") {
        return None;
    }
    let distinct = distinct_states(line)?;
    let (head, _) = line.split_once(" states left on queue")?;
    let queued = head
        .rsplit(", ")
        .next()?
        .trim()
        .replace(',', "")
        .parse()
        .ok()?;
    Some(TlcProgress { distinct, queued })
}

/// The `State n:` blocks after a violation line, up to the next `Error:` or summary line, and
/// the lasso line that ends a temporal counterexample.
fn parse_trace(lines: &[&str], mut i: usize) -> (Vec<TraceStep>, Option<Lasso>, usize) {
    let mut steps = Vec::new();
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(lasso) = lasso_line(line) {
            return (steps, Some(lasso), i + 1);
        }
        if let Some(label) = state_header(line) {
            let (state, next) = parse_block(lines, i + 1);
            steps.push(TraceStep { label, state });
            i = next;
        } else if line.is_empty()
            || line.starts_with("Error: The behavior")
            || line.starts_with("Error: The following behavior")
        {
            i += 1;
        } else {
            break;
        }
    }
    (steps, None, i)
}

/// `State 9: Stuttering`, or `Back to state 7: <Cancel line 48, col 3 to ...>`. An action
/// label always prints in angle brackets, so a bare word is TLC's own stutter marker.
fn lasso_line(line: &str) -> Option<Lasso> {
    if let Some(rest) = line.strip_prefix("Back to state ") {
        let (to, label) = rest.split_once(':')?;
        return Some(Lasso::Back {
            to: to.trim().parse().ok()?,
            label: action_label(label),
        });
    }
    let (num, label) = line.strip_prefix("State ")?.split_once(':')?;
    num.trim().parse::<u64>().ok()?;
    (label.trim() == "Stuttering").then_some(Lasso::Stutter)
}

/// `State 3: <Pay line 9, col 3 to ...>` gives `Pay`; `State 1:` alone gives "".
fn state_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("State ")?;
    let (num, label) = rest.split_once(':')?;
    num.trim().parse::<u64>().ok()?;
    Some(action_label(label))
}

/// `<Pay line 9, col 3 to ...>` gives `Pay`; a label without brackets stays as it is.
fn action_label(label: &str) -> String {
    let label = label.trim();
    let label = label
        .strip_prefix('<')
        .and_then(|l| l.strip_suffix('>'))
        .unwrap_or(label);
    let label = match label.split_once(" line ") {
        Some((op, _)) => op,
        None => label,
    };
    action_name(label.trim()).to_string()
}

/// The action operator of a label: TLC 1.8.0 adds the arguments it took, `Pay(1)` or
/// `Enroll(1,1,1)`, where TLC 1.7.x prints `Pay`.
fn action_name(label: &str) -> &str {
    label.split_once('(').map_or(label, |(name, _)| name)
}

/// The `/\ var = value` lines from `start` to the next blank or header line. A line that
/// does not start a new assignment continues the previous one (TLC wraps long values).
fn parse_block(lines: &[&str], mut i: usize) -> (CanonState, usize) {
    let mut assigns: Vec<String> = Vec::new();
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || state_header(line).is_some() || line.starts_with("Error:") {
            break;
        }
        match line.strip_prefix("/\\ ") {
            Some(a) => assigns.push(a.to_string()),
            None if assigns.is_empty() => assigns.push(line.to_string()),
            None => {
                let last = assigns.last_mut().expect("nonempty");
                last.push(' ');
                last.push_str(line);
            }
        }
        i += 1;
    }
    let mut state = CanonState::new();
    for a in assigns {
        let Some((name, value)) = a.split_once(" = ") else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        match value.strip_prefix("<<").and_then(|v| v.strip_suffix(">>")) {
            Some(items) => {
                for (k, item) in split_top(items).into_iter().enumerate() {
                    state.insert(format!("{name}[{}]", k + 1), item);
                }
            }
            None => {
                state.insert(name.to_string(), value.to_string());
            }
        }
    }
    (state, i)
}

/// Comma-separated items at nesting depth 0, outside string literals.
fn split_top(items: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let chars: Vec<char> = items.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k];
        k += 1;
        if in_str {
            cur.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '<' if chars.get(k) == Some(&'<') => {
                depth += 1;
                cur.push(c);
                cur.push('<');
                k += 1;
                continue;
            }
            '>' if chars.get(k) == Some(&'>') => {
                depth -= 1;
                cur.push(c);
                cur.push('>');
                k += 1;
                continue;
            }
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The state graph `tlc -dump dot,actionlabels` writes. TLC prints a node line per distinct
/// state and an edge line per successor it generates, self-loops included and repeated when
/// two bindings reach the same successor.
#[allow(dead_code)] // read by tests/engine_equivalence.rs
#[derive(Debug)]
pub struct DotGraph<T> {
    /// In the order TLC first names them.
    pub nodes: Vec<DotNode<T>>,
    /// The distinct edge labels: the action operator names.
    pub labels: Vec<String>,
    /// `(from, to, label)`, indices into `nodes` and `labels`, in file order.
    pub edges: Vec<(u32, u32, u32)>,
}

#[allow(dead_code)] // read by tests/engine_equivalence.rs
#[derive(Debug)]
pub struct DotNode<T> {
    pub state: T,
    /// TLC fills the initial states.
    pub initial: bool,
}

/// Reads a dot dump line by line, each node's state through `state`. An `Err` names the line
/// that is neither a node nor an edge, a node `state` rejects (with its text), or a node an
/// edge names but no line declares.
#[allow(dead_code)] // read by tests/engine_equivalence.rs
pub fn parse_dot<T>(
    dot: impl std::io::BufRead,
    mut state: impl FnMut(&CanonState) -> Option<T>,
) -> Result<DotGraph<T>, String> {
    let mut ids: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut nodes: Vec<Option<DotNode<T>>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut edges = Vec::new();
    let mut node = |id: &str, nodes: &mut Vec<Option<DotNode<T>>>| {
        *ids.entry(id.to_string()).or_insert_with(|| {
            nodes.push(None);
            (nodes.len() - 1) as u32
        })
    };
    for line in dot.lines() {
        let line = line.map_err(|e| format!("reading the dot dump: {e}"))?;
        let line = line.trim();
        let Some((head, attrs)) = line.split_once(" [") else {
            continue;
        };
        // Attributes for every node or edge, as TLC 1.8.0 starts with `node [shape=box,...]`.
        if matches!(head, "node" | "edge" | "graph") {
            continue;
        }
        let label = dot_label(attrs).ok_or_else(|| format!("no label: {line}"))?;
        match head.split_once(" -> ") {
            Some((from, to)) => {
                let label = action_name(&label).to_string();
                let from = node(from.trim(), &mut nodes);
                let to = node(to.trim(), &mut nodes);
                let l = match labels.iter().position(|l| *l == label) {
                    Some(l) => l,
                    None => {
                        labels.push(label);
                        labels.len() - 1
                    }
                };
                edges.push((from, to, l as u32));
            }
            None => {
                let at = node(head, &mut nodes) as usize;
                let lines: Vec<&str> = label.lines().collect();
                let (canon, _) = parse_block(&lines, 0);
                let decoded =
                    state(&canon).ok_or_else(|| format!("no state of the cone:\n{label}"))?;
                nodes[at] = Some(DotNode {
                    state: decoded,
                    initial: attrs.contains("style = filled"),
                });
            }
        }
    }
    let nodes = nodes
        .into_iter()
        .enumerate()
        .map(|(i, n)| n.ok_or_else(|| format!("an edge names node {i}, which no line declares")))
        .collect::<Result<_, _>>()?;
    Ok(DotGraph {
        nodes,
        labels,
        edges,
    })
}

/// The `label="..."` attribute, unescaped.
fn dot_label(attrs: &str) -> Option<String> {
    let rest = &attrs[attrs.find("label=\"")? + "label=\"".len()..];
    let mut out = String::new();
    let mut chars = rest.chars();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
}

/// Whether a run's log says the JVM ran out of memory: TLC's `Error: Java ran out of memory`
/// (exit 255), or an `OutOfMemoryError` TLC did not catch.
pub fn out_of_memory(output: &str) -> bool {
    output.contains("OutOfMemoryError") || output.contains("out of memory")
}

/// Whether a run's log shows TLC 1.7.x failing to read a trace back (#314). To print a trace,
/// TLC reads state records back from its trace file, a `BufferedRandomAccessFile`. From JDK 21
/// on, `RandomAccessFile.readLong` goes through that class's bulk `read(byte[], int, int)`,
/// which in TLC 1.7.x computes a negative length for a record still in its write buffer; TLC
/// then prints `java.lang.ArrayIndexOutOfBoundsException` with `arraycopy: length -N is
/// negative` or `Index N out of bounds for length M`, and stops.
pub fn readback_failed(output: &str) -> bool {
    output.contains("TLC threw an unexpected exception")
        && output.contains("ArrayIndexOutOfBoundsException")
        && (output.contains("arraycopy: length -") || output.contains("out of bounds for length"))
}

/// What a run that `readback_failed` says to do.
pub const READBACK_HINT: &str = "hint: TLC stopped with an ArrayIndexOutOfBoundsException while it read a trace back, which TLC 1.7.x does on JDK 21 and later (#314); use TLC 1.8.0 (the repository flake's `tlc` is 1.8.0), or run TLC 1.7.x on JDK 17 or earlier";

/// The TLC and the JVM a run names in its first lines, as TLC prints them:
///
/// ```text
/// TLC2 Version 2.19 of 08 August 2024 (rev: 5a47802)
/// Running breadth-first search Model-Checking with fp 6 and seed -19 with 1 worker on 18 cores with 1024MB heap and 64MB offheap memory [pid: 30808] (Mac OS X 27.0 aarch64, Azul Systems, Inc. 21.0.11 x86_64, MSBDiskFPSet, DiskStateQueue).
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Banner {
    /// `2.19` above. TLC 1.7.x is `2.15` to `2.19`; the TLC 1.8.0 build the flake pins prints
    /// a date, `2026.11.19.180144`.
    pub tlc: Option<String>,
    /// `21.0.11` above; JDK 8 prints `1.8.0_492`.
    pub java: Option<String>,
}

impl Banner {
    /// The banner of a whole log.
    pub fn read(output: &str) -> Banner {
        let mut banner = Banner::default();
        for line in output.lines() {
            if banner.complete() {
                break;
            }
            banner.line(line);
        }
        banner
    }

    /// Reads one line of a log.
    pub fn line(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("TLC2 Version ") {
            self.tlc = rest.split_whitespace().next().map(str::to_string);
        } else if line.starts_with("Running ") {
            self.java = running_java(line).map(str::to_string);
        }
    }

    pub fn complete(&self) -> bool {
        self.tlc.is_some() && self.java.is_some()
    }

    /// The warning for TLC 1.7.x (a `2.` version) on JDK 21 or later, which fails reading a
    /// trace back (`readback_failed`); `None` for any other pair, and while either is unknown.
    pub fn readback_warning(&self) -> Option<String> {
        let (tlc, java) = (self.tlc.as_deref()?, self.java.as_deref()?);
        let feature = java_feature(java)?;
        (tlc.starts_with("2.") && feature >= 21).then(|| {
            format!(
                "TLC {tlc} (TLC 1.7.x) runs on Java {java}; TLC 1.7.x fails reading a trace back on JDK 21 and later, and a run that prints many traces stops with an ArrayIndexOutOfBoundsException (#314); use TLC 1.8.0 (the repository flake's `tlc` is 1.8.0), or run TLC 1.7.x on JDK 17 or earlier"
            )
        })
    }
}

/// The Java version in a `Running ...` line: TLC ends it with `(<os> <version> <arch>,
/// <vendor> <java version> <arch>, <fp set>, <queue>).`, and a vendor can hold a comma.
fn running_java(line: &str) -> Option<&str> {
    let open = line.rfind(" (")?;
    let inner = line[open + 2..].trim_end_matches('.').strip_suffix(')')?;
    let mut parts = inner.rsplitn(3, ", ");
    let (_queue, _fp_set, rest) = (parts.next()?, parts.next()?, parts.next()?);
    rest.split_whitespace().rev().nth(1)
}

/// The JDK feature release of a Java version: 8 for `1.8.0_492`, 21 for `21.0.11` or `21-ea`.
fn java_feature(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    match first {
        1 => parts.next()?.parse().ok(),
        n => Some(n),
    }
}

/// A short report of a failed run: each violated check's error line and its first trace, or
/// the log's tail when nothing was violated. A run that `readback_failed` starts with its hint.
pub fn summarize(run: &TlcRun, output: &str) -> String {
    let mut lines = Vec::new();
    if run.readback_failed {
        lines.push(READBACK_HINT.to_string());
    }
    if !run.violations.is_empty() {
        let mut seen = BTreeSet::new();
        for v in run.violations.iter().filter(|v| seen.insert(&v.line)) {
            lines.push(format!("violated: {}", v.line));
            if !v.trace.is_empty() {
                lines.push("counterexample:".to_string());
                for (n, step) in v.trace.iter().enumerate() {
                    lines.push(format!("  State {}: {}", n + 1, step.label));
                    for (var, value) in &step.state {
                        lines.push(format!("    {var} = {value}"));
                    }
                }
                match &v.lasso {
                    Some(Lasso::Stutter) => {
                        lines.push(format!("  State {}: Stuttering", v.trace.len() + 1));
                    }
                    Some(Lasso::Back { to, label }) => {
                        lines.push(format!("  Back to state {to}: {label}"));
                    }
                    None => {}
                }
            }
        }
        return lines.join("\n");
    }
    if out_of_memory(output) {
        lines.push(
            "hint: TLC ran out of memory; set the heap with RDRA_TLC_HEAP=4g or JAVA_TOOL_OPTIONS=-Xmx4g, or raise --memory when it left the heap less than 1g"
                .to_string(),
        );
    }
    let tail: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let start = tail.len().saturating_sub(20);
    if tail.is_empty() {
        return "no TLC output captured; inspect the generated .tla/.cfg".to_string();
    }
    lines.push("TLC output (tail):".to_string());
    lines.extend(tail[start..].iter().map(|l| format!("  {l}")));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = r#"Finished computing initial states: 1 distinct state generated.
Error: Invariant I1 is violated.
Error: The behavior up to this point is:
State 1: <Initial predicate>
/\ now = 0
/\ e = <<"absent", "absent">>
/\ x = <<-1, -1>>

State 2: <C line 8, col 10 to line 8, col 128 of module M>
/\ now = 0
/\ e = <<"present", "absent">>
/\ x = <<1, -1>>

Error: Action property A1 is violated.
Error: The behavior up to this point is:
State 1: <Initial predicate>
/\ now = 0
/\ e = <<"absent", "absent">>
/\ x = <<-1, -1>>

Model checking completed. No error has been found.
114 states generated, 39 distinct states found, 0 states left on queue.
"#;

    #[test]
    fn run_lists_each_violation_with_its_trace() {
        let run = TlcRun::parse(RUN);
        assert_eq!(run.violations.len(), 2);
        let first = &run.violations[0];
        assert_eq!(first.kind, ViolationKind::Invariant);
        assert_eq!(first.name.as_deref(), Some("I1"));
        assert_eq!(first.trace.len(), 2);
        assert_eq!(first.trace[1].label, "C");
        assert_eq!(first.trace[1].state["e[1]"], "\"present\"");
        assert_eq!(first.trace[1].state["x[2]"], "-1");
        assert_eq!(first.trace[1].state["now"], "0");
        assert_eq!(run.violations[1].kind, ViolationKind::ActionProperty);
        assert_eq!(
            run.violated_names().into_iter().collect::<Vec<_>>(),
            ["A1", "I1"]
        );
        assert_eq!(run.distinct_states, Some(39));
        // `-continue` prints both; a violation wins.
        assert_eq!(run.outcome(true), Outcome::Counterexample);
    }

    #[test]
    fn a_progress_line_gives_the_distinct_and_queued_counts() {
        let line = "Progress(14) at 2026-11-02 10:05:00: 9,210,811 states generated (1,842,162 s/min), 6,877,496 distinct states found (1,375,499 ds/min), 5,949,713 states left on queue.";
        assert_eq!(
            progress(line),
            Some(TlcProgress {
                distinct: 6_877_496,
                queued: 5_949_713
            })
        );
        assert_eq!(progress("39 distinct states found"), None);
        assert_eq!(TlcRun::parse(line).distinct_states, Some(6_877_496));
    }

    #[test]
    fn outcome_needs_the_success_line() {
        let ok = TlcRun::parse("Model checking completed. No error has been found.\n");
        assert_eq!(ok.outcome(true), Outcome::Ok);
        assert_eq!(ok.outcome(false), Outcome::ToolError);
        let crash = TlcRun::parse("Error: TLC threw an unexpected exception.\n");
        assert_eq!(crash.outcome(false), Outcome::ToolError);
        assert_eq!(TlcRun::parse("").outcome(true), Outcome::ToolError);
    }

    #[test]
    fn a_killed_run_keeps_the_verdict_it_printed() {
        let ok = TlcRun::parse("Model checking completed. No error has been found.\n");
        assert_eq!(ok.verdict(), Some(Outcome::Ok));
        assert_eq!(
            TlcRun::parse(STUTTER).verdict(),
            Some(Outcome::Counterexample)
        );
        assert_eq!(TlcRun::parse(BACK).verdict(), Some(Outcome::Counterexample));
        let cut = &BACK[..BACK.find("Back to state").unwrap()];
        assert_eq!(TlcRun::parse(cut).verdict(), None);
        assert_eq!(TlcRun::parse("").verdict(), None);
    }

    #[test]
    fn a_type_ok_violation_is_a_tool_error() {
        let run = TlcRun::parse("Error: Invariant TypeOK is violated.\n");
        assert!(run.violations.is_empty());
        assert_eq!(run.outcome(false), Outcome::ToolError);
        assert_eq!(run.verdict(), None);
    }

    /// A run that halts in an initial state names its invariant in another form, which
    /// `TlcRun` does not name; a state-model run reads both.
    #[test]
    fn an_invariant_scan_names_both_forms_and_flags_type_ok() {
        let log = "Error: Invariant Witness_A is violated by the initial state:\n\
                   /\\ s = <<\"a\">>\n\
                   Error: Invariant Witness_B is violated.\n\
                   Error: Invariant TypeOK is violated.\n\
                   Model checking completed. No error has been found.\n\
                   9 states generated, 4 distinct states found, 0 states left on queue.\n";
        let scan = InvariantScan::parse(log);
        assert_eq!(
            scan.violated.iter().map(String::as_str).collect::<Vec<_>>(),
            ["Witness_A", "Witness_B"]
        );
        assert!(scan.type_ok && scan.completed);
        assert_eq!(scan.distinct_states, Some(4));
    }

    #[test]
    fn temporal_violation_has_no_name() {
        let run = TlcRun::parse("Error: Temporal properties were violated.\n");
        assert_eq!(run.violations[0].kind, ViolationKind::Temporal);
        assert!(run.violations[0].name.is_none());
    }

    /// TLC 1.8.0 names the temporal property it reports, prints an `always` as an invariant
    /// on its state predicate, and adds each action's arguments to its label; each reads as
    /// TLC 1.7.x's form does.
    #[test]
    fn tlc_1_8_forms_read_as_tlc_1_7_forms() {
        let log = "Error: Temporal property ShippedReopens was violated.\n\n\
                   Error: The following behavior constitutes a counter-example:\n\n\
                   State 1: <Initial predicate>\n\
                   /\\ s = <<\"pending\", \"pending\">>\n\n\
                   State 2: <Place(1) line 46, col 3 to line 48, col 27 of module Train>\n\
                   /\\ s = <<\"placed\", \"pending\">>\n\n\
                   State 3: <Enroll(1,1,1) line 50, col 3 to line 52, col 27 of module Train>\n\
                   /\\ s = <<\"placed\", \"placed\">>\n\n\
                   State 4: Stuttering\n";
        let run = TlcRun::parse(log);
        let v = &run.violations[0];
        assert_eq!((v.kind, v.name.as_deref()), (ViolationKind::Temporal, None));
        let labels: Vec<&str> = v.trace.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Initial predicate", "Place", "Enroll"]);
        assert_eq!(v.lasso, Some(Lasso::Stutter));

        let always = "Error: Invariant line 109, col 33 to line 109, col 96 of module Cone_JobWaits_Full is violated.\n";
        let v = &TlcRun::parse(always).violations[0];
        assert_eq!((v.kind, v.name.as_deref()), (ViolationKind::Temporal, None));
        assert_eq!(v.lasso, Some(Lasso::Stutter));
        let named = "Error: Invariant Forbidden_line_0 is violated.\n";
        let v = &TlcRun::parse(named).violations[0];
        assert_eq!(v.kind, ViolationKind::Invariant);
        assert_eq!(v.name.as_deref(), Some("Forbidden_line_0"));

        let dot = "strict digraph DiskGraph {\nnode [shape=box,style=rounded]\n\
                   1 [label=\"/\\\\ s = 0\",style = filled]\n\
                   1 -> 2 [label=\"Place(1)\",color=\"black\",fontcolor=\"black\"];\n\
                   2 [label=\"/\\\\ s = 1\",tooltip=\"/\\\\ s = 1\"];\n}\n";
        let graph = parse_dot(dot.as_bytes(), |s| s.get("s").cloned()).unwrap();
        assert_eq!(graph.labels, ["Place"]);
        assert!(graph.nodes[0].initial && !graph.nodes[1].initial);
    }

    /// TLC 1.7.4's two lasso tails, trimmed from runs of a model with a Reopen use case.
    const STUTTER: &str = r#"Error: Temporal properties were violated.

Error: The following behavior constitutes a counter-example:

State 1: <Initial predicate>
/\ s = <<"pending", "pending">>

State 2: <Place line 60, col 3 to line 62, col 51 of module M>
/\ s = <<"shipped", "pending">>

State 3: Stuttering
Finished checking temporal properties in 00s at 2026-10-29 23:39:55
"#;

    const BACK: &str = r#"Error: Temporal properties were violated.

Error: The following behavior constitutes a counter-example:

State 1: <Initial predicate>
/\ s = <<"pending", "pending">>

State 2: <Cancel line 48, col 3 to line 51, col 51 of module M>
/\ s = <<"cancelled", "pending">>

State 3: <Reopen line 54, col 3 to line 57, col 51 of module M>
/\ s = <<"pending", "pending">>

Back to state 2: <Cancel line 48, col 3 to line 51, col 51 of module M>

Finished checking temporal properties in 00s at 2026-10-29 23:39:55
"#;

    #[test]
    fn a_stuttering_lasso_is_no_step() {
        let run = TlcRun::parse(STUTTER);
        let v = &run.violations[0];
        assert_eq!(v.trace.len(), 2);
        assert_eq!(v.trace[1].label, "Place");
        assert_eq!(v.lasso, Some(Lasso::Stutter));
        let s = summarize(&run, STUTTER);
        assert!(s.ends_with("  State 3: Stuttering"), "{s}");
    }

    #[test]
    fn a_back_edge_names_its_target_and_action() {
        let run = TlcRun::parse(BACK);
        let v = &run.violations[0];
        let labels: Vec<&str> = v.trace.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Initial predicate", "Cancel", "Reopen"]);
        assert_eq!(
            v.lasso,
            Some(Lasso::Back {
                to: 2,
                label: "Cancel".to_string()
            })
        );
        assert_eq!(v.kind, ViolationKind::Temporal);
        let s = summarize(&run, BACK);
        assert!(s.ends_with("  Back to state 2: Cancel"), "{s}");
    }

    #[test]
    fn a_safety_trace_has_no_lasso() {
        assert!(TlcRun::parse(RUN)
            .violations
            .iter()
            .all(|v| v.lasso.is_none()));
    }

    /// TLC 1.7.4's layout: an edge may name its target before the target's node line, an
    /// identical edge repeats, and the rank lines carry no label.
    const DOT: &str = r#"strict digraph DiskGraph {
nodesep=0.35;
subgraph cluster_graph {
color="white";
-64 [label="/\\ a = <<\"x, y\", \"z\">>\n/\\ b = <<1, 2>>",style = filled]
-64 -> 72 [label="Make",color="black",fontcolor="black"];
72 [label="/\\ a = <<\"q\", \"r\">>\n/\\ b = <<3, 4>>"];
72 -> 72 [label="Noop",color="black",fontcolor="black"];
72 -> 72 [label="Noop",color="black",fontcolor="black"];
{rank = same; -64;}
{rank = same; 72;}
}
}"#;

    #[test]
    fn dot_dump_gives_states_and_labelled_edges() {
        let g = parse_dot(DOT.as_bytes(), |s| Some(s.clone())).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert!(g.nodes[0].initial && !g.nodes[1].initial);
        assert_eq!(g.nodes[0].state["a[1]"], "\"x, y\"");
        assert_eq!(g.nodes[0].state["b[2]"], "2");
        assert_eq!(g.nodes[1].state["a[2]"], "\"r\"");
        assert_eq!(g.labels, ["Make", "Noop"]);
        assert_eq!(g.edges, [(0, 1, 0), (1, 1, 1), (1, 1, 1)]);
    }

    #[test]
    fn dot_dump_rejects_a_state_and_an_undeclared_node() {
        let err = parse_dot(DOT.as_bytes(), |s| s.contains_key("b[3]").then_some(())).unwrap_err();
        assert!(
            err.starts_with("no state of the cone:\n/\\ a = <<\"x, y\""),
            "{err}"
        );
        let dangling = "1 -> 2 [label=\"Make\"];\n1 [label=\"/\\\\ a = 1\"];\n";
        assert_eq!(
            parse_dot(dangling.as_bytes(), |_| Some(())).unwrap_err(),
            "an edge names node 1, which no line declares"
        );
    }

    #[test]
    fn summary_names_the_violation() {
        let run = TlcRun::parse(RUN);
        let s = summarize(&run, RUN);
        assert!(s.contains("Error: Invariant I1 is violated."));
        assert!(s.contains("State 2: C"));
        assert!(s.contains("e[1] = \"present\""));
        assert!(s.contains("violated: Error: Action property A1 is violated."));
    }

    #[test]
    fn summary_reports_a_repeated_violation_once() {
        let log = "Error: Invariant I1 is violated.\nError: Invariant I1 is violated.\n";
        let s = summarize(&TlcRun::parse(log), log);
        assert_eq!(s.matches("violated: ").count(), 1);
    }

    #[test]
    fn out_of_memory_names_the_heap_settings() {
        let log = "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space\n";
        let run = TlcRun::parse(log);
        assert_eq!(run.outcome(false), Outcome::ToolError);
        let s = summarize(&run, log);
        assert!(s.starts_with("hint: TLC ran out of memory"), "{s}");
        assert!(s.contains("RDRA_TLC_HEAP=4g"), "{s}");
        assert!(s.contains("OutOfMemoryError"), "{s}");
    }

    /// The end of a `-continue` run of TLC 1.7.4 on JDK 21 that stopped printing a trace.
    const READBACK: &str = r#"Error: Invariant I1 is violated.
Error: The behavior up to this point is:
State 1: <Initial predicate>
/\ x = 0

Error: Action property P is violated.
Error: Evaluating action property P failed.
arraycopy: length -1536 is negative
Error: TLC threw an unexpected exception.
This was probably caused by an error in the spec or model.
The exception was a java.lang.RuntimeException
: java.lang.ArrayIndexOutOfBoundsException: arraycopy: length -1536 is negative
53565 states generated, 10179 distinct states found, 1356 states left on queue.
"#;

    #[test]
    fn a_trace_read_back_failure_is_a_tool_error_with_its_hint() {
        let run = TlcRun::parse(READBACK);
        assert!(run.readback_failed);
        assert!(!run.violations.is_empty());
        assert_eq!(run.outcome(false), Outcome::ToolError);
        let s = summarize(&run, READBACK);
        assert!(s.starts_with(READBACK_HINT), "{s}");
        assert!(s.contains("JDK 17 or earlier"), "{s}");
        let index = "Error: TLC threw an unexpected exception.\nThe exception was a java.lang.ArrayIndexOutOfBoundsException\n: Index 120 out of bounds for length 1\n";
        assert!(readback_failed(index));
        let other = "Error: TLC threw an unexpected exception.\nThe exception was a java.lang.NullPointerException\n";
        assert!(!readback_failed(other));
        assert!(!TlcRun::parse(RUN).readback_failed);
    }

    #[test]
    fn the_banner_warns_only_for_tlc_1_7_on_jdk_21_or_later() {
        let banner = |tlc: &str, java: &str| {
            Banner::read(&format!(
                "TLC2 Version {tlc} (rev: 5a47802)\nRunning breadth-first search Model-Checking with fp 6 and seed -1 with 1 worker on 18 cores with 1024MB heap and 64MB offheap memory [pid: 1] (Mac OS X 27.0 aarch64, Azul Systems, Inc. {java} x86_64, MSBDiskFPSet, DiskStateQueue).\nParsing file M.tla\n"
            ))
        };
        let jdk21 = banner("2.19 of 08 August 2024", "21.0.11");
        assert_eq!(jdk21.tlc.as_deref(), Some("2.19"));
        assert_eq!(jdk21.java.as_deref(), Some("21.0.11"));
        let warning = jdk21.readback_warning().expect("TLC 1.7.4 on JDK 21");
        assert!(warning.contains("JDK 17 or earlier"), "{warning}");
        let date = "2026.11.19.180144";
        assert!(banner("2.19 of 08 August 2024", "25.0.3")
            .readback_warning()
            .is_some());
        assert!(banner("2.19 of 08 August 2024", "17.0.19")
            .readback_warning()
            .is_none());
        assert!(banner("2.19 of 08 August 2024", "1.8.0_492")
            .readback_warning()
            .is_none());
        assert!(banner(date, "21.0.11").readback_warning().is_none());
        assert!(banner(date, "25.0.3").readback_warning().is_none());
        assert!(Banner::read("TLC2 Version 2.19\n")
            .readback_warning()
            .is_none());
        assert_eq!(java_feature("21-ea"), Some(21));
    }
}
