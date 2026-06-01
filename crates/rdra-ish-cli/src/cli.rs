use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(ValueEnum, Clone)]
pub(crate) enum ListKind {
    Actor,
    Entity,
    Buc,
    Usecase,
    System,
    Api,
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
        /// Diagram kind: rdra, boundaryless-graph, er, state, sequence, event-flow, business-area, or technical-area
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
        /// Element kind to list: actor, entity, buc, usecase, system, api, permission-callables, actor-permission-audit, or business-inputs
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
    /// Business area diagram: Actor -> input field -> UseCase
    BusinessArea,
    /// Technical area diagram: System boxes containing only APIs and Entities
    TechnicalArea,
}

#[derive(ValueEnum, Clone, PartialEq)]
pub(crate) enum OutputFormat {
    Puml,
    Svg,
    Png,
    Mermaid,
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
                out,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, DiagramKind::BusinessArea));
                assert!(matches!(format, OutputFormat::Mermaid));
                assert_eq!(buc, vec!["BucScheduling"]);
                assert_eq!(usecase, vec!["BookAppointment"]);
                assert_eq!(out, PathBuf::from("out.mmd"));
            }
            _ => panic!("expected diagram command"),
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
    }
}
