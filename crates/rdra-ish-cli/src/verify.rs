//! `rdra-ish verify --backend tlc`: check every invariant and after-assert on its own cone,
//! and temporal properties on the whole system.
//!
//! A cone (`TransitionSystem::target_cone`) is closed: its restricted system reaches exactly
//! the projection of the whole system's states, so a safety verdict on the cone is the
//! verdict on the whole model. Temporal properties are not restricted: `WF_vars(Next)` on a
//! cone drops the actions outside it, which on the whole system could starve the cone's.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::{BoundsLine, Caps, Cone, Target, TransitionSystem, VarSet};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};

use crate::cli::VerifyBackend;
use crate::export::{checked_system, tla_bounds};
use crate::list_output::{render_violation, TraceSource};
use crate::tlc_env::{tlc_env, Heap};
use crate::tlc_output::{summarize, Outcome, TlcRun};
use crate::tlc_replay::explain_violation;

pub fn run_verify(
    program: &ResolvedProgram,
    model: &SemanticModel,
    backend: VerifyBackend,
    out: Option<PathBuf>,
    instance_count: u32,
    int_range_max: u32,
) -> Result<()> {
    match backend {
        VerifyBackend::Tlc => run_tlc(program, model, out, instance_count, int_range_max),
    }
}

/// One TLC run: a module, its cfg, and the checks it answers.
struct Job {
    module: String,
    tla: String,
    cfg: String,
    checks: Vec<String>,
    heap: Heap,
    /// What a cone run's counterexamples replay on; `None` for the temporal run.
    cone: Option<ConeSystem>,
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

/// Each distinct cone of an invariant or after-assert, in target order, named after the
/// first check that reads it. Cones over the same vars share one run: an action kept for an
/// after-assert writes nothing in the cone, so it adds edges but no states.
fn cone_jobs(ts: &TransitionSystem, names: &TlaNames) -> Vec<Job> {
    let mut at: BTreeMap<VarSet, usize> = BTreeMap::new();
    let mut order: Vec<(Cone, String)> = Vec::new();
    for target in ts.targets() {
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
                heap: Heap::Cone,
                cone: Some(ConeSystem {
                    cone,
                    system: sub,
                    names: bundle.names,
                }),
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
    let (key, value) = tlc_env(job.heap, &tmp);
    let mut command = Command::new(tlc);
    command
        .env(key, value)
        .args(["-workers", workers, "-metadir"])
        .arg(&meta);
    // `-continue` reports every violated check of a cone, each with its own trace. The
    // temporal run keeps TLC's default and stops at its first violation.
    if job.heap == Heap::Cone {
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

/// Runs the cone jobs in parallel, one TLC worker each.
fn run_all(tlc: &Path, dir: &Path, jobs: &[Job]) -> Result<Vec<Ran>> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Result<Ran>)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8)
        .min(jobs.len().max(1));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else { break };
                let ran = run_job(tlc, dir, job, "1");
                done.lock().expect("lock").push((i, ran));
            });
        }
    });
    let mut done = done.into_inner().expect("lock");
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

fn write_job(dir: &Path, job: &Job) -> Result<()> {
    for (ext, text) in [("tla", &job.tla), ("cfg", &job.cfg)] {
        let path = dir.join(format!("{}.{ext}", job.module));
        fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Each violated check of a cone run once, in model terms, from its first TLC trace. A trace
/// that does not replay is a disagreement between the engines and says so.
fn model_traces(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    cs: &ConeSystem,
    run: &TlcRun,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    run.violations
        .iter()
        .filter(|v| v.name.is_some() && seen.insert(&v.name))
        .map(|v| match explain_violation(&cs.system, &cs.names, v) {
            Ok(mut diag) => {
                if let Some(trace) = diag.trace_mut() {
                    trace.over_bound = ts.over_bound(&cs.cone, trace);
                }
                let text = render_violation(program, model, &diag, TraceSource::Tlc)
                    .expect("explain returns a violation");
                format!("error: {text}")
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
    instance_count: u32,
    int_range_max: u32,
) -> Result<()> {
    let bounds = tla_bounds(instance_count, int_range_max)?;
    let ts = checked_system(model, bounds)?;
    let (dir, whole_name, keep) = out_dir(out)?;
    let names = TlaNames::new(&ts);

    let mut jobs = cone_jobs(&ts, &names);
    let cones = jobs.len();
    let temporal = if ts.properties().is_empty() {
        None
    } else {
        let bundle = tla::print(&ts, PrintMode::Temporal, &whole_name);
        Some(Job {
            module: bundle.module_name,
            tla: bundle.tla,
            cfg: bundle.cfg,
            checks: (0..ts.properties().len())
                .map(|i| names.property(i).to_string())
                .collect(),
            heap: Heap::Temporal,
            cone: None,
        })
    };
    let line = BoundsLine::verify(bounds, Some(Caps::VERIFY));
    println!("{line}");
    println!(
        "cones: {cones} for {} invariant(s) and {} after-assert(s)",
        ts.invariants().len(),
        ts.after_asserts().len()
    );
    match &temporal {
        Some(job) => println!(
            "temporal properties: {}, checked on the whole model (fairness does not restrict to a cone)",
            job.checks.len()
        ),
        None => println!("temporal properties: none, no whole-model run"),
    }
    jobs.extend(temporal);
    let bfs_errors = state_models(program, model, &ts);

    let mut index = String::new();
    for job in &jobs {
        write_job(&dir, job)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
    }
    let index_path = dir.join("cones.txt");
    fs::write(&index_path, &index)
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    if keep {
        println!("wrote {} specs and {}", jobs.len(), index_path.display());
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
    let mut ran = run_all(&tlc, &dir, &jobs[..cones])?;
    if let Some(job) = jobs.get(cones) {
        ran.push(run_job(&tlc, &dir, job, "auto")?);
    }

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
            if let (Outcome::Counterexample, Some(cs)) = (r.outcome, &job.cone) {
                for block in model_traces(program, model, &ts, cs, &r.run) {
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

/// BFS on each entity cone at the verify bounds: the `NoWitness` errors, rendered. A state a
/// capped cone leaves undecided prints a note. The exploration is dropped before TLC starts.
fn state_models(program: &ResolvedProgram, model: &SemanticModel, ts: &TransitionSystem) -> usize {
    let entities: Vec<Target> = ts
        .targets()
        .into_iter()
        .filter(|t| matches!(t, Target::Entity(_)))
        .collect();
    let run = ts.explore_targets(&entities, Caps::VERIFY);
    for u in run.undecided() {
        println!("note: {u}");
    }
    let mut errors = 0;
    for d in run.verdicts() {
        if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
            eprintln!("error: {text}");
            errors += 1;
        }
    }
    errors
}

/// The end of a run whose TLC checks passed: a BFS error still fails it.
fn finish(bfs_errors: usize, line: &BoundsLine) -> Result<()> {
    if bfs_errors > 0 {
        bail!(
            "verification failed: {bfs_errors} state-model error(s) from BFS on the entity cones"
        );
    }
    println!("OK: no errors within {}", line.within());
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
