use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::ErrorKind;
use clap::{Arg, Args, Command, Parser, Subcommand, ValueEnum};
use rdra_ish_core::lifecycle::Caps;
use rdra_ish_core::memory::MemorySize;
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
    (
        "event-flow",
        "use `--kind event`, which draws the same raises and triggers links; the transitions a triggered use case drives are in `--kind state`",
    ),
    (
        "diff",
        "write `--kind rdra` for both models and compare the two `.mmd` files with `diff` or `git diff`",
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
    Goal,
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
    Screen,
    Event,
    Timer,
    Condition,
    /// Business rules: every constraint predicate and condition, with its label, formula and site
    Rule,
    /// Permission x callable UC/API list
    PermissionCallables,
    /// Actor x permission assignment audit inferred from UC/API requirements
    ActorPermissionAudit,
    /// Business-side input candidates inferred from BUC/use-case CRUD and API paths
    #[value(alias = "actor-inputs")]
    BusinessInputs,
    /// Actors and external systems by BUC (ordered by business): how each takes part, as a table
    BucRoles,
    /// State transitions: entity, column, from, to, use case, guards, actors and site, one per row
    Transition,
    /// The published surface (`exposes` and `shares`) as canonical text; takes no `--format`
    Surface,
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum NextFormat {
    Text,
    Json,
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CsvFormat {
    Csv,
    Markdown,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum ListFormat {
    Table,
    Json,
    Csv,
    /// A Markdown table, to paste into a design doc or a PRD.
    Markdown,
}

#[derive(Parser)]
#[command(name = "rdra-ish", about = "RDRA DSL compiler", version)]
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
        /// Treat a deprecated form (a `DeprecatedSyntax` warning) as an error (exit 1)
        #[arg(long)]
        deny_deprecated: bool,
        /// Cap on each entity's distinct patterns within one target cone; past it is a
        /// `PatternCapReached` error. Default: 2^(10 + 8 * (n - 1)) for n = --instance-count
        /// (1024 at one instance, 262144 at two)
        #[arg(long)]
        max_patterns: Option<usize>,
        /// Cap on the joint states of each target cone; a cone past it is a
        /// `StateCapReached` error. Default: four times the default --max-patterns, at least
        /// 65536 (65536 at one instance, 1048576 at two)
        #[arg(long)]
        max_states: Option<usize>,
        /// What the run may hold in memory, such as 4GiB or 512MiB (G and GB also mean GiB);
        /// a cone whose states pass it stops, as at --max-states. Default: `memory` under
        /// [check] in rdra.toml, else 4GiB
        #[arg(long, value_name = "SIZE")]
        memory: Option<MemorySize>,
        #[command(flatten)]
        bounds: BoundsArgs,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Generate a Mermaid diagram (.mmd)
    Diagram(DiagramArgs),
    /// Generate CSV
    Csv {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// CSV kind: actor, entity, matrix, api, api-matrix, screen-constraints, permission-callables, actor-permission-audit, or business-inputs
        #[arg(long, default_value = "entity")]
        kind: CsvKind,
        /// Output format: csv, or markdown for a table to paste into a document (writes <out>.md)
        #[arg(long, default_value = "csv")]
        format: CsvFormat,
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// List elements in human-readable form
    List {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Element kind to list: actor, entity, requirement, adr, adr-impact, goal, nfr, quality, constraint, buc, flow, step, usecase, field, system, api, dto, variation, screen, event, timer, condition, rule, permission-callables, actor-permission-audit, business-inputs, buc-roles, transition, or surface
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
    /// Show the incremental modeling stage of each BUC, and what the next stage asks for
    Next {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Show only these BUCs (by id); repeatable
        #[arg(long)]
        buc: Vec<String>,
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: NextFormat,
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
        /// Export kind: openapi, asyncapi, dbml, sql, json-schema, typescript-states, mermaid-er, or tla
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
        /// SQL dialect (export --kind sql only)
        #[arg(long, default_value = "postgres")]
        dialect: SqlDialect,
    },
    /// [experimental] Print the model's domain types as TypeScript, Rust, or Go code for an
    /// agent to finish. Outside the compatibility contract: may change or go away in any
    /// release
    Codegen {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Language: ts, rust, or go
        #[arg(long)]
        lang: CodegenLang,
        /// Write to this file instead of stdout
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Go package name (--lang go only)
        #[arg(long, default_value = "domain")]
        package: String,
        /// Generate even when `check` reports errors: the state types then narrow no column,
        /// and each error is listed as an obligation the model does not meet
        #[arg(long)]
        allow_failing: bool,
    },
    /// Derive reachable state patterns per entity (aggregated across BUCs)
    States {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output format: table, csv, json, or typescript
        #[arg(long, default_value = "table")]
        format: StatesFormat,
        /// Cap on each entity's distinct patterns within one target cone; past it is a
        /// `PatternCapReached` error. Default: 2^(10 + 8 * (n - 1)) for n = --instance-count
        /// (1024 at one instance, 262144 at two)
        #[arg(long)]
        max_patterns: Option<usize>,
        /// Cap on the joint states of each target cone; a cone past it is a
        /// `StateCapReached` error. Default: four times the default --max-patterns, at least
        /// 65536 (65536 at one instance, 1048576 at two)
        #[arg(long)]
        max_states: Option<usize>,
        /// What the run may hold in memory, such as 4GiB or 512MiB (G and GB also mean GiB);
        /// a cone whose states pass it stops, as at --max-states. Default: `memory` under
        /// [check] in rdra.toml, else 4GiB
        #[arg(long, value_name = "SIZE")]
        memory: Option<MemorySize>,
        #[command(flatten)]
        bounds: BoundsArgs,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Formally verify every invariant, after-assert, and temporal property, natively; with
    /// --with-tlc TLC runs beside the native checker
    Verify {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Also run TLC on the same cones beside the native checker; a check the two decide
        /// differently fails the run (exit 1). Needs `tlc` on PATH or --tlc-jar
        #[arg(long)]
        with_tlc: bool,
        /// [deprecated] `--backend tlc` is a spelling of --with-tlc, removed in the next major
        /// release
        #[arg(long, value_name = "BACKEND")]
        backend: Option<VerifyBackend>,
        /// Directory to write the generated TLA+/CFG modules and cones.txt to, for a TLC run
        /// by hand (without it, --with-tlc writes them to a temp dir)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// TLA+ multi-instance `InstanceCount`
        #[arg(long, default_value_t = 2)]
        instance_count: u32,
        /// TLA+ `IntRange == 0..N` upper bound
        #[arg(long, default_value_t = 5)]
        int_range_max: u32,
        /// Cap on each entity's distinct patterns within one cone; past it the cone is not
        /// decided. Default: 256
        #[arg(long)]
        max_patterns: Option<usize>,
        /// Cap on the joint states of every BFS run: each entity, safety, and property cone,
        /// and the whole-model run. A run past it is not decided. Default: none; --memory
        /// stops a run (and --temporal-budget the whole-model one). Under --with-tlc the
        /// whole-model TLC run keeps a cap of 2097152 (2^21) without it
        #[arg(long)]
        max_states: Option<usize>,
        /// Seconds a temporal property's whole-model run gets when its cone does not decide
        /// it; past them the property is reported as not decided, unless the run had already
        /// found a counterexample
        #[arg(long, default_value_t = 300)]
        temporal_budget: u64,
        /// Treat a finding verify leaves undecided (a BFS cone stopped at the state cap or the
        /// memory limit, a temporal property past `--temporal-budget`) as an error (exit 1)
        #[arg(long)]
        deny_undecided: bool,
        /// [experimental] Also try to prove each state rule for any number of rows and any Int value with z3
        /// (SMT inductive invariants); prints proved, unproved with a counterexample to
        /// induction, or out of scope per rule
        #[arg(long)]
        prove: bool,
        /// With --prove, treat a rule left unproved as an error (exit 1)
        #[arg(long, requires = "prove")]
        deny_unproved: bool,
        /// The z3 binary --prove runs; `RDRA_Z3` sets it when the flag is absent, else `z3`
        /// on PATH
        #[arg(long, value_name = "PATH", requires = "prove")]
        z3: Option<PathBuf>,
        /// With --with-tlc, run TLC as `java -cp <path> tlc2.TLC` from `tla2tools.jar` instead
        /// of a `tlc` / `tlc2` launcher on PATH; `RDRA_TLC_JAR` sets it when the flag is absent
        #[arg(long, value_name = "PATH")]
        tlc_jar: Option<PathBuf>,
        /// What the run may hold in memory, such as 16GiB (G and GB also mean GiB); under
        /// --with-tlc BFS takes half, and the TLC runs at once take no more `-Xmx` than the
        /// rest. Default: `memory` under [verify] in rdra.toml, else 16GiB
        #[arg(long, value_name = "SIZE")]
        memory: Option<MemorySize>,
        #[command(flatten)]
        scope: ScopeArgs,
    },
}

/// The size of the system `check` and `states` explore. The defaults are `Bounds::STATES`;
/// `verify` and `export --kind tla` default to two instances instead.
#[derive(Args)]
pub(crate) struct DiagramArgs {
    #[arg(required = true)]
    pub(crate) inputs: Vec<PathBuf>,
    /// Diagram kind: rdra, context, requirement, business, usecase, robustness, sequence, flow, screen,
    /// information, condition, er, state, or event
    #[arg(long, default_value = "rdra", value_parser = WithRemoved::<DiagramKind>::new(REMOVED_DIAGRAM_KINDS))]
    pub(crate) kind: DiagramKind,
    /// Write the review set under --out, a directory: the whole-model context, requirement,
    /// business, information, condition, er, screen, event, and state index diagrams, each
    /// BUC's usecase, robustness, flow, and sequence diagrams, and each entity's state
    /// models, as `<kind>.mmd` and `<kind>__<id>.mmd`. A diagram with nothing to draw is
    /// left out. Not together with --kind or a scope.
    #[arg(long)]
    pub(crate) all: bool,
    /// Output format: mermaid, the only one (prints Mermaid text)
    #[arg(long, default_value = "mermaid", value_parser = WithRemoved::<OutputFormat>::new(REMOVED_FORMATS))]
    pub(crate) format: OutputFormat,
    /// Filter to one or more BUCs (by id); repeatable (e.g. --buc A --buc B).
    /// The union of scoped nodes across all specified BUCs is shown.
    /// Applies to all diagram kinds.
    #[arg(long)]
    pub(crate) buc: Vec<String>,
    /// Draw only these use cases (by id); repeatable. Applies to --kind usecase, robustness, and
    /// sequence (which takes exactly one, or --buc for a BUC's use cases in order). One of
    /// --buc, --usecase, --entity, and --actor at a time.
    #[arg(long)]
    pub(crate) usecase: Vec<String>,
    /// Draw by entity (by id); repeatable. --kind state: these entities' state models, every
    /// lifecycle column. --kind er and information: these entities and the ones a `relate`
    /// joins them to. --kind usecase and robustness: the use cases that read or write them.
    #[arg(long)]
    pub(crate) entity: Vec<String>,
    /// Draw the use cases these actors perform (by id); repeatable. Applies to --kind usecase
    /// and robustness.
    #[arg(long)]
    pub(crate) actor: Vec<String>,
    /// Filter graph diagrams to one or more node kinds; repeatable.
    /// Applies to --kind rdra.
    #[arg(long = "node-kind", alias = "kind-filter")]
    pub(crate) node_kind: Vec<String>,
    /// Filter graph diagrams to one or more edge/relation kinds; repeatable.
    /// Applies to --kind rdra.
    #[arg(long = "edge-kind", alias = "edge-filter")]
    pub(crate) edge_kind: Vec<String>,
    /// Open the text with a Mermaid `title`: the diagram's name and what it is scoped to. A
    /// renderer shows it; the `%%` header, which says the same, it drops.
    #[arg(long)]
    pub(crate) title: bool,
    /// Add a `凡例` box to a flowchart diagram: one node per node kind, in its colour.
    #[arg(long)]
    pub(crate) legend: bool,
    /// Write the Mermaid text to this file (`.mmd` is always the extension). Without it,
    /// the text goes to stdout. With --all, the directory the files go to.
    #[arg(short, long)]
    pub(crate) out: Option<PathBuf>,
}

#[derive(Args, Clone, Debug)]
pub(crate) struct BoundsArgs {
    /// Instances (slots) per entity
    #[arg(long, default_value_t = 1)]
    pub(crate) instance_count: u32,
    /// The Int domain is `0..=N`
    #[arg(long, default_value_t = 5)]
    pub(crate) int_range_max: u32,
}

impl BoundsArgs {
    /// The BFS caps: the defaults for `--instance-count`, each replaced by the flag given.
    pub(crate) fn caps(&self, max_patterns: Option<usize>, max_states: Option<usize>) -> Caps {
        let default = Caps::for_instances(self.instance_count);
        Caps::new(
            max_patterns.unwrap_or(default.patterns),
            max_states.unwrap_or(default.states),
        )
    }

    pub(crate) fn bounds(&self) -> anyhow::Result<rdra_ish_core::lifecycle::Bounds> {
        crate::export::checked_bounds(self.instance_count, self.int_range_max)
    }
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
    /// Assume this use case never runs and leave it out (by id); repeatable. Unlike `--buc`,
    /// witnesses, deadlocks, and use cases that never run are still decided
    #[arg(long, value_name = "USECASE")]
    pub(crate) without_usecase: Vec<String>,
    /// Assume the use cases this BUC reaches, and no other BUC does, never run (by id);
    /// repeatable
    #[arg(long, value_name = "BUC")]
    pub(crate) without_buc: Vec<String>,
    /// Keep only the use cases this actor performs and those no actor performs (by id);
    /// repeatable. Every other use case is assumed never to run
    #[arg(long, value_name = "ACTOR")]
    pub(crate) actor: Vec<String>,
}

impl ScopeArgs {
    /// The scope these flags name, each id checked against the model.
    pub(crate) fn resolve(
        &self,
        model: &rdra_ish_core::SemanticModel,
    ) -> Result<rdra_ish_core::lifecycle::Scope, rdra_ish_core::lifecycle::UnknownId> {
        rdra_ish_core::lifecycle::Scope::parse(model, &self.buc, &self.entity)?.without(
            model,
            &rdra_ish_core::lifecycle::Without {
                usecases: &self.without_usecase,
                bucs: &self.without_buc,
                actors: &self.actor,
            },
        )
    }
}

#[derive(ValueEnum, Clone)]
pub(crate) enum VerifyBackend {
    /// TLA+ model checker (TLC), beside the native checker: the same as --with-tlc
    Tlc,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum DiagramKind {
    /// RDRA layered graph mapped onto the original RDRA-style structure
    Rdra,
    /// システムコンテキスト: the system with the actors who use it and the external
    /// systems it exchanges events with
    Context,
    /// 要求モデル: the requirements with the BUCs they motivate, and the NFRs and
    /// constraints with what they apply to or constrain and their qualities
    Requirement,
    /// ビジネスコンテキスト / BUC: each business with its BUCs, and the actors and external
    /// systems each BUC involves
    Business,
    /// UC複合図: each use case with its actors, screens, information, events, external
    /// systems, timers, conditions, and state transitions
    Usecase,
    /// ロバストネス図: ICONIX boundary / control / entity of each use case, drawn actor ->
    /// boundary -> control -> entity
    Robustness,
    /// シーケンス図: one use case in order, from the starter through the guards, the entity
    /// writes, and the events it raises; needs exactly one --usecase
    Sequence,
    /// 業務フロー: each flow with its steps in order, their branches, exceptions, and
    /// repeats, what each step covers, and a lane per actor when steps are performed
    Flow,
    /// 画面遷移図: each screen, a solid edge for each `navigates` labelled with its `.via`
    /// use case, and a dashed 業務上の順番 edge for the order a business flow uses the
    /// screens in, one actor's lane at a time, where no `navigates` is declared
    Screen,
    /// 情報モデル: the entities with their relate links, state models, and variations,
    /// without columns
    Information,
    /// バリエーション・条件: each condition with the variations and state models it combines
    /// and what it applies to, and each variation with its values
    Condition,
    /// ER図: the entities with their columns, keys, and the cardinality of each `relate`
    Er,
    /// 状態モデル: each state model with its states and the use case of each transition;
    /// without --entity or --buc, an index of the state models
    State,
    /// System-to-system event links: UC / ExtSystem --raises--> Event --triggers--> UC / BUC /
    /// ExtSystem, and Timer --triggers--> UC / BUC
    Event,
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

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CodegenLang {
    #[value(name = "ts", alias = "typescript")]
    Ts,
    Rust,
    Go,
}

#[derive(ValueEnum, Clone)]
pub(crate) enum ExportKind {
    Openapi,
    Asyncapi,
    Dbml,
    /// SQL DDL: tables, keys, indexes, and CHECK constraints from entity-local rules
    Sql,
    JsonSchema,
    #[value(alias = "typescript-states")]
    TypeScriptStates,
    MermaidEr,
    /// TLA+ / TLC specification (writes companion `.cfg` next to `.tla`)
    Tla,
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum SqlDialect {
    Postgres,
    Sqlite,
}

impl SqlDialect {
    pub(crate) fn dialect(self) -> rdra_ish_emit::sql::Dialect {
        match self {
            SqlDialect::Postgres => rdra_ish_emit::sql::Dialect::Postgres,
            SqlDialect::Sqlite => rdra_ish_emit::sql::Dialect::Sqlite,
        }
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
    fn check_and_states_caps_follow_the_instance_count_unless_given() {
        fn caps(args: &[&str]) -> (usize, usize) {
            let mut argv = vec!["rdra-ish", args[0], "m.rdra"];
            argv.extend_from_slice(&args[1..]);
            let (bounds, max_patterns, max_states) =
                match Cli::try_parse_from(argv).unwrap().command {
                    Commands::Check {
                        bounds,
                        max_patterns,
                        max_states,
                        ..
                    }
                    | Commands::States {
                        bounds,
                        max_patterns,
                        max_states,
                        ..
                    } => (bounds, max_patterns, max_states),
                    _ => panic!("expected check or states"),
                };
            let caps = bounds.caps(max_patterns, max_states);
            (caps.patterns, caps.states)
        }
        for command in ["check", "states"] {
            assert_eq!(caps(&[command]), (1 << 10, 1 << 16));
            assert_eq!(
                caps(&[command, "--instance-count", "2"]),
                (1 << 18, 1 << 20)
            );
            assert_eq!(
                caps(&[command, "--instance-count", "3"]),
                (1 << 26, 1 << 28)
            );
            assert_eq!(
                caps(&[command, "--instance-count", "2", "--max-patterns", "7"]),
                (7, 1 << 20),
                "the states default does not follow an explicit pattern cap"
            );
            assert_eq!(
                caps(&[command, "--instance-count", "2", "--max-states", "9"]),
                (1 << 18, 9)
            );
            assert_eq!(
                caps(&[command, "--instance-count", "255"]),
                (usize::MAX, usize::MAX)
            );
        }
    }

    #[test]
    fn parses_changed_subcommands_and_options() {
        let cli = Cli::try_parse_from([
            "rdra-ish",
            "diagram",
            "sample.rdra",
            "--kind",
            "rdra",
            "--format",
            "mermaid",
            "--buc",
            "BucScheduling",
            "--node-kind",
            "usecase",
            "--edge-kind",
            "invokes",
            "--out",
            "out.mmd",
        ])
        .unwrap();

        match cli.command {
            Commands::Diagram(DiagramArgs {
                inputs,
                kind,
                format,
                buc,
                usecase,
                entity,
                node_kind,
                edge_kind,
                out,
                ..
            }) => {
                assert_eq!(inputs, vec![PathBuf::from("sample.rdra")]);
                assert!(usecase.is_empty());
                assert!(entity.is_empty());
                assert!(matches!(kind, DiagramKind::Rdra));
                assert!(matches!(format, OutputFormat::Mermaid));
                assert_eq!(buc, vec!["BucScheduling"]);
                assert_eq!(node_kind, vec!["usecase"]);
                assert_eq!(edge_kind, vec!["invokes"]);
                assert_eq!(out, Some(PathBuf::from("out.mmd")));
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
            Commands::Csv {
                inputs, kind, out, ..
            } => {
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
            "sql",
            "--dialect",
            "sqlite",
        ])
        .unwrap();

        match cli.command {
            Commands::Export { kind, dialect, .. } => {
                assert!(matches!(kind, ExportKind::Sql));
                assert!(matches!(dialect, SqlDialect::Sqlite));
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
                ..
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
            ("diff", "`git diff`"),
            ("event-flow", "`--kind event`"),
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
