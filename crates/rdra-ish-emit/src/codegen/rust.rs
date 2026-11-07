//! The Rust printer: newtype scalars and ids, fieldless enums, structs. Standard types are
//! written by path, so a model type named `String` or `Option` shadows nothing.

use super::ir::{DomainModel, FieldType, RecordKind, Scalar};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

const OPTION: &str = "::core::option::Option";
const STRING: &str = "::std::string::String";

pub(super) fn print(domain: &DomainModel, options: &Options) -> String {
    let mut out = String::new();
    for line in header_lines(options) {
        out.push_str(&comment(&line));
    }
    out.push_str("//\n");
    for line in obligation_lines(domain) {
        out.push_str(&comment(&line));
    }
    out.push_str("\n#![allow(dead_code, non_camel_case_types, non_snake_case, clippy::all)]\n\n");

    out.push_str("// ── Scalars: replace with the project's own types if it has them ──\n\n");
    for scalar in Scalar::PRELUDE {
        out.push_str(&doc(&[scalar.default_doc().to_string()], ""));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]\n");
        out.push_str(&format!("pub struct {}(pub {STRING});\n\n", scalar.name()));
    }

    if !domain.ids.is_empty() {
        out.push_str("// ── Ids ──\n\n");
    }
    for id in &domain.ids {
        out.push_str(&doc(&[format!("The id of entity {}.", id.entity)], ""));
        let copy = if matches!(id.repr, Scalar::Int | Scalar::Bool) {
            "Copy, "
        } else {
            ""
        };
        out.push_str(&format!(
            "#[derive(Debug, Clone, {copy}PartialEq, Eq, Hash, PartialOrd, Ord)]\n"
        ));
        out.push_str(&format!(
            "pub struct {}(pub {});\n\n",
            id.name,
            scalar(id.repr)
        ));
    }

    if !domain.enums.is_empty() {
        out.push_str("// ── Value sets ──\n\n");
    }
    for decl in &domain.enums {
        out.push_str(&doc(&decl.doc, ""));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\n");
        out.push_str(&format!("pub enum {} {{\n", decl.name));
        for value in &decl.values {
            let mut lines = Vec::new();
            if let Some(label) = &value.label {
                lines.push(label.clone());
            }
            lines.push(format!("`{}`", value.wire));
            out.push_str(&doc(&lines, "    "));
            out.push_str(&format!("    {},\n", value.ident));
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl {} {{\n", decl.name));
        out.push_str("    /// Every value, in declaration order.\n");
        out.push_str(&format!(
            "    pub const ALL: [Self; {}] = [{}];\n\n",
            decl.values.len(),
            decl.values
                .iter()
                .map(|v| format!("Self::{}", v.ident))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str("    /// The value as the model spells it.\n");
        out.push_str("    pub fn as_str(self) -> &'static str {\n        match self {\n");
        for value in &decl.values {
            out.push_str(&format!(
                "            Self::{} => {},\n",
                value.ident,
                string_literal(&value.wire)
            ));
        }
        out.push_str("        }\n    }\n\n");
        out.push_str("    /// The value the model spells `text`, if any.\n");
        out.push_str(&format!(
            "    pub fn parse(text: &str) -> {OPTION}<Self> {{\n        Self::ALL.into_iter().find(|v| v.as_str() == text)\n    }}\n"
        ));
        out.push_str("}\n\n");
    }

    for kind in [RecordKind::Entity, RecordKind::Dto] {
        let records: Vec<_> = domain.records.iter().filter(|r| r.kind == kind).collect();
        if records.is_empty() {
            continue;
        }
        out.push_str(match kind {
            RecordKind::Entity => "// ── Entities ──\n\n",
            RecordKind::Dto => "// ── DTOs ──\n\n",
        });
        for record in records {
            out.push_str(&doc(&record.doc, ""));
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
            out.push_str(&format!("pub struct {} {{\n", record.name));
            for field in &record.fields {
                out.push_str(&doc(&field.doc, "    "));
                let ty = field_type(&field.ty);
                let ty = if field.nullable {
                    format!("{OPTION}<{ty}>")
                } else {
                    ty
                };
                out.push_str(&format!("    pub {}: {ty},\n", ident(&field.wire)));
            }
            out.push_str("}\n\n");
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => scalar(*s).to_string(),
        FieldType::Enum(name) | FieldType::Id(name) => name.clone(),
    }
}

fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Int => "i64",
        Scalar::String => STRING,
        Scalar::Bool => "bool",
        other => other.name(),
    }
}

/// A field name: a keyword is raw (`r#type`), and the keywords a raw identifier cannot be
/// get a trailing `_`.
fn ident(name: &str) -> String {
    const NOT_RAW: [&str; 5] = ["self", "Self", "super", "crate", "_"];
    const KEYWORDS: [&str; 46] = [
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use",
        "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "yield",
    ];
    if NOT_RAW.contains(&name) {
        format!("{name}_")
    } else if KEYWORDS.contains(&name) || name == "try" || name == "gen" {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

fn comment(line: &str) -> String {
    if line.is_empty() {
        "//\n".to_string()
    } else {
        format!("// {line}\n")
    }
}

fn doc(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|l| one_line(l))
        .filter(|l| !l.is_empty())
        .map(|l| format!("{indent}/// {l}\n"))
        .collect()
}
