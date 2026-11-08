//! State sums and declared refinement (docs/design/algebraic-codegen.md, "State sums").
//!
//! An entity with a lifecycle column is a sum with one variant per value of its tag column.
//! A variant refines a nullable column (always present, or always null, in that state) only
//! when a rule of the model says so, in one of these shapes:
//!
//! - `required(E, x == present)`: present in every state (each conjunct on its own).
//! - `forbidden(E, tag == v, x == present)`: null in `v` (`tag in (..)` names several states,
//!   no tag atom names every state); `x == null` makes it present.
//! - `invariant(E).when(tag == v).then(x == present)`: present in `v`; `x == null`, null.
//!
//! A rule with any other atom, or with a `.when(Cond)`, refines nothing: it states a fact
//! about a combination, not about one state. A state two rules refine both ways (present and
//! null) keeps the column nullable: no reachable state holds that tag, so it says nothing.

use std::collections::BTreeMap;

use rdra_ish_core::location::LocatedSpan;
use rdra_ish_core::model::{
    CmpOpModel, CmpRhs, ColumnType, ComparisonProp, EffectValue, Entity, EntityKey, RuleRef,
    SemanticModel,
};

/// What a rule makes of a nullable column in some states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refined {
    Present,
    Null,
}

/// One refinement and the rule that states it.
#[derive(Clone, Debug)]
pub(super) struct Refinement {
    pub refined: Refined,
    pub site: Option<LocatedSpan>,
}

/// `(tag value, column) -> refinement` for one entity. Absent: the column stays nullable.
pub(super) type Refinements = BTreeMap<(String, String), Refinement>;

/// The declared refinements of `entity`, whose tag column `tag` holds `values`.
pub(super) fn refinements(
    model: &SemanticModel,
    key: EntityKey,
    entity: &Entity,
    tag: &str,
    values: &[String],
) -> Refinements {
    let nullable = |column: &str| {
        column != tag
            && entity
                .columns
                .iter()
                .any(|c| c.name == column && c.is_nullable && c.derived_expr.is_none())
    };
    let guarded = |rule: RuleRef| model.rule_guards.iter().any(|g| g.rule == rule);
    // (states, column, refined, site), before conflicts are resolved.
    let mut found: Vec<(Vec<String>, String, Refined, Option<LocatedSpan>)> = Vec::new();

    for rule in model
        .required_constraints
        .iter()
        .filter(|r| r.entity == key)
    {
        for (column, value) in &rule.conditions {
            if let (true, Some(refined)) = (nullable(column), presence(value)) {
                found.push((values.to_vec(), column.clone(), refined, rule.site.clone()));
            }
        }
    }

    for (i, rule) in model.forbidden_constraints.iter().enumerate() {
        if rule.entity != key || guarded(RuleRef::Forbidden(i)) {
            continue;
        }
        let Some(states) = tag_states(tag, values, &rule.conditions, &rule.comparisons) else {
            continue;
        };
        let others: Vec<_> = rule.conditions.iter().filter(|(c, _)| c != tag).collect();
        let [(column, value)] = others.as_slice() else {
            continue;
        };
        if let (true, Some(refined)) = (nullable(column), presence(value)) {
            let opposite = match refined {
                Refined::Present => Refined::Null,
                Refined::Null => Refined::Present,
            };
            found.push((states, column.clone(), opposite, rule.site.clone()));
        }
    }

    for (i, rule) in model.entity_invariants.iter().enumerate() {
        if rule.entity != key || guarded(RuleRef::Invariant(i)) {
            continue;
        }
        if rule.guards.iter().all(|(c, _)| c != tag)
            && !rule.guard_comparisons.iter().any(|p| p.lhs_column == tag)
        {
            continue;
        }
        let Some(states) = tag_states(tag, values, &rule.guards, &rule.guard_comparisons) else {
            continue;
        };
        if rule.guards.iter().any(|(c, _)| c != tag) {
            continue;
        }
        for (column, value) in &rule.requireds {
            if let (true, Some(refined)) = (nullable(column), presence(value)) {
                found.push((states.clone(), column.clone(), refined, rule.site.clone()));
            }
        }
    }

    let mut out = Refinements::new();
    let mut conflicting: Vec<(String, String)> = Vec::new();
    for (states, column, refined, site) in found {
        for state in states {
            let slot = (state, column.clone());
            if conflicting.contains(&slot) {
                continue;
            }
            match out.get(&slot) {
                Some(existing) if existing.refined != refined => {
                    out.remove(&slot);
                    conflicting.push(slot);
                }
                Some(_) => {}
                None => {
                    out.insert(
                        slot,
                        Refinement {
                            refined,
                            site: site.clone(),
                        },
                    );
                }
            }
        }
    }
    out
}

/// The tag values a rule's tag atoms select: every value when it has none, `None` when an
/// atom is not a plain `==` or `in` on the tag, or when a comparison reads another column.
fn tag_states(
    tag: &str,
    values: &[String],
    conditions: &[(String, EffectValue)],
    comparisons: &[ComparisonProp],
) -> Option<Vec<String>> {
    let mut states: Vec<String> = values.to_vec();
    for (_, value) in conditions.iter().filter(|(c, _)| c == tag) {
        let EffectValue::EnumVariant(v) = value else {
            return None;
        };
        states.retain(|s| s.eq_ignore_ascii_case(v));
    }
    for prop in comparisons {
        if prop.lhs_column != tag || prop.op != CmpOpModel::In {
            return None;
        }
        let CmpRhs::Values(listed) = &prop.rhs else {
            return None;
        };
        states.retain(|s| {
            listed
                .iter()
                .any(|v| matches!(v, EffectValue::EnumVariant(v) if s.eq_ignore_ascii_case(v)))
        });
    }
    Some(states)
}

fn presence(value: &EffectValue) -> Option<Refined> {
    match value.normalize() {
        EffectValue::Present => Some(Refined::Present),
        EffectValue::Null => Some(Refined::Null),
        _ => None,
    }
}

/// The values of the entity's tag column, when it has one.
pub(super) fn tag_column<'m>(
    model: &'m SemanticModel,
    key: EntityKey,
    entity: &'m Entity,
) -> Option<(&'m str, &'m [String])> {
    let tag = rdra_ish_core::entity_lifecycle::discriminator_column(model, key)?;
    let column = entity.columns.iter().find(|c| c.name == tag)?;
    match &column.col_type {
        ColumnType::Enum(values) if !values.is_empty() => Some((tag, values.as_slice())),
        _ => None,
    }
}
