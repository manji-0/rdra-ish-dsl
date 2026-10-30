//! `rdra-ish next`: the incremental modeling stage of each BUC and what the next stage asks.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::refinement::{assess, BucStanding, Gap, Next, StageId, StageView, Verdict};
use serde_json::{json, Value};

use crate::cli::NextFormat;
use crate::load::{load_model, reject_model_errors};

/// Gaps and notes printed per stage in text output; `--format json` lists every one.
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
        write_stage(&mut out, "Next", StageId::Scope, src, "<Buc>", &[], |out| {
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
            let _ = writeln!(
                out,
                "{:width$}  {:24}  {}",
                p.id,
                stage_label(p.reached()),
                next_label(p)
            );
        }
        out.push('\n');
    }

    for (i, p) in standings.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let complete = if p.required_complete() {
            ", required stages complete"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "{} \"{}\": stage {}{complete}",
            p.id,
            p.label,
            stage_label(p.reached())
        );
        for view in &p.stages {
            let mark = match view.verdict() {
                Verdict::Met => "x",
                Verdict::Open => " ",
            };
            let notes = match view.notes.len() {
                0 => String::new(),
                1 => " (1 note)".to_string(),
                n => format!(" ({n} notes)"),
            };
            let _ = writeln!(
                out,
                "  [{mark}] {:2} {:21} {}{notes}",
                view.id.number(),
                view.spec().name,
                view.evidence
            );
        }
        out.push('\n');
        let moved: Vec<&str> = p.moved.iter().map(String::as_str).collect();
        match p.next() {
            Next::Required(view) => {
                write_stage(&mut out, "Next", view.id, src, &p.id, &moved, |out| {
                    write_list(out, "Gaps", &view.gaps);
                    write_list(out, "Notes", &view.notes);
                });
            }
            Next::Optional(views) => {
                let _ = writeln!(
                    out,
                    "Every required stage is met. Rules are optional; add one when the business has it:"
                );
                for view in views {
                    if view.verdict() == Verdict::Met {
                        let _ = writeln!(
                            out,
                            "  [x] {} {}: {}",
                            view.id.number(),
                            view.spec().name,
                            view.evidence
                        );
                        continue;
                    }
                    write_stage(&mut out, "Optional", view.id, src, &p.id, &moved, |out| {
                        write_list(out, "Gaps", &view.gaps);
                    });
                }
            }
        }
    }
    out
}

/// The summary column: the next required stage, or the optional stages still open.
fn next_label(p: &BucStanding) -> String {
    match p.next() {
        Next::Required(view) => stage_label(Some(view.id)),
        Next::Optional(views) => {
            let open: Vec<String> = views
                .iter()
                .filter(|view| view.verdict() != Verdict::Met)
                .map(|view| view.id.number())
                .collect();
            if open.is_empty() {
                "complete".to_string()
            } else {
                format!("optional: {}", open.join(" "))
            }
        }
    }
}

/// A stage to work on: its question, then what `lists` writes, the questions to ask, and
/// the commands that review the stage.
fn write_stage(
    out: &mut String,
    word: &str,
    stage: StageId,
    src: &str,
    buc: &str,
    entities: &[&str],
    lists: impl FnOnce(&mut String),
) {
    let spec = stage.spec();
    let _ = writeln!(
        out,
        "{word}: stage {} {}\n  {}",
        stage.number(),
        spec.name,
        spec.question,
    );
    lists(out);
    let _ = writeln!(out, "  Ask:");
    for ask in spec.asks {
        let _ = writeln!(out, "    - {ask}");
    }
    let _ = writeln!(out, "  Then review with:");
    for command in spec.review_commands(src, buc, entities) {
        let _ = writeln!(out, "    {command}");
    }
    let _ = writeln!(
        out,
        "  See docs/incremental-modeling.md#{}",
        stage.doc_anchor()
    );
}

fn write_list(out: &mut String, title: &str, items: &[Gap]) {
    if items.is_empty() {
        return;
    }
    let _ = writeln!(out, "  {title}:");
    for gap in items.iter().take(TEXT_GAP_LIMIT) {
        let _ = writeln!(
            out,
            "    - {} {}: {}",
            gap.subject_kind.as_str(),
            gap.subject_id,
            gap.message
        );
    }
    if items.len() > TEXT_GAP_LIMIT {
        let _ = writeln!(
            out,
            "    ... and {} more (`--format json` lists every one)",
            items.len() - TEXT_GAP_LIMIT
        );
    }
}

fn render_json(standings: &[BucStanding], src: &str) -> Value {
    let bucs: Vec<Value> = standings
        .iter()
        .map(|p| {
            let reached = p.reached();
            let moved: Vec<&str> = p.moved.iter().map(String::as_str).collect();
            let stage_json = |view: &StageView| {
                let spec = view.spec();
                json!({
                    "stage": view.id.number(),
                    "stage_name": spec.name,
                    "slug": view.id.slug(),
                    "verdict": verdict_json(view.verdict()),
                    "question": spec.question,
                    "doc": format!("docs/incremental-modeling.md#{}", view.id.doc_anchor()),
                    "asks": spec.asks,
                    "validate": spec.review_commands(src, &p.id, &moved),
                    "gaps": view.gaps.iter().map(gap_json).collect::<Vec<_>>(),
                    "notes": view.notes.iter().map(gap_json).collect::<Vec<_>>(),
                })
            };
            let next = match p.next() {
                Next::Required(view) => {
                    let mut next = stage_json(view);
                    next["kind"] = json!("required");
                    next
                }
                Next::Optional(views) => json!({
                    "kind": "optional",
                    "stages": views.iter().map(|view| stage_json(view)).collect::<Vec<_>>(),
                }),
            };
            json!({
                "id": p.id,
                "label": p.label,
                "stage": reached.map(StageId::number),
                "stage_name": reached.map(|id| id.spec().name),
                "slug": reached.map(StageId::slug),
                "required_complete": p.required_complete(),
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
        "slug": view.id.slug(),
        "passed": view.verdict() == Verdict::Met,
        "verdict": verdict_json(view.verdict()),
        "evidence": view.evidence,
        "gaps": view.gaps.len(),
        "notes": view.notes.len(),
    })
}

fn gap_json(gap: &Gap) -> Value {
    json!({
        "kind": gap.subject_kind.as_str(),
        "id": gap.subject_id,
        "message": gap.message,
    })
}

fn verdict_json(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Met => "met",
        Verdict::Open => "open",
    }
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
