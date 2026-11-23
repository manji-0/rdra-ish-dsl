use super::jobs::{whole_job, ConeSystem, Job, JobKind};
use super::specs::write_whole_specs;
use super::tlc_tool::Tlc;
use super::{run_line, WHOLE_MODEL_STATE_CAP};
use crate::export::Modules;
use crate::memory::tlc_plan;
use crate::property_cone::{AfterPlain, ConeRun, Verdict};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_env::{current_user_heap, tlc_env_sized, Heap};
use crate::tlc_output::{progress, summarize, Outcome, TlcRun};
use crate::tlc_replay::{explain_property, explain_violation};
use anyhow::{Context, Result};
use rdra_ish_core::lifecycle::{StateDiag, TransitionSystem};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

/// What a run decides about its checks.
pub(super) enum Decision {
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
    pub(super) fn outcome(&self) -> Outcome {
        match self {
            Decision::Decided(outcome) => *outcome,
            Decision::Undecided { .. } => Outcome::Ok,
        }
    }

    /// The verdict, when the run gave one.
    pub(super) fn verdict(&self) -> Option<Outcome> {
        match self {
            Decision::Decided(outcome) => Some(*outcome),
            Decision::Undecided { .. } => None,
        }
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Decision::Decided(outcome) => outcome.as_str(),
            Decision::Undecided { .. } => "not decided",
        }
    }
}

/// Why a whole-model run stopped before it decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    Budget,
    StateCap,
    Memory,
}

/// One TLC run of a job, under `--with-tlc`.
pub(super) struct Ran {
    pub(super) decision: Decision,
    /// The run whose counterexample is explained: the last one of the job.
    pub(super) run: TlcRun,
    log: String,
    /// What the run line says after the state count.
    pub(super) notes: Vec<String>,
    /// A property cone's runs and what each said, compared with the native runs.
    pub(super) runs: Vec<(ConeRun, Outcome)>,
}

impl Ran {
    pub(super) fn line(&self, module: &str, checks: &[String]) -> String {
        let states = self.run.distinct_states;
        run_line(module, checks, self.decision.as_str(), states, &self.notes)
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
pub(super) fn undecided_hint(stop: Stop) -> &'static str {
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
pub(super) fn run_cones(
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
pub(super) fn covering(jobs: &[Job]) -> Vec<Option<usize>> {
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
pub(super) struct Covered<'a> {
    pub(super) checks: BTreeSet<&'a str>,
    pub(super) cone: &'a ConeSystem,
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

/// Under `--with-tlc`, runs the properties TLC's cones leave undecided on the whole model,
/// under the budget, and prints their lines. Returns their jobs and runs.
pub(super) fn run_tlc_wholes(
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
pub(super) fn tlc_failures(
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
