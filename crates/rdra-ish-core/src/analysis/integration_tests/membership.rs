//! `col in (a, b)` / `col not in (a, b)`: every construct that takes a comparison takes a set
//! membership, and each value is a value of the left-hand column's type.

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

const BASE: &str = r#"
variation Method "Method" {
  cash "Cash"
  card "Card"
}
entity E "E" {
  id: Int @pk
  paid: Bool
  status: Enum(pending, open, paid) @default(pending)
  method: Method @default(cash)
  paid_at: DateTime @null
  qty: Int
  name: String
  due: Date
}
entity Item "Item" {
  id: Int @pk
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

/// The one error `extra` draws, as its `Debug` text.
fn error(extra: &str) -> String {
    let (_, diags) = with(extra);
    let errs = errors(&diags);
    assert_eq!(errs.len(), 1, "{errs:?}");
    format!("{:?}", errs[0])
}

fn variant(v: &str) -> EffectValue {
    EffectValue::EnumVariant(v.to_string())
}

fn local(op: CmpOpModel, column: &str, values: Vec<EffectValue>) -> ComparisonProp {
    ComparisonProp {
        lhs_column: column.to_string(),
        op,
        rhs: CmpRhs::Values(values),
    }
}

/// `(entity, column, op, values)` of a cross-entity membership.
fn cross(
    model: &SemanticModel,
    cond: &CrossEntityCondition,
) -> (String, String, CmpOpModel, Vec<EffectValue>) {
    match cond {
        CrossEntityCondition::Comparison(CrossComparisonProp {
            lhs,
            op,
            rhs: CrossCmpRhs::Values(values),
        }) => (
            model.entities[lhs.entity].id.clone(),
            lhs.column.clone(),
            *op,
            values.clone(),
        ),
        other => panic!("not a membership: {other:?}"),
    }
}

#[test]
fn single_entity_rules_record_a_membership() {
    let model = ok(r#"
forbidden(E, status in (open, paid))
required(E, method not in (card))
exclusive(E, paid in (true), status not in (pending))
invariant(E).when(status in (open, paid)).then(paid_at not in (null))
"#);
    assert_eq!(
        model.forbidden_constraints[0].comparisons,
        vec![local(
            CmpOpModel::In,
            "status",
            vec![variant("open"), variant("paid")]
        )]
    );
    assert_eq!(
        model.required_constraints[0].comparisons,
        vec![local(CmpOpModel::NotIn, "method", vec![variant("card")])]
    );
    assert_eq!(model.exclusive_constraints[0].comparisons.len(), 2);
    assert_eq!(
        model.exclusive_constraints[0].comparisons[0].rhs,
        CmpRhs::Values(vec![EffectValue::Bool(true)])
    );
    let inv = &model.entity_invariants[0];
    assert_eq!(
        inv.guard_comparisons,
        vec![local(
            CmpOpModel::In,
            "status",
            vec![variant("open"), variant("paid")]
        )]
    );
    assert_eq!(
        inv.required_comparisons,
        vec![local(CmpOpModel::NotIn, "paid_at", vec![EffectValue::Null])]
    );
}

#[test]
fn an_int_column_takes_integer_literals() {
    let model = ok("forbidden(E, qty in (1, 2))\n");
    assert_eq!(
        model.forbidden_constraints[0].comparisons,
        vec![local(
            CmpOpModel::In,
            "qty",
            vec![EffectValue::Int(1), EffectValue::Int(2)]
        )]
    );
}

#[test]
fn cross_entity_constructs_record_a_membership() {
    let model = ok(r#"
condition Live "Live"
holds(Live, E.status not in (paid)).none(Item.status in (paid))
applies_to(Live, U)
forbidden(E, Item, E.status in (open), Item.status not in (pending)).along(Item, E)
invariant(E, Item).when(E.status in (paid)).then(Item.status in (paid))
invariant(E).when(E.status in (paid)).has(Item.status in (paid))
after(U).assert(E.status in (paid, open))
"#);
    let body = model
        .conditions
        .values()
        .next()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        cross(&model, &body.conds[0]),
        (
            "E".to_string(),
            "status".to_string(),
            CmpOpModel::NotIn,
            vec![variant("paid")]
        )
    );
    assert_eq!(
        cross(&model, &body.quantifiers[0].conditions[0]).2,
        CmpOpModel::In
    );
    let f = &model.cross_forbidden_constraints[0];
    assert_eq!(cross(&model, &f.conditions[1]).2, CmpOpModel::NotIn);
    let inv = &model.cross_entity_invariants[0];
    assert_eq!(cross(&model, &inv.requireds[0]).0, "Item");
    let q = &model.quantifier_constraints[0];
    assert_eq!(cross(&model, &q.quantifier.conditions[0]).0, "Item");
    let a = &model.temporal_assertions[0];
    assert_eq!(
        cross(&model, &a.requireds[0]).3,
        vec![variant("paid"), variant("open")]
    );
}

#[test]
fn check_and_property_take_a_membership() {
    let model = ok(r#"
property Settles "settles" eventually(E.status in (paid) or E.method not in (cash))
"#);
    let TemporalFormula::Eventually(TemporalExpr::Or(a, b)) = &model.temporal_properties[0].formula
    else {
        panic!("expected eventually(or)");
    };
    let TemporalExpr::Atom(a) = &**a else {
        panic!("expected atom");
    };
    assert_eq!(a.op, CmpOpModel::In);
    assert_eq!(a.rhs, TemporalRhs::Values(vec![variant("paid")]));
    let TemporalExpr::Atom(b) = &**b else {
        panic!("expected atom");
    };
    assert_eq!(b.op, CmpOpModel::NotIn);

    let (model, diags) = build(
        r#"
entity T "T" {
  id: Int @pk
  status: Enum(open, held, closed) @default(open)
  closed_at: DateTime @null @check(status in (open, held) or closed_at == present)
}
"#,
    );
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let columns = &model.entities.values().next().unwrap().columns;
    let closed_at = columns.iter().find(|c| c.name == "closed_at").unwrap();
    let leaves = closed_at.check_constraints[0].expr.comparisons();
    assert_eq!(
        leaves[0].rhs,
        CmpRhs::Values(vec![variant("open"), variant("held")])
    );
}

#[test]
fn a_value_of_another_type_is_an_error_that_lists_the_values() {
    assert_eq!(
        error("forbidden(E, status in (open, closed))\n"),
        r#"SetValueNotAValue { col: "status", value: "closed", allowed: "pending, open, paid" }"#
    );
    // A column name is no value either, and a set has no column spelling.
    assert_eq!(
        error("condition C \"C\"\nholds(C, E.status in (paid, paid_at))\n"),
        r#"SetValueNotAValue { col: "status", value: "paid_at", allowed: "pending, open, paid" }"#
    );
    assert_eq!(
        error("forbidden(E, method not in (cash, check))\n"),
        r#"SetValueNotAValue { col: "method", value: "check", allowed: "cash, card" }"#
    );
    assert_eq!(
        error("forbidden(E, paid in (true, maybe))\n"),
        r#"SetValueNotAValue { col: "paid", value: "maybe", allowed: "true, false" }"#
    );
    assert_eq!(
        error("forbidden(E, paid_at in (null, never))\n"),
        r#"SetValueNotAValue { col: "paid_at", value: "never", allowed: "null, present" }"#
    );
    assert_eq!(
        error("forbidden(E, qty in (1, many))\n"),
        r#"SetValueNotAValue { col: "qty", value: "many", allowed: "integer literals" }"#
    );
}

#[test]
fn a_repeated_value_is_an_error() {
    assert_eq!(
        error("forbidden(E, status in (open, paid, open))\n"),
        r#"DuplicateSetValue { col: "status", value: "open" }"#
    );
}

#[test]
fn a_column_without_a_closed_set_of_values_is_an_error() {
    assert_eq!(
        error("forbidden(E, name in (a, b))\n"),
        r#"SetColumnNotEnumerable { col: "name", col_type: "String" }"#
    );
    assert_eq!(
        error("after(U).assert(E.due not in (a))\n"),
        r#"SetColumnNotEnumerable { col: "due", col_type: "Date" }"#
    );
}

#[test]
fn sets_and_drives_reject_a_membership() {
    for (src, pred) in [
        ("sets(U, E, status in (open, paid))\n", "sets"),
        ("sets(U, E, status not in (open))\n", "sets"),
        ("drives(U, E, status in (open), true)\n", "drives"),
        ("drives(U, E, status in (open), false)\n", "drives"),
    ] {
        let (model, diags) = with(src);
        let errs = errors(&diags);
        assert_eq!(errs.len(), 1, "{src}: {errs:?}");
        assert_eq!(
            format!("{:?}", errs[0]),
            format!(r#"EffectMembership {{ pred: "{pred}", col: "status" }}"#),
            "{src}"
        );
        assert!(model.column_effects.is_empty(), "{src}");
        assert!(model.proposition_effects.is_empty(), "{src}");
    }
}
