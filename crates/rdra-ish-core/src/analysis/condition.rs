//! RDRA 条件: a variation x state-model combination, with an optional `holds` body.
//!
//! The condition's links to variations and state models have one source. A condition with a
//! body derives them from the columns the body names; one without a body reads its `combines`
//! declarations. `condition_edges` is the only function consumers call.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::arg_resolve::{resolve_arg, resolve_entity_name, resolve_namespaced_variation};
use super::constraint::{
    add_condition_entities_to_scope, resolve_cross_condition, resolve_related_quantifier,
};
use super::nodes::node_kind_tag_str;
use super::predicate_process::predicate_arg_display;
use super::qref_util::qref_display;

/// One link from a condition to the RDRA elements it combines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConditionEdge {
    Variation(VariationKey),
    /// A lifecycle column of an entity (RDRA 状態モデル).
    StateModel {
        entity: EntityKey,
        column: std::string::String,
    },
}

fn body_columns(condition: &CrossEntityCondition) -> Vec<&QualifiedModelColumnRef> {
    match condition {
        CrossEntityCondition::Equals { column, .. } => vec![column],
        CrossEntityCondition::Comparison(prop) => {
            let mut columns = vec![&prop.lhs];
            if let CrossCmpRhs::Column(rhs) = &prop.rhs {
                columns.push(rhs);
            }
            columns
        }
    }
}

/// The variation and state-model links of `ck`, in first-mention order without duplicates.
///
/// With a `holds` body, every operand column typed by a variation yields `Variation` and every
/// operand on a lifecycle column yields `StateModel`. Without a body, the `combines`
/// declarations are the links.
pub fn condition_edges(model: &SemanticModel, ck: ConditionKey) -> Vec<ConditionEdge> {
    let mut edges: Vec<ConditionEdge> = Vec::new();
    let mut push = |edge: ConditionEdge| {
        if !edges.contains(&edge) {
            edges.push(edge);
        }
    };
    let Some(condition) = model.conditions.get(ck) else {
        return edges;
    };
    match &condition.body {
        Some(body) => {
            for operand in body.read_conditions().flat_map(body_columns) {
                let Some(entity) = model.entities.get(operand.entity) else {
                    continue;
                };
                let Some(column) = entity.columns.iter().find(|c| c.name == operand.column) else {
                    continue;
                };
                if let ColumnType::Variation(key) = &column.col_type {
                    push(ConditionEdge::Variation(*key));
                }
                if model.is_lifecycle_column(operand.entity, &operand.column) {
                    push(ConditionEdge::StateModel {
                        entity: operand.entity,
                        column: operand.column.clone(),
                    });
                }
            }
        }
        None => {
            for decl in model.combines.iter().filter(|d| d.condition == ck) {
                push(match &decl.target {
                    CombinesTarget::Variation(key) => ConditionEdge::Variation(*key),
                    CombinesTarget::StateModel { entity, column } => ConditionEdge::StateModel {
                        entity: *entity,
                        column: column.clone(),
                    },
                });
            }
        }
    }
    edges
}

fn effect_value_display(value: &EffectValue) -> std::string::String {
    match value {
        EffectValue::EnumVariant(v) => v.clone(),
        EffectValue::Bool(b) => b.to_string(),
        EffectValue::Int(n) => n.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "present".to_string(),
        EffectValue::Null => "null".to_string(),
    }
}

fn column_display(model: &SemanticModel, column: &QualifiedModelColumnRef) -> std::string::String {
    format!("{}.{}", model.entities[column.entity].id, column.column)
}

fn body_condition_display(
    model: &SemanticModel,
    condition: &CrossEntityCondition,
) -> std::string::String {
    match condition {
        CrossEntityCondition::Equals { column, value } => {
            format!(
                "{} == {}",
                column_display(model, column),
                effect_value_display(value)
            )
        }
        CrossEntityCondition::Comparison(prop) => {
            let rhs = match &prop.rhs {
                CrossCmpRhs::Column(column) => column_display(model, column),
                CrossCmpRhs::IntLit(n) => n.to_string(),
                CrossCmpRhs::Now => "now".to_string(),
            };
            format!(
                "{} {} {}",
                column_display(model, &prop.lhs),
                prop.op.as_str(),
                rhs
            )
        }
    }
}

/// The `holds` body as one line, comparisons joined by `, `; `None` when the condition has no body.
pub fn condition_body_display(
    model: &SemanticModel,
    ck: ConditionKey,
) -> Option<std::string::String> {
    let body = model.conditions.get(ck)?.body.as_ref()?;
    let comparisons = |conds: &[CrossEntityCondition]| {
        conds
            .iter()
            .map(|c| body_condition_display(model, c))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = comparisons(&body.conds);
    if !body.quantifiers.is_empty() {
        text = format!("({text})");
    }
    for q in &body.quantifiers {
        let method = match q.kind {
            QuantifierKind::Has => "has",
            QuantifierKind::None => "none",
        };
        text.push_str(&format!(".{method}({})", comparisons(&q.conditions)));
    }
    Some(text)
}

/// Every `holds` body that guards a use case: `applies_to(Cond, UC)`, or `applies_to(Cond, Api)`
/// for each use case that invokes the API. In relation order, without duplicates.
pub fn applied_bodies(model: &SemanticModel) -> Vec<(ConditionKey, UseCaseKey, &HoldsBody)> {
    let mut out: Vec<(ConditionKey, UseCaseKey, &HoldsBody)> = Vec::new();
    for rel in model
        .relations
        .iter()
        .filter(|r| r.kind == RelKind::AppliesTo)
    {
        let NodeRef::Condition(ck) = rel.from else {
            continue;
        };
        let Some(body) = model.conditions[ck].body.as_ref() else {
            continue;
        };
        let ucs: Vec<UseCaseKey> = match rel.to {
            NodeRef::UseCase(uc) => vec![uc],
            NodeRef::Api(api) => model
                .relations
                .iter()
                .filter_map(|r| match (&r.kind, &r.from, &r.to) {
                    (RelKind::Invokes, NodeRef::UseCase(uc), NodeRef::Api(a)) if *a == api => {
                        Some(*uc)
                    }
                    _ => None,
                })
                .collect(),
            _ => continue,
        };
        for uc in ucs {
            if !out.iter().any(|(c, u, _)| (*c, *u) == (ck, uc)) {
                out.push((ck, uc, body));
            }
        }
    }
    out
}

/// Second argument of `combines`: a variation, or `Entity.column`.
///
/// A one-part reference is a variation. A two-part reference is a variation through the import
/// scope first (`shared.PaymentMethod`) and `Entity.column` second; when both resolve the
/// target is ambiguous and the author writes `variation::Name`.
fn resolve_combines_target(
    model: &SemanticModel,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CombinesTarget> {
    let mismatch = |actual: &str| RdraError::TypeMismatch {
        pred: "combines".to_string(),
        id: qref_display(qref),
        actual: actual.to_string(),
        expected: "variation|Entity.column".to_string(),
    };
    if qref.kind_qualifier.is_none()
        && qref.parts.len() >= 2
        && crate::binding::bound_ext(model, &qref.parts[0]).is_some()
    {
        let entity = QRef {
            kind_qualifier: None,
            parts: qref.parts[..2].to_vec(),
            span: qref.span.clone(),
        };
        match crate::binding::resolve_foreign(model, &entity, ctx, diags) {
            crate::binding::Foreign::Node(node) if qref.parts.len() == 2 => {
                crate::binding::check_foreign_operand(
                    model,
                    "combines",
                    1,
                    &node,
                    qref.span.clone(),
                    ctx,
                    diags,
                );
            }
            crate::binding::Foreign::Node(NodeRef::Entity(entity)) => {
                let column = &qref.parts[2];
                let known = model.entities[entity]
                    .columns
                    .iter()
                    .any(|c| &c.name == column);
                let err = if known {
                    RdraError::ForeignStateNeedsEnvironment {
                        id: model.entities[entity].id.clone(),
                    }
                } else {
                    RdraError::UnknownColumn {
                        entity: model.entities[entity].id.clone(),
                        col: column.clone(),
                    }
                };
                push_error_qref(ctx, diags, qref, err);
            }
            crate::binding::Foreign::Node(other) => {
                push_error_qref(ctx, diags, qref, mismatch(node_kind_tag_str(&other)));
            }
            _ => {}
        }
        return None;
    }
    match (&qref.kind_qualifier, qref.parts.as_slice()) {
        (None, [_]) | (Some(Kind::Variation), _) => {
            match resolve_arg(model, &PredicateArg::Ref(qref.clone()), ctx, diags)? {
                NodeRef::Variation(key) => Some(CombinesTarget::Variation(key)),
                other => {
                    push_error_qref(ctx, diags, qref, mismatch(node_kind_tag_str(&other)));
                    None
                }
            }
        }
        (Some(kind), _) => {
            push_error_qref(ctx, diags, qref, mismatch(kind.name()));
            None
        }
        (None, [head, tail]) => {
            let variation = resolve_namespaced_variation(model, head, tail, ctx);
            let entity = resolve_entity_name(model, head, ctx);
            let state_column =
                entity.filter(|e| model.entities[*e].columns.iter().any(|c| &c.name == tail));
            match (variation, state_column, entity) {
                (Some(_), Some(entity), _) => {
                    push_error_qref(
                        ctx,
                        diags,
                        qref,
                        RdraError::AmbiguousCombinesTarget {
                            target: qref_display(qref),
                            variation: tail.clone(),
                            entity: model.entities[entity].id.clone(),
                        },
                    );
                    None
                }
                (Some(key), None, _) => Some(CombinesTarget::Variation(key)),
                (None, Some(entity), _) => Some(CombinesTarget::StateModel {
                    entity,
                    column: tail.clone(),
                }),
                (None, None, Some(entity)) => {
                    push_error_qref(
                        ctx,
                        diags,
                        qref,
                        RdraError::UnknownColumn {
                            entity: model.entities[entity].id.clone(),
                            col: tail.clone(),
                        },
                    );
                    None
                }
                (None, None, None) => {
                    push_error_qref(
                        ctx,
                        diags,
                        qref,
                        RdraError::UndefinedSymbol {
                            id: qref_display(qref),
                        },
                    );
                    None
                }
            }
        }
        (None, _) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::UndefinedSymbol {
                    id: qref_display(qref),
                },
            );
            None
        }
    }
}

pub(crate) fn process_combines_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::Condition(condition))) = resolved.first() else {
        return;
    };
    let condition = *condition;
    let Some(PredicateArg::Ref(qref)) = pred.args.get(1) else {
        if let Some(arg) = pred.args.get(1) {
            push_error_arg(
                ctx,
                diags,
                &pred.args,
                1,
                RdraError::TypeMismatch {
                    pred: "combines".to_string(),
                    id: predicate_arg_display(arg),
                    actual: "value".to_string(),
                    expected: "variation|Entity.column".to_string(),
                },
            );
        }
        return;
    };
    if model.conditions[condition].body.is_some() {
        push_error(
            ctx,
            diags,
            pred.span.clone(),
            RdraError::ConditionCombinesWithBody {
                condition: model.conditions[condition].id.clone(),
            },
        );
        return;
    }
    let Some(target) = resolve_combines_target(model, qref, ctx, diags) else {
        return;
    };
    if model
        .combines
        .iter()
        .any(|d| d.condition == condition && d.target == target)
    {
        return;
    }
    model.combines.push(CombinesDecl {
        condition,
        target: target.clone(),
        location: Some(ctx.locate(pred.span.clone())),
    });
    model
        .typed_predicates
        .push(TypedPredicate::Combines { condition, target });
}

pub(crate) fn process_holds_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::Condition(condition))) = resolved.first() else {
        return;
    };
    let condition = *condition;
    let id = model.conditions[condition].id.clone();
    if model.conditions[condition].body.is_some() {
        push_error(
            ctx,
            diags,
            pred.span.clone(),
            RdraError::ConditionSecondBody { condition: id },
        );
        return;
    }
    if model.combines.iter().any(|d| d.condition == condition) {
        push_error(
            ctx,
            diags,
            pred.span.clone(),
            RdraError::ConditionCombinesWithBody { condition: id },
        );
        return;
    }

    let mut body = Vec::new();
    for (index, arg) in pred.args.iter().enumerate().skip(1) {
        if !matches!(arg, PredicateArg::Expr(Expr::Cmp(_))) {
            push_error_arg(
                ctx,
                diags,
                &pred.args,
                index,
                RdraError::TypeMismatch {
                    pred: "holds".to_string(),
                    id: predicate_arg_display(arg),
                    actual: "argument".to_string(),
                    expected: "comparison such as `Entity.column == value`".to_string(),
                },
            );
            continue;
        }
        if let Some(parsed) = resolve_cross_condition(model, &[], "holds", arg, ctx, diags) {
            body.push(parsed);
        }
    }
    if body.is_empty() {
        return;
    }
    let mut anchors = Vec::new();
    add_condition_entities_to_scope(&mut anchors, &body);
    let mut quantifiers = Vec::new();
    for cc in &pred.chain {
        if matches!(cc.name.as_str(), "has" | "none") {
            let Some(anchor) = anchors.first().copied() else {
                break;
            };
            quantifiers.extend(resolve_related_quantifier(model, anchor, cc, ctx, diags));
        }
    }
    model.conditions[condition].body = Some(HoldsBody {
        conds: body,
        quantifiers,
        site: Some(ctx.locate(pred.span.clone())),
    });
    model
        .typed_predicates
        .push(TypedPredicate::Holds { condition });
}

/// After every predicate is processed: reject `combines` targets that are not lifecycle
/// columns (`transitions` may come later in the file), then record each condition's derived
/// links as `Combines` relations so diagrams and lint see them.
pub(crate) fn derive_condition_links(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let decls = std::mem::take(&mut model.combines);
    for decl in decls {
        if let CombinesTarget::StateModel { entity, column } = &decl.target {
            if !model.is_lifecycle_column(*entity, column) {
                diags.push(Diagnostic {
                    error: RdraError::ConditionCombinesAttribute {
                        condition: model.conditions[decl.condition].id.clone(),
                        entity: model.entities[*entity].id.clone(),
                        col: column.clone(),
                    },
                    is_warning: false,
                    location: decl.location.clone(),
                });
                continue;
            }
        }
        model.combines.push(decl);
    }

    let keys: Vec<ConditionKey> = model.conditions.keys().collect();
    for ck in keys {
        for edge in condition_edges(model, ck) {
            let to = match edge {
                ConditionEdge::Variation(key) => NodeRef::Variation(key),
                ConditionEdge::StateModel { entity, .. } => NodeRef::Entity(entity),
            };
            model.relations.push(Relation {
                site: None,
                from: NodeRef::Condition(ck),
                to,
                kind: RelKind::Combines,
                options: RelationOptions::default(),
                via: None,
            });
        }
    }
}
