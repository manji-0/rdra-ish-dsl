//! The Go printer: named scalars and ids, string enums with their constants, structs with
//! `json` tags naming the columns. A nullable column is a pointer.

use super::ir::{DomainModel, FieldType, RecordKind, Scalar};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

pub(super) fn print(domain: &DomainModel, options: &Options) -> String {
    let mut out = String::new();
    let mut head = vec![format!(
        "Package {} holds the domain types of the model.",
        options.package
    )];
    head.push(String::new());
    head.extend(header_lines(options));
    head.push(String::new());
    head.extend(obligation_lines(domain));
    for line in head {
        out.push_str(&comment(&line, ""));
    }
    out.push_str(&format!("package {}\n\n", options.package));

    out.push_str("// ── Scalars: replace with the project's own types if it has them ──\n\n");
    for scalar in Scalar::PRELUDE {
        out.push_str(&doc(
            scalar.name(),
            &[format!("is {}", lower_first(scalar.default_doc()))],
            "",
        ));
        out.push_str(&format!("type {} string\n\n", scalar.name()));
    }

    if !domain.ids.is_empty() {
        out.push_str("// ── Ids ──\n\n");
    }
    for id in &domain.ids {
        out.push_str(&doc(
            &id.name,
            &[format!("is the id of entity {}.", id.entity)],
            "",
        ));
        out.push_str(&format!("type {} {}\n\n", id.name, scalar(id.repr)));
    }

    if !domain.enums.is_empty() {
        out.push_str("// ── Value sets ──\n\n");
    }
    for decl in &domain.enums {
        let mut lines = vec![format!("is {}", one_line(&decl.doc.join(" ")))];
        lines.push(format!(
            "Call Valid on a value read from outside: only the {}* constants are values.",
            decl.name
        ));
        out.push_str(&doc(&decl.name, &lines, ""));
        out.push_str(&format!("type {} string\n\n", decl.name));
        out.push_str(&format!("// The values of {}.\nconst (\n", decl.name));
        for value in &decl.values {
            if let Some(label) = &value.label {
                out.push_str(&format!(
                    "\t// {}{} is {}.\n",
                    decl.name,
                    value.ident,
                    one_line(label)
                ));
            }
            out.push_str(&format!(
                "\t{}{} {} = {}\n",
                decl.name,
                value.ident,
                decl.name,
                string_literal(&value.wire)
            ));
        }
        out.push_str(")\n\n");
        out.push_str(&format!(
            "// {0}Values lists every value of {0} in declaration order.\nvar {0}Values = []{0}{{{1}}}\n\n",
            decl.name,
            decl.values
                .iter()
                .map(|v| format!("{}{}", decl.name, v.ident))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!(
            "// Valid reports whether v is one of the values of {0}.\nfunc (v {0}) Valid() bool {{\n\tfor _, x := range {0}Values {{\n\t\tif v == x {{\n\t\t\treturn true\n\t\t}}\n\t}}\n\treturn false\n}}\n\n",
            decl.name
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
            out.push_str(&doc(
                &record.name,
                &[format!("is {}", one_line(&record.doc.join(" ")))],
                "",
            ));
            out.push_str(&format!("type {} struct {{\n", record.name));
            for field in &record.fields {
                for line in &field.doc {
                    out.push_str(&comment(&one_line(line), "\t"));
                }
                let ty = field_type(&field.ty);
                let ty = if field.nullable { format!("*{ty}") } else { ty };
                out.push_str(&format!(
                    "\t{} {ty} `json:{}`\n",
                    field.pascal,
                    string_literal(&field.wire)
                ));
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
        Scalar::Int => "int64",
        Scalar::String => "string",
        Scalar::Bool => "bool",
        other => other.name(),
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn comment(line: &str, indent: &str) -> String {
    if line.is_empty() {
        format!("{indent}//\n")
    } else {
        format!("{indent}// {line}\n")
    }
}

/// A Go doc comment: the first line starts with the declared name, as `go vet` and godoc
/// expect.
fn doc(name: &str, lines: &[String], indent: &str) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let line = one_line(line);
        if i == 0 {
            out.push_str(&format!("{indent}// {name} {line}\n"));
        } else if !line.is_empty() {
            out.push_str(&format!("{indent}// {line}\n"));
        }
    }
    out
}
