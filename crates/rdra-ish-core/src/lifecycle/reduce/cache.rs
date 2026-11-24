//! Quotients shared across the cones of one command (#252). Many cones of `verify` have the
//! same extended sub-lifecycle, and its quotient is most of what reducing a cone costs: the
//! cache explores and partitions each one once, and every cone maps the classes into its
//! own ids (`Quotient::of`).
//!
//! The key is everything `Classes::of` reads: the extended sub-lifecycle (`TransitionSystem`
//! compared field by field, its bounds and each entity's slots included), its `obs`, inputs
//! and rises, and the caps that can stop its runs (states, stopping once every check failed,
//! and whether a pool is charged). The pattern cap is not in it: the run of S gathers no
//! view, so that cap cannot stop it (#251, #296).
//! Two keys are equal only when `==` says so; nothing is matched by a hash alone.
//!
//! The memory limit is the one input not in the key, and a deadline is not either, so caps
//! with one bypass the cache. An entry is stored only when no cap and no limit stopped a run
//! on the way, so it is what `Classes::of` computes under any room at least `need`, the most
//! bytes one of its runs asked its pool for. A hit asks the caller's pool for `need` on the
//! caller's thread and gives it back, as a run of that size would: it sees the same refusal
//! and the same peak (`memory::watch`) as computing would, and a refusal computes after all.
//!
//! A declined computation is stored too (#289): one whose nested reduction was clean and
//! whose run of S then stopped at the memory limit on a request past the room of the pool it
//! charged (`Ended::Declined`), so that no other run's bytes decided where it stopped. Each
//! request its runs were granted was at most `need`, and the one refused was more than that
//! room. In a pool whose room is no larger and which covers `need` now, every granted request
//! is granted again and the refused one is refused again, so the runs take the same steps:
//! the computation stops at the same request, unclean, with the same peak and no crowding. A
//! hit on a declined entry asks for exactly that: the caller's pool's room at most the room
//! the entry was declined in, then `need` as for a clean entry. It answers what computing
//! answers, `Classified::Nothing` and not clean, and never stands for a clean computation: a
//! larger room computes, and a clean result there replaces the declined entry. A computation
//! stopped any other way (a crowded pool, a cap, a nested run that stopped) is not stored.
//!
//! The run of S is shared one level down (#296): keys that differ in `obs`, inputs or rises
//! can explore the same sub-lifecycle, once the nested reduction is done (`Classes::of`'s
//! `sub`). A complete run is held by the sub-lifecycle and the caps that can stop it, and
//! the next key over them partitions it instead of exploring it again (`run_of_s`). Two
//! sub-lifecycles share a run when they are equal but for the `source` of their actions,
//! which names where an action came from and which no run reads (`same_runs`). Only a
//! complete run is held, so a run another key stopped at a cap or the memory limit is never
//! shared: what #289 stores of a declined computation stays with its own key. A held run
//! asked for `need` bytes on the way, its peak, and a later key takes it only when its pool
//! covers `need` now, as for a clean entry; else the held run is dropped and the key runs S
//! itself. A held run is charged to the command's pool, in the bytes it keeps, as an entry
//! is.
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
use super::partition::{Classes, Classified, Ended};
use super::{ClassMove, Reduction};
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::{Caps, ComponentRun};
use crate::lifecycle::{Action, TransitionSystem};
use crate::memory::{watch, Charge, Watched};

/// The quotients of one command's cones, by extended sub-lifecycle and caps. With
/// `RDRA_ISH_NO_QUOTIENT_CACHE` set it holds nothing and every cone computes its own, for
/// comparing the two.
pub struct QuotientCache {
    slots: Mutex<Vec<Arc<Slot>>>,
    /// The complete runs of S, by sub-lifecycle and caps (`run_of_s`).
    runs: Mutex<Vec<Arc<RunSlot>>>,
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
    /// Runs of S held, and runs another key took.
    runs_held: usize,
    run_hits: usize,
    run_bytes: u64,
}

/// A sub-lifecycle and caps, and the complete run of S held for them.
struct RunSlot {
    key: RunKey,
    run: Mutex<Option<HeldRun>>,
}

/// What a run of S reads: the sub-lifecycle, and the caps that can stop it. The memory room
/// is not in it, and the pattern cap is not either, as for `Key`.
struct RunKey {
    system: TransitionSystem,
    states: usize,
    stop_when_all_violated: bool,
    charged: bool,
}

impl RunKey {
    fn of(system: &TransitionSystem, caps: Caps) -> Self {
        RunKey {
            system: system.clone(),
            states: caps.states,
            stop_when_all_violated: caps.stop_when_all_violated,
            charged: caps.memory.is_some(),
        }
    }

    fn is(&self, system: &TransitionSystem, caps: Caps) -> bool {
        let RunKey {
            system: held,
            states,
            stop_when_all_violated,
            charged,
        } = self;
        *states == caps.states
            && *stop_when_all_violated == caps.stop_when_all_violated
            && *charged == caps.memory.is_some()
            && same_runs(held, system)
    }
}

/// Whether a run of `a` is a run of `b`: the systems are equal field by field but for the
/// `source` of each action, the action it was restricted from, which no run reads. Keys over
/// one extended sub-lifecycle restricted from two cones can differ there alone.
fn same_runs(a: &TransitionSystem, b: &TransitionSystem) -> bool {
    let TransitionSystem {
        bounds,
        entities,
        vars,
        actions,
        invariants,
        after,
        properties,
        reach,
        now,
        offsets,
        width,
        api_ids,
        reduction,
        plans,
    } = a;
    let same_action = |(x, y): (&Action, &Action)| {
        let Action {
            name,
            source: _,
            origin,
            site,
            params,
            guards,
            effects,
        } = x;
        *name == y.name
            && *origin == y.origin
            && *site == y.site
            && *params == y.params
            && *guards == y.guards
            && *effects == y.effects
    };
    *width == b.width
        && *offsets == b.offsets
        && *vars == b.vars
        && actions.len() == b.actions.len()
        && actions.iter().zip(&b.actions).all(same_action)
        && *bounds == b.bounds
        && *entities == b.entities
        && *invariants == b.invariants
        && *after == b.after
        && *properties == b.properties
        && *reach == b.reach
        && *now == b.now
        && *api_ids == b.api_ids
        && *reduction == b.reduction
        && *plans == b.plans
}

/// A complete run of S, charged to the command's pool while the cache holds it.
struct HeldRun {
    run: Arc<ComponentRun>,
    /// The most bytes the run asked its pool to cover.
    need: u64,
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
            states,
            stop_when_all_violated,
            charged,
        } = self;
        *states == caps.states
            && *stop_when_all_violated == caps.stop_when_all_violated
            && *charged == caps.memory.is_some()
            && *rise == ext.rise
            && *obs == ext.obs_local
            && *inputs == ext.inputs_local
            && *system == ext.system
    }
}

/// A clean or declined computation's result.
struct Entry {
    classes: Classified,
    /// The most bytes a run of the computation asked its pool to cover.
    need: u64,
    /// For a declined computation, the room of the pool it charged, which the request it was
    /// refused passed; `None` for a clean one.
    declined_in: Option<u64>,
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
            runs: Mutex::new(Vec::new()),
            off: std::env::var_os("RDRA_ISH_NO_QUOTIENT_CACHE").is_some(),
            stats: Mutex::new(Stats::default()),
        }
    }

    /// The run of S of `sub` under `caps`, and what `watch` saw of it: the run another key
    /// held when its pool covers the run's `need` now, else `run()`, held when complete. A
    /// hit asks the pool for `need` as the run would, so `watch` sees what running shows.
    /// Caps with a deadline bypass the holding, as they bypass the entries.
    ///
    /// A `shadow` build holds each hit to its reference twin: a fresh run of `sub`.
    pub(super) fn run_of_s(
        &self,
        sub: &TransitionSystem,
        caps: Caps,
        run: impl Fn() -> ComponentRun,
    ) -> (Arc<ComponentRun>, Watched) {
        if self.off || caps.deadline.is_some() {
            let (run, seen) = watch(run);
            return (Arc::new(run), seen);
        }
        let slot = {
            let mut slots = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
            match slots.iter().find(|s| s.key.is(sub, caps)) {
                Some(slot) => Arc::clone(slot),
                None => {
                    let slot = Arc::new(RunSlot {
                        key: RunKey::of(sub, caps),
                        run: Mutex::new(None),
                    });
                    slots.push(Arc::clone(&slot));
                    slot
                }
            }
        };
        // Held while running, so a key that asks meanwhile waits for the run. A run of S
        // runs no reduction, so no other lock of the cache is taken under it.
        let mut held = slot.run.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(h) = held.as_ref() {
            let (fits, seen) = watch(|| fits(h.need, caps));
            if fits {
                self.note(|s| s.run_hits += 1);
                #[cfg(feature = "shadow")]
                {
                    let fresh = run();
                    if !fresh.complete {
                        crate::shadow::skipped(crate::shadow::Twin::SharedRun);
                    } else {
                        assert!(
                            fresh.same_graph(&h.run),
                            "shadow: QuotientCache::run_of_s: the run of S another key held ({} states) is not a fresh run of the sub-lifecycle of {:?} ({} states)",
                            h.run.states.len(),
                            sub.entities.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
                            fresh.states.len(),
                        );
                        crate::shadow::compared(crate::shadow::Twin::SharedRun);
                    }
                }
                return (Arc::clone(&h.run), seen);
            }
            // The run does not fit the pool now: give its bytes back before running alone.
            *held = None;
        }
        let (mut fresh, seen) = watch(run);
        if fresh.complete {
            // Moved to the command's pool, as an entry is; a pool too full to hold it leaves
            // the run unheld, and the result is the same.
            let charged = match caps.memory {
                Some(pool) => fresh.charge_to(pool.root()),
                None => true,
            };
            if charged {
                let fresh = Arc::new(fresh);
                self.note(|s| {
                    s.runs_held += 1;
                    s.run_bytes += fresh.kept;
                });
                *held = Some(HeldRun {
                    run: Arc::clone(&fresh),
                    need: seen.peak,
                });
                return (fresh, seen);
            }
        }
        (Arc::new(fresh), seen)
    }

    /// The classes of `ext` under `caps`, and how their runs ended: the entry when one is
    /// stored and answers the caller's pool (`Entry::answers`), else `compute()`, stored when
    /// clean or declined. Caps with a deadline bypass the cache: whether a run beats the clock
    /// is not in the key. `compute` is given the cache its nested reductions go through.
    ///
    /// A `shadow` build holds each hit to its reference twin: the classes computed afresh,
    /// through a cache that holds nothing.
    pub(super) fn classes(
        &self,
        ext: &Extended,
        caps: Caps,
        compute: impl FnOnce(&QuotientCache) -> (Classified, Ended),
    ) -> (Classified, Ended) {
        if self.off || caps.deadline.is_some() {
            return compute(self);
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
        if let Some(e) = entry.as_ref().filter(|e| e.answers(caps)) {
            self.note(|s| s.hits += 1);
            let hit = (e.classes.clone(), e.ended());
            // The cache's own tests ask for a hit with a `compute` that must not run.
            #[cfg(all(feature = "shadow", not(test)))]
            {
                let ((fresh, ended), seen) =
                    crate::memory::watch(|| compute(&QuotientCache::off()));
                // A recomputation that a limit stopped says nothing of a clean entry, and one
                // that met a pool others crowded says nothing of a declined one.
                let telling = match e.declined_in {
                    None => ended.is_clean(),
                    Some(_) => !seen.crowded,
                };
                if !telling {
                    crate::shadow::skipped(crate::shadow::Twin::CacheHit);
                } else {
                    assert!(
                        fresh == hit.0 && ended.is_clean() == hit.1.is_clean(),
                        "shadow: QuotientCache::classes: the entry a hit took for the sub-lifecycle of {:?} ({:?}) is not what Classes::of computes afresh ({ended:?})",
                        ext.system
                            .entities
                            .iter()
                            .map(|e| e.id.as_str())
                            .collect::<Vec<_>>(),
                        hit.1,
                    );
                    crate::shadow::compared(crate::shadow::Twin::CacheHit);
                }
            }
            return hit;
        }
        let ((classes, ended), seen) = crate::memory::watch(|| compute(self));
        self.note(|s| s.computed += 1);
        // A clean result replaces a declined one, which a larger room computed past.
        let store = match ended {
            Ended::Clean => entry.as_ref().is_none_or(|e| e.declined_in.is_some()),
            Ended::Declined => entry.is_none(),
            Ended::Stopped => false,
        };
        if store {
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
                    declined_in: match ended {
                        Ended::Declined => caps.memory.map(|pool| pool.room()),
                        Ended::Clean | Ended::Stopped => None,
                    },
                    _charge: charge.flatten(),
                });
            }
        }
        (classes, ended)
    }

    /// A cache that holds nothing: every cone computes its own.
    #[cfg(all(feature = "shadow", not(test)))]
    fn off() -> Self {
        QuotientCache {
            slots: Mutex::new(Vec::new()),
            runs: Mutex::new(Vec::new()),
            off: true,
            stats: Mutex::new(Stats::default()),
        }
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
                "quotient-cache: keys={} computed={} hits={} stored={} bytes={} runs_held={} run_hits={} run_bytes={}",
                self.slots
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len(),
                s.computed,
                s.hits,
                s.stored,
                s.bytes,
                s.runs_held,
                s.run_hits,
                s.run_bytes
            );
        }
    }
}

impl Entry {
    /// Whether the entry is what computing would give under `caps`: a declined one only in a
    /// pool of no more room than it was declined in, and either only when its runs fit the
    /// pool now (`fits`, asked last, as it is what a computation's runs would ask).
    fn answers(&self, caps: Caps) -> bool {
        let room = |room: u64| caps.memory.is_some_and(|pool| pool.room() <= room);
        self.declined_in.is_none_or(room) && fits(self.need, caps)
    }

    /// How the computation the entry stands for ended.
    fn ended(&self) -> Ended {
        match self.declined_in {
            None => Ended::Clean,
            Some(_) => Ended::Declined,
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
    use std::num::NonZeroU8;

    use super::*;
    use crate::lifecycle::reduce::partition::tests::model;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::types::Value;
    use crate::lifecycle::{Bounds, Scope};
    use crate::memory::{MemoryPool, MemorySize, Watched};

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
            runs: Mutex::new(Vec::new()),
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
        cache.classes(ext, caps, |_| {
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
        let (hit, clean) = cache.classes(&b, Caps::VERIFY, |_| unreachable!());
        let (fresh, _) = Classes::of(&b, Caps::VERIFY, &QuotientCache::new());
        let (hit, fresh) = (quotient(hit), quotient(fresh));
        assert!(clean.is_clean());
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
        let slots = e.system.entities[0].slots.get();
        e.system.entities[0].slots = NonZeroU8::new(slots + 1).expect("a slot more");
        near.push(("one entity's instance count", e, Caps::VERIFY));
        let mut e = job_ext(true);
        let id = e.system.entities[0].id.clone();
        let n = NonZeroU8::new(3).expect("three");
        e.system.bounds.per_entity.insert(id, n);
        near.push(("one entity's count in the bounds", e, Caps::VERIFY));
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
            cache.classes(e, *caps, |_| {
                ran.set(true);
                (Classified::Nothing, Ended::Stopped)
            });
            assert!(ran.get(), "{what}: a near miss computes");
        }
        assert!(
            !computes(&cache, &ext, Caps::VERIFY),
            "the stored key still hits"
        );
        // The run of S gathers no view, so the pattern cap cannot change the classes.
        let patterns = Caps {
            patterns: 1,
            ..Caps::VERIFY
        };
        assert!(
            !computes(&cache, &ext, patterns),
            "another pattern cap hits"
        );
    }

    /// The run of S gathers no view (#251, #296), so a pattern cap of one stops nothing: the
    /// classes are the uncapped ones, where a run that gathered views would stop at once.
    #[test]
    fn a_pattern_cap_does_not_stop_a_quotient() {
        let ext = job_ext(true);
        let one = Caps {
            patterns: 1,
            ..Caps::VERIFY
        };
        let capped = quotient(Classes::of(&ext, one, &QuotientCache::new()).0);
        let whole = quotient(Classes::of(&ext, Caps::VERIFY, &QuotientCache::new()).0);
        assert!(capped == whole);
        assert!(
            !ext.system.clone().run(one).complete,
            "a run with views stops"
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
        let run = cache.stats.lock().unwrap().run_bytes;
        assert!(run > 0, "the run of S is held");
        assert_eq!(
            pool.held(),
            stored + run,
            "the entry and the run of S are charged, and only they"
        );
        let short = Caps::VERIFY.charged_to(pool.with_room(need - 1));
        let ran = Cell::new(false);
        let (_, clean) = cache.classes(&ext, short, |_| {
            ran.set(true);
            Classes::of(&ext, short, &QuotientCache::new())
        });
        assert!(ran.get(), "a room short of `need` computes");
        assert!(!clean.is_clean(), "and a run stops at the limit");
        let exact = Caps::VERIFY.charged_to(pool.with_room(need));
        let ((_, clean), seen) =
            crate::memory::watch(|| cache.classes(&ext, exact, |_| unreachable!()));
        assert!(clean.is_clean(), "a room of `need` takes the entry");
        assert_eq!(seen.peak, need, "a hit shows the peak computing shows");
        assert!(!seen.crowded);
        drop(cache);
        assert_eq!(pool.held(), 0, "dropping the cache gives the entry back");
    }

    /// The extended sub-lifecycle of `job_ext`, observing one var more: another key whose
    /// nested reduction leaves the same sub-lifecycle to run.
    fn other_key() -> Extended {
        let mut e = job_ext(true);
        let extra = e.system.var_ids().find(|v| !e.obs_local.contains(v));
        e.obs_local.insert(extra.expect("an unobserved var"));
        e
    }

    /// Two keys over one sub-lifecycle run S once (#296): the second partitions the run the
    /// first held, gets what computing gives, and shows `watch` the peak running shows, so
    /// its own entry answers the rooms a fresh computation's would.
    #[test]
    fn keys_over_one_sub_lifecycle_share_the_run_of_s() {
        let (a, b) = (job_ext(true), other_key());
        let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
        let caps = Caps::VERIFY.charged_to(pool);
        let cache = cache();
        let (computed, _) = ask(&cache, &a, caps);
        assert!(computed.is_some());
        assert_eq!(cache.stats.lock().unwrap().runs_held, 1);
        let (computed, seen) = ask(&cache, &b, caps);
        assert!(computed.is_some(), "another key computes its own classes");
        assert_eq!(cache.stats.lock().unwrap().run_hits, 1, "on the held run");
        let (_, alone) = crate::memory::watch(|| Classes::of(&b, caps, &QuotientCache::new()));
        assert_eq!(seen, alone, "with the peak running shows");
        let need = |k: usize| {
            let slots = cache.slots.lock().unwrap();
            let entry = slots[k].entry.lock().unwrap();
            entry.as_ref().expect("a clean quotient is stored").need
        };
        assert_eq!(
            need(1),
            alone.peak,
            "so the entry's `need` is a fresh one's"
        );
        assert_eq!(need(0), need(1));
    }

    /// A held run answers only a pool that covers its peak now; in a smaller room the key
    /// drops it and runs S alone, stopping where it would. A run a cap stopped is not held.
    #[test]
    fn a_held_run_answers_only_a_room_it_fits_and_a_stopped_run_is_not_held() {
        let (a, b) = (job_ext(true), other_key());
        let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
        let cache = cache();
        assert!(ask(&cache, &a, Caps::VERIFY.charged_to(pool)).0.is_some());
        let held = cache.stats.lock().unwrap().run_bytes;
        assert!(pool.held() > held, "the run and the entry are charged");
        let need = {
            let runs = cache.runs.lock().unwrap();
            let held = runs[0].run.lock().unwrap();
            held.as_ref().expect("a complete run is held").need
        };
        let short = Caps::VERIFY.charged_to(pool.with_room(need - 1));
        let (computed, _) = ask(&cache, &b, short);
        assert!(computed.is_some());
        assert_eq!(
            cache.stats.lock().unwrap().run_hits,
            0,
            "a short room runs S alone"
        );
        assert!(
            cache.runs.lock().unwrap()[0].run.lock().unwrap().is_none(),
            "and drops the held run first"
        );

        let capped = Caps {
            states: 3,
            ..Caps::VERIFY
        };
        let cache = self::cache();
        ask(&cache, &a, capped);
        ask(&cache, &b, capped);
        let stats = cache.stats.lock().unwrap();
        assert_eq!(
            (stats.runs_held, stats.run_hits),
            (0, 0),
            "a capped run is not held"
        );
    }

    /// A run of S is shared between sub-lifecycles that differ only in where their actions
    /// came from (`Action::source`), and not between ones that differ in a guard.
    #[test]
    fn a_run_is_shared_across_action_sources_and_not_across_guards() {
        let a = job_ext(true).system;
        let mut b = a.clone();
        b.actions[0].source = crate::lifecycle::types::ActId::at(b.actions.len() + 7);
        assert!(a != b && same_runs(&a, &b));
        let caps = Caps::VERIFY;
        let ran = |s: &TransitionSystem| {
            s.clone()
                .run_checking(caps, crate::lifecycle::explore::Checks::none())
        };
        assert!(ran(&a).same_graph(&ran(&b)));
        let mut c = a.clone();
        let g = c.actions.iter().position(|x| !x.guards.is_empty());
        c.actions[g.expect("a guarded action")].guards.pop();
        assert!(!same_runs(&a, &c));
    }

    /// Asks `cache` for `ext` under `caps`, and what `watch` saw; `None` for the result when
    /// the cache answered without computing.
    fn ask(cache: &QuotientCache, ext: &Extended, caps: Caps) -> (Option<Classified>, Watched) {
        let ran = Cell::new(false);
        let ((classes, ended), seen) = crate::memory::watch(|| {
            cache.classes(ext, caps, |cache| {
                ran.set(true);
                Classes::of(ext, caps, cache)
            })
        });
        let fresh = Classes::of(ext, caps, &QuotientCache::new());
        assert!(
            classes == fresh.0 && ended.is_clean() == fresh.1.is_clean(),
            "the cache answers what computing does"
        );
        (ran.get().then_some(classes), seen)
    }

    /// A sub-lifecycle whose run of S stopped at the memory limit alone in its pool is stored
    /// declined (#289): a cone with the same key in a pool of no more room that covers what
    /// the runs were granted takes it without running S again, with what computing shows
    /// `watch`. A larger room, or one that does not cover them, computes, and a clean result
    /// replaces the declined entry. A decline that others' bytes decided is not stored.
    #[test]
    fn a_declined_sub_lifecycle_answers_a_pool_no_larger_that_fits_its_runs() {
        let ext = job_ext(true);
        let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
        let whole = Caps::VERIFY.charged_to(pool);
        let ((_, ended), alone) =
            crate::memory::watch(|| Classes::of(&ext, whole, &QuotientCache::new()));
        assert!(ended.is_clean());
        let room = alone.peak / 2;
        let half = Caps::VERIFY.charged_to(pool.with_room(room));
        let (_, ended) = Classes::of(&ext, half, &QuotientCache::new());
        assert_eq!(
            ended,
            Ended::Declined,
            "half the room stops the run of S alone"
        );

        let cache = cache();
        let (computed, declined) = ask(&cache, &ext, half);
        assert!(matches!(computed, Some(Classified::Nothing)));
        let need = {
            let slots = cache.slots.lock().unwrap();
            let entry = slots[0].entry.lock().unwrap();
            let entry = entry.as_ref().expect("a declined computation is stored");
            assert_eq!(entry.declined_in, Some(room));
            entry.need
        };
        assert!(0 < need && need <= room);
        let (hit, seen) = ask(&cache, &ext, half);
        assert!(hit.is_none(), "an equal pool takes the declined entry");
        assert_eq!(seen, declined, "and shows `watch` what computing shows");
        let (hit, _) = ask(&cache, &ext, Caps::VERIFY.charged_to(pool.with_room(need)));
        assert!(hit.is_none(), "a smaller room that covers `need` takes it");
        assert_eq!(cache.hits(), 2);
        let short = Caps::VERIFY.charged_to(pool.with_room(need - 1));
        assert!(
            ask(&cache, &ext, short).0.is_some(),
            "a room short of `need` computes"
        );
        let larger = Caps::VERIFY.charged_to(pool.with_room(room + 1));
        assert!(
            ask(&cache, &ext, larger).0.is_some(),
            "a larger room computes"
        );
        let (clean, _) = ask(&cache, &ext, whole);
        assert!(
            matches!(clean, Some(Classified::Classes(_))),
            "the whole pool computes"
        );
        let (hit, _) = ask(&cache, &ext, whole);
        assert!(
            hit.is_none(),
            "and its clean result replaced the declined one"
        );
        assert!(
            ask(&cache, &ext, half).0.is_some(),
            "which half the room does not fit"
        );

        // A pool with room for the whole run, of which others hold all but half: the run
        // stops where the declined one did, but crowded, so nothing is stored.
        let crowded_pool = pool.with_room(alone.peak);
        let _others = Charge::hold(crowded_pool, alone.peak - room).expect("room to hold");
        let crowded = Caps::VERIFY.charged_to(crowded_pool);
        let empty = self::cache();
        let (computed, seen) = ask(&empty, &ext, crowded);
        assert!(matches!(computed, Some(Classified::Nothing)) && seen.crowded);
        assert!(
            ask(&empty, &ext, crowded).0.is_some(),
            "a crowded decline is not stored"
        );
    }
}
