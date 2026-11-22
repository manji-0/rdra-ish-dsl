//! Temporal `property` declaration lowering for TLA+ export.

use crate::analysis_diag::push_error;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::{
    CmpOpModel, EntityKey, ModelColumn, NodeRef, QualifiedModelColumnRef, SemanticModel,
    TemporalAtom, TemporalExpr, TemporalFormula, TemporalProperty, TemporalRhs, UseCaseKey,
};
use rdra_ish_syntax::ast::{
    AstTemporalFormula, CmpOp, Comparison, Expr, Operand, PredicateArg, PropertyDecl,
};

use super::arg_resolve::resolve_arg;
use super::comparison::{resolve_bare_rhs, resolve_set_values, BareRhs};
use super::constraint::{cross_scope_semantics_from_chain, global_chain};

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
    let scope_semantics =
        cross_scope_semantics_from_chain(model, "property", &decl.chain, ctx, diags);
    let global = global_chain("property", &decl.chain, &scope_semantics, ctx, diags);
    match lower_formula(model, &decl.formula) {
        Ok(formula) => {
            let mut scope = Vec::new();
            formula_entities(&formula, &mut scope);
            model.temporal_properties.push(TemporalProperty {
                site: Some(ctx.locate(decl.span.clone())),
                id: decl.id.clone(),
                label: decl.label.clone().unwrap_or_else(|| decl.id.clone()),
                formula,
                scope,
                scope_semantics,
                global,
                fair,
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

/// The use cases of the one `.fair(...)` chain, `Err` after reporting a chain method a
/// property does not take, a `.fair` on `always`, or a `.fair` that names anything but
/// distinct use cases. The scope reads `.along`, `.role` and `.global`.
fn fair_use_cases(
    model: &SemanticModel,
    decl: &PropertyDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Vec<UseCaseKey>, ()> {
    let mut fair = Vec::new();
    let mut ok = true;
    let id = || decl.id.clone();
    for (index, cc) in decl.chain.iter().enumerate() {
        if matches!(cc.name.as_str(), "along" | "role" | "global") {
            continue;
        }
        if cc.name != "fair" {
            let err = RdraError::UnknownChainMethod {
                pred: "property".into(),
                name: cc.name.clone(),
                hint: "a property takes `.fair(UseCase, ...)`, `.along(A, B, ..)` with `.role(role)`, and `.global()`; `.when(p)` follows `unchanged(..)` directly".into(),
            };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        let form = match &decl.formula {
            AstTemporalFormula::Always(_) => Some("always"),
            AstTemporalFormula::Stable(_) => Some("stable"),
            AstTemporalFormula::Unchanged { .. } => Some("unchanged"),
            AstTemporalFormula::Precedes { .. } => Some("precedes"),
            AstTemporalFormula::Eventually(_)
            | AstTemporalFormula::LeadsTo { .. }
            | AstTemporalFormula::CanReach(_) => None,
        };
        if let Some(form) = form {
            let err = RdraError::FairOnSafetyProperty {
                id: id(),
                form: form.into(),
            };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        if matches!(decl.formula, AstTemporalFormula::CanReach(_)) {
            let err = RdraError::FairOnReachProperty { id: id() };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        let first = !decl.chain[..index].iter().any(|c| c.name == "fair");
        let mut well_formed = first && !cc.args.is_empty();
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

fn lower_formula(
    model: &SemanticModel,
    formula: &AstTemporalFormula,
) -> Result<TemporalFormula, LowerError> {
    match formula {
        AstTemporalFormula::Always(e) => Ok(TemporalFormula::Always(lower_expr(model, e)?)),
        AstTemporalFormula::Eventually(e) => Ok(TemporalFormula::Eventually(lower_expr(model, e)?)),
        AstTemporalFormula::CanReach(e) => Ok(TemporalFormula::CanReach(lower_expr(model, e)?)),
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
                let (written, column) = match operand {
                    Operand::QualifiedColumn(q) => (q.entity.parts.last().cloned(), &q.column),
                    Operand::Column(c) => (None, c),
                    other => {
                        return Err(format!("unchanged(..) takes columns, got {other:?}").into());
                    }
                };
                let (entity, decl) = owner(model, written.as_deref(), column)?;
                if decl.derived_expr.is_some() {
                    return Err(format!(
                        "unchanged(..) keeps stored columns, and {}.{column} is @derived; name the columns its expression reads",
                        model.entities[entity].id
                    )
                    .into());
                }
                let col = QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                };
                if !cols.contains(&col) {
                    cols.push(col);
                }
            }
            Ok(TemporalFormula::Unchanged {
                columns: cols,
                when: when.as_ref().map(|e| lower_expr(model, e)).transpose()?,
            })
        }
        AstTemporalFormula::Precedes { first, then } => Ok(TemporalFormula::Precedes {
            first: lower_expr(model, first)?,
            then: lower_expr(model, then)?,
        }),
    }
}

/// The entities `formula` reads, in the order the lifecycle binds them: the owner of each
/// `unchanged` column, then each atom's owner and the owner of a column it compares with,
/// left to right.
fn formula_entities(formula: &TemporalFormula, out: &mut Vec<EntityKey>) {
    fn walk(expr: &TemporalExpr, out: &mut Vec<EntityKey>) {
        match expr {
            TemporalExpr::Atom(atom) => {
                let rhs = match &atom.rhs {
                    TemporalRhs::Column { entity, .. } => Some(*entity),
                    _ => None,
                };
                for ek in std::iter::once(atom.entity).chain(rhs) {
                    if !out.contains(&ek) {
                        out.push(ek);
                    }
                }
            }
            TemporalExpr::Not(inner) => walk(inner, out),
            TemporalExpr::And(a, b) | TemporalExpr::Or(a, b) => {
                walk(a, out);
                walk(b, out);
            }
        }
    }
    if let TemporalFormula::Unchanged { columns, .. } = formula {
        for col in columns {
            if !out.contains(&col.entity) {
                out.push(col.entity);
            }
        }
    }
    for e in formula.exprs() {
        walk(e, out);
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
