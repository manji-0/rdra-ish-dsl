//! The move generator against its definition.

use crate::lifecycle::tests::model_from;
use std::num::NonZeroU8;

use crate::lifecycle::{Bounds, Caps, Scope, TransitionSystem};

use super::Threads;

const SHOP: &str = r#"
entity Customer "Customer" {
  id: Int @pk
  status: Enum(active, closed) @default(active)
}
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid, shipped) @default(open)
  qty: Int @default(1)
}
entity Line "Line" { id: Int @pk }
relate(Order, Customer, N:1)
relate(Line, Order, N:1)
usecase Join "Join"
usecase Place "Place"
usecase AddLine "Add line"
usecase Pay "Pay"
usecase Ship "Ship"
usecase Close "Close"
usecase Drop "Drop"
condition Active "active"
holds(Active, Customer.status == active)
creates(Join, Customer)
creates(Place, Order)
applies_to(Active, Place)
creates(AddLine, Line)
updates(AddLine, Order)
sets(AddLine, Order.qty == 2)
transitions(Pay, Order.status, open -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Close, Customer.status, active -> closed)
deletes(Drop, Line)
"#;

/// `enabled_bindings` lists the `bindings` whose pre-guards hold, in `bindings` order, in
/// every reachable state: where no instance exists, where one slot is free, where none is,
/// and after a delete.
#[test]
fn enabled_bindings_are_the_bindings_whose_pre_guards_hold() {
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
        let run = ts.run(Caps::VERIFY);
        let mut checked = 0;
        let mut kept = 0;
        {
            let system = &run.system;
            for state in &run.states {
                for action in system.actions() {
                    let want: Vec<Box<[u8]>> = system
                        .bindings(action, state)
                        .into_iter()
                        .filter(|params| system.pre_guards_hold(action, state, params))
                        .map(Vec::into_boxed_slice)
                        .collect();
                    let got = system.enabled_bindings(action, state);
                    assert_eq!(got, want, "{} in {state:?}", action.name);
                    checked += 1;
                    kept += got.len();
                }
            }
        }
        assert!(checked > 100 && kept > 30, "{checked} checks, {kept} kept");
    }
}

/// A run with worker threads is the run on one thread: the same states, edges, and bindings in the
/// same order, the same parents, findings, and views. A run a cap stops is cut at the same
/// state.
#[test]
fn a_run_is_the_same_on_any_number_of_threads() {
    let bounds = Bounds {
        instances: NonZeroU8::new(3).expect("3 instances"),
        int_max: 5,
    };
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    for (caps, complete) in [(Caps::VERIFY, true), (Caps::new(256, 5_000), false)] {
        let one = ts.clone().run_on(caps, Threads::Fixed(0));
        assert_eq!(one.complete, complete);
        // Enough states that the workers take most of them.
        assert!(one.states.len() >= 5_000, "{} states", one.states.len());
        for threads in [1, 2, 4, 7] {
            let many = ts.clone().run_on(caps, Threads::Fixed(threads));
            assert_eq!(many.states, one.states, "{threads} threads");
            assert_eq!(many.edges, one.edges, "{threads} threads");
            assert_eq!(many.bindings, one.bindings, "{threads} threads");
            assert_eq!(many.parents, one.parents, "{threads} threads");
            assert_eq!(many.complete, one.complete, "{threads} threads");
            assert_eq!(many.diags, one.diags, "{threads} threads");
            for (a, b) in many.views.iter().zip(&one.views) {
                assert_eq!(a.patterns, b.patterns, "{threads} threads");
                assert_eq!(a.initial, b.initial, "{threads} threads");
                assert_eq!(a.changes, b.changes, "{threads} threads");
                assert_eq!(a.deletable, b.deletable, "{threads} threads");
                assert_eq!(a.touch, b.touch, "{threads} threads");
                assert_eq!(a.moves, b.moves, "{threads} threads");
                assert_eq!(a.truncated, b.truncated, "{threads} threads");
            }
        }
    }
}

/// A run charged to a pool stops at the memory limit at the same state on any number of
/// threads, short of the states the limit leaves room for, and names the limit.
#[test]
fn a_run_stops_at_the_memory_limit_at_the_same_state_on_any_number_of_threads() {
    use crate::lifecycle::StateDiag;
    use crate::memory::{MemoryPool, MemorySize};

    let bounds = Bounds {
        instances: NonZeroU8::new(3).expect("3 instances"),
        int_max: 5,
    };
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    let limit = MemorySize::from_bytes(512 << 10).expect("not zero");
    let charged = || Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
    let whole = ts.clone().run_on(Caps::VERIFY, Threads::Fixed(0));
    let one = ts.clone().run_on(charged(), Threads::Fixed(0));
    assert!(!one.complete);
    assert!(
        one.states.len() > 100 && one.states.len() < whole.states.len(),
        "{} of {} states",
        one.states.len(),
        whole.states.len()
    );
    assert!(one.diags.iter().any(|d| matches!(
        d.diag,
        StateDiag::MemoryCapReached { limit: l, states, .. }
            if l == limit && states == one.states.len()
    )));
    for threads in [1, 2, 4] {
        let many = ts.clone().run_on(charged(), Threads::Fixed(threads));
        assert_eq!(many.states, one.states, "{threads} threads");
        assert_eq!(many.diags, one.diags, "{threads} threads");
    }
}

/// A run holds no more than it is charged: its store grows only once the pool has covered
/// the larger buffer, the edges' included. The one thing it holds past its charge is the old
/// index of seen states while the index grows, a third of the index at most.
#[test]
fn a_run_holds_no_more_than_its_charge() {
    use crate::memory::{MemoryPool, MemorySize};
    use crate::test_alloc::most_held;

    let bounds = Bounds {
        instances: NonZeroU8::new(2).expect("2 instances"),
        int_max: 5,
    };
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    let whole = ts.clone().run_on(Caps::VERIFY, Threads::Fixed(0));
    let mut worst = (0, 0);
    // Limits a few KiB apart, so that some stop just as the edges fill their buffer.
    for step in 1..=48u64 {
        let limit = whole.peak * step / 48;
        let size = MemorySize::from_bytes(limit).expect("not zero");
        let pool = MemoryPool::for_command(size, "--memory");
        let caps = Caps::VERIFY.charged_to(pool);
        let system = ts.clone();
        let (run, held) = most_held(|| system.run_on(caps, Threads::Fixed(0)));
        // The old index while the index grows: 5 bytes a bucket, half the buckets of now.
        let buckets = (run.states.len() * 8 / 7).max(4).next_power_of_two();
        let past = held.saturating_sub(pool.peak() + 5 * buckets as u64 / 2);
        if past > worst.0 {
            worst = (past, limit);
        }
    }
    // The views, the moves of the states being expanded, and the findings are not charged.
    assert!(
        worst.0 <= 64 << 10,
        "{} B past the charge at a limit of {} B",
        worst.0,
        worst.1
    );
}

/// A run past its deadline stops before it holds a state and says so: `verify` bounds a
/// whole-model run by `--temporal-budget` this way, and a budget of 0 s stops it at once.
#[test]
fn a_run_past_its_deadline_stops_and_names_the_budget() {
    use crate::lifecycle::StateDiag;
    use std::time::{Duration, Instant};

    let ts = TransitionSystem::build(&model_from(SHOP), Bounds::VERIFY, &Scope::ALL).ts;
    let run = ts.clone().run(Caps::VERIFY.until(Instant::now()));
    assert!(!run.complete);
    assert!(run.states.is_empty(), "{} states", run.states.len());
    assert!(run
        .diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::TimeBudgetReached { states: 0 })));
    let whole = ts.run(Caps::VERIFY.until(Instant::now() + Duration::from_secs(3600)));
    assert!(whole.complete);
}

#[test]
fn default_pattern_cap_scales_with_the_instance_count_and_there_is_no_state_cap() {
    let at = |n| {
        let caps = Caps::for_instances(n);
        (caps.patterns, caps.states)
    };
    assert_eq!(at(1), (1 << 10, usize::MAX));
    assert_eq!(at(2), (1 << 18, usize::MAX));
    assert_eq!(at(3), (1 << 26, usize::MAX));
    assert_eq!(at(0), at(1), "no instances counts as one");
}

#[test]
fn default_caps_saturate_where_the_shift_leaves_usize() {
    let last_exact = (usize::BITS - 1 - 10) / 8 + 1;
    let caps = Caps::for_instances(last_exact);
    assert_eq!(caps.patterns, 1 << (10 + 8 * (last_exact - 1)));
    for n in [last_exact + 1, 100, 255, u32::MAX] {
        let caps = Caps::for_instances(n);
        assert_eq!(
            (caps.patterns, caps.states),
            (usize::MAX, usize::MAX),
            "n = {n}"
        );
    }
}

/// `store_bytes` is the store `Bfs::bytes_with_one_more` counts, modelled: on complete runs
/// of a thousand states and more it is never under `peak` and at most 10% over, and at the
/// state a run stops at it is the same within the same bounds.
#[test]
fn store_bytes_models_the_store_a_run_counts() {
    use super::store::store_bytes;
    use crate::lifecycle::StateDiag;
    use crate::memory::{MemoryPool, MemorySize};

    let within = |model: u64, counted: u64, what: &str| {
        assert!(
            model >= counted && model * 10 <= counted * 11,
            "{what}: model {model} B against {counted} B counted"
        );
    };
    for instances in [2u8, 3] {
        let bounds = Bounds {
            instances: NonZeroU8::new(instances).expect("instances"),
            int_max: 5,
        };
        let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
        let whole = ts.clone().run_on(Caps::VERIFY, Threads::Fixed(0));
        assert!(whole.complete && whole.states.len() >= 1000);
        let n = whole.states.len() as u64;
        within(
            store_bytes(n, whole.state_width(), whole.edges().len() as u64),
            whole.peak,
            &format!("{instances} instances, {n} states"),
        );

        let limit = MemorySize::from_bytes(whole.peak / 3).expect("not zero");
        let caps = Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
        let stopped = ts.run_on(caps, Threads::Fixed(0));
        let held = stopped
            .diags
            .iter()
            .find_map(|d| match &d.diag {
                StateDiag::MemoryCapReached { held, .. } => Some(held.clone()),
                _ => None,
            })
            .expect("a run at a third of its peak stops at the limit");
        let states = stopped.states.len() as u64;
        assert!(states >= 1000, "{states} states");
        within(
            store_bytes(states + 1, held.width, held.edges),
            held.bytes,
            &format!("{instances} instances, stopped at {states} states"),
        );
        let need = held.bytes_for(held.bound.expect("a bound"));
        assert!(
            need >= held.bytes,
            "{need} B to finish, {} B held",
            held.bytes
        );
    }
}

#[test]
fn store_bytes_saturates_and_scales_the_edges_by_the_states_expanded() {
    use super::store::{store_bytes, Held};

    assert_eq!(store_bytes(u64::MAX, 16, u64::MAX), u64::MAX);
    assert_eq!(store_bytes(0, 0, 0), 12 * 4 + 5 * 4 + 12 * 4);
    // 10 states reached, 5 of them expanded: 4 edges per expanded state.
    let held = Held {
        bytes: 1000,
        width: 10,
        edges: 20,
        bound: None,
        layers: layers(&[1, 4, 5], 4),
    };
    assert_eq!(held.layers.expanded_states(), 5);
    assert_eq!(held.bytes_for(1024), store_bytes(1024, 10, 4096));
    assert_eq!(held.bytes_per_state(9), 100);
}

/// The layers of a run stopped while it expands the next-to-last layer of `new`, after
/// `expanded` of its states: every layer but the last is whole, and the last holds what that
/// layer has found so far.
fn layers(new: &[u64], expanded: u64) -> super::Layers {
    let mut layers = super::Layers::default();
    for _ in 0..new[0] {
        layers.admit(true);
    }
    let mut at = 0;
    for d in 0..new.len() - 1 {
        for i in 0..new[d] {
            layers.read(at);
            if i == 0 {
                for _ in 0..new[d + 1] {
                    layers.admit(false);
                }
            }
            at += 1;
        }
    }
    layers.expanded = expanded;
    layers
}

#[test]
fn a_run_counts_the_new_states_of_each_depth() {
    let ts = TransitionSystem::build(&model_from(SHOP), Bounds::VERIFY, &Scope::ALL).ts;
    let run = ts.run_on(Caps::VERIFY, Threads::Fixed(0));
    assert!(run.complete);
    let layers = &run.layers;
    assert_eq!(layers.states(), run.states.len() as u64);
    assert_eq!(
        layers.whole().len(),
        layers.new.len(),
        "a finished run has no layer filling"
    );
    // A state's depth is one past its parent's.
    let mut depth = vec![0usize; run.states.len()];
    for (i, parent) in run.parents.iter().enumerate() {
        if let Some(edge) = parent {
            depth[i] = depth[run.edges[*edge as usize].from as usize] + 1;
        }
    }
    let mut counted = vec![0u64; layers.new.len()];
    for d in depth {
        counted[d] += 1;
    }
    assert_eq!(counted, layers.new);
}

#[test]
fn the_estimate_mirrors_the_rising_layers_once_two_have_fallen() {
    use super::Estimate;

    // Still rising: no estimate.
    assert_eq!(
        layers(&[1, 3, 9, 4], 2).estimate(),
        Estimate::Rising {
            depth: 2,
            last: 9,
            before: 3
        }
    );
    // One layer past the peak is not enough.
    assert_eq!(
        layers(&[1, 3, 9, 6, 2], 2).estimate(),
        Estimate::Unsettled { peak: 2, depth: 3 }
    );
    // Two past it: the falling side holds what the rising side did, 1 + 4 on each side of 9,
    // one more than the 18 states reached.
    let falling = layers(&[1, 4, 9, 2, 1, 1], 1).estimate();
    assert_eq!(
        falling,
        Estimate::Falling {
            states: 19,
            peak: 2,
            depth: 4
        }
    );
    assert_eq!(falling.states_to_cover(), Some(23));
    // A fall that already holds more than the mirror says settles nothing.
    assert_eq!(
        layers(&[1, 9, 8, 7, 1], 1).estimate(),
        Estimate::Overrun { peak: 1, depth: 3 }
    );
    // A tie keeps the first of the largest layers as the peak: the mirror of 1 + 4 + 1 is
    // under the 13 states reached.
    assert_eq!(
        layers(&[1, 4, 4, 2, 1, 1], 0).estimate(),
        Estimate::Overrun { peak: 1, depth: 4 }
    );
}

/// A cone of `samples/patent-ops` at `Bounds::VERIFY` (the `PriorityClaim` view, reduced) as
/// `examples/lifecycle_measure.rs` printed it, stopped at nine tenths of its 22233 states:
/// the estimate comes out within a third of the count, and its states to cover over it.
#[test]
fn the_estimate_on_a_sample_cone_comes_within_a_third_of_its_states() {
    use super::Estimate;

    const PRIORITY_CLAIM: [u64; 31] = [
        1, 1, 3, 5, 7, 9, 12, 18, 19, 30, 50, 68, 109, 192, 327, 512, 748, 992, 1260, 1601, 1997,
        2347, 2505, 2552, 2372, 1913, 1368, 758, 336, 106, 15,
    ];
    let total: u64 = PRIORITY_CLAIM.iter().sum();
    assert_eq!(total, 22233);
    // Nine tenths in: depth 25 is whole and depth 26 has 359 of its 1368 states.
    let mut stopped = PRIORITY_CLAIM[..26].to_vec();
    stopped.push(359);
    let estimate = layers(&stopped, 502).estimate();
    let Estimate::Falling {
        states,
        peak,
        depth,
    } = estimate
    else {
        panic!("{estimate:?}");
    };
    assert_eq!((peak, depth), (23, 25));
    assert!(
        states * 3 <= total * 4 && states * 4 >= total * 3,
        "{states} of {total}"
    );
    assert!(estimate.states_to_cover().expect("an estimate") >= total);
    // Eight tenths in, one layer past the peak: no estimate yet.
    let mut stopped = PRIORITY_CLAIM[..25].to_vec();
    stopped.push(49);
    assert_eq!(
        layers(&stopped, 60).estimate(),
        Estimate::Unsettled {
            peak: 23,
            depth: 24
        }
    );
}

/// Under a memory limit the cones explored side by side stop where they stop run by hand one
/// at a time in cone order, each in the room that what the pool held before, and the results
/// of the cones before it, leave; and the pool never holds more than the limit.
#[test]
fn cones_under_a_limit_stop_where_they_stop_one_at_a_time_in_order() {
    use crate::memory::{Charge, MemoryPool, MemorySize};

    let bounds = Bounds {
        instances: NonZeroU8::new(3).expect("3 instances"),
        int_max: 5,
    };
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    let targets = ts.targets();
    let free = ts.explore_targets(&targets, Caps::VERIFY);
    assert!(free.cones.len() >= 3, "{} cones", free.cones.len());
    let total: u64 = free.cones.iter().map(|c| c.run.peak).sum();
    for (fifths, before) in [(2, 0), (3, 0), (2, 5)] {
        let limit = MemorySize::from_bytes(total * fifths / 5).expect("not zero");
        let pool = || {
            let pool = MemoryPool::for_command(limit, "--memory");
            let mut held = Charge::new(pool);
            assert!(held.cover(limit.bytes() * before / 100));
            (pool, held)
        };
        let at = format!("{fifths}/5 of {total} B, {before}% held before");
        let (side, _before) = pool();
        let got = ts.explore_targets(&targets, Caps::VERIFY.charged_to(side));
        let (alone, _before) = pool();
        let mut held = alone.held();
        let mut kept = Vec::new();
        for c in &got.cones {
            let room = alone.room().saturating_sub(held);
            let caps = Caps::VERIFY.charged_to(alone.with_room(room));
            let run = ts.restrict_to(&c.cone.vars, &c.cone.keep).run(caps);
            assert_eq!(run.states, c.run.states, "{at}");
            assert_eq!(run.complete, c.run.complete, "{at}");
            held += run.kept;
            kept.push(run);
        }
        assert!(
            got.cones.iter().any(|c| !c.run.complete),
            "{at}: none stops"
        );
        assert!(
            side.peak() <= limit.bytes(),
            "{at}: {} B at once",
            side.peak()
        );
    }
}
