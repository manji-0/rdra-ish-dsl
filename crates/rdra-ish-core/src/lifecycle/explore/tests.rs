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
