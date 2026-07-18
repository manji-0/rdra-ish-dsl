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
use cli::{Cli, Commands};
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
