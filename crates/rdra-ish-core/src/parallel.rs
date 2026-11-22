//! Ordered parallel map over a bounded number of scoped threads.

/// How many threads parallel work uses: `RDRA_JOBS`, else the machine's cores, at most 8.
/// Each lifecycle cone holds its own reached states, so memory grows with the number.
pub fn jobs() -> usize {
    std::env::var("RDRA_JOBS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get().min(8)))
}

/// Threads that help a run beyond the one it runs on, taken from one budget for the whole
/// process: `jobs()` less one. Runs that are themselves side by side (`in_parallel`) would
/// otherwise start `jobs()` helpers each. Dropping the value gives them back.
pub(crate) struct Helpers(usize);

static HELPING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Helpers {
    /// Every helper the budget has left, which may be none.
    pub(crate) fn take() -> Helpers {
        use std::sync::atomic::Ordering;
        let budget = jobs().saturating_sub(1);
        let mut held = HELPING.load(Ordering::Relaxed);
        loop {
            let free = budget.saturating_sub(held);
            if free == 0 {
                return Helpers(0);
            }
            match HELPING.compare_exchange(held, held + free, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return Helpers(free),
                Err(now) => held = now,
            }
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.0
    }
}

impl Drop for Helpers {
    fn drop(&mut self) {
        HELPING.fetch_sub(self.0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// `f` of each item, in item order, on up to `threads` threads. One thread, or one item,
/// runs on the caller's.
pub fn in_parallel<T: Send, R: Send>(
    items: Vec<T>,
    threads: usize,
    f: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let threads = threads.min(items.len());
    if threads <= 1 {
        return items.into_iter().map(f).collect();
    }
    let slots: Vec<Mutex<Option<T>>> = items.into_iter().map(|t| Mutex::new(Some(t))).collect();
    let results: Vec<Mutex<Option<R>>> = slots.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(slot) = slots.get(i) else { break };
                let item = slot
                    .lock()
                    .expect("an item slot")
                    .take()
                    .expect("taken once");
                *results[i].lock().expect("a result slot") = Some(f(item));
            });
        }
    });
    results
        .into_iter()
        .map(|r| {
            r.into_inner()
                .expect("a result slot")
                .expect("every item ran")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::in_parallel;

    /// The results come back in item order whatever the thread count, and each item runs
    /// once.
    #[test]
    fn results_keep_the_item_order_on_any_thread_count() {
        let items: Vec<u64> = (0..200).collect();
        let slow_square = |n: u64| {
            // Later items finish first on several threads.
            std::thread::sleep(std::time::Duration::from_micros(200 - n));
            n * n
        };
        let alone = in_parallel(items.clone(), 1, slow_square);
        for threads in [2, 4, 16, 500] {
            assert_eq!(in_parallel(items.clone(), threads, slow_square), alone);
        }
        assert_eq!(alone[7], 49);
        assert!(in_parallel(Vec::<u64>::new(), 4, slow_square).is_empty());
    }
}
