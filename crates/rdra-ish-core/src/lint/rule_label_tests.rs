use super::*;
use crate::build_model;
use rdra_ish_syntax::parse;

#[test]
fn rule_unlabeled_is_info_and_fires_for_an_unlabelled_rule_only() {
    let (ast, parse_errors) = parse(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid)  paid_at: DateTime @null }
invariant(Order).when(status == paid).then(paid_at == present).label("入金済には入金日時")
forbidden(Order, status == open, paid_at == present)
"#,
    );
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");

    let unlabeled: Vec<LintIssue> = collect_findings(&model)
        .into_iter()
        .filter(|issue| issue.rule == "rule-unlabeled")
        .collect();
    assert_eq!(unlabeled.len(), 1, "{unlabeled:?}");
    assert_eq!(unlabeled[0].severity, LintSeverity::Info);
    assert_eq!(unlabeled[0].subject_kind, "forbidden");
    assert!(unlabeled[0].location.is_some());
    assert!(lint_review_diagnostics(&model)
        .iter()
        .all(|d| !d.error.to_string().contains("rule-unlabeled")));
}
