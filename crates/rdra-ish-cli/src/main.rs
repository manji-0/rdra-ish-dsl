use anyhow::Result;
use clap::Parser;
mod cli;
mod codegen_cmd;
mod csv_cmd;
mod diagram;
mod export;
mod fmt_cmd;
mod lifecycle_gate;
mod list_output;
mod load;
mod next;
mod property_cone;
mod prove;
mod states;
mod tlc_env;
mod tlc_output;
mod tlc_replay;
mod verify;
use cli::{Cli, Commands, ListFormat, ListKind};
use csv_cmd::run_csv;
use diagram::{run_diagram, DiagramRequest};
use export::run_export;
use fmt_cmd::run_fmt;
use lifecycle_gate::gate;
use list_output::{
    format_lint_issues, format_rules, format_transitions, list_elements, state_findings,
};
use load::{deny_deprecated, eprint_diagnostic, load_clean};
use rdra_ish_core::lifecycle::{Caps, FindingTally, Severity};
use rdra_ish_core::{collect_rdra_files, load_project, Project};
use rdra_ish_core::{lint_issues, LintSeverity, RdraError};
use states::run_states;
use verify::run_tlc;

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
            deny_deprecated: deny,
            max_patterns,
            max_states,
            bounds,
            scope,
        } => {
            let Project {
                program,
                model,
                mut diags,
                ..
            } = load_project(&inputs, None)?;
            if deny {
                deny_deprecated(&mut diags);
            }

            let mut tally = FindingTally::default();
            for diag in &diags {
                eprint_diagnostic(&program, diag);
                tally.count(if diag.is_warning {
                    Severity::Warning
                } else {
                    Severity::Error
                });
            }

            if tally.errors > 0 {
                // The model did not load, so no state was explored and no bound applies.
                println!("FAIL: {tally}; the model did not load, so nothing was explored");
                std::process::exit(1);
            }

            let scope = scope.resolve(&model)?;
            // Same severity as `states` (fail-closed on caps and violated constraints).
            let caps = Caps::new(max_patterns, max_states);
            let lifecycle = state_findings(&program, &model, &scope, bounds.bounds()?, caps);
            eprintln!("{}", lifecycle.bounds);
            for diag in rdra_ish_core::consistency_diagnostics(&model) {
                let as_error = strict && is_strict_access_gap(&diag.error);
                if as_error {
                    eprintln!("error: {}", diag.error);
                } else {
                    eprintln!("warning: {}", diag.error);
                }
                tally.count(if as_error {
                    Severity::Error
                } else {
                    Severity::Warning
                });
            }

            for finding in lifecycle.findings {
                tally.count(finding.severity);
                eprintln!("{finding}");
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

            if tally.errors > 0 {
                println!("{}", lifecycle.bounds.fail(tally.errors, tally.warnings));
                std::process::exit(1);
            }

            println!("{}", lifecycle.bounds.ok());
        }

        Commands::Diagram {
            inputs,
            kind,
            all,
            format: _,
            buc,
            usecase,
            entity,
            actor,
            node_kind,
            edge_kind,
            title,
            legend,
            out,
        } => run_diagram(DiagramRequest {
            inputs,
            kind,
            all,
            buc,
            usecase,
            entity,
            actor,
            node_kind,
            edge_kind,
            title,
            legend,
            out,
        })?,

        Commands::Csv {
            inputs,
            kind,
            format,
            out,
        } => run_csv(&inputs, kind, format, out)?,

        Commands::List {
            inputs,
            kind,
            format,
        } => {
            if matches!(kind, ListKind::Surface) && !matches!(format, ListFormat::Table) {
                use clap::CommandFactory;
                Cli::command()
                    .error(
                        clap::error::ErrorKind::ArgumentConflict,
                        "`list --kind surface` prints canonical text only; drop `--format`",
                    )
                    .exit();
            }
            let (program, model) = load_clean(&inputs)?;
            gate(&program, &model, kind.reads())?;

            let output = match kind {
                ListKind::Rule => format_rules(&model, Some(&program), &format)?,
                ListKind::Transition => format_transitions(&model, Some(&program), &format)?,
                _ => list_elements(&model, &kind, &format)?,
            };
            print!("{}", output);
        }

        Commands::Lint { inputs, format } => {
            let Project { model, diags, .. } = load_project(&inputs, None)?;

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

        Commands::Next {
            inputs,
            buc,
            format,
        } => next::run_next(&inputs, &buc, format)?,

        Commands::Codegen {
            inputs,
            lang,
            out,
            package,
            allow_failing,
        } => codegen_cmd::run_codegen(&inputs, lang, out.as_deref(), &package, allow_failing)?,

        Commands::Files { inputs } => {
            let files = collect_rdra_files(&inputs, None)?;
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
            dialect,
        } => run_export(&inputs, &kind, &out, instance_count, int_range_max, dialect)?,

        Commands::States {
            inputs,
            format,
            max_patterns,
            max_states,
            bounds,
            scope,
        } => run_states(
            &inputs,
            format,
            bounds.bounds()?,
            Caps::new(max_patterns, max_states),
            &scope,
        )?,

        Commands::Verify {
            inputs,
            backend,
            out,
            instance_count,
            int_range_max,
            temporal_budget,
            deny_undecided,
            prove,
            deny_unproved,
            z3,
            tlc_jar,
            scope,
        } => {
            let (program, model) = load_clean(&inputs)?;
            let scope = scope.resolve(&model)?;
            let cli::VerifyBackend::Tlc = backend;
            run_tlc(
                &program,
                &model,
                out,
                (instance_count, int_range_max),
                &scope,
                verify::Budget {
                    temporal: std::time::Duration::from_secs(temporal_budget),
                    deny_undecided,
                    prove: prove.then_some(verify::Prove { z3, deny_unproved }),
                },
                tlc_jar,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests;
