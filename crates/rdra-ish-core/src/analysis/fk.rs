//! The key facts: one `ForeignKey` per relate whose child holds the key, and the key
//! columns each one generates on its child.

use super::constraint::arg_as_str;
use crate::analysis_diag::{push_entity_error, push_error};
use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::{DiagCtxt, LocatedSpan};
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
            min: rel.options.children_min,
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

/// The `(child column, parent key column)` pairs of key `index`, one per parent key column:
/// the `.key(..)` columns when the relate names them, otherwise `{role}_{pk}` columns
/// appended to the child. `role` is lowercase.
fn key_columns(
    model: &mut SemanticModel,
    diags: &mut Vec<Diagnostic>,
    index: usize,
    role: &str,
) -> Option<Vec<(String, String)>> {
    let fk = &model.foreign_keys[index];
    let parent = &model.entities[fk.parent];
    let pk: Option<Vec<(String, ColumnType)>> = parent
        .primary_key
        .iter()
        .map(|name| {
            let column = parent.columns.iter().find(|c| c.name == *name)?;
            Some((column.name.clone(), column.col_type.clone()))
        })
        .collect();
    let pk = match pk {
        Some(pk) if !pk.is_empty() => pk,
        _ => {
            let entity = parent.id.clone();
            let err = RdraError::MissingPk {
                entity: entity.clone(),
            };
            push_entity_error(model, diags, &entity, err);
            return None;
        }
    };
    let declared = &model.relations[fk.relation].options.key;
    if declared.is_empty() {
        generate_key_columns(model, diags, index, role, &pk)
    } else {
        let declared = declared.clone();
        adopt_key_columns(model, diags, index, &declared, &pk)
    }
}

/// Appends `{role}_{pk}` per parent key column, nullable for an optional key and unique for
/// a single-column 1:1 (a composite 1:1 is a `unique_keys` set).
fn generate_key_columns(
    model: &mut SemanticModel,
    diags: &mut Vec<Diagnostic>,
    index: usize,
    role: &str,
    pk: &[(String, ColumnType)],
) -> Option<Vec<(String, String)>> {
    let fk = &model.foreign_keys[index];
    let (child_key, optional, unique) = (fk.child, fk.optional(), fk.unique());
    let child = &model.entities[child_key];
    let names: Vec<String> = pk.iter().map(|(pk, _)| format!("{role}_{pk}")).collect();
    if let Some(col) = names
        .iter()
        .find(|name| child.columns.iter().any(|c| c.name == **name))
    {
        let err = RdraError::FkConflict {
            entity: child.id.clone(),
            col: col.clone(),
        };
        push_entity_error(model, diags, &child.id, err);
        return None;
    }

    for (name, (_, col_type)) in names.iter().zip(pk) {
        model.entities[child_key].columns.push(ModelColumn {
            name: name.clone(),
            col_type: col_type.clone(),
            is_pk: false,
            is_unique: unique && pk.len() == 1,
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
    }
    Some(
        names
            .into_iter()
            .zip(pk.iter().map(|(pk, _)| pk.clone()))
            .collect(),
    )
}

/// Checks that `.key(..)` names one child column per parent key column, of the same type and
/// held by no other key, then gives those columns the key's nullability and 1:1 uniqueness.
fn adopt_key_columns(
    model: &mut SemanticModel,
    diags: &mut Vec<Diagnostic>,
    index: usize,
    declared: &[String],
    pk: &[(String, ColumnType)],
) -> Option<Vec<(String, String)>> {
    let fk = &model.foreign_keys[index];
    let (child_key, optional, unique) = (fk.child, fk.optional(), fk.unique());
    let child = &model.entities[child_key];
    let child_id = child.id.clone();
    let err = if declared.len() != pk.len() {
        Some(RdraError::KeyArity {
            child: child_id.clone(),
            parent: model.entities[fk.parent].id.clone(),
            got: declared.len(),
            want: pk.len(),
        })
    } else {
        declared
            .iter()
            .zip(pk)
            .find_map(|(name, (pk_name, pk_type))| {
                let Some(column) = child.columns.iter().find(|c| c.name == *name) else {
                    return Some(RdraError::KeyColumnUndefined {
                        child: child_id.clone(),
                        column: name.clone(),
                    });
                };
                if column.col_type != *pk_type {
                    return Some(RdraError::KeyTypeMismatch {
                        child: child_id.clone(),
                        column: name.clone(),
                        got: type_name(model, &column.col_type),
                        parent_column: pk_name.clone(),
                        want: type_name(model, pk_type),
                    });
                }
                model
                    .column_fk(child_key, name)
                    .map(|_| RdraError::KeyColumnShared {
                        child: child_id.clone(),
                        column: name.clone(),
                    })
            })
    };
    if let Some(err) = err {
        push_relate_error(model, diags, index, err);
        return None;
    }

    for column in model.entities[child_key].columns.iter_mut() {
        if declared.contains(&column.name) {
            column.is_nullable = optional;
            column.is_unique |= unique && pk.len() == 1;
        }
    }
    Some(
        declared
            .iter()
            .cloned()
            .zip(pk.iter().map(|(pk, _)| pk.clone()))
            .collect(),
    )
}

fn type_name(model: &SemanticModel, col_type: &ColumnType) -> String {
    match col_type {
        ColumnType::Int => "Int".to_string(),
        ColumnType::String => "String".to_string(),
        ColumnType::Money => "Money".to_string(),
        ColumnType::DateTime => "DateTime".to_string(),
        ColumnType::Date => "Date".to_string(),
        ColumnType::Bool => "Bool".to_string(),
        ColumnType::Decimal => "Decimal".to_string(),
        ColumnType::Enum(values) => format!("Enum({})", values.join(", ")),
        ColumnType::Variation(key) => model
            .variations
            .get(*key)
            .map_or_else(|| "variation".to_string(), |v| v.id.clone()),
    }
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

/// Fills each `.along(..)` path's `links`: the one key between each pair of neighbours, or
/// the one whose role `.by(..)` names when several join them.
pub(crate) fn resolve_along_links(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let scopes = model
        .cross_forbidden_constraints
        .iter()
        .map(|c| (&c.scope_semantics, &c.site))
        .chain(
            model
                .cross_entity_invariants
                .iter()
                .map(|c| (&c.scope_semantics, &c.site)),
        );
    let resolved: Vec<Vec<Option<FkId>>> = scopes
        .map(|(scope, site)| match scope {
            CrossConstraintScope::RelationPath { path, by, .. } => {
                along_links(model, path, by.as_deref(), site.as_ref(), diags)
            }
            CrossConstraintScope::GlobalProduct => Vec::new(),
        })
        .collect();
    let scopes = model
        .cross_forbidden_constraints
        .iter_mut()
        .map(|c| &mut c.scope_semantics)
        .chain(
            model
                .cross_entity_invariants
                .iter_mut()
                .map(|c| &mut c.scope_semantics),
        );
    for (scope, resolved) in scopes.zip(resolved) {
        if let CrossConstraintScope::RelationPath { links, .. } = scope {
            *links = resolved;
        }
    }
}

fn along_links(
    model: &SemanticModel,
    path: &[EntityKey],
    by: Option<&str>,
    site: Option<&LocatedSpan>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<FkId>> {
    let push = |diags: &mut Vec<Diagnostic>, err| {
        diags.push(match site {
            Some(site) => Diagnostic::error_at(err, site.clone()),
            None => Diagnostic::error(err),
        });
    };
    let role = |fk: &ForeignKey| fk.role_or(&model.entities[fk.parent].id).to_string();
    let mut by_used = false;
    let mut path_roles: Vec<String> = Vec::new();
    let mut links = Vec::new();
    for pair in path.windows(2) {
        let between: Vec<(FkId, &ForeignKey)> = model
            .foreign_keys()
            .filter(|(_, fk)| {
                (fk.child, fk.parent) == (pair[0], pair[1])
                    || (fk.child, fk.parent) == (pair[1], pair[0])
            })
            .collect();
        path_roles.extend(between.iter().map(|(_, fk)| role(fk)));
        let named: Vec<FkId> = between
            .iter()
            .filter(|(_, fk)| by.is_some_and(|by| role(fk).eq_ignore_ascii_case(by)))
            .map(|&(id, _)| id)
            .collect();
        by_used |= !named.is_empty();
        links.push(match (between.as_slice(), named.as_slice()) {
            ([], _) => None,
            ([(id, _)], _) | (_, [id]) => Some(*id),
            _ => {
                let roles: Vec<String> = between.iter().map(|(_, fk)| role(fk)).collect();
                push(
                    diags,
                    RdraError::AmbiguousOwnerLink {
                        a: model.entities[pair[0]].id.clone(),
                        b: model.entities[pair[1]].id.clone(),
                        first: roles[0].clone(),
                        roles: roles.join(", "),
                    },
                );
                None
            }
        });
    }
    if let Some(by) = by.filter(|_| !by_used) {
        let along: Vec<&str> = path
            .iter()
            .map(|e| model.entities[*e].id.as_str())
            .collect();
        push(
            diags,
            RdraError::UnknownOwnerRole {
                role: by.to_string(),
                along: along.join(", "),
                roles: path_roles.join(", "),
            },
        );
    }
    links
}
