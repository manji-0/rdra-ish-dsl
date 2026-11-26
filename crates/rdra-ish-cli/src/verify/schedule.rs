//! The TLC runs of one kind share a budget of bytes: each holds its `-Xmx` and
//! `JVM_OVERHEAD` (`TlcHeap::charge`) while it goes, and a run starts once what the running
//! ones hold and its own charge fit the budget, or when nothing else runs, so a run whose
//! heap alone is past the budget still runs, alone (#309).
//!
//! The cone and whole-model runs also share the cores (#384,
//! docs/design/tlc-scheduling.md): each holds its TLC workers of them while it goes. `pick`
//! chooses the next run and its workers from what is free and what BFS has counted, and `Pool`
//! runs the jobs one `pick` at a time. The state-model runs keep `run_budgeted`.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

/// What the running TLC runs of one kind hold.
pub(super) struct Budget {
    bytes: u64,
    held: Mutex<Held>,
    freed: Condvar,
}

#[derive(Default)]
struct Held {
    bytes: u64,
    runs: usize,
}

impl Budget {
    pub(super) fn new(bytes: u64) -> Self {
        Budget {
            bytes,
            held: Mutex::new(Held::default()),
            freed: Condvar::new(),
        }
    }

    /// What the runs going at once may hold. The cone and whole-model runs no longer read it
    /// (`Pool`); #323's state-model runs do.
    #[allow(dead_code)]
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Waits until `bytes` fit beside what the running runs hold, or no run holds any, then
    /// holds them until the `Hold` drops.
    pub(super) fn hold(&self, bytes: u64) -> Hold<'_> {
        let mut held = self.held.lock().expect("lock");
        while held.runs > 0 && held.bytes.saturating_add(bytes) > self.bytes {
            held = self.freed.wait(held).expect("lock");
        }
        held.bytes += bytes;
        held.runs += 1;
        Hold {
            budget: self,
            bytes,
        }
    }

    fn release(&self, bytes: u64) {
        let mut held = self.held.lock().expect("lock");
        held.bytes -= bytes;
        held.runs -= 1;
        drop(held);
        self.freed.notify_all();
    }
}

/// What one run holds of its budget.
pub(super) struct Hold<'a> {
    budget: &'a Budget,
    bytes: u64,
}

impl Hold<'_> {
    /// Lets go of what the run holds, then waits until `bytes` fit as `Budget::hold` does. The
    /// run lets go first, so two runs that both grow never wait on each other.
    pub(super) fn regrow(&mut self, bytes: u64) {
        self.budget.release(self.bytes);
        let grown = self.budget.hold(bytes);
        self.bytes = grown.bytes;
        std::mem::forget(grown);
    }
}

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

/// Runs the jobs `order` names, in that order, on up to `threads` threads, each holding
/// `charge(job)` of `budget` while it runs. `run` gets the job, its hold, and how many jobs
/// are running or queued as it starts, itself included. The results come back in job order.
pub(super) fn run_budgeted<R: Send>(
    order: &[usize],
    threads: usize,
    budget: &Budget,
    charge: impl Fn(usize) -> u64 + Sync,
    run: impl Fn(usize, &mut Hold, usize) -> R + Sync,
) -> Vec<(usize, R)> {
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, order.len().max(1)) {
            s.spawn(|| {
                while let Some(&job) = order.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let mut hold = budget.hold(charge(job));
                    let in_flight = order.len() - finished.load(Ordering::SeqCst);
                    let result = run(job, &mut hold, in_flight);
                    drop(hold);
                    finished.fetch_add(1, Ordering::SeqCst);
                    done.lock().expect("lock").push((job, result));
                }
            });
        }
    });
    let mut done = done.into_inner().expect("lock");
    done.sort_by_key(|(job, _)| *job);
    done
}

/// States per TLC worker: a run of `s` states gets `ceil(s / STATES_PER_WORKER)` workers, up
/// to the cap. On #309's 18 cores a 48k-state cone gained nothing past 2 workers and a
/// 203k-state one gained up to 4; past 1.34M states it is a guess
/// (docs/design/tlc-scheduling.md).
pub(super) const STATES_PER_WORKER: u64 = 50_000;

/// The workers a run gets at most, unless memory bounds the runs: past 4 to 8 a run gained no
/// time and spent 50 to 70% more CPU (#309).
pub(super) const WORKER_CAP: usize = 8;

/// From this many cores up, a job BFS has not counted yet is taken to be large and gets the
/// cap; below it, it gets the cores divided among the runs left, as before #384. On a 4-core
/// CI runner the cap made the `--with-tlc` samples 17% slower (5 min 13 s against 6 min 7 s,
/// one run each), since BFS counts nothing until its entity pass ends. A guess between 4 and
/// 64.
const MANY_CORES: usize = 16;

/// When memory, not the queue, bounds the runs, a run may get up to this many times
/// `WORKER_CAP`: two runs of 32 workers kept 25 to 30 of 64 cores busy (#384). A guess.
const UNCAPPED: usize = 4;

/// The cores TLC runs on, and the knobs that fix its runs (#384).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Machine {
    pub(super) cores: usize,
    /// `--tlc-workers` / `RDRA_TLC_WORKERS`: every run's workers.
    pub(super) workers: Option<usize>,
    /// `--tlc-jobs` / `RDRA_TLC_JOBS`: the most runs at once.
    pub(super) jobs: Option<usize>,
}

impl Machine {
    /// This machine's cores, and the knobs.
    pub(super) fn here(workers: Option<usize>, jobs: Option<usize>) -> Machine {
        Machine {
            cores: std::thread::available_parallelism().map_or(4, |n| n.get()),
            workers,
            jobs,
        }
    }
}

/// Today's order among jobs BFS has not counted, the larger first: the charge, then
/// `Job::weight`.
pub(super) type Rank = (u64, (bool, usize));

/// A job waiting to start, as `pick` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Want {
    /// What it holds of the share while it runs: its `-Xmx` and `JVM_OVERHEAD`.
    pub(super) bytes: u64,
    /// The states of its module, when a complete plain BFS run counted them.
    pub(super) states: Option<u64>,
    pub(super) rank: Rank,
}

/// What is free when a run could start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Free {
    /// Cores no run holds.
    pub(super) cores: usize,
    /// Bytes of the share no run holds.
    pub(super) bytes: u64,
    pub(super) share: u64,
    /// Runs going.
    pub(super) running: usize,
    /// Runs holding bytes: those going, but one that lets go to grow its heap.
    pub(super) holding: usize,
    /// BFS has ended, so a job it has not counted gets no count.
    pub(super) counted: bool,
}

/// The workers by the states alone: `ceil(s / STATES_PER_WORKER)`, 1 to `cap`.
fn by_states(states: u64, cap: usize) -> usize {
    usize::try_from(states.div_ceil(STATES_PER_WORKER))
        .unwrap_or(usize::MAX)
        .clamp(1, cap)
}

/// Where a job stands in the order: a counted job that takes the full cap first, the most
/// states first; then the jobs BFS has not counted, by `Rank`; then the other counted jobs,
/// the most states first.
fn priority(want: &Want) -> (u8, u64, Rank) {
    match want.states {
        Some(s) if by_states(s, WORKER_CAP) == WORKER_CAP => (2, s, want.rank),
        None => (1, 0, want.rank),
        Some(s) => (0, s, want.rank),
    }
}

/// The workers `want` asks for when `waiting` jobs, itself included, wait.
///
/// - `--tlc-workers N`: `N`.
/// - Counted, `s` states: `by_states`.
/// - Not counted while BFS runs, on `MANY_CORES` or more: the cap, as if it were large.
/// - Not counted otherwise: the cores divided among the runs left, as before #384.
///
/// A job that asks for the cap gets more when memory, not the queue, bounds the runs: the
/// cores divided among the runs of its charge that fit the share, up to `UNCAPPED` times the
/// cap. Never more than the cores.
pub(super) fn target(want: &Want, machine: &Machine, free: &Free, waiting: usize) -> usize {
    if let Some(n) = machine.workers {
        return n.max(1);
    }
    let cores = machine.cores.max(1);
    let at_once = machine.jobs.unwrap_or(usize::MAX).max(1);
    let cap = WORKER_CAP.min(cores);
    // The runs of this job's charge that fit the share, no more than `--tlc-jobs`.
    let fit = usize::try_from(free.share / want.bytes.max(1))
        .unwrap_or(usize::MAX)
        .clamp(1, at_once);
    let workers = match (want.states, free.counted) {
        (Some(s), _) => by_states(s, cap),
        (None, false) if cores >= MANY_CORES => cap,
        (None, _) => (cores / (waiting + free.running).clamp(1, fit)).clamp(1, cap),
    };
    if workers < cap {
        return workers;
    }
    match fit.saturating_mul(cap) < cores {
        true => (cores / fit).clamp(cap, UNCAPPED * WORKER_CAP).min(cores),
        false => cap,
    }
}

/// The job of `waiting` to start now and its workers, or `None` to wait for a run to end or
/// for BFS to count more. The job at the head of the order (`priority`, then the lower
/// index) goes once the cores for its workers are free (all of them, when it asks for more)
/// and its bytes fit beside what the running runs hold, or at once when nothing runs.
/// Nothing passes the head, so no job waits forever. `--tlc-jobs` bounds the runs at once.
pub(super) fn pick(waiting: &[Want], free: &Free, machine: &Machine) -> Option<(usize, usize)> {
    if machine.jobs.is_some_and(|j| free.running >= j.max(1)) {
        return None;
    }
    let (at, want) =
        (waiting.iter().enumerate()).min_by_key(|(at, want)| (Reverse(priority(want)), *at))?;
    let workers = target(want, machine, free, waiting.len());
    if free.running == 0 {
        return Some((at, workers));
    }
    let need = workers.min(machine.cores.max(1));
    let fits = free.holding == 0 || want.bytes <= free.bytes;
    (free.cores >= need && fits).then_some((at, workers))
}

/// The jobs one `Pool` runs, what they hold, and what BFS has counted, behind one lock.
struct Running {
    /// What the runs going at once may hold; it grows once BFS ends.
    share: u64,
    bytes: u64,
    holding: usize,
    cores: usize,
    running: usize,
    counted: bool,
    states: Vec<Option<u64>>,
    waiting: Vec<usize>,
}

/// Runs TLC jobs one `pick` at a time, on a share of bytes that grows once BFS ends and the
/// cores of its `Machine`.
pub(super) struct Pool {
    machine: Machine,
    /// The share a job's heap is sized by, and its rerun at twice the heap: the share the
    /// pool starts with, whenever the job starts, so neither depends on when BFS ends and no
    /// verdict on the schedule.
    sizing: u64,
    running: Mutex<Running>,
    changed: Condvar,
}

/// A job `Pool` started: its index, its workers, the share its heap is sized by, and the
/// states BFS counted of it by then.
pub(super) struct Started {
    pub(super) job: usize,
    pub(super) workers: usize,
    pub(super) share: u64,
    pub(super) states: Option<u64>,
}

/// Cores held for work beside the TLC runs, as `--prove`'s z3 processes: the runs that start
/// while it is held count them as taken (#421).
pub(super) struct Cores<'a> {
    pool: &'a Pool,
    cores: usize,
}

/// What one run holds of its `Pool`.
pub(super) struct Slot<'a> {
    pool: &'a Pool,
    bytes: u64,
    cores: usize,
}

impl Pool {
    /// A pool for `jobs` jobs on `share` bytes. `counted`: no BFS count is coming.
    pub(super) fn new(machine: Machine, share: u64, jobs: usize, counted: bool) -> Pool {
        Pool {
            machine,
            sizing: share,
            running: Mutex::new(Running {
                share,
                bytes: 0,
                holding: 0,
                cores: 0,
                running: 0,
                counted,
                states: vec![None; jobs],
                waiting: Vec::new(),
            }),
            changed: Condvar::new(),
        }
    }

    /// BFS counted the states of these jobs.
    pub(super) fn learn(&self, counts: impl IntoIterator<Item = (usize, u64)>) {
        let mut running = self.running.lock().expect("lock");
        for (job, states) in counts {
            if let Some(at) = running.states.get_mut(job) {
                *at = Some(states);
            }
        }
        drop(running);
        self.changed.notify_all();
    }

    /// BFS has ended: no more counts come, and the runs at once may hold `share` when that is
    /// more. A heap is still sized by the share the pool started with.
    pub(super) fn bfs_ended(&self, share: u64) {
        let mut running = self.running.lock().expect("lock");
        running.counted = true;
        running.share = running.share.max(share);
        drop(running);
        self.changed.notify_all();
    }

    /// Holds `cores` of the machine's cores, at most all of them, until the `Cores` drops, so
    /// that the TLC runs that start meanwhile leave them free. It does not wait: a run going
    /// keeps its cores, and the next waits for what is left.
    pub(super) fn hold_cores(&self, cores: usize) -> Cores<'_> {
        let cores = cores.min(self.machine.cores.max(1));
        self.running.lock().expect("lock").cores += cores;
        Cores { pool: self, cores }
    }

    /// Runs the jobs `jobs` names, one `pick` at a time, on as many threads as runs can go at
    /// once. `want` gives a job's bytes and rank at the sizing share and its count; `run` gets
    /// the job as it started and its slot. The results come back in the order of `jobs`.
    pub(super) fn run<R: Send>(
        &self,
        jobs: &[usize],
        want: impl Fn(usize, u64, Option<u64>) -> (u64, Rank) + Sync,
        run: impl Fn(Started, &mut Slot) -> R + Sync,
    ) -> Vec<R> {
        self.running.lock().expect("lock").waiting = jobs.to_vec();
        let threads = (self.machine.jobs.unwrap_or(usize::MAX))
            .min(self.machine.cores.max(1))
            .clamp(1, jobs.len().max(1));
        let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    while let Some((started, mut slot)) = self.next(&want) {
                        let job = started.job;
                        let result = run(started, &mut slot);
                        drop(slot);
                        done.lock().expect("lock").push((job, result));
                    }
                });
            }
        });
        let mut done = done.into_inner().expect("lock");
        // Each job's first place in `jobs`, as `position` finds it.
        let mut place: HashMap<usize, usize> = HashMap::with_capacity(jobs.len());
        for (i, &job) in jobs.iter().enumerate() {
            place.entry(job).or_insert(i);
        }
        done.sort_by_key(|(job, _)| place.get(job).copied());
        done.into_iter().map(|(_, r)| r).collect()
    }

    /// Waits until `pick` starts a job and holds what it takes; `None` once none waits.
    fn next(
        &self,
        want: &impl Fn(usize, u64, Option<u64>) -> (u64, Rank),
    ) -> Option<(Started, Slot<'_>)> {
        let cores = self.machine.cores.max(1);
        let mut running = self.running.lock().expect("lock");
        loop {
            if running.waiting.is_empty() {
                return None;
            }
            let wants: Vec<Want> = (running.waiting.iter())
                .map(|&job| {
                    let (bytes, rank) = want(job, self.sizing, running.states[job]);
                    Want {
                        bytes,
                        states: running.states[job],
                        rank,
                    }
                })
                .collect();
            let free = Free {
                cores: cores.saturating_sub(running.cores),
                bytes: running.share.saturating_sub(running.bytes),
                share: running.share,
                running: running.running,
                holding: running.holding,
                counted: running.counted,
            };
            if let Some((at, workers)) = pick(&wants, &free, &self.machine) {
                let job = running.waiting.remove(at);
                let held = workers.min(cores);
                running.bytes += wants[at].bytes;
                running.holding += 1;
                running.cores += held;
                running.running += 1;
                let started = Started {
                    job,
                    workers,
                    share: self.sizing,
                    states: running.states[job],
                };
                let slot = Slot {
                    pool: self,
                    bytes: wants[at].bytes,
                    cores: held,
                };
                return Some((started, slot));
            }
            running = self.changed.wait(running).expect("lock");
        }
    }
}

impl Slot<'_> {
    /// Lets go of the run's bytes, then waits until `bytes` fit beside what the other runs
    /// hold, or none holds any, as `Hold::regrow` does. The run keeps its cores.
    pub(super) fn regrow(&mut self, bytes: u64) {
        let pool = self.pool;
        let mut running = pool.running.lock().expect("lock");
        running.bytes -= self.bytes;
        running.holding -= 1;
        pool.changed.notify_all();
        while running.holding > 0 && running.bytes.saturating_add(bytes) > running.share {
            running = pool.changed.wait(running).expect("lock");
        }
        running.bytes += bytes;
        running.holding += 1;
        self.bytes = bytes;
    }
}

impl Drop for Cores<'_> {
    fn drop(&mut self) {
        self.pool.running.lock().expect("lock").cores -= self.cores;
        self.pool.changed.notify_all();
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut running = self.pool.running.lock().expect("lock");
        running.bytes -= self.bytes;
        running.holding -= 1;
        running.cores -= self.cores;
        running.running -= 1;
        drop(running);
        self.pool.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const GIB: u64 = 1 << 30;
    const OVERHEAD: u64 = 256 << 20;

    /// What was running at once, checked each time a run starts or grows.
    #[derive(Default)]
    struct Seen {
        bytes: u64,
        runs: usize,
        most_runs: usize,
        broke: Vec<String>,
    }

    impl Seen {
        fn start(&mut self, bytes: u64, budget: u64, job: usize) {
            self.bytes += bytes;
            self.runs += 1;
            self.most_runs = self.most_runs.max(self.runs);
            if self.bytes > budget && self.runs > 1 {
                self.broke.push(format!(
                    "job {job}: {} bytes in {} runs over {budget}",
                    self.bytes, self.runs
                ));
            }
        }

        fn stop(&mut self, bytes: u64) {
            self.bytes -= bytes;
            self.runs -= 1;
        }
    }

    /// Mixed heaps, one past the budget alone, and runs that grow to twice their heap: what
    /// runs at once never holds more than the budget, unless one run goes alone.
    #[test]
    fn what_runs_at_once_never_passes_the_budget_unless_it_runs_alone() {
        let budget_bytes = 8 * GIB;
        let heaps: Vec<u64> = [4, 1, 1, 2, 1, 9, 1, 4, 1, 1, 2, 1, 1, 3, 1, 1]
            .iter()
            .map(|g| g * GIB)
            .collect();
        let grows = |job: usize| job % 5 == 1;
        let budget = Budget::new(budget_bytes);
        let seen = Mutex::new(Seen::default());
        let order: Vec<usize> = (0..heaps.len()).collect();
        let ran = run_budgeted(
            &order,
            18,
            &budget,
            |job| heaps[job] + OVERHEAD,
            |job, hold, in_flight| {
                let mut bytes = heaps[job] + OVERHEAD;
                seen.lock().unwrap().start(bytes, budget_bytes, job);
                std::thread::sleep(Duration::from_millis(5));
                if grows(job) {
                    seen.lock().unwrap().stop(bytes);
                    bytes = 2 * heaps[job] + OVERHEAD;
                    hold.regrow(bytes);
                    seen.lock().unwrap().start(bytes, budget_bytes, job);
                    std::thread::sleep(Duration::from_millis(5));
                }
                seen.lock().unwrap().stop(bytes);
                in_flight
            },
        );
        let seen = seen.into_inner().unwrap();
        assert!(seen.broke.is_empty(), "{:?}", seen.broke);
        assert!(seen.most_runs >= 2, "{} at once", seen.most_runs);
        assert_eq!(ran.iter().map(|(job, _)| *job).collect::<Vec<_>>(), order);
        assert!(ran
            .iter()
            .all(|(_, in_flight)| (1..=16).contains(in_flight)));
        let held = budget.held.lock().unwrap();
        assert_eq!((held.bytes, held.runs), (0, 0));
    }
}

/// `pick` on the machines of #384 and on a small one, without the machines: what starts, with
/// how many workers, and what waits (docs/design/tlc-scheduling.md, "What the rule gives").
#[cfg(test)]
mod pick_tests {
    use super::*;

    const GIB: u64 = 1 << 30;
    const OVERHEAD: u64 = 256 << 20;

    fn machine(cores: usize) -> Machine {
        Machine {
            cores,
            workers: None,
            jobs: None,
        }
    }

    fn want(bytes: u64, states: Option<u64>, weight: usize) -> Want {
        Want {
            bytes,
            states,
            rank: (bytes, (false, weight)),
        }
    }

    /// A run of a simulated schedule: when it started and ended, and its workers.
    #[derive(Clone, Copy, Debug)]
    struct Run {
        job: usize,
        workers: usize,
        start: u64,
        end: u64,
    }

    /// A job of the simulation: its charge, its states, when BFS counts them (`None`: never),
    /// and its work in worker-seconds, of which a run of `w` workers does `min(w, 16)` a
    /// second.
    struct Sim {
        bytes: u64,
        states: u64,
        counted_at: Option<u64>,
        work: u64,
    }

    /// Runs `jobs` second by second with `pick`, as `Pool` does: BFS ends at `bfs_end`,
    /// when the share grows to `grown`. Checks at each start that the runs going hold no more
    /// cores than the machine has, and no more bytes than the share unless one runs alone.
    fn simulate(m: Machine, (share, grown, bfs_end): (u64, u64, u64), jobs: &[Sim]) -> Vec<Run> {
        let mut waiting: Vec<usize> = (0..jobs.len()).collect();
        let mut going: Vec<Run> = Vec::new();
        let mut done = Vec::new();
        for t in 0.. {
            going.retain(|r| match r.end <= t {
                true => {
                    done.push(*r);
                    false
                }
                false => true,
            });
            if waiting.is_empty() && going.is_empty() {
                break;
            }
            let share = if t >= bfs_end { grown } else { share };
            loop {
                let wants: Vec<Want> = (waiting.iter())
                    .map(|&k| {
                        let known = jobs[k].counted_at.is_some_and(|at| at <= t);
                        want(jobs[k].bytes, known.then_some(jobs[k].states), k)
                    })
                    .collect();
                let held_cores: usize = going.iter().map(|r| r.workers.min(m.cores)).sum();
                let held_bytes: u64 = going.iter().map(|r| jobs[r.job].bytes).sum();
                let free = Free {
                    cores: m.cores.saturating_sub(held_cores),
                    bytes: share.saturating_sub(held_bytes),
                    share,
                    running: going.len(),
                    holding: going.len(),
                    counted: t >= bfs_end,
                };
                let Some((at, workers)) = pick(&wants, &free, &m) else {
                    break;
                };
                let job = waiting.remove(at);
                assert!(
                    going.is_empty() || held_cores + workers.min(m.cores) <= m.cores,
                    "{job} at {t}: {held_cores} + {workers} cores"
                );
                assert!(
                    going.is_empty() || held_bytes + jobs[job].bytes <= share,
                    "{job} at {t}: over the share"
                );
                let secs = jobs[job].work.div_ceil(workers.min(16) as u64).max(1);
                going.push(Run {
                    job,
                    workers,
                    start: t,
                    end: t + secs,
                });
            }
        }
        done.sort_by_key(|r| r.job);
        done
    }

    /// #384 with `RDRA_TLC_HEAP=16g` on 64 cores and a 48 GiB share: 2 runs fit, and the
    /// cores they would leave idle go to them (2 × 32, as 361ccec ran), where `tlc_workers`
    /// gave 2 × 8. The third waits for the bytes.
    #[test]
    fn a_fixed_heap_that_bounds_the_runs_gets_the_idle_cores() {
        let m = machine(64);
        let wants = vec![want(16 * GIB + OVERHEAD, None, 0); 27];
        let mut free = Free {
            cores: 64,
            bytes: 48 * GIB,
            share: 48 * GIB,
            running: 0,
            holding: 0,
            counted: false,
        };
        assert_eq!(pick(&wants, &free, &m), Some((0, 32)));
        free = Free {
            cores: 32,
            bytes: free.bytes - wants[0].bytes,
            running: 1,
            holding: 1,
            ..free
        };
        assert_eq!(pick(&wants[1..], &free, &m), Some((0, 32)));
        free = Free {
            cores: 0,
            bytes: free.bytes - wants[1].bytes,
            running: 2,
            holding: 2,
            ..free
        };
        assert_eq!(pick(&wants[2..], &free, &m), None);
        // Once BFS has ended the share is all of --memory, 96 GiB: 5 runs fit, 12 workers.
        let grown = Free {
            cores: 64,
            bytes: 96 * GIB,
            share: 96 * GIB,
            running: 0,
            holding: 0,
            counted: true,
        };
        assert_eq!(pick(&wants, &grown, &m), Some((0, 12)));
    }

    /// #384 with the heap unset: every module of 126 kB gets 1 GiB, and 38 runs fit. While BFS
    /// has counted none, the first 8 start with 8 workers each and the 9th waits for cores,
    /// where `tlc_workers` started 24 with 2.
    #[test]
    fn uncounted_jobs_start_as_large_ones_and_fill_the_cores() {
        let m = machine(64);
        let wants = vec![want(GIB + OVERHEAD, None, 0); 27];
        let mut free = Free {
            cores: 64,
            bytes: 48 * GIB,
            share: 48 * GIB,
            running: 0,
            holding: 0,
            counted: false,
        };
        for n in 0..8 {
            assert_eq!(pick(&wants[n..], &free, &m), Some((0, 8)), "run {n}");
            free.cores -= 8;
            free.bytes -= wants[n].bytes;
            free.running += 1;
            free.holding += 1;
        }
        assert_eq!(pick(&wants[8..], &free, &m), None);
    }

    /// The head of the order waits for its cores: a counted giant does not start on the 4
    /// cores left, and a small job behind it does not pass it.
    #[test]
    fn the_head_waits_for_its_cores_and_nothing_passes_it() {
        let m = machine(64);
        let wants = [
            want(GIB, Some(100), 1),
            want(GIB, Some(12_800_000), 0),
            want(GIB, None, 2),
        ];
        let free = Free {
            cores: 4,
            bytes: 40 * GIB,
            share: 48 * GIB,
            running: 7,
            holding: 7,
            counted: false,
        };
        assert_eq!(pick(&wants, &free, &m), None);
        let free = Free { cores: 8, ..free };
        assert_eq!(pick(&wants, &free, &m), Some((1, 8)));
        // Without the giant the job BFS has not counted goes before the small one.
        assert_eq!(pick(&[wants[0], wants[2]], &free, &m), Some((1, 8)));
        assert_eq!(pick(&wants[..1], &free, &m), Some((0, 1)));
    }

    /// Workers follow the states BFS counted: 1 per 50,000, up to 8.
    #[test]
    fn counted_jobs_get_workers_by_their_states() {
        let m = machine(64);
        let free = Free {
            cores: 64,
            bytes: 48 * GIB,
            share: 48 * GIB,
            running: 1,
            holding: 1,
            counted: false,
        };
        for (states, workers) in [
            (31, 1),
            (48_000, 1),
            (50_001, 2),
            (203_000, 5),
            (400_000, 8),
            (12_800_000, 8),
        ] {
            let w = want(GIB, Some(states), 0);
            assert_eq!(target(&w, &m, &free, 1), workers, "{states}");
        }
    }

    /// A sample in CI on 4 cores, where BFS counts nothing until its entity pass ends (here at
    /// 5 s): jobs it has not counted share the cores as before #384, 4 at once on one worker
    /// each, and so do the counted small ones after (`MANY_CORES`).
    #[test]
    fn a_small_machine_shares_its_cores_among_uncounted_jobs() {
        let m = machine(4);
        let jobs: Vec<Sim> = (0..15)
            .map(|k| Sim {
                bytes: GIB + OVERHEAD,
                states: 300,
                counted_at: Some(5),
                work: 1 + (k % 3),
            })
            .collect();
        let runs = simulate(m, (8 * GIB, 16 * GIB, 5), &jobs);
        assert_eq!(runs.len(), 15);
        assert!(runs.iter().all(|r| r.workers == 1), "{runs:?}");
        assert_eq!(runs.iter().filter(|r| r.start == 0).count(), 4, "{runs:?}");
        // 30 worker-seconds on 4 cores.
        let end = runs.iter().map(|r| r.end).max().unwrap();
        assert!(end <= 9, "{end}: {runs:?}");
    }

    /// On 16 cores or more a job BFS has not counted starts on the cap, as if it were large.
    #[test]
    fn a_many_core_machine_starts_uncounted_jobs_on_the_cap() {
        let free = |cores: usize| Free {
            cores,
            bytes: 8 * GIB,
            share: 8 * GIB,
            running: 0,
            holding: 0,
            counted: false,
        };
        let w = want(GIB + OVERHEAD, None, 0);
        assert_eq!(target(&w, &machine(16), &free(16), 15), 8);
        // Below 16: the 15 cores among the 6 runs of 1.25 GiB that fit 8 GiB.
        assert_eq!(target(&w, &machine(15), &free(15), 15), 2);
        assert_eq!(target(&w, &machine(4), &free(4), 2), 2);
    }

    /// #384's model on 64 cores: two giants and property cones of 2M to 6.7M states that BFS
    /// counts late, and small cones it counts at once. The large ones start first on the cap,
    /// the small ones fill the cores left on one worker each, the jobs going never hold more
    /// cores than there are, and the last run to end is one with the most workers.
    #[test]
    fn the_largest_jobs_start_with_the_most_workers_and_end_last() {
        let m = machine(64);
        let mut jobs: Vec<Sim> = Vec::new();
        for (states, at) in [(12_800_000, 30), (12_000_000, 30)] {
            jobs.push(Sim {
                bytes: GIB + OVERHEAD,
                states,
                counted_at: Some(at),
                work: states / 2_000,
            });
        }
        for states in [6_700_000, 5_000_000, 4_000_000, 3_000_000, 2_000_000] {
            jobs.push(Sim {
                bytes: GIB + OVERHEAD,
                states,
                counted_at: Some(20),
                work: states / 2_000,
            });
        }
        for k in 0..20 {
            jobs.push(Sim {
                bytes: GIB + OVERHEAD,
                states: 1_000 * (k + 1),
                counted_at: Some(0),
                work: 2,
            });
        }
        let runs = simulate(m, (48 * GIB, 96 * GIB, 40), &jobs);
        assert_eq!(runs.len(), jobs.len());
        for large in &runs[..7] {
            assert_eq!((large.start, large.workers), (0, 8), "{large:?}");
        }
        let last = runs.iter().max_by_key(|r| r.end).unwrap();
        let most = runs.iter().map(|r| r.workers).max().unwrap();
        assert_eq!(last.workers, most, "{last:?}");
        assert!(runs[7..].iter().all(|r| r.workers == 1), "{runs:?}");
    }

    /// The knobs: `--tlc-workers` fixes every run's workers, and `--tlc-jobs` the most runs.
    #[test]
    fn the_knobs_fix_the_workers_and_the_runs_at_once() {
        let m = Machine {
            cores: 64,
            workers: Some(3),
            jobs: Some(2),
        };
        let wants = vec![want(GIB, Some(12_800_000), 0); 5];
        let mut free = Free {
            cores: 64,
            bytes: 48 * GIB,
            share: 48 * GIB,
            running: 0,
            holding: 0,
            counted: false,
        };
        assert_eq!(pick(&wants, &free, &m), Some((0, 3)));
        free.running = 1;
        free.holding = 1;
        free.cores -= 3;
        assert_eq!(pick(&wants, &free, &m), Some((0, 3)));
        free.running = 2;
        assert_eq!(pick(&wants, &free, &m), None);
        // Once BFS has ended a job it never counted shares the cores among the runs left.
        let m = machine(18);
        let free = Free {
            cores: 18,
            bytes: 8 * GIB,
            share: 8 * GIB,
            running: 0,
            holding: 0,
            counted: true,
        };
        assert_eq!(target(&want(GIB, None, 0), &m, &free, 9), 2);
        assert_eq!(target(&want(GIB, None, 0), &m, &free, 1), 8);
    }

    /// `Pool` on threads: what the runs going hold never passes the cores, every job runs
    /// once, and the results come back in job order; a count learned while jobs wait is read.
    #[test]
    fn the_pool_holds_the_cores_and_returns_in_order() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;
        let pool = Pool::new(machine(6), 8 * GIB, 12, false);
        pool.learn((0..12).map(|k| (k, 100 * (k as u64 + 1))));
        let held = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let jobs: Vec<usize> = (0..12).rev().collect();
        let ran = pool.run(
            &jobs,
            |job, _, _| (GIB, (GIB, (false, job))),
            |started, slot| {
                let now = held.fetch_add(started.workers, Ordering::SeqCst) + started.workers;
                most.fetch_max(now, Ordering::SeqCst);
                if started.job == 3 {
                    slot.regrow(2 * GIB);
                }
                std::thread::sleep(Duration::from_millis(5));
                held.fetch_sub(started.workers, Ordering::SeqCst);
                (started.job, started.workers)
            },
        );
        assert_eq!(ran.iter().map(|r| r.0).collect::<Vec<_>>(), jobs);
        assert!(ran.iter().all(|r| r.1 == 1), "{ran:?}");
        assert!(most.load(Ordering::SeqCst) <= 6);
        let running = pool.running.lock().unwrap();
        assert_eq!(
            (
                running.bytes,
                running.holding,
                running.cores,
                running.running
            ),
            (0, 0, 0, 0)
        );
    }

    /// Cores `--prove`'s z3 processes hold are taken for the runs that start meanwhile: on 6
    /// cores with 4 held, two one-worker runs go at once, not six; the cores come back when the
    /// hold drops (#421).
    #[test]
    fn cores_held_beside_the_pool_are_left_free() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;
        let pool = Pool::new(machine(6), 8 * GIB, 6, false);
        pool.learn((0..6).map(|k| (k, 100)));
        let held = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let jobs: Vec<usize> = (0..6).collect();
        let z3 = pool.hold_cores(4);
        pool.run(
            &jobs,
            |_, _, _| (GIB, (GIB, (false, 0))),
            |started, _| {
                let now = held.fetch_add(started.workers, Ordering::SeqCst) + started.workers;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                held.fetch_sub(started.workers, Ordering::SeqCst);
            },
        );
        assert!(most.load(Ordering::SeqCst) <= 2, "{most:?}");
        drop(z3);
        assert_eq!(pool.running.lock().unwrap().cores, 0);
        // More than the machine has holds all of it, and gives back as much.
        let all = pool.hold_cores(100);
        assert_eq!(pool.running.lock().unwrap().cores, 6);
        drop(all);
        assert_eq!(pool.running.lock().unwrap().cores, 0);
    }
}
