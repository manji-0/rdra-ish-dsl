//! Entity and cross-entity constraint predicate helpers.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use slotmap::SlotMap;

use super::arg_resolve::resolve_arg;
use super::comparison::{
    is_order_op, resolve_bare_rhs, resolve_comparison, resolve_set_values, to_model_op,
    type_category, BareRhs,
};
use super::effect::parse_effect_value;
use super::qref_util::qref_display;

// ── 制約述語用ヘルパー ────────────────────────────────────────────────────────

/// `Lit(s)` または kind修飾なし1セグメントの `Ref` から文字列を取り出す。
/// `whenstatus == delivered` の裸ident引数と `sets(...)` の引用符付きリテラル
/// 引数の両方を許容するための統一抽出。
pub(crate) fn arg_as_str(arg: &PredicateArg) -> Option<String> {
    match arg {
        PredicateArg::Lit(s) => Some(s.clone()),
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some(qref.parts[0].clone())
        }
        _ => None,
    }
}

/// The chain call a `.role(role)` at `at` selects the key of: the nearest `.along`, `.has` or
/// `.none` before it, or `None` when it follows none, where it selects the key of the
/// statement itself (a `holds` body, a copy).
fn by_owner(chain: &[ChainCall], at: usize) -> Option<usize> {
    chain[..at]
        .iter()
        .rposition(|cc| matches!(cc.name.as_str(), "along" | "has" | "none"))
}

/// The roles the `.role(..)` calls that select the key of `owner` name (see [`by_owner`]).
pub(crate) fn selectors(chain: &[ChainCall], owner: Option<usize>) -> Vec<String> {
    chain
        .iter()
        .enumerate()
        .filter(|(at, cc)| cc.name == "role" && by_owner(chain, *at) == owner)
        .flat_map(|(_, cc)| cc.args.iter().filter_map(arg_as_str))
        .collect()
}

/// The `.role(..)` calls that follow no `.along`, `.has` or `.none`, on a statement that has
/// no key of its own to select.
pub(crate) fn report_leading_by(chain: &[ChainCall], ctx: DiagCtxt, diags: &mut Vec<Diagnostic>) {
    for (at, cc) in chain.iter().enumerate() {
        if cc.name == "role" && by_owner(chain, at).is_none() {
            push_error(ctx, diags, cc.span.clone(), RdraError::RoleWithoutJoin);
        }
    }
}

/// The text of the rule's one `.label("...")` chain call. A second call, or an argument that is
/// not a single string literal, is reported and the label is left unset.
pub(crate) fn rule_label(
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let mut calls = pred.chain.iter().filter(|cc| cc.name == "label");
    let first = calls.next()?;
    if let Some(second) = calls.next() {
        let err = RdraError::DuplicateRuleLabel {
            pred: pred.name.clone(),
        };
        push_error(ctx, diags, second.span.clone(), err);
    }
    match first.args.as_slice() {
        [PredicateArg::Lit(text)] => Some(text.clone()),
        _ => {
            let err = RdraError::RuleLabelArg {
                pred: pred.name.clone(),
            };
            push_error(ctx, diags, first.span.clone(), err);
            None
        }
    }
}

/// The condition a rule's `.when(Cond)` names: `Some(key)` when the call's one argument is a
/// reference to a condition, `Ok(None)` when it is anything else (a comparison, or the two-word
/// pair), `Err` after reporting a reference to something that is not a condition.
pub(crate) fn when_condition_ref(
    model: &SemanticModel,
    pred: &str,
    cc: &ChainCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<ConditionKey>, ()> {
    let [PredicateArg::Ref(qref)] = cc.args.as_slice() else {
        return Ok(None);
    };
    match resolve_arg(model, &cc.args[0], ctx, diags) {
        Some(NodeRef::Condition(ck)) => Ok(Some(ck)),
        Some(other) => {
            let got = qref_display(qref);
            let err = RdraError::RuleWhenArg {
                pred: pred.to_string(),
                hint: format!(
                    "`{got}` is a {}; name a condition declared with `holds(Name, Entity.column == value)`, or write the comparison itself",
                    other.kind_tag()
                ),
                got,
            };
            push_error(ctx, diags, cc.span.clone(), err);
            Err(())
        }
        // resolve_arg already reported an undefined or ambiguous reference.
        None => Err(()),
    }
}

/// `.when(...)` on a `forbidden`, which takes a condition only: its comparisons are its
/// arguments. One condition key per call, or `Err` after reporting the first other call.
pub(crate) fn forbidden_when_conditions(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Vec<(ConditionKey, Span)>, ()> {
    let mut conditions = Vec::new();
    for cc in pred.chain.iter().filter(|cc| cc.name == "when") {
        match when_condition_ref(model, &pred.name, cc, ctx, diags)? {
            Some(ck) => conditions.push((ck, cc.span.clone())),
            None => {
                let got = cc
                    .args
                    .iter()
                    .map(super::predicate_process::predicate_arg_display)
                    .collect::<Vec<_>>()
                    .join(", ");
                let err = RdraError::RuleWhenArg {
                    pred: pred.name.clone(),
                    got,
                    hint: "write a comparison as another argument, `forbidden(Entity, ..., status == value)`, and name a condition declared with `holds(Name, Entity.column == value)` in `.when(Name)`".to_string(),
                };
                push_error(ctx, diags, cc.span.clone(), err);
                return Err(());
            }
        }
    }
    Ok(conditions)
}

/// Records the `.when(Cond)` calls of a rule against the rule just pushed.
pub(crate) fn push_rule_guards(
    model: &mut SemanticModel,
    rule: RuleRef,
    conditions: &[(ConditionKey, Span)],
    ctx: DiagCtxt,
) {
    for (condition, span) in conditions {
        model.rule_guards.push(RuleGuard {
            rule,
            condition: *condition,
            site: Some(ctx.locate(span.clone())),
        });
    }
}

/// A body comparison as an entity-local rule reads it, or `None` when it names another
/// entity.
fn local_condition(cond: &CrossEntityCondition, entity: EntityKey) -> Option<LocalCondition> {
    match cond {
        CrossEntityCondition::Equals { column, value } if column.entity == entity => {
            Some(LocalCondition::Equals(column.column.clone(), value.clone()))
        }
        CrossEntityCondition::Comparison(prop) if prop.lhs.entity == entity => {
            let rhs = match &prop.rhs {
                CrossCmpRhs::Column(c) if c.entity == entity => CmpRhs::Column(c.column.clone()),
                CrossCmpRhs::Column(_) => return None,
                CrossCmpRhs::IntLit(n) => CmpRhs::IntLit(*n),
                CrossCmpRhs::Now => CmpRhs::Now,
                CrossCmpRhs::Value(v) => CmpRhs::Value(v.clone()),
                CrossCmpRhs::Values(vs) => CmpRhs::Values(vs.clone()),
            };
            Some(LocalCondition::Comparison(ComparisonProp {
                lhs_column: prop.lhs.column.clone(),
                op: prop.op,
                rhs,
            }))
        }
        _ => None,
    }
}

enum LocalCondition {
    Equals(String, EffectValue),
    Comparison(ComparisonProp),
}

/// The entity a body comparison names first.
fn first_entity(cond: &CrossEntityCondition) -> EntityKey {
    match cond {
        CrossEntityCondition::Equals { column, .. } => column.entity,
        CrossEntityCondition::Comparison(prop) => prop.lhs.entity,
    }
}

/// After every predicate is processed: each `.when(Cond)` on a rule reads the condition's
/// `holds` body as if its comparisons were written in that `.when(...)` (an invariant) or
/// among the rule's arguments (a `forbidden`). A condition with no body, one whose body has a
/// `.has/.none`, `.along` or `.role`, or one that compares an entity an entity-local rule does
/// not read, is an error, and the rule is left as written.
pub(crate) fn resolve_rule_guards(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let guards = model.rule_guards.clone();
    for g in &guards {
        let (pred, subject) = match g.rule {
            RuleRef::Forbidden(i) => ("forbidden", vec![model.forbidden_constraints[i].entity]),
            RuleRef::Invariant(i) => ("invariant", vec![model.entity_invariants[i].entity]),
            RuleRef::CrossForbidden(i) => (
                "forbidden",
                model.cross_forbidden_constraints[i].scope.clone(),
            ),
            RuleRef::CrossInvariant(i) => {
                ("invariant", model.cross_entity_invariants[i].scope.clone())
            }
            RuleRef::Quantifier(i) => ("invariant", vec![model.quantifier_constraints[i].anchor]),
        };
        let condition = &model.conditions[g.condition];
        let id = condition.id.clone();
        let report = |diags: &mut Vec<Diagnostic>, err: RdraError| {
            diags.push(match &g.site {
                Some(site) => Diagnostic::error_at(err, site.clone()),
                None => Diagnostic::error(err),
            });
        };
        let Some(body) = &condition.body else {
            let subject: Vec<&str> = subject
                .iter()
                .map(|e| model.entities[*e].id.as_str())
                .collect();
            report(
                diags,
                RdraError::WhenConditionWithoutBody {
                    pred: pred.to_string(),
                    subject: subject.join(", "),
                    condition: id,
                },
            );
            continue;
        };
        let chained = !body.quantifiers.is_empty()
            || !body.by.is_empty()
            || !matches!(body.along, CrossConstraintScope::GlobalProduct);
        if chained {
            report(
                diags,
                RdraError::RuleWhenConditionScope {
                    pred: pred.to_string(),
                    condition: id,
                    reason: "a rule's `.when` reads the comparisons of a body, and this body also has `.has(...)`, `.none(...)`, `.along(...)` or `.role(...)`".to_string(),
                    fix: "write the quantifier, the path or the key on the rule itself, after its `.when(...)`".to_string(),
                },
            );
            continue;
        }
        let conds = body.conds.clone();
        match g.rule {
            RuleRef::Forbidden(_) | RuleRef::Invariant(_) => {
                let entity = subject[0];
                let mut equals = Vec::new();
                let mut comparisons = Vec::new();
                let mut outside = None;
                for cond in &conds {
                    match local_condition(cond, entity) {
                        Some(LocalCondition::Equals(column, value)) => equals.push((column, value)),
                        Some(LocalCondition::Comparison(prop)) => comparisons.push(prop),
                        None => {
                            outside = Some(first_entity(cond));
                            break;
                        }
                    }
                }
                if let Some(other) = outside {
                    let (this, other) = (
                        model.entities[entity].id.clone(),
                        model.entities[other].id.clone(),
                    );
                    report(
                        diags,
                        RdraError::RuleWhenConditionScope {
                            pred: pred.to_string(),
                            condition: id,
                            reason: format!(
                                "its body compares {other}, and `{pred}({this}, ...)` reads {this} only"
                            ),
                            fix: format!(
                                "list the entities first, `{pred}({this}, {other}, ...)`, with `.along({this}, {other})` to read the {other} row the {this} belongs to"
                            ),
                        },
                    );
                    continue;
                }
                match g.rule {
                    RuleRef::Forbidden(i) => {
                        let rule = &mut model.forbidden_constraints[i];
                        rule.conditions.extend(equals);
                        rule.comparisons.extend(comparisons);
                    }
                    RuleRef::Invariant(i) => {
                        let rule = &mut model.entity_invariants[i];
                        rule.guards.extend(equals);
                        rule.guard_comparisons.extend(comparisons);
                    }
                    _ => unreachable!("matched above"),
                }
            }
            RuleRef::CrossForbidden(i) => {
                let rule = &mut model.cross_forbidden_constraints[i];
                add_condition_entities_to_scope(&mut rule.scope, &conds);
                rule.conditions.extend(conds);
            }
            RuleRef::CrossInvariant(i) => {
                let rule = &mut model.cross_entity_invariants[i];
                add_condition_entities_to_scope(&mut rule.scope, &conds);
                rule.guards.extend(conds);
            }
            RuleRef::Quantifier(i) => model.quantifier_constraints[i].guards.extend(conds),
        }
    }
}

#[derive(Default)]
pub(crate) struct EntityConditions {
    pub(crate) equals: Vec<(String, EffectValue)>,
    pub(crate) comparisons: Vec<ComparisonProp>,
}

pub(crate) fn resolve_entity_equals_from_comparison(
    variations: &SlotMap<VariationKey, Variation>,
    entity_cols: &[ModelColumn],
    entity_id: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<(String, EffectValue)>, ()> {
    if cmp.op != CmpOp::Eq {
        return Ok(None);
    }
    let col_str = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = super::qref_util::qref_id(&qcol.entity) else {
                return Ok(None);
            };
            if q_entity != entity_id {
                return Ok(None);
            }
            qcol.column.clone()
        }
        _ => return Ok(None),
    };
    let Some(col) = entity_cols.iter().find(|c| c.name == col_str).cloned() else {
        push_error(
            ctx,
            diags,
            cmp.span.clone(),
            RdraError::UnknownColumn {
                entity: entity_id.to_string(),
                col: col_str,
            },
        );
        return Err(());
    };
    let value_lit = match &cmp.rhs {
        Operand::Column(name) => {
            let qualified = |name: &str| {
                entity_cols
                    .iter()
                    .any(|c| c.name == name)
                    .then(|| format!("{entity_id}.{name}"))
            };
            return match resolve_bare_rhs(variations, &col, &cmp.op, name, qualified) {
                Ok(BareRhs::Value(value)) => Ok(Some((col_str, value))),
                Ok(BareRhs::Column) => Ok(None),
                Err(e) => {
                    push_error_cmp(ctx, diags, cmp, e);
                    Err(())
                }
            };
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) | Operand::Values(_) => return Ok(None),
    };
    match parse_effect_value(variations, &col, &value_lit) {
        Ok(value) => Ok(Some((col_str, value))),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            Err(())
        }
    }
}

pub(crate) fn collect_entity_conditions(
    variations: &SlotMap<VariationKey, Variation>,
    entity_cols: &[ModelColumn],
    entity_id: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityConditions> {
    let mut conditions = EntityConditions::default();
    for arg in args {
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = arg {
            match resolve_entity_equals_from_comparison(
                variations,
                entity_cols,
                entity_id,
                cmp,
                ctx,
                diags,
            ) {
                Ok(Some(condition)) => conditions.equals.push(condition),
                Ok(None) => {
                    if let Some(prop) =
                        resolve_comparison(variations, entity_cols, entity_id, cmp, ctx, diags)
                    {
                        conditions.comparisons.push(prop);
                    }
                }
                Err(()) => return None,
            }
        }
    }
    Some(conditions)
}

pub(crate) fn context_value_from_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    expected_kind: &str,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<BusinessMappingContextValue> {
    match arg {
        PredicateArg::Lit(s) => Some(BusinessMappingContextValue::Text(s.clone())),
        PredicateArg::Ref(_) => {
            let node = resolve_arg(model, arg, ctx, diags)?;
            let actual = node.kind_tag();
            if actual != expected_kind {
                push_error(
                    ctx,
                    diags,
                    arg_span(arg),
                    RdraError::TypeMismatch {
                        pred: "belongs context".to_string(),
                        id: context_arg_id(arg),
                        actual: actual.to_string(),
                        expected: expected_kind.to_string(),
                    },
                );
                return None;
            }
            Some(BusinessMappingContextValue::Ref(node))
        }
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Expr(_) => None,
    }
}

fn context_arg_id(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Transition { .. } => "<transition>".to_string(),
        PredicateArg::Card(c) => c.clone(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
    }
}

// ── クロスエンティティ制約ヘルパー ───────────────────────────────────────────

pub(crate) fn push_unique_entity(scope: &mut Vec<EntityKey>, entity: EntityKey) {
    if !scope.contains(&entity) {
        scope.push(entity);
    }
}

fn condition_entities(cond: &CrossEntityCondition, out: &mut Vec<EntityKey>) {
    match cond {
        CrossEntityCondition::Equals { column, .. } => push_unique_entity(out, column.entity),
        CrossEntityCondition::Comparison(prop) => {
            push_unique_entity(out, prop.lhs.entity);
            if let CrossCmpRhs::Column(col) = &prop.rhs {
                push_unique_entity(out, col.entity);
            }
        }
    }
}

pub(crate) fn resolve_entity_qref(
    model: &SemanticModel,
    pred: &str,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    let id = qref.parts.last()?.clone();
    if let Some(kind) = &qref.kind_qualifier {
        if kind != &Kind::Entity {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: qref_display(qref),
                    actual: kind.name().to_string(),
                    expected: "entity".to_string(),
                },
            );
            return None;
        }
        return match model.symbols.lookup_qualified(kind, &id).cloned() {
            Some(NodeRef::Entity(k)) => Some(k),
            _ => {
                push_error_qref(
                    ctx,
                    diags,
                    qref,
                    RdraError::UndefinedSymbol {
                        id: format!("entity::{}", id),
                        import: None,
                    },
                );
                None
            }
        };
    }

    if let Some(NodeRef::Entity(k)) = model.symbols.lookup_qualified(&Kind::Entity, &id).cloned() {
        return Some(k);
    }

    match model.symbols.lookup(&id) {
        LookupResult::Found(node) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id,
                    actual: node.kind_tag().to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        LookupResult::NotFound => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::UndefinedSymbol { id, import: None },
            );
            None
        }
        LookupResult::Ambiguous(kinds) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::AmbiguousReference {
                    id,
                    kinds: kinds.join(", "),
                },
            );
            None
        }
    }
}

fn resolve_entity_scope_arg(
    model: &SemanticModel,
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    match arg {
        PredicateArg::Ref(qref) => resolve_entity_qref(model, pred, qref, ctx, diags),
        PredicateArg::Lit(s) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: s.clone(),
                    actual: "literal".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Transition { .. } | PredicateArg::Card(_) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<arg>".to_string(),
                    actual: "argument".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            push_error_cmp(
                ctx,
                diags,
                cmp,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<expr>".to_string(),
                    actual: "expression".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Expr(_) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<expr>".to_string(),
                    actual: "expression".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
    }
}

pub(crate) fn find_entity_column(
    model: &SemanticModel,
    entity: EntityKey,
    column: &str,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<ModelColumn> {
    let entity_id = model.entities[entity].id.clone();
    match model.entities[entity]
        .columns
        .iter()
        .find(|c| c.name == column)
        .cloned()
    {
        Some(col) => Some(col),
        None => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::UnknownColumn {
                    entity: entity_id,
                    col: column.to_string(),
                },
            );
            None
        }
    }
}

fn resolve_cross_operand_column(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    operand: &Operand,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(QualifiedModelColumnRef, ModelColumn)> {
    match operand {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col = find_entity_column(model, entity, column, span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            ))
        }
        Operand::Column(column) => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::CrossConstraintColumnNeedsEntity {
                    column: column.clone(),
                    example: format!("Entity.{}", column),
                },
            );
            None
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags)?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            ))
        }
        Operand::IntLit(_) | Operand::Now | Operand::Values(_) => None,
    }
}

fn resolve_cross_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossComparisonProp> {
    let (lhs, lhs_col) = match resolve_cross_operand_column(
        model,
        scope,
        pred,
        &cmp.lhs,
        cmp.span.clone(),
        ctx,
        diags,
    ) {
        Some(v) => v,
        None => {
            push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
            return None;
        }
    };
    if let Operand::Values(names) = &cmp.rhs {
        return match resolve_set_values(&model.variations, &lhs_col, names) {
            Ok(values) => Some(CrossComparisonProp {
                lhs,
                op: to_model_op(&cmp.op),
                rhs: CrossCmpRhs::Values(values),
            }),
            Err(err) => {
                push_error_cmp(ctx, diags, cmp, err);
                None
            }
        };
    }

    let lhs_cat = type_category(&lhs_col.col_type);

    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        push_error_cmp(
            ctx,
            diags,
            cmp,
            RdraError::ComparisonOpNotOrdered {
                col: lhs.column.clone(),
                col_type: format!("{:?}", lhs_col.col_type),
                op: cmp.op.as_str().to_string(),
            },
        );
        return None;
    }

    if let Operand::Column(name) = &cmp.rhs {
        let qualified = |name: &str| cross_column_named(model, lhs.entity, scope, name);
        match resolve_bare_rhs(&model.variations, &lhs_col, &cmp.op, name, qualified) {
            Ok(BareRhs::Value(value)) => {
                return Some(CrossComparisonProp {
                    lhs,
                    op: to_model_op(&cmp.op),
                    rhs: CrossCmpRhs::Value(value),
                });
            }
            Ok(BareRhs::Column) => {}
            Err(e) => {
                push_error_cmp(ctx, diags, cmp, e);
                return None;
            }
        }
    }

    let rhs = match &cmp.rhs {
        Operand::Column(_) | Operand::QualifiedColumn(_) => {
            let (rhs_ref, rhs_col) = resolve_cross_operand_column(
                model,
                scope,
                pred,
                &cmp.rhs,
                cmp.span.clone(),
                ctx,
                diags,
            )?;
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_ref.column.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Column(rhs_ref)
        }
        Operand::IntLit(s) => {
            if lhs_cat != "numeric" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: s.clone(),
                        rhs_type: "integer_literal".to_string(),
                    },
                );
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CrossCmpRhs::IntLit(n),
                Err(_) => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                    );
                    return None;
                }
            }
        }
        Operand::Now => {
            if lhs_cat != "temporal" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs.column.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Now
        }
        Operand::Values(_) => unreachable!("a set is resolved above"),
    };

    Some(CrossComparisonProp {
        lhs,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

pub(crate) fn resolve_cross_condition(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    match arg {
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            match temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags) {
                Ok(Some(cond)) => Some(cond),
                Ok(None) => resolve_cross_comparison(model, scope, pred, cmp, ctx, diags)
                    .map(CrossEntityCondition::Comparison),
                Err(()) => None,
            }
        }
        _ => None,
    }
}

fn collect_cross_scope_prefix(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<EntityKey>, usize) {
    let mut scope = Vec::new();
    let mut first_condition = pred.args.len();
    for (idx, arg) in pred.args.iter().enumerate() {
        if matches!(arg, PredicateArg::Expr(_)) {
            first_condition = idx;
            break;
        }
        match arg {
            PredicateArg::Ref(_) => {
                if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
                    push_unique_entity(&mut scope, entity);
                }
            }
            _ => {
                first_condition = idx;
                break;
            }
        }
    }
    (scope, first_condition)
}

fn collect_cross_chain_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    for arg in args {
        if let Some(cond) = resolve_cross_condition(model, scope, pred, arg, ctx, diags) {
            conditions.push(cond);
        }
    }
    conditions
}

pub(crate) fn add_condition_entities_to_scope(
    scope: &mut Vec<EntityKey>,
    conditions: &[CrossEntityCondition],
) {
    for cond in conditions {
        condition_entities(cond, scope);
    }
}

fn collect_cross_along_path(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(usize, Vec<EntityKey>)> {
    let at = pred.chain.iter().position(|cc| cc.name == "along")?;
    let mut path = Vec::new();
    for arg in &pred.chain[at].args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            path.push(entity);
        }
    }
    Some((at, path))
}

/// The scope of a cross-entity `forbidden` or `invariant`. A `.role(..)` that follows no
/// `.along`, `.has` or `.none` is reported here.
pub(crate) fn cross_scope_semantics_from_chain(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    report_leading_by(&pred.chain, ctx, diags);
    along_scope(model, pred, ctx, diags)
}

/// The `.along(..)` path of a statement and the selectors of its hops, or `GlobalProduct`
/// without one.
pub(crate) fn along_scope(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    match collect_cross_along_path(model, pred, ctx, diags) {
        Some((at, path)) => CrossConstraintScope::RelationPath {
            path,
            by: selectors(&pred.chain, Some(at)),
            links: Vec::new(),
        },
        None => CrossConstraintScope::GlobalProduct,
    }
}

pub(crate) fn process_forbidden(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let (mut scope, first_condition) = collect_cross_scope_prefix(model, pred, ctx, diags);
    let mut conditions = Vec::new();
    for arg in pred.args.iter().skip(first_condition) {
        if let Some(cond) = resolve_cross_condition(model, &scope, &pred.name, arg, ctx, diags) {
            conditions.push(cond);
        }
    }
    let Ok(when) = forbidden_when_conditions(model, pred, ctx, diags) else {
        return;
    };

    if conditions.is_empty() && when.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &conditions);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    let label = rule_label(pred, ctx, diags);
    model
        .cross_forbidden_constraints
        .push(CrossForbiddenConstraint {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            scope: scope.clone(),
            scope_semantics,
            conditions,
        });
    let rule = RuleRef::CrossForbidden(model.cross_forbidden_constraints.len() - 1);
    push_rule_guards(model, rule, &when, ctx);
    model
        .typed_predicates
        .push(TypedPredicate::CrossForbidden { scope });
}

pub(crate) fn process_invariant(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let mut scope = Vec::new();
    for arg in &pred.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            push_unique_entity(&mut scope, entity);
        }
    }

    let mut guards = Vec::new();
    // `.when(Cond)` calls so far: their bodies join the guards after every predicate is read.
    let mut when: Vec<(ConditionKey, Span)> = Vec::new();
    let mut requireds = Vec::new();
    let mut quantified = false;
    let label = rule_label(pred, ctx, diags);
    let along = along_scope(model, pred, ctx, diags);
    for (at, cc) in pred.chain.iter().enumerate() {
        match cc.name.as_str() {
            "when" => match when_condition_ref(model, &pred.name, cc, ctx, diags) {
                Ok(Some(ck)) => when.push((ck, cc.span.clone())),
                Ok(None) => guards.extend(collect_cross_chain_conditions(
                    model, &scope, &pred.name, &cc.args, ctx, diags,
                )),
                Err(()) => return,
            },
            "then" => requireds.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "has" | "none" => {
                quantified = true;
                // The anchor is the first entity the scope names. An empty scope is a
                // leading entity that did not resolve, which is reported.
                let Some(&anchor) = scope.first() else {
                    continue;
                };
                let by = selectors(&pred.chain, Some(at));
                let Some(quantifier) =
                    resolve_related_quantifier(model, anchor, cc, by, ctx, diags)
                else {
                    continue;
                };
                model
                    .quantifier_constraints
                    .push(crate::model::QuantifierConstraint {
                        label: label.clone(),
                        anchor,
                        guards: guards.clone(),
                        quantifier,
                        along: along.clone(),
                    });
                let rule = RuleRef::Quantifier(model.quantifier_constraints.len() - 1);
                push_rule_guards(model, rule, &when, ctx);
            }
            _ => {}
        }
    }

    report_leading_by(&pred.chain, ctx, diags);
    if requireds.is_empty() {
        if !quantified {
            let err = RdraError::InvariantWithoutConsequence;
            push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        }
        return;
    }
    if guards.is_empty() && when.is_empty() {
        let err = RdraError::InvariantWithoutWhen;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &guards);
    add_condition_entities_to_scope(&mut scope, &requireds);
    let scope_semantics = along;
    model.cross_entity_invariants.push(CrossEntityInvariant {
        label,
        site: Some(ctx.locate(pred.span.clone())),
        scope: scope.clone(),
        scope_semantics,
        guards,
        requireds,
    });
    let rule = RuleRef::CrossInvariant(model.cross_entity_invariants.len() - 1);
    push_rule_guards(model, rule, &when, ctx);
    model
        .typed_predicates
        .push(TypedPredicate::CrossInvariant { scope });
}

/// The related entity, conditions and kind of one `.has(...)` / `.none(...)` chain call. The
/// related entity is the one a bare first argument names (`.none(Permit)`, any row at all), or
/// else the first one the conditions name other than `anchor`. `by` holds the `.role(role)`
/// selectors that follow the call. `None` only with an error reported, so a quantifier is
/// never dropped silently.
pub(crate) fn resolve_related_quantifier(
    model: &SemanticModel,
    anchor: EntityKey,
    cc: &ChainCall,
    by: Vec<String>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<crate::model::RelatedQuantifier> {
    let before = diags.len();
    let quantifier = related_quantifier(model, anchor, cc, by, ctx, diags);
    if quantifier.is_none() {
        let err = RdraError::QuantifierWithoutCondition {
            method: cc.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, cc.span.clone(), err);
    }
    quantifier
}

/// The `.along` path of a `.has`/`.none` invariant chains the counted rows to the anchor's
/// when it runs from the anchor to the counted entity, each entity once. Any other shape is
/// `QuantifierAlongShape`, with the path rewritten as the shortest key path from the anchor to
/// the counted entity, or, with no key path, from the anchor to the counted entity through the
/// other path entities in order. Runs once the keys exist; a misshapen rule is dropped.
pub(crate) fn check_quantifier_paths(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let constraints = std::mem::take(&mut model.quantifier_constraints);
    for c in constraints {
        match quantifier_path_misfit(model, &c) {
            None => model.quantifier_constraints.push(c),
            Some(err) => diags.push(match &c.quantifier.site {
                Some(site) => Diagnostic::error_at(err, site.clone()),
                None => Diagnostic::error(err),
            }),
        }
    }
}

fn quantifier_path_misfit(
    model: &SemanticModel,
    c: &crate::model::QuantifierConstraint,
) -> Option<RdraError> {
    let CrossConstraintScope::RelationPath { path, .. } = &c.along else {
        return None;
    };
    let (anchor, q) = (c.anchor, &c.quantifier);
    let mut seen: Vec<EntityKey> = Vec::new();
    for ek in path {
        push_unique_entity(&mut seen, *ek);
    }
    let each_once = seen.len() == path.len();
    if path.len() >= 2 && path[0] == anchor && path[path.len() - 1] == q.related && each_once {
        return None;
    }
    let id = |ek: &EntityKey| model.entities[*ek].id.clone();
    let fix = model.key_path(&[anchor], q.related).unwrap_or_else(|| {
        let mut fix = vec![anchor];
        for ek in path {
            if *ek != anchor && *ek != q.related {
                push_unique_entity(&mut fix, *ek);
            }
        }
        fix.push(q.related);
        fix
    });
    Some(RdraError::QuantifierAlongShape {
        method: match q.kind {
            crate::model::QuantifierKind::Has => "has".to_string(),
            crate::model::QuantifierKind::None => "none".to_string(),
        },
        ends: format!("{} to {}", id(&anchor), id(&q.related)),
        path: path.iter().map(id).collect::<Vec<_>>().join(", "),
        fix: fix.iter().map(id).collect::<Vec<_>>().join(", "),
    })
}

fn related_quantifier(
    model: &SemanticModel,
    anchor: EntityKey,
    cc: &ChainCall,
    by: Vec<String>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<crate::model::RelatedQuantifier> {
    let kind = match cc.name.as_str() {
        "has" => crate::model::QuantifierKind::Has,
        "none" => crate::model::QuantifierKind::None,
        _ => return None,
    };
    // `.none(Permit)` and `.none(Permit, Permit.status == x)` name the related entity first;
    // `.none(Permit.status == x)` names it in the conditions.
    let (related, related_args) = match cc.args.first() {
        Some(PredicateArg::Ref(_)) => {
            let related = resolve_entity_scope_arg(model, &cc.name, &cc.args[0], ctx, diags)?;
            (related, &cc.args[1..])
        }
        _ => {
            let mut related_scope = Vec::new();
            let preview = collect_cross_chain_conditions(
                model,
                &related_scope,
                &cc.name,
                &cc.args,
                ctx,
                diags,
            );
            add_condition_entities_to_scope(&mut related_scope, &preview);
            let related = related_scope
                .iter()
                .copied()
                .find(|e| *e != anchor)
                .or_else(|| related_scope.first().copied())?;
            (related, cc.args.as_slice())
        }
    };

    let related_scope = vec![related];
    let related_conditions =
        collect_cross_chain_conditions(model, &related_scope, &cc.name, related_args, ctx, diags);
    if related_conditions.is_empty() && !related_args.is_empty() {
        return None;
    }
    Some(crate::model::RelatedQuantifier {
        kind,
        related,
        conditions: related_conditions,
        by,
        site: Some(ctx.locate(cc.span.clone())),
    })
}

fn temporal_equals_from_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<CrossEntityCondition>, ()> {
    if cmp.op != CmpOp::Eq {
        return Ok(None);
    }

    let (column, model_col) = match &cmp.lhs {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col =
                find_entity_column(model, entity, column, cmp.span.clone(), ctx, diags).ok_or(())?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            )
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags).ok_or(())?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)
                    .ok_or(())?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            )
        }
        _ => return Ok(None),
    };

    let value_lit = match &cmp.rhs {
        Operand::Column(name) => {
            let qualified = |name: &str| cross_column_named(model, column.entity, scope, name);
            return match resolve_bare_rhs(&model.variations, &model_col, &cmp.op, name, qualified) {
                Ok(BareRhs::Value(value)) => {
                    Ok(Some(CrossEntityCondition::Equals { column, value }))
                }
                Ok(BareRhs::Column) => Ok(None),
                Err(e) => {
                    push_error_cmp(ctx, diags, cmp, e);
                    Err(())
                }
            };
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) | Operand::Values(_) => return Ok(None),
    };
    match parse_effect_value(&model.variations, &model_col, &value_lit) {
        Ok(value) => Ok(Some(CrossEntityCondition::Equals { column, value })),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            Err(())
        }
    }
}

/// `Entity.name` for the column `name` a bare right-hand side could mean: one of the
/// left-hand entity's, else one of an entity in scope.
fn cross_column_named(
    model: &SemanticModel,
    lhs_entity: EntityKey,
    scope: &[EntityKey],
    name: &str,
) -> Option<String> {
    std::iter::once(lhs_entity)
        .chain(scope.iter().copied())
        .find(|ek| model.entities[*ek].columns.iter().any(|c| c.name == name))
        .map(|ek| format!("{}.{name}", model.entities[ek].id))
}

fn collect_temporal_assert_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    for arg in args {
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = arg {
            match temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags) {
                Ok(Some(cond)) => conditions.push(cond),
                Ok(None) => {
                    if let Some(cond) = resolve_cross_condition(model, scope, pred, arg, ctx, diags)
                    {
                        conditions.push(cond);
                    }
                }
                Err(()) => {}
            }
        }
    }
    conditions
}

pub(crate) fn process_after_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::UseCase(anchor))) = resolved.first() else {
        return;
    };

    let before = diags.len();
    let mut scope = Vec::new();
    let mut requireds = Vec::new();
    for cc in &pred.chain {
        if cc.name == "assert" {
            requireds.extend(collect_temporal_assert_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            ));
        }
    }
    if requireds.is_empty() {
        let err = RdraError::AfterWithoutAssert {
            usecase: model.use_cases[*anchor].id.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &requireds);
    let label = rule_label(pred, ctx, diags);
    model
        .temporal_assertions
        .push(crate::model::TemporalAssertion {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            anchor: *anchor,
            scope,
            requireds,
        });
    model
        .typed_predicates
        .push(TypedPredicate::After { anchor: *anchor });
}
