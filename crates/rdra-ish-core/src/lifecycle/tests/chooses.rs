//! `chooses(UC, Entity.column)` (#54): the actor picks a variation column's value again when
//! the use case runs, one successor per value, as on a creation.

use super::*;

const DRAFT: &str = r#"
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
}
variation Route "経路" {
  mail "郵送"
  online "電子"
}
usecase Receive "受領"
usecase Draft "応答案の作成"
usecase Reopen "最後の通知"
entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  final: Bool @default(false)
  scope: Scope
  route: Route
}
creates(Receive, Notice)
sets(Receive, Notice, scope == limited)
sets(Receive, Notice, route == mail)
transitions(Draft, Notice.status, open -> drafted)
transitions(Reopen, Notice.status, drafted -> open)
sets(Reopen, Notice, final == true)
forbidden(Notice, status == drafted, scope == free)
forbidden(Notice, status == drafted, route == online)
"#;

fn violated(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// Without `chooses` an update keeps the value the creation set: the rules hold.
#[test]
fn an_update_keeps_a_variation_value() {
    let d = derive_src(DRAFT);
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
}

/// With `chooses` the update takes every value of the variation: the rule on `scope` breaks,
/// with a trace through the use case that picks it; the other column keeps its value.
#[test]
fn chooses_takes_every_value_on_an_update() {
    let d = derive_src(&format!("{DRAFT}\nchooses(Draft, Notice.scope)\n"));
    assert_eq!(violated(&d), ["Forbidden_Notice_0"], "{:?}", d.diags);
    let trace = d
        .diags
        .iter()
        .find_map(|x| x.diag.trace())
        .expect("a counterexample");
    let last = trace.steps.last().unwrap();
    assert_eq!(last.action, "Draft");
    // The step cites the `chooses` that picked the value.
    let cause = last
        .changes
        .iter()
        .find_map(|c| match c {
            crate::lifecycle::Change::Set { field, cause, .. } if field == "scope" => cause.clone(),
            _ => None,
        })
        .expect("the step sets scope");
    assert_eq!(cause.fact, crate::lifecycle::ModelRef::ColumnChoice(0));
}

/// Two chosen columns in one step: every pair of values is a successor.
#[test]
fn two_chosen_columns_take_every_pair() {
    let d = derive_src(&format!(
        "{DRAFT}\nchooses(Draft, Notice.scope)\nchooses(Draft, Notice.route)\n"
    ));
    let rows = d.rows("Notice");
    for want in [
        "route=mail, scope=limited",
        "route=mail, scope=free",
        "route=online, scope=limited",
        "route=online, scope=free",
    ] {
        assert!(
            rows.iter()
                .any(|r| r.contains("status=drafted") && r.contains(want)),
            "{want} missing in {rows:?}"
        );
    }
    let mut v = violated(&d);
    v.sort();
    assert_eq!(v, ["Forbidden_Notice_0", "Forbidden_Notice_1"]);
}

/// On the creating use case `chooses` is the creation's own choice: nothing changes.
#[test]
fn chooses_on_a_creation_is_the_creations_choice() {
    let src = DRAFT.replace("sets(Receive, Notice, scope == limited)\n", "");
    let with = derive_src(&format!("{src}\nchooses(Receive, Notice.scope)\n"));
    let without = derive_src(&src);
    assert_eq!(with.rows("Notice"), without.rows("Notice"));
}

/// A `chooses` counts as a write of the entity, as a `sets` does, with no CRUD edge.
#[test]
fn chooses_alone_writes_the_entity() {
    let d = derive_src(&format!(
        "{DRAFT}\nusecase Repick \"選び直し\"\nchooses(Repick, Notice.scope)\n"
    ));
    assert!(
        d.diags.iter().any(|x| matches!(
            &x.diag,
            StateDiag::ConstraintViolated { trace, .. }
                if trace.steps.iter().any(|s| s.action == "Repick")
        )),
        "{:?}",
        d.diags
    );
}
