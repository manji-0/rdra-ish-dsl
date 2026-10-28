//! The explorer and TLC agree on every sample: for each cone `explore_targets` explores, the
//! states TLC reaches on the printed cone (`tlc -dump`) are the states the BFS reached, and
//! the invariants and after-asserts TLC finds violated are the ones the BFS found violated.
//!
//! TLC stops at the first violation unless run with `-continue`, which explores the whole
//! state space and reports every violated invariant and action property. The verdict run
//! uses `-continue` on the Check-mode cfg minus temporal properties (excluded from the
//! comparison), and its distinct-state count must equal the dump's, so it is known to have
//! finished.
//!
//! Without `tlc` on PATH the TLC test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure.

#[path = "../src/tlc_output.rs"]
#[allow(dead_code)]
mod tlc_output;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rdra_ish_core::lifecycle::{Bounds, Caps, ComponentRun, Scope, StateDiag, TransitionSystem};
use rdra_ish_core::{build_merged_model, resolve, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use tlc_output::{parse_dump, CanonState, Outcome, TlcRun};

/// Far above the largest cone of any sample (clinic-ops peaks at a few hundred states).
const STATE_CAP: usize = 200_000;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn bounds_for(sample: &str) -> Vec<Bounds> {
    if sample.starts_with("samples/formal-verification") {
        vec![Bounds::STATES, Bounds::VERIFY]
    } else {
        vec![Bounds::STATES]
    }
}

fn rdra_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .expect("readable sample dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rdra_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rdra") {
            out.push(path);
        }
    }
}

/// The CLI's include rule: each file's parent and grandparent.
fn load(sample: &str) -> SemanticModel {
    let mut files = Vec::new();
    rdra_files(&repo().join(sample), &mut files);
    let mut includes: Vec<PathBuf> = Vec::new();
    for f in &files {
        let canon = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        if let Some(parent) = canon.parent() {
            for a in [parent, parent.parent().unwrap_or(parent)] {
                if !includes.iter().any(|p| p == a) {
                    includes.push(a.to_path_buf());
                }
            }
        }
    }
    let (program, _) = resolve(&files, &includes);
    build_merged_model(&program, &includes).0
}

fn find_tlc() -> Option<PathBuf> {
    let out = Command::new("which").arg("tlc").output().ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

/// Invariants and after-asserts the BFS found violated, by TLA name.
fn bfs_verdicts(run: &ComponentRun, names: &TlaNames) -> BTreeSet<String> {
    let ts = &run.system;
    let mut out = BTreeSet::new();
    for d in &run.diags {
        match &d.diag {
            StateDiag::ConstraintViolated { name, .. } => {
                let i = ts
                    .invariants()
                    .iter()
                    .position(|inv| inv.name == *name)
                    .expect("violated invariant is in the cone");
                out.insert(names.invariant(i).to_string());
            }
            StateDiag::AfterAssertViolated { site, .. } => {
                let i = ts
                    .after_asserts()
                    .iter()
                    .position(|a| a.site == *site)
                    .expect("violated assert is in the cone");
                out.insert(names.after(i).to_string());
            }
            _ => {}
        }
    }
    out
}

struct ConeReport {
    states: usize,
    violated: BTreeSet<String>,
}

fn run_tlc(tlc: &Path, dir: &Path, args: &[&str], tla: &str, cfg: &str) -> (bool, String) {
    std::fs::create_dir_all(dir).expect("work dir");
    std::fs::write(dir.join("Cone.tla"), tla).expect("write tla");
    std::fs::write(dir.join("Cone.cfg"), cfg).expect("write cfg");
    // TLC unpacks its standard modules into java.io.tmpdir; parallel runs must not share it.
    let tmp = dir.join("java-tmp");
    std::fs::create_dir_all(&tmp).expect("java tmp dir");
    let out = Command::new(tlc)
        .env(
            "JAVA_TOOL_OPTIONS",
            format!("-Djava.io.tmpdir={}", tmp.display()),
        )
        .args(["-workers", "1", "-metadir", "meta"])
        .args(args)
        .args(["-config", "Cone.cfg", "Cone.tla"])
        .current_dir(dir)
        .output()
        .expect("run tlc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn check_cone(tlc: &Path, dir: &Path, label: &str, run: &ComponentRun) -> ConeReport {
    let ts = &run.system;
    assert!(run.complete, "{label}: BFS hit a cap");
    let explore = tla::print(ts, PrintMode::Explore, "Cone");
    let names = &explore.names;
    let bfs: BTreeSet<CanonState> = run.states.iter().map(|s| names.canon(ts, s)).collect();

    let (ok, log) = run_tlc(tlc, dir, &["-dump", "states"], &explore.tla, &explore.cfg);
    let outcome = TlcRun::parse(&log).outcome(ok);
    assert_eq!(
        outcome,
        Outcome::Ok,
        "{label}: TLC explore run failed\n{log}"
    );
    let dump = std::fs::read_to_string(dir.join("states.dump")).expect("states.dump");
    let dumped = parse_dump(&dump);
    let tlc_states: BTreeSet<CanonState> = dumped.iter().cloned().collect();
    assert_eq!(
        dumped.len(),
        tlc_states.len(),
        "{label}: duplicate dumped state"
    );
    if tlc_states != bfs {
        let only_tlc: Vec<_> = tlc_states.difference(&bfs).take(3).collect();
        let only_bfs: Vec<_> = bfs.difference(&tlc_states).take(3).collect();
        panic!(
            "{label}: reachable sets differ (TLC {}, BFS {})\nonly TLC: {only_tlc:#?}\nonly BFS: {only_bfs:#?}\n{}",
            tlc_states.len(),
            bfs.len(),
            explore.tla
        );
    }

    let check = tla::print(ts, PrintMode::Check, "Cone");
    let temporal: BTreeSet<String> = (0..ts.properties().len())
        .map(|i| format!("PROPERTY {}", names.property(i)))
        .collect();
    let cfg: String = check
        .cfg
        .lines()
        .filter(|l| !temporal.contains(*l))
        .map(|l| format!("{l}\n"))
        .collect();
    let (_, log) = run_tlc(tlc, dir, &["-continue"], &check.tla, &cfg);
    let parsed = TlcRun::parse(&log);
    assert!(
        parsed.completed,
        "{label}: TLC check run did not finish\n{log}"
    );
    assert_eq!(
        parsed.distinct_states,
        Some(bfs.len() as u64),
        "{label}: TLC check run explored a different state count\n{log}"
    );
    let violated = parsed.violated_names();
    assert!(
        !violated.contains("TypeOK"),
        "{label}: TypeOK violated\n{log}"
    );
    let expected = bfs_verdicts(run, names);
    assert_eq!(
        violated, expected,
        "{label}: verdicts differ\n{}",
        check.tla
    );
    ConeReport {
        states: bfs.len(),
        violated,
    }
}

#[test]
fn bfs_and_tlc_reach_the_same_states_and_verdicts() {
    let Some(tlc) = find_tlc() else {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped engine equivalence: {reason}");
        return;
    };
    let scratch = std::env::temp_dir().join(format!("rdra-equivalence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    for sample in samples() {
        let model = load(&sample);
        for bounds in bounds_for(&sample) {
            let built = TransitionSystem::build(&model, bounds, &Scope::All);
            let exploration = built.ts.explore_targets(Caps::new(usize::MAX, STATE_CAP));
            let cones = &exploration.cones;
            let next = AtomicUsize::new(0);
            let reports: Mutex<Vec<(usize, ConeReport)>> = Mutex::new(Vec::new());
            std::thread::scope(|s| {
                for _ in 0..workers {
                    s.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        let Some(cone) = cones.get(i) else { break };
                        let label = format!("{sample} at {bounds:?}, cone {i}");
                        let dir = scratch.join(format!(
                            "{}-{}-{i}",
                            sample.replace('/', "_"),
                            bounds.instances
                        ));
                        let report = check_cone(&tlc, &dir, &label, &cone.run);
                        reports.lock().unwrap().push((i, report));
                    });
                }
            });
            let mut reports = reports.into_inner().unwrap();
            reports.sort_by_key(|(i, _)| *i);
            let states: usize = reports.iter().map(|(_, r)| r.states).sum();
            let violated: BTreeSet<&String> =
                reports.iter().flat_map(|(_, r)| &r.violated).collect();
            println!(
                "{sample} instances={}: {} cones, {states} states, violated {violated:?}",
                bounds.instances,
                reports.len()
            );
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Every slot of every var has a distinct key, and a reached state's canon form has exactly
/// those keys, so a dumped state and a BFS state cannot disagree on naming alone.
#[test]
fn tla_names_cover_every_slot() {
    for sample in samples() {
        let model = load(&sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let ts = TransitionSystem::build(&model, bounds, &Scope::All).ts;
            let bundle = tla::print(&ts, PrintMode::Explore, "M");
            let names = &bundle.names;
            let keys = names.slot_keys(&ts);
            let unique: BTreeSet<&String> = keys.iter().collect();
            assert_eq!(unique.len(), keys.len(), "{sample}: two slots share a key");
            let width: usize = ts
                .vars()
                .iter()
                .map(|d| match d.kind {
                    rdra_ish_core::lifecycle::VarKind::Now => 1,
                    _ => bounds.slots(),
                })
                .sum();
            assert_eq!(keys.len(), width, "{sample}: a slot has no key");
            let line = format!("VARIABLES {}\n", names.vars().join(", "));
            assert!(
                ts.vars().is_empty() || bundle.tla.contains(&line),
                "{sample}: VARIABLES line does not list the names"
            );
            let run = ts.clone().run(Caps::new(usize::MAX, 1));
            let canon = names.canon(&run.system, &run.states[0]);
            let canon_keys: BTreeSet<&String> = canon.keys().collect();
            assert_eq!(
                canon_keys, unique,
                "{sample}: canon keys differ from slot keys"
            );
        }
    }
}
