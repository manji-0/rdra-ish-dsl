//! Under `--with-tlc`, TLC checks the state models of each entity cone that BFS explored to
//! the end, up to `STATE_MODEL_TLC_STATES` states: the `StateModel_<E>` module `export --kind
//! tla` writes, on the system BFS explored, with its deadlocks cfg and its witnesses cfg
//! (docs/formal-verification.md, "State models with TLC alone").
//!
//! The two checks do not read alike. A `Witness_` TLC reports violated is a state some
//! instance reaches, so a `Witness_` it does not report is a state with no witness, as BFS
//! finds it: the two must name the same ones. A `Deadlock_` TLC reports is a deadlock BFS
//! must find, but a TLC pass proves none away, since a cycle anywhere in the column's cone
//! keeps a move enabled: a deadlock only BFS finds is no disagreement.

use super::bfs::{StateModelCone, StateModels};
use super::schedule::{run_budgeted, Budget};
use super::tlc::{clean_up, tlc_command};
use super::tlc_tool::Tlc;
use crate::export::Modules;
use crate::memory::{tlc_workers, TlcHeap};
use crate::tlc_env::{current_user_heap, Heap};
use crate::tlc_output::{out_of_memory, InvariantScan, Outcome};
use anyhow::{Context, Result};
use rdra_ish_core::lifecycle::TransitionSystem;
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;

/// Up to this many states a witnesses cfg runs with `-continue`, which prints a trace for each
/// state that violates a `Witness_`, so almost every state; above it each run stops at the
/// first one, and the reruns find one witness each. `engine_equivalence` splits the same way.
const CONTINUE_STATES: u64 = 1_000;

/// One cone's `StateModel_` module, printed and written into the spec dir.
pub(super) struct StateModelJob {
    pub(super) module: String,
    witnesses_cfg: String,
    system: TransitionSystem,
    names: TlaNames,
    pub(super) deadlocks: Vec<String>,
    pub(super) witnesses: Vec<String>,
    /// BFS's state count.
    states: usize,
    /// The bytes of the module: TLC's heap follows it (`TlcHeap::of`).
    module_bytes: u64,
}

/// What TLC found on one `StateModel_` module.
pub(super) struct StateModelRan {
    /// TLC's distinct states, from the first deadlocks run.
    pub(super) states: Option<u64>,
    /// The `Deadlock_` invariants TLC reports violated: each a deadlock.
    pub(super) deadlocks: BTreeSet<String>,
    /// The `Witness_` invariants TLC reports violated: each a state some instance reaches.
    pub(super) witnessed: BTreeSet<String>,
    /// A run that failed as a tool: why, and the tail of its output.
    pub(super) error: Option<String>,
}

impl StateModelRan {
    /// A job that failed as a tool before any run: why.
    fn failed(why: String) -> Self {
        StateModelRan {
            states: None,
            deadlocks: BTreeSet::new(),
            witnessed: BTreeSet::new(),
            error: Some(why),
        }
    }

    /// Whether a run of the job stopped because the JVM ran out of memory.
    fn out_of_memory(&self) -> bool {
        self.error.as_deref().is_some_and(out_of_memory)
    }

    /// The deadlocks and the states without a witness, in cfg order.
    pub(super) fn found(&self, job: &StateModelJob) -> Vec<String> {
        let unwitnessed = job
            .witnesses
            .iter()
            .filter(|w| !self.witnessed.contains(*w));
        job.deadlocks
            .iter()
            .filter(|d| self.deadlocks.contains(*d))
            .chain(unwitnessed)
            .cloned()
            .collect()
    }

    /// What the run adds to `tlc_result:`: only a run that failed as a tool. Its findings are
    /// BFS's, which the run checks; `tlc_result:` is the verdict of the runs that print
    /// TLC's own `violated:` lines, and a witnesses run violates every state it reaches.
    pub(super) fn outcome(&self) -> Outcome {
        match self.error {
            Some(_) => Outcome::ToolError,
            None => Outcome::Ok,
        }
    }

    pub(super) fn line(&self, job: &StateModelJob) -> String {
        // TLC counts the states in its deadlocks run; a module with no deadlock check
        // reports BFS's count.
        let states = match self.states {
            Some(n) => format!(" ({n} states)"),
            None => format!(" ({} states by BFS)", job.states),
        };
        let checks = format!(
            "{} deadlock and {} witness checks",
            job.deadlocks.len(),
            job.witnesses.len()
        );
        let found = self.found(job);
        let said = match (&self.error, found.is_empty()) {
            (Some(why), _) => format!("tool_error ({})", why.lines().next().unwrap_or("")),
            (None, true) => "found none".to_string(),
            (None, false) => format!("found {}", found.join(", ")),
        };
        format!("{}{states}: {checks}; {said}", job.module)
    }
}

/// Prints each cone's module and cfgs into `dir`, names them as `export` does, and lists them
/// in the index.
pub(super) fn state_model_jobs(
    dir: &Path,
    modules: &mut Modules,
    models: &StateModels,
    (index, index_path): (&mut String, &Path),
) -> Result<Vec<StateModelJob>> {
    let mut out = Vec::new();
    for cone in &models.cones {
        let StateModelCone {
            system,
            entities,
            first,
            states,
        } = cone;
        let module = modules.claim(first);
        let witnesses_cfg = modules.claim(&format!("{module}_witnesses"));
        let bundle = tla::print_state_models(system, entities, &module);
        let witness_cfg = bundle
            .witness_cfg
            .as_deref()
            .expect("a state-model module has a witness cfg");
        for (file, text) in [
            (format!("{module}.tla"), bundle.tla.as_str()),
            (format!("{module}.cfg"), bundle.cfg.as_str()),
            (format!("{witnesses_cfg}.cfg"), witness_cfg),
        ] {
            let path = dir.join(file);
            fs::write(&path, text)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
        let deadlocks: Vec<String> = bundle
            .names
            .states()
            .iter()
            .filter_map(|s| s.deadlock.clone())
            .collect();
        let witnesses: Vec<String> = bundle
            .names
            .states()
            .iter()
            .map(|s| s.witness.clone())
            .collect();
        index.push_str(&format!(
            "{module}.tla: {}\n",
            deadlocks
                .iter()
                .chain(&witnesses)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ));
        out.push(StateModelJob {
            module,
            witnesses_cfg,
            system: system.clone(),
            names: bundle.names,
            deadlocks,
            witnesses,
            states: *states,
            module_bytes: bundle.tla.len() as u64,
        });
    }
    if !out.is_empty() {
        fs::write(index_path, &index)
            .with_context(|| format!("failed to write {}", index_path.display()))?;
    }
    Ok(out)
}

/// Runs every job, the largest heap first, then the most states, as many at once as the cores
/// allow and their heaps fit `share` (`Budget`), each with the heap its module needs
/// (`TlcHeap::of`) and once more at twice the heap when it runs out of memory and the share
/// holds that. A job whose module needs more than the share has no run. The results come back
/// in job order.
pub(super) fn run_state_models(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    jobs: &[StateModelJob],
    share: u64,
) -> Result<Vec<StateModelRan>> {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let set = current_user_heap();
    let heaps: Vec<TlcHeap> = jobs
        .iter()
        .map(|job| TlcHeap::of(job.module_bytes, Heap::Cone, share, set))
        .collect();
    let mut order: Vec<usize> = (0..jobs.len()).collect();
    order.sort_by_key(|k| std::cmp::Reverse((heaps[*k].charge(), jobs[*k].states)));
    let budget = Budget::new(share);
    let done = run_budgeted(
        &order,
        cores,
        &budget,
        |k| heaps[k].charge(),
        |k, hold, in_flight| {
            let (job, heap) = (&jobs[k], heaps[k]);
            // As in `tlc::run_some`: the runs that fit the share divide the cores.
            let fit = usize::try_from(share / heap.charge().max(1)).unwrap_or(usize::MAX);
            let workers = tlc_workers(cores, in_flight.min(fit.max(1))).to_string();
            let bytes = match heap {
                TlcHeap::OverShare { need } => {
                    let (_, report) =
                        TlcHeap::over_share(&job.module, job.module_bytes, need, share);
                    return Ok(StateModelRan::failed(report));
                }
                TlcHeap::Sized(bytes) | TlcHeap::User(bytes) => bytes,
            };
            let ran = run_one(tlc, dir, modules, job, (&workers, bytes))?;
            let (true, Some(more)) = (ran.out_of_memory(), heap.doubled(share)) else {
                return Ok(ran);
            };
            hold.regrow(more.charge());
            let doubled = more.bytes().expect("a doubled heap is sized");
            run_one(tlc, dir, modules, job, (&workers, doubled))
        },
    );
    done.into_iter().map(|(_, r)| r).collect()
}

/// The deadlocks cfg with `-continue`, then the witnesses cfg, each rerun without the
/// invariants it reported until a rerun reports no new one: with `-continue` TLC reports one
/// invariant per state, the first in cfg order, and without it one per run.
fn run_one(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    job: &StateModelJob,
    jvm: (&str, u64),
) -> Result<StateModelRan> {
    let mut ran = StateModelRan {
        states: None,
        deadlocks: BTreeSet::new(),
        witnessed: BTreeSet::new(),
        error: None,
    };
    let deadlocks = rerun(
        (tlc, dir, modules, job, jvm),
        (&job.module, PrintMode::Deadlocks, &job.deadlocks),
        true,
    )?;
    ran.states = deadlocks.states;
    ran.deadlocks = deadlocks.violated;
    if let Some(why) = deadlocks.error {
        ran.error = Some(why);
        return Ok(ran);
    }
    // A module with no `Deadlock_` check has no deadlocks run to count its states; TLC
    // runs the system BFS explored, so BFS's count stands in.
    let states = ran.states.unwrap_or(job.states as u64);
    let continued = states <= CONTINUE_STATES;
    let witnesses = rerun(
        (tlc, dir, modules, job, jvm),
        (&job.witnesses_cfg, PrintMode::Witnesses, &job.witnesses),
        continued,
    )?;
    ran.witnessed = witnesses.violated;
    ran.error = witnesses.error;
    Ok(ran)
}

/// What the runs of one cfg found.
struct Reruns {
    violated: BTreeSet<String>,
    /// The first run's distinct states, when it explored every state.
    states: Option<u64>,
    error: Option<String>,
}

fn rerun(
    (tlc, dir, modules, job, jvm): (&Tlc, &Path, &Modules, &StateModelJob, (&str, u64)),
    (cfg_stem, mode, checks): (&str, PrintMode, &[String]),
    continued: bool,
) -> Result<Reruns> {
    let mut out = Reruns {
        violated: BTreeSet::new(),
        states: None,
        error: None,
    };
    if checks.is_empty() {
        return Ok(out);
    }
    for n in 0.. {
        let cfg = match n {
            0 => format!("{cfg_stem}.cfg"),
            n => {
                let name = modules.rest_cfg(cfg_stem, n);
                let text = tla::cfg_without(&job.system, &job.names, mode, &out.violated);
                let path = dir.join(&name);
                fs::write(&path, text)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                name
            }
        };
        let (scan, tail) = scan_run(tlc, dir, &job.module, &cfg, jvm, continued)?;
        let stopped_early = !continued && !scan.violated.is_empty();
        if scan.type_ok || !(scan.completed || stopped_early) {
            let why = match scan.type_ok {
                true => "TypeOK is violated",
                false => "TLC did not finish",
            };
            out.error = Some(format!("{cfg}: {why}\n{tail}"));
            return Ok(out);
        }
        if n == 0 && scan.completed {
            out.states = scan.distinct_states;
        }
        let new: BTreeSet<String> = scan.violated.difference(&out.violated).cloned().collect();
        if new.is_empty() {
            break;
        }
        out.violated.extend(new);
        if checks.iter().all(|c| out.violated.contains(c)) {
            break;
        }
    }
    Ok(out)
}

/// One TLC run, its output read line by line: a witnesses run with `-continue` prints a trace
/// for almost every state, which is not kept. Returns the scan and the output's last lines.
fn scan_run(
    tlc: &Tlc,
    dir: &Path,
    module: &str,
    cfg: &str,
    (workers, heap): (&str, u64),
    continued: bool,
) -> Result<(InvariantScan, String)> {
    let mut command = tlc_command(tlc, dir, module, heap, workers)?;
    if continued {
        command.arg("-continue");
    }
    let mut child = command
        .args(["-config", cfg, &format!("{module}.tla")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;
    let mut stderr = child.stderr.take().expect("piped");
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let mut scan = InvariantScan::default();
    let mut tail: VecDeque<String> = VecDeque::new();
    let stdout = child.stdout.take().expect("piped");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        scan.line(&line);
        if tail.len() == 20 {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    child.wait().context("failed to wait for TLC")?;
    let err = err.join().unwrap_or_default();
    clean_up(dir, module);
    let mut tail: Vec<String> = tail.into_iter().map(|l| format!("  {l}")).collect();
    tail.extend(err.lines().map(|l| format!("  {l}")));
    Ok((scan, tail.join("\n")))
}

/// Each check of a run that the two engines decide differently, as an
/// `error[EngineDisagreement]` line: a `Witness_` one engine finds a state for and the other
/// not, or a `Deadlock_` TLC reports and BFS does not.
pub(super) fn state_model_disagreements(
    models: &StateModels,
    jobs: &[StateModelJob],
    ran: &[StateModelRan],
) -> Vec<String> {
    let line = |module: &str, check: &str, native: &str, tlc: &str| {
        format!(
            "error[EngineDisagreement]: {module} [{check}]: native {native}, TLC {tlc}; one engine is wrong (a tool bug, not a finding about the model)"
        )
    };
    let said = |found: bool| match found {
        true => "counterexample",
        false => "ok",
    };
    let mut out = Vec::new();
    for (job, r) in jobs.iter().zip(ran) {
        if r.error.is_some() {
            continue;
        }
        for w in &job.witnesses {
            let (native, tlc) = (models.found.contains(w), !r.witnessed.contains(w));
            if native != tlc {
                out.push(line(&job.module, w, said(native), said(tlc)));
            }
        }
        for d in r.deadlocks.iter().filter(|d| !models.found.contains(*d)) {
            out.push(line(&job.module, d, "ok", "counterexample"));
        }
    }
    out
}
