//! The Go printer: named scalars and ids, string enums with their constants, structs with
//! `json` tags naming the columns. A nullable column is a pointer.

use super::ir::{DomainModel, FieldType, RecordKind, Scalar};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

pub(super) fn print(
    domain: &DomainModel,
    options: &Options,
    traces: Option<&super::conformance::Traces>,
) -> String {
    let mut out = String::new();
    let mut head = vec![format!(
        "Package {} holds the domain types of the model.",
        options.package
    )];
    head.push(String::new());
    head.extend(header_lines(options));
    head.push(String::new());
    head.extend(obligation_lines(domain));
    head.extend(super::conformance_lines(
        domain,
        traces,
        "Conform",
        "ConformancePort",
    ));
    for line in head {
        out.push_str(&comment(&line, ""));
    }
    out.push_str(&format!("package {}\n\n", options.package));
    if domain.port.is_some() {
        out.push_str(super::go_port::IMPORTS);
    }

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
        out.push_str(&format!(
            "// Parse{0} returns the value of {0} the model spells s, or an error when there is none.\nfunc Parse{0}(s string) ({0}, error) {{\n\tif v := {0}(s); v.Valid() {{\n\t\treturn v, nil\n\t}}\n\treturn \"\", &InvalidValue{{Type: {1}, Value: s}}\n}}\n\n",
            decl.name,
            string_literal(&decl.name)
        ));
    }
    if !domain.enums.is_empty() {
        out.push_str("// InvalidValue reports text that is no value of a value set.\ntype InvalidValue struct {\n\tType  string\n\tValue string\n}\n\n");
        out.push_str("// Error names the value set and the text.\nfunc (e *InvalidValue) Error() string {\n\treturn \"not a value of \" + e.Type + \": \" + e.Value\n}\n\n");
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
        let rules = if domain.validators.is_empty() {
            ""
        } else {
            "\t// RuleViolations returns the rules the row breaks in its state.\n\tRuleViolations() []string\n"
        };
        out.push_str(&format!(
            "type {} interface {{\n\t{marker}()\n\t// {1} is the state's {2}.\n\t{1}() {tag_enum}\n{rules}}}\n\n",
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
            lines.push(format!(
                "Only this package makes one, in a transition function or {}FromRow: its fields are unexported, and read by the methods of the same name. A function given the zero value panics.",
                sum.name
            ));
            out.push_str(&doc(&variant.name, &lines, ""));
            out.push_str(&format!("type {} struct {{\n", variant.name));
            for field in &variant.fields {
                for line in &field.doc {
                    out.push_str(&comment(&one_line(line), "\t"));
                }
                let ty = field_type(&field.ty);
                let ty = if field.nullable { format!("*{ty}") } else { ty };
                out.push_str(&format!("\t{} {ty}\n", local(&field.wire)));
            }
            out.push_str("\tvalid_ bool\n}\n\n");
            for field in &variant.fields {
                let ty = field_type(&field.ty);
                let name = local(&field.wire);
                if field.nullable {
                    out.push_str(&format!(
                        "// {0} returns {1}, nil when it is null: a copy, so the state does not change.\nfunc (s {2}) {0}() *{ty} {{\n\tif s.{3} == nil {{\n\t\treturn nil\n\t}}\n\tv := *s.{3}\n\treturn &v\n}}\n\n",
                        field.pascal, field.wire, variant.name, name
                    ));
                } else {
                    out.push_str(&format!(
                        "// {0} returns {1}.\nfunc (s {2}) {0}() {ty} {{ return s.{3} }}\n\n",
                        field.pascal, field.wire, variant.name, name
                    ));
                }
            }
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
    print_validators(domain, &mut out);
    print_rows(domain, &mut out);
    print_dispatchers(domain, &mut out);
    print_helpers(domain, &mut out);
    super::go_port::print_port(domain, &mut out);
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

pub(super) fn field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => scalar(*s).to_string(),
        FieldType::Enum(name) | FieldType::Id(name) => name.clone(),
        FieldType::Rows(name) => format!("[]{name}"),
    }
}

pub(super) fn scalar(s: Scalar) -> &'static str {
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

pub(super) fn comment(line: &str, indent: &str) -> String {
    if line.is_empty() {
        format!("{indent}//\n")
    } else {
        format!("{indent}// {line}\n")
    }
}

/// A Go doc comment: the first line starts with the declared name, as `go vet` and godoc
/// expect.
pub(super) fn doc(name: &str, lines: &[String], indent: &str) -> String {
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
pub(super) fn local(snake_name: &str) -> String {
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
        if event.fields.is_empty() {
            out.push_str(&format!("type {} struct{{}}\n\n", event.name));
        } else {
            out.push_str(&format!("type {} struct {{\n", event.name));
            for field in &event.fields {
                out.push_str(&format!(
                    "\t// {} is the {} row the event was raised on.\n\t{} {}\n",
                    super::type_ident(&field.name),
                    field.entity,
                    super::type_ident(&field.name),
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
            "// {} is the mark step {} of {} leaves on a {} row: the next step takes it. Store it with the row.\ntype {} struct {{\n\tId {}\n}}\n\n",
            m.name, m.step, m.usecase, m.entity, m.name, m.id_ty
        ));
    }
    out.push_str("// ── Use cases ──\n\n");
    out.push_str("// Rejected reports a condition of a use case that did not hold: the use case does not run.\n");
    out.push_str(
        "type Rejected struct {\n\tCondition string\n\tLabel     string\n\tSite      string\n}\n\n",
    );
    out.push_str("// ExpectedSource is what a row a result writes must still be when the write commits, so the\n// store can write it conditionally and take a write that matches no row as NotEnabled (see the\n// obligations). Absent: the row is created, and no row has its id. Otherwise the row exists,\n// and when Column is set, it holds Value.\ntype ExpectedSource struct {\n\tAbsent bool\n\tColumn string\n\tValue  string\n}\n\n");
    out.push_str("// Error names the condition.\nfunc (r *Rejected) Error() string {\n\treturn \"rejected by the condition \" + r.Condition + \": \" + r.Label\n}\n\n");
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
        for o in &f.outputs {
            out.push_str(&format!(
                "\t{} ExpectedSource\n",
                super::type_ident(&format!("{}_expected", o.name))
            ));
        }
        for m in &f.marks_out {
            out.push_str(&format!("\t{} {}\n", super::type_ident(&m.name), m.ty));
        }
        for event in &f.raises {
            out.push_str(&format!("\t{0} {0}\n", event.ty));
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
        let row = |r: usize| local(&f.row(r).name);
        let acc = |r: usize, field: &super::ir::Field| {
            format!("{}.{}", row(r), member(domain, &f.row(r).ty, field))
        };
        let input = |i: usize| local(&f.inputs[i].name);
        let mut params: Vec<String> = f
            .rows
            .iter()
            .map(|r| format!("{} {}", local(&r.name), r.ty))
            .collect();
        for m in &f.marks_in {
            params.push(format!("{} {}", local(&m.name), m.ty));
        }
        if let Some(ev) = &f.event_in {
            params.push(format!("{} {}", local(&ev.name), ev.ty));
        }
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
        for r in &f.rows {
            if is_variant(domain, &r.ty) {
                out.push_str(&format!(
                    "\tif !{}.valid_ {{\n\t\tpanic({})\n\t}}\n",
                    local(&r.name),
                    zero_value(&r.ty)
                ));
            }
        }
        for i in &f.inputs {
            if let FieldType::Enum(e) = &i.ty {
                let check = if i.nullable {
                    format!("{0} != nil && !{0}.Valid()", local(&i.name))
                } else {
                    format!("!{}.Valid()", local(&i.name))
                };
                out.push_str(&format!(
                    "\tif {check} {{\n\t\tpanic({})\n\t}}\n",
                    string_literal(&format!("rdra-ish: {} is not a value of {e}", i.name))
                ));
            }
        }
        for check in &f.checks {
            match check {
                ArgCheck::Link {
                    child,
                    child_field,
                    parent,
                    parent_field,
                    message,
                } => {
                    let (c, p) = (acc(*child, child_field), acc(*parent, parent_field));
                    let differs = if child_field.nullable {
                        format!("{c} == nil || *{c} != {p}")
                    } else {
                        format!("{c} != {p}")
                    };
                    out.push_str(&format!(
                        "\tif {differs} {{\n\t\tpanic({})\n\t}}\n",
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
                        "\tif {} != {}.{} {{\n\t\tpanic({})\n\t}}\n",
                        acc(*r, field),
                        local(&ev.name),
                        super::type_ident(event_field),
                        string_literal(message)
                    ));
                }
                ArgCheck::MustBeNull {
                    row: r,
                    field,
                    message,
                } => {
                    out.push_str(&format!(
                        "\tif {} != nil {{\n\t\tpanic({})\n\t}}\n",
                        acc(*r, field),
                        string_literal(message)
                    ));
                }
            }
        }
        for c in &f.conditions {
            out.push_str(&format!(
                "\tif !({}) {{\n\t\treturn {}{{}}, &Rejected{{Condition: {}, Label: {}, Site: {}}}\n\t}}\n",
                expr(&c.expr, &row, &acc, &input),
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
                                let read = acc(*r, field);
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
                            } => wrap(acc(*r, field), *w),
                            Assigned::MarkId { mark, wrap: w } => {
                                wrap(format!("{}.Id", local(&f.marks_in[*mark].name)), *w)
                            }
                            Assigned::Switch {
                                row: r,
                                field,
                                enum_ty,
                                arms,
                            } => {
                                let mut v = format!(
                                    "func() {enum_ty} {{\n\t\t\t\tswitch {} {{\n",
                                    acc(*r, field)
                                );
                                for (from, to) in arms {
                                    v.push_str(&format!(
                                        "\t\t\t\tcase {enum_ty}{}:\n\t\t\t\t\treturn {enum_ty}{}\n",
                                        from.ident, to.ident
                                    ));
                                }
                                v.push_str(&format!(
                                    "\t\t\t\t}}\n\t\t\t\tpanic({})\n\t\t\t}}()",
                                    string_literal(&format!(
                                        "rdra-ish: {} is in no state {} moves it from",
                                        field.wire, f.usecase
                                    ))
                                ));
                                v
                            }
                            Assigned::EventId { field, wrap: w, .. } => wrap(
                                format!(
                                    "{}.{}",
                                    local(
                                        &f.event_in
                                            .as_ref()
                                            .expect("an event id has an event")
                                            .name
                                    ),
                                    super::type_ident(field)
                                ),
                                *w,
                            ),
                            Assigned::Null => "nil".to_string(),
                        };
                        body.push_str(&format!("\t\t\t{}: {v},\n", member(domain, ty, target)));
                    }
                    if is_variant(domain, ty) {
                        body.push_str("\t\t\tvalid_: true,\n");
                    }
                    body.push_str("\t\t},\n");
                }
                OutKind::Deleted {
                    row: r,
                    id: Some((_, field)),
                } => {
                    body.push_str(&format!(
                        "\t\t{}: {},\n",
                        super::type_ident(&o.name),
                        acc(*r, field)
                    ));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for o in &f.outputs {
            let expected = match f.expected(o) {
                super::ir::Expected::Absent => "ExpectedSource{Absent: true}".to_string(),
                super::ir::Expected::Present => "ExpectedSource{}".to_string(),
                super::ir::Expected::In { column, value } => format!(
                    "ExpectedSource{{Column: {}, Value: {}}}",
                    string_literal(&column),
                    string_literal(&value)
                ),
            };
            body.push_str(&format!(
                "\t\t{}: {expected},\n",
                super::type_ident(&format!("{}_expected", o.name))
            ));
        }
        for m in &f.marks_out {
            let id = match &m.source {
                super::ir::MarkSource::Row { row: r, field } => acc(*r, field),
                super::ir::MarkSource::Input(i) => input(*i),
                super::ir::MarkSource::Mark(k) => format!("{}.Id", local(&f.marks_in[*k].name)),
            };
            body.push_str(&format!(
                "\t\t{}: {}{{Id: {id}}},\n",
                super::type_ident(&m.name),
                m.ty
            ));
        }
        for event in &f.raises {
            let mut parts = Vec::new();
            for (field, source) in &event.ids {
                let id = match source {
                    super::ir::MarkSource::Row { row: r, field } => acc(*r, field),
                    super::ir::MarkSource::Input(i) => input(*i),
                    super::ir::MarkSource::Mark(k) => {
                        format!("{}.Id", local(&f.marks_in[*k].name))
                    }
                };
                parts.push(format!("{}: {id}", super::type_ident(&field.name)));
            }
            body.push_str(&format!(
                "\t\t{0}: {0}{{{1}}},\n",
                event.ty,
                parts.join(", ")
            ));
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

fn expr(
    e: &super::ir::Expr,
    row: &dyn Fn(usize) -> String,
    acc: &dyn Fn(usize, &super::ir::Field) -> String,
    input: &dyn Fn(usize) -> String,
) -> String {
    use super::ir::Expr;
    match e {
        Expr::True => "true".to_string(),
        Expr::False => "false".to_string(),
        Expr::Not(inner) => format!("!({})", expr(inner, row, acc, input)),
        Expr::And(items) if items.is_empty() => "true".to_string(),
        Expr::Or(items) if items.is_empty() => "false".to_string(),
        Expr::And(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row, acc, input)))
            .collect::<Vec<_>>()
            .join(" && "),
        Expr::Or(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row, acc, input)))
            .collect::<Vec<_>>()
            .join(" || "),
        Expr::IsNull { row: r, field } => format!("{} == nil", acc(*r, field)),
        Expr::EnumEq {
            row: r,
            field,
            enum_ty,
            value,
        } => {
            format!("{} == {enum_ty}{}", acc(*r, field), value.ident)
        }
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => format!("{} == {value}", acc(*r, field)),
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => {
            format!("{} {} {value}", acc(*r, field), op.symbol())
        }
        Expr::KeyEq {
            child,
            child_field,
            parent,
            parent_field,
        } => {
            let c = acc(*child, child_field);
            let p = acc(*parent, parent_field);
            if child_field.nullable {
                format!("({c} != nil && *{c} == {p})")
            } else {
                format!("{c} == {p}")
            }
        }
        Expr::Exists { input: i, elem, body } => format!(
            "func() bool {{ for _, {} := range {} {{ if {} {{ return true }} }}; return false }}()",
            row(*elem),
            input(*i),
            expr(body, row, acc, input)
        ),
        Expr::Forall { input: i, elem, body } => format!(
            "func() bool {{ for _, {} := range {} {{ if !({}) {{ return false }} }}; return true }}()",
            row(*elem),
            input(*i),
            expr(body, row, acc, input)
        ),
        Expr::Count {
            input: i,
            elem,
            body,
            op,
            n,
        } => format!(
            "func() bool {{ n := 0; for _, {} := range {} {{ if {} {{ n++ }} }}; return n {} {n} }}()",
            row(*elem),
            input(*i),
            expr(body, row, acc, input),
            op.symbol()
        ),
    }
}

// ── Rule checks ──────────────────────────────────────────────────────────────

pub(super) fn print_validators(domain: &DomainModel, out: &mut String) {
    if domain.validators.is_empty() {
        return;
    }
    out.push_str("// ── Rule checks ──\n\n");
    let row = |_: usize| "row".to_string();
    for v in &domain.validators {
        let acc =
            |_: usize, field: &super::ir::Field| format!("row.{}", member(domain, &v.ty, field));
        out.push_str(
            "// RuleViolations returns the rules row breaks, by the names `check` reports: empty when it keeps them all.\n",
        );
        out.push_str(&format!(
            "func (row {}) RuleViolations() []string {{\n\tvar out []string\n",
            v.ty
        ));
        for rule in &v.rules {
            out.push_str(&format!(
                "\t// {}\n\tif {} {{\n\t\tout = append(out, {})\n\t}}\n",
                one_line(&rule.text),
                expr(&fails(&rule.expr), &row, &acc, &|_| String::new()),
                string_literal(&rule.name)
            ));
        }
        out.push_str("\treturn out\n}\n\n");
    }
}

/// The condition under which `e` fails, folded so that a negated comparison reads plainly.
fn fails(e: &super::ir::Expr) -> super::ir::Expr {
    super::actions::simplify(super::ir::Expr::Not(Box::new(e.clone())))
}

// ── Rows and dispatch ────────────────────────────────────────────────────────

pub(super) fn print_helpers(domain: &DomainModel, out: &mut String) {
    if domain.fns.is_empty() && domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Helpers ──\n\n");
    out.push_str("func ptrTo[T any](v T) *T { return &v }\n\n");
    out.push_str("// mustPresent reads a value the verified model keeps present; nil means the rows broke a rule.\n");
    out.push_str("func mustPresent[T any](p *T, message string) T {\n\tif p == nil {\n\t\tpanic(message)\n\t}\n\treturn *p\n}\n\n");
}

pub(super) fn print_rows(domain: &DomainModel, out: &mut String) {
    if domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Rows: the flat shape a store or the wire holds ──\n\n");
    out.push_str("// InvalidRow reports a row that breaks what the type of its state requires.\n");
    out.push_str(
        "type InvalidRow struct {\n\tEntity string\n\tColumn string\n\tReason string\n}\n\n",
    );
    out.push_str("// Error names the entity, the column, and the reason.\nfunc (e *InvalidRow) Error() string {\n\treturn e.Entity + \".\" + e.Column + \": \" + e.Reason\n}\n\n");
    for sum in &domain.sums {
        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
            continue;
        };
        out.push_str(&doc(
            &sum.row,
            &[format!(
                "holds every column of {} as the data layer declares it.",
                sum.entity
            )],
            "",
        ));
        out.push_str(&format!("type {} struct {{\n", sum.row));
        for f in &sum.row_fields {
            let ty = field_type(&f.ty);
            let ty = if f.nullable { format!("*{ty}") } else { ty };
            out.push_str(&format!(
                "\t{} {ty} `json:{}`\n",
                f.pascal,
                string_literal(&f.wire)
            ));
        }
        out.push_str("}\n\n");
        let invalid = |column: &str, reason: String| {
            format!(
                "&InvalidRow{{Entity: {}, Column: {}, Reason: {}}}",
                string_literal(&sum.entity),
                string_literal(column),
                string_literal(&reason)
            )
        };
        out.push_str(&format!(
            "// {0}FromRow returns the state of a row, or why the row fits none.\nfunc {0}FromRow(row {1}) ({0}, error) {{\n\tswitch row.{2} {{\n",
            sum.name, sum.row, sum.tag.pascal
        ));
        for v in &sum.variants {
            out.push_str(&format!("\tcase {tag_enum}{}:\n", v.value.ident));
            for f in &v.always_null {
                out.push_str(&format!(
                    "\t\tif row.{} != nil {{\n\t\t\treturn nil, {}\n\t\t}}\n",
                    f.pascal,
                    invalid(&f.wire, format!("null in the state {}", v.value.wire))
                ));
            }
            let mut inits = Vec::new();
            for f in &v.fields {
                let declared = sum.row_fields.iter().find(|r| r.wire == f.wire);
                if declared.is_some_and(|d| d.nullable) && !f.nullable {
                    out.push_str(&format!(
                        "\t\tif row.{} == nil {{\n\t\t\treturn nil, {}\n\t\t}}\n",
                        f.pascal,
                        invalid(&f.wire, format!("present in the state {}", v.value.wire))
                    ));
                    inits.push(format!("{}: *row.{}", local(&f.wire), f.pascal));
                } else {
                    inits.push(format!("{}: row.{}", local(&f.wire), f.pascal));
                }
                if let FieldType::Enum(e) = &f.ty {
                    let check = if f.nullable {
                        format!("row.{0} != nil && !row.{0}.Valid()", f.pascal)
                    } else {
                        format!("!row.{}.Valid()", f.pascal)
                    };
                    out.push_str(&format!(
                        "\t\tif {check} {{\n\t\t\treturn nil, {}\n\t\t}}\n",
                        invalid(&f.wire, format!("not a value of {e}"))
                    ));
                }
            }
            inits.push("valid_: true".to_string());
            out.push_str(&format!(
                "\t\treturn {}{{{}}}, nil\n",
                v.name,
                inits.join(", ")
            ));
        }
        out.push_str(&format!(
            "\t}}\n\treturn nil, {}\n}}\n\n",
            invalid(&sum.tag.wire, format!("not a value of {tag_enum}"))
        ));
        out.push_str(&format!(
            "// {0}ToRow returns the flat row of a state.\nfunc {0}ToRow(state {0}) {1} {{\n\tswitch v := state.(type) {{\n",
            sum.name, sum.row
        ));
        for v in &sum.variants {
            let mut inits = Vec::new();
            for f in &sum.row_fields {
                if f.wire == sum.tag.wire {
                    inits.push(format!("{}: {tag_enum}{}", f.pascal, v.value.ident));
                } else if v.always_null.iter().any(|n| n.wire == f.wire) {
                    inits.push(format!("{}: nil", f.pascal));
                } else if let Some(vf) = v.fields.iter().find(|x| x.wire == f.wire) {
                    if f.nullable && !vf.nullable {
                        inits.push(format!("{}: ptrTo(v.{})", f.pascal, local(&f.wire)));
                    } else {
                        inits.push(format!("{}: v.{}", f.pascal, local(&f.wire)));
                    }
                }
            }
            out.push_str(&format!(
                "\tcase {0}:\n\t\tif !v.valid_ {{\n\t\t\tpanic({3})\n\t\t}}\n\t\treturn {1}{{{2}}}\n",
                v.name,
                sum.row,
                inits.join(", "),
                zero_value(&v.name)
            ));
        }
        out.push_str(&format!(
            "\t}}\n\tpanic({})\n}}\n\n",
            string_literal(&format!(
                "rdra-ish: a state of {} this package does not declare",
                sum.name
            ))
        ));
    }
}

pub(super) fn print_dispatchers(domain: &DomainModel, out: &mut String) {
    if domain.dispatchers.is_empty() {
        return;
    }
    out.push_str("// ── Dispatch: run a use case from whatever states its rows are in ──\n\n");
    out.push_str("// NotEnabled reports rows in states the use case does not run from.\n");
    out.push_str("type NotEnabled struct {\n\tUseCase string\n}\n\n");
    out.push_str("// Error names the use case.\nfunc (e *NotEnabled) Error() string {\n\treturn \"the use case \" + e.UseCase + \" does not run from these states\"\n}\n\n");
    for d in &domain.dispatchers {
        out.push_str(&format!(
            "// {} is what {} returns when the use case runs: one case is set.\ntype {} struct {{\n",
            d.outcome, d.name, d.outcome
        ));
        for c in &d.cases {
            out.push_str(&format!("\t{} *{}\n", c.case, c.result));
        }
        out.push_str("}\n\n");
        let mut params: Vec<String> = d
            .rows
            .iter()
            .map(|(n, ty)| format!("{} {ty}", local(n)))
            .collect();
        for (n, ty) in &d.marks {
            params.push(format!("{} {ty}", local(n)));
        }
        for i in &d.inputs {
            let ty = field_type(&i.ty);
            let ty = if i.nullable { format!("*{ty}") } else { ty };
            params.push(format!("{} {ty}", local(&i.name)));
        }
        for (n, ty) in &d.choices {
            params.push(format!("{} {ty}", local(n)));
        }
        out.push_str(&format!(
            "// {} runs {} from whatever states its rows are in: the transition function of those states, or NotEnabled.\nfunc {}({}) ({}, error) {{\n",
            d.name,
            d.usecase,
            d.name,
            params.join(", "),
            d.outcome
        ));
        for c in &d.cases {
            let mut depth = 1;
            for (s, (n, _)) in c.states.iter().zip(&d.rows) {
                if let Some((ty, _, _)) = s {
                    out.push_str(&format!(
                        "{}if {1}, ok := {1}.({ty}); ok {{\n",
                        "\t".repeat(depth),
                        local(n)
                    ));
                    depth += 1;
                }
            }
            for (i, v) in &c.choice {
                let (n, ty) = &d.choices[*i];
                out.push_str(&format!(
                    "{}if {} == {ty}{} {{\n",
                    "\t".repeat(depth),
                    local(n),
                    v.ident
                ));
                depth += 1;
            }
            let mut args: Vec<String> = d.rows.iter().map(|(n, _)| local(n)).collect();
            args.extend(d.marks.iter().map(|(n, _)| local(n)));
            args.extend(c.inputs.iter().map(|i| local(&d.inputs[*i].name)));
            let ind = "\t".repeat(depth);
            if c.rejects {
                out.push_str(&format!(
                    "{ind}r, err := {}({})\n{ind}if err != nil {{\n{ind}\treturn {}{{}}, err\n{ind}}}\n",
                    c.function,
                    args.join(", "),
                    d.outcome
                ));
            } else {
                out.push_str(&format!("{ind}r := {}({})\n", c.function, args.join(", ")));
            }
            out.push_str(&format!(
                "{ind}return {}{{{}: &r}}, nil\n",
                d.outcome, c.case
            ));
            for k in (1..depth).rev() {
                out.push_str(&format!("{}}}\n", "\t".repeat(k)));
            }
        }
        out.push_str(&format!(
            "\treturn {}{{}}, &NotEnabled{{UseCase: {}}}\n}}\n\n",
            d.outcome,
            string_literal(&d.usecase)
        ));
    }
}

/// Whether `ty` is the type of a state, whose fields are unexported.
fn is_variant(domain: &DomainModel, ty: &str) -> bool {
    domain
        .sums
        .iter()
        .any(|s| s.variants.iter().any(|v| v.name == ty))
}

/// The name of `field` in a struct of type `ty`: unexported in a state, so only this package
/// makes one; exported in a record or a row.
fn member(domain: &DomainModel, ty: &str, field: &super::ir::Field) -> String {
    if is_variant(domain, ty) {
        local(&field.wire)
    } else {
        field.pascal.clone()
    }
}

/// The panic message of a state the package did not make.
fn zero_value(ty: &str) -> String {
    string_literal(&format!(
        "rdra-ish: {ty} not made by this package (the zero value)"
    ))
}
