//! The move generator against its definition.

use crate::lifecycle::tests::model_from;
use std::num::NonZeroU8;

use crate::lifecycle::{Bounds, Caps, Scope, TransitionSystem};

use super::{Checks, ComponentRun, Edge, Threads};

/// Each state's parent: the move that first reached it, `None` for an initial state.
fn parents(run: &ComponentRun) -> Vec<Option<Edge>> {
    (0..run.states.len() as u32)
        .map(|at| run.parent(at))
        .collect()
}

pub(super) const SHOP: &str = r#"
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
                        .bindings(action, &state)
                        .into_iter()
                        .filter(|params| system.pre_guards_hold(action, &state, params))
                        .map(Vec::into_boxed_slice)
                        .collect();
                    let got = system.enabled_bindings(action, &state);
                    assert_eq!(got, want, "{} in {state:?}", action.name);
                    checked += 1;
                    kept += got.len();
                }
            }
        }
        assert!(checked > 100 && kept > 30, "{checked} checks, {kept} kept");
    }
}

/// The pattern cap stops only a run that gathers views (#251, #296): at a cap of one pattern
/// the run with views stops at `PatternCapReached`, and the run without views explores the
/// same graph as an uncapped run, gathers no pattern, and says so (`gathered_views`).
#[test]
fn the_pattern_cap_stops_only_a_run_that_gathers_views() {
    let ts = TransitionSystem::build(&model_from(SHOP), Bounds::VERIFY, &Scope::ALL).ts;
    let capped = Caps::new(1, usize::MAX);
    let with = ts.clone().run(capped);
    assert!(!with.complete);
    assert!(with.gathered_views());
    assert!(with.diags.iter().any(|d| matches!(
        d.diag,
        crate::lifecycle::StateDiag::PatternCapReached { cap: 1, .. }
    )));
    let without = ts.clone().run(capped.without_views());
    let whole = ts.clone().run(Caps::VERIFY);
    assert!(without.complete && whole.complete);
    assert!(!without.gathered_views());
    assert!(without.views.iter().all(|v| v.patterns.is_empty()));
    assert_eq!(without.states, whole.states);
    assert_eq!(without.edges(), whole.edges());
    assert_eq!(without.diags, whole.diags);
}

/// Tickets that move between desks alike, and a flag set again where it is set: a run has
/// self-loops (`Mark` on a marked ticket) and, up to instance symmetry, orbit self-edges
/// (`Reassign` to a desk like the one the ticket left, as in `sym_relink_loop`).
pub(super) const DESKS: &str = r#"
entity Desk "Desk" { id: Int @pk  open: Bool @default(true) }
entity Ticket "Ticket" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  flag: Bool @default(false)
}
relate(Ticket, Desk, N:1)
usecase Staff "Staff"
usecase Raise "Raise"
usecase Reassign "Reassign"
usecase Close "Close"
usecase Mark "Mark"
creates(Staff, Desk)
creates(Raise, Ticket)
relinks(Reassign, Ticket, Desk)
transitions(Close, Ticket.status, open -> closed)
sets(Mark, Ticket.flag == true)
"#;

/// The worker counts the any-thread tests compare with a run on the caller's thread alone:
/// one, a few, and more than the chunks a layer of the test systems has.
const THREADS: [usize; 6] = [1, 2, 3, 4, 7, 17];

/// A run up to instance symmetry with worker threads is the run on one thread: workers
/// canonicalize, pack and probe each successor (#296 item 3.1), and the reader numbers the
/// states, so the states, edges, orbit self-edges and orbit counts are the same.
#[test]
fn a_run_up_to_symmetry_is_the_same_on_any_number_of_threads() {
    let bounds = Bounds::uniform(NonZeroU8::new(4).expect("4 instances"), 5);
    let ts = TransitionSystem::build(&model_from(DESKS), bounds, &Scope::ALL).ts;
    let group = super::symmetry::Group::of(&ts);
    let caps = Caps::VERIFY.without_views();
    let run = |threads| {
        ts.clone()
            .bfs_on(caps, Checks::All, threads, Some(&group), true)
    };
    let (one, _, one_parts) = run(Threads::Fixed(0));
    // Enough states that the workers take most of them.
    assert!(one.states.len() >= 1_000, "{} states", one.states.len());
    assert!(!one_parts.orbit_self.is_empty());
    for threads in THREADS {
        let (many, _, parts) = run(Threads::Fixed(threads));
        assert_eq!(many.states, one.states, "{threads} threads");
        assert_eq!(many.edges(), one.edges(), "{threads} threads");
        assert_eq!(many.bindings, one.bindings, "{threads} threads");
        assert_eq!(parents(&many), parents(&one), "{threads} threads");
        assert_eq!(many.diags, one.diags, "{threads} threads");
        assert_eq!(many.peak, one.peak, "{threads} threads");
        assert_eq!(parts.orbit_self, one_parts.orbit_self, "{threads} threads");
        assert_eq!(parts.orbits, one_parts.orbits, "{threads} threads");
    }
}

/// A run with worker threads is the run on one thread: the same states, edges, and bindings in the
/// same order, the same parents, findings, and views. A run a cap stops is cut at the same
/// state.
#[test]
fn a_run_is_the_same_on_any_number_of_threads() {
    let bounds = Bounds::uniform(NonZeroU8::new(3).expect("3 instances"), 5);
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    for (caps, complete) in [(Caps::VERIFY, true), (Caps::new(256, 5_000), false)] {
        let one = ts.clone().run_on(caps, Checks::All, Threads::Fixed(0));
        assert_eq!(one.complete, complete);
        // Enough states that the workers take most of them.
        assert!(one.states.len() >= 5_000, "{} states", one.states.len());
        for threads in THREADS {
            let many = ts
                .clone()
                .run_on(caps, Checks::All, Threads::Fixed(threads));
            assert_eq!(many.states, one.states, "{threads} threads");
            assert_eq!(many.edges(), one.edges(), "{threads} threads");
            assert_eq!(many.bindings, one.bindings, "{threads} threads");
            assert_eq!(parents(&many), parents(&one), "{threads} threads");
            assert_eq!(many.complete, one.complete, "{threads} threads");
            assert_eq!(many.diags, one.diags, "{threads} threads");
            assert_eq!(many.kept, one.kept, "{threads} threads");
            assert_eq!(many.peak, one.peak, "{threads} threads");
            assert_eq!(many.layers, one.layers, "{threads} threads");
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

    let bounds = Bounds::uniform(NonZeroU8::new(3).expect("3 instances"), 5);
    let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
    let limit = MemorySize::from_bytes(512 << 10).expect("not zero");
    let charged = || Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
    let whole = ts
        .clone()
        .run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
    let one = ts.clone().run_on(charged(), Checks::All, Threads::Fixed(0));
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
    for threads in [1, 3, 17] {
        let many = ts
            .clone()
            .run_on(charged(), Checks::All, Threads::Fixed(threads));
        assert_eq!(many.states, one.states, "{threads} threads");
        assert_eq!(many.diags, one.diags, "{threads} threads");
        assert_eq!(many.peak, one.peak, "{threads} threads");
    }
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
        let bounds = Bounds::uniform(NonZeroU8::new(instances).expect("instances"), 5);
        let ts = TransitionSystem::build(&model_from(SHOP), bounds, &Scope::ALL).ts;
        let whole = ts
            .clone()
            .run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
        assert!(whole.complete && whole.states.len() >= 1000);
        let n = whole.states.len() as u64;
        within(
            store_bytes(n, whole.state_width(), whole.moves(), whole.shape()),
            whole.peak,
            &format!("{instances} instances, {n} states"),
        );

        let limit = MemorySize::from_bytes(whole.peak / 3).expect("not zero");
        let caps = Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
        let stopped = ts.run_on(caps, Checks::All, Threads::Fixed(0));
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
        // The model counts a first-edge index for every state, as a complete run holds; a
        // stopped run holds one for the states it read only, so the model is over by at most
        // those of the states it did not read.
        let model = store_bytes(states + 1, held.width, held.edges, held.shape);
        let unread = 4 * (states + 1).next_power_of_two();
        assert!(
            model >= held.bytes,
            "model {model} B under {} B",
            held.bytes
        );
        within(
            model - unread.min(model - held.bytes),
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
    use super::store::{store_bytes, Held, Shape};

    let graph = Shape {
        binding: 1,
        graph: true,
    };
    let checks = Shape {
        binding: 2,
        graph: false,
    };
    assert_eq!(store_bytes(u64::MAX, 16, u64::MAX, graph), u64::MAX);
    assert_eq!(store_bytes(0, 0, 0, graph), 12 * 4 + 5 * 4 + 5 * 4);
    // Without an edge list a state is a source and a 2-byte id more, and an edge nothing.
    assert_eq!(store_bytes(0, 0, 1 << 30, checks), 10 * 4 + 5 * 4);
    // 10 states reached, 5 of them expanded: 4 edges per expanded state.
    let held = Held {
        bytes: 1000,
        width: 10,
        shape: graph,
        edges: 20,
        bound: None,
        layers: layers(&[1, 4, 5], 4),
    };
    assert_eq!(held.layers.expanded_states(), 5);
    assert_eq!(held.bytes_for(1024), store_bytes(1024, 10, 4096, graph));
    assert_eq!(held.bytes_per_state(9), 100);
}

/// States near `usize::MAX` bytes wide pass `u128` before the total passes `u64`, in the
/// product and in the sum after it; the total still saturates.
#[test]
fn store_bytes_saturates_at_the_widest_states() {
    use super::store::{store_bytes, Shape};

    let shape = Shape {
        binding: 4,
        graph: true,
    };
    // (2^64 + 11) * 2^64 passes u128.
    assert_eq!(store_bytes(u64::MAX, usize::MAX, 0, shape), u64::MAX);
    // (2^64 - 1) * 2^64 fits; adding 5 * 2^65 for `seen` does not.
    assert_eq!(
        store_bytes(u64::MAX, usize::MAX - 12, u64::MAX, shape),
        u64::MAX
    );
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
    let run = ts.run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
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
    for (i, parent) in parents(&run).iter().enumerate() {
        if let Some(edge) = parent {
            depth[i] = depth[edge.from as usize] + 1;
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

/// Rules SHOP breaks: a closed customer within two steps, and a shipped order with a line
/// beside a closed customer within six (Join, Place, AddLine, Pay, Ship, Close).
const BROKEN: &str = "forbidden(Customer, status == closed)\n\
    forbidden(Order, Customer, Order.status == shipped, Order.qty == 2, Customer.status == closed)\n";

/// A run told to stop once every check has failed stops right after the step that breaks
/// the last one: a prefix of the run to the end, with the same violations and traces, the
/// same on any number of threads.
#[test]
fn a_run_stops_once_every_check_has_failed() {
    use super::RunEnd;

    let bounds = Bounds::uniform(NonZeroU8::new(3).expect("3 instances"), 5);
    let src = format!("{SHOP}{BROKEN}");
    let ts = TransitionSystem::build(&model_from(&src), bounds, &Scope::ALL).ts;
    let whole = ts
        .clone()
        .run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
    assert_eq!(whole.end(), RunEnd::Complete);
    let caps = Caps::VERIFY.until_all_violated();
    let stopped = ts.clone().run_on(caps, Checks::All, Threads::Fixed(0));
    assert_eq!(stopped.end(), RunEnd::ChecksFailed);
    assert!(!stopped.complete);
    let n = stopped.states.len();
    // Past two chunks of 128 states, so the runs on threads below hand chunks to workers.
    assert!(
        n >= 256 && n * 10 < whole.states.len(),
        "{n} of {} states",
        whole.states.len()
    );
    assert!(stopped.states.iter().eq(whole.states.iter().take(n)));
    // It keeps no edge list; its parents are the whole run's, as the source and binding of
    // the move that reached each state.
    assert!(!stopped.has_edges() && whole.has_edges());
    assert_eq!(parents(&stopped)[..], parents(&whole)[..n]);
    assert_eq!(stopped.diags.len(), 2);
    assert_eq!(stopped.diags, whole.diags);
    for threads in [1, 3, 17] {
        let many = ts
            .clone()
            .run_on(caps, Checks::All, Threads::Fixed(threads));
        assert_eq!(many.states, stopped.states, "{threads} threads");
        assert_eq!(parents(&many), parents(&stopped), "{threads} threads");
        assert_eq!(many.moves(), stopped.moves(), "{threads} threads");
        assert_eq!(many.diags, stopped.diags, "{threads} threads");
        assert_eq!(many.end(), RunEnd::ChecksFailed, "{threads} threads");
    }
}

/// A check that never fails keeps the run going to the end, and so does a system with no
/// check at all: neither stops early.
#[test]
fn a_run_with_a_check_that_holds_runs_to_the_end() {
    use super::RunEnd;

    // No order ever has no item: `qty` starts at 1 and is only ever set to 2.
    for rules in [
        "forbidden(Customer, status == closed)\nforbidden(Order, qty == 0)\n",
        "",
    ] {
        let src = format!("{SHOP}{rules}");
        let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
        let whole = ts
            .clone()
            .run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
        let run = ts.run_on(
            Caps::VERIFY.until_all_violated(),
            Checks::All,
            Threads::Fixed(0),
        );
        assert_eq!(run.end(), RunEnd::Complete, "{rules}");
        assert_eq!(run.states, whole.states, "{rules}");
        assert_eq!(run.diags, whole.diags, "{rules}");
    }
}

/// A run that evaluates some checks is the run that evaluates all of them, with the
/// violations of the others left out: the same states, edges, views, store, and end, and for
/// each check it evaluates the same violation and trace. A run that evaluates none reports
/// none.
#[test]
fn a_run_reports_the_checks_it_evaluates_and_nothing_else() {
    use std::collections::BTreeSet;

    use crate::lifecycle::StateDiag;

    let after = "after(Pay).assert(Order.qty == 2)\n";
    let src = format!("{SHOP}{BROKEN}{after}");
    let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
    let all = ts
        .clone()
        .run_on(Caps::VERIFY, Checks::All, Threads::Fixed(0));
    let names: BTreeSet<String> = ts
        .invariants()
        .iter()
        .map(|i| i.name.clone())
        .chain(ts.after_asserts().iter().map(|a| a.name.clone()))
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    let violation = |d: &StateDiag| match d {
        StateDiag::ConstraintViolated { name, .. }
        | StateDiag::AfterAssertViolated { name, .. } => Some(name.clone()),
        _ => None,
    };
    let reported: BTreeSet<String> = all
        .diags
        .iter()
        .filter_map(|d| violation(&d.diag))
        .collect();
    assert_eq!(reported, names, "every check fails on the whole run");
    let subsets = names
        .iter()
        .map(|n| BTreeSet::from([n.clone()]))
        .chain([BTreeSet::new(), names.clone()]);
    for named in subsets {
        let checks = Checks::Named(named.clone());
        for threads in [0, 2, 7] {
            let run = ts
                .clone()
                .run_on(Caps::VERIFY, checks.clone(), Threads::Fixed(threads));
            assert_eq!(run.checks(), &checks);
            assert_eq!(run.states, all.states, "{named:?}");
            assert_eq!(run.edges(), all.edges(), "{named:?}");
            assert_eq!(parents(&run), parents(&all), "{named:?}");
            assert_eq!(run.end(), all.end(), "{named:?}");
            assert_eq!(run.kept, all.kept, "{named:?}");
            let kept: Vec<_> = all
                .diags
                .iter()
                .filter(|d| violation(&d.diag).is_none_or(|n| named.contains(&n)))
                .cloned()
                .collect();
            assert_eq!(run.diags, kept, "{named:?}");
            assert_eq!(run.views.len(), all.views.len());
            for (v, w) in run.views.iter().zip(&all.views) {
                assert_eq!(v.patterns, w.patterns, "{named:?}");
                assert_eq!(v.initial, w.initial, "{named:?}");
                assert_eq!(v.changes, w.changes, "{named:?}");
                assert_eq!(v.deletable, w.deletable, "{named:?}");
                assert_eq!(v.moves, w.moves, "{named:?}");
                assert_eq!(v.via(), w.via(), "{named:?}");
            }
        }
    }
}

/// The early stop reads every check the system holds, so a run that stops early evaluates
/// them all: with fewer it would stop at another state.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "evaluates every check")]
fn a_run_that_stops_once_every_check_fails_evaluates_every_check() {
    let src = format!("{SHOP}{BROKEN}");
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    ts.run_checking(Caps::VERIFY.until_all_violated(), Checks::none());
}
