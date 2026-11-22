//! Temporal `property` declaration lowering for TLA+ export.

use crate::analysis_diag::push_error;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::{
    CmpOpModel, CrossConstraintScope, EntityKey, ModelColumn, NodeRef, QualifiedModelColumnRef,
    SemanticModel, TemporalAtom, TemporalExpr, TemporalFormula, TemporalProperty, TemporalRhs,
    UseCaseKey,
};
use rdra_ish_syntax::ast::{
    AstTemporalFormula, CmpOp, Comparison, Expr, Operand, PredicateArg, PropertyDecl,
};

use super::arg_resolve::resolve_arg;
use super::comparison::{resolve_bare_rhs, resolve_set_values, BareRhs};
use super::constraint::{chain_along_scope, report_leading_by};

/// Why an atom does not lower: a message for `InvalidTemporalProperty`, a column no single
/// entity owns, or a comparison error reported as itself.
enum LowerError {
    Message(String),
    Unresolved(String),
    Comparison(RdraError),
}

impl From<String> for LowerError {
    fn from(message: String) -> Self {
        LowerError::Message(message)
    }
}

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

    let Ok(fair) = fair_use_cases(model, decl, ctx, diags) else {
        return;
    };
    let Some((scope_semantics, global)) = property_scope(model, decl, ctx, diags) else {
        return;
    };
    match lower_formula(model, &decl.formula) {
        Ok(formula) => {
            model.temporal_properties.push(TemporalProperty {
                site: Some(ctx.locate(decl.span.clone())),
                id: decl.id.clone(),
                label: decl.label.clone().unwrap_or_else(|| decl.id.clone()),
                formula,
                fair,
                scope_semantics,
                global,
            });
        }
        Err(LowerError::Comparison(err)) => push_error(ctx, diags, decl.span.clone(), err),
        Err(LowerError::Unresolved(column)) => {
            let err = RdraError::UnresolvedPropertyColumn {
                property: decl.label.clone().unwrap_or_else(|| decl.id.clone()),
                column,
            };
            push_error(ctx, diags, decl.span.clone(), err);
        }
        Err(LowerError::Message(message)) => {
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

/// The use cases of the one `.fair(...)` chain, `Err` after reporting any other chain, a
/// `.fair` on `always`, or a `.fair` that names anything but distinct use cases.
fn fair_use_cases(
    model: &SemanticModel,
    decl: &PropertyDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Vec<UseCaseKey>, ()> {
    let mut fair = Vec::new();
    let mut ok = true;
    let id = || decl.id.clone();
    let mut seen_fair = false;
    for cc in &decl.chain {
        if SCOPE_METHODS.contains(&cc.name.as_str()) {
            continue;
        }
        if cc.name != "fair" {
            let err = RdraError::UnknownChainMethod {
                pred: "property".into(),
                name: cc.name.clone(),
                hint: "a property takes `.fair(UseCase, ...)`, `.along(Entity, ...)`, `.role(role)`, and `.global()`".into(),
            };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        if decl.formula.is_safety() {
            let err = RdraError::FairOnSafetyProperty { id: id() };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        let mut well_formed = !seen_fair && !cc.args.is_empty();
        seen_fair = true;
        for arg in &cc.args {
            match resolve_arg(model, arg, ctx, diags) {
                Some(NodeRef::UseCase(uk)) if !fair.contains(&uk) => fair.push(uk),
                // resolve_arg already reported an undefined or ambiguous reference.
                None if matches!(arg, PredicateArg::Ref(_)) => ok = false,
                _ => well_formed = false,
            }
        }
        if !well_formed {
            let err = RdraError::FairRequiresUseCases { id: id() };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
        }
    }
    if ok {
        Ok(fair)
    } else {
        Err(())
    }
}

/// The chain methods that scope the rows a property speaks about.
const SCOPE_METHODS: [&str; 3] = ["along", "role", "global"];

/// The `.along(..)` scope of a property and whether it says `.global()`; `None` after
/// reporting `.global()` with `.along`, or an argument to `.global()`.
fn property_scope(
    model: &SemanticModel,
    decl: &PropertyDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(CrossConstraintScope, bool)> {
    report_leading_by(&decl.chain, ctx, diags);
    let scope = chain_along_scope(model, "property", &decl.chain, ctx, diags);
    let Some(global) = decl.chain.iter().find(|cc| cc.name == "global") else {
        return Some((scope, false));
    };
    if matches!(scope, CrossConstraintScope::RelationPath { .. }) {
        let err = RdraError::GlobalWithAlong {
            pred: "property".into(),
        };
        push_error(ctx, diags, global.span.clone(), err);
        return None;
    }
    Some((scope, true))
}

fn lower_formula(
    model: &SemanticModel,
    formula: &AstTemporalFormula,
) -> Result<TemporalFormula, LowerError> {
    match formula {
        AstTemporalFormula::Always(e) => Ok(TemporalFormula::Always(lower_expr(model, e)?)),
        AstTemporalFormula::Eventually(e) => Ok(TemporalFormula::Eventually(lower_expr(model, e)?)),
        AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => Ok(TemporalFormula::LeadsTo {
            antecedent: lower_expr(model, antecedent)?,
            consequent: lower_expr(model, consequent)?,
        }),
        AstTemporalFormula::Stable(e) => Ok(TemporalFormula::Stable(lower_expr(model, e)?)),
        AstTemporalFormula::Unchanged { columns, when } => {
            let mut cols: Vec<QualifiedModelColumnRef> = Vec::new();
            for operand in columns {
                let (entity, column) = match operand {
                    Operand::QualifiedColumn(q) => {
                        (q.entity.parts.last().map(String::as_str), q.column.as_str())
                    }
                    Operand::Column(c) => (None, c.as_str()),
                    other => return Err(format!("`unchanged` takes columns, got {other:?}").into()),
                };
                let (ek, _) = owner(model, entity, column)?;
                let col = QualifiedModelColumnRef {
                    entity: ek,
                    column: column.to_string(),
                };
                if !cols.contains(&col) {
                    cols.push(col);
                }
            }
            let when = when.as_ref().map(|e| lower_expr(model, e)).transpose()?;
            Ok(TemporalFormula::Unchanged {
                columns: cols,
                when,
            })
        }
    }
}

fn lower_expr(model: &SemanticModel, expr: &Expr) -> Result<TemporalExpr, LowerError> {
    let go = |e: &Expr| lower_expr(model, e).map(Box::new);
    match expr {
        Expr::Cmp(cmp) => Ok(TemporalExpr::Atom(lower_atom(model, cmp)?)),
        Expr::Not(inner) => Ok(TemporalExpr::Not(go(inner)?)),
        Expr::And(a, b) => Ok(TemporalExpr::And(go(a)?, go(b)?)),
        Expr::Or(a, b) => Ok(TemporalExpr::Or(go(a)?, go(b)?)),
    }
}

/// The entity that owns `column`: `entity` when written, else the one entity with a column of
/// that name. The column must be declared on it.
fn owner<'m>(
    model: &'m SemanticModel,
    entity: Option<&str>,
    column: &str,
) -> Result<(EntityKey, &'m ModelColumn), LowerError> {
    model
        .property_entity(entity, column)
        .and_then(|ek| {
            let col = model.entities[ek]
                .columns
                .iter()
                .find(|c| c.name == column)?;
            Some((ek, col))
        })
        .ok_or_else(|| LowerError::Unresolved(column.to_string()))
}

fn lower_atom(model: &SemanticModel, cmp: &Comparison) -> Result<TemporalAtom, LowerError> {
    let op = match cmp.op {
        CmpOp::Eq => CmpOpModel::Eq,
        CmpOp::Ne => CmpOpModel::Ne,
        CmpOp::Lt => CmpOpModel::Lt,
        CmpOp::Gt => CmpOpModel::Gt,
        CmpOp::Le => CmpOpModel::Le,
        CmpOp::Ge => CmpOpModel::Ge,
        CmpOp::In => CmpOpModel::In,
        CmpOp::NotIn => CmpOpModel::NotIn,
    };

    let (written, column) = match &cmp.lhs {
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
        other => {
            return Err(format!("temporal atom lhs must be a column, got {other:?}").into());
        }
    };
    let (entity, lhs) = owner(model, written.as_deref(), &column)?;

    let rhs = match &cmp.rhs {
        Operand::IntLit(n) => {
            let parsed = n
                .parse::<i64>()
                .map_err(|_| format!("invalid integer literal `{n}`"))?;
            TemporalRhs::IntLit(parsed)
        }
        Operand::QualifiedColumn(q) => {
            let (rk, _) = owner(model, q.entity.parts.last().map(String::as_str), &q.column)?;
            TemporalRhs::Column {
                entity: rk,
                column: q.column.clone(),
            }
        }
        Operand::Column(name) if matches!(op, CmpOpModel::Eq | CmpOpModel::Ne) => {
            let ent = &model.entities[entity];
            let qualified = |n: &str| {
                ent.columns
                    .iter()
                    .any(|c| c.name == n)
                    .then(|| format!("{}.{n}", ent.id))
            };
            match resolve_bare_rhs(&model.variations, lhs, &cmp.op, name, qualified) {
                Ok(BareRhs::Value(value)) => TemporalRhs::Value(value),
                Ok(BareRhs::Column) => TemporalRhs::Column {
                    entity,
                    column: name.clone(),
                },
                Err(err) => return Err(LowerError::Comparison(err)),
            }
        }
        // An order comparison: another column, of the written entity when there is one.
        Operand::Column(name) => {
            let (rk, _) = owner(model, written.as_deref(), name)?;
            TemporalRhs::Column {
                entity: rk,
                column: name.clone(),
            }
        }
        Operand::Now => {
            return Err(LowerError::Message(
                "`now` is not supported in temporal property atoms".into(),
            ));
        }
        Operand::Values(names) => TemporalRhs::Values(
            resolve_set_values(&model.variations, lhs, names).map_err(LowerError::Comparison)?,
        ),
    };

    // Relational ops require numeric-ish rhs (IntLit or Column), not enum literals.
    if !matches!(op, CmpOpModel::Eq | CmpOpModel::Ne) && matches!(rhs, TemporalRhs::Value(_)) {
        return Err(format!(
            "temporal comparison `{}` requires an Int literal or column on the rhs",
            op.as_str()
        )
        .into());
    }

    Ok(TemporalAtom {
        entity,
        column,
        op,
        rhs,
    })
}
