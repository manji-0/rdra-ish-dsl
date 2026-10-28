//! `rdra-ish verify --backend tlc` — export TLA+ and run TLC.

use anyhow::{bail, Context, Result};
use rdra_ish_core::SemanticModel;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::VerifyBackend;
use crate::export::{tla_bounds, tla_bundle};
use crate::tlc_output::{summarize, Outcome, TlcRun};

pub fn run_verify(
    model: &SemanticModel,
    backend: VerifyBackend,
    out: Option<PathBuf>,
    instance_count: u32,
    int_range_max: u32,
) -> Result<()> {
    for notice in condition_body_notices(model) {
        println!("{notice}");
    }
    match backend {
        VerifyBackend::Tlc => run_tlc(model, out, instance_count, int_range_max),
    }
}

/// A `holds` body is stored and validated in phase 3 but no engine reads it, so say so instead
/// of letting a passing run imply the condition was checked.
pub(crate) fn condition_body_notices(model: &SemanticModel) -> Vec<String> {
    let mut ids: Vec<&str> = model
        .conditions
        .values()
        .filter(|condition| condition.body.is_some())
        .map(|condition| condition.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.into_iter()
        .map(|id| format!("condition {id}: body stored, not yet checked (phase 4)"))
        .collect()
}

/// Returns (tla_path, cfg_path, work_dir, tla_filename_for_tlc, cfg_filename_for_tlc, keep).
fn resolve_tla_out_paths(
    out: Option<PathBuf>,
    module_name: &str,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf, bool)> {
    match out {
        Some(path) if path.extension().is_some_and(|e| e == "tla") => {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let work_dir = if parent.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
                parent.to_path_buf()
            };
            let cfg = path.with_extension("cfg");
            let tla_name = PathBuf::from(path.file_name().unwrap_or_default());
            let cfg_name = PathBuf::from(cfg.file_name().unwrap_or_default());
            Ok((path, cfg, work_dir, tla_name, cfg_name, true))
        }
        Some(path) => {
            let dir = if (path.exists() && path.is_dir()) || path.extension().is_none() {
                path
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
            let tla_name = PathBuf::from(format!("{module_name}.tla"));
            let cfg_name = PathBuf::from(format!("{module_name}.cfg"));
            let tla = dir.join(&tla_name);
            let cfg = dir.join(&cfg_name);
            Ok((tla, cfg, dir, tla_name, cfg_name, true))
        }
        None => {
            let dir = std::env::temp_dir().join(format!("rdra-ish-tlc-{}", std::process::id()));
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
            let tla_name = PathBuf::from(format!("{module_name}.tla"));
            let cfg_name = PathBuf::from(format!("{module_name}.cfg"));
            let tla = dir.join(&tla_name);
            let cfg = dir.join(&cfg_name);
            Ok((tla, cfg, dir, tla_name, cfg_name, false))
        }
    }
}

fn run_tlc(
    model: &SemanticModel,
    out: Option<PathBuf>,
    instance_count: u32,
    int_range_max: u32,
) -> Result<()> {
    let module_hint = out
        .as_ref()
        .filter(|p| p.extension().is_some_and(|e| e == "tla"))
        .and_then(|p| p.file_stem()?.to_str().map(str::to_string));

    let bounds = tla_bounds(instance_count, int_range_max)?;
    let bundle = tla_bundle(model, bounds, module_hint.as_deref().unwrap_or("RdraSpec"))?;

    let (tla_path, cfg_path, work_dir, tla_arg, cfg_arg, keep) =
        resolve_tla_out_paths(out, &bundle.module_name)?;

    fs::write(&tla_path, &bundle.tla)
        .with_context(|| format!("failed to write {}", tla_path.display()))?;
    fs::write(&cfg_path, &bundle.cfg)
        .with_context(|| format!("failed to write {}", cfg_path.display()))?;

    println!("wrote {}", tla_path.display());
    println!("wrote {}", cfg_path.display());

    let tlc = find_tlc()?;
    // Pass bare filenames so TLC resolves them relative to work_dir (module name == file stem).
    let output = Command::new(&tlc)
        .arg("-config")
        .arg(&cfg_arg)
        .arg(&tla_arg)
        .current_dir(&work_dir)
        .output()
        .with_context(|| format!("failed to execute {}", tlc.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if !keep {
        let _ = fs::remove_dir_all(&work_dir);
    }

    let run = TlcRun::parse(&combined);
    let outcome = run.outcome(output.status.success());
    if outcome == Outcome::Ok {
        println!("tlc_result: ok");
        println!("TLC: OK");
        return Ok(());
    }
    eprintln!("tlc_result: {}", outcome.as_str());
    match outcome {
        Outcome::Ok => Ok(()),
        Outcome::Counterexample => {
            bail!(
                "TLC verification failed (spec: {})\n{}",
                tla_path.display(),
                summarize(&run, &combined)
            );
        }
        Outcome::ToolError => {
            bail!(
                "TLC tool failed without a model violation (spec: {})\n{}",
                tla_path.display(),
                summarize(&run, &combined)
            );
        }
    }
}

fn find_tlc() -> Result<PathBuf> {
    for candidate in ["tlc", "tlc2"] {
        if let Ok(path) = which(candidate) {
            return Ok(path);
        }
    }
    bail!(
        "TLC not found on PATH (tried `tlc`, `tlc2`)\n\
         Install the TLA+ tools and ensure `tlc` is available, or use \
         `rdra-ish export --kind tla` to generate the spec without running TLC."
    );
}

fn which(cmd: &str) -> Result<PathBuf> {
    let output = Command::new("which")
        .arg(cmd)
        .output()
        .with_context(|| format!("failed to resolve `{cmd}` on PATH"))?;
    if !output.status.success() {
        bail!("`{cmd}` not found");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("`{cmd}` not found");
    }
    Ok(PathBuf::from(path))
}
