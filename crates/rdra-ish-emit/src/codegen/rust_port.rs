//! The Rust conformance port and harness: `StoreSnapshot`, `ConformancePort`, and `conform`,
//! which drives a port along the traces `conformance::traces` writes.

use super::conformance::{TRACES_ENV, TRACES_FORMAT};
use super::ir::{DomainModel, Field, FieldType, PortArg, PortDecl, PortMethod, Scalar, Track};
use super::rust::{doc, field_type, ident, OPTION, STRING};
use super::snake;
use super::string_literal;

const VEC: &str = "::std::vec::Vec";
const RESULT: &str = "::core::result::Result";

pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    print_contracts(domain, port, out);
    if !port.harness {
        return;
    }
    print_snapshot(port, out);
    print_port_trait(port, out);
    print_conform(port, out);
    print_calls(domain, port, out);
    print_seed_rows(port, out);
    print_abstraction(port, out);
}

/// The run marks and `StoreSnapshot`: everything a store holds, as the harness reads it.
fn print_snapshot(port: &PortDecl, out: &mut String) {
    out.push_str(
        "// ── Conformance: the port a finished service implements, and the harness that drives it ──\n\n",
    );
    for r in &port.runs {
        out.push_str(&format!(
            "/// A run mark of {} on a {} row, as a store keeps it: the last step of the run that wrote the row.\n",
            r.usecase, r.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!(
            "pub struct {} {{\n    /// The id of the marked row.\n    pub id: {},\n    /// The last step of the run that wrote the row.\n    pub step: u8,\n}}\n\n",
            r.name, r.id_ty
        ));
    }
    out.push_str("/// Everything a store holds, as the conformance harness reads it: the rows of each entity,\n/// the pending run marks, and the events raised on rows that a triggered use case reads.\n");
    out.push_str("#[derive(Debug, Clone, Default, PartialEq, Eq)]\n");
    out.push_str("pub struct StoreSnapshot {\n");
    for e in &port.entities {
        out.push_str(&format!(
            "    /// The {} rows.\n    pub {}: {VEC}<{}>,\n",
            e.entity,
            ident(&e.field),
            e.ty
        ));
    }
    for r in &port.runs {
        out.push_str(&format!(
            "    /// The pending run marks of {} on {} rows.\n    pub {}: {VEC}<{}>,\n",
            r.usecase,
            r.entity,
            ident(&r.field),
            r.name
        ));
    }
    for ev in &port.events {
        out.push_str(&format!(
            "    /// The {} events raised.\n    pub {}: {VEC}<{}>,\n",
            ev.ty,
            ident(&ev.field),
            ev.ty
        ));
    }
    out.push_str("}\n\n");
}

/// `ConformancePort`, what a finished service offers the harness, and `ConformanceReport`.
fn print_port_trait(port: &PortDecl, out: &mut String) {
    out.push_str("/// What a finished service offers the conformance harness: every use case's contract, and\n/// a store the harness can seed and read back.\n");
    let supers: Vec<&str> = port.contracts.iter().map(|c| c.name.as_str()).collect();
    let bound = if supers.is_empty() {
        String::new()
    } else {
        format!(": {}", supers.join(" + "))
    };
    out.push_str(&format!("pub trait ConformancePort{bound} {{\n"));
    out.push_str(
        "    /// Empties the store and stores these rows: the initial state of a trace.\n",
    );
    out.push_str("    fn seed(&mut self, rows: StoreSnapshot);\n");
    out.push_str("    /// Everything the store holds now.\n");
    out.push_str("    fn snapshot(&mut self) -> StoreSnapshot;\n");
    out.push_str("}\n\n");

    out.push_str("/// What `conform` checked.\n");
    out.push_str("#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]\n");
    out.push_str("pub struct ConformanceReport {\n    /// The traces run.\n    pub traces: usize,\n    /// The calls the model runs, made.\n    pub calls: usize,\n    /// The calls the model refuses, made and refused.\n    pub refusals: usize,\n    /// The commits after another call changed a row they write, refused.\n    pub interleavings: usize,\n}\n\n");
}

/// `conform`, which drives a fresh port per trace, and the helpers it shares with every model.
fn print_conform(port: &PortDecl, out: &mut String) {
    out.push_str("/// A prepared write of any use case.\nenum ConfWrite {\n");
    for m in &port.methods {
        out.push_str(&format!("    {}({}),\n", m.name, m.write));
    }
    out.push_str("}\n\n");

    out.push_str(&format!(
        r##"/// Drives a fresh port per trace along `traces` (the file `codegen --conformance` writes): seeds
/// each trace's initial state, makes each call, and after each compares what the store holds
/// with the state the model reaches. A call the model refuses must return `Err` and change
/// nothing, and so must a commit whose rows another call changed after its prepare. Returns the
/// first difference.
///
/// # Errors
///
/// The first difference, or what is wrong with `traces`, as text naming its line.
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
                if let ::core::option::Option::Some(rows) = states.get_mut(id) {{
                    rows.push(rest.join(" "));
                }}
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
                if let {RESULT}::Err(why) = conf_run(p, call).map_err(at)? {{
                    return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{why}}", call.join(" "))));
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &next, &format!("after `{{}}`", call.join(" "))).map_err(at)?;
                want = next;
                report.calls += 1;
            }}
            "X" => {{
                let ::core::option::Option::Some((&kind, rest)) = rest.split_first() else {{
                    return {RESULT}::Err(at("a refused call without its kind".to_string()));
                }};
                let p = conf_port.as_mut().ok_or_else(|| at("a call outside a trace".to_string()))?;
                match conf_run(p, rest).map_err(at)? {{
                    {RESULT}::Ok(()) => return {RESULT}::Err(at(format!("the model refuses `{{}}` here, and the service ran it", rest.join(" ")))),
                    {RESULT}::Err(why) if kind != "-" && !why.starts_with(&format!("{{kind}}: ")) => {{
                        return {RESULT}::Err(at(format!("the model refuses `{{}}` here with {{kind}}, and the service refused it with {{why}}", rest.join(" "))));
                    }}
                    {RESULT}::Err(_) => {{}}
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &want, &format!("after the refused `{{}}`", rest.join(" "))).map_err(at)?;
                report.refusals += 1;
            }}
            "I" => {{
                let next = state(rest.first())?;
                let split = rest.iter().position(|t| *t == "|").ok_or_else(|| at("an interleaving without `|`".to_string()))?;
                let (first, second) = (rest.get(1..split).unwrap_or_default(), rest.get(split + 1..).unwrap_or_default());
                let p = conf_port.as_mut().ok_or_else(|| at("a call outside a trace".to_string()))?;
                let early = match conf_prepare(p, first).map_err(at)? {{
                    {RESULT}::Ok(w) => w,
                    {RESULT}::Err(why) => return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{why}}", first.join(" ")))),
                }};
                if let {RESULT}::Err(why) = conf_run(p, second).map_err(at)? {{
                    return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{why}}", second.join(" "))));
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &next, &format!("after `{{}}`", second.join(" "))).map_err(at)?;
                if conf_commit(p, early).is_ok() {{
                    return {RESULT}::Err(at(format!("the service committed `{{}}`, prepared before `{{}}` changed a row it writes: the write must be conditional on what the row was", first.join(" "), second.join(" "))));
                }}
                conf_compare(&conf_abstract(&p.snapshot()), &next, &format!("after the refused commit of `{{}}`", first.join(" "))).map_err(at)?;
                want = next;
                report.interleavings += 1;
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

/// Prepares and commits a call: the outer `Err` is a call the port cannot make, the inner one
/// the use case's error, as text.
fn conf_run<P: ConformancePort>(conf_port: &mut P, call: &[&str]) -> {RESULT}<{RESULT}<(), {STRING}>, {STRING}> {{
    {RESULT}::Ok(match conf_prepare(conf_port, call)? {{
        {RESULT}::Ok(w) => conf_commit(conf_port, w),
        {RESULT}::Err(why) => {RESULT}::Err(why),
    }})
}}

fn conf_next<'a>(args: &mut impl ::core::iter::Iterator<Item = &'a str>) -> {RESULT}<&'a str, {STRING}> {{
    args.next().ok_or_else(|| "too few values".to_string())
}}

/// The value `t` spells, of the type `T`.
fn conf_parse<T: ::core::str::FromStr>(t: &str) -> {RESULT}<T, {STRING}>
where
    T::Err: {FMT}::Display,
{{
    t.parse::<T>().map_err(|e| format!("{{t}}: {{e}}"))
}}

"##,
        format = string_literal(TRACES_FORMAT),
        FMT = "::core::fmt",
    ));
}

/// The calls: `conf_prepare` reads a call's values and prepares it; `conf_commit` commits it.
fn print_calls(domain: &DomainModel, port: &PortDecl, out: &mut String) {
    // A model whose every action is an entry point has no call: the harness names none.
    if port.methods.is_empty() {
        out.push_str(&format!(
            "fn conf_prepare<P: ConformancePort>(_conf_port: &mut P, call: &[&str]) -> {RESULT}<{RESULT}<ConfWrite, {STRING}>, {STRING}> {{\n    let action = call.first().copied().unwrap_or_default();\n    {RESULT}::Err(format!(\"no use case {{action}} in this port\"))\n}}\n\n"
        ));
        out.push_str(&format!(
            "fn conf_commit<P: ConformancePort>(_conf_port: &mut P, write: ConfWrite) -> {RESULT}<(), {STRING}> {{\n    match write {{}}\n}}\n\n"
        ));
        return;
    }
    // The calls.
    out.push_str(&format!(
        "#[allow(unused_mut, unused_variables)] // a model whose calls take no values reads none\nfn conf_prepare<P: ConformancePort>(conf_port: &mut P, call: &[&str]) -> {RESULT}<{RESULT}<ConfWrite, {STRING}>, {STRING}> {{\n"
    ));
    out.push_str("    let ::core::option::Option::Some((&action, values)) = call.split_first() else {\n        return ::core::result::Result::Err(\"a call names no use case\".to_string());\n    };\n");
    out.push_str("    let mut conf_args = values.iter().copied();\n");
    out.push_str("    match action {\n");
    for (k, m) in port.methods.iter().enumerate() {
        out.push_str(&format!("        {} => {{\n", string_literal(&m.action)));
        let mut args = Vec::new();
        // The harness runs each call as the first actor that performs the use case.
        let caller = port
            .contracts
            .iter()
            .find(|c| c.methods.contains(&k))
            .and_then(|c| c.caller.as_ref());
        if let Some(caller) = caller {
            let cap = &domain.capabilities[caller.actors[0]];
            args.push(format!("{}::from({}::issue())", caller.name, cap.name));
        }
        for a in m.args() {
            let value = match a {
                PortArg::Id(a) => parse(&FieldType::Id(a.ty.clone()), false),
                PortArg::Event(a) => {
                    let event = domain.events.iter().find(|e| e.name == a.ty);
                    let fields: Vec<String> = event
                        .map(|e| e.fields.clone())
                        .unwrap_or_default()
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                ident(&f.name),
                                parse(&FieldType::Id(f.id_ty.clone()), false)
                            )
                        })
                        .collect();
                    if fields.is_empty() {
                        a.ty.clone()
                    } else {
                        format!("{} {{ {} }}", a.ty, fields.join(", "))
                    }
                }
                PortArg::Input(i) => parse(&i.ty, i.nullable),
                PortArg::Choice(a) => parse(&FieldType::Enum(a.ty.clone()), false),
            };
            let v = ident(&format!("conf_{}", a.name()));
            out.push_str(&format!("            let {v} = {value};\n"));
            args.push(v);
        }
        out.push_str(&format!(
            "            {RESULT}::Ok(conf_port.{}({}).map(ConfWrite::{}).map_err(|e| conf_why(&e)))\n        }}\n",
            method_names(m)[0],
            args.join(", "),
            m.name
        ));
    }
    out.push_str(&format!(
        "        other => {RESULT}::Err(format!(\"no use case {{other}} in this port\")),\n    }}\n}}\n\n"
    ));
    out.push_str(&format!(
        "fn conf_commit<P: ConformancePort>(conf_port: &mut P, write: ConfWrite) -> {RESULT}<(), {STRING}> {{\n    match write {{\n"
    ));
    for m in &port.methods {
        out.push_str(&format!(
            "        ConfWrite::{}(w) => conf_port.{}(w).map_err(|e| conf_why(&e)),\n",
            m.name,
            method_names(m)[1]
        ));
    }
    out.push_str("    }\n}\n\n");
    out.push_str(&format!(
        "/// Why the service refused a call: the kind of its error, `: `, and its text.\nfn conf_why(e: &(dyn ::std::error::Error + 'static)) -> {STRING} {{\n    let kind = match e.source() {{\n        {OPTION}::Some(d) if d.is::<NotEnabled>() => \"NotEnabled\",\n        {OPTION}::Some(d) if d.is::<Rejected>() => \"Rejected\",\n        {OPTION}::Some(d) if d.is::<InvalidArgument>() => \"InvalidArgument\",\n        _ => \"an error of no kind\",\n    }};\n    format!(\"{{kind}}: {{e}}\")\n}}\n\n"
    ));
}

/// `conf_seed_row`: the rows of an initial state.
fn print_seed_rows(port: &PortDecl, out: &mut String) {
    // The rows of an initial state.
    out.push_str(&format!(
        "#[allow(unused_mut)] // an entity of one column takes it without advancing\nfn conf_seed_row(seed: &mut StoreSnapshot, toks: &[&str]) -> {RESULT}<(), {STRING}> {{\n"
    ));
    out.push_str("    let ::core::option::Option::Some((&entity, values)) = toks.split_first() else {\n        return ::core::result::Result::Err(\"a row names no entity\".to_string());\n    };\n");
    out.push_str("    let mut conf_args = values.iter().copied();\n");
    out.push_str("    match entity {\n");
    for e in &port.entities {
        let fields: Vec<String> = e
            .columns
            .iter()
            .map(|f| format!("{}: {}", ident(&f.wire), parse(&f.ty, f.nullable)))
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
}

/// `conf_abstract`: the store as the traces spell a state.
fn print_abstraction(port: &PortDecl, out: &mut String) {
    // The abstraction.
    out.push_str(&format!(
        "/// The store as the traces spell a state: one line per row with the columns the model\n/// tracks, per pending run mark, and per raise a triggered use case reads; sorted.\n#[allow(unused_mut)]\nfn conf_abstract(s: &StoreSnapshot) -> {VEC}<{STRING}> {{\n    let mut out: {VEC}<{STRING}> = {VEC}::new();\n"
    ));
    for e in &port.entities {
        out.push_str(&format!("    for row in &s.{} {{\n", ident(&e.field)));
        out.push_str(&format!(
            "        let mut line = format!(\"{} {{}}\", row.{});\n",
            e.entity,
            ident(&e.id.wire)
        ));
        for (field, how) in &e.tracked {
            out.push_str(&format!(
                "        line.push_str(&format!(\" {}={{}}\", {}));\n",
                field.wire,
                abstracted(field, *how)
            ));
        }
        out.push_str("        out.push(line);\n    }\n");
    }
    for r in &port.runs {
        out.push_str(&format!(
            "    for m in &s.{} {{\n        out.push(format!(\"mark {} {} {{}} {{}}\", m.id, m.step));\n    }}\n",
            ident(&r.field),
            r.usecase,
            r.entity
        ));
    }
    for ev in &port.events {
        out.push_str(&format!("    for ev in &s.{} {{\n", ident(&ev.field)));
        for (field, tracked) in &ev.ids {
            if *tracked {
                out.push_str(&format!(
                    "        out.push(format!(\"raised {} {} {{}}\", ev.{}));\n",
                    ev.ty,
                    field.entity,
                    ident(&field.name)
                ));
            }
        }
        out.push_str("    }\n");
    }
    out.push_str("    out.sort();\n    out.dedup();\n    out\n}\n\n");
}

/// The probe (docs/design/code-check.md): a test file of the service's crate that states, one
/// line per use case, that the service implements its contract, so a use case nobody
/// implements is a compile error on its line; and the conformance test, when bound.
pub(super) fn probe(
    domain: &DomainModel,
    binding: &super::ProbeBinding,
) -> Option<super::probe::Probe> {
    let port = domain.port_with_contracts()?;
    let mut w = super::probe::ProbeWriter::new(
        "// Generated by rdra-ish codegen (experimental): the contract probe of `check --code`.",
    );
    w.line(
        "// Never edited by hand: `rdra-ish codegen --code` writes it again. A compile error on a",
    );
    w.line("// use case's line is that use case not implemented by the service.");
    w.line(
        "//! The contract probe of `rdra-ish check --code`: the service implements every use case.",
    );
    w.line("");
    w.line(&format!("type Service = {};", binding.service));
    w.line("");
    let module = &binding.module;
    for c in &port.contracts {
        w.usecase(
            &format!(
                "const _: fn() = || {{ fn uc<T: {module}::{}>() {{}} uc::<Service>(); }}; // usecase {}",
                c.name, c.usecase
            ),
            c,
        );
    }
    if let (Some((new, traces)), true) = (&binding.conformance, domain.harness().is_some()) {
        w.line("");
        w.line("/// The service against the model's traces, by the generated harness.");
        w.line("#[test]");
        w.line("fn rdra_conformance() {");
        w.line(&format!(
            "    let traces = match std::env::var({}) {{",
            string_literal(TRACES_ENV)
        ));
        w.line("        Ok(path) if !path.is_empty() => std::fs::read_to_string(&path)");
        w.line("            .unwrap_or_else(|e| panic!(\"rdra-ish conformance: cannot read {path}: {e}\")),");
        w.line(&format!(
            "        _ => include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {})).to_string(),",
            string_literal(&format!("/{traces}"))
        ));
        w.line("    };");
        w.line(&format!(
            "    if let Err(why) = {module}::conform(|| -> Service {{ {new} }}, &traces) {{"
        ));
        w.line("        panic!(\"rdra-ish conformance: {why}\");");
        w.line("    }");
        w.line("}");
    }
    Some(w.finish())
}

/// The capability of each actor and the caller type of each use case (#337): a prepare or an
/// entry point takes the caller, so code that has no capability of an actor that performs the
/// use case cannot run it.
fn print_capabilities(domain: &DomainModel, port: &super::ir::PortDecl, out: &mut String) {
    if domain.capabilities.is_empty() {
        return;
    }
    out.push_str("// ── Capabilities: who may run each use case ──\n\n");
    for cap in &domain.capabilities {
        out.push_str(&doc(
            &[
                format!("The capability of the actor {} ({}): the use cases it performs take it.", cap.actor, cap.label),
                format!("Made only by `issue`, which the code calls once it has authenticated a {}; rdra-ish does not check who calls it.", cap.actor),
            ],
            "",
        ));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {}(());\n\n", cap.name));
        out.push_str(&format!(
            "impl {} {{\n    /// The capability, for code that has authenticated a {}.\n    pub fn issue() -> Self {{\n        Self(())\n    }}\n}}\n\n",
            cap.name, cap.actor
        ));
    }
    for c in &port.contracts {
        let Some(caller) = &c.caller else {
            continue;
        };
        out.push_str(&format!(
            "/// Who may run {}: an actor that performs it, by its capability.\n",
            c.usecase
        ));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
        out.push_str(&format!("pub enum {} {{\n", caller.name));
        for &i in &caller.actors {
            let cap = &domain.capabilities[i];
            out.push_str(&format!(
                "    /// The actor {}.\n    {}({}),\n",
                cap.actor,
                super::type_ident(&cap.actor),
                cap.name
            ));
        }
        out.push_str("}\n\n");
        for &i in &caller.actors {
            let cap = &domain.capabilities[i];
            out.push_str(&format!(
                "impl ::core::convert::From<{0}> for {1} {{\n    fn from(cap: {0}) -> Self {{\n        Self::{2}(cap)\n    }}\n}}\n\n",
                cap.name,
                caller.name,
                super::type_ident(&cap.actor)
            ));
        }
    }
}

/// One trait per use case (docs/design/code-check.md, decision 6): the prepare and commit
/// methods of its actions. A service implements each; the probe names each.
/// A port method's two calls: `prepare_<action>` and `commit_<action>`.
pub(super) fn method_names(m: &PortMethod) -> [String; 2] {
    let snake = snake(&m.name);
    [format!("prepare_{snake}"), format!("commit_{snake}")]
}

/// The method of an entry point (#335): `run_<action>`.
pub(super) fn entry_name(name: &str) -> String {
    format!("run_{}", snake(name))
}

fn print_contracts(domain: &DomainModel, port: &super::ir::PortDecl, out: &mut String) {
    if port.contracts.is_empty() {
        return;
    }
    print_capabilities(domain, port, out);
    out.push_str("// ── Contracts: one per use case, which the service implements ──\n\n");
    for c in &port.contracts {
        let at = c
            .site
            .as_ref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        out.push_str(&doc(
            &[
                format!("The use case {}{at}: {}.", c.usecase, c.label),
                "Each action with functions is two calls: `prepare_*` loads its rows by id and calls the dispatcher or transition function, writing nothing; the matching `commit_*` stores the rows, marks, and events the result holds in one transaction, each row only where it still is what the result's `<row>_expected` names. Either returns the use case's error and changes nothing when the use case does not run.".to_string(),
            ],
            "",
        ));
        out.push_str(&format!("pub trait {} {{\n", c.name));
        for &i in &c.methods {
            let m = &port.methods[i];
            let error = &c.error;
            out.push_str(&doc(
                &[format!(
                    "Loads the rows of {} (the action {}) and runs it, writing nothing.",
                    m.usecase, m.action
                )],
                "    ",
            ));
            let mut params = params(m);
            if let Some(caller) = &c.caller {
                params.insert(0, format!("by: {}", caller.name));
            }
            out.push_str(&super::rust::allow_arguments(params.len() + 1, "    "));
            let [prepare, commit] = method_names(m);
            out.push_str(&format!(
                "    fn {prepare}(&mut self{}) -> {RESULT}<{}, {error}>;\n",
                params.iter().map(|p| format!(", {p}")).collect::<String>(),
                m.write
            ));
            out.push_str(&doc(
                &[format!(
                    "Stores what `{prepare}` returned in one transaction."
                )],
                "    ",
            ));
            out.push_str(&format!(
                "    fn {commit}(&mut self, write: {}) -> {RESULT}<(), {error}>;\n",
                m.write
            ));
        }
        for e in &c.entries {
            out.push_str(&doc(
                &[format!(
                    "Runs the action {} of {} whole, by the ids of its rows: the generator printed no function for it (#335), so the service writes it and keeps the obligations.",
                    e.action, c.usecase
                )],
                "    ",
            ));
            let mut params: Vec<String> = e
                .rows
                .iter()
                .map(|(n, ty)| format!(", {}: {ty}", ident(n)))
                .collect();
            if let Some(caller) = &c.caller {
                params.insert(0, format!(", by: {}", caller.name));
            }
            out.push_str(&super::rust::allow_arguments(params.len() + 1, "    "));
            out.push_str(&format!(
                "    fn {}(&mut self{}) -> {RESULT}<(), {}>;\n",
                entry_name(&e.name),
                params.concat(),
                c.error
            ));
        }
        out.push_str("}\n\n");
    }
}

fn params(m: &super::ir::PortMethod) -> Vec<String> {
    m.args()
        .map(|a| match a {
            PortArg::Id(a) | PortArg::Event(a) | PortArg::Choice(a) => {
                format!("{}: {}", ident(&a.name), a.ty)
            }
            PortArg::Input(i) => {
                let ty = field_type(&i.ty);
                let ty = if i.nullable {
                    format!("{OPTION}<{ty}>")
                } else {
                    ty
                };
                format!("{}: {ty}", ident(&i.name))
            }
        })
        .collect()
}

/// An expression that reads the next value of `conf_args` as `ty`, through the parser of the
/// type: the harness meets the module at its boundary as a store would.
fn parse(ty: &FieldType, nullable: bool) -> String {
    let value = |t: &str| -> String {
        match ty {
            FieldType::Scalar(Scalar::String) => format!("{t}.to_string()"),
            other => format!("conf_parse::<{}>({t})?", field_type(other)),
        }
    };
    if nullable {
        format!(
            "{{ let t = conf_next(&mut conf_args)?; if t == \"~\" {{ {OPTION}::None }} else {{ {OPTION}::Some({}) }} }}",
            value("t")
        )
    } else {
        format!("{{ let t = conf_next(&mut conf_args)?; {} }}", value("t"))
    }
}

/// The tracked column `field` of `row`, abstracted as the model holds it: a format argument.
fn abstracted(field: &Field, how: Track) -> String {
    let place = format!("row.{}", ident(&field.wire));
    match (how, field.nullable) {
        (Track::Presence, true) => {
            format!("if {place}.is_some() {{ \"+\" }} else {{ \"~\" }}")
        }
        (Track::Presence, false) => "\"+\"".to_string(),
        (Track::Exact, true) => format!(
            "match &{place} {{ {OPTION}::Some(x) => x.to_string(), {OPTION}::None => \"~\".to_string() }}"
        ),
        (Track::Exact, false) => place,
    }
}
