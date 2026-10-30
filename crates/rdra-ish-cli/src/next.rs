//! `rdra-ish next`: the incremental modeling stage of each BUC and what the next stage asks.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::refinement::{assess, BucStanding, Next, StageId, StageView, Verdict};
use serde_json::{json, Value};

use crate::cli::NextFormat;
use crate::load::{load_model, reject_model_errors};

/// Gaps printed per stage in text output; `--format json` lists every gap.
const TEXT_GAP_LIMIT: usize = 8;

pub(crate) fn run_next(inputs: &[PathBuf], bucs: &[String], format: NextFormat) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;

    let mut standings = assess(&model).bucs;
    if !bucs.is_empty() {
        for id in bucs {
            if !standings.iter().any(|p| &p.id == id) {
                bail!("unknown BUC `{id}`");
            }
        }
        standings.retain(|p| bucs.contains(&p.id));
    }

    let src = inputs
        .iter()
        .map(|input| shell_word(&input.display().to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    let output = match format {
        NextFormat::Text => render_text(&standings, &src),
        NextFormat::Json => {
            let mut text = serde_json::to_string_pretty(&render_json(&standings, &src))?;
            text.push('\n');
            text
        }
    };
    print!("{output}");
    Ok(())
}

fn render_text(standings: &[BucStanding], src: &str) -> String {
    let mut out = String::new();
    if standings.is_empty() {
        let _ = writeln!(out, "No BUC yet: the model is before stage 0.\n");
        write_stage(&mut out, StageId::Scope, src, "<Buc>", |out| {
            let _ = writeln!(
                out,
                "  Gaps:\n    - add a `business` and a `buc`, and `belongs(Buc, Business)`"
            );
        });
        return out;
    }

    if standings.len() > 1 {
        let width = standings.iter().map(|p| p.id.len()).max().unwrap_or(0);
        let _ = writeln!(out, "{:width$}  {:24}  Next", "BUC", "Stage");
        for p in standings {
            let next = match p.next() {
                Next::Required(view) => stage_label(Some(view.id)),
                Next::Optional(_) => "done".to_string(),
            };
            let _ = writeln!(
                out,
                "{:width$}  {:24}  {}",
                p.id,
                stage_label(p.reached()),
                next
            );
        }
        out.push('\n');
    }

    for (i, p) in standings.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{} \"{}\": stage {}",
            p.id,
            p.label,
            stage_label(p.reached())
        );
        for view in &p.stages {
            let mark = match view.verdict() {
                Verdict::Met => "x",
                Verdict::Open => " ",
            };
            let _ = writeln!(
                out,
                "  [{mark}] {:2} {:21} {}",
                view.id.number(),
                view.spec().name,
                view.evidence
            );
        }
        out.push('\n');
        match p.next() {
            Next::Required(view) => {
                write_stage(&mut out, view.id, src, &p.id, |out| write_gaps(out, view));
            }
            Next::Optional(_) => {
                let _ = writeln!(
                    out,
                    "Every stage gate passes. Review `rdra-ish states {src} --buc {}` diagnostics, and refine any stage whose review is not settled yet.",
                    p.id
                );
            }
        }
    }
    out
}

/// The next stage: its question, then what `gaps` writes, the questions to ask, and the
/// commands that review the stage.
fn write_stage(
    out: &mut String,
    stage: StageId,
    src: &str,
    buc: &str,
    gaps: impl FnOnce(&mut String),
) {
    let spec = stage.spec();
    let _ = writeln!(
        out,
        "Next: stage {} {}\n  {}",
        stage.number(),
        spec.name,
        spec.question,
    );
    gaps(out);
    let _ = writeln!(out, "  Ask:");
    for ask in spec.asks {
        let _ = writeln!(out, "    - {ask}");
    }
    let _ = writeln!(out, "  Then review with:");
    for command in spec.review_commands(src, buc) {
        let _ = writeln!(out, "    {command}");
    }
    let _ = writeln!(
        out,
        "  See docs/incremental-modeling.md#{}",
        stage.doc_anchor()
    );
}

fn write_gaps(out: &mut String, view: &StageView) {
    if view.gaps.is_empty() {
        return;
    }
    let _ = writeln!(out, "  Gaps:");
    for gap in view.gaps.iter().take(TEXT_GAP_LIMIT) {
        let _ = writeln!(
            out,
            "    - {} {}: {}",
            gap.subject_kind.as_str(),
            gap.subject_id,
            gap.message
        );
    }
    if view.gaps.len() > TEXT_GAP_LIMIT {
        let _ = writeln!(
            out,
            "    ... and {} more (`--format json` lists every gap)",
            view.gaps.len() - TEXT_GAP_LIMIT
        );
    }
}

fn render_json(standings: &[BucStanding], src: &str) -> Value {
    let bucs: Vec<Value> = standings
        .iter()
        .map(|p| {
            let reached = p.reached();
            let next = match p.next() {
                Next::Required(view) => {
                    let spec = view.spec();
                    json!({
                        "stage": view.id.number(),
                        "stage_name": spec.name,
                        "question": spec.question,
                        "doc": format!("docs/incremental-modeling.md#{}", view.id.doc_anchor()),
                        "asks": spec.asks,
                        "validate": spec.review_commands(src, &p.id),
                        "gaps": view.gaps.iter().map(|gap| json!({
                            "kind": gap.subject_kind.as_str(),
                            "id": gap.subject_id,
                            "message": gap.message,
                        })).collect::<Vec<_>>(),
                    })
                }
                Next::Optional(_) => Value::Null,
            };
            json!({
                "id": p.id,
                "label": p.label,
                "stage": reached.map(StageId::number),
                "stage_name": reached.map(|id| id.spec().name),
                "gates": p.stages.iter().map(gate_json).collect::<Vec<_>>(),
                "next": next,
            })
        })
        .collect();
    json!({ "bucs": bucs })
}

fn gate_json(view: &StageView) -> Value {
    json!({
        "stage": view.id.number(),
        "stage_name": view.spec().name,
        "passed": view.verdict() == Verdict::Met,
        "evidence": view.evidence,
        "gaps": view.gaps.len(),
    })
}

fn stage_label(stage: Option<StageId>) -> String {
    match stage {
        Some(stage) => format!("{} {}", stage.number(), stage.spec().name),
        None => "- not started".to_string(),
    }
}

/// `word` quoted for a POSIX shell when it holds a character the shell would split on.
fn shell_word(word: &str) -> String {
    if word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/\\._-:".contains(c))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}
