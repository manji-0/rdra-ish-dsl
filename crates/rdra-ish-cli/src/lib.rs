use anyhow::Result;
use clap::Parser;
mod cli;
mod codegen_cmd;
mod csv_cmd;
mod diagram;
mod export;
mod fmt_cmd;
mod guard_report;
mod lifecycle_gate;
mod list_output;
mod load;
mod memory;
mod next;
pub mod property_cone;
mod prove;
mod session;
mod state_findings;
mod states;
pub mod tlc_batch;
pub mod tlc_env;
pub mod tlc_output;
pub mod tlc_replay;
mod verify;
use cli::{BoundsArgs, Cli, Commands, ListFormat, ListKind};
use csv_cmd::run_csv;
use diagram::run_diagram;
use export::run_export;
use fmt_cmd::run_fmt;
use list_output::format_lint_issues;
use load::{deny_deprecated, eprint_diagnostic, load_clean};
use memory::{charged, memory_limit, Budgeted};
use rdra_ish_core::lifecycle::{FindingTally, Severity};
use rdra_ish_core::{collect_rdra_files, load_project, Project};
use rdra_ish_core::{lint_issues, LintSeverity, RdraError};
#[doc(hidden)]
pub use session::Session;
use state_findings::state_findings;
use states::run_states;

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

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            inputs,
            strict,
            deny_deprecated: deny,
            max_patterns,
            max_states,
            memory,
            guards,
            bounds,
            scope,
        } => {
            let Project {
                program,
                model,
                mut diags,
                manifest,
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
            let bounds = bounds.bounds(&model, manifest.as_ref())?;
            let caps = charged(
                BoundsArgs::caps(&bounds, max_patterns, max_states),
                memory,
                manifest.as_ref(),
                Budgeted::Check,
            );
            let lifecycle = state_findings(&program, &model, &scope, bounds.clone(), caps);
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
                    "note: {property_count} temporal properties were not model-checked by check; use verify"
                );
            }

            if guards {
                if tally.errors > 0 {
                    eprintln!("note: --guards reports nothing on a model that fails check; fix the errors first");
                } else {
                    print!(
                        "{}",
                        guard_report::run(&program, &model, &scope, bounds, caps)
                    );
                }
            }

            if tally.errors > 0 {
                println!("{}", lifecycle.bounds.fail(tally.errors, tally.warnings));
                std::process::exit(1);
            }

            println!("{}", lifecycle.bounds.ok());
        }

        Commands::Diagram(args) => run_diagram(args)?,

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
            print!("{}", Session::load(&inputs)?.list_text(&kind, &format)?);
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
            conformance,
            conformance_states,
            check,
        } => codegen_cmd::run_codegen(
            &inputs,
            lang,
            out.as_deref(),
            &package,
            allow_failing,
            conformance
                .as_deref()
                .map(|path| (path, conformance_states)),
            check.as_deref(),
        )?,

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
            instances,
            int_range_max,
            dialect,
        } => run_export(
            &inputs,
            &kind,
            &out,
            (instance_count, &instances, int_range_max),
            dialect,
        )?,

        Commands::States {
            inputs,
            format,
            max_patterns,
            max_states,
            memory,
            bounds,
            scope,
        } => run_states(
            &inputs,
            format,
            &bounds,
            (max_patterns, max_states, memory),
            &scope,
        )?,

        Commands::Verify {
            inputs,
            with_tlc,
            backend,
            out,
            instance_count,
            instances,
            int_range_max,
            max_patterns,
            max_states,
            temporal_budget,
            deny_undecided,
            prove,
            deny_unproved,
            z3,
            tlc_jar,
            memory,
            symmetry,
            scope,
        } => {
            // `--backend tlc` is the old spelling of `--with-tlc`, kept until the next major
            // release.
            if let Some(cli::VerifyBackend::Tlc) = backend {
                eprintln!("warning: --backend tlc is deprecated: it now means --with-tlc, which runs TLC beside the native checker, and the next major release removes it");
            }
            let with_tlc = with_tlc || backend.is_some();
            if tlc_jar.is_some() && !with_tlc {
                eprintln!(
                    "warning: --tlc-jar applies only with --with-tlc; this run starts no TLC"
                );
            }
            let (program, model, manifest) = load_clean(&inputs)?;
            let memory = memory_limit(memory, manifest.as_ref(), Budgeted::Verify);
            if manifest
                .as_ref()
                .is_some_and(|m| m.check.symmetry.is_some())
            {
                eprintln!("warning: `symmetry` under [check] in rdra.toml is not read: check and states explore every state; verify reads it under [verify]");
            }
            let symmetry = symmetry
                .or(manifest.as_ref().and_then(|m| m.verify.symmetry))
                .unwrap_or_default();
            let memory = memory::VerifySplit::of(memory, with_tlc)?;
            let scope = scope.resolve(&model)?;
            let bounds = export::verify_bounds(
                (instance_count, &instances, int_range_max),
                manifest.as_ref(),
                &model,
            )?;
            verify::run(
                &program,
                &model,
                out,
                bounds,
                &scope,
                verify::Budget {
                    temporal: std::time::Duration::from_secs(temporal_budget),
                    max_patterns,
                    max_states,
                    deny_undecided,
                    prove: prove.then_some(verify::Prove { z3, deny_unproved }),
                    memory,
                    symmetry,
                },
                with_tlc.then_some(verify::WithTlc { jar: tlc_jar }),
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
pub mod cli_tests;
