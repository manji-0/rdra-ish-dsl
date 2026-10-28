//! Lifecycle behaviour tests: a model in, views and diagnostics out.

mod conditions;
mod cone;
mod constraints;
mod effects;
mod owners;
mod system;
mod trace;
mod tx_groups;
mod witness;

use super::{Bounds, Caps, Diag, EntityStateResult, Scope, StateDiag, TransitionSystem};
use crate::analysis::build_model;
use crate::model::SemanticModel;
use rdra_ish_syntax::parse;

pub(super) fn model_from(src: &str) -> SemanticModel {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (model, diags) = build_model(&ast);
    let errs: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errs.is_empty(), "model errors: {errs:?}");
    model
}

pub(super) struct Derived {
    pub views: Vec<EntityStateResult>,
    pub diags: Vec<Diag>,
}

impl Derived {
    pub fn entity(&self, id: &str) -> &EntityStateResult {
        self.views
            .iter()
            .find(|v| v.entity_id == id)
            .unwrap_or_else(|| panic!("no view for {id}"))
    }

    /// `col=val, ..` rows of an entity's patterns, in discovery order.
    pub fn rows(&self, id: &str) -> Vec<String> {
        self.entity(id)
            .patterns
            .iter()
            .map(|p| {
                p.pattern
                    .values
                    .iter()
                    .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect()
    }

    pub fn has(&self, id: &str, pred: impl Fn(&StateDiag) -> bool) -> bool {
        self.entity(id).diagnostics.iter().any(pred)
    }
}

pub(super) fn derive_scoped(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Derived {
    let built = TransitionSystem::build(model, bounds, scope);
    let run = built.ts.explore(Caps::new(Caps::DEFAULT_PATTERNS, 100_000));
    let views = run.entity_views(model, &built.diags);
    let mut diags = built.diags.clone();
    diags.extend(run.diags().cloned());
    Derived { views, diags }
}

/// What `states` derives: `Bounds::STATES`, each view and verdict on its own cone.
pub(super) fn derive(model: &SemanticModel) -> Derived {
    let report = super::derive_states(model, &Scope::All, Caps::DEFAULT_PATTERNS);
    Derived {
        views: report.views,
        diags: report.diags,
    }
}

pub(super) fn derive_src(src: &str) -> Derived {
    derive(&model_from(src))
}

pub(super) fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(super) fn sample_model(rel: &str) -> SemanticModel {
    let src = std::fs::read_to_string(repo_root().join(rel)).expect("sample readable");
    model_from(&src)
}
