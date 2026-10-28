use anyhow::{Context, Result};
use clap::Parser;
mod cli;
mod csv_cmd;
mod diagram;
mod export;
mod fmt_cmd;
mod list_output;
mod load;
mod states;
mod tlc_env;
mod tlc_output;
mod tlc_replay;
mod verify;
use cli::{Cli, Commands, ExportKind};
use csv_cmd::run_csv;
use diagram::{run_diagram, DiagramRequest};
use export::export_artifact;
use fmt_cmd::run_fmt;
use list_output::{
    format_lint_issues, list_elements, note_state_errors, reject_state_errors, severity_word,
    state_findings,
};
use load::{collect_rdra_files, eprint_diagnostic, load_model, reject_model_errors};
use rdra_ish_core::lifecycle::{Scope, Severity};
use rdra_ish_core::{lint_issues, LintSeverity, RdraError};
use rdra_ish_emit::View;
use states::run_states;
use std::fs;
use std::path::Path;
use verify::run_verify;

fn is_strict_access_gap(error: &RdraError) -> bool {
    matches!(
        error,
        RdraError::ActorPermissionMissing { .. }
            | RdraError::ActorPermissionExcess { .. }
            | RdraError::UseCasePermissionNoActor { .. }
            | RdraError::ApiPermissionNoActor { .. }
            | RdraError::ApiPermissionNotHeld { .. }
            | RdraError::UseCasePermissionNotHeld { .. }
    )
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            inputs,
            strict,
            scope,
        } => {
            let (program, model, diags) = load_model(&inputs)?;

            let mut has_error = false;
            for diag in &diags {
                eprint_diagnostic(&program, diag);
                if !diag.is_warning {
                    has_error = true;
                }
            }

            if has_error {
                std::process::exit(1);
            }

            let scope = Scope::parse(&model, &scope.buc, &scope.entity)?;
            // Same severity as `states` (fail-closed on caps and violated constraints).
            let lifecycle = state_findings(&program, &model, &scope);
            eprintln!("{}", lifecycle.bounds);
            for diag in rdra_ish_core::consistency_diagnostics(&model) {
                let as_error = strict && is_strict_access_gap(&diag.error);
                if as_error {
                    eprintln!("error: {}", diag.error);
                    has_error = true;
                } else {
                    eprintln!("warning: {}", diag.error);
                }
            }

            for (severity, message) in lifecycle.findings {
                if severity == Severity::Error {
                    has_error = true;
                }
                eprintln!("{}: {}", severity_word(severity), message);
            }
            if let Some(block) = lifecycle.not_checked {
                eprintln!("{block}");
            }

            let property_count = model.temporal_properties.len();
            if property_count > 0 {
                eprintln!(
                    "note: {property_count} temporal properties were not model-checked by check; use verify --backend tlc"
                );
            }

            if has_error {
                std::process::exit(1);
            }

            println!("{}", lifecycle.bounds.ok());
        }

        Commands::Diagram {
            inputs,
            kind,
            format,
            buc,
            usecase,
            diff_base,
            show_description,
            node_kind,
            edge_kind,
            view_preset,
            out,
        } => run_diagram(DiagramRequest {
            inputs,
            kind,
            format,
            buc,
            usecase,
            diff_base,
            show_description,
            node_kind,
            edge_kind,
            view_preset,
            out,
        })?,

        Commands::Csv { inputs, kind, out } => run_csv(&inputs, kind, out)?,

        Commands::List {
            inputs,
            kind,
            format,
        } => {
            let (program, model, diags) = load_model(&inputs)?;
            reject_model_errors(&program, &diags)?;
            reject_state_errors(&program, &model)?;

            let output = list_elements(&model, &kind, &format)?;
            print!("{}", output);
        }

        Commands::Lint { inputs, format } => {
            let (_program, model, diags) = load_model(&inputs)?;

            let issues = lint_issues(&model, &diags);
            let has_error = issues
                .iter()
                .any(|issue| issue.severity == LintSeverity::Error);
            let output = format_lint_issues(&issues, &format)?;
            print!("{}", output);

            if has_error {
                std::process::exit(1);
            }
        }

        Commands::Files { inputs } => {
            let files = collect_rdra_files(&inputs)?;
            if files.is_empty() {
                anyhow::bail!("no .rdra files found in the given inputs");
            }
            for file in files {
                println!("{}", file.display());
            }
        }

        Commands::Fmt {
            inputs,
            write,
            check,
        } => run_fmt(&inputs, write, check)?,

        Commands::Export {
            inputs,
            kind,
            out,
            instance_count,
            int_range_max,
        } => {
            let (program, model, diags) = load_model(&inputs)?;
            reject_model_errors(&program, &diags)?;
            if kind.reads_lifecycle() {
                reject_state_errors(&program, &model)?;
            } else if !matches!(kind, ExportKind::Tla) {
                note_state_errors(&program, &model);
            }

            let view = View::whole();
            if matches!(kind, ExportKind::Tla) {
                let module_name = if out.extension().is_some_and(|e| e == "tla") {
                    out.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("RdraSpec")
                } else {
                    "RdraSpec"
                };
                let bounds = export::tla_bounds(instance_count, int_range_max)?;
                let bundle = export::tla_bundle(&model, bounds, module_name)?;
                let (tla_path, cfg_path) = if out.extension().is_some_and(|e| e == "tla") {
                    let cfg = out.with_extension("cfg");
                    (out.clone(), cfg)
                } else if out.extension().is_none()
                    || out.extension().is_some_and(|e| e != "tla" && e != "cfg")
                {
                    // Directory or stem without .tla → write RdraSpec.tla/.cfg under/out.
                    let dir = if out.exists() && out.is_dir() {
                        out.clone()
                    } else if out.extension().is_none() {
                        // Treat as directory path (create parent).
                        out.clone()
                    } else {
                        out.parent().unwrap_or(Path::new(".")).to_path_buf()
                    };
                    if !dir.exists() {
                        fs::create_dir_all(&dir)
                            .with_context(|| format!("failed to create {}", dir.display()))?;
                    }
                    (
                        dir.join(format!("{}.tla", bundle.module_name)),
                        dir.join(format!("{}.cfg", bundle.module_name)),
                    )
                } else {
                    (out.with_extension("tla"), out.with_extension("cfg"))
                };
                fs::write(&tla_path, &bundle.tla)
                    .with_context(|| format!("failed to write {}", tla_path.display()))?;
                fs::write(&cfg_path, &bundle.cfg)
                    .with_context(|| format!("failed to write {}", cfg_path.display()))?;
                println!("wrote {}", tla_path.display());
                println!("wrote {}", cfg_path.display());
            } else {
                let (content, ext) = export_artifact(&model, &kind, &view)?;

                let out_path = if out.extension().is_some() {
                    out.clone()
                } else {
                    out.with_extension(ext.trim_start_matches("*."))
                };

                fs::write(&out_path, &content)
                    .with_context(|| format!("failed to write {}", out_path.display()))?;
                println!("wrote {}", out_path.display());
            }
        }

        Commands::States {
            inputs,
            format,
            max_patterns,
            scope,
        } => run_states(&inputs, format, max_patterns, &scope)?,

        Commands::Verify {
            inputs,
            backend,
            out,
            instance_count,
            int_range_max,
            scope,
        } => {
            let (program, model, diags) = load_model(&inputs)?;
            reject_model_errors(&program, &diags)?;
            let scope = Scope::parse(&model, &scope.buc, &scope.entity)?;
            run_verify(
                &program,
                &model,
                backend,
                out,
                (instance_count, int_range_max),
                &scope,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests;
