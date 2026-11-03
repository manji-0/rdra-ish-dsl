//! A guard per chosen value (#65): `chooses(UC, E.col, v).when(Cond)` lists the values the
//! actor may pick, each line one value, and a `.when` makes that value a choice only while the
//! condition's `holds` body holds, read as a `transitions(..).when(Cond)` arm guard reads it.

use super::*;
use crate::lifecycle::diag::Blocked;
use crate::lifecycle::GuardWhy;
use rdra_ish_syntax::parse;

/// A notice the attorney answers, then reopens as a final one: a final notice admits a
/// limited amendment only.
const NOTICE: &str = r#"
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
  none "補正なし"
}
usecase Receive "受領"
usecase Draft "応答案の作成"
usecase Reopen "最後の通知"
entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  final: Bool @default(false)
  scope: Scope
}
creates(Receive, Notice)
sets(Receive, Notice.scope == limited)
transitions(Draft, Notice.status, open -> drafted)
transitions(Reopen, Notice.status, drafted -> open)
sets(Reopen, Notice.final == true)
sets(Reopen, Notice.scope == limited)
condition NotFinal "最後の通知でない"
holds(NotFinal, Notice.final == false)
forbidden(Notice, final == true, scope == free)
"#;

const GUARDED: &str =
    "chooses(Draft, Notice.scope, free).when(NotFinal)\nchooses(Draft, Notice.scope, limited)";

fn violated(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn never_chosen(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ValueNeverChosen { action, value, .. } => Some(format!("{action} {value}")),
            _ => None,
        })
        .collect()
}

/// The guard keeps the rule: `free` only on a notice that is not final, `limited` always;
/// the value-less form picks `free` on a final notice too.
#[test]
fn a_guarded_value_is_chosen_only_while_its_guard_holds() {
    let d = derive_src(&format!("{NOTICE}\n{GUARDED}\n"));
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    let rows = d.rows("Notice");
    assert!(
        rows.iter()
            .any(|r| r.contains("status=drafted") && r.contains("scope=free")),
        "{rows:?}"
    );
    assert!(never_chosen(&d).is_empty(), "{:?}", d.diags);

    let any = derive_src(&format!("{NOTICE}\nchooses(Draft, Notice.scope)\n"));
    assert_eq!(violated(&any), ["Forbidden_Notice_0"], "{:?}", any.diags);
}

/// Only the listed values are choices: `none` is a value of the variation no line names.
#[test]
fn only_the_listed_values_are_chosen() {
    let d = derive_src(&format!("{NOTICE}\n{GUARDED}\n"));
    let rows = d.rows("Notice");
    assert!(!rows.iter().any(|r| r.contains("scope=none")), "{rows:?}");
    let all = derive_src(&format!("{NOTICE}\nchooses(Draft, Notice.scope)\n"));
    let rows = all.rows("Notice");
    assert!(rows.iter().any(|r| r.contains("scope=none")), "{rows:?}");
}

/// A counterexample step cites the line of the value it picked.
#[test]
fn a_step_cites_the_line_of_the_value_it_chose() {
    let src = format!(
        "{NOTICE}\nchooses(Draft, Notice.scope, limited)\nchooses(Draft, Notice.scope, free)\n"
    );
    let d = derive_src(&src);
    let trace = d
        .diags
        .iter()
        .find_map(|x| x.diag.trace())
        .expect("a counterexample");
    let last = trace.steps.last().unwrap();
    assert_eq!(last.action, "Draft");
    let cause = last
        .changes
        .iter()
        .find_map(|c| match c {
            crate::lifecycle::Change::Set { field, cause, .. } if field == "scope" => cause.clone(),
            _ => None,
        })
        .expect("the step sets scope");
    assert_eq!(cause.fact, crate::lifecycle::ModelRef::ColumnChoice(0));
    let span = cause.span.expect("the value's line");
    assert_eq!(
        &src[span.span.clone()],
        "chooses(Draft, Notice.scope, free)"
    );
}

/// A guard that never holds drops its value and a warning names it; the others still run.
#[test]
fn a_value_whose_guard_never_holds_is_never_chosen_and_says_so() {
    let d = derive_src(&format!(
        "{NOTICE}\ncondition Never \"起きない\"\nholds(Never, Notice.final == true, Notice.status == drafted)\nchooses(Draft, Notice.scope, free).when(Never)\nchooses(Draft, Notice.scope, limited)\n"
    ));
    assert!(
        !d.rows("Notice").iter().any(|r| r.contains("scope=free")),
        "{:?}",
        d.rows("Notice")
    );
    assert_eq!(never_chosen(&d), ["Draft free"]);
    let warning = d
        .diags
        .iter()
        .find(|x| matches!(x.diag, StateDiag::ValueNeverChosen { .. }))
        .unwrap();
    assert_eq!(warning.diag.severity(), crate::lifecycle::Severity::Warning);
    assert_eq!(
        warning.diag.to_string(),
        "Draft never chooses Notice.scope = free: wherever it could, its guard Never does not hold"
    );
}

/// Every value guarded and none enabled: the use case cannot run, and `NeverRuns` names the
/// value guard.
#[test]
fn a_use_case_with_no_enabled_value_never_runs() {
    let d = derive_src(&format!(
        "{NOTICE}\ncondition Never \"起きない\"\nholds(Never, Notice.final == true, Notice.status == drafted)\nchooses(Draft, Notice.scope, free).when(Never)\n"
    ));
    let why = d
        .diags
        .iter()
        .find_map(|x| match &x.diag {
            StateDiag::NeverRuns { action, why, .. } if action == "Draft" => Some(why.clone()),
            _ => None,
        })
        .expect("Draft never runs");
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

/// The same value twice, as for a `transitions` arm declared twice: either line enables it,
/// so an unguarded duplicate lifts the guard.
#[test]
fn a_value_listed_twice_is_enabled_by_either_line() {
    let d = derive_src(&format!(
        "{NOTICE}\n{GUARDED}\nchooses(Draft, Notice.scope, free)\n"
    ));
    assert_eq!(violated(&d), ["Forbidden_Notice_0"], "{:?}", d.diags);
}

/// On the creating use case the listed values restrict the creation's own choice.
#[test]
fn listed_values_restrict_a_creation() {
    let src = NOTICE.replace("sets(Receive, Notice.scope == limited)\n", "");
    let d = derive_src(&format!(
        "{src}\nchooses(Receive, Notice.scope, limited)\nchooses(Receive, Notice.scope, none)\n"
    ));
    let rows = d.rows("Notice");
    assert!(!rows.iter().any(|r| r.contains("scope=free")), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("scope=none")), "{rows:?}");
}

fn errors(src: &str) -> Vec<String> {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (_, diags) = build_model(&ast);
    diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect()
}

/// A value outside the variation is an error that lists the variation's values.
#[test]
fn a_value_outside_the_variation_lists_the_values() {
    let errs = errors(&format!("{NOTICE}\nchooses(Draft, Notice.scope, wide)\n"));
    assert_eq!(
        errs,
        ["invalid variation value: 'wide' is not a value of variation 'Scope'\n  hint: declared values are: limited, free, none"]
    );
}

/// Listing values and picking any value for one use case and column is an error, in either
/// order.
#[test]
fn listed_values_and_any_value_do_not_mix() {
    for lines in [
        "chooses(Draft, Notice.scope)\nchooses(Draft, Notice.scope, free)",
        "chooses(Draft, Notice.scope, free)\nchooses(Draft, Notice.scope)",
    ] {
        let errs = errors(&format!("{NOTICE}\n{lines}\n"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0]
                .starts_with("chooses(Draft, Notice.scope) both lists values and picks any value"),
            "{errs:?}"
        );
    }
}

/// `.when` on the value-less form guards no value.
#[test]
fn a_guard_needs_a_value() {
    let errs = errors(&format!(
        "{NOTICE}\nchooses(Draft, Notice.scope).when(NotFinal)\n"
    ));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].starts_with("`chooses(Draft, Notice.scope).when(...)` guards no value"),
        "{errs:?}"
    );
}

/// A guard is a condition with a `holds` body, as for a `transitions` arm.
#[test]
fn a_value_guard_needs_a_condition_with_a_body() {
    let errs = errors(&format!(
        "{NOTICE}\ncondition Bare \"本体なし\"\nchooses(Draft, Notice.scope, free).when(Bare)\n"
    ));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].starts_with("condition 'Bare' guards `chooses(Draft, ...)` with `.when(Bare)`"),
        "{errs:?}"
    );
    let errs = errors(&format!(
        "{NOTICE}\nchooses(Draft, Notice.scope, free).when(Notice.final == false)\n"
    ));
    assert!(
        errs.iter()
            .any(|e| e.starts_with("`chooses(...).when(...)` must name one condition, once")),
        "{errs:?}"
    );
}

/// A guard reads the state the use case starts from, as an arm guard does: a guard on the
/// chosen column itself sees the old value.
#[test]
fn a_value_guard_reads_the_pre_state() {
    let src = r#"
variation Scope "範囲" {
  limited "限定"
  free "限定なし"
}
usecase Receive "受領"
usecase Draft "作成"
entity Notice "通知" { id: Int @pk  scope: Scope }
creates(Receive, Notice)
sets(Receive, Notice.scope == limited)
condition WasLimited "限定だった"
holds(WasLimited, Notice.scope == limited)
chooses(Draft, Notice.scope, free).when(WasLimited)
chooses(Draft, Notice.scope, limited)
"#;
    let d = derive_src(src);
    // From `limited` Draft may pick `free`; from `free` only `limited`, never `free` again.
    let rows = d.rows("Notice");
    assert!(rows.iter().any(|r| r == "scope=free"), "{rows:?}");
    assert!(never_chosen(&d).is_empty(), "{:?}", d.diags);
}
