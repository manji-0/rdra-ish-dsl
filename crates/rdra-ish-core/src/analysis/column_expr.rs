//! `@check(expr)` and `@derived(expr)`: resolved against the columns of their own entity.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::comparison::{resolve_comparison, type_category};

/// Fills `check_constraints` and `derived_expr` once every column of `inst` exists.
pub(super) fn resolve_column_exprs(
    inst: &InstanceDecl,
    columns: &mut [ModelColumn],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, col) in inst.columns.iter().enumerate() {
        let derived = col
            .annotations
            .iter()
            .any(|a| matches!(a, Annotation::Derived(_)));
        let mut checks = Vec::new();
        let mut derived_expr = None;
        for ann in &col.annotations {
            let refuse = match ann {
                Annotation::Check(_) => Some("@check"),
                Annotation::Default(_) => Some("@default"),
                Annotation::Pk | Annotation::PkComposite(_) => Some("@pk"),
                Annotation::Unique | Annotation::UniqueComposite(_) => Some("@unique"),
                _ => None,
            };
            if let (true, Some(annotation)) = (derived, refuse) {
                push_error(
                    ctx,
                    diags,
                    col.span.clone(),
                    RdraError::DerivedColumnAnnotation {
                        entity: inst.id.clone(),
                        column: col.name.clone(),
                        annotation: annotation.to_string(),
                    },
                );
                continue;
            }
            match ann {
                Annotation::Check(ColumnExpr::Logical(expr)) => {
                    if let Some(expr) = resolve_logical(columns, &inst.id, expr, ctx, diags) {
                        checks.push(ColumnCheck {
                            expr,
                            site: Some(ctx.locate(col.span.clone())),
                        });
                    }
                }
                Annotation::Check(ColumnExpr::Arith(arith)) => push_error(
                    ctx,
                    diags,
                    arith.span.clone(),
                    RdraError::ArithNotInCheck {
                        entity: inst.id.clone(),
                        column: col.name.clone(),
                    },
                ),
                Annotation::Derived(expr) => {
                    derived_expr =
                        resolve_derived(columns, inst, col, &columns[i], expr, ctx, diags);
                }
                _ => {}
            }
        }
        columns[i].check_constraints = checks;
        columns[i].derived_expr = derived_expr;
    }
}

fn resolve_logical(
    columns: &[ModelColumn],
    entity: &str,
    expr: &Expr,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CheckExpr> {
    let mut go = |e: &Expr| resolve_logical(columns, entity, e, ctx, diags);
    Some(match expr {
        Expr::Cmp(cmp) => CheckExpr::Cmp(resolve_comparison(columns, entity, cmp, ctx, diags)?),
        Expr::Not(inner) => CheckExpr::Not(Box::new(go(inner)?)),
        Expr::And(a, b) => {
            let (a, b) = (go(a), go(b));
            CheckExpr::And(Box::new(a?), Box::new(b?))
        }
        Expr::Or(a, b) => {
            let (a, b) = (go(a), go(b));
            CheckExpr::Or(Box::new(a?), Box::new(b?))
        }
    })
}

fn resolve_derived(
    columns: &[ModelColumn],
    inst: &InstanceDecl,
    col: &Column,
    target: &ModelColumn,
    expr: &ColumnExpr,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<DerivedExpr> {
    let (kind, fits) = match expr {
        ColumnExpr::Logical(_) => ("a condition", matches!(target.col_type, ColumnType::Bool)),
        ColumnExpr::Arith(_) => ("arithmetic", type_category(&target.col_type) == "numeric"),
    };
    if !fits {
        push_error(
            ctx,
            diags,
            col.span.clone(),
            RdraError::DerivedTypeMismatch {
                entity: inst.id.clone(),
                column: col.name.clone(),
                kind: kind.to_string(),
                col_type: format!("{:?}", target.col_type),
            },
        );
        return None;
    }
    match expr {
        ColumnExpr::Logical(e) => {
            resolve_logical(columns, &inst.id, e, ctx, diags).map(DerivedExpr::Logical)
        }
        ColumnExpr::Arith(arith) => {
            let mut operand = |o: &Operand| match o {
                Operand::IntLit(s) => match s.parse::<i64>() {
                    Ok(n) => Some(ArithOperand::Int(n)),
                    Err(_) => {
                        push_error(
                            ctx,
                            diags,
                            arith.span.clone(),
                            RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                        );
                        None
                    }
                },
                Operand::Column(name) => {
                    let Some(c) = columns.iter().find(|c| &c.name == name) else {
                        push_error(
                            ctx,
                            diags,
                            arith.span.clone(),
                            RdraError::UnknownColumn {
                                entity: inst.id.clone(),
                                col: name.clone(),
                            },
                        );
                        return None;
                    };
                    if type_category(&c.col_type) != "numeric" {
                        push_error(
                            ctx,
                            diags,
                            arith.span.clone(),
                            RdraError::ArithTypeMismatch {
                                entity: inst.id.clone(),
                                column: col.name.clone(),
                                operand: name.clone(),
                                operand_type: format!("{:?}", c.col_type),
                            },
                        );
                        return None;
                    }
                    Some(ArithOperand::Column(name.clone()))
                }
                // The parser admits only a column or an integer.
                Operand::QualifiedColumn(_) | Operand::Now => None,
            };
            let (lhs, rhs) = (operand(&arith.lhs), operand(&arith.rhs));
            Some(DerivedExpr::Arith(ArithExpr {
                lhs: lhs?,
                op: arith.op,
                rhs: rhs?,
            }))
        }
    }
}

/// A `sets` naming a derived column: its value follows from its expression. `transitions`
/// drive an Enum column, which no `@derived` derives.
pub(super) fn reject_derived_writes(model: &SemanticModel, diags: &mut Vec<Diagnostic>) {
    for ce in &model.column_effects {
        let derived = model.entities[ce.entity]
            .columns
            .iter()
            .any(|c| c.name == ce.column && c.derived_expr.is_some());
        if !derived {
            continue;
        }
        let err = RdraError::DerivedColumnWritten {
            entity: model.entities[ce.entity].id.clone(),
            column: ce.column.clone(),
            usecase: model.use_cases[ce.origin].id.clone(),
        };
        diags.push(match &ce.site {
            Some(site) => Diagnostic::error_at(err, site.clone()),
            None => Diagnostic::error(err),
        });
    }
}
