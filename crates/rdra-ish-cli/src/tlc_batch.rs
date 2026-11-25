//! TLC jobs in a JVM that stays up between them (#310).
//!
//! A TLC run of a small spec is mostly JVM start and stop around a few milliseconds of model
//! checking: 0.6 to 0.8 s on TLC 1.7.4, 0.18 s on the flake's TLC 1.8.0 on JDK 25.
//! `nix/tlc-batch/RdraTlcBatch.java` runs one job after another in one JVM, each through a
//! class loader of its own, and answers each on stdout; this is its caller. A job that gets
//! no answer was not run as far as the caller knows: [`Pool::run`] returns `None`, and the
//! caller starts TLC as a process of its own for it, which is also what happens when there
//! is no driver at all. On TLC 1.8.0 the driver still saves `engine_equivalence` about 60%
//! of its CPU (docs/formal-verification.md).

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// The launcher of the driver: `java ... RdraTlcBatch <tla2tools.jar>`, which the flake
/// builds beside `tlc` with the same JVM and flags.
pub const LAUNCHER: &str = "tlc-batch";

/// `0` turns batch runs off: every run is then a process of its own.
pub const BATCH_ENV: &str = "RDRA_TLC_BATCH";

/// The jobs one JVM runs before the pool replaces it: as many as were measured. Over 432
/// jobs in one JDK 17 JVM of `-Xmx1g` (36 small cones and state models, 12 times), the median
/// job stayed at 0.10 s, and the heap in use after a GC was 23 MiB plus 4.8 MiB for each
/// thread the jobs had left, with nothing else growing.
pub const JOBS_PER_JVM: usize = 400;

/// The threads the jobs of one JVM may leave before the pool replaces it. TLC 1.7.4 starts an
/// executor to print a liveness counterexample and never shuts it down, which a process ends
/// by exiting; in a JVM that stays, the thread keeps its job's classes and data in the heap
/// (5 MiB for a small cone). No other job left a thread in the measurement, beside two the
/// JVM itself starts once inside a job.
pub const THREADS_LEFT: usize = 8;

/// How long a JVM gets to say `ready`: it starts in 0.3 s on an idle machine.
const READY: Duration = Duration::from_secs(60);

/// How many jobs in a row may go unanswered before the pool stops starting JVMs.
const LOST_IN_A_ROW: usize = 3;

/// One TLC run: where its stdout and stderr go, and its arguments. The JVM does not run in
/// the job's directory, so every path among `args` is absolute.
pub struct Job<'a> {
    pub stdout: &'a Path,
    pub stderr: &'a Path,
    pub args: &'a [OsString],
}

/// Why a job has no answer; the JVM it ran in is gone either way.
#[derive(Debug)]
pub enum Lost {
    /// The JVM ended, or closed its stdout, before answering: TLC called `System.exit`, or
    /// the JVM ran out of memory.
    Died,
    /// No answer within the limit; the JVM was killed.
    TimedOut,
    /// The driver said the job threw, or said something this caller does not read.
    Failed(String),
}

/// A driver JVM that said `ready`.
pub struct Jvm {
    child: Child,
    stdin: Option<ChildStdin>,
    answers: Receiver<String>,
    jobs: usize,
    threads_left: usize,
}

impl Jvm {
    /// Starts `command`, a driver, and waits for its `ready`. `Err` is what it said instead,
    /// such as `unsupported ...` for a jar whose `tlc2.TLC` lacks a method the driver calls.
    pub fn start(mut command: Command) -> Result<Jvm, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start the TLC batch driver: {e}"))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, answers) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut jvm = Jvm {
            stdin: child.stdin.take(),
            child,
            answers,
            jobs: 0,
            threads_left: 0,
        };
        match jvm.answers.recv_timeout(READY) {
            Ok(line) if line == "ready" => Ok(jvm),
            Ok(line) => Err(line),
            Err(RecvTimeoutError::Timeout) => {
                jvm.kill();
                Err(format!("no `ready` within {} s", READY.as_secs()))
            }
            Err(RecvTimeoutError::Disconnected) => Err("ended before `ready`".to_string()),
        }
    }

    /// Runs `job` and returns the exit status a TLC process of its arguments has. On `Err`
    /// the JVM is gone and the job's files are whatever it wrote until then.
    pub fn run(&mut self, job: &Job, limit: Duration) -> Result<i32, Lost> {
        let id = self.jobs;
        self.jobs += 1;
        let mut line = format!("{id}\t{}\t{}", job.stdout.display(), job.stderr.display());
        for arg in job.args {
            let arg = arg.to_string_lossy();
            assert!(
                !arg.contains(['\t', '\n']),
                "a TLC argument holds a tab or a newline: {arg:?}"
            );
            line.push('\t');
            line.push_str(&arg);
        }
        line.push('\n');
        let sent = self.stdin.as_mut().is_some_and(|stdin| {
            stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.flush())
                .is_ok()
        });
        if !sent {
            self.kill();
            return Err(Lost::Died);
        }
        let answer = match self.answers.recv_timeout(limit) {
            Ok(answer) => answer,
            Err(RecvTimeoutError::Timeout) => {
                self.kill();
                return Err(Lost::TimedOut);
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.kill();
                return Err(Lost::Died);
            }
        };
        // `done <id> <exit status> <seconds> <threads left>`
        let fields: Vec<&str> = answer.split(' ').collect();
        let done = match fields[..] {
            ["done", of, status, _, left, ..] if of.parse() == Ok(id) => {
                status.parse().ok().zip(left.parse().ok())
            }
            _ => None,
        };
        let Some((status, left)) = done else {
            self.kill();
            return Err(Lost::Failed(answer));
        };
        self.threads_left = left;
        Ok(status)
    }

    /// Whether this JVM takes another job: it has run fewer than `JOBS_PER_JVM`, and they
    /// left fewer than `THREADS_LEFT` threads.
    fn fresh(&self) -> bool {
        self.jobs < JOBS_PER_JVM && self.threads_left < THREADS_LEFT
    }

    /// The jobs this JVM was given.
    pub fn jobs(&self) -> usize {
        self.jobs
    }

    fn kill(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Jvm {
    /// The driver exits at the end of its stdin; a job is never left running here, since
    /// `run` returns only on an answer or after a kill.
    fn drop(&mut self) {
        self.stdin = None;
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.kill();
    }
}

/// What a pool did so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// JVMs started and ready.
    pub jvms: usize,
    /// Jobs answered.
    pub answered: usize,
    /// Jobs given to a JVM and not answered, which the caller ran as processes.
    pub lost: usize,
    /// The longest an answered job waited for its answer, in milliseconds.
    pub longest_ms: u64,
}

/// Driver JVMs for callers on several threads, at most `limit` at once: a thread takes an
/// idle JVM, starts one, or waits for one, and each JVM runs one job at a time. A JVM that
/// stays up holds its heap and about 300 MiB beside it whether or not it has a job, so the
/// limit is what bounds the pool's memory.
pub struct Pool {
    command: Box<dyn Fn() -> Command + Send + Sync>,
    limit: usize,
    jvms: Mutex<Jvms>,
    freed: Condvar,
    started: AtomicUsize,
    answered: AtomicUsize,
    lost: AtomicUsize,
    longest_ms: AtomicU64,
}

#[derive(Default)]
struct Jvms {
    idle: Vec<Jvm>,
    /// The JVMs that are idle, running a job, or starting.
    live: usize,
    lost_in_a_row: usize,
    /// Why the pool starts no more JVMs, once it does not.
    off: Option<String>,
}

impl Pool {
    /// A pool of at most `limit` JVMs, started with `command`'s commands, one call per JVM:
    /// each JVM needs a `java.io.tmpdir` of its own, where TLC unpacks its standard modules.
    pub fn new(limit: usize, command: impl Fn() -> Command + Send + Sync + 'static) -> Pool {
        Pool {
            command: Box::new(command),
            limit: limit.max(1),
            jvms: Mutex::new(Jvms::default()),
            freed: Condvar::new(),
            started: AtomicUsize::new(0),
            answered: AtomicUsize::new(0),
            lost: AtomicUsize::new(0),
            longest_ms: AtomicU64::new(0),
        }
    }

    /// Runs `job` in a JVM of the pool and returns TLC's exit status, or `None` when the job
    /// has to run as a process of its own: the driver does not start or does not take this
    /// TLC, the job got no answer within `limit`, or `LOST_IN_A_ROW` jobs before it got none.
    pub fn run(&self, job: &Job, limit: Duration) -> Option<i32> {
        let mut jvm = self.take()?;
        let sent = Instant::now();
        let result = jvm.run(job, limit);
        let mut jvms = self.jvms.lock().unwrap();
        let status = match result {
            Ok(status) => {
                let waited = sent.elapsed().as_millis() as u64;
                self.longest_ms.fetch_max(waited, Ordering::SeqCst);
                self.answered.fetch_add(1, Ordering::SeqCst);
                jvms.lost_in_a_row = 0;
                if jvm.fresh() && jvms.off.is_none() {
                    jvms.idle.push(jvm);
                } else {
                    jvms.live -= 1;
                }
                Some(status)
            }
            Err(lost) => {
                self.lost.fetch_add(1, Ordering::SeqCst);
                jvms.live -= 1;
                jvms.lost_in_a_row += 1;
                if jvms.lost_in_a_row >= LOST_IN_A_ROW {
                    Self::turn_off(
                        &mut jvms,
                        format!("{LOST_IN_A_ROW} jobs in a row got no answer, the last: {lost:?}"),
                    );
                }
                None
            }
        };
        drop(jvms);
        self.freed.notify_all();
        status
    }

    /// A JVM for one job: an idle one, a new one while there are fewer than the limit, else
    /// the next one freed. `None` once the pool is off.
    fn take(&self) -> Option<Jvm> {
        let mut jvms = self.jvms.lock().unwrap();
        loop {
            if jvms.off.is_some() {
                return None;
            }
            if let Some(jvm) = jvms.idle.pop() {
                return Some(jvm);
            }
            if jvms.live < self.limit {
                jvms.live += 1;
                break;
            }
            jvms = self.freed.wait(jvms).unwrap();
        }
        drop(jvms);
        match Jvm::start((self.command)()) {
            Ok(jvm) => {
                self.started.fetch_add(1, Ordering::SeqCst);
                Some(jvm)
            }
            Err(why) => {
                let mut jvms = self.jvms.lock().unwrap();
                jvms.live -= 1;
                Self::turn_off(&mut jvms, why);
                drop(jvms);
                self.freed.notify_all();
                None
            }
        }
    }

    fn turn_off(jvms: &mut Jvms, why: String) {
        jvms.off.get_or_insert(why);
        jvms.live -= jvms.idle.len();
        jvms.idle.clear();
    }

    /// Why the pool stopped starting JVMs, when it did.
    pub fn why_off(&self) -> Option<String> {
        self.jvms.lock().unwrap().off.clone()
    }

    pub fn counts(&self) -> Counts {
        Counts {
            jvms: self.started.load(Ordering::SeqCst),
            answered: self.answered.load(Ordering::SeqCst),
            lost: self.lost.load(Ordering::SeqCst),
            longest_ms: self.longest_ms.load(Ordering::SeqCst),
        }
    }
}

/// The driver's launcher on PATH, unless `RDRA_TLC_BATCH=0`.
pub fn launcher() -> Option<PathBuf> {
    if std::env::var(BATCH_ENV).as_deref() == Ok("0") {
        return None;
    }
    crate::verify::find_on_path(LAUNCHER)
}
