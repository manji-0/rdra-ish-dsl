//! A bare right-hand side of `col == x` / `col != x` is read by the type of `col`: a value of
//! an Enum, variation, or Bool column (and `null`/`present` of a nullable one) before a
//! column of the same name. Every construct that takes a comparison reads it the same way.

use crate::diagnostics::RdraError;
use crate::model::*;
use rdra_ish_syntax::parse;

fn build(src: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    crate::analysis::build_model(&ast)
}

fn errors(diags: &[crate::Diagnostic]) -> Vec<&RdraError> {
    diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| &d.error)
        .collect()
}

/// `paid` is both a Bool column and a `status` value; `open` is a value only; `paid_at` and
/// `other` are columns only.
const BASE: &str = r#"
variation Method "Method" {
  cash "Cash"
  card "Card"
}
entity E "E" {
  id: Int @pk
  paid: Bool
  cash: Bool
  status: Enum(pending, open, paid) @default(pending)
  other: Enum(pending, open, paid) @default(pending)
  method: Method @default(cash)
  paid_at: DateTime @null
  qty: Int
  limit: Int
}
entity Item "Item" {
  id: Int @pk
  paid: Bool
  status: Enum(pending, paid) @default(pending)
}
relate(Item, E, N:1)
usecase U "U"
transitions(U, E.status, pending -> paid)
"#;

fn with(extra: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    build(&format!("{BASE}{extra}"))
}

fn ok(extra: &str) -> SemanticModel {
    let (model, diags) = with(extra);
    let errs = errors(&diags);
    assert!(errs.is_empty(), "{errs:?}");
    model
}

fn variant(v: &str) -> EffectValue {
    EffectValue::EnumVariant(v.to_string())
}

fn equals(model: &SemanticModel, cond: &CrossEntityCondition) -> (String, String, EffectValue) {
    match cond {
        CrossEntityCondition::Equals { column, value } => (
            model.entities[column.entity].id.clone(),
            column.column.clone(),
            value.clone(),
        ),
        other => panic!("not a value condition: {other:?}"),
    }
}

#[test]
fn forbidden_reads_a_bare_rhs_that_is_also_a_column_as_the_value() {
    let model = ok("forbidden(E, status == paid, paid == false)\n");
    let f = &model.forbidden_constraints[0];
    assert_eq!(
        f.conditions,
        vec![
            ("status".to_string(), variant("paid")),
            ("paid".to_string(), EffectValue::Bool(false)),
        ]
    );
    assert!(f.comparisons.is_empty());
}

#[test]
fn required_and_exclusive_read_the_value() {
    let model = ok("required(E, status == paid)\nexclusive(E, status == paid, method == cash)\n");
    assert_eq!(
        model.required_constraints[0].conditions,
        vec![("status".to_string(), variant("paid"))]
    );
    assert_eq!(
        model.exclusive_constraints[0].conditions,
        vec![
            ("status".to_string(), variant("paid")),
            ("method".to_string(), variant("cash")),
        ]
    );
}

#[test]
fn single_entity_invariant_reads_the_value_in_when_and_then() {
    let model = ok("invariant(E).when(status == paid).then(method == cash)\n");
    let inv = &model.entity_invariants[0];
    assert_eq!(inv.guards, vec![("status".to_string(), variant("paid"))]);
    assert_eq!(inv.requireds, vec![("method".to_string(), variant("cash"))]);
}

#[test]
fn cross_invariant_quantifier_reads_the_value() {
    let model = ok("invariant(E).when(E.status == paid).none(Item.status == paid)\n");
    let q = &model.quantifier_constraints[0];
    assert_eq!(
        equals(&model, &q.guards[0]),
        ("E".to_string(), "status".to_string(), variant("paid"))
    );
    assert_eq!(
        equals(&model, &q.quantifier.conditions[0]),
        ("Item".to_string(), "status".to_string(), variant("paid"))
    );
}

#[test]
fn cross_forbidden_reads_the_value() {
    let model = ok("forbidden(E, Item, E.status == paid, Item.status == paid).along(Item, E)\n");
    let f = &model.cross_forbidden_constraints[0];
    assert_eq!(
        equals(&model, &f.conditions[0]),
        ("E".to_string(), "status".to_string(), variant("paid"))
    );
}

#[test]
fn holds_guard_reads_the_value() {
    let model = ok(
        "condition Settled \"Settled\"\nholds(Settled, E.status == paid, E.method == cash)\napplies_to(Settled, U)\n",
    );
    let body = model
        .conditions
        .values()
        .next()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        equals(&model, &body.conds[0]),
        ("E".to_string(), "status".to_string(), variant("paid"))
    );
    assert_eq!(
        equals(&model, &body.conds[1]),
        ("E".to_string(), "method".to_string(), variant("cash"))
    );
}

#[test]
fn after_assert_reads_the_value() {
    let model = ok("after(U).assert(E.status == paid)\n");
    let a = &model.temporal_assertions[0];
    assert_eq!(
        equals(&model, &a.requireds[0]),
        ("E".to_string(), "status".to_string(), variant("paid"))
    );
}

#[test]
fn sets_reads_the_value() {
    let model = ok("sets(U, E, method == cash)\n");
    let ce = &model.column_effects[0];
    assert_eq!(
        (ce.column.as_str(), ce.value.literal()),
        ("method", Some(&variant("cash")))
    );
}

#[test]
fn check_reads_an_enum_value_instead_of_a_column() {
    let (model, diags) = build(
        r#"
entity T "T" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  closed_at: DateTime @null @check(status == open or closed_at == present)
}
"#,
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let columns = &model.entities.values().next().unwrap().columns;
    let closed_at = columns.iter().find(|c| c.name == "closed_at").unwrap();
    let check = &closed_at.check_constraints[0];
    let leaves = check.expr.comparisons();
    assert_eq!(leaves[0].rhs, CmpRhs::Value(variant("open")));
    assert_eq!(leaves[1].rhs, CmpRhs::Value(EffectValue::Present));
}

#[test]
fn derived_reads_the_value() {
    let (model, diags) = build(
        r#"
entity T "T" {
  id: Int @pk
  paid: Bool
  status: Enum(open, paid) @default(open)
  settled: Bool @derived(status == paid)
}
"#,
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let settled = model
        .entities
        .values()
        .next()
        .unwrap()
        .columns
        .iter()
        .find(|c| c.name == "settled")
        .unwrap();
    let Some(DerivedExpr::Logical(CheckExpr::Cmp(prop))) = &settled.derived_expr else {
        panic!("{:?}", settled.derived_expr);
    };
    assert_eq!(prop.rhs, CmpRhs::Value(variant("paid")));
}

#[test]
fn not_equal_to_a_value_is_a_comparison_with_a_value() {
    let model = ok("forbidden(E, status != open, paid == false)\n");
    let f = &model.forbidden_constraints[0];
    assert_eq!(f.comparisons[0].rhs, CmpRhs::Value(variant("open")));
    assert_eq!(f.comparisons[0].op, CmpOpModel::Ne);
    let model = ok("invariant(E).when(E.status != paid).none(Item.status != pending)\n");
    let q = &model.quantifier_constraints[0];
    let CrossEntityCondition::Comparison(prop) = &q.guards[0] else {
        panic!("{:?}", q.guards[0]);
    };
    assert_eq!(prop.rhs, CrossCmpRhs::Value(variant("paid")));
}

#[test]
fn a_bare_column_that_is_not_a_value_is_an_error_with_the_qualified_rewrite() {
    let (_, diags) = with("forbidden(E, status == other)\n");
    let errs = errors(&diags);
    assert!(
        matches!(
            errs.as_slice(),
            [RdraError::ComparisonRhsNotAValue { col, rhs, qualified, .. }]
                if col == "status" && rhs == "other" && qualified == "E.other"
        ),
        "{errs:?}"
    );
    let message = errs[0].to_string();
    assert!(
        message.contains("write `E.other` to compare columns"),
        "{message}"
    );

    let (_, diags) = with("invariant(E).when(E.paid == cash).none(Item.paid == true)\n");
    assert!(
        matches!(
            errors(&diags).as_slice(),
            [RdraError::ComparisonRhsNotAValue { qualified, .. }] if qualified == "E.cash"
        ),
        "{diags:?}"
    );
}

#[test]
fn a_qualified_rhs_is_a_column_comparison() {
    let model = ok("forbidden(E, status == E.other)\n");
    assert_eq!(
        model.forbidden_constraints[0].comparisons[0].rhs,
        CmpRhs::Column("other".to_string())
    );
}

#[test]
fn a_bare_rhs_on_a_numeric_column_is_still_a_column() {
    let model = ok("forbidden(E, qty == limit)\nforbidden(E, paid_at == null)\n");
    assert_eq!(
        model.forbidden_constraints[0].comparisons[0].rhs,
        CmpRhs::Column("limit".to_string())
    );
    assert_eq!(
        model.forbidden_constraints[1].conditions,
        vec![("paid_at".to_string(), EffectValue::Null)]
    );
}

fn atom(model: &SemanticModel, i: usize) -> &TemporalAtom {
    match &model.temporal_properties[i].formula {
        TemporalFormula::Always(TemporalExpr::Atom(a)) => a,
        other => panic!("{other:?}"),
    }
}

#[test]
fn property_atoms_read_by_the_column_type() {
    let model = ok(
        "property A\n  always(E.status != paid)\nproperty B\n  always(E.qty <= limit)\nproperty C\n  always(E.qty == limit)\n",
    );
    assert_eq!(atom(&model, 0).rhs, TemporalRhs::Value(variant("paid")));
    let column = TemporalRhs::Column {
        entity: None,
        column: "limit".to_string(),
    };
    assert_eq!(atom(&model, 1).rhs, column);
    // Before, `==` read any bare name as a value, so `qty == limit` compared to nothing.
    assert_eq!(atom(&model, 2).rhs, column);

    let (_, diags) = with("property D\n  always(E.status == other)\n");
    assert!(
        matches!(
            errors(&diags).as_slice(),
            [RdraError::ComparisonRhsNotAValue { qualified, .. }] if qualified == "E.other"
        ),
        "{diags:?}"
    );
}

#[test]
fn transitions_read_values_named_like_columns() {
    let model = ok("usecase V \"V\"\ntransitions(V, E.status, paid -> open)\n");
    assert!(model
        .state_transitions
        .iter()
        .any(|t| t.from == "paid" && t.to == "open"));
}

/// A value is a bare token and a quoted string is free text (#48): a quoted `chooses` value or
/// an Enum or variation `@default` is `QuotedValue`, naming the bare rewrite.
#[test]
fn a_quoted_value_is_rejected_with_its_bare_rewrite() {
    let quoted = |extra: &str| -> Vec<(String, String)> {
        let (_, diags) = with(extra);
        errors(&diags)
            .into_iter()
            .filter_map(|e| match e {
                RdraError::QuotedValue { value, rewrite, .. } => {
                    Some((value.clone(), rewrite.clone()))
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        quoted("usecase V \"V\"\nupdates(V, E)\nchooses(V, E.method, \"card\")\n"),
        [("card".to_string(), "chooses(V, E.method, card)".to_string())]
    );
    let entity = "entity Q \"Q\" {\n  id: Int @pk\n  s: Enum(x, y) @default(\"x\")\n  m: Method @default(\"cash\")\n  n: Int @default(\"4\")\n  t: String @default(\"hello\")\n}\n";
    assert_eq!(
        quoted(entity),
        [
            ("x".to_string(), "@default(x)".to_string()),
            ("cash".to_string(), "@default(cash)".to_string()),
        ]
    );
    // The bare forms load.
    ok("usecase V \"V\"\nupdates(V, E)\nchooses(V, E.method, card)\n");
    ok("entity Q \"Q\" {\n  id: Int @pk\n  s: Enum(x, y) @default(x)\n  m: Method @default(cash)\n}\n");
}

/// Every other place a value is written takes only a bare token: a quoted one does not parse.
#[test]
fn a_quoted_value_does_not_parse_where_the_grammar_takes_a_token() {
    for extra in [
        "transitions(U, E.status, \"pending\" -> paid)",
        "transitions(U, E.status, pending -> \"paid\")",
        "sets(U, E, status == \"paid\")",
        "condition C \"C\"\nholds(C, E.status == \"paid\")",
        "condition C \"C\"\nholds(C, E.status in (\"paid\", open))",
        "condition C \"C\"\nholds(C, E.status not in (\"paid\"))",
        "invariant(E).when(status == \"paid\").then(qty > 0)",
        "entity Q \"Q\" {\n  id: Int @pk\n  s: Enum(\"x\", y)\n}",
        "variation V \"V\" {\n  \"x\"\n}",
    ] {
        let (_, errors) = parse(&format!("{BASE}{extra}\n"));
        assert!(!errors.is_empty(), "{extra} parsed");
    }
}
