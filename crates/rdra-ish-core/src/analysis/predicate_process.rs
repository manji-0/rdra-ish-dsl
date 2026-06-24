//! Predicate argument resolution and semantic processing.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::predicate::predicate_signature;
use rdra_ish_syntax::ast::*;

use super::arg_resolve::resolve_arg;
use super::comparison::resolve_comparison;
use super::constraint::{
    arg_as_str, collect_entity_conditions, context_value_from_arg, process_after_predicate,
    process_cross_forbidden, process_cross_invariant, process_forbidden_when_predicate,
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
                return if matches!(pred.name.as_str(), "forbidden" | "required" | "exclusive") {
                    None
                } else {
                    resolve_arg(model, arg, ctx, diags)
                };
            };
            if matches!(kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
                return None;
            }
            resolve_arg(model, arg, ctx, diags)
        })
        .collect()
}

fn predicate_arg_display(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Tuple(_) => "<tuple>".to_string(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
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
        if matches!(expected_kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
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
                | (NodeRef::Aggregate(_), NodeRef::DomainObject(_))
                | (NodeRef::Aggregate(_), NodeRef::ValueObject(_))
                | (NodeRef::Aggregate(_), NodeRef::Concept(_))
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
                    expected: "buc->usecase|buc->flow|flow->step|screen->field|system->api|aggregate->domain_object|aggregate->valueobject|aggregate->concept".to_string(),
                },
            );
            return false;
        }
    }
    true
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
        from: NodeRef::Field(*field),
        to: NodeRef::Entity(*entity),
        kind: RelKind::MapsField,
        options: RelationOptions::default(),
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

fn process_maps_to_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (Some(Some(from)), Some(Some(NodeRef::Entity(entity)))) =
        (resolved.first(), resolved.get(1))
    {
        let Some(source) = ConceptualRef::from_node_ref(from) else {
            return;
        };
        model.concept_mappings.push(ConceptMapping {
            source: source.clone(),
            entity: *entity,
        });
        model.relations.push(Relation {
            from: from.clone(),
            to: NodeRef::Entity(*entity),
            kind: RelKind::MapsTo,
            options: RelationOptions::default(),
        });
    }
}

fn process_transitions_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let (
        Some(Some(NodeRef::Event(event))),
        Some(Some(NodeRef::State(state_before))),
        Some(Some(NodeRef::State(state_after))),
    ) = (resolved.first(), resolved.get(1), resolved.get(2))
    {
        model.state_transitions.push(crate::model::StateTransition {
            event: *event,
            from: *state_before,
            to: *state_after,
        });
        model.relations.push(Relation {
            from: NodeRef::State(*state_before),
            to: NodeRef::State(*state_after),
            kind: RelKind::Transitions,
            options: RelationOptions::default(),
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
    let (Some(Some(origin)), Some(Some(entity_ref))) = (resolved.first(), resolved.get(1)) else {
        return;
    };
    let entity_key = match entity_ref {
        NodeRef::Entity(k) => *k,
        _ => return,
    };

    match pred.args.get(2) {
        Some(PredicateArg::Expr(Expr::Cmp(cmp))) => {
            let truth_str = match pred.args.get(3) {
                Some(PredicateArg::Ref(q)) if q.kind_qualifier.is_none() && q.parts.len() == 1 => {
                    q.parts[0].as_str().to_string()
                }
                Some(PredicateArg::Lit(s)) => s.clone(),
                _ => return,
            };
            if truth_str != "true" && truth_str != "false" {
                return;
            }
            let entity_id = model.entities[entity_key].id.clone();
            let entity_cols = model.entities[entity_key].columns.clone();
            if let Some(prop) = resolve_comparison(&entity_cols, &entity_id, cmp, ctx, diags) {
                model.proposition_effects.push(PropositionEffect {
                    origin: origin.clone(),
                    entity: entity_key,
                    prop: prop.clone(),
                    truth: truth_str == "true",
                });
                if let Some(origin) = DataOrigin::from_node_ref(origin) {
                    model
                        .typed_predicates
                        .push(TypedPredicate::SetsProposition {
                            origin,
                            entity: entity_key,
                            prop,
                            truth: truth_str == "true",
                        });
                }
            }
        }
        Some(PredicateArg::Lit(col_name)) => {
            let col_name = col_name.clone();
            let val_lit = match pred.args.get(3) {
                Some(PredicateArg::Lit(s)) => s.clone(),
                _ => return,
            };
            let col = model.entities[entity_key]
                .columns
                .iter()
                .find(|c| c.name == col_name)
                .cloned();
            let Some(col) = col else {
                push_error(
                    ctx,
                    diags,
                    pred.span.clone(),
                    RdraError::UnknownColumn {
                        entity: model.entities[entity_key].id.clone(),
                        col: col_name,
                    },
                );
                return;
            };
            match parse_effect_value(&col, &val_lit) {
                Ok(value) => {
                    model.column_effects.push(ColumnEffect {
                        origin: origin.clone(),
                        entity: entity_key,
                        column: col_name.clone(),
                        value,
                    });
                    if let Some(origin) = DataOrigin::from_node_ref(origin) {
                        model.typed_predicates.push(TypedPredicate::SetsColumn {
                            origin,
                            entity: entity_key,
                            column: col_name,
                        });
                    }
                }
                Err(e) => {
                    let span = pred
                        .args
                        .get(3)
                        .map(arg_span)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| pred.span.clone());
                    push_error(ctx, diags, span, e);
                }
            }
        }
        _ => {}
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
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.forbidden_constraints.push(ForbiddenConstraint {
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
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if !conditions.equals.is_empty() || !conditions.comparisons.is_empty() {
        model.required_constraints.push(RequiredConstraint {
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
    let Some(conditions) =
        collect_entity_conditions(&entity_cols, &entity_id, &pred.args[1..], ctx, diags)
    else {
        return;
    };

    if conditions.equals.len() + conditions.comparisons.len() >= 2 {
        model.exclusive_constraints.push(ExclusiveConstraint {
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
                    match parse_effect_value(&col, &val_str) {
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
        "uses" => RelKind::Uses,
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
        "maps_to" => RelKind::MapsTo,
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
    let mut whens = Vec::new();
    let mut wheres = Vec::new();
    let mut bys = Vec::new();

    for cc in &pred.chain {
        let (target, expected_kind) = match cc.name.as_str() {
            "when" => (&mut whens, "timing"),
            "where" => (&mut wheres, "location"),
            "by" => (&mut bys, "medium"),
            _ => continue,
        };
        for arg in &cc.args {
            if let Some(value) = context_value_from_arg(model, arg, expected_kind, ctx, diags) {
                target.push(value);
            }
        }
    }

    if !whens.is_empty() || !wheres.is_empty() || !bys.is_empty() {
        model
            .business_mapping_contexts
            .push(BusinessMappingContext {
                buc: *buc,
                business: *business,
                whens,
                wheres,
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
            from: from.clone(),
            to: to.clone(),
            kind,
            options: RelationOptions::default(),
        });
        if pred.name == "belongs" {
            process_belongs_context(model, pred, from, to, ctx, diags);
        }
    }
}

fn relation_options_from_chain(pred: &PredicateCall) -> RelationOptions {
    let mut options = RelationOptions::default();
    for cc in &pred.chain {
        match cc.name.as_str() {
            "optional" => options.optional = true,
            "on_delete" => {
                if let Some(value) = cc.args.first().and_then(arg_as_str) {
                    options.on_delete = Some(value);
                }
            }
            "on_update" => {
                if let Some(value) = cc.args.first().and_then(arg_as_str) {
                    options.on_update = Some(value);
                }
            }
            _ => {}
        }
    }
    options
}

fn process_relate_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    if let (Some(Some(from)), Some(Some(to)), Some(PredicateArg::Lit(card))) =
        (resolved.first(), resolved.get(1), pred.args.get(2))
    {
        let kind = match card.as_str() {
            "1:1" => RelKind::RelateOneToOne,
            "1:N" => RelKind::RelateOneToMany,
            "N:1" => RelKind::RelateManyToOne,
            "N:M" => {
                let from_id = match from {
                    NodeRef::Entity(k) => model.entities[*k].id.clone(),
                    _ => "?".into(),
                };
                let to_id = match to {
                    NodeRef::Entity(k) => model.entities[*k].id.clone(),
                    _ => "?".into(),
                };
                push_warning(
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
                RelKind::RelateManyToMany
            }
            _ => return,
        };
        model.relations.push(Relation {
            from: from.clone(),
            to: to.clone(),
            kind,
            options: relation_options_from_chain(pred),
        });
    }
}

pub(crate) fn process_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(sig) = predicate_signature(&pred.name) else {
        return;
    };

    match pred.name.as_str() {
        "cross_forbidden" => {
            process_cross_forbidden(model, pred, ctx, diags);
            return;
        }
        "cross_invariant" => {
            process_cross_invariant(model, pred, ctx, diags);
            return;
        }
        _ => {}
    }

    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    validate_predicate_arg_types(pred, &sig, &resolved, ctx, diags);
    if !validate_contains_pair(pred, &resolved, ctx, diags) {
        return;
    }

    match pred.name.as_str() {
        "coordinates" => process_coordinates_predicate(model, &resolved),
        "maps_to" => process_maps_to_predicate(model, &resolved),
        "transitions" => process_transitions_predicate(model, &resolved),
        "outbox" => process_outbox_predicate(model, &resolved),
        "after" => process_after_predicate(model, pred, &resolved, ctx, diags),
        "sets" => process_sets_predicate(model, pred, &resolved, ctx, diags),
        "forbidden" => process_forbidden_predicate(model, pred, &resolved, ctx, diags),
        "invariant" => process_invariant_predicate(model, pred, &resolved, ctx, diags),
        "required" => process_required_predicate(model, pred, &resolved, ctx, diags),
        "exclusive" => process_exclusive_predicate(model, pred, &resolved, ctx, diags),
        "forbidden_when" => process_forbidden_when_predicate(model, pred, &resolved, ctx, diags),
        "maps_field" => process_maps_field_predicate(model, pred, &resolved, ctx, diags),
        "relate" => process_relate_predicate(model, pred, &resolved, ctx, diags),
        _ => process_relation_predicate(model, pred, &resolved, ctx, diags),
    }

    if let Some(typed) = crate::typed_predicate::build_typed_predicate(&pred.name, &resolved, pred)
    {
        model.typed_predicates.push(typed);
    }
}
