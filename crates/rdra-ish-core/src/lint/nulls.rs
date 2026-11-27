//! A `not` over an ordering comparison of a nullable column.
//!
//! A rule's comparison is two-valued: a null operand meets no `<`, `<=`, `>`, `>=`, so
//! `not (qty < 1)` holds while `qty` is null, and is not `qty >= 1`, which fails there
//! (language reference, § Null). Only `@check` and `property` take `not`; a logical
//! `@derived` reads it three-valued (null in, null out) and is left alone. `not` over `==` /
//! `!=` says what `!=` / `==` say, null included, so it is left alone too, as is an
//! expression that already names the column's null (`qty == null or ...`).

use std::collections::HashSet;

use crate::location::LocatedSpan;
use crate::model::{
    CheckExpr, CmpOpModel, CmpRhs, EffectValue, EntityKey, SemanticModel, TemporalExpr, TemporalRhs,
};

use super::{LintIssue, LintSeverity};

const RULE: &str = "negated-order-admits-null";

/// An ordering comparison under an odd number of `not`s: its text, its negation, and the
/// nullable columns it reads.
struct Negated {
    text: String,
    flipped: String,
    nullable: Vec<String>,
}

fn flip(op: CmpOpModel) -> &'static str {
    match op {
        CmpOpModel::Lt => ">=",
        CmpOpModel::Le => ">",
        CmpOpModel::Gt => "<=",
        CmpOpModel::Ge => "<",
        _ => unreachable!("only an ordering is flipped"),
    }
}

fn names_null(rhs_values: &[&EffectValue]) -> bool {
    rhs_values
        .iter()
        .any(|v| matches!(v, EffectValue::Null | EffectValue::Present))
}

fn is_nullable(model: &SemanticModel, entity: EntityKey, column: &str) -> bool {
    model.entities[entity]
        .columns
        .iter()
        .any(|c| c.name == column && c.is_nullable)
}

/// `@check` leaves, with the parity of the `not`s above each.
fn check_leaves<'a>(
    expr: &'a CheckExpr,
    negated: bool,
    out: &mut Vec<(&'a crate::model::ComparisonProp, bool)>,
) {
    match expr {
        CheckExpr::Cmp(prop) => out.push((prop, negated)),
        CheckExpr::Not(inner) => check_leaves(inner, !negated, out),
        CheckExpr::And(a, b) | CheckExpr::Or(a, b) => {
            check_leaves(a, negated, out);
            check_leaves(b, negated, out);
        }
    }
}

fn temporal_leaves<'a>(
    expr: &'a TemporalExpr,
    negated: bool,
    out: &mut Vec<(&'a crate::model::TemporalAtom, bool)>,
) {
    match expr {
        TemporalExpr::Atom(atom) => out.push((atom, negated)),
        TemporalExpr::Not(inner) => temporal_leaves(inner, !negated, out),
        TemporalExpr::And(a, b) | TemporalExpr::Or(a, b) => {
            temporal_leaves(a, negated, out);
            temporal_leaves(b, negated, out);
        }
    }
}

fn issue(
    found: Vec<Negated>,
    subject_kind: &str,
    subject_id: &str,
    site: &Option<LocatedSpan>,
    issues: &mut Vec<LintIssue>,
) {
    for n in found {
        let nulls = n.nullable.join(" or ");
        let admit = n
            .nullable
            .iter()
            .map(|c| format!("{c} == null or "))
            .collect::<String>();
        let mut issue = LintIssue::new(
            LintSeverity::Warning,
            RULE,
            subject_kind,
            subject_id,
            format!(
                "not ({}) holds while {nulls} is null, since null meets no ordering comparison; it is not {}, which fails there",
                n.text, n.flipped
            ),
            format!(
                "write `{}` to leave null out, or `{admit}{}` to admit it",
                n.flipped, n.flipped
            ),
        );
        if let Some(site) = site {
            issue = issue.with_location(site.clone());
        }
        issues.push(issue);
    }
}

pub(super) fn lint_negated_null_orders(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (key, entity) in &model.entities {
        for column in &entity.columns {
            for check in &column.check_constraints {
                let mut leaves = Vec::new();
                check_leaves(&check.expr, false, &mut leaves);
                let said: HashSet<&str> = leaves
                    .iter()
                    .filter_map(|(p, _)| match &p.rhs {
                        CmpRhs::Value(v) if names_null(&[v]) => Some(p.lhs_column.as_str()),
                        CmpRhs::Values(vs) if names_null(&vs.iter().collect::<Vec<_>>()) => {
                            Some(p.lhs_column.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                let found = leaves
                    .iter()
                    .filter(|(p, negated)| *negated && p.op.is_ordered())
                    .filter_map(|(p, _)| {
                        let mut nullable = vec![];
                        if is_nullable(model, key, &p.lhs_column) {
                            nullable.push(p.lhs_column.clone());
                        }
                        if let CmpRhs::Column(rhs) = &p.rhs {
                            if is_nullable(model, key, rhs) {
                                nullable.push(rhs.clone());
                            }
                        }
                        nullable.retain(|c| !said.contains(c.as_str()));
                        (!nullable.is_empty()).then(|| Negated {
                            text: p.display(),
                            flipped: format!("{} {} {}", p.lhs_column, flip(p.op), p.rhs.display()),
                            nullable,
                        })
                    })
                    .collect();
                issue(found, "entity", &entity.id, &check.site, issues);
            }
        }
    }
    for prop in &model.temporal_properties {
        let mut leaves = Vec::new();
        for expr in prop.formula.exprs() {
            temporal_leaves(expr, false, &mut leaves);
        }
        let name =
            |entity: EntityKey, column: &str| format!("{}.{column}", model.entities[entity].id);
        let said: HashSet<String> = leaves
            .iter()
            .filter(|(a, _)| match &a.rhs {
                TemporalRhs::Value(v) => names_null(&[v]),
                TemporalRhs::Values(vs) => names_null(&vs.iter().collect::<Vec<_>>()),
                _ => false,
            })
            .map(|(a, _)| name(a.entity, &a.column))
            .collect();
        let found = leaves
            .iter()
            .filter(|(a, negated)| *negated && a.op.is_ordered())
            .filter_map(|(a, _)| {
                let lhs = name(a.entity, &a.column);
                let mut nullable = vec![];
                if is_nullable(model, a.entity, &a.column) {
                    nullable.push(lhs.clone());
                }
                let rhs = match &a.rhs {
                    TemporalRhs::Column { entity, column } => {
                        let rhs = name(*entity, column);
                        if is_nullable(model, *entity, column) {
                            nullable.push(rhs.clone());
                        }
                        rhs
                    }
                    TemporalRhs::IntLit(n) | TemporalRhs::Value(EffectValue::Int(n)) => {
                        n.to_string()
                    }
                    TemporalRhs::Value(v) => v.literal(),
                    TemporalRhs::Values(_) => return None,
                };
                nullable.retain(|c| !said.contains(c));
                (!nullable.is_empty()).then(|| Negated {
                    text: format!("{lhs} {} {rhs}", a.op.as_str()),
                    flipped: format!("{lhs} {} {rhs}", flip(a.op)),
                    nullable,
                })
            })
            .collect();
        issue(found, "property", &prop.id, &prop.site, issues);
    }
}
