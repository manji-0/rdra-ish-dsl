use anyhow::Result;
use clap::Parser;
mod cli;
mod code_check;
mod codegen_cmd;
mod csv_cmd;
mod diagram;
mod export;
mod fmt_cmd;
mod generated_file;
mod guard_report;
mod lifecycle_gate;
mod list_output;
mod load;
mod memory;
mod next;
mod paths;
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
use cli::{Cli, Commands, ListFormat, ListKind};
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
            caps,
            guards,
            code,
            deny_incomplete,
            mutants,
            mutant_seed,
            bounds,
            scope,
        } => {
            for (given, flag) in [
                (deny_incomplete, "--deny-incomplete"),
                (mutants.is_some(), "--mutants"),
                (mutant_seed.is_some(), "--mutant-seed"),
            ] {
                if given && code != Some(cli::CodeLevel::Run) {
                    anyhow::bail!(
                        "{flag} needs --code=run: only a conformance run uses the traces"
                    );
                }
            }
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
            match (code, &manifest) {
                (Some(_), m) => {
                    codegen_cmd::code_manifest(m.as_ref())?;
                }
                (None, Some(m)) if m.code.as_ref().is_some_and(|c| !c.targets().is_empty()) => {
                    eprintln!(
                        "note: {} has [code]; `check --code` also checks the service code against the model",
                        paths::shown(&m.path)
                    );
                }
                _ => {}
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
                caps.caps(&bounds),
                caps.memory,
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

            for diag in rdra_ish_core::check_lint_diagnostics(&model) {
                if let rdra_ish_core::RdraError::LintFinding { rule, .. } = &diag.error {
                    let message = format!("[{rule}] {}", diag.error);
                    eprintln!(
                        "{}",
                        rdra_ish_core::format_diagnostic_message(
                            Some(&program),
                            true,
                            diag.location.as_ref(),
                            &message,
                        )
                    );
                    tally.count(Severity::Warning);
                }
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
                if code.is_some() {
                    eprintln!("note: --code checked nothing: the model fails `check`, so no code is generated from it");
                }
                println!("{}", lifecycle.bounds.fail(tally.errors, tally.warnings));
                std::process::exit(1);
            }

            println!("{}", lifecycle.bounds.ok());
            if let Some(level) = code {
                let manifest = manifest.as_ref().expect("checked above");
                let mutation = code_check::SelfCheck {
                    per_kind: mutants.unwrap_or(1),
                    seed: mutant_seed.unwrap_or(0),
                };
                if !code_check::run(&inputs, manifest, level, deny_incomplete, mutation)? {
                    std::process::exit(1);
                }
            }
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
            format,
            code,
            probe,
            out,
            package,
            allow_failing,
            conformance,
            conformance_states,
            seal,
            check,
            verus,
            adapter,
        } => {
            if seal && !matches!(lang, Some(cli::CodegenLang::Ts) | None) {
                use clap::CommandFactory;
                Cli::command()
                    .error(
                        clap::error::ErrorKind::ArgumentConflict,
                        "--seal is for --lang ts only",
                    )
                    .exit();
            }
            if verus.is_some()
                && (!matches!(lang, Some(cli::CodegenLang::Rust))
                    || format == cli::CodegenFormat::Json)
            {
                use clap::CommandFactory;
                Cli::command()
                    .error(
                        clap::error::ErrorKind::ArgumentConflict,
                        "--verus is for --lang rust only",
                    )
                    .exit();
            }
            codegen_cmd::run_codegen(codegen_cmd::CodegenArgs {
                inputs: &inputs,
                lang,
                json: format == cli::CodegenFormat::Json,
                out: out.as_deref(),
                package: &package,
                allow_failing,
                conformance: conformance
                    .as_deref()
                    .map(|path| (path, conformance_states)),
                seal,
                check_file: check.as_deref(),
                code,
                probe,
                verus: verus.as_deref(),
                adapter,
            })?
        }

        Commands::Files { inputs } => {
            let files = collect_rdra_files(&inputs, None)?;
            if files.is_empty() {
                anyhow::bail!("no .rdra files found in the given inputs");
            }
            for file in files {
                println!("{}", file.display());
            }
        }

        Commands::Lsp => rdra_ish_lsp::run_stdio(),

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
            caps,
            bounds,
            scope,
        } => run_states(&inputs, format, &bounds, &caps, &scope)?,

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
            tlc_jobs,
            tlc_workers,
            plan,
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
            for (flag, given) in [("--tlc-jobs", tlc_jobs), ("--tlc-workers", tlc_workers)] {
                if given.is_some() && !with_tlc {
                    eprintln!(
                        "warning: {flag} applies only with --with-tlc; this run starts no TLC"
                    );
                }
            }
            if plan.is_some() && (!with_tlc || out.is_none()) {
                anyhow::bail!(
                    "--plan needs --with-tlc and -o <DIR>: the plan names the specs -o writes"
                );
            }
            let tlc_jobs = tlc_count(tlc_jobs, "RDRA_TLC_JOBS")?;
            let tlc_workers = tlc_count(tlc_workers, "RDRA_TLC_WORKERS")?;
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
                with_tlc.then_some(verify::WithTlc {
                    jar: tlc_jar,
                    jobs: tlc_jobs,
                    workers: tlc_workers,
                    plan,
                }),
            )?;
        }
    }

    Ok(())
}

/// `--tlc-jobs` or `--tlc-workers`, else the variable `env` names: a whole number above 0.
fn tlc_count(flag: Option<u32>, env: &str) -> anyhow::Result<Option<usize>> {
    if let Some(n) = flag {
        return Ok(Some(n as usize));
    }
    match std::env::var(env) {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) => match text.trim().parse::<usize>() {
            Ok(n) if n > 0 => Ok(Some(n)),
            _ => anyhow::bail!("{env} must be a whole number above 0, not `{text}`"),
        },
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
pub mod cli_tests;
