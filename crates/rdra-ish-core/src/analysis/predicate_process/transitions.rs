//! The `transitions` and `chooses` predicates and the `.when(Cond)` chain they share.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::predicate_arg_display;
use crate::analysis::arg_resolve::resolve_arg;

pub(super) fn process_transitions_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let guard = when_condition(model, pred, ctx, diags);
    let Some(Some(NodeRef::UseCase(usecase))) = resolved.first() else {
        return;
    };
    let Ok(guard) = guard else {
        return;
    };

    let (entity_id, column) = match pred.args.get(1) {
        Some(PredicateArg::Ref(qref)) if qref.parts.len() == 2 => {
            (qref.parts[0].clone(), qref.parts[1].clone())
        }
        Some(arg) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::InvalidTransitionsColumnRef {
                    got: predicate_arg_display(arg),
                },
            );
            return;
        }
        None => {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::InvalidTransitionsColumnRef {
                    got: "<missing>".into(),
                },
            );
            return;
        }
    };

    let (froms, to) = match pred.args.get(2) {
        Some(PredicateArg::Transition { from, to }) => (from.clone(), to.clone()),
        Some(arg) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: "transitions".into(),
                    id: predicate_arg_display(arg),
                    actual: "value".into(),
                    expected: "from -> to".into(),
                },
            );
            return;
        }
        None => {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::TypeMismatch {
                    pred: "transitions".into(),
                    id: "<missing>".into(),
                    actual: "missing".into(),
                    expected: "from -> to".into(),
                },
            );
            return;
        }
    };

    let entity_key = model
        .entities
        .iter()
        .find(|(_, e)| e.id == entity_id)
        .map(|(k, _)| k);
    let Some(entity_key) = entity_key else {
        push_error(
            ctx,
            diags,
            arg_span(&pred.args[1]),
            RdraError::UndefinedSymbol {
                id: entity_id.clone(),
                import: None,
            },
        );
        return;
    };

    let col = model.entities[entity_key]
        .columns
        .iter()
        .find(|c| c.name == column)
        .cloned();
    let Some(col) = col else {
        push_error(
            ctx,
            diags,
            arg_span(&pred.args[1]),
            RdraError::UnknownColumn {
                entity: entity_id.clone(),
                col: column.clone(),
            },
        );
        return;
    };
    let ColumnType::Enum(variants) = &col.col_type else {
        push_error(
            ctx,
            diags,
            arg_span(&pred.args[1]),
            RdraError::TransitionsColumnNotEnum {
                entity: entity_id.clone(),
                col: column.clone(),
            },
        );
        return;
    };

    // Normalize to declared enum casing.
    let declared = |variant: &str| {
        variants
            .iter()
            .find(|v| *v == variant || v.eq_ignore_ascii_case(variant))
            .cloned()
    };
    let mut arms = Vec::new();
    for variant in froms.iter().chain(std::iter::once(&to)) {
        let Some(value) = declared(variant) else {
            push_error(
                ctx,
                diags,
                arg_span(&pred.args[2]),
                RdraError::InvalidEnumVariant {
                    col: column.clone(),
                    value: variant.clone(),
                    allowed: variants.join(", "),
                },
            );
            return;
        };
        arms.push(value);
    }
    let to = arms.pop().unwrap_or(to);

    // `(a, b) -> z` is one arm per source, each as if written on its own line.
    for from in arms {
        model.state_transitions.push(crate::model::StateTransition {
            site: Some(ctx.locate(pred.span.clone())),
            usecase: *usecase,
            entity: entity_key,
            column: column.clone(),
            from: from.clone(),
            to: to.clone(),
            guard,
        });
        model.typed_predicates.push(TypedPredicate::Transitions {
            entity: entity_key,
            column: column.clone(),
            usecase: *usecase,
            from,
            to: to.clone(),
        });
    }
}

/// `chooses(UseCase, Entity.column)` on a column typed by a variation, or
/// `chooses(UseCase, Entity.column, value)` with an optional `.when(Cond)`: one listed value.
pub(super) fn process_chooses_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let guard = when_condition(model, pred, ctx, diags);
    let Some(Some(NodeRef::UseCase(usecase))) = resolved.first() else {
        return;
    };
    let usecase = *usecase;
    let Ok(guard) = guard else {
        return;
    };
    let (entity_id, column) = match pred.args.get(1) {
        Some(PredicateArg::Ref(qref)) if qref.parts.len() == 2 && qref.kind_qualifier.is_none() => {
            (qref.parts[0].clone(), qref.parts[1].clone())
        }
        Some(arg) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::InvalidChoosesColumnRef {
                    got: predicate_arg_display(arg),
                },
            );
            return;
        }
        None => return,
    };
    let span = arg_span(&pred.args[1]);
    let Some((entity, decl)) = model.entities.iter().find(|(_, e)| e.id == entity_id) else {
        push_error(
            ctx,
            diags,
            span,
            RdraError::UndefinedSymbol {
                id: entity_id,
                import: None,
            },
        );
        return;
    };
    let Some(col) = decl.columns.iter().find(|c| c.name == column) else {
        push_error(
            ctx,
            diags,
            span,
            RdraError::UnknownColumn {
                entity: entity_id,
                col: column,
            },
        );
        return;
    };
    let hint = match &col.col_type {
        ColumnType::Variation(_) => None,
        ColumnType::Enum(_) => Some(format!(
            "an inline Enum is a state model: let the actor pick the next state with several `transitions({uc}, {entity_id}.{column}, from -> to)` arms from one state, or type the column by a `variation`",
            uc = model.use_cases[usecase].id
        )),
        ColumnType::Bool => Some(format!(
            "write `sets({uc}, {entity_id}.{column} == true)` for a fixed value, or type the column by a `variation` whose values the actor picks",
            uc = model.use_cases[usecase].id
        )),
        _ => Some("declare a `variation` with the values the actor picks and type the column by it".to_string()),
    };
    if let Some(hint) = hint {
        push_error(
            ctx,
            diags,
            span,
            RdraError::ChoosesNotVariation {
                usecase: model.use_cases[usecase].id.clone(),
                entity: entity_id,
                col: column,
                hint,
            },
        );
        return;
    }
    let ColumnType::Variation(variation) = &col.col_type else {
        return;
    };
    let value = match pred.args.get(2) {
        None => None,
        Some(arg) => {
            let written = match arg {
                PredicateArg::Ref(q) if q.parts.len() == 1 && q.kind_qualifier.is_none() => {
                    q.parts[0].clone()
                }
                // A value is a token; a quoted string is free text (#48).
                PredicateArg::Lit(value) => {
                    push_error(
                        ctx,
                        diags,
                        pred.span.clone(),
                        RdraError::QuotedValue {
                            place: "chooses".into(),
                            value: value.clone(),
                            rewrite: format!(
                                "chooses({}, {entity_id}.{column}, {value})",
                                model.use_cases[usecase].id
                            ),
                        },
                    );
                    return;
                }
                other => predicate_arg_display(other),
            };
            let variation = &model.variations[*variation];
            if !variation.values.contains(&written) {
                push_error(
                    ctx,
                    diags,
                    arg_span(arg),
                    RdraError::InvalidVariationValue {
                        variation: variation.id.clone(),
                        value: written,
                        allowed: variation.values.join(", "),
                    },
                );
                return;
            }
            Some(written)
        }
    };
    let site = Some(ctx.locate(pred.span.clone()));
    let chosen = match value {
        Some(value) => Some(crate::model::ChosenValue {
            value,
            guard,
            site: site.clone(),
        }),
        None if guard.is_some() => {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::ChoosesWhenWithoutValue {
                    usecase: model.use_cases[usecase].id.clone(),
                    entity: entity_id,
                    col: column,
                },
            );
            return;
        }
        None => None,
    };
    let existing = model
        .column_choices
        .iter_mut()
        .find(|c| (c.origin, c.entity, c.column.as_str()) == (usecase, entity, column.as_str()));
    match existing {
        // One form per use case and column: listed values, or the one value-less line.
        Some(c) if c.values.is_empty() != chosen.is_none() => {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::ChoosesMixedForms {
                    usecase: model.use_cases[usecase].id.clone(),
                    entity: entity_id,
                    col: column,
                },
            );
            return;
        }
        Some(c) => c.values.extend(chosen),
        None => model.column_choices.push(crate::model::ColumnChoice {
            origin: usecase,
            entity,
            column: column.clone(),
            values: chosen.into_iter().collect(),
            site,
        }),
    }
    model.typed_predicates.push(TypedPredicate::Chooses {
        origin: usecase,
        entity,
        column,
    });
}

/// The condition of a `.when(Cond)` chain, at most one: `Ok(None)` without one, `Err` after
/// reporting a chain that names no condition. Whether the condition has a `holds` body is
/// checked once every predicate is read (`holds` may come later in the file).
pub(super) fn when_condition(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<ConditionKey>, ()> {
    let mut guard = Ok(None);
    for (index, cc) in pred.chain.iter().filter(|cc| cc.name == "when").enumerate() {
        let err = || RdraError::WhenRequiresCondition {
            pred: pred.name.clone(),
        };
        if index > 0 || cc.args.len() != 1 {
            push_error(ctx, diags, cc.span.clone(), err());
            guard = Err(());
            continue;
        }
        match resolve_arg(model, &cc.args[0], ctx, diags) {
            Some(NodeRef::Condition(ck)) => guard = Ok(Some(ck)),
            Some(_) => {
                push_error(ctx, diags, cc.span.clone(), err());
                guard = Err(());
            }
            // resolve_arg already reported an undefined or ambiguous reference.
            None if matches!(cc.args.first(), Some(PredicateArg::Ref(_))) => guard = Err(()),
            None => {
                push_error(ctx, diags, cc.span.clone(), err());
                guard = Err(());
            }
        }
    }
    guard
}
