//! A witness per group rule (docs/design/instance-symmetry.md, "Always by default", the
//! flip's gates): a system on which the rule leaves a class out, and on which dropping that
//! rule alone (`dropped`) makes the quotient fail the bisimulation check, the `orbit_run`
//! twin's check 2. No model the builder emits today needs a scan rule (breaks.rs), so each
//! system here is a built model with one hand-made change that the rule is there for.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::num::NonZeroU8;

use crate::lifecycle::explore::Caps;
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::tests::model_from;
use crate::lifecycle::types::{
    At, Choice, CmpOp, Effect, Guard, GuardWhy, ModelRef, ParamRole, Pred, Rhs, Site, Term, Value,
    VarId, VarKind,
};
use crate::lifecycle::{Bounds, Scope, TransitionSystem};

use super::{Canon, Group, Rule};

thread_local! {
    /// The rule a test on this thread drops (`dropped`).
    pub(super) static DROPPED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// `f` with the rule `name` dropped on this thread.
pub(super) fn dropping<T>(name: &str, f: impl FnOnce() -> T) -> T {
    DROPPED.with(|d| *d.borrow_mut() = Some(name.to_string()));
    let out = f();
    DROPPED.with(|d| *d.borrow_mut() = None);
    out
}

fn system(src: &str, n: u8) -> TransitionSystem {
    let bounds = Bounds::uniform(NonZeroU8::new(n).unwrap(), 2);
    TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts
}

/// Check 2 of the `orbit_run` twin on `ts`'s own group: every reachable state steps into the
/// classes its canonical form steps into. `Err` names the first state that does not.
fn bisimulation(ts: &TransitionSystem) -> Result<(), String> {
    let g = Group::of(ts);
    let plain = ts.clone().run(Caps::new(usize::MAX, 200_000));
    assert!(plain.complete, "the witness is small enough to run plainly");
    let mut canon = Canon::default();
    let mut form = |s: &[u8]| -> Vec<u8> {
        let mut out = vec![0u8; s.len()];
        canon.canonical(&g, s, &mut out);
        out
    };
    for s in plain.states.iter() {
        let rep = form(&s.0);
        let mine: BTreeSet<Vec<u8>> = ts.moves(&s).map(|m| form(&m.next.0)).collect();
        let theirs: BTreeSet<Vec<u8>> = ts
            .moves(StateSlice::new(&rep))
            .map(|m| form(&m.next.0))
            .collect();
        if mine != theirs {
            return Err(format!("{:?} and its form {rep:?} step apart", &s.0));
        }
    }
    Ok(())
}

/// `ts` leaves `entity` out for `rule`, its quotient is a bisimulation, and with `rule`
/// dropped alone the class moves and the quotient is not one.
fn witnesses(ts: &TransitionSystem, rule: Rule, entity: &str) {
    let g = Group::of(ts);
    assert!(
        g.left_out()
            .iter()
            .any(|l| l.rule == rule && l.entity == entity),
        "{rule:?} leaves {entity} out: {:?}",
        g.left_out()
    );
    bisimulation(ts).expect("the quotient with every rule is a bisimulation");
    let name = format!("{rule:?}");
    dropping(&name, || {
        assert!(
            Group::of(ts).left_out().iter().all(|l| l.entity != entity),
            "dropping {rule:?} alone lets {entity} move"
        );
        assert!(
            bisimulation(ts).is_err(),
            "dropping {rule:?} alone breaks the quotient"
        );
    });
}

fn action(ts: &TransitionSystem, name: &str) -> usize {
    ts.actions
        .iter()
        .position(|a| a.name == name)
        .unwrap_or_else(|| panic!("{name}"))
}

fn guard(pred: Pred) -> Guard {
    Guard {
        pred,
        why: GuardWhy::Clock,
        site: Site::bare(ModelRef::Synthesized),
    }
}

/// Items made and turned a -> b -> c, three of them: nothing tells two apart.
const ITEMS: &str = "entity Item \"品\" { id: Int @pk  st: Enum(a, b, c) @default(a) }
usecase Make \"作\"
usecase Turn \"回\"
usecase Finish \"終\"
creates(Make, Item)
transitions(Turn, Item.st, a -> b)
transitions(Finish, Item.st, b -> c)";

/// Shops promoted basic -> gold, and orders placed under one: a shop's tier tells shops
/// apart, and an order names its shop.
const SHOPS: &str = "entity Shop \"店\" { id: Int @pk  tier: Enum(basic, gold) @default(basic) }
entity Order \"注文\" { id: Int @pk }
relate(Order, Shop, N:1)
usecase Open \"開\"
usecase Promote \"昇\"
usecase Place \"注\"
creates(Open, Shop)
transitions(Promote, Shop.tier, basic -> gold)
creates(Place, Order)";

fn items() -> TransitionSystem {
    let ts = system(ITEMS, 3);
    assert!(Group::of(&ts).left_out().is_empty(), "nothing breaks ITEMS");
    ts
}

fn shops() -> TransitionSystem {
    let ts = system(SHOPS, 2);
    assert!(Group::of(&ts).left_out().is_empty(), "nothing breaks SHOPS");
    ts
}

/// The owner var of `Order.shop` and the assignment `Place` writes it with.
fn order_key(ts: &TransitionSystem) -> (VarId, usize, usize) {
    let key = ts
        .var_ids()
        .find(|v| matches!(ts.var(*v).kind, VarKind::Owner { .. }))
        .expect("an order names its shop");
    let place = action(ts, "Place");
    let at = ts.actions[place]
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Assign { var, .. } if *var == key))
        .expect("Place links the order to its shop");
    (key, place, at)
}

#[test]
fn ordered_id_has_a_witness() {
    let mut ts = items();
    let turn = action(&ts, "Turn");
    ts.actions[turn].guards.push(guard(Pred::Cmp(
        Term::Index(At::Param(0)),
        CmpOp::Lt,
        Term::Int(1),
    )));
    ts.layout();
    witnesses(&ts, Rule::OrderedId, "Item");
}

#[test]
fn id_against_literal_has_a_witness() {
    let mut ts = items();
    let turn = action(&ts, "Turn");
    ts.actions[turn].guards.push(guard(Pred::Cmp(
        Term::Index(At::Param(0)),
        CmpOp::Eq,
        Term::Int(0),
    )));
    ts.layout();
    witnesses(&ts, Rule::IdAgainstLiteral, "Item");
}

#[test]
fn mixed_ids_has_a_witness() {
    // `Turn` writes the turned row's own slot into its column.
    let mut ts = items();
    let st = ts.entities[0].columns[0];
    let turn = action(&ts, "Turn");
    ts.actions[turn].effects = vec![Effect::Assign {
        var: st,
        param: 0,
        rhs: Rhs::Param(0),
        site: Site::bare(ModelRef::Synthesized),
    }];
    ts.layout();
    witnesses(&ts, Rule::MixedIds, "Item");
}

#[test]
fn owner_literal_has_a_witness() {
    // Every order goes under the shop in slot 0: `Place` links it to the shop it binds, as
    // built, so the creation writes its key, and then moves it to slot 0.
    let mut ts = shops();
    let (_, place, at) = order_key(&ts);
    let mut to_zero = ts.actions[place].effects[at].clone();
    if let Effect::Assign { rhs, .. } = &mut to_zero {
        *rhs = Rhs::Lit(Value(0));
    }
    ts.actions[place].effects.insert(at + 1, to_zero);
    ts.layout();
    witnesses(&ts, Rule::OwnerLiteral, "Shop");
}

#[test]
fn owner_choice_has_a_witness() {
    // An order is placed under a shop chosen among slot 0 alone.
    let mut ts = shops();
    let (key, place, at) = order_key(&ts);
    let Effect::Assign { param, .. } = ts.actions[place].effects[at] else {
        unreachable!("an assignment")
    };
    ts.actions[place].effects[at] = Effect::Choose {
        var: key,
        param,
        values: vec![Choice {
            value: Value(0),
            site: None,
            conds: Vec::new(),
        }],
    };
    ts.layout();
    witnesses(&ts, Rule::OwnerChoice, "Shop");
}

#[test]
fn unlinked_create_has_a_witness() {
    // `Place` creates the order and leaves its key at the base value, the shop in slot 0.
    let mut ts = shops();
    let (_, place, at) = order_key(&ts);
    ts.actions[place].effects.remove(at);
    ts.layout();
    witnesses(&ts, Rule::UnlinkedCreate, "Shop");
}

#[test]
fn absent_owner_has_a_witness() {
    // `Place` runs only when the new order's key, read before the step, names the shop it
    // places under. The order is absent then and its key holds the base value, the shop in
    // slot 0, so orders go only under that shop, and a promoted shop in slot 0 is not a
    // promoted shop in slot 1 (P30).
    let mut ts = shops();
    let (key, place, _) = order_key(&ts);
    let params = &ts.actions[place].params;
    let new = params.iter().position(|p| p.role == ParamRole::New);
    let shop = params.iter().position(|p| p.role != ParamRole::New);
    let (Some(new), Some(shop)) = (new, shop) else {
        panic!("Place creates an order under a shop: {params:?}")
    };
    ts.actions[place].guards.push(guard(Pred::Cmp(
        Term::Slot {
            var: key,
            at: At::Param(new as u8),
        },
        CmpOp::Eq,
        Term::Index(At::Param(shop as u8)),
    )));
    ts.layout();
    witnesses(&ts, Rule::AbsentOwner, "Shop");
}

/// The names of the checks a run's diagnostics report violated.
fn violated<'a>(
    diags: impl Iterator<Item = &'a crate::lifecycle::diag::StateDiag>,
) -> BTreeSet<String> {
    use crate::lifecycle::diag::StateDiag;
    diags
        .filter(|d| {
            matches!(
                d,
                StateDiag::ConstraintViolated { .. } | StateDiag::AfterAssertViolated { .. }
            )
        })
        .map(|d| d.name(&[]))
        .collect()
}

/// Tasks spawned under a parent task, eight of them: one class whose rows name each other,
/// with 8! arrangements, past `JOINT_CAP`. No column, so that the run up to symmetry with
/// the cap dropped, which enumerates up to `BRANCH_CAP` arrangements a state, stays short.
const FOREST: &str = "entity Task \"作業\" { id: Int @pk }
relate(Task, Task, N:1).optional()
usecase Add \"追加\"
usecase Spawn \"子作業\"
api ParentApi \"親 API\"
api ChildApi \"子 API\"
creates(Add, Task)
invokes(Spawn, ParentApi)
updates(ParentApi, Task)
invokes(Spawn, ChildApi)
creates(ChildApi, Task)";

/// `JointTooLarge` caps what the canonicalizer enumerates, and is no soundness rule: with it
/// dropped, the block runs up to symmetry, its quotient is a bisimulation, and the run ends
/// with the plain run's verdicts.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "enumerates up to BRANCH_CAP arrangements a state; CI runs it in release"
)]
fn joint_too_large_is_a_cost_cap() {
    let ts = system(FOREST, 8);
    assert!(
        Group::of(&ts)
            .left_out()
            .iter()
            .any(|l| l.rule == Rule::JointTooLarge),
        "8! arrangements pass JOINT_CAP"
    );
    let caps = Caps::new(usize::MAX, 200_000);
    let plain = ts.clone().run(caps);
    assert!(plain.complete);
    dropping("JointTooLarge", || {
        assert!(Group::of(&ts).left_out().is_empty(), "the block moves");
        bisimulation(&ts).expect("the quotient is a bisimulation");
        let orbits = ts.clone().run_orbits(caps);
        assert!(orbits.complete());
        assert_eq!(orbits.end(), plain.end());
        assert_eq!(
            violated(orbits.diags().iter().map(|d| &d.diag)),
            violated(plain.diags.iter().map(|d| &d.diag))
        );
    });
}

/// Past `BRANCH_CAP` a state is stored without its canonical form, and the run still
/// decides: eight shops, each named by one of eight orders, tie in 8! ways. A shop can turn
/// gold, so `Forbidden_Shop_0` is violated, and the run that stores the cut states says so.
#[test]
fn past_the_branch_cap_the_verdict_stands() {
    let ts = system(
        "entity Shop \"店\" { id: Int @pk  tier: Enum(basic, gold) @default(basic) }
entity Order \"注文\" { id: Int @pk }
relate(Order, Shop, N:1)
usecase Open \"開\"
usecase Place \"注\"
usecase Promote \"昇\"
creates(Open, Shop)
creates(Place, Order)
transitions(Promote, Shop.tier, basic -> gold)
forbidden(Shop, Shop.tier == gold)",
        8,
    );
    let orbits = ts.clone().run_orbits(Caps::new(usize::MAX, 2_000_000));
    assert!(orbits.complete());
    assert!(orbits.cut() > 0, "some states pass BRANCH_CAP");
    let names = violated(orbits.diags().iter().map(|d| &d.diag));
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names.iter().all(|n| n.starts_with("Forbidden_Shop")),
        "{names:?}"
    );
}
