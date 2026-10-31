//! The project loader the CLI, the LSP, and the tests share.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::binding::{DependencySurface, ForeignModules};
use crate::{build_merged_model, resolve_overlaid, Diagnostic, ResolvedProgram, SemanticModel};

pub const MANIFEST_FILE: &str = "rdra.toml";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Manifest {
    #[serde(skip)]
    pub path: PathBuf,
    pub model: ModelSection,
    #[serde(default)]
    pub dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelSection {
    pub name: String,
}

#[derive(Debug)]
pub enum ProjectError {
    NotFound(Vec<String>),
    NoRdraFiles,
    Manifest {
        path: PathBuf,
        line: Option<usize>,
        message: String,
    },
    /// The inputs belong to two models.
    ManifestConflict {
        first: Option<PathBuf>,
        second: Option<PathBuf>,
    },
    Dependency {
        manifest: PathBuf,
        name: String,
        path: String,
        kind: DependencyErrorKind,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyErrorKind {
    NotFound,
    Cycle,
    HasErrors,
}

impl DependencyErrorKind {
    pub fn name(self) -> &'static str {
        match self {
            DependencyErrorKind::NotFound => "DependencyNotFound",
            DependencyErrorKind::Cycle => "DependencyCycle",
            DependencyErrorKind::HasErrors => "DependencyHasErrors",
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(paths) => write!(f, "input path(s) not found: {}", paths.join(", ")),
            Self::NoRdraFiles => write!(f, "no .rdra files found in the given inputs"),
            Self::Manifest {
                path,
                line: Some(line),
                message,
            } => write!(f, "{}:{line}: {message}", path.display()),
            Self::Manifest {
                path,
                line: None,
                message,
            } => write!(f, "{}: {message}", path.display()),
            Self::ManifestConflict { first, second } => {
                let name = |m: &Option<PathBuf>| match m {
                    Some(path) => path.display().to_string(),
                    None => "no manifest".to_string(),
                };
                write!(
                    f,
                    "ManifestConflict: the inputs belong to two models ({} and {}); run each model on its own",
                    name(first),
                    name(second)
                )
            }
            Self::Dependency {
                manifest,
                name,
                path,
                kind,
                detail,
            } => write!(
                f,
                "{}: {}: dependency '{name}' (path {path}) {detail}",
                manifest.display(),
                kind.name()
            ),
        }
    }
}

impl std::error::Error for ProjectError {}

#[derive(Debug)]
pub struct Project {
    pub entry_files: Vec<PathBuf>,
    pub include_paths: Vec<PathBuf>,
    pub program: ResolvedProgram,
    pub model: SemanticModel,
    pub diags: Vec<Diagnostic>,
    pub manifest: Option<Manifest>,
    /// The direct dependencies of the manifest, each checked as its own model.
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug)]
pub struct Dependency {
    pub name: String,
    /// The `path` value as written in the manifest.
    pub path: String,
    pub surface: Arc<DependencySurface>,
}

enum DependencySource {
    Path(String),
}

impl Manifest {
    fn dependency_sources(&self) -> Result<Vec<(String, DependencySource)>, ProjectError> {
        let mut sources = Vec::new();
        for (name, value) in &self.dependencies {
            let fail = |message: String| ProjectError::Manifest {
                path: self.path.clone(),
                line: None,
                message,
            };
            let table = value.as_table().ok_or_else(|| {
                fail(format!(
                    "DependencyNeedsPath: dependency '{name}' needs `{{ path = \"...\" }}`"
                ))
            })?;
            if table.contains_key("git") {
                return Err(fail(format!(
                    "DependencyGitUnsupported: dependency '{name}' uses `git`, which a later release adds; use `path` for now"
                )));
            }
            let path = table
                .get("path")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| {
                    fail(format!(
                        "DependencyNeedsPath: dependency '{name}' needs `path = \"...\"`"
                    ))
                })?;
            if Path::new(path).is_absolute() {
                return Err(fail(format!(
                    "DependencyPathAbsolute: dependency '{name}' has the absolute path {path}; write a path relative to this rdra.toml"
                )));
            }
            sources.push((name.clone(), DependencySource::Path(path.to_string())));
        }
        Ok(sources)
    }
}

/// Expand `inputs` into `.rdra` entry files. A directory contributes its `.rdra` files in
/// file-name order, except those under a nested directory with its own `rdra.toml`, which
/// belong to another model. A file input is taken as given. A path with an `overlays` entry
/// counts as existing, so an unsaved editor buffer loads.
pub fn collect_rdra_files(
    inputs: &[PathBuf],
    overlays: Option<&HashMap<PathBuf, String>>,
) -> Result<Vec<PathBuf>, ProjectError> {
    let mut files = vec![];
    let mut missing = Vec::new();
    for input in inputs {
        if input.is_file() || overlays.is_some_and(|o| o.contains_key(input)) {
            files.push(input.clone());
        } else if input.is_dir() {
            for entry in walkdir::WalkDir::new(input)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| e.depth() == 0 || !e.path().join(MANIFEST_FILE).is_file())
                .flatten()
            {
                if entry.path().extension().is_some_and(|e| e == "rdra") {
                    files.push(entry.path().to_owned());
                }
            }
        } else {
            missing.push(input.display().to_string());
        }
    }
    if !missing.is_empty() {
        return Err(ProjectError::NotFound(missing));
    }
    Ok(files)
}

/// Include paths: the parent and the grandparent of each canonical entry file.
pub fn root_include_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut paths = vec![];
    for f in files {
        let canon = dunce::canonicalize(f).unwrap_or_else(|_| f.clone());
        if let Some(parent) = canon.parent() {
            for ancestor in [parent, parent.parent().unwrap_or(parent)] {
                let ap = ancestor.to_path_buf();
                if seen.insert(ap.clone()) {
                    paths.push(ap);
                }
            }
        }
    }
    paths
}

/// Find the manifest for `inputs`. Each input is made absolute without resolving symlinks,
/// and the search walks up from its directory. It stops at the first directory that holds
/// `.git` (that directory is still searched) or at the filesystem root, so a stray
/// `rdra.toml` above a repository never applies. Every distinct manifest is parsed once, so a
/// malformed one fails whatever the input order. Inputs that belong to two models, or to a
/// model and to none, are `ManifestConflict`.
pub fn discover(inputs: &[PathBuf]) -> Result<Option<Manifest>, ProjectError> {
    let mut found: Option<(Option<PathBuf>, Option<Manifest>)> = None;
    let mut parsed: HashSet<PathBuf> = HashSet::new();
    for input in inputs {
        let path = manifest_path_above(input);
        let manifest = match &path {
            Some(path) if parsed.insert(path.clone()) => Some(parse_manifest(path)?),
            _ => None,
        };
        match &mut found {
            None => found = Some((path, manifest)),
            Some((first, first_manifest)) => {
                if *first != path {
                    return Err(ProjectError::ManifestConflict {
                        first: first.clone(),
                        second: path,
                    });
                }
                if first_manifest.is_none() {
                    *first_manifest = manifest;
                }
            }
        }
    }
    Ok(found.and_then(|(_, manifest)| manifest))
}

/// The manifest directory an input belongs to, or `None` outside any model.
pub fn manifest_root(input: &Path) -> Option<PathBuf> {
    manifest_path_above(input).and_then(|m| m.parent().map(Path::to_path_buf))
}

/// `inputs` split into one group per model, in first-seen order: the inputs under one manifest
/// directory, and the inputs under none. Each group loads as its own project.
pub fn group_by_model(inputs: &[PathBuf]) -> Vec<Vec<PathBuf>> {
    let mut groups: Vec<(Option<PathBuf>, Vec<PathBuf>)> = Vec::new();
    for input in inputs {
        let root = manifest_root(input);
        match groups.iter_mut().find(|(r, _)| *r == root) {
            Some((_, members)) => members.push(input.clone()),
            None => groups.push((root, vec![input.clone()])),
        }
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

fn manifest_path_above(input: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(input).ok()?;
    let start = if absolute.is_dir() {
        lexically_normalized(&absolute)
    } else {
        let mut dir = lexically_normalized(&absolute);
        dir.pop();
        dir
    };
    for dir in start.ancestors() {
        let candidate = dir.join(MANIFEST_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            break;
        }
    }
    None
}

fn lexically_normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn parse_manifest(path: &Path) -> Result<Manifest, ProjectError> {
    let fail = |line, message: String| ProjectError::Manifest {
        path: path.to_path_buf(),
        line,
        message,
    };
    let text = std::fs::read_to_string(path).map_err(|e| fail(None, e.to_string()))?;
    let mut manifest: Manifest = toml::from_str(&text).map_err(|e| {
        let line = e
            .span()
            .map(|span| text[..span.start].matches('\n').count() + 1);
        fail(line, e.message().to_string())
    })?;
    manifest.path = path.to_path_buf();
    Ok(manifest)
}

pub(crate) fn build_project(
    entry_files: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
    overlays: Option<&HashMap<PathBuf, String>>,
    manifest: Option<Manifest>,
    dependencies: Vec<Dependency>,
) -> Project {
    let surfaces: Vec<Arc<DependencySurface>> =
        dependencies.iter().map(|d| d.surface.clone()).collect();
    let (program, mut diags) = resolve_overlaid(
        &entry_files,
        &include_paths,
        overlays,
        &ForeignModules::of(&surfaces),
    );
    let (model, model_diags) = build_merged_model(
        &program,
        &include_paths,
        manifest.as_ref().map(|_| surfaces.as_slice()),
    );
    diags.extend(model_diags);
    Project {
        entry_files,
        include_paths,
        program,
        model,
        diags,
        manifest,
        dependencies,
    }
}

/// Load the project for `inputs`. `overlays` carries unsaved editor text. Each `[dependencies]`
/// entry of the manifest is loaded the same way, as its own model, and a dependency with an
/// error, a missing path, or a cycle back to a model being loaded fails the whole load.
pub fn load_project(
    inputs: &[PathBuf],
    overlays: Option<&HashMap<PathBuf, String>>,
) -> Result<Project, ProjectError> {
    let mut loader = Loader {
        overlays,
        load_path: Vec::new(),
        loaded: HashMap::new(),
    };
    loader.load(inputs)
}

struct Loader<'o> {
    overlays: Option<&'o HashMap<PathBuf, String>>,
    /// Manifest directories on the current load path.
    load_path: Vec<PathBuf>,
    /// Surfaces already loaded, by manifest directory. Each consumer binds one under its own
    /// `[dependencies]` key.
    loaded: HashMap<PathBuf, Arc<DependencySurface>>,
}

impl Loader<'_> {
    fn load(&mut self, inputs: &[PathBuf]) -> Result<Project, ProjectError> {
        let entry_files = collect_rdra_files(inputs, self.overlays)?;
        if entry_files.is_empty() {
            return Err(ProjectError::NoRdraFiles);
        }
        let manifest = discover(inputs)?;
        let dependencies = match &manifest {
            Some(manifest) => self.load_dependencies(manifest)?,
            None => Vec::new(),
        };
        let include_paths = root_include_paths(&entry_files);
        Ok(build_project(
            entry_files,
            include_paths,
            self.overlays,
            manifest,
            dependencies,
        ))
    }

    fn load_dependencies(&mut self, manifest: &Manifest) -> Result<Vec<Dependency>, ProjectError> {
        let manifest_dir = manifest.path.parent().unwrap_or(Path::new("."));
        let own_dir =
            dunce::canonicalize(manifest_dir).unwrap_or_else(|_| manifest_dir.to_path_buf());
        self.load_path.push(own_dir);
        let mut dependencies = Vec::new();
        for (name, source) in manifest.dependency_sources()? {
            let DependencySource::Path(path) = source;
            let fail = |kind, detail: String| ProjectError::Dependency {
                manifest: manifest.path.clone(),
                name: name.clone(),
                path: path.clone(),
                kind,
                detail,
            };
            let dir = manifest_dir.join(&path);
            if !dir.is_dir() {
                return Err(fail(
                    DependencyErrorKind::NotFound,
                    format!("is not a directory: {}", dir.display()),
                ));
            }
            let canon = dunce::canonicalize(&dir).unwrap_or(dir.clone());
            if self.load_path.contains(&canon) {
                let chain: Vec<String> = self
                    .load_path
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect();
                return Err(fail(
                    DependencyErrorKind::Cycle,
                    format!(
                        "depends back on a model being loaded: {}",
                        chain.join(" -> ")
                    ),
                ));
            }
            let surface = match self.loaded.get(&canon) {
                Some(surface) => Arc::new(DependencySurface {
                    name: name.clone(),
                    ..(**surface).clone()
                }),
                None => {
                    if !canon.join(MANIFEST_FILE).is_file() {
                        return Err(fail(
                            DependencyErrorKind::NotFound,
                            format!("has no {MANIFEST_FILE}: {}", dir.display()),
                        ));
                    }
                    let project = self.load(std::slice::from_ref(&dir)).map_err(|e| {
                        fail(DependencyErrorKind::HasErrors, format!("cannot load: {e}"))
                    })?;
                    let errors = DependencySurface::errors(&project);
                    if let Some(first) = errors.first() {
                        return Err(fail(
                            DependencyErrorKind::HasErrors,
                            format!("has {} error(s); the first: {first}", errors.len()),
                        ));
                    }
                    let surface = Arc::new(DependencySurface::of(&name, &project));
                    self.loaded.insert(canon, surface.clone());
                    surface
                }
            };
            dependencies.push(Dependency {
                name,
                path,
                surface,
            });
        }
        self.load_path.pop();
        Ok(dependencies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze_workspace;
    use std::fs;

    const MANIFEST: &str =
        "[model]\nname = \"shop\"\n\n[dependencies]\nauth = { path = \"auth\" }\n";

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

    fn with_auth(root: &Path) {
        write(&root.join("auth/rdra.toml"), "[model]\nname = \"auth\"\n");
        write(&root.join("auth/auth/session.rdra"), AUTH);
    }

    fn layout(root: &Path) -> PathBuf {
        write(
            &root.join("src/shared/actors.rdra"),
            "module shared.actors\nactor Customer \"Customer\"\n",
        );
        write(
            &root.join("src/buc/buy.rdra"),
            "module buc.buy\nimport shared.actors\nusecase Buy \"Buy\"\nperforms(Customer, Buy)\nreads(Customer, Missing)\n",
        );
        root.join("src")
    }

    #[test]
    fn finds_a_manifest_above_the_inputs() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        with_auth(dir.path());
        let src = layout(dir.path());

        let project = load_project(&[src], None).unwrap();

        let manifest = project.manifest.expect("manifest found");
        assert_eq!(manifest.model.name, "shop");
        assert!(manifest.dependencies.contains_key("auth"));
        assert_eq!(manifest.path, dir.path().join("rdra.toml"));
    }

    #[test]
    fn finds_a_manifest_above_a_file_input() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        with_auth(dir.path());
        layout(dir.path());

        let project = load_project(&[dir.path().join("src/buc/buy.rdra")], None).unwrap();

        assert!(project.manifest.is_some());
    }

    #[test]
    fn a_project_without_a_manifest_has_none() {
        let dir = repo();
        let src = layout(dir.path());

        assert!(load_project(&[src], None).unwrap().manifest.is_none());
    }

    #[test]
    fn the_walk_stops_at_the_directory_holding_git() {
        let outer = tempfile::tempdir().unwrap();
        write(
            &outer.path().join("rdra.toml"),
            "[model]\nname = \"stray\"\n",
        );
        let inner = outer.path().join("repo");
        fs::create_dir_all(inner.join(".git")).unwrap();
        let src = layout(&inner);

        assert!(load_project(&[src], None).unwrap().manifest.is_none());
    }

    #[test]
    fn the_directory_holding_git_is_still_searched() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        let src = layout(dir.path());

        assert!(discover(&[src]).unwrap().is_some());
    }

    #[test]
    fn dot_dot_components_are_collapsed_before_the_walk() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        let src = layout(dir.path());

        let manifest = discover(&[src.join("buc/../shared")]).unwrap();

        assert_eq!(manifest.unwrap().path, dir.path().join("rdra.toml"));
    }

    #[test]
    fn a_malformed_manifest_names_the_file_and_line() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\nbroken = = 1\n",
        );
        let src = layout(dir.path());

        let err = load_project(&[src], None).unwrap_err().to_string();

        assert!(err.contains("rdra.toml:3:"), "{err}");
    }

    #[test]
    fn a_malformed_manifest_fails_whatever_the_input_order() {
        let good = repo();
        write(&good.path().join("rdra.toml"), MANIFEST);
        let good_src = layout(good.path());
        let bad = repo();
        write(&bad.path().join("rdra.toml"), "[model]\nbroken = = 1\n");
        let bad_src = layout(bad.path());

        for inputs in [
            [good_src.clone(), bad_src.clone()],
            [bad_src.clone(), good_src.clone()],
        ] {
            let err = discover(&inputs).unwrap_err().to_string();
            assert!(err.contains("rdra.toml:2:"), "{err}");
        }
    }

    #[test]
    fn inputs_of_two_models_are_a_manifest_conflict() {
        let first = repo();
        write(&first.path().join("rdra.toml"), MANIFEST);
        with_auth(first.path());
        let first_src = layout(first.path());
        let second = repo();
        write(
            &second.path().join("rdra.toml"),
            "[model]\nname = \"other\"\n",
        );
        let second_src = layout(second.path());
        let loose = repo();
        let loose_src = layout(loose.path());

        let err = discover(&[first_src.clone(), second_src])
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("ManifestConflict: the inputs belong to two models ("),
            "{err}"
        );
        assert!(
            err.contains("rdra.toml and ") && err.ends_with("run each model on its own"),
            "{err}"
        );

        let err = discover(&[first_src.clone(), loose_src])
            .unwrap_err()
            .to_string();
        assert!(err.contains(" and no manifest)"), "{err}");

        assert!(discover(&[first_src.clone(), first_src.join("buc")])
            .unwrap()
            .is_some());
    }

    #[test]
    fn a_manifest_without_a_model_name_is_rejected() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), "[model]\n");
        let src = layout(dir.path());

        let err = load_project(&[src], None).unwrap_err().to_string();

        assert!(err.contains("rdra.toml:"), "{err}");
        assert!(err.contains("name"), "{err}");
    }

    #[test]
    fn missing_inputs_and_empty_inputs_are_reported() {
        let dir = repo();
        let missing = dir.path().join("nope");
        let err = load_project(std::slice::from_ref(&missing), None).unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");

        let err = load_project(&[dir.path().to_path_buf()], None).unwrap_err();
        assert_eq!(err.to_string(), "no .rdra files found in the given inputs");
    }

    #[test]
    fn overlays_stand_in_for_files_that_are_not_on_disk() {
        let dir = repo();
        let unsaved = dir.path().join("draft.rdra");
        let overlays = HashMap::from([(unsaved.clone(), "usecase Draft \"Draft\"\n".to_string())]);

        let project = load_project(&[unsaved], Some(&overlays)).unwrap();

        assert!(project.model.use_cases.values().any(|u| u.id == "Draft"));
    }

    const AUTH: &str = "module auth.session\nsystem AuthSystem \"Auth\"\nusecase Login \"Log in\"\nactor User \"User\"\nperforms(User, Login)\nexposes(AuthSystem, Login)\n";

    fn shop_and_auth(auth_manifest: &str, auth_src: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = repo();
        write(
            &dir.path().join("shop/rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"../auth\" }\n",
        );
        write(
            &dir.path().join("shop/shop/checkout.rdra"),
            "module shop.checkout\nusecase Checkout \"Checkout\"\n",
        );
        write(&dir.path().join("auth/rdra.toml"), auth_manifest);
        write(&dir.path().join("auth/auth/session.rdra"), auth_src);
        let shop = dir.path().join("shop");
        (dir, shop)
    }

    #[test]
    fn a_path_dependency_is_loaded_as_its_own_model() {
        let (_dir, shop) = shop_and_auth("[model]\nname = \"auth\"\n", AUTH);

        let project = load_project(&[shop], None).unwrap();

        assert!(project.diags.is_empty(), "{:?}", project.diags);
        let [dep] = project.dependencies.as_slice() else {
            panic!("one dependency, got {:?}", project.dependencies);
        };
        assert_eq!((dep.name.as_str(), dep.path.as_str()), ("auth", "../auth"));
        assert_eq!(dep.surface.systems, ["AuthSystem"]);
        assert_eq!(dep.surface.surface.use_cases[0].id, "Login");
        assert_eq!(dep.surface.modules, ["auth.session"]);
        assert_eq!(
            dep.surface.declared.get("User").map(String::as_str),
            Some("actor")
        );
        assert!(project.model.use_cases.values().all(|u| u.id == "Checkout"));
    }

    #[test]
    fn a_missing_dependency_path_names_the_dependency_and_the_cause() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"../nope\" }\n",
        );
        let src = layout(dir.path());

        let err = load_project(&[src], None).unwrap_err().to_string();

        assert!(
            err.contains("DependencyNotFound: dependency 'auth' (path ../nope) is not a directory"),
            "{err}"
        );
    }

    #[test]
    fn a_dependency_with_errors_fails_the_load() {
        let (_dir, shop) = shop_and_auth(
            "[model]\nname = \"auth\"\n",
            "module auth.session\nperforms(Nobody, Login)\n",
        );

        let err = load_project(&[shop], None).unwrap_err().to_string();

        assert!(
            err.contains("DependencyHasErrors: dependency 'auth' (path ../auth) has 2 error(s); the first: error: "),
            "{err}"
        );
        assert!(err.contains("session.rdra:2:"), "{err}");
        assert!(err.contains("undefined symbol: Nobody"), "{err}");
    }

    #[test]
    fn a_dependency_cycle_is_reported() {
        let (_dir, shop) = shop_and_auth(
            "[model]\nname = \"auth\"\n[dependencies]\nshop = { path = \"../shop\" }\n",
            AUTH,
        );

        let err = load_project(&[shop], None).unwrap_err().to_string();

        assert!(err.contains("DependencyCycle: dependency 'shop' (path ../shop) depends back on a model being loaded"), "{err}");
    }

    #[test]
    fn a_transitive_dependency_is_checked_but_not_a_direct_dependency() {
        let (dir, shop) = shop_and_auth(
            "[model]\nname = \"auth\"\n[dependencies]\nidp = { path = \"../idp\" }\n",
            AUTH,
        );
        write(
            &dir.path().join("idp/rdra.toml"),
            "[model]\nname = \"idp\"\n",
        );
        write(
            &dir.path().join("idp/idp/core.rdra"),
            "module idp.core\nreads(Nobody, Nothing)\n",
        );

        let err = load_project(std::slice::from_ref(&shop), None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("DependencyHasErrors: dependency 'auth' (path ../auth)"),
            "{err}"
        );
        assert!(
            err.contains("DependencyHasErrors: dependency 'idp' (path ../idp)"),
            "{err}"
        );

        write(
            &dir.path().join("idp/idp/core.rdra"),
            "module idp.core\nusecase Verify \"Verify\"\n",
        );
        let project = load_project(&[shop], None).unwrap();
        let names: Vec<&str> = project
            .dependencies
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(names, ["auth"]);
    }

    #[test]
    fn a_git_dependency_is_refused_for_now() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\npay = { git = \"https://x/y.git\", rev = \"v1\" }\n",
        );
        let src = layout(dir.path());

        let err = load_project(&[src], None).unwrap_err().to_string();

        assert!(
            err.contains("rdra.toml: DependencyGitUnsupported: dependency 'pay' uses `git`"),
            "{err}"
        );
    }

    #[test]
    fn a_dependency_nested_under_the_product_is_not_a_product_file() {
        let dir = repo();
        write(
            &dir.path().join("shop/rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"vendor/auth\" }\n",
        );
        write(
            &dir.path().join("shop/shop/checkout.rdra"),
            "module shop.checkout\nextsystem Auth \"Auth\" from auth.AuthSystem\nusecase Checkout \"Checkout\"\nactor U \"U\"\nperforms(U, Checkout)\ninvokes(Checkout, Auth.Login)\n",
        );
        write(
            &dir.path().join("shop/vendor/auth/rdra.toml"),
            "[model]\nname = \"auth\"\n",
        );
        write(&dir.path().join("shop/vendor/auth/auth/session.rdra"), AUTH);

        let project = load_project(&[dir.path().join("shop")], None).unwrap();

        assert!(project.diags.is_empty(), "{:?}", project.diags);
        let files: Vec<String> = project
            .program
            .sources
            .iter()
            .map(|(p, _, _)| p.display().to_string())
            .collect();
        assert!(files.iter().all(|f| !f.contains("vendor")), "{files:?}");
        let local: Vec<&str> = project
            .model
            .use_cases
            .values()
            .map(|u| u.id.as_str())
            .collect();
        assert_eq!(local, ["Checkout"]);
        assert_eq!(project.dependencies[0].name, "auth");
    }

    #[test]
    fn one_directory_binds_under_each_consumers_own_key() {
        let dir = repo();
        write(
            &dir.path().join("shop/rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"../idp\" }\nalso = { path = \"../idp\" }\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n",
        );
        write(
            &dir.path().join("shop/shop/checkout.rdra"),
            "module shop.checkout\nextsystem Auth \"Auth\" from auth.AuthSystem\nextsystem Also \"Also\" from also.AuthSystem\nusecase Checkout \"Checkout\"\nactor U \"U\"\nperforms(U, Checkout)\ninvokes(Checkout, Auth.Login)\ninvokes(Checkout, Also.Login)\n",
        );
        write(
            &dir.path().join("idp/rdra.toml"),
            "[model]\nname = \"idp\"\n",
        );
        write(&dir.path().join("idp/auth/session.rdra"), AUTH);
        for (side, key) in [("left", "idp"), ("right", "identity")] {
            write(
                &dir.path().join(format!("{side}/rdra.toml")),
                &format!(
                    "[model]\nname = \"{side}\"\n[dependencies]\n{key} = {{ path = \"../idp\" }}\n"
                ),
            );
            write(
                &dir.path().join(format!("{side}/{side}/main.rdra")),
                &format!("module {side}.main\nextsystem Idp \"Idp\" from {key}.AuthSystem\nusecase Go \"Go\"\nactor U \"U\"\nperforms(U, Go)\ninvokes(Go, Idp.Login)\n"),
            );
        }

        let project = load_project(&[dir.path().join("shop")], None).unwrap();

        assert!(project.diags.is_empty(), "{:?}", project.diags);
        let names: Vec<&str> = project
            .dependencies
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(names, ["also", "auth", "left", "right"]);
        assert!(project
            .dependencies
            .iter()
            .all(|d| d.surface.name == d.name));
    }

    #[test]
    fn a_dependency_with_a_lifecycle_error_fails_the_load() {
        let (_dir, shop) = shop_and_auth(
            "[model]\nname = \"auth\"\n",
            "module auth.session\nactor U \"U\"\nusecase Add \"Add\"\nperforms(U, Add)\nentity CartLine \"Line\" { id: Int @pk  qty: Int @check(qty >= 1) }\ncreates(Add, CartLine)\nsets(Add, CartLine, qty == 0)\n",
        );

        let err = load_project(&[shop], None).unwrap_err().to_string();

        assert!(
            err.contains("DependencyHasErrors: dependency 'auth' (path ../auth) has 1 error(s); the first: error[Check_CartLine_qty_0]: "),
            "{err}"
        );
    }

    #[test]
    fn an_absolute_dependency_path_is_rejected() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"/srv/auth\" }\n",
        );
        let src = layout(dir.path());

        let err = load_project(&[src], None).unwrap_err().to_string();

        assert!(
            err.contains("rdra.toml: DependencyPathAbsolute: dependency 'auth' has the absolute path /srv/auth; write a path relative to this rdra.toml"),
            "{err}"
        );
    }

    #[test]
    fn inputs_are_grouped_by_model() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        with_auth(dir.path());
        let src = layout(dir.path());
        let auth_file = dir.path().join("auth/auth/session.rdra");
        let loose = tempfile::tempdir().unwrap();
        write(&loose.path().join("x.rdra"), "usecase X \"X\"\n");

        let groups = group_by_model(&[
            src.clone(),
            auth_file.clone(),
            loose.path().join("x.rdra"),
            src.join("buc"),
        ]);

        assert_eq!(
            groups,
            vec![
                vec![src.clone(), src.join("buc")],
                vec![auth_file],
                vec![loose.path().join("x.rdra")],
            ]
        );
    }

    #[test]
    fn a_manifest_changes_neither_include_paths_nor_diagnostics() {
        let plain = repo();
        let with = repo();
        write(&with.path().join("rdra.toml"), MANIFEST);
        with_auth(with.path());
        let plain_src = layout(plain.path());
        let with_src = layout(with.path());

        let cli_plain = load_project(std::slice::from_ref(&plain_src), None).unwrap();
        let cli_with = load_project(std::slice::from_ref(&with_src), None).unwrap();
        let rebase = |paths: &[PathBuf], root: &Path| -> Vec<PathBuf> {
            let root = dunce::canonicalize(root).unwrap();
            paths
                .iter()
                .map(|p| p.strip_prefix(&root).unwrap().to_path_buf())
                .collect()
        };
        assert_eq!(
            rebase(&cli_plain.include_paths, plain.path()),
            rebase(&cli_with.include_paths, with.path()),
        );
        let messages = |diags: &[Diagnostic], root: &Path| -> Vec<String> {
            let root = dunce::canonicalize(root).unwrap();
            diags
                .iter()
                .map(|d| {
                    d.error
                        .to_string()
                        .replace(root.to_str().unwrap(), "<root>")
                })
                .collect()
        };
        let cli_messages = messages(&cli_plain.diags, plain.path());
        assert!(
            !cli_messages.is_empty(),
            "the layout should raise a diagnostic"
        );
        assert_eq!(cli_messages, messages(&cli_with.diags, with.path()));

        let lsp_with = analyze_workspace(
            &cli_with.entry_files,
            &cli_with.include_paths,
            &HashMap::new(),
        );
        let lsp_plain = analyze_workspace(
            &cli_plain.entry_files,
            &cli_plain.include_paths,
            &HashMap::new(),
        );
        let lsp_messages = messages(&lsp_plain.diagnostics, plain.path());
        assert_eq!(lsp_messages, messages(&lsp_with.diagnostics, with.path()));
        assert!(lsp_messages.starts_with(&cli_messages));
    }
}
