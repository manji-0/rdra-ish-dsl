use crate::model::{
    ApiKey, MediumKey, NodeRef, PermissionKey, RelKind, ScreenKey, SemanticModel, UseCaseKey,
};
use std::collections::HashMap;

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
