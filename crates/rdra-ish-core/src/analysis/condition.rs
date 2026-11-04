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
    add_condition_entities_to_scope, along_scope, resolve_cross_condition,
    resolve_related_quantifier, selectors,
};
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
/// operand on a lifecycle column yields `StateModel`, and so does the column of each
/// `transitions` arm the condition guards with `.when`; the variation of each `chooses` value
/// it guards yields `Variation`. Without a body, the `combines`
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
            for st in model
                .state_transitions
                .iter()
                .filter(|t| t.guard == Some(ck))
            {
                push(ConditionEdge::StateModel {
                    entity: st.entity,
                    column: st.column.clone(),
                });
            }
            for choice in model
                .column_choices
                .iter()
                .filter(|c| c.values.iter().any(|v| v.guard == Some(ck)))
            {
                if let Some(key) = chosen_variation(model, choice) {
                    push(ConditionEdge::Variation(key));
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

/// The variation that types a `chooses` column.
pub fn chosen_variation(model: &SemanticModel, choice: &ColumnChoice) -> Option<VariationKey> {
    let column = model.entities[choice.entity]
        .columns
        .iter()
        .find(|c| c.name == choice.column)?;
    match column.col_type {
        ColumnType::Variation(key) => Some(key),
        _ => None,
    }
}

/// `value` of `column` as a human reads it: an `Enum` or variation value by its label.
pub(crate) fn effect_value_display(
    model: &SemanticModel,
    entity: EntityKey,
    column: &str,
    value: &EffectValue,
) -> std::string::String {
    match value {
        EffectValue::EnumVariant(v) => model.value_display(entity, column, v).to_string(),
        EffectValue::Bool(b) => b.to_string(),
        EffectValue::Int(n) => n.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "present".to_string(),
        EffectValue::Null => "null".to_string(),
    }
}

fn column_display(model: &SemanticModel, column: &QualifiedModelColumnRef) -> std::string::String {
    format!("{}.{}", model.entities[column.entity].id, column.column)
}

pub(crate) fn body_condition_display(
    model: &SemanticModel,
    condition: &CrossEntityCondition,
) -> std::string::String {
    match condition {
        CrossEntityCondition::Equals { column, value } => {
            format!(
                "{} == {}",
                column_display(model, column),
                effect_value_display(model, column.entity, &column.column, value)
            )
        }
        CrossEntityCondition::Comparison(prop) => {
            let rhs = match &prop.rhs {
                CrossCmpRhs::Column(column) => column_display(model, column),
                CrossCmpRhs::IntLit(n) => n.to_string(),
                CrossCmpRhs::Now => "now".to_string(),
                CrossCmpRhs::Value(v) => {
                    effect_value_display(model, prop.lhs.entity, &prop.lhs.column, v)
                }
                CrossCmpRhs::Values(vs) => values_display(vs, |v| {
                    effect_value_display(model, prop.lhs.entity, &prop.lhs.column, v)
                }),
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
    let chained = !matches!(body.along, CrossConstraintScope::GlobalProduct);
    if chained || !body.quantifiers.is_empty() || !body.by.is_empty() {
        text = format!("({text})");
    }
    text.push_str(&crate::rules::by_display(&body.by));
    text.push_str(&crate::rules::along_display(model, &body.along));
    for q in &body.quantifiers {
        let method = match q.kind {
            QuantifierKind::Has => "has",
            QuantifierKind::None => "none",
        };
        let arg = if q.conditions.is_empty() {
            model.entities[q.related].id.clone()
        } else {
            comparisons(&q.conditions)
        };
        text.push_str(&format!(".{method}({arg})"));
        text.push_str(&crate::rules::by_display(&q.by));
    }
    Some(text)
}

/// A `holds` body that guards a use case, and the API it is applied to when it guards the
/// use case through `applies_to(Cond, Api)`: that API's transaction carries it.
#[derive(Clone, Copy)]
pub struct AppliedBody<'m> {
    pub cond: ConditionKey,
    pub usecase: UseCaseKey,
    pub api: Option<ApiKey>,
    pub body: &'m HoldsBody,
}

/// Every `holds` body that guards a use case: `applies_to(Cond, UC)`, or `applies_to(Cond, Api)`
/// for each use case that invokes the API. In relation order, without duplicates.
pub fn applied_bodies(model: &SemanticModel) -> Vec<AppliedBody<'_>> {
    let mut out: Vec<AppliedBody<'_>> = Vec::new();
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
        let targets: Vec<(UseCaseKey, Option<ApiKey>)> = match rel.to {
            NodeRef::UseCase(uc) => vec![(uc, None)],
            NodeRef::Api(api) => model
                .relations
                .iter()
                .filter_map(|r| match (&r.kind, &r.from, &r.to) {
                    (RelKind::Invokes, NodeRef::UseCase(uc), NodeRef::Api(a)) if *a == api => {
                        Some((*uc, Some(api)))
                    }
                    _ => None,
                })
                .collect(),
            _ => continue,
        };
        for (uc, api) in targets {
            if !out
                .iter()
                .any(|b| (b.cond, b.usecase, b.api) == (ck, uc, api))
            {
                out.push(AppliedBody {
                    cond: ck,
                    usecase: uc,
                    api,
                    body,
                });
            }
        }
    }
    out
}

/// The labels of the conditions whose `holds` body guards `uc`, in `applied_bodies` order.
pub fn guard_labels(model: &SemanticModel, uc: UseCaseKey) -> Vec<&str> {
    applied_bodies(model)
        .into_iter()
        .filter(|b| b.usecase == uc)
        .map(|b| model.conditions[b.cond].label.as_str())
        .collect()
}

/// The labels of the conditions that guard one `transitions` arm: those of its use case, in
/// `guard_labels` order, then the arm's own `.when(Cond)`.
pub fn transition_guard_labels<'m>(model: &'m SemanticModel, st: &StateTransition) -> Vec<&'m str> {
    let mut labels = guard_labels(model, st.usecase);
    if let Some(ck) = st.guard {
        let label = model.conditions[ck].label.as_str();
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels
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
    match (&qref.kind_qualifier, qref.parts.as_slice()) {
        (None, [_]) | (Some(Kind::Variation), _) => {
            match resolve_arg(model, &PredicateArg::Ref(qref.clone()), ctx, diags)? {
                NodeRef::Variation(key) => Some(CombinesTarget::Variation(key)),
                other => {
                    push_error_qref(ctx, diags, qref, mismatch(other.kind_tag()));
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
                            import: None,
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
                    import: None,
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
    for (at, cc) in pred.chain.iter().enumerate() {
        if matches!(cc.name.as_str(), "has" | "none") {
            let Some(anchor) = anchors.first().copied() else {
                break;
            };
            let by = selectors(&pred.chain, Some(at));
            quantifiers.extend(resolve_related_quantifier(
                model, anchor, cc, by, ctx, diags,
            ));
        }
    }
    let by = selectors(&pred.chain, None);
    let along = along_scope(model, pred, ctx, diags);
    model.conditions[condition].body = Some(HoldsBody {
        conds: body,
        quantifiers,
        by,
        along,
        site: Some(ctx.locate(pred.span.clone())),
    });
    model
        .typed_predicates
        .push(TypedPredicate::Holds { condition });
}

/// After every predicate is processed: a use case that `chooses` a column does not also
/// `sets` it.
pub(crate) fn check_choices(model: &SemanticModel, diags: &mut Vec<Diagnostic>) {
    for choice in &model.column_choices {
        let set = model.column_effects.iter().any(|e| {
            (e.origin, e.entity, &e.column) == (choice.origin, choice.entity, &choice.column)
        });
        if set {
            diags.push(Diagnostic {
                error: RdraError::ChoosesAndSets {
                    usecase: model.use_cases[choice.origin].id.clone(),
                    entity: model.entities[choice.entity].id.clone(),
                    col: choice.column.clone(),
                },
                is_warning: false,
                location: choice.site.clone(),
            });
        }
    }
}

/// After every predicate is processed: a `.when(Cond)` guard, on a `transitions` arm, a
/// `chooses` value, or a `sets` value, needs the condition's `holds` body, which may come later in the file than
/// the guarded write. Each guard that has its body becomes a `Guards` relation to its use case,
/// once.
pub(crate) fn check_write_guards(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let arms = model
        .state_transitions
        .iter()
        .filter_map(|st| Some(("transitions", st.guard?, st.usecase, &st.site)));
    let values = model.column_choices.iter().flat_map(|c| {
        c.values
            .iter()
            .filter_map(move |v| Some(("chooses", v.guard?, c.origin, &v.site)))
    });
    let sets = model
        .column_effects
        .iter()
        .filter_map(|ce| Some(("sets", ce.guard?, ce.origin, &ce.site)));
    let mut guards: Vec<(ConditionKey, UseCaseKey)> = Vec::new();
    for (pred, ck, uc, site) in arms.chain(values).chain(sets) {
        if model.conditions[ck].body.is_some() {
            if !guards.contains(&(ck, uc)) {
                guards.push((ck, uc));
            }
            continue;
        }
        diags.push(Diagnostic {
            error: RdraError::WhenConditionWithoutBody {
                pred: pred.into(),
                subject: model.use_cases[uc].id.clone(),
                condition: model.conditions[ck].id.clone(),
            },
            is_warning: false,
            location: site.clone(),
        });
    }
    for (ck, uc) in guards {
        model.relations.push(Relation {
            site: None,
            from: NodeRef::Condition(ck),
            to: NodeRef::UseCase(uc),
            kind: RelKind::Guards,
            options: RelationOptions::default(),
            via: None,
        });
    }
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
