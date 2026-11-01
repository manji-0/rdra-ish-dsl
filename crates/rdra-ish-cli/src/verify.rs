//! `rdra-ish verify --backend tlc`: check every invariant, after-assert, and temporal property
//! on its own cone, and a temporal property on the whole system only when its cone does not
//! decide it.
//!
//! A cone (`TransitionSystem::target_cone`) is closed: its restricted system reaches exactly
//! the projection of the whole system's states, so a safety verdict on the cone is the
//! verdict on the whole model. A property's cone (`property_cone`) also keeps the actions its
//! `.fair` names whole, so their weak fairness restricts to it; the whole model's
//! `WF_vars(Next)` does not, which is what `property_cone::Verdict` decides. Each property
//! runs alone, because TLC does not say which property its lasso violates.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::{
    BoundsLine, Caps, Cone, LtlKind, NotChecked, Scope, Selection, Severity, StateDiag, Target,
    TransitionSystem, VarSet,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};

use crate::cli::VerifyBackend;
use crate::export::{checked_system, tla_bounds};
use crate::list_output::{render_violation, Finding, TraceSource};
use crate::property_cone::{ConeRun, PropertyCone, Verdict};
use crate::tlc_env::{tlc_env, Heap};
use crate::tlc_output::{progress, summarize, Outcome, TlcRun};
use crate::tlc_replay::{explain_property, explain_violation};

/// `bounds` is `(--instance-count, --int-range-max)`; `budget` is `--temporal-budget`, the
/// wall time a whole-model property run gets.
pub fn run_verify(
    program: &ResolvedProgram,
    model: &SemanticModel,
    backend: VerifyBackend,
    out: Option<PathBuf>,
    bounds: (u32, u32),
    scope: &Scope,
    budget: Duration,
) -> Result<()> {
    match backend {
        VerifyBackend::Tlc => run_tlc(program, model, out, bounds, scope, budget),
    }
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

/// A property on its cone; the `_WF` module beside it adds `WF_vars(Next)` for the second
/// run an `eventually` or `leads_to` may need.
struct PropertyJob {
    index: usize,
    cone: PropertyCone,
    names: TlaNames,
    wf: Option<Spec>,
}

impl Job {
    fn heap(&self) -> Heap {
        match self.kind {
            JobKind::Cone(_) | JobKind::PropertyCone(_) => Heap::Cone,
            JobKind::Whole(_) => Heap::Temporal,
        }
    }
}

/// A cone as TLC checks it: the whole system's cone, its restricted system, and the names
/// its module prints.
struct ConeSystem {
    cone: Cone,
    system: TransitionSystem,
    names: TlaNames,
}

struct Ran {
    outcome: Outcome,
    /// The run whose counterexample is explained: the last one of the job.
    run: TlcRun,
    log: String,
    /// What the run line says after the state count.
    notes: Vec<String>,
    /// The property is not decided, and why.
    undecided: Option<String>,
}

impl Ran {
    fn line(&self, module: &str, checks: &[String]) -> String {
        let verdict = match self.undecided {
            Some(_) => "not decided",
            None => self.outcome.as_str(),
        };
        let mut inside: Vec<String> = self
            .run
            .distinct_states
            .map(|n| format!("{n} states"))
            .into_iter()
            .collect();
        inside.extend(self.notes.iter().cloned());
        let inside = match inside.is_empty() {
            true => String::new(),
            false => format!(" ({})", inside.join(", ")),
        };
        format!("{module} [{}]: {verdict}{inside}", checks.join(", "))
    }
}

/// Where the specs go, and the whole-model module name. `-o X.tla` writes next to `X.tla`
/// and names the whole-model module `X`; any other `-o` is a directory.
fn out_dir(out: Option<PathBuf>) -> Result<(PathBuf, String, bool)> {
    let (dir, whole, keep) = match out {
        Some(path) if path.extension().is_some_and(|e| e == "tla") => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("RdraSpec")
                .to_string();
            let parent = match path.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                _ => PathBuf::from("."),
            };
            (parent, stem, true)
        }
        Some(path) => (path, "RdraSpec".to_string(), true),
        None => (
            std::env::temp_dir().join(format!("rdra-ish-tlc-{}", std::process::id())),
            "RdraSpec".to_string(),
            false,
        ),
    };
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    // TLC runs inside `dir`, so the metadir and java tmpdir paths must not be relative.
    let dir = dunce::canonicalize(&dir)
        .with_context(|| format!("failed to resolve {}", dir.display()))?;
    Ok((dir, whole, keep))
}

/// Each distinct cone of a selected invariant or after-assert, in target order, named after
/// the first check that reads it. Cones over the same vars share one run: an action kept for
/// an after-assert writes nothing in the cone, so it adds edges but no states.
fn cone_jobs(ts: &TransitionSystem, names: &TlaNames, targets: &[Target]) -> Vec<Job> {
    let mut at: BTreeMap<VarSet, usize> = BTreeMap::new();
    let mut order: Vec<(Cone, String)> = Vec::new();
    for &target in targets {
        let first = match target {
            Target::Entity(_) => continue,
            Target::Invariant(i) => names.invariant(i).to_string(),
            Target::AfterAssert(i) => names.after(i).to_string(),
        };
        let cone = ts.target_cone(target);
        match at.get(&cone.vars) {
            Some(&k) => order[k].0.keep.extend(cone.keep),
            None => {
                at.insert(cone.vars.clone(), order.len());
                order.push((cone, first));
            }
        }
    }
    order
        .into_iter()
        .map(|(cone, first)| {
            let sub = ts.restrict_to(&cone.vars, &cone.keep);
            let bundle = tla::print(&sub, PrintMode::Safety, &format!("Cone_{first}"));
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
                    cone,
                    system: sub,
                    names: bundle.names,
                })),
            }
        })
        .collect()
}

/// One run per temporal property on its cone, module `Cone_<property>`, with the `_WF`
/// module (the same cone with `WF_vars(Next)`) printed beside it for the second run.
fn property_jobs(ts: &TransitionSystem, names: &TlaNames) -> Vec<Job> {
    (0..ts.properties().len())
        .map(|i| {
            let name = names.property(i);
            let cone = PropertyCone::new(ts, i);
            let module = format!("Cone_{name}");
            let plain = cone.bundle(ConeRun::Plain, &module);
            let wf = (cone.kind != LtlKind::Always).then(|| {
                let b = cone.bundle(ConeRun::WfNext, &format!("{module}_WF"));
                Spec {
                    module: b.module_name,
                    tla: b.tla,
                    cfg: b.cfg,
                }
            });
            Job {
                module: plain.module_name,
                tla: plain.tla,
                cfg: plain.cfg,
                checks: vec![name.to_string()],
                kind: JobKind::PropertyCone(Box::new(PropertyJob {
                    index: i,
                    cone,
                    names: plain.names,
                    wf,
                })),
            }
        })
        .collect()
}

/// `properties()[i]` on the whole system, module `<whole>_<property>`: the same module text
/// as the whole-model module, fairness included, and a cfg that names the one property.
fn whole_job(ts: &TransitionSystem, names: &TlaNames, whole: &str, i: usize) -> Job {
    let name = names.property(i);
    let bundle = tla::print(ts, PrintMode::Property(i), &format!("{whole}_{name}"));
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
fn run_job(tlc: &Path, dir: &Path, job: &Job, workers: &str) -> Result<Ran> {
    let cfg = format!("{}.cfg", job.module);
    let safety = matches!(job.kind, JobKind::Cone(_));
    let mut ran = run_cfg(tlc, dir, &job.module, job.heap(), &cfg, workers, safety)?;
    match &job.kind {
        JobKind::Cone(cs) => {
            let mut reruns = 0;
            while ran.outcome == Outcome::Counterexample {
                let found = ran.run.violated_names();
                if found.len() >= job.checks.len() {
                    break;
                }
                reruns += 1;
                let cfg_name = format!("{}_rest{reruns}.cfg", job.module);
                let cfg = tla::cfg_without(&cs.system, &cs.names, PrintMode::Safety, &found);
                let path = dir.join(&cfg_name);
                fs::write(&path, cfg)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                let more = run_cfg(tlc, dir, &job.module, job.heap(), &cfg_name, workers, true)?;
                ran.log.push_str(&more.log);
                ran.run.violations.extend(more.run.violations);
                if ran.run.violated_names().len() == found.len() {
                    break;
                }
            }
        }
        JobKind::PropertyCone(p) => {
            let (cone, wf) = (&p.cone, &p.wf);
            let plain = ran.outcome;
            let mut verdict = cone.verdict(plain, None);
            if verdict == Verdict::NeedsWfNext {
                let wf = wf
                    .as_ref()
                    .expect("an eventually or leads_to has a WF module");
                let cfg = format!("{}.cfg", wf.module);
                let more = run_cfg(tlc, dir, &wf.module, job.heap(), &cfg, workers, false)?;
                verdict = cone.verdict(plain, Some(more.outcome));
                ran.notes
                    .push(format!("{}: {}", wf.module, more.outcome.as_str()));
                ran.log.push_str(&more.log);
                ran.run = more.run;
            }
            ran.outcome = match verdict {
                Verdict::Ok => Outcome::Ok,
                Verdict::Counterexample => Outcome::Counterexample,
                Verdict::ToolError => Outcome::ToolError,
                Verdict::Undecided(why) => {
                    ran.undecided = Some(why.as_str().to_string());
                    ran.notes.push(why.as_str().to_string());
                    Outcome::Ok
                }
                Verdict::NeedsWfNext => unreachable!("the WF run answers it"),
            };
        }
        JobKind::Whole(_) => {}
    }
    Ok(ran)
}

fn tlc_command(tlc: &Path, dir: &Path, module: &str, heap: Heap, workers: &str) -> Result<Command> {
    let meta = dir.join(format!("{module}.states"));
    // TLC unpacks its standard modules into java.io.tmpdir; parallel runs must not share it.
    let tmp = dir.join(format!("{module}.tmp"));
    fs::create_dir_all(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;
    let (key, value) = tlc_env(heap, &tmp);
    let mut command = Command::new(tlc);
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
    tlc: &Path,
    dir: &Path,
    module: &str,
    heap: Heap,
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
        outcome: run.outcome(output.status.success()),
        run,
        log,
        notes: Vec::new(),
        undecided: None,
    })
}

/// A whole-model property run under a budget: TLC's progress lines (about one a minute) print
/// as they come, and the run is killed and left undecided past `budget` or past
/// `Caps::VERIFY` distinct states.
fn run_whole(tlc: &Path, dir: &Path, job: &Job, workers: &str, budget: Duration) -> Result<Ran> {
    let tla = format!("{}.tla", job.module);
    let cfg = format!("{}.cfg", job.module);
    let mut child = tlc_command(tlc, dir, &job.module, job.heap(), workers)?
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
    let cap = Caps::VERIFY.states as u64;
    let mut log = String::new();
    let mut stopped: Option<String> = None;
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
    let outcome = match &stopped {
        Some(_) => Outcome::Ok,
        None => run.outcome(exit_ok),
    };
    Ok(Ran {
        outcome,
        run,
        log,
        notes: stopped.iter().cloned().collect(),
        undecided: stopped,
    })
}

/// Runs the cone jobs, up to 8 at once with one TLC worker each.
fn run_cones(tlc: &Path, dir: &Path, jobs: &[Job]) -> Result<Vec<Ran>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    run_some(jobs, cores.min(8), "1", |job, workers| {
        run_job(tlc, dir, job, workers)
    })
}

/// Runs the whole-model property jobs, up to 2 at once (each takes the temporal heap) sharing
/// the cores.
fn run_wholes(tlc: &Path, dir: &Path, jobs: &[Job], budget: Duration) -> Result<Vec<Ran>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let at_once = jobs.len().clamp(1, 2);
    let workers = (cores / at_once).max(1).to_string();
    run_some(jobs, at_once, &workers, |job, workers| {
        run_whole(tlc, dir, job, workers, budget)
    })
}

/// Runs `jobs` on `threads` threads, each TLC run with `workers` workers, in job order.
fn run_some(
    jobs: &[Job],
    threads: usize,
    workers: &str,
    run: impl Fn(&Job, &str) -> Result<Ran> + Sync,
) -> Result<Vec<Ran>> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Result<Ran>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.min(jobs.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else { break };
                let ran = run(job, workers);
                done.lock().expect("lock").push((i, ran));
            });
        }
    });
    let mut done = done.into_inner().expect("lock");
    done.sort_by_key(|(i, _)| *i);
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
                    trace.over_bound = ts.over_bound(&cs.cone, trace);
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
                let finding = Finding {
                    severity: Severity::Error,
                    name: diag.name(&[]),
                    message,
                };
                finding.to_string()
            }
            Err(why) => {
                format!("error: {why}; BFS and TLC disagree (a tool bug). Raw TLC trace follows.")
            }
        })
        .collect()
}

fn run_tlc(
    program: &ResolvedProgram,
    model: &SemanticModel,
    out: Option<PathBuf>,
    (instance_count, int_range_max): (u32, u32),
    scope: &Scope,
    budget: Duration,
) -> Result<()> {
    let bounds = tla_bounds(instance_count, int_range_max)?;
    let built = checked_system(model, bounds, scope)?;
    let (ts, selection) = (built.ts, built.selection);
    let (dir, whole_name, keep) = out_dir(out)?;
    let names = TlaNames::new(&ts);

    let mut jobs = cone_jobs(&ts, &names, &selection.targets);
    let cones = jobs.len();
    // A scope checks no property: `--buc` drops use cases fairness would read, and an error
    // `--entity` defers may sit in a property's cone.
    let temporal = !ts.properties().is_empty() && scope.is_whole();
    let line =
        BoundsLine::verify(bounds, Some(Caps::VERIFY)).with_scope(scope.line(model, &selection));
    println!("{line}");
    let checked = |pick: fn(&Target) -> bool| selection.targets.iter().filter(|t| pick(t)).count();
    println!(
        "cones: {cones} for {} invariant(s) and {} after-assert(s)",
        checked(|t| matches!(t, Target::Invariant(_))),
        checked(|t| matches!(t, Target::AfterAssert(_)))
    );
    match (temporal, ts.properties().len()) {
        (_, 0) => println!("temporal properties: none"),
        (true, n) => println!(
            "temporal properties: {n}, each checked on its cone, and on the whole model only when the cone does not decide it (budget {} s)",
            budget.as_secs()
        ),
        (false, n) => println!("temporal properties: {n}, not checked: a scoped run checks none"),
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
    // The whole-model module with every property, for a run by hand; verify runs a property
    // on it only when its cone leaves it undecided.
    let whole = temporal.then(|| tla::print(&ts, PrintMode::Temporal, &whole_name));
    if temporal {
        jobs.extend(property_jobs(&ts, &names));
    }
    let bfs_errors = state_models(program, model, &ts, scope, &selection);

    let mut index = String::new();
    for job in &jobs {
        write_spec(&dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
        if let JobKind::PropertyCone(p) = &job.kind {
            let Some(wf) = &p.wf else { continue };
            write_spec(&dir, &wf.module, &wf.tla, &wf.cfg)?;
            index.push_str(&format!("{}.tla: {}\n", wf.module, job.checks.join(" ")));
        }
    }
    if let Some(w) = &whole {
        write_spec(&dir, &w.module_name, &w.tla, &w.cfg)?;
    }
    let index_path = dir.join("cones.txt");
    fs::write(&index_path, &index)
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    if keep {
        println!(
            "wrote {} specs and {}",
            jobs.len() + usize::from(whole.is_some()),
            index_path.display()
        );
    }

    if jobs.is_empty() {
        if !keep {
            let _ = fs::remove_dir_all(&dir);
        }
        println!("nothing to check");
        println!("tlc_result: ok");
        return finish(bfs_errors, &line, &[]);
    }
    let tlc = find_tlc()?;
    let mut ran = run_cones(&tlc, &dir, &jobs)?;
    for (job, r) in jobs.iter().zip(&ran) {
        println!("{}", r.line(&job.module, &job.checks));
    }

    // The properties the cones leave undecided run on the whole model, under the budget.
    let wholes: Vec<Job> = jobs
        .iter()
        .zip(&ran)
        .filter(|(_, r)| r.undecided.is_some())
        .filter_map(|(job, _)| match &job.kind {
            JobKind::PropertyCone(p) => Some(whole_job(&ts, &names, &whole_name, p.index)),
            _ => None,
        })
        .collect();
    for job in &wholes {
        write_spec(&dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
    }
    if !wholes.is_empty() {
        fs::write(&index_path, &index)
            .with_context(|| format!("failed to write {}", index_path.display()))?;
        for r in &mut ran {
            r.undecided = None;
        }
        let more = run_wholes(&tlc, &dir, &wholes, budget)?;
        for (job, r) in wholes.iter().zip(&more) {
            println!("{}", r.line(&job.module, &job.checks));
        }
        jobs.extend(wholes);
        ran.extend(more);
    }
    let undecided: Vec<String> = jobs
        .iter()
        .zip(&ran)
        .filter_map(|(job, r)| {
            let why = r.undecided.as_ref()?;
            let states = r
                .run
                .distinct_states
                .map_or(String::new(), |n| format!(" after {n} states"));
            Some(format!(
                "  {}: the whole-model run {why}{states}; raise --temporal-budget or shrink the model",
                job.checks.join(", ")
            ))
        })
        .collect();

    let outcome = if ran.iter().any(|r| r.outcome == Outcome::Counterexample) {
        Outcome::Counterexample
    } else if ran.iter().any(|r| r.outcome == Outcome::ToolError) {
        Outcome::ToolError
    } else {
        Outcome::Ok
    };
    if outcome == Outcome::Ok {
        if !keep {
            let _ = fs::remove_dir_all(&dir);
        }
        println!("tlc_result: ok");
        return finish(bfs_errors, &line, &undecided);
    }
    eprintln!("tlc_result: {}", outcome.as_str());
    let failed: Vec<String> = jobs
        .iter()
        .zip(&ran)
        .filter(|(_, r)| r.outcome == outcome)
        .map(|(job, r)| {
            let mut body = format!(
                "{} ({}):\n",
                job.module,
                dir.join(format!("{}.tla", job.module)).display()
            );
            if r.outcome == Outcome::Counterexample {
                for block in model_traces(program, model, (&ts, &names), &job.kind, &r.run) {
                    body.push_str(&block);
                    body.push('\n');
                }
            }
            body.push_str(&summarize(&r.run, &r.log));
            body
        })
        .collect();
    let specs = if keep {
        format!("specs: {}", dir.display())
    } else {
        format!("specs: {} (kept because a check failed)", dir.display())
    };
    match outcome {
        Outcome::Counterexample => bail!("TLC verification failed\n{}\n{specs}", failed.join("\n")),
        _ => bail!(
            "TLC tool failed without a model violation\n{}\n{specs}",
            failed.join("\n")
        ),
    }
}

/// BFS on each selected entity cone at the verify bounds: the `NoWitness` errors, the
/// `Deadlock`s (errors, or warnings at the bound), and the `NeverRuns` warnings, rendered,
/// then the `not checked:` block and one summary line. The exploration is dropped before TLC
/// starts.
fn state_models(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
) -> usize {
    let entities: Vec<Target> = selection
        .targets
        .iter()
        .copied()
        .filter(|t| matches!(t, Target::Entity(_)))
        .collect();
    let mut run = ts.explore_targets(&entities, Caps::VERIFY);
    if !scope.bucs.is_empty() {
        run.defer_progress();
    }
    let not_checked = NotChecked {
        ts,
        model,
        scope,
        selection,
        undecided: run.undecided(),
        temporal: ts.properties().len(),
    };
    if !not_checked.is_empty() {
        println!("{not_checked}");
    }
    let mut errors = 0;
    for d in run.verdicts() {
        if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
            let finding = Finding::new(&d.diag, &[], text);
            eprintln!("{finding}");
            errors += usize::from(finding.severity == Severity::Error);
        }
    }
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
    errors
}

/// The end of a run whose TLC checks passed: a BFS error still fails it. A temporal property
/// the budget left undecided is listed, as the BFS cap lists the findings it leaves.
fn finish(bfs_errors: usize, line: &BoundsLine, undecided: &[String]) -> Result<()> {
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
    if bfs_errors > 0 {
        bail!(
            "verification failed: {bfs_errors} state-model error(s) from BFS on the entity cones"
        );
    }
    println!("{}", line.ok());
    Ok(())
}

fn find_tlc() -> Result<PathBuf> {
    for candidate in ["tlc", "tlc2"] {
        if let Ok(path) = which(candidate) {
            return Ok(path);
        }
    }
    bail!(
        "TLC not found on PATH (tried `tlc`, `tlc2`)\n\
         Install the TLA+ tools and ensure `tlc` is available, or use \
         `rdra-ish export --kind tla` to generate the spec without running TLC."
    );
}

fn which(cmd: &str) -> Result<PathBuf> {
    let output = Command::new("which")
        .arg(cmd)
        .output()
        .with_context(|| format!("failed to resolve `{cmd}` on PATH"))?;
    if !output.status.success() {
        bail!("`{cmd}` not found");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("`{cmd}` not found");
    }
    Ok(PathBuf::from(path))
}
