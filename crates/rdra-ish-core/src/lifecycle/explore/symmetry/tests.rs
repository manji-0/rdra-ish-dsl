//! The canonical form on every state of small layouts, against every element of its group,
//! and the scan on hand-built systems.

use std::collections::BTreeSet;
use std::num::NonZeroU8;

use crate::lifecycle::expr::{State, StateSlice};
use crate::lifecycle::tests::model_from;
use crate::lifecycle::types::{At, CmpOp, Guard, GuardWhy, ModelRef, Pred, Site, Term, Value};
use crate::lifecycle::{Bounds, Scope, TransitionSystem};

use super::reference::{self, REFERENCE_CAP};
use super::{Canon, Group, Rule};

fn system(src: &str, bounds: Bounds) -> TransitionSystem {
    TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts
}

fn bounds(n: u8, per_entity: &[(&str, u8)]) -> Bounds {
    let mut b = Bounds::uniform(NonZeroU8::new(n).unwrap(), 2);
    for (id, k) in per_entity {
        b.per_entity
            .insert(id.to_string(), NonZeroU8::new(*k).unwrap());
    }
    b
}

/// Every state of `ts`: each slot of each var at each value of its domain.
fn every_state(ts: &TransitionSystem) -> Vec<State> {
    let mut sizes = Vec::new();
    for v in ts.var_ids() {
        let d = ts.var(v);
        for _ in 0..ts.slots_of(&d.kind) {
            sizes.push(d.domain.size(ts) as u8);
        }
    }
    let total: u64 = sizes.iter().map(|s| u64::from(*s)).product();
    assert!(total <= 200_000, "{total} states is too many to enumerate");
    let mut out = Vec::with_capacity(total as usize);
    let mut cur = vec![0u8; sizes.len()];
    loop {
        // `cur` is in var order, slot by slot, which is the state's own layout.
        out.push(State(cur.clone().into_boxed_slice()));
        let mut i = 0;
        loop {
            if i == sizes.len() {
                return out;
            }
            cur[i] += 1;
            if cur[i] < sizes[i] {
                break;
            }
            cur[i] = 0;
            i += 1;
        }
    }
}

/// For every state: the canonical form is the twin's, is some `g(s)`, is idempotent, is
/// the same for every `g(s)`, and the orbit it reports has as many states as `G_s` makes.
fn exhaustive(ts: &TransitionSystem) -> usize {
    assert_eq!(
        ts.var_ids()
            .map(|v| ts.slots_of(&ts.var(v).kind))
            .sum::<usize>(),
        ts.width,
        "the enumeration's layout is the state's"
    );
    let g = Group::of(ts);
    assert!(!g.is_trivial(), "the layout has something to permute");
    let mut canon = Canon::default();
    let mut out = vec![0u8; ts.width];
    let mut again = vec![0u8; ts.width];
    let mut image_out = vec![0u8; ts.width];
    let mut checked = 0;
    for s in every_state(ts) {
        let found = canon.canonical(&g, &s.0, &mut out);
        assert!(!found.cut, "no cut at this size");
        let twin = reference::canonical(ts, &g, &s).expect("small enough to enumerate");
        assert_eq!(out, twin.canonical, "state {:?}", s.0);
        let images: BTreeSet<Vec<u8>> = reference::elements(ts, &g, &s)
            .unwrap()
            .iter()
            .map(|perm| reference::apply(ts, &g, perm, &s))
            .collect();
        assert!(
            images.contains(&out),
            "the canonical form is some g(s): {:?}",
            s.0
        );
        assert_eq!(found.orbit, images.len() as u128, "orbit of {:?}", s.0);
        assert_eq!(found.orbit, twin.size / twin.aut);
        canon.canonical(&g, &out, &mut again);
        assert_eq!(again, out, "idempotent on {:?}", s.0);
        for image in &images {
            canon.canonical(&g, image, &mut image_out);
            assert_eq!(image_out, out, "g(s) {image:?} of {:?}", s.0);
        }
        // The permutation it reports takes `s` to its canonical form.
        g.apply(canon_perm(&mut canon, &g, &s), &s.0, &mut again);
        assert_eq!(again, out);
        checked += 1;
    }
    checked
}

fn canon_perm<'a>(canon: &'a mut Canon, g: &Group, s: &StateSlice) -> &'a [u8] {
    let mut out = vec![0u8; s.0.len()];
    canon.canonical(g, &s.0, &mut out);
    canon.perm()
}

#[test]
fn one_class_of_three_slots_with_a_three_valued_column() {
    let ts = system(
        "entity Item \"品\" { id: Int @pk  st: Enum(a, b, c) @default(a) }
usecase Make \"作\"
usecase Turn \"回\"
creates(Make, Item)
transitions(Turn, Item.st, a -> b)
transitions(Turn, Item.st, b -> c)",
        bounds(3, &[]),
    );
    assert!(exhaustive(&ts) > 0);
}

#[test]
fn a_parent_and_an_owner_child() {
    let ts = system(
        "entity Shop \"店\" { id: Int @pk  kind: Enum(x, y) @default(x) }
entity Order \"注文\" { id: Int @pk  st: Enum(o, c) @default(o) }
relate(Order, Shop, N:1)
usecase Open \"開\"
usecase Place \"注\"
usecase Kind \"種\"
usecase Close \"閉\"
creates(Open, Shop)
creates(Place, Order)
transitions(Kind, Shop.kind, x -> y)
transitions(Close, Order.st, o -> c)",
        bounds(2, &[("Shop", 3)]),
    );
    assert!(exhaustive(&ts) > 0);
}

#[test]
fn an_identity_pair() {
    let ts = system(
        "entity Account \"口座\" { id: Int @pk  tier: Enum(a, b) @default(a) }
entity Balance \"残高\" { id: Int @pk  amt: Enum(z, p) @default(z) }
relate(Balance, Account, 1:1)
usecase Tier \"格\"
usecase Pay \"払\"
transitions(Tier, Account.tier, a -> b)
transitions(Pay, Balance.amt, z -> p)",
        bounds(2, &[]),
    );
    let g = Group::of(&ts);
    assert_eq!(
        g.classes.iter().filter(|c| c.entities.len() == 2).count(),
        1,
        "the identity start couples Balance and Account"
    );
    assert!(exhaustive(&ts) > 0);
}

#[test]
fn a_self_owning_entity() {
    let ts = system(
        "entity Node \"節\" { id: Int @pk  st: Enum(a, b) @default(a) }
relate(Node, Node, N:1).optional()
usecase Add \"加\"
usecase Flip \"変\"
creates(Add, Node)
transitions(Flip, Node.st, a -> b)",
        bounds(3, &[]),
    );
    let g = Group::of(&ts);
    assert!(
        g.blocks.iter().any(|b| b.joint),
        "a self-owned entity is a joint block"
    );
    assert!(exhaustive(&ts) > 0);
}

#[test]
fn the_reference_enumerates_no_more_than_its_cap() {
    assert_eq!(REFERENCE_CAP, 6u128.pow(4));
}

/// The trap the corpus cannot state, since the DSL cannot name a slot (`sym_index_named`):
/// a guard that orders row ids leaves the entity's class out, and the canonical form then
/// moves none of its rows.
#[test]
fn a_guard_that_orders_row_ids_leaves_the_class_out() {
    let mut ts = system(
        "entity Item \"品\" { id: Int @pk  st: Enum(a, b) @default(a) }
usecase Make \"作\"
usecase Turn \"回\"
creates(Make, Item)
transitions(Turn, Item.st, a -> b)",
        bounds(3, &[]),
    );
    let before = Group::of(&ts);
    assert!(
        before.left_out().is_empty(),
        "nothing breaks the plain model"
    );
    let turn = ts
        .actions
        .iter()
        .position(|a| a.name == "Turn")
        .expect("Turn");
    ts.actions[turn].guards.push(Guard {
        pred: Pred::Cmp(Term::Index(At::Param(0)), CmpOp::Lt, Term::Int(1)),
        why: GuardWhy::Clock,
        site: Site::bare(ModelRef::Synthesized),
    });
    ts.layout();
    let g = Group::of(&ts);
    assert_eq!(g.left_out().len(), 1);
    assert_eq!(g.left_out()[0].entity, "Item");
    assert_eq!(g.left_out()[0].rule, Rule::OrderedId);
    assert!(g.is_trivial());
    // Two present rows apart: slot 0 at b, slot 1 at a. The group would swap them.
    let exists = ts.entities[0].exists;
    let st = ts.entities[0].columns[0];
    let mut s = ts.base_state();
    for slot in 0..2 {
        s.set(&ts, exists, slot, Value::PRESENT);
    }
    s.set(&ts, st, 0, Value(1));
    let mut out = vec![0u8; ts.width];
    Canon::default().canonical(&g, &s.0, &mut out);
    assert_eq!(
        out,
        s.0.to_vec(),
        "a class left out keeps its rows where they are"
    );
}
