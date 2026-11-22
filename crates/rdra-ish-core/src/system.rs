//! System boundary derivation from API membership and API CRUD.

use std::collections::{HashMap, HashSet};

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::push_model_decl_diagnostic;
use crate::model::{ApiKey, EntityKey, NodeRef, RelKind, SemanticModel, SystemKey};

#[derive(Debug, Clone)]
pub struct SystemBoundary {
    pub system: SystemKey,
    pub apis: Vec<ApiKey>,
    pub entities: Vec<EntityKey>,
}

pub fn derive_system_boundaries(model: &SemanticModel) -> Vec<SystemBoundary> {
    let mut apis_by_system: HashMap<SystemKey, Vec<ApiKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::Contains {
            if let (NodeRef::System(sk), NodeRef::Api(ak)) = (&rel.from, &rel.to) {
                apis_by_system.entry(*sk).or_default().push(*ak);
            }
        }
    }

    let mut owned_entities_by_system: HashMap<SystemKey, Vec<EntityKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::Owns {
            if let (NodeRef::System(sk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                owned_entities_by_system.entry(*sk).or_default().push(*ek);
            }
        }
    }

    let mut systems: HashSet<SystemKey> = apis_by_system.keys().copied().collect();
    systems.extend(owned_entities_by_system.keys().copied());

    let mut result = Vec::new();
    for system in systems {
        let mut apis = apis_by_system.remove(&system).unwrap_or_default();
        apis.sort_by_key(|ak| {
            model
                .apis
                .get(*ak)
                .map(|a| a.id.clone())
                .unwrap_or_default()
        });
        apis.dedup();

        let api_set: HashSet<ApiKey> = apis.iter().copied().collect();
        let mut entity_set: HashSet<EntityKey> = HashSet::new();
        if let Some(owned) = owned_entities_by_system.get(&system) {
            entity_set.extend(owned.iter().copied());
        }
        for rel in &model.relations {
            if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                if api_set.contains(ak) && is_entity_operation(&rel.kind) {
                    entity_set.insert(*ek);
                }
            }
        }
        let mut entities: Vec<EntityKey> = entity_set.into_iter().collect();
        entities.sort_by_key(|ek| {
            model
                .entities
                .get(*ek)
                .map(|e| e.id.clone())
                .unwrap_or_default()
        });

        result.push(SystemBoundary {
            system,
            apis,
            entities,
        });
    }

    result.sort_by_key(|b| {
        model
            .systems
            .get(b.system)
            .map(|s| s.id.clone())
            .unwrap_or_default()
    });
    result
}

struct SystemIndex {
    systems_by_api: HashMap<ApiKey, Vec<SystemKey>>,
    boundary_systems_by_entity: HashMap<EntityKey, Vec<SystemKey>>,
    api_systems_by_entity: HashMap<EntityKey, Vec<SystemKey>>,
    owners_by_entity: HashMap<EntityKey, Vec<SystemKey>>,
}

impl SystemIndex {
    fn build(model: &SemanticModel) -> Self {
        let mut systems_by_api: HashMap<ApiKey, Vec<SystemKey>> = HashMap::new();
        let mut boundary_systems_by_entity: HashMap<EntityKey, Vec<SystemKey>> = HashMap::new();
        for boundary in derive_system_boundaries(model) {
            for &api in &boundary.apis {
                systems_by_api.entry(api).or_default().push(boundary.system);
            }
            for &entity in &boundary.entities {
                boundary_systems_by_entity
                    .entry(entity)
                    .or_default()
                    .push(boundary.system);
            }
        }
        let api_systems_by_entity = api_systems_by_entity(model, &systems_by_api);
        Self {
            systems_by_api,
            boundary_systems_by_entity,
            api_systems_by_entity,
            owners_by_entity: owners_by_entity(model),
        }
    }

    /// The single system an entity belongs to, or `None` when it has none or several.
    fn sole_system(&self, entity: EntityKey) -> Option<SystemKey> {
        match self.boundary_systems_by_entity.get(&entity)?.as_slice() {
            [system] => Some(*system),
            _ => None,
        }
    }
}

fn warn(model: &SemanticModel, diags: &mut Vec<Diagnostic>, kind: &str, id: &str, err: RdraError) {
    push_model_decl_diagnostic(model, diags, kind, id, err, true);
}

pub fn system_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let index = SystemIndex::build(model);
    let mut diags = Vec::new();
    membership_conflicts(model, &index, &mut diags);
    ownership_conflicts(model, &index, &mut diags);
    operations_outside_owner(model, &index, &mut diags);
    cross_system_relations(model, &index, &mut diags);
    coordination_conflicts(model, &index, &mut diags);
    diags
}

fn membership_conflicts(model: &SemanticModel, index: &SystemIndex, diags: &mut Vec<Diagnostic>) {
    for (api, systems) in by_id(&index.systems_by_api, |api| api_id(model, api)) {
        if systems.len() > 1 {
            let api_id = api_id(model, *api);
            let err = RdraError::ApiInMultipleSystems {
                api: api_id.clone(),
                systems: system_ids(model, systems),
            };
            warn(model, diags, "api", &api_id, err);
        }
    }
    for (entity, systems) in by_id(&index.api_systems_by_entity, |e| entity_id(model, e)) {
        if systems.len() > 1 {
            let entity_id = entity_id(model, *entity);
            let err = RdraError::EntityInMultipleSystems {
                entity: entity_id.clone(),
                systems: system_ids(model, systems),
            };
            warn(model, diags, "entity", &entity_id, err);
        }
    }
}

fn ownership_conflicts(model: &SemanticModel, index: &SystemIndex, diags: &mut Vec<Diagnostic>) {
    for (entity, owners) in by_id(&index.owners_by_entity, |e| entity_id(model, e)) {
        let entity_id = entity_id(model, *entity);
        if owners.len() > 1 {
            let err = RdraError::EntityOwnedByMultipleSystems {
                entity: entity_id.clone(),
                systems: system_ids(model, owners),
            };
            warn(model, diags, "entity", &entity_id, err);
        }
        for owner in owners {
            let operated_by_owner = index
                .api_systems_by_entity
                .get(entity)
                .is_some_and(|systems| systems.contains(owner));
            if !operated_by_owner {
                let err = RdraError::OwnedEntityWithoutApiOperation {
                    system: system_id(model, *owner),
                    entity: entity_id.clone(),
                };
                warn(model, diags, "entity", &entity_id, err);
            }
        }
    }
}

fn operations_outside_owner(
    model: &SemanticModel,
    index: &SystemIndex,
    diags: &mut Vec<Diagnostic>,
) {
    for rel in &model.relations {
        if !is_entity_operation(&rel.kind) {
            continue;
        }
        let (NodeRef::Api(api), NodeRef::Entity(entity)) = (&rel.from, &rel.to) else {
            continue;
        };
        let (Some(owner_systems), Some(api_systems)) = (
            index.owners_by_entity.get(entity),
            index.systems_by_api.get(api),
        ) else {
            continue;
        };
        for api_system in api_systems {
            if !owner_systems.contains(api_system) {
                let api_id = api_id(model, *api);
                let err = RdraError::ApiOperatesEntityOutsideOwner {
                    api: api_id.clone(),
                    api_system: system_id(model, *api_system),
                    entity: entity_id(model, *entity),
                    owner_systems: system_ids(model, owner_systems),
                };
                warn(model, diags, "api", &api_id, err);
            }
        }
    }
}

fn cross_system_relations(model: &SemanticModel, index: &SystemIndex, diags: &mut Vec<Diagnostic>) {
    for rel in &model.relations {
        if !is_entity_relation(&rel.kind) {
            continue;
        }
        let (NodeRef::Entity(from), NodeRef::Entity(to)) = (&rel.from, &rel.to) else {
            continue;
        };
        let (Some(from_system), Some(to_system)) =
            (index.sole_system(*from), index.sole_system(*to))
        else {
            continue;
        };
        if from_system != to_system && !has_coordination(model, *from, *to) {
            let from_id = entity_id(model, *from);
            let err = RdraError::CrossSystemEntityRelation {
                from: from_id.clone(),
                from_system: system_id(model, from_system),
                to: entity_id(model, *to),
                to_system: system_id(model, to_system),
            };
            warn(model, diags, "entity", &from_id, err);
        }
    }
}

fn coordination_conflicts(model: &SemanticModel, index: &SystemIndex, diags: &mut Vec<Diagnostic>) {
    for coordination in &model.boundary_coordinations {
        let uc_id = usecase_id(model, coordination.usecase);
        let (left, right) = (coordination.left, coordination.right);
        let sides = (index.sole_system(left), index.sole_system(right));
        let (Some(left_system), Some(right_system)) = sides else {
            warn(
                model,
                diags,
                "usecase",
                &uc_id,
                not_cross_system(model, &uc_id, left, right),
            );
            continue;
        };
        if left_system == right_system {
            warn(
                model,
                diags,
                "usecase",
                &uc_id,
                not_cross_system(model, &uc_id, left, right),
            );
            continue;
        }
        for (entity, system) in [(left, left_system), (right, right_system)] {
            if !usecase_invokes_api_for_entity_in_system(
                model,
                coordination.usecase,
                entity,
                system,
            ) {
                let err = RdraError::CoordinationMissingApi {
                    usecase: uc_id.clone(),
                    entity: entity_id(model, entity),
                    system: system_id(model, system),
                };
                warn(model, diags, "usecase", &uc_id, err);
            }
        }
    }
}

fn not_cross_system(
    model: &SemanticModel,
    uc_id: &str,
    left: EntityKey,
    right: EntityKey,
) -> RdraError {
    RdraError::CoordinationNotCrossSystem {
        usecase: uc_id.to_string(),
        from: entity_id(model, left),
        to: entity_id(model, right),
    }
}

fn api_systems_by_entity(
    model: &SemanticModel,
    systems_by_api: &HashMap<ApiKey, Vec<SystemKey>>,
) -> HashMap<EntityKey, Vec<SystemKey>> {
    let mut systems_by_entity: HashMap<EntityKey, Vec<SystemKey>> = HashMap::new();
    for rel in &model.relations {
        if !is_entity_operation(&rel.kind) {
            continue;
        }
        let (NodeRef::Api(api), NodeRef::Entity(entity)) = (&rel.from, &rel.to) else {
            continue;
        };
        let Some(systems) = systems_by_api.get(api) else {
            continue;
        };
        for system in systems {
            let entry = systems_by_entity.entry(*entity).or_default();
            if !entry.contains(system) {
                entry.push(*system);
            }
        }
    }
    systems_by_entity
}

fn owners_by_entity(model: &SemanticModel) -> HashMap<EntityKey, Vec<SystemKey>> {
    let mut owners: HashMap<EntityKey, Vec<SystemKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind != RelKind::Owns {
            continue;
        }
        let (NodeRef::System(system), NodeRef::Entity(entity)) = (&rel.from, &rel.to) else {
            continue;
        };
        let entry = owners.entry(*entity).or_default();
        if !entry.contains(system) {
            entry.push(*system);
        }
    }
    owners
}

fn has_coordination(model: &SemanticModel, left: EntityKey, right: EntityKey) -> bool {
    model.boundary_coordinations.iter().any(|coordination| {
        (coordination.left == left && coordination.right == right)
            || (coordination.left == right && coordination.right == left)
    })
}

fn usecase_invokes_api_for_entity_in_system(
    model: &SemanticModel,
    usecase: crate::model::UseCaseKey,
    entity: EntityKey,
    system: SystemKey,
) -> bool {
    model.relations.iter().any(|invoke| {
        if invoke.kind != RelKind::Invokes || invoke.from != NodeRef::UseCase(usecase) {
            return false;
        }
        let NodeRef::Api(api) = invoke.to else {
            return false;
        };
        api_belongs_to_system(model, api, system) && api_operates_entity(model, api, entity)
    })
}

fn api_belongs_to_system(model: &SemanticModel, api: ApiKey, system: SystemKey) -> bool {
    model.relations.iter().any(|rel| {
        rel.kind == RelKind::Contains
            && rel.from == NodeRef::System(system)
            && rel.to == NodeRef::Api(api)
    })
}

fn api_operates_entity(model: &SemanticModel, api: ApiKey, entity: EntityKey) -> bool {
    model.relations.iter().any(|rel| {
        rel.from == NodeRef::Api(api)
            && rel.to == NodeRef::Entity(entity)
            && is_entity_operation(&rel.kind)
    })
}

fn is_entity_operation(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::Reads | RelKind::Creates | RelKind::Updates | RelKind::Deletes
    )
}

fn is_entity_relation(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::RelateOneToOne
            | RelKind::RelateOneToMany
            | RelKind::RelateManyToOne
            | RelKind::RelateManyToMany
    )
}

/// The entries of `map` sorted by the id of their key, so diagnostics come out in the same
/// order on every run.
fn by_id<K: Copy, V>(map: &HashMap<K, V>, id: impl Fn(K) -> String) -> Vec<(&K, &V)> {
    let mut entries: Vec<(&K, &V)> = map.iter().collect();
    entries.sort_by_cached_key(|(key, _)| id(**key));
    entries
}

fn api_id(model: &SemanticModel, key: ApiKey) -> String {
    model.apis[key].id.clone()
}

fn entity_id(model: &SemanticModel, key: EntityKey) -> String {
    model.entities[key].id.clone()
}

fn system_id(model: &SemanticModel, key: SystemKey) -> String {
    model.systems[key].id.clone()
}

fn usecase_id(model: &SemanticModel, key: crate::model::UseCaseKey) -> String {
    model.use_cases[key].id.clone()
}

fn system_ids(model: &SemanticModel, keys: &[SystemKey]) -> String {
    let mut ids: Vec<String> = keys.iter().map(|&key| system_id(model, key)).collect();
    ids.sort();
    ids.dedup();
    ids.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errs) = parse(src);
        assert!(errs.is_empty(), "parse errors: {:?}", errs);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "model errors: {:?}", errors);
        model
    }

    #[test]
    fn test_system_entities_are_derived_from_api_crud() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
api StoreApi "店舗API"
entity Store "店舗" { id: Int @pk }
entity Organization "組織" { id: Int @pk }
contains(StoreSystem, StoreApi)
reads(StoreApi, Organization)
updates(StoreApi, Store)
"#,
        );

        let boundaries = derive_system_boundaries(&model);
        assert_eq!(boundaries.len(), 1);
        let boundary = &boundaries[0];
        let system_id = model.systems.get(boundary.system).unwrap().id.as_str();
        assert_eq!(system_id, "StoreSystem");

        let entity_ids: Vec<&str> = boundary
            .entities
            .iter()
            .map(|key| model.entities.get(*key).unwrap().id.as_str())
            .collect();
        assert_eq!(entity_ids, vec!["Organization", "Store"]);
    }

    #[test]
    fn system_diagnostics_come_in_id_order() {
        let mut src = String::from("system SysA \"A\"\nsystem SysB \"B\"\n");
        let ids = ["Api3", "Api1", "Api4", "Api0", "Api2"];
        for id in ids {
            src.push_str(&format!(
                "api {id} \"{id}\"\ncontains(SysA, {id})\ncontains(SysB, {id})\n"
            ));
        }
        let model = model_from(&src);
        let apis: Vec<String> = system_diagnostics(&model)
            .iter()
            .filter_map(|d| match &d.error {
                RdraError::ApiInMultipleSystems { api, .. } => Some(api.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(apis, ["Api0", "Api1", "Api2", "Api3", "Api4"]);
    }

    #[test]
    fn test_system_entities_include_explicit_ownership_without_api() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
entity Store "店舗" { id: Int @pk }
owns(StoreSystem, Store)
"#,
        );

        let boundaries = derive_system_boundaries(&model);
        assert_eq!(boundaries.len(), 1);
        let boundary = &boundaries[0];
        let system_id = model.systems.get(boundary.system).unwrap().id.as_str();
        assert_eq!(system_id, "StoreSystem");
        assert!(boundary.apis.is_empty());

        let entity_ids: Vec<&str> = boundary
            .entities
            .iter()
            .map(|key| model.entities.get(*key).unwrap().id.as_str())
            .collect();
        assert_eq!(entity_ids, vec!["Store"]);
    }

    #[test]
    fn test_explicit_ownership_diagnostics_compare_api_access() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
system OrgSystem "組織システム"
api OrgApi "組織API"
entity Store "店舗" { id: Int @pk }
contains(OrgSystem, OrgApi)
owns(StoreSystem, Store)
reads(OrgApi, Store)
"#,
        );

        let diags = system_diagnostics(&model);
        let messages: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("owns entity 'Store'") && msg.contains("no API")),
            "expected owner-without-API warning, got {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("operates entity 'Store' owned by system(s) StoreSystem")),
            "expected API-outside-owner warning, got {messages:?}"
        );
    }

    #[test]
    fn test_explicit_ownership_is_satisfied_by_owner_api_operation() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
api StoreApi "店舗API"
entity Store "店舗" { id: Int @pk }
contains(StoreSystem, StoreApi)
owns(StoreSystem, Store)
updates(StoreApi, Store)
"#,
        );

        let diags = system_diagnostics(&model);
        assert!(
            diags.is_empty(),
            "owner API should satisfy explicit ownership diagnostics, got {:?}",
            diags
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_entity_owned_by_multiple_systems_warns() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
system OrgSystem "組織システム"
entity Store "店舗" { id: Int @pk }
owns(StoreSystem, Store)
owns(OrgSystem, Store)
"#,
        );

        let diags = system_diagnostics(&model);
        let messages: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("explicitly owned by multiple systems")),
            "expected multiple-owner warning, got {messages:?}"
        );
    }

    #[test]
    fn test_cross_system_entity_relation_warns() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
system OrgSystem "組織システム"
api StoreApi "店舗API"
api OrgApi "組織API"
entity Store "店舗" { id: Int @pk }
entity Organization "組織" { id: Int @pk }
contains(StoreSystem, StoreApi)
contains(OrgSystem, OrgApi)
updates(StoreApi, Store)
reads(OrgApi, Organization)
relate(Store, Organization, N:1)
"#,
        );

        let diags = system_diagnostics(&model);
        let messages: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("relation crosses system boundary")),
            "expected cross-system relation warning, got {messages:?}"
        );
    }

    #[test]
    fn test_coordination_requires_apis_on_both_system_sides() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
system OrgSystem "組織システム"
api StoreApi "店舗API"
api OrgApi "組織API"
entity Store "店舗" { id: Int @pk }
entity Organization "組織" { id: Int @pk }
usecase ChangeParentOrg "親組織を変更する"
contains(StoreSystem, StoreApi)
contains(OrgSystem, OrgApi)
updates(StoreApi, Store)
reads(OrgApi, Organization)
relate(Store, Organization, N:1)
coordinates(ChangeParentOrg, Store, Organization)
invokes(ChangeParentOrg, StoreApi)
"#,
        );

        let diags = system_diagnostics(&model);
        let messages: Vec<String> = diags.iter().map(|d| d.error.to_string()).collect();
        assert!(
            !messages
                .iter()
                .any(|msg| msg.contains("without use case coordination")),
            "coordinates should suppress the uncoordinated relation warning: {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("invokes no API") && msg.contains("Organization")),
            "expected missing OrgSystem API warning, got {messages:?}"
        );
    }

    #[test]
    fn test_valid_coordination_invokes_apis_on_both_sides() {
        let model = model_from(
            r#"
system StoreSystem "店舗システム"
system OrgSystem "組織システム"
api StoreApi "店舗API"
api OrgApi "組織API"
entity Store "店舗" { id: Int @pk }
entity Organization "組織" { id: Int @pk }
usecase ChangeParentOrg "親組織を変更する"
contains(StoreSystem, StoreApi)
contains(OrgSystem, OrgApi)
updates(StoreApi, Store)
reads(OrgApi, Organization)
relate(Store, Organization, N:1)
coordinates(ChangeParentOrg, Store, Organization)
invokes(ChangeParentOrg, StoreApi)
invokes(ChangeParentOrg, OrgApi)
"#,
        );

        let diags = system_diagnostics(&model);
        assert!(
            diags.is_empty(),
            "valid coordination should have no system diagnostics, got {:?}",
            diags
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
    }
}
