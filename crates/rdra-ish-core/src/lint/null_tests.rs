use super::*;
use crate::build_model;
use rdra_ish_syntax::parse;

fn negated(src: &str) -> Vec<LintIssue> {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    collect_findings(&model)
        .into_iter()
        .filter(|issue| issue.rule == "negated-order-admits-null")
        .collect()
}

fn item(columns: &str, rest: &str) -> String {
    format!(
        "entity Item \"品\" {{ id: Int @pk  n: Int  {columns} }}\nusecase Make \"作成\"\ncreates(Make, Item)\n{rest}"
    )
}

/// `not` over `<`, `<=`, `>`, `>=` with a nullable operand holds while the operand is null,
/// unlike the ordering it reads as (`not (qty < 1)` is not `qty >= 1`): a warning, in a
/// `@check` and in a property, where comparisons are two-valued.
#[test]
fn a_negated_order_on_a_nullable_column_is_a_warning() {
    let hits = negated(&item("qty: Int @null @check(not (qty < 1))", ""));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].severity, LintSeverity::Warning);
    assert!(
        hits[0].message.contains("not (qty < 1)") && hits[0].message.contains("null"),
        "{}",
        hits[0].message
    );
    assert!(hits[0].hint.contains("qty >= 1"), "{}", hits[0].hint);
    assert!(hits[0].hint.contains("qty == null or"), "{}", hits[0].hint);
    assert!(hits[0].location.is_some());

    // Under `and` / `or`, and with the nullable column on the right.
    let hits = negated(&item(
        "qty: Int @null\n  lim: Int @null @check(n == 1 and not (n <= lim))",
        "",
    ));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].hint.contains("n > lim"), "{}", hits[0].hint);

    let hits = negated(&item(
        "qty: Int @null",
        "property Big always(not (Item.qty < 2))\n",
    ));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].subject_id, "Big");
}

/// What the warning leaves alone: an even number of `not`s, an ordering without `not`,
/// `not` over `==` / `!=` (which `!=` / `==` state, null included), a non-null column, a
/// logical `@derived` (three-valued: null in, null out), and an expression that already
/// says what null means (`qty == null or ...`, `qty == present and ...`).
#[test]
fn a_negated_order_that_means_what_it_says_is_not_flagged() {
    for columns in [
        "qty: Int @null @check(not (not (qty < 1)))",
        "qty: Int @null @check(qty >= 1)",
        "qty: Int @null @check(not (qty == 1))",
        "qty: Int @null @check(qty != 1)",
        "qty: Int @check(not (qty < 1))",
        "m: Int @check(not (m < n))",
        "qty: Int @null\n  big: Bool @null @derived(not (qty < 3))",
        "qty: Int @null @check(qty == null or not (qty < 1))",
        "qty: Int @null @check(qty == present and not (qty < 1))",
    ] {
        assert!(negated(&item(columns, "")).is_empty(), "{columns}");
    }
    assert!(negated(&item(
        "qty: Int @null",
        "property Big always((Item.qty == null) or (not (Item.qty < 2)))\n"
    ))
    .is_empty());
}
