use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::push_model_decl_diagnostic;
use crate::model::{
    ActorKey, ApiKey, BucKey, CallerKind, EntityKey, MediumKey, NodeRef, PermissionKey, RelKind,
    ScreenKey, SemanticModel, UseCaseKey,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionCallable {
    pub permission: PermissionKey,
    pub usecases: Vec<UseCaseKey>,
    pub apis: Vec<ApiKey>,
    pub api_paths: Vec<PermissionApiPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PermissionApiPath {
    pub usecase: UseCaseKey,
    pub api: ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActorPermissionRequirementSource {
    pub usecase: UseCaseKey,
    pub api: Option<ApiKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorPermissionAuditStatus {
    Ok,
    Missing,
    Excess,
}

impl ActorPermissionAuditStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Excess => "excess",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorPermissionAudit {
    pub actor: ActorKey,
    pub permission: PermissionKey,
    pub assigned: bool,
    pub required: bool,
    pub status: ActorPermissionAuditStatus,
    pub sources: Vec<ActorPermissionRequirementSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenConstraintPattern {
    pub screen: ScreenKey,
    pub usecase: UseCaseKey,
    pub api: Option<ApiKey>,
    pub permissions: Vec<PermissionKey>,
    pub media: Vec<MediumKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorInputOperation {
    Create,
    Update,
}

impl ActorInputOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorInputSource {
    UseCase,
    Api(ApiKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorInputInference {
    pub actor: ActorKey,
    pub buc: Option<BucKey>,
    pub usecase: UseCaseKey,
    pub source: ActorInputSource,
    pub entity: EntityKey,
    pub column: String,
    pub operation: ActorInputOperation,
    pub reason: String,
}

/// Infer candidate input fields per actor from the modeled BUC/use-case path.
///
/// Actor resolution follows the same rule as sequence and permission analysis:
/// union of direct `performs(Actor, UseCase)` and actors on containing BUCs.
/// Candidate fields are derived from `creates` / `updates` / `writes` on the use case or
/// on APIs invoked by the use case. Primary keys, foreign keys, defaults, and columns
/// explicitly driven by `sets(...)` are treated as system/model supplied values and skipped.
pub fn derive_actor_input_inferences(model: &SemanticModel) -> Vec<ActorInputInference> {
    let mut rows = Vec::new();

    let mut usecases: Vec<_> = model.use_cases.keys().collect();
    usecases.sort_by_key(|usecase| model.use_cases[*usecase].id.as_str());

    for usecase in usecases {
        let actors = actors_for_usecase(model, usecase);
        if actors.is_empty() {
            continue;
        }

        let bucs = bucs_containing_usecase(model, usecase);
        let buc_options: Vec<Option<BucKey>> = if bucs.is_empty() {
            vec![None]
        } else {
            bucs.into_iter().map(Some).collect()
        };

        let operations = collect_usecase_input_operations(model, usecase);
        for actor in actors {
            for buc in &buc_options {
                for op in &operations {
                    add_input_rows_for_operation(model, &mut rows, actor, *buc, usecase, op);
                }
            }
        }
    }

    rows.sort_by(|a, b| {
        model.actors[a.actor]
            .id
            .cmp(&model.actors[b.actor].id)
            .then_with(|| option_buc_id(model, a.buc).cmp(option_buc_id(model, b.buc)))
            .then_with(|| {
                model.use_cases[a.usecase]
                    .id
                    .cmp(&model.use_cases[b.usecase].id)
            })
            .then_with(|| {
                model.entities[a.entity]
                    .id
                    .cmp(&model.entities[b.entity].id)
            })
            .then_with(|| a.column.cmp(&b.column))
            .then_with(|| source_sort_id(model, a.source).cmp(source_sort_id(model, b.source)))
    });
    rows.dedup_by(|a, b| {
        a.actor == b.actor
            && a.buc == b.buc
            && a.usecase == b.usecase
            && a.source == b.source
            && a.entity == b.entity
            && a.column == b.column
            && a.operation == b.operation
    });
    rows
}

pub fn derive_screen_constraint_patterns(model: &SemanticModel) -> Vec<ScreenConstraintPattern> {
    let mut uc_to_screens: HashMap<UseCaseKey, Vec<ScreenKey>> = HashMap::new();
    let mut uc_to_apis: HashMap<UseCaseKey, Vec<ApiKey>> = HashMap::new();
    let mut uc_permissions: HashMap<UseCaseKey, Vec<PermissionKey>> = HashMap::new();
    let mut api_permissions: HashMap<ApiKey, Vec<PermissionKey>> = HashMap::new();
    let mut uc_media: HashMap<UseCaseKey, Vec<MediumKey>> = HashMap::new();
    let mut api_media: HashMap<ApiKey, Vec<MediumKey>> = HashMap::new();

    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Displays, NodeRef::UseCase(uc), NodeRef::Screen(screen)) => {
                push_unique(uc_to_screens.entry(*uc).or_default(), *screen);
            }
            (RelKind::Invokes, NodeRef::UseCase(uc), NodeRef::Api(api)) => {
                push_unique(uc_to_apis.entry(*uc).or_default(), *api);
            }
            (
                RelKind::RequiresPermission,
                NodeRef::UseCase(uc),
                NodeRef::Permission(permission),
            ) => {
                push_unique(uc_permissions.entry(*uc).or_default(), *permission);
            }
            (RelKind::RequiresPermission, NodeRef::Api(api), NodeRef::Permission(permission)) => {
                push_unique(api_permissions.entry(*api).or_default(), *permission);
            }
            (RelKind::RequiresMedium, NodeRef::UseCase(uc), NodeRef::Medium(medium)) => {
                push_unique(uc_media.entry(*uc).or_default(), *medium);
            }
            (RelKind::RequiresMedium, NodeRef::Api(api), NodeRef::Medium(medium)) => {
                push_unique(api_media.entry(*api).or_default(), *medium);
            }
            _ => {}
        }
    }

    let mut patterns = Vec::new();
    let mut usecases: Vec<_> = uc_to_screens.keys().copied().collect();
    usecases.sort_by_key(|uc| model.use_cases[*uc].id.as_str());

    for usecase in usecases {
        let mut screens = uc_to_screens.get(&usecase).cloned().unwrap_or_default();
        screens.sort_by_key(|screen| model.screens[*screen].id.as_str());

        let mut apis = uc_to_apis.get(&usecase).cloned().unwrap_or_default();
        apis.sort_by_key(|api| model.apis[*api].id.as_str());

        for screen in screens {
            if apis.is_empty() {
                patterns.push(ScreenConstraintPattern {
                    screen,
                    usecase,
                    api: None,
                    permissions: uc_permissions.get(&usecase).cloned().unwrap_or_default(),
                    media: uc_media.get(&usecase).cloned().unwrap_or_default(),
                });
            } else {
                for api in &apis {
                    let mut permissions = uc_permissions.get(&usecase).cloned().unwrap_or_default();
                    extend_unique(
                        &mut permissions,
                        api_permissions.get(api).cloned().unwrap_or_default(),
                    );

                    let mut media = uc_media.get(&usecase).cloned().unwrap_or_default();
                    extend_unique(&mut media, api_media.get(api).cloned().unwrap_or_default());

                    patterns.push(ScreenConstraintPattern {
                        screen,
                        usecase,
                        api: Some(*api),
                        permissions,
                        media,
                    });
                }
            }
        }
    }

    patterns
}

/// For each permission, collect direct use cases, APIs, and UC->API paths that require it.
pub fn derive_permission_callables(model: &SemanticModel) -> Vec<PermissionCallable> {
    let mut uc_by_permission: HashMap<PermissionKey, Vec<UseCaseKey>> = HashMap::new();
    let mut api_by_permission: HashMap<PermissionKey, Vec<ApiKey>> = HashMap::new();

    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (
                RelKind::RequiresPermission,
                NodeRef::UseCase(uc),
                NodeRef::Permission(permission),
            ) => {
                push_unique(uc_by_permission.entry(*permission).or_default(), *uc);
            }
            (RelKind::RequiresPermission, NodeRef::Api(api), NodeRef::Permission(permission)) => {
                push_unique(api_by_permission.entry(*permission).or_default(), *api);
            }
            _ => {}
        }
    }

    let mut permissions: Vec<_> = model.permissions.keys().collect();
    permissions.sort_by_key(|key| model.permissions[*key].id.as_str());

    permissions
        .into_iter()
        .map(|permission| {
            let mut usecases = uc_by_permission
                .get(&permission)
                .cloned()
                .unwrap_or_default();
            usecases.sort_by_key(|uc| model.use_cases[*uc].id.as_str());

            let mut apis = api_by_permission
                .get(&permission)
                .cloned()
                .unwrap_or_default();
            apis.sort_by_key(|api| model.apis[*api].id.as_str());

            let mut api_paths = Vec::new();
            for &api in &apis {
                for usecase in usecases_invoking_api(model, api) {
                    push_permission_api_path(&mut api_paths, PermissionApiPath { usecase, api });
                }
            }
            sort_permission_api_paths(model, &mut api_paths);

            PermissionCallable {
                permission,
                usecases,
                apis,
                api_paths,
            }
        })
        .collect()
}

/// Infer actor-side permission assignments from use-case and API permission requirements.
///
/// The result contains every actor/permission pair that is either assigned with
/// `has_permission` or required by a use-case/API path the actor can perform. Rows are
/// classified as:
///
/// - `ok`: assigned and required by at least one path;
/// - `missing`: required by at least one path but not assigned;
/// - `excess`: assigned but not required by any modeled path for that actor.
pub fn derive_actor_permission_audit(model: &SemanticModel) -> Vec<ActorPermissionAudit> {
    let assigned = collect_actor_permissions(model);
    let required = collect_actor_permission_requirements(model);

    let mut pairs: Vec<(ActorKey, PermissionKey)> = Vec::new();
    for (actor, permissions) in &assigned {
        for permission in permissions {
            pairs.push((*actor, *permission));
        }
    }
    for pair in required.keys() {
        pairs.push(*pair);
    }

    pairs.sort_by(|(actor_a, permission_a), (actor_b, permission_b)| {
        model.actors[*actor_a]
            .id
            .cmp(&model.actors[*actor_b].id)
            .then_with(|| {
                model.permissions[*permission_a]
                    .id
                    .cmp(&model.permissions[*permission_b].id)
            })
    });
    pairs.dedup();

    pairs
        .into_iter()
        .map(|(actor, permission)| {
            let assigned = assigned
                .get(&actor)
                .is_some_and(|permissions| permissions.contains(&permission));
            let mut sources = required
                .get(&(actor, permission))
                .cloned()
                .unwrap_or_default();
            sort_requirement_sources(model, &mut sources);
            let required = !sources.is_empty();
            let status = match (assigned, required) {
                (true, true) => ActorPermissionAuditStatus::Ok,
                (false, true) => ActorPermissionAuditStatus::Missing,
                (true, false) => ActorPermissionAuditStatus::Excess,
                (false, false) => unreachable!("audit pairs are assigned or required"),
            };

            ActorPermissionAudit {
                actor,
                permission,
                assigned,
                required,
                status,
                sources,
            }
        })
        .collect()
}

/// Report permission requirements that are not backed by the actor permission model.
///
/// Use-case actor resolution unions direct `performs(Actor, UseCase)` with actors on
/// containing BUCs (sort/dedup). API requirements are checked per invoking use case so
/// a shared API cannot be made valid by one authorized path while another invocation
/// path lacks authority.
pub fn permission_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let uc_requirements = collect_usecase_requirements(model);
    let api_requirements = collect_api_requirements(model);

    let mut diags = Vec::new();

    for (usecase, permissions) in &uc_requirements {
        let usecase = *usecase;
        let actors = actors_for_usecase(model, usecase);
        if actors.is_empty() && !system_may_call(model, usecase) {
            let uc_id = model.use_cases[usecase].id.clone();
            for &permission in permissions {
                push_model_decl_diagnostic(
                    model,
                    &mut diags,
                    "usecase",
                    &uc_id,
                    RdraError::UseCasePermissionNoActor {
                        usecase: uc_id.clone(),
                        permission: model.permissions[permission].id.clone(),
                    },
                    true,
                );
            }
        }
    }

    for (api, permissions) in &api_requirements {
        let api = *api;
        let mut invoking_usecases = usecases_invoking_api(model, api);
        invoking_usecases.sort_by_key(|uc| model.use_cases[*uc].id.as_str());
        for usecase in invoking_usecases {
            let actors = actors_for_usecase(model, usecase);
            if actors.is_empty() && !system_may_call(model, usecase) {
                let api_id = model.apis[api].id.clone();
                let uc_id = model.use_cases[usecase].id.clone();
                for &permission in permissions {
                    push_model_decl_diagnostic(
                        model,
                        &mut diags,
                        "api",
                        &api_id,
                        RdraError::ApiPermissionNoActor {
                            api: api_id.clone(),
                            permission: model.permissions[permission].id.clone(),
                            usecase: uc_id.clone(),
                        },
                        true,
                    );
                }
            }
        }
    }

    for entry in derive_actor_permission_audit(model) {
        match entry.status {
            ActorPermissionAuditStatus::Missing => {
                let actor = model.actors[entry.actor].id.clone();
                let through_buc = match bucs_lending_usecases(model, entry.actor, &entry.sources)
                    .as_slice()
                {
                    [] => String::new(),
                    bucs => {
                        let performs = bucs
                            .iter()
                            .map(|buc| format!("`performs({actor}, {buc})`"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let names = bucs.join(", ");
                        format!(
                            "; {performs} makes {actor} perform every use case of {names}: to say {actor} only takes part in {names}, write `performs({actor}, UseCase)` for the use cases it runs or `performs({actor}, Step)` for its steps instead"
                        )
                    }
                };
                push_model_decl_diagnostic(
                    model,
                    &mut diags,
                    "actor",
                    &actor,
                    RdraError::ActorPermissionMissing {
                        actor: actor.clone(),
                        permission: model.permissions[entry.permission].id.clone(),
                        required_by: describe_requirement_sources(model, &entry.sources),
                        through_buc,
                    },
                    true,
                );
            }
            ActorPermissionAuditStatus::Excess => {
                push_model_decl_diagnostic(
                    model,
                    &mut diags,
                    "actor",
                    &model.actors[entry.actor].id,
                    RdraError::ActorPermissionExcess {
                        actor: model.actors[entry.actor].id.clone(),
                        permission: model.permissions[entry.permission].id.clone(),
                    },
                    true,
                );
            }
            ActorPermissionAuditStatus::Ok => {}
        }
    }

    diags
}

/// The system may run the use case with no actor: another system calls it through the
/// published surface, or a timer or an event starts it. A started use case with no
/// `performs(Actor, UseCase)` is the system's work (see `actors_for_usecase`), so its own and
/// its APIs' permissions are the system's, which the model does not check.
fn system_may_call(model: &SemanticModel, usecase: UseCaseKey) -> bool {
    matches!(
        model.publication.caller_of(usecase),
        Some(CallerKind::System | CallerKind::Any)
    ) || triggered_use_cases(model).contains(&usecase)
}

fn collect_actor_permissions(model: &SemanticModel) -> HashMap<ActorKey, HashSet<PermissionKey>> {
    let mut by_actor: HashMap<ActorKey, HashSet<PermissionKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::HasPermission {
            if let (NodeRef::Actor(actor), NodeRef::Permission(permission)) = (&rel.from, &rel.to) {
                by_actor.entry(*actor).or_default().insert(*permission);
            }
        }
    }
    by_actor
}

fn collect_usecase_requirements(model: &SemanticModel) -> HashMap<UseCaseKey, Vec<PermissionKey>> {
    let mut by_usecase: HashMap<UseCaseKey, Vec<PermissionKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::RequiresPermission {
            if let (NodeRef::UseCase(usecase), NodeRef::Permission(permission)) =
                (&rel.from, &rel.to)
            {
                push_unique(by_usecase.entry(*usecase).or_default(), *permission);
            }
        }
    }
    by_usecase
}

fn collect_api_requirements(model: &SemanticModel) -> HashMap<ApiKey, Vec<PermissionKey>> {
    let mut by_api: HashMap<ApiKey, Vec<PermissionKey>> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::RequiresPermission {
            if let (NodeRef::Api(api), NodeRef::Permission(permission)) = (&rel.from, &rel.to) {
                push_unique(by_api.entry(*api).or_default(), *permission);
            }
        }
    }
    by_api
}

fn collect_actor_permission_requirements(
    model: &SemanticModel,
) -> HashMap<(ActorKey, PermissionKey), Vec<ActorPermissionRequirementSource>> {
    let uc_requirements = collect_usecase_requirements(model);
    let api_requirements = collect_api_requirements(model);
    let mut by_actor_permission: HashMap<
        (ActorKey, PermissionKey),
        Vec<ActorPermissionRequirementSource>,
    > = HashMap::new();

    for (usecase, permissions) in uc_requirements {
        let actors = actors_for_usecase(model, usecase);
        for actor in actors {
            for &permission in &permissions {
                push_requirement_source(
                    by_actor_permission.entry((actor, permission)).or_default(),
                    ActorPermissionRequirementSource { usecase, api: None },
                );
            }
        }
    }

    for (api, permissions) in api_requirements {
        for usecase in usecases_invoking_api(model, api) {
            let actors = actors_for_usecase(model, usecase);
            for actor in actors {
                for &permission in &permissions {
                    push_requirement_source(
                        by_actor_permission.entry((actor, permission)).or_default(),
                        ActorPermissionRequirementSource {
                            usecase,
                            api: Some(api),
                        },
                    );
                }
            }
        }
    }

    by_actor_permission
}

#[derive(Debug, Clone, Copy)]
struct InputOperation {
    source: ActorInputSource,
    entity: EntityKey,
    operation: ActorInputOperation,
}

fn collect_usecase_input_operations(
    model: &SemanticModel,
    usecase: UseCaseKey,
) -> Vec<InputOperation> {
    let mut operations = Vec::new();

    for rel in &model.relations {
        if rel.from == NodeRef::UseCase(usecase) {
            if let Some((entity, operation)) = input_operation_from_relation(rel) {
                operations.push(InputOperation {
                    source: ActorInputSource::UseCase,
                    entity,
                    operation,
                });
            }
        }
    }

    for api in apis_invoked_by_usecase(model, usecase) {
        for rel in &model.relations {
            if rel.from == NodeRef::Api(api) {
                if let Some((entity, operation)) = input_operation_from_relation(rel) {
                    operations.push(InputOperation {
                        source: ActorInputSource::Api(api),
                        entity,
                        operation,
                    });
                }
            }
        }
    }

    operations.sort_by(|a, b| {
        model.entities[a.entity]
            .id
            .cmp(&model.entities[b.entity].id)
            .then_with(|| a.operation.as_str().cmp(b.operation.as_str()))
            .then_with(|| source_sort_id(model, a.source).cmp(source_sort_id(model, b.source)))
    });
    operations.dedup_by(|a, b| {
        a.source == b.source && a.entity == b.entity && a.operation == b.operation
    });
    operations
}

fn input_operation_from_relation(
    rel: &crate::model::Relation,
) -> Option<(EntityKey, ActorInputOperation)> {
    let entity = match rel.to {
        NodeRef::Entity(entity) => entity,
        _ => return None,
    };
    let operation = match rel.kind {
        RelKind::Creates => ActorInputOperation::Create,
        RelKind::Updates => ActorInputOperation::Update,
        _ => return None,
    };
    Some((entity, operation))
}

fn add_input_rows_for_operation(
    model: &SemanticModel,
    rows: &mut Vec<ActorInputInference>,
    actor: ActorKey,
    buc: Option<BucKey>,
    usecase: UseCaseKey,
    op: &InputOperation,
) {
    let entity = &model.entities[op.entity];
    for column in &entity.columns {
        if column.is_pk
            || column.default_val.is_some()
            || model.column_fk(op.entity, &column.name).is_some()
        {
            continue;
        }
        if is_column_set_by_path(model, usecase, op.source, op.entity, &column.name) {
            continue;
        }
        rows.push(ActorInputInference {
            actor,
            buc,
            usecase,
            source: op.source,
            entity: op.entity,
            column: column.name.clone(),
            operation: op.operation,
            reason: format!(
                "{}({}) requires non-derived column",
                op.operation.as_str(),
                entity.id
            ),
        });
    }
}

fn is_column_set_by_path(
    model: &SemanticModel,
    usecase: UseCaseKey,
    source: ActorInputSource,
    entity: EntityKey,
    column: &str,
) -> bool {
    // `sets` takes a use case origin only, so an API path never sets a column.
    if !matches!(source, ActorInputSource::UseCase) {
        return false;
    }
    model.column_effects.iter().any(|effect| {
        effect.origin == usecase && effect.entity == entity && effect.column == column
    }) || model.column_choices.iter().any(|choice| {
        choice.origin == usecase && choice.entity == entity && choice.column == column
    })
}

/// The use cases an event or a timer starts. Only a `performs(Actor, UseCase)` names who
/// performs one, never an actor of its BUC or step: it is the system's work.
pub fn triggered_use_cases(model: &SemanticModel) -> HashSet<UseCaseKey> {
    model
        .relations
        .iter()
        .filter_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Triggers, NodeRef::Event(_) | NodeRef::Timer(_), NodeRef::UseCase(uc)) => {
                Some(*uc)
            }
            _ => None,
        })
        .collect()
}

/// Actors that perform a use case: direct `performs(Actor, UC)` unioned with
/// `performs(Actor, BUC)` for every BUC that contains the use case, unless an event or a
/// timer starts it.
pub fn actors_for_usecase(model: &SemanticModel, usecase: UseCaseKey) -> Vec<ActorKey> {
    let mut actors: Vec<ActorKey> = model
        .relations
        .iter()
        .filter_map(|rel| {
            if rel.kind == RelKind::Performs && rel.to == NodeRef::UseCase(usecase) {
                if let NodeRef::Actor(actor) = &rel.from {
                    return Some(*actor);
                }
            }
            None
        })
        .collect();

    if triggered_use_cases(model).contains(&usecase) {
        sort_dedup_actors(model, &mut actors);
        return actors;
    }
    let bucs = bucs_containing_usecase(model, usecase);
    for rel in &model.relations {
        if rel.kind == RelKind::Performs {
            if let (NodeRef::Actor(actor), NodeRef::Buc(buc)) = (&rel.from, &rel.to) {
                if bucs.contains(buc) {
                    actors.push(*actor);
                }
            }
        }
    }
    sort_dedup_actors(model, &mut actors);
    actors
}

/// The BUCs whose `performs(actor, Buc)` is the only reason `actor` performs some use case of
/// `sources`: it performs none of them directly. Sorted ids.
fn bucs_lending_usecases(
    model: &SemanticModel,
    actor: ActorKey,
    sources: &[ActorPermissionRequirementSource],
) -> Vec<String> {
    let performs = |target: NodeRef| {
        model.relations.iter().any(|rel| {
            rel.kind == RelKind::Performs && rel.from == NodeRef::Actor(actor) && rel.to == target
        })
    };
    let mut bucs: Vec<String> = Vec::new();
    for source in sources {
        if performs(NodeRef::UseCase(source.usecase)) {
            continue;
        }
        for buc in bucs_containing_usecase(model, source.usecase) {
            if performs(NodeRef::Buc(buc)) {
                bucs.push(model.bucs[buc].id.clone());
            }
        }
    }
    bucs.sort();
    bucs.dedup();
    bucs
}

fn apis_invoked_by_usecase(model: &SemanticModel, usecase: UseCaseKey) -> Vec<ApiKey> {
    let mut apis: Vec<ApiKey> = model
        .relations
        .iter()
        .filter_map(|rel| {
            if rel.kind == RelKind::Invokes && rel.from == NodeRef::UseCase(usecase) {
                if let NodeRef::Api(api) = &rel.to {
                    return Some(*api);
                }
            }
            None
        })
        .collect();
    apis.sort_by_key(|api| model.apis[*api].id.as_str());
    apis.dedup();
    apis
}

fn bucs_containing_usecase(model: &SemanticModel, usecase: UseCaseKey) -> Vec<BucKey> {
    let mut bucs: Vec<BucKey> = model
        .relations
        .iter()
        .filter_map(|rel| {
            if rel.kind == RelKind::Contains && rel.to == NodeRef::UseCase(usecase) {
                if let NodeRef::Buc(buc) = &rel.from {
                    return Some(*buc);
                }
            }
            None
        })
        .collect();
    bucs.sort_by_key(|buc| model.bucs[*buc].id.as_str());
    bucs.dedup();
    bucs
}

fn option_buc_id(model: &SemanticModel, buc: Option<BucKey>) -> &str {
    buc.map(|key| model.bucs[key].id.as_str()).unwrap_or("")
}

fn source_sort_id(model: &SemanticModel, source: ActorInputSource) -> &str {
    match source {
        ActorInputSource::UseCase => "",
        ActorInputSource::Api(api) => model.apis[api].id.as_str(),
    }
}

fn usecases_invoking_api(model: &SemanticModel, api: ApiKey) -> Vec<UseCaseKey> {
    let mut usecases: Vec<UseCaseKey> = model
        .relations
        .iter()
        .filter_map(|rel| {
            if rel.kind == RelKind::Invokes && rel.to == NodeRef::Api(api) {
                if let NodeRef::UseCase(usecase) = &rel.from {
                    return Some(*usecase);
                }
            }
            None
        })
        .collect();
    usecases.sort_by_key(|usecase| model.use_cases[*usecase].id.as_str());
    usecases.dedup();
    usecases
}

fn sort_dedup_actors(model: &SemanticModel, actors: &mut Vec<ActorKey>) {
    actors.sort_by_key(|actor| model.actors[*actor].id.as_str());
    actors.dedup();
}

fn sort_requirement_sources(
    model: &SemanticModel,
    sources: &mut Vec<ActorPermissionRequirementSource>,
) {
    sources.sort_by(|a, b| {
        model.use_cases[a.usecase]
            .id
            .cmp(&model.use_cases[b.usecase].id)
            .then_with(|| match (a.api, b.api) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(api_a), Some(api_b)) => model.apis[api_a].id.cmp(&model.apis[api_b].id),
            })
    });
    sources.dedup();
}

fn sort_permission_api_paths(model: &SemanticModel, paths: &mut Vec<PermissionApiPath>) {
    paths.sort_by(|a, b| {
        model.use_cases[a.usecase]
            .id
            .cmp(&model.use_cases[b.usecase].id)
            .then_with(|| model.apis[a.api].id.cmp(&model.apis[b.api].id))
    });
    paths.dedup();
}

fn push_permission_api_path(paths: &mut Vec<PermissionApiPath>, path: PermissionApiPath) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn push_requirement_source(
    sources: &mut Vec<ActorPermissionRequirementSource>,
    source: ActorPermissionRequirementSource,
) {
    if !sources.contains(&source) {
        sources.push(source);
    }
}

fn describe_requirement_sources(
    model: &SemanticModel,
    sources: &[ActorPermissionRequirementSource],
) -> String {
    sources
        .iter()
        .map(|source| {
            let usecase_id = model.use_cases[source.usecase].id.as_str();
            match source.api {
                Some(api) => {
                    format!("api '{}' via usecase '{}'", model.apis[api].id, usecase_id)
                }
                None => format!("usecase '{}'", usecase_id),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_unique<T: Copy + Eq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

fn extend_unique<T: Copy + Eq>(items: &mut Vec<T>, additional: Vec<T>) {
    for item in additional {
        push_unique(items, item);
    }
}

#[cfg(test)]
mod tests;
