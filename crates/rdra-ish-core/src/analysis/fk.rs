//! Foreign-key generation from relate predicates.

use crate::analysis_diag::push_entity_error;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::*;

pub(crate) fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let rels: Vec<_> = model
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelKind::RelateManyToOne | RelKind::RelateOneToMany))
        .map(|r| {
            (
                r.from.clone(),
                r.to.clone(),
                r.kind.clone(),
                r.options.clone(),
            )
        })
        .collect();

    for (from, to, kind, options) in rels {
        let (many_key, one_key) = match kind {
            RelKind::RelateManyToOne => {
                if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (&from, &to) {
                    (*fk, *tk)
                } else {
                    continue;
                }
            }
            RelKind::RelateOneToMany => {
                if let (NodeRef::Entity(ok), NodeRef::Entity(mk)) = (&from, &to) {
                    (*mk, *ok)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

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
                continue;
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
                    continue;
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
            continue;
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
}
