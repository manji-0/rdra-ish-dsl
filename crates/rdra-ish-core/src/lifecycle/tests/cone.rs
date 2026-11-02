//! The cone reduction is exact: on every sample whose components finish, each target's cone
//! reaches exactly the projection of the whole component's states, and gives the same view
//! and the same verdict. A cone that frees an unread entity's slots reaches a superset.

use std::collections::BTreeSet;

use super::*;
use crate::lifecycle::{EntityStateResult, Exploration, State, Target};
use crate::load_project;

pub(super) fn load(rel: &str) -> SemanticModel {
    load_project(&[repo_root().join(rel)], None)
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
        .model
}

/// A view as an order-free value: per pattern its row, flags, and VIA.
fn view_set(v: &EntityStateResult) -> BTreeSet<String> {
    v.patterns
        .iter()
        .map(|p| {
            format!(
                "{:?} initial={} terminal={} via={:?}",
                p.pattern.values, p.is_initial, p.is_terminal, p.provenance.via
            )
        })
        .collect()
}

fn violated(diags: &[&StateDiag], target: Target, ts: &TransitionSystem) -> bool {
    diags.iter().any(|d| match (target, d) {
        (Target::Invariant(i), StateDiag::ConstraintViolated { name, .. }) => {
            *name == ts.invariants()[i].name
        }
        (Target::AfterAssert(i), StateDiag::AfterAssertViolated { site, .. }) => {
            *site == ts.after_asserts()[i].site
        }
        _ => false,
    })
}

pub(super) const SAMPLES: &[&str] = &[
    "samples/ec-site",
    "samples/personal-info",
    "samples/formal-verification/after_assert_unmet.rdra",
    "samples/formal-verification/check_met.rdra",
    "samples/formal-verification/check_unmet.rdra",
    "samples/formal-verification/cross_order_payment.rdra",
    "samples/formal-verification/cross_order_payment_split.rdra",
    "samples/formal-verification/deadlock.rdra",
    "samples/formal-verification/int_stock.rdra",
    "samples/formal-verification/min_card_met.rdra",
    "samples/formal-verification/min_card_unmet.rdra",
    "samples/formal-verification/none_any_child.rdra",
    "samples/formal-verification/none_any_child_bypassed.rdra",
    "samples/formal-verification/now_coupon.rdra",
    "samples/formal-verification/one_to_one_shared.rdra",
    "samples/formal-verification/order.rdra",
    "samples/formal-verification/partial_scope.rdra",
    "samples/formal-verification/quantifier_none.rdra",
    "samples/formal-verification/untriggered_sets.rdra",
    "samples/formal-verification/unwitnessed_state.rdra",
    "samples/formal-verification-fail/order.rdra",
];

#[test]
fn cone_exploration_equals_the_projection_of_the_whole_component() {
    let mut checked = 0;
    let mut freed = 0;
    for sample in SAMPLES {
        let (exact, free) = assert_cones_exact(sample, &load(sample));
        assert!(exact > 0, "{sample}: no target checked exactly");
        checked += exact;
        freed += free;
    }
    assert!(
        checked >= 25,
        "checked {checked} targets exactly, {freed} freed"
    );
    assert!(freed > 0, "no sample frees a slot");
}

/// Each target's cone reaches the projection of its whole component's states, with the same
/// view and verdict; a cone that frees an unread entity's slots reaches a superset. Returns the
/// targets checked exactly and the freed ones.
pub(super) fn assert_cones_exact(sample: &str, model: &SemanticModel) -> (usize, usize) {
    let caps = Caps::new(usize::MAX, 200_000);
    let mut checked = 0;
    let mut freed = 0;
    let built = TransitionSystem::build(model, Bounds::STATES, &Scope::ALL);
    for comp in built.ts.components() {
        let sub = built.ts.restrict(&comp);
        let full = sub.clone().run(caps);
        assert!(full.complete, "{sample}: component does not finish");
        let whole = Exploration { runs: vec![full] };
        let full_views = whole.entity_views(model, &[]);
        let full_diags: Vec<&StateDiag> = whole.diags().map(|d| &d.diag).collect();
        let full_run = &whole.runs[0];
        let cones = sub.explore_targets(&sub.targets(), caps);
        let cone_views = cones.entity_views(&sub, model, &[]);
        let cone_diags: Vec<&StateDiag> = cones.verdicts().iter().map(|d| &d.diag).collect();
        for target in sub.targets() {
            let cr = cones.cone_of(target);
            let projected: BTreeSet<State> = full_run
                .states
                .iter()
                .map(|s| sub.project(s, &cr.cone.vars))
                .collect();
            let reached: BTreeSet<State> = cr.run.states.iter().cloned().collect();
            if sub.cone_frees_slots(&cr.cone) {
                // An entity the target does not read never runs out of slots.
                assert!(
                    projected.is_subset(&reached),
                    "{sample} {target:?}: a freed cone still reaches the projection"
                );
                freed += 1;
                continue;
            }
            assert_eq!(projected, reached, "{sample} {target:?}: reachable states");
            match target {
                Target::Entity(e) => {
                    let id = &sub.entity(e).id;
                    let a = full_views.iter().find(|v| v.entity_id == *id).unwrap();
                    let b = cone_views.iter().find(|v| v.entity_id == *id).unwrap();
                    assert_eq!(view_set(a), view_set(b), "{sample} {id}: view");
                }
                _ => assert_eq!(
                    violated(&full_diags, target, &sub),
                    violated(&cone_diags, target, &sub),
                    "{sample} {target:?}: verdict"
                ),
            }
            checked += 1;
        }
    }
    (checked, freed)
}

#[test]
fn a_parent_existence_joins_the_cone_without_its_columns() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, closed) }
entity Line "明細" { id: Int @pk  state: Enum(added, removed) }
usecase Open "開く"
usecase Close "締める"
usecase AddLine "追加"
relate(Line, Order, N:1)
creates(Open, Order)
transitions(Close, Order.status, open -> closed)
creates(AddLine, Line)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let ts = &built.ts;
    let order = ts.entities().iter().find(|e| e.id == "Order").unwrap();
    let line = ts
        .targets()
        .into_iter()
        .find(|t| matches!(t, Target::Entity(e) if ts.entity(*e).id == "Line"))
        .unwrap();
    let cone = ts.target_cone(line);
    assert!(cone.vars.contains(&order.exists));
    assert!(
        !cone.vars.contains(&order.columns[0]),
        "Order.status stays out"
    );
    let sub = ts.restrict_to(&cone.vars, &cone.keep);
    let names: Vec<&str> = sub.actions().iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["AddLine", "Open"],
        "Close writes nothing in the cone"
    );
}

#[test]
fn an_entity_the_view_does_not_read_never_runs_out_of_slots() {
    // Every card and address use case also creates an AuditLog. With one slot per entity the
    // first creator would spend it; the card and address views do not read AuditLog.
    let model = load("samples/personal-info");
    let report = crate::lifecycle::derive_states(&model, &Scope::ALL, Caps::STATES);
    let via = |id: &str| -> BTreeSet<String> {
        report
            .views
            .iter()
            .find(|v| v.entity_id == id)
            .unwrap()
            .patterns
            .iter()
            .flat_map(|p| p.provenance.via.iter().map(|(_, uc)| uc.clone()))
            .collect()
    };
    for uc in ["RegisterCard", "DeleteCard"] {
        assert!(via("CreditCard").contains(uc), "CreditCard via {uc}");
    }
    for uc in ["RegisterAddress", "DeleteAddress"] {
        assert!(via("PostalAddress").contains(uc), "PostalAddress via {uc}");
    }
}

/// The run's edges are exactly the moves of each expanded state, in order, and every state's
/// parent path replays through `moves` from an initial state to that state.
#[test]
fn every_state_replays_from_its_parent_path_through_moves() {
    let caps = Caps::new(usize::MAX, 200_000);
    let mut replayed = 0;
    for sample in SAMPLES {
        let model = load(sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
            for cr in &built.ts.explore_targets(&built.ts.targets(), caps).cones {
                let run = &cr.run;
                let ts = &run.system;
                assert!(run.complete, "{sample}: cone does not finish");
                let mut edges = run.edges().iter();
                for (from, s) in run.states.iter().enumerate() {
                    for m in ts.moves(s) {
                        let e = edges.next().expect("an edge per move");
                        assert_eq!(e.from as usize, from, "{sample}: edge source");
                        assert_eq!(run.step(e), (m.act, &m.params[..]), "{sample}: binding");
                        assert_eq!(run.states[e.to as usize], m.next, "{sample}: successor");
                    }
                }
                assert!(edges.next().is_none(), "{sample}: an edge with no move");
                for at in 0..run.states.len() as u32 {
                    let path = run.path_to(at);
                    let start = path.first().map_or(at, |e| e.from);
                    assert!(run.parent(start).is_none(), "{sample}: path starts initial");
                    let mut cur = run.states[start as usize].clone();
                    for e in &path {
                        assert_eq!(run.states[e.from as usize], cur, "{sample}: path is joined");
                        let (act, params) = run.step(e);
                        let next = &run.states[e.to as usize];
                        assert!(
                            ts.moves(&cur).any(|m| m.act == act
                                && &m.params[..] == params
                                && m.next == *next),
                            "{sample}: the parent edge of {} is a move",
                            e.to
                        );
                        cur = next.clone();
                    }
                    assert_eq!(
                        cur, run.states[at as usize],
                        "{sample}: replay ends at {at}"
                    );
                    let states = std::iter::once(start)
                        .chain(path.iter().map(|e| e.to))
                        .map(|i| run.states[i as usize].clone())
                        .collect();
                    let acts: Vec<_> = path.iter().map(|e| run.step(e).0).collect();
                    let replay = ts.replay(states, &acts).expect("a BFS path replays");
                    assert_eq!(
                        ts.replay_trace(&replay),
                        run.trace_to(at),
                        "{sample}: the replay of the path to {at} is its BFS trace"
                    );
                    replayed += 1;
                }
            }
        }
    }
    assert!(replayed > 1000, "replayed {replayed} states");
}

/// A property's cone holds what the formula reads and each fair action whole: its guard's
/// reads on another entity, the slots it creates, and the columns it writes that the
/// formula does not read. Without `.fair` the cone is the formula's reads, closed.
#[test]
fn a_property_cone_keeps_its_fair_actions_whole() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  note: Bool @default(false)
}
entity Warehouse "倉庫" { id: Int @pk  state: Enum(closed, open) @default(closed) }
entity Receipt "領収書" { id: Int @pk }
entity Ticket "問い合わせ" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Build "開設"
usecase OpenWh "開場"
usecase Ask "起票"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
sets(Ship, Order, note == true)
creates(Ship, Receipt)
creates(Build, Warehouse)
transitions(OpenWh, Warehouse.state, closed -> open)
creates(Ask, Ticket)
condition Open "倉庫が開いている"
holds(Open, Order.status == paid, Warehouse.state == open)
applies_to(Open, Ship)
property Fair leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
property Unfair leads_to(Order.status == paid, Order.status == shipped)
"#;
    let model = model_from(src);
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let names = |vars: &BTreeSet<crate::lifecycle::VarId>| -> BTreeSet<String> {
        vars.iter()
            .map(|v| match &ts.var(*v).kind {
                crate::lifecycle::VarKind::Exists(e) => ts.entity(*e).id.clone(),
                crate::lifecycle::VarKind::Column { entity, column, .. } => {
                    format!("{}.{column}", ts.entity(*entity).id)
                }
                crate::lifecycle::VarKind::Owner { child, parent, .. } => {
                    format!("{}->{}", ts.entity(*child).id, ts.entity(*parent).id)
                }
                other => format!("{other:?}"),
            })
            .collect()
    };
    let set =
        |items: &[&str]| -> BTreeSet<String> { items.iter().map(|s| s.to_string()).collect() };

    let fair = ts.property_cone(0);
    assert_eq!(
        names(&fair.vars),
        set(&[
            "Order",
            "Order.status",
            "Order.note",
            "Warehouse",
            "Warehouse.state",
            "Receipt",
            "Receipt->Order",
        ]),
        "the fair action's guard reads, created slots, and writes are in the cone"
    );
    let ship = ts
        .actions()
        .iter()
        .position(|a| a.name == "Ship")
        .expect("Ship");
    assert_eq!(fair.keep.len(), 1);
    assert!(fair.keep.contains(&crate::lifecycle::ActId::at(ship)));
    assert!(!ts.cone_frees_slots(&fair));
    assert!(
        !ts.cone_keeps_every_write(&fair.vars),
        "Ask writes the ticket outside it"
    );

    // Ship writes the status, so the closure keeps its guard's reads; its created receipt
    // and its note are not read by anything in the cone.
    let unfair = ts.property_cone(1);
    assert_eq!(
        names(&unfair.vars),
        set(&["Order", "Order.status", "Warehouse", "Warehouse.state"])
    );
    assert!(unfair.keep.is_empty());
    assert!(
        ts.cone_frees_slots(&unfair),
        "Ship creates a receipt the cone does not count"
    );
}
