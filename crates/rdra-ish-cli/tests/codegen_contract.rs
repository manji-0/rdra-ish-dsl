//! The part of `codegen --lang ts` that docs/compatibility.md covers ("The TypeScript types
//! of `codegen`"): the scalars, ids, value sets, state types, and `<Entity>Row` types, with
//! comments left out. The snapshot holds only those declarations, so a change to the
//! experimental rest (functions, rule checks, dispatchers, the header) leaves it as it is,
//! and a change to a covered type shows up as a diff to review against the contract.

use std::path::Path;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/codegen/covered_types.rdra";

/// The sections whose every declaration is covered.
const COVERED_SECTIONS: &[&str] = &["Scalars", "Ids", "Value sets", "Entities with states"];

#[test]
fn covered_types() {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .args(["codegen", FIXTURE, "--lang", "ts"])
        .output()
        .expect("rdra-ish runs");
    // No `--allow-failing`: the contract covers a model `check` passes.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    insta::assert_snapshot!(covered(&String::from_utf8(output.stdout).unwrap()));
}

/// The covered declarations of a generated module, without comments.
fn covered(module: &str) -> String {
    let mut out = String::new();
    let mut section = "";
    let mut block: Option<(bool, String)> = None;
    for line in module.lines() {
        if let Some((keep, mut text)) = block.take() {
            text.push_str(&uncommented(line));
            if line == "}" || line == ";" || line.starts_with("};") {
                if keep {
                    out.push_str(&text);
                    out.push('\n');
                }
            } else {
                block = Some((keep, text));
            }
            continue;
        }
        if let Some(title) = line.strip_prefix("// ── ") {
            section = COVERED_SECTIONS
                .iter()
                .chain(&["Rows"])
                .find(|s| title.starts_with(**s))
                .copied()
                .unwrap_or("");
            continue;
        }
        if !line.starts_with("export ") {
            continue;
        }
        let keep = COVERED_SECTIONS.contains(&section) || is_row_type(section, line);
        let text = uncommented(line);
        if line.ends_with(';') {
            if keep {
                out.push_str(&text);
                out.push('\n');
            }
        } else {
            block = Some((keep, text));
        }
    }
    out
}

/// `export interface <Entity>Row {` in the rows section; `InvalidRow` is not covered.
fn is_row_type(section: &str, line: &str) -> bool {
    section == "Rows"
        && line
            .strip_prefix("export interface ")
            .and_then(|rest| rest.strip_suffix(" {"))
            .is_some_and(|name| name.ends_with("Row") && name != "InvalidRow")
}

/// `line` and its newline, or nothing for a comment line; a trailing `// label` is cut.
fn uncommented(line: &str) -> String {
    let trimmed = line.trim_start();
    if ["/**", "*", "//"].iter().any(|p| trimmed.starts_with(p)) {
        return String::new();
    }
    let code = line.split_once(" // ").map_or(line, |(code, _)| code);
    format!("{code}\n")
}
