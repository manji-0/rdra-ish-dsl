//! The generated blocks of the docs and skills, rendered from the stage specs.
//!
//! A block sits between `<!-- stage-spec: <block> -->` and `<!-- /stage-spec -->`. The
//! `stage_docs_are_in_sync` test compares each block with its rendering and, with
//! `RDRA_ISH_BLESS=1`, rewrites it. The hand-written prose around a block stays.

use std::fmt::Write as _;

use super::{spec, stages, StageId};

/// Placeholders the docs fill the review templates with.
const DOC_SRC: &str = "src/";
const DOC_BUC: &str = "BucOrder";
const DOC_ENTITY: &str = "Order";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Block {
    /// The Stage Map table of `docs/incremental-modeling.md`.
    Map,
    /// The `## Stage N: Name` heading of one stage section, so the anchor derives.
    Heading(StageId),
    /// The "Ask the user" list and the "Validation" commands of one stage section.
    Ask(StageId),
    /// The gate table of `docs/cli-reference.md#next`.
    Gates,
    /// The stage table of a skill: number, concern, question, and what the stage adds.
    SkillTable,
}

impl Block {
    /// The marker text after `stage-spec:`.
    pub(crate) fn parse(label: &str) -> Option<Block> {
        let stage = |slug: &str| stages().find(|id| id.slug() == slug);
        match label.split_once(' ') {
            None if label == "map" => Some(Block::Map),
            None if label == "gates" => Some(Block::Gates),
            None if label == "skill-table" => Some(Block::SkillTable),
            Some(("heading", slug)) => stage(slug).map(Block::Heading),
            Some(("ask", slug)) => stage(slug).map(Block::Ask),
            _ => None,
        }
    }
}

pub(crate) fn render(block: Block) -> String {
    let mut out = String::new();
    match block {
        Block::Map => {
            out.push_str("| Stage | Concern | Abstraction | Main question | Add | Validate |\n");
            out.push_str("|---|---|---|---|---|---|\n");
            for id in stages() {
                let s = spec(id);
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} |",
                    id.number(),
                    s.concern,
                    s.name,
                    s.question,
                    s.adds,
                    s.review
                        .iter()
                        .map(|template| format!("`{}`", short_command(template)))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        Block::Heading(id) => {
            let _ = writeln!(out, "## Stage {}: {}", id.number(), spec(id).name);
        }
        Block::Ask(id) => {
            let s = spec(id);
            out.push_str("Ask the user:\n\n");
            for ask in s.asks {
                let _ = writeln!(out, "- {ask}");
            }
            out.push_str("\nValidation:\n\n```sh\n");
            for command in s.review_commands(DOC_SRC, DOC_BUC, &[DOC_ENTITY]) {
                let _ = writeln!(out, "{command}");
            }
            out.push_str("```\n");
        }
        Block::Gates => {
            out.push_str("| Stage | The gate passes when |\n|---|---|\n");
            for id in stages() {
                let s = spec(id);
                let _ = writeln!(
                    out,
                    "| {} {} | {} |",
                    id.number(),
                    s.name,
                    s.passes_when().join("; and ")
                );
            }
        }
        Block::SkillTable => {
            out.push_str("| Stage | Concern | Main question | Add |\n|---|---|---|---|\n");
            for id in stages() {
                let s = spec(id);
                let _ = writeln!(
                    out,
                    "| {} {} | {} | {} | {} |",
                    id.number(),
                    s.name,
                    s.concern,
                    s.question,
                    s.adds
                );
            }
        }
    }
    out
}

/// A review template as the Stage Map's Validate column abbreviates it: without the binary,
/// the inputs, the placeholder flags, and the default `--format mermaid`.
fn short_command(template: &str) -> String {
    let words: Vec<&str> = template.split(' ').collect();
    let mut kept = Vec::new();
    let mut skip = false;
    for (i, word) in words.iter().enumerate().skip(1) {
        if skip {
            skip = false;
            continue;
        }
        let value = words.get(i + 1).copied().unwrap_or("");
        if word.starts_with('{') || (word.starts_with("--") && value.starts_with('{')) {
            skip = word.starts_with("--");
            continue;
        }
        if *word == "--format" && value == "mermaid" {
            skip = true;
            continue;
        }
        kept.push(*word);
    }
    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                markdown_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                out.push(path);
            }
        }
    }

    /// The generated blocks of one file: `(label, body)`, and the text with each body
    /// replaced by its rendering.
    fn sync_blocks(text: &str, failures: &mut Vec<String>, where_: &str) -> (Vec<Block>, String) {
        let mut blocks = Vec::new();
        let mut rewritten = String::new();
        let mut lines = text.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            rewritten.push_str(line);
            let Some(label) = line
                .trim_end()
                .strip_prefix("<!-- stage-spec: ")
                .and_then(|rest| rest.strip_suffix(" -->"))
            else {
                continue;
            };
            let Some(block) = Block::parse(label) else {
                failures.push(format!("{where_}: unknown stage-spec block `{label}`"));
                continue;
            };
            let mut body = String::new();
            let mut closed = false;
            for inner in lines.by_ref() {
                if inner.trim_end() == "<!-- /stage-spec -->" {
                    closed = true;
                    rewritten.push_str(&render(block));
                    rewritten.push_str(inner);
                    break;
                }
                body.push_str(inner);
            }
            if !closed {
                failures.push(format!("{where_}: unclosed stage-spec block `{label}`"));
            } else if body != render(block) {
                failures.push(format!("{where_}: stale stage-spec block `{label}`"));
            }
            blocks.push(block);
        }
        (blocks, rewritten)
    }

    #[test]
    fn stage_docs_are_in_sync() {
        let repo = repo();
        let bless = std::env::var_os("RDRA_ISH_BLESS").is_some();
        let mut files = vec![repo.join("README.md")];
        markdown_files(&repo.join("docs"), &mut files);
        markdown_files(&repo.join("skills"), &mut files);
        markdown_files(&repo.join("samples"), &mut files);
        files.sort();
        let anchors: Vec<String> = stages().map(|id| id.doc_anchor()).collect();
        let mut failures = Vec::new();
        for path in &files {
            let text = fs::read_to_string(path).unwrap();
            let where_ = path.strip_prefix(&repo).unwrap().display().to_string();
            let (blocks, rewritten) = sync_blocks(&text, &mut failures, &where_);
            if bless && rewritten != text {
                fs::write(path, &rewritten).unwrap();
            }
            if where_ == "docs/incremental-modeling.md" {
                let expected: Vec<Block> = std::iter::once(Block::Map)
                    .chain(stages().flat_map(|id| [Block::Heading(id), Block::Ask(id)]))
                    .collect();
                if blocks != expected {
                    failures.push(format!(
                        "{where_}: expected the map block, then a heading and an ask block per stage in ORDER; found {blocks:?}"
                    ));
                }
            }
            if where_.starts_with("skills/") {
                let mut inside = false;
                for (n, line) in text.lines().enumerate() {
                    if line.starts_with("<!-- stage-spec: ") {
                        inside = true;
                    } else if line.starts_with("<!-- /stage-spec -->") {
                        inside = false;
                    } else if !inside && line.starts_with("| Stage |") {
                        failures.push(format!(
                            "{where_}:{}: a stage table outside a stage-spec block",
                            n + 1
                        ));
                    }
                }
            }
            for (n, line) in text.lines().enumerate() {
                for (i, _) in line.match_indices("incremental-modeling.md#stage-") {
                    let anchor: String = line[i + "incremental-modeling.md#".len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect();
                    let is_stage = anchor
                        .strip_prefix("stage-")
                        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
                    if is_stage && !anchors.contains(&anchor) {
                        failures.push(format!(
                            "{where_}:{}: `#{anchor}` names no stage section",
                            n + 1
                        ));
                    }
                }
            }
        }
        if bless {
            return;
        }
        assert!(
            failures.is_empty(),
            "{}\nrun `RDRA_ISH_BLESS=1 cargo test -p rdra-ish-core stage_docs` to rewrite the blocks",
            failures.join("\n")
        );
    }

    #[test]
    fn review_templates_use_only_the_known_placeholders() {
        for id in stages() {
            for template in spec(id).review {
                for (i, _) in template.match_indices('{') {
                    let name: String = template[i + 1..]
                        .chars()
                        .take_while(|c| *c != '}')
                        .collect();
                    assert!(
                        ["src", "buc", "entity"].contains(&name.as_str()),
                        "{}: unknown placeholder {{{name}}} in `{template}`",
                        id.slug()
                    );
                }
            }
        }
    }

    #[test]
    fn the_validate_column_abbreviates_a_template() {
        assert_eq!(
            short_command("rdra-ish diagram {src} --kind rdra --format mermaid --buc {buc}"),
            "diagram --kind rdra"
        );
        assert_eq!(
            short_command("rdra-ish states {src} --entity {entity}"),
            "states"
        );
        assert_eq!(
            short_command("rdra-ish list {src} --kind buc --format table"),
            "list --kind buc --format table"
        );
    }
}
