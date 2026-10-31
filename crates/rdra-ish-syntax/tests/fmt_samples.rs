//! `fmt` over every committed `.rdra` file under `samples/` and `skills/`: the output parses
//! to the same declarations with the same comments, and formatting it again changes nothing.

use std::fs;
use std::path::{Path, PathBuf};

use rdra_ish_syntax::{format_source, parse, Ast, Item};

fn rdra_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rdra_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rdra") {
            out.push(path);
        }
    }
}

/// The declarations of `ast` with every source span removed, so that two parses of the same
/// declarations laid out differently compare equal. `fmt` writes metadata clauses in a fixed
/// order, so the record of which values were written bare is compared as a set.
fn declarations(ast: &Ast) -> String {
    let mut items = ast.items.clone();
    for item in &mut items {
        if let Item::Instance(instance) = item {
            instance.bare_values.sort();
        }
    }
    let debug: Vec<char> = format!("{items:?}").chars().collect();
    let digits = |from: usize| {
        debug[from..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let mut out = String::with_capacity(debug.len());
    let mut at = 0;
    while at < debug.len() {
        // A `start..end` range prints as `..`.
        let start = digits(at);
        if start > 0 && debug[at + start..].starts_with(&['.', '.']) {
            let end = digits(at + start + 2);
            if end > 0 {
                out.push_str("..");
                at += start + 2 + end;
                continue;
            }
        }
        out.push(debug[at]);
        at += 1;
    }
    out
}

fn comments(ast: &Ast) -> Vec<String> {
    let mut texts: Vec<String> = ast
        .comments
        .iter()
        .map(|comment| ast.source[comment.span.clone()].trim_end().to_string())
        .collect();
    texts.sort();
    texts
}

#[test]
fn fmt_keeps_every_sample_the_same_model_and_is_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rdra_files(&root.join("samples"), &mut files);
    rdra_files(&root.join("skills"), &mut files);
    assert!(files.len() > 50, "found only {} files", files.len());

    for file in files {
        let src = fs::read_to_string(&file).unwrap();
        let (before, errors) = parse(&src);
        assert!(errors.is_empty(), "{}: {errors:?}", file.display());
        let formatted = format_source(&src).unwrap();
        let (after, errors) = parse(&formatted);
        assert!(errors.is_empty(), "{}: {errors:?}", file.display());
        assert_eq!(
            declarations(&after),
            declarations(&before),
            "{}",
            file.display()
        );
        assert_eq!(comments(&after), comments(&before), "{}", file.display());
        assert_eq!(
            format_source(&formatted).unwrap(),
            formatted,
            "{}: not idempotent",
            file.display()
        );
    }
}
