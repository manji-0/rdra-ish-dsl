//! `check --code[=fresh|build|run]` (docs/design/code-check.md): does the service code
//! `[code.<lang>]` in rdra.toml names have what the model requires?
//!
//! rdra-ish parses none of the user's code. `fresh` generates every configured file again
//! and compares (by hash for the traces); `build` runs the language's compiler on the probe
//! and maps its errors back to model locations; `run` runs the conformance test the probe
//! holds. A language plugs in through [`Backend`]; the rest is shared.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rdra_ish_core::project::{CodeTarget, Manifest};

use crate::cli::CodeLevel;
use crate::codegen_cmd::{
    check_code, check_traces, configured_request, generate, shown, Generated, Loaded, Request,
};

/// One finding of the code check, printed as `error[Name]: message` with the model location,
/// the code location, and the fix on the lines under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeDiagnostic {
    /// `CodeMissingUseCase_ShipOrder`: a kind and the model name it is about.
    pub name: String,
    pub message: String,
    /// `path:line:col` and what is declared there.
    pub model: Option<String>,
    /// The compiler's or the scanner's location, and its message cut to one line.
    pub code: Option<String>,
    pub fix: Option<String>,
    /// A warning does not fail the check.
    pub warning: bool,
}

impl fmt::Display for CodeDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = if self.warning { "warning" } else { "error" };
        write!(f, "{level}[{}]: {}", self.name, self.message)?;
        for (key, value) in [
            ("model:", &self.model),
            ("code: ", &self.code),
            ("fix:  ", &self.fix),
        ] {
            if let Some(v) = value {
                write!(f, "\n  {key} {v}")?;
            }
        }
        Ok(())
    }
}

/// What a language's backend is given for one `[code.<lang>]` table. The fields the fresh-only
/// backends do not read yet are for the build backends.
#[allow(dead_code)]
pub(crate) struct Target<'a> {
    /// `rust`, `go`, or `ts`.
    pub name: &'static str,
    pub target: &'a CodeTarget,
    pub manifest: &'a Manifest,
    pub request: Request,
    pub generated: Generated,
}

impl Target<'_> {
    /// A path of the table, resolved against the directory of rdra.toml.
    pub fn path(&self, path: &Path) -> PathBuf {
        self.manifest.dir().join(path)
    }

    #[allow(dead_code)]
    /// The directory the build runs in and the ledger scans: `root`, else the nearest
    /// directory above `domain` holding a project file of any language.
    pub fn root(&self) -> PathBuf {
        if let Some(root) = &self.target.root {
            return self.path(root);
        }
        let domain = self.path(&self.target.domain);
        let start = domain.parent().map(Path::to_path_buf).unwrap_or_default();
        start
            .ancestors()
            .find(|d| {
                ["Cargo.toml", "go.mod", "package.json", "tsconfig.json"]
                    .iter()
                    .any(|f| d.join(f).is_file())
            })
            .map_or(start.clone(), Path::to_path_buf)
    }
}

/// A language's part of `check --code`: the files only it writes and the compiler it runs.
/// The default commands run only for `--code=build` / `--code=run`, and so does a command
/// `rdra.toml` gives in their place (decision 7).
pub(crate) trait Backend {
    /// The generated files beyond the domain code and the traces that `fresh` compares
    /// (the probe): `(path, text)`.
    fn extra_files(&self, _target: &Target<'_>) -> Result<Vec<(PathBuf, String)>> {
        Ok(Vec::new())
    }

    /// `build`: compile the probe and map the errors; `None` when the language has no build
    /// check yet.
    fn build(&self, _target: &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>> {
        None
    }

    /// `run`: run the conformance test and map its failure; `None` when the language has no
    /// run check yet.
    fn run(&self, _target: &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>> {
        None
    }
}

/// A level past `fresh`: the backend's `build` or `run`.
type Step = fn(&dyn Backend, &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>>;

/// A language with no build or run check yet: `fresh` only.
struct FreshOnly;
impl Backend for FreshOnly {}

fn backend(_name: &str) -> Box<dyn Backend> {
    Box::new(FreshOnly)
}

/// Checks the code `manifest`'s `[code]` names at `level`; prints the findings on stderr and
/// a summary line on stdout. Whether it passed.
pub(crate) fn run(inputs: &[PathBuf], manifest: &Manifest, level: CodeLevel) -> Result<bool> {
    let loaded = Loaded::load(inputs)?;
    let code = manifest.code.as_ref().expect("the caller checked [code]");
    let mut failed = 0usize;
    let mut findings: Vec<CodeDiagnostic> = Vec::new();
    for (name, target) in code.targets() {
        let request = configured_request(manifest, name, target, inputs);
        let generated = generate(&loaded, &request)?;
        let t = Target {
            name,
            target,
            manifest,
            request,
            generated,
        };
        let backend = backend(name);
        let mut fresh = true;
        fresh &= compare(&t.path(&target.domain), |p| {
            check_code(p, &t.generated.code, &t.generated.model_hash)
        })?;
        if let (Some(path), Some((text, _))) = (&target.traces, &t.generated.traces) {
            fresh &= compare(&t.path(path), |p| {
                check_traces(p, text, &t.generated.model_hash)
            })?;
        }
        for (path, text) in backend.extra_files(&t)? {
            fresh &= compare(&path, |p| check_code(p, &text, &t.generated.model_hash))?;
        }
        if !fresh {
            failed += 1;
            eprintln!(
                "  generate them again: rdra-ish codegen {} --code",
                inputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            continue;
        }
        let steps: [(CodeLevel, &str, Step); 2] = [
            (CodeLevel::Build, "build", |b, t| b.build(t)),
            (CodeLevel::Run, "run", |b, t| b.run(t)),
        ];
        for (at, what, step) in steps {
            if level < at {
                break;
            }
            match step(backend.as_ref(), &t) {
                None => {
                    eprintln!("note: --code={what} is not supported for [code.{name}] yet; checked up to the level before it");
                    break;
                }
                Some(result) => {
                    let found = result?;
                    let errors = found.iter().filter(|d| !d.warning).count();
                    findings.extend(found);
                    if errors > 0 {
                        failed += errors;
                        break;
                    }
                }
            }
        }
    }
    for d in &findings {
        eprintln!("{d}");
    }
    let level_name = match level {
        CodeLevel::Fresh => "fresh",
        CodeLevel::Build => "build",
        CodeLevel::Run => "run",
    };
    if failed > 0 {
        println!("FAIL: code: {failed} error(s) at --code={level_name}");
        return Ok(false);
    }
    println!("OK: code: what the model requires, at --code={level_name}");
    Ok(true)
}

/// Runs `check` on `path`, or reports that the file is missing.
fn compare(path: &Path, check: impl FnOnce(&Path) -> Result<bool>) -> Result<bool> {
    if !path.is_file() {
        eprintln!(
            "error: {} does not exist: `codegen --code` writes it",
            shown(path)
        );
        return Ok(false);
    }
    check(path)
}
