//! Quotients shared across the cones of one command (#252). Many cones of `verify` have the
//! same extended sub-lifecycle, and its quotient is most of what reducing a cone costs: the
//! cache explores and partitions each one once, and every cone maps the classes into its
//! own ids (`Quotient::of`).
//!
//! The key is everything `Classes::of` reads: the extended sub-lifecycle (`TransitionSystem`
//! compared field by field), its `obs`, inputs and rises, and the caps that can stop its
//! runs (patterns, states, stopping once every check failed, and whether a pool is charged).
//! Two keys are equal only when `==` says so; nothing is matched by a hash alone.
//!
//! The memory limit is the one input not in the key, and a deadline is not either, so caps
//! with one bypass the cache. An entry is stored only when no cap and no limit stopped a run
//! on the way, so it is what `Classes::of` computes under any room at least `need`, the most
//! bytes one of its runs asked its pool for. A hit asks the caller's pool for `need` on the
//! caller's thread and gives it back, as a run of that size would: it sees the same refusal
//! and the same peak (`memory::watch`) as computing would, and a refusal computes after all.
//!
//! An entry stays charged to the command's pool while the cache holds it, in the bytes its
//! tables take (`Classes::bytes`), and never to a room-limited child pool, so a cone that
//! happened to fill it loses no room for its own run. What the cache holds is the same on any
//! number of threads wherever it can change a result: `verify` reduces the safety and the
//! property cones one at a time in cone order, so their entries come and are charged in that
//! order; the entity pass reduces side by side, but there a run's result is the one it has
//! alone in its room in cone order (`explore_targets_with`), which a hit or an entry held
//! beside it does not change; and the cache is dropped when that pass, the last to reduce,
//! ends, before the runs that follow.

use std::collections::{BTreeMap, HashMap};
use std::mem::size_of;
use std::sync::{Arc, Mutex, PoisonError};

use super::extended::Extended;
use super::partition::{Classes, Classified};
use super::{ClassMove, Reduction};
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::Caps;
use crate::lifecycle::TransitionSystem;
use crate::memory::Charge;

/// The quotients of one command's cones, by extended sub-lifecycle and caps. With
/// `RDRA_ISH_NO_QUOTIENT_CACHE` set it holds nothing and every cone computes its own, for
/// comparing the two.
pub struct QuotientCache {
    slots: Mutex<Vec<Arc<Slot>>>,
    off: bool,
    stats: Mutex<Stats>,
}

/// What a cache did, for `RDRA_ISH_QUOTIENT_STATS`.
#[derive(Default)]
struct Stats {
    hits: usize,
    computed: usize,
    stored: usize,
    bytes: u64,
}

/// One key and, once a computation of it finished clean, its entry. A cone that asks for a
/// key another cone is computing waits for it.
struct Slot {
    key: Key,
    entry: Mutex<Option<Entry>>,
}

/// Everything `Classes::of` reads but the memory room.
struct Key {
    system: TransitionSystem,
    obs: VarSet,
    inputs: VarSet,
    rise: Vec<bool>,
    patterns: usize,
    states: usize,
    stop_when_all_violated: bool,
    charged: bool,
}

impl Key {
    fn of(ext: &Extended, caps: Caps) -> Self {
        Key {
            system: ext.system.clone(),
            obs: ext.obs_local.clone(),
            inputs: ext.inputs_local.clone(),
            rise: ext.rise.clone(),
            patterns: caps.patterns,
            states: caps.states,
            stop_when_all_violated: caps.stop_when_all_violated,
            charged: caps.memory.is_some(),
        }
    }

    fn is(&self, ext: &Extended, caps: Caps) -> bool {
        let Key {
            system,
            obs,
            inputs,
            rise,
            patterns,
            states,
            stop_when_all_violated,
            charged,
        } = self;
        *patterns == caps.patterns
            && *states == caps.states
            && *stop_when_all_violated == caps.stop_when_all_violated
            && *charged == caps.memory.is_some()
            && *rise == ext.rise
            && *obs == ext.obs_local
            && *inputs == ext.inputs_local
            && *system == ext.system
    }
}

/// A clean computation's result.
struct Entry {
    classes: Classified,
    /// The most bytes a run of the computation asked its pool to cover.
    need: u64,
    /// `classes`, charged to the command's pool while the cache holds it.
    _charge: Option<Charge>,
}

impl Default for QuotientCache {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotientCache {
    pub fn new() -> Self {
        QuotientCache {
            slots: Mutex::new(Vec::new()),
            off: std::env::var_os("RDRA_ISH_NO_QUOTIENT_CACHE").is_some(),
            stats: Mutex::new(Stats::default()),
        }
    }

    /// The classes of `ext` under `caps`, and whether they came out clean: the entry when one
    /// is stored and its runs fit the caller's pool, else `compute()`, stored when clean. Caps
    /// with a deadline bypass the cache: whether a run beats the clock is not in the key.
    pub(super) fn classes(
        &self,
        ext: &Extended,
        caps: Caps,
        compute: impl FnOnce() -> (Classified, bool),
    ) -> (Classified, bool) {
        if self.off || caps.deadline.is_some() {
            return compute();
        }
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
            match slots.iter().find(|s| s.key.is(ext, caps)) {
                Some(slot) => Arc::clone(slot),
                None => {
                    let slot = Arc::new(Slot {
                        key: Key::of(ext, caps),
                        entry: Mutex::new(None),
                    });
                    slots.push(Arc::clone(&slot));
                    slot
                }
            }
        };
        // Held while computing, so a cone that asks meanwhile waits for the entry. A nested
        // key is a smaller system, so the locks are taken in one order.
        let mut entry = slot.entry.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(e) = entry.as_ref().filter(|e| fits(e.need, caps)) {
            self.note(|s| s.hits += 1);
            return (e.classes.clone(), true);
        }
        let ((classes, clean), seen) = crate::memory::watch(compute);
        self.note(|s| s.computed += 1);
        if clean && entry.is_none() {
            let bytes = match &classes {
                Classified::Classes(c) => c.bytes(),
                Classified::Nothing | Classified::TooLarge => 0,
            };
            let charge = caps.memory.map(|pool| Charge::hold(pool.root(), bytes));
            // A pool too full to hold the entry leaves it out; the result is the same.
            if !matches!(charge, Some(None)) {
                self.note(|s| {
                    s.stored += 1;
                    s.bytes += bytes;
                });
                *entry = Some(Entry {
                    classes: classes.clone(),
                    need: seen.peak,
                    _charge: charge.flatten(),
                });
            }
        }
        (classes, clean)
    }

    fn note(&self, f: impl FnOnce(&mut Stats)) {
        f(&mut self.stats.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl Drop for QuotientCache {
    fn drop(&mut self) {
        if std::env::var_os("RDRA_ISH_QUOTIENT_STATS").is_some() {
            let s = self.stats.get_mut().unwrap_or_else(PoisonError::into_inner);
            eprintln!(
                "quotient-cache: keys={} computed={} hits={} stored={} bytes={}",
                self.slots
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len(),
                s.computed,
                s.hits,
                s.stored,
                s.bytes
            );
        }
    }
}

/// Whether runs that asked for `need` bytes fit the pool `caps` charges now: the pool is
/// asked for `need` on this thread and given it back, as such a run would be, so `watch`
/// sees what computing would show. With no pool everything fits.
fn fits(need: u64, caps: Caps) -> bool {
    caps.memory.is_none_or(|pool| Charge::new(pool).cover(need))
}

/// A heap block for `n` bytes: the allocator rounds up to 16.
fn block(n: usize) -> u64 {
    n.div_ceil(16) as u64 * 16
}

/// A hash map of `n` entries of `entry` bytes: its buckets (`n * 8 / 7` rounded up to a
/// power of two, 4 at the least), a control byte per bucket and a group of 16 more.
fn map_bytes(n: usize, entry: usize) -> u64 {
    let buckets = (n * 8).div_ceil(7).max(4).next_power_of_two() as u64;
    buckets * (entry as u64 + 1) + 16
}

/// What a quotient's tables take, counted from their lengths as `store_bytes` counts a run:
/// each boxed key once per table that holds it, the maps' buckets, the move lists, the rises
/// at half again their entries for the B-tree's slack, the loop marks, and an inner
/// reduction's tables.
#[allow(clippy::too_many_arguments)]
fn tables_bytes(
    reps: &[Box<[u8]>],
    class_of: &HashMap<Box<[u8]>, u32>,
    init: &HashMap<Box<[u8]>, u32>,
    moves: &[Vec<ClassMove>],
    rises: &BTreeMap<(u32, Box<[u8]>), u32>,
    loops: &[bool],
    inner: Option<&Reduction>,
) -> u64 {
    let boxes =
        |keys: &mut dyn Iterator<Item = &Box<[u8]>>| keys.map(|k| block(k.len())).sum::<u64>();
    let boxes_of: u64 = boxes(&mut reps.iter())
        + boxes(&mut class_of.keys())
        + boxes(&mut init.keys())
        + boxes(&mut moves.iter().flatten().map(|(_, p, _)| p))
        + boxes(&mut rises.keys().map(|(_, k)| k));
    let entry = size_of::<(Box<[u8]>, u32)>();
    let rise = size_of::<((u32, Box<[u8]>), u32)>() as u64;
    boxes_of
        + std::mem::size_of_val(reps) as u64
        + map_bytes(class_of.len(), entry)
        + map_bytes(init.len(), entry)
        + moves
            .iter()
            .map(|m| (size_of::<Vec<ClassMove>>() + m.len() * size_of::<ClassMove>()) as u64)
            .sum::<u64>()
        + rises.len() as u64 * rise * 3 / 2
        + loops.len() as u64
        + inner.map_or(0, Reduction::tables_bytes)
}

impl Classes {
    /// What the cache holds for these classes, in the bytes the memory pool counts.
    pub(super) fn bytes(&self) -> u64 {
        tables_bytes(
            &self.reps,
            &self.class_of,
            &self.init,
            &self.moves,
            &self.rises,
            &self.loops,
            self.inner.as_deref(),
        )
    }
}

impl Reduction {
    /// What this reduction's quotient tables take, as `Classes::bytes` counts them.
    fn tables_bytes(&self) -> u64 {
        tables_bytes(
            &self.reps,
            &self.class_of,
            &self.init,
            &self.moves,
            &self.rises,
            &self.loops,
            self.inner.as_deref(),
        )
    }
}

#[cfg(test)]
impl QuotientCache {
    /// Lookups an entry answered.
    pub(crate) fn hits(&self) -> usize {
        self.stats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hits
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::lifecycle::reduce::partition::tests::model;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::types::Value;
    use crate::lifecycle::{Bounds, Scope};
    use crate::memory::{MemoryPool, MemorySize};

    /// The extended sub-lifecycle of the job's cone: the gate and the key behind it.
    fn job_ext(key_cycles: bool) -> Extended {
        let model = model_from(&model(key_cycles));
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let (sys, seeds) = ts.cone_system(&ts.property_cone(0), [ts.property_seed(0)]);
        let split = sys.split_with(&seeds, true);
        let ext = Extended::of(&sys, &split);
        assert!(!ext.obs_local.is_empty(), "the job reads the gate");
        ext
    }

    fn cache() -> QuotientCache {
        QuotientCache {
            slots: Mutex::new(Vec::new()),
            off: false,
            stats: Mutex::new(Stats::default()),
        }
    }

    fn quotient(c: Classified) -> Arc<Classes> {
        match c {
            Classified::Classes(c) => c,
            Classified::Nothing | Classified::TooLarge => panic!("a quotient"),
        }
    }

    /// Asks `cache` for `ext` under `caps`; true when it computed.
    fn computes(cache: &QuotientCache, ext: &Extended, caps: Caps) -> bool {
        let ran = Cell::new(false);
        cache.classes(ext, caps, || {
            ran.set(true);
            Classes::of(ext, caps, cache)
        });
        ran.get()
    }

    /// Two equal extended sub-lifecycles share one quotient, and what the second gets is what
    /// computing gives.
    #[test]
    fn an_equal_sub_lifecycle_takes_the_stored_quotient() {
        let cache = cache();
        let (a, b) = (job_ext(true), job_ext(true));
        assert!(computes(&cache, &a, Caps::VERIFY));
        assert!(!computes(&cache, &b, Caps::VERIFY), "an equal key is a hit");
        let (hit, clean) = cache.classes(&b, Caps::VERIFY, || unreachable!());
        let (fresh, _) = Classes::of(&b, Caps::VERIFY, &QuotientCache::new());
        let (hit, fresh) = (quotient(hit), quotient(fresh));
        assert!(clean);
        assert_eq!(hit.reps, fresh.reps);
        assert_eq!(hit.class_of, fresh.class_of);
        assert_eq!(hit.init, fresh.init);
        assert_eq!(hit.moves, fresh.moves);
        assert_eq!(hit.rises, fresh.rises);
        assert_eq!(hit.loops, fresh.loops);
        assert_eq!(hit.inner, fresh.inner);
        assert_eq!(hit.states, fresh.states);
        assert_eq!(hit.divergence, fresh.divergence);
    }

    /// A sub-lifecycle that differs from a stored one in one input of the quotient (its
    /// system, `obs`, inputs, rises, or a cap that can stop its runs) computes its own.
    #[test]
    fn a_sub_lifecycle_that_differs_in_one_input_computes_its_own() {
        let cache = cache();
        let ext = job_ext(true);
        assert!(computes(&cache, &ext, Caps::VERIFY));
        let mut near: Vec<(&str, Extended, Caps)> = Vec::new();
        let other = job_ext(false);
        assert!(other.system != ext.system);
        near.push(("a key that only rises", other, Caps::VERIFY));
        let mut e = job_ext(true);
        let v = e.system.vars.len() - 1;
        e.system.vars[v].ends.push(Value(1));
        near.push(("one var's ends", e, Caps::VERIFY));
        let mut e = job_ext(true);
        let g = e.system.actions.iter().position(|a| !a.guards.is_empty());
        e.system.actions[g.expect("a guarded action")].guards.pop();
        near.push(("one guard fewer", e, Caps::VERIFY));
        let mut e = job_ext(true);
        let extra = e.system.var_ids().find(|v| !e.obs_local.contains(v));
        e.obs_local.insert(extra.expect("an unobserved var"));
        near.push(("one more var observed", e, Caps::VERIFY));
        let mut e = job_ext(true);
        let extra = e.system.var_ids().find(|v| !e.inputs_local.contains(v));
        e.inputs_local
            .insert(extra.expect("a var that is no input"));
        near.push(("one more input", e, Caps::VERIFY));
        let mut e = job_ext(true);
        e.rise[0] = !e.rise[0];
        near.push(("one rise flag", e, Caps::VERIFY));
        let caps = Caps::VERIFY;
        let patterns = Caps {
            patterns: caps.patterns - 1,
            ..caps
        };
        near.push(("the pattern cap", job_ext(true), patterns));
        let states = Caps {
            states: 1 << 20,
            ..caps
        };
        near.push(("the state cap", job_ext(true), states));
        let failed = caps.until_all_violated();
        near.push(("stopping at failed checks", job_ext(true), failed));
        let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
        near.push(("a charged pool", job_ext(true), caps.charged_to(pool)));
        for (what, e, caps) in &near {
            let ran = Cell::new(false);
            cache.classes(e, *caps, || {
                ran.set(true);
                (Classified::Nothing, false)
            });
            assert!(ran.get(), "{what}: a near miss computes");
        }
        assert!(
            !computes(&cache, &ext, Caps::VERIFY),
            "the stored key still hits"
        );
    }

    /// A stored quotient answers a pool only when its runs fit the pool's room: in a room one
    /// byte short of what they asked for, the cone computes and stops as it would alone. The
    /// entry stays charged to the command's pool while the cache holds it.
    #[test]
    fn a_stored_quotient_answers_only_a_room_its_runs_fit() {
        let cache = cache();
        let ext = job_ext(true);
        let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
        let caps = Caps::VERIFY.charged_to(pool);
        assert!(computes(&cache, &ext, caps));
        let need = {
            let slots = cache.slots.lock().unwrap();
            let entry = slots[slots.len() - 1].entry.lock().unwrap();
            entry.as_ref().expect("a clean quotient is stored").need
        };
        assert!(need > 0, "its runs charged the pool");
        let stored = Classes::of(&ext, Caps::VERIFY, &QuotientCache::new()).0;
        let stored = quotient(stored).bytes();
        assert_eq!(pool.held(), stored, "the entry is charged, and only it");
        let short = Caps::VERIFY.charged_to(pool.with_room(need - 1));
        let ran = Cell::new(false);
        let (_, clean) = cache.classes(&ext, short, || {
            ran.set(true);
            Classes::of(&ext, short, &QuotientCache::new())
        });
        assert!(ran.get(), "a room short of `need` computes");
        assert!(!clean, "and a run stops at the limit");
        let exact = Caps::VERIFY.charged_to(pool.with_room(need));
        let ((_, clean), seen) =
            crate::memory::watch(|| cache.classes(&ext, exact, || unreachable!()));
        assert!(clean, "a room of `need` takes the entry");
        assert_eq!(seen.peak, need, "a hit shows the peak computing shows");
        assert!(!seen.crowded);
        drop(cache);
        assert_eq!(pool.held(), 0, "dropping the cache gives the entry back");
    }
}
