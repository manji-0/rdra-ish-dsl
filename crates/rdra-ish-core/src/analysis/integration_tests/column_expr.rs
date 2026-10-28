use crate::analysis::build_model;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::*;
use rdra_ish_syntax::parse;

fn build(src: &str) -> (SemanticModel, Vec<Diagnostic>) {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    build_model(&ast)
}

fn errors(src: &str) -> Vec<RdraError> {
    build(src)
        .1
        .into_iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error)
        .collect()
}

fn column<'m>(model: &'m SemanticModel, entity: &str, column: &str) -> &'m ModelColumn {
    model
        .entities
        .values()
        .find(|e| e.id == entity)
        .and_then(|e| e.columns.iter().find(|c| c.name == column))
        .unwrap()
}

#[test]
fn a_check_resolves_to_a_typed_condition_over_its_entity() {
    let src = r#"entity Item "Item" {
  id: Int @pk
  qty: Int @check(qty >= 1 and not qty > stock)
  stock: Int
  net: Int @derived(stock - qty)
}"#;
    let (model, diags) = build(src);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    let qty = column(&model, "Item", "qty");
    assert_eq!(qty.check_constraints.len(), 1);
    assert_eq!(qty.checks_display(), "(qty >= 1) and (not (qty > stock))");
    assert!(qty.check_constraints[0].site.is_some());
    // The emitters print what `rdra-ish fmt` writes.
    let formatted = rdra_ish_syntax::format_source(src).unwrap();
    assert!(
        formatted.contains(&format!("@check({})", qty.checks_display())),
        "{formatted}"
    );
    let net = column(&model, "Item", "net").derived_display();
    assert!(
        formatted.contains(&format!("@derived({net})")),
        "{formatted}"
    );
}

#[test]
fn a_check_naming_a_missing_column_is_unknown_column() {
    let errs = errors(r#"entity Item "Item" { id: Int @pk qty: Int @check(count >= 1) }"#);
    assert!(
        matches!(&errs[..], [RdraError::UnknownColumn { entity, col }] if entity == "Item" && col == "count"),
        "{errs:?}"
    );
}

#[test]
fn a_check_compares_by_the_rules_forbidden_uses() {
    let errs = errors(r#"entity Item "Item" { id: Int @pk name: String @check(name < 3) }"#);
    assert!(
        matches!(&errs[..], [RdraError::ComparisonOpNotOrdered { .. }]),
        "{errs:?}"
    );
}

#[test]
fn arithmetic_in_a_check_is_arith_not_in_check() {
    let errs = errors(r#"entity Item "Item" { id: Int @pk qty: Int @check(qty - 1) }"#);
    assert!(
        matches!(&errs[..], [RdraError::ArithNotInCheck { column, .. }] if column == "qty"),
        "{errs:?}"
    );
}

#[test]
fn derived_arithmetic_reads_numeric_columns_only() {
    let (model, diags) = build(
        r#"entity Order "Order" {
  id: Int @pk
  total: Money
  discount: Money
  net: Money @derived(total - discount)
  big: Bool @derived(total >= 3)
}"#,
    );
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    assert_eq!(
        column(&model, "Order", "net").derived_display(),
        "total - discount"
    );
    assert_eq!(
        column(&model, "Order", "big").derived_display(),
        "total >= 3"
    );

    let errs = errors(
        r#"entity Order "Order" { id: Int @pk note: String total: Money @derived(note * 2) }"#,
    );
    assert!(
        matches!(&errs[..], [RdraError::ArithTypeMismatch { operand, .. }] if operand == "note"),
        "{errs:?}"
    );
    let errs =
        errors(r#"entity Order "Order" { id: Int @pk total: Money @derived(total + gift) }"#);
    assert!(
        matches!(&errs[..], [RdraError::UnknownColumn { col, .. }] if col == "gift"),
        "{errs:?}"
    );
}

#[test]
fn a_derived_column_type_matches_its_expression() {
    let errs = errors(
        r#"entity Order "Order" { id: Int @pk total: Money flag: Bool @derived(total + 1) n: Int @derived(total >= 1) }"#,
    );
    assert!(
        matches!(
            &errs[..],
            [
                RdraError::DerivedTypeMismatch { column: a, .. },
                RdraError::DerivedTypeMismatch { column: b, .. }
            ] if a == "flag" && b == "n"
        ),
        "{errs:?}"
    );
}

#[test]
fn a_derived_column_carries_no_check_default_pk_or_unique() {
    let errs = errors(
        r#"entity Order "Order" {
  id: Int @pk
  total: Money
  net: Money @derived(total * 2) @check(net >= 0) @default(0) @unique
}"#,
    );
    let refused: Vec<&str> = errs
        .iter()
        .filter_map(|e| match e {
            RdraError::DerivedColumnAnnotation { annotation, .. } => Some(annotation.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(refused, ["@check", "@default", "@unique"], "{errs:?}");
}

#[test]
fn a_sets_or_transitions_on_a_derived_column_is_derived_column_written() {
    let errs = errors(
        r#"entity Order "Order" {
  id: Int @pk
  total: Money
  big: Bool @derived(total >= 3)
}
usecase Mark "Mark"
sets(Mark, Order, big == true)
"#,
    );
    assert!(
        errs.iter().any(|e| matches!(
            e,
            RdraError::DerivedColumnWritten { column, usecase, .. } if column == "big" && usecase == "Mark"
        )),
        "{errs:?}"
    );
}
