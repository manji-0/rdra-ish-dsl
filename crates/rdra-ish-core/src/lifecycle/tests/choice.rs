//! Several `transitions` of one use case from one state on one column are a choice: one run of
//! the use case takes exactly one arm (#52).

use super::*;
use crate::lifecycle::{Change, ModelRef};

const DOC: &str = r#"
actor A "A"
usecase Submit "Submit"
usecase Decide "Decide"
performs(A, Submit)
performs(A, Decide)
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
}
creates(Submit, Doc)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, searched -> drop)
"#;

fn unreachable(d: &Derived, id: &str) -> Vec<String> {
    d.entity(id)
        .diagnostics
        .iter()
        .filter_map(|x| match x {
            StateDiag::UnreachableEnumVariant { column, variant } => {
                Some(format!("{column}={variant}"))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn arms_from_one_state_are_a_choice_and_each_is_reached() {
    let d = derive_src(DOC);
    assert_eq!(
        d.rows("Doc"),
        [
            "status=searched",
            "status=file",
            "status=secret",
            "status=drop"
        ]
    );
    assert!(unreachable(&d, "Doc").is_empty(), "{:?}", d.entity("Doc"));
}

#[test]
fn a_repeated_arm_is_one_arm() {
    let d = derive_src(&format!(
        "{DOC}\ntransitions(Decide, Doc.status, searched -> file)\n"
    ));
    assert_eq!(d.rows("Doc").len(), 4);
}

/// Each arm carries the rest of the use case: the `sets` on another column, the creation of
/// another entity, and the transition of another entity.
#[test]
fn every_arm_carries_the_other_effects_of_the_use_case() {
    let d = derive_src(&format!(
        r#"{DOC}
entity Log "記録" {{ id: Int @pk }}
entity Board "審議" {{ id: Int @pk  phase: Enum(open, closed) @default(open) }}
usecase Open "Open"
creates(Open, Board)
creates(Decide, Log)
updates(Decide, Board)
transitions(Decide, Board.phase, open -> closed)
"#
    ));
    assert_eq!(d.rows("Doc").len(), 4);
    assert!(unreachable(&d, "Doc").is_empty());
    assert_eq!(d.rows("Board"), ["phase=open", "phase=closed"]);
    let doc = d.entity("Doc");
    for p in doc.patterns.iter().skip(1) {
        let via: Vec<&str> = p.provenance.via.iter().map(|(_, uc)| uc.as_str()).collect();
        assert!(via.contains(&"Decide"), "{via:?}");
    }
}

/// Two choice columns in one use case choose independently: every pair of arms is a run.
#[test]
fn two_choice_columns_take_every_pair_of_arms() {
    let d = derive_src(
        r#"
usecase Submit "Submit"
usecase Decide "Decide"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, drop) @default(searched)
  route: Enum(none, fast, slow) @default(none)
}
creates(Submit, Doc)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
transitions(Decide, Doc.route, none -> fast)
transitions(Decide, Doc.route, none -> slow)
"#,
    );
    let rows = d.rows("Doc");
    for want in [
        "route=none, status=searched",
        "route=fast, status=file",
        "route=fast, status=drop",
        "route=slow, status=file",
        "route=slow, status=drop",
    ] {
        assert!(rows.iter().any(|r| r == want), "{want} missing in {rows:?}");
    }
    assert_eq!(rows.len(), 5, "{rows:?}");
}

/// `after(UC).assert` must hold after every arm: an assert two arms meet and the third does
/// not is violated, with a trace through the third.
#[test]
fn an_after_assert_must_hold_after_every_arm() {
    let d = derive_src(&format!(
        "{DOC}\nafter(Decide).assert(Doc.status != drop)\n"
    ));
    let violated: Vec<&StateDiag> = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .filter(|x| matches!(x, StateDiag::AfterAssertViolated { .. }))
        .collect();
    assert_eq!(violated.len(), 1, "{:?}", d.diags);
    let trace = violated[0].trace().unwrap();
    let last = trace.steps.last().unwrap();
    assert_eq!(last.action, "Decide");
    assert!(
        last.changes
            .iter()
            .any(|c| matches!(c, Change::Set { to, .. } if to == "drop")),
        "{last:?}"
    );
}

/// A condition applied to the use case guards every arm alike.
#[test]
fn a_condition_guards_every_arm() {
    let d = derive_src(&format!(
        r#"{DOC}
entity Gate "関門" {{ id: Int @pk  open: Bool @default(false) }}
usecase Unlock "Unlock"
creates(Unlock, Gate)
sets(Unlock, Gate, open == true)
condition Ready "Ready"
holds(Ready, Gate.open == true)
applies_to(Ready, Decide)
"#
    ));
    assert_eq!(d.rows("Doc").len(), 4);
    assert!(unreachable(&d, "Doc").is_empty());

    let blocked = derive_src(&format!(
        r#"{DOC}
entity Gate "関門" {{ id: Int @pk  open: Bool @default(false) }}
usecase Unlock "Unlock"
creates(Unlock, Gate)
condition Ready "Ready"
holds(Ready, Gate.open == true)
applies_to(Ready, Decide)
"#
    ));
    assert_eq!(blocked.rows("Doc"), ["status=searched"]);
    let exits = blocked
        .diags
        .iter()
        .find_map(|x| match &x.diag {
            StateDiag::Deadlock { exits, .. } => Some(exits.len()),
            _ => None,
        })
        .expect("Doc is stuck at searched");
    assert_eq!(exits, 3, "the deadlock names every arm");
}

/// A trace names the arm the step took, not the first arm from the same state.
#[test]
fn a_trace_step_cites_the_arm_it_took() {
    let d = derive_src(&format!("{DOC}\nforbidden(Doc, status == drop)\n"));
    let trace = d
        .diags
        .iter()
        .find_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("drop is reachable and forbidden");
    let cause = trace
        .steps
        .last()
        .unwrap()
        .changes
        .iter()
        .find_map(|c| match c {
            Change::Set { to, cause, .. } if to == "drop" => cause.clone(),
            _ => None,
        })
        .expect("the step sets drop");
    let model = model_from(DOC);
    let ModelRef::Transition(i) = cause.fact else {
        panic!("{cause:?}")
    };
    assert_eq!(model.state_transitions[i].to, "drop");
}
