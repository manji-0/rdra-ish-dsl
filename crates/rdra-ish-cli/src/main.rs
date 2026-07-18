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
mod verify;
use cli::{Cli, Commands, ExportKind};
use csv_cmd::run_csv;
use diagram::{run_diagram, DiagramRequest};
use export::export_artifact;
use fmt_cmd::run_fmt;
use list_output::{consistency_warnings, format_lint_issues, list_elements};
use load::{eprint_diagnostic, load_model};
use rdra_ish_core::{lint_issues, LintSeverity};
use rdra_ish_emit::View;
use states::run_states;
use std::fs;
use std::path::Path;
use verify::run_verify;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { inputs } => {
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

            for warning in consistency_warnings(&model) {
                eprintln!("warning: {}", warning);
            }

            println!("OK: no errors");
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

            for diag in &diags {
                eprint_diagnostic(&program, diag);
            }

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

        Commands::Fmt {
            inputs,
            write,
            check,
        } => run_fmt(&inputs, write, check)?,

        Commands::Export { inputs, kind, out } => {
            let (program, model, diags) = load_model(&inputs)?;

            for diag in &diags {
                eprint_diagnostic(&program, diag);
            }

            let view = View::whole();
            if matches!(kind, ExportKind::Tla) {
                let bundle = export::export_tla_bundle(&model, &view)?;
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
            buc,
            max_patterns,
            entity,
        } => run_states(&inputs, format, buc, max_patterns, entity)?,

        Commands::Verify {
            inputs,
            backend,
            out,
        } => {
            let (program, model, diags) = load_model(&inputs)?;
            for diag in &diags {
                eprint_diagnostic(&program, diag);
                if !diag.is_warning {
                    std::process::exit(1);
                }
            }
            run_verify(&model, backend, out)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests;
