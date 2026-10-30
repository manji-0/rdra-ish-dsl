//! The project loader the CLI, the LSP, and the tests share.
//!
//! A project is the entry files a command was given, the include paths those files imply, the
//! resolved program and merged model, and the `rdra.toml` manifest found above the inputs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::{build_merged_model, resolve_overlaid, Diagnostic, ResolvedProgram, SemanticModel};

pub const MANIFEST_FILE: &str = "rdra.toml";

/// The parsed `rdra.toml`. Nothing reads `dependencies` yet.
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
        }
    }
}

impl std::error::Error for ProjectError {}

/// Everything one load produces.
#[derive(Debug)]
pub struct Project {
    pub entry_files: Vec<PathBuf>,
    pub include_paths: Vec<PathBuf>,
    pub program: ResolvedProgram,
    pub model: SemanticModel,
    pub diags: Vec<Diagnostic>,
    pub manifest: Option<Manifest>,
}

/// Expand `inputs` into `.rdra` entry files. A directory contributes its `.rdra` files in
/// file-name order. A file input is taken as given. A path with an `overlays` entry counts as
/// existing, so an unsaved editor buffer loads.
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

/// Include paths: the parent and the grandparent of each canonical entry file. This lets
/// `shared/actors.rdra` resolve from a root that holds both `shared/` and `buc/`.
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
/// `rdra.toml` above a repository never applies. The first input with a manifest wins.
pub fn discover(inputs: &[PathBuf]) -> Result<Option<Manifest>, ProjectError> {
    for input in inputs {
        let Ok(absolute) = std::path::absolute(input) else {
            continue;
        };
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
                return parse_manifest(&candidate).map(Some);
            }
            if dir.join(".git").exists() {
                break;
            }
        }
    }
    Ok(None)
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

/// Resolve and build one project from entry files and include paths already chosen.
pub(crate) fn build_project(
    entry_files: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
    overlays: Option<&HashMap<PathBuf, String>>,
    manifest: Option<Manifest>,
) -> Project {
    let (program, mut diags) = resolve_overlaid(&entry_files, &include_paths, overlays);
    let (model, model_diags) = build_merged_model(&program, &include_paths);
    diags.extend(model_diags);
    Project {
        entry_files,
        include_paths,
        program,
        model,
        diags,
        manifest,
    }
}

/// Load the project for `inputs`. `overlays` carries unsaved editor text.
pub fn load_project(
    inputs: &[PathBuf],
    overlays: Option<&HashMap<PathBuf, String>>,
) -> Result<Project, ProjectError> {
    let entry_files = collect_rdra_files(inputs, overlays)?;
    if entry_files.is_empty() {
        return Err(ProjectError::NoRdraFiles);
    }
    let manifest = discover(inputs)?;
    let include_paths = root_include_paths(&entry_files);
    Ok(build_project(
        entry_files,
        include_paths,
        overlays,
        manifest,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze_workspace;
    use std::fs;

    const MANIFEST: &str =
        "[model]\nname = \"shop\"\n\n[dependencies]\nauth = { path = \"../auth\" }\n";

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        dir
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

    #[test]
    fn a_manifest_changes_neither_include_paths_nor_diagnostics() {
        let plain = repo();
        let with = repo();
        write(&with.path().join("rdra.toml"), MANIFEST);
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
