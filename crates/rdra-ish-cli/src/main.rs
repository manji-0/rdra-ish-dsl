use anyhow::{Context, Result};
use clap::Parser;
mod cli;
use cli::{
    Cli, Commands, CsvKind, DiagramKind, DiagramViewPreset, ExportKind, ListFormat, ListKind,
    OutputFormat,
};
use rdra_ish_core::model::{NodeRef, RelKind};
use rdra_ish_core::{
    build_merged_model, derive_actor_input_inferences, derive_actor_permission_audit,
    derive_permission_callables, resolve, ActorInputSource,
};
use rdra_ish_emit::{
    asyncapi::AsyncApiJsonEmitter,
    csv::{
        ActorListCsvEmitter, ActorPermissionAuditCsvEmitter, ApiEntityMatrixCsvEmitter,
        ApiListCsvEmitter, BusinessInputCsvEmitter, EntityListCsvEmitter,
        PermissionCallableCsvEmitter, RelationMatrixCsvEmitter, ScreenConstraintCsvEmitter,
    },
    dbml::DbmlEmitter,
    diff::{DiffMermaidEmitter, DiffPlantUmlEmitter},
    json_schema::JsonSchemaEmitter,
    mermaid::{
        BusinessAreaMermaidEmitter, ErMermaidEmitter, EventFlowMermaidEmitter,
        ObjectGraphMermaidEmitter, RdraMermaidEmitter, SequenceMermaidEmitter, StateMermaidEmitter,
        TechnicalAreaMermaidEmitter,
    },
    openapi::OpenApiJsonEmitter,
    plantuml::{
        BusinessAreaPlantUmlEmitter, ErPlantUmlEmitter, EventFlowPlantUmlEmitter,
        ObjectGraphPlantUmlEmitter, RdraPlantUmlEmitter, SequenceDiagramEmitter,
        StateDiagramEmitter, TechnicalAreaPlantUmlEmitter,
    },
    state_pattern::{StatePatternCsvEmitter, StatePatternJsonEmitter, StatePatternTableEmitter},
    Emitter, Filter, Scope, View,
};
use rdra_ish_syntax::{
    Annotation, Ast, ChainCall, CmpOp, ColType, Column, DottedName, Expr, ImportDecl, ImportKind,
    InstanceDecl, Item, Operand, PredicateArg, PredicateCall, QRef,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

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

fn diagram_preset_filters(preset: &Option<DiagramViewPreset>) -> (Vec<String>, Vec<String>) {
    let Some(preset) = preset else {
        return (Vec::new(), Vec::new());
    };
    let (nodes, edges): (&[&str], &[&str]) = match preset {
        DiagramViewPreset::Business => (
            &[
                "actor",
                "requirement",
                "nfr",
                "business",
                "buc",
                "flow",
                "step",
                "usecase",
                "event",
            ],
            &[
                "performs",
                "motivates",
                "belongs",
                "contains",
                "precedes",
                "branches",
                "excepts",
                "repeats",
                "covers",
                "raises",
                "triggers",
                "compensates",
            ],
        ),
        DiagramViewPreset::System => (
            &[
                "usecase", "screen", "field", "event", "system", "api", "dto", "entity",
            ],
            &[
                "contains",
                "invokes",
                "request",
                "response",
                "error-response",
                "reads",
                "writes",
                "creates",
                "updates",
                "deletes",
                "displays",
                "shows",
                "maps-field",
                "raises",
                "triggers",
                "owns",
            ],
        ),
        DiagramViewPreset::Data => (
            &[
                "concept",
                "domain-object",
                "aggregate",
                "value-object",
                "entity",
                "state",
            ],
            &["contains", "maps-to", "relate", "transitions", "owns"],
        ),
        DiagramViewPreset::Api => (
            &[
                "usecase",
                "system",
                "api",
                "dto",
                "entity",
                "permission",
                "medium",
            ],
            &[
                "contains",
                "invokes",
                "request",
                "response",
                "error-response",
                "reads",
                "writes",
                "creates",
                "updates",
                "deletes",
                "requires-permission",
                "requires-medium",
                "owns",
            ],
        ),
        DiagramViewPreset::Ui => (
            &["actor", "buc", "usecase", "screen", "field", "entity"],
            &["performs", "contains", "displays", "shows", "maps-field"],
        ),
    };
    (
        nodes.iter().map(|value| (*value).to_string()).collect(),
        edges.iter().map(|value| (*value).to_string()).collect(),
    )
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
            diff_base,
            show_description,
            node_kind,
            edge_kind,
            view_preset,
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

            if !usecase.is_empty()
                && !matches!(kind, DiagramKind::Sequence | DiagramKind::BusinessArea)
            {
                anyhow::bail!(
                    "--usecase is currently supported only for --kind sequence and --kind business-area"
                );
            }
            if !usecase.is_empty() && !buc.is_empty() {
                anyhow::bail!("--buc and --usecase cannot be combined");
            }
            if (!node_kind.is_empty() || !edge_kind.is_empty() || view_preset.is_some())
                && !matches!(
                    kind,
                    DiagramKind::Rdra | DiagramKind::BoundarylessGraph | DiagramKind::Diff
                )
            {
                anyhow::bail!(
                    "--node-kind, --edge-kind, and --view-preset are currently supported only for --kind rdra, --kind boundaryless-graph, or --kind diff"
                );
            }
            if matches!(kind, DiagramKind::Diff) && diff_base.is_empty() {
                anyhow::bail!("--kind diff requires at least one --diff-base path");
            }
            if !matches!(kind, DiagramKind::Diff) && !diff_base.is_empty() {
                anyhow::bail!("--diff-base is supported only with --kind diff");
            }

            let scope = if !usecase.is_empty() {
                Scope::UseCases(usecase)
            } else if buc.is_empty() {
                Scope::Whole
            } else {
                Scope::Bucs(buc)
            };
            let (preset_node_kinds, preset_edge_kinds) = diagram_preset_filters(&view_preset);
            let node_kinds = if node_kind.is_empty() {
                preset_node_kinds
            } else {
                node_kind
            };
            let edge_kinds = if edge_kind.is_empty() {
                preset_edge_kinds
            } else {
                edge_kind
            };

            // 図種に応じて filter を決定し、View を組み立てる
            let view = match &kind {
                DiagramKind::Er => View {
                    scope,
                    filter: Filter::Er,
                    show_descriptions: show_description,
                    node_kinds: Vec::new(),
                    edge_kinds: Vec::new(),
                }
                .with_graph_filters(node_kinds, edge_kinds),
                DiagramKind::Rdra
                | DiagramKind::BoundarylessGraph
                | DiagramKind::State
                | DiagramKind::Sequence
                | DiagramKind::EventFlow
                | DiagramKind::Diff
                | DiagramKind::BusinessArea
                | DiagramKind::TechnicalArea => View {
                    scope,
                    filter: Filter::None,
                    show_descriptions: show_description,
                    node_kinds: Vec::new(),
                    edge_kinds: Vec::new(),
                }
                .with_graph_filters(node_kinds, edge_kinds),
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
                    DiagramKind::Diff => {
                        let (base_model, _) = load_model(&diff_base)?;
                        DiffMermaidEmitter { base: &base_model }.emit_diff(&model, &view)?
                    }
                    DiagramKind::BusinessArea => BusinessAreaMermaidEmitter.emit(&model, &view)?,
                    DiagramKind::TechnicalArea => {
                        TechnicalAreaMermaidEmitter.emit(&model, &view)?
                    }
                },
                _ => match kind {
                    DiagramKind::Rdra => ObjectGraphPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::BoundarylessGraph => RdraPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::Er => ErPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::State => StateDiagramEmitter.emit(&model, &view)?,
                    DiagramKind::Sequence => SequenceDiagramEmitter.emit(&model, &view)?,
                    DiagramKind::EventFlow => EventFlowPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::Diff => {
                        let (base_model, _) = load_model(&diff_base)?;
                        DiffPlantUmlEmitter { base: &base_model }.emit_diff(&model, &view)?
                    }
                    DiagramKind::BusinessArea => BusinessAreaPlantUmlEmitter.emit(&model, &view)?,
                    DiagramKind::TechnicalArea => {
                        TechnicalAreaPlantUmlEmitter.emit(&model, &view)?
                    }
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
                CsvKind::BusinessInputs => (
                    BusinessInputCsvEmitter.emit(&model, &view)?,
                    "business-inputs.csv",
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

        Commands::Lint { inputs, format } => {
            let (model, diags) = load_model(&inputs)?;

            let issues = lint_issues(&model, &diags);
            let has_error = issues.iter().any(|issue| issue.severity == "error");
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
        } => {
            if write && check {
                anyhow::bail!("--write and --check cannot be combined");
            }

            let mut files = collect_rdra_files(&inputs);
            files.sort();
            if files.is_empty() {
                anyhow::bail!("no .rdra files found in the given inputs");
            }

            let multiple_files = files.len() > 1;
            let mut changed = Vec::new();
            for (index, file) in files.into_iter().enumerate() {
                let src = fs::read_to_string(&file)
                    .with_context(|| format!("failed to read {}", file.display()))?;
                let formatted = format_source(&src)
                    .with_context(|| format!("failed to format {}", file.display()))?;
                if formatted != src {
                    changed.push(file.clone());
                    if write {
                        fs::write(&file, &formatted)
                            .with_context(|| format!("failed to write {}", file.display()))?;
                    }
                }

                if !write && !check {
                    if multiple_files {
                        if index > 0 {
                            println!();
                        }
                        println!("// {}", file.display());
                    }
                    print!("{}", formatted);
                }
            }

            if check && !changed.is_empty() {
                for file in &changed {
                    eprintln!("needs formatting: {}", file.display());
                }
                std::process::exit(1);
            }

            if write {
                println!("formatted {} file(s)", changed.len());
            } else if check {
                println!("OK: all files formatted");
            }
        }

        Commands::Export { inputs, kind, out } => {
            let (model, diags) = load_model(&inputs)?;

            for diag in &diags {
                if diag.is_warning {
                    eprintln!("warning: {}", diag.error);
                } else {
                    eprintln!("error: {}", diag.error);
                }
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

fn export_artifact(
    model: &rdra_ish_core::SemanticModel,
    kind: &ExportKind,
    view: &View,
) -> Result<(String, &'static str)> {
    match kind {
        ExportKind::Openapi => Ok((OpenApiJsonEmitter.emit(model, view)?, "openapi.json")),
        ExportKind::Asyncapi => Ok((AsyncApiJsonEmitter.emit(model, view)?, "asyncapi.json")),
        ExportKind::Dbml => Ok((DbmlEmitter.emit(model, view)?, "schema.dbml")),
        ExportKind::JsonSchema => Ok((JsonSchemaEmitter.emit(model, view)?, "json-schema.json")),
        ExportKind::MermaidEr => Ok((ErMermaidEmitter.emit(model, view)?, "er.mmd")),
        ExportKind::PlantumlEr => Ok((ErPlantUmlEmitter.emit(model, view)?, "er.puml")),
    }
}

fn format_source(src: &str) -> Result<String> {
    let (ast, errors) = rdra_ish_syntax::parse(src);
    if !errors.is_empty() {
        anyhow::bail!("parse errors: {:?}", errors);
    }
    Ok(format_ast(&ast))
}

fn format_ast(ast: &Ast) -> String {
    let mut out = String::new();
    for (index, item) in ast.items.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format_item(item));
        out.push('\n');
    }
    out
}

fn format_item(item: &Item) -> String {
    match item {
        Item::Module(name, _) => format!("module {}", format_dotted_name(name)),
        Item::Import(import) => format_import(import),
        Item::Instance(instance) => format_instance(instance),
        Item::Predicate(predicate) => format_predicate(predicate),
    }
}

fn format_dotted_name(name: &DottedName) -> String {
    name.0.join(".")
}

fn format_import(import: &ImportDecl) -> String {
    let path = format_dotted_name(&import.path);
    match &import.kind {
        ImportKind::All => format!("import {}", path),
        ImportKind::Alias(alias) => format!("import {} as {}", path, alias),
        ImportKind::Select(items) => {
            let items = items
                .iter()
                .map(|item| match &item.alias {
                    Some(alias) => format!("{} as {}", item.name, alias),
                    None => item.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("import {}.{{{}}}", path, items)
        }
    }
}

fn format_instance(instance: &InstanceDecl) -> String {
    let mut lines = vec![format!(
        "{} {} {}",
        instance.kind.name(),
        instance.id,
        quote_string(&instance.label)
    )];

    if let Some(description) = &instance.description {
        lines.push(format!("  description {}", quote_string(description)));
    }

    lines.extend(format_requirement_metadata(instance));
    lines.extend(format_adr_metadata(instance));
    lines.extend(format_api_metadata(instance));
    lines.extend(format_nfr_metadata(instance));
    lines.extend(format_field_metadata(instance));
    lines.extend(format_usecase_metadata(instance));

    if !instance.columns.is_empty() {
        lines[0].push_str(" {");
        for column in &instance.columns {
            lines.push(format!("  {}", format_column(column)));
        }
        lines.push("}".to_string());
    }

    lines.join("\n")
}

fn format_requirement_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.requirement.priority {
        lines.push(format!("  priority {}", quote_string(value)));
    }
    for value in &instance.requirement.sources {
        lines.push(format!("  source {}", quote_string(value)));
    }
    for value in &instance.requirement.stakeholders {
        lines.push(format!("  stakeholder {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.owner {
        lines.push(format!("  owner {}", quote_string(value)));
    }
    for value in &instance.requirement.acceptance_criteria {
        lines.push(format!("  acceptance criteria {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.status {
        lines.push(format!("  status {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.risk {
        lines.push(format!("  risk {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.rationale {
        lines.push(format!("  rationale {}", quote_string(value)));
    }
    lines
}

fn format_adr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.adr.status {
        lines.push(format!("  adr_status {}", format_metadata_value(value)));
    }
    for value in &instance.adr.context {
        lines.push(format!("  context {}", quote_string(value)));
    }
    if let Some(value) = &instance.adr.decision {
        lines.push(format!("  decision {}", quote_string(value)));
    }
    for value in &instance.adr.consequences {
        lines.push(format!("  consequence {}", quote_string(value)));
    }
    for value in &instance.adr.accepted_options {
        lines.push(format!("  accepted {}", quote_string(value)));
    }
    for value in &instance.adr.rejected_options {
        lines.push(format!("  rejected {}", quote_string(value)));
    }
    for value in &instance.adr.reasons {
        lines.push(format!("  reason {}", quote_string(value)));
    }
    lines
}

fn format_api_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.api.method {
        lines.push(format!("  method {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.path {
        lines.push(format!("  path {}", quote_string(value)));
    }
    if let Some(value) = &instance.api.idempotency {
        lines.push(format!("  idempotency {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.mode {
        lines.push(format!("  mode {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.auth_scheme {
        lines.push(format!("  auth {}", format_metadata_value(value)));
    }
    lines
}

fn format_nfr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.nfr.metric {
        lines.push(format!("  metric {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.target {
        lines.push(format!("  target {}", quote_string(value)));
    }
    if let Some(value) = &instance.nfr.window {
        lines.push(format!("  window {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.slo {
        lines.push(format!("  slo {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.availability {
        lines.push(format!("  availability {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.resilience {
        lines.push(format!("  resilience {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.audit {
        lines.push(format!("  audit {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.logging {
        lines.push(format!("  logging {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.retention {
        lines.push(format!("  retention {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.privacy {
        lines.push(format!("  privacy {}", format_metadata_value(value)));
    }
    lines
}

fn format_field_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.field.access {
        lines.push(format!("  access {}", format_metadata_value(value)));
    }
    if let Some(value) = instance.field.required {
        lines.push(format!("  required {}", bool_cell(value)));
    }
    if let Some(value) = &instance.field.source {
        lines.push(format!("  source {}", format_metadata_value(value)));
    }
    lines
}

fn format_usecase_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    for value in &instance.usecase.preconditions {
        lines.push(format!("  precondition {}", quote_string(value)));
    }
    for value in &instance.usecase.guards {
        lines.push(format!("  guard {}", quote_string(value)));
    }
    for value in &instance.usecase.postconditions {
        lines.push(format!("  postcondition {}", quote_string(value)));
    }
    for value in &instance.usecase.alternatives {
        lines.push(format!("  alternative {}", quote_string(value)));
    }
    for value in &instance.usecase.errors {
        lines.push(format!("  error {}", quote_string(value)));
    }
    lines
}

fn format_column(column: &Column) -> String {
    let mut out = format!("{}: {}", column.name, format_col_type(&column.col_type));
    for annotation in &column.annotations {
        out.push(' ');
        out.push_str(&format_annotation(annotation));
    }
    out
}

fn format_col_type(col_type: &ColType) -> String {
    match col_type {
        ColType::Int => "Int".to_string(),
        ColType::String => "String".to_string(),
        ColType::Money => "Money".to_string(),
        ColType::DateTime => "DateTime".to_string(),
        ColType::Date => "Date".to_string(),
        ColType::Bool => "Bool".to_string(),
        ColType::Decimal => "Decimal".to_string(),
        ColType::Enum(values) => format!("Enum({})", values.join(", ")),
    }
}

fn format_annotation(annotation: &Annotation) -> String {
    match annotation {
        Annotation::Pk => "@pk".to_string(),
        Annotation::PkComposite(columns) => format!("@pk({})", columns.join(", ")),
        Annotation::Unique => "@unique".to_string(),
        Annotation::UniqueComposite(columns) => format!("@unique({})", columns.join(", ")),
        Annotation::Index => "@index".to_string(),
        Annotation::IndexComposite(columns) => format!("@index({})", columns.join(", ")),
        Annotation::Check(expr) => format!("@check({})", quote_string(expr)),
        Annotation::Null => "@null".to_string(),
        Annotation::Default(value) => format!("@default({})", format_metadata_value(value)),
        Annotation::Label(value) => format!("@label({})", quote_string(value)),
        Annotation::SoftDelete => "@soft_delete".to_string(),
        Annotation::History => "@history".to_string(),
        Annotation::Tenant => "@tenant".to_string(),
        Annotation::Derived(expr) => format!("@derived({})", quote_string(expr)),
    }
}

fn format_predicate(predicate: &PredicateCall) -> String {
    let args = predicate
        .args
        .iter()
        .map(format_predicate_arg)
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!("{}({})", predicate.name, args);
    for chain in &predicate.chain {
        out.push_str(&format_chain_call(chain));
    }
    out
}

fn format_chain_call(chain: &ChainCall) -> String {
    let args = chain
        .args
        .iter()
        .map(format_predicate_arg)
        .collect::<Vec<_>>()
        .join(", ");
    format!(".{}({})", chain.name, args)
}

fn format_predicate_arg(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(qref) => format_qref(qref),
        PredicateArg::Lit(value) => quote_string(value),
        PredicateArg::Tuple(args) => format!(
            "({})",
            args.iter()
                .map(format_predicate_arg)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PredicateArg::Expr(expr) => format_expr(expr),
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Cmp(cmp) => format!(
            "{} {} {}",
            format_operand(&cmp.lhs),
            format_cmp_op(&cmp.op),
            format_operand(&cmp.rhs)
        ),
    }
}

fn format_operand(operand: &Operand) -> String {
    match operand {
        Operand::Column(value) => value.clone(),
        Operand::QualifiedColumn(column) => {
            format!("{}.{}", format_qref(&column.entity), column.column)
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
    }
}

fn format_cmp_op(op: &CmpOp) -> &'static str {
    match op {
        CmpOp::Lt => "<",
        CmpOp::Gt => ">",
        CmpOp::Le => "<=",
        CmpOp::Ge => ">=",
        CmpOp::Eq => "==",
        CmpOp::Ne => "!=",
    }
}

fn format_qref(qref: &QRef) -> String {
    if let Some(kind) = &qref.kind_qualifier {
        format!("{}::{}", kind.name(), qref.parts.join("."))
    } else {
        qref.parts.join(".")
    }
}

fn format_metadata_value(value: &str) -> String {
    if is_identish(value) {
        value.to_string()
    } else {
        quote_string(value)
    }
}

fn quote_string(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "'"))
}

fn is_identish(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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

#[derive(Debug, Clone)]
struct LintIssue {
    severity: &'static str,
    rule: &'static str,
    subject_kind: String,
    subject_id: String,
    message: String,
    hint: String,
}

impl LintIssue {
    fn new(
        severity: &'static str,
        rule: &'static str,
        subject_kind: impl Into<String>,
        subject_id: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            rule,
            subject_kind: subject_kind.into(),
            subject_id: subject_id.into(),
            message: message.into(),
            hint: hint.into(),
        }
    }
}

fn format_lint_issues(issues: &[LintIssue], format: &ListFormat) -> Result<String> {
    let headers = [
        "severity",
        "rule",
        "subject_kind",
        "subject_id",
        "message",
        "hint",
    ];
    let rows: Vec<[String; 6]> = issues
        .iter()
        .map(|issue| {
            [
                issue.severity.to_string(),
                issue.rule.to_string(),
                issue.subject_kind.clone(),
                issue.subject_id.clone(),
                issue.message.clone(),
                issue.hint.clone(),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "lint issues")
}

fn lint_issues(
    model: &rdra_ish_core::SemanticModel,
    diags: &[rdra_ish_core::Diagnostic],
) -> Vec<LintIssue> {
    let mut findings = Vec::new();

    for diag in diags {
        findings.push(LintIssue::new(
            if diag.is_warning { "warning" } else { "error" },
            if diag.is_warning {
                "semantic-warning"
            } else {
                "semantic-error"
            },
            "model",
            "",
            diag.error.to_string(),
            "fix the DSL diagnostic before relying on derived review output",
        ));
    }

    for warning in consistency_warnings(model) {
        findings.push(LintIssue::new(
            "warning",
            "consistency-warning",
            "model",
            "",
            warning,
            "review the consistency diagnostic and add the missing relation or metadata",
        ));
    }

    let degree = relation_degree(model);
    for node in lint_node_refs(model) {
        let Some(id) = node_id(model, &node) else {
            continue;
        };
        let kind = node_kind_name(&node).to_string();

        if degree.get(&node).copied().unwrap_or(0) == 0 {
            findings.push(LintIssue::new(
                "warning",
                "orphan-node",
                kind.clone(),
                id.clone(),
                "model element is declared but not connected to another element",
                "connect it with a predicate, map it, or remove it if it is not part of the model",
            ));
        }

        if !is_upper_camelish_id(&id) {
            findings.push(LintIssue::new(
                "warning",
                "naming-id",
                kind,
                id,
                "element id does not follow UpperCamelCase project convention",
                "rename the id to UpperCamelCase; keep labels for human-readable text",
            ));
        }
    }

    lint_structural_coverage(model, &mut findings);
    lint_column_names(model, &mut findings);

    let mut issues = vec![
        coverage_score_issue(model, &findings),
        stage_readiness_issue(model),
    ];
    issues.extend(findings);
    issues
}

fn lint_structural_coverage(model: &rdra_ish_core::SemanticModel, issues: &mut Vec<LintIssue>) {
    for (buc_key, buc) in &model.bucs {
        let has_child = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Buc(buc_key)
                && matches!(rel.to, NodeRef::UseCase(_) | NodeRef::Flow(_))
        });
        if !has_child {
            issues.push(LintIssue::new(
                "warning",
                "buc-empty",
                "buc",
                buc.id.clone(),
                "BUC contains no use case or business flow",
                "add contains(Buc, UseCase) or contains(Buc, Flow)",
            ));
        }
    }

    for (flow_key, flow) in &model.flows {
        let has_step = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Flow(flow_key)
                && matches!(rel.to, NodeRef::Step(_))
        });
        if !has_step {
            issues.push(LintIssue::new(
                "warning",
                "flow-empty",
                "flow",
                flow.id.clone(),
                "business flow has no steps",
                "add contains(Flow, Step) for each business step",
            ));
        }
    }

    for (step_key, step) in &model.steps {
        let covers_model_element = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Covers
                && rel.from == NodeRef::Step(step_key)
                && matches!(
                    rel.to,
                    NodeRef::UseCase(_) | NodeRef::Api(_) | NodeRef::Event(_)
                )
        });
        if !covers_model_element {
            issues.push(LintIssue::new(
                "warning",
                "step-no-cover",
                "step",
                step.id.clone(),
                "business step is not anchored to a use case, API, or event",
                "add covers(Step, UseCase), covers(Step, Api), or covers(Step, Event)",
            ));
        }
    }

    for (api_key, api) in &model.apis {
        let invoked = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Invokes && rel.to == NodeRef::Api(api_key));
        if !invoked {
            issues.push(LintIssue::new(
                "warning",
                "api-unused",
                "api",
                api.id.clone(),
                "API is not invoked by any use case",
                "add invokes(UseCase, Api) or remove the API from this model slice",
            ));
        }

        if api.method.is_some() ^ api.path.is_some() {
            issues.push(LintIssue::new(
                "warning",
                "api-contract-incomplete",
                "api",
                api.id.clone(),
                "API contract declares only one of method/path",
                "declare both method and path before exporting OpenAPI",
            ));
        }
    }

    for (dto_key, dto) in &model.dtos {
        let used = model.relations.iter().any(|rel| {
            matches!(
                rel.kind,
                RelKind::Request | RelKind::Response | RelKind::ErrorResponse
            ) && rel.to == NodeRef::Dto(dto_key)
        });
        if !used {
            issues.push(LintIssue::new(
                "warning",
                "dto-unused",
                "dto",
                dto.id.clone(),
                "DTO is not referenced by any API request or response",
                "add request(Api, Dto), response(Api, Dto), or error_response(Api, Dto)",
            ));
        }
    }

    for (field_key, field) in &model.fields {
        let contained = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.to == NodeRef::Field(field_key)
                && matches!(rel.from, NodeRef::Screen(_))
        });
        if !contained {
            issues.push(LintIssue::new(
                "warning",
                "field-unplaced",
                "field",
                field.id.clone(),
                "screen field is not contained by any screen",
                "add contains(Screen, Field)",
            ));
        }

        let mapped = model
            .field_mappings
            .iter()
            .any(|mapping| mapping.field == field_key);
        if !mapped {
            issues.push(LintIssue::new(
                "warning",
                "field-unmapped",
                "field",
                field.id.clone(),
                "screen field is not mapped to an Entity column",
                "add maps_field(Field, Entity, \"column\") when the field has data lineage",
            ));
        }
    }

    for (screen_key, screen) in &model.screens {
        let has_field = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Screen(screen_key)
                && matches!(rel.to, NodeRef::Field(_))
        });
        if !has_field {
            issues.push(LintIssue::new(
                "warning",
                "screen-no-fields",
                "screen",
                screen.id.clone(),
                "screen has no first-class fields",
                "add field declarations and contains(Screen, Field) for input/output review",
            ));
        }
    }

    for (requirement_key, requirement) in &model.requirements {
        let motivates = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Motivates && rel.from == NodeRef::Requirement(requirement_key)
        });
        if !motivates {
            issues.push(LintIssue::new(
                "warning",
                "requirement-untraced",
                "requirement",
                requirement.id.clone(),
                "requirement does not motivate a BUC",
                "add motivates(Requirement, Buc) to preserve requirement traceability",
            ));
        }
    }

    for (nfr_key, nfr) in &model.nfrs {
        let applies = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::AppliesTo && rel.from == NodeRef::Nfr(nfr_key));
        if !applies {
            issues.push(LintIssue::new(
                "warning",
                "nfr-unscoped",
                "nfr",
                nfr.id.clone(),
                "NFR is not applied to a use case, API, or system",
                "add applies_to(Nfr, UseCase|Api|System)",
            ));
        }
    }

    for (constraint_key, constraint) in &model.constraints {
        let constrains = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Constrains && rel.from == NodeRef::Constraint(constraint_key)
        });
        if !constrains {
            issues.push(LintIssue::new(
                "warning",
                "constraint-unscoped",
                "constraint",
                constraint.id.clone(),
                "constraint is not attached to a target model element",
                "add constrains(Constraint, UseCase|Api|System|Entity|Dto)",
            ));
        }
    }
}

fn lint_column_names(model: &rdra_ish_core::SemanticModel, issues: &mut Vec<LintIssue>) {
    for (_, entity) in &model.entities {
        for column in &entity.columns {
            if !is_snake_caseish(&column.name) {
                issues.push(LintIssue::new(
                    "warning",
                    "naming-column",
                    "entity-column",
                    format!("{}.{}", entity.id, column.name),
                    "entity column name does not follow snake_case project convention",
                    "rename the column to snake_case or document the external naming constraint",
                ));
            }
        }
    }

    for (_, dto) in &model.dtos {
        for field in &dto.fields {
            if !is_snake_caseish(&field.name) {
                issues.push(LintIssue::new(
                    "warning",
                    "naming-dto-field",
                    "dto-field",
                    format!("{}.{}", dto.id, field.name),
                    "DTO field name does not follow snake_case project convention",
                    "rename the DTO field to snake_case or document the external contract constraint",
                ));
            }
        }
    }
}

fn coverage_score_issue(model: &rdra_ish_core::SemanticModel, findings: &[LintIssue]) -> LintIssue {
    let penalty: i32 = findings
        .iter()
        .map(|issue| match issue.severity {
            "error" => 15,
            "warning" => {
                if issue.rule == "semantic-warning" || issue.rule == "consistency-warning" {
                    4
                } else {
                    2
                }
            }
            _ => 0,
        })
        .sum();
    let score = (100 - penalty).max(0);
    LintIssue::new(
        "info",
        "coverage-score",
        "model",
        "",
        format!(
            "coverage score: {}/100 across {} model elements and {} findings",
            score,
            model_element_count(model),
            findings.len()
        ),
        "use warning rows as the review backlog; the score is a lightweight readiness signal",
    )
}

fn stage_readiness_issue(model: &rdra_ish_core::SemanticModel) -> LintIssue {
    let stages = [
        (
            "scope",
            !model.actors.is_empty() && !model.bucs.is_empty() && !model.use_cases.is_empty(),
        ),
        (
            "business-flow",
            !model.bucs.is_empty() && (!model.flows.is_empty() || !model.use_cases.is_empty()),
        ),
        ("data", !model.entities.is_empty()),
        (
            "interaction",
            !model.screens.is_empty() || !model.apis.is_empty(),
        ),
        (
            "system-boundary",
            !model.systems.is_empty() && !model.apis.is_empty(),
        ),
        (
            "rules",
            !model.requirements.is_empty()
                || !model.nfrs.is_empty()
                || !model.constraints.is_empty()
                || !model.forbidden_constraints.is_empty()
                || !model.entity_invariants.is_empty()
                || !model.required_constraints.is_empty()
                || !model.exclusive_constraints.is_empty()
                || !model.cross_forbidden_constraints.is_empty()
                || !model.cross_entity_invariants.is_empty(),
        ),
    ];
    let ready: Vec<_> = stages
        .iter()
        .filter_map(|(stage, ok)| ok.then_some(*stage))
        .collect();
    let missing: Vec<_> = stages
        .iter()
        .filter_map(|(stage, ok)| (!ok).then_some(*stage))
        .collect();
    LintIssue::new(
        "info",
        "stage-readiness",
        "model",
        "",
        format!(
            "ready stages: {}; missing stages: {}",
            if ready.is_empty() {
                "none".to_string()
            } else {
                ready.join("|")
            },
            if missing.is_empty() {
                "none".to_string()
            } else {
                missing.join("|")
            }
        ),
        "use missing stages to decide the next modeling refinement pass",
    )
}

fn model_element_count(model: &rdra_ish_core::SemanticModel) -> usize {
    model.actors.len()
        + model.ext_systems.len()
        + model.systems.len()
        + model.requirements.len()
        + model.nfrs.len()
        + model.qualities.len()
        + model.constraints.len()
        + model.concepts.len()
        + model.domain_objects.len()
        + model.aggregates.len()
        + model.value_objects.len()
        + model.businesses.len()
        + model.bucs.len()
        + model.flows.len()
        + model.steps.len()
        + model.usage_scenes.len()
        + model.use_cases.len()
        + model.screens.len()
        + model.fields.len()
        + model.events.len()
        + model.entities.len()
        + model.states.len()
        + model.conditions.len()
        + model.variations.len()
        + model.apis.len()
        + model.dtos.len()
        + model.locations.len()
        + model.timings.len()
        + model.media.len()
        + model.permissions.len()
}

fn relation_degree(model: &rdra_ish_core::SemanticModel) -> HashMap<NodeRef, usize> {
    let mut degree = HashMap::new();
    let mut touch = |node: NodeRef| {
        *degree.entry(node).or_insert(0) += 1;
    };

    for rel in &model.relations {
        touch(rel.from.clone());
        touch(rel.to.clone());
    }
    for mapping in &model.field_mappings {
        touch(NodeRef::Field(mapping.field));
        touch(NodeRef::Entity(mapping.entity));
    }
    for transition in &model.state_transitions {
        touch(transition.event.clone());
        touch(transition.from.clone());
        touch(transition.to.clone());
    }
    for coordination in &model.boundary_coordinations {
        touch(NodeRef::UseCase(coordination.usecase));
        touch(NodeRef::Entity(coordination.left));
        touch(NodeRef::Entity(coordination.right));
    }
    for effect in &model.column_effects {
        touch(effect.origin.clone());
        touch(NodeRef::Entity(effect.entity));
    }
    for effect in &model.proposition_effects {
        touch(effect.origin.clone());
        touch(NodeRef::Entity(effect.entity));
    }
    for constraint in &model.forbidden_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for invariant in &model.entity_invariants {
        touch(NodeRef::Entity(invariant.entity));
    }
    for constraint in &model.required_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for constraint in &model.exclusive_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for constraint in &model.cross_forbidden_constraints {
        for entity in &constraint.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for invariant in &model.cross_entity_invariants {
        for entity in &invariant.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for assertion in &model.temporal_assertions {
        touch(NodeRef::UseCase(assertion.anchor));
        for entity in &assertion.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for constraint in &model.quantifier_constraints {
        touch(NodeRef::Entity(constraint.anchor));
        touch(NodeRef::Entity(constraint.related));
    }
    for event in &model.outbox_events {
        touch(NodeRef::Event(*event));
    }

    degree
}

fn lint_node_refs(model: &rdra_ish_core::SemanticModel) -> Vec<NodeRef> {
    let mut nodes = Vec::new();
    nodes.extend(model.actors.iter().map(|(key, _)| NodeRef::Actor(key)));
    nodes.extend(
        model
            .ext_systems
            .iter()
            .map(|(key, _)| NodeRef::ExtSystem(key)),
    );
    nodes.extend(model.systems.iter().map(|(key, _)| NodeRef::System(key)));
    nodes.extend(
        model
            .requirements
            .iter()
            .map(|(key, _)| NodeRef::Requirement(key)),
    );
    nodes.extend(model.adrs.iter().map(|(key, _)| NodeRef::Adr(key)));
    nodes.extend(model.nfrs.iter().map(|(key, _)| NodeRef::Nfr(key)));
    nodes.extend(model.qualities.iter().map(|(key, _)| NodeRef::Quality(key)));
    nodes.extend(
        model
            .constraints
            .iter()
            .map(|(key, _)| NodeRef::Constraint(key)),
    );
    nodes.extend(model.concepts.iter().map(|(key, _)| NodeRef::Concept(key)));
    nodes.extend(
        model
            .domain_objects
            .iter()
            .map(|(key, _)| NodeRef::DomainObject(key)),
    );
    nodes.extend(
        model
            .aggregates
            .iter()
            .map(|(key, _)| NodeRef::Aggregate(key)),
    );
    nodes.extend(
        model
            .value_objects
            .iter()
            .map(|(key, _)| NodeRef::ValueObject(key)),
    );
    nodes.extend(
        model
            .businesses
            .iter()
            .map(|(key, _)| NodeRef::Business(key)),
    );
    nodes.extend(model.bucs.iter().map(|(key, _)| NodeRef::Buc(key)));
    nodes.extend(model.flows.iter().map(|(key, _)| NodeRef::Flow(key)));
    nodes.extend(model.steps.iter().map(|(key, _)| NodeRef::Step(key)));
    nodes.extend(
        model
            .usage_scenes
            .iter()
            .map(|(key, _)| NodeRef::UsageScene(key)),
    );
    nodes.extend(model.use_cases.iter().map(|(key, _)| NodeRef::UseCase(key)));
    nodes.extend(model.screens.iter().map(|(key, _)| NodeRef::Screen(key)));
    nodes.extend(model.fields.iter().map(|(key, _)| NodeRef::Field(key)));
    nodes.extend(model.events.iter().map(|(key, _)| NodeRef::Event(key)));
    nodes.extend(model.entities.iter().map(|(key, _)| NodeRef::Entity(key)));
    nodes.extend(model.states.iter().map(|(key, _)| NodeRef::State(key)));
    nodes.extend(
        model
            .conditions
            .iter()
            .map(|(key, _)| NodeRef::Condition(key)),
    );
    nodes.extend(
        model
            .variations
            .iter()
            .map(|(key, _)| NodeRef::Variation(key)),
    );
    nodes.extend(model.apis.iter().map(|(key, _)| NodeRef::Api(key)));
    nodes.extend(model.dtos.iter().map(|(key, _)| NodeRef::Dto(key)));
    nodes.extend(
        model
            .locations
            .iter()
            .map(|(key, _)| NodeRef::Location(key)),
    );
    nodes.extend(model.timings.iter().map(|(key, _)| NodeRef::Timing(key)));
    nodes.extend(model.media.iter().map(|(key, _)| NodeRef::Medium(key)));
    nodes.extend(
        model
            .permissions
            .iter()
            .map(|(key, _)| NodeRef::Permission(key)),
    );
    nodes
}

fn node_kind_name(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Adr(_) => "adr",
        NodeRef::Nfr(_) => "nfr",
        NodeRef::Quality(_) => "quality",
        NodeRef::Constraint(_) => "constraint",
        NodeRef::Concept(_) => "concept",
        NodeRef::DomainObject(_) => "domain-object",
        NodeRef::Aggregate(_) => "aggregate",
        NodeRef::ValueObject(_) => "value-object",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::Flow(_) => "flow",
        NodeRef::Step(_) => "step",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Field(_) => "field",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Dto(_) => "dto",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}

fn node_id(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.id.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.id.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.id.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.id.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.id.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.id.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.id.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.id.clone(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.id.clone(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.id.clone(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.id.clone(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.id.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.id.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.id.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.id.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.id.clone(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.id.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.id.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.id.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.id.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.id.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.id.clone(),
        NodeRef::State(key) => model.states.get(*key)?.id.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.id.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.id.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.id.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.id.clone(),
        NodeRef::Location(key) => model.locations.get(*key)?.id.clone(),
        NodeRef::Timing(key) => model.timings.get(*key)?.id.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.id.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.id.clone(),
    })
}

fn is_upper_camelish_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && id.chars().all(|c| c.is_ascii_alphanumeric())
        && id.chars().any(|c| c.is_ascii_lowercase())
}

fn is_snake_caseish(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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
            correlation_hint,
        } => {
            let mut message = format!(
                "forbidden state is reachable: {} witnessed by {}",
                conditions, pattern_desc
            );
            if let Some(hint) = correlation_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::InvariantViolated {
            guards,
            requireds,
            pattern_desc,
            flow_order_hint,
        } => {
            let mut message = format!(
                "invariant violated: when {} then {} is broken by {}",
                guards, requireds, pattern_desc
            );
            if let Some(hint) = flow_order_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::RequiredStateViolated {
            conditions,
            pattern_desc,
        } => format!(
            "required state is missing: {} is not satisfied by {}",
            conditions, pattern_desc
        ),
        rdra_ish_core::StateDiag::ExclusiveStateViolated {
            conditions,
            pattern_desc,
        } => format!(
            "exclusive state conditions co-occur: {} witnessed by {}",
            conditions, pattern_desc
        ),
        rdra_ish_core::StateDiag::CrossForbiddenViolated {
            entities,
            conditions,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "cross-entity forbidden state is reachable across [{}]: {} witnessed by {}",
                entities, conditions, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::CrossInvariantViolated {
            entities,
            guards,
            requireds,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "cross-entity invariant violated across [{}]: when {} then {} is broken by {}",
                entities, guards, requireds, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint,
            reason,
        } => format!(
            "cross-entity constraint was not fully evaluated across [{}]: {} ({})",
            entities, constraint, reason
        ),
        rdra_ish_core::StateDiag::TemporalAssertionViolated {
            anchor,
            requireds,
            actual,
        } => format!(
            "temporal assertion violated after '{}': expected {}, but {}",
            anchor, requireds, actual
        ),
        rdra_ish_core::StateDiag::TemporalAssertionNotEvaluated {
            anchor,
            requireds,
            reason,
        } => format!(
            "temporal assertion after '{}' was not evaluated: {} ({})",
            anchor, requireds, reason
        ),
        rdra_ish_core::StateDiag::QuantifierConstraintNotEvaluated {
            anchor,
            related,
            constraint,
            reason,
        } => format!(
            "to-many quantifier constraint was not evaluated from '{}' to '{}': {} ({})",
            anchor, related, constraint, reason
        ),
        rdra_ish_core::StateDiag::UndrivenComparisonProp {
            proposition,
            usage,
            effect,
        } => format!(
            "comparison proposition '{}' used in {} is not driven by sets(..., <comparison>, true/false): {}",
            proposition, usage, effect
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
        ListKind::Flow => {
            let mut items: Vec<(&str, &str)> = model
                .flows
                .iter()
                .map(|(_, f)| (f.id.as_str(), f.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "flows")
        }
        ListKind::Step => {
            let mut items: Vec<(&str, &str)> = model
                .steps
                .iter()
                .map(|(_, s)| (s.id.as_str(), s.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "steps")
        }
        ListKind::Usecase => format_usecases(model, format),
        ListKind::Field => format_fields(model, format),
        ListKind::Entity => format_entities(model, format),
        ListKind::Requirement => format_requirements(model, format),
        ListKind::Adr => format_adrs(model, format),
        ListKind::AdrImpact => format_adr_impacts(model, format),
        ListKind::Nfr => format_nfrs(model, format),
        ListKind::Quality => {
            let mut items: Vec<(&str, &str)> = model
                .qualities
                .iter()
                .map(|(_, q)| (q.id.as_str(), q.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "qualities")
        }
        ListKind::Constraint => format_constraints(model, format),
        ListKind::Concept => {
            let mut items: Vec<(&str, &str)> = model
                .concepts
                .iter()
                .map(|(_, c)| (c.id.as_str(), c.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "concepts")
        }
        ListKind::DomainObject => {
            let mut items: Vec<(&str, &str)> = model
                .domain_objects
                .iter()
                .map(|(_, d)| (d.id.as_str(), d.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "domain objects")
        }
        ListKind::Aggregate => {
            let mut items: Vec<(&str, &str)> = model
                .aggregates
                .iter()
                .map(|(_, a)| (a.id.as_str(), a.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "aggregates")
        }
        ListKind::ValueObject => {
            let mut items: Vec<(&str, &str)> = model
                .value_objects
                .iter()
                .map(|(_, v)| (v.id.as_str(), v.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "value objects")
        }
        ListKind::System => {
            let mut items: Vec<(&str, &str)> = model
                .systems
                .iter()
                .map(|(_, s)| (s.id.as_str(), s.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "systems")
        }
        ListKind::Api => format_apis(model, format),
        ListKind::Dto => format_dtos(model, format),
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
        ListKind::BusinessInputs => format_business_inputs(model, format),
    }
}

fn format_business_inputs(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "actor_id",
        "buc_id",
        "usecase_id",
        "source",
        "entity_id",
        "column_name",
        "operation",
    ];
    let rows: Vec<[String; 7]> = derive_actor_input_inferences(model)
        .into_iter()
        .map(|entry| {
            let source = match entry.source {
                ActorInputSource::UseCase => model.use_cases[entry.usecase].id.clone(),
                ActorInputSource::Api(api) => model.apis[api].id.clone(),
            };
            [
                model.actors[entry.actor].id.clone(),
                entry
                    .buc
                    .map(|buc| model.bucs[buc].id.clone())
                    .unwrap_or_default(),
                model.use_cases[entry.usecase].id.clone(),
                source,
                model.entities[entry.entity].id.clone(),
                entry.column,
                entry.operation.as_str().to_string(),
            ]
        })
        .collect();
    format_rows(&headers, &rows, format, "actor input candidates")
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
        "usecase_api_paths",
    ];

    let rows: Vec<[String; 5]> = derive_permission_callables(model)
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
            let usecase_api_paths = permission_api_paths(model, &entry.api_paths);
            [
                permission.id.clone(),
                permission.label.clone(),
                usecase_ids,
                api_ids,
                usecase_api_paths,
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
                        "{{\"permission_id\":{},\"permission_label\":{},\"usecase_ids\":{},\"api_ids\":{},\"usecase_api_paths\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                        serde_json::to_string(&row[4]).unwrap(),
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

fn permission_api_paths(
    model: &rdra_ish_core::SemanticModel,
    paths: &[rdra_ish_core::PermissionApiPath],
) -> String {
    let mut paths: Vec<String> = paths
        .iter()
        .map(|path| {
            format!(
                "{}->{}",
                model.use_cases[path.usecase].id, model.apis[path.api].id
            )
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.join("|")
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

fn format_rows<const N: usize>(
    headers: &[&str; N],
    rows: &[[String; N]],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in rows {
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
            for row in rows {
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
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    let fields: Vec<String> = headers
                        .iter()
                        .enumerate()
                        .map(|(i, header)| {
                            format!(
                                "{}:{}",
                                serde_json::to_string(header).unwrap(),
                                serde_json::to_string(&row[i]).unwrap()
                            )
                        })
                        .collect();
                    format!("{{{}}}", fields.join(","))
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
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

fn optional_cell(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn repeated_cell(values: &[String]) -> String {
    values.join("|")
}

fn format_requirements(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut requirements: Vec<_> = model.requirements.iter().collect();
    requirements.sort_by_key(|(_, requirement)| requirement.id.as_str());

    let headers = [
        "id",
        "label",
        "priority",
        "sources",
        "stakeholders",
        "owner",
        "acceptance_criteria",
        "status",
        "risk",
        "rationale",
        "description",
    ];
    let rows: Vec<[String; 11]> = requirements
        .into_iter()
        .map(|(_, requirement)| {
            [
                requirement.id.clone(),
                requirement.label.clone(),
                optional_cell(&requirement.priority),
                repeated_cell(&requirement.sources),
                repeated_cell(&requirement.stakeholders),
                optional_cell(&requirement.owner),
                repeated_cell(&requirement.acceptance_criteria),
                optional_cell(&requirement.status),
                optional_cell(&requirement.risk),
                optional_cell(&requirement.rationale),
                optional_cell(&requirement.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "requirements")
}

fn format_adrs(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());

    let headers = [
        "id",
        "label",
        "status",
        "context",
        "decision",
        "consequences",
        "accepted_options",
        "rejected_options",
        "reasons",
        "target_kinds",
        "target_ids",
        "description",
    ];
    let rows: Vec<[String; 12]> = adrs
        .into_iter()
        .map(|(key, adr)| {
            let targets = adr_targets(model, key);
            let target_kinds = targets
                .iter()
                .map(|target| node_kind_name(target).to_string())
                .collect::<Vec<_>>()
                .join("|");
            let target_ids = targets
                .iter()
                .filter_map(|target| node_id(model, target))
                .collect::<Vec<_>>()
                .join("|");
            [
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                repeated_cell(&adr.context),
                optional_cell(&adr.decision),
                repeated_cell(&adr.consequences),
                repeated_cell(&adr.accepted_options),
                repeated_cell(&adr.rejected_options),
                repeated_cell(&adr.reasons),
                target_kinds,
                target_ids,
                optional_cell(&adr.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "ADRs")
}

fn format_adr_impacts(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let headers = [
        "adr_id",
        "adr_label",
        "adr_status",
        "target_kind",
        "target_id",
        "target_label",
    ];
    let mut rows = Vec::new();
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());
    for (key, adr) in adrs {
        for target in adr_targets(model, key) {
            rows.push([
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                node_kind_name(&target).to_string(),
                node_id(model, &target).unwrap_or_default(),
                node_label(model, &target).unwrap_or_default(),
            ]);
        }
    }
    rows.sort_by(|left, right| {
        left[0]
            .cmp(&right[0])
            .then_with(|| left[3].cmp(&right[3]))
            .then_with(|| left[4].cmp(&right[4]))
    });

    format_rows(&headers, &rows, format, "ADR impacts")
}

fn adr_targets(
    model: &rdra_ish_core::SemanticModel,
    adr: rdra_ish_core::model::AdrKey,
) -> Vec<NodeRef> {
    let mut targets: Vec<_> = model
        .relations
        .iter()
        .filter(|relation| relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr))
        .map(|relation| relation.to.clone())
        .collect();
    targets.sort_by_key(|target| {
        (
            node_kind_name(target).to_string(),
            node_id(model, target).unwrap_or_default(),
        )
    });
    targets
}

fn node_label(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.label.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.label.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.label.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.label.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.label.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.label.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.label.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.label.clone(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.label.clone(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.label.clone(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.label.clone(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.label.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.label.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.label.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.label.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.label.clone(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.label.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.label.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.label.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.label.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.label.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.label.clone(),
        NodeRef::State(key) => model.states.get(*key)?.label.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.label.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.label.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.label.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.label.clone(),
        NodeRef::Location(key) => model.locations.get(*key)?.label.clone(),
        NodeRef::Timing(key) => model.timings.get(*key)?.label.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.label.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.label.clone(),
    })
}

fn format_usecases(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut usecases: Vec<_> = model.use_cases.iter().collect();
    usecases.sort_by_key(|(_, usecase)| usecase.id.as_str());

    let headers = [
        "id",
        "label",
        "preconditions",
        "guards",
        "postconditions",
        "alternatives",
        "errors",
        "description",
    ];
    let rows: Vec<[String; 8]> = usecases
        .into_iter()
        .map(|(_, usecase)| {
            [
                usecase.id.clone(),
                usecase.label.clone(),
                repeated_cell(&usecase.preconditions),
                repeated_cell(&usecase.guards),
                repeated_cell(&usecase.postconditions),
                repeated_cell(&usecase.alternatives),
                repeated_cell(&usecase.errors),
                optional_cell(&usecase.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "use cases")
}

const NFR_HEADERS: [&str; 13] = [
    "id",
    "label",
    "metric",
    "target",
    "window",
    "slo",
    "availability",
    "resilience",
    "audit",
    "logging",
    "retention",
    "privacy",
    "description",
];

fn nfr_row(
    id: &str,
    label: &str,
    description: &Option<String>,
    metadata: [&Option<String>; 10],
) -> [String; 13] {
    [
        id.to_string(),
        label.to_string(),
        optional_cell(metadata[0]),
        optional_cell(metadata[1]),
        optional_cell(metadata[2]),
        optional_cell(metadata[3]),
        optional_cell(metadata[4]),
        optional_cell(metadata[5]),
        optional_cell(metadata[6]),
        optional_cell(metadata[7]),
        optional_cell(metadata[8]),
        optional_cell(metadata[9]),
        optional_cell(description),
    ]
}

fn format_nfrs(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut nfrs: Vec<_> = model.nfrs.iter().collect();
    nfrs.sort_by_key(|(_, nfr)| nfr.id.as_str());

    let rows: Vec<[String; 13]> = nfrs
        .into_iter()
        .map(|(_, nfr)| {
            nfr_row(
                &nfr.id,
                &nfr.label,
                &nfr.description,
                [
                    &nfr.metric,
                    &nfr.target,
                    &nfr.window,
                    &nfr.slo,
                    &nfr.availability,
                    &nfr.resilience,
                    &nfr.audit,
                    &nfr.logging,
                    &nfr.retention,
                    &nfr.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "NFRs")
}

fn format_constraints(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut constraints: Vec<_> = model.constraints.iter().collect();
    constraints.sort_by_key(|(_, constraint)| constraint.id.as_str());

    let rows: Vec<[String; 13]> = constraints
        .into_iter()
        .map(|(_, constraint)| {
            nfr_row(
                &constraint.id,
                &constraint.label,
                &constraint.description,
                [
                    &constraint.metric,
                    &constraint.target,
                    &constraint.window,
                    &constraint.slo,
                    &constraint.availability,
                    &constraint.resilience,
                    &constraint.audit,
                    &constraint.logging,
                    &constraint.retention,
                    &constraint.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "constraints")
}

fn col_type_s(ct: &rdra_ish_core::model::ColumnType) -> &'static str {
    use rdra_ish_core::model::ColumnType;
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

fn format_apis(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, api)| api.id.as_str());

    let headers = [
        "api_id",
        "api_label",
        "method",
        "path",
        "idempotency",
        "mode",
        "auth_scheme",
    ];
    let rows: Vec<[String; 7]> = apis
        .into_iter()
        .map(|(_, api)| {
            [
                api.id.clone(),
                api.label.clone(),
                optional_cell(&api.method),
                optional_cell(&api.path),
                optional_cell(&api.idempotency),
                optional_cell(&api.mode),
                optional_cell(&api.auth_scheme),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "APIs")
}

fn format_dtos(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, dto)| dto.id.as_str());

    let headers = [
        "dto_id",
        "dto_label",
        "field_name",
        "field_type",
        "required",
    ];
    let mut rows: Vec<[String; 5]> = Vec::new();
    for (_, dto) in dtos {
        if dto.fields.is_empty() {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for field in &dto.fields {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                field.name.clone(),
                col_type_s(&field.col_type).to_string(),
                bool_cell(!field.is_nullable),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "DTOs")
}

fn format_fields(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut fields: Vec<_> = model.fields.iter().collect();
    fields.sort_by_key(|(_, field)| field.id.as_str());

    let headers = [
        "field_id",
        "field_label",
        "access",
        "required",
        "source",
        "entity_id",
        "column_name",
    ];
    let mut rows: Vec<[String; 7]> = Vec::new();
    for (field_key, field) in fields {
        let mappings: Vec<_> = model
            .field_mappings
            .iter()
            .filter(|mapping| mapping.field == field_key)
            .collect();
        if mappings.is_empty() {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for mapping in mappings {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                model.entities[mapping.entity].id.clone(),
                mapping.column.clone(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "fields")
}

fn format_entities(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut ents: Vec<_> = model.entities.iter().collect();
    ents.sort_by_key(|(_, e)| e.id.as_str());

    let headers = [
        "entity_id",
        "entity_label",
        "column_name",
        "column_type",
        "is_pk",
        "is_unique",
        "is_indexed",
        "is_fk",
        "fk_target",
        "fk_optional",
        "fk_on_delete",
        "fk_on_update",
        "is_nullable",
        "default_val",
        "check_constraints",
        "is_soft_delete",
        "is_history",
        "is_tenant_scope",
        "derived_expr",
    ];

    let mut rows: Vec<[String; 19]> = Vec::new();
    for (_, ent) in &ents {
        for col in &ent.columns {
            rows.push([
                ent.id.clone(),
                ent.label.clone(),
                col.name.clone(),
                col_type_s(&col.col_type).to_string(),
                if col.is_pk { "true" } else { "false" }.to_string(),
                if col.is_unique { "true" } else { "false" }.to_string(),
                if col.is_indexed { "true" } else { "false" }.to_string(),
                if col.is_fk { "true" } else { "false" }.to_string(),
                col.fk_target.clone().unwrap_or_default(),
                bool_cell(col.fk_optional),
                col.fk_on_delete.clone().unwrap_or_default(),
                col.fk_on_update.clone().unwrap_or_default(),
                bool_cell(col.is_nullable),
                col.default_val.clone().unwrap_or_default(),
                col.check_constraints.join("|"),
                bool_cell(col.is_soft_delete),
                bool_cell(col.is_history),
                bool_cell(col.is_tenant_scope),
                col.derived_expr.clone().unwrap_or_default(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "entities")
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
    fn load_model_rejects_inputs_without_rdra_files() {
        let err = load_model(&[PathBuf::from("missing-input")]).unwrap_err();

        assert_eq!(err.to_string(), "no .rdra files found in the given inputs");
    }

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

        assert_eq!(
            csv,
            "api_id,api_label,method,path,idempotency,mode,auth_scheme\n"
        );
        assert_eq!(json, "[]\n");
    }

    #[test]
    fn list_api_includes_contract_metadata() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
api CreateOrder "Create order" method POST path "/orders" idempotency "idempotent" mode sync auth bearer
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Api, &ListFormat::Csv).unwrap();
        assert!(csv.contains("api_id,api_label,method,path,idempotency,mode,auth_scheme"));
        assert!(csv.contains("CreateOrder,Create order,POST,/orders,idempotent,sync,bearer"));
    }

    #[test]
    fn lint_reports_coverage_readiness_and_review_findings() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
actor customer "Customer"
buc Checkout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
api CreateOrder "Create order" method POST
dto CreateOrderRequest "Create order request"
field ShippingAddress "Shipping address" access editable source actor
entity Order "Order" {
  Id: Int @pk
  total: Money
}
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let issues = lint_issues(&model, &diags);
        let csv = format_lint_issues(&issues, &ListFormat::Csv).unwrap();

        assert!(csv.contains("coverage-score"));
        assert!(csv.contains("stage-readiness"));
        assert!(csv.contains("naming-id"));
        assert!(csv.contains("api-contract-incomplete"));
        assert!(csv.contains("field-unmapped"));
        assert!(csv.contains("naming-column"));
    }

    #[test]
    fn fmt_canonicalizes_source_and_preserves_parseability() {
        let src = r#"module shop.checkout
import shared.actors.{Customer as Buyer, Staff}
requirement ReqCheckout "Checkout reliable" priority "must" source "Interview"
adr AdrOutbox "Use outbox" adr_status accepted decision "Use transactional outbox." reason "Avoid synchronous callbacks."
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {customer_id:Int note:String @null}
invariant(Order).when(status, paid).then(total > 0)
"#;

        let formatted = format_source(src).unwrap();

        assert_eq!(
            formatted,
            r#"module shop.checkout

import shared.actors.{Customer as Buyer, Staff}

requirement ReqCheckout "Checkout reliable"
  priority "must"
  source "Interview"

adr AdrOutbox "Use outbox"
  adr_status accepted
  decision "Use transactional outbox."
  reason "Avoid synchronous callbacks."

api CreateOrder "Create order"
  method POST
  path "/orders"
  auth bearer

dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}

invariant(Order).when(status, paid).then(total > 0)
"#
        );

        let (_ast, errors) = rdra_ish_syntax::parse(&formatted);
        assert!(
            errors.is_empty(),
            "formatted output should parse: {errors:?}"
        );
    }

    #[test]
    fn list_requirement_outputs_metadata() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority "must"
  source "Customer interview"
  source "Support tickets"
  stakeholder "Store Operations"
  owner "Product Owner"
  acceptance criteria "A payment timeout leaves the cart recoverable."
  status "proposed"
  risk "high"
  rationale "Checkout failures directly block revenue."
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Requirement, &ListFormat::Csv).unwrap();
        assert!(csv.contains(
            "id,label,priority,sources,stakeholders,owner,acceptance_criteria,status,risk,rationale,description"
        ));
        assert!(csv.contains(
            "ReqCheckout,Checkout must be reliable,must,Customer interview|Support tickets,Store Operations,Product Owner,A payment timeout leaves the cart recoverable.,proposed,high,Checkout failures directly block revenue.,The checkout flow must preserve customer intent."
        ));
    }

    #[test]
    fn list_adr_outputs_decisions_and_impacts() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
adr AdrOutbox "Use transactional outbox"
  description "Decision record for external event publication."
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."
system CustomerSystem "Customer System"
entity Customer "Customer" { id: Int @pk }
decides(AdrOutbox, CustomerSystem)
decides(AdrOutbox, Customer)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let adr_csv = list_elements(&model, &ListKind::Adr, &ListFormat::Csv).unwrap();
        assert!(adr_csv.contains(
            "id,label,status,context,decision,consequences,accepted_options,rejected_options,reasons,target_kinds,target_ids,description"
        ));
        assert!(adr_csv.contains("AdrOutbox,Use transactional outbox,accepted"));
        assert!(adr_csv.contains("entity|system"));
        assert!(adr_csv.contains("Customer|CustomerSystem"));

        let impact_csv = list_elements(&model, &ListKind::AdrImpact, &ListFormat::Csv).unwrap();
        assert!(
            impact_csv.contains("adr_id,adr_label,adr_status,target_kind,target_id,target_label")
        );
        assert!(impact_csv
            .contains("AdrOutbox,Use transactional outbox,accepted,entity,Customer,Customer"));
        assert!(impact_csv.contains(
            "AdrOutbox,Use transactional outbox,accepted,system,CustomerSystem,Customer System"
        ));
    }

    #[test]
    fn list_usecase_outputs_conditions_and_alternatives() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
usecase CapturePayment "Capture payment"
  description "Captures authorized payment."
  precondition "Order is authorized."
  guard "Provider is available."
  postcondition "Payment is captured."
  alternative "Customer changes payment method."
  error "Authorization expires."
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Usecase, &ListFormat::Csv).unwrap();
        assert!(csv.contains(
            "id,label,preconditions,guards,postconditions,alternatives,errors,description"
        ));
        assert!(csv.contains(
            "CapturePayment,Capture payment,Order is authorized.,Provider is available.,Payment is captured.,Customer changes payment method.,Authorization expires.,Captures authorized payment."
        ));
    }

    #[test]
    fn export_openapi_projects_api_contracts() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {
  customer_id: Int
}
dto OrderResponse "Order response" {
  order_id: Int
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let (json, ext) = export_artifact(&model, &ExportKind::Openapi, &View::whole()).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(ext, "openapi.json");
        assert_eq!(doc["openapi"], "3.0.3");
        assert_eq!(
            doc["paths"]["/orders"]["post"]["operationId"],
            "CreateOrder"
        );
        assert_eq!(
            doc["paths"]["/orders"]["post"]["security"],
            serde_json::json!([{ "bearer": [] }])
        );
        assert_eq!(
            doc["components"]["schemas"]["CreateOrderRequest"]["required"],
            serde_json::json!(["customer_id"])
        );
    }

    #[test]
    fn export_dbml_projects_logical_data_model() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" { id: Int @pk  status: Enum(pending, paid) }
relate(Order, Customer, "N:1").on_delete(cascade)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let (dbml, ext) = export_artifact(&model, &ExportKind::Dbml, &View::whole()).unwrap();

        assert_eq!(ext, "schema.dbml");
        assert!(dbml.contains("Table Customer"));
        assert!(dbml.contains("Enum Order_status"));
        assert!(dbml.contains("Ref: Order.customer_id > Customer.id [delete: cascade]"));
    }

    #[test]
    fn export_asyncapi_projects_event_catalog() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
usecase SignEncounter "Sign encounter"
event EncounterSigned "Encounter signed"
raises(SignEncounter, EncounterSigned)
outbox(EncounterSigned)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let (json, ext) = export_artifact(&model, &ExportKind::Asyncapi, &View::whole()).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(ext, "asyncapi.json");
        assert_eq!(doc["asyncapi"], "3.1.0");
        assert_eq!(
            doc["operations"]["publishEncounterSigned"]["action"],
            "send"
        );
        assert_eq!(
            doc["components"]["messages"]["EncounterSigned"]["x-rdra-ish-outbox"],
            true
        );
    }

    #[test]
    fn export_json_schema_projects_dtos_and_entities() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid)
}
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let (json, ext) = export_artifact(&model, &ExportKind::JsonSchema, &View::whole()).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(ext, "json-schema.json");
        assert_eq!(
            doc["$defs"]["Dto.CreateOrderRequest"]["x-rdra-ish-kind"],
            "dto"
        );
        assert_eq!(doc["$defs"]["Entity.Order"]["x-rdra-ish-kind"], "entity");
    }

    #[test]
    fn export_er_text_formats_project_logical_data_model() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" { id: Int @pk }
relate(Order, Customer, "N:1")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let (mermaid, mermaid_ext) =
            export_artifact(&model, &ExportKind::MermaidEr, &View::whole()).unwrap();
        let (plantuml, plantuml_ext) =
            export_artifact(&model, &ExportKind::PlantumlEr, &View::whole()).unwrap();

        assert_eq!(mermaid_ext, "er.mmd");
        assert!(mermaid.contains("erDiagram"));
        assert!(mermaid.contains("Order }o--|| Customer"));
        assert_eq!(plantuml_ext, "er.puml");
        assert!(plantuml.contains("@startuml"));
        assert!(plantuml.contains("Order }o--|| Customer"));
    }

    #[test]
    fn list_dto_outputs_fields() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Dto, &ListFormat::Csv).unwrap();
        assert!(csv.contains("dto_id,dto_label,field_name,field_type,required"));
        assert!(csv.contains("CreateOrderRequest,Create order request,customer_id,Int,true"));
        assert!(csv.contains("CreateOrderRequest,Create order request,note,String,false"));
    }

    #[test]
    fn list_field_outputs_ui_metadata_and_entity_column_mapping() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
screen CheckoutScreen "Checkout screen"
field ShippingAddress "Shipping address" access editable required true source actor
entity Order "Order" {
  id: Int @pk
  shipping_address: String
}
contains(CheckoutScreen, ShippingAddress)
maps_field(ShippingAddress, Order, "shipping_address")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Field, &ListFormat::Csv).unwrap();
        assert!(csv.contains("field_id,field_label,access,required,source,entity_id,column_name"));
        assert!(csv.contains(
            "ShippingAddress,Shipping address,editable,true,actor,Order,shipping_address"
        ));
    }

    #[test]
    fn list_entity_outputs_data_modeling_metadata() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" {
  id: Int @pk
  tenant_id: Int @tenant
  total: Money @check("total >= 0")
  deleted_at: DateTime @null @soft_delete
  valid_from: DateTime @history
  net_total: Money @derived("total - discount")
}
relate(Order, Customer, "N:1").optional().on_delete(set_null).on_update(cascade)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Entity, &ListFormat::Csv).unwrap();
        assert!(csv.contains("fk_optional,fk_on_delete,fk_on_update"));
        assert!(csv.contains("tenant_id,Int,false,false,false,false,,false,,,false,,"));
        assert!(csv.contains("total,Money,false,false,false,false,,false,,,false,,total >= 0"));
        assert!(csv.contains("deleted_at,DateTime,false,false,false,false,,false,,,true,,"));
        assert!(csv.contains("valid_from,DateTime,false,false,false,false,,false,,,false,,"));
        assert!(csv.contains("net_total,Money,false,false,false,false,,false,,,false,,"));
        assert!(csv.contains(
            "customer_id,Int,false,false,false,true,Customer,true,set_null,cascade,true"
        ));
        assert!(csv.contains("total - discount"));
    }

    #[test]
    fn list_nfr_outputs_operational_metadata() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Nfr, &ListFormat::Csv).unwrap();
        assert!(csv.contains("id,label,metric,target,window,slo"));
        assert!(csv.contains("CheckoutLatency,Checkout latency,p95_latency_ms,<=300,5m,99.9%"));
        assert!(csv.contains("multi_az,retryable"));
    }

    #[test]
    fn list_constraint_outputs_audit_retention_privacy_metadata() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let csv = list_elements(&model, &ListKind::Constraint, &ListFormat::Csv).unwrap();
        assert!(csv.contains("AuditRetention,Audit retention"));
        assert!(csv.contains("enabled,structured,7y,restricted"));
    }

    #[test]
    fn list_conceptual_model_elements_separately_from_entities() {
        use rdra_ish_core::build_model;
        use rdra_ish_syntax::parse;

        let src = r#"
concept CarePlan "Care plan"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"
entity AppointmentTable "appointment table" { id: Int @pk }
maps_to(Appointment, AppointmentTable)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        let concept_csv = list_elements(&model, &ListKind::Concept, &ListFormat::Csv).unwrap();
        let domain_csv = list_elements(&model, &ListKind::DomainObject, &ListFormat::Csv).unwrap();
        let entity_csv = list_elements(&model, &ListKind::Entity, &ListFormat::Csv).unwrap();

        assert!(concept_csv.contains("CarePlan,Care plan"));
        assert!(domain_csv.contains("Appointment,Appointment"));
        assert!(entity_csv.contains("AppointmentTable,appointment table"));
        assert!(!entity_csv.contains("CarePlan"));
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
invokes(BookAppointment, BookingApi)
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
        assert!(output.contains("BookAppointment->BookingApi"));

        let json =
            list_elements(&model, &ListKind::PermissionCallables, &ListFormat::Json).unwrap();
        assert!(json.contains("\"usecase_api_paths\""));
        assert!(json.contains("BookAppointment->BookingApi"));
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
            flow_order_hint: None,
        });

        assert_eq!(
            message,
            "invariant violated: when status=booked then booked_at=present is broken by status=booked, booked_at=null"
        );
    }

    #[test]
    fn state_diag_message_includes_cross_scope_hint() {
        let message = state_diag_message(&rdra_ish_core::StateDiag::CrossInvariantViolated {
            entities: "Order, Payment".to_string(),
            guards: "Order.status=paid".to_string(),
            requireds: "Payment.status=captured".to_string(),
            pattern_desc: "Order(status=paid); Payment(status=pending)".to_string(),
            scope_hint: Some(
                "use .along(Order, Payment) if this rule is intended to apply only to linked instances"
                    .to_string(),
            ),
        });

        assert!(message.contains("cross-entity invariant violated across [Order, Payment]"));
        assert!(message.contains(
            "hint: use .along(Order, Payment) if this rule is intended to apply only to linked instances"
        ));
    }

    #[test]
    fn state_diag_message_formats_temporal_assertion_violation() {
        let message = state_diag_message(&rdra_ish_core::StateDiag::TemporalAssertionViolated {
            anchor: "ExecuteCertIssue".to_string(),
            requireds: "CertificateOrder.status=executed".to_string(),
            actual: "CertificateOrder.status has no immediate effect".to_string(),
        });

        assert_eq!(
            message,
            "temporal assertion violated after 'ExecuteCertIssue': expected CertificateOrder.status=executed, but CertificateOrder.status has no immediate effect"
        );
    }

    #[test]
    fn state_diag_message_formats_quantifier_not_evaluated() {
        let message =
            state_diag_message(&rdra_ish_core::StateDiag::QuantifierConstraintNotEvaluated {
                anchor: "ClientCertificate".to_string(),
                related: "TerminalCertAssignment".to_string(),
                constraint: "ClientCertificate when (status=revoked) none TerminalCertAssignment where (status=active)".to_string(),
                reason: "linked-instance cardinality is not represented in states".to_string(),
            });

        assert!(message.contains(
            "to-many quantifier constraint was not evaluated from 'ClientCertificate' to 'TerminalCertAssignment'"
        ));
        assert!(message.contains("linked-instance cardinality is not represented in states"));
    }
}
