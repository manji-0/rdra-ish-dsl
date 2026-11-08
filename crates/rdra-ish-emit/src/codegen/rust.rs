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
    print_validators(domain, &mut out);
    print_rows(domain, &mut out);
    print_dispatchers(domain, &mut out);
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
        if event.fields.is_empty() {
            out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
            out.push_str(&format!("pub struct {};\n\n", event.name));
        } else {
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
            out.push_str(&format!("pub struct {} {{\n", event.name));
            for field in &event.fields {
                out.push_str(&format!(
                    "    /// The {} row the event was raised on.\n    pub {}: {},\n",
                    field.entity,
                    ident(&field.name),
                    field.id_ty
                ));
            }
            out.push_str("}\n\n");
        }
    }
    if domain.fns.is_empty() {
        return;
    }
    if !domain.marks.is_empty() {
        out.push_str("// ── Run marks of use cases split into transactions ──\n\n");
    }
    for m in &domain.marks {
        out.push_str(&format!(
            "/// The mark step {} of {} leaves on a {} row: the next step takes it. Store it with the row.\n",
            m.step, m.usecase, m.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!(
            "pub struct {} {{\n    pub id: {},\n}}\n\n",
            m.name, m.id_ty
        ));
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
        for m in &f.marks_out {
            out.push_str(&format!("    pub {}: {},\n", ident(&m.name), m.ty));
        }
        for event in &f.raises {
            out.push_str(&format!(
                "    pub {}: {},\n",
                ident(&snake(&event.ty)),
                event.ty
            ));
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
        for m in &f.marks_in {
            params.push(format!("{}: {}", ident(&m.name), m.ty));
        }
        if let Some(ev) = &f.event_in {
            params.push(format!("{}: {}", ident(&ev.name), ev.ty));
        }
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
                    let (c, p) = (
                        format!("{}.{}", row(*child), ident(&child_field.wire)),
                        format!("{}.{}", row(*parent), ident(&parent_field.wire)),
                    );
                    let differs = if child_field.nullable {
                        format!("{c}.as_ref() != ::core::option::Option::Some(&{p})")
                    } else {
                        format!("{c} != {p}")
                    };
                    out.push_str(&format!(
                        "    if {differs} {{\n        panic!({});\n    }}\n",
                        string_literal(message)
                    ));
                }
                ArgCheck::EventRow {
                    row: r,
                    field,
                    event_field,
                    message,
                } => {
                    let ev = f
                        .event_in
                        .as_ref()
                        .expect("an event row check has an event");
                    out.push_str(&format!(
                        "    if {}.{} != {}.{} {{\n        panic!({});\n    }}\n",
                        row(*r),
                        ident(&field.wire),
                        ident(&ev.name),
                        ident(event_field),
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
        // The marks first: the result below moves the rows' fields.
        for m in &f.marks_out {
            let id = match &m.source {
                super::ir::MarkSource::Row { row: r, field } => {
                    format!("{}.{}.clone()", row(*r), ident(&field.wire))
                }
                super::ir::MarkSource::Input(i) => format!("{}.clone()", input(*i)),
                super::ir::MarkSource::Mark(k) => {
                    format!("{}.id.clone()", ident(&f.marks_in[*k].name))
                }
            };
            out.push_str(&format!(
                "    let out_{} = {} {{ id: {id} }};\n",
                m.name, m.ty
            ));
        }
        for event in &f.raises {
            let mut parts = Vec::new();
            for (field, source) in &event.ids {
                let id = match source {
                    super::ir::MarkSource::Row { row: r, field } => {
                        format!("{}.{}.clone()", row(*r), ident(&field.wire))
                    }
                    super::ir::MarkSource::Input(i) => format!("{}.clone()", input(*i)),
                    super::ir::MarkSource::Mark(k) => {
                        format!("{}.id.clone()", ident(&f.marks_in[*k].name))
                    }
                };
                parts.push(format!("{}: {id}", ident(&field.name)));
            }
            let value = if parts.is_empty() {
                event.ty.clone()
            } else {
                format!("{} {{ {} }}", event.ty, parts.join(", "))
            };
            out.push_str(&format!("    let out_{} = {value};\n", snake(&event.ty)));
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
                            Assigned::Switch {
                                row: r,
                                field,
                                enum_ty,
                                arms,
                            } => {
                                let mut v = format!("match {}.{} {{ ", row(*r), ident(&field.wire));
                                for (from, to) in arms {
                                    v.push_str(&format!(
                                        "{enum_ty}::{} => {enum_ty}::{}, ",
                                        variant_ident(&from.ident),
                                        variant_ident(&to.ident)
                                    ));
                                }
                                let all = domain
                                    .enums
                                    .iter()
                                    .find(|e| e.name == *enum_ty)
                                    .map_or(0, |e| e.values.len());
                                if arms.len() < all {
                                    v.push_str(&format!(
                                        "_ => unreachable!({}), ",
                                        string_literal(&format!(
                                            "rdra-ish: {} is in no state {} moves it from",
                                            field.wire, f.usecase
                                        ))
                                    ));
                                }
                                v.push('}');
                                v
                            }
                            Assigned::EventId { field, wrap: w, .. } => wrap(
                                format!(
                                    "{}.{}.clone()",
                                    ident(
                                        &f.event_in
                                            .as_ref()
                                            .expect("an event id has an event")
                                            .name
                                    ),
                                    ident(field)
                                ),
                                *w,
                            ),
                            Assigned::MarkId { mark, wrap: w } => {
                                wrap(format!("{}.id.clone()", ident(&f.marks_in[*mark].name)), *w)
                            }
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
        for m in &f.marks_out {
            body.push_str(&format!("        {}: out_{},\n", ident(&m.name), m.name));
        }
        for event in &f.raises {
            body.push_str(&format!(
                "        {}: out_{},\n",
                ident(&snake(&event.ty)),
                snake(&event.ty)
            ));
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

// ── Rule checks ──────────────────────────────────────────────────────────────

pub(super) fn print_validators(domain: &DomainModel, out: &mut String) {
    if domain.validators.is_empty() {
        return;
    }
    out.push_str("// ── Rule checks ──\n\n");
    let row = |_: usize| "self".to_string();
    for v in &domain.validators {
        out.push_str(&format!("impl {} {{\n", v.ty));
        out.push_str(
            "    /// The rules this row breaks, by the names `check` reports: empty when it keeps them all.\n",
        );
        out.push_str("    pub fn rule_violations(&self) -> ::std::vec::Vec<&'static str> {\n");
        if v.rules.is_empty() {
            out.push_str("        ::std::vec::Vec::new()\n    }\n}\n\n");
            continue;
        }
        out.push_str("        let mut out = ::std::vec::Vec::new();\n");
        for rule in &v.rules {
            out.push_str(&format!(
                "        // {}\n        if {} {{\n            out.push({});\n        }}\n",
                one_line(&rule.text),
                expr(&fails(&rule.expr), &row),
                string_literal(&rule.name)
            ));
        }
        out.push_str("        out\n    }\n}\n\n");
    }
    for sum in &domain.sums {
        out.push_str(&format!("impl {} {{\n", sum.name));
        out.push_str(
            "    /// The rules this row breaks, by its state: empty when it keeps them all.\n",
        );
        out.push_str("    pub fn rule_violations(&self) -> ::std::vec::Vec<&'static str> {\n        match self {\n");
        for variant in &sum.variants {
            out.push_str(&format!(
                "            Self::{}(state) => state.rule_violations(),\n",
                variant_ident(&variant.value.ident)
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
    }
}

/// The condition under which `e` fails, folded so that a negated comparison reads plainly.
fn fails(e: &super::ir::Expr) -> super::ir::Expr {
    super::actions::simplify(super::ir::Expr::Not(Box::new(e.clone())))
}

// ── Rows and dispatch ────────────────────────────────────────────────────────

pub(super) fn print_rows(domain: &DomainModel, out: &mut String) {
    if domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Rows: the flat shape a store or the wire holds ──\n\n");
    out.push_str("/// A row that breaks what the type of its state requires.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str("pub struct InvalidRow {\n    pub entity: &'static str,\n    pub column: &'static str,\n    pub reason: &'static str,\n}\n\n");
    for sum in &domain.sums {
        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
            continue;
        };
        out.push_str(&format!(
            "/// Every column of {} as the data layer declares it.\n",
            sum.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", sum.row));
        for f in &sum.row_fields {
            let ty = field_type(&f.ty);
            let ty = if f.nullable {
                format!("{OPTION}<{ty}>")
            } else {
                ty
            };
            out.push_str(&format!("    pub {}: {ty},\n", ident(&f.wire)));
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl {} {{\n", sum.name));
        out.push_str("    /// The state of a row, or why the row fits none.\n");
        out.push_str(&format!(
            "    pub fn from_row(row: {}) -> ::core::result::Result<Self, InvalidRow> {{\n        match row.{} {{\n",
            sum.row,
            ident(&sum.tag.wire)
        ));
        for v in &sum.variants {
            let vi = variant_ident(&v.value.ident);
            out.push_str(&format!("            {tag_enum}::{vi} => {{\n"));
            let invalid = |column: &str, reason: String| {
                format!(
                    "InvalidRow {{ entity: {}, column: {}, reason: {} }}",
                    string_literal(&sum.entity),
                    string_literal(column),
                    string_literal(&reason)
                )
            };
            for f in &v.always_null {
                out.push_str(&format!(
                    "                if row.{}.is_some() {{\n                    return ::core::result::Result::Err({});\n                }}\n",
                    ident(&f.wire),
                    invalid(&f.wire, format!("null in the state {}", v.value.wire))
                ));
            }
            let mut inits = Vec::new();
            for f in &v.fields {
                let declared = sum.row_fields.iter().find(|r| r.wire == f.wire);
                if declared.is_some_and(|d| d.nullable) && !f.nullable {
                    let local = format!("present_{}", f.wire);
                    out.push_str(&format!(
                        "                let ::core::option::Option::Some({local}) = row.{} else {{\n                    return ::core::result::Result::Err({});\n                }};\n",
                        ident(&f.wire),
                        invalid(&f.wire, format!("present in the state {}", v.value.wire))
                    ));
                    inits.push(format!("{}: {local}", ident(&f.wire)));
                } else {
                    inits.push(format!("{0}: row.{0}", ident(&f.wire)));
                }
            }
            out.push_str(&format!(
                "                ::core::result::Result::Ok(Self::{vi}({} {{ {} }}))\n            }}\n",
                v.name,
                inits.join(", ")
            ));
        }
        out.push_str("        }\n    }\n\n");
        out.push_str("    /// The flat row of this state.\n");
        out.push_str(&format!(
            "    pub fn into_row(self) -> {} {{\n        match self {{\n",
            sum.row
        ));
        for v in &sum.variants {
            let vi = variant_ident(&v.value.ident);
            let mut inits = Vec::new();
            for f in &sum.row_fields {
                let name = ident(&f.wire);
                if f.wire == sum.tag.wire {
                    inits.push(format!("{name}: {tag_enum}::{vi}"));
                } else if v.always_null.iter().any(|n| n.wire == f.wire) {
                    inits.push(format!("{name}: None"));
                } else if let Some(vf) = v.fields.iter().find(|x| x.wire == f.wire) {
                    if f.nullable && !vf.nullable {
                        inits.push(format!("{name}: Some(state.{name})"));
                    } else {
                        inits.push(format!("{name}: state.{name}"));
                    }
                }
            }
            out.push_str(&format!(
                "            Self::{vi}(state) => {} {{ {} }},\n",
                sum.row,
                inits.join(", ")
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
    }
}

pub(super) fn print_dispatchers(domain: &DomainModel, out: &mut String) {
    use super::actions::snake;
    if domain.dispatchers.is_empty() {
        return;
    }
    out.push_str("// ── Dispatch: run a use case from whatever states its rows are in ──\n\n");
    out.push_str("/// The rows are in states the use case does not run from.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str("pub struct NotEnabled {\n    pub usecase: &'static str,\n}\n\n");
    out.push_str("/// Why a use case did not run.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str("pub enum Refused {\n    Rejected(Rejected),\n    NotEnabled(NotEnabled),\n}\n\n");
    for d in &domain.dispatchers {
        out.push_str(&format!(
            "/// What `{}` returns when the use case runs.\n",
            snake(&d.name)
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub enum {} {{\n", d.outcome));
        for c in &d.cases {
            out.push_str(&format!("    {}({}),\n", c.case, c.result));
        }
        out.push_str("}\n\n");
        out.push_str(&format!(
            "/// Runs {} from whatever states its rows are in: the transition function of those states, or `NotEnabled`.\n",
            d.usecase
        ));
        let mut params: Vec<String> = d
            .rows
            .iter()
            .map(|(n, ty)| format!("{}: {ty}", ident(n)))
            .collect();
        for (n, ty) in &d.marks {
            params.push(format!("{}: {ty}", ident(n)));
        }
        for i in &d.inputs {
            let ty = field_type(&i.ty);
            let ty = if i.nullable {
                format!("{OPTION}<{ty}>")
            } else {
                ty
            };
            params.push(format!("{}: {ty}", ident(&i.name)));
        }
        for (n, ty) in &d.choices {
            params.push(format!("{}: {ty}", ident(n)));
        }
        out.push_str(&format!(
            "pub fn {}({}) -> ::core::result::Result<{}, Refused> {{\n",
            snake(&d.name),
            params.join(", "),
            d.outcome
        ));
        let scrutinee: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
        out.push_str(&format!("    match ({},) {{\n", scrutinee.join(", ")));
        for c in &d.cases {
            let pats: Vec<String> = c
                .states
                .iter()
                .zip(&d.rows)
                .map(|(s, (n, ty))| match s {
                    Some((_, _, value)) => {
                        format!("{ty}::{}({})", variant_ident(&value.ident), ident(n))
                    }
                    None => ident(n),
                })
                .collect();
            let mut args: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
            args.extend(d.marks.iter().map(|(n, _)| ident(n)));
            args.extend(c.inputs.iter().map(|i| ident(&d.inputs[*i].name)));
            let call = format!("{}({})", snake(&c.function), args.join(", "));
            let body = if c.rejects {
                format!(
                    "{call}.map({}::{}).map_err(Refused::Rejected)",
                    d.outcome, c.case
                )
            } else {
                format!(
                    "::core::result::Result::Ok({}::{}({call}))",
                    d.outcome, c.case
                )
            };
            let guard: Vec<String> = c
                .choice
                .iter()
                .map(|(i, v)| {
                    let (n, ty) = &d.choices[*i];
                    format!("{} == {ty}::{}", ident(n), variant_ident(&v.ident))
                })
                .collect();
            let guard = if guard.is_empty() {
                String::new()
            } else {
                format!(" if {}", guard.join(" && "))
            };
            out.push_str(&format!(
                "        ({},){guard} => {body},\n",
                pats.join(", ")
            ));
        }
        // A catch-all arm only where the cases leave some combination of states out.
        let combos: usize = d
            .rows
            .iter()
            .enumerate()
            .map(|(r, (_, ty))| {
                if d.cases.iter().any(|c| c.states[r].is_some()) {
                    domain
                        .sums
                        .iter()
                        .find(|s| s.name == *ty)
                        .map_or(1, |s| s.variants.len())
                } else {
                    1
                }
            })
            .product();
        // A guarded arm never counts as covering its states.
        let guarded = d.cases.iter().any(|c| !c.choice.is_empty());
        if guarded || d.cases.len() < combos {
            out.push_str(&format!(
                "        _ => ::core::result::Result::Err(Refused::NotEnabled(NotEnabled {{ usecase: {} }})),\n",
                string_literal(&d.usecase)
            ));
        }
        out.push_str("    }\n}\n\n");
    }
}
