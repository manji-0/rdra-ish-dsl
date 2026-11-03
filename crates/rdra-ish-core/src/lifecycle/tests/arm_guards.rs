//! A guard per `transitions` arm (#62): `transitions(UC, E.col, a -> b).when(Cond)` takes the
//! arm only when the condition's `holds` body holds. The use case runs when some arm is
//! enabled; among the enabled arms the choice is free.

use super::*;
use crate::lifecycle::diag::Blocked;
use crate::lifecycle::GuardWhy;

/// A filing decision: filing needs the gate open, keeping it secret or dropping it does not.
const DOC: &str = r#"
actor A "A"
usecase Submit "Submit"
usecase Decide "Decide"
usecase Unlock "Unlock"
performs(A, Submit)
performs(A, Decide)
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
}
entity Gate "関門" { id: Int @pk  open: Bool @default(false) }
creates(Submit, Doc)
creates(Unlock, Gate)
condition Ready "関門が開いている"
holds(Ready, Gate.open == true)
transitions(Decide, Doc.status, searched -> file).when(Ready)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, searched -> drop)
"#;

const OPENS: &str = "sets(Unlock, Gate.open == true)";

fn arm_never_taken(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ArmNeverTaken {
                action, from, to, ..
            } => Some(format!("{action} {from} -> {to}")),
            _ => None,
        })
        .collect()
}

#[test]
fn a_guarded_arm_is_taken_once_its_guard_holds() {
    let d = derive_src(&format!("{DOC}\n{OPENS}\n"));
    assert_eq!(
        d.rows("Doc"),
        [
            "status=searched",
            "status=secret",
            "status=drop",
            "status=file"
        ]
    );
    assert!(arm_never_taken(&d).is_empty(), "{:?}", d.diags);
}

/// The guard never holds: the other arms still run, the guarded one never does, and a
/// warning names it (no arm drops without a word).
#[test]
fn a_guard_that_never_holds_drops_only_its_arm_and_says_so() {
    let d = derive_src(DOC);
    assert_eq!(
        d.rows("Doc"),
        ["status=searched", "status=secret", "status=drop"]
    );
    assert_eq!(arm_never_taken(&d), ["Decide searched -> file"]);
    let warning = d
        .diags
        .iter()
        .find(|x| matches!(x.diag, StateDiag::ArmNeverTaken { .. }))
        .unwrap();
    assert_eq!(warning.diag.severity(), crate::lifecycle::Severity::Warning);
    let text = warning.diag.to_string();
    assert!(text.contains("Ready"), "{text}");
}

/// The arm guard reads the pre-state: filing takes the gate as it was when the use case ran.
#[test]
fn a_rule_the_guard_protects_holds() {
    let src = r#"
usecase Submit "Submit"
usecase Acquire "Acquire"
usecase Decide "Decide"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret) @default(searched)
  acquired: Bool @default(false)
}
creates(Submit, Doc)
sets(Acquire, Doc.acquired == true)
condition Acquired "権利を取得している"
holds(Acquired, Doc.acquired == true)
transitions(Decide, Doc.status, searched -> file).when(Acquired)
transitions(Decide, Doc.status, searched -> secret)
forbidden(Doc, status == file, acquired == false)
"#;
    let d = derive_src(src);
    let violated = d
        .diags
        .iter()
        .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. }));
    assert!(!violated, "{:?}", d.diags);
    assert!(d.rows("Doc").iter().any(|r| r.contains("status=file")));

    // Without the guard the rule breaks: the guard is what keeps it.
    let unguarded = derive_src(&src.replace(".when(Acquired)", ""));
    assert!(unguarded
        .diags
        .iter()
        .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. })));
}

/// A single guarded arm is a guard on that arm only: unlike `applies_to`, the use case still
/// moves the column from another state.
#[test]
fn a_guard_on_one_arm_leaves_the_other_sources_free() {
    let src = r#"
usecase Submit "Submit"
usecase Decide "Decide"
usecase Unlock "Unlock"
usecase Skip "Skip"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, held, file, drop) @default(searched)
}
entity Gate "関門" { id: Int @pk  open: Bool @default(false) }
creates(Submit, Doc)
creates(Unlock, Gate)
transitions(Skip, Doc.status, searched -> held)
condition Ready "Ready"
holds(Ready, Gate.open == true)
transitions(Decide, Doc.status, searched -> file).when(Ready)
transitions(Decide, Doc.status, held -> drop)
"#;
    let d = derive_src(src);
    let rows = d.rows("Doc");
    assert!(rows.iter().any(|r| r == "status=drop"), "{rows:?}");
    assert!(!rows.iter().any(|r| r == "status=file"), "{rows:?}");

    // The same guard applied to the use case stops the arm from `held` too.
    let whole = derive_src(&src.replace(".when(Ready)", "").replace(
        "transitions(Decide, Doc.status, searched -> file)",
        "transitions(Decide, Doc.status, searched -> file)\napplies_to(Ready, Decide)",
    ));
    let rows = whole.rows("Doc");
    assert!(!rows.iter().any(|r| r == "status=drop"), "{rows:?}");
}

/// Every arm of the use case guarded and no guard ever holds: the use case never runs, and
/// the warning names the arm guard as what blocks it.
#[test]
fn a_use_case_whose_every_arm_is_blocked_never_runs() {
    let d = derive_src(&DOC.replace(
        "transitions(Decide, Doc.status, searched -> secret)\ntransitions(Decide, Doc.status, searched -> drop)\n",
        "",
    ));
    let why = d
        .diags
        .iter()
        .find_map(|x| match &x.diag {
            StateDiag::NeverRuns { action, why, .. } if action == "Decide" => Some(why.clone()),
            _ => None,
        })
        .expect("Decide never runs");
    let Blocked::Guards(guards) = why else {
        panic!("{why:?}")
    };
    assert!(
        guards
            .iter()
            .any(|(w, _)| matches!(w, GuardWhy::ArmCondition { .. })),
        "{guards:?}"
    );
}

/// Two arms from one state, each with its own guard: each is taken where its guard holds.
#[test]
fn each_arm_reads_its_own_guard() {
    let src = r#"
usecase Submit "Submit"
usecase Decide "Decide"
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, drop) @default(searched)
  ok: Bool
}
creates(Submit, Doc)
usecase Approve "Approve"
sets(Approve, Doc.ok == true)
condition Good "Good"
holds(Good, Doc.ok == true)
condition Bad "Bad"
holds(Bad, Doc.ok == false)
transitions(Decide, Doc.status, searched -> file).when(Good)
transitions(Decide, Doc.status, searched -> drop).when(Bad)
forbidden(Doc, status == file, ok == false)
"#;
    let d = derive_src(src);
    let rows = d.rows("Doc");
    assert!(rows.iter().any(|r| r.contains("status=file")), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("status=drop")), "{rows:?}");
    assert!(
        !d.diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. })),
        "{:?}",
        d.diags
    );
}

/// A cone a cap stopped rules no arm out: the untaken arm of a use case that ran is
/// undecided, with the cap, and not a warning.
#[test]
fn a_capped_cone_leaves_an_untaken_arm_undecided() {
    use crate::lifecycle::{Bounds, Subject, UndecidedWhy};
    let model = model_from(DOC);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let run = built
        .ts
        .explore_targets(&built.ts.targets(), Caps::new(Caps::DEFAULT_PATTERNS, 4));
    assert!(
        !run.verdicts()
            .iter()
            .any(|d| matches!(d.diag, StateDiag::ArmNeverTaken { .. })),
        "{:?}",
        run.verdicts()
    );
    let arm = run
        .undecided()
        .iter()
        .find(|u| matches!(u.subject, Subject::Arm { .. }))
        .unwrap_or_else(|| panic!("{:?}", run.undecided()));
    assert_eq!(arm.why, UndecidedWhy::StateCap(4));
    assert_eq!(
        arm.to_string(),
        "whether Decide can take Doc.status searched -> file is not decided: the Doc cone stopped at the state cap (4 states)"
    );
}

/// `--buc` that leaves out the use case enabling the guard rules the arm out no more.
#[test]
fn a_scope_that_drops_use_cases_leaves_an_untaken_arm_undecided() {
    use crate::lifecycle::{Bounds, Subject, UndecidedWhy};
    let built = TransitionSystem::build(&model_from(DOC), Bounds::STATES, &Scope::ALL);
    let mut report = built.ts.explore_targets(&built.ts.targets(), Caps::STATES);
    report.defer_progress();
    assert!(report.undecided().iter().any(|u| matches!(
        &u.subject,
        Subject::Arm { action, from, to, .. } if action == "Decide" && from == "searched" && to == "file"
    ) && u.why == UndecidedWhy::Scope));
}

/// An arm guard that reads a variation column nothing else reads still sees its value: the
/// guard names the column, so the system models it (before, the column was left out and the
/// guard never held).
#[test]
fn an_arm_guard_names_the_columns_it_reads() {
    let d = derive_src(
        r#"
variation Kind "Kind" {
  a "A"
  b "B"
}
usecase Submit "Submit"
usecase Decide "Decide"
entity Doc "Doc" {
  id: Int @pk
  status: Enum(open, done, drop) @default(open)
  kind: Kind
}
creates(Submit, Doc)
condition IsB "B"
holds(IsB, Doc.kind == b)
transitions(Decide, Doc.status, open -> done).when(IsB)
transitions(Decide, Doc.status, open -> drop)
"#,
    );
    let rows = d.rows("Doc");
    assert!(rows.iter().any(|r| r == "kind=b, status=done"), "{rows:?}");
    assert!(!rows.iter().any(|r| r == "kind=a, status=done"), "{rows:?}");
    assert!(arm_never_taken(&d).is_empty(), "{:?}", d.diags);
}
