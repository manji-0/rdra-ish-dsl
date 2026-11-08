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
    if !domain.sums.is_empty() {
        out.push_str("// ── Entities with states ──\n\n");
    }
    for sum in &domain.sums {
        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
            unreachable!("a tag column is an enum");
        };
        let marker = format!("is{}", sum.name);
        let names: Vec<&str> = sum.variants.iter().map(|v| v.name.as_str()).collect();
        out.push_str(&doc(
            &sum.name,
            &[
                format!("is {}", one_line(&sum.doc.join(" "))),
                format!(
                    "It is one of {}, told apart by {}(). Only this package declares its states.",
                    names.join(", "),
                    sum.tag.pascal
                ),
            ],
            "",
        ));
        out.push_str(&format!(
            "type {} interface {{\n\t{marker}()\n\t// {1} is the state's {2}.\n\t{1}() {tag_enum}\n}}\n\n",
            sum.name, sum.tag.pascal, sum.tag.wire
        ));
        for variant in &sum.variants {
            let mut lines = vec![format!("is {}", one_line(&variant.doc.join(" ")))];
            for field in &variant.always_null {
                lines.push(format!(
                    "{}: {}",
                    field.wire,
                    field.doc.last().cloned().unwrap_or_default()
                ));
            }
            out.push_str(&doc(&variant.name, &lines, ""));
            out.push_str(&format!("type {} struct {{\n", variant.name));
            for field in &variant.fields {
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
            out.push_str(&format!("func ({}) {marker}() {{}}\n\n", variant.name));
            out.push_str(&format!(
                "// {method} returns {tag_enum}{value}.\nfunc ({name}) {method}() {tag_enum} {{ return {tag_enum}{value} }}\n\n",
                name = variant.name,
                method = sum.tag.pascal,
                value = variant.value.ident,
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

// ── Use cases ────────────────────────────────────────────────────────────────

/// A Go identifier for a local name: camelCase, with a keyword given a trailing `_`.
fn local(snake_name: &str) -> String {
    const KEYWORDS: [&str; 25] = [
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ];
    let name = super::camel(&super::type_ident(snake_name));
    if KEYWORDS.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

pub(super) fn print_functions(domain: &DomainModel, out: &mut String) {
    use super::ir::{Adapt, ArgCheck, Assigned, OutKind};
    if !domain.events.is_empty() {
        out.push_str("// ── Events ──\n\n");
    }
    for event in &domain.events {
        out.push_str(&doc(
            &event.name,
            &[format!("is the event {}.", one_line(&event.doc.join(" ")))],
            "",
        ));
        out.push_str(&format!("type {} struct{{}}\n\n", event.name));
    }
    if domain.fns.is_empty() {
        return;
    }
    out.push_str("// ── Use cases ──\n\n");
    out.push_str("// Rejected reports a condition of a use case that did not hold: the use case does not run.\n");
    out.push_str(
        "type Rejected struct {\n\tCondition string\n\tLabel     string\n\tSite      string\n}\n\n",
    );
    out.push_str("// Error names the condition.\nfunc (r *Rejected) Error() string {\n\treturn \"rejected by the condition \" + r.Condition + \": \" + r.Label\n}\n\n");
    out.push_str("func ptrTo[T any](v T) *T { return &v }\n\n");
    out.push_str("// mustPresent reads a value the verified model keeps present; nil means the rows broke a rule.\n");
    out.push_str("func mustPresent[T any](p *T, message string) T {\n\tif p == nil {\n\t\tpanic(message)\n\t}\n\treturn *p\n}\n\n");
    for f in &domain.fns {
        out.push_str(&format!("// {} is what {} returns.\n", f.result, f.name));
        out.push_str(&format!("type {} struct {{\n", f.result));
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    out.push_str(&format!("\t{} {ty}\n", super::type_ident(&o.name)))
                }
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => out.push_str(&format!("\t{} {ty}\n", super::type_ident(&o.name))),
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            out.push_str(&format!("\t{event} {event}\n"));
        }
        out.push_str("}\n\n");

        let mut lines = vec![format!("runs {}", one_line(&f.doc.join(" ")))];
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
        out.push_str(&doc(&f.name, &lines, ""));
        let row = |r: usize| local(&f.rows[r].name);
        let input = |i: usize| local(&f.inputs[i].name);
        let mut params: Vec<String> = f
            .rows
            .iter()
            .map(|r| format!("{} {}", local(&r.name), r.ty))
            .collect();
        for i in &f.inputs {
            let ty = field_type(&i.ty);
            let ty = if i.nullable { format!("*{ty}") } else { ty };
            params.push(format!("{} {ty}", local(&i.name)));
        }
        let ret = if f.conditions.is_empty() {
            f.result.clone()
        } else {
            format!("({}, error)", f.result)
        };
        out.push_str(&format!(
            "func {}({}) {ret} {{\n",
            f.name,
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
                        "\tif {}.{} != {}.{} {{\n\t\tpanic({})\n\t}}\n",
                        row(*child),
                        child_field.pascal,
                        row(*parent),
                        parent_field.pascal,
                        string_literal(message)
                    ));
                }
                ArgCheck::MustBeNull {
                    row: r,
                    field,
                    message,
                } => {
                    out.push_str(&format!(
                        "\tif {}.{} != nil {{\n\t\tpanic({})\n\t}}\n",
                        row(*r),
                        field.pascal,
                        string_literal(message)
                    ));
                }
            }
        }
        for c in &f.conditions {
            out.push_str(&format!(
                "\tif !({}) {{\n\t\treturn {}{{}}, &Rejected{{Condition: {}, Label: {}, Site: {}}}\n\t}}\n",
                expr(&c.expr, &row),
                f.result,
                string_literal(&c.id),
                string_literal(&c.label),
                string_literal(c.site.as_deref().unwrap_or(""))
            ));
        }
        let mut body = format!("{}{{\n", f.result);
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, fields, .. } => {
                    body.push_str(&format!("\t\t{}: {ty}{{\n", super::type_ident(&o.name)));
                    for (target, assigned) in fields {
                        let wrap = |v: String, w: bool| if w { format!("ptrTo({v})") } else { v };
                        let v = match assigned {
                            Assigned::From {
                                row: r,
                                field,
                                adapt,
                            } => {
                                let read = format!("{}.{}", row(*r), field.pascal);
                                match adapt {
                                    Adapt::Same => read,
                                    Adapt::Wrap => format!("ptrTo({read})"),
                                    Adapt::Unwrap(message) => {
                                        format!("mustPresent({read}, {})", string_literal(message))
                                    }
                                    Adapt::AbsentNull => "nil".to_string(),
                                }
                            }
                            Assigned::Lit { lit, wrap: w } => wrap(lit_go(lit, &target.ty), *w),
                            Assigned::Input { input: i, wrap: w } => wrap(input(*i), *w),
                            Assigned::ParentId {
                                row: r,
                                field,
                                wrap: w,
                            } => wrap(format!("{}.{}", row(*r), field.pascal), *w),
                            Assigned::Null => "nil".to_string(),
                        };
                        body.push_str(&format!("\t\t\t{}: {v},\n", target.pascal));
                    }
                    body.push_str("\t\t},\n");
                }
                OutKind::Deleted {
                    row: r,
                    id: Some((_, field)),
                } => {
                    body.push_str(&format!(
                        "\t\t{}: {}.{},\n",
                        super::type_ident(&o.name),
                        row(*r),
                        field.pascal
                    ));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for event in &f.raises {
            body.push_str(&format!("\t\t{event}: {event}{{}},\n"));
        }
        body.push_str("\t}");
        if f.conditions.is_empty() {
            out.push_str(&format!("\treturn {body}\n}}\n\n"));
        } else {
            out.push_str(&format!("\treturn {body}, nil\n}}\n\n"));
        }
    }
}

fn lit_go(lit: &super::ir::Lit, ty: &FieldType) -> String {
    match lit {
        super::ir::Lit::Enum { ty, value } => format!("{ty}{}", value.ident),
        super::ir::Lit::Bool(b) => b.to_string(),
        super::ir::Lit::Int(n) => match ty {
            FieldType::Scalar(Scalar::Int) => format!("int64({n})"),
            _ => n.to_string(),
        },
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
        Expr::IsNull { row: r, field } => format!("{}.{} == nil", row(*r), field.pascal),
        Expr::EnumEq {
            row: r,
            field,
            enum_ty,
            value,
        } => {
            format!("{}.{} == {enum_ty}{}", row(*r), field.pascal, value.ident)
        }
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => format!("{}.{} == {value}", row(*r), field.pascal),
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => {
            format!("{}.{} {} {value}", row(*r), field.pascal, op.symbol())
        }
    }
}
