//! `rdra-ish verify --backend tlc` — export TLA+ and run TLC.

use anyhow::{bail, Context, Result};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::View;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::VerifyBackend;
use crate::export::{export_tla_bundle, export_tla_bundle_named, tla_obligation_errors};

pub fn run_verify(
    model: &SemanticModel,
    backend: VerifyBackend,
    out: Option<PathBuf>,
) -> Result<()> {
    match backend {
        VerifyBackend::Tlc => run_tlc(model, out),
    }
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

fn run_tlc(model: &SemanticModel, out: Option<PathBuf>) -> Result<()> {
    let module_hint = out
        .as_ref()
        .filter(|p| p.extension().is_some_and(|e| e == "tla"))
        .and_then(|p| p.file_stem()?.to_str().map(str::to_string));

    let bundle = if let Some(name) = module_hint.as_deref() {
        export_tla_bundle_named(model, &View::whole(), name)?
    } else {
        export_tla_bundle(model, &View::whole())?
    };

    for w in &bundle.warnings {
        eprintln!("warning: tla export: {w}");
    }
    if let Some(fatal) = tla_obligation_errors(&bundle.warnings) {
        bail!("{fatal}");
    }

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

    if !output.status.success() {
        let summary = summarize_tlc_failure(&combined);
        if !keep {
            let _ = fs::remove_dir_all(&work_dir);
        }
        bail!(
            "TLC verification failed (spec: {})\n{summary}",
            tla_path.display()
        );
    }

    if !keep {
        let _ = fs::remove_dir_all(&work_dir);
    }

    println!("TLC: OK");
    Ok(())
}

/// Extract a short RDRA-oriented summary from TLC stdout/stderr.
fn summarize_tlc_failure(output: &str) -> String {
    let mut lines = Vec::new();

    if let Some(inv) = find_line_containing(output, "Invariant") {
        lines.push(format!("invariant/property: {inv}"));
    } else if let Some(prop) = find_line_containing(output, "Property") {
        lines.push(format!("invariant/property: {prop}"));
    } else if let Some(err) = find_line_containing(output, "Error:") {
        lines.push(format!("error: {err}"));
    }

    let states: Vec<&str> = output
        .lines()
        .filter(|line| {
            let t = line.trim();
            t.starts_with("State ")
                || t.contains("/\\")
                || t.contains(" = ") && (t.contains("status") || t.contains("Order_"))
        })
        .take(12)
        .collect();

    if !states.is_empty() {
        lines.push("counterexample (first steps):".into());
        for s in states {
            lines.push(format!("  {}", s.trim()));
        }
    } else {
        let tail: Vec<&str> = output
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if !tail.is_empty() {
            lines.push("TLC output (tail):".into());
            for s in tail {
                lines.push(format!("  {s}"));
            }
        }
    }

    if lines.is_empty() {
        "no counterexample details captured; inspect the generated .tla/.cfg".into()
    } else {
        lines.join("\n")
    }
}

fn find_line_containing<'a>(output: &'a str, needle: &str) -> Option<&'a str> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.contains(needle))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_extracts_invariant_and_states() {
        let output = r#"
Error: Invariant Safety is violated.
The behavior up to this point is:
State 1: <Initial predicate>
/\ Order_status = "pending"
/\ Order_delivered_at = "null"
State 2: <Order_EvDeliver line 29>
/\ Order_status = "delivered"
/\ Order_delivered_at = "null"
"#;
        let summary = summarize_tlc_failure(output);
        assert!(summary.contains("Safety"));
        assert!(summary.contains("counterexample"));
        assert!(summary.contains("Order_status"));
    }
}
