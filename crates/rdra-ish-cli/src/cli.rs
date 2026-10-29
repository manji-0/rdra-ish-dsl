use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::ErrorKind;
use clap::{Arg, Args, Command, Parser, Subcommand, ValueEnum};
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::path::PathBuf;

/// Parses a `ValueEnum`, and answers a value the CLI no longer accepts with what to use
/// instead rather than clap's generic "invalid value".
#[derive(Clone)]
pub(crate) struct WithRemoved<E> {
    /// Each removed value with the hint that follows "is no longer supported;".
    removed: &'static [(&'static str, &'static str)],
    values: PhantomData<E>,
}

impl<E> WithRemoved<E> {
    pub(crate) const fn new(removed: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            removed,
            values: PhantomData,
        }
    }
}

impl<E: ValueEnum + Clone + Send + Sync + 'static> TypedValueParser for WithRemoved<E> {
    type Value = E;

    fn parse_ref(&self, cmd: &Command, arg: Option<&Arg>, value: &OsStr) -> Result<E, clap::Error> {
        let removed = value
            .to_str()
            .and_then(|value| self.removed.iter().find(|(name, _)| *name == value));
        if let Some((name, hint)) = removed {
            let flag = arg.and_then(Arg::get_long).unwrap_or_default();
            return Err(clap::Error::raw(
                ErrorKind::InvalidValue,
                format!("`--{flag} {name}` is no longer supported; {hint}\n"),
            )
            .with_cmd(cmd));
        }
        clap::builder::EnumValueParser::<E>::new().parse_ref(cmd, arg, value)
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            E::value_variants()
                .iter()
                .filter_map(ValueEnum::to_possible_value),
        ))
    }
}

const MERMAID_ONLY: &str = "rdra-ish writes Mermaid only (`.mmd`). Drop `--format` (Mermaid is the default) or pass `--format mermaid`; to get an image, render the `.mmd` with `mmdc` or hand it to a tool that draws Mermaid";

const REMOVED_FORMATS: &[(&str, &str)] = &[
    ("puml", MERMAID_ONLY),
    ("svg", MERMAID_ONLY),
    ("png", MERMAID_ONLY),
];

const REMOVED_DIAGRAM_KINDS: &[(&str, &str)] = &[
    (
        "boundaryless-graph",
        "use `--kind rdra`, which draws the same elements and relations in the RDRA layers; narrow it with `--node-kind` / `--edge-kind`",
    ),
    (
        "business-area",
        "use `csv --kind business-inputs`, which lists the same inferred actor input fields per use case",
    ),
    (
        "technical-area",
        "use `csv --kind api-matrix` for the API x entity CRUD, or `--kind rdra --node-kind system --node-kind api --node-kind entity`",
    ),
];

const REMOVED_EXPORT_KINDS: &[(&str, &str)] =
    &[("plantuml-er", "use `--kind mermaid-er` (writes `er.mmd`)")];

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
    Buc,
    Flow,
    Step,
    Usecase,
    Field,
    System,
    Api,
    Dto,
    Variation,
    Condition,
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
        /// Treat permission / access consistency gaps as errors (exit 1)
        #[arg(long)]
        strict: bool,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Generate a Mermaid diagram (.mmd)
    Diagram {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Diagram kind: rdra, er, state, sequence, event-flow, or diff
        #[arg(long, default_value = "rdra", value_parser = WithRemoved::<DiagramKind>::new(REMOVED_DIAGRAM_KINDS))]
        kind: DiagramKind,
        /// Output format: mermaid, the only one (writes <out>.mmd)
        #[arg(long, default_value = "mermaid", value_parser = WithRemoved::<OutputFormat>::new(REMOVED_FORMATS))]
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
        /// Applies to --kind rdra and --kind diff.
        #[arg(long = "node-kind", alias = "kind-filter")]
        node_kind: Vec<String>,
        /// Filter graph diagrams to one or more edge/relation kinds; repeatable.
        /// Applies to --kind rdra and --kind diff.
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
        /// Element kind to list: actor, entity, requirement, adr, adr-impact, nfr, quality, constraint, buc, flow, step, usecase, field, system, api, dto, variation, condition, permission-callables, actor-permission-audit, or business-inputs
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
    /// Print the .rdra files the inputs load, one per line, in load order
    Files {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
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
        /// Export kind: openapi, asyncapi, dbml, json-schema, typescript-states, mermaid-er, or tla
        #[arg(long, default_value = "openapi", value_parser = WithRemoved::<ExportKind>::new(REMOVED_EXPORT_KINDS))]
        kind: ExportKind,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
        /// TLA+ multi-instance `InstanceCount` (export --kind tla / verify only)
        #[arg(long, default_value_t = 2)]
        instance_count: u32,
        /// TLA+ `IntRange == 0..N` upper bound (export --kind tla / verify only)
        #[arg(long, default_value_t = 5)]
        int_range_max: u32,
    },
    /// Derive reachable state patterns per entity (aggregated across BUCs)
    States {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output format: table, csv, json, or typescript
        #[arg(long, default_value = "table")]
        format: StatesFormat,
        /// Cap on each entity's distinct patterns within one target cone (that cone also stops
        /// after 16 joint states per allowed pattern)
        #[arg(long, default_value_t = 256)]
        max_patterns: usize,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Formally verify temporal/safety properties via an external checker
    Verify {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Verification backend
        #[arg(long, default_value = "tlc")]
        backend: VerifyBackend,
        /// Directory to write generated TLA+/CFG (defaults to a temp dir)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// TLA+ multi-instance `InstanceCount`
        #[arg(long, default_value_t = 2)]
        instance_count: u32,
        /// TLA+ `IntRange == 0..N` upper bound
        #[arg(long, default_value_t = 5)]
        int_range_max: u32,
        #[command(flatten)]
        scope: ScopeArgs,
    },
}

/// The part of the model `check`, `states`, and `verify` cover.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ScopeArgs {
    /// Keep only the use cases reachable from this BUC (by id); repeatable. Whether a state is
    /// reachable, a use case runs, or an instance gets stuck is then not decided.
    #[arg(long)]
    pub(crate) buc: Vec<String>,
    /// Check only this entity's view and the invariants and after-asserts that read it (by
    /// id); repeatable. Build errors on entities no checked cone reads are deferred.
    #[arg(long)]
    pub(crate) entity: Vec<String>,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum VerifyBackend {
    /// TLA+ model checker (TLC)
    Tlc,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum DiagramKind {
    /// RDRA layered graph mapped onto the original RDRA-style structure
    Rdra,
    Er,
    State,
    /// Write-focused sequence diagram with FK-inferred transaction boundaries
    Sequence,
    /// Event-flow diagram: UC --raises--> Event --triggers--> UC / --transitions--> State
    EventFlow,
    /// Model graph diff between --diff-base and current inputs
    Diff,
}

impl DiagramKind {
    /// Whether a lifecycle error makes the diagram wrong. ER draws only the data layer, as
    /// `export --kind mermaid-er` does; the other kinds keep the gate.
    pub(crate) fn reads_lifecycle(&self) -> bool {
        !matches!(self, DiagramKind::Er)
    }
}

#[derive(ValueEnum, Clone)]
pub(crate) enum DiagramViewPreset {
    Business,
    System,
    Data,
    Api,
    Ui,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum OutputFormat {
    Mermaid,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum StatesFormat {
    Table,
    Csv,
    Json,
    #[value(name = "typescript", alias = "type-script")]
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
    /// TLA+ / TLC specification (writes companion `.cfg` next to `.tla`)
    Tla,
}

impl ExportKind {
    /// Whether the output prints the derived lifecycle, so a lifecycle error makes it wrong.
    /// TLA is not one: it builds its own system and reports violations through TLC.
    pub(crate) fn reads_lifecycle(&self) -> bool {
        matches!(self, ExportKind::TypeScriptStates)
    }
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
            "sequence",
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
                assert!(matches!(kind, DiagramKind::Sequence));
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
            Commands::Export {
                inputs, kind, out, ..
            } => {
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
            Commands::Export {
                inputs, kind, out, ..
            } => {
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
            Commands::Export {
                inputs,
                kind,
                out,
                instance_count,
                int_range_max,
            } => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(matches!(kind, ExportKind::JsonSchema));
                assert_eq!(out, PathBuf::from("schemas.json"));
                assert_eq!(instance_count, 2);
                assert_eq!(int_range_max, 5);
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
            Commands::Export { kind, out, .. } => {
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
            "tla",
            "--out",
            "/tmp/rdra-tla",
            "--instance-count",
            "3",
            "--int-range-max",
            "9",
        ])
        .unwrap();

        match cli.command {
            Commands::Export {
                kind,
                instance_count,
                int_range_max,
                ..
            } => {
                assert!(matches!(kind, ExportKind::Tla));
                assert_eq!(instance_count, 3);
                assert_eq!(int_range_max, 9);
            }
            _ => panic!("expected export command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "verify",
            "sample.rdra",
            "--backend",
            "tlc",
            "--instance-count",
            "4",
            "--int-range-max",
            "7",
        ])
        .unwrap();

        match cli.command {
            Commands::Verify {
                instance_count,
                int_range_max,
                ..
            } => {
                assert_eq!(instance_count, 4);
                assert_eq!(int_range_max, 7);
            }
            _ => panic!("expected verify command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "states",
            "sample.rdra",
            "--format",
            "type-script",
        ])
        .unwrap();
        match cli.command {
            Commands::States { format, .. } => {
                assert!(matches!(format, StatesFormat::TypeScript));
            }
            _ => panic!("expected states command"),
        }

        let cli = Cli::try_parse_from([
            "rdra-ish",
            "states",
            "sample.rdra",
            "--format",
            "typescript",
        ])
        .unwrap();
        match cli.command {
            Commands::States { format, .. } => {
                assert!(matches!(format, StatesFormat::TypeScript));
            }
            _ => panic!("expected states command"),
        }
    }

    fn parse_error(args: &[&str]) -> String {
        match Cli::try_parse_from(args) {
            Ok(_) => panic!("{args:?} parsed"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn removed_plantuml_values_name_their_replacement() {
        for format in ["puml", "svg", "png"] {
            let message = parse_error(&["rdra-ish", "diagram", "m.rdra", "--format", format]);
            assert!(
                message.contains(&format!("`--format {format}` is no longer supported"))
                    && message.contains("`--format mermaid`"),
                "{message}"
            );
        }
        let message = parse_error(&["rdra-ish", "export", "m.rdra", "--kind", "plantuml-er"]);
        assert!(
            message.contains("`--kind plantuml-er` is no longer supported")
                && message.contains("`--kind mermaid-er`"),
            "{message}"
        );
    }

    #[test]
    fn removed_diagram_kinds_name_their_replacement() {
        for (kind, replacement) in [
            ("boundaryless-graph", "`--kind rdra`"),
            ("business-area", "`csv --kind business-inputs`"),
            ("technical-area", "`csv --kind api-matrix`"),
        ] {
            let message = parse_error(&["rdra-ish", "diagram", "m.rdra", "--kind", kind]);
            assert!(
                message.contains(&format!("`--kind {kind}` is no longer supported"))
                    && message.contains(replacement),
                "{message}"
            );
        }
        let message = parse_error(&["rdra-ish", "export", "m.rdra", "--kind", "plantuml-er"]);
        assert!(
            message.contains("`--kind plantuml-er` is no longer supported")
                && message.contains("`--kind mermaid-er`"),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_value_keeps_the_generic_error() {
        let message = parse_error(&["rdra-ish", "diagram", "m.rdra", "--format", "gif"]);
        assert!(message.contains("invalid value 'gif'"), "{message}");
        assert!(message.contains("[possible values: mermaid]"), "{message}");
    }
}
