//! `rdra-ish verify --backend tlc`: check every invariant and after-assert on its own cone,
//! and each temporal property on the whole system.
//!
//! A cone (`TransitionSystem::target_cone`) is closed: its restricted system reaches exactly
//! the projection of the whole system's states, so a safety verdict on the cone is the
//! verdict on the whole model. Temporal properties are not restricted: `WF_vars(Next)` on a
//! cone drops the actions outside it, which on the whole system could starve the cone's.
//! Each property runs alone, because TLC does not say which property its lasso violates.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::{
    BoundsLine, Caps, Cone, NotChecked, Scope, Selection, Severity, StateDiag, Target,
    TransitionSystem, VarSet,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};

use crate::cli::VerifyBackend;
use crate::export::{checked_system, tla_bounds};
use crate::list_output::{render_violation, Finding, TraceSource};
use crate::tlc_env::{tlc_env, Heap};
use crate::tlc_output::{summarize, Outcome, TlcRun};
use crate::tlc_replay::{explain_property, explain_violation};

/// `bounds` is `(--instance-count, --int-range-max)`.
pub fn run_verify(
    program: &ResolvedProgram,
    model: &SemanticModel,
    backend: VerifyBackend,
    out: Option<PathBuf>,
    bounds: (u32, u32),
    scope: &Scope,
) -> Result<()> {
    match backend {
        VerifyBackend::Tlc => run_tlc(program, model, out, bounds, scope),
    }
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
    /// `properties()[i]` on the whole system.
    Property(usize),
}

impl Job {
    fn heap(&self) -> Heap {
        match self.kind {
            JobKind::Cone(_) => Heap::Cone,
            JobKind::Property(_) => Heap::Temporal,
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
    run: TlcRun,
    log: String,
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
    let dir =
        fs::canonicalize(&dir).with_context(|| format!("failed to resolve {}", dir.display()))?;
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

/// One whole-system run per temporal property, module `<whole>_<property>`. Each prints the
/// same module text as the whole-model module, fairness included, and a cfg that names one
/// property.
fn property_jobs(ts: &TransitionSystem, names: &TlaNames, whole: &str) -> Vec<Job> {
    (0..ts.properties().len())
        .map(|i| {
            let name = names.property(i);
            let bundle = tla::print(ts, PrintMode::Property(i), &format!("{whole}_{name}"));
            Job {
                module: bundle.module_name,
                tla: bundle.tla,
                cfg: bundle.cfg,
                checks: vec![name.to_string()],
                kind: JobKind::Property(i),
            }
        })
        .collect()
}

fn run_job(tlc: &Path, dir: &Path, job: &Job, workers: &str) -> Result<Ran> {
    let tla = format!("{}.tla", job.module);
    let cfg = format!("{}.cfg", job.module);
    let meta = dir.join(format!("{}.states", job.module));
    // TLC unpacks its standard modules into java.io.tmpdir; parallel runs must not share it.
    let tmp = dir.join(format!("{}.tmp", job.module));
    fs::create_dir_all(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;
    let (key, value) = tlc_env(job.heap(), &tmp);
    let mut command = Command::new(tlc);
    command
        .env(key, value)
        .args(["-workers", workers, "-metadir"])
        .arg(&meta);
    // `-continue` reports every violated check of a cone, each with its own trace. A property
    // run keeps TLC's default and stops at its first violation.
    if job.heap() == Heap::Cone {
        command.arg("-continue");
    }
    let output = command
        .args(["-config", &cfg, &tla])
        .current_dir(dir)
        .output()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;
    let _ = fs::remove_dir_all(&meta);
    let _ = fs::remove_dir_all(&tmp);
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
    })
}

/// Runs the cone jobs, up to 8 at once with one TLC worker each, then the property jobs, up
/// to 2 at once (each takes the temporal heap) sharing the cores.
fn run_all(tlc: &Path, dir: &Path, jobs: &[Job]) -> Result<Vec<Ran>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let split = jobs
        .iter()
        .position(|j| matches!(j.kind, JobKind::Property(_)))
        .unwrap_or(jobs.len());
    let (cones, properties) = jobs.split_at(split);
    let mut ran = run_some(tlc, dir, cones, cores.min(8), "1")?;
    let at_once = properties.len().clamp(1, 2);
    let workers = (cores / at_once).max(1).to_string();
    ran.extend(run_some(tlc, dir, properties, at_once, &workers)?);
    Ok(ran)
}

/// Runs `jobs` on `threads` threads, each TLC run with `workers` workers, in job order.
fn run_some(
    tlc: &Path,
    dir: &Path,
    jobs: &[Job],
    threads: usize,
    workers: &str,
) -> Result<Vec<Ran>> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Result<Ran>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.min(jobs.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else { break };
                let ran = run_job(tlc, dir, job, workers);
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
        JobKind::Property(i) => run
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
) -> Result<()> {
    let bounds = tla_bounds(instance_count, int_range_max)?;
    let built = checked_system(model, bounds, scope)?;
    let (ts, selection) = (built.ts, built.selection);
    let (dir, whole_name, keep) = out_dir(out)?;
    let names = TlaNames::new(&ts);

    let mut jobs = cone_jobs(&ts, &names, &selection.targets);
    let cones = jobs.len();
    // A scope checks no whole model: `--buc` drops use cases fairness would read, and an
    // error `--entity` defers may sit in it.
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
        (_, 0) => println!("temporal properties: none, no whole-model run"),
        (true, n) => println!(
            "temporal properties: {n}, each checked in its own run on the whole model (fairness does not restrict to a cone)"
        ),
        (false, n) => println!(
            "temporal properties: {n}, not checked: a scoped run has no whole-model run"
        ),
    }
    // The whole-model module with every property, for a run by hand; verify runs each
    // property in its own copy of it.
    let whole = temporal.then(|| tla::print(&ts, PrintMode::Temporal, &whole_name));
    if temporal {
        jobs.extend(property_jobs(&ts, &names, &whole_name));
    }
    let bfs_errors = state_models(program, model, &ts, scope, &selection);

    let mut index = String::new();
    for job in &jobs {
        write_spec(&dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
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
        return finish(bfs_errors, &line);
    }
    let tlc = find_tlc()?;
    let ran = run_all(&tlc, &dir, &jobs)?;

    for (job, r) in jobs.iter().zip(&ran) {
        let states = r
            .run
            .distinct_states
            .map_or(String::new(), |n| format!(" ({n} states)"));
        println!(
            "{} [{}]: {}{states}",
            job.module,
            job.checks.join(", "),
            r.outcome.as_str()
        );
    }
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
        return finish(bfs_errors, &line);
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

/// The end of a run whose TLC checks passed: a BFS error still fails it.
fn finish(bfs_errors: usize, line: &BoundsLine) -> Result<()> {
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
