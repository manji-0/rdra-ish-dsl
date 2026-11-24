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
use super::schedule::{run_budgeted, Budget, Hold};
use super::tlc::{clean_up, tlc_command};
use super::tlc_tool::Tlc;
use crate::export::Modules;
use crate::memory::{tlc_workers, TlcHeap, HEAP_PER_MODULE_BYTE, JVM_OVERHEAD};
use crate::tlc_batch::{self, with_the_fingerprints_of, without_run_noise};
use crate::tlc_env::{current_user_heap, tlc_env_sized, Heap, Jit};
use crate::tlc_output::{out_of_memory, InvariantScan, Outcome};
use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::TransitionSystem;
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Up to this many states a witnesses cfg runs with `-continue`, which prints a trace for each
/// state that violates a `Witness_`, so almost every state; above it each run stops at the
/// first one, and the reruns find one witness each. `engine_equivalence` splits the same way.
const CONTINUE_STATES: u64 = 1_000;

/// Up to this many states by BFS a run compiles with C1 only (`Jit::C1`): it ends within a
/// second or two, where C1 saves CPU and start-up time, and C2 would pay off only on a long
/// run (#309). A cone run's states are not known when it starts, so this is for state models.
const SMALL_RUN_STATES: usize = 10_000;

/// The heap of a batch JVM (#310). A JVM that stays up commits its whole `-Xmx`, so it gets
/// what a small module needs by `TlcHeap::of`'s rule before that rounds up to 1 GiB, and a
/// module that needs more runs as a process.
const BATCH_HEAP: u64 = 512 << 20;

/// The batch JVMs of one `verify` at most. Their jobs end within a second or two each.
const BATCH_JVMS: usize = 8;

/// How long a run may take in a batch JVM before the JVM is killed and the job runs as
/// processes: a module of at most `SMALL_RUN_STATES` states checks in a few seconds, so a
/// run that takes this long is on a machine with no core to spare, or hangs.
const BATCH_LIMIT: Duration = Duration::from_secs(120);

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

/// Runs every job. First the small ones in batch JVMs (`Batch`), when this TLC has a batch
/// driver; then the others, and any a batch JVM did not answer, as processes
/// (`run_as_processes`). Both hold what they are charged of one `Budget` of `share`. They do
/// not overlap: a batch JVM's runs have one worker and the JVMs are as many as the cores, and
/// a process run divides the cores among its workers (`tlc_workers`), so the two at once
/// would run more threads than cores. The results come back in job order.
pub(super) fn run_state_models(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    jobs: &[StateModelJob],
    share: u64,
) -> Result<Vec<StateModelRan>> {
    let set = current_user_heap();
    let budget = Budget::new(share);
    let mut ran: Vec<Option<StateModelRan>> = jobs.iter().map(|_| None).collect();
    let small: Vec<usize> = (0..jobs.len())
        .filter(|k| Batch::takes(&jobs[*k], set))
        .collect();
    if let Some(batch) = Batch::start(tlc, dir, small.len(), share)? {
        for (k, answered) in batch.run((tlc, dir, modules), jobs, &small, &budget)? {
            ran[k] = answered;
        }
    }
    let rest: Vec<usize> = (0..jobs.len()).filter(|k| ran[*k].is_none()).collect();
    for (k, r) in run_as_processes(tlc, dir, modules, jobs, &rest, (&budget, set))? {
        ran[k] = Some(r);
    }
    Ok(ran
        .into_iter()
        .map(|r| r.expect("every job ran one way or the other"))
        .collect())
}

/// The JVM a job's runs go to.
#[derive(Clone, Copy)]
enum Jvm<'a> {
    /// A process per run, with this many workers and this heap.
    Process(&'a str, u64),
    /// A batch JVM, with one worker.
    Batch(&'a Batch),
}

/// The batch JVMs of one `verify` (#310): a TLC run of a small module is 0.6 s of JVM start
/// and stop around a few milliseconds of checking, and a JVM that stays up runs the next
/// one in 0.1 s. At most as many JVMs as their heaps and `JVM_OVERHEAD` fit the share, so
/// they hold no more of `--memory` than the process runs they replace; they are gone before
/// a process run starts. Each run has one worker: the JVMs are as many as the cores allow,
/// and with one worker a run and its rerun as a process print the same log.
struct Batch {
    pool: tlc_batch::Pool,
    jvms: usize,
    /// The spec dir, absolute: a batch JVM does not run in it.
    dir: PathBuf,
    /// `RDRA_TLC_BATCH=both`: each run goes again as a process, and the two must agree.
    both: bool,
    compared: AtomicUsize,
}

impl Batch {
    /// Whether the job's runs fit a batch JVM: a module BFS found small, whose heap by
    /// `TlcHeap::of`'s rule is within `BATCH_HEAP`, when the user names no heap.
    fn takes(job: &StateModelJob, set: Option<u64>) -> bool {
        set.is_none()
            && job.states <= SMALL_RUN_STATES
            && job.module_bytes.saturating_mul(HEAP_PER_MODULE_BYTE) <= BATCH_HEAP
    }

    /// The batch JVMs for `small` jobs, none started yet; `None` when there is no job, no
    /// driver beside this `tlc`, or no room for one JVM in `share`.
    fn start(tlc: &Tlc, dir: &Path, small: usize, share: u64) -> Result<Option<Batch>> {
        let Some(launcher) = tlc.batch_launcher().filter(|_| small > 0) else {
            return Ok(None);
        };
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let fit = usize::try_from(share / (BATCH_HEAP + JVM_OVERHEAD)).unwrap_or(usize::MAX);
        let jvms = small.min(cores).min(BATCH_JVMS).min(fit);
        let dir = std::path::absolute(dir)
            .with_context(|| format!("failed to resolve {}", dir.display()))?;
        // The driver reads a job as one line of tab-separated fields.
        if jvms == 0 || dir.to_string_lossy().contains(['\t', '\n']) {
            return Ok(None);
        }
        // Every job of a batch JVM is small, so C1 as for a small process run.
        let jit = tlc.jit(Jit::C1);
        let started = AtomicUsize::new(0);
        let tmp_root = dir.clone();
        let pool = tlc_batch::Pool::new(jvms, move || {
            // TLC unpacks its standard modules into java.io.tmpdir; the JVMs must not share it.
            let n = started.fetch_add(1, Ordering::SeqCst);
            let tmp = tmp_root.join(format!("batch-{n}.tmp"));
            let _ = fs::create_dir_all(&tmp);
            let (key, value) = tlc_env_sized(BATCH_HEAP, &tmp, jit);
            let mut command = Command::new(&launcher);
            command.env(key, value).stderr(Stdio::null());
            command
        });
        let both = std::env::var(tlc_batch::BATCH_ENV).as_deref() == Ok("both");
        Ok(Some(Batch {
            pool,
            jvms,
            dir,
            both,
            compared: AtomicUsize::new(0),
        }))
    }

    /// Runs the jobs `small` names, one thread per JVM, and ends the JVMs. A thread holds a
    /// JVM's heap and `JVM_OVERHEAD` of `budget` from before it starts its JVM until the
    /// JVMs end: a JVM that stays up holds its memory between runs too. A job comes back
    /// `None` when one of its runs got no answer; it then runs whole as processes.
    fn run(
        self,
        (tlc, dir, modules): (&Tlc, &Path, &Modules),
        jobs: &[StateModelJob],
        small: &[usize],
        budget: &Budget,
    ) -> Result<Vec<(usize, Option<StateModelRan>)>> {
        let next = AtomicUsize::new(0);
        let done: Mutex<Vec<(usize, Result<Option<StateModelRan>>)>> = Mutex::new(Vec::new());
        let held: Mutex<Vec<Hold>> = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..self.jvms {
                s.spawn(|| {
                    let hold = budget.hold(BATCH_HEAP + JVM_OVERHEAD);
                    held.lock().expect("lock").push(hold);
                    while let Some(&k) = small.get(next.fetch_add(1, Ordering::SeqCst)) {
                        let ran = run_one(tlc, dir, modules, &jobs[k], Jvm::Batch(&self));
                        done.lock().expect("lock").push((k, ran));
                    }
                });
            }
        });
        let counts = self.pool.counts();
        if self.both {
            eprintln!(
                "tlc batch: {} runs in {} JVMs, {} compared with a process run and the same, {} without an answer",
                counts.answered,
                counts.jvms,
                self.compared.load(Ordering::SeqCst),
                counts.lost
            );
        }
        let tmps = counts.jvms;
        let root = self.dir.clone();
        // The JVMs end here, before their tmpdirs go and before any process run starts, and
        // then let go of what they held.
        drop(self);
        drop(held);
        for n in 0..tmps {
            let _ = fs::remove_dir_all(root.join(format!("batch-{n}.tmp")));
        }
        done.into_inner()
            .expect("lock")
            .into_iter()
            .map(|(k, ran)| Ok((k, ran?)))
            .collect()
    }

    /// One run in a batch JVM: the scan, the output's last lines, and whether TLC's exit
    /// status was 0. `None` when the job got no answer.
    fn scan(
        &self,
        tlc: &Tlc,
        (module, cfg): (&str, &str),
        continued: bool,
    ) -> Result<Option<(InvariantScan, String)>> {
        let dir = &self.dir;
        let mut args: Vec<OsString> = vec!["-workers".into(), "1".into(), "-metadir".into()];
        args.push(dir.join(format!("{module}.states")).into());
        if continued {
            args.push("-continue".into());
        }
        args.push("-config".into());
        args.push(dir.join(cfg).into());
        args.push(dir.join(format!("{module}.tla")).into());
        let (stdout, stderr) = (
            dir.join(format!("{module}.batch-out")),
            dir.join(format!("{module}.batch-err")),
        );
        let job = tlc_batch::Job {
            stdout: &stdout,
            stderr: &stderr,
            args: &args,
        };
        let status = self.pool.run(&job, BATCH_LIMIT);
        let read = |path: &Path| {
            let text = fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned());
            let _ = fs::remove_file(path);
            text.unwrap_or_default()
        };
        let (out, err) = (read(&stdout), read(&stderr));
        clean_up(dir, module);
        let Some(status) = status else {
            return Ok(None);
        };
        let scanned = scan_text(&out, &err);
        if self.both {
            // With the fingerprints of the batch run, and one worker as it had: two runs of
            // one job then print the same log, whichever way they start.
            let pinned = with_the_fingerprints_of(&out, &args[4..]);
            let command = tlc_command(tlc, dir, module, (BATCH_HEAP, Jit::C1), "1")?;
            let (alone, alone_ok) = run_process(tlc, command, &pinned)?;
            clean_up(dir, module);
            let (batch_lines, alone_lines) = (
                without_run_noise(&format!("{out}{err}")),
                without_run_noise(&format!("{}{}", alone.out, alone.err)),
            );
            if (status == 0) != alone_ok
                || scanned.0 != scan_text(&alone.out, &alone.err).0
                || batch_lines != alone_lines
            {
                let differ: Vec<String> = batch_lines
                    .iter()
                    .zip(&alone_lines)
                    .filter(|(b, a)| b != a)
                    .take(5)
                    .map(|(b, a)| format!("  batch:   {b}\n  process: {a}"))
                    .collect();
                bail!(
                    "{cfg}: TLC in a batch JVM and as a process differ (exit {status} and {}, {} and {} lines)\n{}",
                    if alone_ok { "0" } else { "not 0" },
                    batch_lines.len(),
                    alone_lines.len(),
                    differ.join("\n")
                );
            }
            self.compared.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Some(scanned))
    }
}

/// Runs the jobs `rest` names as processes, the largest heap first, then the most states, as
/// many at once as the cores allow and their heaps fit `share` (`Budget`), each with the heap
/// its module needs (`TlcHeap::of`) and once more at twice the heap when it runs out of
/// memory and the share holds that. A job whose module needs more than the share has no run.
fn run_as_processes(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    jobs: &[StateModelJob],
    rest: &[usize],
    (budget, set): (&Budget, Option<u64>),
) -> Result<Vec<(usize, StateModelRan)>> {
    let share = budget.bytes();
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let heaps: Vec<TlcHeap> = jobs
        .iter()
        .map(|job| TlcHeap::of(job.module_bytes, Heap::Cone, share, set))
        .collect();
    let mut order: Vec<usize> = rest.to_vec();
    order.sort_by_key(|k| std::cmp::Reverse((heaps[*k].charge(), jobs[*k].states)));
    let done = run_budgeted(
        &order,
        cores,
        budget,
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
            let process = |heap: u64| -> Result<StateModelRan> {
                let ran = run_one(tlc, dir, modules, job, Jvm::Process(&workers, heap))?;
                Ok(ran.expect("a process run has its output"))
            };
            let ran = process(bytes)?;
            let (true, Some(more)) = (ran.out_of_memory(), heap.doubled(share)) else {
                return Ok(ran);
            };
            hold.regrow(more.charge());
            process(more.bytes().expect("a doubled heap is sized"))
        },
    );
    done.into_iter().map(|(k, r)| Ok((k, r?))).collect()
}

/// The deadlocks cfg with `-continue`, then the witnesses cfg, each rerun without the
/// invariants it reported until a rerun reports no new one: with `-continue` TLC reports one
/// invariant per state, the first in cfg order, and without it one per run. `None` when a
/// batch JVM did not answer one of the runs.
fn run_one(
    tlc: &Tlc,
    dir: &Path,
    modules: &Modules,
    job: &StateModelJob,
    jvm: Jvm,
) -> Result<Option<StateModelRan>> {
    let mut ran = StateModelRan {
        states: None,
        deadlocks: BTreeSet::new(),
        witnessed: BTreeSet::new(),
        error: None,
    };
    let Some(deadlocks) = rerun(
        (tlc, dir, modules, job, jvm),
        (&job.module, PrintMode::Deadlocks, &job.deadlocks),
        true,
    )?
    else {
        return Ok(None);
    };
    ran.states = deadlocks.states;
    ran.deadlocks = deadlocks.violated;
    if let Some(why) = deadlocks.error {
        ran.error = Some(why);
        return Ok(Some(ran));
    }
    // A module with no `Deadlock_` check has no deadlocks run to count its states; TLC
    // runs the system BFS explored, so BFS's count stands in.
    let states = ran.states.unwrap_or(job.states as u64);
    let continued = states <= CONTINUE_STATES;
    let Some(witnesses) = rerun(
        (tlc, dir, modules, job, jvm),
        (&job.witnesses_cfg, PrintMode::Witnesses, &job.witnesses),
        continued,
    )?
    else {
        return Ok(None);
    };
    ran.witnessed = witnesses.violated;
    ran.error = witnesses.error;
    Ok(Some(ran))
}

/// What the runs of one cfg found.
struct Reruns {
    violated: BTreeSet<String>,
    /// The first run's distinct states, when it explored every state.
    states: Option<u64>,
    error: Option<String>,
}

fn rerun(
    (tlc, dir, modules, job, jvm): (&Tlc, &Path, &Modules, &StateModelJob, Jvm),
    (cfg_stem, mode, checks): (&str, PrintMode, &[String]),
    continued: bool,
) -> Result<Option<Reruns>> {
    let mut out = Reruns {
        violated: BTreeSet::new(),
        states: None,
        error: None,
    };
    if checks.is_empty() {
        return Ok(Some(out));
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
        let jit = match job.states <= SMALL_RUN_STATES {
            true => Jit::C1,
            false => Jit::Tiered,
        };
        let ran = match jvm {
            Jvm::Process(workers, heap) => Some(scan_run(
                tlc,
                dir,
                (&job.module, &cfg),
                ((workers, heap), jit),
                continued,
            )?),
            Jvm::Batch(batch) => batch.scan(tlc, (&job.module, &cfg), continued)?,
        };
        let Some((scan, tail)) = ran else {
            return Ok(None);
        };
        let stopped_early = !continued && !scan.violated.is_empty();
        if scan.type_ok || !(scan.completed || stopped_early) {
            let why = match scan.type_ok {
                true => "TypeOK is violated",
                false => "TLC did not finish",
            };
            out.error = Some(format!("{cfg}: {why}\n{tail}"));
            return Ok(Some(out));
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
    Ok(Some(out))
}

/// One TLC run as a process, its output read line by line: a witnesses run with `-continue`
/// prints a trace for almost every state, which is not kept. Returns the scan and the
/// output's last lines.
fn scan_run(
    tlc: &Tlc,
    dir: &Path,
    (module, cfg): (&str, &str),
    ((workers, heap), jit): ((&str, u64), Jit),
    continued: bool,
) -> Result<(InvariantScan, String)> {
    let mut command = tlc_command(tlc, dir, module, (heap, jit), workers)?;
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

/// The scan of a run's whole output and its last lines, as `scan_run` returns them.
fn scan_text(out: &str, err: &str) -> (InvariantScan, String) {
    let mut scan = InvariantScan::default();
    for line in out.lines() {
        scan.line(line);
    }
    let lines: Vec<&str> = out.lines().collect();
    let tail: Vec<String> = lines[lines.len().saturating_sub(20)..]
        .iter()
        .copied()
        .chain(err.lines())
        .map(|l| format!("  {l}"))
        .collect();
    (scan, tail.join("\n"))
}

/// What a process run printed.
struct Printed {
    out: String,
    err: String,
}

/// `command` with `args` more, run to its end: what it printed and whether it exited 0.
fn run_process(tlc: &Tlc, mut command: Command, args: &[OsString]) -> Result<(Printed, bool)> {
    let output = command
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;
    let printed = Printed {
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    Ok((printed, output.status.success()))
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
