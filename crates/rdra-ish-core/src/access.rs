use crate::model::{
    ApiKey, MediumKey, NodeRef, PermissionKey, RelKind, ScreenKey, SemanticModel, UseCaseKey,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionCallable {
    pub permission: PermissionKey,
    pub usecases: Vec<UseCaseKey>,
    pub apis: Vec<ApiKey>,
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
}
