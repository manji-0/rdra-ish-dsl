//! `sets(UC, E.column == F.source)` (#69): a copy is typed against its target column.

use crate::diagnostics::RdraError;
use crate::model::*;
use rdra_ish_syntax::parse;

const BASE: &str = r#"
variation Kind "Kind" {
  food "Food"
  drink "Drink"
}
variation Route "Route" {
  food "Food"
  drink "Drink"
}
entity Parent "Parent" {
  id: Int @pk
  kind: Kind
  route: Route
  status: Enum(open, closed) @default(open)
  other: Enum(open, done) @default(open)
  on: Bool @default(false)
  count: Int @default(0)
  amount: Money @default(0)
  maybe: Int @null
  due: Date
  name: String
}
entity Child "Child" {
  id: Int @pk
  kind: Kind
  status: Enum(open, closed) @default(open)
  on: Bool @default(false)
  count: Int @default(0)
  maybe: Int @null
  due: DateTime
  name: String
}
relate(Child, Parent, N:1)
usecase U "U"
creates(U, Child)
"#;

fn build(line: &str) -> (SemanticModel, Vec<RdraError>) {
    let (ast, errors) = parse(&format!("{BASE}{line}\n"));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (model, diags) = crate::analysis::build_model(&ast);
    let errors = diags
        .into_iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error)
        .collect();
    (model, errors)
}

fn copy_of(line: &str) -> (String, String, String) {
    let (model, errors) = build(line);
    assert!(errors.is_empty(), "{line}: {errors:?}");
    let ce = model.column_effects.last().expect("an effect");
    let SetValue::Copy(source) = &ce.value else {
        panic!("{line}: not a copy: {:?}", ce.value);
    };
    (
        ce.column.clone(),
        model.entities[source.entity].id.clone(),
        source.column.clone(),
    )
}

#[test]
fn a_qualified_column_is_a_copy() {
    for (line, column, source) in [
        ("sets(U, Child.kind == Parent.kind)", "kind", "kind"),
        ("sets(U, Child.status == Parent.status)", "status", "status"),
        ("sets(U, Child.on == Parent.on)", "on", "on"),
        ("sets(U, Child.count == Parent.amount)", "count", "amount"),
        ("sets(U, Child.maybe == Parent.count)", "maybe", "count"),
        ("sets(U, Child.maybe == Parent.maybe)", "maybe", "maybe"),
        ("sets(U, Child.due == Parent.due)", "due", "due"),
        ("sets(U, Child.on == Parent.on)", "on", "on"),
    ] {
        assert_eq!(
            copy_of(line),
            (column.to_string(), "Parent".to_string(), source.to_string()),
            "{line}"
        );
    }
}

#[test]
fn a_source_whose_values_the_target_cannot_hold_is_a_mismatch() {
    for line in [
        // Two variations, even with the same value names.
        "sets(U, Child.kind == Parent.route)",
        // Two Enums with different variants.
        "sets(U, Child.status == Parent.other)",
        "sets(U, Child.on == Parent.status)",
        "sets(U, Child.count == Parent.due)",
        // A null has no image in a non-null column.
        "sets(U, Child.count == Parent.maybe)",
    ] {
        let (_, errors) = build(line);
        assert!(
            matches!(errors.as_slice(), [RdraError::SetsCopyMismatch { .. }]),
            "{line}: {errors:?}"
        );
    }
}

#[test]
fn a_copy_into_a_column_without_state_is_rejected() {
    let (_, errors) = build("sets(U, Child.name == Parent.name)");
    assert!(
        matches!(
            errors.as_slice(),
            [RdraError::EffectOnNonStateColumn { .. }]
        ),
        "{errors:?}"
    );
}

/// A foreign key is the relate's owner, not a state column: no copy reads or writes it.
#[test]
fn a_foreign_key_is_no_copy_source_or_target() {
    let (ast, errors) = parse(&format!(
        "{BASE}entity Other \"O\" {{\n  id: Int @pk\n  ref_id: Int\n  count: Int\n}}\nrelate(Other, Parent, N:1).key(ref_id)\nusecase V \"V\"\ncreates(V, Other)\nsets(V, Other.count == Other.ref_id)\n"
    ));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (_, diags) = crate::analysis::build_model(&ast);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| &d.error)
        .collect();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, RdraError::EffectOnNonStateColumn { col, .. } if col == "ref_id")),
        "{errors:?}"
    );
}

#[test]
fn an_unknown_source_column_is_reported_once() {
    let (_, errors) = build("sets(U, Child.kind == Parent.nope)");
    assert!(
        matches!(errors.as_slice(), [RdraError::UnknownColumn { col, .. }] if col == "nope"),
        "{errors:?}"
    );
}

/// `!=` names no value to leave, and a bare column is no copy: both set nothing.
#[test]
fn only_an_equality_with_a_qualified_column_copies() {
    for line in [
        "sets(U, Child.kind != Parent.kind)",
        "sets(U, Child.count == maybe)",
    ] {
        let (_, errors) = build(line);
        assert!(
            matches!(errors.as_slice(), [RdraError::SetsNoEffect { .. }]),
            "{line}: {errors:?}"
        );
    }
}

/// A copy is a `sets`: with a `chooses` of the same column it is `ChoosesAndSets`.
#[test]
fn a_copy_and_a_choice_of_one_column_conflict() {
    let (_, errors) = build("sets(U, Child.kind == Parent.kind)\nchooses(U, Child.kind)");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, RdraError::ChoosesAndSets { .. })),
        "{errors:?}"
    );
}
