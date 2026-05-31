use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rdra_ish_core::{
    build_merged_model, derive_actor_permission_audit, derive_permission_callables, resolve,
};
use rdra_ish_emit::{
    csv::{
        ActorListCsvEmitter, ActorPermissionAuditCsvEmitter, ApiEntityMatrixCsvEmitter,
        ApiListCsvEmitter, EntityListCsvEmitter, PermissionCallableCsvEmitter,
        RelationMatrixCsvEmitter, ScreenConstraintCsvEmitter,
    },
    mermaid::{
        ErMermaidEmitter, EventFlowMermaidEmitter, ObjectGraphMermaidEmitter, RdraMermaidEmitter,
        SequenceMermaidEmitter, StateMermaidEmitter,
    },
    plantuml::{
        ErPlantUmlEmitter, EventFlowPlantUmlEmitter, ObjectGraphPlantUmlEmitter,
        RdraPlantUmlEmitter, SequenceDiagramEmitter, StateDiagramEmitter,
    },
    state_pattern::{StatePatternCsvEmitter, StatePatternJsonEmitter, StatePatternTableEmitter},
    Emitter, Filter, Scope, View,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(ValueEnum, Clone)]
enum ListKind {
    Actor,
    Entity,
    Buc,
    Usecase,
    System,
    Api,
    /// Permission × callable UC/API list
    PermissionCallables,
    /// Actor × permission assignment audit inferred from UC/API requirements
    ActorPermissionAudit,
}

#[derive(ValueEnum, Clone)]
enum ListFormat {
    Table,
    Json,
    Csv,
}

#[derive(Parser)]
#[command(name = "rdra-ish", about = "RDRA DSL compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and type-check only (no output)
    Check {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
    },
    /// Generate diagram (PlantUML or Mermaid)
    Diagram {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Diagram kind: rdra, boundaryless-graph, er, state, sequence, or event-flow
        #[arg(long, default_value = "rdra")]
        kind: DiagramKind,
        /// Output format: puml, svg, png, or mermaid (mermaid outputs .mmd text only)
        #[arg(long, default_value = "puml")]
        format: OutputFormat,
        /// Filter to one or more BUCs (by id); repeatable (e.g. --buc A --buc B).
        /// The union of scoped nodes across all specified BUCs is shown.
        /// Applies to all diagram kinds. For sequence, only directly contained use cases are shown.
        #[arg(long)]
        buc: Vec<String>,
        /// Filter sequence diagrams to one or more use cases (by id); repeatable.
        #[arg(long)]
        usecase: Vec<String>,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// Generate CSV
    Csv {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// CSV kind: actor, entity, matrix, api, api-matrix, screen-constraints, permission-callables, or actor-permission-audit
        #[arg(long, default_value = "entity")]
        kind: CsvKind,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// List elements in human-readable form
    List {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Element kind to list: actor, entity, buc, usecase, system, api, permission-callables, or actor-permission-audit
        #[arg(long, default_value = "actor")]
        kind: ListKind,
        /// Output format: table, json, csv
        #[arg(long, default_value = "table")]
        format: ListFormat,
    },
    /// Derive reachable state patterns per entity (aggregated across BUCs)
    States {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output format: table, csv, json
        #[arg(long, default_value = "table")]
        format: ListFormat,
        /// Filter to one or more BUCs (by id); repeatable. Union of reachable nodes.
        #[arg(long)]
        buc: Vec<String>,
        /// Cap per-entity pattern count before truncation
        #[arg(long, default_value_t = 256)]
        max_patterns: usize,
        /// Restrict output to a single entity id
        #[arg(long)]
        entity: Option<String>,
    },
}

#[derive(ValueEnum, Clone)]
enum DiagramKind {
    /// RDRA layered graph mapped onto the original RDRA-style structure
    Rdra,
    /// Boundaryless relationship graph kept for dense link inspection
    BoundarylessGraph,
    Er,
    State,
    /// Write-focused sequence diagram with FK-inferred transaction boundaries
    Sequence,
    /// Event-flow diagram: UC --raises--> Event --triggers--> UC / --transitions--> State
    EventFlow,
}

#[derive(ValueEnum, Clone, PartialEq)]
enum OutputFormat {
    Puml,
    Svg,
    Png,
    Mermaid,
}

#[derive(ValueEnum, Clone)]
enum CsvKind {
    Actor,
    Entity,
    Matrix,
    /// API リスト (id, label)
    Api,
    /// API × Entity CRUD マトリクス
    ApiMatrix,
    /// Screen × UC/API permission/medium constraints
    ScreenConstraints,
    /// Permission × callable UC/API list
    PermissionCallables,
    /// Actor × permission assignment audit inferred from UC/API requirements
    ActorPermissionAudit,
}

/// Collect all `.rdra` files from the given paths (files and/or directories).
fn collect_rdra_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = vec![];
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
        } else if input.is_dir() {
            for entry in walkdir::WalkDir::new(input).into_iter().flatten() {
                if entry.path().extension().is_some_and(|e| e == "rdra") {
                    files.push(entry.path().to_owned());
                }
            }
        }
    }
    files
}

/// Compute the set of include paths by going up one level from any directories
/// that contain the entry files.  This ensures `shared/actors.rdra` resolves
/// from a root that contains both `shared/` and `buc/`.
fn root_include_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut paths = vec![];
    for f in files {
        // Canonicalize so relative paths work.
        let canon = fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        // For a file like /root/buc/buc_purchase.rdra we want /root as include path.
        // Walk up from the file's parent until we can't go further.
        if let Some(parent) = canon.parent() {
            // Try parent and grandparent.
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

fn load_model(
    inputs: &[PathBuf],
) -> Result<(rdra_ish_core::SemanticModel, Vec<rdra_ish_core::Diagnostic>)> {
    let entry_files = collect_rdra_files(inputs);
    if entry_files.is_empty() {
        anyhow::bail!("no .rdra files found in the given inputs");
    }

    let include_paths = root_include_paths(&entry_files);

    let (program, resolve_diags) = resolve(&entry_files, &include_paths);

    let (model, model_diags) = build_merged_model(&program, &include_paths);

    let mut all_diags = resolve_diags;
    all_diags.extend(model_diags);

    Ok((model, all_diags))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { inputs } => {
            let (model, diags) = load_model(&inputs)?;

            let mut has_error = false;
            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
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
            out,
        } => {
            let (model, diags) = load_model(&inputs)?;

            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
                }
            }

            if !usecase.is_empty() && !matches!(kind, DiagramKind::Sequence) {
                anyhow::bail!("--usecase is currently supported only for --kind sequence");
            }
            if !usecase.is_empty() && !buc.is_empty() {
                anyhow::bail!("--buc and --usecase cannot be combined");
            }

            let scope = if !usecase.is_empty() {
                Scope::UseCases(usecase)
            } else if buc.is_empty() {
                Scope::Whole
            } else {
                Scope::Bucs(buc)
            };

            // 図種に応じて filter を決定し、View を組み立てる
            let view = match &kind {
                DiagramKind::Er => View {
                    scope,
                    filter: Filter::Er,
                },
                DiagramKind::Rdra
                | DiagramKind::BoundarylessGraph
                | DiagramKind::State
                | DiagramKind::Sequence
                | DiagramKind::EventFlow => View {
                    scope,
                    filter: Filter::None,
                },
            };

            // TX診断 + API診断: sequence 図生成時に warning を表示
            if matches!(kind, DiagramKind::Sequence) {
                let txs = rdra_ish_core::infer_usecase_transactions(&model);
                for diag in rdra_ish_core::tx_diagnostics(&model, &txs) {
                    eprintln!("warning: {}", diag.error);
                }
                for diag in rdra_ish_core::api_diagnostics(&model) {
                    eprintln!("warning: {}", diag.error);
                }
                for diag in rdra_ish_core::system_diagnostics(&model) {
                    eprintln!("warning: {}", diag.error);
                }
            }

            // イベント整合性診断: event-flow 図生成時に warning を表示
            if matches!(kind, DiagramKind::EventFlow) {
                for diag in rdra_ish_core::event_diagnostics(&model) {
                    eprintln!("warning: {}", diag.error);
                }
            }

            // PlantUML/Mermaid どちらのエミッタを使うかを format で決定
            let diagram_text = match format {
                OutputFormat::Mermaid => match kind {
                    DiagramKind::Rdra => ObjectGraphMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::BoundarylessGraph => RdraMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::Er => ErMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::State => StateMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::Sequence => SequenceMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::EventFlow => EventFlowMermaidEmitter.emit(&model, &view)?,
                },
                _ => match kind {
                    DiagramKind::Rdra => ObjectGraphPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::BoundarylessGraph => RdraPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::Er => ErPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::State => StateDiagramEmitter.emit(&model, &view)?,
                    DiagramKind::Sequence => SequenceDiagramEmitter.emit(&model, &view)?,
                    DiagramKind::EventFlow => EventFlowPlantUmlEmitter.emit(&model, &view)?,
                },
            };

            match format {
                OutputFormat::Puml => {
                    let out_path = out.with_extension("puml");
                    fs::write(&out_path, &diagram_text)
                        .with_context(|| format!("failed to write {}", out_path.display()))?;
                    println!("wrote {}", out_path.display());
                }
                OutputFormat::Mermaid => {
                    let out_path = out.with_extension("mmd");
                    fs::write(&out_path, &diagram_text)
                        .with_context(|| format!("failed to write {}", out_path.display()))?;
                    println!("wrote {}", out_path.display());
                }
                OutputFormat::Svg => {
                    use rdra_ish_render::{render_to_file, PlantumlCliRenderer, RenderFormat};
                    let renderer =
                        PlantumlCliRenderer::discover().context("failed to find plantuml.jar")?;
                    let out_path = out.with_extension("svg");
                    render_to_file(&renderer, &diagram_text, &out_path, RenderFormat::Svg)
                        .context("plantuml rendering failed")?;
                    println!("wrote {}", out_path.display());
                }
                OutputFormat::Png => {
                    use rdra_ish_render::{render_to_file, PlantumlCliRenderer, RenderFormat};
                    let renderer =
                        PlantumlCliRenderer::discover().context("failed to find plantuml.jar")?;
                    let out_path = out.with_extension("png");
                    render_to_file(&renderer, &diagram_text, &out_path, RenderFormat::Png)
                        .context("plantuml rendering failed")?;
                    println!("wrote {}", out_path.display());
                }
            }
        }

        Commands::Csv { inputs, kind, out } => {
            let (model, diags) = load_model(&inputs)?;

            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
                }
            }

            let view = View::whole();

            let (csv_content, ext) = match kind {
                CsvKind::Actor => (ActorListCsvEmitter.emit(&model, &view)?, "actor.csv"),
                CsvKind::Entity => (EntityListCsvEmitter.emit(&model, &view)?, "entity.csv"),
                CsvKind::Matrix => (RelationMatrixCsvEmitter.emit(&model, &view)?, "matrix.csv"),
                CsvKind::Api => (ApiListCsvEmitter.emit(&model, &view)?, "api.csv"),
                CsvKind::ApiMatrix => (
                    ApiEntityMatrixCsvEmitter.emit(&model, &view)?,
                    "api-matrix.csv",
                ),
                CsvKind::ScreenConstraints => (
                    ScreenConstraintCsvEmitter.emit(&model, &view)?,
                    "screen-constraints.csv",
                ),
                CsvKind::PermissionCallables => (
                    PermissionCallableCsvEmitter.emit(&model, &view)?,
                    "permission-callables.csv",
                ),
                CsvKind::ActorPermissionAudit => (
                    ActorPermissionAuditCsvEmitter.emit(&model, &view)?,
                    "actor-permission-audit.csv",
                ),
            };

            let out_path = if out.extension().is_some() {
                out.clone()
            } else {
                out.with_extension(ext.trim_start_matches("*."))
            };

            fs::write(&out_path, &csv_content)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
        }

        Commands::List {
            inputs,
            kind,
            format,
        } => {
            let (model, diags) = load_model(&inputs)?;

            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
                }
            }

            let output = list_elements(&model, &kind, &format)?;
            print!("{}", output);
        }

        Commands::States {
            inputs,
            format,
            buc,
            max_patterns,
            entity,
        } => {
            let (model, diags) = load_model(&inputs)?;

            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
                }
            }

            let view = View::bucs(buc);

            let output = match format {
                ListFormat::Table => {
                    let emitter = StatePatternTableEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
                ListFormat::Csv => {
                    let emitter = StatePatternCsvEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
                ListFormat::Json => {
                    let emitter = StatePatternJsonEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
            };

            // --entity フィルタ: 特定 entity のみ出力
            if let Some(ref entity_id) = entity {
                let filtered = filter_entity_output(&output, entity_id, &format);
                print!("{}", filtered);
            } else {
                print!("{}", output);
            }
        }
    }

    Ok(())
}

fn consistency_warnings(model: &rdra_ish_core::SemanticModel) -> Vec<String> {
    let mut warnings = Vec::new();

    for diag in rdra_ish_core::permission_diagnostics(model) {
        warnings.push(diag.error.to_string());
    }

    for diag in rdra_ish_core::api_diagnostics(model) {
        warnings.push(diag.error.to_string());
    }

    for diag in rdra_ish_core::system_diagnostics(model) {
        warnings.push(diag.error.to_string());
    }

    let txs = rdra_ish_core::infer_usecase_transactions(model);
    for diag in rdra_ish_core::tx_diagnostics(model, &txs) {
        warnings.push(diag.error.to_string());
    }

    for diag in rdra_ish_core::event_diagnostics(model) {
        warnings.push(diag.error.to_string());
    }

    for result in
        rdra_ish_core::derive_state_patterns(model, &[], rdra_ish_core::DEFAULT_PATTERN_CAP)
    {
        for diag in result.diagnostics {
            warnings.push(format!(
                "state derivation for entity '{}': {}",
                result.entity_id,
                state_diag_message(&diag)
            ));
        }
    }

    warnings
}

fn state_diag_message(diag: &rdra_ish_core::StateDiag) -> String {
    match diag {
        rdra_ish_core::StateDiag::UnreachableEnumVariant { column, variant } => format!(
            "enum variant '{}.{}' is unreachable; add a create/default/transition/sets path or remove the variant",
            column, variant
        ),
        rdra_ish_core::StateDiag::ConflictingEffects { usecase, column } => format!(
            "usecase '{}' assigns conflicting effects to column '{}'; the last effect wins",
            usecase, column
        ),
        rdra_ish_core::StateDiag::DoubleModeledEnum { column } => format!(
            "enum column '{}' is driven by both transitions and sets; transitions are treated as the source of truth",
            column
        ),
        rdra_ish_core::StateDiag::NoCreationPath => {
            "no creates path; state derivation is seeded from defaults only".to_string()
        }
        rdra_ish_core::StateDiag::PatternCapReached { cap, bound } => format!(
            "pattern cap reached at {} while the theoretical state-space bound is {}",
            cap, bound
        ),
        rdra_ish_core::StateDiag::ForbiddenStateViolated {
            conditions,
            pattern_desc,
        } => format!(
            "forbidden state is reachable: {} witnessed by {}",
            conditions, pattern_desc
        ),
        rdra_ish_core::StateDiag::InvariantViolated {
            guards,
            requireds,
            pattern_desc,
        } => format!(
            "invariant violated: when {} then {} is broken by {}",
            guards, requireds, pattern_desc
        ),
    }
}

/// Build a table row separator line.
fn table_separator(col_widths: &[usize]) -> String {
    col_widths
        .iter()
        .map(|&w| "\u{2500}".repeat(w))
        .collect::<Vec<_>>()
        .join("  ")
}

fn list_elements(
    model: &rdra_ish_core::SemanticModel,
    kind: &ListKind,
    format: &ListFormat,
) -> Result<String> {
    match kind {
        ListKind::Actor => {
            let mut items: Vec<(&str, &str)> = model
                .actors
                .iter()
                .map(|(_, a)| (a.id.as_str(), a.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "actors")
        }
        ListKind::Buc => {
            let mut items: Vec<(&str, &str)> = model
                .bucs
                .iter()
                .map(|(_, b)| (b.id.as_str(), b.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "BUCs")
        }
        ListKind::Usecase => {
            let mut items: Vec<(&str, &str)> = model
                .use_cases
                .iter()
                .map(|(_, u)| (u.id.as_str(), u.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "use cases")
        }
        ListKind::Entity => format_entities(model, format),
        ListKind::System => {
            let mut items: Vec<(&str, &str)> = model
                .systems
                .iter()
                .map(|(_, s)| (s.id.as_str(), s.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "systems")
        }
        ListKind::Api => {
            let mut items: Vec<(&str, &str)> = model
                .apis
                .iter()
                .map(|(_, a)| (a.id.as_str(), a.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "APIs")
        }
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
    }
}

fn format_permission_callables(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "permission_id",
        "permission_label",
        "usecase_ids",
        "api_ids",
    ];

    let rows: Vec<[String; 4]> = derive_permission_callables(model)
        .into_iter()
        .map(|entry| {
            let permission = &model.permissions[entry.permission];
            let usecase_ids = entry
                .usecases
                .iter()
                .map(|key| model.use_cases[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let api_ids = entry
                .apis
                .iter()
                .map(|key| model.apis[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            [
                permission.id.clone(),
                permission.label.clone(),
                usecase_ids,
                api_ids,
            ]
        })
        .collect();

    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(String::from("No permissions found.\n"));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in &rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    format!(
                        "{{\"permission_id\":{},\"permission_label\":{},\"usecase_ids\":{},\"api_ids\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn format_actor_permission_audit(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "actor_id",
        "actor_label",
        "permission_id",
        "permission_label",
        "assigned",
        "required",
        "status",
        "required_usecase_ids",
        "required_api_paths",
    ];

    let rows: Vec<[String; 9]> = derive_actor_permission_audit(model)
        .into_iter()
        .map(|entry| {
            let actor = &model.actors[entry.actor];
            let permission = &model.permissions[entry.permission];
            [
                actor.id.clone(),
                actor.label.clone(),
                permission.id.clone(),
                permission.label.clone(),
                bool_cell(entry.assigned),
                bool_cell(entry.required),
                entry.status.as_str().to_string(),
                required_usecase_ids(model, &entry.sources),
                required_api_paths(model, &entry.sources),
            ]
        })
        .collect();

    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(String::from("No actor permission audit rows found.\n"));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in &rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    format!(
                        "{{\"actor_id\":{},\"actor_label\":{},\"permission_id\":{},\"permission_label\":{},\"assigned\":{},\"required\":{},\"status\":{},\"required_usecase_ids\":{},\"required_api_paths\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                        row[4],
                        row[5],
                        serde_json::to_string(&row[6]).unwrap(),
                        serde_json::to_string(&row[7]).unwrap(),
                        serde_json::to_string(&row[8]).unwrap(),
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn required_usecase_ids(
    model: &rdra_ish_core::SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut ids: Vec<&str> = sources
        .iter()
        .filter(|source| source.api.is_none())
        .map(|source| model.use_cases[source.usecase].id.as_str())
        .collect();
    ids.sort();
    ids.dedup();
    ids.join("|")
}

fn required_api_paths(
    model: &rdra_ish_core::SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut paths: Vec<String> = sources
        .iter()
        .filter_map(|source| {
            source.api.map(|api| {
                format!(
                    "{}->{}",
                    model.use_cases[source.usecase].id, model.apis[api].id
                )
            })
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.join("|")
}

fn bool_cell(value: bool) -> String {
    (if value { "true" } else { "false" }).to_string()
}

fn format_id_label(
    items: &[(&str, &str)],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if items.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let id_w = items
                .iter()
                .map(|(id, _)| id.len())
                .max()
                .unwrap_or(2)
                .max(2);
            let label_w = items
                .iter()
                .map(|(_, l)| l.chars().count())
                .max()
                .unwrap_or(5)
                .max(5);
            let header_id = format!("{:<width$}", "ID", width = id_w);
            let header_label = format!("{:<width$}", "LABEL", width = label_w);
            let sep_id = table_separator(&[id_w]);
            let sep_label = table_separator(&[label_w]);
            let mut out = format!(
                "{}  {}\n{}  {}\n",
                header_id, header_label, sep_id, sep_label
            );
            for (id, label) in items {
                out.push_str(&format!("{:<width$}  {}\n", id, label, width = id_w));
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = String::from("id,label\n");
            for (id, label) in items {
                // Simple CSV: quote if contains comma or quote
                let escaped_id = csv_field(id);
                let escaped_label = csv_field(label);
                out.push_str(&format!("{},{}\n", escaped_id, escaped_label));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = items
                .iter()
                .map(|(id, label)| {
                    format!(
                        "{{\"id\":{},\"label\":{}}}",
                        serde_json::to_string(id).unwrap(),
                        serde_json::to_string(label).unwrap()
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn format_entities(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    use rdra_ish_core::model::ColumnType;

    let mut ents: Vec<_> = model.entities.iter().collect();
    ents.sort_by_key(|(_, e)| e.id.as_str());

    fn col_type_s(ct: &ColumnType) -> &'static str {
        match ct {
            ColumnType::Int => "Int",
            ColumnType::String => "String",
            ColumnType::Money => "Money",
            ColumnType::DateTime => "DateTime",
            ColumnType::Date => "Date",
            ColumnType::Bool => "Bool",
            ColumnType::Decimal => "Decimal",
            ColumnType::Enum(_) => "Enum",
        }
    }

    // Build flat rows: (entity_id, entity_label, col_name, col_type, is_pk, is_fk)
    let mut rows: Vec<[String; 6]> = Vec::new();
    for (_, ent) in &ents {
        for col in &ent.columns {
            rows.push([
                ent.id.clone(),
                ent.label.clone(),
                col.name.clone(),
                col_type_s(&col.col_type).to_string(),
                if col.is_pk { "true" } else { "false" }.to_string(),
                if col.is_fk { "true" } else { "false" }.to_string(),
            ]);
        }
    }

    let headers = [
        "entity_id",
        "entity_label",
        "column_name",
        "column_type",
        "is_pk",
        "is_fk",
    ];

    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(String::from("No entities found.\n"));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            // header
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            // separator
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in &rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    format!(
                        "{{\"entity_id\":{},\"entity_label\":{},\"column_name\":{},\"column_type\":{},\"is_pk\":{},\"is_fk\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                        serde_json::to_string(&row[4]).unwrap(),
                        serde_json::to_string(&row[5]).unwrap(),
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

/// `--entity` フィルタ: 指定 entity_id の出力行のみを残す。
/// table 形式はブロック単位で、csv/json はフィールドでフィルタする。
fn filter_entity_output(output: &str, entity_id: &str, format: &ListFormat) -> String {
    match format {
        ListFormat::Table => {
            // "Entity: <id>" で始まるブロックを切り出す
            let prefix = format!("Entity: {} ", entity_id);
            let mut in_block = false;
            let mut block = String::new();
            for line in output.lines() {
                if line.starts_with("Entity: ") {
                    if in_block {
                        break; // 次のエンティティが来たら終了
                    }
                    if line.starts_with(&prefix) {
                        in_block = true;
                    }
                }
                if in_block {
                    block.push_str(line);
                    block.push('\n');
                }
            }
            block
        }
        ListFormat::Csv => {
            // entity_id カラム（第1列）でフィルタ
            let mut filtered = String::new();
            for (i, line) in output.lines().enumerate() {
                if i == 0 {
                    filtered.push_str(line);
                    filtered.push('\n');
                    continue;
                }
                if line
                    .split(',')
                    .next()
                    .is_some_and(|id| id.trim_matches('"') == entity_id)
                {
                    filtered.push_str(line);
                    filtered.push('\n');
                }
            }
            filtered
        }
        ListFormat::Json => {
            // JSON 配列から entity_id が一致するオブジェクトのみ残す
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
                let filtered: Vec<_> = arr
                    .into_iter()
                    .filter(|v| {
                        v.get("entity_id")
                            .and_then(|id| id.as_str())
                            .is_some_and(|id| id == entity_id)
                    })
                    .collect();
                serde_json::to_string_pretty(&filtered).unwrap_or_default() + "\n"
            } else {
                output.to_string()
            }
        }
    }
}

/// Minimal CSV field escaping.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::SemanticModel;

    #[test]
    fn table_list_reports_empty_api_result() {
        let model = SemanticModel::default();

        let output = list_elements(&model, &ListKind::Api, &ListFormat::Table).unwrap();

        assert_eq!(output, "No APIs found.\n");
    }

    #[test]
    fn structured_empty_lists_stay_machine_readable() {
        let model = SemanticModel::default();

        let csv = list_elements(&model, &ListKind::Api, &ListFormat::Csv).unwrap();
        let json = list_elements(&model, &ListKind::Api, &ListFormat::Json).unwrap();

        assert_eq!(csv, "id,label\n");
        assert_eq!(json, "[]\n");
    }

    #[test]
    fn table_list_reports_empty_entity_result() {
        let model = SemanticModel::default();

        let output = list_elements(&model, &ListKind::Entity, &ListFormat::Table).unwrap();

        assert_eq!(output, "No entities found.\n");
    }

    #[test]
    fn table_list_permission_callables() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
requires_permission(BookAppointment, ScheduleWrite)
requires_permission(BookingApi, ScheduleWrite)
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);

        let output =
            list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Table).unwrap();

        assert!(output.contains("PERMISSION_ID"));
        assert!(output.contains("ScheduleWrite"));
        assert!(output.contains("BookAppointment"));
        assert!(output.contains("BookingApi"));
    }

    #[test]
    fn table_list_reports_empty_permission_callables() {
        let model = SemanticModel::default();

        let output =
            list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Table).unwrap();

        assert_eq!(output, "No permissions found.\n");
    }

    #[test]
    fn table_list_actor_permission_audit() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission LegacyAdmin "Legacy Admin"
performs(Staff, BookAppointment)
has_permission(Staff, LegacyAdmin)
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);

        let output =
            list_elements(&model, &ListKind::ActorPermissionAudit, &ListFormat::Table).unwrap();

        assert!(output.contains("ACTOR_ID"));
        assert!(output.contains("LegacyAdmin"));
        assert!(output.contains("excess"));
        assert!(output.contains("ScheduleWrite"));
        assert!(output.contains("missing"));
        assert!(output.contains("BookAppointment->BookingApi"));
    }

    #[test]
    fn consistency_warnings_include_permission_and_state_findings() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(draft, booked) @default(draft)
}
performs(Staff, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        assert!(diags.iter().all(|diag| diag.is_warning));

        let warnings = consistency_warnings(&model);

        assert!(
            warnings
                .iter()
                .any(|warning| warning
                    .contains("actor 'Staff' is missing permission 'ScheduleWrite'"))
        );
        assert!(warnings.iter().any(|warning| warning
            .contains("state derivation for entity 'Appointment': no creates path")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("enum variant 'status.booked' is unreachable")));
    }

    #[test]
    fn state_diag_message_formats_invariant_violation() {
        let message = state_diag_message(&rdra_ish_core::StateDiag::InvariantViolated {
            guards: "status=booked".to_string(),
            requireds: "booked_at=present".to_string(),
            pattern_desc: "status=booked, booked_at=null".to_string(),
        });

        assert_eq!(
            message,
            "invariant violated: when status=booked then booked_at=present is broken by status=booked, booked_at=null"
        );
    }
}
