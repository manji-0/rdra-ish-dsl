//! `rdra-ish next`: the incremental modeling stage of each BUC and what the next stage asks.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::Severity;
use rdra_ish_core::refinement::{
    assess, BucStanding, Depth, FindingRef, Gap, Next, StageId, StageView, Verdict,
};
use serde_json::{json, Value};

use crate::cli::NextFormat;
use crate::load::{load_model, reject_model_errors};

const TEXT_ITEM_LIMIT: usize = 8;

pub(crate) fn run_next(inputs: &[PathBuf], bucs: &[String], format: NextFormat) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;

    let assessment = assess(&model, Depth::Lifecycle);
    let mut standings = assessment.bucs;
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
    let unowned = &assessment.unowned_findings;
    let output = match format {
        NextFormat::Text => render_text(&standings, unowned, &src),
        NextFormat::Json => {
            let mut text = serde_json::to_string_pretty(&render_json(&standings, unowned, &src))?;
            text.push('\n');
            text
        }
    };
    print!("{output}");
    Ok(())
}

fn render_text(standings: &[BucStanding], unowned: &[FindingRef], src: &str) -> String {
    let mut out = String::new();
    if !unowned.is_empty() {
        let _ = writeln!(
            out,
            "check reports findings no BUC owns (run `rdra-ish check {src}` for the traces):"
        );
        for finding in unowned {
            let _ = writeln!(out, "  - {}", finding_label(finding));
        }
        out.push('\n');
    }
    if standings.is_empty() {
        let spec = StageId::Scope.spec();
        let _ = writeln!(
            out,
            "No BUC yet: the model is before stage 0.\n\nNext: stage {} {}\n  {}\n  Gaps:\n    - add a `business` and a `buc`, and `belongs(Buc, Business)`\n  Ask:",
            StageId::Scope.number(),
            spec.name,
            spec.question
        );
        for ask in spec.asks {
            let _ = writeln!(out, "    - {ask}");
        }
        let _ = writeln!(out, "  Then review with:");
        for command in spec.review_commands(src, "<Buc>", &[]) {
            let _ = writeln!(out, "    {command}");
        }
        let _ = writeln!(
            out,
            "  See docs/incremental-modeling.md#{}",
            StageId::Scope.doc_anchor()
        );
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
                Verdict::Blocked => "!",
            };
            let _ = writeln!(
                out,
                "  [{mark}] {:2} {:21} {}{}",
                view.id.number(),
                view.spec().name,
                view.evidence,
                count_suffix(view)
            );
        }
        out.push('\n');
        let moved: Vec<&str> = p.moved.iter().map(String::as_str).collect();
        match p.next() {
            Next::Required(view) => {
                write_stage(&mut out, "Next", view, src, &p.id, &moved);
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
                    write_stage(&mut out, "Optional", view, src, &p.id, &moved);
                }
            }
        }
    }
    out
}

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

fn count_suffix(view: &StageView) -> String {
    let mut parts = Vec::new();
    let plural = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    parts.extend(plural(view.notes.len(), "note", "notes"));
    for (severity, one, many) in [
        (Severity::Error, "error from check", "errors from check"),
        (
            Severity::Warning,
            "warning from check",
            "warnings from check",
        ),
        (Severity::Info, "info from check", "infos from check"),
    ] {
        let n = view
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .count();
        parts.extend(plural(n, one, many));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    }
}

fn finding_label(finding: &FindingRef) -> String {
    let word = match finding.severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    format!("{word}[{}]", finding.name)
}

fn write_stage(
    out: &mut String,
    word: &str,
    view: &StageView,
    src: &str,
    buc: &str,
    entities: &[&str],
) {
    let stage = view.id;
    let spec = stage.spec();
    let blocked = if view.verdict() == Verdict::Blocked {
        " is blocked: the elements exist, but check rejects them"
    } else {
        ""
    };
    let _ = writeln!(
        out,
        "{word}: stage {} {}{blocked}\n  {}",
        stage.number(),
        spec.name,
        spec.question,
    );
    write_list(out, "Gaps", &view.gaps);
    write_list(out, "Notes", &view.notes);
    if !view.findings.is_empty() {
        let _ = writeln!(
            out,
            "  Findings (run `rdra-ish check {src}` for the traces):"
        );
        for finding in &view.findings {
            let _ = writeln!(out, "    - {}", finding_label(finding));
        }
    }
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
    for gap in items.iter().take(TEXT_ITEM_LIMIT) {
        let _ = writeln!(
            out,
            "    - {} {}: {}",
            gap.subject_kind.as_str(),
            gap.subject_id,
            gap.message
        );
    }
    if items.len() > TEXT_ITEM_LIMIT {
        let _ = writeln!(
            out,
            "    ... and {} more (`--format json` lists every one)",
            items.len() - TEXT_ITEM_LIMIT
        );
    }
}

fn render_json(standings: &[BucStanding], unowned: &[FindingRef], src: &str) -> Value {
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
                    "findings": view.findings.iter().map(finding_json).collect::<Vec<_>>(),
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
    json!({
        "bucs": bucs,
        "unowned_findings": unowned.iter().map(finding_json).collect::<Vec<_>>(),
    })
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
        "findings": view.findings.iter().map(finding_json).collect::<Vec<_>>(),
    })
}

fn finding_json(finding: &FindingRef) -> Value {
    json!({
        "name": finding.name,
        "severity": finding.severity.as_label(),
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
        Verdict::Blocked => "blocked",
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
