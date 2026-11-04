//! A guard per set value (#98): `sets(UC, E.col == v).when(Cond)` leaves `v` only while the
//! condition's `holds` body holds, read as a `chooses(..).when(Cond)` value guard reads it, so a
//! deadline can follow a reason, and a timer's use case that compares the deadline with `now`
//! ends the period at the reason's mark.

use super::*;
use crate::diagnostics::RdraError;
use crate::lifecycle::Severity;

/// An extension is created with a reason; the reason sets its deadline on the clock. The timer's
/// use case ends an extended claim's period once the clock reaches the deadline.
const EXTENSION: &str = r#"
variation Reason "延長事由" {
  disaster "被災地の調査"
  expert "鑑定等の照会"
}
entity Claim "請求" {
  id: Int @pk
  period: Enum(running, extended, overdue) @default(running)
}
entity Extension "延長" {
  id: Int @pk
  reason: Reason
  due_on: Date
}
relate(Extension, Claim, 1:1)
usecase File "請求"
usecase Extend "延長"
usecase MarkOverdue "期限の経過"
creates(File, Claim)
creates(Extend, Extension)
updates(Extend, Claim)
transitions(Extend, Claim.period, running -> extended)
condition Disaster "被災地の調査で延ばす"
holds(Disaster, Extension.reason == disaster)
condition Expert "鑑定等の照会で延ばす"
holds(Expert, Extension.reason == expert)
timer Deadline "支払期限の到来"
triggers(Deadline, MarkOverdue)
updates(MarkOverdue, Claim)
condition Passed "延ばした期限が来た"
holds(Passed, Extension.due_on <= now)
transitions(MarkOverdue, Claim.period, extended -> overdue).when(Passed)
invariant(Extension).when(reason == expert).then(due_on == 3)
forbidden(Claim, Extension, Claim.period == overdue, Extension.due_on > now).along(Extension, Claim)
"#;

const BY_REASON: &str = "sets(Extend, Extension.due_on == 2).when(Disaster)\nsets(Extend, Extension.due_on == 3).when(Expert)";

fn violated(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn model_errors(src: &str) -> Vec<RdraError> {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (_, diags) = build_model(&ast);
    diags
        .into_iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error)
        .collect()
}

/// Each reason leaves its own deadline: the created row's reason is read after the step, as a
/// condition applied to a creating use case reads it.
#[test]
fn a_guarded_value_follows_the_created_row() {
    let d = derive_src(&format!("{EXTENSION}\n{BY_REASON}\n"));
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    let rows = d.rows("Extension");
    assert_eq!(
        rows,
        ["due_on=2, reason=disaster", "due_on=3, reason=expert"],
        "{rows:?}"
    );
}

/// The rule reads the guard: give the expert inquiry another deadline and the invariant fails.
#[test]
fn a_wrong_deadline_for_the_reason_is_a_counterexample() {
    let d = derive_src(&format!(
        "{EXTENSION}\nsets(Extend, Extension.due_on == 2).when(Disaster)\nsets(Extend, Extension.due_on == 4).when(Expert)\n"
    ));
    assert_eq!(violated(&d), ["Inv_Extension_0"], "{:?}", d.diags);
}

/// The timer's use case waits for the deadline of the row it ends; without the arm guard it ends
/// the period before the clock reaches it.
#[test]
fn the_timer_ends_the_period_only_once_the_clock_reaches_the_deadline() {
    let d = derive_src(&format!("{EXTENSION}\n{BY_REASON}\n"));
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    let unguarded = EXTENSION.replace("extended -> overdue).when(Passed)", "extended -> overdue)");
    let d = derive_src(&format!("{unguarded}\n{BY_REASON}\n"));
    assert_eq!(violated(&d), ["CrossForbidden_Claim_0"], "{:?}", d.diags);
}

/// No line enabled, no run: a reason no line names leaves no deadline, so the use case does not
/// create an extension with it.
#[test]
fn a_reason_no_line_names_is_never_extended() {
    let d = derive_src(&format!(
        "{EXTENSION}\nsets(Extend, Extension.due_on == 3).when(Expert)\n"
    ));
    let rows = d.rows("Extension");
    assert_eq!(rows, ["due_on=3, reason=expert"], "{rows:?}");
}

/// An updated row is read on the state the use case starts from: the reason recorded earlier
/// picks the deadline.
#[test]
fn a_guard_on_an_updated_row_reads_the_state_before_the_step() {
    let src = r#"
variation Reason "延長事由" {
  disaster "被災地の調査"
  expert "鑑定等の照会"
}
entity Claim "請求" {
  id: Int @pk
  status: Enum(open, extended) @default(open)
  reason: Reason
  due_on: Date @null
}
usecase File "請求"
usecase Extend "延長"
creates(File, Claim)
transitions(Extend, Claim.status, open -> extended)
condition Expert "鑑定等の照会"
holds(Expert, Claim.reason == expert)
condition Disaster "被災地の調査"
holds(Disaster, Claim.reason == disaster)
sets(Extend, Claim.due_on == 3).when(Expert)
sets(Extend, Claim.due_on == 2).when(Disaster)
invariant(Claim).when(status == extended, reason == expert).then(due_on == 3)
"#;
    let d = derive_src(src);
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    let rows = d.rows("Claim");
    assert!(
        rows.iter().any(|r| r.contains("status=extended")
            && r.contains("reason=disaster")
            && r.contains("due_on=2")),
        "{rows:?}"
    );
}

/// A guarded line beside an unguarded `sets` of the same column is two writes of one column.
#[test]
fn an_unguarded_set_beside_a_guarded_one_conflicts() {
    let d = derive_src(&format!(
        "{EXTENSION}\n{BY_REASON}\nsets(Extend, Extension.due_on == 1)\n"
    ));
    assert!(
        d.diags.iter().any(|x| matches!(
            &x.diag,
            StateDiag::ConflictingEffects { usecase, .. } if usecase == "Extend"
        )),
        "{:?}",
        d.diags
    );
}

/// A guard that never holds where the use case runs drops its value, and the warning names it.
#[test]
fn a_value_whose_guard_never_holds_says_so() {
    let d = derive_src(&format!(
        "{EXTENSION}\n{BY_REASON}\ncondition Never \"起きない\"\nholds(Never, Extension.reason == expert, Extension.due_on == 0)\nsets(Extend, Extension.due_on == 5).when(Never)\n"
    ));
    let warning = d
        .diags
        .iter()
        .find(|x| matches!(x.diag, StateDiag::ValueNeverChosen { .. }))
        .unwrap_or_else(|| panic!("{:?}", d.diags));
    assert_eq!(warning.diag.severity(), Severity::Warning);
    assert_eq!(
        warning.diag.to_string(),
        "Extend never chooses Extension.due_on = 5: wherever it could, its guard Never does not hold"
    );
}

/// `.when` names one condition with a body, on a literal value; a copy takes `applies_to`.
#[test]
fn a_guard_needs_a_condition_with_a_body_on_a_literal() {
    let errors = model_errors(&format!(
        "{EXTENSION}\ncondition Bare \"本体なし\"\nsets(Extend, Extension.due_on == 2).when(Bare)\n"
    ));
    assert!(
        errors.iter().any(|e| matches!(e,
            RdraError::WhenConditionWithoutBody { pred, condition, .. } if pred == "sets" && condition == "Bare")),
        "{errors:?}"
    );
    let errors = model_errors(&format!(
        "{EXTENSION}\nsets(Extend, Extension.due_on == 2).when(Extend)\n"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, RdraError::WhenRequiresCondition { pred } if pred == "sets")),
        "{errors:?}"
    );
    let errors = model_errors(&format!(
        "{EXTENSION}\nentity Rule \"規則\" {{\n  id: Int @pk\n  due_on: Date\n}}\nrelate(Extension, Rule, N:1)\nsets(Extend, Extension.due_on == Rule.due_on).when(Expert)\n"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, RdraError::SetsWhenOnCopy { .. })),
        "{errors:?}"
    );
    assert_eq!(
        errors
            .iter()
            .find(|e| matches!(e, RdraError::SetsWhenOnCopy { .. }))
            .unwrap()
            .to_string(),
        "`sets(Extend, Extension.due_on == Rule.due_on).when(...)` guards a copy\n  hint: `.when` guards a literal value, `sets(Extend, Extension.due_on == value).when(Cond)`; to guard the copy write `applies_to(Cond, Extend)`"
    );
}

/// A copy of the column beside a guarded line is a second write of it.
#[test]
fn a_copy_beside_a_guarded_set_conflicts() {
    let src = EXTENSION.replace(
        "period: Enum(running, extended, overdue) @default(running)",
        "period: Enum(running, extended, overdue) @default(running)\n  agreed_on: Date",
    );
    let d = derive_src(&format!(
        "{src}\n{BY_REASON}\nsets(Extend, Extension.due_on == Claim.agreed_on)\ninvariant(Claim).when(agreed_on == 1).then(period == running)\n"
    ));
    assert!(
        d.diags.iter().any(|x| matches!(
            &x.diag,
            StateDiag::ConflictingEffects { usecase, .. } if usecase == "Extend"
        )),
        "{:?}",
        d.diags
    );
}

/// `present` on a nullable column is guarded as a literal is, and a reason with no line enabled
/// does not run the use case.
#[test]
fn a_guarded_present_is_left_only_where_its_guard_holds() {
    let src = EXTENSION.replace(
        "period: Enum(running, extended, overdue) @default(running)",
        "period: Enum(running, extended, overdue) @default(running)\n  reminder: Int @null",
    );
    let d = derive_src(&format!(
        "{src}\n{BY_REASON}\nsets(Extend, Claim.reminder == present).when(Expert)\ninvariant(Claim).when(period == extended).then(reminder != null)\n"
    ));
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    let rows = d.rows("Claim");
    assert!(
        rows.contains(&"period=extended, reminder=present".to_string()),
        "{rows:?}"
    );
    assert!(
        !d.rows("Extension")
            .iter()
            .any(|r| r.contains("reason=disaster")),
        "{:?}",
        d.rows("Extension")
    );
}

/// The repro of #110: no rule but the guards reads the reason, so only the guards make it a
/// state var. Picked on the claim's creation or by a later use case, each reason leaves its
/// deadline, and the use case runs.
const READ_ONLY_BY_THE_GUARDS: &str = r#"
variation Reason "Reason" {
  a "A"
  b "B"
}
entity Claim "Claim" {
  id: Int @pk
  reason: Reason
  due_on: Date
  status: Enum(open, extended) @default(open)
}
usecase Open "Open"
usecase PickReason "Pick reason"
usecase Extend "Extend"
creates(Open, Claim)
chooses(PickReason, Claim.reason)
condition IsA "A"
holds(IsA, Claim.reason == a)
condition IsB "B"
holds(IsB, Claim.reason == b)
transitions(Extend, Claim.status, open -> extended)
sets(Extend, Claim.due_on == 2).when(IsA)
sets(Extend, Claim.due_on == 3).when(IsB)
"#;

#[test]
fn the_columns_only_a_guard_reads_are_state() {
    let on_creation = READ_ONLY_BY_THE_GUARDS.replace("chooses(PickReason,", "chooses(Open,");
    for src in [READ_ONLY_BY_THE_GUARDS, on_creation.as_str()] {
        let d = derive_src(src);
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        let rows = d.rows("Claim");
        for (reason, due_on) in [("a", 2), ("b", 3)] {
            assert!(
                rows.iter().any(|r| r.contains("status=extended")
                    && r.contains(&format!("reason={reason}"))
                    && r.contains(&format!("due_on={due_on}"))),
                "{rows:?}"
            );
        }
    }
}
