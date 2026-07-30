//! Foreign-key generation from relate predicates.

use crate::analysis_diag::push_entity_error;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let mut jobs: Vec<(EntityKey, EntityKey, RelationOptions)> = Vec::new();
    // Junction → N:M endpoint pairs (for composite unique + shared-via detection).
    let mut junction_nms: BTreeMap<EntityKey, Vec<(EntityKey, EntityKey)>> = BTreeMap::new();

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
                if let (NodeRef::Entity(a), NodeRef::Entity(b), Some(NodeRef::Entity(junction))) =
                    (&rel.from, &rel.to, rel.via.as_ref())
                {
                    junction_nms.entry(*junction).or_default().push((*a, *b));
                }
            }
            _ => {}
        }
    }

    for (many_key, one_key, options) in jobs {
        generate_fk_column(model, diags, many_key, one_key, options);
    }

    for (junction, ends) in &junction_nms {
        let mut unique_pairs: BTreeSet<(EntityKey, EntityKey)> = BTreeSet::new();
        for &(a, b) in ends {
            let pair = if model.entities[a].id <= model.entities[b].id {
                (a, b)
            } else {
                (b, a)
            };
            unique_pairs.insert(pair);
        }
        if unique_pairs.len() > 1 {
            let junction_id = model.entities[*junction].id.clone();
            let pairs_desc = unique_pairs
                .iter()
                .map(|(a, b)| format!("{}×{}", model.entities[*a].id, model.entities[*b].id))
                .collect::<Vec<_>>()
                .join(", ");
            push_entity_error(
                model,
                diags,
                &junction_id,
                RdraError::SharedJunctionVia {
                    junction: junction_id.clone(),
                    pairs: pairs_desc,
                },
            );
            continue;
        }
        if let Some(&(a, b)) = ends.first() {
            ensure_junction_composite_unique(model, *junction, a, b);
        }
    }
}

fn fk_col_name_for(model: &SemanticModel, target: EntityKey) -> String {
    format!("{}_id", model.entities[target].id.to_lowercase())
}

fn ensure_junction_composite_unique(
    model: &mut SemanticModel,
    junction: EntityKey,
    end_a: EntityKey,
    end_b: EntityKey,
) {
    let name_a = fk_col_name_for(model, end_a);
    let name_b = fk_col_name_for(model, end_b);
    let has_a = model.entities[junction]
        .columns
        .iter()
        .any(|c| c.is_fk && c.name == name_a);
    let has_b = model.entities[junction]
        .columns
        .iter()
        .any(|c| c.is_fk && c.name == name_b);
    if !has_a || !has_b {
        return;
    }
    let mut pair = vec![name_a, name_b];
    pair.sort();

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
