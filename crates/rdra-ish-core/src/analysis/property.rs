//! Temporal `property` declaration lowering for TLA+ export.

use crate::analysis_diag::push_error;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::{
    CmpOpModel, EffectValue, NodeRef, SemanticModel, TemporalAtom, TemporalExpr, TemporalFormula,
    TemporalProperty, TemporalRhs, UseCaseKey,
};
use rdra_ish_syntax::ast::{
    AstTemporalFormula, CmpOp, Comparison, Expr, Operand, PredicateArg, PropertyDecl,
};

use super::arg_resolve::resolve_arg;
use super::comparison::{resolve_bare_rhs, resolve_set_values, BareRhs};

/// Why an atom does not lower: a message for `InvalidTemporalProperty`, or a comparison error
/// reported as itself.
enum LowerError {
    Message(String),
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
    match lower_formula(model, &decl.formula) {
        Ok(formula) => {
            model.temporal_properties.push(TemporalProperty {
                site: Some(ctx.locate(decl.span.clone())),
                id: decl.id.clone(),
                label: decl.label.clone().unwrap_or_else(|| decl.id.clone()),
                formula,
                fair,
            });
        }
        Err(LowerError::Comparison(err)) => push_error(ctx, diags, decl.span.clone(), err),
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
    for (index, cc) in decl.chain.iter().enumerate() {
        if cc.name != "fair" {
            let err = RdraError::UnknownChainMethod {
                pred: "property".into(),
                name: cc.name.clone(),
                hint: "a property takes only `.fair(UseCase, ...)`".into(),
            };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        if matches!(decl.formula, AstTemporalFormula::Always(_)) {
            let err = RdraError::FairOnSafetyProperty { id: id() };
            push_error(ctx, diags, cc.span.clone(), err);
            ok = false;
            continue;
        }
        let mut well_formed = index == 0 && !cc.args.is_empty();
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
        AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => Ok(TemporalFormula::LeadsTo {
            antecedent: lower_expr(model, antecedent)?,
            consequent: lower_expr(model, consequent)?,
        }),
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

/// A bare right-hand side of `==` / `!=` read by the left-hand column's type, when the atom's
/// entity and column resolve; `None` leaves the atom to the lifecycle, which reports it.
fn typed_bare_rhs(
    model: &SemanticModel,
    entity: Option<&str>,
    column: &str,
    op: &CmpOp,
    name: &str,
) -> Option<Result<TemporalRhs, LowerError>> {
    if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
        return None;
    }
    let ek = model.property_entity(entity, column)?;
    let ent = &model.entities[ek];
    let lhs = ent.columns.iter().find(|c| c.name == column)?;
    let qualified = |n: &str| {
        ent.columns
            .iter()
            .any(|c| c.name == n)
            .then(|| format!("{}.{n}", ent.id))
    };
    Some(
        match resolve_bare_rhs(&model.variations, lhs, op, name, qualified) {
            Ok(BareRhs::Value(value)) => Ok(TemporalRhs::Value(value)),
            Ok(BareRhs::Column) => Ok(TemporalRhs::Column {
                entity: None,
                column: name.to_string(),
            }),
            Err(err) => Err(LowerError::Comparison(err)),
        },
    )
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
        other => {
            return Err(format!("temporal atom lhs must be a column, got {other:?}").into());
        }
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
            match typed_bare_rhs(model, entity.as_deref(), &column, &cmp.op, name) {
                Some(rhs) => rhs?,
                // An order comparison, or an atom whose column does not resolve (the lifecycle
                // reports it): literals by their spelling, any other name a column.
                None => match name.as_str() {
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
                },
            }
        }
        Operand::Now => {
            return Err(LowerError::Message(
                "`now` is not supported in temporal property atoms".into(),
            ));
        }
        Operand::Values(names) => set_rhs(model, entity.as_deref(), &column, names)?,
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

/// The values of `in` / `not in`, read by the left-hand column's type when the atom's entity
/// and column resolve; otherwise by their spelling, and the lifecycle reports the column.
fn set_rhs(
    model: &SemanticModel,
    entity: Option<&str>,
    column: &str,
    names: &[String],
) -> Result<TemporalRhs, LowerError> {
    let lhs = model
        .property_entity(entity, column)
        .and_then(|ek| model.entities[ek].columns.iter().find(|c| c.name == column));
    let values = match lhs {
        Some(lhs) => {
            resolve_set_values(&model.variations, lhs, names).map_err(LowerError::Comparison)?
        }
        None => names
            .iter()
            .map(|name| match name.parse::<i64>() {
                Ok(n) => EffectValue::Int(n),
                Err(_) => parse_effect_literal(name),
            })
            .collect(),
    };
    Ok(TemporalRhs::Values(values))
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
