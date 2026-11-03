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
    arg_as_str, collect_entity_conditions, context_value_from_arg, forbidden_when_conditions,
    process_after_predicate, process_forbidden as process_cross_forbidden,
    process_invariant as process_cross_invariant, push_rule_guards,
    resolve_entity_equals_from_comparison, rule_label, when_condition_ref,
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
                        | "drives"
                        | "invariant"
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
        PredicateArg::Transition { from, to } => {
            format!("{} -> {to}", rdra_ish_syntax::format::format_sources(from))
        }
        PredicateArg::Card(c) => c.clone(),
    }
}

fn validate_predicate_arg_types(
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, expected_kinds) in sig.iter().enumerate() {
        if matches!(
            expected_kinds.as_slice(),
            ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"]
        ) {
            continue;
        }
        if let Some(Some(node)) = resolved.get(i) {
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
    let Some(Some(NodeRef::Field(field))) = resolved.first() else {
        return;
    };
    let field = *field;
    let Some(arg) = pred.args.get(1) else {
        return;
    };
    // `Entity.column`: every part but the last names the entity.
    let (qref, column, path) = match arg {
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() > 1 => {
            let (column, path) = qref.parts.split_last().expect("more than one part");
            (qref, column.clone(), path)
        }
        other => {
            let err = RdraError::TypeMismatch {
                pred: pred.name.clone(),
                id: predicate_arg_display(other),
                actual: "argument".to_string(),
                expected: "Entity.column".to_string(),
            };
            push_error(ctx, diags, arg_span(other), err);
            return;
        }
    };
    let qualifier = PredicateArg::Ref(QRef {
        kind_qualifier: None,
        parts: path.to_vec(),
        span: qref.span.clone(),
    });
    let entity = match resolve_arg(model, &qualifier, ctx, diags) {
        Some(NodeRef::Entity(entity)) => entity,
        Some(other) => {
            let err = RdraError::TypeMismatch {
                pred: pred.name.clone(),
                id: path.join("."),
                actual: node_kind_tag_str(&other).to_string(),
                expected: "entity".to_string(),
            };
            push_error(ctx, diags, qref.span.clone(), err);
            return;
        }
        None => return,
    };
    if !model.entities[entity]
        .columns
        .iter()
        .any(|col| col.name == column)
    {
        push_error(
            ctx,
            diags,
            qref.span.clone(),
            RdraError::UnknownColumn {
                entity: model.entities[entity].id.clone(),
                col: column,
            },
        );
        return;
    }

    model.field_mappings.push(FieldMapping {
        field,
        entity,
        column: column.clone(),
    });
    model.typed_predicates.push(TypedPredicate::MapsField {
        field,
        entity,
        column,
    });
    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: NodeRef::Field(field),
        to: NodeRef::Entity(entity),
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
fn process_chooses_predicate(
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
            RdraError::UndefinedSymbol { id: entity_id },
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
    let before = diags.len();
    let Some(target) = effect_target(model, pred, resolved, ctx, diags) else {
        return;
    };
    let recorded = target.entity.is_some_and(|entity| {
        record_sets_effect(model, pred, target.origin, entity, target.cmp, ctx, diags)
    });
    if !recorded {
        let err = RdraError::SetsNoEffect {
            usecase: model.use_cases[target.origin].id.clone(),
            entity: target.entity_id(model),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    }
}

fn process_drives_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let Some(target) = effect_target(model, pred, resolved, ctx, diags) else {
        return;
    };
    let recorded = target.entity.is_some_and(|entity| {
        record_drives_effect(model, pred, target.origin, entity, target.cmp, ctx, diags)
    });
    if !recorded {
        let err = RdraError::DrivesNoEffect {
            usecase: model.use_cases[target.origin].id.clone(),
            entity: target.entity_id(model),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    }
}

/// What a `sets` or `drives` writes: the use case it names first, and the entity and
/// comparison of its second argument, `Entity.column op rhs`.
struct EffectTarget<'a> {
    origin: UseCaseKey,
    /// `None` when the second argument names no entity column; the caller reports it.
    entity: Option<EntityKey>,
    cmp: Option<&'a Comparison>,
}

impl EffectTarget<'_> {
    fn entity_id(&self, model: &SemanticModel) -> String {
        self.entity
            .map_or_else(|| "Entity".to_string(), |k| model.entities[k].id.clone())
    }
}

/// The target of a `sets` or `drives`, or `None` when its use case or its entity does not
/// resolve, either of which is already reported.
fn effect_target<'a>(
    model: &SemanticModel,
    pred: &'a PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EffectTarget<'a>> {
    let Some(Some(NodeRef::UseCase(origin))) = resolved.first() else {
        return None;
    };
    let cmp = match pred.args.get(1) {
        Some(PredicateArg::Expr(Expr::Cmp(cmp))) => Some(cmp),
        _ => None,
    };
    let entity = match cmp.map(|cmp| &cmp.lhs) {
        Some(Operand::QualifiedColumn(qcol)) => Some(super::constraint::resolve_entity_qref(
            model,
            &pred.name,
            &qcol.entity,
            ctx,
            diags,
        )?),
        _ => None,
    };
    Some(EffectTarget {
        origin: *origin,
        entity,
        cmp,
    })
}

/// `cmp`, or `None` after reporting a set membership: a use case leaves one value.
fn effect_comparison<'a>(
    pred: &PredicateCall,
    cmp: Option<&'a Comparison>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<&'a Comparison> {
    let cmp = cmp?;
    if cmp.op.is_membership() {
        let col = match &cmp.lhs {
            Operand::Column(name) => name.clone(),
            Operand::QualifiedColumn(q) => q.column.clone(),
            _ => String::new(),
        };
        push_error(
            ctx,
            diags,
            cmp.span.clone(),
            RdraError::EffectMembership {
                pred: pred.name.clone(),
                col,
            },
        );
        return None;
    }
    Some(cmp)
}

fn push_column_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity: EntityKey,
    column: String,
    value: SetValue,
    ctx: DiagCtxt,
) {
    let by = super::constraint::selectors(&pred.chain, None);
    model.column_effects.push(ColumnEffect {
        site: Some(ctx.locate(pred.span.clone())),
        origin,
        entity,
        column: column.clone(),
        value,
        by,
    });
    model.typed_predicates.push(TypedPredicate::SetsColumn {
        origin,
        entity,
        column,
    });
}

/// Records the column effect `sets(UC, E.column == value)` states, and says whether it did.
fn record_sets_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity_key: EntityKey,
    cmp: Option<&Comparison>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(cmp) = effect_comparison(pred, cmp, ctx, diags) else {
        return false;
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let value = match super::constraint::resolve_entity_equals_from_comparison(
        &model.variations,
        &entity_cols,
        &entity_id,
        cmp,
        ctx,
        diags,
    ) {
        Ok(Some((column, value))) => Some((column, SetValue::Value(value))),
        Ok(None) => match super::effect::resolve_copy(model, entity_key, cmp, ctx, diags) {
            Ok(copy) => copy.map(|(column, source)| (column, SetValue::Copy(source))),
            Err(()) => None,
        },
        Err(()) => None,
    };
    let Some((column, value)) = value else {
        return false;
    };
    if let SetValue::Value(_) = &value {
        super::constraint::report_leading_by(&pred.chain, ctx, diags);
    }
    push_column_effect(model, pred, origin, entity_key, column, value, ctx);
    true
}

/// Records the comparison `drives(UC, E.column op rhs, true|false)` drives, and says whether
/// it did. A value equality (`E.status == paid, true`) sets the value, as `sets` does; driving
/// one `false` is an error, since it names no value to leave.
fn record_drives_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity_key: EntityKey,
    cmp: Option<&Comparison>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let truth = match pred.args.get(2) {
        Some(PredicateArg::Ref(q)) if q.kind_qualifier.is_none() && q.parts.len() == 1 => {
            q.parts[0].as_str()
        }
        Some(PredicateArg::Lit(s)) => s.as_str(),
        _ => return false,
    };
    let truth = match truth {
        "true" => true,
        "false" => false,
        _ => return false,
    };
    let Some(cmp) = effect_comparison(pred, cmp, ctx, diags) else {
        return false;
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    match super::constraint::resolve_entity_equals_from_comparison(
        &model.variations,
        &entity_cols,
        &entity_id,
        cmp,
        ctx,
        diags,
    ) {
        Ok(Some((column, value))) if truth => {
            let value = SetValue::Value(value);
            push_column_effect(model, pred, origin, entity_key, column, value, ctx);
            true
        }
        Ok(Some((col, _))) => {
            let value = match &cmp.rhs {
                Operand::Column(v) | Operand::IntLit(v) => v.clone(),
                Operand::Now => "now".into(),
                Operand::QualifiedColumn(q) => q.column.clone(),
                Operand::Values(vs) => format!("({})", vs.join(", ")),
            };
            push_error(
                ctx,
                diags,
                cmp.span.clone(),
                RdraError::DrivesFalseOnEquals { col, value },
            );
            false
        }
        Ok(None) => {
            let Some(prop) =
                resolve_comparison(&model.variations, &entity_cols, &entity_id, cmp, ctx, diags)
            else {
                return false;
            };
            model.proposition_effects.push(PropositionEffect {
                site: Some(ctx.locate(pred.span.clone())),
                origin,
                entity: entity_key,
                prop: prop.clone(),
                truth,
            });
            model.typed_predicates.push(TypedPredicate::Drives {
                origin,
                entity: entity_key,
                prop,
                truth,
            });
            true
        }
        Err(()) => false,
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
    let before = diags.len();
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
    let Ok(when) = forbidden_when_conditions(model, pred, ctx, diags) else {
        return;
    };

    if conditions.equals.is_empty() && conditions.comparisons.is_empty() && when.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.forbidden_constraints.push(ForbiddenConstraint {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions: conditions.equals,
            comparisons: conditions.comparisons,
        });
        let rule = RuleRef::Forbidden(model.forbidden_constraints.len() - 1);
        push_rule_guards(model, rule, &when, ctx);
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
    let before = diags.len();
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

    if conditions.equals.is_empty() && conditions.comparisons.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.required_constraints.push(RequiredConstraint {
            label,
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
    let before = diags.len();
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

    let got = conditions.equals.len() + conditions.comparisons.len();
    if got < 2 {
        let err = RdraError::ExclusiveNeedsTwo { got };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.exclusive_constraints.push(ExclusiveConstraint {
            label,
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
    let before = diags.len();
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let mut guards: Vec<(String, EffectValue)> = Vec::new();
    let mut guard_comparisons: Vec<ComparisonProp> = Vec::new();
    // `.when(Cond)` calls: their bodies join the guards after every predicate is read.
    let mut when: Vec<(ConditionKey, Span)> = Vec::new();
    let mut requireds: Vec<(String, EffectValue)> = Vec::new();
    let mut required_comparisons: Vec<ComparisonProp> = Vec::new();

    for cc in &pred.chain {
        let is_guard = cc.name == "when";
        let is_required = cc.name == "then";
        if !is_guard && !is_required {
            continue;
        }
        if is_guard {
            match when_condition_ref(model, &pred.name, cc, ctx, diags) {
                Ok(Some(ck)) => {
                    when.push((ck, cc.span.clone()));
                    continue;
                }
                Ok(None) => {}
                Err(()) => return,
            }
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
                            if let Some(prop) = resolve_comparison(
                                &model.variations,
                                &entity_cols,
                                &entity_id,
                                cmp,
                                ctx,
                                diags,
                            ) {
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

    let has_guards = !guards.is_empty() || !guard_comparisons.is_empty() || !when.is_empty();
    let has_requireds = !requireds.is_empty() || !required_comparisons.is_empty();
    if !has_requireds {
        let err = RdraError::InvariantWithoutConsequence;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else if !has_guards {
        let err = RdraError::InvariantWithoutWhen;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.entity_invariants.push(EntityInvariant {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            guards,
            guard_comparisons,
            requireds,
            required_comparisons,
        });
        let rule = RuleRef::Invariant(model.entity_invariants.len() - 1);
        push_rule_guards(model, rule, &when, ctx);
        model
            .typed_predicates
            .push(TypedPredicate::Invariant { entity: entity_key });
    }
}

fn relation_kind_for_predicate(pred_name: &str) -> Option<RelKind> {
    let kind = match pred_name {
        "performs" => RelKind::Performs,
        "reads" => RelKind::Reads,
        "creates" => RelKind::Creates,
        "updates" => RelKind::Updates,
        "deletes" => RelKind::Deletes,
        "displays" => RelKind::Displays,
        "shows" => RelKind::Shows,
        "navigates" => RelKind::Navigates,
        "raises" => RelKind::Raises,
        "triggers" => RelKind::Triggers,
        "contains" => RelKind::Contains,
        "belongs" => RelKind::Belongs,
        "has_permission" => RelKind::HasPermission,
        "requires_permission" => RelKind::RequiresPermission,
        "requires_medium" => RelKind::RequiresMedium,
        "motivates" => RelKind::Motivates,
        "achieves" => RelKind::Achieves,
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

/// `navigates(Screen, Screen)` with at most one `.via(UseCase)`: the use case whose operation
/// moves the user from the first screen to the second.
fn process_navigates_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let mut via = None;
    for (index, cc) in pred.chain.iter().filter(|cc| cc.name == "via").enumerate() {
        let span = cc.span.clone();
        if index > 0 || cc.args.len() != 1 {
            push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase);
            continue;
        }
        match resolve_arg(model, &cc.args[0], ctx, diags) {
            Some(NodeRef::UseCase(usecase)) => via = Some(usecase),
            Some(_) => push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase),
            // resolve_arg already reported an undefined or ambiguous reference.
            None if matches!(cc.args.first(), Some(PredicateArg::Ref(_))) => {}
            None => push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase),
        }
    }
    let (Some(Some(NodeRef::Screen(from))), Some(Some(NodeRef::Screen(to)))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: NodeRef::Screen(*from),
        to: NodeRef::Screen(*to),
        kind: RelKind::Navigates,
        options: RelationOptions::default(),
        via: via.map(NodeRef::UseCase),
    });
    model.typed_predicates.push(TypedPredicate::Navigates {
        from: *from,
        to: *to,
        via,
    });
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

    // Unified forbidden/invariant: detect cross-entity vs single-entity form.
    // Cross-entity: `.along(...)`, two or more leading entity refs
    // (e.g. `forbidden(Order, Payment, Order.status == x, ...)`), or an invariant whose
    // `.has/.none(...)` ranges over related rows. Single-entity: `forbidden(Order, status == x)`.
    if pred.name == "forbidden" || pred.name == "invariant" {
        let leading_entities = pred
            .args
            .iter()
            .take_while(|a| matches!(a, PredicateArg::Ref(_)))
            .count();
        let is_cross = pred.chain.iter().any(|cc| cc.name == "along")
            || leading_entities >= 2
            || (pred.name == "invariant"
                && pred
                    .chain
                    .iter()
                    .any(|cc| matches!(cc.name.as_str(), "has" | "none")));
        if is_cross {
            match pred.name.as_str() {
                "forbidden" => process_cross_forbidden(model, pred, ctx, diags),
                "invariant" => process_cross_invariant(model, pred, ctx, diags),
                _ => {}
            }
            return;
        }
        if let Some(role) = pred.chain.iter().find(|cc| cc.name == "role") {
            push_error(ctx, diags, role.span.clone(), RdraError::RoleWithoutJoin);
        }
    }

    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    validate_predicate_arg_types(pred, &sig, &resolved, ctx, diags);
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
        "drives" => process_drives_predicate(model, pred, &resolved, ctx, diags),
        "chooses" => process_chooses_predicate(model, pred, &resolved, ctx, diags),
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
        "navigates" => process_navigates_predicate(model, pred, &resolved, ctx, diags),
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
        "sets" => ("2".into(), got == 2),
        "drives" => ("3".into(), got == 3),
        "after" => ("at least 1".into(), got >= 1),
        "forbidden" | "invariant" | "required" | "exclusive" => ("at least 1".into(), got >= 1),
        "holds" => ("at least 2".into(), got >= 2),
        "transitions" => ("3".into(), got == 3),
        "chooses" => ("2 or 3".into(), got == 2 || got == 3),
        "maps_field" => ("2".into(), got == 2),
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
