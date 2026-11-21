//! `diagram` CLI command.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use rdra_ish_emit::{
    mermaid::{
        BusinessMermaidEmitter, ConditionMermaidEmitter, ContextMermaidEmitter, ErMermaidEmitter,
        EventMermaidEmitter, FlowMermaidEmitter, InformationMermaidEmitter, RdraMermaidEmitter,
        RequirementMermaidEmitter, RobustnessMermaidEmitter, ScreenMermaidEmitter,
        SequenceMermaidEmitter, StateMermaidEmitter, UsecaseMermaidEmitter,
    },
    unknown_diagram_filter_kinds, Filter, Scope, StateView, View,
};
use std::fs;

use crate::cli::DiagramKind;
use crate::lifecycle_gate::{Gated, Output};
use crate::load::load_clean;

/// Mermaid's default `maxTextSize`: a diagram with more characters is not rendered.
const MERMAID_MAX_TEXT: usize = 50_000;
/// Mermaid's default `maxEdges`: a flowchart with more edges is not rendered.
const MERMAID_MAX_EDGES: usize = 500;

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub all: bool,
    pub buc: Vec<String>,
    pub usecase: Vec<String>,
    pub entity: Vec<String>,
    pub actor: Vec<String>,
    pub node_kind: Vec<String>,
    pub edge_kind: Vec<String>,
    pub title: bool,
    pub legend: bool,
    pub out: Option<PathBuf>,
}

/// What one diagram draws: its scope, and the entities `--entity` names for the kinds that
/// read them.
struct Shot {
    scope: Scope,
    entities: Vec<String>,
}

pub fn run_diagram(request: DiagramRequest) -> Result<()> {
    let DiagramRequest {
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
    } = request;
    let inputs = inputs.as_slice();
    let (program, model) = load_clean(inputs)?;
    let model_path = inputs
        .iter()
        .map(|input| input.display().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let dress = Dress { title, legend };

    if all {
        let scoped = !buc.is_empty() || !usecase.is_empty() || !entity.is_empty();
        if scoped || !actor.is_empty() || !node_kind.is_empty() || !edge_kind.is_empty() {
            anyhow::bail!(
                "--all writes every diagram of the model; drop --buc, --usecase, --entity, --actor, --node-kind, and --edge-kind"
            );
        }
        let Some(dir) = out else {
            anyhow::bail!("--all writes one file per diagram; pass --out <directory>");
        };
        return write_all(&program, &model, &model_path, dress, &dir);
    }

    if matches!(kind, DiagramKind::Sequence) {
        if !entity.is_empty() || !actor.is_empty() {
            anyhow::bail!("--kind sequence takes one --usecase <id>, or --buc <id> for the use cases of a BUC in order");
        }
        if buc.is_empty() && usecase.len() != 1 {
            anyhow::bail!("--kind sequence needs exactly one --usecase <id>, or --buc <id> for the use cases of a BUC in order");
        }
    }
    for buc_id in &buc {
        if !model.bucs.values().any(|b| b.id == *buc_id) {
            anyhow::bail!("unknown buc `{buc_id}`");
        }
    }
    for usecase_id in &usecase {
        if !model.use_cases.values().any(|u| u.id == *usecase_id) {
            anyhow::bail!("unknown usecase `{usecase_id}`");
        }
    }
    for entity_id in &entity {
        if !model.entities.values().any(|e| e.id == *entity_id) {
            anyhow::bail!("unknown entity `{entity_id}`");
        }
    }
    for actor_id in &actor {
        if !model.actors.values().any(|a| a.id == *actor_id) {
            anyhow::bail!("unknown actor `{actor_id}`");
        }
    }
    let given = [
        ("--buc", !buc.is_empty()),
        ("--usecase", !usecase.is_empty()),
        ("--entity", !entity.is_empty()),
        ("--actor", !actor.is_empty()),
    ];
    let named: Vec<&str> = given
        .iter()
        .filter(|(_, on)| *on)
        .map(|(f, _)| *f)
        .collect();
    if let [first, second, ..] = named[..] {
        anyhow::bail!("{first} and {second} cannot be combined; pass one of them");
    }

    let by_usecase = matches!(kind, DiagramKind::Usecase | DiagramKind::Robustness);
    if !usecase.is_empty() && !by_usecase && !matches!(kind, DiagramKind::Sequence) {
        anyhow::bail!("--usecase applies only to --kind usecase, robustness, or sequence");
    }
    if !actor.is_empty() && !by_usecase {
        anyhow::bail!("--actor applies only to --kind usecase or robustness: it draws the use cases the actor performs");
    }
    let by_entity = matches!(
        kind,
        DiagramKind::State | DiagramKind::Er | DiagramKind::Information
    );
    if !entity.is_empty() && !by_entity && !by_usecase {
        anyhow::bail!(
            "--entity applies only to --kind state (its state models), er and information (it and the entities it relates to), and usecase and robustness (the use cases that touch it)"
        );
    }
    if matches!(kind, DiagramKind::State) {
        for entity_id in &entity {
            let modeled = model.entities.iter().any(|(key, e)| {
                e.id == *entity_id
                    && e.columns
                        .iter()
                        .any(|c| model.is_lifecycle_column(key, &c.name))
            });
            if !modeled {
                anyhow::bail!("entity `{entity_id}` has no state model");
            }
        }
    }

    if (!node_kind.is_empty() || !edge_kind.is_empty()) && !matches!(kind, DiagramKind::Rdra) {
        anyhow::bail!("--node-kind and --edge-kind are currently supported only for --kind rdra");
    }
    if !node_kind.is_empty() || !edge_kind.is_empty() {
        let (unknown_nodes, unknown_edges) = unknown_diagram_filter_kinds(&node_kind, &edge_kind);
        if !unknown_nodes.is_empty() || !unknown_edges.is_empty() {
            let mut parts = Vec::new();
            if !unknown_nodes.is_empty() {
                parts.push(format!("unknown --node-kind: {}", unknown_nodes.join(", ")));
            }
            if !unknown_edges.is_empty() {
                parts.push(format!("unknown --edge-kind: {}", unknown_edges.join(", ")));
            }
            anyhow::bail!("{}", parts.join("; "));
        }
    }

    // What the diagram is about, for its title.
    let about = if !actor.is_empty() {
        labels(&model, &actor, actor_label)
    } else if !entity.is_empty() {
        labels(&model, &entity, entity_label)
    } else if !usecase.is_empty() {
        labels(&model, &usecase, usecase_label)
    } else if !buc.is_empty() {
        labels(&model, &buc, buc_label)
    } else {
        String::new()
    };

    let shot = if !usecase.is_empty() {
        Shot {
            scope: Scope::UseCases(usecase),
            entities: Vec::new(),
        }
    } else if !buc.is_empty() {
        Shot {
            scope: Scope::Bucs(buc),
            entities: Vec::new(),
        }
    } else if !actor.is_empty() {
        let performed = use_cases_of_actors(&model, &actor);
        if performed.is_empty() {
            anyhow::bail!("{} performs no use case", actor.join(", "));
        }
        Shot {
            scope: Scope::UseCases(performed),
            entities: Vec::new(),
        }
    } else if !entity.is_empty() && by_usecase {
        let touching = use_cases_touching(&model, &entity);
        if touching.is_empty() {
            anyhow::bail!("no use case reads or writes {}", entity.join(", "));
        }
        Shot {
            scope: Scope::UseCases(touching),
            entities: Vec::new(),
        }
    } else {
        Shot {
            scope: Scope::Whole,
            entities: entity,
        }
    };

    if matches!(kind, DiagramKind::Event) {
        for diag in rdra_ish_core::event_diagnostics(&model) {
            eprintln!("warning: {}", diag.error);
        }
    }
    let gated = diagram_output(&kind).gate(&program, &model)?;
    let text = render(
        &gated,
        &model,
        &kind,
        shot,
        &model_path,
        node_kind,
        edge_kind,
    )?;
    for line in findings(&text, &kind) {
        eprintln!("{line}");
    }
    let text = dress.apply(text, &kind, &about);
    match out {
        Some(out) => {
            let out_path = out.with_extension("mmd");
            fs::write(&out_path, &text)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
        }
        None => print!("{text}"),
    }

    Ok(())
}

fn render(
    gated: &Gated,
    model: &SemanticModel,
    kind: &DiagramKind,
    shot: Shot,
    model_path: &str,
    node_kind: Vec<String>,
    edge_kind: Vec<String>,
) -> Result<String> {
    let view = View {
        state: StateView {
            entities: shot.entities,
        },
        scope: shot.scope,
        filter: match kind {
            DiagramKind::Er => Filter::Er,
            _ => Filter::None,
        },
        model_path: Some(model_path.to_string()),
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
    }
    .with_graph_filters(node_kind, edge_kind);
    Ok(gated.emit(model, &view)?)
}

/// `--all`: the review set, one file per diagram under `dir`. The whole-model kinds that
/// stay readable at any size, each BUC's use case, robustness, flow, and sequence diagrams,
/// and each entity's state models. A diagram with nothing to draw is left out.
fn write_all(
    program: &rdra_ish_core::ResolvedProgram,
    model: &SemanticModel,
    model_path: &str,
    dress: Dress,
    dir: &Path,
) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let mut bucs: Vec<&str> = model.bucs.values().map(|b| b.id.as_str()).collect();
    bucs.sort_unstable();
    let mut modeled: Vec<&str> = model
        .entities
        .iter()
        .filter(|(key, e)| {
            e.columns
                .iter()
                .any(|c| model.is_lifecycle_column(*key, &c.name))
        })
        .map(|(_, e)| e.id.as_str())
        .collect();
    modeled.sort_unstable();

    let whole = [
        DiagramKind::Context,
        DiagramKind::Requirement,
        DiagramKind::Business,
        DiagramKind::Information,
        DiagramKind::Condition,
        DiagramKind::Er,
        DiagramKind::Screen,
        DiagramKind::Event,
        DiagramKind::State,
    ];
    let per_buc = [
        DiagramKind::Usecase,
        DiagramKind::Robustness,
        DiagramKind::Flow,
        DiagramKind::Sequence,
    ];
    let mut written = 0;
    let mut empty = 0;
    let mut write = |kind: &DiagramKind, gated: &Gated, name: String, shot: Shot, about: &str| {
        let text = match render(gated, model, kind, shot, model_path, Vec::new(), Vec::new()) {
            Ok(text) => text,
            // A sequence diagram of a BUC without use cases.
            Err(_) => {
                empty += 1;
                return Ok(());
            }
        };
        let lines = findings(&text, kind);
        if lines.iter().any(|l| l.starts_with("note: nothing to draw")) {
            empty += 1;
            return Ok(());
        }
        let path = dir.join(format!("{name}.mmd"));
        for line in lines {
            eprintln!("{}: {line}", path.display());
        }
        fs::write(&path, dress.apply(text, kind, about))
            .with_context(|| format!("failed to write {}", path.display()))?;
        println!("wrote {}", path.display());
        written += 1;
        anyhow::Ok(())
    };
    for kind in whole.iter().chain(&per_buc) {
        let name = kind_name(kind);
        // A model with lifecycle errors has no state diagram; the rest are still written.
        let gated = match diagram_output(kind).gate(program, model) {
            Ok(gated) => gated,
            Err(err) => {
                eprintln!("note: --kind {name} is left out: {err}");
                continue;
            }
        };
        if whole.iter().any(|k| kind_name(k) == name) {
            let shot = Shot {
                scope: Scope::Whole,
                entities: Vec::new(),
            };
            write(kind, &gated, name.to_string(), shot, "")?;
            if matches!(kind, DiagramKind::State) {
                for entity in &modeled {
                    let shot = Shot {
                        scope: Scope::Whole,
                        entities: vec![entity.to_string()],
                    };
                    let about = entity_label(model, entity);
                    write(kind, &gated, format!("{name}__{entity}"), shot, &about)?;
                }
            }
        } else {
            for buc in &bucs {
                let shot = Shot {
                    scope: Scope::Bucs(vec![buc.to_string()]),
                    entities: Vec::new(),
                };
                let about = buc_label(model, buc);
                write(kind, &gated, format!("{name}__{buc}"), shot, &about)?;
            }
        }
    }
    println!(
        "wrote {written} diagrams to {} ({empty} with nothing to draw left out)",
        dir.display()
    );
    Ok(())
}

/// What a reader of the diagram should know before rendering it: that Mermaid's default
/// limits refuse it, or that it draws nothing.
fn findings(text: &str, kind: &DiagramKind) -> Vec<String> {
    let mut out = Vec::new();
    let name = kind_name(kind);
    if text.lines().any(|l| l.trim() == "%% kinds: (none)") {
        out.push(format!(
            "note: nothing to draw: the model declares nothing --kind {name} draws in this scope"
        ));
        return out;
    }
    let chars = text.chars().count();
    let flowchart = text.lines().any(|l| l.starts_with("flowchart"));
    let edges = if flowchart {
        text.lines()
            .filter(|l| !l.trim_start().starts_with("%%"))
            .filter(|l| ["-->", "-.->", "==>", "---"].iter().any(|a| l.contains(a)))
            .count()
    } else {
        0
    };
    if chars > MERMAID_MAX_TEXT || edges > MERMAID_MAX_EDGES {
        let narrow = match kind {
            DiagramKind::Usecase | DiagramKind::Robustness => {
                "--buc, --usecase, --actor, or --entity"
            }
            DiagramKind::Er | DiagramKind::Information => "--buc or --entity",
            DiagramKind::Rdra => "--buc, --node-kind, or --edge-kind, or use a narrower --kind",
            _ => "--buc",
        };
        let size = if flowchart {
            format!("{chars} characters and {edges} edges")
        } else {
            format!("{chars} characters")
        };
        out.push(format!(
            "warning: this diagram has {size}; Mermaid's defaults stop at {MERMAID_MAX_TEXT} characters (maxTextSize) and {MERMAID_MAX_EDGES} edges (maxEdges), so it does not render as is. Narrow it with {narrow}, or raise the limits in the renderer's config"
        ));
    }
    out
}

/// `--title` and `--legend`: what a rendered diagram shows of itself. The `%%` header says
/// the same to a reader of the text, and a renderer drops it.
#[derive(Clone, Copy)]
struct Dress {
    title: bool,
    legend: bool,
}

impl Dress {
    fn apply(self, text: String, kind: &DiagramKind, about: &str) -> String {
        let mut text = text;
        if self.legend {
            text = with_legend(text);
        }
        if self.title {
            let name = kind_title(kind);
            let title = if about.is_empty() {
                name.to_string()
            } else {
                format!("{name}: {about}")
            };
            // YAML front matter; a double-quoted scalar escapes `\` and `"`.
            let quoted = title.replace('\\', "\\\\").replace('"', "\\\"");
            text = format!("---\ntitle: \"{quoted}\"\n---\n{text}");
        }
        text
    }
}

/// A flowchart with a `凡例` subgraph that holds one node per node class it styles, before
/// its `classDef` lines. Other diagram types have no node classes and stay as they are.
fn with_legend(text: String) -> String {
    if !text.lines().any(|l| l.starts_with("flowchart")) {
        return text;
    }
    let classes: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("classDef "))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    let Some(at) = text.find("\n  classDef ") else {
        return text;
    };
    let mut legend = String::from("\n  subgraph legend[\"凡例\"]");
    for class in &classes {
        legend.push_str(&format!("\n    legend__{class}[\"{class}\"]:::{class}"));
    }
    legend.push_str("\n  end");
    let mut out = text.clone();
    out.insert_str(at, &legend);
    out
}

fn labels(
    model: &SemanticModel,
    ids: &[String],
    label: impl Fn(&SemanticModel, &str) -> String,
) -> String {
    ids.iter()
        .map(|id| label(model, id))
        .collect::<Vec<_>>()
        .join("、")
}

fn buc_label(model: &SemanticModel, id: &str) -> String {
    model
        .bucs
        .values()
        .find(|b| b.id == id)
        .map_or_else(|| id.to_string(), |b| b.label.clone())
}

fn usecase_label(model: &SemanticModel, id: &str) -> String {
    model
        .use_cases
        .values()
        .find(|u| u.id == id)
        .map_or_else(|| id.to_string(), |u| u.label.clone())
}

fn entity_label(model: &SemanticModel, id: &str) -> String {
    model
        .entities
        .values()
        .find(|e| e.id == id)
        .map_or_else(|| id.to_string(), |e| e.label.clone())
}

fn actor_label(model: &SemanticModel, id: &str) -> String {
    model
        .actors
        .values()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_string(), |a| a.label.clone())
}

/// The ids of the use cases one of `actors` performs, directly or through a BUC.
fn use_cases_of_actors(model: &SemanticModel, actors: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = model
        .use_cases
        .iter()
        .filter(|(key, _)| {
            rdra_ish_core::actors_for_usecase(model, *key)
                .into_iter()
                .any(|a| actors.contains(&model.actors[a].id))
        })
        .map(|(_, u)| u.id.clone())
        .collect();
    ids.sort_unstable();
    ids
}

/// The ids of the use cases that read or write one of `entities`, themselves or through an
/// API they invoke.
fn use_cases_touching(model: &SemanticModel, entities: &[String]) -> Vec<String> {
    let touched = |node: &NodeRef| {
        model.relations.iter().any(|rel| {
            rel.from == *node
                && matches!(
                    rel.kind,
                    RelKind::Reads | RelKind::Creates | RelKind::Updates | RelKind::Deletes
                )
                && matches!(&rel.to, NodeRef::Entity(e) if entities.contains(&model.entities[*e].id))
        })
    };
    let mut ids: Vec<String> = model
        .use_cases
        .iter()
        .filter(|(key, _)| {
            let node = NodeRef::UseCase(*key);
            touched(&node)
                || model
                    .relations
                    .iter()
                    .any(|rel| rel.kind == RelKind::Invokes && rel.from == node && touched(&rel.to))
        })
        .map(|(_, u)| u.id.clone())
        .collect();
    ids.sort_unstable();
    ids
}

/// The `--kind` value.
fn kind_name(kind: &DiagramKind) -> &'static str {
    match kind {
        DiagramKind::Rdra => "rdra",
        DiagramKind::Context => "context",
        DiagramKind::Requirement => "requirement",
        DiagramKind::Business => "business",
        DiagramKind::Usecase => "usecase",
        DiagramKind::Robustness => "robustness",
        DiagramKind::Sequence => "sequence",
        DiagramKind::Information => "information",
        DiagramKind::Condition => "condition",
        DiagramKind::Flow => "flow",
        DiagramKind::Screen => "screen",
        DiagramKind::Er => "er",
        DiagramKind::State => "state",
        DiagramKind::Event => "event",
    }
}

/// The diagram's name as RDRA calls it, for its title.
fn kind_title(kind: &DiagramKind) -> &'static str {
    match kind {
        DiagramKind::Rdra => "RDRA",
        DiagramKind::Context => "システムコンテキスト",
        DiagramKind::Requirement => "要求モデル",
        DiagramKind::Business => "業務の流れ図",
        DiagramKind::Usecase => "UC複合図",
        DiagramKind::Robustness => "ロバストネス図",
        DiagramKind::Sequence => "シーケンス図",
        DiagramKind::Information => "情報モデル",
        DiagramKind::Condition => "バリエーション・条件",
        DiagramKind::Flow => "業務フロー",
        DiagramKind::Screen => "画面遷移図",
        DiagramKind::Er => "ER図",
        DiagramKind::State => "状態モデル",
        DiagramKind::Event => "システム間連携",
    }
}

fn diagram_output(kind: &DiagramKind) -> Output {
    match kind {
        DiagramKind::Rdra => Output::declared(RdraMermaidEmitter),
        DiagramKind::Context => Output::declared(ContextMermaidEmitter),
        DiagramKind::Requirement => Output::declared(RequirementMermaidEmitter),
        DiagramKind::Business => Output::declared(BusinessMermaidEmitter),
        DiagramKind::Usecase => Output::declared(UsecaseMermaidEmitter),
        DiagramKind::Robustness => Output::declared(RobustnessMermaidEmitter),
        DiagramKind::Sequence => Output::declared(SequenceMermaidEmitter),
        DiagramKind::Information => Output::declared(InformationMermaidEmitter),
        DiagramKind::Condition => Output::declared(ConditionMermaidEmitter),
        DiagramKind::Flow => Output::declared(FlowMermaidEmitter),
        DiagramKind::Screen => Output::declared(ScreenMermaidEmitter),
        DiagramKind::Er => Output::declared(ErMermaidEmitter),
        DiagramKind::State => Output::states(StateMermaidEmitter),
        DiagramKind::Event => Output::declared(EventMermaidEmitter),
    }
}
