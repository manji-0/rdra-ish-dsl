//! The key facts: one `ForeignKey` per relate whose child holds the key, and the key
//! columns each one generates on its child.

use super::constraint::arg_as_str;
use crate::analysis_diag::{push_entity_error, push_error};
use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::PredicateCall;
use std::collections::{BTreeMap, BTreeSet};

/// `.role(..)`, `.key(..)`, `.optional()`, `.on_delete(..)` and `.on_update(..)` of a relate
/// chain.
pub(crate) fn relation_options(
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> RelationOptions {
    let mut options = RelationOptions::default();
    for cc in &pred.chain {
        let slot = match cc.name.as_str() {
            "role" => {
                options.role = cc.args.first().and_then(arg_as_str);
                continue;
            }
            "key" => {
                options.key = cc.args.iter().filter_map(arg_as_str).collect();
                continue;
            }
            "optional" => {
                options.optional = true;
                continue;
            }
            "on_delete" => &mut options.on_delete,
            "on_update" => &mut options.on_update,
            _ => continue,
        };
        let Some(text) = cc.args.first().and_then(arg_as_str) else {
            continue;
        };
        match RefAction::parse(&text) {
            Some(action) => *slot = Some(action),
            None => push_error(
                ctx,
                diags,
                cc.span.clone(),
                RdraError::UnknownRefAction {
                    method: cc.name.clone(),
                    got: text,
                },
            ),
        }
    }
    options
}

/// Writes `model.foreign_keys` and `model.junction_keys`, and appends each key's generated
/// columns to its child. A key the child cannot hold (a second key with the same role, no
/// parent `@pk`, a composite parent key, a name clash) is reported and keeps no columns.
pub(crate) fn resolve_foreign_keys(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    model.foreign_keys = model
        .relations
        .iter()
        .enumerate()
        .filter_map(|(relation, rel)| key_fact(relation, rel))
        .collect();

    let mut roles: BTreeSet<(EntityKey, String)> = BTreeSet::new();
    for index in 0..model.foreign_keys.len() {
        let fk = &model.foreign_keys[index];
        let role = fk.role_or(&model.entities[fk.parent].id).to_string();
        if !roles.insert((fk.child, role.to_lowercase())) {
            let err = RdraError::DuplicateRelateRole {
                child: model.entities[fk.child].id.clone(),
                role,
            };
            push_relate_error(model, diags, index, err);
            continue;
        }
        if let Some(columns) = key_columns(model, diags, index, &role.to_lowercase()) {
            model.foreign_keys[index].columns = columns;
        }
    }

    resolve_junction_keys(model, diags);
}

/// At the relate that yields key `index`, or at its child when the relate was synthesized.
fn push_relate_error(
    model: &SemanticModel,
    diags: &mut Vec<Diagnostic>,
    index: usize,
    err: RdraError,
) {
    let fk = &model.foreign_keys[index];
    match &fk.site {
        Some(site) => diags.push(Diagnostic::error_at(err, site.clone())),
        None => push_entity_error(model, diags, &model.entities[fk.child].id, err),
    }
}

fn key_fact(relation: usize, rel: &Relation) -> Option<ForeignKey> {
    let (NodeRef::Entity(from), NodeRef::Entity(to)) = (&rel.from, &rel.to) else {
        return None;
    };
    let (child, parent, max) = match rel.kind {
        RelKind::RelateManyToOne => (*from, *to, Max::Many),
        RelKind::RelateOneToMany => (*to, *from, Max::Many),
        RelKind::RelateOneToOne => (*from, *to, Max::One),
        _ => return None,
    };
    Some(ForeignKey {
        child,
        parent,
        role: rel.options.role.clone(),
        columns: Vec::new(),
        children: Multiplicity {
            min: Min::Zero,
            max,
        },
        parent_min: if rel.options.optional {
            Min::Zero
        } else {
            Min::One
        },
        on_delete: rel.options.on_delete,
        on_update: rel.options.on_update,
        relation,
        site: rel.site.clone(),
    })
}

/// Appends `{role}_{pk}` to the child, unique for a 1:1, and returns the `(child column,
/// parent key column)` pairs. `role` is lowercase.
fn key_columns(
    model: &mut SemanticModel,
    diags: &mut Vec<Diagnostic>,
    index: usize,
    role: &str,
) -> Option<Vec<(String, String)>> {
    let (child_key, parent_key, optional, unique) = {
        let fk = &model.foreign_keys[index];
        (fk.child, fk.parent, fk.optional(), fk.unique())
    };
    let parent = &model.entities[parent_key];
    if parent.primary_key.len() > 1 {
        push_entity_error(
            model,
            diags,
            &parent.id,
            RdraError::CompositePkFkUnsupported {
                entity: parent.id.clone(),
            },
        );
        return None;
    }
    let pk = parent.columns.iter().find(|c| c.is_pk).or_else(|| {
        parent
            .primary_key
            .first()
            .and_then(|name| parent.columns.iter().find(|c| c.name == *name))
    });
    let Some(pk) = pk else {
        push_entity_error(
            model,
            diags,
            &parent.id,
            RdraError::MissingPk {
                entity: parent.id.clone(),
            },
        );
        return None;
    };
    let (pk_name, pk_type) = (pk.name.clone(), pk.col_type.clone());
    let name = format!("{role}_{pk_name}");

    let child = &model.entities[child_key];
    if child.columns.iter().any(|c| c.name == name) {
        push_entity_error(
            model,
            diags,
            &child.id,
            RdraError::FkConflict {
                entity: child.id.clone(),
                col: name,
            },
        );
        return None;
    }

    model.entities[child_key].columns.push(ModelColumn {
        name: name.clone(),
        col_type: pk_type,
        is_pk: false,
        is_unique: unique,
        is_indexed: false,
        is_nullable: optional,
        default_val: None,
        label: None,
        check_constraints: Vec::new(),
        is_soft_delete: false,
        is_history: false,
        is_tenant_scope: false,
        derived_expr: None,
    });
    Some(vec![(name, pk_name)])
}

/// Each N:M junction's two keys, which `unique_keys` reads as a composite unique set. A
/// junction shared by two N:M pairs is an error and gets none.
fn resolve_junction_keys(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let mut ends: BTreeMap<EntityKey, Vec<(EntityKey, EntityKey)>> = BTreeMap::new();
    for rel in &model.relations {
        if let (
            RelKind::RelateManyToMany,
            NodeRef::Entity(a),
            NodeRef::Entity(b),
            Some(NodeRef::Entity(junction)),
        ) = (&rel.kind, &rel.from, &rel.to, rel.via.as_ref())
        {
            ends.entry(*junction).or_default().push((*a, *b));
        }
    }

    let mut junction_keys = Vec::new();
    for (junction, pairs) in &ends {
        let distinct: BTreeSet<(EntityKey, EntityKey)> = pairs
            .iter()
            .map(|&(a, b)| {
                if model.entities[a].id <= model.entities[b].id {
                    (a, b)
                } else {
                    (b, a)
                }
            })
            .collect();
        if distinct.len() > 1 {
            let junction_id = model.entities[*junction].id.clone();
            let pairs = distinct
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
                    pairs,
                },
            );
            continue;
        }
        let (a, b) = pairs[0];
        let key_to = |end: EntityKey| {
            model
                .foreign_keys()
                .find(|(_, fk)| fk.child == *junction && fk.parent == end && !fk.columns.is_empty())
                .map(|(id, _)| id)
        };
        if let (Some(fa), Some(fb)) = (key_to(a), key_to(b)) {
            junction_keys.push((*junction, fa, fb));
        }
    }
    model.junction_keys = junction_keys;
}
