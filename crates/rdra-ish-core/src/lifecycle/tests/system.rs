//! Whole-system behaviour: order.rdra, bounds.

use super::*;
use crate::lifecycle::Birth;

#[test]
fn order_sample_reaches_its_five_patterns() {
    let model = sample_model("samples/formal-verification/order.rdra");
    let d = derive(&model);
    let order = d.entity("Order");
    assert_eq!(order.patterns.len(), 5);
    assert_eq!(order.bound(), 10);
    assert_eq!(
        d.rows("Order"),
        vec![
            "delivered_at=null, status=pending",
            "delivered_at=null, status=paid",
            "delivered_at=null, status=cancelled",
            "delivered_at=null, status=shipped",
            "delivered_at=present, status=delivered",
        ]
    );
    let flags: Vec<(bool, bool)> = order
        .patterns
        .iter()
        .map(|p| (p.is_initial, p.is_terminal))
        .collect();
    assert_eq!(
        flags,
        vec![
            (true, false),
            (false, false),
            (false, true),
            (false, false),
            (false, true)
        ]
    );
    let via: Vec<Vec<String>> = order
        .patterns
        .iter()
        .map(|p| p.provenance.via.iter().map(|(_, uc)| uc.clone()).collect())
        .collect();
    assert_eq!(
        via[4],
        vec!["CapturePay", "DeliverOrder", "PlaceOrder", "ShipOrder"]
    );
    assert!(order.diagnostics.is_empty(), "{:?}", order.diagnostics);
    assert!(!has_errors(&d.diags), "{:?}", d.diags);
}

fn has_errors(diags: &[Diag]) -> bool {
    crate::lifecycle::has_errors(diags)
}

#[test]
fn use_case_is_one_atomic_action_across_entities() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) }
entity Payment "決済" { id: Int @pk  state: Enum(open, captured) }
usecase Place "注文"
usecase Pay "支払"
relate(Payment, Order, N:1)
creates(Place, Order)
creates(Pay, Payment)
updates(Pay, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Payment.state == captured)
forbidden(Order, Payment, Order.status == pending, Payment.state == captured)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let pay = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "Pay")
        .expect("Pay action");
    assert_eq!(pay.params.len(), 2, "one param per touched entity");
    let d = derive(&model);
    assert!(
        !d.diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. })),
        "the in-between state never exists: {:?}",
        d.diags
    );
    assert_eq!(d.rows("Payment"), vec!["state=captured"]);
}

#[test]
fn created_child_binds_to_a_present_parent() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, closed) }
entity Line "明細" { id: Int @pk  state: Enum(added, removed) }
usecase Open "開く"
usecase AddLine "追加"
relate(Line, Order, N:1)
creates(Open, Order)
creates(AddLine, Line)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let add = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "AddLine")
        .unwrap();
    assert!(add
        .params
        .iter()
        .any(|p| !p.role.is_written() && built.ts.entity(p.entity).id == "Order"));
    let run = built.ts.clone().run(Caps::new(256, 1000));
    // absent/absent, open/absent, open/added
    assert_eq!(run.states.len(), 3);
}

#[test]
fn entity_no_use_case_creates_starts_present_with_free_variations() {
    let src = r#"
variation Tier "階層" {
  gold "G"
  silver "S"
}
entity Member "会員" { id: Int @pk  tier: Tier  active: Bool @default(true) }
usecase Promote "昇格"
sets(Promote, Member.tier == gold)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    assert_eq!(built.ts.entities()[0].birth, Birth::PreExisting);
    let d = derive(&model);
    let member = d.entity("Member");
    assert!(member.no_creation_path);
    assert!(member.patterns.iter().all(|p| p.is_initial));
    assert_eq!(
        d.rows("Member"),
        vec!["active=true, tier=gold", "active=true, tier=silver"]
    );
    assert!(d.has("Member", |x| *x == StateDiag::NoCreationPath));
}

#[test]
fn verify_bounds_index_two_slots_and_take_the_lowest_absent() {
    let src = r#"
entity Ticket "券" { id: Int @pk  used: Bool }
usecase Issue "発行"
usecase Use "使用"
creates(Issue, Ticket)
sets(Use, Ticket.used == true)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL);
    let caps = Caps::new(256, 1000);
    let run = built.ts.clone().run(caps);
    // Slots fill in order: (a,a) (p,a) (p,p); each present slot is used or not.
    // (a,a) + (f,a) (t,a) + (f,f) (t,f) (f,t) (t,t) = 7
    assert_eq!(run.states.len(), 7);
    let cones = built.ts.explore_targets(&built.selection.targets, caps);
    let views = cones.entity_views(&built.ts, &model, &built.diags);
    assert_eq!(views[0].patterns.len(), 2);
}

#[test]
fn now_ticks_only_when_a_predicate_reads_it() {
    let model = sample_model("samples/formal-verification/now_coupon.rdra");
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    assert!(built.ts.now().is_some());
    assert!(built.ts.actions().iter().any(|a| a.name == "Tick"));
    let d = derive(&model);
    assert!(
        d.has("Coupon", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "a tick makes expired_at < now: {:?}",
        d.entity("Coupon").diagnostics
    );
    assert!(d.has("Coupon", |x| matches!(
        x,
        StateDiag::UnwrittenConstrainedVar { .. }
    )));
    let plain = model_from(
        r#"
entity Coupon "券" { id: Int @pk  state: Enum(live, used) }
usecase Grant "付与"
creates(Grant, Coupon)
"#,
    );
    let built = TransitionSystem::build(&plain, Bounds::STATES, &Scope::ALL);
    assert!(built.ts.now().is_none());
}

#[test]
fn buc_scope_keeps_birth_and_drops_other_use_cases() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, closed) }
usecase Open "開く"
usecase Close "締める"
buc B1 "開始"
buc B2 "終了"
contains(B1, Open)
contains(B2, Close)
creates(Open, Order)
transitions(Close, Order.status, open -> closed)
"#;
    let model = model_from(src);
    let b1 = derive_scoped(&model, Bounds::STATES, &Scope::from_bucs(&["B1".into()]));
    assert_eq!(b1.rows("Order"), vec!["status=open"]);
    let b2 = derive_scoped(&model, Bounds::STATES, &Scope::from_bucs(&["B2".into()]));
    assert!(
        b2.entity("Order").patterns.is_empty(),
        "Order is still created, by a use case outside the scope"
    );
}

/// `VIA` names the use cases on a path into a pattern. A use case that only reads the entity
/// leads into none of them, so it names no row, whatever pattern the slot holds when it runs.
#[test]
fn via_leaves_out_a_use_case_that_only_reads_the_entity() {
    let d = derive_src(
        r#"
usecase Open "Open"
usecase Close "Close"
usecase Show "Show"
usecase Notify "Notify"
api DocApi "Doc API"
entity Doc "届出" { id: Int @pk  status: Enum(open, closed) @default(open) }
creates(Open, Doc)
transitions(Close, Doc.status, open -> closed)
reads(Show, Doc)
invokes(Notify, DocApi)
updates(DocApi, Doc)
"#,
    );
    let via: Vec<Vec<String>> = d
        .entity("Doc")
        .patterns
        .iter()
        .map(|p| p.provenance.via.iter().map(|(_, uc)| uc.clone()).collect())
        .collect();
    assert_eq!(via, vec![vec!["Open"], vec!["Close", "Open"]]);
}
