//! The Rust conformance port and harness: `StoreSnapshot`, `ConformancePort`, and `conform`,
//! which drives a port along the traces `conformance::traces` writes.

use super::actions::snake;
use super::conformance::TRACES_FORMAT;
use super::ir::{DomainModel, Field, FieldType, Scalar, Track};
use super::rust::{doc, field_type, ident, OPTION, STRING};
use super::string_literal;

const VEC: &str = "::std::vec::Vec";
const RESULT: &str = "::core::result::Result";

pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    out.push_str(
        "// ── Conformance: the port a finished service implements, and the harness that drives it ──\n\n",
    );
    if domain.dispatchers.is_empty() && !domain.fns.is_empty() {
        out.push_str("/// The rows are in states the use case does not run from.\n");
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str("pub struct NotEnabled {\n    pub usecase: &'static str,\n}\n\n");
        out.push_str("/// Why a use case did not run.\n");
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(
            "pub enum Refused {\n    Rejected(Rejected),\n    NotEnabled(NotEnabled),\n}\n\n",
        );
    }
    for r in &port.runs {
        out.push_str(&format!(
            "/// A run mark of {} on a {} row, as a store keeps it: the last step of the run that wrote the row.\n",
            r.usecase, r.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!(
            "pub struct {} {{\n    pub id: {},\n    pub step: u8,\n}}\n\n",
            r.name, r.id_ty
        ));
    }
    out.push_str("/// Everything a store holds, as the conformance harness reads it: the rows of each entity,\n/// the pending run marks, and the events raised on rows that a triggered use case reads.\n");
    out.push_str("#[derive(Debug, Clone, Default, PartialEq, Eq)]\n");
    out.push_str("pub struct StoreSnapshot {\n");
    for e in &port.entities {
        out.push_str(&format!("    pub {}: {VEC}<{}>,\n", ident(&e.field), e.ty));
    }
    for r in &port.runs {
        out.push_str(&format!(
            "    pub {}: {VEC}<{}>,\n",
            ident(&r.field),
            r.name
        ));
    }
    for ev in &port.events {
        out.push_str(&format!(
            "    pub {}: {VEC}<{}>,\n",
            ident(&ev.field),
            ev.ty
        ));
    }
    out.push_str("}\n\n");

    out.push_str("/// What a finished service offers the conformance harness. Each `run_*` loads its rows by id,\n/// calls the dispatcher or transition function, and stores the rows, marks, and events it\n/// returns in one transaction; it returns `Err` and changes nothing when the use case does not run.\n");
    out.push_str("pub trait ConformancePort {\n");
    out.push_str(
        "    /// Empties the store and stores these rows: the initial state of a trace.\n",
    );
    out.push_str("    fn seed(&mut self, rows: StoreSnapshot);\n");
    out.push_str("    /// Everything the store holds now.\n");
    out.push_str("    fn snapshot(&mut self) -> StoreSnapshot;\n");
    for m in &port.methods {
        out.push_str(&doc(
            &[format!("Runs {} (the action {}).", m.usecase, m.action)],
            "    ",
        ));
        out.push_str(&format!(
            "    fn {}(&mut self{}) -> {RESULT}<(), Refused>;\n",
            method_ident(&m.name),
            params(m)
                .iter()
                .map(|p| format!(", {p}"))
                .collect::<String>()
        ));
    }
    out.push_str("}\n\n");

    out.push_str("/// What `conform` checked.\n");
    out.push_str("#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]\n");
    out.push_str("pub struct ConformanceReport {\n    pub traces: usize,\n    pub calls: usize,\n    pub refusals: usize,\n}\n\n");

    out.push_str(&format!(
        r##"/// Drives a fresh port per trace along `traces` (the file `codegen --conformance` writes): seeds
/// each trace's initial state, makes each call, and after each compares what the store holds
/// with the state the model reaches. A call the model refuses must return `Err` and change
/// nothing. Returns the first difference.
pub fn conform<P: ConformancePort>(mut new_port: impl FnMut() -> P, traces: &str) -> {RESULT}<ConformanceReport, {STRING}> {{
    let mut lines = traces.lines().enumerate();
    match lines.next() {{
        ::core::option::Option::Some((_, first)) if first.trim_end() == {format} => {{}}
        _ => return {RESULT}::Err("not a traces file of this version: generate the code and the traces again".to_string()),
    }}
    let mut states: ::std::collections::BTreeMap<{STRING}, {VEC}<{STRING}>> = ::std::collections::BTreeMap::new();
    let mut defining: {OPTION}<{STRING}> = None;
    let mut report = ConformanceReport::default();
    let mut conf_port: {OPTION}<P> = None;
    let mut seed = StoreSnapshot::default();
    let mut want: {VEC}<{STRING}> = {VEC}::new();
    for (n, line) in lines {{
        let toks: {VEC}<&str> = line.split_whitespace().collect();
        let ::core::option::Option::Some((&kind, rest)) = toks.split_first() else {{
            continue;
        }};
        let at = |what: {STRING}| format!("traces line {{}}: {{what}}", n + 1);
        let state = |id: {OPTION}<&&str>| -> {RESULT}<{VEC}<{STRING}>, {STRING}> {{
            id.and_then(|id| states.get(*id)).cloned().ok_or_else(|| at("names no state defined above".to_string()))
        }};
        match kind {{
            "#" => {{}}
            "S" => {{
                let id = rest.first().ok_or_else(|| at("a state without a number".to_string()))?.to_string();
                states.insert(id.clone(), {VEC}::new());
                defining = Some(id);
            }}
            "A" => {{
                let id = defining.as_ref().ok_or_else(|| at("a row outside a state".to_string()))?;
                states.get_mut(id).expect("defined").push(rest.join(" "));
            }}
            "T" => {{
                want = state(rest.first())?;
                conf_port = Some(new_port());
                seed = StoreSnapshot::default();
                report.traces += 1;
            }}
            "R" => conf_seed_row(&mut seed, rest).map_err(at)?,
            "G" => {{
                let p = conf_port.as_mut().ok_or_else(|| at("seeding outside a trace".to_string()))?;
                p.seed(::core::mem::take(&mut seed));
                conf_compare(&conf_abstract(&p.snapshot()), &want, "after seeding the initial state").map_err(at)?;
            }}
            "C" => {{
                let next = state(rest.first())?;
                let call = rest.get(1..).unwrap_or_default();
                let p = conf_port.as_mut().ok_or_else(|| at("a call outside a trace".to_string()))?;
                if let {RESULT}::Err(why) = conf_call(p, call).map_err(at)? {{
                    return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{why:?}}", call.join(" "))));
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &next, &format!("after `{{}}`", call.join(" "))).map_err(at)?;
                want = next;
                report.calls += 1;
            }}
            "X" => {{
                let p = conf_port.as_mut().ok_or_else(|| at("a call outside a trace".to_string()))?;
                if conf_call(p, rest).map_err(at)?.is_ok() {{
                    return {RESULT}::Err(at(format!("the model refuses `{{}}` here, and the service ran it", rest.join(" "))));
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &want, &format!("after the refused `{{}}`", rest.join(" "))).map_err(at)?;
                report.refusals += 1;
            }}
            other => return {RESULT}::Err(at(format!("unknown record {{other}}"))),
        }}
    }}
    {RESULT}::Ok(report)
}}

/// The store against the model's state: every line one holds and the other does not.
fn conf_compare(got: &[{STRING}], want: &[{STRING}], when: &str) -> {RESULT}<(), {STRING}> {{
    if got == want {{
        return {RESULT}::Ok(());
    }}
    let mut out = format!("{{when}}, the store differs from the model:");
    for line in want.iter().filter(|l| !got.contains(l)) {{
        out.push_str(&format!("\n  model only: {{line}}"));
    }}
    for line in got.iter().filter(|l| !want.contains(l)) {{
        out.push_str(&format!("\n  store only: {{line}}"));
    }}
    {RESULT}::Err(out)
}}

fn conf_next<'a>(args: &mut impl ::core::iter::Iterator<Item = &'a str>) -> {RESULT}<&'a str, {STRING}> {{
    args.next().ok_or_else(|| "too few values".to_string())
}}

fn conf_int(t: &str) -> {RESULT}<i64, {STRING}> {{
    t.parse::<i64>().map_err(|e| format!("{{t}}: {{e}}"))
}}

fn conf_bool(t: &str) -> {RESULT}<bool, {STRING}> {{
    match t {{
        "true" => {RESULT}::Ok(true),
        "false" => {RESULT}::Ok(false),
        _ => {RESULT}::Err(format!("not a bool: {{t}}")),
    }}
}}

"##,
        format = string_literal(TRACES_FORMAT),
    ));

    // The calls.
    out.push_str(&format!(
        "#[allow(unused_mut)]\nfn conf_call<P: ConformancePort>(conf_port: &mut P, call: &[&str]) -> {RESULT}<{RESULT}<(), Refused>, {STRING}> {{\n"
    ));
    out.push_str("    let ::core::option::Option::Some((&action, values)) = call.split_first() else {\n        return ::core::result::Result::Err(\"a call names no use case\".to_string());\n    };\n");
    out.push_str("    let mut conf_args = values.iter().copied();\n");
    out.push_str("    match action {\n");
    for m in &port.methods {
        out.push_str(&format!("        {} => {{\n", string_literal(&m.action)));
        let mut args = Vec::new();
        for (name, ty) in m.rows.iter().chain(&m.marks) {
            out.push_str(&format!(
                "            let {} = {};\n",
                ident(&format!("conf_{name}")),
                parse(domain, &FieldType::Id(ty.clone()), false)
            ));
            args.push(ident(&format!("conf_{name}")));
        }
        if let Some((name, ty)) = &m.event {
            let event = domain.events.iter().find(|e| e.name == *ty);
            let fields: Vec<String> = event
                .map(|e| e.fields.clone())
                .unwrap_or_default()
                .iter()
                .map(|f| {
                    format!(
                        "{}: {}",
                        ident(&f.name),
                        parse(domain, &FieldType::Id(f.id_ty.clone()), false)
                    )
                })
                .collect();
            let value = if fields.is_empty() {
                ty.clone()
            } else {
                format!("{ty} {{ {} }}", fields.join(", "))
            };
            out.push_str(&format!(
                "            let {} = {value};\n",
                ident(&format!("conf_{name}"))
            ));
            args.push(ident(&format!("conf_{name}")));
        }
        for i in &m.inputs {
            out.push_str(&format!(
                "            let {} = {};\n",
                ident(&format!("conf_{}", i.name)),
                parse(domain, &i.ty, i.nullable)
            ));
            args.push(ident(&format!("conf_{}", i.name)));
        }
        for (name, ty) in &m.choices {
            out.push_str(&format!(
                "            let {} = {};\n",
                ident(&format!("conf_{name}")),
                parse(domain, &FieldType::Enum(ty.clone()), false)
            ));
            args.push(ident(&format!("conf_{name}")));
        }
        out.push_str(&format!(
            "            {RESULT}::Ok(conf_port.{}({}))\n        }}\n",
            method_ident(&m.name),
            args.join(", ")
        ));
    }
    out.push_str(&format!(
        "        other => {RESULT}::Err(format!(\"no use case {{other}} in this port\")),\n    }}\n}}\n\n"
    ));

    // The rows of an initial state.
    out.push_str(&format!(
        "#[allow(unused_mut)]\nfn conf_seed_row(seed: &mut StoreSnapshot, toks: &[&str]) -> {RESULT}<(), {STRING}> {{\n"
    ));
    out.push_str("    let ::core::option::Option::Some((&entity, values)) = toks.split_first() else {\n        return ::core::result::Result::Err(\"a row names no entity\".to_string());\n    };\n");
    out.push_str("    let mut conf_args = values.iter().copied();\n");
    out.push_str("    match entity {\n");
    for e in &port.entities {
        let fields: Vec<String> = e
            .columns
            .iter()
            .map(|f| format!("{}: {}", ident(&f.wire), parse(domain, &f.ty, f.nullable)))
            .collect();
        out.push_str(&format!(
            "        {} => seed.{}.push({} {{ {} }}),\n",
            string_literal(&e.entity),
            ident(&e.field),
            e.ty,
            fields.join(", ")
        ));
    }
    out.push_str(&format!(
        "        other => return {RESULT}::Err(format!(\"no entity {{other}} in this port\")),\n    }}\n    {RESULT}::Ok(())\n}}\n\n"
    ));

    // The abstraction.
    out.push_str(&format!(
        "/// The store as the traces spell a state: one line per row with the columns the model\n/// tracks, per pending run mark, and per raise a triggered use case reads; sorted.\n#[allow(unused_mut)]\nfn conf_abstract(s: &StoreSnapshot) -> {VEC}<{STRING}> {{\n    let mut out: {VEC}<{STRING}> = {VEC}::new();\n"
    ));
    for e in &port.entities {
        out.push_str(&format!("    for row in &s.{} {{\n", ident(&e.field)));
        out.push_str(&format!(
            "        let mut line = format!(\"{} {{}}\", {});\n",
            e.entity,
            text(domain, &format!("row.{}", ident(&e.id.wire)), &e.id.ty)
        ));
        for (field, how) in &e.tracked {
            out.push_str(&format!(
                "        line.push_str(&format!(\" {}={{}}\", {}));\n",
                field.wire,
                abstracted(domain, field, *how)
            ));
        }
        out.push_str("        out.push(line);\n    }\n");
    }
    for r in &port.runs {
        out.push_str(&format!(
            "    for m in &s.{} {{\n        out.push(format!(\"mark {} {} {{}} {{}}\", {}, m.step));\n    }}\n",
            ident(&r.field),
            r.usecase,
            r.entity,
            text(domain, "m.id", &FieldType::Id(r.id_ty.clone()))
        ));
    }
    for ev in &port.events {
        out.push_str(&format!("    for ev in &s.{} {{\n", ident(&ev.field)));
        for (field, tracked) in &ev.ids {
            if *tracked {
                out.push_str(&format!(
                    "        out.push(format!(\"raised {} {} {{}}\", {}));\n",
                    ev.ty,
                    field.entity,
                    text(
                        domain,
                        &format!("ev.{}", ident(&field.name)),
                        &FieldType::Id(field.id_ty.clone())
                    )
                ));
            }
        }
        out.push_str("    }\n");
    }
    out.push_str("    out.sort();\n    out.dedup();\n    out\n}\n\n");
}

fn method_ident(pascal: &str) -> String {
    format!("run_{}", snake(pascal))
}

fn params(m: &super::ir::PortMethod) -> Vec<String> {
    let mut out: Vec<String> = m
        .rows
        .iter()
        .chain(&m.marks)
        .map(|(n, ty)| format!("{}: {ty}", ident(n)))
        .collect();
    if let Some((n, ty)) = &m.event {
        out.push(format!("{}: {ty}", ident(n)));
    }
    for i in &m.inputs {
        let ty = field_type(&i.ty);
        let ty = if i.nullable {
            format!("{OPTION}<{ty}>")
        } else {
            ty
        };
        out.push(format!("{}: {ty}", ident(&i.name)));
    }
    for (n, ty) in &m.choices {
        out.push(format!("{}: {ty}", ident(n)));
    }
    out
}

/// The scalar an id type wraps.
fn id_repr(domain: &DomainModel, id: &str) -> Scalar {
    domain
        .ids
        .iter()
        .find(|i| i.name == id)
        .map_or(Scalar::Int, |i| i.repr)
}

/// An expression that reads the next value of `conf_args` as `ty`.
fn parse(domain: &DomainModel, ty: &FieldType, nullable: bool) -> String {
    let value = |t: &str| -> String {
        match ty {
            FieldType::Scalar(s) => scalar_parse(*s, t),
            FieldType::Enum(name) => format!(
                "{name}::parse({t}).ok_or_else(|| format!(\"not a value of {name}: {{}}\", {t}))?"
            ),
            FieldType::Id(name) => format!("{name}({})", scalar_parse(id_repr(domain, name), t)),
            FieldType::Rows(_) => unreachable!("a list of rows is no port argument"),
        }
    };
    if nullable {
        format!(
            "{{ let t = conf_next(&mut conf_args)?; if t == \"~\" {{ None }} else {{ Some({}) }} }}",
            value("t")
        )
    } else {
        format!("{{ let t = conf_next(&mut conf_args)?; {} }}", value("t"))
    }
}

fn scalar_parse(s: Scalar, t: &str) -> String {
    match s {
        Scalar::Int => format!("conf_int({t})?"),
        Scalar::Bool => format!("conf_bool({t})?"),
        Scalar::String => format!("{t}.to_string()"),
        other => format!("{}({t}.to_string())", other.name()),
    }
}

/// The text of the value at place `e` of type `ty` (not null), as the traces spell it.
fn text(domain: &DomainModel, e: &str, ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => scalar_text(*s, e),
        FieldType::Enum(_) => format!("{e}.as_str().to_string()"),
        FieldType::Id(name) => scalar_text(id_repr(domain, name), &format!("{e}.0")),
        FieldType::Rows(_) => unreachable!("a list of rows is no column"),
    }
}

fn scalar_text(s: Scalar, e: &str) -> String {
    match s {
        Scalar::Int | Scalar::Bool => format!("{e}.to_string()"),
        Scalar::String => format!("{e}.clone()"),
        _ => format!("{e}.0.clone()"),
    }
}

/// The tracked column `field` of `row`, abstracted as the model holds it.
fn abstracted(domain: &DomainModel, field: &Field, how: Track) -> String {
    let place = format!("row.{}", ident(&field.wire));
    match (how, field.nullable) {
        (Track::Presence, true) => {
            format!("if {place}.is_some() {{ \"+\" }} else {{ \"~\" }}")
        }
        (Track::Presence, false) => "\"+\"".to_string(),
        (Track::Exact, true) => format!(
            "match &{place} {{ Some(x) => {}, None => \"~\".to_string() }}",
            text(domain, "x", &field.ty)
        ),
        (Track::Exact, false) => text(domain, &place, &field.ty),
    }
}
