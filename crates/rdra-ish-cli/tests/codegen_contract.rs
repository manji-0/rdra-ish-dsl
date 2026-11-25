//! The part of `codegen --lang ts` that docs/compatibility.md covers ("The TypeScript types
//! of `codegen`"): `Result`, `InvalidValue`, `InvalidRow`, the scalars, ids, value sets, state
//! types and `<Entity>Kind`, the `<Entity>Row` types, the signatures of their companion objects
//! (`parse`, `is`, `values`, `fromRow`, `toRow`), and `parse<Entity>Row`, with comments and
//! function bodies left out. The snapshot holds only those declarations, so a change to the
//! experimental rest (functions, rule checks, dispatchers, the header, the bodies) leaves it as
//! it is, and a change to a covered declaration shows up as a diff to review against the
//! contract.

use std::path::Path;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/codegen/covered_types.rdra";

/// The sections whose every exported declaration is covered; in `Rows`, the row types and
/// their parsers.
const COVERED_SECTIONS: &[&str] = &[
    "Results and errors",
    "Scalars",
    "Ids",
    "Value sets",
    "Entities with states",
    "Rows",
];

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
    let covered = covered(&String::from_utf8(output.stdout).unwrap());
    // Every kind of covered declaration made it through the filter.
    for expected in [
        "export type Result<T, E>",
        "export type InvalidRow",
        "export const Money = {",
        "  parse: (value: unknown): Result<Money, InvalidValue>",
        "export type OrderKind =",
        "  fromRow: (row: OrderRow): Result<Order, InvalidRow>",
        "export function parseOrderRow(value: unknown): Result<OrderRow, InvalidRow>",
    ] {
        assert!(covered.contains(expected), "{expected} missing:\n{covered}");
    }
    insta::assert_snapshot!(covered);
}

/// What one exported declaration keeps.
enum Block {
    /// A type: every line to its end (`}>;`, or the `;` of its last union case).
    Type,
    /// A companion object: the member signatures, to `} as const;`.
    Companion,
}

/// The covered declarations of a generated module, without comments and bodies.
fn covered(module: &str) -> String {
    let mut out = String::new();
    let mut section = "";
    let mut block: Option<Block> = None;
    for line in module.lines() {
        let code = uncommented(line);
        match block {
            Some(Block::Type) => {
                if !code.is_empty() {
                    out.push_str(&code);
                    out.push('\n');
                }
                if code == "}>;" || (code.starts_with("  | ") && code.ends_with(';')) {
                    block = None;
                }
                continue;
            }
            Some(Block::Companion) => {
                if code == "} as const;" {
                    out.push_str("} as const;\n");
                    block = None;
                } else if let Some(member) = member_signature(&code) {
                    out.push_str(&member);
                    out.push('\n');
                }
                continue;
            }
            None => {}
        }
        if let Some(title) = line.strip_prefix("// ── ") {
            section = COVERED_SECTIONS
                .iter()
                .find(|s| title.starts_with(**s))
                .copied()
                .unwrap_or("");
            continue;
        }
        if section.is_empty() || !line.starts_with("export ") {
            continue;
        }
        if let Some(rest) = code.strip_prefix("export function ") {
            // In `Rows`, `parse<Entity>Row`: its signature.
            if section == "Rows" && rest.starts_with("parse") {
                out.push_str(code.trim_end_matches(" {"));
                out.push('\n');
            }
            continue;
        }
        if code.starts_with("export const ") {
            out.push_str(&code);
            out.push('\n');
            block = Some(Block::Companion);
            continue;
        }
        // `export type`: whole when it ends on its line, else to its end.
        out.push_str(&code);
        out.push('\n');
        if !code.ends_with(';') {
            block = Some(Block::Type);
        }
    }
    out
}

/// A companion member's name and type: `  parse: (value: unknown): Result<..>` from its
/// definition, or the whole line of a member that is not a function.
fn member_signature(code: &str) -> Option<String> {
    let member = code.strip_prefix("  ")?;
    if member.starts_with(' ') || !member.contains(": ") {
        return None;
    }
    let head = member
        .split_once(" =>")
        .map_or(member, |(signature, _)| signature);
    Some(format!("  {head}"))
}

/// `line` without a comment: empty for a comment line, a trailing `// label` cut.
fn uncommented(line: &str) -> String {
    let trimmed = line.trim_start();
    if ["/**", "*", "//"].iter().any(|p| trimmed.starts_with(p)) {
        return String::new();
    }
    line.split_once(" // ")
        .map_or(line, |(code, _)| code)
        .to_string()
}
