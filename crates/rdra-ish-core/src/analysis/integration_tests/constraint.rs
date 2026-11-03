use super::support::*;
use crate::analysis::build_model;
use crate::analysis::constraint::{
    add_condition_entities_to_scope, cross_scope_semantics_from_chain, push_unique_entity,
};
use crate::diagnostics::RdraError;
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
    let CrossConstraintScope::RelationPath { path, .. } = semantics else {
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
fn when_entity_prefix_bare_column_registers_quantifier() {
    let (ast, parse_errors) = parse(
        r#"
entity Cert "証明書" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity Assign "割当" {
  id: Int @pk
  status: Enum(active, ended) @default(active)
}
invariant(Cert).when(status == revoked).none(Assign.status == active)
"#,
    );
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(model.quantifier_constraints.len(), 1);
    let constraint = &model.quantifier_constraints[0];
    assert_eq!(model.entities[constraint.anchor].id, "Cert");
    assert_eq!(model.entities[constraint.quantifier.related].id, "Assign");
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
invariant(ClientCertificate).when(ClientCertificate.status == revoked)
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
        model.entities[constraint.quantifier.related].id,
        "TerminalCertAssignment"
    );
    assert_eq!(constraint.guards.len(), 1);
    assert_eq!(constraint.quantifier.conditions.len(), 1);
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
drives(Sell, Stock.stock < selling, true)
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
    assert_eq!(model.use_cases[effect.origin].id, "Sell");
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
    let CrossConstraintScope::RelationPath { path, .. } = &invariant.scope_semantics else {
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
fn test_cross_invariant_ranges_over_its_leading_entities() {
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

#[test]
fn a_bad_equality_value_in_a_cross_entity_constraint_is_one_error() {
    let src = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
}
entity Payment "Payment" {
  id: Int @pk
  amount: Int
}
forbidden(Order, Payment, Order.status == gone, Payment.amount > 1)
forbidden(Order, Payment, Order.nope == 1, Payment.amount > 1)
"#;
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "{errors:?}");
    let (_, diags) = build_model(&ast);
    let errs: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert_eq!(errs.len(), 2, "one error per bad condition: {errs:?}");
}

const LABEL_BASE: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid, cancelled)  paid_at: DateTime @null  total: Int }
entity Payment "支払い" { id: Int @pk  status: Enum(pending, captured)  amount: Int }
entity Line "明細" { id: Int @pk  state: Enum(a, b) }
relate(Line, Order, N:1)
relate(Payment, Order, N:1)
usecase Pay "支払"
"#;

fn build_labelled(extra: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    let (ast, parse_errors) = parse(&format!("{LABEL_BASE}{extra}"));
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    build_model(&ast)
}

#[test]
fn a_label_lands_on_every_kind_of_rule() {
    let (model, diags) = build_labelled(
        r#"
forbidden(Order, status == cancelled, paid_at == present).label("取消後は入金しない")
invariant(Order).when(status == paid).then(paid_at == present).label("入金済には入金日時")
required(Order, total >= 0).label("合計は負でない")
exclusive(Order, status == open, status == paid).label("開と入金済は両立しない")
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).label("取消注文は支払えない")
invariant(Order, Payment).when(Payment.status == captured).then(Order.status == paid).label("支払済なら入金済")
invariant(Order).when(status == cancelled).none(Payment, status == captured).label("取消注文に支払なし")
after(Pay).assert(Order.status == paid).label("支払後は入金済")
"#,
    );
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let label = |l: &Option<String>| l.clone().unwrap();
    assert_eq!(
        label(&model.forbidden_constraints[0].label),
        "取消後は入金しない"
    );
    assert_eq!(
        label(&model.entity_invariants[0].label),
        "入金済には入金日時"
    );
    assert_eq!(
        label(&model.required_constraints[0].label),
        "合計は負でない"
    );
    assert_eq!(
        label(&model.exclusive_constraints[0].label),
        "開と入金済は両立しない"
    );
    assert_eq!(
        label(&model.cross_forbidden_constraints[0].label),
        "取消注文は支払えない"
    );
    assert_eq!(
        label(&model.cross_entity_invariants[0].label),
        "支払済なら入金済"
    );
    assert_eq!(
        label(&model.quantifier_constraints[0].label),
        "取消注文に支払なし"
    );
    assert_eq!(label(&model.temporal_assertions[0].label), "支払後は入金済");
}

#[test]
fn a_rule_without_label_has_none() {
    let (model, diags) = build_labelled("forbidden(Order, status == cancelled)\n");
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    assert_eq!(model.forbidden_constraints[0].label, None);
}

#[test]
fn a_label_that_is_not_one_string_is_an_error() {
    for bad in [".label(foo)", ".label(\"a\", \"b\")", ".label()"] {
        let (model, diags) =
            build_labelled(&format!("forbidden(Order, status == cancelled){bad}\n"));
        assert!(
            diags.iter().any(
                |d| matches!(&d.error, RdraError::RuleLabelArg { pred } if pred == "forbidden")
            ),
            "{bad}: {diags:?}"
        );
        assert_eq!(model.forbidden_constraints[0].label, None, "{bad}");
    }
}

#[test]
fn a_second_label_on_one_rule_is_an_error() {
    let (_model, diags) = build_labelled(
        "invariant(Order).when(status == paid).then(paid_at == present).label(\"a\").label(\"b\")\n",
    );
    assert!(
        diags.iter().any(
            |d| matches!(&d.error, RdraError::DuplicateRuleLabel { pred } if pred == "invariant")
        ),
        "{diags:?}"
    );
}

#[test]
fn label_is_not_a_chain_method_of_other_predicates() {
    let (_model, diags) = build_labelled("updates(Pay, Order).label(\"x\")\n");
    assert!(
        diags.iter().any(
            |d| matches!(&d.error, RdraError::UnknownChainMethod { pred, name, .. } if pred == "updates" && name == "label")
        ),
        "{diags:?}"
    );
}

const LIBRARY: &str = r#"
entity Member "会員" { id: Int @pk  status: Enum(active, withdrawn) @default(active) }
entity Card "カード" { id: Int @pk }
entity Loan "貸出" { id: Int @pk  status: Enum(open, returned) @default(open) }
entity Fine "延滞金" { id: Int @pk }
relate(Card, Member, N:1)
relate(Loan, Card, N:1)
relate(Fine, Loan, N:1)
"#;

fn library_rule(rule: &str) -> (SemanticModel, Vec<RdraError>) {
    let (ast, parse_errors) = parse(&format!("{LIBRARY}{rule}\n"));
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .into_iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error)
        .collect();
    (model, errors)
}

/// The `.along` of a `.has`/`.none` invariant is kept on the rule with its hops' keys (#90), and
/// the rule table prints it.
#[test]
fn a_quantified_invariant_keeps_its_path() {
    let (model, errors) = library_rule(
        "invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Card, Loan)",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let [constraint] = model.quantifier_constraints.as_slice() else {
        panic!("{:?}", model.quantifier_constraints);
    };
    let CrossConstraintScope::RelationPath { path, links, .. } = &constraint.along else {
        panic!("{:?}", constraint.along);
    };
    let ids: Vec<&str> = path
        .iter()
        .map(|e| model.entities[*e].id.as_str())
        .collect();
    assert_eq!(ids, ["Member", "Card", "Loan"]);
    assert!(links.iter().all(Option::is_some), "{links:?}");
    let rules = crate::rules::business_rules(&model);
    assert_eq!(
        rules[0].text,
        "when Member.status == withdrawn then none Loan(Loan.status == open).along(Member, Card, Loan)"
    );
}

/// A path that does not run from the anchor to the counted entity, each entity once, is
/// `QuantifierAlongShape`, rewritten from the anchor to the counted entity through the other
/// path entities in order.
#[test]
fn a_quantifier_path_off_the_anchor_or_the_counted_entity_is_an_error_with_the_rewrite() {
    for (path, fix) in [
        ("Loan, Card, Member", "Member, Card, Loan"),
        ("Card, Loan", "Member, Card, Loan"),
        ("Member, Card, Loan, Fine", "Member, Card, Loan"),
        ("Member, Card, Card, Loan", "Member, Card, Loan"),
        ("Loan", "Member, Card, Loan"),
    ] {
        let (model, errors) = library_rule(&format!(
            "invariant(Member).when(status == withdrawn).none(Loan.status == open).along({path})"
        ));
        assert!(
            matches!(
                errors.as_slice(),
                [RdraError::QuantifierAlongShape { method, ends, path: p, fix: f }]
                    if method == "none" && ends == "Member to Loan" && p == path && f == fix
            ),
            "{path}: {errors:?}"
        );
        assert!(model.quantifier_constraints.is_empty(), "{path}");
    }
}

/// The rule table of a model, as `list --kind rule` prints it: kind, entities, text.
fn rule_rows(src: &str) -> Vec<String> {
    let (model, diags) = build_labelled(src);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "{errors:?}");
    crate::rules::business_rules(&model)
        .into_iter()
        .filter(|r| r.kind != "condition")
        .map(|r| format!("{} {}: {}", r.kind, r.entities.join(","), r.text))
        .collect()
}

/// `.when(Cond)` on every kind of rule reads the condition's body as if its comparisons were
/// written in place (#89): the rule table of the condition form is the inline form's, and the
/// `holds` may come after the rule.
#[test]
fn a_rule_when_reads_the_conditions_body_as_its_own_comparisons() {
    const CONDITIONS: &str = r#"
condition Paid "paid"
holds(Paid, Order.status == paid)
condition Captured "captured"
holds(Captured, Payment.status == captured, Payment.amount > 0)
"#;
    let with_conditions = rule_rows(&format!(
        r#"
forbidden(Order, paid_at == null).when(Paid)
forbidden(Order).when(Paid)
invariant(Order).when(Paid).then(paid_at == present)
invariant(Order).when(Paid).when(total > 0).then(paid_at == present)
forbidden(Order, Payment, Order.status == cancelled).when(Captured)
invariant(Order, Payment).along(Order, Payment).when(Captured).then(Order.status == paid)
invariant(Order).when(Paid).none(Payment.status == pending)
invariant(Order).when(Paid).then(paid_at == present).none(Payment.status == pending)
{CONDITIONS}"#
    ));
    let inline = rule_rows(
        r#"
forbidden(Order, paid_at == null, status == paid)
forbidden(Order, status == paid)
invariant(Order).when(status == paid).then(paid_at == present)
invariant(Order).when(total > 0).when(status == paid).then(paid_at == present)
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured, Payment.amount > 0)
invariant(Order, Payment).along(Order, Payment).when(Payment.status == captured, Payment.amount > 0).then(Order.status == paid)
invariant(Order).when(status == paid).none(Payment.status == pending)
invariant(Order).when(status == paid).then(paid_at == present).none(Payment.status == pending)
"#,
    );
    assert_eq!(with_conditions, inline);
    assert!(
        with_conditions[0].contains("status == paid"),
        "{with_conditions:?}"
    );
}

#[test]
fn a_rule_when_rejects_what_it_cannot_read() {
    let errors = |extra: &str| -> Vec<RdraError> {
        let (_, diags) = build_labelled(extra);
        diags
            .into_iter()
            .filter(|d| !d.is_warning)
            .map(|d| d.error)
            .collect()
    };
    // No body.
    let errs =
        errors("condition Paid \"paid\"\ninvariant(Order).when(Paid).then(paid_at == present)\n");
    assert!(
        matches!(errs.as_slice(), [RdraError::WhenConditionWithoutBody { pred, subject, condition }]
            if pred == "invariant" && subject == "Order" && condition == "Paid"),
        "{errs:?}"
    );
    // A quantifier in the body.
    let errs = errors(
        "condition Lone \"lone\"\nholds(Lone, Order.status == paid).none(Payment)\nforbidden(Order, paid_at == null).when(Lone)\n",
    );
    assert!(
        matches!(errs.as_slice(), [RdraError::RuleWhenConditionScope { pred, condition, .. }]
            if pred == "forbidden" && condition == "Lone"),
        "{errs:?}"
    );
    // Another entity on an entity-local rule.
    let errs = errors(
        "condition Captured \"captured\"\nholds(Captured, Payment.status == captured)\nforbidden(Order, status == cancelled).when(Captured)\n",
    );
    assert!(
        matches!(errs.as_slice(), [RdraError::RuleWhenConditionScope { reason, fix, .. }]
            if reason.contains("compares Payment") && fix.contains("forbidden(Order, Payment, ...)")),
        "{errs:?}"
    );
    // Not a condition.
    let errs = errors("invariant(Order).when(Payment).then(paid_at == present)\n");
    assert!(
        matches!(errs.as_slice(), [RdraError::RuleWhenArg { pred, got, .. }]
            if pred == "invariant" && got == "Payment"),
        "{errs:?}"
    );
    let errs = errors("forbidden(Order, paid_at == null).when(status == paid)\n");
    assert!(
        matches!(errs.as_slice(), [RdraError::RuleWhenArg { pred, .. }] if pred == "forbidden"),
        "{errs:?}"
    );
    // The condition is still the rule's once its `holds` is read: no InvariantWithoutWhen.
    let errs = errors("invariant(Order).when(Nope).then(paid_at == present)\n");
    assert!(
        matches!(errs.as_slice(), [RdraError::UndefinedSymbol { .. }]),
        "{errs:?}"
    );
}
