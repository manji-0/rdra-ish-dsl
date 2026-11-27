//! The witness of the group rules and reasons that have no witness of their own
//! (docs/design/instance-symmetry.md, "Always by default", the per-rule gate): a snapshot
//! that pins, for every cone of the samples and the engine corpus, which entities the group
//! the cone runs up to (`G`, or `G_T` on a #74-reduced cone) leaves where they are and why,
//! which entities `H` leaves where they are on the sub-lifecycle and why, and which scan rule
//! leaves an entity out. A changed or dropped rule changes the snapshot, and the test fails.
//!
//! Nothing here runs a cone: the reduction runs S plainly to build its classes, and the rest
//! reads the systems. Each model runs at the bounds of its `// expect verify` headers (a
//! sample) or its `// flags:` line, and at `--instance-count 3` too where the header names
//! BFS verdicts per bound (a corpus case), as `verify-samples.sh` and engine_corpus run them.
//! The system is built with `Scope::ALL`, so a header line scoped by `--entity` gives a
//! superset of the cones it runs.
//!
//! After a deliberate change, rewrite the snapshot and read its diff:
//! `INSTA_UPDATE=always cargo test --release -p rdra-ish-core --lib symmetry::snapshot`.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};

use crate::lifecycle::explore::Caps;
use crate::lifecycle::types::EntId;
use crate::lifecycle::{Bounds, Cone, QuotientCache, Scope, Seed, Target, TransitionSystem};
use crate::load_project;

use super::witnesses::dropping;
use super::{Group, Rule};

/// The state cap of each run of S, which `--max-states` replaces. `verify` has none and stops
/// a run at the memory limit, past which `--symmetry always` runs S up to `H`; here a cone
/// whose plain run of S passes the cap is reduced with S up to `H` (`Ended::UpToH`). Two
/// sub-lifecycles of the samples pass it, and with no cap the test passed 40 GB.
const S_CAP: usize = 2_000_000;

/// The rules and reasons the owner kept on 2026-11-25 with no witness of their own: this
/// snapshot is theirs.
const KEPT: [&str; 11] = [
    "PinnedStart",
    "Reduced:S var",
    "Reduced:input",
    "Reduced:named by S",
    "Reduced:named by an input",
    "Reduced:bit key",
    "Reduced:S action param",
    "Observed:named by obs",
    "Observed:named by an input",
    "Observed:stub",
    "Observed:named by T",
];

const SCAN: [Rule; 9] = [
    Rule::OrderedId,
    Rule::IdAgainstLiteral,
    Rule::OwnerLiteral,
    Rule::OwnerChoice,
    Rule::PinnedStart,
    Rule::UnlinkedCreate,
    Rule::AbsentOwner,
    Rule::MixedIds,
    Rule::JointTooLarge,
];

/// `Reduction::fixed_why`'s reasons.
const G_T: [&str; 7] = [
    "S",
    "S var",
    "input",
    "named by S",
    "named by an input",
    "bit key",
    "S action param",
];

/// `Extended::fixed_why`'s reasons.
const H: [&str; 6] = [
    "obs",
    "input",
    "named by obs",
    "named by an input",
    "stub",
    "named by T",
];

/// Every rule and reason by the name `dropped` reads.
fn names() -> Vec<String> {
    SCAN.iter()
        .map(|r| format!("{r:?}"))
        .chain(G_T.iter().map(|w| format!("Reduced:{w}")))
        .chain(H.iter().map(|w| format!("Observed:{w}")))
        .collect()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The flags that change a cone's system: the bounds, and the state cap of the run of S.
/// The others leave the system alone (`--prove`, `--deny-undecided`, ...), scope its targets
/// (`--entity`, which `Scope::ALL` covers), or set the memory limit (`--memory`), which this
/// run of S does not have.
const READ: [&str; 4] = [
    "--instance-count",
    "--int-range-max",
    "--instances",
    "--max-states",
];

/// `flags` with only the ones of `READ`, each with its value.
fn read_flags(flags: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, f) in flags.iter().enumerate() {
        if READ.contains(f) {
            out.push(format!("{f} {}", flags[i + 1]));
        }
    }
    out
}

fn bounds_and_caps(flags: &[String]) -> (Bounds, Caps) {
    let mut bounds = Bounds::VERIFY;
    let mut cap = S_CAP;
    for f in flags {
        let (name, value) = f.split_once(' ').expect("a flag and its value");
        match name {
            "--instance-count" => {
                bounds.instances = NonZeroU8::new(value.parse().unwrap()).unwrap();
            }
            "--int-range-max" => bounds.int_max = value.parse().unwrap(),
            "--instances" => {
                let (e, n) = value.split_once('=').expect("<Entity>=<n>");
                let n = NonZeroU8::new(n.parse().unwrap()).unwrap();
                bounds.per_entity.insert(e.to_string(), n);
            }
            "--max-states" => cap = value.parse().unwrap(),
            _ => unreachable!("{name} is not read"),
        }
    }
    (bounds, Caps::new(usize::MAX, cap))
}

/// A model and the flag sets it runs at.
struct Case {
    name: String,
    path: PathBuf,
    runs: Vec<Vec<String>>,
}

/// The sample roots of samples.txt, each at the flags of its `// expect verify` lines.
fn samples() -> Vec<Case> {
    let root = repo_root();
    let list = std::fs::read_to_string(root.join("samples.txt")).expect("samples.txt");
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|rel| {
            let path = root.join(rel);
            let mut runs: Vec<Vec<String>> = Vec::new();
            for file in walkdir::WalkDir::new(&path).sort_by_file_name() {
                let file = file.expect("a sample file");
                if file.path().extension().is_none_or(|x| x != "rdra") {
                    continue;
                }
                let text = std::fs::read_to_string(file.path()).expect("readable");
                for line in text.lines() {
                    let Some(rest) = line.strip_prefix("// expect verify") else {
                        continue;
                    };
                    let (flags, _) = rest.split_once(':').expect("// expect verify <flags>: ...");
                    let flags: Vec<&str> = flags.split_whitespace().collect();
                    let run = read_flags(&flags);
                    if !runs.contains(&run) {
                        runs.push(run);
                    }
                }
            }
            if runs.is_empty() {
                runs.push(Vec::new());
            }
            Case {
                name: rel.to_string(),
                path,
                runs,
            }
        })
        .collect()
}

/// The engine corpus, each case at its `// flags:` line, and at `--instance-count 3` too
/// when an `expect` line names BFS verdicts per bound.
fn corpus() -> Vec<Case> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../rdra-ish-cli/tests/engine_corpus");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the engine corpus")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("readable");
            let header: Vec<&str> = text.lines().map_while(|l| l.strip_prefix("//")).collect();
            let flags: Vec<&str> = header
                .iter()
                .find_map(|l| l.trim().strip_prefix("flags: "))
                .map(|f| f.split_whitespace().collect())
                .unwrap_or_default();
            let main = read_flags(&flags);
            let mut runs = vec![main.clone()];
            let per_bound = header
                .iter()
                .any(|l| l.trim().starts_with("expect ") && l.contains(" bfs="));
            if per_bound {
                let mut three: Vec<String> = main
                    .into_iter()
                    .filter(|f| !f.starts_with("--instance-count "))
                    .collect();
                three.insert(0, "--instance-count 3".to_string());
                runs.push(three);
            }
            Case {
                name: format!(
                    "engine_corpus/{}",
                    path.file_name().unwrap().to_string_lossy()
                ),
                path,
                runs,
            }
        })
        .collect()
}

/// One run of a model: its cones; `Err` for a model that does not load.
struct Run {
    title: String,
    cones: Result<Vec<ConeSystem>, String>,
}

/// A cone reduced as `verify` reduces it, by the targets and properties that share it.
struct ConeSystem {
    names: Vec<String>,
    system: TransitionSystem,
    /// How the runs of the reduction ended.
    ended: Ended,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ended {
    /// No run of the reduction stopped.
    Clean,
    /// A plain run of S stopped at the cap, and the reduction ran S up to `H` instead, as
    /// `verify --symmetry always` does when a plain run of S stops at the memory limit; no
    /// run of that one stopped.
    UpToH,
    /// A run stopped at the cap up to `H` too.
    Stopped,
}

/// The cones `verify` runs, reduced through one quotient cache: the safety cones, one per
/// set of vars as `cone_jobs` groups them, the entity cones, and the cones of the
/// properties that are not decided on an explored graph. A system two cones share, reduced
/// by runs that ended alike, is listed once, under every name.
fn cones(ts: &TransitionSystem, caps: Caps) -> Vec<ConeSystem> {
    let cache = QuotientCache::new();
    // A safety cone takes in each check with its vars, keeps and all; any other is one
    // cone per target.
    let mut jobs: Vec<(Vec<String>, Cone, Vec<Seed>, bool)> = Vec::new();
    let mut add = |name: String, cone: Cone, seed: Seed, safety: bool| {
        let at = jobs.iter().position(|(_, c, _, s)| {
            *s == safety && c.vars == cone.vars && (safety || c.keep == cone.keep)
        });
        match at {
            Some(at) => {
                jobs[at].0.push(name);
                jobs[at].1.keep.extend(cone.keep);
                jobs[at].2.push(seed);
            }
            None => jobs.push((vec![name], cone, vec![seed], safety)),
        }
    };
    for target in ts.targets() {
        let (name, safety) = match target {
            Target::Invariant(i) => (ts.invariants()[i].name.clone(), true),
            Target::AfterAssert(i) => (ts.after_asserts()[i].name.clone(), true),
            Target::Entity(e) => (format!("entity {}", ts.entity(e).id), false),
            Target::Reach(_) => continue,
        };
        add(name, ts.target_cone(target), ts.target_seed(target), safety);
    }
    for (i, p) in ts.properties().iter().enumerate() {
        if !p.formula.kind().is_explored() {
            add(
                format!("property {}", p.name),
                ts.property_cone(i),
                ts.property_seed(i),
                false,
            );
        }
    }
    let mut out: Vec<ConeSystem> = Vec::new();
    for (names, cone, seeds, _) in jobs {
        // `reduced_cone_in`, with how its runs ended.
        let (sys, local) = ts.cone_system(&cone, seeds);
        let (system, ended) = match sys.reduced_in(&local, caps, &cache) {
            (plain, true) => (plain, Ended::Clean),
            (_, false) => match sys.reduced_in(&local, caps.forcing_s_up_to_h(), &cache) {
                (up_to_h, true) => (up_to_h, Ended::UpToH),
                (up_to_h, false) => (up_to_h, Ended::Stopped),
            },
        };
        match out
            .iter_mut()
            .find(|c| c.system == system && c.ended == ended)
        {
            Some(shared) => shared.names.extend(names),
            None => out.push(ConeSystem {
                names,
                system,
                ended,
            }),
        }
    }
    out
}

fn runs() -> Vec<Run> {
    let cases: Vec<Case> = samples().into_iter().chain(corpus()).collect();
    let jobs: Vec<(String, PathBuf, Vec<String>)> = cases
        .into_iter()
        .flat_map(|c| {
            c.runs
                .into_iter()
                .map(move |flags| (c.name.clone(), c.path.clone(), flags))
        })
        .collect();
    let run = |(name, path, flags): &(String, PathBuf, Vec<String>)| {
        let title = match flags.is_empty() {
            true => name.clone(),
            false => format!("{name} {}", flags.join(" ")),
        };
        let cones = load_project(std::slice::from_ref(path), None)
            .map_err(|e| e.to_string())
            .and_then(|p| match p.diags.iter().any(|d| !d.is_warning) {
                true => Err("the model has errors".to_string()),
                false => Ok(p.model),
            })
            .map(|model| {
                let (bounds, caps) = bounds_and_caps(flags);
                let ts = TransitionSystem::build(&model, bounds, &Scope::ALL).ts;
                cones(&ts, caps)
            });
        Run { title, cones }
    };
    // The models reduce side by side, on threads that drop no rule.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let mut done: Vec<(usize, Run)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(job) = jobs.get(at) else {
                            return mine;
                        };
                        mine.push((at, run(job)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, run)| run).collect()
}

/// Per entity id, the scan rules `g` leaves it out for.
fn scan_rules(g: &Group) -> BTreeMap<&str, Vec<Rule>> {
    let mut out: BTreeMap<&str, Vec<Rule>> = BTreeMap::new();
    for l in g.left_out() {
        if SCAN.contains(&l.rule) {
            out.entry(l.entity.as_str()).or_default().push(l.rule);
        }
    }
    out
}

/// What the group `g` of `ts` does to `e`: whether it moves, the scan rules, and the reasons
/// (`label=` the reasons), each counted in `seen` under `prefix`.
fn part(
    ts: &TransitionSystem,
    g: &Group,
    e: EntId,
    why: Option<&Vec<&'static str>>,
    (label, prefix): (&str, &str),
    seen: &mut BTreeMap<String, usize>,
) -> String {
    let mut out = String::from(match g.moves(e) {
        true => "moves",
        false => "stays",
    });
    if let Some(rules) = scan_rules(g).get(ts.entity(e).id.as_str()) {
        let rules: Vec<String> = rules.iter().map(|r| format!("{r:?}")).collect();
        for r in &rules {
            *seen.entry(r.clone()).or_default() += 1;
        }
        write!(out, " scan={}", rules.join(",")).unwrap();
    }
    if let Some(why) = why {
        for w in why {
            *seen.entry(format!("{prefix}:{w}")).or_default() += 1;
        }
        write!(out, " {label}={}", why.join(",")).unwrap();
    }
    out
}

/// The snapshot of `runs` under the rules dropped on this thread, and per rule and reason
/// how many entity lines name it.
fn render(runs: &[Run]) -> (String, BTreeMap<String, usize>) {
    let mut seen: BTreeMap<String, usize> = names().into_iter().map(|n| (n, 0)).collect();
    let mut body = String::new();
    for run in runs {
        writeln!(body, "== {}", run.title).unwrap();
        let cones = match &run.cones {
            Ok(cones) => cones,
            Err(e) => {
                writeln!(body, "  does not load: {e}").unwrap();
                continue;
            }
        };
        for cone in cones {
            let ts = &cone.system;
            let g = Group::of(ts);
            let reduced = ts.reduction();
            let gt = reduced.map(|r| r.fixed_why(ts)).unwrap_or_default();
            let h = reduced.map(|r| {
                let (depth, sub, why) = r.sub_lifecycle_up_to_h(ts);
                let fixed: Vec<EntId> = why.keys().copied().collect();
                let group = Group::fixing(&sub, &fixed);
                (depth, sub, why, group)
            });
            let state = match &h {
                Some((depth, ..)) => format!("reduced, H at depth {depth}"),
                None => "unreduced".to_string(),
            };
            let ended = match cone.ended {
                Ended::Clean => "",
                Ended::UpToH => ", S past the cap plainly and run up to H",
                Ended::Stopped => ", S past the cap plainly and up to H",
            };
            writeln!(body, "  [{}] {state}{ended}", cone.names.join(", ")).unwrap();
            if let Some((_, sub, ..)) = &h {
                for he in (0..sub.entities().len()).map(EntId::at) {
                    let id = &sub.entity(he).id;
                    assert!(
                        ts.entities().iter().any(|d| d.id == *id),
                        "the sub-lifecycle's {id} is an entity of its cone"
                    );
                }
            }
            for e in (0..ts.entities().len()).map(EntId::at) {
                let id = &ts.entity(e).id;
                let mut line = format!("    {id}[{}] ", ts.slots(e));
                line += &part(ts, &g, e, gt.get(&e), ("G_T", "Reduced"), &mut seen);
                if let Some((_, sub, why, group)) = &h {
                    if let Some(he) = (0..sub.entities().len())
                        .map(EntId::at)
                        .find(|x| sub.entity(*x).id == *id)
                    {
                        line += " | H ";
                        line += &part(sub, group, he, why.get(&he), ("H", "Observed"), &mut seen);
                    }
                }
                writeln!(body, "{line}").unwrap();
            }
        }
    }
    let mut text = String::new();
    writeln!(
        text,
        "The group rules and reasons, per cone (docs/design/instance-symmetry.md, the per-rule gate)."
    )
    .unwrap();
    writeln!(
        text,
        "A line: entity[slots], then G (G_T on a reduced cone): moves or stays, scan=<the scan \
         rules>, G_T=<the reasons>; then, on a reduced cone, H on the sub-lifecycle: the same, \
         H=<the reasons>."
    )
    .unwrap();
    writeln!(
        text,
        "Flags read: {}. S runs plainly up to {S_CAP} states (or --max-states) with no memory \
         limit, and past them up to H, as under --symmetry always at the memory limit. The \
         models build with Scope::ALL, a superset of the cones of an --entity line.",
        READ.join(", ")
    )
    .unwrap();
    writeln!(text, "Entity lines that name each rule or reason:").unwrap();
    for (name, n) in &seen {
        let kept = match KEPT.contains(&name.as_str()) {
            true => " (kept without a witness of its own)",
            false => "",
        };
        let none = match n {
            0 => ": fires in no cone",
            _ => "",
        };
        writeln!(text, "  {name}: {n}{none}{kept}").unwrap();
    }
    text.push('\n');
    text.push_str(&body);
    (text, seen)
}

/// The snapshot holds, and dropping any one rule or reason changes it, unless that rule
/// fires in no cone, which the snapshot's header says.
#[test]
fn the_group_rules_of_every_cone_are_the_snapshot() {
    let runs = runs();
    let (text, seen) = render(&runs);
    for name in names() {
        let (dropped, _) = dropping(&name, || render(&runs));
        match seen[&name] {
            0 => assert_eq!(dropped, text, "{name} fires in no cone"),
            _ => assert_ne!(dropped, text, "dropping {name} changes nothing"),
        }
    }
    insta::assert_snapshot!("group_rules", text);
}
