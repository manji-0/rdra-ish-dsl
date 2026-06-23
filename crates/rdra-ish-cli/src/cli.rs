use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(ValueEnum, Clone)]
pub(crate) enum ListKind {
    Actor,
    Entity,
    Requirement,
    Adr,
    AdrImpact,
    Nfr,
    Quality,
    Constraint,
    Concept,
    #[value(alias = "domain_object")]
    DomainObject,
    Aggregate,
    #[value(alias = "valueobject")]
    ValueObject,
    Buc,
    Flow,
    Step,
    Usecase,
    Field,
    System,
    Api,
    Dto,
    /// Permission x callable UC/API list
    PermissionCallables,
    /// Actor x permission assignment audit inferred from UC/API requirements
    ActorPermissionAudit,
    /// Business-side input candidates inferred from BUC/use-case CRUD and API paths
    #[value(alias = "actor-inputs")]
    BusinessInputs,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum ListFormat {
    Table,
    Json,
    Csv,
}

#[derive(Parser)]
#[command(name = "rdra-ish", about = "RDRA DSL compiler")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Parse and type-check only (no output)
    Check {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
    },
    /// Generate diagram (PlantUML or Mermaid)
    Diagram {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Diagram kind: rdra, boundaryless-graph, er, state, sequence, event-flow, diff, business-area, or technical-area
        #[arg(long, default_value = "rdra")]
        kind: DiagramKind,
        /// Output format: puml, svg, png, or mermaid (mermaid outputs .mmd text only)
        #[arg(long, default_value = "puml")]
        format: OutputFormat,
        /// Filter to one or more BUCs (by id); repeatable (e.g. --buc A --buc B).
        /// The union of scoped nodes across all specified BUCs is shown.
        /// Applies to all diagram kinds. For sequence and business-area, only directly contained use cases are shown.
        #[arg(long)]
        buc: Vec<String>,
        /// Filter sequence and business-area diagrams to one or more use cases (by id); repeatable.
        #[arg(long)]
        usecase: Vec<String>,
        /// Baseline inputs for --kind diff. Repeat for multiple files/directories.
        #[arg(long = "diff-base")]
        diff_base: Vec<PathBuf>,
        /// Render element descriptions as diagram notes or annotations when supported.
        #[arg(long)]
        show_description: bool,
        /// Filter graph diagrams to one or more node kinds; repeatable.
        /// Applies to --kind rdra, --kind boundaryless-graph, and --kind diff.
        #[arg(long = "node-kind", alias = "kind-filter")]
        node_kind: Vec<String>,
        /// Filter graph diagrams to one or more edge/relation kinds; repeatable.
        /// Applies to --kind rdra, --kind boundaryless-graph, and --kind diff.
        #[arg(long = "edge-kind", alias = "edge-filter")]
        edge_kind: Vec<String>,
        /// Apply a graph view preset: business, system, data, api, or ui.
        #[arg(long)]
        view_preset: Option<DiagramViewPreset>,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// Generate CSV
    Csv {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// CSV kind: actor, entity, matrix, api, api-matrix, screen-constraints, permission-callables, actor-permission-audit, or business-inputs
        #[arg(long, default_value = "entity")]
        kind: CsvKind,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// List elements in human-readable form
    List {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Element kind to list: actor, entity, requirement, adr, adr-impact, nfr, quality, constraint, concept, domain-object, aggregate, value-object, buc, flow, step, usecase, field, system, api, dto, permission-callables, actor-permission-audit, or business-inputs
        #[arg(long, default_value = "actor")]
        kind: ListKind,
        /// Output format: table, json, csv
        #[arg(long, default_value = "table")]
        format: ListFormat,
    },
    /// Audit model coverage and review readiness
    Lint {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output format: table, json, csv
        #[arg(long, default_value = "table")]
        format: ListFormat,
    },
    /// Format RDRA DSL source files
    Fmt {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Rewrite files in place
        #[arg(long)]
        write: bool,
        /// Exit with status 1 when any file is not formatted
        #[arg(long)]
        check: bool,
    },
    /// Export machine-readable or review artifacts
    Export {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Export kind: openapi, asyncapi, dbml, json-schema, typescript-states, mermaid-er, or plantuml-er
        #[arg(long, default_value = "openapi")]
        kind: ExportKind,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// Derive reachable state patterns per entity (aggregated across BUCs)
    States {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output format: table, csv, json, or typescript
        #[arg(long, default_value = "table")]
        format: StatesFormat,
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
pub(crate) enum DiagramKind {
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
    /// Model graph diff between --diff-base and current inputs
    Diff,
    /// Business area diagram: Actor -> input field -> UseCase
    BusinessArea,
    /// Technical area diagram: System boxes containing only APIs and Entities
    TechnicalArea,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum DiagramViewPreset {
    Business,
    System,
    Data,
    Api,
    Ui,
}

#[derive(ValueEnum, Clone, PartialEq)]
pub(crate) enum OutputFormat {
    Puml,
    Svg,
    Png,
    Mermaid,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum StatesFormat {
    Table,
    Csv,
    Json,
    TypeScript,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum ExportKind {
    Openapi,
    Asyncapi,
    Dbml,
    JsonSchema,
    #[value(alias = "typescript-states")]
    TypeScriptStates,
    MermaidEr,
    PlantumlEr,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum CsvKind {
    Actor,
    Entity,
    Matrix,
    /// API list (id, label)
    Api,
    /// API x Entity CRUD matrix
    ApiMatrix,
    /// Screen x UC/API permission/medium constraints
    ScreenConstraints,
    /// Permission x callable UC/API list
    PermissionCallables,
    /// Actor x permission assignment audit inferred from UC/API requirements
    ActorPermissionAudit,
    /// Business-side input candidates inferred from BUC/use-case CRUD and API paths
    #[value(alias = "actor-inputs")]
    BusinessInputs,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_changed_subcommands_and_options() {
        let cli = Cli::try_parse_from([
            "rdra-ish",
            "diagram",
            "sample.rdra",
            "--kind",
            "business-area",
            "--format",
            "mermaid",
            "--buc",
            "BucScheduling",
            "--usecase",
            "BookAppointment",
            "--show-description",
            "--node-kind",
            "usecase",
            "--edge-kind",
            "invokes",
            "--view-preset",
            "api",
            "--out",
            "out.mmd",
        ])
        .unwrap();

        match cli.command {
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
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, DiagramKind::BusinessArea));
                assert!(matches!(format, OutputFormat::Mermaid));
                assert_eq!(buc, vec!["BucScheduling"]);
                assert_eq!(usecase, vec!["BookAppointment"]);
                assert!(diff_base.is_empty());
                assert!(show_description);
                assert_eq!(node_kind, vec!["usecase"]);
                assert_eq!(edge_kind, vec!["invokes"]);
                assert!(matches!(view_preset, Some(DiagramViewPreset::Api)));
                assert_eq!(out, PathBuf::from("out.mmd"));
            }
            _ => panic!("expected diagram command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "diagram",
            "target.rdra",
            "--kind",
            "diff",
            "--diff-base",
            "base.rdra",
            "--format",
            "mermaid",
        ])
        .unwrap();

        match cli.command {
            Commands::Diagram {
                inputs,
                kind,
                format,
                diff_base,
                ..
            } => {
                assert_eq!(inputs, vec![PathBuf::from("target.rdra")]);
                assert!(matches!(kind, DiagramKind::Diff));
                assert!(matches!(format, OutputFormat::Mermaid));
                assert_eq!(diff_base, vec![PathBuf::from("base.rdra")]);
            }
            _ => panic!("expected diagram command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "list",
            "sample.rdra",
            "--kind",
            "requirement",
            "--format",
            "json",
        ])
        .unwrap();

        match cli.command {
            Commands::List {
                inputs,
                kind,
                format,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ListKind::Requirement));
                assert!(matches!(format, ListFormat::Json));
            }
            _ => panic!("expected list command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "list",
            "sample.rdra",
            "--kind",
            "adr-impact",
            "--format",
            "csv",
        ])
        .unwrap();

        match cli.command {
            Commands::List {
                inputs,
                kind,
                format,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ListKind::AdrImpact));
                assert!(matches!(format, ListFormat::Csv));
            }
            _ => panic!("expected list command"),
        }

        let cli =
            Cli::try_parse_from(["rdra-ish", "lint", "sample.rdra", "--format", "csv"]).unwrap();

        match cli.command {
            Commands::Lint { inputs, format } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(format, ListFormat::Csv));
            }
            _ => panic!("expected lint command"),
        }

        let cli = Cli::try_parse_from(["rdra-ish", "fmt", "sample.rdra", "--write"]).unwrap();

        match cli.command {
            Commands::Fmt {
                inputs,
                write,
                check,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(write);
                assert!(!check);
            }
            _ => panic!("expected fmt command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "list",
            "sample.rdra",
            "--kind",
            "actor-inputs",
            "--format",
            "json",
        ])
        .unwrap();

        match cli.command {
            Commands::List {
                inputs,
                kind,
                format,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ListKind::BusinessInputs));
                assert!(matches!(format, ListFormat::Json));
            }
            _ => panic!("expected list command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "csv",
            "sample.rdra",
            "--kind",
            "business-inputs",
            "--out",
            "inputs.csv",
        ])
        .unwrap();

        match cli.command {
            Commands::Csv { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, CsvKind::BusinessInputs));
                assert_eq!(out, PathBuf::from("inputs.csv"));
            }
            _ => panic!("expected csv command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "export",
            "sample.rdra",
            "--kind",
            "dbml",
            "--out",
            "schema.dbml",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::Dbml));
                assert_eq!(out, PathBuf::from("schema.dbml"));
            }
            _ => panic!("expected export command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "export",
            "sample.rdra",
            "--kind",
            "asyncapi",
            "--out",
            "events.json",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::Asyncapi));
                assert_eq!(out, PathBuf::from("events.json"));
            }
            _ => panic!("expected export command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "export",
            "sample.rdra",
            "--kind",
            "json-schema",
            "--out",
            "schemas.json",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::JsonSchema));
                assert_eq!(out, PathBuf::from("schemas.json"));
            }
            _ => panic!("expected export command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "export",
            "sample.rdra",
            "--kind",
            "mermaid-er",
            "--out",
            "er.mmd",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::MermaidEr));
                assert_eq!(out, PathBuf::from("er.mmd"));
            }
            _ => panic!("expected export command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "export",
            "sample.rdra",
            "--kind",
            "plantuml-er",
            "--out",
            "er.puml",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { inputs, kind, out } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::PlantumlEr));
                assert_eq!(out, PathBuf::from("er.puml"));
            }
            _ => panic!("expected export command"),
        }
    }
}
