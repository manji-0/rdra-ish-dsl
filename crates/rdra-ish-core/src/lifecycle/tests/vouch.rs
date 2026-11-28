//! The certificates of a counterexample found on a cone (`lifecycle/vouch.rs`, claims A2 and
//! A16): each catches its planted fault, and none fires on the samples, the domain samples
//! and the engine corpus.

use super::reduce::{walked_models, GATE};
use super::*;
use crate::lifecycle::explore::symmetry::witnesses::dropping;
use crate::lifecycle::{LassoFault, Recheck, ReplayError, Target, Trace};
use crate::model::SemanticModel;

/// A handler `Finalize` whose event `Review` raises: the rule's own cone leaves the mark out,
/// so its counterexample runs `Finalize` without the event.
const DOC: &str = r#"
entity Doc "文書" { id: Int @pk  status: Enum(draft, final) @default(draft)  seen: Bool @default(false) }
usecase Write "作成"
usecase Review "確認"
usecase Finalize "確定"
event EvReviewed "確認済"
creates(Write, Doc)
updates(Review, Doc)
sets(Review, Doc.seen == true)
raises(Review, EvReviewed)
triggers(EvReviewed, Finalize)
transitions(Finalize, Doc.status, draft -> final)
forbidden(Doc, status == final)
"#;

fn doc() -> TransitionSystem {
    TransitionSystem::build(&model_from(DOC), Bounds::STATES, &Scope::ALL).ts
}

fn caps() -> Caps {
    Caps::new(usize::MAX, 100_000)
}

/// The rule's counterexample as the recheck decides it: `Write`, `Review`, `Finalize`.
fn real_trace(ts: &TransitionSystem) -> Trace {
    let Recheck::Violated(d) = ts.recheck(Target::Invariant(0), false, caps()) else {
        panic!("Review raises the event, so Finalize runs");
    };
    d.diag.trace().expect("a trace").clone()
}

fn steps(t: &Trace) -> Vec<&str> {
    t.steps.iter().map(|s| s.action.as_str()).collect()
}

/// The free triggers are checked against the steps' guards. On the rule's own cone, which
/// leaves the mark out, `Finalize` runs free: the list and the check agree. A planted list
/// that misses it (`free_triggers`) is caught, and the check adds what it misses, so the
/// trace stays a candidate and the recheck still decides the rule. A step of an action the
/// system lacks is caught too.
#[test]
fn a_planted_free_trigger_list_is_caught_and_the_trace_stays_a_candidate() {
    let ts = doc();
    let rule = Target::Invariant(0);
    let own = ts.target_cone(rule);
    assert!(ts.cone_leaves_marks(&own));
    let trace = real_trace(&ts);
    assert_eq!(steps(&trace), ["Write", "Review", "Finalize"]);
    let free = vec![("Finalize".to_string(), "EvReviewed".to_string())];

    let mut t = trace.clone();
    ts.annotate(&own, &mut t);
    assert_eq!(t.free_triggers, free);
    assert_eq!(ts.free_trigger_faults(&own, &mut t), None);

    let mut planted = trace.clone();
    dropping("free_triggers", || ts.annotate(&own, &mut planted));
    assert_eq!(
        planted.free_triggers, free,
        "the check adds what the list missed"
    );
    assert!(!planted.vouched(false));
    let mut bare = trace.clone();
    bare.free_triggers.clear();
    let fault = ts.free_trigger_faults(&own, &mut bare).expect("caught");
    assert!(fault.contains("EvReviewed"), "{fault}");

    let rechecked = dropping("free_triggers", || ts.recheck(rule, false, caps()));
    let Recheck::Violated(d) = rechecked else {
        panic!("the recheck still decides the rule: {rechecked:?}");
    };
    assert_eq!(
        steps(d.diag.trace().unwrap()),
        ["Write", "Review", "Finalize"]
    );

    let mut unknown = trace.clone();
    unknown.steps[0].action = "Nowhere".to_string();
    assert!(ts.free_trigger_faults(&own, &mut unknown).is_some());
    assert!(!unknown.vouched(false));
}

/// The exact cone reads every mark and counts every slot in every build. A planted one that
/// leaves the mark out (`exact_target_cone`) is caught, and the whole system stands for it.
#[test]
fn a_planted_exact_cone_that_leaves_a_mark_out_falls_back_to_the_whole_system() {
    let ts = doc();
    let rule = Target::Invariant(0);
    for slots in [false, true] {
        let exact = ts.exact_target_cone(rule, slots);
        assert!(!ts.cone_leaves_marks(&exact));
        let planted = dropping("exact_target_cone", || ts.exact_target_cone(rule, slots));
        assert_eq!(planted.vars.len(), ts.var_ids().count(), "the whole system");
        assert_eq!(planted.keep.len(), ts.action_ids().count());
        assert!(!ts.cone_leaves_marks(&planted) && !ts.cone_frees_slots(&planted));
    }
}

/// The exact cone's counterexample must be one it vouches for, in every build. A planted
/// exact cone that leaves the mark out, reached at once and unchecked
/// (`exact_target_cone_unchecked`), finds `Write`, `Finalize`: a tool error for the rule,
/// never a counterexample.
#[test]
fn an_exact_cone_that_does_not_vouch_for_its_counterexample_is_a_tool_error() {
    let ts = doc();
    let rule = Target::Invariant(0);
    let (verdict, cone) = dropping("exact_target_cone_unchecked", || {
        ts.recheck_on(rule, false, caps())
    });
    assert!(ts.cone_leaves_marks(&cone));
    let Recheck::Fault(d) = verdict else {
        panic!("a tool error: {verdict:?}");
    };
    let StateDiag::TraceFault {
        fault: LassoFault::Unvouched(why),
        ..
    } = &d.diag
    else {
        panic!("{:?}", d.diag);
    };
    assert!(why.contains("EvReviewed"), "{why}");
    assert!(d.diag.to_string().starts_with("tool error:"), "{}", d.diag);
}

/// A lifted start is an initial state of the unreduced cone, in every build. On the gate's
/// reduced cone the violation lifts; a planted start that is not (`lifted_start`) is
/// `NotInitial`, and the violation is a `TraceFault`.
#[test]
fn a_planted_lifted_start_that_is_no_initial_state_is_a_replay_error() {
    let ts = TransitionSystem::build(&model_from(GATE), Bounds::STATES, &Scope::ALL).ts;
    let rule = Target::Invariant(0);
    let caps = Caps::for_instances(1);
    let run = ts.explore_targets_reduced(&[rule], caps);
    let cone = run.cone_of(rule);
    assert!(
        cone.run.system.reduction().is_some(),
        "the cone reduces the gate"
    );
    let at = (0..cone.run.states.len() as u32).last().unwrap();
    assert!(cone.run.trace_to(at).is_ok());
    let planted = dropping("lifted_start", || cone.run.trace_to(at));
    assert_eq!(planted.err(), Some(ReplayError::NotInitial));
}

/// The test of `is_unreduced_initial` accepts exactly the initial states `initial_states`
/// enumerates on a system without reduction: each passes, and each with one slot's existence
/// flipped fails.
fn initial_states_agree(ts: &TransitionSystem) -> usize {
    let mut n = 0;
    for s in ts.initial_states() {
        assert!(ts.is_unreduced_initial(&s));
        for e in &ts.entities {
            let mut flipped = s.clone();
            let v = match s.get(ts, e.exists, 0) {
                crate::lifecycle::Value::PRESENT => crate::lifecycle::Value::ABSENT,
                _ => crate::lifecycle::Value::PRESENT,
            };
            flipped.set(ts, e.exists, 0, v);
            assert!(!ts.is_unreduced_initial(&flipped));
        }
        n += 1;
    }
    n
}

/// What `walk` saw: initial states, exact cones, counterexamples (those with a free trigger,
/// those rechecked), and lifted paths.
#[derive(Debug, Default)]
struct Walked {
    initial: usize,
    exact: usize,
    compared: usize,
    free: usize,
    rechecked: usize,
    lifted: usize,
}

/// Walks `models` at `Bounds::STATES`: the initial-state test agrees with the enumeration;
/// each check's exact cone, with and without slots, passes its check; and on each check's
/// cone, reduced as `check` reduces it, each counterexample's free triggers pass theirs, each
/// one the cone cannot vouch for is decided again with no fault, and the first states of a
/// reduced cone's run lift from an initial state of the unreduced cone. A fault panics in this
/// debug build.
fn walk<'a>(models: impl Iterator<Item = &'a (String, SemanticModel)>) -> Walked {
    let caps = Caps::new(usize::MAX, 20_000);
    let mut w = Walked::default();
    for (name, model) in models {
        let ts = TransitionSystem::build(model, Bounds::STATES, &Scope::ALL).ts;
        w.initial += initial_states_agree(&ts);
        let checks: Vec<Target> = (ts.targets().into_iter())
            .filter(|t| matches!(t, Target::Invariant(_) | Target::AfterAssert(_)))
            .collect();
        for &t in &checks {
            for slots in [false, true] {
                let cone = ts.exact_target_cone(t, slots);
                assert!(!ts.cone_leaves_marks(&cone), "{name}: {t:?}");
                assert!(!slots || !ts.cone_frees_slots(&cone), "{name}: {t:?}");
                w.exact += 1;
            }
        }
        if checks.is_empty() {
            continue;
        }
        let run = ts.explore_targets_reduced(&checks, caps);
        for c in &run.cones {
            for d in &c.run.diags {
                let Some(trace) = d.diag.trace() else {
                    continue;
                };
                let mut t = trace.clone();
                ts.annotate(&c.cone, &mut t);
                assert_eq!(ts.free_trigger_faults(&c.cone, &mut t), None, "{name}");
                w.compared += 1;
                if t.free_triggers.is_empty() {
                    continue;
                }
                w.free += 1;
                let target = ts.violated_check(&d.diag).expect("a check's violation");
                let r = ts.recheck(target, false, caps);
                assert!(!matches!(r, Recheck::Fault(_)), "{name}: {r:?}");
                w.rechecked += 1;
            }
            if c.run.system.reduction().is_some() {
                for at in 0..c.run.states.len().min(4) as u32 {
                    assert!(c.run.trace_to(at).is_ok(), "{name}: state {at} lifts");
                    w.lifted += 1;
                }
            }
        }
    }
    eprintln!("{w:?}");
    w
}

/// No certificate fires on the samples and the domain samples (`walk`). The counts show each
/// check ran on something; no sample has a check whose cone counterexample runs a use case
/// without its event.
#[test]
fn no_counterexample_certificate_fires_on_the_samples() {
    let models = walked_models();
    let w = walk(models.iter().filter(|(n, _)| n.starts_with("samples/")));
    assert!(
        w.initial > 0 && w.exact > 0 && w.compared > 0 && w.lifted > 0,
        "{w:?}"
    );
}

/// No certificate fires on the engine corpus cases that build a model, and `DOC` (`walk`). No
/// corpus case but `DOC` has a check whose cone counterexample runs a use case without its
/// event.
#[test]
fn no_counterexample_certificate_fires_on_the_corpus() {
    let mut models: Vec<(String, SemanticModel)> = (walked_models().into_iter())
        .filter(|(n, _)| !n.starts_with("samples/"))
        .collect();
    models.push(("DOC".to_string(), model_from(DOC)));
    let w = walk(models.iter());
    assert!(
        w.initial > 0 && w.exact > 0 && w.compared > 0 && w.free > 0 && w.rechecked > 0,
        "{w:?}"
    );
}

/// Outside a planted test, a debug build stops at a fault of the free triggers with `shadow:`.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "shadow: annotate:")]
fn a_free_trigger_fault_panics_in_a_debug_build() {
    let ts = doc();
    let mut t = real_trace(&ts);
    ts.certify_free_triggers(&ts.target_cone(Target::Invariant(0)), &mut t);
}

/// Outside a planted test, a debug build stops at an exact cone that leaves a mark out.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "shadow: exact_target_cone:")]
fn an_exact_cone_fault_panics_in_a_debug_build() {
    let ts = doc();
    let rule = Target::Invariant(0);
    ts.certified_exact(rule, false, ts.target_cone(rule));
}

/// Outside a planted test, a debug build stops at an exact cone's unvouched counterexample.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "shadow: recheck_on:")]
fn an_unvouched_exact_counterexample_panics_in_a_debug_build() {
    let ts = doc();
    let Recheck::Violated(mut d) = ts.recheck(Target::Invariant(0), false, caps()) else {
        panic!("a violation");
    };
    let t = d.diag.trace_mut().unwrap();
    t.free_triggers = vec![("Finalize".to_string(), "EvReviewed".to_string())];
    ts.unvouched_fault(Target::Invariant(0), &d, false);
}

/// Outside a planted test, a debug build stops at a lifted start that is no initial state.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "shadow: lift:")]
fn a_lifted_start_fault_panics_in_a_debug_build() {
    let ts = doc();
    let mut s = ts.initial_states().remove(0);
    let e = &ts.entities[0];
    s.set(&ts, e.exists, 0, crate::lifecycle::Value::PRESENT);
    let _ = crate::lifecycle::vouch::lifted_start(&ts, &mut s);
}
