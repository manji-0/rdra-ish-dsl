//! Rule validators: the entity-local invariants of the transition system (`forbidden`,
//! `invariant`, `required`, `exclusive`, `@check`) as checks on one row, one validator per
//! record or state type. A rule that reads more than one row (a multi-entity rule, a
//! quantifier, `Unique_*`, `MinCard_*`) or a value no field holds becomes an obligation.

use rdra_ish_core::lifecycle::{At, InvKind, InvText, TransitionSystem};
use rdra_ish_core::location::LocatedSpan;

use super::actions::{expr, simplify};
use super::ir::{DomainModel, Expr, Obligation, RowParam, RuleCheck, ValidatorDecl};
use super::{CodegenError, Names};

pub(super) fn add_validators(
    ts: &TransitionSystem,
    domain: &mut DomainModel,
    names: &mut Names,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<(), CodegenError> {
    // Every record and state type of an entity, with the row the checks read.
    // (entity id, row, state)
    let mut types: Vec<(String, RowParam, Option<super::ir::VariantDecl>)> = Vec::new();
    for record in domain
        .records
        .iter()
        .filter(|r| r.kind == super::ir::RecordKind::Entity)
    {
        types.push((
            record.id.clone(),
            RowParam {
                name: "row".to_string(),
                param: 0,
                state: None,
                entity: record.id.clone(),
                ty: record.name.clone(),
                fields: record.fields.clone(),
                always_null: Vec::new(),
            },
            None,
        ));
    }
    for sum in &domain.sums {
        for v in &sum.variants {
            types.push((
                sum.entity.clone(),
                RowParam {
                    name: "row".to_string(),
                    param: 0,
                    state: Some((sum.tag.wire.clone(), v.value.wire.clone())),
                    entity: sum.entity.clone(),
                    ty: v.name.clone(),
                    fields: v.fields.clone(),
                    always_null: v.always_null.iter().map(|f| f.wire.clone()).collect(),
                },
                Some(v.clone()),
            ));
        }
    }

    let mut validators: Vec<ValidatorDecl> = types
        .iter()
        .map(|(entity_ty, row, _)| ValidatorDecl {
            ty: row.ty.clone(),
            entity: entity_ty.clone(),
            row: row.clone(),
            rules: Vec::new(),
        })
        .collect();

    let mut obligations: Vec<(String, String, Option<String>)> = Vec::new();
    for inv in ts.invariants() {
        if matches!(inv.kind, InvKind::Unique | InvKind::MinCard) {
            continue; // obligations of the data layer
        }
        let at = inv.site.span.as_ref().and_then(site);
        let [binder] = inv.binders.as_slice() else {
            obligations.push((inv.name.clone(), text(&inv.text), at));
            continue;
        };
        let entity_ty = ts.entity(*binder).id.clone();
        let mut per_type: Vec<(usize, Expr)> = Vec::new();
        let mut failed = false;
        for (i, (ety, row, variant)) in types.iter().enumerate() {
            if *ety != entity_ty {
                continue;
            }
            let bind = |a: At| match a {
                At::Bound(0) => Some((0, variant.as_ref())),
                _ => None,
            };
            match expr(ts, &inv.body, std::slice::from_ref(row), &bind) {
                Ok(e) => per_type.push((i, simplify(e))),
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed || per_type.is_empty() {
            obligations.push((inv.name.clone(), text(&inv.text), at));
            continue;
        }
        for (i, e) in per_type {
            if e == Expr::True {
                continue;
            }
            validators[i].rules.push(RuleCheck {
                name: inv.name.clone(),
                text: text(&inv.text),
                expr: e,
            });
        }
    }

    for v in &validators {
        names.claim(
            format!("{}RuleViolations", v.ty),
            format!("the rule check of {}", v.ty),
        )?;
        if v.row.fields.iter().any(|f| f.pascal == "RuleViolations") {
            return Err(CodegenError::NameCollision {
                name: "RuleViolations".to_string(),
                first: format!("a column of {}", v.ty),
                second: "the rule check method".to_string(),
            });
        }
    }
    domain.validators = validators;
    let has_rules = domain.validators.iter().any(|v| !v.rules.is_empty());
    if has_rules {
        push(
            domain,
            None,
            "every row the code writes keeps its rules: its RuleViolations is empty.".to_string(),
            None,
        );
    }
    for (name, text, at) in obligations {
        push(
            domain,
            Some(name),
            format!("holds in every state ({text}); no one-row check prints it."),
            at,
        );
    }
    Ok(())
}

fn push(domain: &mut DomainModel, invariant: Option<String>, text: String, site: Option<String>) {
    let id = format!("O{}", domain.obligations.len() + 1);
    domain.obligations.push(Obligation {
        id,
        invariant,
        text,
        site,
    });
}

fn text(t: &InvText) -> String {
    match t {
        InvText::Conditions(c) => c.clone(),
        InvText::WhenThen { guards, requireds } => format!("when {guards} then {requireds}"),
        InvText::Cross {
            entities,
            conditions,
        } => format!("{entities}: {conditions}"),
        InvText::CrossWhenThen {
            entities,
            guards,
            requireds,
        } => format!("{entities}: when {guards} then {requireds}"),
        InvText::Quantifier {
            anchor,
            related,
            constraint,
        } => format!("{anchor} with {related}: {constraint}"),
        InvText::Unique {
            child,
            parent,
            role,
        } => format!("{child} unique per {parent} ({role})"),
        InvText::MinCard {
            child,
            parent,
            role,
        } => {
            format!("{parent} has a {child} ({role})")
        }
    }
}
