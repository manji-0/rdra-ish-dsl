//! Reachability witnesses: each state of a state model is witnessed by a path, or is a
//! `NoWitness` error, or a capped or scoped run leaves it undecided.

use std::collections::BTreeMap;

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{
    Change, Severity, Subject, Target, Trace, Undecided, UndecidedWhy, Witness,
};

const UNWITNESSED: &str = "samples/formal-verification/unwitnessed_state.rdra";

/// What a trace leaves each instance holding: its start, then every step's changes.
pub(super) fn final_values(trace: &Trace) -> BTreeMap<(String, u8), BTreeMap<String, String>> {
    let mut held: BTreeMap<(String, u8), BTreeMap<String, String>> = BTreeMap::new();
    let changes = trace
        .start
        .iter()
        .chain(trace.steps.iter().flat_map(|s| &s.changes));
    for change in changes {
        match change {
            Change::Create { at, values } => {
                held.insert(
                    (at.entity.clone(), at.slot),
                    values.iter().cloned().collect(),
                );
            }
            Change::Delete { at } => {
                held.remove(&(at.entity.clone(), at.slot));
            }
            Change::Set { at, field, to, .. } => {
                held.entry((at.entity.clone(), at.slot))
                    .or_default()
                    .insert(field.clone(), to.clone());
            }
            Change::Tick { .. } => {}
        }
    }
    held
}

/// Each pattern's witness is a path whose last state has an instance holding the pattern.
#[test]
fn every_witness_ends_in_its_pattern() {
    let mut witnessed = 0;
    for sample in SAMPLES {
        let model = load(sample);
        let report = derive_states_all(&model);
        for view in report.iter().filter(|v| !v.axes.is_empty()) {
            for p in &view.patterns {
                let trace = p.witness.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{sample} {}: {:?} has no witness",
                        view.entity_id, p.pattern
                    )
                });
                let want: BTreeMap<String, String> = p
                    .pattern
                    .values
                    .iter()
                    .map(|(c, v)| {
                        let field = c.strip_prefix("__cmp:").unwrap_or(c).to_string();
                        (field, v.display_with_type(None))
                    })
                    .collect();
                let held = final_values(trace);
                assert!(
                    held.iter().any(|((e, _), values)| *e == view.entity_id
                        && want.iter().all(|(f, v)| values.get(f) == Some(v))),
                    "{sample} {}: the witness of {want:?} ends in {held:?}",
                    view.entity_id
                );
                witnessed += 1;
            }
        }
    }
    assert!(witnessed > 50, "only {witnessed} witnessed patterns");
}

fn derive_states_all(model: &SemanticModel) -> Vec<EntityStateResult> {
    super::super::derive_states(model, &Scope::ALL, Caps::DEFAULT_PATTERNS).views
}

/// At both bounds, a witnessed state's first state holds it in a present slot, and no state
/// of a complete graph is missing on any sample but the one built to miss one.
#[test]
fn a_witness_state_holds_its_value() {
    for sample in SAMPLES {
        let model = load(sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
            let ts = &built.ts;
            let entities: Vec<Target> = ts
                .targets()
                .into_iter()
                .filter(|t| matches!(t, Target::Entity(_)))
                .collect();
            let run = ts.explore_targets(&entities, Caps::new(usize::MAX, 200_000));
            for &(target, at) in &run.targets {
                let Target::Entity(e) = target else {
                    unreachable!()
                };
                let cone = &run.cones[at].run;
                let local = cone.system.local_entity(ts.entity(e).key);
                let g = cone.graph();
                for w in cone.system.state_model_findings(local, &g).witnesses {
                    match w.witness {
                        Witness::At(i) => {
                            let s = &g.states[i as usize];
                            assert!(
                                (0..bounds.slots()).any(|slot| s.is_present(
                                    &cone.system,
                                    local,
                                    slot
                                ) && s.get(
                                    &cone.system,
                                    w.var,
                                    slot
                                ) == w.value),
                                "{sample}: state {i} does not hold its witnessed value"
                            );
                        }
                        Witness::Missing(d) => {
                            assert!(*sample == UNWITNESSED, "{sample} at {bounds:?}: {d}")
                        }
                        Witness::NotDecided => {
                            assert!(!g.complete, "{sample}: a complete graph decides")
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_state_no_use_case_reaches_has_no_witness() {
    let d = derive(&sample_model(UNWITNESSED));
    let missing: Vec<&StateDiag> = d
        .entity("Order")
        .diagnostics
        .iter()
        .filter(|x| matches!(x, StateDiag::NoWitness { .. }))
        .collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(
        missing[0].name().as_deref(),
        Some("Witness_Order_status_refunded")
    );
    assert_eq!(missing[0].severity(), Severity::Error);
    let StateDiag::NoWitness { into, .. } = missing[0] else {
        unreachable!()
    };
    assert_eq!(into.len(), 1, "{into:?}");
    assert_eq!(
        (into[0].action.as_str(), into[0].from.as_deref()),
        ("Refund", Some("shipped"))
    );
    assert!(
        d.rows("Order").iter().any(|r| r.contains("status=shipped")),
        "shipped is still witnessed"
    );
    assert!(
        !d.has("Order", |x| matches!(
            x,
            StateDiag::UnreachableEnumVariant { .. }
        )),
        "a state of a state model does not also warn"
    );
}

#[test]
fn a_declared_value_outside_the_state_model_still_warns() {
    let d = derive(&sample_model(
        "samples/formal-verification/untriggered_sets.rdra",
    ));
    assert!(d.has("Order", |x| matches!(
        x,
        StateDiag::UnreachableEnumVariant { variant, .. } if variant == "draft"
    )));
    assert!(!d.has("Order", |x| matches!(x, StateDiag::NoWitness { .. })));
}

/// What a run left undecided, by subject: states of state models and deadlocks by value,
/// actions by name.
struct BySubject<'a> {
    states: Vec<(&'a str, UndecidedWhy)>,
    actions: Vec<&'a str>,
    deadlocks: Vec<(&'a str, UndecidedWhy)>,
}

fn undecided_by_subject(undecided: &[Undecided]) -> BySubject<'_> {
    let mut out = BySubject {
        states: Vec::new(),
        actions: Vec::new(),
        deadlocks: Vec::new(),
    };
    for u in undecided {
        match &u.subject {
            Subject::State { value, .. } => out.states.push((value.as_str(), u.why)),
            Subject::Runs { action } => out.actions.push(action.as_str()),
            Subject::Deadlock { value, .. } => out.deadlocks.push((value.as_str(), u.why)),
        }
    }
    out
}

/// A cone stopped by its state cap rules nothing out: its unwitnessed states, its actions
/// with no edge, and every state with a way out are undecided, with the cap, and none is an
/// error or a warning.
#[test]
fn a_capped_cone_leaves_its_states_and_actions_undecided() {
    let model = sample_model(UNWITNESSED);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let run = built
        .ts
        .explore_targets(&built.ts.targets(), Caps::new(Caps::DEFAULT_PATTERNS, 2));
    assert!(
        !run.verdicts().iter().any(|d| matches!(
            d.diag,
            StateDiag::NoWitness { .. } | StateDiag::NeverRuns { .. } | StateDiag::Deadlock { .. }
        )),
        "{:?}",
        run.verdicts()
    );
    let BySubject {
        states,
        actions,
        deadlocks,
    } = undecided_by_subject(run.undecided());
    assert_eq!(
        states,
        [
            ("paid", UndecidedWhy::StateCap(2)),
            ("shipped", UndecidedWhy::StateCap(2)),
            ("refunded", UndecidedWhy::StateCap(2))
        ]
    );
    assert!(actions.contains(&"Refund"), "{actions:?}");
    assert!(!actions.contains(&"Place"), "Place fired before the cap");
    assert_eq!(
        deadlocks,
        [
            ("pending", UndecidedWhy::StateCap(2)),
            ("paid", UndecidedWhy::StateCap(2)),
            ("shipped", UndecidedWhy::StateCap(2))
        ]
    );
    let shipped = run
        .undecided()
        .iter()
        .find(|u| matches!(&u.subject, Subject::Deadlock { value, .. } if value == "shipped"))
        .expect("shipped is undecided");
    assert_eq!(
        shipped.to_string(),
        "whether some Order can get stuck at status = shipped is not decided: the Order cone stopped at the state cap (2 states)"
    );
    assert_eq!(
        run.undecided()[2].to_string(),
        "state Order.status = refunded is not decided: the Order cone stopped at the state cap (2 states)"
    );
    let refund = run
        .undecided()
        .iter()
        .find(|u| {
            u.subject
                == Subject::Runs {
                    action: "Refund".to_string(),
                }
        })
        .expect("Refund is undecided");
    assert_eq!(
        refund.to_string(),
        "whether Refund can run is not decided: the Order cone stopped at the state cap (2 states)"
    );
}

/// `--buc` drops use cases, so a run under it rules neither a state, an action, nor a
/// deadlock out.
#[test]
fn a_buc_scope_leaves_unwitnessed_states_and_dead_actions_undecided() {
    let model = sample_model(UNWITNESSED);
    let report = super::super::derive_states(
        &model,
        &Scope::from_bucs(&["BucOrderRefund".to_string()]),
        Caps::DEFAULT_PATTERNS,
    );
    assert!(!report.has_errors(), "{:?}", report.diags);
    assert!(
        !report.diags.iter().any(|d| matches!(
            d.diag,
            StateDiag::NeverRuns { .. } | StateDiag::Deadlock { .. }
        )),
        "{:?}",
        report.diags
    );
    let BySubject {
        states,
        actions,
        deadlocks,
    } = undecided_by_subject(&report.undecided);
    assert_eq!(states, [("refunded", UndecidedWhy::Scope)]);
    assert_eq!(actions, ["Refund"]);
    assert_eq!(deadlocks, [("shipped", UndecidedWhy::Scope)]);
    assert!(report
        .undecided
        .iter()
        .all(|u| u.why == UndecidedWhy::Scope));
}
