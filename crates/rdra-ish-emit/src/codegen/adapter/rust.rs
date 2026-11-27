//! The Rust model adapter: `ModelState`, `Call`, `ModelAdapter` and `conform`.

use super::super::conformance::{TRACES_ENV, TRACES_FORMAT};
use super::super::rust::{ident, variant_ident};
use super::super::string_literal;
use super::{AdColumn, Adapter, ArgKind, RowToken, Val, REPORT};

const VEC: &str = "::std::vec::Vec";
const STRING: &str = "::std::string::String";
const OPTION: &str = "::core::option::Option";
const RESULT: &str = "::core::result::Result";

pub(super) fn print(a: &Adapter, header: &[String]) -> String {
    let mut out = String::new();
    for line in header {
        if line.is_empty() {
            out.push_str("//\n");
        } else {
            out.push_str(&format!("// {line}\n"));
        }
    }
    out.push('\n');
    out.push_str("//! The model adapter of the conformance traces, as `rdra-ish codegen --adapter` generates it.\n\n");
    print_enums(a, &mut out);
    print_state(a, &mut out);
    print_calls(a, &mut out);
    print_trait(&mut out);
    print_conform(&mut out);
    print_seed_rows(a, &mut out);
    print_call_parser(a, &mut out);
    print_abstraction(a, &mut out);
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// The Rust type of a value.
fn ty(a: &Adapter, val: Val) -> String {
    match val {
        Val::Int => "i64".to_string(),
        Val::Bool => "bool".to_string(),
        Val::Text => STRING.to_string(),
        Val::Enum(i) => a.enums[i].name.clone(),
    }
}

fn column_ty(a: &Adapter, c: &AdColumn) -> String {
    let t = ty(a, c.val);
    if c.nullable {
        format!("{OPTION}<{t}>")
    } else {
        t
    }
}

fn print_enums(a: &Adapter, out: &mut String) {
    if a.enums.is_empty() {
        return;
    }
    out.push_str("// ── Values ──\n\n");
    for e in &a.enums {
        out.push_str(&format!(
            "/// A value of {}, as the model spells it by `as_str`.\n",
            e.name
        ));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\n");
        if e.values
            .iter()
            .any(|(_, id)| !upper_camel(&variant_ident(id)))
        {
            out.push_str("#[allow(non_camel_case_types)] // the model's spelling of a value\n");
        }
        out.push_str(&format!("pub enum {} {{\n", e.name));
        for (wire, id) in &e.values {
            out.push_str(&format!("    /// `{wire}`\n    {},\n", variant_ident(id)));
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl {} {{\n", e.name));
        out.push_str("    /// The value as the model spells it.\n    pub fn as_str(self) -> &'static str {\n        match self {\n");
        for (wire, id) in &e.values {
            out.push_str(&format!(
                "            Self::{} => {},\n",
                variant_ident(id),
                string_literal(wire)
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
        out.push_str(&format!(
            "impl ::core::str::FromStr for {} {{\n    type Err = {STRING};\n\n    /// The value the model spells `text`.\n    fn from_str(text: &str) -> {RESULT}<Self, {STRING}> {{\n        match text {{\n",
            e.name
        ));
        for (wire, id) in &e.values {
            out.push_str(&format!(
                "            {} => {RESULT}::Ok(Self::{}),\n",
                string_literal(wire),
                variant_ident(id)
            ));
        }
        out.push_str(&format!(
            "            _ => {RESULT}::Err(format!(\"{{text}}: not a value of {}\")),\n        }}\n    }}\n}}\n\n",
            e.name
        ));
        out.push_str(&format!(
            "impl ::core::fmt::Display for {} {{\n    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{\n        f.write_str(self.as_str())\n    }}\n}}\n\n",
            e.name
        ));
    }
}

fn upper_camel(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && !name.contains('_')
}

/// The rows, run marks and raises, and `ModelState`.
fn print_state(a: &Adapter, out: &mut String) {
    out.push_str("// ── The store as the model sees it ──\n\n");
    for e in &a.entities {
        out.push_str(&format!(
            "/// A {} row as the model sees it: its id and the columns the model tracks. The\n/// adapter picks the other columns' values when it seeds.\n",
            e.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", e.name));
        out.push_str(&format!(
            "    /// The primary key `{}`.\n    pub {}: {},\n",
            e.id.wire,
            ident(&e.id.wire),
            column_ty(a, &e.id)
        ));
        for c in &e.tracked {
            let what = if c.presence {
                format!("Whether `{}` is set: the model tracks only that.", c.wire)
            } else if c.nullable {
                format!("`{}`; `None` for null.", c.wire)
            } else {
                format!("`{}`.", c.wire)
            };
            out.push_str(&format!(
                "    /// {what}\n    pub {}: {},\n",
                ident(&c.wire),
                column_ty(a, c)
            ));
        }
        out.push_str("}\n\n");
    }
    for r in &a.runs {
        out.push_str(&format!(
            "/// A pending run mark of {} on a {} row: the last step of the run that wrote the row.\n",
            r.usecase, r.entity
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!(
            "pub struct {} {{\n    /// The id of the marked row.\n    pub id: {},\n    /// The last step of the run that wrote the row.\n    pub step: u8,\n}}\n\n",
            r.name,
            ty(a, r.id)
        ));
    }
    for ev in &a.events {
        out.push_str(&format!(
            "/// The payload of a {} event: the ids of the rows it was raised on.\n",
            ev.ty
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", ev.name));
        for (name, entity, val, _) in &ev.ids {
            out.push_str(&format!(
                "    /// The {entity} row.\n    pub {}: {},\n",
                ident(name),
                ty(a, *val)
            ));
        }
        out.push_str("}\n\n");
        if ev.field.is_some() {
            out.push_str(&format!(
                "/// A raise of {} the model remembers: the ids of the rows a triggered use case reads.\n",
                ev.ty
            ));
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
            out.push_str(&format!("pub struct {} {{\n", ev.raised));
            for (name, entity, val, tracked) in &ev.ids {
                if *tracked {
                    out.push_str(&format!(
                        "    /// The {entity} row.\n    pub {}: {},\n",
                        ident(name),
                        ty(a, *val)
                    ));
                }
            }
            out.push_str("}\n\n");
        }
    }
    out.push_str("/// Everything a store holds, as the model sees it: the rows of each entity, the pending run\n/// marks, and the raises a triggered use case reads. The order of a list does not matter.\n");
    out.push_str("#[derive(Debug, Clone, Default, PartialEq, Eq)]\n");
    out.push_str("pub struct ModelState {\n");
    for e in &a.entities {
        out.push_str(&format!(
            "    /// The {} rows.\n    pub {}: {VEC}<{}>,\n",
            e.entity,
            ident(&e.field),
            e.name
        ));
    }
    for r in &a.runs {
        out.push_str(&format!(
            "    /// The pending run marks of {} on {} rows.\n    pub {}: {VEC}<{}>,\n",
            r.usecase,
            r.entity,
            ident(&r.field),
            r.name
        ));
    }
    for ev in &a.events {
        if let Some(field) = &ev.field {
            out.push_str(&format!(
                "    /// The {} events raised.\n    pub {}: {VEC}<{}>,\n",
                ev.ty,
                ident(field),
                ev.raised
            ));
        }
    }
    out.push_str("}\n\n");
}

/// `Call` and `Refusal`.
fn print_calls(a: &Adapter, out: &mut String) {
    out.push_str("// ── Calls ──\n\n");
    out.push_str("/// One call of a use case: the ids of its rows and run marks, its event, its inputs and\n/// its choices, as the traces give them.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\npub enum Call {\n");
    for c in &a.calls {
        out.push_str(&format!(
            "    /// The action {} of the use case {}.\n",
            c.action, c.usecase
        ));
        if c.args.is_empty() {
            out.push_str(&format!("    {},\n", c.pascal));
            continue;
        }
        out.push_str(&format!("    {} {{\n", c.pascal));
        for arg in &c.args {
            let t = match &arg.kind {
                ArgKind::Value { val, nullable } => {
                    let t = ty(a, *val);
                    if *nullable {
                        format!("{OPTION}<{t}>")
                    } else {
                        t
                    }
                }
                ArgKind::Event(i) => a.events[*i].name.clone(),
            };
            out.push_str(&format!(
                "        /// `{}`.\n        {}: {t},\n",
                arg.name,
                ident(&arg.name)
            ));
        }
        out.push_str("    },\n");
    }
    out.push_str("}\n\n");
    out.push_str("/// Why a use case did not run. The store is unchanged.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum Refusal {\n");
    out.push_str("    /// The call does not run from its rows' states, a row it names does not exist, or the rows\n    /// a commit writes changed since its prepare.\n    NotEnabled,\n");
    out.push_str(
        "    /// A condition the model applies to the use case rejects the call.\n    Rejected,\n",
    );
    out.push_str(
        "    /// An argument breaks what the verified model keeps.\n    InvalidArgument,\n}\n\n",
    );
    out.push_str("impl Refusal {\n    /// The kind, in words.\n    pub fn as_str(self) -> &'static str {\n        match self {\n            Self::NotEnabled => \"not enabled\",\n            Self::Rejected => \"rejected\",\n            Self::InvalidArgument => \"invalid argument\",\n        }\n    }\n}\n\n");
}

/// `ModelAdapter` and `AdapterReport`.
fn print_trait(out: &mut String) {
    out.push_str(&format!(
        r#"// ── The adapter ──

/// What a service offers the conformance harness: a store it can empty, seed and read back
/// as the model sees it, and its use cases by call. It uses no type of the generated domain
/// code, so the service may be written, renamed and restructured freely.
pub trait ModelAdapter {{
    /// Empties the store: a trace starts here.
    fn reset(&mut self);
    /// Stores these rows in the empty store, with a value of the adapter's choosing for each
    /// column the model does not track.
    fn seed(&mut self, state: &ModelState);
    /// What the store holds now, as the model sees it.
    fn observe(&mut self) -> ModelState;
    /// Makes the call in one transaction: `Ok` when it ran, else why it did not, with the
    /// store unchanged.
    ///
    /// # Errors
    ///
    /// The kind of refusal, when the use case does not run.
    fn invoke(&mut self, call: &Call) -> {RESULT}<(), Refusal>;
    /// Optional, for the interleaved rows of the traces: reads the call's rows and holds its
    /// write for [`ModelAdapter::commit`], writing nothing. `None`, the default, when the
    /// service does not split a call: the harness then runs those rows as plain calls and
    /// counts them as skipped, so conditional writes go unchecked.
    fn prepare(&mut self, call: &Call) -> {OPTION}<{RESULT}<(), Refusal>> {{
        let _ = call;
        {OPTION}::None
    }}
    /// Stores the write [`ModelAdapter::prepare`] holds in one transaction, only where its
    /// rows are still what the prepare read.
    ///
    /// # Errors
    ///
    /// The kind of refusal, when the rows changed.
    fn commit(&mut self) -> {RESULT}<(), Refusal> {{
        {RESULT}::Err(Refusal::NotEnabled)
    }}
}}

/// What `conform` checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdapterReport {{
    /// The traces run.
    pub traces: usize,
    /// The calls the model runs, made.
    pub calls: usize,
    /// The calls the model refuses, made and refused.
    pub refusals: usize,
    /// The commits after another call changed a row they write, refused.
    pub interleavings: usize,
    /// The interleaved rows run as plain calls: the adapter has no `prepare`.
    pub skipped: usize,
}}

impl AdapterReport {{
    /// The line a conformance test prints on a pass, which `check --code=run` reads.
    pub fn line(&self) -> {STRING} {{
        format!(
            "{report}{{}} traces, {{}} calls, {{}} refusals, {{}} interleavings, {{}} interleavings skipped",
            self.traces, self.calls, self.refusals, self.interleavings, self.skipped
        )
    }}
}}

"#,
        report = REPORT,
    ));
}

/// `conform` and the helpers every model shares.
fn print_conform(out: &mut String) {
    out.push_str(&format!(
        r##"// ── The harness ──

/// Drives `adapter` along `traces` (the file `codegen --conformance` writes): resets and seeds
/// each trace's initial state, makes each call, and after each compares what the store holds
/// with the state the model reaches. A call the model refuses must be refused and change
/// nothing, and so must a commit whose rows another call changed after its prepare. Returns
/// the first difference.
///
/// # Errors
///
/// The first difference, or what is wrong with `traces`, as text naming its line.
pub fn conform<A: ModelAdapter>(adapter: &mut A, traces: &str) -> {RESULT}<AdapterReport, {STRING}> {{
    let mut lines = traces.lines().enumerate();
    match lines.next() {{
        {OPTION}::Some((_, first)) if first.trim_end() == {format} => {{}}
        _ => return {RESULT}::Err("not a traces file of this version: generate the adapter and the traces again".to_string()),
    }}
    let mut states: ::std::collections::BTreeMap<{STRING}, {VEC}<{STRING}>> = ::std::collections::BTreeMap::new();
    let mut defining: {OPTION}<{STRING}> = {OPTION}::None;
    let mut report = AdapterReport::default();
    let mut in_trace = false;
    let mut seed = ModelState::default();
    let mut want: {VEC}<{STRING}> = {VEC}::new();
    for (n, line) in lines {{
        let toks: {VEC}<&str> = line.split_whitespace().collect();
        let {OPTION}::Some((&kind, rest)) = toks.split_first() else {{
            continue;
        }};
        let at = |what: {STRING}| format!("traces line {{}}: {{what}}", n + 1);
        let state = |id: {OPTION}<&&str>| -> {RESULT}<{VEC}<{STRING}>, {STRING}> {{
            id.and_then(|id| states.get(*id)).cloned().ok_or_else(|| at("names no state defined above".to_string()))
        }};
        if matches!(kind, "G" | "C" | "X" | "I") && !in_trace {{
            return {RESULT}::Err(at("a call outside a trace".to_string()));
        }}
        match kind {{
            "#" => {{}}
            "S" => {{
                let id = rest.first().ok_or_else(|| at("a state without a number".to_string()))?.to_string();
                states.insert(id.clone(), {VEC}::new());
                defining = {OPTION}::Some(id);
            }}
            "A" => {{
                let id = defining.as_ref().ok_or_else(|| at("a row outside a state".to_string()))?;
                if let {OPTION}::Some(rows) = states.get_mut(id) {{
                    rows.push(rest.join(" "));
                }}
            }}
            "T" => {{
                want = state(rest.first())?;
                adapter.reset();
                in_trace = true;
                seed = ModelState::default();
                report.traces += 1;
            }}
            "R" => ad_seed_row(&mut seed, rest).map_err(at)?,
            "G" => {{
                adapter.seed(&::core::mem::take(&mut seed));
                ad_compare(&ad_abstract(&adapter.observe()), &want, "after seeding the initial state").map_err(at)?;
            }}
            "C" => {{
                let next = state(rest.first())?;
                let call = rest.get(1..).unwrap_or_default();
                if let {RESULT}::Err(why) = adapter.invoke(&ad_call(call).map_err(at)?) {{
                    return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{}}", call.join(" "), why.as_str())));
                }}
                ad_compare(&ad_abstract(&adapter.observe()), &next, &format!("after `{{}}`", call.join(" "))).map_err(at)?;
                want = next;
                report.calls += 1;
            }}
            "X" => {{
                if adapter.invoke(&ad_call(rest).map_err(at)?).is_ok() {{
                    return {RESULT}::Err(at(format!("the model refuses `{{}}` here, and the service ran it", rest.join(" "))));
                }}
                ad_compare(&ad_abstract(&adapter.observe()), &want, &format!("after the refused `{{}}`", rest.join(" "))).map_err(at)?;
                report.refusals += 1;
            }}
            "I" => {{
                let next = state(rest.first())?;
                let split = rest.iter().position(|t| *t == "|").ok_or_else(|| at("an interleaving without `|`".to_string()))?;
                let (first, second) = (rest.get(1..split).unwrap_or_default(), rest.get(split + 1..).unwrap_or_default());
                let (early, late) = (ad_call(first).map_err(at)?, ad_call(second).map_err(at)?);
                let held = match adapter.prepare(&early) {{
                    {OPTION}::None => false,
                    {OPTION}::Some({RESULT}::Ok(())) => true,
                    {OPTION}::Some({RESULT}::Err(why)) => return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{}}", first.join(" "), why.as_str()))),
                }};
                if let {RESULT}::Err(why) = adapter.invoke(&late) {{
                    return {RESULT}::Err(at(format!("the model runs `{{}}` here, and the service refused it: {{}}", second.join(" "), why.as_str())));
                }}
                ad_compare(&ad_abstract(&adapter.observe()), &next, &format!("after `{{}}`", second.join(" "))).map_err(at)?;
                if held {{
                    if adapter.commit().is_ok() {{
                        return {RESULT}::Err(at(format!("the service committed `{{}}`, prepared before `{{}}` changed a row it writes: the write must be conditional on what the row was", first.join(" "), second.join(" "))));
                    }}
                    ad_compare(&ad_abstract(&adapter.observe()), &next, &format!("after the refused commit of `{{}}`", first.join(" "))).map_err(at)?;
                    report.interleavings += 1;
                }} else {{
                    report.skipped += 1;
                }}
                want = next;
            }}
            other => return {RESULT}::Err(at(format!("unknown record {{other}}"))),
        }}
    }}
    {RESULT}::Ok(report)
}}

/// The store against the model's state: every line one holds and the other does not.
fn ad_compare(got: &[{STRING}], want: &[{STRING}], when: &str) -> {RESULT}<(), {STRING}> {{
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

fn ad_next<'a>(args: &mut impl ::core::iter::Iterator<Item = &'a str>) -> {RESULT}<&'a str, {STRING}> {{
    args.next().ok_or_else(|| "too few values".to_string())
}}

#[allow(dead_code)] // a model with no Int value reads none
fn ad_int(t: &str) -> {RESULT}<i64, {STRING}> {{
    t.parse::<i64>().map_err(|e| format!("{{t}}: {{e}}"))
}}

#[allow(dead_code)] // a model with no Bool value reads none
fn ad_bool(t: &str) -> {RESULT}<bool, {STRING}> {{
    t.parse::<bool>().map_err(|e| format!("{{t}}: {{e}}"))
}}

"##,
        format = string_literal(TRACES_FORMAT),
    ));
}

/// An expression that reads the next token of `ad_args` as `val`.
fn read(a: &Adapter, val: Val, nullable: bool, presence: bool) -> String {
    let value = match val {
        _ if presence => "t != \"~\"".to_string(),
        Val::Int => "ad_int(t)?".to_string(),
        Val::Bool => "ad_bool(t)?".to_string(),
        Val::Text => "t.to_string()".to_string(),
        Val::Enum(i) => format!("t.parse::<{}>()?", a.enums[i].name),
    };
    if nullable {
        format!(
            "{{ let t = ad_next(&mut ad_args)?; if t == \"~\" {{ {OPTION}::None }} else {{ {OPTION}::Some({value}) }} }}"
        )
    } else {
        format!("{{ let t = ad_next(&mut ad_args)?; {value} }}")
    }
}

/// `ad_seed_row`: a row `R` of the traces into the state to seed, its untracked columns
/// skipped.
fn print_seed_rows(a: &Adapter, out: &mut String) {
    out.push_str(&format!(
        "#[allow(unused_mut)] // an entity of one column takes it without advancing\nfn ad_seed_row(seed: &mut ModelState, toks: &[&str]) -> {RESULT}<(), {STRING}> {{\n"
    ));
    out.push_str(&format!("    let {OPTION}::Some((&entity, values)) = toks.split_first() else {{\n        return {RESULT}::Err(\"a row names no entity\".to_string());\n    }};\n"));
    out.push_str("    let mut ad_args = values.iter().copied();\n    match entity {\n");
    for e in &a.entities {
        out.push_str(&format!("        {} => {{\n", string_literal(&e.entity)));
        let mut fields = Vec::new();
        for (k, token) in e.tokens.iter().enumerate() {
            match token {
                RowToken::Id => {
                    out.push_str(&format!(
                        "            let ad_id = {};\n",
                        read(a, e.id.val, false, false)
                    ));
                    fields.push(format!("{}: ad_id", ident(&e.id.wire)));
                }
                RowToken::Tracked(i) => {
                    let c = &e.tracked[*i];
                    out.push_str(&format!(
                        "            let ad_{k} = {};\n",
                        read(a, c.val, c.nullable, c.presence)
                    ));
                    fields.push(format!("{}: ad_{k}", ident(&c.wire)));
                }
                RowToken::Skip => {
                    out.push_str("            ad_next(&mut ad_args)?;\n");
                }
            }
        }
        out.push_str(&format!(
            "            seed.{}.push({} {{ {} }});\n        }}\n",
            ident(&e.field),
            e.name,
            fields.join(", ")
        ));
    }
    out.push_str(&format!(
        "        other => return {RESULT}::Err(format!(\"no entity {{other}} in this adapter\")),\n    }}\n    {RESULT}::Ok(())\n}}\n\n"
    ));
}

/// `ad_call`: a call of the traces as a `Call`.
fn print_call_parser(a: &Adapter, out: &mut String) {
    // A model whose every action is an entry point has no call: the harness names none.
    if a.calls.is_empty() {
        out.push_str(&format!(
            "fn ad_call(call: &[&str]) -> {RESULT}<Call, {STRING}> {{\n    let action = call.first().copied().unwrap_or_default();\n    {RESULT}::Err(format!(\"no use case {{action}} in this adapter\"))\n}}\n\n"
        ));
        return;
    }
    out.push_str(&format!(
        "#[allow(unused_mut, unused_variables)] // a model whose calls take no values reads none\nfn ad_call(call: &[&str]) -> {RESULT}<Call, {STRING}> {{\n"
    ));
    out.push_str(&format!("    let {OPTION}::Some((&action, values)) = call.split_first() else {{\n        return {RESULT}::Err(\"a call names no use case\".to_string());\n    }};\n"));
    out.push_str("    let mut ad_args = values.iter().copied();\n    let call = match action {\n");
    for c in &a.calls {
        if c.args.is_empty() {
            out.push_str(&format!(
                "        {} => Call::{},\n",
                string_literal(&c.action),
                c.pascal
            ));
            continue;
        }
        let mut fields = Vec::new();
        for arg in &c.args {
            let value = match &arg.kind {
                ArgKind::Value { val, nullable } => read(a, *val, *nullable, false),
                ArgKind::Event(i) => {
                    let ev = &a.events[*i];
                    let ids: Vec<String> = ev
                        .ids
                        .iter()
                        .map(|(name, _, val, _)| {
                            format!("{}: {}", ident(name), read(a, *val, false, false))
                        })
                        .collect();
                    format!("{} {{ {} }}", ev.name, ids.join(", "))
                }
            };
            fields.push(format!("{}: {value}", ident(&arg.name)));
        }
        out.push_str(&format!(
            "        {} => Call::{} {{ {} }},\n",
            string_literal(&c.action),
            c.pascal,
            fields.join(", ")
        ));
    }
    out.push_str(&format!(
        "        other => return {RESULT}::Err(format!(\"no use case {{other}} in this adapter\")),\n    }};\n    {RESULT}::Ok(call)\n}}\n\n"
    ));
}

/// A tracked column of `row`, as the traces spell it: a format argument.
fn abstracted(c: &AdColumn) -> String {
    let place = format!("row.{}", ident(&c.wire));
    if c.presence {
        return format!("if {place} {{ \"+\" }} else {{ \"~\" }}");
    }
    if c.nullable {
        format!(
            "match &{place} {{ {OPTION}::Some(x) => x.to_string(), {OPTION}::None => \"~\".to_string() }}"
        )
    } else {
        place
    }
}

/// `ad_abstract`: the state as the traces spell it.
fn print_abstraction(a: &Adapter, out: &mut String) {
    out.push_str(&format!(
        "/// The state as the traces spell it: one line per row with the columns the model tracks,\n/// per pending run mark, and per raise a triggered use case reads; sorted.\n#[allow(unused_mut)]\nfn ad_abstract(s: &ModelState) -> {VEC}<{STRING}> {{\n    let mut out: {VEC}<{STRING}> = {VEC}::new();\n"
    ));
    for e in &a.entities {
        out.push_str(&format!("    for row in &s.{} {{\n", ident(&e.field)));
        out.push_str(&format!(
            "        let mut line = format!(\"{} {{}}\", row.{});\n",
            e.entity,
            ident(&e.id.wire)
        ));
        for c in &e.tracked {
            out.push_str(&format!(
                "        line.push_str(&format!(\" {}={{}}\", {}));\n",
                c.wire,
                abstracted(c)
            ));
        }
        out.push_str("        out.push(line);\n    }\n");
    }
    for r in &a.runs {
        out.push_str(&format!(
            "    for m in &s.{} {{\n        out.push(format!(\"mark {} {} {{}} {{}}\", m.id, m.step));\n    }}\n",
            ident(&r.field),
            r.usecase,
            r.entity
        ));
    }
    for ev in &a.events {
        let Some(field) = &ev.field else {
            continue;
        };
        out.push_str(&format!("    for ev in &s.{} {{\n", ident(field)));
        for (name, entity, _, tracked) in &ev.ids {
            if *tracked {
                out.push_str(&format!(
                    "        out.push(format!(\"raised {} {entity} {{}}\", ev.{}));\n",
                    ev.ty,
                    ident(name)
                ));
            }
        }
        out.push_str("    }\n");
    }
    out.push_str("    out.sort();\n    out.dedup();\n    out\n}\n");
}

/// The adapter probe: a test of the service's crate that drives the adapter `new` makes
/// along the traces, and prints what it ran.
pub(super) fn probe(
    binding: &super::super::ProbeBinding,
    new: &str,
    traces: &str,
) -> Option<super::super::probe::Probe> {
    let mut w = super::super::probe::ProbeWriter::new(
        "// Generated by rdra-ish codegen (experimental): the adapter probe of `check --code`.",
    );
    w.line("// Never edited by hand: `rdra-ish codegen --code` writes it again.");
    w.line("//! The conformance test of `rdra-ish check --code=run`: the model adapter along the traces.");
    w.line("");
    let module = &binding.module;
    w.line(&format!("type Adapter = {};", binding.service));
    w.line("");
    w.line(&format!(
        "const _: fn() = || {{ fn adapter<T: {module}::ModelAdapter>() {{}} adapter::<Adapter>(); }};"
    ));
    w.line("");
    w.line("/// The adapter against the model's traces, by the generated harness.");
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
    w.line(&format!("    let mut adapter: Adapter = {new};"));
    w.line(&format!(
        "    match {module}::conform(&mut adapter, &traces) {{"
    ));
    w.line("        Ok(report) => println!(\"{}\", report.line()),");
    w.line("        Err(why) => panic!(\"rdra-ish conformance: {why}\"),");
    w.line("    }");
    w.line("}");
    Some(w.finish())
}
