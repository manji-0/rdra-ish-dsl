//! The published-entity rules an owner's `check` enforces, read from one table of entity uses.

use std::collections::HashSet;

use rdra_ish_syntax::ast::{ChainCall, PredicateArg};

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::LocatedSpan;
use crate::model::{
    CallerKind, CombinesTarget, CrossCmpRhs, CrossEntityCondition, EntityKey, Exposed, NodeRef,
    PublishMode, RelKind, SemanticModel,
};
use crate::tx::infer_usecase_transactions;

/// Why an `exposes` line publishes nothing.
pub enum ExposeError {
    WrongKind { chain: String, hint: String },
    BadCaller { args: Vec<PredicateArg> },
    NoMode,
}

/// What `exposes(System, target)` with `chain` publishes. `None` for a target kind the
/// signature already rejected.
pub fn exposed_item(target: &NodeRef, chain: &[ChainCall]) -> Option<Result<Exposed, ExposeError>> {
    let wrong_kind = |cc: &ChainCall, hint: &str| ExposeError::WrongKind {
        chain: cc.name.clone(),
        hint: hint.to_string(),
    };
    let item = match target {
        NodeRef::UseCase(usecase) => {
            let mut caller = CallerKind::default();
            for cc in chain {
                if cc.name != "caller" {
                    return Some(Err(wrong_kind(
                        cc,
                        "`.append()` and `.upsert()` publish an entity",
                    )));
                }
                caller = match cc.args.as_slice() {
                    [PredicateArg::Ref(q)] if q.parts.len() == 1 => match q.parts[0].as_str() {
                        "actor" => CallerKind::Actor,
                        "system" => CallerKind::System,
                        "any" => CallerKind::Any,
                        _ => {
                            return Some(Err(ExposeError::BadCaller {
                                args: cc.args.clone(),
                            }))
                        }
                    },
                    _ => {
                        return Some(Err(ExposeError::BadCaller {
                            args: cc.args.clone(),
                        }))
                    }
                };
            }
            Exposed::UseCase {
                usecase: *usecase,
                caller,
            }
        }
        NodeRef::Event(event) => {
            if let Some(cc) = chain.first() {
                return Some(Err(wrong_kind(
                    cc,
                    "an exposed event takes no chain method",
                )));
            }
            Exposed::Event(*event)
        }
        NodeRef::Entity(entity) => {
            let mut mode = None;
            for cc in chain {
                let next = match cc.name.as_str() {
                    "append" => PublishMode::Append,
                    "upsert" => PublishMode::Upsert,
                    _ => {
                        return Some(Err(wrong_kind(
                            cc,
                            "`.caller(...)` applies to an exposed use case",
                        )))
                    }
                };
                if !cc.args.is_empty() {
                    return Some(Err(wrong_kind(
                        cc,
                        "`.append()` and `.upsert()` take no argument",
                    )));
                }
                if mode.is_some() {
                    return Some(Err(wrong_kind(
                        cc,
                        "choose one of `.append()` or `.upsert()`",
                    )));
                }
                mode = Some(next);
            }
            let Some(mode) = mode else {
                return Some(Err(ExposeError::NoMode));
            };
            Exposed::Entity {
                entity: *entity,
                mode,
            }
        }
        _ => return None,
    };
    Some(Ok(item))
}

enum Use {
    Read(&'static str),
    Create,
    Update(&'static str),
    Set,
    Delete,
    Transition,
    Key(EntityKey),
}

struct EntityUse {
    entity: EntityKey,
    what: Use,
    site: Option<LocatedSpan>,
}

fn distinct<'a>(
    scope: &[EntityKey],
    conds: impl Iterator<Item = &'a CrossEntityCondition>,
) -> Vec<EntityKey> {
    let mut out: Vec<EntityKey> = scope
        .iter()
        .copied()
        .chain(conds.flat_map(cond_entities))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn cond_entities(cond: &CrossEntityCondition) -> Vec<EntityKey> {
    match cond {
        CrossEntityCondition::Equals { column, .. } => vec![column.entity],
        CrossEntityCondition::Comparison(p) => match &p.rhs {
            CrossCmpRhs::Column(rc) => vec![p.lhs.entity, rc.entity],
            _ => vec![p.lhs.entity],
        },
    }
}

fn entity_uses(model: &SemanticModel) -> Vec<EntityUse> {
    let mut uses = Vec::new();
    let mut push = |entity: EntityKey, what: Use, site: &Option<LocatedSpan>| {
        uses.push(EntityUse {
            entity,
            what,
            site: site.clone(),
        })
    };
    for rel in &model.relations {
        let NodeRef::Entity(entity) = rel.to else {
            continue;
        };
        let what = match rel.kind {
            RelKind::Reads => Use::Read("reads"),
            RelKind::Shows => Use::Read("shows"),
            RelKind::MapsField => Use::Read("maps_field"),
            RelKind::Creates => Use::Create,
            RelKind::Updates => Use::Update("updates"),
            RelKind::Writes => Use::Update("writes"),
            RelKind::Deletes => Use::Delete,
            _ => continue,
        };
        push(entity, what, &rel.site);
    }
    for t in &model.state_transitions {
        push(t.entity, Use::Transition, &t.site);
    }
    for e in &model.column_effects {
        push(e.entity, Use::Set, &e.site);
    }
    for e in &model.proposition_effects {
        push(e.entity, Use::Set, &e.site);
    }
    for cond in model.conditions.values() {
        if let Some(body) = &cond.body {
            for entity in body.read_conditions().flat_map(cond_entities) {
                push(entity, Use::Read("holds"), &body.site);
            }
        }
    }
    for c in &model.combines {
        if let CombinesTarget::StateModel { entity, .. } = &c.target {
            push(*entity, Use::Read("combines"), &c.location);
        }
    }
    for c in &model.forbidden_constraints {
        push(c.entity, Use::Read("forbidden"), &c.site);
    }
    for c in &model.entity_invariants {
        push(c.entity, Use::Read("invariant"), &c.site);
    }
    for c in &model.required_constraints {
        push(c.entity, Use::Read("required"), &c.site);
    }
    for c in &model.exclusive_constraints {
        push(c.entity, Use::Read("exclusive"), &c.site);
    }
    for c in &model.cross_forbidden_constraints {
        for entity in distinct(&c.scope, c.conditions.iter()) {
            push(entity, Use::Read("forbidden"), &c.site);
        }
    }
    for c in &model.cross_entity_invariants {
        for entity in distinct(&c.scope, c.guards.iter().chain(&c.requireds)) {
            push(entity, Use::Read("invariant"), &c.site);
        }
    }
    for a in &model.temporal_assertions {
        for entity in distinct(&a.scope, a.requireds.iter()) {
            push(entity, Use::Read("after"), &a.site);
        }
    }
    for q in &model.quantifier_constraints {
        push(q.anchor, Use::Read("when"), &q.quantifier.site);
        push(q.quantifier.related, Use::Read("when"), &q.quantifier.site);
    }
    for (_, fk) in model.foreign_keys() {
        push(fk.child, Use::Key(fk.parent), &fk.site);
        push(fk.parent, Use::Key(fk.child), &fk.site);
    }
    uses
}

fn at(err: RdraError, site: &Option<LocatedSpan>, warning: bool) -> Diagnostic {
    match (site, warning) {
        (Some(loc), true) => Diagnostic::warning_at(err, loc.clone()),
        (Some(loc), false) => Diagnostic::error_at(err, loc.clone()),
        (None, true) => Diagnostic::warning(err),
        (None, false) => Diagnostic::error(err),
    }
}

fn mutated(
    model: &SemanticModel,
    entity: EntityKey,
    mode: PublishMode,
    predicate: &str,
) -> RdraError {
    let id = model.entities[entity].id.clone();
    let hint = match predicate {
        "deletes" => format!("a published row is never deleted; write its end state with `sets(UseCase, {id}, col == val)`"),
        "transitions" => format!("a transition reads the row's current state, and the owner only writes {id}; use `sets(UseCase, {id}, col == val)`"),
        _ => format!("an append entity only grows; add a row with `creates(UseCase, {id})` and `sets(...)`, or publish it with `.upsert()`"),
    };
    RdraError::PublishedEntityMutated {
        entity: id,
        mode: mode.as_str().to_string(),
        predicate: predicate.to_string(),
        hint,
    }
}

/// The published-entity rules, `SharedEntityAlsoExposed`, and the `SharedInternalEntity` warning.
pub fn publication_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let publication = &model.publication;
    if publication.is_empty() {
        return Vec::new();
    }
    let mut diags = Vec::new();
    let entity_id = |entity: EntityKey| model.entities[entity].id.clone();
    let system_id = |system| model.systems[system].id.clone();

    for s in &publication.shares {
        let diag = match publication.mode_of(s.entity) {
            Some(mode) => at(
                RdraError::SharedEntityAlsoExposed {
                    system: system_id(s.system),
                    entity: entity_id(s.entity),
                    mode: mode.as_str().to_string(),
                },
                &s.site,
                false,
            ),
            None => at(
                RdraError::SharedInternalEntity {
                    system: system_id(s.system),
                    entity: entity_id(s.entity),
                },
                &s.site,
                true,
            ),
        };
        diags.push(diag);
    }

    let mut written: HashSet<EntityKey> = HashSet::new();
    for u in entity_uses(model) {
        let Some(mode) = publication.mode_of(u.entity) else {
            continue;
        };
        let err = match u.what {
            Use::Read(predicate) => RdraError::PublishedEntityReadByOwner {
                entity: entity_id(u.entity),
                predicate: predicate.to_string(),
            },
            Use::Create | Use::Set => {
                written.insert(u.entity);
                continue;
            }
            Use::Update(predicate) => {
                written.insert(u.entity);
                if mode == PublishMode::Upsert {
                    continue;
                }
                mutated(model, u.entity, mode, predicate)
            }
            Use::Delete => mutated(model, u.entity, mode, "deletes"),
            Use::Transition => mutated(model, u.entity, mode, "transitions"),
            Use::Key(other) => {
                if publication.is_published(other) {
                    continue;
                }
                RdraError::PublishedEntityKeysInternal {
                    entity: entity_id(u.entity),
                    other: entity_id(other),
                }
            }
        };
        diags.push(at(err, &u.site, false));
    }

    for x in &publication.exposures {
        let Exposed::Entity { entity, .. } = x.item else {
            continue;
        };
        if written.contains(&entity) {
            continue;
        }
        diags.push(at(
            RdraError::PublishedEntityNeverWritten {
                entity: entity_id(entity),
            },
            &x.site,
            true,
        ));
    }

    for utx in infer_usecase_transactions(model) {
        let is_published = |w: &crate::tx::UcWrite| publication.is_published(w.entity);
        let internal = utx
            .groups()
            .filter(|g| !g.ordered_writes.iter().all(is_published))
            .count();
        for g in utx.groups() {
            if !g.ordered_writes.iter().all(is_published) {
                continue;
            }
            for w in &g.ordered_writes {
                let site = model
                    .relations
                    .iter()
                    .find(|r| {
                        r.to == NodeRef::Entity(w.entity)
                            && matches!(
                                r.kind,
                                RelKind::Creates | RelKind::Updates | RelKind::Writes
                            )
                            && match (&r.from, w.via_api) {
                                (NodeRef::Api(a), Some(via)) => *a == via,
                                (NodeRef::UseCase(u), None) => *u == utx.usecase,
                                _ => false,
                            }
                    })
                    .and_then(|r| r.site.clone())
                    .or_else(|| {
                        model
                            .column_effects
                            .iter()
                            .find(|e| e.origin == utx.usecase && e.entity == w.entity)
                            .and_then(|e| e.site.clone())
                    });
                let usecase = model.use_cases[utx.usecase].id.clone();
                let entity = entity_id(w.entity);
                let hint = match (w.via_api, internal) {
                    (Some(api), _) => format!(
                        "'{usecase}' writes '{entity}' through API '{}', which is its own transaction; route the internal change and '{entity}' through one API",
                        model.apis[api].id
                    ),
                    (None, 0) => format!(
                        "'{usecase}' makes no internal change; write '{entity}' from the use case that makes the change it reports"
                    ),
                    (None, n) => format!(
                        "'{usecase}' has {n} internal transactions, so '{entity}' has no one transaction to join; write it from a use case that makes exactly one internal change, or route both writes through one API"
                    ),
                };
                diags.push(at(
                    RdraError::PublishedNotInSourceTx {
                        usecase,
                        entity,
                        hint,
                    },
                    &site,
                    false,
                ));
            }
        }
    }
    diags
}
