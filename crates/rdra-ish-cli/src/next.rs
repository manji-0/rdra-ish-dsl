//! `rdra-ish next`: the incremental modeling stage of each BUC and what the next stage asks.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::refinement::{refinement_progress, BucProgress, Gate, Stage};
use serde_json::{json, Value};

use crate::cli::NextFormat;
use crate::load::{load_model, reject_model_errors};

/// Gaps printed per stage in text output; `--format json` lists every gap.
const TEXT_GAP_LIMIT: usize = 8;

pub(crate) fn run_next(inputs: &[PathBuf], bucs: &[String], format: NextFormat) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;

    let mut progress = refinement_progress(&model);
    if !bucs.is_empty() {
        for id in bucs {
            if !progress.iter().any(|p| &p.buc_id == id) {
                bail!("unknown BUC `{id}`");
            }
        }
        progress.retain(|p| bucs.contains(&p.buc_id));
    }

    let src = inputs
        .iter()
        .map(|input| shell_word(&input.display().to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    let output = match format {
        NextFormat::Text => render_text(&progress, &src),
        NextFormat::Json => {
            let mut text = serde_json::to_string_pretty(&render_json(&progress, &src))?;
            text.push('\n');
            text
        }
    };
    print!("{output}");
    Ok(())
}

fn render_text(progress: &[BucProgress], src: &str) -> String {
    let mut out = String::new();
    if progress.is_empty() {
        let stage = Stage::Scope;
        let _ = writeln!(out, "No BUC yet: the model is before stage 0.\n");
        write_stage(&mut out, stage, src, "<Buc>", |out| {
            let _ = writeln!(
                out,
                "  Gaps:\n    - add a `business` and a `buc`, and `belongs(Buc, Business)`"
            );
        });
        return out;
    }

    if progress.len() > 1 {
        let width = progress.iter().map(|p| p.buc_id.len()).max().unwrap_or(0);
        let _ = writeln!(out, "{:width$}  {:24}  Next", "BUC", "Stage");
        for p in progress {
            let _ = writeln!(
                out,
                "{:width$}  {:24}  {}",
                p.buc_id,
                stage_label(p.stage()),
                p.next_gate()
                    .map_or("done".to_string(), |g| stage_label(Some(g.stage)))
            );
        }
        out.push('\n');
    }

    for (i, p) in progress.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "{} \"{}\": stage {}",
            p.buc_id,
            p.buc_label,
            stage_label(p.stage())
        );
        for gate in &p.gates {
            let mark = if gate.passed { "x" } else { " " };
            let _ = writeln!(
                out,
                "  [{mark}] {:2} {:21} {}",
                gate.stage.number(),
                gate.stage.name(),
                gate.evidence
            );
        }
        out.push('\n');
        match p.next_gate() {
            Some(gate) => {
                write_next(&mut out, gate, src, &p.buc_id);
            }
            None => {
                let _ = writeln!(
                    out,
                    "Every stage gate passes. Review `rdra-ish states {src} --buc {}` diagnostics, and refine any stage whose review is not settled yet.",
                    p.buc_id
                );
            }
        }
    }
    out
}

fn write_next(out: &mut String, gate: &Gate, src: &str, buc: &str) {
    write_stage(out, gate.stage, src, buc, |out| write_gaps(out, gate));
}

/// The next stage: its question, then what `gaps` writes, the questions to ask, and the
/// commands that review the stage.
fn write_stage(
    out: &mut String,
    stage: Stage,
    src: &str,
    buc: &str,
    gaps: impl FnOnce(&mut String),
) {
    let _ = writeln!(
        out,
        "Next: stage {} {}\n  {}",
        stage.number(),
        stage.name(),
        stage.question(),
    );
    gaps(out);
    let _ = writeln!(out, "  Ask:");
    for ask in stage.asks() {
        let _ = writeln!(out, "    - {ask}");
    }
    let _ = writeln!(out, "  Then review with:");
    for command in validate_commands(stage, src, buc) {
        let _ = writeln!(out, "    {command}");
    }
    let _ = writeln!(
        out,
        "  See docs/incremental-modeling.md#{}",
        stage.doc_anchor()
    );
}

fn write_gaps(out: &mut String, gate: &Gate) {
    if gate.gaps.is_empty() {
        return;
    }
    let _ = writeln!(out, "  Gaps:");
    for gap in gate.gaps.iter().take(TEXT_GAP_LIMIT) {
        let _ = writeln!(
            out,
            "    - {} {}: {}",
            gap.subject_kind, gap.subject_id, gap.message
        );
    }
    if gate.gaps.len() > TEXT_GAP_LIMIT {
        let _ = writeln!(
            out,
            "    ... and {} more (`--format json` lists every gap)",
            gate.gaps.len() - TEXT_GAP_LIMIT
        );
    }
}

fn render_json(progress: &[BucProgress], src: &str) -> Value {
    let bucs: Vec<Value> = progress
        .iter()
        .map(|p| {
            let stage = p.stage();
            json!({
                "id": p.buc_id,
                "label": p.buc_label,
                "stage": stage.map(Stage::number),
                "stage_name": stage.map(Stage::name),
                "gates": p.gates.iter().map(gate_json).collect::<Vec<_>>(),
                "next": p.next_gate().map(|gate| json!({
                    "stage": gate.stage.number(),
                    "stage_name": gate.stage.name(),
                    "question": gate.stage.question(),
                    "doc": format!("docs/incremental-modeling.md#{}", gate.stage.doc_anchor()),
                    "asks": gate.stage.asks(),
                    "validate": validate_commands(gate.stage, src, &p.buc_id),
                    "gaps": gate.gaps.iter().map(|gap| json!({
                        "kind": gap.subject_kind,
                        "id": gap.subject_id,
                        "message": gap.message,
                    })).collect::<Vec<_>>(),
                })),
            })
        })
        .collect();
    json!({ "bucs": bucs })
}

fn gate_json(gate: &Gate) -> Value {
    json!({
        "stage": gate.stage.number(),
        "stage_name": gate.stage.name(),
        "passed": gate.passed,
        "evidence": gate.evidence,
        "gaps": gate.gaps.len(),
    })
}

fn stage_label(stage: Option<Stage>) -> String {
    match stage {
        Some(stage) => format!("{} {}", stage.number(), stage.name()),
        None => "- not started".to_string(),
    }
}

fn validate_commands(stage: Stage, src: &str, buc: &str) -> Vec<String> {
    stage
        .validate()
        .iter()
        .map(|command| command.replace("{src}", src).replace("{buc}", buc))
        .collect()
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
