use super::support::*;
use crate::analysis::build_model;
use crate::analysis::constraint::{
    add_condition_entities_to_scope, cross_scope_semantics_from_chain, push_unique_entity,
};
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;
use rdra_ish_syntax::parse;

#[test]
fn push_unique_entity_preserves_first_seen_scope_order() {
    let model = simple_entity_model(&["Order", "Payment"]);
    let order = entity_key(&model, "Order");
    let payment = entity_key(&model, "Payment");
    let mut scope = Vec::new();

    push_unique_entity(&mut scope, order);
    push_unique_entity(&mut scope, payment);
    push_unique_entity(&mut scope, order);

    assert_eq!(scope, vec![order, payment]);
}

#[test]
fn add_condition_entities_to_scope_adds_equals_and_comparison_entities_once() {
    let model = simple_entity_model(&["Order", "Payment", "Invoice"]);
    let order = entity_key(&model, "Order");
    let payment = entity_key(&model, "Payment");
    let invoice = entity_key(&model, "Invoice");
    let conditions = vec![
        CrossEntityCondition::Equals {
            column: QualifiedModelColumnRef {
                entity: order,
                column: "status".to_string(),
            },
            value: EffectValue::EnumVariant("closed".to_string()),
        },
        CrossEntityCondition::Comparison(CrossComparisonProp {
            lhs: QualifiedModelColumnRef {
                entity: payment,
                column: "amount".to_string(),
            },
            op: CmpOpModel::Gt,
            rhs: CrossCmpRhs::Column(QualifiedModelColumnRef {
                entity: invoice,
                column: "amount".to_string(),
            }),
        }),
        CrossEntityCondition::Comparison(CrossComparisonProp {
            lhs: QualifiedModelColumnRef {
                entity: order,
                column: "amount".to_string(),
            },
            op: CmpOpModel::Ge,
            rhs: CrossCmpRhs::IntLit(1),
        }),
    ];
    let mut scope = vec![order];

    add_condition_entities_to_scope(&mut scope, &conditions);

    assert_eq!(scope, vec![order, payment, invoice]);
}

#[test]
fn cross_scope_semantics_from_chain_returns_relation_path_for_along_chain() {
    let model = simple_entity_model(&["Order", "Payment"]);
    let mut diags = Vec::new();
    let pred = PredicateCall {
        name: "cross_invariant".to_string(),
        args: Vec::new(),
        chain: vec![ChainCall {
            name: "along".to_string(),
            args: vec![
                PredicateArg::Ref(qref("Order")),
                PredicateArg::Ref(qref("Payment")),
            ],
            span: 0..0,
        }],
        span: 0..0,
    };

    let semantics = cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let CrossConstraintScope::RelationPath(path) = semantics else {
        panic!("expected relation path scope");
    };
    assert_eq!(
        path,
        vec![entity_key(&model, "Order"), entity_key(&model, "Payment")]
    );
}

#[test]
fn cross_scope_semantics_from_chain_defaults_to_global_product_without_along() {
    let model = simple_entity_model(&["Order"]);
    let mut diags = Vec::new();
    let pred = PredicateCall {
        name: "cross_forbidden".to_string(),
        args: Vec::new(),
        chain: Vec::new(),
        span: 0..0,
    };

    let semantics = cross_scope_semantics_from_chain(&model, &pred, DiagCtxt::new(0), &mut diags);

    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert!(matches!(semantics, CrossConstraintScope::GlobalProduct));
}
#[test]
fn after_assert_registers_temporal_assertion_from_equality_expr() {
    let (ast, parse_errors) = parse(
        r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" {
  id: Int @pk
  status: Enum(requested, executed) @default(requested)
}
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
    );
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    assert_eq!(model.temporal_assertions.len(), 1);
    let assertion = &model.temporal_assertions[0];
    assert_eq!(model.use_cases[assertion.anchor].id, "ExecuteCertIssue");
    assert_eq!(assertion.requireds.len(), 1);
}

#[test]
fn forbidden_when_none_registers_quantifier_constraint() {
    let (ast, parse_errors) = parse(
        r#"
entity ClientCertificate "Client Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity TerminalCertAssignment "Terminal Cert Assignment" {
  id: Int @pk
  status: Enum(active, inactive) @default(active)
}
when(ClientCertificate.status == revoked)
  .none(TerminalCertAssignment.status == active)
"#,
    );
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    assert_eq!(model.quantifier_constraints.len(), 1);
    let constraint = &model.quantifier_constraints[0];
    assert_eq!(model.entities[constraint.anchor].id, "ClientCertificate");
    assert_eq!(
        model.entities[constraint.related].id,
        "TerminalCertAssignment"
    );
    assert_eq!(constraint.guards.len(), 1);
    assert_eq!(constraint.related_conditions.len(), 1);
}

#[test]
fn test_sets_comparison_registers_proposition_effect() {
    let src = r#"
usecase Sell "販売する"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
}
updates(Sell, Stock)
sets(Sell, Stock, stock < selling, true)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.proposition_effects.len(), 1);
    let effect = &model.proposition_effects[0];
    assert_eq!(effect.prop.axis_key(), "stock<selling");
    assert!(effect.truth);
    assert!(matches!(effect.origin, NodeRef::UseCase(_)));
}

#[test]
fn test_required_registers_conditions_and_comparison() {
    let src = r#"
entity Coupon "クーポン" {
  id: Int @pk
  status: Enum(usable, expired)
  expired_at: DateTime @null
}
required(Coupon, status == usable, expired_at < now)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.required_constraints.len(), 1);
    let constraint = &model.required_constraints[0];
    assert_eq!(constraint.conditions.len(), 1);
    assert_eq!(constraint.comparisons.len(), 1);
    assert_eq!(constraint.comparisons[0].axis_key(), "expired_at<now");
}

#[test]
fn test_exclusive_registers_flat_pair_conditions() {
    let src = r#"
entity Document "文書" {
  id: Int @pk
  approved: Bool
  rejected: Bool
}
exclusive(Document, approved == true, rejected == true)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.exclusive_constraints.len(), 1);
    let constraint = &model.exclusive_constraints[0];
    assert_eq!(constraint.conditions.len(), 2);
    assert_eq!(constraint.comparisons.len(), 0);
}

#[test]
fn test_cross_forbidden_registers_qualified_conditions() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, cancelled)
  total: Decimal
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
  amount: Decimal
}
forbidden(Order, Payment, Order.status == cancelled, Payment.amount > Order.total)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.cross_forbidden_constraints.len(), 1);
    let constraint = &model.cross_forbidden_constraints[0];
    assert_eq!(constraint.scope.len(), 2);
    assert_eq!(constraint.conditions.len(), 2);

    let order_key = model
        .entities
        .iter()
        .find_map(|(key, entity)| (entity.id == "Order").then_some(key))
        .unwrap();
    let payment_key = model
        .entities
        .iter()
        .find_map(|(key, entity)| (entity.id == "Payment").then_some(key))
        .unwrap();

    assert!(matches!(
        &constraint.conditions[0],
        CrossEntityCondition::Equals { column, value }
            if column.entity == order_key
                && column.column == "status"
                && value == &EffectValue::EnumVariant("cancelled".to_string())
    ));
    assert!(matches!(
        &constraint.conditions[1],
        CrossEntityCondition::Comparison(CrossComparisonProp {
            lhs,
            op: CmpOpModel::Gt,
            rhs: CrossCmpRhs::Column(rhs),
        }) if lhs.entity == payment_key
            && lhs.column == "amount"
            && rhs.entity == order_key
            && rhs.column == "total"
    ));
}

#[test]
fn test_cross_invariant_registers_when_then_conditions() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
invariant(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.cross_entity_invariants.len(), 1);
    let invariant = &model.cross_entity_invariants[0];
    assert_eq!(invariant.scope.len(), 2);
    assert_eq!(invariant.guards.len(), 1);
    assert_eq!(invariant.requireds.len(), 1);
}

#[test]
fn test_cross_invariant_registers_along_scope() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
relate(Payment, Order, 1:1)
invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let invariant = &model.cross_entity_invariants[0];
    let CrossConstraintScope::RelationPath(path) = &invariant.scope_semantics else {
        panic!(
            "expected relation-path scope, got {:?}",
            invariant.scope_semantics
        );
    };
    let path_ids: Vec<_> = path
        .iter()
        .map(|key| model.entities[*key].id.as_str())
        .collect();
    assert_eq!(path_ids, vec!["Order", "Payment"]);
}

#[test]
fn test_cross_invariant_can_infer_scope_from_qualified_columns() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
invariant()
  .when(Order.status == paid)
  .then(Payment.status == captured)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let invariant = &model.cross_entity_invariants[0];
    let scope_ids: Vec<_> = invariant
        .scope
        .iter()
        .map(|key| model.entities[*key].id.as_str())
        .collect();
    assert_eq!(scope_ids, vec!["Order", "Payment"]);
}

#[test]
fn test_cross_invariant_requires_column_qualifier_for_multi_entity_scope() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured)
}
invariant(Order, Payment)
  .when(status == paid)
  .then(Payment.status == captured)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors
            .iter()
            .any(|d| d.error.to_string().contains("needs an entity qualifier")),
        "expected qualifier diagnostic, got: {:?}",
        errors
    );
}
