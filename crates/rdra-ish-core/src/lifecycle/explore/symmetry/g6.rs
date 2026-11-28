//! (G6) of Theorem A18 for the predicates the scan does not read: invariants, `after`
//! assertions, properties, and `can_reach`. `Rule::AbsentOwner` covers action guards, where a
//! `New` parameter names a row before it exists. Outside guards, every read a verdict depends
//! on happens where each row it names is present or deleted, whatever the predicate is:
//!
//! - an invariant binds present slots (`find_violation`), and its quantifiers range over
//!   present slots;
//! - an `after` assertion reads the state after its step, where every parameter's row is
//!   present or deleted (a `New` row was just created; the others were present before), and
//!   its binders are present;
//! - a property's trigger is `present ∧ link` (`TransitionSystem::triggers`), and `always`,
//!   `stable`, `unchanged`, `precedes` and a `leads_to` antecedent are read only where the
//!   trigger holds or, for the second state of a step, where the rows are present; the goal
//!   of `eventually` and `leads_to` is read on the states after a trigger, where a row that
//!   was present is present or deleted, since a slot never becomes absent again;
//! - `can_reach` reads its link and goal only where the bound rows are present (`reach.rs`).
//!
//! `Group::apply` relabels an owner on a present or deleted row (G5), so these reads take the
//! same value on `s` and `g(s)`. The test below checks that, on built models that read owners
//! through `.along` and quantifiers, on every reachable state and its canonical form. It
//! also gives a goal a read of an absent row's key by hand: the read differs on the absent
//! rows, and no gated read and no verdict does.

use std::collections::BTreeSet;
use std::num::NonZeroU8;

use crate::lifecycle::explore::Caps;
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::tests::model_from;
use crate::lifecycle::types::{At, CmpOp, EntId, Ltl, Pred, Term, Value};
use crate::lifecycle::{Bounds, Fairness, PropertyVerdict, Scope, TransitionSystem};

use super::{Canon, Group};

/// Orders under shops, read through the order's key by a cross `forbidden`, an `at_most`,
/// and every property form with `.along`; `Drop` deletes an order, and two `after`
/// assertions read the order of `Pay` and of `Drop`.
const SHOPS: &str = r#"
entity Shop "店" { id: Int @pk  tier: Enum(basic, gold) @default(basic) }
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending)  rush: Enum(no, yes) @default(no) }
relate(Order, Shop, N:1)
usecase OpenShop "開店"
usecase Promote "昇格"
usecase Place "注文"
usecase Pay "支払"
usecase Hurry "急ぎ"
usecase Drop "取消"
creates(OpenShop, Shop)
transitions(Promote, Shop.tier, basic -> gold)
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Hurry, Order.rush, no -> yes)
deletes(Drop, Order)
forbidden(Order, Shop, Order.rush == yes, Shop.tier == basic).along(Order, Shop)
invariant(Shop).at_most(1, Order.status == paid)
after(Pay).assert(Order.status == paid)
after(Drop).assert(Order.status == pending)
property PaidPromotes leads_to(Order.status == paid, Shop.tier == gold).along(Order, Shop)
property ShopGold eventually(Shop.tier == gold).along(Order, Shop)
property CanPromote can_reach(Shop.tier == gold).along(Order, Shop)
property GoldStays stable(Shop.tier == gold).along(Order, Shop)
property PaidBefore precedes(Order.status == paid, Shop.tier == gold).along(Order, Shop)
"#;

fn system(src: &str) -> TransitionSystem {
    let bounds = Bounds::uniform(NonZeroU8::new(2).expect("not zero"), 2);
    TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts
}

fn entity(ts: &TransitionSystem, id: &str) -> EntId {
    (ts.entity_ids())
        .find(|&e| ts.entity(e).id == id)
        .unwrap_or_else(|| panic!("no entity {id}"))
}

/// Each reachable state with its canonical form and the permutation that takes it there.
fn forms(ts: &TransitionSystem, g: &Group) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let plain = ts.clone().run(Caps::new(usize::MAX, 200_000));
    assert!(plain.complete, "the model is small enough to run plainly");
    let mut canon = Canon::default();
    (plain.states.iter())
        .map(|s| {
            let mut rep = vec![0u8; s.0.len()];
            canon.canonical(g, &s.0, &mut rep);
            let sigma = canon.perm().to_vec();
            let mut moved = vec![0u8; s.0.len()];
            g.apply(&sigma, &s.0, &mut moved);
            assert_eq!(moved, rep, "the permutation takes the state to its form");
            (s.0.to_vec(), rep, sigma)
        })
        .collect()
}

fn mapped(g: &Group, sigma: &[u8], binders: &[EntId], b: &[u8]) -> Vec<u8> {
    (binders.iter().zip(b))
        .map(|(&e, &slot)| g.map_slot(sigma, e, slot))
        .collect()
}

fn all(ts: &TransitionSystem, binders: &[EntId], b: &[u8], s: &StateSlice, ok: &[Value]) -> bool {
    (binders.iter().zip(b))
        .all(|(&e, &slot)| ok.contains(&s.get(ts, ts.entity(e).exists, usize::from(slot))))
}

/// Every read a verdict depends on, outside action guards, takes the same value on each
/// reachable `s` at a binding `b` as on its canonical form `σ(s)` at `σ(b)`; `Err` names the
/// first that does not.
fn g6(ts: &TransitionSystem) -> Result<(), String> {
    let g = Group::of(ts);
    let present = [Value::PRESENT];
    let there = [Value::PRESENT, Value::DELETED];
    for (s, rep, sigma) in forms(ts, &g) {
        let (s, rep) = (StateSlice::new(&s), StateSlice::new(&rep));
        let same = |what: &str, x: &Pred, binders: &[EntId], b: &[u8]| {
            let b2 = mapped(&g, &sigma, binders, b);
            match ts.holds_bound(x, s, b) == ts.holds_bound(x, rep, &b2) {
                true => Ok(()),
                false => Err(format!("{what} at {b:?} on {:?}", &s.0)),
            }
        };
        for inv in ts.invariants() {
            for b in ts.binder_slots(&inv.binders) {
                if all(ts, &inv.binders, &b, s, &present) {
                    same(&inv.name, &inv.body, &inv.binders, &b)?;
                }
            }
        }
        for p in ts.properties() {
            for b in ts.binder_slots(&p.binders) {
                let b2 = mapped(&g, &sigma, &p.binders, &b);
                if ts.triggers(p, &b, s) != ts.triggers(p, &b2, rep) {
                    return Err(format!("{}'s trigger at {b:?} on {:?}", p.name, &s.0));
                }
                if all(ts, &p.binders, &b, s, &there) {
                    for x in p.formula.preds() {
                        same(&p.name, x, &p.binders, &b)?;
                    }
                }
            }
        }
        for r in ts.reach_properties() {
            for b in ts.binder_slots(&r.binders) {
                if all(ts, &r.binders, &b, s, &present) {
                    for x in r.preds() {
                        same(&r.name, x, &r.binders, &b)?;
                    }
                }
            }
        }
        for m in ts.moves(s) {
            let action = &ts.actions()[m.act.index()];
            let params: Vec<u8> = (action.params.iter().zip(&m.params[..]))
                .map(|(p, &slot)| g.map_slot(&sigma, p.entity, slot))
                .collect();
            let mut t2 = vec![0u8; m.next.0.len()];
            g.apply(&sigma, &m.next.0, &mut t2);
            let t2 = StateSlice::new(&t2);
            for a in ts.after_asserts() {
                for step in a.steps.iter().filter(|step| step.act == m.act) {
                    let here = ts.find_violation(&step.binders, &step.post, &m.next, &m.params);
                    let there = ts.find_violation(&step.binders, &step.post, t2, &params);
                    if here.is_some() != there.is_some() {
                        return Err(format!("{} after {:?} on {:?}", a.name, m.act, &s.0));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Whether the goal of property `i`, read at every binding whatever its rows' existence,
/// differs between some reachable state and its canonical form.
fn goal_differs_ungated(ts: &TransitionSystem, i: usize) -> bool {
    let g = Group::of(ts);
    let p = &ts.properties()[i];
    let Ltl::LeadsTo(_, goal) = &p.formula else {
        panic!("{} is a leads_to", p.name)
    };
    forms(ts, &g).iter().any(|(s, rep, sigma)| {
        ts.binder_slots(&p.binders).any(|b| {
            let b2 = mapped(&g, sigma, &p.binders, &b);
            ts.holds_bound(goal, StateSlice::new(s), &b)
                != ts.holds_bound(goal, StateSlice::new(rep), &b2)
        })
    })
}

/// Each `eventually` and `leads_to` verdict and each violated state rule, on a plain run and
/// on a run up to symmetry.
fn verdicts(ts: &TransitionSystem) -> (Vec<String>, Vec<String>) {
    let plain = ts.clone().run(Caps::VERIFY);
    let orbits = ts.clone().run_annotated_orbits(Caps::VERIFY);
    let kind = |v: PropertyVerdict| match v {
        PropertyVerdict::Holds => "holds",
        PropertyVerdict::Violated(_) => "violated",
        PropertyVerdict::Undecided => "undecided",
        PropertyVerdict::Fault(_) => "fault",
    };
    let mut one = Vec::new();
    let mut two = Vec::new();
    for (i, p) in ts.properties().iter().enumerate() {
        if matches!(p.formula, Ltl::Eventually(_) | Ltl::LeadsTo(..)) {
            let a = plain
                .decide_liveness(i, Fairness::Plain)
                .expect("no lasso fault");
            let b = orbits
                .decide_liveness(i, Fairness::Plain)
                .expect("no lasso fault");
            one.push(format!("{}: {}", p.name, kind(a)));
            two.push(format!("{}: {}", p.name, kind(b)));
        }
    }
    let names = |diags: &mut dyn Iterator<Item = String>| -> BTreeSet<String> { diags.collect() };
    let a = names(&mut plain.diags.iter().map(|d| d.diag.name(&[])));
    let b = names(&mut orbits.diags().iter().map(|d| d.diag.name(&[])));
    one.extend(a);
    two.extend(b);
    (one, two)
}

/// On a built model whose invariants, `after` assertions, properties and `can_reach` read
/// owners, with both classes in the group, every read a verdict depends on is the same on a
/// state and its canonical form (claim A18 of docs/theory/claims.md). A goal given, by hand, a
/// read of the order's key reads it on orders never created too, whose key `apply` does not
/// relabel, and differs there; the gated reads and the verdicts up to symmetry stay the plain
/// run's.
#[test]
fn reads_outside_guards_meet_no_absent_owner() {
    let mut ts = system(SHOPS);
    let g = Group::of(&ts);
    let (shop, order) = (entity(&ts, "Shop"), entity(&ts, "Order"));
    assert!(g.moves(shop) && g.moves(order), "{:?}", g.left_out());
    g6(&ts).expect("every gated read is the same on a state and its form");
    let (plain, orbits) = verdicts(&ts);
    assert_eq!(plain, orbits);

    let i = (ts.properties.iter())
        .position(|p| p.name == "PaidPromotes")
        .expect("the property");
    assert!(!goal_differs_ungated(&ts, i));
    let p = &mut ts.properties[i];
    let at = |e: EntId| {
        let k = p.binders.iter().position(|&b| b == e).expect("bound");
        At::Bound(u8::try_from(k).expect("few binders"))
    };
    let (o, s) = (at(order), at(shop));
    let key = ts.entities[order.index()].owners[0];
    let Ltl::LeadsTo(_, goal) = &mut p.formula else {
        panic!("PaidPromotes is a leads_to")
    };
    *goal = Pred::Or(vec![
        goal.clone(),
        Pred::Cmp(Term::Slot { var: key, at: o }, CmpOp::Eq, Term::Index(s)),
    ]);
    ts.layout();
    assert!(
        Group::of(&ts).moves(shop),
        "the scan does not read the goal"
    );
    assert!(goal_differs_ungated(&ts, i), "the goal reads absent keys");
    g6(&ts).expect("no gated read reaches an absent key");
    let (plain, orbits) = verdicts(&ts);
    assert_eq!(plain, orbits);
}
