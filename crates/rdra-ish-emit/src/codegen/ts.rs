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
    if !domain.sums.is_empty() {
        out.push_str("// ── Entities with states ──\n\n");
    }
    for sum in &domain.sums {
        for variant in &sum.variants {
            out.push_str(&doc(&variant.doc, ""));
            out.push_str(&format!("export interface {} {{\n", variant.name));
            out.push_str(&format!(
                "  {}: {};\n",
                property(&sum.tag.wire),
                string_literal(&variant.value.wire)
            ));
            for field in &variant.fields {
                out.push_str(&doc(&field.doc, "  "));
                let ty = field_type(&field.ty);
                let ty = if field.nullable {
                    format!("{ty} | null")
                } else {
                    ty
                };
                out.push_str(&format!("  {}: {ty};\n", property(&field.wire)));
            }
            for field in &variant.always_null {
                out.push_str(&doc(&field.doc, "  "));
                out.push_str(&format!("  {}: null;\n", property(&field.wire)));
            }
            out.push_str("}\n\n");
        }
        let mut lines = sum.doc.clone();
        lines.push(format!(
            "One of its states, told apart by `{}`.",
            sum.tag.wire
        ));
        out.push_str(&doc(&lines, ""));
        out.push_str(&format!("export type {} =\n", sum.name));
        for (i, variant) in sum.variants.iter().enumerate() {
            let bar = if i == 0 { "  " } else { "  | " };
            out.push_str(&format!("{bar}{}\n", variant.name));
        }
        out.push_str(";\n\n");
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

// ── Use cases ────────────────────────────────────────────────────────────────

pub(super) fn print_functions(domain: &DomainModel, out: &mut String) {
    use super::ir::{Adapt, ArgCheck, Assigned, OutKind};
    if !domain.events.is_empty() {
        out.push_str("// ── Events ──\n\n");
    }
    for event in &domain.events {
        out.push_str(&doc(&event.doc, ""));
        out.push_str(&format!(
            "export interface {0} {{\n  readonly event: \"{0}\";\n}}\n\n",
            event.name
        ));
    }
    if domain.fns.is_empty() {
        return;
    }
    out.push_str("// ── Use cases ──\n\n");
    out.push_str("/** A condition of the use case did not hold: the use case does not run. */\n");
    out.push_str("export interface Rejected {\n  readonly rejected: string;\n  readonly label: string;\n  readonly site: string | null;\n}\n\n");
    out.push_str("/** A state the verified model does not reach: the arguments break a rule. */\n");
    out.push_str(
        "function unreachable(message: string): never {\n  throw new Error(message);\n}\n\n",
    );
    for f in &domain.fns {
        // The result type.
        out.push_str(&doc(
            &[format!("What {} returns.", super::camel(&f.name))],
            "",
        ));
        out.push_str(&format!("export interface {} {{\n", f.result));
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    out.push_str(&format!("  {}: {ty};\n", property(&o.name)));
                }
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => {
                    out.push_str(&format!("  {}: {ty};\n", property(&o.name)));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            out.push_str(&format!(
                "  {}: {event};\n",
                property(&super::actions::snake(event))
            ));
        }
        out.push_str("}\n\n");

        // The function.
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
        let mut params: Vec<String> = f
            .rows
            .iter()
            .map(|r| format!("{}: {}", super::camel(&super::type_ident(&r.name)), r.ty))
            .collect();
        for input in &f.inputs {
            let ty = field_type(&input.ty);
            let ty = if input.nullable {
                format!("{ty} | null")
            } else {
                ty
            };
            params.push(format!(
                "{}: {ty}",
                super::camel(&super::type_ident(&input.name))
            ));
        }
        let ret = if f.conditions.is_empty() {
            f.result.clone()
        } else {
            format!("{} | Rejected", f.result)
        };
        out.push_str(&format!(
            "export function {}({}): {ret} {{\n",
            super::camel(&f.name),
            params.join(", ")
        ));
        let row = |r: usize| super::camel(&super::type_ident(&f.rows[r].name));
        let input = |i: usize| super::camel(&super::type_ident(&f.inputs[i].name));
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
                        "  if ({}.{} !== {}.{}) {{\n    throw new Error({});\n  }}\n",
                        row(*child),
                        property(&child_field.wire),
                        row(*parent),
                        property(&parent_field.wire),
                        string_literal(message)
                    ));
                }
                ArgCheck::MustBeNull {
                    row: r,
                    field,
                    message,
                } => {
                    out.push_str(&format!(
                        "  if ({}.{} !== null) {{\n    unreachable({});\n  }}\n",
                        row(*r),
                        property(&field.wire),
                        string_literal(message)
                    ));
                }
            }
        }
        for c in &f.conditions {
            out.push_str(&format!(
                "  if (!({})) {{\n    return {{ rejected: {}, label: {}, site: {} }};\n  }}\n",
                expr(&c.expr, &row),
                string_literal(&c.id),
                string_literal(&c.label),
                c.site
                    .as_ref()
                    .map_or("null".to_string(), |s| string_literal(s))
            ));
        }
        out.push_str("  return {\n");
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row {
                    tag,
                    fields,
                    always_null,
                    ..
                } => {
                    out.push_str(&format!("    {}: {{\n", property(&o.name)));
                    if let Some((wire, value)) = tag {
                        out.push_str(&format!(
                            "      {}: {},\n",
                            property(wire),
                            string_literal(&value.wire)
                        ));
                    }
                    for (target, assigned) in fields {
                        let v = match assigned {
                            Assigned::From {
                                row: r,
                                field,
                                adapt,
                            } => match adapt {
                                Adapt::Same | Adapt::Wrap => {
                                    format!("{}.{}", row(*r), property(&field.wire))
                                }
                                Adapt::Unwrap(message) => format!(
                                    "{}.{} ?? unreachable({})",
                                    row(*r),
                                    property(&field.wire),
                                    string_literal(message)
                                ),
                                Adapt::AbsentNull => "null".to_string(),
                            },
                            Assigned::Lit { lit, .. } => lit_ts(lit),
                            Assigned::Input { input: i, .. } => input(*i),
                            Assigned::ParentId { row: r, field, .. } => {
                                format!("{}.{}", row(*r), property(&field.wire))
                            }
                            Assigned::Null => "null".to_string(),
                        };
                        out.push_str(&format!("      {}: {v},\n", property(&target.wire)));
                    }
                    for wire in always_null {
                        out.push_str(&format!("      {}: null,\n", property(wire)));
                    }
                    out.push_str("    },\n");
                }
                OutKind::Deleted {
                    row: r,
                    id: Some((_, field)),
                } => {
                    out.push_str(&format!(
                        "    {}: {}.{},\n",
                        property(&o.name),
                        row(*r),
                        property(&field.wire)
                    ));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            out.push_str(&format!(
                "    {}: {{ event: \"{event}\" }},\n",
                property(&super::actions::snake(event))
            ));
        }
        out.push_str("  };\n}\n\n");
    }
}

fn lit_ts(lit: &super::ir::Lit) -> String {
    match lit {
        super::ir::Lit::Enum { value, .. } => string_literal(&value.wire),
        super::ir::Lit::Bool(b) => b.to_string(),
        super::ir::Lit::Int(n) => n.to_string(),
    }
}

fn expr(e: &super::ir::Expr, row: &dyn Fn(usize) -> String) -> String {
    use super::ir::{CmpOp, Expr};
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
        Expr::IsNull { row: r, field } => format!("{}.{} === null", row(*r), property(&field.wire)),
        Expr::EnumEq {
            row: r,
            field,
            value,
            ..
        } => format!(
            "{}.{} === {}",
            row(*r),
            property(&field.wire),
            string_literal(&value.wire)
        ),
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => {
            format!("{}.{} === {value}", row(*r), property(&field.wire))
        }
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => {
            let op = match op {
                CmpOp::Eq => "===",
                CmpOp::Ne => "!==",
                other => other.symbol(),
            };
            format!("{}.{} {op} {value}", row(*r), property(&field.wire))
        }
    }
}
