use anyhow::{Context, Result};
use clap::Parser;
mod cli;
mod list_output;
mod load;
use cli::{Cli, Commands, CsvKind, DiagramKind, ExportKind, OutputFormat, StatesFormat};
use list_output::{consistency_warnings, filter_entity_output, format_lint_issues, list_elements};
use load::{collect_rdra_files, diagram_preset_filters, eprint_diagnostic, load_model};
use rdra_ish_core::{lint_issues, LintSeverity};
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
    typescript::TypeScriptStateUnionEmitter,
    Emitter, Filter, Scope, View,
};
use rdra_ish_syntax::format_source;
use std::fs;

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
        } => {
            let (program, model, diags) = load_model(&inputs)?;

            for diag in &diags {
                eprint_diagnostic(&program, diag);
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
                        let (_program, base_model, _) = load_model(&diff_base)?;
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
                        let (_program, base_model, _) = load_model(&diff_base)?;
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
            let (program, model, diags) = load_model(&inputs)?;

            for diag in &diags {
                eprint_diagnostic(&program, diag);
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
                    .map_err(|err| anyhow::anyhow!("parse errors: {:?}", err.parse_errors))
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
        } => {
            let (program, model, diags) = load_model(&inputs)?;

            for diag in &diags {
                eprint_diagnostic(&program, diag);
            }

            let view = View::bucs(buc);

            let output = match format {
                StatesFormat::Table => {
                    let emitter = StatePatternTableEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
                StatesFormat::Csv => {
                    let emitter = StatePatternCsvEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
                StatesFormat::Json => {
                    let emitter = StatePatternJsonEmitter { cap: max_patterns };
                    emitter.emit(&model, &view)?
                }
                StatesFormat::TypeScript => {
                    let emitter = TypeScriptStateUnionEmitter { cap: max_patterns };
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
        ExportKind::TypeScriptStates => Ok((
            TypeScriptStateUnionEmitter::default().emit(model, view)?,
            "entity-states.ts",
        )),
        ExportKind::MermaidEr => Ok((ErMermaidEmitter.emit(model, view)?, "er.mmd")),
        ExportKind::PlantumlEr => Ok((ErPlantUmlEmitter.emit(model, view)?, "er.puml")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ListFormat, ListKind};
    use crate::list_output::state_diag_message;
    use rdra_ish_core::{format_diagnostic_message, SemanticModel};
    use std::path::PathBuf;

    #[test]
    fn load_model_rejects_inputs_without_rdra_files() {
        let err = load_model(&[PathBuf::from("missing-input")]).unwrap_err();

        assert_eq!(err.to_string(), "no .rdra files found in the given inputs");
    }

    fn errors_fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(path)
    }

    #[test]
    fn check_command_includes_source_location() {
        let inputs = vec![errors_fixture("tests/fixtures/errors/type_mismatch.rdra")];
        let (program, _, diags) = load_model(&inputs).expect("load model");
        let mismatch = diags
            .iter()
            .find(|d| matches!(&d.error, rdra_ish_core::RdraError::TypeMismatch { .. }))
            .expect("type mismatch diagnostic");
        assert!(
            mismatch.location.is_some(),
            "expected location on diagnostic: {:?}",
            diags
        );
        let message = format_diagnostic_message(
            Some(&program),
            mismatch.is_warning,
            mismatch.location.as_ref(),
            &mismatch.error.to_string(),
        );
        assert!(
            message.contains("type_mismatch.rdra"),
            "cli diagnostic should include file path: {message}"
        );
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

        let formatted = rdra_ish_syntax::format_source(src).unwrap();

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
