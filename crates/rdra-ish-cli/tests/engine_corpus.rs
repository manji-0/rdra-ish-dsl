//! The adversarial corpus: small models, each built around one way the native checker, TLC,
//! and z3 could disagree, with the verdict every engine must give written in the model's
//! header. `docs/engine-agreement.md` states the agreement rules, the header grammar, and
//! how to add a case.
//!
//! Every case under `tests/engine_corpus/` runs `verify --with-tlc --prove` (without TLC or
//! z3 when they are missing, which `RDRA_REQUIRE_TLC=1` and `RDRA_REQUIRE_Z3=1` turn into
//! failures), and `verify --instance-count 1` and `3` when the header names BFS verdicts per
//! bound. A case passes when every engine gives the verdict its header states and the
//! engines agree with each other by the rules below, whatever the header says. A known
//! disagreement is a `// known` line with its issue: the case stays green while the engine
//! keeps giving the recorded wrong verdict, and fails once it stops, so the line goes with
//! the fix.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rdra_ish_core::lifecycle::{Bounds, Caps, Fairness, PropertyVerdict, Scope, TransitionSystem};
use rdra_ish_core::load_project;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/engine_corpus")
}

/// A check's verdict from one engine. `None` is "the engine printed no verdict for it":
/// TLC checks no `stable`, `unchanged`, `precedes`, or `can_reach`, nor a state-model
/// finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V {
    Ok,
    Counterexample,
    NotDecided,
    None,
}

impl V {
    fn parse(s: &str) -> Option<V> {
        Some(match s {
            "ok" => V::Ok,
            "counterexample" => V::Counterexample,
            "not_decided" => V::NotDecided,
            "none" => V::None,
            _ => return None,
        })
    }

    fn decided(self) -> bool {
        matches!(self, V::Ok | V::Counterexample)
    }
}

impl fmt::Display for V {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            V::Ok => "ok",
            V::Counterexample => "counterexample",
            V::NotDecided => "not_decided",
            V::None => "none",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Z3 {
    Proved,
    Unproved,
    OutOfScope,
}

impl Z3 {
    fn parse(s: &str) -> Option<Z3> {
        Some(match s {
            "proved" => Z3::Proved,
            "unproved" => Z3::Unproved,
            "out_of_scope" => Z3::OutOfScope,
            _ => return None,
        })
    }
}

impl fmt::Display for Z3 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Z3::Proved => "proved",
            Z3::Unproved => "unproved",
            Z3::OutOfScope => "out_of_scope",
        })
    }
}

/// What one header line `// expect <Check>: ...` states.
#[derive(Clone, Debug)]
struct Expect {
    native: V,
    tlc: V,
    z3: Option<Z3>,
    /// The native verdict at 1, 2, and 3 instances per entity.
    bfs: Option<[V; 3]>,
}

/// The engines a `// known` line may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Engine {
    Native,
    Tlc,
    Z3,
    Bfs(u8),
}

impl Engine {
    fn parse(s: &str) -> Option<Engine> {
        Some(match s {
            "native" => Engine::Native,
            "tlc" => Engine::Tlc,
            "z3" => Engine::Z3,
            "bfs1" => Engine::Bfs(1),
            "bfs2" => Engine::Bfs(2),
            "bfs3" => Engine::Bfs(3),
            _ => return None,
        })
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Engine::Native => f.write_str("native"),
            Engine::Tlc => f.write_str("tlc"),
            Engine::Z3 => f.write_str("z3"),
            Engine::Bfs(n) => write!(f, "bfs{n}"),
        }
    }
}

/// `// known <Check>: <engine>=<observed> <issue URL>`: the engine gives `observed` where the
/// `expect` line states the right verdict, and the issue tracks it.
#[derive(Clone, Debug)]
struct Known {
    check: String,
    engine: Engine,
    observed: String,
    issue: String,
}

#[derive(Debug)]
struct Case {
    path: PathBuf,
    name: String,
    flags: Vec<String>,
    /// `// tlc: skip, <why>`: the case never runs TLC.
    tlc_skip: bool,
    expects: BTreeMap<String, Expect>,
    known: Vec<Known>,
    /// The `can_reach` properties of the model, which print a line only when they fail.
    can_reach: BTreeSet<String>,
}

impl Case {
    fn bounds(&self) -> Bounds {
        let flag = |name: &str| {
            self.flags.iter().position(|f| f == name).map(|i| {
                self.flags[i + 1]
                    .parse::<u8>()
                    .expect("a number after the flag")
            })
        };
        Bounds {
            instances: NonZeroU8::new(flag("--instance-count").unwrap_or(2)).unwrap(),
            int_max: flag("--int-range-max").unwrap_or(Bounds::VERIFY.int_max),
        }
    }
}

/// Parses the header: the `//` lines before the first line that is not a comment. Every
/// header line must be one of the forms the grammar names.
fn parse_case(path: &Path) -> Result<Case, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let mut case = Case {
        path: path.to_path_buf(),
        name,
        flags: Vec::new(),
        tlc_skip: false,
        expects: BTreeMap::new(),
        known: Vec::new(),
        can_reach: can_reach_properties(&text),
    };
    let mut trap = false;
    for line in text.lines() {
        let Some(body) = line.strip_prefix("//") else {
            break;
        };
        let body = body.trim();
        if body.starts_with("trap: ") {
            if trap {
                return Err("two `// trap:` lines".to_string());
            }
            trap = true;
        } else if let Some(flags) = body.strip_prefix("flags: ") {
            case.flags = flags.split_whitespace().map(str::to_string).collect();
            if case
                .flags
                .iter()
                .any(|f| f == "--with-tlc" || f == "--prove")
            {
                return Err(
                    "`// flags:` names --with-tlc or --prove, which the harness adds".into(),
                );
            }
        } else if body.starts_with("tlc: skip, ") {
            case.tlc_skip = true;
        } else if let Some(rest) = body.strip_prefix("expect ") {
            let (check, exp) = parse_expect(rest).ok_or(format!("malformed: {line}"))?;
            if case.expects.insert(check.clone(), exp).is_some() {
                return Err(format!("two `expect` lines for {check}"));
            }
        } else if let Some(rest) = body.strip_prefix("known ") {
            case.known
                .push(parse_known(rest).ok_or(format!("malformed: {line}"))?);
        } else if ["expect", "known", "flags", "tlc", "trap"]
            .iter()
            .any(|k| body.starts_with(&format!("{k}:")) || body.starts_with(&format!("{k} ")))
        {
            return Err(format!("malformed: {line}"));
        }
    }
    if !trap {
        return Err("no `// trap:` line naming the trap".to_string());
    }
    if case.expects.is_empty() {
        return Err("no `// expect` line".to_string());
    }
    for k in &case.known {
        if !case.expects.contains_key(&k.check) {
            return Err(format!(
                "`known` names {}, which no `expect` line states",
                k.check
            ));
        }
    }
    for (check, e) in &case.expects {
        if case.tlc_skip && e.tlc != V::None {
            return Err(format!("{check}: tlc={} on a case that skips TLC", e.tlc));
        }
        if e.bfs.is_some_and(|b| b[1] != e.native) {
            return Err(format!("{check}: bfs at 2 instances differs from native"));
        }
        if e.z3 == Some(Z3::Proved) && e.bfs.is_none() {
            return Err(format!(
                "{check}: z3=proved needs bfs=, the verdicts a proof must not contradict"
            ));
        }
    }
    Ok(case)
}

/// The names of the `property` declarations whose formula is a `can_reach`: the
/// declaration's line and the indented lines under it.
fn can_reach_properties(text: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("property ") else {
            continue;
        };
        let body = lines[i + 1..]
            .iter()
            .take_while(|l| l.starts_with(' '))
            .fold(rest.to_string(), |acc, l| acc + l);
        if body.contains("can_reach(") {
            out.extend(rest.split_whitespace().next().map(str::to_string));
        }
    }
    out
}

/// `<Check>: native=V tlc=V [z3=Z] [bfs=V,V,V]`
fn parse_expect(rest: &str) -> Option<(String, Expect)> {
    let (check, fields) = rest.split_once(": ")?;
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    for field in fields.split_whitespace() {
        let (k, v) = field.split_once('=')?;
        if map.insert(k, v).is_some() {
            return None;
        }
    }
    let native = V::parse(map.remove("native")?)?;
    let tlc = V::parse(map.remove("tlc")?)?;
    let z3 = match map.remove("z3") {
        Some(z) => Some(Z3::parse(z)?),
        None => None,
    };
    let bfs = match map.remove("bfs") {
        Some(b) => {
            let v: Vec<V> = b.split(',').map(V::parse).collect::<Option<_>>()?;
            Some(<[V; 3]>::try_from(v).ok()?)
        }
        None => None,
    };
    (map.is_empty() && valid_name(check)).then(|| {
        (
            check.to_string(),
            Expect {
                native,
                tlc,
                z3,
                bfs,
            },
        )
    })
}

/// `<Check>: <engine>=<observed> <issue URL>`
fn parse_known(rest: &str) -> Option<Known> {
    let (check, fields) = rest.split_once(": ")?;
    let mut words = fields.split_whitespace();
    let (engine, observed) = words.next()?.split_once('=')?;
    let issue = words.next()?;
    let engine = Engine::parse(engine)?;
    (words.next().is_none() && issue.starts_with("https://github.com/") && valid_name(check)).then(
        || Known {
            check: check.to_string(),
            engine,
            observed: observed.to_string(),
            issue: issue.to_string(),
        },
    )
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn cases() -> Vec<Case> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(corpus_dir())
        .expect("tests/engine_corpus")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| parse_case(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .collect()
}

/// A verdict line of `verify`: `Cone_P [P, Q]: ok (13 states, Cone_P_WF: ok, ...)`, from the
/// native checker or, after `tlc: `, from TLC.
#[derive(Clone, Debug)]
struct RunLine {
    module: String,
    checks: Vec<String>,
    verdict: V,
    /// The other runs the line names, `Cone_P_WF: ok`, by module.
    notes: BTreeMap<String, V>,
}

fn line_verdict(text: &str) -> Option<V> {
    Some(if text.starts_with("ok") {
        V::Ok
    } else if text.starts_with("counterexample") || text.starts_with("violated") {
        V::Counterexample
    } else if text.starts_with("not decided") {
        V::NotDecided
    } else {
        return None;
    })
}

fn parse_run_line(line: &str) -> Option<RunLine> {
    if !(line.starts_with("Cone_") || line.starts_with("RdraSpec_")) {
        return None;
    }
    let (module, rest) = line.split_once(" [")?;
    let (checks, rest) = rest.split_once("]: ")?;
    let verdict = line_verdict(rest)?;
    let notes = rest
        .split_once('(')
        .map(|(_, n)| n.trim_end_matches(')'))
        .unwrap_or("")
        .split(", ")
        .filter_map(|note| {
            let (m, v) = note.split_once(": ")?;
            (m.starts_with("Cone_") || m.starts_with("RdraSpec_"))
                .then(|| Some((m.to_string(), line_verdict(v)?)))
                .flatten()
        })
        .collect();
    Some(RunLine {
        module: module.to_string(),
        checks: checks.split(", ").map(str::to_string).collect(),
        verdict,
        notes,
    })
}

/// What one `verify` run printed, per engine.
#[derive(Default, Debug)]
struct Observed {
    native_lines: Vec<RunLine>,
    tlc_lines: Vec<RunLine>,
    /// `property P (BFS on its cone): ...` lines: `stable`, `unchanged`, `precedes`.
    order_lines: BTreeMap<String, V>,
    native_errors: BTreeSet<String>,
    tlc_errors: BTreeSet<String>,
    z3: BTreeMap<String, Z3>,
    /// `error[EngineDisagreement]` and `error[LassoFault]` lines.
    faults: Vec<String>,
    /// The `M` of `can_reach (BFS on N cones): N checked, M violated`.
    can_reach_violated: Option<usize>,
    /// Errors without a name: a load or build error.
    unnamed: Vec<String>,
    tlc_ran: bool,
}

impl Observed {
    fn parse(stdout: &str, stderr: &str) -> Observed {
        let mut out = Observed::default();
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("tlc: ") {
                out.tlc_ran = true;
                out.tlc_lines.extend(parse_run_line(rest));
            } else if let Some(run) = parse_run_line(line) {
                out.native_lines.push(run);
            } else if let Some(rest) = line.strip_prefix("property ") {
                if let Some((name, v)) = rest.split_once(" (BFS on its cone): ") {
                    out.order_lines
                        .insert(name.to_string(), line_verdict(v).unwrap_or(V::None));
                }
            } else if let Some(rest) = line.strip_prefix("can_reach (BFS on ") {
                out.can_reach_violated = rest
                    .rsplit(", ")
                    .next()
                    .and_then(|v| v.strip_suffix(" violated"))
                    .and_then(|n| n.parse().ok());
            } else if let Some(rest) = line.strip_prefix("proved: ") {
                out.z3.insert(rule_name(rest), Z3::Proved);
            } else if let Some(rest) = line.strip_prefix("unproved: ") {
                out.z3.insert(rule_name(rest), Z3::Unproved);
            } else if let Some(rest) = line.strip_prefix("out of scope: ") {
                out.z3.insert(rule_name(rest), Z3::OutOfScope);
            }
        }
        // An error block is its `error[Name]:` line and the indented lines under it; TLC's
        // counterexamples say `from TLC`.
        for text in [stdout, stderr] {
            let mut lines = text.lines().peekable();
            while let Some(line) = lines.next() {
                if line.starts_with("error: ") || line.starts_with("Error: model has errors") {
                    out.unnamed.push(line.to_string());
                    continue;
                }
                let Some(rest) = line.strip_prefix("error[") else {
                    continue;
                };
                let name = rest.split(']').next().unwrap_or("").to_string();
                let mut from_tlc = false;
                while let Some(next) = lines.peek() {
                    if !next.starts_with(' ') {
                        break;
                    }
                    from_tlc |= next.contains(", from TLC)");
                    lines.next();
                }
                if name == "EngineDisagreement" || name == "LassoFault" {
                    if !out.faults.iter().any(|f| f == line) {
                        out.faults.push(line.to_string());
                    }
                } else if from_tlc {
                    out.tlc_errors.insert(name);
                } else {
                    out.native_errors.insert(name);
                }
            }
        }
        out
    }

    /// Every check the run names.
    fn checks(&self) -> BTreeSet<String> {
        self.native_lines
            .iter()
            .chain(&self.tlc_lines)
            .flat_map(|l| l.checks.iter().cloned())
            .chain(self.order_lines.keys().cloned())
            .chain(self.native_errors.iter().cloned())
            .chain(self.tlc_errors.iter().cloned())
            .chain(self.z3.keys().cloned())
            .collect()
    }

    /// `check`'s verdict from one side: a counterexample when the side printed its error,
    /// otherwise the whole-model line's verdict, else the cone line's, where a counterexample
    /// of a cone that holds several checks breaks only the ones it names.
    fn verdict(&self, tlc: bool, check: &str, can_reach: bool) -> V {
        let (lines, errors) = match tlc {
            true => (&self.tlc_lines, &self.tlc_errors),
            false => (&self.native_lines, &self.native_errors),
        };
        if errors.contains(check) {
            return V::Counterexample;
        }
        if !tlc {
            if let Some(v) = self.order_lines.get(check) {
                return *v;
            }
        }
        let mine = |l: &&RunLine| l.checks.iter().any(|c| c == check);
        let line = lines
            .iter()
            .filter(mine)
            .find(|l| l.module.starts_with("RdraSpec_"))
            .or_else(|| lines.iter().find(mine));
        match line.map(|l| l.verdict) {
            Some(V::Counterexample) => V::Ok,
            Some(v) => v,
            // A `can_reach` prints a line only when it fails.
            None if can_reach && !tlc => V::Ok,
            None => V::None,
        }
    }
}

/// The rule's name after `proved: ` and the like: `lemma X [note]: why` names `X`.
fn rule_name(rest: &str) -> String {
    let rest = rest.strip_prefix("lemma ").unwrap_or(rest);
    rest.split([' ', ':']).next().unwrap_or("").to_string()
}

struct Tools {
    tlc: bool,
    z3: bool,
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

fn tools() -> Tools {
    let require = |var: &str, found: bool, what: &str| {
        if !found {
            assert!(
                std::env::var(var).as_deref() != Ok("1"),
                "{var}=1 but {what} is not on PATH (run inside `nix develop`)"
            );
            println!("{what} not on PATH: its verdicts are not compared");
        }
        found
    };
    Tools {
        tlc: require("RDRA_REQUIRE_TLC", on_path("tlc") || on_path("tlc2"), "tlc"),
        z3: require(
            "RDRA_REQUIRE_Z3",
            std::env::var_os("RDRA_Z3").is_some() || on_path("z3"),
            "z3",
        ),
    }
}

fn verify(case: &Case, extra: &[&str]) -> Observed {
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("verify")
        .arg(&case.path)
        .args(extra)
        .args(&case.flags)
        .output()
        .expect("rdra-ish runs");
    Observed::parse(
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
    )
}

/// Everything one case found wrong, and the known disagreements it reproduced.
#[derive(Default)]
struct Report {
    failures: Vec<String>,
    known: Vec<String>,
}

fn check_case(case: &Case, tools: &Tools) -> Report {
    let mut report = Report::default();
    let run_tlc = tools.tlc && !case.tlc_skip;
    let mut args = vec![];
    if run_tlc {
        args.push("--with-tlc");
    }
    if tools.z3 {
        args.push("--prove");
    }
    let main = verify(case, &args);
    let needs_bounds = case.expects.values().any(|e| e.bfs.is_some());
    let bounded: BTreeMap<u8, Observed> = match needs_bounds {
        true => [1u8, 3]
            .into_iter()
            .map(|n| (n, verify(case, &["--instance-count", &n.to_string()])))
            .collect(),
        false => BTreeMap::new(),
    };
    let fail = |report: &mut Report, msg: String| report.failures.push(msg);

    for line in &main.unnamed {
        fail(&mut report, format!("an error without a name: {line}"));
    }
    // Every check the run names is stated, and every stated check is in the run.
    let seen = main.checks();
    for check in seen.difference(&case.expects.keys().cloned().collect()) {
        fail(
            &mut report,
            format!("{check} is in the run but has no `expect` line"),
        );
    }
    for (check, e) in &case.expects {
        let absent = e.native == V::None || case.known.iter().any(|k| k.check == *check);
        if !seen.contains(check) && !case.can_reach.contains(check) && !absent {
            fail(
                &mut report,
                format!("{check} is stated but neither in the run nor a `can_reach` of the model"),
            );
        }
    }
    // A `can_reach` prints no line of its own when it holds, so the summary's count of
    // violations must be the count of `can_reach` errors.
    if let Some(violated) = main.can_reach_violated {
        let errors = case
            .can_reach
            .iter()
            .filter(|c| main.native_errors.contains(*c))
            .count();
        if violated != errors {
            fail(&mut report, format!("the can_reach summary counts {violated} violated, the run prints {errors} errors"));
        }
    } else if !case.can_reach.is_empty() {
        fail(
            &mut report,
            "the model has a can_reach and the run prints no can_reach summary".into(),
        );
    }

    // Each observation, against the header and the `known` lines.
    let judge = |report: &mut Report, check: &str, engine: Engine, want: String, got: String| {
        let known = case
            .known
            .iter()
            .find(|k| k.check == check && k.engine == engine);
        match (want == got, known) {
            (true, None) => {}
            (false, None) => report.failures.push(format!(
                "{check}: {engine} gives {got}, the header states {want}"
            )),
            (false, Some(k)) if k.observed == got => report.known.push(format!(
                "{check}: {engine} gives {got}, should be {want} ({})",
                k.issue
            )),
            (false, Some(k)) => report.failures.push(format!(
                "{check}: {engine} gives {got}, the header states {want}, and `known` records {}",
                k.observed
            )),
            (true, Some(k)) => report.failures.push(format!(
                "{check}: {engine} now gives {want}, as the header states; the known disagreement ({}) is gone, so remove its `known` line",
                k.issue
            )),
        }
    };
    let can_reach = |check: &str| case.can_reach.contains(check);
    for (check, e) in &case.expects {
        let native = main.verdict(false, check, can_reach(check));
        judge(
            &mut report,
            check,
            Engine::Native,
            e.native.to_string(),
            native.to_string(),
        );
        let tlc = main.verdict(true, check, false);
        if run_tlc {
            judge(
                &mut report,
                check,
                Engine::Tlc,
                e.tlc.to_string(),
                tlc.to_string(),
            );
            // Rule 1: both engines decided, so they agree.
            if native.decided() && tlc.decided() && native != tlc {
                fail(&mut report, format!("{check}: native {native}, TLC {tlc}"));
            }
        }
        let z3 = main.z3.get(check).copied();
        if tools.z3 {
            let want = e.z3.map_or("none".to_string(), |z| z.to_string());
            let got = z3.map_or("none".to_string(), |z| z.to_string());
            judge(&mut report, check, Engine::Z3, want, got);
        }
        let mut at_bounds = vec![(2u8, native)];
        if let Some(bfs) = e.bfs {
            for (n, run) in &bounded {
                let got = run.verdict(false, check, can_reach(check));
                judge(
                    &mut report,
                    check,
                    Engine::Bfs(*n),
                    bfs[*n as usize - 1].to_string(),
                    got.to_string(),
                );
                at_bounds.push((*n, got));
            }
        }
        // Rule 3: a rule z3 proves for any number of rows has no counterexample at any bound.
        if z3 == Some(Z3::Proved) {
            for (n, v) in &at_bounds {
                if *v == V::Counterexample {
                    fail(
                        &mut report,
                        format!(
                            "{check}: z3 proves it, BFS at {n} instances finds a counterexample"
                        ),
                    );
                }
            }
        }
    }
    // Rule 2: per run. Each module both engines ran, and each run a line names, has the same
    // verdict where both decided it.
    for mine in &main.native_lines {
        let Some(theirs) = main.tlc_lines.iter().find(|l| l.module == mine.module) else {
            continue;
        };
        let mut runs = vec![(mine.module.clone(), mine.verdict, theirs.verdict)];
        for (m, v) in &mine.notes {
            if let Some(t) = theirs.notes.get(m) {
                runs.push((m.clone(), *v, *t));
            }
        }
        for (module, a, b) in runs {
            if a.decided() && b.decided() && a != b {
                fail(&mut report, format!("{module}: native {a}, TLC {b}"));
            }
        }
    }
    // Rule 4: no engine disagreement and no counterexample that fails its own check, unless
    // a `known` line covers a check the fault line names.
    for line in main
        .faults
        .iter()
        .chain(bounded.values().flat_map(|o| &o.faults))
    {
        let covered = case.known.iter().any(|k| {
            line.contains(&format!("[{}]", k.check)) || line.contains(&format!("{}, ", k.check))
        });
        match covered {
            true => report.known.push(line.clone()),
            false => fail(&mut report, line.clone()),
        }
    }
    report
}

/// Every case: its header holds for every engine, and the engines agree by the rules of
/// `docs/engine-agreement.md`. The prefix keeps it out of the debug `test` step of CI,
/// which skips `bfs_and_tlc_`; the engine-equivalence job runs it in release.
#[test]
fn bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus() {
    let tools = tools();
    let cases = cases();
    let next = AtomicUsize::new(0);
    let reports: Mutex<Vec<(String, Report)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(case) = cases.get(i) else { break };
                let report = check_case(case, &tools);
                reports.lock().unwrap().push((case.name.clone(), report));
            });
        }
    });
    let mut reports = reports.into_inner().unwrap();
    reports.sort_by(|a, b| a.0.cmp(&b.0));
    let mut failed = 0;
    for (name, report) in &reports {
        for k in &report.known {
            println!("known    {name}: {k}");
        }
        for f in &report.failures {
            println!("FAIL     {name}: {f}");
        }
        if report.failures.is_empty() {
            println!("ok       {name}");
        }
        failed += usize::from(!report.failures.is_empty());
    }
    println!("engine corpus: {} cases, {failed} failed", reports.len());
    assert_eq!(
        failed, 0,
        "{failed} corpus case(s) failed; see the FAIL lines above"
    );
}

/// Every header parses, names its trap, and states a verdict for each check. Cheap, so it
/// runs in the debug `test` step as well.
#[test]
fn every_corpus_header_is_well_formed() {
    let cases = cases();
    assert!(cases.len() >= 30, "{} cases", cases.len());
}

/// A graph a cap stopped partway decides nothing the full graph contradicts: on the whole
/// model of every case, each `always`, `eventually`, and `leads_to` explored under every
/// state cap from 1 up to its size is either undecided or has the full graph's
/// counterexample, under both fairness readings, and never gives `ok`.
#[test]
fn a_capped_graph_decides_nothing_the_full_graph_contradicts() {
    for case in cases() {
        let project = load_project(std::slice::from_ref(&case.path), None)
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let ts = TransitionSystem::build(&project.model, case.bounds(), &Scope::default()).ts;
        let full = ts.clone().run(Caps::new(Caps::DEFAULT_PATTERNS, 1 << 20));
        assert!(
            full.complete,
            "{}: the whole model passes 2^20 states",
            case.name
        );
        let n = full.states.len();
        let caps: BTreeSet<usize> = (1..n.min(64))
            .chain((0..12).map(|k| n * k / 12).filter(|&c| c > 0))
            .filter(|&c| c < n)
            .collect();
        let decide = |run: &rdra_ish_core::lifecycle::ComponentRun, i: usize, f: Fairness| match ts
            .properties()[i]
            .formula
            .kind()
            .is_liveness()
        {
            true => run.decide_liveness(i, f).unwrap_or_else(|fault| {
                panic!(
                    "{}: property {i}: a lasso fails its own check: {fault:?}",
                    case.name
                )
            }),
            false => run.decide_property(i).expect("an always"),
        };
        let verdicts: Vec<usize> = (0..ts.properties().len())
            .filter(|&i| {
                use rdra_ish_core::lifecycle::LtlKind::*;
                matches!(
                    ts.properties()[i].formula.kind(),
                    Always | Eventually | LeadsTo
                )
            })
            .collect();
        for fairness in [Fairness::Plain, Fairness::WfNext] {
            let whole: Vec<bool> = verdicts
                .iter()
                .map(|&i| matches!(decide(&full, i, fairness), PropertyVerdict::Violated(_)))
                .collect();
            for &cap in &caps {
                let run = ts.clone().run(Caps::new(Caps::DEFAULT_PATTERNS, cap));
                if run.complete {
                    continue;
                }
                for (k, &i) in verdicts.iter().enumerate() {
                    let name = &ts.properties()[i].name;
                    match decide(&run, i, fairness) {
                        PropertyVerdict::Holds => panic!(
                            "{}: {name} is ok on a graph capped at {cap} of {n} states ({fairness:?})",
                            case.name
                        ),
                        PropertyVerdict::Violated(_) => assert!(
                            whole[k],
                            "{}: {name} has a counterexample at a cap of {cap} states, and the full graph of {n} has none ({fairness:?})",
                            case.name
                        ),
                        PropertyVerdict::Undecided => {}
                    }
                }
            }
        }
    }
}
