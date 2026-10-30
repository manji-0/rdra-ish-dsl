//! The explorer and TLC agree on every sample: for each cone a target reads, the state graph
//! TLC dumps for the printed cone (`tlc -dump dot,actionlabels`) has the BFS's states, initial
//! states, and `(from, to, action)` edges; the state-model findings (witnesses, use cases that
//! never run, deadlocks) of every entity are the same on both graphs; and the invariants and
//! after-asserts TLC finds violated are the ones the BFS found violated.
//!
//! Every sample runs at `Bounds::STATES` and `Bounds::VERIFY`. A cone whose BFS passes
//! `STATE_CAP` at VERIFY is compared at STATES only; `VERIFY_FALLBACK` lists those cones, so a
//! cone that grows past the cap fails the test until it is listed.
//!
//! TLC stops at the first violation unless run with `-continue`, which explores the whole
//! state space and reports every violated invariant and action property. One run per cone
//! uses `-continue` on the Safety-mode cfg (temporal properties are excluded from the
//! comparison) and dumps the graph; its distinct-state count must equal the BFS's, so it is
//! known to have finished.
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
#[allow(dead_code)]
mod tlc_replay;

use std::collections::{BTreeSet, HashMap};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rdra_ish_core::lifecycle::{
    ActId, Bounds, Caps, ComponentRun, Cone, LabelledEdges, Progress, Runs, Scope, State,
    StateDiag, StateModelFindings, Target, TransitionSystem, Witness,
};
use rdra_ish_core::{load_project, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use tlc_env::{tlc_env, Heap};
use tlc_output::{parse_dot, TlcRun};
use tlc_replay::explain_violation;

/// The largest cone the test compares. At `Bounds::STATES` every cone of every sample is far
/// below it.
const STATE_CAP: usize = 200_000;

/// `(sample, cone)`: the cones whose BFS at `Bounds::VERIFY` passes `STATE_CAP`, each named by
/// the first target that reads it. Each is compared at `Bounds::STATES`.
const VERIFY_FALLBACK: &[(&str, &str)] = &[
    ("samples/clinic-ops", "entity Charge"),
    ("samples/clinic-ops", "entity ClinicalOrder"),
    ("samples/clinic-ops", "entity EligibilityCheck"),
    ("samples/clinic-ops", "entity LabResult"),
    ("samples/clinic-ops", "entity Prescription"),
    ("samples/clinic-ops", "invariant Forbidden_ClinicalOrder_1"),
    ("samples/clinic-ops", "invariant Inv_ClinicalOrder_3"),
    ("samples/clinic-ops", "invariant Inv_Prescription_4"),
];

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

fn load(sample: &str) -> SemanticModel {
    load_project(&[repo().join(sample)], None)
        .unwrap_or_else(|e| panic!("{sample}: {e}"))
        .model
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

/// TLC's state graph of the cone in BFS terms: its edges as BFS state indices and actions,
/// sorted and without repeats. Panics where TLC's states or initial states differ from the
/// BFS's.
fn tlc_edges(dir: &Path, label: &str, run: &ComponentRun, names: &TlaNames) -> Vec<Move> {
    let ts = &run.system;
    let index: HashMap<&State, u32> = run
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (s, i as u32))
        .collect();
    let decoder = names.decoder(ts);
    let file = std::fs::File::open(dir.join("graph.dot")).expect("graph.dot");
    let dot = parse_dot(BufReader::new(file), |canon| {
        let state = decoder.decode(canon)?;
        index.get(&state).copied()
    })
    .unwrap_or_else(|why| panic!("{label}: TLC's graph does not map onto the BFS's: {why}"));
    let at: Vec<u32> = dot.nodes.iter().map(|n| n.state).collect();
    let distinct: BTreeSet<u32> = at.iter().copied().collect();
    if distinct.len() != at.len() || distinct.len() != run.states.len() {
        let only_bfs: Vec<_> = (0..run.states.len() as u32)
            .filter(|i| !distinct.contains(i))
            .take(3)
            .map(|i| names.canon(ts, &run.states[i as usize]))
            .collect();
        panic!(
            "{label}: reachable sets differ (TLC {} nodes, {} distinct; BFS {})\nonly BFS: {only_bfs:#?}",
            at.len(),
            distinct.len(),
            run.states.len()
        );
    }
    let tlc_initial: BTreeSet<u32> = dot
        .nodes
        .iter()
        .filter(|n| n.initial)
        .map(|n| n.state)
        .collect();
    let bfs_initial: BTreeSet<u32> = (0..run.states.len() as u32)
        .filter(|&i| run.parent(i).is_none())
        .collect();
    assert_eq!(tlc_initial, bfs_initial, "{label}: initial states differ");
    // A cone with no action prints `Next == UNCHANGED vars`, a step BFS does not take.
    let stutter = |l: &str| ts.actions().is_empty() && l == "Next";
    let acts: Vec<Option<ActId>> = dot
        .labels
        .iter()
        .map(|l| {
            let act = names.action_index(ts, l);
            assert!(
                act.is_some() || stutter(l),
                "{label}: TLC edge label {l} names no use case"
            );
            act
        })
        .collect();
    let mut edges: Vec<Move> = dot
        .edges
        .iter()
        .filter_map(|&(from, to, l)| {
            let (from, to) = (at[from as usize], at[to as usize]);
            let act = acts[l as usize];
            assert!(
                act.is_some() || from == to,
                "{label}: `Next` changes a state"
            );
            Some((from, to, act?))
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// `(from, to, action)` over BFS state indices.
type Move = (u32, u32, ActId);

/// A few edges only one engine has, in model terms: the action, the source state, and the
/// slots the step changes.
fn describe_moves(run: &ComponentRun, names: &TlaNames, moves: &[&Move]) -> String {
    let ts = &run.system;
    moves
        .iter()
        .take(3)
        .map(|&&(from, to, act)| {
            let pre = names.canon(ts, &run.states[from as usize]);
            let post = names.canon(ts, &run.states[to as usize]);
            let changed: Vec<String> = post
                .iter()
                .filter(|(k, v)| pre.get(*k) != Some(v))
                .map(|(k, v)| format!("{k} := {v}"))
                .collect();
            format!(
                "  {} from state {from} to state {to}, changing [{}]\n  from {pre:?}",
                ts.actions()[act.index()].name,
                changed.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The findings with the state indices left out: which state of each model is witnessed or
/// missing, which actions run, and which states are deadlocks, by name and severity.
fn finding_names(ts: &TransitionSystem, f: &StateModelFindings) -> Vec<String> {
    let value = |var, v| format!("{} = {v:?}", ts.var_name(var));
    let witnesses = f.witnesses.iter().map(|w| match &w.witness {
        Witness::At(_) => format!("witnessed {}", value(w.var, w.value)),
        Witness::Missing(d) => d.name(&[]),
        Witness::NotDecided => format!("not decided {}", value(w.var, w.value)),
    });
    let runs = f.runs.iter().map(|r| match &r.runs {
        Runs::Fired => format!("runs {}", ts.actions()[r.act.index()].name),
        Runs::Never(d) => format!("{d:?}"),
        Runs::NotDecided => format!("not decided {}", ts.actions()[r.act.index()].name),
    });
    let deadlocks = f.deadlocks.iter().map(|d| match &d.progress {
        Progress::Leaves => format!("leaves {}", value(d.var, d.value)),
        Progress::Stuck { diag, .. } => format!("{} {:?}", diag.name(&[]), diag.severity()),
        Progress::NotDecided => format!("not decided {}", value(d.var, d.value)),
    });
    witnesses.chain(runs).chain(deadlocks).collect()
}

fn check_cone(tlc: &Path, dir: &Path, label: &str, run: &ComponentRun) -> ConeReport {
    let ts = &run.system;
    let check = tla::print(ts, PrintMode::Safety, "Cone");
    let names = &check.names;
    let (_, log) = run_tlc(
        tlc,
        dir,
        &["-continue", "-dump", "dot,actionlabels", "graph"],
        &check.tla,
        &check.cfg,
    );
    let parsed = TlcRun::parse(&log);
    assert!(
        parsed.completed,
        "{label}: TLC check run did not finish\n{log}"
    );
    assert_eq!(
        parsed.distinct_states,
        Some(run.states.len() as u64),
        "{label}: TLC check run explored a different state count\n{log}"
    );

    let of_tlc = tlc_edges(dir, label, run, names);
    let mut of_bfs: Vec<Move> = run.graph().edges().collect();
    of_bfs.sort_unstable();
    of_bfs.dedup();
    if of_tlc != of_bfs {
        let only_tlc: Vec<&Move> = of_tlc
            .iter()
            .filter(|m| of_bfs.binary_search(m).is_err())
            .collect();
        let only_bfs: Vec<&Move> = of_bfs
            .iter()
            .filter(|m| of_tlc.binary_search(m).is_err())
            .collect();
        panic!(
            "{label}: edge sets differ (TLC {}, BFS {})\nonly TLC ({}):\n{}\nonly BFS ({}):\n{}",
            of_tlc.len(),
            of_bfs.len(),
            only_tlc.len(),
            describe_moves(run, names, &only_tlc),
            only_bfs.len(),
            describe_moves(run, names, &only_bfs),
        );
    }
    let tlc_moves = LabelledEdges::new(ts, of_tlc);
    let tlc_graph = tlc_moves.graph(&run.states, true);
    for e in ts.entity_ids() {
        assert_eq!(
            finding_names(ts, &ts.state_model_findings(e, &run.graph())),
            finding_names(ts, &ts.state_model_findings(e, &tlc_graph)),
            "{label}: state-model findings of {} differ (left BFS, right TLC)",
            ts.entity(e).id
        );
    }

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
    replay_violations(label, ts, names, &parsed);
    ConeReport {
        states: run.states.len(),
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
    let mut fallback = Vec::new();
    for sample in samples() {
        let model = load(&sample);
        assert!(
            check_model(&tlc, &scratch, &sample, &model, Bounds::STATES).is_empty(),
            "{sample}: a cone passes the state cap at STATES"
        );
        for cone in check_model(&tlc, &scratch, &sample, &model, Bounds::VERIFY) {
            fallback.push((sample.clone(), cone));
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    let expected: Vec<(String, String)> = VERIFY_FALLBACK
        .iter()
        .map(|(s, c)| (s.to_string(), c.to_string()))
        .collect();
    assert_eq!(
        fallback, expected,
        "the cones past the state cap at VERIFY are not VERIFY_FALLBACK (docs/formal-verification.md lists them)"
    );
}

/// The first target that reads a cone, as the cone's name.
fn target_label(ts: &TransitionSystem, t: Target) -> String {
    match t {
        Target::Entity(e) => format!("entity {}", ts.entity(e).id),
        Target::Invariant(i) => format!("invariant {}", ts.invariants()[i].name),
        Target::AfterAssert(i) => format!("after {}", ts.after_asserts()[i].name),
    }
}

/// Every cone of the model at these bounds, explored and checked in parallel; returns the
/// cones whose BFS passes `STATE_CAP`, which are not compared.
fn check_model(
    tlc: &Path,
    scratch: &Path,
    sample: &str,
    model: &SemanticModel,
    bounds: Bounds,
) -> Vec<String> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let mut cones: Vec<(Cone, String)> = Vec::new();
    for t in ts.targets() {
        let cone = ts.target_cone(t);
        if !cones.iter().any(|(c, _)| *c == cone) {
            cones.push((cone, target_label(&ts, t)));
        }
    }
    let next = AtomicUsize::new(0);
    let reports: Mutex<Vec<(usize, Option<ConeReport>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((cone, name)) = cones.get(i) else {
                    break;
                };
                let run = ts
                    .restrict_to(&cone.vars, &cone.keep)
                    .run(Caps::new(usize::MAX, STATE_CAP));
                let report = run.complete.then(|| {
                    let label = format!("{sample} at {bounds:?}, cone {i} ({name})");
                    let dir = scratch.join(format!(
                        "{}-{}-{i}",
                        sample.replace('/', "_"),
                        bounds.instances
                    ));
                    check_cone(tlc, &dir, &label, &run)
                });
                reports.lock().unwrap().push((i, report));
            });
        }
    });
    let mut reports = reports.into_inner().unwrap();
    reports.sort_by_key(|(i, _)| *i);
    let checked: Vec<&ConeReport> = reports.iter().filter_map(|(_, r)| r.as_ref()).collect();
    let states: usize = checked.iter().map(|r| r.states).sum();
    let violated: BTreeSet<&String> = checked.iter().flat_map(|r| &r.violated).collect();
    let capped: Vec<String> = reports
        .iter()
        .filter(|(_, r)| r.is_none())
        .map(|(i, _)| cones[*i].1.clone())
        .collect();
    println!(
        "{sample} instances={}: {} cones, {states} states, violated {violated:?}, past the cap {capped:?}",
        bounds.instances,
        checked.len()
    );
    capped
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

/// A published row written with the internal change it reports: Login creates Session and
/// the published LoginRecord in one action, Revoke ends the Session and the published view
/// in one action, and the published entities never enter the lifecycle.
const PUBLISHED_OUTBOX: &str = r#"
system Auth "Auth"
actor User "User"
usecase Login "Log in"
usecase Revoke "Revoke"
performs(User, Login)
performs(User, Revoke)
entity Session "Session" { id: Int @pk  status: Enum(active, revoked) @default(active) }
entity SessionView "Session (published)" { view_id: Int @pk  status: Enum(active, ended) }
entity LoginRecord "Login record" { id: Int @pk  result: Enum(succeeded, failed) }
creates(Login, Session)
creates(Login, LoginRecord)
sets(Login, LoginRecord, result == succeeded)
writes(Login, SessionView)
sets(Login, SessionView, status == active)
transitions(Session.status, Revoke, active -> revoked)
writes(Revoke, SessionView)
sets(Revoke, SessionView, status == ended)
exposes(Auth, Login)
exposes(Auth, SessionView).upsert()
exposes(Auth, LoginRecord).append()
"#;

#[test]
fn bfs_and_tlc_agree_on_a_published_outbox() {
    check_fixture("published_outbox", PUBLISHED_OUTBOX);
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
        let capped = check_model(&tlc, &scratch, name, &model, bounds);
        assert!(capped.is_empty(), "{name}: {capped:?} pass the state cap");
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
