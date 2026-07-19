//! Temporal `property` declaration lowering for TLA+ export.

use crate::analysis_diag::push_error;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::{
    CmpOpModel, EffectValue, TemporalAtom, TemporalExpr, TemporalFormula, TemporalProperty,
    TemporalRhs,
};
use rdra_ish_syntax::ast::{AstTemporalFormula, CmpOp, Comparison, Expr, Operand, PropertyDecl};

pub(crate) fn register_property(
    model: &mut crate::model::SemanticModel,
    decl: &PropertyDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    if model.temporal_properties.iter().any(|p| p.id == decl.id) {
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::DuplicateProperty {
                id: decl.id.clone(),
            },
        );
        return;
    }

    match lower_formula(&decl.formula) {
        Ok(formula) => {
            model.temporal_properties.push(TemporalProperty {
                id: decl.id.clone(),
                label: decl.label.clone().unwrap_or_else(|| decl.id.clone()),
                formula,
            });
        }
        Err(message) => {
            push_error(
                ctx,
                diags,
                decl.span.clone(),
                RdraError::InvalidTemporalProperty {
                    id: decl.id.clone(),
                    message,
                },
            );
        }
    }
}

fn lower_formula(formula: &AstTemporalFormula) -> Result<TemporalFormula, String> {
    match formula {
        AstTemporalFormula::Always(e) => Ok(TemporalFormula::Always(lower_expr(e)?)),
        AstTemporalFormula::Eventually(e) => Ok(TemporalFormula::Eventually(lower_expr(e)?)),
        AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => Ok(TemporalFormula::LeadsTo {
            antecedent: lower_expr(antecedent)?,
            consequent: lower_expr(consequent)?,
        }),
    }
}

fn lower_expr(expr: &Expr) -> Result<TemporalExpr, String> {
    match expr {
        Expr::Cmp(cmp) => Ok(TemporalExpr::Atom(lower_atom(cmp)?)),
        Expr::Not(inner) => Ok(TemporalExpr::Not(Box::new(lower_expr(inner)?))),
        Expr::And(a, b) => Ok(TemporalExpr::And(
            Box::new(lower_expr(a)?),
            Box::new(lower_expr(b)?),
        )),
        Expr::Or(a, b) => Ok(TemporalExpr::Or(
            Box::new(lower_expr(a)?),
            Box::new(lower_expr(b)?),
        )),
    }
}

fn lower_atom(cmp: &Comparison) -> Result<TemporalAtom, String> {
    let op = match cmp.op {
        CmpOp::Eq => CmpOpModel::Eq,
        CmpOp::Ne => CmpOpModel::Ne,
        CmpOp::Lt => CmpOpModel::Lt,
        CmpOp::Gt => CmpOpModel::Gt,
        CmpOp::Le => CmpOpModel::Le,
        CmpOp::Ge => CmpOpModel::Ge,
    };

    let (entity, column) = match &cmp.lhs {
        Operand::QualifiedColumn(q) => {
            let entity = q
                .entity
                .parts
                .last()
                .cloned()
                .ok_or_else(|| "empty entity qualifier".to_string())?;
            (Some(entity), q.column.clone())
        }
        Operand::Column(c) => (None, c.clone()),
        other => return Err(format!("temporal atom lhs must be a column, got {other:?}")),
    };

    let rhs = match &cmp.rhs {
        Operand::IntLit(n) => {
            let parsed = n
                .parse::<i64>()
                .map_err(|_| format!("invalid integer literal `{n}`"))?;
            TemporalRhs::IntLit(parsed)
        }
        Operand::QualifiedColumn(q) => {
            let ent = q.entity.parts.last().cloned();
            TemporalRhs::Column {
                entity: ent,
                column: q.column.clone(),
            }
        }
        Operand::Column(name) => {
            // Bare idents that look like enum/bool/null literals stay as Value;
            // otherwise treat as same-entity column reference for arithmetic.
            match name.as_str() {
                "true" | "false" | "null" | "present" => {
                    TemporalRhs::Value(parse_effect_literal(name))
                }
                other if other.chars().all(|c| c.is_ascii_digit() || c == '-') => {
                    let parsed = other
                        .parse::<i64>()
                        .map_err(|_| format!("invalid integer literal `{other}`"))?;
                    TemporalRhs::IntLit(parsed)
                }
                other if matches!(op, CmpOpModel::Eq | CmpOpModel::Ne) => {
                    // Heuristic: ==/!= with bare ident → enum/bool literal.
                    TemporalRhs::Value(parse_effect_literal(other))
                }
                other => TemporalRhs::Column {
                    entity: None,
                    column: other.to_string(),
                },
            }
        }
        Operand::Now => {
            return Err("`now` is not supported in temporal property atoms".into());
        }
    };

    // Relational ops require numeric-ish rhs (IntLit or Column), not enum literals.
    if !matches!(op, CmpOpModel::Eq | CmpOpModel::Ne) && matches!(rhs, TemporalRhs::Value(_)) {
        return Err(format!(
            "temporal comparison `{}` requires an Int literal or column on the rhs",
            op.as_str()
        ));
    }

    Ok(TemporalAtom {
        entity,
        column,
        op,
        rhs,
    })
}

fn parse_effect_literal(name: &str) -> EffectValue {
    match name {
        "true" => EffectValue::Bool(true),
        "false" => EffectValue::Bool(false),
        "null" => EffectValue::Null,
        "present" => EffectValue::Present,
        other => EffectValue::EnumVariant(other.to_string()),
    }
}
