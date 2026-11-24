//! The TLC runs of one kind share a budget of bytes: each holds its `-Xmx` and
//! `JVM_OVERHEAD` (`TlcHeap::charge`) while it goes, and a run starts once what the running
//! ones hold and its own charge fit the budget, or when nothing else runs, so a run whose
//! heap alone is past the budget still runs, alone (#309).

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

    /// What the runs going at once may hold.
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
