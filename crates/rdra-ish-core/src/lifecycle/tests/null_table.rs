//! The truth table of a comparison over null in the language reference (§ Null), computed
//! here through the builder and the evaluator, so the table cannot drift from them (P62).
//!
//! Each row is a `@check` on a row whose `x` and `y` (`Int @null`) are null and whose `k`
//! (`Int`) is 3: the check fails exactly where the comparison is false. Its negation is a
//! second `@check`. A rule reads every comparison the same way (`forbidden`, `required`,
//! `invariant`, `holds`, `property`), so `@check` stands for all of them.

use super::{derive_src, StateDiag};

const SHAPES: [(&str, &str); 4] = [("x", "3"), ("x", "k"), ("k", "x"), ("x", "y")];
const OPS: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];

/// Whether `expr` and `not (expr)` hold on the row.
fn truth(expr: &str) -> (bool, bool) {
    let src = format!(
        r#"
entity Item "品" {{
  id: Int @pk
  k: Int @default(3)
  y: Int @null
  x: Int @null @check({expr}) @check(not ({expr}))
}}
usecase Make "作成"
creates(Make, Item)
"#
    );
    let d = derive_src(&src);
    let failed = |n: usize| {
        d.diags.iter().any(|x| {
            matches!(&x.diag, StateDiag::ConstraintViolated { name, .. }
                if name == &format!("Check_Item_x_{n}"))
        })
    };
    (!failed(0), !failed(1))
}

fn cell(expr: &str) -> String {
    let word = |b: bool| if b { "true" } else { "false" };
    let (plain, negated) = truth(expr);
    format!("{} / {}", word(plain), word(negated))
}

/// One row per operand shape, one column per operator; a cell is `expr / not (expr)`. Then
/// the comparisons with `null` itself and the sets.
fn table() -> String {
    let mut out = format!(
        "| Operands | {} |\n|---|",
        OPS.map(|op| format!("`{op}`")).join(" | ")
    );
    out.push_str(&"---|".repeat(OPS.len()));
    out.push('\n');
    for (l, r) in SHAPES {
        let cells: Vec<String> = OPS
            .iter()
            .map(|op| cell(&format!("{l} {op} {r}")))
            .collect();
        out.push_str(&format!("| `{l}`, `{r}` | {} |\n", cells.join(" | ")));
    }
    out.push_str("\n| Comparison | Holds / `not` holds |\n|---|---|\n");
    for expr in ["x == null", "x != null", "x in (1, 3)", "x not in (1, 3)"] {
        out.push_str(&format!("| `{expr}` | {} |\n", cell(expr)));
    }
    out
}

/// The table between the markers in docs/language-reference.md is the one the evaluator
/// gives. On a mismatch, paste the printed table between the markers.
#[test]
fn the_null_truth_table_in_the_reference_is_the_evaluators() {
    let reference = include_str!("../../../../../docs/language-reference.md");
    let (begin, end) = (
        "<!-- null-truth-table:begin -->\n",
        "<!-- null-truth-table:end -->",
    );
    let start = reference.find(begin).expect("begin marker") + begin.len();
    let stop = reference.find(end).expect("end marker");
    let documented = reference[start..stop].trim();
    let computed = table();
    assert_eq!(
        documented,
        computed.trim(),
        "the table in docs/language-reference.md (§ Null) differs from the evaluator; it reads:\n{computed}"
    );
}

/// The cells the owner's review (P62) names, stated outright so a regenerated table that
/// changed them fails here too.
#[test]
fn null_meets_no_comparison_but_every_ne_and_not_flips_that() {
    assert_eq!(truth("x != 3"), (true, false));
    assert_eq!(truth("x < 3"), (false, true));
    assert_eq!(truth("x == y"), (false, true));
    assert_eq!(truth("x != y"), (true, false));
    assert_eq!(truth("x >= 3"), (false, true));
}
