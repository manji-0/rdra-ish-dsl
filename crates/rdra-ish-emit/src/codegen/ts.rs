//! The TypeScript printer: branded scalars and ids, string-literal unions, interfaces.

use super::ir::{DomainModel, FieldType, RecordKind, Scalar};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

pub(super) fn print(domain: &DomainModel, options: &Options) -> String {
    let mut out = String::new();
    for line in header_lines(options) {
        out.push_str(&comment(&line));
    }
    out.push_str("//\n");
    for line in obligation_lines(domain) {
        out.push_str(&comment(&line));
    }
    out.push('\n');

    out.push_str("// ── Scalars: replace with the project's own types if it has them ──\n\n");
    for scalar in Scalar::PRELUDE {
        out.push_str(&doc(&[scalar.default_doc().to_string()], ""));
        out.push_str(&format!(
            "export type {0} = string & {{ readonly __brand: \"{0}\" }};\n\n",
            scalar.name()
        ));
    }

    if !domain.ids.is_empty() {
        out.push_str("// ── Ids ──\n\n");
    }
    for id in &domain.ids {
        out.push_str(&doc(&[format!("The id of entity {}.", id.entity)], ""));
        out.push_str(&format!(
            "export type {0} = {1} & {{ readonly __brand: \"{0}\" }};\n\n",
            id.name,
            scalar(id.repr)
        ));
    }

    if !domain.enums.is_empty() {
        out.push_str("// ── Value sets ──\n\n");
    }
    for decl in &domain.enums {
        out.push_str(&doc(&decl.doc, ""));
        let values: Vec<String> = decl
            .values
            .iter()
            .map(|v| string_literal(&v.wire))
            .collect();
        out.push_str(&format!("export type {} =\n", decl.name));
        for (i, value) in decl.values.iter().enumerate() {
            let label = value
                .label
                .as_ref()
                .map(|l| format!(" // {}", one_line(l)))
                .unwrap_or_default();
            let bar = if i == 0 { "  " } else { "  | " };
            out.push_str(&format!("{bar}{}{label}\n", string_literal(&value.wire)));
        }
        out.push_str(";\n");
        out.push_str(&format!(
            "export const {0}Values: readonly {0}[] = [{1}];\n\n",
            decl.name,
            values.join(", ")
        ));
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
            out.push_str(&format!("export interface {} {{\n", record.name));
            for field in &record.fields {
                out.push_str(&doc(&field.doc, "  "));
                let ty = field_type(&field.ty);
                let ty = if field.nullable {
                    format!("{ty} | null")
                } else {
                    ty
                };
                out.push_str(&format!("  {}: {ty};\n", property(&field.wire)));
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
        Scalar::Int => "number",
        Scalar::String => "string",
        Scalar::Bool => "boolean",
        other => other.name(),
    }
}

/// A property name as written: bare when it is an ASCII identifier, quoted otherwise.
fn property(name: &str) -> String {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if plain {
        name.to_string()
    } else {
        string_literal(name)
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
    let lines: Vec<String> = lines
        .iter()
        .map(|l| one_line(l).replace("*/", "* /"))
        .filter(|l| !l.is_empty())
        .collect();
    match lines.as_slice() {
        [] => String::new(),
        [one] => format!("{indent}/** {one} */\n"),
        many => {
            let mut out = format!("{indent}/**\n");
            for line in many {
                out.push_str(&format!("{indent} * {line}\n"));
            }
            out.push_str(&format!("{indent} */\n"));
            out
        }
    }
}
