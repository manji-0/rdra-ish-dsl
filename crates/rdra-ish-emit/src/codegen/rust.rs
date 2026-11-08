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
    out.push_str("\n#![allow(dead_code, unused_variables, non_camel_case_types, non_snake_case, clippy::all)]\n\n");

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
    if !domain.sums.is_empty() {
        out.push_str("// ── Entities with states ──\n\n");
    }
    for sum in &domain.sums {
        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
            unreachable!("a tag column is an enum");
        };
        for variant in &sum.variants {
            let mut lines = variant.doc.clone();
            for field in &variant.always_null {
                lines.push(format!(
                    "`{}`: {}",
                    field.wire,
                    field.doc.last().cloned().unwrap_or_default()
                ));
            }
            out.push_str(&doc(&lines, ""));
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
            out.push_str(&format!("pub struct {} {{\n", variant.name));
            for field in &variant.fields {
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
        let mut lines = sum.doc.clone();
        lines.push(format!(
            "One of its states, told apart by `{}`.",
            sum.tag.wire
        ));
        out.push_str(&doc(&lines, ""));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub enum {} {{\n", sum.name));
        for variant in &sum.variants {
            out.push_str(&format!(
                "    {}({}),\n",
                variant_ident(&variant.value.ident),
                variant.name
            ));
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl {} {{\n", sum.name));
        out.push_str(&format!("    /// The state's `{}`.\n", sum.tag.wire));
        out.push_str(&format!(
            "    pub fn {}(&self) -> {tag_enum} {{\n        match self {{\n",
            ident(&sum.tag.wire)
        ));
        for variant in &sum.variants {
            let v = variant_ident(&variant.value.ident);
            out.push_str(&format!("            Self::{v}(_) => {tag_enum}::{v},\n"));
        }
        out.push_str("        }\n    }\n}\n\n");
        for variant in &sum.variants {
            out.push_str(&format!(
                "impl ::core::convert::From<{0}> for {1} {{\n    fn from(state: {0}) -> Self {{\n        Self::{2}(state)\n    }}\n}}\n\n",
                variant.name,
                sum.name,
                variant_ident(&variant.value.ident)
            ));
        }
    }
    print_functions(domain, &mut out);
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

/// An enum variant: `Self` is the one PascalCase word Rust does not take.
fn variant_ident(name: &str) -> String {
    if name == "Self" {
        "Self_".to_string()
    } else {
        name.to_string()
    }
}

// ── Use cases ────────────────────────────────────────────────────────────────

pub(super) fn print_functions(domain: &DomainModel, out: &mut String) {
    use super::actions::snake;
    use super::ir::{Adapt, ArgCheck, Assigned, OutKind};
    if !domain.events.is_empty() {
        out.push_str("// ── Events ──\n\n");
    }
    for event in &domain.events {
        out.push_str(&doc(&event.doc, ""));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {};\n\n", event.name));
    }
    if domain.fns.is_empty() {
        return;
    }
    out.push_str("// ── Use cases ──\n\n");
    out.push_str("/// A condition of the use case did not hold: the use case does not run.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str("pub struct Rejected {\n    pub condition: &'static str,\n    pub label: &'static str,\n    pub site: ::core::option::Option<&'static str>,\n}\n\n");
    for f in &domain.fns {
        out.push_str(&format!("/// What `{}` returns.\n", snake(&f.name)));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", f.result));
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    out.push_str(&format!("    pub {}: {ty},\n", ident(&o.name)))
                }
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => out.push_str(&format!("    pub {}: {ty},\n", ident(&o.name))),
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            out.push_str(&format!("    pub {}: {event},\n", ident(&snake(event))));
        }
        out.push_str("}\n\n");

        let mut lines = f.doc.clone();
        for c in &f.conditions {
            lines.push(format!(
                "Requires the condition {} ({}){}.",
                c.id,
                one_line(&c.label),
                c.site
                    .as_ref()
                    .map(|s| format!(" at {s}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str(&doc(&lines, ""));
        let row = |r: usize| ident(&f.rows[r].name);
        let input = |i: usize| ident(&f.inputs[i].name);
        let mut params: Vec<String> = f
            .rows
            .iter()
            .map(|r| format!("{}: {}", ident(&r.name), r.ty))
            .collect();
        for i in &f.inputs {
            let ty = field_type(&i.ty);
            let ty = if i.nullable {
                format!("{OPTION}<{ty}>")
            } else {
                ty
            };
            params.push(format!("{}: {ty}", ident(&i.name)));
        }
        let ret = if f.conditions.is_empty() {
            f.result.clone()
        } else {
            format!("::core::result::Result<{}, Rejected>", f.result)
        };
        out.push_str(&format!(
            "pub fn {}({}) -> {ret} {{\n",
            snake(&f.name),
            params.join(", ")
        ));
        for check in &f.checks {
            match check {
                ArgCheck::Link {
                    child,
                    child_field,
                    parent,
                    parent_field,
                    message,
                } => {
                    out.push_str(&format!(
                        "    if {}.{} != {}.{} {{\n        panic!({});\n    }}\n",
                        row(*child),
                        ident(&child_field.wire),
                        row(*parent),
                        ident(&parent_field.wire),
                        string_literal(message)
                    ));
                }
                ArgCheck::MustBeNull {
                    row: r,
                    field,
                    message,
                } => {
                    out.push_str(&format!(
                        "    if {}.{}.is_some() {{\n        panic!({});\n    }}\n",
                        row(*r),
                        ident(&field.wire),
                        string_literal(message)
                    ));
                }
            }
        }
        for c in &f.conditions {
            out.push_str(&format!(
                "    if !({}) {{\n        return ::core::result::Result::Err(Rejected {{ condition: {}, label: {}, site: {} }});\n    }}\n",
                expr(&c.expr, &row),
                string_literal(&c.id),
                string_literal(&c.label),
                c.site.as_ref().map_or("None".to_string(), |s| format!("Some({})", string_literal(s)))
            ));
        }
        let mut body = format!("{} {{\n", f.result);
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, fields, .. } => {
                    body.push_str(&format!("        {}: {ty} {{\n", ident(&o.name)));
                    for (target, assigned) in fields {
                        let wrap = |v: String, w: bool| if w { format!("Some({v})") } else { v };
                        let v = match assigned {
                            Assigned::From {
                                row: r,
                                field,
                                adapt,
                            } => {
                                let read = format!("{}.{}", row(*r), ident(&field.wire));
                                match adapt {
                                    Adapt::Same => read,
                                    Adapt::Wrap => format!("Some({read})"),
                                    Adapt::Unwrap(message) => {
                                        format!("{read}.expect({})", string_literal(message))
                                    }
                                    Adapt::AbsentNull => "None".to_string(),
                                }
                            }
                            Assigned::Lit { lit, wrap: w } => wrap(lit_rust(lit), *w),
                            Assigned::Input { input: i, wrap: w } => wrap(input(*i), *w),
                            Assigned::ParentId {
                                row: r,
                                field,
                                wrap: w,
                            } => wrap(format!("{}.{}.clone()", row(*r), ident(&field.wire)), *w),
                            Assigned::Null => "None".to_string(),
                        };
                        body.push_str(&format!("            {}: {v},\n", ident(&target.wire)));
                    }
                    body.push_str("        },\n");
                }
                OutKind::Deleted {
                    row: r,
                    id: Some((_, field)),
                } => {
                    body.push_str(&format!(
                        "        {}: {}.{},\n",
                        ident(&o.name),
                        row(*r),
                        ident(&field.wire)
                    ));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            body.push_str(&format!("        {}: {event},\n", ident(&snake(event))));
        }
        body.push_str("    }");
        if f.conditions.is_empty() {
            out.push_str(&format!("    {body}\n}}\n\n"));
        } else {
            out.push_str(&format!("    ::core::result::Result::Ok({body})\n}}\n\n"));
        }
    }
}

fn lit_rust(lit: &super::ir::Lit) -> String {
    match lit {
        super::ir::Lit::Enum { ty, value } => format!("{ty}::{}", variant_ident(&value.ident)),
        super::ir::Lit::Bool(b) => b.to_string(),
        super::ir::Lit::Int(n) => n.to_string(),
    }
}

fn expr(e: &super::ir::Expr, row: &dyn Fn(usize) -> String) -> String {
    use super::ir::Expr;
    match e {
        Expr::True => "true".to_string(),
        Expr::False => "false".to_string(),
        Expr::Not(inner) => format!("!({})", expr(inner, row)),
        Expr::And(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row)))
            .collect::<Vec<_>>()
            .join(" && "),
        Expr::Or(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row)))
            .collect::<Vec<_>>()
            .join(" || "),
        Expr::IsNull { row: r, field } => format!("{}.{}.is_none()", row(*r), ident(&field.wire)),
        Expr::EnumEq {
            row: r,
            field,
            enum_ty,
            value,
        } => format!(
            "{}.{} == {enum_ty}::{}",
            row(*r),
            ident(&field.wire),
            variant_ident(&value.ident)
        ),
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => {
            format!("{}.{} == {value}", row(*r), ident(&field.wire))
        }
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => {
            format!("{}.{} {} {value}", row(*r), ident(&field.wire), op.symbol())
        }
    }
}
