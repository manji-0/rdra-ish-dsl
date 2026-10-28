//! The explorer and TLC agree on every sample: for each cone `explore_targets` explores, the
//! states TLC reaches on the printed cone (`tlc -dump`) are the states the BFS reached, and
//! the invariants and after-asserts TLC finds violated are the ones the BFS found violated.
//!
//! TLC stops at the first violation unless run with `-continue`, which explores the whole
//! state space and reports every violated invariant and action property. The verdict run
//! uses `-continue` on the Safety-mode cfg (temporal properties are excluded from the
//! comparison), and its distinct-state count must equal the dump's, so it is known to have
//! finished.
//!
//! Every trace TLC prints for a violation replays through the BFS step relation, and the
//! check `explain` builds at its end is the one TLC named.
//!
//! Without `tlc` on PATH the TLC test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure.

#[path = "../src/tlc_env.rs"]
#[allow(dead_code)]
mod tlc_env;
#[path = "../src/tlc_output.rs"]
#[allow(dead_code)]
mod tlc_output;
#[path = "../src/tlc_replay.rs"]
mod tlc_replay;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rdra_ish_core::lifecycle::{Bounds, Caps, ComponentRun, Scope, StateDiag, TransitionSystem};
use rdra_ish_core::{build_merged_model, resolve, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use tlc_env::{tlc_env, Heap};
use tlc_output::{parse_dump, CanonState, Outcome, TlcRun};
use tlc_replay::explain_violation;

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

/// The TLA name of the invariant or after-assert a violation reports.
fn verdict_name(ts: &TransitionSystem, names: &TlaNames, diag: &StateDiag) -> Option<String> {
    match diag {
        StateDiag::ConstraintViolated { name, .. } => {
            let i = ts
                .invariants()
                .iter()
                .position(|inv| inv.name == *name)
                .expect("violated invariant is in the cone");
            Some(names.invariant(i).to_string())
        }
        StateDiag::AfterAssertViolated { site, .. } => {
            let i = ts
                .after_asserts()
                .iter()
                .position(|a| a.site == *site)
                .expect("violated assert is in the cone");
            Some(names.after(i).to_string())
        }
        _ => None,
    }
}

/// Invariants and after-asserts the BFS found violated, by TLA name.
fn bfs_verdicts(run: &ComponentRun, names: &TlaNames) -> BTreeSet<String> {
    run.diags
        .iter()
        .filter_map(|d| verdict_name(&run.system, names, &d.diag))
        .collect()
}

/// Each trace TLC printed replays step for step, with TLC's labels, and ends in the check TLC
/// named.
fn replay_violations(label: &str, ts: &TransitionSystem, names: &TlaNames, run: &TlcRun) {
    for v in &run.violations {
        let name = v.name.as_deref().expect("a safety run names its checks");
        let diag = explain_violation(ts, names, v)
            .unwrap_or_else(|why| panic!("{label}: TLC trace of {name} does not replay: {why}"));
        assert_eq!(
            verdict_name(ts, names, &diag).as_deref(),
            Some(name),
            "{label}: explain names another check"
        );
        let steps: Vec<&str> = diag
            .trace()
            .expect("a violation has a trace")
            .steps
            .iter()
            .map(|s| s.action.as_str())
            .collect();
        let labels: Vec<&str> = v.trace[1..]
            .iter()
            .map(|s| {
                let a = names.action_index(ts, &s.label).expect("a known label");
                ts.actions()[a.index()].name.as_str()
            })
            .collect();
        assert_eq!(steps, labels, "{label}: {name} replays another path");
    }
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
    let (key, value) = tlc_env(Heap::Cone, &tmp);
    let out = Command::new(tlc)
        .env(key, value)
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

    let check = tla::print(ts, PrintMode::Safety, "Cone");
    let (_, log) = run_tlc(tlc, dir, &["-continue"], &check.tla, &check.cfg);
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
    replay_violations(label, ts, &check.names, &parsed);
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
    for sample in samples() {
        let model = load(&sample);
        for bounds in bounds_for(&sample) {
            check_model(&tlc, &scratch, &sample, &model, bounds);
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Every cone of the model at these bounds, checked in parallel.
fn check_model(tlc: &Path, scratch: &Path, sample: &str, model: &SemanticModel, bounds: Bounds) {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let built = TransitionSystem::build(model, bounds, &Scope::ALL);
    let exploration = built
        .ts
        .explore_targets(&built.ts.targets(), Caps::new(usize::MAX, STATE_CAP));
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
                let report = check_cone(tlc, &dir, &label, &cone.run);
                reports.lock().unwrap().push((i, report));
            });
        }
    });
    let mut reports = reports.into_inner().unwrap();
    reports.sort_by_key(|(i, _)| *i);
    let states: usize = reports.iter().map(|(_, r)| r.states).sum();
    let violated: BTreeSet<&String> = reports.iter().flat_map(|(_, r)| &r.violated).collect();
    println!(
        "{sample} instances={}: {} cones, {states} states, violated {violated:?}",
        bounds.instances,
        reports.len()
    );
}

/// Condition guards in both engines: a bound atom, an `Exists` along an owner, an independent
/// `Exists`, and an atom on a created param that filters the creation's choices, each enabled
/// in some reachable states and not in others.
const CONDITION_GUARDS: &str = r#"
variation Kind "種別" {
  selfpay "自費"
  insured "保険"
}
entity Claim "請求" { id: Int @pk  kind: Kind @default(selfpay) }
relate(Claim, Visit, N:1)
usecase File "請求作成"
creates(File, Claim)
condition Filed "請求可能"
holds(Filed, Claim.kind == insured, Visit.status == arrived, Room.state == busy)
applies_to(Filed, File)
entity Visit "来院" { id: Int @pk  status: Enum(booked, arrived) }
entity Payment "支払い" { id: Int @pk  status: Enum(open, paid)  method: Enum(cash, insurance) }
entity Room "診察室" { id: Int @pk  state: Enum(free, busy) }
relate(Payment, Visit, N:1)
usecase Book "予約"
usecase Arrive "到着"
usecase Open "会計開始"
usecase Insure "保険"
usecase Pay "支払い"
usecase Prepare "準備"
usecase Occupy "入室"
creates(Book, Visit)
transitions(Visit.status, Arrive, booked -> arrived)
creates(Open, Payment)
sets(Insure, Payment, method == insurance)
transitions(Payment.status, Pay, open -> paid)
creates(Prepare, Room)
transitions(Room.state, Occupy, free -> busy)
condition Ready "支払い可能"
holds(Ready, Payment.method == insurance, Visit.status == arrived, Room.state == busy)
applies_to(Ready, Pay)
invariant(Payment, Visit)
  .along(Payment, Visit)
  .when(Payment.status == paid)
  .then(Visit.status == arrived)
"#;

#[test]
fn bfs_and_tlc_agree_on_condition_guards() {
    check_fixture("condition_guards", CONDITION_GUARDS);
}

/// A use case split into two transaction groups (one API each): an after-assert checked after
/// each step, with the other group's entity universally bound, and a condition guard on the
/// group whose entity it reads.
const SPLIT_USE_CASE: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending)  rush: Bool }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Rush "急ぎ"
api CapturePaymentApi "Capture"
api PayOrderApi "Pay order"
creates(Place, Order)
creates(Place, Payment)
invokes(Pay, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Pay, PayOrderApi)
updates(PayOrderApi, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Payment.status, Pay, pending -> captured)
sets(Rush, Order, rush == true)
condition Ready "可"
holds(Ready, Order.rush == true)
applies_to(Ready, Pay)
after(Pay).assert(Order.status == paid, Payment.status == captured)
"#;

#[test]
fn bfs_and_tlc_agree_on_split_use_cases() {
    check_fixture("split_use_case", SPLIT_USE_CASE);
}

/// A created parent links an updated child: the optional charge only while unlinked, so each
/// claim keeps its charge; the mandatory bed always moves, which empties its first ward at two
/// slots.
const RELINK: &str = r#"
entity Claim "請求" { id: Int @pk }
entity Charge "料金" { id: Int @pk }
relate(Charge, Claim, 1..*:1).optional()
usecase CreateCharge "計上"
usecase GenerateClaim "請求作成"
creates(CreateCharge, Charge)
creates(GenerateClaim, Claim)
updates(GenerateClaim, Charge)
entity Ward "病棟" { id: Int @pk }
entity Bed "病床" { id: Int @pk }
relate(Bed, Ward, 1..*:1)
usecase OpenWard "開設"
usecase AddBed "増床"
usecase SplitWard "分割"
creates(OpenWard, Ward)
creates(OpenWard, Bed)
creates(AddBed, Bed)
creates(SplitWard, Ward)
updates(SplitWard, Bed)
"#;

#[test]
fn bfs_and_tlc_agree_on_relinks() {
    check_fixture("relink", RELINK);
}

/// One inline model at both bounds.
fn check_fixture(name: &str, src: &str) {
    let Some(tlc) = find_tlc() else {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped {name} equivalence: {reason}");
        return;
    };
    let scratch =
        std::env::temp_dir().join(format!("rdra-equivalence-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let file = scratch.join(format!("{name}.rdra"));
    std::fs::write(&file, src).expect("write model");
    let model = load(file.to_str().expect("utf-8 path"));
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        check_model(&tlc, &scratch, name, &model, bounds);
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
            let ts = TransitionSystem::build(&model, bounds, &Scope::ALL).ts;
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

/// A TLC trace that leaves the step relation fails closed: a state no step reaches, a label
/// that names no use case, and a text that is no value. The untouched trace replays.
#[test]
fn a_corrupted_tlc_trace_does_not_replay() {
    let Some(tlc) = find_tlc() else {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped corrupted trace: {reason}");
        return;
    };
    let dir = std::env::temp_dir().join(format!("rdra-corrupted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let model = load("samples/formal-verification/cross_order_payment_split.rdra");
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let exploration = ts.explore_targets(&ts.targets(), Caps::new(usize::MAX, STATE_CAP));
    let cone = exploration
        .cones
        .iter()
        .find(|c| !c.run.system.invariants().is_empty())
        .expect("the forbidden's cone");
    let sub = &cone.run.system;
    let check = tla::print(sub, PrintMode::Safety, "Cone");
    let (_, log) = run_tlc(&tlc, &dir, &["-continue"], &check.tla, &check.cfg);
    let _ = std::fs::remove_dir_all(&dir);
    let parsed = TlcRun::parse(&log);
    let v = parsed.violations.first().expect("TLC finds the violation");
    let labels: Vec<&str> = v.trace.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Initial predicate", "Place", "Pay_1", "Cancel"],
        "{log}"
    );
    assert!(explain_violation(sub, &check.names, v).is_ok());

    let mut state = v.clone();
    state.trace[1]
        .state
        .insert("Order_status[1]".to_string(), "\"cancelled\"".to_string());
    assert_eq!(
        explain_violation(sub, &check.names, &state),
        Err("TLC step 2 (label Place) matches no move of the cone".to_string())
    );
    let mut label = v.clone();
    label.trace[2].label = "Cancel".to_string();
    assert_eq!(
        explain_violation(sub, &check.names, &label),
        Err("TLC step 3 (label Cancel) matches no move of the cone".to_string())
    );
    let mut unknown = v.clone();
    unknown.trace[2].label = "NoSuchAction".to_string();
    assert_eq!(
        explain_violation(sub, &check.names, &unknown),
        Err("TLC step 3 (label NoSuchAction) names no use case of the cone".to_string())
    );
    let mut text = v.clone();
    text.trace[3]
        .state
        .insert("Order_status[1]".to_string(), "\"refunded\"".to_string());
    assert_eq!(
        explain_violation(sub, &check.names, &text),
        Err("TLC state 4 is no state of the cone".to_string())
    );
}
