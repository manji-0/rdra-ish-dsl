//! Predicate argument resolution and semantic processing.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::predicate::{
    predicate_chain_methods, predicate_signature, unknown_chain_method_hint, TRIGGER_PAIRS,
};
use rdra_ish_syntax::ast::*;

use super::arg_resolve::resolve_arg;
use super::comparison::resolve_comparison;
use super::constraint::{
    arg_as_str, collect_entity_conditions, context_value_from_arg, process_after_predicate,
    process_forbidden as process_cross_forbidden, process_invariant as process_cross_invariant,
    process_when_quantifier_predicate, resolve_entity_equals_from_comparison,
};
use super::effect::parse_effect_value;
use super::nodes::node_kind_tag_str;

fn resolve_predicate_args(
    model: &SemanticModel,
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<NodeRef>> {
    pred.args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let Some(kinds) = sig.get(i) else {
                return if matches!(
                    pred.name.as_str(),
                    "forbidden"
                        | "required"
                        | "exclusive"
                        | "sets"
                        | "invariant"
                        | "when"
                        | "after"
                        | "holds"
                ) {
                    None
                } else {
                    resolve_arg(model, arg, ctx, diags)
                };
            };
            if matches!(
                kinds.as_slice(),
                ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"]
            ) {
                return None;
            }
            resolve_arg(model, arg, ctx, diags)
        })
        .collect()
}

pub(super) fn predicate_arg_display(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
        PredicateArg::Transition { from, to } => format!("{from} -> {to}"),
        PredicateArg::Card(c) => c.clone(),
    }
}

/// Check each resolved argument's kind. A local argument must match the signature; a foreign
/// stub must sit at a position `FOREIGN_OPERANDS` lists. `false` when a foreign stub was
/// rejected, so the predicate records nothing.
fn validate_predicate_arg_types(
    model: &SemanticModel,
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let mut accepted = true;
    for (i, expected_kinds) in sig.iter().enumerate() {
        if matches!(
            expected_kinds.as_slice(),
            ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"]
        ) {
            continue;
        }
        if let Some(Some(node)) = resolved.get(i) {
            if model.is_foreign(node) {
                accepted &= crate::binding::check_foreign_operand(
                    model,
                    &pred.name,
                    i,
                    node,
                    arg_span(&pred.args[i]),
                    ctx,
                    diags,
                );
                continue;
            }
            let actual = node_kind_tag_str(node);
            if !expected_kinds.contains(&actual) {
                push_error_arg(
                    ctx,
                    diags,
                    &pred.args,
                    i,
                    RdraError::TypeMismatch {
                        pred: pred.name.clone(),
                        id: predicate_arg_display(&pred.args[i]),
                        actual: actual.to_string(),
                        expected: expected_kinds.join("|"),
                    },
                );
            }
        }
    }
    accepted
}

fn validate_contains_pair(
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if pred.name != "contains" {
        return true;
    }
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        let valid = matches!(
            (from, to),
            (NodeRef::Buc(_), NodeRef::UseCase(_))
                | (NodeRef::Buc(_), NodeRef::Flow(_))
                | (NodeRef::Flow(_), NodeRef::Step(_))
                | (NodeRef::Screen(_), NodeRef::Field(_))
                | (NodeRef::System(_), NodeRef::Api(_))
        );
        if !valid {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::TypeMismatch {
                    pred: pred.name.clone(),
                    id: "contains pair".to_string(),
                    actual: format!("{} -> {}", node_kind_tag_str(from), node_kind_tag_str(to)),
                    expected: "buc->usecase|buc->flow|flow->step|screen->field|system->api"
                        .to_string(),
                },
            );
            return false;
        }
    }
    true
}

/// `triggers` accepts only the kind pairs in `TRIGGER_PAIRS`; the per-argument signature
/// cannot express that a timer has no external-system target.
fn validate_trigger_pair(
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if pred.name != "triggers" {
        return true;
    }
    let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) else {
        return true;
    };
    let pair = (node_kind_tag_str(from), node_kind_tag_str(to));
    // A kind outside the signature was already reported by `validate_predicate_arg_types`.
    let in_signature = sig[0].contains(&pair.0) && sig[1].contains(&pair.1);
    if TRIGGER_PAIRS.contains(&pair) || !in_signature {
        return true;
    }
    push_error(
        ctx,
        diags,
        pred.span.clone(),
        RdraError::TypeMismatch {
            pred: pred.name.clone(),
            id: "triggers pair".to_string(),
            actual: format!("{} -> {}", pair.0, pair.1),
            expected: TRIGGER_PAIRS
                .iter()
                .map(|(source, target)| format!("{source}->{target}"))
                .collect::<Vec<_>>()
                .join("|"),
        },
    );
    false
}

fn process_maps_field_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (Some(Some(NodeRef::Field(field))), Some(Some(NodeRef::Entity(entity)))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    let Some(column) = pred.args.get(2).and_then(arg_as_str) else {
        return;
    };

    let entity_id = model.entities[*entity].id.clone();
    if !model.entities[*entity]
        .columns
        .iter()
        .any(|col| col.name == column)
    {
        let span = pred
            .args
            .get(2)
            .map(arg_span)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| pred.span.clone());
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
    }

    model.field_mappings.push(FieldMapping {
        field: *field,
        entity: *entity,
        column,
    });
    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: NodeRef::Field(*field),
        to: NodeRef::Entity(*entity),
        kind: RelKind::MapsField,
        options: RelationOptions::default(),
        via: None,
    });
}

fn process_coordinates_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (Some(Some(usecase)), Some(Some(left)), Some(Some(right))) =
        (resolved.first(), resolved.get(1), resolved.get(2))
    {
        if let (NodeRef::UseCase(uk), NodeRef::Entity(left_ek), NodeRef::Entity(right_ek)) =
            (usecase, left, right)
        {
            model
                .boundary_coordinations
                .push(crate::model::BoundaryCoordination {
                    usecase: *uk,
                    left: *left_ek,
                    right: *right_ek,
                });
        }
    }
}

fn process_transitions_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::UseCase(usecase))) = resolved.get(1) else {
        return;
    };

    let (entity_id, column) = match pred.args.first() {
        Some(PredicateArg::Ref(qref)) if qref.parts.len() == 2 => {
            (qref.parts[0].clone(), qref.parts[1].clone())
        }
        Some(PredicateArg::Ref(qref))
            if qref.parts.len() == 3
                && crate::binding::bound_ext(model, &qref.parts[0]).is_some() =>
        {
            let entity = QRef {
                kind_qualifier: None,
                parts: qref.parts[..2].to_vec(),
                span: qref.span.clone(),
            };
            if let Some(Some(node)) = crate::binding::resolve_foreign(model, &entity, ctx, diags) {
                crate::binding::check_foreign_operand(
                    model,
                    "transitions",
                    0,
                    &node,
                    qref.span.clone(),
                    ctx,
                    diags,
                );
            }
            return;
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

    let (from, to) = match pred.args.get(2) {
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
            arg_span(&pred.args[0]),
            RdraError::UndefinedSymbol {
                id: entity_id.clone(),
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
            arg_span(&pred.args[0]),
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
            arg_span(&pred.args[0]),
            RdraError::TransitionsColumnNotEnum {
                entity: entity_id.clone(),
                col: column.clone(),
            },
        );
        return;
    };

    for variant in [&from, &to] {
        if !variants
            .iter()
            .any(|v| v == variant || v.eq_ignore_ascii_case(variant))
        {
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
        }
    }

    // Normalize to declared enum casing.
    let from = variants
        .iter()
        .find(|v| *v == &from || v.eq_ignore_ascii_case(&from))
        .cloned()
        .unwrap_or(from);
    let to = variants
        .iter()
        .find(|v| *v == &to || v.eq_ignore_ascii_case(&to))
        .cloned()
        .unwrap_or(to);

    ensure_entity_variant_state(model, &entity_id, &from, col.value_label(&from));
    ensure_entity_variant_state(model, &entity_id, &to, col.value_label(&to));

    if let Some(existing) = model
        .state_transitions
        .iter()
        .find(|st| st.entity == entity_key && st.column != column)
    {
        push_warning(
            ctx,
            diags,
            arg_span(&pred.args[0]),
            RdraError::MultipleLifecycleColumns {
                entity: entity_id.clone(),
                existing: existing.column.clone(),
                column: column.clone(),
            },
        );
    }

    model.state_transitions.push(crate::model::StateTransition {
        site: Some(ctx.locate(pred.span.clone())),
        usecase: *usecase,
        entity: entity_key,
        column: column.clone(),
        from: from.clone(),
        to: to.clone(),
    });
    model.typed_predicates.push(TypedPredicate::Transitions {
        entity: entity_key,
        column,
        usecase: *usecase,
        from,
        to,
    });
}

fn ensure_entity_variant_state(
    model: &mut SemanticModel,
    entity_id: &str,
    variant: &str,
    label: Option<&str>,
) -> crate::model::StateKey {
    let id = format!("{entity_id}_{variant}");
    if let Some((k, _)) = model.states.iter().find(|(_, s)| s.id == id) {
        return k;
    }
    model.states.insert(crate::model::State {
        id,
        label: label.unwrap_or(variant).to_string(),
        description: None,
    })
}

fn process_exposes_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::publication::ExposeError;
    let (Some(Some(NodeRef::System(system))), Some(Some(target))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    let Some(item) = crate::publication::exposed_item(target, &pred.chain) else {
        return;
    };
    let kind = node_kind_tag_str(target).to_string();
    let id = predicate_arg_display(&pred.args[1]);
    let error = match item {
        Ok(_) if model.publication.exposes(target) => RdraError::DuplicateExposure { kind, id },
        Ok(item) => {
            model.publication.exposures.push(Exposure {
                system: *system,
                item,
                site: Some(ctx.locate(pred.span.clone())),
            });
            return;
        }
        Err(ExposeError::WrongKind { chain, hint }) => RdraError::ExposesChainInvalid {
            chain,
            kind,
            id,
            hint,
        },
        Err(ExposeError::BadCaller { args }) => RdraError::ExposesCallerInvalid {
            usecase: id,
            args: args
                .iter()
                .map(predicate_arg_display)
                .collect::<Vec<_>>()
                .join(", "),
        },
        Err(ExposeError::NoMode) => RdraError::InternalEntityExposed {
            system: model.systems[*system].id.clone(),
            entity: id,
        },
    };
    push_error(ctx, diags, pred.span.clone(), error);
}

fn process_shares_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
) {
    if let (Some(Some(NodeRef::System(system))), Some(Some(NodeRef::Entity(entity)))) =
        (resolved.first(), resolved.get(1))
    {
        if model.publication.is_shared(*entity) {
            return;
        }
        model.publication.shares.push(Share {
            system: *system,
            entity: *entity,
            site: Some(ctx.locate(pred.span.clone())),
        });
    }
}

fn process_outbox_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let Some(Some(NodeRef::Event(event))) = resolved.first() {
        model.outbox_events.insert(*event);
    }
}

fn process_sets_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (Some(Some(NodeRef::UseCase(origin))), Some(Some(NodeRef::Entity(entity_key)))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    let (origin, entity_key) = (*origin, *entity_key);

    if let Some(PredicateArg::Expr(Expr::Cmp(cmp))) = pred.args.get(2) {
        let entity_id = model.entities[entity_key].id.clone();
        let entity_cols = model.entities[entity_key].columns.clone();

        // Fourth arg true/false drives comparison propositions (stock < selling, …).
        // Value equality (status == paid) with `, true` falls through to a column effect;
        // `, false` is invalid for value equality (would silently set the value).
        let truth_str = match pred.args.get(3) {
            Some(PredicateArg::Ref(q)) if q.kind_qualifier.is_none() && q.parts.len() == 1 => {
                Some(q.parts[0].as_str().to_string())
            }
            Some(PredicateArg::Lit(s)) => Some(s.clone()),
            _ => None,
        };
        if let Some(truth_str) = truth_str.filter(|s| s == "true" || s == "false") {
            match super::constraint::resolve_entity_equals_from_comparison(
                &model.variations,
                &entity_cols,
                &entity_id,
                cmp,
                ctx,
                diags,
            ) {
                Ok(Some((col_name, value))) => {
                    if truth_str == "true" {
                        model.column_effects.push(ColumnEffect {
                            site: Some(ctx.locate(pred.span.clone())),
                            origin,
                            entity: entity_key,
                            column: col_name.clone(),
                            value,
                        });
                        model.typed_predicates.push(TypedPredicate::SetsColumn {
                            origin,
                            entity: entity_key,
                            column: col_name,
                        });
                    } else {
                        push_error(
                            ctx,
                            diags,
                            cmp.span.clone(),
                            RdraError::SetsFalseOnEquals {
                                col: col_name,
                                value: match &cmp.rhs {
                                    Operand::Column(v) | Operand::IntLit(v) => v.clone(),
                                    Operand::Now => "now".into(),
                                    Operand::QualifiedColumn(q) => q.column.clone(),
                                },
                            },
                        );
                    }
                    return;
                }
                Ok(None) => {
                    if let Some(prop) =
                        resolve_comparison(&entity_cols, &entity_id, cmp, ctx, diags)
                    {
                        model.proposition_effects.push(PropositionEffect {
                            site: Some(ctx.locate(pred.span.clone())),
                            origin,
                            entity: entity_key,
                            prop: prop.clone(),
                            truth: truth_str == "true",
                        });
                        model
                            .typed_predicates
                            .push(TypedPredicate::SetsProposition {
                                origin,
                                entity: entity_key,
                                prop,
                                truth: truth_str == "true",
                            });
                    }
                    return;
                }
                Err(()) => return,
            }
        }

        // Column effect form: sets(UC, E, status == captured)
        if let Ok(Some((col_name, value))) =
            super::constraint::resolve_entity_equals_from_comparison(
                &model.variations,
                &entity_cols,
                &entity_id,
                cmp,
                ctx,
                diags,
            )
        {
            model.column_effects.push(ColumnEffect {
                site: Some(ctx.locate(pred.span.clone())),
                origin,
                entity: entity_key,
                column: col_name.clone(),
                value,
            });
            model.typed_predicates.push(TypedPredicate::SetsColumn {
                origin,
                entity: entity_key,
                column: col_name,
            });
        }
    }
}

fn process_forbidden_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.forbidden_constraints.push(ForbiddenConstraint {
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Forbidden { entity: entity_key });
    }
}

fn process_required_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.required_constraints.push(RequiredConstraint {
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Required { entity: entity_key });
    }
}

fn process_exclusive_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };

    if conditions.equals.len() + conditions.comparisons.len() >= 2 {
        model.exclusive_constraints.push(ExclusiveConstraint {
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Exclusive { entity: entity_key });
    }
}

fn process_invariant_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let mut guards: Vec<(String, EffectValue)> = Vec::new();
    let mut guard_comparisons: Vec<ComparisonProp> = Vec::new();
    let mut requireds: Vec<(String, EffectValue)> = Vec::new();
    let mut required_comparisons: Vec<ComparisonProp> = Vec::new();

    for cc in &pred.chain {
        let is_guard = cc.name == "when";
        let is_required = cc.name == "then";
        if !is_guard && !is_required {
            continue;
        }

        let mut processed_eq = false;
        for arg in &cc.args {
            if processed_eq {
                break;
            }
            match arg {
                PredicateArg::Expr(Expr::Cmp(cmp)) => {
                    match resolve_entity_equals_from_comparison(
                        &model.variations,
                        &entity_cols,
                        &entity_id,
                        cmp,
                        ctx,
                        diags,
                    ) {
                        Ok(Some((col_str, value))) => {
                            if is_guard {
                                guards.push((col_str, value));
                            } else {
                                requireds.push((col_str, value));
                            }
                        }
                        Ok(None) => {
                            if let Some(prop) =
                                resolve_comparison(&entity_cols, &entity_id, cmp, ctx, diags)
                            {
                                if is_guard {
                                    guard_comparisons.push(prop);
                                } else {
                                    required_comparisons.push(prop);
                                }
                            }
                        }
                        Err(()) => return,
                    }
                }
                _ => {
                    if cc.args.len() < 2 {
                        break;
                    }
                    let Some(col_str) = arg_as_str(&cc.args[0]) else {
                        break;
                    };
                    let Some(val_str) = arg_as_str(&cc.args[1]) else {
                        break;
                    };
                    let col = entity_cols.iter().find(|c| c.name == col_str).cloned();
                    let Some(col) = col else {
                        push_error(
                            ctx,
                            diags,
                            arg_span(&cc.args[0]),
                            RdraError::UnknownColumn {
                                entity: entity_id.clone(),
                                col: col_str,
                            },
                        );
                        return;
                    };
                    match parse_effect_value(&model.variations, &col, &val_str) {
                        Ok(value) => {
                            if is_guard {
                                guards.push((col_str, value));
                            } else {
                                requireds.push((col_str, value));
                            }
                        }
                        Err(e) => {
                            push_error_parse_effect(ctx, diags, &cc.args[1], e);
                            return;
                        }
                    }
                    processed_eq = true;
                }
            }
        }
    }

    let has_guards = !guards.is_empty() || !guard_comparisons.is_empty();
    let has_requireds = !requireds.is_empty() || !required_comparisons.is_empty();
    if has_guards && has_requireds {
        model.entity_invariants.push(EntityInvariant {
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            guards,
            guard_comparisons,
            requireds,
            required_comparisons,
        });
        model
            .typed_predicates
            .push(TypedPredicate::Invariant { entity: entity_key });
    }
}

fn relation_kind_for_predicate(pred_name: &str) -> Option<RelKind> {
    let kind = match pred_name {
        "performs" => RelKind::Performs,
        "reads" => RelKind::Reads,
        "writes" => RelKind::Writes,
        "creates" => RelKind::Creates,
        "updates" => RelKind::Updates,
        "deletes" => RelKind::Deletes,
        "displays" => RelKind::Displays,
        "shows" => RelKind::Shows,
        "raises" => RelKind::Raises,
        "triggers" => RelKind::Triggers,
        "contains" => RelKind::Contains,
        "belongs" => RelKind::Belongs,
        "has_permission" => RelKind::HasPermission,
        "requires_permission" => RelKind::RequiresPermission,
        "requires_medium" => RelKind::RequiresMedium,
        "motivates" => RelKind::Motivates,
        "decides" => RelKind::Decides,
        "invokes" => RelKind::Invokes,
        "precedes" => RelKind::Precedes,
        "branches" => RelKind::Branches,
        "excepts" => RelKind::Excepts,
        "repeats" => RelKind::Repeats,
        "covers" => RelKind::Covers,
        "compensates" => RelKind::Compensates,
        "request" => RelKind::Request,
        "response" => RelKind::Response,
        "error_response" => RelKind::ErrorResponse,
        "applies_to" => RelKind::AppliesTo,
        "qualifies" => RelKind::Qualifies,
        "constrains" => RelKind::Constrains,
        "maps_field" => RelKind::MapsField,
        "owns" => RelKind::Owns,
        _ => return None,
    };
    Some(kind)
}

fn process_belongs_context(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    from: &NodeRef,
    to: &NodeRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (NodeRef::Buc(buc), NodeRef::Business(business)) = (from, to) else {
        return;
    };
    let mut bys = Vec::new();

    for cc in pred.chain.iter().filter(|cc| cc.name == "by") {
        for arg in &cc.args {
            if let Some(value) = context_value_from_arg(model, arg, "medium", ctx, diags) {
                bys.push(value);
            }
        }
    }

    if !bys.is_empty() {
        model
            .business_mapping_contexts
            .push(BusinessMappingContext {
                buc: *buc,
                business: *business,
                bys,
            });
    }
}

fn process_relation_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(kind) = relation_kind_for_predicate(&pred.name) else {
        return;
    };
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        model.relations.push(Relation {
            site: Some(ctx.locate(pred.span.clone())),
            from: from.clone(),
            to: to.clone(),
            kind,
            options: RelationOptions::default(),
            via: None,
        });
        if pred.name == "belongs" {
            process_belongs_context(model, pred, from, to, ctx, diags);
        }
    }
}

/// Resolve `.via(Entity)` from the relate chain. Returns `(via, had_via_call)`.
fn resolve_relate_via(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> (Option<NodeRef>, bool) {
    let mut via = None;
    let mut had_via = false;
    for cc in &pred.chain {
        if cc.name != "via" {
            continue;
        }
        had_via = true;
        let span = cc.span.clone();
        if cc.args.len() != 1 {
            push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
            continue;
        }
        match resolve_arg(model, &cc.args[0], ctx, diags) {
            Some(node @ NodeRef::Entity(_)) => {
                via = Some(node);
            }
            Some(_) => {
                push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
            }
            None => {
                // resolve_arg already reported undefined / ambiguity when Ref.
                if !matches!(cc.args.first(), Some(PredicateArg::Ref(_))) {
                    push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
                }
            }
        }
    }
    (via, had_via)
}

/// A card's kind and its child-side minimum. A side is `1`, `N`/`M`, or `lo..hi` with `lo`
/// in {0, 1} and `hi` in {1, *}; a range stands for the side `1` (hi 1) or `N`/`M` (hi *),
/// and only the child side, the many side of N:1 / 1:N and the left of 1:1, may carry one.
/// `Ok(None)` for a card no relate kind names, which today's forms drop silently.
fn parse_card(card: &str) -> Result<Option<(RelKind, Min)>, RdraError> {
    let Some((left, right)) = card.split_once(':') else {
        return Ok(None);
    };
    let range = |side: &str| -> Result<Option<(Min, bool)>, RdraError> {
        let Some((lo, hi)) = side.split_once("..") else {
            return Ok(None);
        };
        let out_of_range = || RdraError::MinCardOutOfRange {
            range: side.to_string(),
        };
        let min = match lo {
            "0" => Min::Zero,
            "1" => Min::One,
            _ => return Err(out_of_range()),
        };
        let many = match hi {
            "1" => false,
            "*" => true,
            _ => return Err(out_of_range()),
        };
        Ok(Some((min, many)))
    };
    let (l, r) = (range(left)?, range(right)?);
    let base = |side: &'static str, written: &str, r: Option<(Min, bool)>| match r {
        Some((_, true)) => side,
        Some((_, false)) => "1",
        None => match written {
            "1" => "1",
            "N" => "N",
            "M" => "M",
            _ => "?",
        },
    };
    let left_base = base("N", left, l);
    let right_base = base(if left_base == "1" { "N" } else { "M" }, right, r);
    let kind = match (left_base, right_base) {
        ("1", "1") => RelKind::RelateOneToOne,
        ("1", "N") => RelKind::RelateOneToMany,
        ("N", "1") => RelKind::RelateManyToOne,
        ("N", "M") => RelKind::RelateManyToMany,
        _ => return Ok(None),
    };
    let (child, parent) = match kind {
        RelKind::RelateOneToMany => (r, l),
        RelKind::RelateManyToMany if l.is_some() || r.is_some() => {
            return Err(RdraError::MinCardOnManyToMany {
                card: card.to_string(),
            })
        }
        _ => (l, r),
    };
    if parent.is_some() {
        return Err(RdraError::MinCardOnParentSide {
            card: card.to_string(),
        });
    }
    Ok(Some((kind, child.map_or(Min::Zero, |(min, _)| min))))
}

fn process_relate_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let card_str = match pred.args.get(2) {
        Some(PredicateArg::Card(c)) => c.as_str(),
        Some(PredicateArg::Lit(c)) => c.as_str(),
        _ => return,
    };
    let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) else {
        return;
    };

    let (via, had_via) = resolve_relate_via(model, pred, ctx, diags);
    let mut options = super::fk::relation_options(pred, ctx, diags);

    let (kind, children_min) = match parse_card(card_str) {
        Ok(Some(card)) => card,
        Ok(None) => return,
        Err(err) => {
            let span = pred
                .args
                .get(2)
                .map(arg_span)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| pred.span.clone());
            push_error(ctx, diags, span, err);
            return;
        }
    };
    options.children_min = children_min;

    if had_via && kind != RelKind::RelateManyToMany {
        let span = pred
            .chain
            .iter()
            .find(|cc| cc.name == "via")
            .map(|cc| cc.span.clone())
            .unwrap_or_else(|| pred.span.clone());
        push_error(ctx, diags, span, RdraError::ViaOnlyForNM);
        return;
    }

    if kind == RelKind::RelateManyToMany {
        for cc in pred
            .chain
            .iter()
            .filter(|cc| matches!(cc.name.as_str(), "role" | "key"))
        {
            push_error(
                ctx,
                diags,
                cc.span.clone(),
                RdraError::KeyOptionOnManyToMany {
                    method: cc.name.clone(),
                },
            );
        }
        // The junction relates synthesized from this one must not inherit them.
        options.role = None;
        options.key.clear();
    }

    if kind == RelKind::RelateManyToMany && via.is_none() {
        // L4: if `.via` was present but failed to resolve, keep that diagnostic only.
        if had_via {
            return;
        }
        let from_id = match from {
            NodeRef::Entity(k) => model.entities[*k].id.clone(),
            _ => "?".into(),
        };
        let to_id = match to {
            NodeRef::Entity(k) => model.entities[*k].id.clone(),
            _ => "?".into(),
        };
        push_error(
            ctx,
            diags,
            pred.args
                .get(2)
                .map(arg_span)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| pred.span.clone()),
            RdraError::NMRelation {
                from: from_id,
                to: to_id,
            },
        );
        return;
    }

    if kind == RelKind::RelateManyToMany {
        if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (from, to) {
            if fk == tk {
                push_error(
                    ctx,
                    diags,
                    pred.span.clone(),
                    RdraError::SelfNMRelation {
                        entity: model.entities[*fk].id.clone(),
                    },
                );
                return;
            }
        }
        if let (NodeRef::Entity(fk), NodeRef::Entity(tk), Some(NodeRef::Entity(jk))) =
            (from, to, via.as_ref())
        {
            if jk == fk || jk == tk {
                let via_span = pred
                    .chain
                    .iter()
                    .find(|cc| cc.name == "via")
                    .map(|cc| cc.span.clone())
                    .unwrap_or_else(|| pred.span.clone());
                push_error(
                    ctx,
                    diags,
                    via_span,
                    RdraError::ViaMustBeDistinct {
                        from: model.entities[*fk].id.clone(),
                        to: model.entities[*tk].id.clone(),
                        via: model.entities[*jk].id.clone(),
                    },
                );
                return;
            }
        }
    }

    let synthesize_via_n1 = kind == RelKind::RelateManyToMany && via.is_some();
    let via_for_synth = via.clone();
    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: from.clone(),
        to: to.clone(),
        kind,
        options: options.clone(),
        via,
    });

    // N:M.via(J) ≡ two N:1 edges J→A, J→B so TLA/ER/.along/has_relate_edge see the junction path.
    if synthesize_via_n1 {
        if let (NodeRef::Entity(_), NodeRef::Entity(_), Some(NodeRef::Entity(junction))) =
            (from, to, via_for_synth.as_ref())
        {
            let junction_ref = NodeRef::Entity(*junction);
            for end in [from.clone(), to.clone()] {
                if !relation_exists(model, &junction_ref, &end, RelKind::RelateManyToOne)
                    && !relation_exists(model, &end, &junction_ref, RelKind::RelateOneToMany)
                {
                    model.relations.push(Relation {
                        site: Some(ctx.locate(pred.span.clone())),
                        from: junction_ref.clone(),
                        to: end,
                        kind: RelKind::RelateManyToOne,
                        options: options.clone(),
                        via: None,
                    });
                }
            }
        }
    }
}

fn relation_exists(model: &SemanticModel, from: &NodeRef, to: &NodeRef, kind: RelKind) -> bool {
    model
        .relations
        .iter()
        .any(|r| r.from == *from && r.to == *to && r.kind == kind)
}

pub(crate) fn process_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(sig) = predicate_signature(&pred.name) else {
        push_error(
            ctx,
            diags,
            pred.span.clone(),
            RdraError::UnknownPredicate {
                name: pred.name.clone(),
            },
        );
        return;
    };

    let allowed = predicate_chain_methods(&pred.name);
    for cc in &pred.chain {
        if !allowed.contains(&cc.name.as_str()) {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::UnknownChainMethod {
                    pred: pred.name.clone(),
                    name: cc.name.clone(),
                    hint: unknown_chain_method_hint(&pred.name, &cc.name),
                },
            );
        }
    }

    if let Some(err) = arity_error(pred, &sig) {
        push_error(ctx, diags, pred.span.clone(), err);
        return;
    }

    // Top-level `when(...)` quantifier predicate uses cross-entity path.
    if pred.name == "when" {
        process_when_quantifier_predicate(model, pred, ctx, diags);
        return;
    }

    // Unified forbidden/invariant: detect cross-entity vs single-entity form.
    // Cross-entity: first arg is Expr, `.along(...)`, or two+ leading entity refs
    // (e.g. `forbidden(Order, Payment, Order.status == x, ...)`).
    // Single-entity: `forbidden(Order, status == x)`.
    if pred.name == "forbidden" || pred.name == "invariant" {
        let leading_entities = pred
            .args
            .iter()
            .take_while(|a| matches!(a, PredicateArg::Ref(_)))
            .count();
        let is_cross = pred
            .args
            .first()
            .is_some_and(|a| matches!(a, PredicateArg::Expr(_)))
            || pred.chain.iter().any(|cc| cc.name == "along")
            || leading_entities >= 2
            || (pred.name == "invariant" && pred.args.is_empty());
        if is_cross {
            match pred.name.as_str() {
                "forbidden" => process_cross_forbidden(model, pred, ctx, diags),
                "invariant" => process_cross_invariant(model, pred, ctx, diags),
                _ => {}
            }
            return;
        }
        if let Some(by) = pred.chain.iter().find(|cc| cc.name == "by") {
            push_error(ctx, diags, by.span.clone(), RdraError::ByWithoutAlong);
        }
    }

    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    if !validate_predicate_arg_types(model, pred, &sig, &resolved, ctx, diags) {
        return;
    }
    if !validate_contains_pair(pred, &resolved, ctx, diags)
        || !validate_trigger_pair(pred, &sig, &resolved, ctx, diags)
    {
        return;
    }

    match pred.name.as_str() {
        "coordinates" => process_coordinates_predicate(model, &resolved),
        "transitions" => process_transitions_predicate(model, pred, &resolved, ctx, diags),
        "outbox" => process_outbox_predicate(model, &resolved),
        "exposes" => process_exposes_predicate(model, pred, &resolved, ctx, diags),
        "shares" => process_shares_predicate(model, pred, &resolved, ctx),
        "after" => process_after_predicate(model, pred, &resolved, ctx, diags),
        "sets" => process_sets_predicate(model, pred, &resolved, ctx, diags),
        "combines" => {
            super::condition::process_combines_predicate(model, pred, &resolved, ctx, diags)
        }
        "holds" => super::condition::process_holds_predicate(model, pred, &resolved, ctx, diags),
        "forbidden" => process_forbidden_predicate(model, pred, &resolved, ctx, diags),
        "invariant" => process_invariant_predicate(model, pred, &resolved, ctx, diags),
        "required" => process_required_predicate(model, pred, &resolved, ctx, diags),
        "exclusive" => process_exclusive_predicate(model, pred, &resolved, ctx, diags),
        "maps_field" => process_maps_field_predicate(model, pred, &resolved, ctx, diags),
        "relate" => process_relate_predicate(model, pred, &resolved, ctx, diags),
        _ => process_relation_predicate(model, pred, &resolved, ctx, diags),
    }

    if let Some(typed) = crate::typed_predicate::build_typed_predicate(&pred.name, &resolved, pred)
    {
        model.typed_predicates.push(typed);
    }
}

/// Fixed-arity predicates must match the signature length exactly.
/// Variadic / trailing-condition predicates only enforce a minimum.
fn arity_error(pred: &PredicateCall, sig: &[Vec<&'static str>]) -> Option<RdraError> {
    let got = pred.args.len();
    let (expected, ok) = match pred.name.as_str() {
        "when" => return None,
        "sets" => ("at least 2".into(), got >= 2),
        "after" => ("at least 1".into(), got >= 1),
        // Cross-entity `invariant` may be empty before `.when(...).none/has(...)`.
        "invariant" => ("0 or more".into(), true),
        "forbidden" | "required" | "exclusive" => ("at least 1".into(), got >= 1),
        "holds" => ("at least 2".into(), got >= 2),
        "transitions" => ("3".into(), got == 3),
        "maps_field" => ("3".into(), got == 3),
        "relate" => ("3".into(), got == 3),
        _ => {
            let n = sig.len();
            (n.to_string(), got == n)
        }
    };
    if ok {
        None
    } else {
        Some(RdraError::WrongArity {
            name: pred.name.clone(),
            expected,
            got,
        })
    }
}
