//! TLC's text output in a canonical form: the state graph `tlc -dump dot` writes and the run log
//! (violated invariant or property names and their trace blocks).
//!
//! A state is a map from `name[slot]` (or a bare `name` for a global) to the value text as
//! TLC prints it. The TLA printer renders values the same way, so the two compare as strings.
//! Std only: `tests/engine_equivalence.rs` includes this file by path.

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    /// The invariant or property name, when TLC names it.
    pub name: Option<String>,
    /// TLC's error line, verbatim.
    pub line: String,
    pub trace: Vec<TraceStep>,
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
            let (trace, next) = parse_trace(&lines, i);
            i = next;
            run.violations.push(Violation {
                kind,
                name,
                line: line.to_string(),
                trace,
            });
        }
        run
    }

    /// Violations decide first, then the exit status and TLC's own success line.
    pub fn outcome(&self, exit_ok: bool) -> Outcome {
        if !self.violations.is_empty() {
            Outcome::Counterexample
        } else if exit_ok && self.completed {
            Outcome::Ok
        } else {
            Outcome::ToolError
        }
    }

    #[allow(dead_code)] // read by tests/engine_equivalence.rs
    pub fn violated_names(&self) -> BTreeSet<String> {
        self.violations
            .iter()
            .filter_map(|v| v.name.clone())
            .collect()
    }
}

fn violation_kind(line: &str) -> Option<ViolationKind> {
    let rest = line.strip_prefix("Error:")?.trim();
    if rest.starts_with("Invariant ") && rest.ends_with("is violated.") {
        Some(ViolationKind::Invariant)
    } else if rest.starts_with("Action property ") && rest.ends_with("is violated.") {
        Some(ViolationKind::ActionProperty)
    } else if rest.starts_with("Temporal properties were violated") {
        Some(ViolationKind::Temporal)
    } else if rest.contains("violated") {
        Some(ViolationKind::Other)
    } else {
        None
    }
}

fn violation_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Error:")?.trim();
    let rest = rest
        .strip_prefix("Invariant ")
        .or_else(|| rest.strip_prefix("Action property "))?;
    let name = rest.strip_suffix("is violated.")?.trim();
    (!name.is_empty()).then(|| name.to_string())
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

/// The `State n:` blocks after a violation line, up to the next `Error:` or summary line.
fn parse_trace(lines: &[&str], mut i: usize) -> (Vec<TraceStep>, usize) {
    let mut steps = Vec::new();
    while i < lines.len() {
        let line = lines[i].trim();
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
    (steps, i)
}

/// `State 3: <Pay line 9, col 3 to ...>` gives `Pay`; `State 1:` alone gives "".
fn state_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("State ")?;
    let (num, label) = rest.split_once(':')?;
    num.trim().parse::<u64>().ok()?;
    let label = label.trim();
    let label = label
        .strip_prefix('<')
        .and_then(|l| l.strip_suffix('>'))
        .unwrap_or(label);
    let label = match label.split_once(" line ") {
        Some((op, _)) => op,
        None => label,
    };
    Some(label.trim().to_string())
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
        let label = dot_label(attrs).ok_or_else(|| format!("no label: {line}"))?;
        match head.split_once(" -> ") {
            Some((from, to)) => {
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

/// A short report of a failed run: each violated check's error line and its first trace, or
/// the log's tail when nothing was violated.
pub fn summarize(run: &TlcRun, output: &str) -> String {
    let mut lines = Vec::new();
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
            }
        }
        return lines.join("\n");
    }
    if output.contains("OutOfMemoryError") || output.contains("out of memory") {
        lines.push(
            "hint: TLC ran out of memory; raise the heap with RDRA_TLC_HEAP=4g or JAVA_TOOL_OPTIONS=-Xmx4g"
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
    fn outcome_needs_the_success_line() {
        let ok = TlcRun::parse("Model checking completed. No error has been found.\n");
        assert_eq!(ok.outcome(true), Outcome::Ok);
        assert_eq!(ok.outcome(false), Outcome::ToolError);
        let crash = TlcRun::parse("Error: TLC threw an unexpected exception.\n");
        assert_eq!(crash.outcome(false), Outcome::ToolError);
        assert_eq!(TlcRun::parse("").outcome(true), Outcome::ToolError);
    }

    #[test]
    fn temporal_violation_has_no_name() {
        let run = TlcRun::parse("Error: Temporal properties were violated.\n");
        assert_eq!(run.violations[0].kind, ViolationKind::Temporal);
        assert!(run.violations[0].name.is_none());
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
}
