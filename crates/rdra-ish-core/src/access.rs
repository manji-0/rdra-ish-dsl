use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::{
    ActorKey, ApiKey, BucKey, MediumKey, NodeRef, PermissionKey, RelKind, ScreenKey, SemanticModel,
    UseCaseKey,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionCallable {
    pub permission: PermissionKey,
    pub usecases: Vec<UseCaseKey>,
    pub apis: Vec<ApiKey>,
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

/// For each permission, collect use cases and APIs that declare `requires_permission`.
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

            PermissionCallable {
                permission,
                usecases,
                apis,
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
/// Use-case actor resolution follows the sequence diagram rule: direct
/// `performs(Actor, UseCase)` wins; otherwise actors on containing BUCs are used.
/// API requirements are checked per invoking use case so a shared API cannot be
/// made valid by one authorized path while another invocation path lacks authority.
pub fn permission_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let uc_requirements = collect_usecase_requirements(model);
    let api_requirements = collect_api_requirements(model);

    let mut diags = Vec::new();

    for (usecase, permissions) in &uc_requirements {
        let usecase = *usecase;
        let actors = actors_for_usecase(model, usecase);
        if actors.is_empty() {
            for &permission in permissions {
                diags.push(Diagnostic::warning(RdraError::UseCasePermissionNoActor {
                    usecase: model.use_cases[usecase].id.clone(),
                    permission: model.permissions[permission].id.clone(),
                }));
            }
        }
    }

    for (api, permissions) in &api_requirements {
        let api = *api;
        let mut invoking_usecases = usecases_invoking_api(model, api);
        invoking_usecases.sort_by_key(|uc| model.use_cases[*uc].id.as_str());
        for usecase in invoking_usecases {
            let actors = actors_for_usecase(model, usecase);
            if actors.is_empty() {
                for &permission in permissions {
                    diags.push(Diagnostic::warning(RdraError::ApiPermissionNoActor {
                        api: model.apis[api].id.clone(),
                        permission: model.permissions[permission].id.clone(),
                        usecase: model.use_cases[usecase].id.clone(),
                    }));
                }
            }
        }
    }

    for entry in derive_actor_permission_audit(model) {
        match entry.status {
            ActorPermissionAuditStatus::Missing => {
                diags.push(Diagnostic::warning(RdraError::ActorPermissionMissing {
                    actor: model.actors[entry.actor].id.clone(),
                    permission: model.permissions[entry.permission].id.clone(),
                    required_by: describe_requirement_sources(model, &entry.sources),
                }));
            }
            ActorPermissionAuditStatus::Excess => {
                diags.push(Diagnostic::warning(RdraError::ActorPermissionExcess {
                    actor: model.actors[entry.actor].id.clone(),
                    permission: model.permissions[entry.permission].id.clone(),
                }));
            }
            ActorPermissionAuditStatus::Ok => {}
        }
    }

    diags
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

fn actors_for_usecase(model: &SemanticModel, usecase: UseCaseKey) -> Vec<ActorKey> {
    let mut direct: Vec<ActorKey> = model
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
    sort_dedup_actors(model, &mut direct);
    if !direct.is_empty() {
        return direct;
    }

    let bucs = bucs_containing_usecase(model, usecase);
    let mut actors: Vec<ActorKey> = model
        .relations
        .iter()
        .filter_map(|rel| {
            if rel.kind == RelKind::Performs {
                if let (NodeRef::Actor(actor), NodeRef::Buc(buc)) = (&rel.from, &rel.to) {
                    if bucs.contains(buc) {
                        return Some(*actor);
                    }
                }
            }
            None
        })
        .collect();
    sort_dedup_actors(model, &mut actors);
    actors
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
mod tests {
    use super::*;
    use crate::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        model
    }

    #[test]
    fn derive_permission_callables_groups_usecases_and_apis() {
        let model = model_from(
            r#"
usecase BookAppointment "Book Appointment"
usecase CancelAppointment "Cancel Appointment"
api BookingApi "Booking API"
api CancelApi "Cancel API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
requires_permission(BookAppointment, ScheduleWrite)
requires_permission(BookAppointment, PatientRead)
requires_permission(CancelAppointment, ScheduleWrite)
requires_permission(BookingApi, PatientRead)
requires_permission(CancelApi, ScheduleWrite)
"#,
        );

        let callables = derive_permission_callables(&model);
        assert_eq!(callables.len(), 2);

        let schedule = callables
            .iter()
            .find(|entry| model.permissions[entry.permission].id == "ScheduleWrite")
            .expect("ScheduleWrite should be listed");
        assert_eq!(
            schedule
                .usecases
                .iter()
                .map(|key| model.use_cases[*key].id.as_str())
                .collect::<Vec<_>>(),
            vec!["BookAppointment", "CancelAppointment"]
        );
        assert_eq!(
            schedule
                .apis
                .iter()
                .map(|key| model.apis[*key].id.as_str())
                .collect::<Vec<_>>(),
            vec!["CancelApi"]
        );

        let patient = callables
            .iter()
            .find(|entry| model.permissions[entry.permission].id == "PatientRead")
            .expect("PatientRead should be listed");
        assert_eq!(
            patient
                .usecases
                .iter()
                .map(|key| model.use_cases[*key].id.as_str())
                .collect::<Vec<_>>(),
            vec!["BookAppointment"]
        );
        assert_eq!(
            patient
                .apis
                .iter()
                .map(|key| model.apis[*key].id.as_str())
                .collect::<Vec<_>>(),
            vec!["BookingApi"]
        );
    }

    #[test]
    fn derive_permission_callables_includes_permissions_without_callables() {
        let model = model_from(
            r#"
permission UnusedPermission "Unused Permission"
usecase BookAppointment "Book Appointment"
requires_permission(BookAppointment, ScheduleWrite)
permission ScheduleWrite "Schedule Write"
"#,
        );

        let callables = derive_permission_callables(&model);
        assert_eq!(callables.len(), 2);

        let unused = callables
            .iter()
            .find(|entry| model.permissions[entry.permission].id == "UnusedPermission")
            .expect("unused permission should be listed");
        assert!(unused.usecases.is_empty());
        assert!(unused.apis.is_empty());
    }

    #[test]
    fn derive_actor_permission_audit_marks_missing_ok_and_excess() {
        let model = model_from(
            r#"
actor Staff "Staff"
actor BillingBot "Billing Bot"
usecase BookAppointment "Book Appointment"
usecase GenerateClaim "Generate Claim"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission BillingClaimWrite "Billing Claim Write"
permission LegacyAdmin "Legacy Admin"
performs(Staff, BookAppointment)
performs(BillingBot, GenerateClaim)
has_permission(BillingBot, BillingClaimWrite)
has_permission(Staff, LegacyAdmin)
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
requires_permission(GenerateClaim, BillingClaimWrite)
"#,
        );

        let audit = derive_actor_permission_audit(&model);

        let staff_schedule = audit
            .iter()
            .find(|entry| {
                model.actors[entry.actor].id == "Staff"
                    && model.permissions[entry.permission].id == "ScheduleWrite"
            })
            .expect("Staff/ScheduleWrite should be inferred");
        assert_eq!(staff_schedule.status, ActorPermissionAuditStatus::Missing);
        assert!(!staff_schedule.assigned);
        assert!(staff_schedule.required);
        assert_eq!(staff_schedule.sources.len(), 2);

        let staff_legacy = audit
            .iter()
            .find(|entry| {
                model.actors[entry.actor].id == "Staff"
                    && model.permissions[entry.permission].id == "LegacyAdmin"
            })
            .expect("Staff/LegacyAdmin should be listed");
        assert_eq!(staff_legacy.status, ActorPermissionAuditStatus::Excess);
        assert!(staff_legacy.assigned);
        assert!(!staff_legacy.required);

        let bot_billing = audit
            .iter()
            .find(|entry| {
                model.actors[entry.actor].id == "BillingBot"
                    && model.permissions[entry.permission].id == "BillingClaimWrite"
            })
            .expect("BillingBot/BillingClaimWrite should be listed");
        assert_eq!(bot_billing.status, ActorPermissionAuditStatus::Ok);
    }

    #[test]
    fn permission_diagnostics_warns_when_usecase_actor_lacks_required_permission() {
        let model = model_from(
            r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
        );

        let diags = permission_diagnostics(&model);
        assert!(diags.iter().any(|diag| matches!(
            &diag.error,
            RdraError::ActorPermissionMissing {
                actor,
                permission,
                required_by,
            } if actor == "Staff"
                && permission == "ScheduleWrite"
                && required_by.contains("usecase 'BookAppointment'")
        )));
    }

    #[test]
    fn permission_diagnostics_warns_when_required_usecase_has_no_actor_path() {
        let model = model_from(
            r#"
usecase GenerateClaim "Generate Claim"
permission BillingClaimWrite "Billing Claim Write"
requires_permission(GenerateClaim, BillingClaimWrite)
"#,
        );

        let diags = permission_diagnostics(&model);
        assert!(diags.iter().any(|diag| matches!(
            &diag.error,
            RdraError::UseCasePermissionNoActor {
                usecase,
                permission,
            } if usecase == "GenerateClaim" && permission == "BillingClaimWrite"
        )));
    }

    #[test]
    fn permission_diagnostics_accepts_buc_actor_permission_for_contained_usecase() {
        let model = model_from(
            r#"
actor Staff "Staff"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BucScheduling)
has_permission(Staff, ScheduleWrite)
contains(BucScheduling, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
        );

        assert!(permission_diagnostics(&model).is_empty());
    }

    #[test]
    fn permission_diagnostics_warns_for_each_actor_missing_on_same_path() {
        let model = model_from(
            r#"
actor Staff "Staff"
actor Assistant "Assistant"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BucScheduling)
performs(Assistant, BucScheduling)
has_permission(Staff, ScheduleWrite)
contains(BucScheduling, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
        );

        let diags = permission_diagnostics(&model);
        assert_eq!(diags.len(), 1);
        assert!(matches!(
            &diags[0].error,
            RdraError::ActorPermissionMissing {
                actor,
                permission,
                required_by,
            } if actor == "Assistant"
                && permission == "ScheduleWrite"
                && required_by.contains("usecase 'BookAppointment'")
        ));
    }

    #[test]
    fn permission_diagnostics_checks_api_requirement_per_invoking_usecase() {
        let model = model_from(
            r#"
actor Staff "Staff"
actor Patient "Patient"
usecase BookAppointment "Book Appointment"
usecase CancelAppointment "Cancel Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
performs(Staff, BookAppointment)
performs(Patient, CancelAppointment)
has_permission(Staff, ScheduleWrite)
invokes(BookAppointment, BookingApi)
invokes(CancelAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#,
        );

        let diags = permission_diagnostics(&model);
        assert_eq!(diags.len(), 1);
        assert!(matches!(
            &diags[0].error,
            RdraError::ActorPermissionMissing {
                actor,
                permission,
                required_by,
            } if actor == "Patient"
                && permission == "ScheduleWrite"
                && required_by.contains("api 'BookingApi' via usecase 'CancelAppointment'")
        ));
    }

    #[test]
    fn permission_diagnostics_warns_when_api_invocation_has_no_actor_path() {
        let model = model_from(
            r#"
usecase GenerateClaim "Generate Claim"
api ClaimsApi "Claims API"
permission BillingClaimWrite "Billing Claim Write"
invokes(GenerateClaim, ClaimsApi)
requires_permission(ClaimsApi, BillingClaimWrite)
"#,
        );

        let diags = permission_diagnostics(&model);
        assert!(diags.iter().any(|diag| matches!(
            &diag.error,
            RdraError::ApiPermissionNoActor {
                api,
                permission,
                usecase,
            } if api == "ClaimsApi"
                && permission == "BillingClaimWrite"
                && usecase == "GenerateClaim"
        )));
    }
}
