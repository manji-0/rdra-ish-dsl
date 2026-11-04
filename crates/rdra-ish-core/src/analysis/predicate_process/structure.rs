//! Structural predicates: `maps`, `coordinates`, `exposes`, `shares`, and `outbox`.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::predicate_arg_display;
use crate::analysis::arg_resolve::resolve_arg;

pub(super) fn process_maps_field_predicate(
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
                actual: other.kind_tag().to_string(),
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

pub(super) fn process_coordinates_predicate(
    model: &mut SemanticModel,
    resolved: &[Option<NodeRef>],
) {
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

pub(super) fn process_exposes_predicate(
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
    let kind = target.kind_tag().to_string();
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

pub(super) fn process_shares_predicate(
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

pub(super) fn process_outbox_predicate(model: &mut SemanticModel, resolved: &[Option<NodeRef>]) {
    if let Some(Some(NodeRef::Event(event))) = resolved.first() {
        model.outbox_events.insert(*event);
    }
}
