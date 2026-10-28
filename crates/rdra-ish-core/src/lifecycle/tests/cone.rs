//! The cone reduction is exact: on every sample whose components finish, each target's cone
//! reaches exactly the projection of the whole component's states, and gives the same view
//! and the same verdict. A cone that frees an unread entity's slots reaches a superset.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::*;
use crate::lifecycle::{EntityStateResult, Exploration, State, Target};
use crate::{build_merged_model, resolve};

fn load(rel: &str) -> SemanticModel {
    let root = repo_root().join(rel);
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            stack.extend(std::fs::read_dir(&p).unwrap().flatten().map(|e| e.path()));
        } else if p.extension().is_some_and(|e| e == "rdra") {
            files.push(p);
        }
    }
    files.sort();
    let mut includes: Vec<PathBuf> = Vec::new();
    for f in &files {
        let c = std::fs::canonicalize(f).unwrap();
        let parent = c.parent().unwrap();
        for a in [parent, parent.parent().unwrap_or(parent)] {
            if !includes.iter().any(|x| x == a) {
                includes.push(a.to_path_buf());
            }
        }
    }
    let (program, _) = resolve(&files, &includes);
    build_merged_model(&program, &includes).0
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

const SAMPLES: &[&str] = &[
    "samples/ec-site",
    "samples/personal-info",
    "samples/formal-verification/after_assert_unmet.rdra",
    "samples/formal-verification/check_met.rdra",
    "samples/formal-verification/check_unmet.rdra",
    "samples/formal-verification/cross_order_payment.rdra",
    "samples/formal-verification/cross_order_payment_split.rdra",
    "samples/formal-verification/int_stock.rdra",
    "samples/formal-verification/min_card_met.rdra",
    "samples/formal-verification/min_card_unmet.rdra",
    "samples/formal-verification/now_coupon.rdra",
    "samples/formal-verification/one_to_one_shared.rdra",
    "samples/formal-verification/order.rdra",
    "samples/formal-verification/quantifier_none.rdra",
    "samples/formal-verification/untriggered_sets.rdra",
    "samples/formal-verification-fail/order.rdra",
];

#[test]
fn cone_exploration_equals_the_projection_of_the_whole_component() {
    let caps = Caps::new(usize::MAX, 200_000);
    let mut checked = 0;
    let mut freed = 0;
    for sample in SAMPLES {
        let mut exact_here = 0;
        let model = load(sample);
        let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
        for comp in built.ts.components() {
            let sub = built.ts.restrict(&comp);
            let full = sub.clone().run(caps);
            assert!(full.complete, "{sample}: component does not finish");
            let whole = Exploration { runs: vec![full] };
            let full_views = whole.entity_views(&model, &[]);
            let full_diags: Vec<&StateDiag> = whole.diags().map(|d| &d.diag).collect();
            let full_run = &whole.runs[0];
            let cones = sub.explore_targets(caps);
            let cone_views = cones.entity_views(&sub, &model, &[]);
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
                exact_here += 1;
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
        assert!(exact_here > 0, "{sample}: no target checked exactly");
    }
    assert!(
        checked >= 25,
        "checked {checked} targets exactly, {freed} freed"
    );
    assert!(freed > 0, "no sample frees a slot");
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
transitions(Order.status, Close, open -> closed)
creates(AddLine, Line)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
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
    let report = crate::lifecycle::derive_states(&model, &Scope::All, Caps::DEFAULT_PATTERNS);
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
    for uc in ["RegisterCard", "SetPrimaryCard", "DeleteCard"] {
        assert!(via("CreditCard").contains(uc), "CreditCard via {uc}");
    }
    for uc in [
        "RegisterAddress",
        "UpdateAddress",
        "SetPrimaryAddress",
        "DeleteAddress",
    ] {
        assert!(via("PostalAddress").contains(uc), "PostalAddress via {uc}");
    }
}
