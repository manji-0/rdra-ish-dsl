//! Foreign-key generation from relate predicates.

use crate::analysis_diag::push_entity_error;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::*;
use std::collections::BTreeSet;

pub(crate) fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let mut jobs: Vec<(EntityKey, EntityKey, RelationOptions)> = Vec::new();
    let mut junctions: BTreeSet<EntityKey> = BTreeSet::new();

    for rel in &model.relations {
        match &rel.kind {
            RelKind::RelateManyToOne => {
                if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (&rel.from, &rel.to) {
                    jobs.push((*fk, *tk, rel.options.clone()));
                }
            }
            RelKind::RelateOneToMany => {
                if let (NodeRef::Entity(ok), NodeRef::Entity(mk)) = (&rel.from, &rel.to) {
                    jobs.push((*mk, *ok, rel.options.clone()));
                }
            }
            // N:M.via FKs come from synthesized N:1 edges (see process_relate_predicate).
            RelKind::RelateManyToMany => {
                if let Some(NodeRef::Entity(junction)) = rel.via.as_ref() {
                    junctions.insert(*junction);
                }
            }
            _ => {}
        }
    }

    for (many_key, one_key, options) in jobs {
        generate_fk_column(model, diags, many_key, one_key, options);
    }

    for junction in junctions {
        ensure_junction_composite_unique(model, junction);
    }
}

fn ensure_junction_composite_unique(model: &mut SemanticModel, junction: EntityKey) {
    let fk_cols: Vec<String> = model.entities[junction]
        .columns
        .iter()
        .filter(|c| c.is_fk)
        .map(|c| c.name.clone())
        .collect();
    if fk_cols.len() < 2 {
        return;
    }
    // Prefer the two FKs toward the N:M ends (stable order by name).
    let mut pair = fk_cols;
    pair.sort();
    pair.truncate(2);

    let already = model.entities[junction].unique_constraints.iter().any(|u| {
        let mut sorted = u.clone();
        sorted.sort();
        sorted == pair
    });
    if !already {
        model.entities[junction].unique_constraints.push(pair);
    }
}

fn generate_fk_column(
    model: &mut SemanticModel,
    diags: &mut Vec<Diagnostic>,
    many_key: EntityKey,
    one_key: EntityKey,
    options: RelationOptions,
) {
    let (one_id, pk_type) = {
        let one = &model.entities[one_key];
        if one.primary_key.len() > 1 {
            push_entity_error(
                model,
                diags,
                &one.id,
                RdraError::CompositePkFkUnsupported {
                    entity: one.id.clone(),
                },
            );
            return;
        }
        let pk = one.columns.iter().find(|c| c.is_pk).or_else(|| {
            one.primary_key
                .first()
                .and_then(|name| one.columns.iter().find(|c| c.name == *name))
        });
        match pk {
            Some(col) => (one.id.clone(), col.col_type.clone()),
            None => {
                push_entity_error(
                    model,
                    diags,
                    &one.id,
                    RdraError::MissingPk {
                        entity: one.id.clone(),
                    },
                );
                return;
            }
        }
    };

    let fk_col_name = format!("{}_id", one_id.to_lowercase());

    let many_entity_id = model.entities[many_key].id.clone();
    if model.entities[many_key]
        .columns
        .iter()
        .any(|c| c.name == fk_col_name)
    {
        push_entity_error(
            model,
            diags,
            &many_entity_id,
            RdraError::FkConflict {
                entity: many_entity_id.clone(),
                col: fk_col_name.clone(),
            },
        );
        return;
    }

    let fk_col = ModelColumn {
        name: fk_col_name,
        col_type: pk_type,
        is_pk: false,
        is_unique: false,
        is_indexed: false,
        is_nullable: options.optional,
        default_val: None,
        label: None,
        is_fk: true,
        fk_target: Some(one_id),
        fk_optional: options.optional,
        fk_on_delete: options.on_delete,
        fk_on_update: options.on_update,
        check_constraints: Vec::new(),
        is_soft_delete: false,
        is_history: false,
        is_tenant_scope: false,
        derived_expr: None,
    };
    model.entities[many_key].columns.push(fk_col);
}
