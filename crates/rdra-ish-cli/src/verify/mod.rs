//! `rdra-ish verify`: check every invariant, after-assert, and temporal property on its own
//! cone, and a temporal property on the whole system only when its cone does not decide it.
//! The native checker decides each one on the graph BFS explores: a safety cone by the
//! explorer (`bfs_cones`), `stable`, `unchanged`, and `precedes` on their cones
//! (`TransitionSystem::decide_property`), and `always`, `eventually`, and `leads_to` by the
//! runs `property_cone` defines (`native_property`), the whole-model run included
//! (`native_wholes`), under `--temporal-budget` and `WHOLE_MODEL_STATE_CAP`.
//!
//! A cone (`TransitionSystem::target_cone`) is closed: its restricted system reaches exactly
//! the projection of the whole system's states, so a safety verdict on the cone is the
//! verdict on the whole model. A property's cone (`property_cone`) also keeps the actions its
//! `.fair` names whole, so their weak fairness restricts to it; the whole model's
//! `WF_vars(Next)` does not, which is what `property_cone::Verdict` decides.
//!
//! `--with-tlc` also prints a TLA+ module per run and has TLC run the same cones and the
//! same ladder of property runs beside the native checker, each property alone, because TLC
//! does not say which property its lasso violates. A check the two engines decide
//! differently is a tool bug (`disagreements`): the run fails and keeps its specs. A check
//! one engine leaves undecided is no disagreement.

mod tlc_tool;

pub(crate) use self::tlc_tool::find_on_path;
use self::tlc_tool::Tlc;
use crate::export::{checked_system, tla_bounds, tla_file, Modules};
use crate::memory::{tlc_plan, Budgeted, VerifySplit};
use crate::property_cone::{AfterPlain, ConeRun, PropertyCone, Verdict};
use crate::prove::{find_z3, run_proof, Marks, ProofTally};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_env::{current_user_heap, tlc_env_sized, Heap};
use crate::tlc_output::{progress, summarize, Outcome, TlcRun};
use crate::tlc_replay::{explain_property, explain_violation};
use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::{
    ActId, Bounds, BoundsLine, Caps, ComponentRun, Cone, Diag, Fairness, LassoFault, NotChecked,
    PropertyVerdict, Scope, Selection, Severity, StateDiag, Target, TransitionSystem, VarSet,
};
use rdra_ish_core::memory::MemoryPool;
use rdra_ish_core::{in_parallel, jobs, ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaBundle, TlaNames};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

/// Distinct states a whole-model temporal run may reach before it is stopped, natively and
/// under TLC. Separate from `Caps::VERIFY`, the BFS cap per cone: the run's memory and time
/// grow with this one, and `--temporal-budget` bounds only the time.
const WHOLE_MODEL_STATE_CAP: u64 = 1 << 21;

/// `--with-tlc`: TLC runs beside the native checker.
pub struct WithTlc {
    /// `--tlc-jar`: run TLC from `tla2tools.jar`; else `RDRA_TLC_JAR`, else `tlc` on PATH.
    pub jar: Option<PathBuf>,
}

/// What `verify` spends on a check it may not finish, and whether one it leaves undecided
/// fails the run.
pub struct Budget {
    /// Seconds a temporal property's whole-model run gets.
    pub temporal: Duration,
    /// `--deny-undecided`: an undecided finding is an error, not a warning.
    pub deny_undecided: bool,
    /// `--prove`: z3 tries each state rule for any number of rows.
    pub prove: Option<Prove>,
    /// `--memory`, split between BFS and the TLC runs under `--with-tlc`.
    pub memory: VerifySplit,
}

/// `verify --prove`.
pub struct Prove {
    /// `--z3`: the binary; else `RDRA_Z3`, else `z3` on PATH.
    pub z3: Option<PathBuf>,
    /// `--deny-unproved`: a rule left unproved fails the run.
    pub deny_unproved: bool,
}

/// A printed module and its cfg.
struct Spec {
    module: String,
    tla: String,
    cfg: String,
}

/// One TLC run: a module, its cfg, and the checks it answers.
struct Job {
    module: String,
    tla: String,
    cfg: String,
    checks: Vec<String>,
    kind: JobKind,
}

/// What a run's counterexamples replay on.
enum JobKind {
    Cone(Box<ConeSystem>),
    /// `properties()[i]` on its cone.
    PropertyCone(Box<PropertyJob>),
    /// `properties()[i]` on the whole system, after its cone left it undecided.
    Whole(usize),
}

/// A property on its cone; the `_Full` module beside it is the unreduced cone, run after a
/// counterexample on the reduced one, and the `_WF` module adds `WF_vars(Next)` for the
/// second run an `eventually` or `leads_to` may need. `names` are the unreduced cone's, which
/// both of those print.
struct PropertyJob {
    index: usize,
    cone: PropertyCone,
    names: TlaNames,
    full: Option<Spec>,
    wf: Option<Spec>,
}

impl Job {
    /// A guess at how long the run takes, to start the long ones first: a temporal property
    /// before a safety cone, since TLC also searches its behavior graph, then the system with
    /// more variables.
    fn weight(&self) -> (bool, usize) {
        match &self.kind {
            JobKind::Cone(cone) => (false, cone.system.var_ids().count()),
            JobKind::PropertyCone(p) => (true, p.cone.reduced.var_ids().count()),
            JobKind::Whole(_) => (true, usize::MAX),
        }
    }
}

/// A cone as TLC checks it: the whole system's cone, its restricted system, and the names
/// its module prints.
struct ConeSystem {
    cone: Cone,
    /// The invariants and after-asserts whose cone this is.
    targets: Vec<Target>,
    system: TransitionSystem,
    names: TlaNames,
    /// Per action of the cone before any reduction, by its id in the whole system: how many
    /// guards and params the cone leaves it. A cone drops a guard that reads outside it and
    /// a param whose entity it leaves out, so a larger cone may hold an action back where a
    /// smaller one lets it run (`covering`).
    steps: BTreeMap<ActId, (usize, usize)>,
}

/// What a run decides about its checks.
enum Decision {
    Decided(Outcome),
    /// The property is not decided, and why. `stop`: why a whole-model run stopped, which
    /// says what to change; `None` when a cone left it undecided.
    Undecided {
        why: String,
        stop: Option<Stop>,
    },
}

impl Decision {
    /// What the run adds to the verdict of the whole: an undecided property fails nothing.
    fn outcome(&self) -> Outcome {
        match self {
            Decision::Decided(outcome) => *outcome,
            Decision::Undecided { .. } => Outcome::Ok,
        }
    }

    /// The verdict, when the run gave one.
    fn verdict(&self) -> Option<Outcome> {
        match self {
            Decision::Decided(outcome) => Some(*outcome),
            Decision::Undecided { .. } => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Decision::Decided(outcome) => outcome.as_str(),
            Decision::Undecided { .. } => "not decided",
        }
    }
}

/// Why a whole-model run stopped before it decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Budget,
    StateCap,
    Memory,
}

/// A run line: `<module> [<checks>]: <verdict> (<states> states, <notes>)`, the same for
/// either engine.
fn run_line(
    module: &str,
    checks: &[String],
    verdict: &str,
    states: Option<u64>,
    notes: &[String],
) -> String {
    let inside: Vec<String> = states
        .map(|n| format!("{n} states"))
        .into_iter()
        .chain(notes.iter().cloned())
        .collect();
    let inside = match inside.is_empty() {
        true => String::new(),
        false => format!(" ({})", inside.join(", ")),
    };
    format!("{module} [{}]: {verdict}{inside}", checks.join(", "))
}

/// One TLC run of a job, under `--with-tlc`.
struct Ran {
    decision: Decision,
    /// The run whose counterexample is explained: the last one of the job.
    run: TlcRun,
    log: String,
    /// What the run line says after the state count.
    notes: Vec<String>,
    /// A property cone's runs and what each said, compared with the native runs.
    runs: Vec<(ConeRun, Outcome)>,
}

impl Ran {
    fn line(&self, module: &str, checks: &[String]) -> String {
        let states = self.run.distinct_states;
        run_line(module, checks, self.decision.as_str(), states, &self.notes)
    }
}

/// The whole-model module name, and the directory `-o` names: `-o X.tla` writes next to
/// `X.tla` and names the whole-model module `X`; any other `-o` is a directory.
fn out_names(out: Option<PathBuf>) -> (Option<PathBuf>, String) {
    match out {
        Some(path) => match tla_file(&path) {
            Some((parent, stem)) => (Some(parent), stem),
            None => (Some(path), "RdraSpec".to_string()),
        },
        None => (None, "RdraSpec".to_string()),
    }
}

/// Creates the directory the specs go to: `out`, or a temp dir when `-o` names none.
fn spec_dir(out: Option<PathBuf>) -> Result<PathBuf> {
    let dir = out.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("rdra-ish-tlc-{}", std::process::id()))
    });
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    // TLC runs inside `dir`, so the metadir and java tmpdir paths must not be relative.
    dunce::canonicalize(&dir).with_context(|| format!("failed to resolve {}", dir.display()))
}

/// Each distinct cone of a selected invariant or after-assert, in target order, named after
/// the first check that reads it. Cones over the same vars share one run: an action kept for
/// an after-assert writes nothing in the cone, so it adds edges but no states.
fn cone_jobs(
    ts: &TransitionSystem,
    names: &TlaNames,
    targets: &[Target],
    modules: &mut Modules,
    caps: Caps,
) -> Vec<Job> {
    let mut at: BTreeMap<VarSet, usize> = BTreeMap::new();
    let mut order: Vec<(Cone, String, Vec<Target>)> = Vec::new();
    for &target in targets {
        let first = match target {
            // BFS decides a `can_reach`, which TLC cannot check.
            Target::Entity(_) | Target::Reach(_) => continue,
            Target::Invariant(i) => names.invariant(i).to_string(),
            Target::AfterAssert(i) => names.after(i).to_string(),
        };
        let cone = ts.target_cone(target);
        match at.get(&cone.vars) {
            Some(&k) => {
                order[k].0.keep.extend(cone.keep);
                order[k].2.push(target);
            }
            None => {
                at.insert(cone.vars.clone(), order.len());
                order.push((cone, first, vec![target]));
            }
        }
    }
    // The names are claimed in target order; reducing and printing a cone reads nothing of
    // another, so the cones are prepared side by side.
    let named: Vec<(Cone, String, Vec<Target>)> = order
        .into_iter()
        .map(|(cone, first, shared)| (cone, modules.claim(&format!("Cone_{first}")), shared))
        .collect();
    in_parallel(named, jobs(), |(cone, module, shared)| {
        let seeds = shared.iter().map(|t| ts.target_seed(*t));
        let sub = ts.reduced_cone(&cone, seeds, caps);
        let bundle = tla::print(&sub, PrintMode::Safety, &module);
        let n = &bundle.names;
        let checks = (0..sub.invariants().len())
            .map(|i| n.invariant(i).to_string())
            .chain((0..sub.after_asserts().len()).map(|i| n.after(i).to_string()))
            .collect();
        Job {
            module: bundle.module_name,
            tla: bundle.tla,
            cfg: bundle.cfg,
            checks,
            kind: JobKind::Cone(Box::new(ConeSystem {
                steps: ts
                    .restrict_to(&cone.vars, &cone.keep)
                    .actions()
                    .iter()
                    .map(|a| (a.source, (a.guards.len(), a.params.len())))
                    .collect(),
                targets: shared,
                cone,
                system: sub,
                names: bundle.names,
            })),
        }
    })
}

/// One run per `always`, `eventually`, and `leads_to` on its cone, module `Cone_<property>`, with the `_WF`
/// module (the same cone with `WF_vars(Next)`) printed beside it for the second run. Every
/// `Cone_<property>` is claimed before any `_Full` or `_WF`, so each property's own module
/// keeps its name.
fn property_jobs(
    ts: &TransitionSystem,
    names: &TlaNames,
    modules: &mut Modules,
    caps: Caps,
) -> Vec<Job> {
    // The forms `LtlKind::is_explored` names are decided on their cones by
    // `explored_properties`, with no run of their own.
    let tlc: Vec<usize> = (0..ts.properties().len())
        .filter(|&i| !ts.properties()[i].formula.kind().is_explored())
        .collect();
    let plain: Vec<String> = tlc
        .iter()
        .map(|&i| modules.claim(&format!("Cone_{}", names.property(i))))
        .collect();
    // Reducing a property's cone reads nothing of another's; the `_Full` and `_WF` names are
    // claimed afterwards, in property order.
    let cones = in_parallel(tlc.clone(), jobs(), |i| PropertyCone::under(ts, i, caps));
    tlc.into_iter()
        .zip(plain.into_iter().zip(cones))
        .map(|(i, (module, cone))| {
            let name = names.property(i);
            let plain = cone.bundle(ConeRun::Plain, &module);
            let spec = |b: TlaBundle| Spec {
                module: b.module_name,
                tla: b.tla,
                cfg: b.cfg,
            };
            let mut names = plain.names;
            let full = cone.is_reduced().then(|| {
                let b = cone.bundle(ConeRun::Full, &modules.claim(&format!("{module}_Full")));
                names = b.names.clone();
                spec(b)
            });
            let wf = cone.kind.is_liveness().then(|| {
                let module = modules.claim(&format!("{module}_WF"));
                spec(cone.bundle(ConeRun::WfNext, &module))
            });
            Job {
                module: plain.module_name,
                tla: plain.tla,
                cfg: plain.cfg,
                checks: vec![name.to_string()],
                kind: JobKind::PropertyCone(Box::new(PropertyJob {
                    index: i,
                    cone,
                    names,
                    full,
                    wf,
                })),
            }
        })
        .collect()
}

/// `properties()[i]` on the whole system, module `module` (`<whole>_<property>`): the same
/// module text as the whole-model module, fairness included, and a cfg that names the one
/// property.
fn whole_job(ts: &TransitionSystem, names: &TlaNames, module: &str, i: usize) -> Job {
    let name = names.property(i);
    let bundle = tla::print(ts, PrintMode::Property(i), module);
    Job {
        module: bundle.module_name,
        tla: bundle.tla,
        cfg: bundle.cfg,
        checks: vec![name.to_string()],
        kind: JobKind::Whole(i),
    }
}

/// One TLC run of `job`; a cone run with a violation is rerun without the checks it has
/// reported until a rerun reports no new one. With `-continue` TLC reports one violated check
/// per state or step, the first in cfg order, so a check violated only where an earlier one
/// is would otherwise go unreported. The reruns' violations and logs join the first run's.
/// A property cone run that finds a counterexample is followed by the `_WF` run when the
/// verdict asks for it (`property_cone`).
fn run_job(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    job: &Job,
    (workers, heap): (&str, u64),
) -> Result<Ran> {
    let cfg = format!("{}.cfg", job.module);
    let safety = matches!(job.kind, JobKind::Cone(_));
    // With several workers TLC reaches the same states, so an `ok` is the `ok` of one
    // worker, but it no longer finds a violation by the shortest trace, nor the same one on
    // every run. A safety cone that does not pass is therefore run again on one worker, and
    // only that run is reported. A property's lasso differs from run to run on one worker too.
    let workers = match (safety, workers) {
        (true, many) if many != "1" => {
            let ran = run_cfg(tlc, dir, &job.module, heap, &cfg, many, safety)?;
            if ran.decision.outcome() == Outcome::Ok {
                return Ok(ran);
            }
            "1"
        }
        _ => workers,
    };
    let mut ran = run_cfg(tlc, dir, &job.module, heap, &cfg, workers, safety)?;
    match &job.kind {
        JobKind::Cone(cs) => {
            let mut reruns = 0;
            while ran.decision.outcome() == Outcome::Counterexample {
                let found = ran.run.violated_names();
                if found.len() >= job.checks.len() {
                    break;
                }
                reruns += 1;
                let cfg_name = modules.rest_cfg(&job.module, reruns);
                let cfg = tla::cfg_without(&cs.system, &cs.names, PrintMode::Safety, &found);
                let path = dir.join(&cfg_name);
                fs::write(&path, cfg)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                let more = run_cfg(tlc, dir, &job.module, heap, &cfg_name, workers, true)?;
                ran.log.push_str(&more.log);
                ran.run.violations.extend(more.run.violations);
                if ran.run.violated_names().len() == found.len() {
                    break;
                }
            }
        }
        JobKind::PropertyCone(p) => {
            let (cone, wf) = (&p.cone, &p.wf);
            let mut plain = ran.decision.outcome();
            ran.runs.push((ConeRun::Plain, plain));
            // A counterexample on the reduced cone is real, but its lasso is read on the
            // unreduced cone, whose run gives the verdict from here on.
            if let (Outcome::Counterexample, Some(full)) = (plain, &p.full) {
                let cfg = format!("{}.cfg", full.module);
                let more = run_cfg(tlc, dir, &full.module, heap, &cfg, workers, false)?;
                ran.notes.push(format!(
                    "{}: {}",
                    full.module,
                    more.decision.outcome().as_str()
                ));
                ran.log.push_str(&more.log);
                ran.run = more.run;
                ran.runs.push((ConeRun::Full, more.decision.outcome()));
                plain = match more.decision.outcome() {
                    Outcome::Ok => {
                        ran.notes.push(
                            "the reduced cone finds a counterexample the cone does not (a tool bug)"
                                .to_string(),
                        );
                        Outcome::ToolError
                    }
                    other => other,
                };
            }
            let verdict = match cone.after_plain(plain) {
                AfterPlain::Decided(verdict) => verdict,
                AfterPlain::NeedsWfNext => {
                    let wf = wf
                        .as_ref()
                        .expect("an eventually or leads_to has a WF module");
                    let cfg = format!("{}.cfg", wf.module);
                    let more = run_cfg(tlc, dir, &wf.module, heap, &cfg, workers, false)?;
                    let wf_outcome = more.decision.outcome();
                    ran.runs.push((ConeRun::WfNext, wf_outcome));
                    ran.notes
                        .push(format!("{}: {}", wf.module, wf_outcome.as_str()));
                    ran.log.push_str(&more.log);
                    ran.run = more.run;
                    cone.after_wf(plain, wf_outcome)
                }
            };
            ran.decision = match verdict {
                Verdict::Ok => Decision::Decided(Outcome::Ok),
                Verdict::Counterexample => Decision::Decided(Outcome::Counterexample),
                Verdict::ToolError => Decision::Decided(Outcome::ToolError),
                Verdict::Undecided(why) => {
                    ran.notes.push(why.as_str().to_string());
                    Decision::Undecided {
                        why: why.as_str().to_string(),
                        stop: None,
                    }
                }
            };
        }
        JobKind::Whole(_) => {}
    }
    Ok(ran)
}

fn tlc_command(tlc: &Tlc, dir: &Path, module: &str, heap: u64, workers: &str) -> Result<Command> {
    let meta = dir.join(format!("{module}.states"));
    // TLC unpacks its standard modules into java.io.tmpdir; parallel runs must not share it.
    let tmp = dir.join(format!("{module}.tmp"));
    fs::create_dir_all(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;
    let (key, value) = tlc_env_sized(heap, &tmp);
    let mut command = tlc.command();
    command
        .env(key, value)
        .args(["-workers", workers, "-metadir"])
        .arg(&meta)
        .current_dir(dir);
    Ok(command)
}

fn clean_up(dir: &Path, module: &str) {
    let _ = fs::remove_dir_all(dir.join(format!("{module}.states")));
    let _ = fs::remove_dir_all(dir.join(format!("{module}.tmp")));
}

/// `safety` runs TLC with `-continue`, which keeps a cone run going past a violation so it
/// reports more than one check, each with its own trace (see `run_job`). A property run
/// keeps TLC's default and stops at its first violation.
fn run_cfg(
    tlc: &Tlc,
    dir: &Path,
    module: &str,
    heap: u64,
    cfg: &str,
    workers: &str,
    safety: bool,
) -> Result<Ran> {
    let tla = format!("{module}.tla");
    let mut command = tlc_command(tlc, dir, module, heap, workers)?;
    if safety {
        command.arg("-continue");
    }
    let output = command
        .args(["-config", cfg, &tla])
        .output()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;
    clean_up(dir, module);
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let run = TlcRun::parse(&log);
    Ok(Ran {
        decision: Decision::Decided(run.outcome(output.status.success())),
        run,
        log,
        notes: Vec::new(),
        runs: Vec::new(),
    })
}

/// What to change when a property's whole-model run stopped. A property without `.fair` rests
/// on the whole model's `WF_vars(Next)`; naming the use cases it relies on lets its cone decide
/// it. The state cap is fixed, so only a smaller model or a cone lifts it.
fn undecided_hint(stop: Stop) -> &'static str {
    match stop {
        Stop::StateCap => "the model has more states than the cap; --temporal-budget does not lift it, so name the use cases the property relies on with .fair(UseCase, ...) to decide it on its cone, or shrink the model",
        Stop::Budget => "raise --temporal-budget, or name the use cases the property relies on with .fair(UseCase, ...) to decide it on its cone",
        Stop::Memory => "raise --memory, or name the use cases the property relies on with .fair(UseCase, ...) to decide it on its cone",
    }
}

/// A whole-model property run under a budget: TLC's progress lines (about one a minute) print
/// as they come, and the run is killed past `budget` or past `WHOLE_MODEL_STATE_CAP` distinct states,
/// and left undecided unless TLC had already printed its verdict.
fn run_whole(
    tlc: &Tlc,
    dir: &Path,
    job: &Job,
    (workers, heap): (&str, u64),
    budget: Duration,
) -> Result<Ran> {
    let tla = format!("{}.tla", job.module);
    let cfg = format!("{}.cfg", job.module);
    let mut child = tlc_command(tlc, dir, &job.module, heap, workers)?
        .args(["-config", &cfg, &tla])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
    })
    .collect();
    drop(tx);
    let start = Instant::now();
    let cap = WHOLE_MODEL_STATE_CAP;
    let mut log = String::new();
    let mut stopped: Option<String> = None;
    let mut capped = false;
    let mut status = None;
    while status.is_none() {
        while let Ok(line) = rx.try_recv() {
            // TLC prints a last progress line with an empty queue when it finishes; the run
            // line reports that.
            if let Some(p) = progress(&line).filter(|p| p.queued > 0) {
                println!(
                    "  {}: {} distinct states, {} queued, {} s",
                    job.module,
                    p.distinct,
                    p.queued,
                    start.elapsed().as_secs()
                );
                if p.distinct > cap && stopped.is_none() {
                    stopped = Some(format!("stopped at the state cap of {cap} states"));
                    capped = true;
                    let _ = child.kill();
                }
            }
            log.push_str(&line);
            log.push('\n');
        }
        status = child.try_wait().context("failed to wait for TLC")?;
        if status.is_none() && start.elapsed() > budget && stopped.is_none() {
            stopped = Some(format!(
                "stopped at the time budget of {} s",
                budget.as_secs()
            ));
            let _ = child.kill();
        }
        if status.is_none() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    for line in rx.try_iter() {
        log.push_str(&line);
        log.push('\n');
    }
    clean_up(dir, &job.module);
    let run = TlcRun::parse(&log);
    let exit_ok = status.is_some_and(|s| s.success());
    // A kill that lands after TLC printed its verdict stops nothing that decides.
    let (decision, notes) = match (stopped, run.verdict()) {
        (Some(_), Some(outcome)) => (Decision::Decided(outcome), Vec::new()),
        (Some(why), None) => (
            Decision::Undecided {
                why: why.clone(),
                stop: Some(match capped {
                    true => Stop::StateCap,
                    false => Stop::Budget,
                }),
            },
            vec![why],
        ),
        (None, _) => (Decision::Decided(run.outcome(exit_ok)), Vec::new()),
    };
    Ok(Ran {
        decision,
        run,
        log,
        notes,
        runs: Vec::new(),
    })
}

/// Runs the cone jobs, up to 8 at once and no more than `share` bytes of `-Xmx` hold
/// (`tlc_plan`). Each run gets the cores divided by the runs at once,
/// and no fewer than 2 workers on 4 cores or more: most models have a few long cones and
/// many short ones, and once the short ones are done a long run on one worker would leave
/// the other cores idle.
///
/// A cone another cone covers (`covering`) waits for that one: when it passes, the covered
/// cone is `ok` without a run, and when it does not, the covered cone runs as it would have.
fn run_cones(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    jobs: &[Job],
    share: u64,
) -> Result<Vec<Ran>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let run = |only: &[usize]| {
        let wanted = cores.min(8).min(only.len()).max(1);
        let plan = tlc_plan(share, wanted, Heap::Cone, current_user_heap());
        let workers = cone_workers(cores, plan.at_once).to_string();
        run_some(
            jobs,
            only,
            plan.at_once,
            (&workers, plan.heap),
            |job, jvm| run_job(tlc, dir, modules, job, jvm),
        )
    };
    let covers = covering(jobs);
    let first: Vec<usize> = (0..jobs.len()).filter(|i| covers[*i].is_none()).collect();
    let mut ran: Vec<Option<Ran>> = jobs.iter().map(|_| None).collect();
    for (i, r) in first.iter().zip(run(&first)?) {
        ran[*i] = Some(r);
    }
    let passed = |ran: &[Option<Ran>], i: usize| {
        matches!(
            ran[i].as_ref().map(|r| &r.decision),
            Some(Decision::Decided(Outcome::Ok))
        )
    };
    let again: Vec<usize> = (0..jobs.len())
        .filter(|i| covers[*i].is_some_and(|by| !passed(&ran, by)))
        .collect();
    for (i, r) in again.iter().zip(run(&again)?) {
        ran[*i] = Some(r);
    }
    Ok(ran
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.unwrap_or_else(|| Ran {
                decision: Decision::Decided(Outcome::Ok),
                run: TlcRun::parse(""),
                log: String::new(),
                notes: vec![format!(
                    "checked in {}",
                    jobs[covers[i].expect("a job without a run is covered")].module
                )],
                runs: Vec::new(),
            })
        })
        .collect())
}

/// For each job, the job that covers it: a safety cone that passes only if this one does.
/// Among the cones that cover a job, the one with the most checks is taken, the first of
/// those: no cone covers it in turn, so it runs.
fn covering(jobs: &[Job]) -> Vec<Option<usize>> {
    let cones: Vec<Option<Covered>> = jobs
        .iter()
        .map(|job| match &job.kind {
            JobKind::Cone(cone) => Some(Covered::of(job, cone)),
            JobKind::PropertyCone(_) | JobKind::Whole(_) => None,
        })
        .collect();
    (0..jobs.len())
        .map(|a| {
            let small = cones[a].as_ref()?;
            (0..jobs.len())
                .filter(|&b| {
                    cones[b].as_ref().is_some_and(|large| {
                        large.covers(small) && (large.checks.len() > small.checks.len() || b < a)
                    })
                })
                .max_by_key(|&b| {
                    let checks = cones[b].as_ref().map_or(0, |c| c.checks.len());
                    (checks, std::cmp::Reverse(b))
                })
        })
        .collect()
}

/// What `covering` compares of a safety cone.
struct Covered<'a> {
    checks: BTreeSet<&'a str>,
    cone: &'a ConeSystem,
}

impl<'a> Covered<'a> {
    fn of(job: &'a Job, cone: &'a ConeSystem) -> Self {
        Covered {
            checks: job.checks.iter().map(String::as_str).collect(),
            cone,
        }
    }

    /// Whether this cone passing means `small` passes: this cone takes every step `small`
    /// takes, and checks everything `small` checks.
    ///
    /// - Its module holds every check of `small`.
    /// - It holds the vars and the kept actions of `small`. For an invariant that follows
    ///   from the cones being closed. For an after-assert it does not: the assert's cone
    ///   holds everything its use case writes, and a cone without those writes takes the
    ///   use case's step as a stutter, on which the assert passes.
    /// - It leaves each action of `small` the guards and params `small` leaves it, no more.
    ///   A cone drops the guards that read outside it and the params of the entities it
    ///   leaves out (a creation it does not count, a triggering event it does not hold). A
    ///   guard or param this cone keeps and `small` drops would hold back here a step
    ///   `small` takes, and `small` could fail a check this cone passes.
    ///
    /// Then every step of `small` is a step here from the same vars to the same vars, so
    /// this cone reaches every state `small` reaches.
    fn covers(&self, small: &Covered) -> bool {
        let (large, small_cone) = (&self.cone, &small.cone);
        !small.checks.is_empty()
            && small.checks.is_subset(&self.checks)
            && small_cone.cone.vars.is_subset(&large.cone.vars)
            && small_cone.cone.keep.is_subset(&large.cone.keep)
            && small_cone
                .steps
                .iter()
                .all(|(action, shape)| large.steps.get(action) == Some(shape))
    }
}

/// TLC workers per cone run when `at_once` runs share `cores`.
fn cone_workers(cores: usize, at_once: usize) -> usize {
    let floor = if cores >= 4 { 2 } else { 1 };
    (cores / at_once).max(floor)
}

/// Runs the whole-model property jobs, up to 2 at once (each takes the temporal heap) and no
/// more than `share` bytes of `-Xmx` hold (`tlc_plan`), sharing the cores.
fn run_wholes(
    tlc: &Tlc,
    dir: &Path,
    jobs: &[Job],
    budget: Duration,
    share: u64,
) -> Result<Vec<Ran>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let wanted = jobs.len().clamp(1, 2);
    let plan = tlc_plan(share, wanted, Heap::Temporal, current_user_heap());
    let workers = (cores / plan.at_once).max(1).to_string();
    let all: Vec<usize> = (0..jobs.len()).collect();
    run_some(
        jobs,
        &all,
        plan.at_once,
        (&workers, plan.heap),
        |job, jvm| run_whole(tlc, dir, job, jvm, budget),
    )
}

/// Runs the jobs `only` names on `threads` threads, each TLC run with `jvm` (its workers and
/// its heap in bytes), the heaviest first (`Job::weight`). The results come back in the order of `only`.
fn run_some(
    jobs: &[Job],
    only: &[usize],
    threads: usize,
    jvm: (&str, u64),
    run: impl Fn(&Job, (&str, u64)) -> Result<Ran> + Sync,
) -> Result<Vec<Ran>> {
    // Positions in `only`, heaviest job first.
    let mut order: Vec<usize> = (0..only.len()).collect();
    order.sort_by_key(|at| std::cmp::Reverse(jobs[only[*at]].weight()));
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Result<Ran>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.min(only.len()) {
            s.spawn(|| {
                while let Some(&at) = order.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let ran = run(&jobs[only[at]], jvm);
                    done.lock().expect("lock").push((at, ran));
                }
            });
        }
    });
    let mut done = done.into_inner().expect("lock");
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, r)| r).collect()
}

fn write_spec(dir: &Path, module: &str, tla: &str, cfg: &str) -> Result<()> {
    for (ext, text) in [("tla", tla), ("cfg", cfg)] {
        let path = dir.join(format!("{module}.{ext}"));
        fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Each violated check of a cone run once, in model terms, from its first TLC trace. A trace
/// that does not replay is a disagreement between the engines and says so.
fn model_traces(
    program: &ResolvedProgram,
    model: &SemanticModel,
    (ts, names): (&TransitionSystem, &TlaNames),
    job: &JobKind,
    run: &TlcRun,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let explained: Vec<Result<StateDiag, String>> = match job {
        JobKind::Cone(cs) => run
            .violations
            .iter()
            .filter(|v| v.name.is_some() && seen.insert(&v.name))
            .map(|v| {
                let mut diag = explain_violation(&cs.system, &cs.names, v)?;
                if let Some(trace) = diag.trace_mut() {
                    ts.annotate(&cs.cone, trace);
                }
                Ok(diag)
            })
            .collect(),
        JobKind::PropertyCone(p) => run
            .violations
            .iter()
            .map(|v| explain_property(&p.cone.system, &p.names, p.cone.index, v))
            .collect(),
        JobKind::Whole(i) => run
            .violations
            .iter()
            .map(|v| explain_property(ts, names, *i, v))
            .collect(),
    };
    explained
        .into_iter()
        .map(|explained| match explained {
            Ok(diag) => {
                let message = render_violation(program, model, &diag, TraceSource::Tlc)
                    .expect("explain returns a violation");
                Finding::new(&diag, &[], message).to_string()
            }
            Err(why) => {
                format!("error: {why}; BFS and TLC disagree (a tool bug). Raw TLC trace follows.")
            }
        })
        .collect()
}

/// Runs `verify`. `bounds` is `(--instance-count, --int-range-max)`; `with_tlc` is
/// `--with-tlc` and its `--tlc-jar`.
pub fn run(
    program: &ResolvedProgram,
    model: &SemanticModel,
    out: Option<PathBuf>,
    (instance_count, int_range_max): (u32, u32),
    scope: &Scope,
    Budget {
        temporal: budget,
        deny_undecided,
        prove,
        memory,
    }: Budget,
    with_tlc: Option<WithTlc>,
) -> Result<()> {
    let bounds = tla_bounds(instance_count, int_range_max)?;
    let built = checked_system(model, bounds, scope)?;
    let (ts, selection) = (built.ts, built.selection);
    let (out, whole_name) = out_names(out);
    let keep = out.is_some();
    let names = TlaNames::new(&ts);

    // A scope checks no property: `--buc` drops use cases fairness would read, and an error
    // `--entity` defers may sit in a property's cone.
    let temporal = !ts.properties().is_empty() && scope.is_whole();
    // The whole-model module keeps the name `-o` gives it; the generated names yield.
    let mut modules = Modules::default();
    let whole_stem = whole_name.clone();
    let whole_name = match temporal {
        true => modules.claim(&whole_name),
        false => whole_name,
    };
    // A `.lemma()` rule only helps `--prove`, which keeps a lemma only when it proves it, so
    // no cone runs for one: a lemma that joins two large cones would cost more than the rules
    // it helps. `check` still checks it.
    let lemmas = rdra_ish_core::lifecycle::prove::lemma_invariants(model, &ts);
    let targets: Vec<Target> = selection
        .targets
        .iter()
        .copied()
        .filter(|t| !matches!(t, Target::Invariant(i) if lemmas.contains(i)))
        .collect();
    // Every BFS of the run, the ones that reduce a cone included, is charged to one pool:
    // all of `--memory`, or BFS's half of it under `--with-tlc`.
    let raise = if memory.bfs == memory.limit {
        Budgeted::Verify.raise().to_string()
    } else {
        format!("{}; BFS has {} of it", Budgeted::Verify.raise(), memory.bfs)
    };
    let caps = Caps::VERIFY.charged_to(MemoryPool::share_of(
        memory.limit,
        memory.bfs,
        Box::leak(raise.into_boxed_str()),
    ));
    let mut jobs = cone_jobs(&ts, &names, &targets, &mut modules, caps);
    let cones = jobs.len();
    let line = print_plan(
        model,
        &ts,
        scope,
        &selection,
        bounds,
        (cones, temporal, with_tlc.is_some()),
        budget,
    );
    let mut whole_modules = Vec::new();
    if temporal {
        jobs.extend(property_jobs(&ts, &names, &mut modules, caps));
        whole_modules = (0..ts.properties().len())
            .map(|i| modules.claim(&format!("{whole_stem}_{}", names.property(i))))
            .collect();
    }
    // The modules are written for `-o`, a run by hand, and for TLC under `--with-tlc`.
    let dir = match keep || with_tlc.is_some() {
        true => Some(spec_dir(out)?),
        false => None,
    };
    // The whole-model module with every property, for a run by hand; a property runs on it
    // only when its cone leaves it undecided.
    let whole =
        (temporal && dir.is_some()).then(|| tla::print(&ts, PrintMode::Temporal, &whole_name));
    // TLC starts on the cones while BFS and the proof run here: none reads what another
    // finds. A failure to write the specs or to find a tool is reported after what BFS and
    // the proof print, and starts no TLC run.
    let written = dir
        .as_ref()
        .map(|dir| write_specs_and_index(dir, &jobs, &whole));
    let tlc = with_tlc.map(|w| Tlc::find(w.jar));
    let z3 = prove.as_ref().map(|p| find_z3(p.z3.clone()));
    let start = match (&dir, &written, &tlc, &z3) {
        (Some(dir), Some(Ok(_)), Some(Ok(tlc)), None | Some(Ok(_))) => Some((tlc, dir)),
        _ => None,
    };
    let (bfs, natives, explored, proof, cones_ran) = std::thread::scope(|threads| {
        let cones = start
            .map(|(tlc, dir)| threads.spawn(|| run_cones(tlc, dir, &modules, &jobs, memory.cones)));
        let bfs = state_models(program, model, &ts, scope, &selection, caps);
        let (safety, mut vacuous) = bfs_cones(&ts, &jobs, caps);
        let (properties, property_vacuous) = native_properties(&ts, &jobs, caps);
        // A `--buc` drops use cases that may reach a premise.
        if scope.bucs.is_empty() {
            vacuous.extend(property_vacuous);
            for d in vacuous {
                if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                    eprintln!("{}", Finding::new(&d.diag, &[], text));
                }
            }
        }
        let explored = match temporal {
            true => explored_properties(program, model, &ts, caps, budget),
            false => Explored::default(),
        };
        let open: Vec<usize> = jobs
            .iter()
            .zip(&properties)
            .filter_map(|(job, native)| match (&job.kind, native) {
                (JobKind::PropertyCone(p), Some(n)) if n.goes_whole() => Some(p.index),
                _ => None,
            })
            .collect();
        let wholes = native_wholes(&ts, &open, caps, budget);
        let proof = match (&prove, z3) {
            (Some(Prove { deny_unproved, .. }), Some(z3)) => z3.and_then(|z3| {
                let marks = Marks::of(model, &ts);
                run_proof(&ts, &selection, &marks, &z3, *deny_unproved).map(Some)
            }),
            _ => Ok(None),
        };
        let cones_ran = cones.map(|cones| match cones.join() {
            Ok(ran) => ran,
            Err(panic) => std::panic::resume_unwind(panic),
        });
        let natives = Natives {
            safety,
            properties,
            wholes: open.into_iter().zip(wholes).collect(),
        };
        (bfs, natives, explored, proof, cones_ran)
    });
    let mut end = End {
        bfs,
        explored,
        safety: Safety::default(),
        temporal: Temporal::default(),
        tlc: TlcEnd::default(),
        deny_undecided,
        proof: proof?,
    };

    let mut index = match written {
        Some(written) => Some(written?),
        None => None,
    };
    if let (true, Some((_, index_path))) = (keep, &index) {
        println!(
            "wrote {} specs and {}",
            jobs.len() + usize::from(whole.is_some()),
            index_path.display()
        );
    }
    if jobs.is_empty() && end.explored.checked == 0 && ts.reach_properties().is_empty() {
        println!("nothing to check");
    }
    end.safety = report_bfs_cones(program, model, &jobs, &natives.safety);
    end.temporal = report_properties(program, model, &ts, &jobs, &natives, &whole_modules);
    let undecided = natives.undecided_lines(&names);
    // Under `-o` the whole-model module of each property its cone leaves undecided is
    // written, for a run by hand, as TLC's whole-model run writes it.
    if let (true, Some(dir), Some((index, index_path))) = (keep, &dir, &mut index) {
        let open: Vec<usize> = natives.wholes.iter().map(|(i, _)| *i).collect();
        write_whole_specs(
            dir,
            (index, index_path),
            (&ts, &names),
            &whole_modules,
            &open,
        )?;
    }

    let mut disagreed = Vec::new();
    let mut tlc_outcome = None;
    if let Some(tlc) = tlc {
        let tlc = tlc?;
        let dir = dir.as_deref().expect("--with-tlc writes the specs");
        let (index, index_path) = index.as_mut().expect("--with-tlc writes the index");
        let ran = cones_ran.expect("TLC started once the specs and the tools were there")?;
        for (job, r) in jobs.iter().zip(&ran) {
            println!("tlc: {}", r.line(&job.module, &job.checks));
        }
        let (wholes, whole_ran) = run_tlc_wholes(
            (&tlc, dir),
            (&ts, &names),
            &whole_modules,
            (budget, memory.wholes),
            (&jobs, &ran),
            (index, index_path.as_path()),
        )?;
        disagreed = disagreements(&whole_modules, &jobs, &natives, (&ran, &whole_ran));
        let all_jobs: Vec<&Job> = jobs.iter().chain(&wholes).collect();
        let all_ran: Vec<&Ran> = ran.iter().chain(&whole_ran).collect();
        let outcome = worst(all_ran.iter().map(|r| r.decision.outcome()));
        end.tlc.tool_errors = all_ran
            .iter()
            .filter(|r| r.decision.outcome() == Outcome::ToolError)
            .count();
        if outcome != Outcome::Ok {
            tlc_failures(program, model, (&ts, &names), dir, (&all_jobs, &all_ran));
        }
        tlc_outcome = Some(outcome);
    }
    for d in &disagreed {
        eprintln!("{d}");
    }
    end.tlc.disagreements = disagreed.len();

    // A disagreement reports neither engine's verdict as the result.
    let result = match disagreed.is_empty() {
        true => end.outcome(),
        false => Outcome::ToolError,
    };
    let say = |label: &str, outcome: Outcome| match outcome {
        Outcome::Ok => println!("{label}: ok"),
        other => eprintln!("{label}: {}", other.as_str()),
    };
    say("result", result);
    if let Some(outcome) = tlc_outcome {
        say(
            "tlc_result",
            match disagreed.is_empty() {
                true => outcome,
                false => Outcome::ToolError,
            },
        );
    }
    if let Some(dir) = &dir {
        let failed = tlc_outcome.is_some_and(|o| o != Outcome::Ok);
        let kept = match (!disagreed.is_empty(), failed) {
            (true, _) => Some("the engines disagree"),
            (false, true) => Some("a check failed"),
            (false, false) => None,
        };
        match (keep, kept) {
            (true, Some(_)) => eprintln!("specs: {}", dir.display()),
            (true, None) => {}
            (false, Some(why)) => eprintln!("specs: {} (kept because {why})", dir.display()),
            (false, None) => {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }
    end.finish(&line, &undecided)
}

/// The worst of `outcomes`: a counterexample over a tool error over `ok`.
fn worst(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
    outcomes.into_iter().fold(Outcome::Ok, |a, b| match (a, b) {
        (Outcome::Counterexample, _) | (_, Outcome::Counterexample) => Outcome::Counterexample,
        (Outcome::ToolError, _) | (_, Outcome::ToolError) => Outcome::ToolError,
        _ => Outcome::Ok,
    })
}

/// The header of a run: the bounds line, the cone and property counts, and the fairness
/// assumptions. Returns the bounds line for the closing summary.
fn print_plan(
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    bounds: Bounds,
    (cones, temporal, with_tlc): (usize, bool, bool),
    budget: Duration,
) -> BoundsLine {
    let line =
        BoundsLine::verify(bounds, Some(Caps::VERIFY)).with_scope(scope.line(model, selection));
    println!("{line}");
    let checked = |pick: fn(&Target) -> bool| selection.targets.iter().filter(|t| pick(t)).count();
    println!(
        "cones: {cones} for {} invariant(s) and {} after-assert(s)",
        checked(|t| matches!(t, Target::Invariant(_))),
        checked(|t| matches!(t, Target::AfterAssert(_)))
    );
    let explored = ts
        .properties()
        .iter()
        .filter(|p| p.formula.kind().is_explored())
        .count();
    match (temporal, ts.properties().len() - explored, explored) {
        (_, 0, 0) => println!("temporal properties: none"),
        (true, n, m) => {
            if n > 0 {
                println!(
                    "temporal properties: {n}, each checked on its cone, and on the whole model only when the cone does not decide it (budget {} s)",
                    budget.as_secs()
                );
            }
            if m > 0 {
                println!("order and stability properties: {m}, each decided by BFS on its cone");
            }
        }
        (false, n, m) => println!(
            "temporal properties: {}, not checked: a scoped run checks none",
            n + m
        ),
    }
    match (scope.is_whole(), ts.reach_properties().len()) {
        (_, 0) => {}
        (true, n) => println!(
            "can_reach properties: {n}, decided natively by BFS on each one's cone (TLC cannot check AG EF)"
        ),
        (false, n) => println!("can_reach properties: {n}, not checked: a scoped run checks none"),
    }
    if with_tlc {
        println!("tlc: runs the same cones beside the native checker (--with-tlc); a check the two decide differently fails the run");
    }
    // A verdict under `.fair(...)` holds only on the runs that do not starve those use cases.
    for p in ts.properties().iter().filter(|p| !p.fair.is_empty()) {
        let ucs: Vec<&str> = p
            .fair
            .iter()
            .map(|uk| model.use_cases[*uk].id.as_str())
            .collect();
        println!(
            "  {} assumes weak fairness per instance on {}",
            p.name,
            ucs.join(", ")
        );
    }
    line
}

/// Writes every job's specs, the whole-model module, and `cones.txt` under `dir`. Returns the
/// index text and its path.
fn write_specs_and_index<'a>(
    dir: &Path,
    jobs: impl IntoIterator<Item = &'a Job>,
    whole: &Option<TlaBundle>,
) -> Result<(String, PathBuf)> {
    let mut index = String::new();
    for job in jobs {
        write_spec(dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
        if let JobKind::PropertyCone(p) = &job.kind {
            for extra in p.full.iter().chain(&p.wf) {
                write_spec(dir, &extra.module, &extra.tla, &extra.cfg)?;
                index.push_str(&format!("{}.tla: {}\n", extra.module, job.checks.join(" ")));
            }
        }
    }
    if let Some(w) = &whole {
        write_spec(dir, &w.module_name, &w.tla, &w.cfg)?;
    }
    let index_path = dir.join("cones.txt");
    fs::write(&index_path, &index)
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    Ok((index, index_path))
}

/// Writes the whole-model module of each property in `open` that `cones.txt` does not list
/// yet, and the index with them.
fn write_whole_specs(
    dir: &Path,
    (index, index_path): (&mut String, &Path),
    (ts, names): (&TransitionSystem, &TlaNames),
    whole_modules: &[String],
    open: &[usize],
) -> Result<()> {
    let wholes: Vec<Job> = open
        .iter()
        .filter(|&&i| {
            let listed = format!("{}.tla: ", whole_modules[i]);
            !index.lines().any(|l| l.starts_with(&listed))
        })
        .map(|&i| whole_job(ts, names, &whole_modules[i], i))
        .collect();
    for job in &wholes {
        write_spec(dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
    }
    if !wholes.is_empty() {
        fs::write(index_path, &index)
            .with_context(|| format!("failed to write {}", index_path.display()))?;
    }
    Ok(())
}

/// Under `--with-tlc`, runs the properties TLC's cones leave undecided on the whole model,
/// under the budget, and prints their lines. Returns their jobs and runs.
fn run_tlc_wholes(
    (tlc, dir): (&Tlc, &Path),
    (ts, names): (&TransitionSystem, &TlaNames),
    whole_modules: &[String],
    (budget, share): (Duration, u64),
    (jobs, ran): (&[Job], &[Ran]),
    index: (&mut String, &Path),
) -> Result<(Vec<Job>, Vec<Ran>)> {
    let open: Vec<usize> = jobs
        .iter()
        .zip(ran)
        .filter(|(_, r)| matches!(r.decision, Decision::Undecided { .. }))
        .filter_map(|(job, _)| match &job.kind {
            JobKind::PropertyCone(p) => Some(p.index),
            _ => None,
        })
        .collect();
    if open.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // A whole-model module `-o` already wrote for the native run is run as it is.
    write_whole_specs(dir, index, (ts, names), whole_modules, &open)?;
    let wholes: Vec<Job> = open
        .iter()
        .map(|&i| whole_job(ts, names, &whole_modules[i], i))
        .collect();
    let more = run_wholes(tlc, dir, &wholes, budget, share)?;
    for (job, r) in wholes.iter().zip(&more) {
        println!("tlc: {}", r.line(&job.module, &job.checks));
    }
    Ok((wholes, more))
}

/// Prints TLC's failed runs under `--with-tlc`: their counterexamples in model terms and TLC's
/// own summary, with its `violated:` lines.
fn tlc_failures(
    program: &ResolvedProgram,
    model: &SemanticModel,
    (ts, names): (&TransitionSystem, &TlaNames),
    dir: &Path,
    (jobs, ran): (&[&Job], &[&Ran]),
) {
    for (job, r) in jobs.iter().zip(ran) {
        if r.decision.outcome() == Outcome::Ok {
            continue;
        }
        let mut body = format!(
            "tlc: {} ({}):\n",
            job.module,
            dir.join(format!("{}.tla", job.module)).display()
        );
        if r.decision.outcome() == Outcome::Counterexample {
            for block in model_traces(program, model, (ts, names), &job.kind, &r.run) {
                body.push_str(&block);
                body.push('\n');
            }
        }
        body.push_str(&summarize(&r.run, &r.log));
        eprintln!("{body}");
    }
}

/// What BFS found at the verify bounds.
struct Bfs {
    /// Errors of the entity cones.
    errors: usize,
    /// Errors of the `can_reach` cones: violations, and cones the state cap stopped.
    reach_errors: usize,
    /// Findings an entity cone left undecided at the state cap.
    capped: usize,
}

/// BFS on each selected entity cone at the verify bounds: the `NoWitness` errors, the
/// `Deadlock`s (errors, or warnings at the bound), and the `NeverRuns` warnings, rendered;
/// then BFS on each `can_reach` cone, unreduced; then the `not checked:` block and one
/// summary line each. Every run is held to `caps`, the state cap and the memory limit.
fn state_models(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    caps: Caps,
) -> Bfs {
    let pick = |want: fn(&Target) -> bool| -> Vec<Target> {
        selection.targets.iter().copied().filter(want).collect()
    };
    let entities = pick(|t| matches!(t, Target::Entity(_)));
    // A scoped run checks no property, `can_reach` included.
    let reach = match scope.is_whole() {
        true => ts.reach_targets(),
        false => Vec::new(),
    };
    let mut run = ts.explore_targets_reduced(&entities, caps);
    // Its own call, unreduced: a cone shared with an entity target would share its run.
    let reach_run = (!reach.is_empty()).then(|| ts.explore_targets(&reach, caps));
    if !scope.bucs.is_empty() {
        run.defer_progress();
    }
    // A finding `--buc` leaves open is the scope's choice, not a bound's.
    let capped = run.undecided().iter().filter(|u| u.cone.is_some()).count();
    let not_checked = NotChecked {
        ts,
        model,
        scope,
        selection,
        undecided: run.undecided(),
        temporal: ts.properties().len() + ts.reach_properties().len(),
    };
    if !not_checked.is_empty() {
        println!("{not_checked}");
    }
    let report = |verdicts: &[Diag]| {
        let mut errors = 0;
        for d in verdicts {
            if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                let finding = Finding::new(&d.diag, &[], text);
                eprintln!("{finding}");
                errors += usize::from(finding.severity == Severity::Error);
            }
        }
        errors
    };
    let errors = report(run.verdicts());
    let reach_errors = reach_run.as_ref().map_or(0, |r| report(r.verdicts()));
    let t = run.tally();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    println!(
        "state models (BFS on {}): {}, {} witnessed, {}, {}",
        plural(t.entity_cones, "entity cone", "entity cones"),
        plural(t.state_models, "state model", "state models"),
        plural(t.witnessed, "state", "states"),
        plural(t.deadlocks, "deadlock", "deadlocks"),
        plural(
            t.never_run,
            "use case that never runs",
            "use cases that never run"
        ),
    );
    if let Some(r) = &reach_run {
        let cones = r.cones.len();
        println!(
            "can_reach (BFS on {cones} cone{}): {} checked, {} violated",
            if cones == 1 { "" } else { "s" },
            reach.len(),
            r.verdicts()
                .iter()
                .filter(|d| matches!(d.diag, StateDiag::GoalUnreachable { .. }))
                .count()
        );
    }
    Bfs {
        errors,
        reach_errors,
        capped,
    }
}

/// What BFS decided of the `stable`, `unchanged`, and `precedes` properties.
#[derive(Default)]
struct Explored {
    /// Properties decided this way.
    checked: usize,
    violated: usize,
    /// Properties whose cone BFS stopped at the state cap or the memory limit before a violation.
    capped: usize,
}

/// Caps for a whole-model run that starts now: `WHOLE_MODEL_STATE_CAP` states, and the time
/// budget from now.
fn whole_caps(caps: Caps, budget: Duration) -> Caps {
    let states = usize::try_from(WHOLE_MODEL_STATE_CAP).expect("the cap fits in usize");
    Caps { states, ..caps }.until(Instant::now() + budget)
}

/// Each property `LtlKind::is_explored` names, decided by BFS on its cone: a line per property,
/// and each violation rendered with the shortest trace to the step or state that breaks it.
/// A cone that frees slots is decided again on the whole model, under the budget and the
/// whole-model cap.
fn explored_properties(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    caps: Caps,
    budget: Duration,
) -> Explored {
    let mut out = Explored::default();
    for (i, p) in ts.properties().iter().enumerate() {
        if !p.formula.kind().is_explored() {
            continue;
        }
        out.checked += 1;
        let verdict = ts.decide_property(i, caps, whole_caps(caps, budget));
        // BFS decides this property, so no other run looks for its premises.
        if let (PropertyVerdict::Holds, Some(d)) = (&verdict, ts.vacuous_property(i, caps)) {
            if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                eprintln!("{}", Finding::new(&d.diag, &[], text));
            }
        }
        let said = match &verdict {
            PropertyVerdict::Holds => "ok",
            PropertyVerdict::Violated(_) => "violated",
            PropertyVerdict::Undecided => {
                "not decided: BFS stopped at the state cap or the memory limit"
            }
        };
        println!("property {} (BFS on its cone): {said}", p.name);
        match verdict {
            PropertyVerdict::Holds => {}
            PropertyVerdict::Undecided => out.capped += 1,
            PropertyVerdict::Violated(diag) => {
                let text = render_violation(program, model, &diag, TraceSource::Bfs)
                    .expect("a property violation renders");
                eprintln!("{}", Finding::new(&diag, &[], text));
                out.violated += 1;
            }
        }
    }
    out
}

/// A safety cone decided by BFS: the explorer on the module's own system, the reduced cone
/// TLC would check, at verify's bounds and caps.
enum BfsCone {
    Ran {
        states: usize,
        /// The violated invariants and after-asserts, each with BFS's shortest trace.
        found: Vec<Diag>,
        /// False when the run stopped at the state cap or the memory limit.
        complete: bool,
    },
    /// The cone at this index of the jobs covers this one (`covering`) and passed.
    Covered(usize),
}

impl BfsCone {
    /// The verdict of `run`, a run of `cone`'s system.
    fn of(ts: &TransitionSystem, cone: &ConeSystem, run: &ComponentRun) -> BfsCone {
        // The module checks every invariant and after-assert its system holds, as TLC does.
        let found = run
            .diags
            .iter()
            .filter(|d| {
                matches!(
                    d.diag,
                    StateDiag::ConstraintViolated { .. } | StateDiag::AfterAssertViolated { .. }
                )
            })
            .cloned()
            .map(|mut d| {
                if let Some(trace) = d.diag.trace_mut() {
                    ts.annotate(&cone.cone, trace);
                }
                d
            })
            .collect();
        BfsCone::Ran {
            states: run.states.len(),
            found,
            complete: run.complete,
        }
    }

    fn passed(&self) -> bool {
        self.verdict() == Some(Outcome::Ok)
    }

    /// The verdict, `None` when the run stopped before a violation.
    fn verdict(&self) -> Option<Outcome> {
        match self {
            BfsCone::Ran {
                found, complete, ..
            } => match (found.is_empty(), complete) {
                (false, _) => Some(Outcome::Counterexample),
                (true, true) => Some(Outcome::Ok),
                (true, false) => None,
            },
            BfsCone::Covered(_) => Some(Outcome::Ok),
        }
    }

    /// The run line, in the form of `Ran::line`.
    fn line(&self, module: &str, checks: &[String], jobs: &[Job]) -> String {
        let verdict = self.verdict().map_or("not decided", Outcome::as_str);
        match self {
            BfsCone::Ran {
                states, complete, ..
            } => {
                let notes = match complete {
                    true => Vec::new(),
                    false => vec!["stopped at the state cap or the memory limit".to_string()],
                };
                run_line(module, checks, verdict, Some(*states as u64), &notes)
            }
            BfsCone::Covered(by) => run_line(
                module,
                checks,
                verdict,
                None,
                &[format!("checked in {}", jobs[*by].module)],
            ),
        }
    }
}

/// The run met the memory limit, maybe only because the runs beside it held the rest.
fn crowded(run: &ComponentRun) -> bool {
    run.diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }))
}

/// Each safety cone of `jobs` decided by BFS, `None` for a property's job, and the vacuous
/// checks their runs show. As `run_cones` does for TLC, a cone another cone covers waits for
/// it: it is `ok` when that one passes and runs itself when it does not. Its vacuous checks
/// are read off the cone that covers it. A run that met the memory limit beside others runs
/// again alone, so that a crowded pool does not leave it undecided.
fn bfs_cones(ts: &TransitionSystem, jobs: &[Job], caps: Caps) -> (Vec<Option<BfsCone>>, Vec<Diag>) {
    let covers = covering(jobs);
    let cone = |at: usize| match &jobs[at].kind {
        JobKind::Cone(cone) => Some(cone.as_ref()),
        JobKind::PropertyCone(_) | JobKind::Whole(_) => None,
    };
    // The verdict of the cone at `at`, and the vacuous checks of it and of the cones it
    // covers, read off its run; the run is dropped here.
    let decide = |at: usize| {
        let me = cone(at).expect("a safety cone");
        let run = me.system.clone().run(caps);
        let vacuous: Vec<Diag> = std::iter::once(at)
            .chain((0..jobs.len()).filter(|a| covers[*a] == Some(at)))
            .filter_map(cone)
            .flat_map(|c| c.targets.iter())
            .filter_map(|t| ts.vacuity(*t, &run))
            .collect();
        (BfsCone::of(ts, me, &run), vacuous, crowded(&run))
    };
    let first: Vec<usize> = (0..jobs.len())
        .filter(|a| cone(*a).is_some() && covers[*a].is_none())
        .collect();
    let ran = in_parallel(first.clone(), rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<BfsCone>> = jobs.iter().map(|_| None).collect();
    let mut found = Vec::new();
    for (&at, (verdict, vacuous, crowded)) in first.iter().zip(ran) {
        let (verdict, vacuous, _) = match crowded {
            true => decide(at),
            false => (verdict, vacuous, crowded),
        };
        found.extend(vacuous);
        out[at] = Some(verdict);
    }
    for at in 0..jobs.len() {
        if let (Some(by), Some(me)) = (covers[at], cone(at)) {
            out[at] = Some(match out[by].as_ref().is_some_and(BfsCone::passed) {
                true => BfsCone::Covered(by),
                false => BfsCone::of(ts, me, &me.system.clone().run(caps)),
            });
        }
    }
    (out, found)
}

/// What BFS decided of the safety cones.
#[derive(Default)]
struct Safety {
    /// Distinct invariants and after-asserts violated.
    violated: usize,
    /// Cones the state cap or the memory limit stopped before a violation.
    capped: usize,
}

/// Prints each safety cone's run line and renders each violation with its BFS trace.
fn report_bfs_cones(
    program: &ResolvedProgram,
    model: &SemanticModel,
    jobs: &[Job],
    cones: &[Option<BfsCone>],
) -> Safety {
    let mut out = Safety::default();
    let mut reported = BTreeSet::new();
    for (job, cone) in jobs.iter().zip(cones) {
        let Some(cone) = cone else {
            continue;
        };
        println!("{}", cone.line(&job.module, &job.checks, jobs));
        let BfsCone::Ran {
            found, complete, ..
        } = cone
        else {
            continue;
        };
        // A check several cones hold is reported once, from the first cone that breaks it, so
        // the count does not depend on how the cones overlap.
        for d in found.iter().filter(|d| reported.insert(d.diag.name(&[]))) {
            let text = render_violation(program, model, &d.diag, TraceSource::Bfs)
                .expect("a violation renders");
            eprintln!("{}", Finding::new(&d.diag, &[], text));
        }
        out.capped += usize::from(found.is_empty() && !complete);
    }
    out.violated = reported.len();
    out
}

/// What the native checker decides of one `always`, `eventually`, or `leads_to`, on its cone
/// or on the whole model.
struct Native {
    decision: Decision,
    /// The states of the run the line reports: the plain cone's, or the whole model's.
    states: Option<u64>,
    notes: Vec<String>,
    /// Each cone run and what it said, for `--with-tlc` to compare.
    runs: Vec<(ConeRun, Outcome)>,
    /// The counterexample of a `counterexample` verdict.
    found: Option<Box<StateDiag>>,
    /// A counterexample that failed its own check, or a reduced cone that disagrees with its
    /// cone: a tool bug, never a finding about the model.
    fault: Option<String>,
    /// A cone run stopped at the state cap or the memory limit: a finding the cone leaves
    /// undecided, which no whole-model run takes up.
    cone_capped: bool,
    /// A run met the memory limit, maybe only because the runs beside it held the rest.
    crowded: bool,
}

impl Native {
    fn line(&self, module: &str, checks: &[String]) -> String {
        run_line(
            module,
            checks,
            self.decision.as_str(),
            self.states,
            &self.notes,
        )
    }

    /// The cone leaves the property to the whole model.
    fn goes_whole(&self) -> bool {
        matches!(self.decision, Decision::Undecided { .. }) && !self.cone_capped
    }
}

/// One run's answer on a graph.
enum Answer {
    Verdict(Outcome, Option<Box<StateDiag>>),
    /// The run stopped at a cap or the budget before it found a counterexample.
    Stopped,
    Fault(LassoFault),
}

impl Answer {
    /// `properties()[i]` of `run`'s system on its graph: an `always` by its states, an
    /// `eventually` or `leads_to` by its fair components under `fairness`.
    fn of(run: &ComponentRun, i: usize, fairness: Fairness) -> Answer {
        let verdict = match run.system.properties()[i].formula.kind().is_liveness() {
            true => run.decide_liveness(i, fairness),
            false => Ok(run.decide_property(i).expect("an always")),
        };
        match verdict {
            Ok(PropertyVerdict::Holds) => Answer::Verdict(Outcome::Ok, None),
            Ok(PropertyVerdict::Violated(d)) => Answer::Verdict(Outcome::Counterexample, Some(d)),
            Ok(PropertyVerdict::Undecided) => Answer::Stopped,
            Err(fault) => Answer::Fault(fault),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Answer::Verdict(outcome, _) => outcome.as_str(),
            Answer::Stopped => "not decided",
            Answer::Fault(_) => "tool_error",
        }
    }

    fn outcome(&self) -> Outcome {
        match self {
            Answer::Verdict(outcome, _) => *outcome,
            Answer::Stopped => Outcome::Ok,
            Answer::Fault(_) => Outcome::ToolError,
        }
    }
}

/// Explores `system` under `caps`, and marks `crowded` when the run met the memory limit.
fn explore(system: &TransitionSystem, caps: Caps, crowded_run: &mut bool) -> ComponentRun {
    let run = system.clone().run(caps);
    *crowded_run |= crowded(&run);
    run
}

/// `p` on its cone by the runs `property_cone` defines, each on a graph BFS explores: `Plain`
/// on the reduced cone, `Full` on the unreduced cone after a counterexample there, and
/// `WfNext` on the same unreduced graph when `after_plain` asks for it. Also returns the
/// vacuity the plain run shows.
fn native_property(ts: &TransitionSystem, p: &PropertyJob, caps: Caps) -> (Native, Option<Diag>) {
    let cone = &p.cone;
    let mut out = Native {
        decision: Decision::Decided(Outcome::Ok),
        states: None,
        notes: Vec::new(),
        runs: Vec::new(),
        found: None,
        fault: None,
        cone_capped: false,
        crowded: false,
    };
    let mut crowded_run = false;
    let plain_run = explore(&cone.reduced, caps, &mut crowded_run);
    let vacuous = ts.property_vacuity(p.index, &plain_run);
    out.states = Some(plain_run.states.len() as u64);
    let mut last = Answer::of(&plain_run, cone.reduced_index, Fairness::Plain);
    drop(plain_run);
    out.runs.push((ConeRun::Plain, last.outcome()));
    let mut unreduced: Option<ComponentRun> = None;
    // A counterexample on the reduced cone is real, but its lasso is read on the unreduced
    // cone, whose run gives the verdict from here on.
    if let (Answer::Verdict(Outcome::Counterexample, _), Some(full)) = (&last, &p.full) {
        let run = explore(&cone.system, caps, &mut crowded_run);
        let again = Answer::of(&run, cone.index, Fairness::Plain);
        unreduced = Some(run);
        out.runs.push((ConeRun::Full, again.outcome()));
        out.notes
            .push(format!("{}: {}", full.module, again.as_str()));
        last = match again {
            Answer::Verdict(Outcome::Ok, _) => {
                out.fault =
                    Some("the reduced cone finds a counterexample the cone does not".to_string());
                Answer::Verdict(Outcome::ToolError, None)
            }
            other => other,
        };
    }
    out.crowded = crowded_run;
    let (plain, found) = match last {
        Answer::Verdict(outcome, found) => (outcome, found),
        Answer::Stopped => return (out.capped(), vacuous),
        Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
    };
    out.found = found;
    let verdict = match cone.after_plain(plain) {
        AfterPlain::Decided(verdict) => verdict,
        AfterPlain::NeedsWfNext => {
            let wf =
                p.wf.as_ref()
                    .expect("an eventually or leads_to has a WF module");
            let run = match unreduced {
                Some(run) => run,
                None => explore(&cone.system, caps, &mut crowded_run),
            };
            out.crowded = crowded_run;
            let second = Answer::of(&run, cone.index, Fairness::WfNext);
            out.runs.push((ConeRun::WfNext, second.outcome()));
            out.notes
                .push(format!("{}: {}", wf.module, second.as_str()));
            match second {
                Answer::Verdict(outcome, found) => {
                    out.found = found.or(out.found.take());
                    cone.after_wf(plain, outcome)
                }
                Answer::Stopped => return (out.capped(), vacuous),
                Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
            }
        }
    };
    out.decision = match verdict {
        Verdict::Ok => Decision::Decided(Outcome::Ok),
        Verdict::Counterexample => Decision::Decided(Outcome::Counterexample),
        Verdict::ToolError => Decision::Decided(Outcome::ToolError),
        Verdict::Undecided(why) => {
            out.notes.push(why.as_str().to_string());
            Decision::Undecided {
                why: why.as_str().to_string(),
                stop: None,
            }
        }
    };
    if out.decision.verdict() != Some(Outcome::Counterexample) {
        out.found = None;
    }
    (out, vacuous)
}

impl Native {
    /// A cone run stopped at the state cap or the memory limit before it decided.
    fn capped(mut self) -> Native {
        let why = "stopped at the state cap or the memory limit".to_string();
        self.notes.push(why.clone());
        self.decision = Decision::Undecided { why, stop: None };
        self.cone_capped = true;
        self.found = None;
        self
    }

    /// A counterexample that failed its own check.
    fn faulty(mut self, fault: &LassoFault) -> Native {
        self.fault = Some(format!(
            "the counterexample fails its own check ({fault:?})"
        ));
        self.decision = Decision::Decided(Outcome::ToolError);
        self.found = None;
        self
    }
}

/// `native_property` for every property job, side by side, `None` for a safety cone. A
/// property whose runs met the memory limit beside others is decided again alone. Also
/// returns the vacuous properties.
fn native_properties(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
) -> (Vec<Option<Native>>, Vec<Diag>) {
    let at: Vec<usize> = (0..jobs.len())
        .filter(|&k| matches!(jobs[k].kind, JobKind::PropertyCone(_)))
        .collect();
    let decide = |k: usize| match &jobs[k].kind {
        JobKind::PropertyCone(p) => native_property(ts, p, caps),
        JobKind::Cone(_) | JobKind::Whole(_) => unreachable!("a property's job"),
    };
    let ran = in_parallel(at.clone(), rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<Native>> = jobs.iter().map(|_| None).collect();
    let mut vacuous = Vec::new();
    for (k, (native, vacuity)) in at.into_iter().zip(ran) {
        let (native, vacuity) = match native.crowded {
            true => decide(k),
            false => (native, vacuity),
        };
        vacuous.extend(vacuity);
        out[k] = Some(native);
    }
    (out, vacuous)
}

/// The properties `open` names, which their cones leave undecided, on the whole model: one
/// exploration under the budget and `WHOLE_MODEL_STATE_CAP`, and each property decided on
/// its graph with `WF_vars(Next)`. A run that stopped decides a property only by a
/// counterexample, which is real on the states it reached, as TLC's verdict printed before a
/// kill is.
fn native_wholes(
    ts: &TransitionSystem,
    open: &[usize],
    caps: Caps,
    budget: Duration,
) -> Vec<Native> {
    if open.is_empty() {
        return Vec::new();
    }
    let run = ts.clone().run(whole_caps(caps, budget));
    let states = run.states.len() as u64;
    let stop = run.diags.iter().find_map(|d| match &d.diag {
        StateDiag::StateCapReached { cap, .. } => Some((
            Stop::StateCap,
            format!("stopped at the state cap of {cap} states"),
        )),
        StateDiag::MemoryCapReached { limit, .. } => Some((
            Stop::Memory,
            format!("stopped at the memory limit of {limit}"),
        )),
        StateDiag::TimeBudgetReached { .. } => Some((
            Stop::Budget,
            format!("stopped at the time budget of {} s", budget.as_secs()),
        )),
        _ => None,
    });
    open.iter()
        .map(|&i| {
            let mut native = Native {
                decision: Decision::Decided(Outcome::Ok),
                states: (states > 0).then_some(states),
                notes: Vec::new(),
                runs: Vec::new(),
                found: None,
                fault: None,
                cone_capped: false,
                crowded: false,
            };
            match Answer::of(&run, i, Fairness::WfNext) {
                Answer::Verdict(outcome, found) => {
                    native.decision = Decision::Decided(outcome);
                    native.found = found;
                }
                Answer::Stopped => {
                    let (stop, why) = stop.clone().expect("a run that stopped says why");
                    native.notes.push(why.clone());
                    native.decision = Decision::Undecided {
                        why,
                        stop: Some(stop),
                    };
                }
                Answer::Fault(fault) => native = native.faulty(&fault),
            }
            native
        })
        .collect()
}

/// What the native checker decided: per job, a safety cone's verdict or a property's on its
/// cone, and per property the cone left open, its whole-model verdict.
struct Natives {
    safety: Vec<Option<BfsCone>>,
    properties: Vec<Option<Native>>,
    wholes: Vec<(usize, Native)>,
}

impl Natives {
    /// The whole-model verdict of property `i`, when its cone left it open.
    fn whole(&self, i: usize) -> Option<&Native> {
        self.wholes.iter().find(|(at, _)| *at == i).map(|(_, n)| n)
    }

    /// The properties the whole-model run leaves undecided, one line each.
    fn undecided_lines(&self, names: &TlaNames) -> Vec<String> {
        self.wholes
            .iter()
            .filter_map(|(i, native)| {
                let Decision::Undecided {
                    why,
                    stop: Some(stop),
                } = &native.decision
                else {
                    return None;
                };
                let states = native
                    .states
                    .map_or(String::new(), |n| format!(" after {n} states"));
                Some(format!(
                    "  {}: the whole-model run {why}{states}; {}",
                    names.property(*i),
                    undecided_hint(*stop)
                ))
            })
            .collect()
    }
}

/// What the native checker decided of the `always`, `eventually`, and `leads_to` properties.
#[derive(Default)]
struct Temporal {
    violated: usize,
    /// Counterexamples that failed their own check: a tool bug.
    faults: usize,
    /// Properties a cone run stopped at the state cap or the memory limit left undecided.
    capped: usize,
}

/// Prints each property's cone line and then its whole-model line, renders each
/// counterexample, and reports each fault.
fn report_properties(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    jobs: &[Job],
    natives: &Natives,
    whole_modules: &[String],
) -> Temporal {
    let mut out = Temporal::default();
    let mut report = |module: &str, checks: &[String], native: &Native| {
        println!("{}", native.line(module, checks));
        if let Some(diag) = &native.found {
            let text = render_violation(program, model, diag, TraceSource::Bfs)
                .expect("a property violation renders");
            eprintln!("{}", Finding::new(diag, &[], text));
            out.violated += 1;
        }
        if let Some(fault) = &native.fault {
            eprintln!("error[LassoFault]: {module} [{}]: {fault}; a tool bug, not a finding about the model", checks.join(", "));
            out.faults += 1;
        }
    };
    let mut capped = 0;
    for (job, native) in jobs.iter().zip(&natives.properties) {
        if let Some(native) = native {
            report(&job.module, &job.checks, native);
            capped += usize::from(native.cone_capped);
        }
    }
    for (i, native) in &natives.wholes {
        let checks = [ts.properties()[*i].name.clone()];
        report(&whole_modules[*i], &checks, native);
    }
    out.capped = capped;
    out
}

/// Under `--with-tlc`, each check the two engines decide differently, as an
/// `error[EngineDisagreement]` line: a safety cone's verdict and the checks its counterexample
/// breaks, and a property's verdict on each run both made and in the end. A check one engine
/// leaves undecided, or a TLC run that failed as a tool, is no disagreement.
fn disagreements(
    whole_modules: &[String],
    jobs: &[Job],
    natives: &Natives,
    (ran, whole_ran): (&[Ran], &[Ran]),
) -> Vec<String> {
    let line = |module: &str, checks: &[String], native: &str, tlc: &str| {
        format!(
            "error[EngineDisagreement]: {module} [{}]: native {native}, TLC {tlc}; one engine is wrong (a tool bug, not a finding about the model)",
            checks.join(", ")
        )
    };
    let differ = |a: Option<Outcome>, b: Option<Outcome>| match (a, b) {
        (Some(a), Some(b)) => a != b && a != Outcome::ToolError && b != Outcome::ToolError,
        _ => false,
    };
    let mut out = Vec::new();
    let mut tlc_wholes = whole_ran.iter();
    for (k, (job, tlc)) in jobs.iter().zip(ran).enumerate() {
        match &job.kind {
            JobKind::Cone(cs) => {
                let Some(native) = &natives.safety[k] else {
                    continue;
                };
                let (a, b) = (native.verdict(), tlc.decision.verdict());
                if differ(a, b) {
                    out.push(line(&job.module, &job.checks, as_str(a), as_str(b)));
                    continue;
                }
                if (a, b) != (Some(Outcome::Counterexample), Some(Outcome::Counterexample)) {
                    continue;
                }
                let BfsCone::Ran { found, .. } = native else {
                    continue;
                };
                let mine: BTreeSet<String> = found.iter().map(|d| d.diag.name(&[])).collect();
                let theirs: Option<BTreeSet<String>> = tlc
                    .run
                    .violations
                    .iter()
                    .map(|v| explain_violation(&cs.system, &cs.names, v).map(|d| d.name(&[])))
                    .collect::<Result<_, _>>()
                    .ok();
                if let Some(theirs) = theirs.filter(|t| *t != mine) {
                    let list =
                        |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(" ");
                    out.push(line(
                        &job.module,
                        &job.checks,
                        &format!("violates {}", list(&mine)),
                        &format!("violates {}", list(&theirs)),
                    ));
                }
            }
            JobKind::PropertyCone(p) => {
                let Some(native) = &natives.properties[k] else {
                    continue;
                };
                let module = |run: ConeRun| match run {
                    ConeRun::Plain => job.module.clone(),
                    ConeRun::Full => p
                        .full
                        .as_ref()
                        .map_or(job.module.clone(), |s| s.module.clone()),
                    ConeRun::WfNext => {
                        p.wf.as_ref()
                            .map_or(job.module.clone(), |s| s.module.clone())
                    }
                };
                for (run, mine) in &native.runs {
                    let theirs = tlc.runs.iter().find(|(r, _)| r == run).map(|(_, o)| *o);
                    if differ(Some(*mine), theirs) {
                        out.push(line(
                            &module(*run),
                            &job.checks,
                            mine.as_str(),
                            as_str(theirs),
                        ));
                    }
                }
                // The final verdict: the whole-model run's where a cone left the property open.
                let tlc_whole = match tlc.decision {
                    Decision::Undecided { .. } => tlc_wholes.next(),
                    Decision::Decided(_) => None,
                };
                let mine = natives.whole(p.index).unwrap_or(native);
                let theirs = tlc_whole.unwrap_or(tlc);
                let (a, b) = (mine.decision.verdict(), theirs.decision.verdict());
                if differ(a, b) {
                    let module = match (natives.whole(p.index), tlc_whole) {
                        (None, None) => &job.module,
                        _ => &whole_modules[p.index],
                    };
                    out.push(line(module, &job.checks, as_str(a), as_str(b)));
                }
            }
            JobKind::Whole(_) => {}
        }
    }
    out
}

fn as_str(outcome: Option<Outcome>) -> &'static str {
    outcome.map_or("not decided", Outcome::as_str)
}

/// What TLC adds to the end of a `--with-tlc` run.
#[derive(Default)]
struct TlcEnd {
    /// TLC runs that failed without a model violation.
    tool_errors: usize,
    /// Checks the two engines decide differently.
    disagreements: usize,
}

/// How a run ends.
struct End {
    bfs: Bfs,
    explored: Explored,
    /// What BFS decided of the safety cones.
    safety: Safety,
    /// What the native checker decided of `always`, `eventually`, and `leads_to`.
    temporal: Temporal,
    tlc: TlcEnd,
    deny_undecided: bool,
    /// What `--prove` proved, when it ran.
    proof: Option<ProofTally>,
}

impl End {
    /// The verdict the `result:` line states: of the invariants, the after-asserts, and the
    /// properties, every form and `can_reach` included. The state-model findings of the
    /// entity cones fail the run beside it and do not move it.
    fn outcome(&self) -> Outcome {
        if self.temporal.faults > 0 {
            return Outcome::ToolError;
        }
        let violated = self.safety.violated
            + self.explored.violated
            + self.temporal.violated
            + self.bfs.reach_errors;
        match violated {
            0 => Outcome::Ok,
            _ => Outcome::Counterexample,
        }
    }

    /// A violation, an error of BFS, or a tool error fails the run. A temporal property the
    /// budget left undecided is listed, as the BFS cap lists the findings it leaves; what the
    /// bounds left undecided is a warning, and an error under `--deny-undecided`.
    fn finish(&self, line: &BoundsLine, undecided: &[String]) -> Result<()> {
        if !undecided.is_empty() {
            println!(
                "not checked: {} not decided",
                match undecided.len() {
                    1 => "1 temporal property".to_string(),
                    n => format!("{n} temporal properties"),
                }
            );
            for u in undecided {
                println!("{u}");
            }
        }
        let failed: Vec<String> = [
            (
                self.bfs.errors,
                "state-model error(s) from BFS on the entity cones",
            ),
            (
                self.bfs.reach_errors,
                "can_reach error(s) from BFS on their cones",
            ),
            (
                self.safety.violated,
                "invariant or after-assert violation(s)",
            ),
            (
                self.explored.violated + self.temporal.violated,
                "property violation(s)",
            ),
            (
                self.temporal.faults,
                "counterexample(s) that fail their own check (a tool bug)",
            ),
            (
                self.tlc.disagreements,
                "check(s) the native checker and TLC decide differently (a tool bug)",
            ),
            (
                self.tlc.tool_errors,
                "TLC run(s) that failed without a model violation",
            ),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !failed.is_empty() {
            bail!("verification failed: {}", failed.join(" and "));
        }
        let open = [
            (
                self.bfs.capped + self.explored.capped + self.safety.capped + self.temporal.capped,
                "finding a cone stopped at the state cap or the memory limit left",
                "findings a cone stopped at the state cap or the memory limit left",
            ),
            (
                undecided.len(),
                "temporal property the budget left",
                "temporal properties the budget left",
            ),
        ]
        .iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect::<Vec<_>>();
        if !open.is_empty() {
            let what = format!("{} undecided", open.join(" and "));
            if self.deny_undecided {
                bail!("verification incomplete: {what} (--deny-undecided)");
            }
            eprintln!("warning: {what}; the OK line holds only for what was decided (--deny-undecided fails the run)");
        }
        match self.proof {
            Some(p) if p.unproved > 0 && p.deny_unproved => {
                bail!(
                    "verification incomplete: {} of {} rules unproved for any number of rows and any Int value (--deny-unproved)",
                    p.unproved,
                    p.rules
                );
            }
            Some(p) => println!("{}; {}", line.ok(), p.clause()),
            None => println!("{}", line.ok()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod worker_tests {
    use super::cone_workers;

    /// The runs at once never get fewer workers than the cores divide into, and a long run
    /// keeps a second worker once the short ones are done.
    #[test]
    fn cone_runs_share_the_cores_and_keep_two_workers_on_four_cores() {
        assert_eq!(cone_workers(1, 1), 1);
        assert_eq!(cone_workers(2, 2), 1);
        assert_eq!(cone_workers(4, 4), 2);
        assert_eq!(cone_workers(4, 1), 4);
        assert_eq!(cone_workers(8, 8), 2);
        assert_eq!(cone_workers(16, 8), 2);
        assert_eq!(cone_workers(16, 2), 8);
    }
}

#[cfg(test)]
mod hint_tests {
    use super::{undecided_hint, Stop};

    /// Raising the budget cannot lift the state cap, so that hint must not offer it.
    #[test]
    fn a_state_cap_hint_does_not_offer_the_budget() {
        let hint = undecided_hint(Stop::StateCap);
        assert!(hint.contains("does not lift"), "{hint}");
        assert!(hint.contains(".fair(UseCase, ...)"), "{hint}");
        assert!(!hint.starts_with("raise --temporal-budget"), "{hint}");
    }

    #[test]
    fn a_time_budget_hint_offers_the_budget_and_fair() {
        let hint = undecided_hint(Stop::Budget);
        assert!(hint.starts_with("raise --temporal-budget"), "{hint}");
        assert!(hint.contains(".fair(UseCase, ...)"), "{hint}");
    }

    #[test]
    fn a_memory_hint_offers_the_memory_limit_and_fair() {
        let hint = undecided_hint(Stop::Memory);
        assert!(hint.starts_with("raise --memory"), "{hint}");
        assert!(hint.contains(".fair(UseCase, ...)"), "{hint}");
    }
}
