//! The explorer and TLC agree on every sample: for each cone a target reads, the state graph
//! TLC dumps for the printed cone (`tlc -dump dot,actionlabels`) has the BFS's states, initial
//! states, and `(from, to, action)` edges; the state-model findings (witnesses, use cases that
//! never run, deadlocks) of every entity are the same on both graphs; and the invariants and
//! after-asserts TLC finds violated are the ones the BFS found violated.
//!
//! Every sample runs at `Bounds::STATES` and `Bounds::VERIFY`, on each cone and, where the
//! cone has an independent sub-lifecycle, on the cone reduced by it as `verify` explores it
//! (`TransitionSystem::reduced_cone`): the reduced cone's BFS and TLC agree the same way, and
//! the checks it keeps have the cone's verdicts. A cone whose BFS as `verify` runs it passes
//! `STATE_CAP` at VERIFY is compared at STATES only; `VERIFY_FALLBACK` lists those cones, so a
//! cone that grows past the cap fails the test until it is listed. A cone whose unreduced BFS
//! passes the cap while its reduced one fits is compared reduced only at VERIFY;
//! `VERIFY_REDUCED_ONLY` lists those the same way.
//!
//! TLC stops at the first violation unless run with `-continue`, which explores the whole
//! state space but reports one violated check per state or step, the first in cfg order. One
//! run per cone uses `-continue` on the Safety-mode cfg (temporal properties are excluded from
//! the comparison) and dumps the graph; its distinct-state count must equal the BFS's, so it
//! is known to have finished. As `verify` does, a run with a violation is followed by reruns
//! without the checks reported so far, until a rerun reports no new one.
//!
//! Every trace TLC prints for a violation replays through the BFS step relation, and the
//! check `explain` builds at its end is the one TLC named. On a reduced cone the counterexample
//! `verify` prints is TLC's trace lifted; it replays through the unreduced cone, which explains
//! it as the same violation.
//!
//! Without `tlc` on PATH the TLC test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure. `RDRA_EQV_TRACE=1` prints one line per cone and per
//! state-model module with its sizes and wall times, to see where a run's time goes.

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rdra_ish_cli::property_cone::{AfterPlain, ConeRun, PropertyCone, Verdict, Why};
use rdra_ish_cli::tlc_batch::{self, Job, Pool};
use rdra_ish_cli::tlc_env::{current_user_heap, tlc_env, tlc_env_sized, Heap, Jit};
use rdra_ish_cli::tlc_output::{parse_dot, CanonState, InvariantScan, Outcome, TlcRun};
use rdra_ish_cli::tlc_replay::{explain_property, explain_violation};
use rdra_ish_core::lifecycle::{
    ActId, Bounds, Caps, ComponentRun, Cone, Fairness, LabelledEdges, Loop, Progress,
    PropertyVerdict, Runs, Scope, Seed, State, StateDiag, StateModelFindings, Taken, Target,
    TransitionSystem, Witness,
};
use rdra_ish_core::{load_project, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};

/// The largest cone the test compares. At `Bounds::STATES` every cone of every sample is far
/// below it. At `Bounds::VERIFY` it admits the patent-ops cones of `PatentApplication` (293,833
/// states reduced) and `Patent`, and the food-permit-ops cone of `Inspection`; the next cone
/// is 573,751.
const STATE_CAP: usize = 300_000;

/// `(sample, cone)`: the cones whose BFS at `Bounds::VERIFY`, reduced as `verify` explores
/// it, passes `STATE_CAP`, each named by the first target that reads it. Each is compared at
/// `Bounds::STATES`. docs/formal-verification.md tabulates their sizes and what keeps each
/// large.
const VERIFY_FALLBACK: &[(&str, &str)] = &[
    // 905,528: nothing reduces.
    ("samples/clinic-ops", "entity EligibilityCheck"),
    // 1,341,707 reduced: the office action times 185 classes of the application's side.
    ("samples/patent-ops", "entity OfficeAction"),
    ("samples/patent-ops", "invariant Forbidden_OfficeAction_3"),
    // 4,648,771 reduced: the permit's side times 444 classes of the disposition's (#112).
    ("samples/food-permit-ops", "entity BusinessPermit"),
    // 633,787 to 903,987 reduced: the claim and the payment times 2,428 classes of the case.
    ("samples/insurance-claim-ops", "entity Claim"),
    // Reduced by the claim's and the payment's side in 4,368 classes under the case's two
    // bits (#112); the case's side alone has 63,823 states.
    ("samples/insurance-claim-ops", "entity ClaimCase"),
    ("samples/insurance-claim-ops", "entity Payment"),
    ("samples/insurance-claim-ops", "invariant Forbidden_Claim_0"),
    ("samples/insurance-claim-ops", "invariant Forbidden_Claim_4"),
    (
        "samples/insurance-claim-ops",
        "invariant MultiForbidden_Payment_3",
    ),
    (
        "samples/insurance-claim-ops",
        "after AfterAssert_PayClaim_0",
    ),
    // Over 2,000,000, nothing reduces: the browser flow (request, interaction, session,
    // consent, grant, code, and tokens) is one cone; its verify header runs 1 instance.
    ("samples/oidc-auth", "entity AccessToken"),
    ("samples/oidc-auth", "entity AuthorizationCode"),
    ("samples/oidc-auth", "entity AuthorizationRequest"),
    ("samples/oidc-auth", "entity Consent"),
    ("samples/oidc-auth", "entity Grant"),
    ("samples/oidc-auth", "entity IdToken"),
    ("samples/oidc-auth", "entity Interaction"),
    ("samples/oidc-auth", "entity RefreshToken"),
    ("samples/oidc-auth", "invariant Forbidden_Interaction_1"),
    (
        "samples/oidc-auth",
        "invariant MultiForbidden_AuthorizationCode_3",
    ),
    ("samples/oidc-auth", "invariant Quantifier_Grant_0"),
    (
        "samples/oidc-auth",
        "invariant Unique_Grant_AuthorizationRequest",
    ),
    (
        "samples/oidc-auth",
        "invariant Unique_AuthorizationCode_Grant",
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

/// One trace line under `RDRA_EQV_TRACE=1`.
fn trace(line: impl FnOnce() -> String) {
    if std::env::var_os("RDRA_EQV_TRACE").is_some() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs();
        eprintln!(
            "trace {:02}:{:02}:{:02} {}",
            (t / 3600) % 24,
            (t / 60) % 60,
            t % 60,
            line()
        );
    }
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

/// The check a violation reports, as a target of `ts`, found by name: a reduced cone drops
/// the checks that read its sub-lifecycle, so its indices are not the cone's.
fn violated_target(ts: &TransitionSystem, diag: &StateDiag) -> Target {
    match diag {
        StateDiag::ConstraintViolated { name, .. } => Target::Invariant(
            ts.invariants()
                .iter()
                .position(|inv| inv.name == *name)
                .expect("violated invariant is in the cone"),
        ),
        StateDiag::AfterAssertViolated { site, .. } => Target::AfterAssert(
            ts.after_asserts()
                .iter()
                .position(|a| a.site == *site)
                .expect("violated assert is in the cone"),
        ),
        d => panic!("no invariant or after-assert: {d:?}"),
    }
}

/// Each trace TLC printed replays step for step, with TLC's labels, and ends in the check TLC
/// named. The counterexample `verify` prints for it is a path of `unreduced`, the cone `ts`
/// reduces (or `ts` itself), with the steps TLC took on the target side, and `unreduced`
/// explains that path as the same violation. On a reduced cone the path is longer than TLC's
/// trace: a class move becomes the sub-lifecycle's steps that realize it (#109).
fn replay_violations(
    label: &str,
    ts: &TransitionSystem,
    unreduced: &TransitionSystem,
    names: &TlaNames,
    run: &TlcRun,
) {
    let s_actions = ts
        .reduction()
        .map(|r| r.s_actions().clone())
        .unwrap_or_default();
    let target_side = |acts: &[ActId]| -> Vec<ActId> {
        acts.iter()
            .copied()
            .filter(|a| !s_actions.contains(a))
            .collect()
    };
    for v in &run.violations {
        let name = v.name.as_deref().expect("a safety run names its checks");
        let diag = explain_violation(ts, names, v)
            .unwrap_or_else(|why| panic!("{label}: TLC trace of {name} does not replay: {why}"));
        assert_eq!(
            verdict_name(ts, names, &diag).as_deref(),
            Some(name),
            "{label}: explain names another check"
        );
        let states: Vec<State> = v
            .trace
            .iter()
            .map(|s| names.decode(ts, &s.state).expect("a state of the cone"))
            .collect();
        let labels: Vec<ActId> = v.trace[1..]
            .iter()
            .map(|s| names.action_index(ts, &s.label).expect("a known label"))
            .collect();
        let replay = ts.replay(states, &labels).expect("TLC's trace replays");
        let (path, steps) = ts.lifted_path(&replay);
        assert_eq!(
            target_side(&steps),
            target_side(&labels),
            "{label}: {name} replays another path"
        );
        let on_cone = unreduced.replay(path, &steps).unwrap_or_else(|e| {
            panic!("{label}: the counterexample of {name} is no path of the unreduced cone: {e:?}")
        });
        assert_eq!(
            unreduced.explain(violated_target(unreduced, &diag), &on_cone),
            Some(diag),
            "{label}: the unreduced cone explains the counterexample of {name} otherwise"
        );
    }
}

struct ConeReport {
    states: usize,
    /// The invariants and after-asserts of the printed cone, by TLA name.
    checks: BTreeSet<String>,
    violated: BTreeSet<String>,
}

/// Where a TLC run goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    /// A JVM of [`batch_pool`], when there is one.
    Batch,
    /// A process of its own: a run known to be long, which gains nothing from a JVM already
    /// up and would hold one of the pool's past [`BATCH_LIMIT`].
    Process,
}

/// A cone of more states than this starts TLC as a process of its own. The 18 sample cones
/// of 2,500 to 5,000 states take 1.1 s each in a JVM that is up (2.1 s at most) and 1.6 s as
/// a process; past that, the 0.6 s a JVM start costs is a small part of the run.
const BATCH_STATES: usize = 5_000;

/// How long a job may take in a batch JVM before the JVM is killed and the job runs as a
/// process. The cones are held to [`BATCH_STATES`] and the whole-model property runs are
/// processes, but a state model's size is not known when its run starts: the longest job
/// took 79 and 93 s in two runs on a 4-core CI runner, where 16 threads share the cores.
const BATCH_LIMIT: Duration = Duration::from_secs(300);

/// The heap of a batch JVM, unless the user names one (`RDRA_TLC_HEAP`, or `-Xmx` in
/// `JAVA_TOOL_OPTIONS`). A JVM that stays up commits its whole `-Xmx`, whatever its jobs
/// need, and about 300 MiB beside the heap: 16 JVMs of the 1 GiB a process gets held
/// 13.5 GB together.
///
/// A JVM keeps the data of the job before, and at times of the two before, past a GC: on
/// food-permit-ops' `StateModel_Disposition` witnesses, about 100 MB a job on TLC 1.7.4 and
/// 1.8.0 alike. At 256 MiB that left no room for the next such job: on TLC 1.8.0 and JDK
/// 25, which kept two jobs' data on 8 of 12 runs of it where 1.7.4 kept it on 4, one job of
/// the test ran out of memory in two runs of four.
const BATCH_HEAP: u64 = 512 << 20;

/// The batch JVMs of this test binary (#310), or `None` without `tlc-batch` on PATH or with
/// `RDRA_TLC_BATCH=0`: every run is then a process. `RDRA_TLC_BATCH=both` runs every job
/// both ways and compares them ([`assert_same_run`]); `RDRA_TLC_BATCH=twice` runs it as a
/// process twice and compares those, which shows what two runs of one job differ in with no
/// batch JVM involved.
fn batch_pool() -> Option<&'static Pool> {
    static POOL: OnceLock<Option<Pool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let launcher = tlc_batch::launcher()?;
        // A pool in a static is never dropped, so the directories of a run stay; the next
        // run removes those a day old.
        let temp = std::env::temp_dir();
        for old in std::fs::read_dir(&temp).into_iter().flatten().flatten() {
            let stale = old
                .file_name()
                .to_string_lossy()
                .starts_with("rdra-tlc-batch-")
                && old
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|at| at.elapsed().is_ok_and(|age| age.as_secs() > 86_400));
            if stale {
                let _ = std::fs::remove_dir_all(old.path());
            }
        }
        let root = temp.join(format!("rdra-tlc-batch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let started = AtomicUsize::new(0);
        // As many JVMs as one test's workers; a job waits a second or two for one at most.
        let limit = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
        Some(Pool::new(limit, move || {
            // TLC unpacks its standard modules into java.io.tmpdir; the JVMs must not share it.
            let tmp = root.join(started.fetch_add(1, Ordering::SeqCst).to_string());
            std::fs::create_dir_all(&tmp).expect("java tmp dir");
            let heap = current_user_heap().unwrap_or(BATCH_HEAP);
            let (key, value) = tlc_env_sized(heap, &tmp, Jit::Tiered);
            let log = std::fs::File::create(tmp.join("jvm.err")).expect("jvm log");
            let mut command = Command::new(&launcher);
            command.env(key, value).stderr(log);
            command
        }))
    })
    .as_ref()
}

/// Prints what the batch JVMs did so far. With `RDRA_REQUIRE_TLC_BATCH=1` (CI), a test
/// binary that ran no TLC job in a batch JVM, or lost one, fails: a driver that stops
/// working would otherwise only make the test slower. A job that passed [`BATCH_LIMIT`] is
/// not such a loss: it ran as a process, as a slow run on a loaded runner should.
fn assert_batch_ran() {
    let required = std::env::var("RDRA_REQUIRE_TLC_BATCH").as_deref() == Ok("1");
    let Some(pool) = batch_pool() else {
        assert!(
            !required,
            "RDRA_REQUIRE_TLC_BATCH=1 but tlc-batch is not on PATH"
        );
        return;
    };
    let counts = pool.counts();
    println!(
        "TLC batch JVMs: {counts:?}; jobs compared with a process: {}",
        BOTH_WAYS.load(Ordering::SeqCst)
    );
    assert!(
        !required || (counts.answered > 0 && pool.sound()),
        "TLC batch JVMs lost jobs: {counts:?}, {:?}",
        pool.why_off()
    );
}

fn run_tlc(tlc: &Path, dir: &Path, args: &[&str], tla: &str, cfg: &str) -> (bool, String) {
    run_tlc_on(Route::Batch, batch_pool(), tlc, dir, args, tla, cfg)
}

/// One TLC run of `tla` under `cfg` in `dir`: whether TLC exited 0, and what it printed.
/// Every path TLC gets is absolute, because a batch JVM does not run in `dir`. A job a batch
/// JVM does not answer runs as a process.
fn run_tlc_on(
    route: Route,
    pool: Option<&Pool>,
    tlc: &Path,
    dir: &Path,
    args: &[&str],
    tla: &str,
    cfg: &str,
) -> (bool, String) {
    std::fs::create_dir_all(dir).expect("work dir");
    std::fs::write(dir.join("Cone.tla"), tla).expect("write tla");
    std::fs::write(dir.join("Cone.cfg"), cfg).expect("write cfg");
    let mut all: Vec<OsString> = vec!["-workers".into(), "1".into(), "-metadir".into()];
    all.push(dir.join("meta").into());
    all.extend(args.iter().map(OsString::from));
    all.push("-config".into());
    all.push(dir.join("Cone.cfg").into());
    all.push(dir.join("Cone.tla").into());
    tlc_run(route, pool.map(|p| (p, BATCH_LIMIT)), tlc, dir, &all)
}

/// TLC on `args`, whose files are in `dir`, in a JVM of the pool when it answers within the
/// pool's limit.
fn tlc_run(
    route: Route,
    pool: Option<(&Pool, Duration)>,
    tlc: &Path,
    dir: &Path,
    all: &[OsString],
) -> (bool, String) {
    let Some((pool, limit)) = pool.filter(|_| route == Route::Batch) else {
        return tlc_process(tlc, dir, all);
    };
    if std::env::var(tlc_batch::BATCH_ENV).as_deref() == Ok("twice") {
        let first = tlc_process(tlc, dir, all);
        let graph = graph_shape(dir);
        let alone = tlc_process(tlc, dir, &with_the_fingerprints_of(&first.1, all));
        assert_same_run(dir, (&first, &graph), (&alone, &graph_shape(dir)));
        return alone;
    }
    let batched = tlc_batched((pool, limit), dir, all);
    if std::env::var(tlc_batch::BATCH_ENV).as_deref() != Ok("both") {
        return batched.unwrap_or_else(|| tlc_process(tlc, dir, all));
    }
    let Some(batched) = batched else {
        return tlc_process(tlc, dir, all);
    };
    let graph = graph_shape(dir);
    let alone = tlc_process(tlc, dir, &with_the_fingerprints_of(&batched.1, all));
    assert_same_run(dir, (&batched, &graph), (&alone, &graph_shape(dir)));
    alone
}

/// `args` with the fingerprint function and seed the run of `log` drew (`-fp`, `-seed`). TLC
/// draws both anew for a run that names neither, and they decide the order it stores states
/// in, and with it which counterexample to a temporal property it prints.
fn with_the_fingerprints_of(log: &str, args: &[OsString]) -> Vec<OsString> {
    let drawn = |name: &str| {
        let words: Vec<&str> = log
            .lines()
            .find(|l| l.starts_with("Running breadth-first search Model-Checking with fp "))?
            .split(' ')
            .collect();
        let at = words.iter().position(|w| *w == name)?;
        words.get(at + 1).map(|w| OsString::from(*w))
    };
    let (Some(fp), Some(seed)) = (drawn("fp"), drawn("seed")) else {
        return args.to_vec();
    };
    let mut pinned = vec!["-fp".into(), fp, "-seed".into(), seed];
    pinned.extend(args.iter().cloned());
    pinned
}

/// Removes what an earlier run of the job left, which a run that was killed may have half
/// written.
fn clear_run(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join("meta"));
    let _ = std::fs::remove_file(dir.join("graph.dot"));
}

fn tlc_process(tlc: &Path, dir: &Path, args: &[OsString]) -> (bool, String) {
    clear_run(dir);
    // TLC unpacks its standard modules into java.io.tmpdir; parallel runs must not share it.
    let tmp = dir.join("java-tmp");
    std::fs::create_dir_all(&tmp).expect("java tmp dir");
    let (key, value) = tlc_env(Heap::Cone, &tmp);
    let out = Command::new(tlc)
        .env(key, value)
        .args(args)
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

/// The run in a JVM of `pool`, with TLC's stdout before its stderr as [`tlc_process`] has
/// them; `None` when the job got no answer within `limit`.
fn tlc_batched(
    (pool, limit): (&Pool, Duration),
    dir: &Path,
    args: &[OsString],
) -> Option<(bool, String)> {
    clear_run(dir);
    let (stdout, stderr) = (dir.join("tlc.out"), dir.join("tlc.err"));
    let job = Job {
        stdout: &stdout,
        stderr: &stderr,
        args,
    };
    let status = pool.run(&job, limit)?;
    let read = |path: &Path| {
        String::from_utf8_lossy(&std::fs::read(path).expect("TLC's output")).into_owned()
    };
    Some((status == 0, format!("{}{}", read(&stdout), read(&stderr))))
}

/// The graph a run dumped, in an order of its own: the states with whether each is initial,
/// and the edges between them. TLC names a state by its fingerprint, which is drawn anew on
/// every run.
type GraphShape = (Vec<(CanonState, bool)>, Vec<(usize, usize, String)>);

fn graph_shape(dir: &Path) -> Option<GraphShape> {
    let file = std::fs::File::open(dir.join("graph.dot")).ok()?;
    let dot = parse_dot(BufReader::new(file), |canon| Some(canon.clone())).expect("graph.dot");
    let mut order: Vec<usize> = (0..dot.nodes.len()).collect();
    order.sort_by(|&a, &b| dot.nodes[a].state.cmp(&dot.nodes[b].state));
    let mut at = vec![0; order.len()];
    for (to, &from) in order.iter().enumerate() {
        at[from] = to;
    }
    let mut edges: Vec<(usize, usize, String)> = dot
        .edges
        .iter()
        .map(|&(a, b, l)| {
            (
                at[a as usize],
                at[b as usize],
                dot.labels[l as usize].clone(),
            )
        })
        .collect();
    edges.sort();
    let nodes = order
        .iter()
        .map(|&i| (dot.nodes[i].state.clone(), dot.nodes[i].initial))
        .collect();
    Some((nodes, edges))
}

/// A TLC log without what two runs of one job with one fingerprint function and seed do not
/// share, whichever way they are started: the JVM's own first line, the dates, the time a
/// run took, the process id, the directory the standard modules were unpacked into, and what
/// TLC does on a timer, which is a progress line and, with a temporal property, a check of
/// the states found until then (a run that ends before the timer has neither, and a slower
/// one of the same job has them at whatever count it had reached; the check of the complete
/// state space is always printed). One more differs between a batch JVM and a
/// process: the heap TLC reports, since a batch JVM has [`BATCH_HEAP`] and not the heap of a
/// process (and at one `-Xmx` the two still report different sizes, 966MB and 981MB of
/// 1 GiB: the figure is the JVM's `maxMemory`, which leaves out a survivor space that the
/// parallel collector has resized in a JVM that has run jobs).
fn without_run_noise(log: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for line in log.lines() {
        let interim = line.starts_with("Checking ")
            && line.contains(" of temporal properties for the current state space ");
        if line.starts_with("Picked up JAVA_TOOL_OPTIONS:")
            || line.starts_with("Progress(")
            || interim
            || line.starts_with("Finished checking temporal properties in ")
        {
            continue;
        }
        let mut line = without_dates(line);
        if line.starts_with("Running breadth-first search Model-Checking with fp ") {
            let mut words: Vec<&str> = line.split(' ').collect();
            for i in 1..words.len() {
                if words[i - 1] == "[pid:" || words.get(i + 1) == Some(&"heap") {
                    words[i] = "N";
                }
            }
            line = words.join(" ");
        }
        if let (true, Some(from), Some(to)) = (
            line.starts_with("Finished"),
            line.find(" in "),
            line.find(" at "),
        ) {
            line.replace_range(from..to, " in T");
        }
        // A standard module, unpacked into the run's or the batch JVM's java.io.tmpdir.
        if let Some(module) = line.strip_prefix("Parsing file ") {
            if module.contains("/java-tmp/") || module.contains("/rdra-tlc-batch-") {
                let name = Path::new(module).file_name().expect("a module file");
                line = format!("Parsing file <tmp>/{}", name.to_string_lossy());
            }
        }
        lines.push(line);
    }
    lines
}

/// `line` with each `2026-11-25 02:20:47` as `<date>`.
fn without_dates(line: &str) -> String {
    const SHAPE: &[u8] = b"dddd-dd-dd dd:dd:dd";
    let bytes = line.as_bytes();
    let is_date = |at: usize| {
        bytes.len() >= at + SHAPE.len()
            && SHAPE.iter().zip(&bytes[at..]).all(|(s, b)| {
                if *s == b'd' {
                    b.is_ascii_digit()
                } else {
                    s == b
                }
            })
    };
    let mut out = String::new();
    let mut at = 0;
    while at < bytes.len() {
        if is_date(at) {
            out.push_str("<date>");
            at += SHAPE.len();
        } else {
            let next = (at + 1..=bytes.len())
                .find(|i| line.is_char_boundary(*i))
                .expect("a boundary");
            out.push_str(&line[at..next]);
            at = next;
        }
    }
    out
}

/// The jobs [`assert_same_run`] compared.
static BOTH_WAYS: AtomicUsize = AtomicUsize::new(0);

/// A job run in a batch JVM and as a process gave the same exit, the same log up to
/// [`without_run_noise`], the same parsed run and invariant scan, and the same dumped graph.
fn assert_same_run(
    dir: &Path,
    batched: (&(bool, String), &Option<GraphShape>),
    alone: (&(bool, String), &Option<GraphShape>),
) {
    let ((b_ok, b_log), b_graph) = batched;
    let ((a_ok, a_log), a_graph) = alone;
    let at = dir.display();
    assert_eq!(
        b_ok, a_ok,
        "{at}: exit differs (left batch, right process)\n{b_log}\n{a_log}"
    );
    let (b_lines, a_lines) = (without_run_noise(b_log), without_run_noise(a_log));
    if b_lines != a_lines {
        let differ: Vec<String> = b_lines
            .iter()
            .zip(&a_lines)
            .filter(|(b, a)| b != a)
            .take(5)
            .map(|(b, a)| format!("batch:   {b}\nprocess: {a}"))
            .collect();
        panic!(
            "{at}: logs differ ({} lines in batch, {} as a process)\n{}",
            b_lines.len(),
            a_lines.len(),
            differ.join("\n")
        );
    }
    assert_eq!(
        format!("{:?}", TlcRun::parse(b_log)),
        format!("{:?}", TlcRun::parse(a_log)),
        "{at}: parsed runs differ (left batch, right process)"
    );
    assert_eq!(
        InvariantScan::parse(b_log),
        InvariantScan::parse(a_log),
        "{at}: invariant scans differ (left batch, right process)"
    );
    assert!(b_graph == a_graph, "{at}: dumped graphs differ");
    BOTH_WAYS.fetch_add(1, Ordering::SeqCst);
}

/// A module with one violated invariant, for the tests of the batch JVMs themselves.
const COUNTER_TLA: &str = "---- MODULE Cone ----\nEXTENDS Naturals\nVARIABLE x\nInit == x = 0\n\
    Next == x' = (x + 1) % 4\nSmall == x < 3\n====\n";
const COUNTER_CFG: &str = "INIT Init\nNEXT Next\nINVARIANT Small\n";
/// With the fingerprint function and seed named, so that two runs print the same log.
const COUNTER_ARGS: [&str; 5] = ["-continue", "-fp", "0", "-seed", "0"];

/// A pool whose driver, compiled from `nix/tlc-batch/RdraTlcBatch.java`, is given the classes
/// of `fake` (file name and source) in place of `tla2tools.jar`. `None`, with the reason
/// printed, without `javac` and `java` on PATH.
fn pool_over_fake_tlc(dir: &Path, fake: &[(&str, String)]) -> Option<Pool> {
    let javac = |out: &Path, sources: &[PathBuf]| {
        std::fs::create_dir_all(out).expect("class dir");
        let done = Command::new("javac")
            .arg("-d")
            .arg(out)
            .args(sources)
            .output();
        match done {
            Ok(done) => assert!(
                done.status.success(),
                "javac: {}",
                String::from_utf8_lossy(&done.stderr)
            ),
            Err(_) => return false,
        }
        true
    };
    let driver = dir.join("driver");
    let source = repo().join("nix/tlc-batch/RdraTlcBatch.java");
    if !javac(&driver, &[source]) {
        let reason = "javac not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped: {reason}");
        return None;
    }
    let sources: Vec<PathBuf> = fake
        .iter()
        .map(|(file, text)| {
            let path = dir.join("fake-src").join(file);
            std::fs::create_dir_all(path.parent().expect("a package dir")).expect("source dir");
            std::fs::write(&path, text).expect("write source");
            path
        })
        .collect();
    let classes = dir.join("fake");
    assert!(javac(&classes, &sources), "javac ran a moment ago");
    Some(Pool::new(1, move || {
        let mut command = Command::new("java");
        command
            .arg("-cp")
            .arg(&driver)
            .arg("RdraTlcBatch")
            .arg(&classes)
            .stderr(std::process::Stdio::null());
        command
    }))
}

/// The classes the driver asks of a TLC, with `process` as the body of `tlc2.TLC.process()`.
fn fake_tlc(process: &str) -> Vec<(&'static str, String)> {
    let tlc = format!(
        "package tlc2; public class TLC {{ \
         public boolean handleParameters(String[] a) {{ return true; }} \
         public void setResolver(util.FilenameToStream r) {{}} \
         public String getMainFile() {{ return \"Cone\"; }} \
         public int process() throws Exception {{ {process} }} }}"
    );
    let rest = [
        (
            "tlc2/output/EC.java",
            "package tlc2.output; public class EC { public static class ExitStatus { \
             public static int errorConstantToExitStatus(int c) { return c; } } }",
        ),
        (
            "util/FilenameToStream.java",
            "package util; public interface FilenameToStream {}",
        ),
        (
            "util/SimpleFilenameToStream.java",
            "package util; public class SimpleFilenameToStream implements FilenameToStream { \
             public SimpleFilenameToStream() {} public SimpleFilenameToStream(String d) {} }",
        ),
        (
            "util/FileUtil.java",
            "package util; public class FileUtil { \
             public static String parseDirname(String f) { return \"\"; } }",
        ),
    ];
    std::iter::once(("tlc2/TLC.java", tlc))
        .chain(rest.map(|(file, text)| (file, text.to_string())))
        .collect()
}

/// A job past its time limit runs as a process, and is a slow run, not a driver that fails:
/// the pool stays sound, as `RDRA_REQUIRE_TLC_BATCH=1` asks, when one job passes the limit
/// and when three in a row do and the pool turns off.
#[test]
fn a_job_past_the_time_limit_runs_as_a_process_and_is_no_fault() {
    let Some(tlc) = require_tlc() else { return };
    let dir = std::env::temp_dir().join(format!("rdra-batch-slow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A `process` that outlasts the limit below.
    let fake = fake_tlc("Thread.sleep(600000); return 0;");
    let Some(pool) = pool_over_fake_tlc(&dir, &fake) else {
        return;
    };
    let run = dir.join("run");
    let alone = run_tlc_on(
        Route::Process,
        None,
        &tlc,
        &run,
        &COUNTER_ARGS,
        COUNTER_TLA,
        COUNTER_CFG,
    );
    let mut all: Vec<OsString> = vec!["-workers".into(), "1".into(), "-metadir".into()];
    all.push(run.join("meta").into());
    all.extend(COUNTER_ARGS.iter().map(OsString::from));
    all.push("-config".into());
    all.push(run.join("Cone.cfg").into());
    all.push(run.join("Cone.tla").into());
    let limit = Duration::from_secs(2);
    for lost in 1..=3 {
        let fell_back = tlc_run(Route::Batch, Some((&pool, limit)), &tlc, &run, &all);
        assert_same_run(&run, (&fell_back, &None), (&alone, &None));
        assert!(TlcRun::parse(&fell_back.1).completed, "{}", fell_back.1);
        assert_eq!(
            pool.counts(),
            tlc_batch::Counts {
                jvms: lost,
                answered: 0,
                lost,
                timed_out: lost,
                longest_ms: 0
            }
        );
        assert!(pool.sound(), "{:?}", pool.why_off());
    }
    let why = pool.why_off().expect("the pool is off");
    assert!(why.contains("TimedOut"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The counter module through `pool`, whose jobs get no answer, and then as a process: the
/// first must have become a process run with the second's result.
fn falls_back_to_a_process(tlc: &Path, dir: &Path, pool: &Pool) {
    let run = dir.join("run");
    let fell_back = run_tlc_on(
        Route::Batch,
        Some(pool),
        tlc,
        &run,
        &COUNTER_ARGS,
        COUNTER_TLA,
        COUNTER_CFG,
    );
    let alone = run_tlc_on(
        Route::Process,
        None,
        tlc,
        &run,
        &COUNTER_ARGS,
        COUNTER_TLA,
        COUNTER_CFG,
    );
    assert_same_run(&run, (&fell_back, &None), (&alone, &None));
    let parsed = TlcRun::parse(&fell_back.1);
    assert!(parsed.completed, "{}", fell_back.1);
    assert_eq!(
        parsed.violated_names(),
        BTreeSet::from(["Small".to_string()])
    );
    assert_eq!(pool.counts().answered, 0);
}

#[test]
fn a_tlc_the_batch_driver_cannot_call_runs_as_a_process() {
    let Some(tlc) = require_tlc() else { return };
    let dir = std::env::temp_dir().join(format!("rdra-batch-unsupported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A `tlc2.TLC` without `handleParameters`, `setResolver` and `process`.
    let fake = [(
        "tlc2/TLC.java",
        "package tlc2; public class TLC { public static void main(String[] a) {} }".to_string(),
    )];
    let Some(pool) = pool_over_fake_tlc(&dir, &fake) else {
        return;
    };
    falls_back_to_a_process(&tlc, &dir, &pool);
    let why = pool.why_off().expect("the pool is off");
    assert!(
        why.starts_with("unsupported ") && why.contains("handleParameters"),
        "{why}"
    );
    assert_eq!(pool.counts(), tlc_batch::Counts::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_job_that_exits_the_batch_jvm_runs_as_a_process() {
    let Some(tlc) = require_tlc() else { return };
    let dir = std::env::temp_dir().join(format!("rdra-batch-exit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A `process` that exits 0: only the answer that does not come says the job was not run.
    let fake = fake_tlc("System.exit(0); return 0;");
    let Some(pool) = pool_over_fake_tlc(&dir, &fake) else {
        return;
    };
    falls_back_to_a_process(&tlc, &dir, &pool);
    assert_eq!(
        pool.counts(),
        tlc_batch::Counts {
            jvms: 1,
            answered: 0,
            lost: 1,
            timed_out: 0,
            longest_ms: 0
        }
    );
    assert_eq!(pool.why_off(), None);
    assert!(!pool.sound());
    // The pool gives up after three jobs in a row without an answer.
    falls_back_to_a_process(&tlc, &dir, &pool);
    falls_back_to_a_process(&tlc, &dir, &pool);
    let why = pool.why_off().expect("the pool is off");
    assert!(why.contains("Died"), "{why}");
    falls_back_to_a_process(&tlc, &dir, &pool);
    assert_eq!(pool.counts().jvms, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Not for CI: replays TLC runs recorded from another caller (`verify --with-tlc` under a
/// `tlc` that saves each launch) in a batch JVM and as a process, and compares each pair.
/// Reads `RDRA_TLC_REPLAY`, a directory of job directories, each with the run's `.tla` and
/// `.cfg` files and `args`, one TLC argument per line as `verify` gave them. Every run is
/// replayed with one worker: with several, two process runs of one job differ in the
/// counterexample they print and in the graph statistics, whatever the fingerprints.
#[test]
#[ignore]
fn recorded_tlc_runs_are_the_same_in_a_batch_jvm() {
    let Some(tlc) = require_tlc() else { return };
    let root = PathBuf::from(std::env::var("RDRA_TLC_REPLAY").expect("RDRA_TLC_REPLAY"));
    let pool = batch_pool().expect("tlc-batch on PATH");
    let mut jobs: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("the recording")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("args").is_file())
        .collect();
    jobs.sort();
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                while let Some(dir) = jobs.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let recorded = std::fs::read_to_string(dir.join("args")).expect("args");
                    let mut all: Vec<OsString> = Vec::new();
                    let mut before = "";
                    let last = recorded.lines().count() - 1;
                    for (i, arg) in recorded.lines().enumerate() {
                        all.push(match before {
                            "-metadir" => dir.join("meta").into(),
                            "-workers" => "1".into(),
                            "-config" => dir.join(arg).into(),
                            _ if i == last => dir.join(arg).into(),
                            _ => arg.into(),
                        });
                        before = arg;
                    }
                    let batched = tlc_batched((pool, BATCH_LIMIT), dir, &all).expect("an answer");
                    let pinned = with_the_fingerprints_of(&batched.1, &all);
                    let alone = tlc_process(&tlc, dir, &pinned);
                    assert_same_run(dir, (&batched, &None), (&alone, &None));
                }
            });
        }
    });
    println!(
        "{} recorded runs; TLC batch JVMs: {:?}; compared: {}",
        jobs.len(),
        pool.counts(),
        BOTH_WAYS.load(Ordering::SeqCst)
    );
}

/// TLC's state graph of the cone in BFS terms: its edges as BFS state indices and actions,
/// sorted and without repeats. Panics where TLC's states or initial states differ from the
/// BFS's.
fn tlc_edges(dir: &Path, label: &str, run: &ComponentRun, names: &TlaNames) -> Vec<Move> {
    let ts = &run.system;
    let index: HashMap<State, u32> = run
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.into_state(), i as u32))
        .collect();
    let decoder = names.decoder(ts);
    let file = std::fs::File::open(dir.join("graph.dot")).expect("graph.dot");
    let dot = parse_dot(BufReader::new(file), |canon| {
        let state = decoder.decode(canon)?;
        index.get(&*state).copied()
    })
    .unwrap_or_else(|why| panic!("{label}: TLC's graph does not map onto the BFS's: {why}"));
    let at: Vec<u32> = dot.nodes.iter().map(|n| n.state).collect();
    let distinct: BTreeSet<u32> = at.iter().copied().collect();
    if distinct.len() != at.len() || distinct.len() != run.states.len() {
        let only_bfs: Vec<_> = (0..run.states.len() as u32)
            .filter(|i| !distinct.contains(i))
            .take(3)
            .map(|i| names.canon(ts, &run.states.at(i as usize)))
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
            let pre = names.canon(ts, &run.states.at(from as usize));
            let post = names.canon(ts, &run.states.at(to as usize));
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

/// `unreduced` is the cone `run` reduces, or `run`'s own system when it is not reduced.
fn check_cone(
    tlc: &Path,
    dir: &Path,
    label: &str,
    run: &ComponentRun,
    unreduced: &TransitionSystem,
) -> ConeReport {
    let ts = &run.system;
    let check = tla::print(ts, PrintMode::Safety, "Cone");
    let names = &check.names;
    let route = if run.states.len() > BATCH_STATES {
        Route::Process
    } else {
        Route::Batch
    };
    let graph = dir.join("graph");
    let (_, log) = run_tlc_on(
        route,
        batch_pool(),
        tlc,
        dir,
        &[
            "-continue",
            "-dump",
            "dot,actionlabels",
            graph.to_str().expect("a UTF-8 temp dir"),
        ],
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
        let (_, more) = run_tlc_on(
            route,
            batch_pool(),
            tlc,
            dir,
            &["-continue"],
            &check.tla,
            &cfg,
        );
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
    replay_violations(label, ts, unreduced, names, &parsed);
    let checks = (0..ts.invariants().len())
        .map(|i| names.invariant(i).to_string())
        .chain((0..ts.after_asserts().len()).map(|i| names.after(i).to_string()))
        .collect();
    ConeReport {
        states: run.states.len(),
        checks,
        violated,
    }
}

/// `(sample, cone)`: the cones whose unreduced BFS at `Bounds::VERIFY` passes `STATE_CAP`
/// while the cone reduced as `verify` explores it fits. Each is compared with TLC reduced only,
/// where TLC reads the quotient tables BFS computed, so the comparison says nothing
/// independent of the reduction there; the unreduced cone is compared at `Bounds::STATES`.
const VERIFY_REDUCED_ONLY: &[(&str, &str)] = &[
    ("samples/clinic-ops", "entity CarePlan"),
    ("samples/clinic-ops", "entity Charge"),
    ("samples/clinic-ops", "entity ClinicalOrder"),
    ("samples/clinic-ops", "entity FollowUpMessage"),
    ("samples/clinic-ops", "entity LabResult"),
    ("samples/clinic-ops", "entity Notification"),
    ("samples/clinic-ops", "entity PatientMessage"),
    ("samples/clinic-ops", "entity Prescription"),
    ("samples/clinic-ops", "invariant Forbidden_ClinicalOrder_1"),
    ("samples/clinic-ops", "invariant Inv_ClinicalOrder_3"),
    ("samples/clinic-ops", "invariant Inv_Prescription_4"),
    (
        "samples/clinic-ops",
        "invariant Unique_FollowUpMessage_CarePlan",
    ),
    ("samples/patent-ops", "entity DivisionalFiling"),
    ("samples/patent-ops", "entity Patent"),
    ("samples/patent-ops", "entity PatentApplication"),
    (
        "samples/patent-ops",
        "invariant Forbidden_PatentApplication_1",
    ),
    ("samples/patent-ops", "invariant Forbidden_Patent_4"),
    ("samples/patent-ops", "invariant Inv_Patent_0"),
    ("samples/food-permit-ops", "entity ChangeNotice"),
    ("samples/food-permit-ops", "entity Disposition"),
    ("samples/food-permit-ops", "entity Inspection"),
    ("samples/food-permit-ops", "entity PermitApplication"),
    ("samples/food-permit-ops", "entity SuccessionNotice"),
    (
        "samples/food-permit-ops",
        "invariant Forbidden_Disposition_2",
    ),
    ("samples/food-permit-ops", "invariant Inv_Disposition_0"),
    (
        "samples/food-permit-ops",
        "after AfterAssert_SuspendBusiness_0",
    ),
    ("samples/insurance-claim-ops", "entity Complaint"),
    ("samples/insurance-claim-ops", "entity Subrogation"),
    (
        "samples/insurance-claim-ops",
        "invariant MultiForbidden_Subrogation_7",
    ),
    (
        "samples/insurance-claim-ops",
        "after AfterAssert_StartSubrogation_1",
    ),
];

/// Named to sort first: libtest starts tests in name order, and this one is the longest in the
/// file, so it starts at once and the others fill the threads beside it (#305). On a runner's
/// 4 test threads, `a_corrupted_tlc_trace_does_not_replay`, `a_property_cone_decides_what_the_whole_model_decides`
/// and `a_tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds`, the next longest,
/// start beside it.
#[test]
fn a_bfs_and_tlc_reach_the_same_states_and_verdicts() {
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
    let samples = samples();
    let plans: Vec<ModelPlan> = samples
        .iter()
        .flat_map(|sample| {
            let model = load(sample);
            [Bounds::STATES, Bounds::VERIFY].map(|bounds| plan_model(sample, &model, bounds))
        })
        .collect();
    // Every cone of every sample at both bounds in one queue, so a sample's longest cone
    // runs beside the next sample's cones rather than before them.
    let reports = check_models(&tlc, &scratch, &plans);
    let _ = std::fs::remove_dir_all(&scratch);
    assert_batch_ran();
    let mut fallback = Vec::new();
    let mut reduced_only = Vec::new();
    for (plan, report) in plans.iter().zip(reports) {
        if plan.bounds == Bounds::STATES {
            assert!(
                report.capped.is_empty() && report.reduced_only.is_empty(),
                "{}: a cone passes the state cap at STATES",
                plan.sample
            );
        } else {
            fallback.extend(report.capped.into_iter().map(|c| (plan.sample.clone(), c)));
            reduced_only
                .extend((report.reduced_only.into_iter()).map(|c| (plan.sample.clone(), c)));
        }
    }
    let listed: Vec<(String, String)> = VERIFY_REDUCED_ONLY
        .iter()
        .map(|(s, c)| (s.to_string(), c.to_string()))
        .collect();
    assert_eq!(
        reduced_only, listed,
        "the cones compared reduced only at VERIFY (the unreduced cone passes the state cap) are not VERIFY_REDUCED_ONLY"
    );
    let expected: Vec<(String, String)> = VERIFY_FALLBACK
        .iter()
        .map(|(s, c)| (s.to_string(), c.to_string()))
        .collect();
    assert_eq!(
        fallback, expected,
        "the cones past the state cap at VERIFY are not VERIFY_FALLBACK (docs/formal-verification.md lists them)"
    );
}

/// A root with a cone this file compares at `Bounds::STATES` only runs TLC at its header bounds
/// in formal-verify's `verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt` step,
/// which runs TLC on the roots that file lists (#305).
#[test]
fn every_fallback_root_runs_tlc_in_formal_verify() {
    let listed: BTreeSet<String> = std::fs::read_to_string(repo().join("scripts/tlc-roots.txt"))
        .expect("scripts/tlc-roots.txt")
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let missing: BTreeSet<&str> = VERIFY_FALLBACK
        .iter()
        .map(|(sample, _)| *sample)
        .filter(|sample| !listed.contains(*sample))
        .collect();
    assert!(
        missing.is_empty(),
        "{missing:?} have a cone in VERIFY_FALLBACK but are not in scripts/tlc-roots.txt"
    );
}

/// The first target that reads a cone, as the cone's name.
fn target_label(ts: &TransitionSystem, t: Target) -> String {
    match t {
        Target::Entity(e) => format!("entity {}", ts.entity(e).id),
        Target::Invariant(i) => format!("invariant {}", ts.invariants()[i].name),
        Target::AfterAssert(i) => format!("after {}", ts.after_asserts()[i].name),
        Target::Reach(i) => format!("can_reach {}", ts.reach_properties()[i].name),
    }
}

/// What [`check_model`] found: the cones past the state cap, and the checks TLC and the BFS
/// both found violated in the others.
struct ModelReport {
    capped: Vec<String>,
    /// The cones whose unreduced BFS passes the cap while the reduced one fits: compared
    /// reduced only, where TLC reads the quotient tables BFS computed.
    reduced_only: Vec<String>,
    violated: BTreeSet<String>,
}

/// What one cone's exploration found: the cone's report when its BFS finished under the cap,
/// the reduced cone's when the cone reduces and its BFS finished, and whether the run
/// `verify` uses (the reduced one when there is one) finished.
struct ConeRuns {
    plain: Option<ConeReport>,
    reduced: Option<ConeReport>,
    decided: bool,
}

/// A model at one bounds: its system, and its cones as `verify` groups the targets, each named
/// by the first target that reads it, with the seeds of the targets that read it.
struct ModelPlan {
    sample: String,
    bounds: Bounds,
    ts: TransitionSystem,
    cones: Vec<(Cone, String, Vec<Seed>)>,
}

fn plan_model(sample: &str, model: &SemanticModel, bounds: Bounds) -> ModelPlan {
    let ts = TransitionSystem::build(model, bounds.clone(), &Scope::ALL).ts;
    let mut cones: Vec<(Cone, String, Vec<Seed>)> = Vec::new();
    // `verify` runs no TLC cone for a `.lemma()` rule or a `can_reach`, so neither does the
    // comparison.
    let lemmas = rdra_ish_core::lifecycle::prove::lemma_invariants(model, &ts);
    let targets = ts.targets().into_iter().filter(|t| match t {
        Target::Invariant(i) => !lemmas.contains(i),
        Target::Reach(_) => false,
        _ => true,
    });
    for t in targets {
        let cone = ts.target_cone(t);
        match cones.iter_mut().find(|(c, _, _)| *c == cone) {
            Some((_, _, seeds)) => seeds.push(ts.target_seed(t)),
            None => cones.push((cone, target_label(&ts, t), vec![ts.target_seed(t)])),
        }
    }
    ModelPlan {
        sample: sample.to_string(),
        bounds,
        ts,
        cones,
    }
}

/// Every cone of the model at these bounds, explored and checked in parallel, unreduced and
/// reduced; returns the cones whose BFS as `verify` runs it passes `STATE_CAP`, which are
/// not compared.
fn check_model(
    tlc: &Path,
    scratch: &Path,
    sample: &str,
    model: &SemanticModel,
    bounds: Bounds,
) -> ModelReport {
    let plans = [plan_model(sample, model, bounds)];
    check_models(tlc, scratch, &plans)
        .pop()
        .expect("one report per plan")
}

/// [`check_model`] on each plan, with the cones of all of them in one queue: those at
/// `Bounds::VERIFY`, the larger, first. Returns one report per plan, in `plans` order.
fn check_models(tlc: &Path, scratch: &Path, plans: &[ModelPlan]) -> Vec<ModelReport> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let jobs: Vec<(usize, usize)> = [true, false]
        .into_iter()
        .flat_map(|verify| {
            plans
                .iter()
                .enumerate()
                .filter(move |(_, p)| (p.bounds == Bounds::VERIFY) == verify)
                .flat_map(|(at, p)| (0..p.cones.len()).map(move |i| (at, i)))
        })
        .collect();
    let next = AtomicUsize::new(0);
    let runs: Mutex<Vec<(usize, usize, ConeRuns)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(&(at, i)) = jobs.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let found = check_one_cone(tlc, scratch, &plans[at], i);
                    runs.lock().unwrap().push((at, i, found));
                }
            });
        }
    });
    let mut runs = runs.into_inner().unwrap();
    runs.sort_by_key(|(at, i, _)| (*at, *i));
    let mut by_plan: Vec<Vec<(usize, ConeRuns)>> = plans.iter().map(|_| Vec::new()).collect();
    for (at, i, found) in runs {
        by_plan[at].push((i, found));
    }
    plans
        .iter()
        .zip(by_plan)
        .map(|(plan, reports)| model_report(plan, reports))
        .collect()
}

/// Cone `i` of `plan`, explored and compared with TLC unreduced and, where it reduces,
/// reduced as `verify` explores it.
fn check_one_cone(tlc: &Path, scratch: &Path, plan: &ModelPlan, i: usize) -> ConeRuns {
    let ModelPlan {
        sample,
        bounds,
        ts,
        cones,
    } = plan;
    let (cone, name, seeds) = &cones[i];
    let caps = Caps::new(usize::MAX, STATE_CAP);
    let label = format!("{sample} at {bounds:?}, cone {i} ({name})");
    let dir = scratch.join(format!(
        "{}-{}-{i}",
        sample.replace('/', "_"),
        bounds.instances
    ));
    let t0 = std::time::Instant::now();
    let run = ts.restrict_to(&cone.vars, &cone.keep).run(caps);
    let t_bfs = t0.elapsed().as_secs_f64();
    let plain = run
        .complete
        .then(|| check_cone(tlc, &dir, &label, &run, &run.system));
    let t_plain = t0.elapsed().as_secs_f64();
    // The sub-lifecycle is explored by BFS alone, under `verify`'s cap.
    let system = ts.reduced_cone(cone, seeds.iter().cloned(), Caps::VERIFY);
    let t_reduce = t0.elapsed().as_secs_f64();
    let reduction = system
        .reduction()
        .map(|r| (r.classes(), r.inputs().len(), r.bits().len(), r.tla_cells()));
    let reduced_run = reduction.is_some().then(|| system.run(caps));
    let t_rbfs = t0.elapsed().as_secs_f64();
    let reduced = reduced_run.as_ref().filter(|r| r.complete).map(|r| {
        check_cone(
            tlc,
            &dir.join("reduced"),
            &format!("{label}, reduced"),
            r,
            &run.system,
        )
    });
    trace(|| {
        let sizes = |r: &ComponentRun| {
            format!(
                "{} states{}",
                r.states.len(),
                if r.complete { "" } else { " CAPPED" }
            )
        };
        format!(
            "{label}: plain {} (bfs {t_bfs:.1}s, tlc {:.1}s); reduction {reduction:?} (classes, inputs, bits, cells; {:.1}s); reduced run {} ({:.1}s); reduced tlc {:.1}s",
            sizes(&run),
            t_plain - t_bfs,
            t_reduce - t_plain,
            reduced_run.as_ref().map_or("none".to_string(), sizes),
            t_rbfs - t_reduce,
            t0.elapsed().as_secs_f64() - t_rbfs,
        )
    });
    // The reduced cone keeps the checks that read its target side and what it observes of
    // the sub-lifecycle; each of those has the cone's verdict.
    if let (Some(a), Some(b)) = (&plain, &reduced) {
        let kept: BTreeSet<String> = a.violated.intersection(&b.checks).cloned().collect();
        assert_eq!(
            b.violated, kept,
            "{label}: the reduced cone's verdicts differ from the cone's"
        );
    }
    let decided = reduced_run.as_ref().map_or(run.complete, |r| r.complete);
    ConeRuns {
        plain,
        reduced,
        decided,
    }
}

/// The plan's [`ModelReport`] from its cones' runs, in cone order, with its summary line.
fn model_report(plan: &ModelPlan, reports: Vec<(usize, ConeRuns)>) -> ModelReport {
    let ModelPlan {
        sample,
        bounds,
        cones,
        ..
    } = plan;
    let checked: Vec<&ConeReport> = reports
        .iter()
        .filter_map(|(_, r)| r.plain.as_ref())
        .collect();
    let states: usize = checked.iter().map(|r| r.states).sum();
    let reduced: Vec<&ConeReport> = reports
        .iter()
        .filter_map(|(_, r)| r.reduced.as_ref())
        .collect();
    let reduced_states: usize = reduced.iter().map(|r| r.states).sum();
    let violated: BTreeSet<String> = checked
        .iter()
        .flat_map(|r| r.violated.iter().cloned())
        .collect();
    let capped: Vec<String> = reports
        .iter()
        .filter(|(_, r)| !r.decided)
        .map(|(i, _)| cones[*i].1.clone())
        .collect();
    println!(
        "{sample} instances={}: {} cones, {states} states ({} reduced, {reduced_states} states), violated {violated:?}, past the cap {capped:?}",
        bounds.instances,
        checked.len(),
        reduced.len()
    );
    let reduced_only: Vec<String> = reports
        .iter()
        .filter(|(_, r)| r.plain.is_none() && r.reduced.is_some())
        .map(|(i, _)| cones[*i].1.clone())
        .collect();
    ModelReport {
        capped,
        reduced_only,
        violated,
    }
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

/// `sets(..).when(Cond)` (#98): each reason leaves its own deadline on the created extension,
/// a value is read on an updated row before the step, a guard that never holds drops its value,
/// and the timer's use case ends the period once `now` reaches the deadline. `Flag` after
/// `Remind` leaves an urgent claim reminded at 4, which `Inv_Claim_1` reports.
const SET_GUARDS: &str = r#"
variation Reason "延長事由" {
  disaster "被災地の調査"
  expert "鑑定等の照会"
  overseas "国外の調査"
}
entity Claim "請求" { id: Int @pk  period: Enum(running, extended, overdue) @default(running)  urgent: Bool @default(false)  reminder_day: Int @null }
entity Extension "延長" { id: Int @pk  reason: Reason  due_on: Date }
relate(Extension, Claim, 1:1)
usecase File "請求"
usecase Flag "急ぎ"
usecase Extend "延長"
usecase Remind "督促の予定"
usecase MarkOverdue "期限の経過"
creates(File, Claim)
sets(Flag, Claim.urgent == true)
creates(Extend, Extension)
updates(Extend, Claim)
transitions(Extend, Claim.period, running -> extended)
condition Disaster "被災地の調査で延ばす"
holds(Disaster, Extension.reason == disaster)
condition Expert "鑑定等の照会で延ばす"
holds(Expert, Extension.reason == expert)
condition Never "起きない"
holds(Never, Extension.reason == expert, Extension.due_on == 0)
sets(Extend, Extension.due_on == 2).when(Disaster)
sets(Extend, Extension.due_on == 3).when(Expert)
sets(Extend, Extension.due_on == 5).when(Never)
condition Urgent "急ぎの請求"
holds(Urgent, Claim.urgent == true)
condition NotUrgent "急ぎでない請求"
holds(NotUrgent, Claim.urgent == false)
sets(Remind, Claim.reminder_day == 1).when(Urgent)
sets(Remind, Claim.reminder_day == 4).when(NotUrgent)
timer Deadline "支払期限の到来"
triggers(Deadline, MarkOverdue)
updates(MarkOverdue, Claim)
condition Passed "延ばした期限が来た"
holds(Passed, Extension.due_on <= now)
transitions(MarkOverdue, Claim.period, extended -> overdue).when(Passed)
invariant(Extension).when(reason == expert).then(due_on == 3)
invariant(Claim).when(urgent == true).then(reminder_day != 4)
forbidden(Claim, Extension, Claim.period == overdue, Extension.due_on > now).along(Extension, Claim)
"#;

#[test]
fn bfs_and_tlc_agree_on_set_guards() {
    check_fixture("set_guards", SET_GUARDS);
}

/// The repro of #110: only the guards read the reason. Both engines run `Extend` for either
/// reason, so each `forbidden` on an extended claim's deadline is reached.
const SET_GUARDS_ONLY_READER: &str = r#"
variation Reason "Reason" {
  a "A"
  b "B"
}
entity Claim "Claim" {
  id: Int @pk
  reason: Reason
  due_on: Date
  status: Enum(open, extended) @default(open)
}
usecase Open "Open"
usecase PickReason "Pick reason"
usecase Extend "Extend"
creates(Open, Claim)
chooses(PickReason, Claim.reason)
condition IsA "A"
holds(IsA, Claim.reason == a)
condition IsB "B"
holds(IsB, Claim.reason == b)
transitions(Extend, Claim.status, open -> extended)
sets(Extend, Claim.due_on == 2).when(IsA)
sets(Extend, Claim.due_on == 3).when(IsB)
forbidden(Claim, status == extended, due_on == 2)
forbidden(Claim, status == extended, due_on == 3)
"#;

#[test]
fn bfs_and_tlc_read_the_columns_only_a_set_guard_reads() {
    if let Some(violated) = check_fixture("set_guards_only_reader", SET_GUARDS_ONLY_READER) {
        assert_eq!(
            violated,
            BTreeSet::from([
                "Forbidden_Claim_0".to_string(),
                "Forbidden_Claim_1".to_string()
            ])
        );
    }
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
            BTreeSet::from(["MultiForbidden_Claim_0".to_string()])
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
        assert!(!renews.contains("MultiForbidden_App_0"), "{renews:?}");
        assert!(
            registered.contains("MultiForbidden_App_0"),
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
        assert!(!guarded.contains("MultiForbidden_Member_0"), "{guarded:?}");
        assert!(
            unguarded.contains("MultiForbidden_Member_0"),
            "{unguarded:?}"
        );
    }
}

/// A `.none` and a `.has` on an `invariant` count along the rule's `.along` path (#90), as on
/// a `holds` (#81). Guarded, both rules hold at two instances, and the `.none` is where the
/// path and the one-hop reading part: no key joins Member and Loan, so one hop from the
/// anchor `Quantifier_Member_0` would count another member's open loan and fail. Unguarded,
/// a member withdraws with an open loan on their own chain, and both engines reach both
/// violations.
const QUANTIFIER_ALONG_PATH: &str = r#"
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
invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Card, Loan)
invariant(Loan).when(status == open).has(Member.status == active).along(Loan, Card, Member)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_quantifier_along_a_path() {
    let guarded = check_fixture(
        "quantifier_along_guarded",
        &QUANTIFIER_ALONG_PATH.replace("{GUARD}", "applies_to(NoOpenLoan, Withdraw)"),
    );
    let unguarded = check_fixture(
        "quantifier_along_unguarded",
        &QUANTIFIER_ALONG_PATH.replace("{GUARD}", ""),
    );
    if let (Some(guarded), Some(unguarded)) = (guarded, unguarded) {
        for rule in ["Quantifier_Member_0", "Quantifier_Loan_1"] {
            assert!(!guarded.contains(rule), "{rule}: {guarded:?}");
            assert!(unguarded.contains(rule), "{rule}: {unguarded:?}");
        }
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
        assert!(ship.contains("MultiForbidden_Order_0"), "{ship:?}");
        assert!(!bill.contains("MultiForbidden_Order_0"), "{bill:?}");
    }
}

/// `deletes` over keys with different `.on_delete` (#199): the lines' key states none, the
/// SQL default, so Cancel waits until no present line names the Order, and a line Cancel
/// deletes in the same step does not count; the notes' key says cascade, which the
/// lifecycle does not run, so a note outlives its Order with no guard and no rule. Both
/// engines reach the same states, and `DanglingKey_OrderLine_Order`, the only such rule,
/// holds.
const DELETE_ACTIONS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(draft, placed) @default(draft) }
entity OrderLine "明細" { id: Int @pk }
entity Note "メモ" { id: Int @pk }
relate(OrderLine, Order, N:1)
relate(Note, Order, N:1).on_delete(cascade)
usecase Open "起票"
usecase AddLine "明細追加"
usecase RemoveLine "明細削除"
usecase AddNote "メモ追加"
usecase Cancel "取消"
usecase Place "確定"
creates(Open, Order)
creates(AddLine, OrderLine)
deletes(RemoveLine, OrderLine)
creates(AddNote, Note)
deletes(Cancel, Order)
deletes(Cancel, OrderLine)
transitions(Place, Order.status, draft -> placed)
"#;

#[test]
fn bfs_and_tlc_agree_on_a_delete_its_children_restrict() {
    if let Some(violated) = check_fixture("delete_actions", DELETE_ACTIONS) {
        assert!(violated.is_empty(), "{violated:?}");
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
        assert!(billed.contains("MultiForbidden_Order_0"), "{billed:?}");
        assert!(!shipped.contains("MultiForbidden_Order_0"), "{shipped:?}");
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
        assert!(!along.contains("MultiForbidden_Claim_0"), "{along:?}");
        assert!(anywhere.contains("MultiForbidden_Claim_0"), "{anywhere:?}");
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
            let width: usize = ts.vars().iter().map(|d| ts.slots_of(&d.kind)).sum();
            assert_eq!(keys.len(), width, "{sample}: a slot has no key");
            // The declaration, a line per entity, up to the blank line that ends it.
            let declared: Vec<&str> = bundle
                .tla
                .split_once("\nVARIABLES")
                .and_then(|(_, rest)| rest.split_once("\n\n"))
                .map(|(block, _)| {
                    block
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let mut listed: Vec<&str> = names.vars().iter().map(String::as_str).collect();
            let mut declared_sorted = declared.clone();
            listed.sort_unstable();
            declared_sorted.sort_unstable();
            assert!(
                ts.vars().is_empty() || declared_sorted == listed,
                "{sample}: VARIABLES does not list the names"
            );
            let run = ts.clone().run(Caps::new(usize::MAX, 1));
            let canon = names.canon(&run.system, &run.states.at(0));
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
/// Two properties on a cone that reduces its sub-lifecycle (#74): a gate behind a key that
/// opens and closes forever, and a job that starts only at an open gate. `JobStarts` fails on
/// the reduced cone and, under `WF_vars(Next)`, on the unreduced one (the gate loops while
/// `Start` is enabled only now and then), and `JobWaits` fails on the reduced cone alone; each
/// counterexample is read on the `Full` run, whose lasso is a path of the unreduced cone.
const REDUCED_GATE: &str = r#"
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
entity Gate "門" { id: Int @pk  status: Enum(shut, half, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Gate, Key, N:1)
relate(Job, Gate, N:1)
usecase Unlock "解錠"
usecase Crack "少し開ける"
usecase Unlatch "掛け金を外す"
usecase Swing "開く"
usecase Close "閉める"
usecase Submit "投入"
usecase Start "開始"
transitions(Unlock, Key.status, locked -> unlocked)
transitions(Crack, Gate.status, shut -> half)
sets(Unlatch, Gate.latch == true)
transitions(Swing, Gate.status, half -> open)
transitions(Close, Gate.status, open -> shut)
condition Unlatched "掛け金が外れている"
holds(Unlatched, Gate.latch == true)
applies_to(Unlatched, Swing)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
property JobStarts "作業はいずれ始まる" eventually(Job.status == running).fair(Start)
property JobWaits "作業は始まらない" always(Job.status == waiting)
"#;

/// A split use case `Go` (steps `Go_1`, `Go_2`) beside a use case named `Go_1` that moves
/// the gate the job's cone reduces (#74): two actions of one name, which only the action's
/// identity tells apart. The job starts once the gate is open, so the rule is violated.
const NAME_COLLISION: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Go_1 "門を開ける"
usecase BuildGate "設置"
usecase Submit "投入"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Go_1, Gate.status, shut -> open)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// The collision where the job's first step binds a second entity, so the two actions of
/// one name differ in their params too.
const NAME_COLLISION_TWO_PARAMS: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
entity Crew "班" { id: Int @pk  status: Enum(idle, busy) @default(idle) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Go_1 "門を開ける"
usecase BuildGate "設置"
usecase Submit "投入"
usecase Hire "雇う"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
creates(Hire, Crew)
invokes(Go, JobApi)
updates(JobApi, Job)
updates(JobApi, Crew)
transitions(Go, Job.status, waiting -> running)
transitions(Go, Crew.status, idle -> busy)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Go_1, Gate.status, shut -> open)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
"#;

/// A use case named `Tick` beside the clock's action, which is then `Tick_2`: the deadline
/// passes by the clock alone, so the rule is violated, and the use case runs after it.
const TICK_USE_CASE: &str = r#"
entity Coupon "クーポン" { id: Int @pk  status: Enum(active, expired) @default(active)  expires_at: Date }
usecase Issue "発行"
usecase Tick "失効"
creates(Issue, Coupon)
sets(Issue, Coupon.expires_at == 2)
transitions(Tick, Coupon.status, active -> expired)
condition PastDeadline "期限を過ぎた"
holds(PastDeadline, Coupon.expires_at < now)
applies_to(PastDeadline, Tick)
forbidden(Coupon, status == active, expires_at < now)
"#;

/// A use case named `Go_2` beside the second step of the split use case `Go`, which is then
/// `Go_2_2`; the use case closes the gate, so a job can start only while it stays open.
const GO_2_USE_CASE: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Open "門を開ける"
usecase Go_2 "門を閉める"
usecase BuildGate "設置"
usecase Submit "投入"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Open, Gate.status, shut -> open)
transitions(Go_2, Gate.status, open -> shut)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// A violation whose path runs through the sub-lifecycle the job's cone reduces (#109): the
/// job starts at an open gate, and the gate opens only after `Unlatch` and the key's
/// `Unlock`, steps the reduced cone folds into the class move of `Swing`.
const GATE_BEHIND_A_KEY: &str = r#"
entity Card "札" { id: Int @pk  status: Enum(blank, issued) @default(blank) }
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Issue "札を出す"
transitions(Issue, Card.status, blank -> issued)
condition CardIssued "札が出ている"
holds(CardIssued, Card.status == issued)
applies_to(CardIssued, Unlock)
entity Gate "門" { id: Int @pk  status: Enum(shut, half, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Gate, Key, N:1)
relate(Job, Gate, N:1)
usecase Unlock "解錠"
usecase Crack "少し開ける"
usecase Unlatch "掛け金を外す"
usecase Swing "開く"
usecase Submit "投入"
usecase Start "開始"
transitions(Unlock, Key.status, locked -> unlocked)
transitions(Crack, Gate.status, shut -> half)
sets(Unlatch, Gate.latch == true)
transitions(Swing, Gate.status, half -> open)
condition Unlatched "掛け金が外れている"
holds(Unlatched, Gate.latch == true)
applies_to(Unlatched, Swing)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
forbidden(Job, status == running).label("作業は始まらない")
"#;

#[test]
fn bfs_and_tlc_agree_on_a_violation_through_the_sub_lifecycle() {
    if let Some(violated) = check_fixture("gate_behind_a_key", GATE_BEHIND_A_KEY) {
        assert_eq!(
            violated.into_iter().collect::<Vec<_>>(),
            ["Forbidden_Job_0"]
        );
    }
}

#[test]
fn bfs_and_tlc_agree_on_two_actions_of_one_name() {
    for (name, src, expected) in [
        ("name_collision", NAME_COLLISION, &["Forbidden_Job_0"][..]),
        ("name_collision_two_params", NAME_COLLISION_TWO_PARAMS, &[]),
        ("tick_use_case", TICK_USE_CASE, &["Forbidden_Coupon_0"]),
        ("go_2_use_case", GO_2_USE_CASE, &["Forbidden_Job_0"]),
    ] {
        if let Some(violated) = check_fixture(name, src) {
            let expected: BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(violated, expected, "{name}");
        }
    }
}

/// The job's cone reduces the gate, whose latch it does not read, and two use cases are named
/// like the reduced module's tables: `Succ_Swing` (Swing's class moves) and `Rep_Gate_status`
/// (the gate's status by class). The tables take `_2`; the module that defined each name
/// twice failed SANY.
const REDUCTION_TABLE_NAMES: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase BuildGate "設置"
usecase Swing "開く"
usecase Unlatch "掛け金を外す"
usecase Succ_Swing "投入"
usecase Rep_Gate_status "開始"
creates(BuildGate, Gate)
transitions(Swing, Gate.status, shut -> open)
sets(Unlatch, Gate.latch == true)
creates(Succ_Swing, Job)
transitions(Rep_Gate_status, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Rep_Gate_status)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// `Cart.Line_qty` and `Cart_Line.qty` both spell `Check_Cart_Line_qty_0`; the second check
/// is `Check_Cart_Line_qty_0_2` in both engines.
const CHECK_NAMES: &str = r#"
entity Cart "カート" { id: Int @pk  Line_qty: Int @default("1") @check(Line_qty >= 1) }
entity Cart_Line "明細" { id: Int @pk  qty: Int @default("1") @check(qty >= 1) }
usecase AddCart "カート追加"
usecase AddLine "明細追加"
usecase ClearQty "数量クリア"
creates(AddCart, Cart)
creates(AddLine, Cart_Line)
sets(ClearQty, Cart_Line.qty == 0)
"#;

/// Columns named like the run mark of the split use case `Go`, the existence var, and the
/// owner var: each pair of vars has one spelling until the later takes `_2`, and TLC's
/// states decode to the BFS's.
const VAR_NAMES: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting)  run_Go: Bool @default(false)  exists: Bool @default(false)  owner_Gate: Bool @default(false) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Open "門を開ける"
usecase BuildGate "設置"
usecase Submit "投入"
usecase Mark "印"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Open, Gate.status, shut -> open)
sets(Mark, Job.run_Go == true)
sets(Mark, Job.exists == true)
sets(Mark, Job.owner_Gate == true)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
forbidden(Job, status == running, run_Go == true, exists == true, owner_Gate == true)
"#;

/// A use case `Forbidden_Order_0` split into steps `Forbidden_Order_0_1` and
/// `Forbidden_Order_0_2` beside the check `Forbidden_Order_0`, which keeps its name.
const SPLIT_USE_CASE_NAMED_LIKE_A_CHECK: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文確定"
usecase Forbidden_Order_0 "決済確定"
usecase Cancel "キャンセル"
api PayOrderApi "Order payment API"
api CapturePaymentApi "Payment capture API"
buc B "b"
contains(B, Place)
contains(B, Forbidden_Order_0)
contains(B, Cancel)
creates(Place, Order)
creates(Place, Payment)
invokes(Forbidden_Order_0, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Forbidden_Order_0, PayOrderApi)
updates(PayOrderApi, Order)
updates(Cancel, Order)
transitions(Forbidden_Order_0, Order.status, pending -> paid)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Forbidden_Order_0, Payment.status, pending -> captured)
relate(Payment, Order, N:1)
forbidden(Order, status == cancelled)
"#;

/// The column `Forbidden.Job_0` and the use case `Forbidden_Job_0` spell the check
/// `Forbidden_Job_0`: the check keeps its name, and the var and the action take suffixes.
const RULE_NAMED_LIKE_A_VAR: &str = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
entity Forbidden "x" { id: Int @pk  Job_0: Enum(a, b) @default(a) }
usecase Submit "投入"
usecase Forbidden_Job_0 "開始"
usecase Flip "flip"
creates(Submit, Job)
creates(Submit, Forbidden)
transitions(Forbidden_Job_0, Job.status, waiting -> running)
transitions(Flip, Forbidden.Job_0, a -> b)
forbidden(Job, status == running)
"#;

#[test]
fn bfs_and_tlc_agree_on_generated_names_beside_model_ids() {
    for (name, src, expected) in [
        (
            "reduction_table_names",
            REDUCTION_TABLE_NAMES,
            &["Forbidden_Job_0"][..],
        ),
        ("check_names", CHECK_NAMES, &["Check_Cart_Line_qty_0_2"]),
        ("var_names", VAR_NAMES, &["Forbidden_Job_0"]),
        (
            "split_use_case_named_like_a_check",
            SPLIT_USE_CASE_NAMED_LIKE_A_CHECK,
            &["Forbidden_Order_0"],
        ),
        (
            "rule_named_like_a_var",
            RULE_NAMED_LIKE_A_VAR,
            &["Forbidden_Job_0"],
        ),
    ] {
        if let Some(violated) = check_fixture(name, src) {
            let expected: BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(violated, expected, "{name}");
        }
    }
}

/// A sub-lifecycle of 1294 classes (a random model, minimized): four entities whose steps
/// `E1`'s reads join in no way, so every one of them is an S var of `P0`'s cone. Each
/// `Succ_<A>` table of the reduced cone's TLA+ module has a cell per class; printed as a
/// chain of `@@`, TLC overflowed its stack evaluating it, and `verify` reported `tool_error`
/// on a cone 1.1 decided (`ok`, 5176 states). Each of the 1294 states is a class of its own,
/// which alone reduces nothing (#101); `E4`, which only enables `S_1`, is a sub-lifecycle of
/// the sub-lifecycle that its own reduction folds away, so the 1294 classes are of an S
/// reduced inside and the cone stays reduced.
const MANY_CLASSES: &str = r#"
entity E4 "e4" {
  status: Enum(s0, s1) @default(s0)
}
usecase New4 "New4"
creates(New4, E4)
usecase T4_0 "T4_0"
transitions(T4_0, E4.status, s0 -> s1)
condition C9 "C9"
holds(C9, E4.status == s1)
applies_to(C9, S_1)
entity E0 "e0" {
  status: Enum(s0, s1, s2) @default(s0)
}
entity E1 "e1" {
  status: Enum(s0, s1) @default(s0)
}
entity E2 "e2" {
  status: Enum(s0, s1) @default(s0)
}
entity E3 "e3" {
  status: Enum(s0, s1, s2) @default(s0)
}
usecase New3 "New3"
creates(New3, E3)
usecase T0_0 "T0_0"
usecase T1_0 "T1_0"
usecase T1_1 "T1_1"
usecase T3_0 "T3_0"
transitions(T0_0, E0.status, s1 -> s2)
transitions(T0_0, E0.status, s0 -> s2)
transitions(T1_0, E1.status, s0 -> s1)
condition C0 "C0"
holds(C0, E0.status in (s1, s2)).none(E1.status == s0)
applies_to(C0, T1_0)
transitions(T1_1, E1.status, s0 -> s1)
condition C1 "C1"
holds(C1, E2.status == s0).none(E3.status == s0)
applies_to(C1, T1_1)
transitions(T3_0, E3.status, s0 -> s1)
condition C6 "C6"
holds(C6, E2.status == s1)
transitions(T3_0, E3.status, s1 -> s2).when(C6)
usecase S "S"
transitions(S, E0.status, s2 -> s1)
transitions(S, E2.status, s1 -> s0)
usecase S_1 "S_1"
transitions(S_1, E2.status, s0 -> s1)
property P0
  leads_to(E1.status == s1, E1.status == s1)
"#;

/// The random model (seed 2563) whose reduced cones overflowed TLC's stack the same way while
/// its property and three of its checks have counterexamples: `P0` on a cone of 11480 states,
/// and the two forbidden states and the `after` assertion of `T1_0` (the `after` assertion
/// of `T0_1` holds). Its cones reduced to 1640 classes of 1640 states, so since #101 they
/// stay unreduced and keep these verdicts.
const MANY_CLASSES_COUNTEREXAMPLE: &str = r#"
entity E0 "e0" {
  id: Int @pk
  status: Enum(s0, s1, s2, s3) @default(s0)
  flag: Bool @default(false)
}

entity E1 "e1" {
  id: Int @pk
  status: Enum(s0, s1) @default(s0)
  flag: Bool @default(false)
}

entity E2 "e2" {
  id: Int @pk
  status: Enum(s0, s1, s2) @default(s0)
}

entity E3 "e3" {
  id: Int @pk
  status: Enum(s0, s1) @default(s0)
}

relate(E1, E0, N:1).optional()
relate(E2, E1, N:1)
relate(E3, E1, N:1)

usecase New0 "New0"
creates(New0, E0)
usecase New1 "New1"
creates(New1, E1)
usecase New2 "New2"
creates(New2, E2)
usecase New3 "New3"
creates(New3, E3)
usecase T0_0 "T0_0"
updates(T0_0, E0)
usecase T0_1 "T0_1"
updates(T0_1, E0)
usecase T1_0 "T1_0"
updates(T1_0, E1)
usecase T1_1 "T1_1"
updates(T1_1, E1)
usecase T2_0 "T2_0"
updates(T2_0, E2)
usecase T2_1 "T2_1"
updates(T2_1, E2)
usecase T2_2 "T2_2"
updates(T2_2, E2)
usecase T3_0 "T3_0"
updates(T3_0, E3)

transitions(T0_0, E0.status, s0 -> s1)
sets(T0_0, E0.flag == true)
condition C0 "C0"
holds(C0, E1.status == s1).none(E2.status == s2)
transitions(T0_1, E0.status, s0 -> s1).when(C0)
condition C1 "C1"
holds(C1, E1.status == s0).has(E3.status == s0)
transitions(T0_1, E0.status, s1 -> s2).when(C1)
condition C2 "C2"
holds(C2, E1.status == s1).none(E3)
transitions(T0_1, E0.status, s2 -> s3).when(C2)
sets(T0_1, E0.flag == true)
condition C3 "C3"
holds(C3, E1.status == s0).none(E3.status == s1)
applies_to(C3, T0_1)
condition C4 "C4"
holds(C4, E3.status == s0)
transitions(T1_0, E1.status, s0 -> s1).when(C4)
transitions(T1_0, E1.status, s1 -> s0)
sets(T1_0, E1.flag == true)
condition C5 "C5"
holds(C5, E3.status == s1)
applies_to(C5, T1_0)
condition C6 "C6"
holds(C6, E3.status == s1)
transitions(T1_1, E1.status, s0 -> s1).when(C6)
condition C7 "C7"
holds(C7, E2.status in (s2, s1))
applies_to(C7, T1_1)
transitions(T2_0, E2.status, s0 -> s1)
transitions(T2_0, E2.status, s2 -> s1)
condition C8 "C8"
holds(C8, E2.status in (s1, s2))
transitions(T2_1, E2.status, s1 -> s2).when(C8)
transitions(T2_2, E2.status, s1 -> s0)
transitions(T3_0, E3.status, s0 -> s1)
condition C9 "C9"
holds(C9, E3.status == s1)
transitions(T3_0, E3.status, s0 -> s1).when(C9)

forbidden(E1, E3, E1.status == s1, E3.status == s1).along(E1, E3)
forbidden(E2, status == s1)
after(T1_0).assert(E1.status == s1)
after(T0_1).assert(E0.status == s2)
property P0
  always(E0.status != s0)
"#;

/// The safety cones of the seed-2563 model decide, through TLC, what the unreduced cones
/// decide. Its sub-lifecycles quotient to themselves, so since #101 no cone of it reduces.
#[test]
fn bfs_and_tlc_agree_on_a_sub_lifecycle_of_many_classes() {
    if let Some(violated) =
        check_fixture("many_classes_counterexample", MANY_CLASSES_COUNTEREXAMPLE)
    {
        let expected: BTreeSet<String> = [
            "Forbidden_E2_0",
            "MultiForbidden_E1_0",
            "AfterAssert_T1_0_0",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(violated, expected);
    }
}

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
    ("samples/formal-verification/temporal_along.rdra", "", &[]),
    ("samples/formal-verification/order_stability.rdra", "", &[]),
    ("explored_along", EXPLORED_ALONG, &[]),
    ("explored_relink", EXPLORED_RELINK, &[]),
    ("explored_delete", EXPLORED_DELETE, &[]),
    (
        "explored_frees_slots",
        EXPLORED_FREES_SLOTS,
        &[("NeverD", Why::FreesSlots)],
    ),
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
    ("reduced_gate", REDUCED_GATE, &[]),
    ("many_classes", MANY_CLASSES, &[]),
    (
        "many_classes_counterexample",
        MANY_CLASSES_COUNTEREXAMPLE,
        &[],
    ),
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

/// `precedes` over two entities, along the key and over every pair: Capture captures a payment
/// and ships the one order it belongs to (#166). The latch is per pair of slots.
const EXPLORED_ALONG: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, shipped) @default(open) }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, 1:1)
usecase Capture "確定"
usecase Reopen "再開"
transitions(Capture, Payment.status, pending -> captured)
transitions(Capture, Order.status, open -> shipped)
transitions(Reopen, Order.status, shipped -> open)
property ShippedWithItsCapture
  precedes(Payment.status == captured, Order.status == shipped).along(Payment, Order)
property ShippedWithAnyCapture
  precedes(Payment.status == captured, Order.status == shipped).global()
property CaptureStays stable(Payment.status == captured).along(Payment, Order)
property ShippedStays stable(Order.status == shipped)
"#;

/// The link of `stable(..).along(..)` is read before the step only: Move relinks a captured
/// payment and voids it in one step, a violation in both engines.
const EXPLORED_RELINK: &str = r#"
entity Order "注文" { id: Int @pk }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured, voided) @default(pending) }
relate(Payment, Order, N:1)
usecase Capture "確定"
usecase Move "付け替え"
transitions(Capture, Payment.status, pending -> captured)
relinks(Move, Payment, Order)
transitions(Move, Payment.status, captured -> voided)
property CapturedStays stable(Payment.status == captured).along(Payment, Order)
"#;

/// A deleted row resets to its base values; a step that deletes it breaks no `stable`, since
/// the row is not present after it.
const EXPLORED_DELETE: &str = r#"
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Open "起票"
usecase Capture "確定"
usecase Drop "削除"
creates(Open, Payment)
transitions(Capture, Payment.status, pending -> captured)
deletes(Drop, Payment)
property CapturedStays stable(Payment.status == captured)
property PendingStays stable(Payment.status == pending)
property AmountFree unchanged(Payment.status).when(Payment.status == captured)
"#;

/// Each Bump creates a receipt `NeverD` does not read, so its cone counts no receipt and bumps
/// an order to `d`; the whole model runs out of receipts first. The cone leaves the property
/// undecided for TLC, and BFS decides it on the whole model, as TLC does.
const EXPLORED_FREES_SLOTS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(a, b, c, d) @default(a) }
entity Receipt "領収書" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "注文"
usecase Bump "進行"
creates(Place, Order)
transitions(Bump, Order.status, a -> b)
transitions(Bump, Order.status, b -> c)
transitions(Bump, Order.status, c -> d)
creates(Bump, Receipt)
property NeverD stable(Order.status != d)
"#;

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

/// `stable`, `unchanged`, and `precedes` (#166): what BFS decides on the property's cone is
/// what TLC decides on the whole model, and a violation's trace is as long as TLC's, which
/// replays as the same violation.
fn explored_agrees(
    ts: &TransitionSystem,
    i: usize,
    label: &str,
    (whole, outcome, run): (&tla::TlaBundle, Outcome, &TlcRun),
    log: &str,
) {
    match (ts.decide_property(i, Caps::VERIFY, Caps::VERIFY), outcome) {
        (PropertyVerdict::Holds, Outcome::Ok) => {}
        (PropertyVerdict::Violated(native), Outcome::Counterexample) => {
            let StateDiag::PropertyViolated { trace, witness, .. } = *native else {
                panic!("{label}: BFS reports no property violation: {native:?}");
            };
            let v = run
                .violations
                .first()
                .expect("a counterexample has a trace");
            let tlc = explain_property(ts, &whole.names, i, v)
                .unwrap_or_else(|why| panic!("{label}: TLC's trace does not replay: {why}\n{log}"));
            let StateDiag::PropertyViolated {
                trace: tlc_trace,
                witness: tlc_witness,
                ..
            } = tlc
            else {
                panic!("{label}: TLC's trace explains no property violation");
            };
            assert_eq!(
                (trace.steps.len(), witness),
                (tlc_trace.steps.len(), tlc_witness),
                "{label}: BFS and TLC find counterexamples of different lengths\n{log}"
            );
        }
        (native, outcome) => panic!(
            "{label}: BFS decides {native:?}, TLC on the whole model {}\n{log}",
            outcome.as_str()
        ),
    }
}

/// One property run of `bundle` in `dir`; the module is printed as `Cone`.
fn run_property(
    route: Route,
    tlc: &Path,
    dir: &Path,
    bundle: &tla::TlaBundle,
) -> (Outcome, TlcRun, String) {
    let (ok, log) = run_tlc_on(route, batch_pool(), tlc, dir, &[], &bundle.tla, &bundle.cfg);
    let run = TlcRun::parse(&log);
    (run.outcome(ok), run, log)
}

/// The native checker beside each TLC run of a property: the verdict on the same system and
/// fairness must be TLC's, and a native counterexample must certify itself. `runs` gets one
/// line per run: the verdict, and for a counterexample the steps of the native lasso and of
/// TLC's.
fn native_agrees(
    label: &str,
    run_name: &str,
    graph: &ComponentRun,
    i: usize,
    fairness: Fairness,
    (outcome, tlc_run, names): (Outcome, &TlcRun, &TlaNames),
    runs: &mut Vec<String>,
) {
    let ts = &graph.system;
    let verdict = match ts.properties()[i].formula.kind().is_liveness() {
        true => graph.decide_liveness(i, fairness).unwrap_or_else(|fault| {
            panic!("{label} ({run_name}): a native lasso fails its own check: {fault:?}")
        }),
        false => graph.decide_property(i).expect("a safety form"),
    };
    let native = match &verdict {
        PropertyVerdict::Holds => Outcome::Ok,
        PropertyVerdict::Violated(_) => Outcome::Counterexample,
        PropertyVerdict::Undecided => Outcome::ToolError,
    };
    assert_eq!(
        native,
        outcome,
        "{label} ({run_name}): native {} on {} states, TLC {} on {}",
        native.as_str(),
        graph.states.len(),
        outcome.as_str(),
        tlc_run.distinct_states.unwrap_or(0)
    );
    let PropertyVerdict::Violated(diag) = verdict else {
        runs.push(format!("{label} ({run_name}): ok"));
        return;
    };
    let steps = |d: &StateDiag| match d {
        StateDiag::PropertyViolated { trace, lasso, .. } => {
            let tail = match lasso {
                Some(Loop::Back { to }) => format!(" back to {to}"),
                Some(Loop::Stutter) => " stutter".to_string(),
                None => String::new(),
            };
            format!("{}{tail}", trace.steps.len())
        }
        other => panic!("{label}: not a property violation: {other:?}"),
    };
    let tlc = tlc_run
        .violations
        .first()
        .map(|v| match explain_property(ts, names, i, v) {
            Ok(d) => steps(&d),
            Err(why) => format!("no replay ({why})"),
        })
        .unwrap_or_else(|| "no trace".to_string());
    runs.push(format!(
        "{label} ({run_name}): counterexample, native {}, TLC {tlc}",
        steps(&diag)
    ));
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
    let mut runs = Vec::new();
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
        let mut full_runs = 0;
        for i in 0..ts.properties().len() {
            let property = &ts.properties()[i].name;
            let label = format!("{name}: {property}");
            let cone = PropertyCone::new(&ts, i);
            assert!(
                *name != "reduced_gate" || cone.is_reduced(),
                "{label}: the gate's cone is not reduced"
            );
            let classes = cone.reduced.reduction().map_or(0, |r| r.classes());
            assert!(
                *name != "many_classes" || classes >= 1000,
                "{label}: the sub-lifecycle has {classes} classes, which exercises no depth"
            );
            let dir = scratch.join(format!("{}-{i}", name.replace('/', "_")));
            let plain = cone.bundle(ConeRun::Plain, "Cone");
            assert!(
                !plain.tla.contains("WF_vars(Next)"),
                "{label}: the plain cone run assumes WF_vars(Next)"
            );
            let (mut first, mut run, mut log) =
                run_property(Route::Batch, &tlc, &dir.join("plain"), &plain);
            let mut names = plain.names.clone();
            native_agrees(
                &label,
                "plain",
                &cone.reduced.clone().run(Caps::VERIFY),
                cone.reduced_index,
                Fairness::Plain,
                (first, &run, &names),
                &mut runs,
            );
            // The `Full` and `WfNext` runs read one graph, as the native checker does.
            let unreduced = std::cell::OnceCell::new();
            let unreduced = || unreduced.get_or_init(|| cone.system.clone().run(Caps::VERIFY));
            // As `verify` does: a counterexample on the reduced cone is read on the unreduced
            // one, whose run gives the outcome from here on.
            if first == Outcome::Counterexample && cone.is_reduced() {
                let full = cone.bundle(ConeRun::Full, "Cone");
                assert!(
                    !full.tla.contains("S_class"),
                    "{label}: the Full run is reduced"
                );
                let (again, r, l) = run_property(Route::Batch, &tlc, &dir.join("full"), &full);
                native_agrees(
                    &label,
                    "full",
                    unreduced(),
                    cone.index,
                    Fairness::Plain,
                    (again, &r, &full.names),
                    &mut runs,
                );
                assert_eq!(
                    again,
                    Outcome::Counterexample,
                    "{label}: the reduced cone finds a counterexample the cone does not\n{l}"
                );
                (first, run, log, names) = (again, r, l, full.names);
                full_runs += 1;
            }
            let verdict = match cone.after_plain(first) {
                AfterPlain::Decided(verdict) => verdict,
                AfterPlain::NeedsWfNext => {
                    let wf = cone.bundle(ConeRun::WfNext, "Cone");
                    assert!(
                        wf.tla.contains("WF_vars(Next)"),
                        "{label}: the WF cone run lacks WF_vars(Next)"
                    );
                    let (second, r, l) = run_property(Route::Batch, &tlc, &dir.join("wf"), &wf);
                    native_agrees(
                        &label,
                        "wf",
                        unreduced(),
                        cone.index,
                        Fairness::WfNext,
                        (second, &r, &wf.names),
                        &mut runs,
                    );
                    (run, log, names) = (r, l, wf.names);
                    cone.after_wf(first, second)
                }
            };
            let whole = tla::print(&ts, PrintMode::Property(i), "Cone");
            let (whole_outcome, whole_run, whole_log) =
                // The whole model, the longest run of a property: a process of its own.
                run_property(Route::Process, &tlc, &dir.join("whole"), &whole);
            assert_ne!(
                whole_outcome,
                Outcome::ToolError,
                "{label}: whole model\n{whole_log}"
            );
            native_agrees(
                &label,
                "whole",
                &ts.clone().run(Caps::VERIFY),
                i,
                Fairness::WfNext,
                (whole_outcome, &whole_run, &whole.names),
                &mut runs,
            );
            if ts.properties()[i].formula.kind().is_explored() {
                explored_agrees(
                    &ts,
                    i,
                    &label,
                    (&whole, whole_outcome, &whole_run),
                    &whole_log,
                );
            }
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
                        explain_property(&cone.system, &names, cone.index, v).unwrap_or_else(
                            |why| panic!("{label}: the cone's lasso does not replay: {why}\n{log}"),
                        );
                    }
                }
                Verdict::Undecided(why) => undecided.push((property.as_str(), why)),
                Verdict::ToolError => panic!("{label}: {verdict:?}\n{log}"),
            }
            println!(
                "{label}: cone {verdict:?} ({} states), whole {}",
                run.distinct_states.unwrap_or(0),
                whole_outcome.as_str()
            );
        }
        let expected: Vec<(&str, Why)> = expected.to_vec();
        assert_eq!(undecided, expected, "{name}: the undecided properties");
        assert!(
            *name != "reduced_gate" || full_runs == 2,
            "{name}: {full_runs} counterexamples on the reduced cone, read on the Full run"
        );
    }
    println!("native and TLC agree on {} runs:", runs.len());
    for line in &runs {
        println!("  {line}");
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A `Date` column set to a number is a point on the `now` clock in both engines (#89): the
/// deadline `Issue` sets is passed by `Tick` alone, so the rule that an active coupon is in
/// date is violated, and `Expire`, guarded by the deadline, runs only after it.
const DATE_SETS: &str = r#"
entity Coupon "クーポン" { id: Int @pk  status: Enum(active, expired) @default(active)  expires_at: Date }
usecase Issue "発行"
usecase Expire "失効"
creates(Issue, Coupon)
sets(Issue, Coupon.expires_at == 2)
transitions(Expire, Coupon.status, active -> expired)
condition PastDeadline "期限を過ぎた"
holds(PastDeadline, Coupon.expires_at < now)
applies_to(PastDeadline, Expire)
forbidden(Coupon, status == active, expires_at < now)
forbidden(Coupon, status == expired, expires_at >= now)
"#;

/// `links(UC, Child, Parent)` in both engines (#89): the case opens unlinked, `Assign` binds a
/// present handler, marks it busy and links the case to it, so the assigned case's handler is
/// busy; `Reassign` never runs, since an assigned case is linked. On `{CARD}` `1:1` two cases
/// linked to one handler break `Unique_Case_Handler` at two instances.
const LINKS: &str = r#"
entity Handler "担当" { id: Int @pk  busy: Bool @default(false) }
entity Case "案件" { id: Int @pk  status: Enum(new, assigned) @default(new) }
relate(Case, Handler, {CARD}).optional()
usecase OpenCase "受付"
usecase Assign "担当を割り当てる"
usecase Reassign "担当を替える"
usecase Free "手空き"
creates(OpenCase, Case)
updates(Assign, Handler)
transitions(Assign, Case.status, new -> assigned)
sets(Assign, Handler.busy == true)
links(Assign, Case, Handler)
links(Reassign, Case, Handler)
condition Assigned "割当済"
holds(Assigned, Case.status == assigned)
applies_to(Assigned, Reassign)
sets(Free, Handler.busy == false)
invariant(Case).when(status == assigned).has(Handler)
"#;

#[test]
fn bfs_and_tlc_agree_on_links() {
    let many = check_fixture("links_n_to_one", &LINKS.replace("{CARD}", "N:1"));
    let one = check_fixture("links_one_to_one", &LINKS.replace("{CARD}", "1:1"));
    if let (Some(many), Some(one)) = (many, one) {
        assert!(many.is_empty(), "{many:?}");
        assert_eq!(
            one.into_iter().collect::<Vec<_>>(),
            vec!["Unique_Case_Handler".to_string()]
        );
    }
}

/// `relinks` and `unlinks` in both engines (#92): `Reassign` moves a linked case to the
/// handler it marks busy, `Release` frees the case's own handler and unlinks it. With
/// `{REASSIGN_GUARD}` keeping Reassign to a free handler, each handler holds one case and the
/// rules hold. Without it Reassign stacks a second case on a busy handler, and Release then
/// frees that handler under the other case: `Quantifier_Case_0` fails at two instances.
const RELINKS: &str = r#"
entity Handler "担当" { id: Int @pk  busy: Bool @default(false) }
entity Case "案件" { id: Int @pk  status: Enum(new, assigned) @default(new) }
relate(Case, Handler, N:1).optional()
usecase OpenCase "受付"
usecase Assign "担当を割り当てる"
usecase Reassign "担当を替える"
usecase Release "担当を外す"
creates(OpenCase, Case)
updates(Assign, Handler)
transitions(Assign, Case.status, new -> assigned)
sets(Assign, Handler.busy == true)
links(Assign, Case, Handler)
updates(Reassign, Handler)
sets(Reassign, Handler.busy == true)
relinks(Reassign, Case, Handler)
updates(Release, Handler)
sets(Release, Handler.busy == false)
transitions(Release, Case.status, assigned -> new)
unlinks(Release, Case, Handler)
condition Free "空き"
holds(Free, Handler.busy == false)
applies_to(Free, Assign)
{REASSIGN_GUARD}
invariant(Case).when(status == assigned).has(Handler.busy == true)
invariant(Case).when(status == new).none(Handler)
"#;

#[test]
fn bfs_and_tlc_agree_on_relinks_and_unlinks() {
    let guarded = check_fixture(
        "relinks_guarded",
        &RELINKS.replace("{REASSIGN_GUARD}", "applies_to(Free, Reassign)"),
    );
    let stacked = check_fixture("relinks_stacked", &RELINKS.replace("{REASSIGN_GUARD}", ""));
    if let (Some(guarded), Some(stacked)) = (guarded, stacked) {
        assert!(guarded.is_empty(), "{guarded:?}");
        assert_eq!(
            stacked.into_iter().collect::<Vec<_>>(),
            vec!["Quantifier_Case_0".to_string()]
        );
    }
}

#[test]
fn bfs_and_tlc_agree_on_a_date_sets() {
    if let Some(violated) = check_fixture("date_sets", DATE_SETS) {
        assert_eq!(
            violated.into_iter().collect::<Vec<_>>(),
            vec!["Forbidden_Coupon_0".to_string()]
        );
    }
}

/// The invariants a cfg lists, in order.
fn cfg_invariants(cfg: &str) -> Vec<String> {
    cfg.lines()
        .filter_map(|l| l.strip_prefix("INVARIANT "))
        .filter(|n| *n != "TypeOK")
        .map(str::to_string)
        .collect()
}

/// Above this many states a witnesses cfg runs without `-continue`: its invariants fail in
/// most reached states, and `-continue` prints a trace for each (1.5 GB for a 93,425-state
/// clinic-ops cone).
const CONTINUE_STATES: u64 = 1_000;

/// The invariants TLC finds violated on `module` with `cfg`, as a user reruns it by hand:
/// each rerun drops the reported ones, until a run reports no new one or no invariant but
/// `TypeOK` is left. A `continued` run explores every state and reports one invariant per
/// state; another halts at the first (`run_tlc` runs one worker, so one is reported), and
/// `TypeOK` is checked on every state only by the continued deadlocks run before it. Also
/// returns the first run's distinct states when it finished.
fn violated_by_hand(
    tlc: &Path,
    dir: &Path,
    module: &str,
    cfg: &str,
    label: &str,
    continued: bool,
) -> (BTreeSet<String>, Option<u64>) {
    // `run_tlc` runs the module as `Cone.tla`.
    let tla = std::fs::read_to_string(dir.join(format!("{module}.tla")))
        .expect("module")
        .replacen(&format!("MODULE {module} "), "MODULE Cone ", 1);
    let args: &[&str] = if continued { &["-continue"] } else { &[] };
    let mut found = BTreeSet::new();
    let mut states = None;
    for rerun in 0.. {
        let keep: Vec<&str> = cfg
            .lines()
            .filter(|l| {
                l.strip_prefix("INVARIANT ")
                    .is_none_or(|n| !found.contains(n))
            })
            .collect();
        let left = keep
            .iter()
            .filter_map(|l| l.strip_prefix("INVARIANT "))
            .any(|n| n != "TypeOK");
        if rerun > 0 && !left {
            break;
        }
        let (_, log) = run_tlc(
            tlc,
            &dir.join(format!("{module}-{label}-{rerun}")),
            args,
            &tla,
            &(keep.join("\n") + "\n"),
        );
        // A run that halts at an initial state says `is violated by the initial state:`,
        // which `TlcRun` does not name.
        let new: BTreeSet<String> = log
            .lines()
            .filter_map(|l| {
                l.strip_prefix("Error: Invariant ")?
                    .split_once(" is violated")
            })
            .map(|(name, _)| name.to_string())
            .collect();
        assert!(
            !new.contains("TypeOK"),
            "{module} ({label}): TypeOK violated\n{log}"
        );
        let finished = log.contains(" 0 states left on queue");
        assert!(
            finished || (!continued && !new.is_empty()),
            "{module} ({label}): TLC did not finish\n{log}"
        );
        if rerun == 0 && finished {
            states = TlcRun::parse(&log).distinct_states;
        }
        if new.is_subset(&found) {
            break;
        }
        found.extend(new);
    }
    (found, states)
}

/// A sample's `StateModel_` specs as `export --kind tla` writes them into `dir`, the ones TLC
/// runs, and the deadlocks and missing witnesses `verify`'s BFS finds at `Bounds::VERIFY`. A
/// cone in `VERIFY_FALLBACK` under `label` is too large to run here, and neither engine
/// explores it.
struct StateModels {
    label: String,
    dir: PathBuf,
    run: Vec<String>,
    bfs: BTreeSet<String>,
}

/// `input`'s [`StateModels`], or `None` when a build error stops `export` and `verify` before
/// either engine runs.
fn export_state_models(dir: &Path, label: &str, input: &str) -> Option<StateModels> {
    let model = load(input);
    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL);
    if rdra_ish_core::lifecycle::has_errors(&built.diags) {
        return None;
    }
    let ts = built.ts;
    let fallback: Vec<Cone> = ts
        .entity_ids()
        .filter(|e| {
            let entity = format!("entity {}", ts.entity(*e).id);
            VERIFY_FALLBACK.contains(&(label, entity.as_str()))
        })
        .map(|e| ts.target_cone(Target::Entity(e)))
        .collect();
    let entities: Vec<Target> = ts
        .entity_ids()
        .map(Target::Entity)
        .filter(|t| !fallback.contains(&ts.target_cone(*t)))
        .collect();
    let t0 = std::time::Instant::now();
    let bfs: BTreeSet<String> = ts
        .explore_targets_reduced(&entities, Caps::VERIFY)
        .verdicts()
        .iter()
        .map(|d| d.diag.name(&[]))
        .filter(|n| n.starts_with("Witness_") || n.starts_with("Deadlock_"))
        .collect();
    drop(ts);
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["export", "--kind", "tla", "-o"])
        .arg(dir)
        .arg(repo().join(input))
        .output()
        .expect("run export");
    assert!(out.status.success(), "{label}: {out:?}");
    trace(|| format!("{label}: state models {:.1}s", t0.elapsed().as_secs_f64()));
    let mut modules: Vec<String> = std::fs::read_dir(dir)
        .expect("export dir")
        .filter_map(|f| {
            let name = f.ok()?.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".tla")?;
            stem.starts_with("StateModel_").then(|| stem.to_string())
        })
        .collect();
    modules.sort();
    let mut listed = BTreeSet::new();
    let mut run = Vec::new();
    for m in modules {
        listed.extend(cfg_invariants(&read_cfg(dir, label, &m)));
        listed.extend(cfg_invariants(&read_cfg(
            dir,
            label,
            &format!("{m}_witnesses"),
        )));
        let entity = format!("entity {}", &m["StateModel_".len()..]);
        if !VERIFY_FALLBACK.contains(&(label, entity.as_str())) {
            run.push(m);
        }
    }
    let missing: BTreeSet<&String> = bfs.difference(&listed).collect();
    assert!(missing.is_empty(), "{label}: no spec checks {missing:?}");
    Some(StateModels {
        label: label.to_string(),
        dir: dir.to_path_buf(),
        run,
        bfs,
    })
}

/// A `<module>_witnesses` stem takes a `_<k>` suffix only on a collision, which no input here
/// has.
fn read_cfg(dir: &Path, label: &str, name: &str) -> String {
    std::fs::read_to_string(dir.join(format!("{name}.cfg")))
        .unwrap_or_else(|e| panic!("{label}: {name}.cfg: {e}"))
}

/// The deadlocks and missing witnesses TLC alone finds on each sample's specs, asserted equal
/// to those the BFS finds: a `Deadlock_` TLC reports violated with the spec's cfg, and a
/// `Witness_` it does not report with its `_witnesses` cfg. The modules of all samples share
/// one pool of TLC runs.
fn tlc_alone_and_bfs(tlc: &Path, samples: &[StateModels]) -> Vec<BTreeSet<String>> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let jobs: Vec<(usize, &str)> = samples
        .iter()
        .enumerate()
        .flat_map(|(i, s)| s.run.iter().map(move |m| (i, m.as_str())))
        .collect();
    let next = AtomicUsize::new(0);
    let checked = Mutex::new(vec![BTreeSet::new(); samples.len()]);
    let found = Mutex::new(vec![BTreeSet::new(); samples.len()]);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(&(i, m)) = jobs.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let StateModels { label, dir, .. } = &samples[i];
                    let deadlock_cfg = read_cfg(dir, label, m);
                    let t0 = std::time::Instant::now();
                    let (deadlocks, states) =
                        violated_by_hand(tlc, dir, m, &deadlock_cfg, "deadlocks", true);
                    let witnesses = read_cfg(dir, label, &format!("{m}_witnesses"));
                    let continued = states.is_some_and(|n| n <= CONTINUE_STATES);
                    let (reached, _) =
                        violated_by_hand(tlc, dir, m, &witnesses, "witnesses", continued);
                    trace(|| {
                        format!(
                            "{label} {m}: tlc states {states:?}, {:.1}s",
                            t0.elapsed().as_secs_f64()
                        )
                    });
                    let unreached = cfg_invariants(&witnesses)
                        .into_iter()
                        .filter(|w| !reached.contains(w));
                    found.lock().unwrap()[i].extend(deadlocks.into_iter().chain(unreached));
                    checked.lock().unwrap()[i].extend(
                        cfg_invariants(&deadlock_cfg)
                            .into_iter()
                            .chain(cfg_invariants(&witnesses)),
                    );
                }
            });
        }
    });
    let checked = checked.into_inner().unwrap();
    let found: Vec<BTreeSet<String>> = found.into_inner().unwrap();
    for (i, s) in samples.iter().enumerate() {
        let expected: BTreeSet<String> = s.bfs.intersection(&checked[i]).cloned().collect();
        assert_eq!(found[i], expected, "{}: TLC alone and BFS", s.label);
    }
    found
}

/// `tlc`, or `None` with the reason printed; `RDRA_REQUIRE_TLC=1` turns the skip into a
/// failure.
fn require_tlc() -> Option<PathBuf> {
    let tlc = find_tlc();
    if tlc.is_none() {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped: {reason}");
    }
    tlc
}

/// Named to sort among the first tests, as `a_bfs_and_tlc_reach_the_same_states_and_verdicts`
/// is: the second longest in the file, it starts at once rather than after the fixtures (#305).
#[test]
fn a_tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds() {
    let Some(tlc) = require_tlc() else { return };
    let scratch = std::env::temp_dir().join(format!("rdra-state-models-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let exported: Vec<StateModels> = samples()
        .iter()
        .filter_map(|sample| {
            // `-o` names a directory only without an extension.
            let dir = scratch.join(sample.replace(['/', '.'], "_"));
            export_state_models(&dir, sample, sample)
        })
        .collect();
    let found_any: BTreeSet<String> = tlc_alone_and_bfs(&tlc, &exported)
        .into_iter()
        .flatten()
        .collect();
    let _ = std::fs::remove_dir_all(&scratch);
    for name in [
        "Deadlock_Order_status_paid",
        "Deadlock_Order_status_shipped",
        "Witness_Order_status_refunded",
    ] {
        assert!(found_any.contains(name), "no sample shows {name}");
    }
}

/// `deadlock.rdra` with a flag that Mark and Unmark toggle forever: a paid Order still has a
/// move, but none of its status's cone, so the toggle does not hide the deadlock from TLC.
const TOGGLED_DEADLOCK: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  flag: Bool @default(false)
}
usecase Place "注文確定"
usecase Pay "支払い"
usecase Ship "発送"
usecase Express "即日発送"
usecase Mark "印"
usecase Unmark "印消し"
creates(Place, Order)
updates(Pay, Order)
updates(Ship, Order)
updates(Express, Order)
updates(Mark, Order)
updates(Unmark, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Express, Order.status, pending -> shipped)
sets(Mark, Order.flag == true)
sets(Unmark, Order.flag == false)
condition Packed "梱包済み"
holds(Packed, Order.status == pending)
applies_to(Packed, Ship)
"#;

#[test]
fn tlc_alone_finds_a_deadlock_beside_a_toggled_column() {
    let Some(tlc) = require_tlc() else { return };
    let scratch = std::env::temp_dir().join(format!("rdra-toggled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let file = scratch.join("toggled.rdra");
    std::fs::write(&file, TOGGLED_DEADLOCK).expect("write model");
    let exported = export_state_models(&scratch.join("tla"), "toggled", file.to_str().unwrap())
        .expect("the model builds");
    let found = tlc_alone_and_bfs(&tlc, &[exported]).remove(0);
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        ["Deadlock_Order_status_paid"]
    );
}

/// Post moves a Ledger only after Execute raised EvExecuted on the Ledger's Order. A ledger
/// at `open` beside an Order still at `requested` is not stuck: Execute sets the event mark
/// and Post can then run. Neither engine reports a deadlock (#261).
const TRIGGERED_POST: &str = r#"
entity Order "依頼" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
entity Ledger "台帳" { id: Int @pk  state: Enum(open, posted) @default(open) }
relate(Ledger, Order, N:1)
usecase Request "依頼"
usecase OpenLedger "台帳作成"
usecase Execute "実行"
usecase Post "記帳"
event EvExecuted "実行済"
creates(Request, Order)
creates(OpenLedger, Ledger)
transitions(Execute, Order.status, requested -> executed)
raises(Execute, EvExecuted)
triggers(EvExecuted, Post)
transitions(Post, Ledger.state, open -> posted)
"#;

#[test]
fn tlc_alone_finds_no_deadlock_before_a_handler_s_event_is_raised() {
    let Some(tlc) = require_tlc() else { return };
    let scratch = std::env::temp_dir().join(format!("rdra-triggered-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let file = scratch.join("triggered.rdra");
    std::fs::write(&file, TRIGGERED_POST).expect("write model");
    let exported = export_state_models(&scratch.join("tla"), "triggered", file.to_str().unwrap())
        .expect("the model builds");
    assert!(
        exported.run.iter().any(|m| m == "StateModel_Ledger"),
        "{:?}",
        exported.run
    );
    let found = tlc_alone_and_bfs(&tlc, &[exported]).remove(0);
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(found.is_empty(), "TLC alone: {found:?}");
}

/// `deadlock.rdra`'s shape with Ship guarded by a column that Flip and Flop move back and
/// forth: a paid Order is stuck, which the BFS reports, but every state keeps a move of the
/// status's cone, so TLC alone passes the `Deadlock_` invariants. The known gap of the
/// `StateModel_` specs (docs/formal-verification.md, "State models with TLC alone"); an exact
/// encoding fails this test.
const CYCLING_CONE_DEADLOCK: &str = r#"
entity Order "o" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  review: Enum(a, b, ok) @default(a)
}
usecase Place "p"
usecase Pay "p"
usecase Ship "s"
usecase Flip "f"
usecase Flop "g"
usecase Express "x"
creates(Place, Order)
updates(Pay, Order)
updates(Ship, Order)
updates(Flip, Order)
updates(Flop, Order)
updates(Express, Order)
transitions(Express, Order.status, pending -> shipped)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Flip, Order.review, a -> b)
transitions(Flop, Order.review, b -> a)
condition Ok "ok"
holds(Ok, Order.review == ok)
applies_to(Ok, Ship)
"#;

#[test]
fn tlc_alone_misses_a_deadlock_while_its_cone_cycles() {
    let Some(tlc) = require_tlc() else { return };
    let scratch = std::env::temp_dir().join(format!("rdra-cycling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let file = scratch.join("cycling.rdra");
    std::fs::write(&file, CYCLING_CONE_DEADLOCK).expect("write model");
    let exported = export_state_models(&scratch.join("tla"), "cycling", file.to_str().unwrap())
        .expect("the model builds");
    assert!(
        exported.bfs.contains("Deadlock_Order_status_paid"),
        "BFS: {:?}",
        exported.bfs
    );
    let module = "StateModel_Order";
    let cfg = read_cfg(&exported.dir, "cycling", module);
    let (deadlocks, _) = violated_by_hand(&tlc, &exported.dir, module, &cfg, "deadlocks", true);
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(deadlocks.is_empty(), "TLC alone: {deadlocks:?}");
}
