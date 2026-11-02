//! The business rules of a model as one list: the conditions and every constraint predicate.
//!
//! `list --kind rule` prints it and `lint` reads it for rules that carry no label.

use crate::analysis::{body_condition_display, condition_body_display, effect_value_display};
use crate::lint::node_id;
use crate::location::LocatedSpan;
use crate::model::{
    values_display, CmpRhs, ComparisonProp, CrossEntityCondition, EffectValue, EntityKey, NodeRef,
    QuantifierKind, RelKind, SemanticModel,
};

/// One row of the business-rule table.
#[derive(Debug, Clone)]
pub struct BusinessRule {
    /// `forbidden`, `invariant`, `required`, `exclusive`, `quantifier`, `after` or `condition`.
    pub kind: &'static str,
    /// The `.label("...")` of a constraint, the label of a condition.
    pub label: Option<String>,
    /// The entities the rule reads, in the order it names them.
    pub entities: Vec<String>,
    /// The rule as formulas.
    pub text: String,
    /// The use cases an `after` assert follows, or the use cases and APIs a condition guards.
    pub applies_to: Vec<String>,
    pub site: Option<LocatedSpan>,
}

impl BusinessRule {
    /// Whether the author wrote this rule as a constraint predicate that can carry `.label`.
    pub fn is_constraint(&self) -> bool {
        self.kind != "condition"
    }
}

/// The conditions of a single-entity rule, each qualified by the entity.
fn local_parts(
    model: &SemanticModel,
    entity: EntityKey,
    equals: &[(String, EffectValue)],
    comparisons: &[ComparisonProp],
) -> Vec<String> {
    let id = &model.entities[entity].id;
    equals
        .iter()
        .map(|(column, value)| {
            let value = effect_value_display(model, entity, column, value);
            format!("{id}.{column} == {value}")
        })
        .chain(
            comparisons
                .iter()
                .map(|prop| format!("{id}.{}", comparison_display(model, entity, prop))),
        )
        .collect()
}

/// `prop` as written, with the values of an `Enum` or variation column by their labels.
fn comparison_display(model: &SemanticModel, entity: EntityKey, prop: &ComparisonProp) -> String {
    let show = |v: &EffectValue| effect_value_display(model, entity, &prop.lhs_column, v);
    let rhs = match &prop.rhs {
        CmpRhs::Value(v) => show(v),
        CmpRhs::Values(vs) => values_display(vs, show),
        other => other.display(),
    };
    format!("{} {} {rhs}", prop.lhs_column, prop.op.as_str())
}

fn cross_parts(model: &SemanticModel, conds: &[CrossEntityCondition]) -> Vec<String> {
    conds
        .iter()
        .map(|cond| body_condition_display(model, cond))
        .collect()
}

/// `.by(a).by(b)` for the selectors of a join, empty without one.
pub fn by_display(by: &[String]) -> String {
    by.iter().map(|role| format!(".by({role})")).collect()
}

fn all_of(parts: &[String]) -> String {
    parts.join(" AND ")
}

fn when_then(guards: &[String], requireds: &[String]) -> String {
    format!("when {} then {}", all_of(guards), all_of(requireds))
}

fn entity_ids(model: &SemanticModel, keys: &[EntityKey]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for key in keys {
        let id = model.entities[*key].id.clone();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn condition_entities(model: &SemanticModel, conds: &[&CrossEntityCondition]) -> Vec<String> {
    let mut keys = Vec::new();
    for cond in conds {
        match cond {
            CrossEntityCondition::Equals { column, .. } => keys.push(column.entity),
            CrossEntityCondition::Comparison(prop) => keys.push(prop.lhs.entity),
        }
    }
    entity_ids(model, &keys)
}

/// Every condition and constraint of `model`, in source order.
pub fn business_rules(model: &SemanticModel) -> Vec<BusinessRule> {
    let mut rules: Vec<BusinessRule> = Vec::new();
    let mut push =
        |kind, label: &Option<String>, entities, text, applies_to, site: &Option<LocatedSpan>| {
            rules.push(BusinessRule {
                kind,
                label: label.clone(),
                entities,
                text,
                applies_to,
                site: site.clone(),
            });
        };

    for c in &model.forbidden_constraints {
        let text = all_of(&local_parts(model, c.entity, &c.conditions, &c.comparisons));
        let entities = entity_ids(model, &[c.entity]);
        push("forbidden", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.cross_forbidden_constraints {
        let text = all_of(&cross_parts(model, &c.conditions));
        let entities = entity_ids(model, &c.scope);
        push("forbidden", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.entity_invariants {
        let guards = local_parts(model, c.entity, &c.guards, &c.guard_comparisons);
        let requireds = local_parts(model, c.entity, &c.requireds, &c.required_comparisons);
        let entities = entity_ids(model, &[c.entity]);
        let text = when_then(&guards, &requireds);
        push("invariant", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.cross_entity_invariants {
        let text = when_then(
            &cross_parts(model, &c.guards),
            &cross_parts(model, &c.requireds),
        );
        let entities = entity_ids(model, &c.scope);
        push("invariant", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.required_constraints {
        let text = all_of(&local_parts(model, c.entity, &c.conditions, &c.comparisons));
        let entities = entity_ids(model, &[c.entity]);
        push("required", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.exclusive_constraints {
        let parts = local_parts(model, c.entity, &c.conditions, &c.comparisons);
        let text = format!("at most one of: {}", parts.join(", "));
        let entities = entity_ids(model, &[c.entity]);
        push("exclusive", &c.label, entities, text, vec![], &c.site);
    }
    for c in &model.quantifier_constraints {
        let q = &c.quantifier;
        let method = match q.kind {
            QuantifierKind::Has => "has",
            QuantifierKind::None => "none",
        };
        let mut text = format!(
            "when {} then {method} {}",
            all_of(&cross_parts(model, &c.guards)),
            model.entities[q.related].id,
        );
        if !q.conditions.is_empty() {
            text.push_str(&format!("({})", all_of(&cross_parts(model, &q.conditions))));
        }
        text.push_str(&by_display(&q.by));
        let entities = entity_ids(model, &[c.anchor, q.related]);
        push("quantifier", &c.label, entities, text, vec![], &q.site);
    }
    for c in &model.temporal_assertions {
        let text = all_of(&cross_parts(model, &c.requireds));
        let entities = entity_ids(model, &c.scope);
        let anchor = vec![model.use_cases[c.anchor].id.clone()];
        push("after", &c.label, entities, text, anchor, &c.site);
    }
    for (key, condition) in &model.conditions {
        let text = condition_body_display(model, key).unwrap_or_default();
        let reads: Vec<&CrossEntityCondition> = condition
            .body
            .iter()
            .flat_map(|body| body.read_conditions())
            .collect();
        let mut applies_to: Vec<String> = model
            .relations
            .iter()
            .filter(|rel| {
                matches!(rel.kind, RelKind::AppliesTo | RelKind::Guards)
                    && rel.from == NodeRef::Condition(key)
            })
            .filter_map(|rel| node_id(model, &rel.to))
            .collect();
        applies_to.sort();
        applies_to.dedup();
        let site = condition
            .body
            .as_ref()
            .and_then(|body| body.site.clone())
            .or_else(|| model.decl_sites.located("condition", &condition.id));
        push(
            "condition",
            &Some(condition.label.clone()),
            condition_entities(model, &reads),
            text,
            applies_to,
            &site,
        );
    }

    rules.sort_by_key(|rule| match &rule.site {
        Some(site) => (0, site.source_id, site.span.start),
        None => (1, 0, 0),
    });
    rules
}
