//! The consumer side of a cross-model reference: what a dependency publishes, and how a
//! product binds it through `extsystem Auth "..." from auth.AuthSystem`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::location::format_diagnostic_message;
use crate::project::Project;
use crate::surface::Surface;

/// A loaded dependency as the product's analysis sees it: its published surface, plus the
/// names it keeps internal (for `NotExposed`) and its modules and files (for `ForeignImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct DependencySurface {
    /// The `[dependencies]` key of the product's manifest.
    pub name: String,
    pub systems: Vec<String>,
    pub surface: Surface,
    /// Every declared id of the dependency with its kind, published or not.
    pub declared: BTreeMap<String, String>,
    pub modules: Vec<String>,
    pub files: Vec<PathBuf>,
}

impl DependencySurface {
    pub fn of(name: &str, project: &Project) -> DependencySurface {
        let model = &project.model;
        let mut systems: Vec<String> = model.systems.values().map(|s| s.id.clone()).collect();
        systems.sort();
        let declared = model
            .decl_sites
            .iter()
            .map(|(kind, id, _)| (id.to_string(), kind.to_string()))
            .collect();
        let mut modules: Vec<String> = model.import_scopes.modules().map(str::to_string).collect();
        modules.sort();
        let files = project
            .program
            .sources
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect();
        DependencySurface {
            name: name.to_string(),
            systems,
            surface: Surface::of(model),
            declared,
            modules,
            files,
        }
    }

    /// The first error of a dependency's own `check`, as the product reports it.
    pub fn first_error(project: &Project) -> Option<String> {
        project.diags.iter().find(|d| !d.is_warning).map(|d| {
            format_diagnostic_message(
                Some(&project.program),
                false,
                d.location.as_ref(),
                &d.error.to_string(),
            )
        })
    }
}

/// Which dependency a module path or a source file belongs to, read where the product
/// resolves its imports.
#[derive(Debug, Default)]
pub struct ForeignModules {
    modules: BTreeMap<String, String>,
    files: Vec<(PathBuf, String)>,
}

impl ForeignModules {
    pub fn of(deps: &[Arc<DependencySurface>]) -> ForeignModules {
        let mut foreign = ForeignModules::default();
        for dep in deps {
            for module in &dep.modules {
                foreign
                    .modules
                    .entry(module.clone())
                    .or_insert_with(|| dep.name.clone());
            }
            for file in &dep.files {
                foreign.files.push((file.clone(), dep.name.clone()));
            }
        }
        foreign
    }

    pub fn dep_of_module(&self, module: &str) -> Option<&str> {
        self.modules.get(module).map(String::as_str)
    }

    pub fn dep_of_file(&self, path: &Path) -> Option<&str> {
        let canon = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.files
            .iter()
            .find(|(file, _)| *file == canon)
            .map(|(_, dep)| dep.as_str())
    }
}
