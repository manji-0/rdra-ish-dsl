use super::support::*;
use crate::analysis::comparison::{resolve_comparison, to_model_op, type_category};
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

#[test]
fn type_category_groups_comparison_compatible_column_types() {
    assert_eq!(type_category(&ColumnType::Int), "numeric");
    assert_eq!(type_category(&ColumnType::Money), "numeric");
    assert_eq!(type_category(&ColumnType::Decimal), "numeric");
    assert_eq!(type_category(&ColumnType::Date), "temporal");
    assert_eq!(type_category(&ColumnType::DateTime), "temporal");
    assert_eq!(type_category(&ColumnType::String), "equality");
    assert_eq!(type_category(&ColumnType::Bool), "equality");
    assert_eq!(
        type_category(&ColumnType::Enum(vec!["open".to_string()])),
        "equality"
    );
}

#[test]
fn to_model_op_maps_every_ast_comparison_operator() {
    assert_eq!(to_model_op(&CmpOp::Lt), CmpOpModel::Lt);
    assert_eq!(to_model_op(&CmpOp::Gt), CmpOpModel::Gt);
    assert_eq!(to_model_op(&CmpOp::Le), CmpOpModel::Le);
    assert_eq!(to_model_op(&CmpOp::Ge), CmpOpModel::Ge);
    assert_eq!(to_model_op(&CmpOp::Eq), CmpOpModel::Eq);
    assert_eq!(to_model_op(&CmpOp::Ne), CmpOpModel::Ne);
}
#[test]
fn resolve_comparison_accepts_same_entity_qualified_columns_and_literals() {
    let cols = vec![
        model_column("stock", ColumnType::Int),
        model_column("selling", ColumnType::Int),
        model_column("expired_at", ColumnType::DateTime),
    ];
    let mut diags = Vec::new();

    let col_prop = resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: qcol("Stock", "stock"),
            op: CmpOp::Lt,
            rhs: qcol("Stock", "selling"),
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .unwrap();
    let int_prop = resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: Operand::Column("stock".to_string()),
            op: CmpOp::Ge,
            rhs: Operand::IntLit("10".to_string()),
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .unwrap();
    let now_prop = resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: Operand::Column("expired_at".to_string()),
            op: CmpOp::Lt,
            rhs: Operand::Now,
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .unwrap();

    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(col_prop.axis_key(), "stock<selling");
    assert_eq!(int_prop.rhs, CmpRhs::IntLit(10));
    assert_eq!(now_prop.rhs, CmpRhs::Now);
}

#[test]
fn resolve_comparison_rejects_cross_entity_and_invalid_type_comparisons() {
    let cols = vec![
        model_column("stock", ColumnType::Int),
        model_column("active", ColumnType::Bool),
        model_column("name", ColumnType::String),
    ];
    let mut diags = Vec::new();

    assert!(resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: qcol("Other", "stock"),
            op: CmpOp::Lt,
            rhs: Operand::Column("stock".to_string()),
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .is_none());
    assert!(resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: Operand::Column("active".to_string()),
            op: CmpOp::Lt,
            rhs: Operand::Column("stock".to_string()),
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .is_none());
    assert!(resolve_comparison(
        &cols,
        "Stock",
        &Comparison {
            lhs: Operand::Column("name".to_string()),
            op: CmpOp::Eq,
            rhs: Operand::IntLit("1".to_string()),
            span: 0..0,
        },
        DiagCtxt::new(0),
        &mut diags,
    )
    .is_none());

    let messages: Vec<_> = diags.iter().map(|d| d.error.to_string()).collect();
    assert!(
        messages.iter().any(|msg| msg.contains("type mismatch")),
        "expected cross-entity type mismatch, got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("order comparison operator")),
        "expected ordered comparison diagnostic, got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("comparison type mismatch")),
        "expected rhs type mismatch diagnostic, got {messages:?}"
    );
}
