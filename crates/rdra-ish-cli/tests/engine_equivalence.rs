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
//! state space but reports one violated check per state or step, the first in cfg order. One
//! run per cone uses `-continue` on the Safety-mode cfg (temporal properties are excluded from
//! the comparison) and dumps the graph; its distinct-state count must equal the BFS's, so it
//! is known to have finished. As `verify` does, a run with a violation is followed by reruns
//! without the checks reported so far, until a rerun reports no new one.
//!
//! Every trace TLC prints for a violation replays through the BFS step relation, and the
//! check `explain` builds at its end is the one TLC named.
//!
//! Without `tlc` on PATH the TLC test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure.

#[path = "../src/property_cone.rs"]
#[allow(dead_code)]
mod property_cone;
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

use property_cone::{ConeRun, PropertyCone, Verdict, Why};
use rdra_ish_core::lifecycle::{
    ActId, Bounds, Caps, ComponentRun, Cone, LabelledEdges, Progress, Runs, Scope, State,
    StateDiag, StateModelFindings, Taken, Target, TransitionSystem, Witness,
};
use rdra_ish_core::{load_project, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use tlc_env::{tlc_env, Heap};
use tlc_output::{parse_dot, Outcome, TlcRun};
use tlc_replay::{explain_property, explain_violation};

/// The largest cone the test compares. At `Bounds::STATES` every cone of every sample is far
/// below it.
const STATE_CAP: usize = 200_000;

/// `(sample, cone)`: the cones whose BFS at `Bounds::VERIFY` passes `STATE_CAP`, each named by
/// the first target that reads it. Each is compared at `Bounds::STATES`.
const VERIFY_FALLBACK: &[(&str, &str)] = &[
    ("samples/clinic-ops", "entity CarePlan"),
    ("samples/clinic-ops", "entity Charge"),
    ("samples/clinic-ops", "entity ClinicalOrder"),
    ("samples/clinic-ops", "entity EligibilityCheck"),
    ("samples/clinic-ops", "entity LabResult"),
    ("samples/clinic-ops", "entity PatientMessage"),
    ("samples/clinic-ops", "entity Prescription"),
    ("samples/clinic-ops", "invariant Forbidden_ClinicalOrder_1"),
    ("samples/clinic-ops", "invariant Inv_ClinicalOrder_3"),
    ("samples/clinic-ops", "invariant Inv_Prescription_4"),
    ("samples/patent-ops", "entity OfficeAction"),
    ("samples/patent-ops", "invariant Forbidden_OfficeAction_3"),
    ("samples/food-permit-ops", "entity Disposition"),
    ("samples/food-permit-ops", "entity Inspection"),
    ("samples/food-permit-ops", "entity PermitApplication"),
    (
        "samples/food-permit-ops",
        "invariant Forbidden_Disposition_2",
    ),
    ("samples/food-permit-ops", "invariant Inv_Disposition_0"),
    (
        "samples/food-permit-ops",
        "invariant Inv_PermitApplication_1",
    ),
    (
        "samples/food-permit-ops",
        "invariant Inv_PermitApplication_2",
    ),
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
    let arms = f.arms.iter().map(|a| match &a.taken {
        Taken::Yes => format!("takes {:?}", a.site.fact),
        Taken::Never(d) => format!("{d:?}"),
        Taken::Moot => format!("moot {:?}", a.site.fact),
        Taken::NotDecided => format!("not decided {:?}", a.site.fact),
    });
    witnesses.chain(runs).chain(deadlocks).chain(arms).collect()
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

    let mut parsed = parsed;
    loop {
        let found = parsed.violated_names();
        if found.is_empty() {
            break;
        }
        let cfg = tla::cfg_without(ts, names, PrintMode::Safety, &found);
        let (_, more) = run_tlc(tlc, dir, &["-continue"], &check.tla, &cfg);
        parsed.violations.extend(TlcRun::parse(&more).violations);
        if parsed.violated_names().len() == found.len() {
            break;
        }
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
            check_model(&tlc, &scratch, &sample, &model, Bounds::STATES)
                .capped
                .is_empty(),
            "{sample}: a cone passes the state cap at STATES"
        );
        for cone in check_model(&tlc, &scratch, &sample, &model, Bounds::VERIFY).capped {
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

/// What [`check_model`] found: the cones past the state cap, and the checks TLC and the BFS
/// both found violated in the others.
struct ModelReport {
    capped: Vec<String>,
    violated: BTreeSet<String>,
}

/// Every cone of the model at these bounds, explored and checked in parallel; returns the
/// cones whose BFS passes `STATE_CAP`, which are not compared.
fn check_model(
    tlc: &Path,
    scratch: &Path,
    sample: &str,
    model: &SemanticModel,
    bounds: Bounds,
) -> ModelReport {
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
    let violated: BTreeSet<String> = checked
        .iter()
        .flat_map(|r| r.violated.iter().cloned())
        .collect();
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
    ModelReport { capped, violated }
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
transitions(Arrive, Visit.status, booked -> arrived)
creates(Open, Payment)
sets(Insure, Payment.method == insurance)
transitions(Pay, Payment.status, open -> paid)
creates(Prepare, Room)
transitions(Occupy, Room.state, free -> busy)
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

/// `transitions` arms of one use case from one state are a choice (#52): three arms from
/// `searched` beside a deterministic arm from `file`, a second choice column, a `sets` and a
/// creation every arm carries, and an after-assert one arm breaks.
const TRANSITION_CHOICE: &str = r#"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
  route: Enum(none, fast, slow) @default(none)
  flag: Bool @default(false)
}
entity Log "記録" { id: Int @pk }
usecase Submit "届出"
usecase Decide "判断"
usecase Withdraw "取下げ"
creates(Submit, Doc)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, searched -> drop)
transitions(Decide, Doc.status, file -> drop)
transitions(Decide, Doc.route, none -> fast)
transitions(Decide, Doc.route, none -> slow)
sets(Decide, Doc.flag == true)
creates(Decide, Log)
transitions(Withdraw, Doc.status, secret -> searched)
usecase Reroute "経路の取消"
transitions(Reroute, Doc.route, fast -> none)
transitions(Reroute, Doc.route, slow -> none)
after(Decide).assert(Doc.status != drop)
"#;

#[test]
fn bfs_and_tlc_agree_on_transition_choices() {
    check_fixture("transition_choice", TRANSITION_CHOICE);
}

/// `after(Pay).assert` on a use case of two transaction groups is read after the last step
/// on the run's rows (#83): the order step 1 marked paid is step 2's `Earlier` row, and the
/// payment step 2 creates is captured, so the assert holds although another order's payment
/// is refunded. `{REFUND}` lets a refund run between the two steps of the other order.
const AFTER_SPLIT: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(captured, refunded) @default(captured) }
relate(Payment, Order, N:1)
api OrderApi "mark paid"
api PaymentApi "capture"
usecase Place "注文"
usecase Pay "支払"
usecase Refund "返金"
creates(Place, Order)
invokes(Pay, OrderApi)
invokes(Pay, PaymentApi)
updates(OrderApi, Order)
creates(PaymentApi, Payment)
transitions(Pay, Order.status, pending -> paid)
updates(Refund, Payment)
transitions(Refund, Payment.status, captured -> refunded)
after(Pay).assert(Order.status == paid, Payment.status == captured)
"#;

/// `@check` is two-valued (#84): null fails `qty >= 1` from creation on, `{ADMIT}` lets a
/// check admit it, and both engines read `!=` as true on null.
const NULL_CHECK: &str = r#"
entity Line "明細" { id: Int @pk  qty: Int @null @check({ADMIT}qty >= 1) @check(qty != 3) }
usecase Add "追加"
usecase Fill "数量"
creates(Add, Line)
updates(Fill, Line)
sets(Fill, Line.qty == 2)
"#;

#[test]
fn bfs_and_tlc_agree_on_null_in_a_check() {
    let strict = check_fixture("null_check_strict", &NULL_CHECK.replace("{ADMIT}", ""));
    let admits = check_fixture(
        "null_check_admits",
        &NULL_CHECK.replace("{ADMIT}", "qty == null or "),
    );
    if let (Some(strict), Some(admits)) = (strict, admits) {
        assert_eq!(
            strict.into_iter().collect::<Vec<_>>(),
            vec!["Check_Line_qty_0".to_string()]
        );
        assert!(admits.is_empty(), "{admits:?}");
    }
}

#[test]
fn bfs_and_tlc_agree_on_an_after_assert_of_a_split_use_case() {
    if let Some(violated) = check_fixture("after_split", AFTER_SPLIT) {
        assert!(!violated.contains("AfterAssert_Pay_0"), "{violated:?}");
    }
}

/// A guard per `transitions` arm (#62): of three arms from `searched`, filing needs the right
/// on the row itself and the fast route needs an open gate on another entity (an `Exists`
/// witness), beside an unguarded arm; a gate created closed and opened later; a guarded arm
/// whose guard never holds; and a rule only the guard keeps.
const ARM_GUARDS: &str = r#"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
  route: Enum(none, fast, slow) @default(none)
  acquired: Bool @default(false)
}
entity Gate "関門" { id: Int @pk  open: Bool @default(false) }
usecase Submit "届出"
usecase Acquire "承継"
usecase Build "関門の設置"
usecase Open "開門"
usecase Decide "判断"
usecase Route "経路"
creates(Submit, Doc)
sets(Acquire, Doc.acquired == true)
creates(Build, Gate)
sets(Open, Gate.open == true)
condition Acquired "権利を取得している"
holds(Acquired, Doc.acquired == true)
condition GateOpen "関門が開いている"
holds(GateOpen, Gate.open == true)
condition Never "起きない"
holds(Never, Doc.acquired == true, Doc.status == drop)
transitions(Decide, Doc.status, searched -> file).when(Acquired)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, searched -> drop)
transitions(Decide, Doc.status, secret -> drop).when(Never)
transitions(Decide, Doc.status, secret -> file).when(Acquired)
transitions(Route, Doc.route, none -> fast).when(GateOpen)
transitions(Route, Doc.route, none -> slow)
forbidden(Doc, status == file, acquired == false)
"#;

#[test]
fn bfs_and_tlc_agree_on_arm_guards() {
    check_fixture("arm_guards", ARM_GUARDS);
}

/// `chooses` (#54): an update re-picks two variation columns a creation pinned (one successor
/// per pair of values), a rule each choice can break, a Bool another use case sets, and a
/// `chooses` on the creating use case, which is the creation's own choice.
const CHOOSES: &str = r#"
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
}
variation Route "経路" {
  mail "郵送"
  online "電子"
}
entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted, filed) @default(open)
  final: Bool @default(false)
  scope: Scope
  route: Route
}
usecase Receive "受領"
usecase Draft "応答案の作成"
usecase File "提出"
usecase Reopen "最後の通知"
creates(Receive, Notice)
sets(Receive, Notice.scope == limited)
chooses(Receive, Notice.route)
transitions(Draft, Notice.status, open -> drafted)
chooses(Draft, Notice.scope)
chooses(Draft, Notice.route)
transitions(File, Notice.status, drafted -> filed)
transitions(Reopen, Notice.status, filed -> open)
sets(Reopen, Notice.final == true)
forbidden(Notice, final == true, scope == free)
forbidden(Notice, status == filed, route == mail, scope == free)
"#;

#[test]
fn bfs_and_tlc_agree_on_chooses() {
    check_fixture("chooses", CHOOSES);
}

/// A guard per chosen value (#65): of three listed values, `free` needs a notice that is not
/// final, `fast` needs an open gate on another entity (an `Exists` witness), `limited` is
/// unguarded; a value whose guard never holds; listed values on the creating use case; and a
/// rule only the guard keeps.
const CHOICE_GUARDS: &str = r#"
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
  fast "早期"
  none "補正なし"
}
entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  final: Bool @default(false)
  scope: Scope
}
entity Gate "関門" { id: Int @pk  open: Bool @default(false) }
usecase Receive "受領"
usecase Draft "応答案の作成"
usecase Reopen "最後の通知"
usecase Build "関門の設置"
usecase Open "開門"
creates(Receive, Notice)
chooses(Receive, Notice.scope, limited)
chooses(Receive, Notice.scope, none)
transitions(Draft, Notice.status, open -> drafted)
transitions(Reopen, Notice.status, drafted -> open)
sets(Reopen, Notice.final == true)
sets(Reopen, Notice.scope == limited)
creates(Build, Gate)
sets(Open, Gate.open == true)
condition NotFinal "最後の通知でない"
holds(NotFinal, Notice.final == false)
condition GateOpen "関門が開いている"
holds(GateOpen, Gate.open == true)
condition Never "起きない"
holds(Never, Notice.final == true, Notice.status == drafted)
chooses(Draft, Notice.scope, free).when(NotFinal)
chooses(Draft, Notice.scope, fast).when(GateOpen)
chooses(Draft, Notice.scope, limited)
chooses(Draft, Notice.scope, none).when(Never)
forbidden(Notice, final == true, scope == free)
"#;

#[test]
fn bfs_and_tlc_agree_on_choice_guards() {
    check_fixture("choice_guards", CHOICE_GUARDS);
}

/// A use case split into two transaction groups (one API each): an after-assert checked after
/// each step, with the other group's entity universally bound, and a condition guard on every
/// group's step.
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
transitions(Pay, Order.status, pending -> paid)
transitions(Pay, Payment.status, pending -> captured)
sets(Rush, Order.rush == true)
condition Ready "可"
holds(Ready, Order.rush == true)
applies_to(Ready, Pay)
after(Pay).assert(Order.status == paid, Payment.status == captured)
"#;

#[test]
fn bfs_and_tlc_agree_on_split_use_cases() {
    check_fixture("split_use_case", SPLIT_USE_CASE);
}

/// A use case that moves the claim through one API and creates the settlement under it through
/// another, guarded by the claim's kind (#78, #80). The first step carries the guard and the
/// second creates the settlement under the claim the first moved, so neither engine creates a
/// settlement under a vehicle claim.
const GUARDED_SPLIT: &str = r#"
variation Kind "種目" {
  liability "賠償"
  vehicle "車両"
}
entity Claim "案件" { id: Int @pk  kind: Kind  status: Enum(open, negotiating, closed) @default(open) }
entity Settlement "示談" { id: Int @pk  status: Enum(pending, agreed) @default(pending) }
relate(Settlement, Claim, N:1)
usecase Report "受付"
usecase Close "終了"
usecase OpenSettlement "示談開始"
api ClaimApi "案件 API"
api SettlementApi "示談 API"
creates(Report, Claim)
transitions(Close, Claim.status, open -> closed)
invokes(OpenSettlement, ClaimApi)
updates(ClaimApi, Claim)
transitions(OpenSettlement, Claim.status, open -> negotiating)
invokes(OpenSettlement, SettlementApi)
creates(SettlementApi, Settlement)
condition Eligible "賠償責任の事故"
holds(Eligible, Claim.kind == liability)
applies_to(Eligible, OpenSettlement)
forbidden(Claim, Settlement, Claim.kind == vehicle, Settlement.status == pending).along(Claim, Settlement)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_guard_over_every_group() {
    if let Some(violated) = check_fixture("guarded_split", GUARDED_SPLIT) {
        assert!(violated.is_empty(), "{violated:?}");
    }
}

/// The repro of #80: the two steps of OpenSettlement run in order on one run's rows, so the
/// settlement is created once under the claim step 1 moved (`Unique_Settlement_Claim` holds)
/// and no claim is stuck at negotiating. Both engines agree, at 1 and 2 instances.
#[test]
fn bfs_and_tlc_agree_on_an_ordered_run() {
    let src =
        std::fs::read_to_string(repo().join("samples/formal-verification/split_run_order.rdra"))
            .expect("sample readable");
    if let Some(violated) = check_fixture("split_run_order", &src) {
        assert!(violated.is_empty(), "{violated:?}");
    }
}

/// Between two steps of a run any other use case runs: Flag marks a negotiating claim with no
/// settlement yet, then the settlement step creates a pending settlement under it, and both
/// engines reach the forbidden pair.
const INTERLEAVED_SPLIT: &str = r#"
entity Claim "案件" { id: Int @pk  status: Enum(open, negotiating, closed) @default(open)  flagged: Bool @default(false) }
entity Settlement "示談" { id: Int @pk  status: Enum(pending, agreed) @default(pending) }
relate(Settlement, Claim, 1:1)
usecase Report "受付"
usecase OpenSettlement "示談開始"
usecase Flag "注意"
api ClaimApi "案件 API"
api SettlementApi "示談 API"
creates(Report, Claim)
invokes(OpenSettlement, ClaimApi)
updates(ClaimApi, Claim)
transitions(OpenSettlement, Claim.status, open -> negotiating)
invokes(OpenSettlement, SettlementApi)
creates(SettlementApi, Settlement)
sets(Flag, Claim.flagged == true)
condition NoSettlementYet "示談未作成"
holds(NoSettlementYet, Claim.status == negotiating).none(Settlement)
applies_to(NoSettlementYet, Flag)
forbidden(Claim, Settlement, Claim.flagged == true, Settlement.status == pending).along(Claim, Settlement)
"#;

#[test]
fn bfs_and_tlc_agree_on_an_interleaving_between_two_steps() {
    if let Some(violated) = check_fixture("interleaved_split", INTERLEAVED_SPLIT) {
        assert_eq!(
            violated,
            BTreeSet::from(["CrossForbidden_Claim_0".to_string()])
        );
    }
}

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
sets(Login, LoginRecord.result == succeeded)
updates(Login, SessionView)
sets(Login, SessionView.status == active)
transitions(Revoke, Session.status, active -> revoked)
updates(Revoke, SessionView)
sets(Revoke, SessionView.status == ended)
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

/// `in` / `not in` on an Enum, a Bool, an Int, and a nullable column, in a guard, a guard's
/// quantifier over the entity its use case creates, local and cross-entity rules, a
/// quantifier, and an after-assert. The cross-entity rule and the quantifier fail in exactly
/// the same states: a sent parcel of a cancelled order.
const SET_MEMBERSHIP: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped, cancelled) @default(pending)
  rush: Bool
  qty: Int @default(0)
  paid_at: DateTime @null
}
entity Parcel "荷物" { id: Int @pk  state: Enum(packed, sent) @default(packed) }
relate(Parcel, Order, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Cancel "取消"
usecase Bump "増量"
usecase Pack "梱包"
usecase Send "発送"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.paid_at == present)
transitions(Ship, Order.status, paid -> shipped)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Cancel, Order.status, paid -> cancelled)
sets(Bump, Order.qty == 2)
sets(Bump, Order.rush == true)
creates(Pack, Parcel)
transitions(Send, Parcel.state, packed -> sent)
condition Open "未確定"
holds(Open, Order.status not in (shipped, cancelled), Order.qty in (0, 2))
applies_to(Open, Bump)
condition Packable "梱包可"
holds(Packable, Order.status in (paid, shipped)).none(Parcel.state in (sent))
applies_to(Packable, Pack)
forbidden(Order, status in (shipped, cancelled), rush in (true))
invariant(Order).when(status not in (pending, cancelled)).then(paid_at in (present))
invariant(Order, Parcel).when(Parcel.state in (sent)).then(Order.status in (paid, shipped))
invariant(Order).when(status in (cancelled)).none(Parcel.state not in (packed))
after(Ship).assert(Order.status in (shipped), Order.paid_at not in (null))
"#;

#[test]
fn bfs_and_tlc_agree_on_set_membership() {
    check_fixture("set_membership", SET_MEMBERSHIP);
}

/// A `.none` guard on the use case that creates the quantified entity (#63): the guard ranges
/// over the parcels present before the step, never the one being created, and a `.has` guard
/// on the use case that updates it ranges over every parcel, the updated one included.
const CREATED_QUANTIFIER: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  last: Enum(none, pack, send) @default(none)
}
entity Parcel "荷物" { id: Int @pk  state: Enum(packed, sent) @default(packed) }
relate(Parcel, Order, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Pack "梱包"
usecase Send "発送"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
creates(Pack, Parcel)
sets(Pack, Order.last == pack)
transitions(Send, Parcel.state, packed -> sent)
sets(Send, Order.last == send)
condition Packable "梱包可"
holds(Packable, Order.status == paid).none(Parcel.state == sent)
applies_to(Packable, Pack)
condition Sendable "発送可"
holds(Sendable, Order.status == paid).has(Parcel.state == packed)
applies_to(Sendable, Send)
invariant(Order).when(last == pack).none(Parcel.state == sent)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_guard_over_the_created_entity() {
    check_fixture("created_quantifier", CREATED_QUANTIFIER);
}

/// Checks that fail only together (#64): the cross-entity invariant fails in exactly the
/// states where the forbidden state does, and both after-asserts on every step of Send. TLC
/// reports one check per state or step, the first in cfg order, so each later one shows only
/// in a rerun without the earlier.
const SIMULTANEOUS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Parcel "荷物" { id: Int @pk  state: Enum(packed, sent) @default(packed) }
relate(Parcel, Order, N:1)
usecase Place "注文"
usecase Pack "梱包"
usecase Send "発送"
creates(Place, Order)
creates(Pack, Parcel)
transitions(Send, Parcel.state, packed -> sent)
forbidden(Parcel, state == sent)
invariant(Order, Parcel).along(Parcel, Order).when(Parcel.state == sent).then(Order.status == paid)
after(Send).assert(Parcel.state == packed)
after(Send).assert(Order.status == paid)
"#;

#[test]
fn bfs_and_tlc_report_every_check_that_fails_in_the_same_state() {
    check_fixture("simultaneous", SIMULTANEOUS);
}

/// Two lifecycle columns on one entity (#68): `status` and `publication` move independently,
/// share the value `open`, and the rules read both. Publishing needs a decided status (a guard
/// on one column's arm that reads the other), a forbidden state and an after-assert pair the
/// two, and an arm whose guard never holds leaves `ended` without a witness.
const TWO_LIFECYCLES: &str = r#"
entity D "処分" {
  id: Int @pk
  status: Enum(open, decided, closed) @default(open)
  publication: Enum(hidden, open, ended) @default(hidden)
}
usecase Make "作成"
usecase Decide "決定"
usecase Close "完結"
usecase Publish "公表"
usecase Withdraw "取下げ"
usecase End "公表終了"
creates(Make, D)
transitions(Decide, D.status, open -> decided)
transitions(Close, D.status, decided -> closed)
condition Decided "決定済み"
holds(Decided, D.status in (decided, closed))
condition Never "起きない"
holds(Never, D.status == open, D.publication == open)
transitions(Publish, D.publication, hidden -> open).when(Decided)
transitions(Withdraw, D.publication, open -> hidden)
transitions(End, D.publication, open -> ended).when(Never)
forbidden(D, publication == open, status == open)
after(Close).assert(D.publication != hidden)
"#;

#[test]
fn bfs_and_tlc_agree_on_two_lifecycle_columns() {
    check_fixture("two_lifecycles", TWO_LIFECYCLES);
}

/// `sets(UC, E.column == F.source)` (#69): a permit created under its application copies the
/// application's kind, and an update copies `staffed` from the application its owner names.
/// Without the copies the permit's kind is a free choice and `staffed` keeps its default, so
/// both rules break; with them the kind rule holds and the `staffed` rule breaks only while the
/// application moves after the copy.
const COPIES: &str = r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Application "申請" {
  id: Int @pk
  kind: Kind
  status: Enum(submitted, approved) @default(submitted)
  staffed: Bool @default(false)
}
entity Permit "許可" {
  id: Int @pk
  kind: Kind
  staffed: Bool @default(false)
}
relate(Permit, Application, N:1)
usecase Apply "申請"
usecase Staff "責任者"
usecase Approve "承認"
usecase Register "登録"
usecase Refresh "更新"
creates(Apply, Application)
sets(Staff, Application.staffed == true)
transitions(Approve, Application.status, submitted -> approved)
creates(Register, Permit)
updates(Refresh, Permit)
condition Approved "承認済み"
holds(Approved, Application.status == approved)
applies_to(Approved, Register)
forbidden(Permit, Application, Permit.kind != Application.kind).along(Permit, Application)
forbidden(Permit, Application, Permit.staffed == true, Application.staffed == false).along(Permit, Application)
after(Refresh).assert(Permit.staffed == true)
"#;

#[test]
fn bfs_and_tlc_agree_on_copies() {
    check_fixture("no_copies", COPIES);
    let copies = format!(
        "{COPIES}\nsets(Register, Permit.kind == Application.kind)\nsets(Register, Permit.staffed == Application.staffed)\nsets(Refresh, Permit.staffed == Application.staffed)\n"
    );
    check_fixture("copies", &copies);
    check_fixture("copy_in_one_step", COPY_IN_ONE_STEP);
}

/// A use case that creates an order and its invoice in one step copies the kind the order is
/// created with: the guard reads the source in the successor, `Invoice_kind'[p] =
/// Order_kind'[q]`.
const COPY_IN_ONE_STEP: &str = r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Order "注文" { id: Int @pk  kind: Kind }
entity Invoice "請求" { id: Int @pk  kind: Kind }
relate(Invoice, Order, N:1)
usecase Open "開始"
usecase Reissue "再発行"
creates(Open, Order)
creates(Open, Invoice)
sets(Open, Invoice.kind == Order.kind)
creates(Reissue, Invoice)
forbidden(Invoice, Order, Invoice.kind != Order.kind).along(Invoice, Order)
"#;

/// A condition on a use case that creates the row reads a column the creation leaves at its
/// default: the guard reads the successor of a var only `UNCHANGED` binds, so TLC needs the
/// `UNCHANGED` before the guard.
const GUARD_ON_UNASSIGNED: &str = r#"
entity E "E" { id: Int @pk  status: Enum(a, b) @default(a) }
usecase Make "作成"
usecase Flip "切替"
creates(Make, E)
transitions(Flip, E.status, a -> b)
condition Fresh "新規"
holds(Fresh, E.status == a)
applies_to(Fresh, Make)
forbidden(E, status == b)
property P eventually(E.status == b)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_guard_over_an_unassigned_column() {
    check_fixture("guard_on_unassigned", GUARD_ON_UNASSIGNED);
}

/// One inline model at both bounds.
/// Checks `src` as [`check_model`] does at both bounds; returns the checks found violated at
/// `Bounds::VERIFY`, or `None` without `tlc`.
fn check_fixture(name: &str, src: &str) -> Option<BTreeSet<String>> {
    let Some(tlc) = find_tlc() else {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped {name} equivalence: {reason}");
        return None;
    };
    let scratch =
        std::env::temp_dir().join(format!("rdra-equivalence-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let file = scratch.join(format!("{name}.rdra"));
    std::fs::write(&file, src).expect("write model");
    let model = load(file.to_str().expect("utf-8 path"));
    let mut violated = BTreeSet::new();
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        let report = check_model(&tlc, &scratch, name, &model, bounds);
        let capped = report.capped;
        assert!(capped.is_empty(), "{name}: {capped:?} pass the state cap");
        violated = report.violated;
    }
    let _ = std::fs::remove_dir_all(&scratch);
    Some(violated)
}

/// Two keys join App and Permit: the permit is registered under its application (1:1), and
/// a renewal application `renews` a permit. Every join names its key with `.role` (#73): the
/// registration guard's `.none(Permit).role(App)`, the renewal's `holds(..).role(renews)`, which
/// also links the renewal to the valid permit it reads, and the expiry's
/// `.none(App.status == done).role(EXPIRY_KEY)`. Through `renews` the expiry waits for no
/// renewal of the permit to be done, so the forbidden state is unreachable; through the
/// registration key it reads the new application, which is never renewed, so a permit
/// expires under a done renewal and both engines report the violation.
const JOIN_KEYS: &str = r#"
entity Permit "許可" { id: Int @pk  status: Enum(valid, expired) @default(valid) }
entity App "申請" { id: Int @pk  kind: Enum(new, renewal)  status: Enum(open, done) @default(open) }
relate(Permit, App, 1:1)
relate(App, Permit, N:1).optional().role(renews)
usecase Issue "新規申請"
usecase Register "登録"
usecase Apply "更新申請"
usecase Renew "更新"
usecase Expire "失効"
creates(Issue, App)
sets(Issue, App.kind == new)
creates(Register, Permit)
condition NoPermitYet "許可がまだない新規申請"
holds(NoPermitYet, App.kind == new).none(Permit).role(App)
applies_to(NoPermitYet, Register)
creates(Apply, App)
sets(Apply, App.kind == renewal)
reads(Apply, Permit)
condition Renewable "更新する許可が有効"
holds(Renewable, Permit.status == valid).role(renews)
applies_to(Renewable, Apply)
applies_to(Renewable, Renew)
transitions(Renew, App.status, open -> done)
transitions(Expire, Permit.status, valid -> expired)
condition NotRenewed "更新済みの申請がない"
holds(NotRenewed, Permit.status == valid).none(App.status == done).role(EXPIRY_KEY)
applies_to(NotRenewed, Expire)
forbidden(App, Permit, App.status == done, Permit.status == expired).along(App, Permit).role(renews)
"#;

#[test]
fn bfs_and_tlc_agree_on_the_key_a_join_names() {
    let renews = check_fixture("join_renews", &JOIN_KEYS.replace("EXPIRY_KEY", "renews"));
    let registered = check_fixture("join_registered", &JOIN_KEYS.replace("EXPIRY_KEY", "App"));
    if let (Some(renews), Some(registered)) = (renews, registered) {
        assert!(!renews.contains("CrossForbidden_App_0"), "{renews:?}");
        assert!(
            registered.contains("CrossForbidden_App_0"),
            "{registered:?}"
        );
    }
}

/// A `.none` on a `holds` counts along the guard's `.along` path (#81): `Withdraw` runs while no
/// loan on any of the member's cards is open, and a member with no card or no loan withdraws,
/// so no member is stuck. The rule forbids a withdrawn member with an open loan on their
/// chain; `{GUARD}` is the `applies_to` that keeps it, and without it both engines reach the
/// violation.
const NONE_ALONG_PATH: &str = r#"
entity Member "会員" { id: Int @pk  status: Enum(active, withdrawn) @default(active) }
entity Card "カード" { id: Int @pk }
entity Loan "貸出" { id: Int @pk  status: Enum(open, returned) @default(open) }
relate(Card, Member, N:1)
relate(Loan, Card, N:1)
usecase Register "登録"
usecase IssueCard "カード発行"
usecase Lend "貸出"
usecase Return "返却"
usecase Withdraw "退会"
creates(Register, Member)
creates(IssueCard, Card)
creates(Lend, Loan)
transitions(Return, Loan.status, open -> returned)
transitions(Withdraw, Member.status, active -> withdrawn)
condition MemberActive "有効な会員"
holds(MemberActive, Member.status == active)
applies_to(MemberActive, IssueCard)
condition CardOfActive "有効な会員のカード"
holds(CardOfActive, Member.status == active).along(Loan, Card, Member)
applies_to(CardOfActive, Lend)
condition NoOpenLoan "未返却なし"
holds(NoOpenLoan, Member.status == active).none(Loan.status == open).along(Member, Card, Loan)
{GUARD}
forbidden(Member, Loan, Member.status == withdrawn, Loan.status == open).along(Member, Card, Loan)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_none_along_a_path() {
    let guarded = check_fixture(
        "none_along_guarded",
        &NONE_ALONG_PATH.replace("{GUARD}", "applies_to(NoOpenLoan, Withdraw)"),
    );
    let unguarded = check_fixture(
        "none_along_unguarded",
        &NONE_ALONG_PATH.replace("{GUARD}", ""),
    );
    if let (Some(guarded), Some(unguarded)) = (guarded, unguarded) {
        assert!(!guarded.contains("CrossForbidden_Member_0"), "{guarded:?}");
        assert!(
            unguarded.contains("CrossForbidden_Member_0"),
            "{unguarded:?}"
        );
    }
}

/// Two mandatory keys from Order to Address, and `Place` creates the order without writing an
/// address (#75). Each key has its own parent param, so an order may ship to one address and
/// bill another. `Ship` runs while the `SHIP_KEY` address is in the east, and the rule forbids
/// a shipped order billed to the west. Through `ship_to` both engines reach the violation,
/// which needs the two keys on different rows; with one shared param they missed it. Through
/// `bill_to` the guard rules it out.
const TWO_KEYS: &str = r#"
entity Address "住所" { id: Int @pk  region: Enum(east, west) }
entity Order "注文" { id: Int @pk  status: Enum(open, shipped, cancelled) @default(open) }
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
usecase AddEast "東の住所"
usecase AddWest "西の住所"
usecase Place "注文"
usecase Ship "出荷"
usecase Cancel "取消"
creates(AddEast, Address)
sets(AddEast, Address.region == east)
creates(AddWest, Address)
sets(AddWest, Address.region == west)
creates(Place, Order)
transitions(Ship, Order.status, open -> shipped)
transitions(Cancel, Order.status, open -> cancelled)
condition ShipsEast "東へ出荷する"
holds(ShipsEast, Address.region == east).role(SHIP_KEY)
applies_to(ShipsEast, Ship)
forbidden(Order, Address, Order.status == shipped, Address.region == west).along(Order, Address).role(bill_to)
"#;

#[test]
fn bfs_and_tlc_agree_on_two_keys_to_one_parent() {
    let ship = check_fixture("two_keys_ship", &TWO_KEYS.replace("SHIP_KEY", "ship_to"));
    let bill = check_fixture("two_keys_bill", &TWO_KEYS.replace("SHIP_KEY", "bill_to"));
    if let (Some(ship), Some(bill)) = (ship, bill) {
        assert!(ship.contains("CrossForbidden_Order_0"), "{ship:?}");
        assert!(!bill.contains("CrossForbidden_Order_0"), "{bill:?}");
    }
}

/// `ShipBilled` checks the address it writes and ships the order (#76). The written address is
/// the order's on the key the guard's `.role(BILLED_KEY)` names only; the other key keeps its own
/// row. Billed to it, the order may ship to an address in the west, and both engines reach the
/// violation; shipped to it, the guard rules it out.
const WRITTEN_PARENT: &str = r#"
entity Address "住所" { id: Int @pk  region: Enum(east, west)  checked: Bool @default(false) }
entity Order "注文" { id: Int @pk  status: Enum(open, shipped, cancelled) @default(open) }
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
usecase AddEast "東の住所"
usecase AddWest "西の住所"
usecase Place "注文"
usecase ShipBilled "請求先を確認して出荷"
usecase Cancel "取消"
creates(AddEast, Address)
sets(AddEast, Address.region == east)
creates(AddWest, Address)
sets(AddWest, Address.region == west)
creates(Place, Order)
sets(ShipBilled, Address.checked == true)
transitions(ShipBilled, Order.status, open -> shipped)
transitions(Cancel, Order.status, open -> cancelled)
condition BillsEast "請求先が東"
holds(BillsEast, Address.region == east).role(BILLED_KEY)
applies_to(BillsEast, ShipBilled)
forbidden(Order, Address, Order.status == shipped, Address.region == west).along(Order, Address).role(ship_to)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_written_parent_on_one_key() {
    let billed = check_fixture(
        "written_bill",
        &WRITTEN_PARENT.replace("BILLED_KEY", "bill_to"),
    );
    let shipped = check_fixture(
        "written_ship",
        &WRITTEN_PARENT.replace("BILLED_KEY", "ship_to"),
    );
    if let (Some(billed), Some(shipped)) = (billed, shipped) {
        assert!(billed.contains("CrossForbidden_Order_0"), "{billed:?}");
        assert!(!shipped.contains("CrossForbidden_Order_0"), "{shipped:?}");
    }
}

/// A claim and a settlement are both children of a case (#77). `Complete` runs while the
/// settlement GUARD_PATH reads is agreed. Along the case it is the settlement of the claim's
/// own case, and both engines find the rule holds; with no path it is any settlement, and
/// both reach a completed claim of a case whose settlement is still negotiating.
const ALONG_GUARD: &str = r#"
entity Case "案件" { id: Int @pk  status: Enum(open, negotiating) @default(open) }
entity Claim "請求" { id: Int @pk  status: Enum(submitted, completed) @default(submitted) }
entity Settlement "示談" { id: Int @pk  status: Enum(negotiating, agreed) @default(negotiating) }
relate(Claim, Case, N:1)
relate(Settlement, Case, N:1)
usecase Open "受付"
usecase Submit "請求"
usecase Negotiate "示談開始"
usecase Agree "示談成立"
usecase Complete "請求完了"
creates(Open, Case)
creates(Submit, Claim)
creates(Negotiate, Settlement)
transitions(Negotiate, Case.status, open -> negotiating)
transitions(Agree, Settlement.status, negotiating -> agreed)
transitions(Complete, Claim.status, submitted -> completed)
condition Signed "示談書がある"
holds(Signed, Settlement.status == agreed)GUARD_PATH
applies_to(Signed, Complete)
forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_guard_along_a_path() {
    let along = check_fixture(
        "along_guard",
        &ALONG_GUARD.replace("GUARD_PATH", ".along(Claim, Case, Settlement)"),
    );
    let anywhere = check_fixture("unjoined_guard", &ALONG_GUARD.replace("GUARD_PATH", ""));
    if let (Some(along), Some(anywhere)) = (along, anywhere) {
        assert!(!along.contains("CrossForbidden_Claim_0"), "{along:?}");
        assert!(anywhere.contains("CrossForbidden_Claim_0"), "{anywhere:?}");
    }
}

/// `.none(Invoice)` and `.none(Invoice.status in (<every value>))` are one guard (#73): both
/// engines agree on each, and every cone has the same states under either form.
#[test]
fn none_of_an_entity_is_none_of_every_value() {
    let every =
        std::fs::read_to_string(repo().join("samples/formal-verification/none_any_child.rdra"))
            .expect("sample");
    let listed_src = every.replace(
        ".none(Invoice)",
        ".none(Invoice.status in (draft, issued, paid))",
    );
    assert_ne!(every, listed_src, "the sample guards with .none(Invoice)");
    let cones = |src: &str| -> Vec<usize> {
        let dir = std::env::temp_dir().join(format!("rdra-none-any-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let file = dir.join("m.rdra");
        std::fs::write(&file, src).expect("write model");
        let model = load(file.to_str().expect("utf-8 path"));
        let _ = std::fs::remove_dir_all(&dir);
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        ts.targets()
            .into_iter()
            .map(|t| {
                let cone = ts.target_cone(t);
                let run = ts
                    .restrict_to(&cone.vars, &cone.keep)
                    .run(Caps::new(usize::MAX, STATE_CAP));
                run.states.len()
            })
            .collect()
    };
    assert_eq!(cones(&every), cones(&listed_src));
    let direct = check_fixture("none_entity", &every);
    let listed = check_fixture("none_every_value", &listed_src);
    assert_eq!(direct, listed);
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

/// A property's cone decides what the whole model decides: an `ok` on the cone is an `ok` on
/// the whole model, a counterexample is a counterexample, and the properties the cone leaves
/// undecided are the ones expected, with the reason. Each fixture names what it exercises.
type TemporalCase = (&'static str, &'static str, &'static [(&'static str, Why)]);

const TEMPORAL_CASES: &[TemporalCase] = &[
    (
        "samples/formal-verification/order.rdra",
        "",
        &[
            ("PaidLeadsToShipped", Why::WfNext),
            ("EventuallyTerminal", Why::WfNext),
        ],
    ),
    ("samples/formal-verification/int_stock.rdra", "", &[]),
    ("samples/formal-verification/temporal_fair.rdra", "", &[]),
    ("samples/formal-verification/temporal_unmet.rdra", "", &[]),
    ("samples/formal-verification/split_run_order.rdra", "", &[]),
    (
        "independent_loop",
        INDEPENDENT_LOOP,
        &[("PaidShipsUnfair", Why::WfNext)],
    ),
    (
        "guard_reads_other",
        GUARD_READS_OTHER,
        &[("PaidShipsHalfFair", Why::WfNext)],
    ),
    ("guard_toggles", GUARD_TOGGLES, &[]),
    (
        "frees_slots",
        FREES_SLOTS,
        &[("PaidShips", Why::FreesSlots)],
    ),
    ("whole_cone", WHOLE_CONE, &[]),
    ("guard_on_unassigned", GUARD_ON_UNASSIGNED, &[]),
    (
        "copy_parent",
        COPY_PARENT,
        &[("IssueEventuallyPlain", Why::WfNext)],
    ),
    ("copy_outside", COPY_OUTSIDE, &[]),
    (
        "quantifier_chain",
        QUANTIFIER_CHAIN,
        &[
            ("OrderShips", Why::WfNext),
            ("OrderShipsPlain", Why::WfNext),
        ],
    ),
];

/// A fair use case copies its parent's kind: the copy target is in the cone, so the parent's
/// column, its owner and its creation come in with it.
const COPY_PARENT: &str = r#"
entity Application "申請" { id: Int @pk  kind: Enum(food, retail) @default(food) }
entity Permit "許可" {
  id: Int @pk
  status: Enum(draft, issued) @default(draft)
  kind: Enum(food, retail) @default(food)
}
relate(Permit, Application, N:1)
usecase File "申請"
usecase Draft "許可作成"
usecase Issue "発行"
creates(File, Application)
creates(Draft, Permit)
updates(Issue, Permit)
transitions(Issue, Permit.status, draft -> issued)
sets(Issue, Permit.kind == Application.kind)
property IssueEventually leads_to(Permit.status == draft, Permit.status == issued).fair(Issue)
property IssueEventuallyPlain leads_to(Permit.status == draft, Permit.status == issued)
"#;

/// `Pay` writes the cone and copies the tier from an optional parent: `PaidShips` leaves the
/// tiers out and drops the copy guard, which some source value meets in the whole model. Pay
/// needs a linked customer, and once `Join` takes every customer slot `Link` cannot create
/// one, so both `PendingPays` fail on the cone and on the whole model.
const COPY_OUTSIDE: &str = r#"
entity Customer "顧客" { id: Int @pk  tier: Enum(basic, gold) @default(basic) }
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  tier: Enum(basic, gold) @default(basic)
}
relate(Order, Customer, N:1).optional()
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Join "入会"
usecase Upgrade "昇格"
usecase Link "紐付け"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.tier == Customer.tier)
transitions(Ship, Order.status, paid -> shipped)
creates(Join, Customer)
sets(Upgrade, Customer.tier == gold)
creates(Link, Customer)
updates(Link, Order)
property PaidShips leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
property PendingPays leads_to(Order.status == pending, Order.status == paid).fair(Pay)
property PendingPaysLinked leads_to(Order.status == pending, Order.status == paid).fair(Pay, Link)
"#;

/// A fair use case guarded by `.none` over holds that another use case releases; the clerk
/// loops outside every cone. With `Ship` alone fair, a hold that is never released blocks the
/// order, and the cone needs the whole model's `WF_vars(Next)`, which the clerk's loop meets
/// without releasing; with `Release` fair too, every hold is released and the order ships.
const QUANTIFIER_CHAIN: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, shipped) @default(open) }
entity Hold "保留" { id: Int @pk  state: Enum(active, released) @default(active) }
entity Clerk "担当" { id: Int @pk  state: Enum(away, here) @default(away) }
relate(Hold, Order, N:1)
usecase Place "注文"
usecase PlaceHold "保留登録"
usecase Release "保留解除"
usecase Ship "出荷"
usecase Arrive "出勤"
usecase Leave "退勤"
creates(Place, Order)
creates(PlaceHold, Hold)
transitions(Release, Hold.state, active -> released)
updates(Ship, Order)
transitions(Ship, Order.status, open -> shipped)
transitions(Arrive, Clerk.state, away -> here)
transitions(Leave, Clerk.state, here -> away)
condition Clear "保留がない"
holds(Clear, Order.status == open).none(Hold.state == active)
applies_to(Clear, Ship)
property OrderShips leads_to(Order.status == open, Order.status == shipped).fair(Ship)
property OrderShipsReleased leads_to(Order.status == open, Order.status == shipped).fair(Ship, Release)
property OrderShipsPlain leads_to(Order.status == open, Order.status == shipped)
"#;

/// The cone holds every var an action writes, so its `WF_vars(Next)` is the whole model's:
/// the stutter at `paid` the plain run finds is ruled out by the `_WF` run, which decides
/// `ok` without a whole-model run.
const WHOLE_CONE: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped)
"#;

/// Another entity loops forever outside the property's cone. Without `.fair`, the cone's
/// counterexample (a stutter at `paid`) is ruled out by the cone's own `WF_vars(Next)`, so the
/// cone cannot decide; the whole model's `WF_vars(Next)` is met by the ticket's loop, and the
/// property fails there. With `.fair(Ship)` the cone decides `ok`, and the whole model agrees.
const INDEPENDENT_LOOP: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
entity Ticket "問い合わせ" { id: Int @pk  status: Enum(open, closed) @default(open) }
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Open "起票"
usecase Close "完了"
usecase Reopen "再開"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
creates(Open, Ticket)
transitions(Close, Ticket.status, open -> closed)
transitions(Reopen, Ticket.status, closed -> open)
property PaidShipsUnfair leads_to(Order.status == paid, Order.status == shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// The fair use case's guard reads another entity, which is built once and only opens. The
/// cone keeps the warehouse's state, its building, and its opening, and leaves the order's
/// note out. With the three use cases fair the cone decides `ok`; with `Ship` alone the
/// cone's stutter before the warehouse opens needs `WF_vars(Next)` to rule out, which the
/// cone cannot assume while `Annotate` writes outside it, and the whole model, where nothing
/// loops, decides `ok`.
const GUARD_READS_OTHER: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  note: Bool @default(false)
}
entity Warehouse "倉庫" { id: Int @pk  state: Enum(closed, open) @default(closed) }
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Annotate "備考"
usecase Build "開設"
usecase OpenWh "開場"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
sets(Annotate, Order.note == true)
creates(Build, Warehouse)
transitions(OpenWh, Warehouse.state, closed -> open)
condition Open "倉庫が開いている"
holds(Open, Order.status == paid, Warehouse.state == open)
applies_to(Open, Ship)
property PaidShipsFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship, Build, OpenWh)
property PaidShipsHalfFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// The same guard on a warehouse that opens and closes forever: `Ship` is enabled again and
/// again but never stays enabled, so weak fairness does not force it, and the property fails
/// on the cone (a loop) and on the whole model.
const GUARD_TOGGLES: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
entity Warehouse "倉庫" { id: Int @pk  state: Enum(closed, open) @default(closed) }
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Build "開設"
usecase OpenWh "開場"
usecase CloseWh "閉場"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
creates(Build, Warehouse)
transitions(OpenWh, Warehouse.state, closed -> open)
transitions(CloseWh, Warehouse.state, open -> closed)
condition Open "倉庫が開いている"
holds(Open, Order.status == paid, Warehouse.state == open)
applies_to(Open, Ship)
property PaidShips leads_to(Order.status == paid, Order.status == shipped).fair(Ship, OpenWh)
"#;

/// `Pay` moves the order and, in the same transaction (the receipt's key joins the writes),
/// creates a receipt the cone does not read, so the cone does not count receipt slots: its
/// loop of paying and reopening forever creates a receipt per round, which the bounded whole
/// model cannot do. The cone's counterexample is not taken, and the whole model decides.
const FREES_SLOTS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
entity Receipt "領収書" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "注文"
usecase Pay "支払い"
usecase Reopen "差し戻し"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
creates(Pay, Receipt)
transitions(Reopen, Order.status, paid -> pending)
transitions(Ship, Order.status, paid -> shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// One property run of `bundle` in `dir`; the module is printed as `Cone`.
fn run_property(tlc: &Path, dir: &Path, bundle: &tla::TlaBundle) -> (Outcome, TlcRun, String) {
    let (ok, log) = run_tlc(tlc, dir, &[], &bundle.tla, &bundle.cfg);
    let run = TlcRun::parse(&log);
    (run.outcome(ok), run, log)
}

#[test]
fn a_property_cone_decides_what_the_whole_model_decides() {
    let Some(tlc) = find_tlc() else {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped temporal cone equivalence: {reason}");
        return;
    };
    let scratch = std::env::temp_dir().join(format!("rdra-temporal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    for (name, src, expected) in TEMPORAL_CASES {
        let model = if src.is_empty() {
            load(name)
        } else {
            let file = scratch.join(format!("{name}.rdra"));
            std::fs::write(&file, src).expect("write model");
            load(file.to_str().expect("utf-8 path"))
        };
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let mut undecided = Vec::new();
        for i in 0..ts.properties().len() {
            let property = &ts.properties()[i].name;
            let label = format!("{name}: {property}");
            let cone = PropertyCone::new(&ts, i);
            let dir = scratch.join(format!("{}-{i}", name.replace('/', "_")));
            let plain = cone.bundle(ConeRun::Plain, "Cone");
            assert!(
                !plain.tla.contains("WF_vars(Next)"),
                "{label}: the plain cone run assumes WF_vars(Next)"
            );
            let (first, mut run, mut log) = run_property(&tlc, &dir.join("plain"), &plain);
            let mut verdict = cone.verdict(first, None);
            if verdict == Verdict::NeedsWfNext {
                let wf = cone.bundle(ConeRun::WfNext, "Cone");
                assert!(
                    wf.tla.contains("WF_vars(Next)"),
                    "{label}: the WF cone run lacks WF_vars(Next)"
                );
                let (second, r, l) = run_property(&tlc, &dir.join("wf"), &wf);
                verdict = cone.verdict(first, Some(second));
                (run, log) = (r, l);
            }
            let whole = tla::print(&ts, PrintMode::Property(i), "Cone");
            let (whole_outcome, _, whole_log) = run_property(&tlc, &dir.join("whole"), &whole);
            assert_ne!(
                whole_outcome,
                Outcome::ToolError,
                "{label}: whole model\n{whole_log}"
            );
            match verdict {
                Verdict::Ok => assert_eq!(
                    whole_outcome,
                    Outcome::Ok,
                    "{label}: the cone says ok, the whole model does not\n{whole_log}"
                ),
                Verdict::Counterexample => {
                    assert_eq!(
                        whole_outcome,
                        Outcome::Counterexample,
                        "{label}: the cone finds a counterexample, the whole model does not\n{log}"
                    );
                    for v in &run.violations {
                        explain_property(&cone.system, &plain.names, cone.index, v).unwrap_or_else(
                            |why| panic!("{label}: the cone's lasso does not replay: {why}\n{log}"),
                        );
                    }
                }
                Verdict::Undecided(why) => undecided.push((property.as_str(), why)),
                Verdict::NeedsWfNext | Verdict::ToolError => panic!("{label}: {verdict:?}\n{log}"),
            }
            println!(
                "{label}: cone {verdict:?} ({} states), whole {}",
                run.distinct_states.unwrap_or(0),
                whole_outcome.as_str()
            );
        }
        let expected: Vec<(&str, Why)> = expected.to_vec();
        assert_eq!(undecided, expected, "{name}: the undecided properties");
    }
    let _ = std::fs::remove_dir_all(&scratch);
}
