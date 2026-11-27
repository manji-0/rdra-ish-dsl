//! The project loader the CLI, the LSP, and the tests share.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::memory::MemorySize;
use crate::{build_merged_model, resolve_overlaid, Diagnostic, ResolvedProgram, SemanticModel};

pub const MANIFEST_FILE: &str = "rdra.toml";

/// `rdra.toml`. An unknown table or key is an error, so a misspelled `[verfiy]` cannot leave a
/// command at its defaults; only `[dependencies]` stays free-form.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(skip)]
    pub path: PathBuf,
    pub model: ModelSection,
    #[serde(default)]
    pub dependencies: BTreeMap<String, toml::Value>,
    /// `[check]`: what `check` and `states` run with.
    #[serde(default)]
    pub check: CommandSection,
    /// `[verify]`: what `verify` runs with.
    #[serde(default)]
    pub verify: CommandSection,
    /// `[code]`: the service code `check --code` holds against the model
    /// (docs/design/code-check.md).
    pub code: Option<CodeSection>,
}

/// `[code]`: one table per language whose code follows the model, and the obligation ledger.
/// Every path is relative to the directory of `rdra.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeSection {
    pub rust: Option<CodeTarget>,
    pub go: Option<CodeTarget>,
    /// `[code.ts]`, or `[code.typescript]` as `--lang typescript` spells it.
    #[serde(alias = "typescript")]
    pub ts: Option<CodeTarget>,
    /// `[code.obligations]`: obligation name -> the file that owns it.
    #[serde(default)]
    pub obligations: BTreeMap<String, PathBuf>,
}

impl CodeSection {
    /// The configured languages, by their table name, in a fixed order.
    pub fn targets(&self) -> Vec<(&'static str, &CodeTarget)> {
        [("rust", &self.rust), ("go", &self.go), ("ts", &self.ts)]
            .into_iter()
            .filter_map(|(name, t)| t.as_ref().map(|t| (name, t)))
            .collect()
    }

    /// The headers a table of `targets` may be spelled with: `[code.ts]` also as
    /// `[code.typescript]`.
    pub fn headers(name: &str) -> Vec<String> {
        let mut headers = vec![format!("[code.{name}]")];
        if name == "ts" {
            headers.push("[code.typescript]".to_string());
        }
        headers
    }
}

/// `[code.<lang>]`: the generated files of one language and how its service binds to them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeTarget {
    /// The `codegen` output: the domain types, functions, contracts and, with `traces`, the
    /// conformance harness. A table names it or `adapter`, not both.
    pub domain: Option<PathBuf>,
    /// The black-box model adapter (`codegen --adapter`) in place of the domain code: the
    /// service implements `ModelAdapter` over code of its own, and `check --code=fresh`
    /// compares only this file, the traces and the probe. Needs `traces`.
    pub adapter: Option<PathBuf>,
    /// The conformance traces; with it the domain code holds the harness.
    pub traces: Option<PathBuf>,
    /// `--conformance-states` of the traces.
    pub conformance_states: Option<usize>,
    /// The probe `codegen` writes: one line per use case stating that `service` implements it.
    pub probe: Option<PathBuf>,
    /// The type that implements the contracts, as the probe names it
    /// (`ecsvc::service::EcService`).
    pub service: Option<String>,
    /// Rust: the module path the probe imports the contracts from (`ecsvc::domain`).
    /// Required for a Rust probe: without it, no probe is written.
    pub module: Option<String>,
    /// An expression the conformance test calls for a fresh service. Without it, there is no
    /// conformance run.
    pub new: Option<String>,
    /// Go: the package name of `domain` (`--package`).
    pub package: Option<String>,
    /// The directory the build runs in and the ledger scans. Default: the nearest directory
    /// above `domain` with `Cargo.toml`, `go.mod`, `package.json` or `tsconfig.json`.
    pub root: Option<PathBuf>,
    /// The command `check --code=build` runs in place of the language's default, as argv.
    /// It runs only when `--code=build` or `--code=run` is given on the command line.
    pub build: Option<Vec<String>>,
    /// The command `check --code=run` runs in place of the language's default, as argv.
    pub run: Option<Vec<String>>,
    /// TypeScript: generate the state types sealed (`codegen --seal`).
    pub seal: Option<bool>,
    /// TypeScript: the project's tsconfig that the generated `tsconfig.rdra.json` extends.
    /// Default: `tsconfig.json` in `root`, when it exists.
    pub tsconfig: Option<PathBuf>,
}

impl CodeTarget {
    /// The generated code file: `adapter`, else `domain`. A table with neither is refused
    /// before any command reads it ([`CodeTarget::problem`]).
    pub fn code_file(&self) -> &Path {
        self.adapter
            .as_deref()
            .or(self.domain.as_deref())
            .unwrap_or(Path::new(""))
    }

    /// What is wrong with the table's choice of code file, when something is.
    pub fn problem(&self) -> Option<&'static str> {
        match (&self.domain, &self.adapter) {
            (None, None) => Some("names neither `domain` nor `adapter`"),
            (Some(_), Some(_)) => {
                Some("names both `domain` and `adapter`: the adapter stands in for the domain code")
            }
            (None, Some(_)) if self.traces.is_none() => {
                Some("names `adapter` without `traces`: the adapter drives the traces")
            }
            _ => None,
        }
    }
}

impl Manifest {
    /// The directory `rdra.toml` is in: every `[code]` path is relative to it.
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSection {
    pub name: String,
}

/// The settings of one command in `rdra.toml`; its flag overrides each of them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSection {
    /// `memory = "4GiB"`: what the command may use (`--memory`).
    pub memory: Option<MemorySize>,
    /// `instances = { Loan = 4 }`: the slot count of each entity it names (`--instances`).
    #[serde(default)]
    pub instances: BTreeMap<String, std::num::NonZeroU8>,
    /// `symmetry = "always"`: when `verify` explores a safety cone up to instance symmetry
    /// (`--symmetry`). Only `[verify]` reads it.
    pub symmetry: Option<crate::lifecycle::Symmetry>,
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
/// `rdra.toml` above a repository never applies. Every input's manifest is parsed, so a
/// malformed one fails whatever the input order. The first input's manifest is the project's.
pub fn discover(inputs: &[PathBuf]) -> Result<Option<Manifest>, ProjectError> {
    let mut found = None;
    for input in inputs {
        let Some(path) = manifest_path_above(input) else {
            continue;
        };
        let manifest = parse_manifest(&path)?;
        found.get_or_insert(manifest);
    }
    Ok(found)
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
    // A `[code.<lang>]` table names its code file once, as a required key would.
    if let Some(code) = &manifest.code {
        for (name, target) in code.targets() {
            if let Some(problem) = target.problem() {
                let table = format!("[code.{name}]");
                let headers = CodeSection::headers(name);
                let line = text
                    .lines()
                    .position(|l| headers.iter().any(|h| l.trim() == h))
                    .map(|i| i + 1);
                return Err(fail(line, format!("{table} {problem}")));
            }
        }
    }
    Ok(manifest)
}

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
    fn memory_is_read_per_command() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n\n[check]\nmemory = \"512MiB\"\n\n[verify]\nmemory = \"8G\"\n",
        );
        let src = layout(dir.path());

        let manifest = discover(&[src]).unwrap().unwrap();

        assert_eq!(
            manifest.check.memory.map(|m| m.to_string()).as_deref(),
            Some("512MiB")
        );
        assert_eq!(
            manifest.verify.memory.map(|m| m.to_string()).as_deref(),
            Some("8GiB")
        );
    }

    #[test]
    fn a_manifest_without_memory_leaves_it_to_the_default() {
        let dir = repo();
        write(&dir.path().join("rdra.toml"), MANIFEST);
        let src = layout(dir.path());

        let manifest = discover(&[src]).unwrap().unwrap();

        assert_eq!(
            (manifest.check.memory, manifest.verify.memory),
            (None, None)
        );
    }

    #[test]
    fn a_bad_memory_value_names_the_file_line_and_value() {
        for (body, line, what) in [
            ("[verify]\nmemory = \"4096\"\n", 4, "\"4096\""),
            ("[check]\n\nmemory = \"lots\"\n", 5, "\"lots\""),
            ("[check]\nmemory = 4\n", 4, "string"),
            ("[check]\nmemroy = \"4GiB\"\n", 4, "memroy"),
        ] {
            let dir = repo();
            write(
                &dir.path().join("rdra.toml"),
                &format!("[model]\nname = \"shop\"\n{body}"),
            );
            let src = layout(dir.path());

            let err = discover(&[src]).unwrap_err().to_string();

            assert!(err.contains(&format!("rdra.toml:{line}:")), "{body}: {err}");
            assert!(err.contains(what), "{body}: {err}");
        }
    }

    #[test]
    fn a_misspelled_table_or_model_key_names_the_file_line_and_key() {
        for (body, line, what) in [
            (
                "[model]\nname = \"shop\"\n[verfiy]\nmemory = \"1KiB\"\n",
                3,
                "verfiy",
            ),
            (
                "[model]\nname = \"shop\"\n\n[chek]\nmemory = \"1KiB\"\n",
                4,
                "chek",
            ),
            ("[model]\nname = \"shop\"\nnmae = \"typo\"\n", 3, "nmae"),
            (
                "depenencies = 1\n[model]\nname = \"shop\"\n",
                1,
                "depenencies",
            ),
        ] {
            let dir = repo();
            write(&dir.path().join("rdra.toml"), body);
            let src = layout(dir.path());

            let err = discover(&[src]).unwrap_err().to_string();

            assert!(err.contains(&format!("rdra.toml:{line}:")), "{body}: {err}");
            assert!(err.contains(what), "{body}: {err}");
        }
    }

    #[test]
    fn dependencies_stay_free_form() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n\n[dependencies]\nauth = { path = \"../auth\", anything = 1 }\nother = \"1.0\"\n",
        );
        let src = layout(dir.path());

        let manifest = discover(&[src]).unwrap().unwrap();

        assert_eq!(manifest.dependencies.len(), 2);
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
    fn the_first_inputs_manifest_is_the_projects() {
        let first = repo();
        write(&first.path().join("rdra.toml"), MANIFEST);
        let first_src = layout(first.path());
        let second = repo();
        write(
            &second.path().join("rdra.toml"),
            "[model]\nname = \"other\"\n",
        );
        let second_src = layout(second.path());

        let manifest = discover(&[first_src, second_src]).unwrap().unwrap();

        assert_eq!(manifest.model.name, "shop");
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
    fn code_typescript_is_the_ts_table() {
        let dir = repo();
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n\n[code.typescript]\ndomain = \"web/domain.ts\"\nseal = true\n",
        );
        let src = layout(dir.path());

        let manifest = discover(std::slice::from_ref(&src)).unwrap().unwrap();
        let code = manifest.code.unwrap();
        assert_eq!(code.targets().len(), 1);
        assert_eq!(code.ts.unwrap().seal, Some(true));

        // Both spellings at once are one table named twice.
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n\n[code.ts]\ndomain = \"a.ts\"\n\n[code.typescript]\ndomain = \"b.ts\"\n",
        );
        let err = load_project(std::slice::from_ref(&src), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("duplicate"), "{err}");

        // A problem in the table names the line it is spelled on.
        write(
            &dir.path().join("rdra.toml"),
            "[model]\nname = \"shop\"\n\n[code.typescript]\ntraces = \"t.txt\"\n",
        );
        let err = load_project(&[src], None).unwrap_err().to_string();
        assert!(
            err.contains("rdra.toml:4: [code.ts] names neither"),
            "{err}"
        );
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
