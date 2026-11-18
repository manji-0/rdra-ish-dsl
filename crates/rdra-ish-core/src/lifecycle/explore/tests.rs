//! The move generator against its definition.

use crate::lifecycle::tests::model_from;
use crate::lifecycle::{Bounds, Caps, Scope, TransitionSystem};

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
