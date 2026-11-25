//! The TypeScript contracts, conformance port and harness: one `<UseCase>UseCase` type per use
//! case (function-property notation, so a narrower parameter does not pass), `StoreSnapshot`,
//! `ConformancePort` as their composition, and `conform`, which drives a port along the traces
//! `conformance::traces` writes. The contracts are async (`Promise<Result<..>>`): a TypeScript
//! service reads its store asynchronously; the Rust and Go ones are sync.

use super::conformance::TRACES_FORMAT;
use super::ir::{DomainModel, Field, FieldType, PortMethod, Scalar, Track};
use super::string_literal;
use super::ts::{column_type, doc, doc1, local, property};

/// The contract type of the use case `usecase` (provisional: the IR will name it).
pub(super) fn contract_name(usecase: &str) -> String {
    format!("{}UseCase", super::type_ident(usecase))
}

/// The use cases of the port, in the order of their first method.
fn usecases(methods: &[PortMethod]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for m in methods {
        if !out.contains(&m.usecase.as_str()) {
            out.push(&m.usecase);
        }
    }
    out
}

fn error_type(domain: &DomainModel, usecase: &str) -> String {
    domain
        .error_of(usecase)
        .map_or_else(|| "never".to_string(), |e| e.name.clone())
}

/// `name: type` per argument of a method's `prepare`.
fn params(m: &PortMethod) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = m
        .rows
        .iter()
        .chain(&m.marks)
        .map(|(n, ty)| (local(n), ty.clone()))
        .collect();
    if let Some((n, ty)) = &m.event {
        out.push((local(n), ty.clone()));
    }
    for i in &m.inputs {
        out.push((local(&i.name), column_type(&i.ty, i.nullable)));
    }
    for (n, ty) in &m.choices {
        out.push((local(n), ty.clone()));
    }
    out
}

/// The contract types, one per use case with printed functions.
pub(super) fn print_contracts(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    if port.methods.is_empty() {
        return;
    }
    out.push_str("// ── Contracts: one per use case, which the service implements ──\n\n");
    for uc in usecases(&port.methods) {
        let decl = domain.usecases.iter().find(|u| u.id == uc);
        let mut lines = vec![format!(
            "What a service offers for the use case {uc}{}: per action, `prepare*` loads its rows by id",
            decl.and_then(|d| d.site.as_ref())
                .map(|s| format!(" ({s})"))
                .unwrap_or_default()
        )];
        lines.push("and returns what the core returns, writing nothing; `commit*` stores that result in one".to_string());
        lines.push("transaction, each row only where it still is what the result expects. Either returns an".to_string());
        lines.push("error and changes nothing when the use case does not run.".to_string());
        out.push_str(&doc(&lines, ""));
        let error = error_type(domain, uc);
        out.push_str(&format!(
            "export type {} = Readonly<{{\n",
            contract_name(uc)
        ));
        for m in port.methods.iter().filter(|m| m.usecase == uc) {
            out.push_str(&doc1_indent(&format!(
                "Runs {} (the action {}).",
                m.usecase, m.action
            )));
            let ps: Vec<String> = params(m)
                .iter()
                .map(|(n, ty)| format!("{n}: {ty}"))
                .collect();
            out.push_str(&format!(
                "  prepare{}: ({}) => Promise<Result<{}, {error}>>;\n",
                m.name,
                ps.join(", "),
                m.write
            ));
            out.push_str(&format!(
                "  commit{}: (write: {}) => Promise<Result<void, {error}>>;\n",
                m.name, m.write
            ));
        }
        out.push_str("}>;\n\n");
    }
}

fn doc1_indent(line: &str) -> String {
    doc(&[line.to_string()], "  ")
}

/// The port and the harness.
pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    let mut p = String::new();
    p.push_str(
        "// ── Conformance: the port a finished service implements, and the harness that drives it ──\n\n",
    );
    for r in &port.runs {
        p.push_str(&doc1(&format!(
            "A run mark of {} on a {} row, as a store keeps it: the last step of the run that wrote the row.",
            r.usecase, r.entity
        )));
        p.push_str(&format!(
            "export type {} = Readonly<{{ id: {}; step: number }}>;\n\n",
            r.name, r.id_ty
        ));
    }
    p.push_str(&doc(
        &[
            "Everything a store holds, as the conformance harness reads it: the rows of each entity,".to_string(),
            "the pending run marks, and the events raised on rows that a triggered use case reads.".to_string(),
        ],
        "",
    ));
    let mut fields: Vec<(String, String)> = Vec::new();
    for e in &port.entities {
        fields.push((property(&e.field), e.ty.clone()));
    }
    for r in &port.runs {
        fields.push((property(&r.field), r.name.clone()));
    }
    for ev in &port.events {
        fields.push((property(&ev.field), ev.ty.clone()));
    }
    p.push_str("export type StoreSnapshot = Readonly<{\n");
    for (f, ty) in &fields {
        p.push_str(&format!("  {f}: readonly {ty}[];\n"));
    }
    p.push_str("}>;\n\n");

    p.push_str(&doc(
        &[
            "What a finished service offers the conformance harness: `seed` empties the store and stores".to_string(),
            "the rows of a trace's initial state, `snapshot` returns everything it holds, and the".to_string(),
            "contract of every use case runs the calls.".to_string(),
        ],
        "",
    ));
    p.push_str("export type ConformancePort = Readonly<{\n  seed: (rows: StoreSnapshot) => Promise<void>;\n  snapshot: () => Promise<StoreSnapshot>;\n}>");
    for uc in usecases(&port.methods) {
        p.push_str(&format!(" &\n  {}", contract_name(uc)));
    }
    p.push_str(";\n\n");

    p.push_str(&doc1("What `conform` checked."));
    p.push_str("export type ConformanceReport = Readonly<{ traces: number; calls: number; refusals: number; interleavings: number }>;\n\n");

    // A prepared write of any use case.
    p.push_str("type $ConfWrite =\n");
    if port.methods.is_empty() {
        p.push_str("  never;\n\n");
    } else {
        p.push_str(&super::ts::union(port.methods.iter().map(|m| {
            (
                format!(
                    "Readonly<{{ action: {}; write: {} }}>",
                    string_literal(&m.action),
                    m.write
                ),
                String::new(),
            )
        })));
        p.push('\n');
    }
    p.push_str("type $ConfStore = {\n");
    for (f, ty) in &fields {
        p.push_str(&format!("  {f}: {ty}[];\n"));
    }
    p.push_str("};\n\n");

    p.push_str(&format!(
        r##"/**
 * Drives a fresh port per trace along `traces` (the file `codegen --conformance` writes): seeds
 * each trace's initial state, makes each call, and after each compares what the store holds with
 * the state the model reaches. A call the model refuses must return an error and change nothing,
 * and so must a commit whose rows another call changed after its prepare. Returns the first
 * difference.
 */
export async function conform(newPort: () => ConformancePort, traces: string): Promise<Result<ConformanceReport, string>> {{
  const lines = traces.split("\n");
  if (lines[0]?.trimEnd() !== {format}) {{
    return {{ ok: false, error: "not a traces file of this version: generate the code and the traces again" }};
  }}
  const states = new Map<string, string[]>();
  let defining: string[] | null = null;
  let report = {{ traces: 0, calls: 0, refusals: 0, interleavings: 0 }};
  let port: ConformancePort | null = null;
  let seed = $confEmpty();
  let want: readonly string[] = [];
  for (let n = 1; n < lines.length; n++) {{
    const toks = (lines[n] ?? "").split(/\s+/).filter((t) => t !== "");
    const [kind, ...rest] = toks;
    const at = (what: string): Result<never, string> => ({{ ok: false, error: `traces line ${{n + 1}}: ${{what}}` }});
    if (kind === undefined || kind === "#") {{
      continue;
    }}
    const state = (id: string | undefined): readonly string[] | undefined => (id === undefined ? undefined : states.get(id));
    switch (kind) {{
      case "S": {{
        const id = rest[0];
        if (id === undefined) {{
          return at("a state without a number");
        }}
        defining = [];
        states.set(id, defining);
        break;
      }}
      case "A": {{
        if (defining === null) {{
          return at("a row outside a state");
        }}
        defining.push(rest.join(" "));
        break;
      }}
      case "T": {{
        const s = state(rest[0]);
        if (s === undefined) {{
          return at("names no state defined above");
        }}
        want = s;
        port = newPort();
        seed = $confEmpty();
        report = {{ ...report, traces: report.traces + 1 }};
        break;
      }}
      case "R": {{
        const row = $confSeedRow(seed, rest);
        if (!row.ok) {{
          return at(row.error);
        }}
        break;
      }}
      case "G": {{
        if (port === null) {{
          return at("seeding outside a trace");
        }}
        await port.seed(seed);
        const diff = $confCompare($confAbstract(await port.snapshot()), want, "after seeding the initial state");
        if (diff !== null) {{
          return at(diff);
        }}
        break;
      }}
      case "C": {{
        const next = state(rest[0]);
        if (next === undefined) {{
          return at("names no state defined above");
        }}
        const call = rest.slice(1);
        if (port === null) {{
          return at("a call outside a trace");
        }}
        const ran = await $confRun(port, call);
        if (!ran.ok) {{
          return at(ran.error);
        }}
        if (!ran.value.ok) {{
          return at(`the model runs \`${{call.join(" ")}}\` here, and the service refused it: ${{$confShow(ran.value.error)}}`);
        }}
        const diff = $confCompare($confAbstract(await port.snapshot()), next, `after \`${{call.join(" ")}}\``);
        if (diff !== null) {{
          return at(diff);
        }}
        want = next;
        report = {{ ...report, calls: report.calls + 1 }};
        break;
      }}
      case "X": {{
        if (port === null) {{
          return at("a call outside a trace");
        }}
        const ran = await $confRun(port, rest);
        if (!ran.ok) {{
          return at(ran.error);
        }}
        if (ran.value.ok) {{
          return at(`the model refuses \`${{rest.join(" ")}}\` here, and the service ran it`);
        }}
        const diff = $confCompare($confAbstract(await port.snapshot()), want, `after the refused \`${{rest.join(" ")}}\``);
        if (diff !== null) {{
          return at(diff);
        }}
        report = {{ ...report, refusals: report.refusals + 1 }};
        break;
      }}
      case "I": {{
        const next = state(rest[0]);
        if (next === undefined) {{
          return at("names no state defined above");
        }}
        const split = rest.indexOf("|");
        if (split < 0) {{
          return at("an interleaving without `|`");
        }}
        const first = rest.slice(1, split);
        const second = rest.slice(split + 1);
        if (port === null) {{
          return at("a call outside a trace");
        }}
        const early = await $confPrepare(port, first);
        if (!early.ok) {{
          return at(early.error);
        }}
        if (!early.value.ok) {{
          return at(`the model runs \`${{first.join(" ")}}\` here, and the service refused it: ${{$confShow(early.value.error)}}`);
        }}
        const ran = await $confRun(port, second);
        if (!ran.ok) {{
          return at(ran.error);
        }}
        if (!ran.value.ok) {{
          return at(`the model runs \`${{second.join(" ")}}\` here, and the service refused it: ${{$confShow(ran.value.error)}}`);
        }}
        const between = $confCompare($confAbstract(await port.snapshot()), next, `after \`${{second.join(" ")}}\``);
        if (between !== null) {{
          return at(between);
        }}
        const late = await $confCommit(port, early.value.value);
        if (late.ok) {{
          return at(`the service committed \`${{first.join(" ")}}\`, prepared before \`${{second.join(" ")}}\` changed a row it writes: the write must be conditional on what the row was`);
        }}
        const diff = $confCompare($confAbstract(await port.snapshot()), next, `after the refused commit of \`${{first.join(" ")}}\``);
        if (diff !== null) {{
          return at(diff);
        }}
        want = next;
        report = {{ ...report, interleavings: report.interleavings + 1 }};
        break;
      }}
      default:
        return at(`unknown record ${{kind}}`);
    }}
  }}
  return {{ ok: true, value: report }};
}}

function $confEmpty(): $ConfStore {{
  return {{ {empty} }};
}}

/** An error value as one line. */
function $confShow(error: unknown): string {{
  return globalThis.JSON.stringify(error) ?? "an error";
}}

/** The store against the model's state, both as sorted sets of lines: what one holds and the other does not, or null. */
function $confCompare(got: readonly string[], want: readonly string[], when: string): string | null {{
  const g = [...new Set(got)].sort();
  const w = [...new Set(want)].sort();
  if (g.length === w.length && g.every((line, i) => line === w[i])) {{
    return null;
  }}
  let out = `${{when}}, the store differs from the model:`;
  for (const line of w.filter((l) => !g.includes(l))) {{
    out += `\n  model only: ${{line}}`;
  }}
  for (const line of g.filter((l) => !w.includes(l))) {{
    out += `\n  store only: ${{line}}`;
  }}
  return out;
}}

/** Prepares and commits a call: an error when the call cannot be read, else what the service returned. */
async function $confRun(port: ConformancePort, call: readonly string[]): Promise<Result<Result<void, unknown>, string>> {{
  const prepared = await $confPrepare(port, call);
  if (!prepared.ok) {{
    return prepared;
  }}
  if (!prepared.value.ok) {{
    return {{ ok: true, value: prepared.value }};
  }}
  return {{ ok: true, value: await $confCommit(port, prepared.value.value) }};
}}

/** The value at `values[i]`, read by `read`. */
function $confArg<T>(values: readonly string[], i: number, read: (text: string) => Result<T, string>): Result<T, string> {{
  const text = values[i];
  return text === undefined ? {{ ok: false, error: "too few values" }} : read(text);
}}

"##,
        format = string_literal(TRACES_FORMAT),
        empty = fields
            .iter()
            .map(|(f, _)| format!("{f}: []"))
            .collect::<Vec<_>>()
            .join(", "),
    ));

    // The calls.
    p.push_str("async function $confPrepare(port: ConformancePort, call: readonly string[]): Promise<Result<Result<$ConfWrite, unknown>, string>> {\n");
    p.push_str("  const [action, ...values] = call;\n  switch (action) {\n");
    for m in &port.methods {
        p.push_str(&format!("    case {}: {{\n", string_literal(&m.action)));
        let mut args = Vec::new();
        let mut i = 0usize;
        let mut arg = |p: &mut String, reader: String| -> String {
            let name = format!("$a{i}");
            p.push_str(&format!(
                "      const {name} = $confArg(values, {i}, {reader});\n      if (!{name}.ok) {{\n        return {name};\n      }}\n"
            ));
            i += 1;
            format!("{name}.value")
        };
        for (_, ty) in m.rows.iter().chain(&m.marks) {
            args.push(arg(
                &mut p,
                reader(domain, &FieldType::Id(ty.clone()), false),
            ));
        }
        if let Some((_, ty)) = &m.event {
            let event = domain.events.iter().find(|e| e.name == *ty);
            let mut parts = vec![format!("kind: {}", string_literal(ty))];
            for f in event.map(|e| e.fields.clone()).unwrap_or_default() {
                let v = arg(
                    &mut p,
                    reader(domain, &FieldType::Id(f.id_ty.clone()), false),
                );
                parts.push(format!("{}: {v}", property(&f.name)));
            }
            args.push(format!("{{ {} }}", parts.join(", ")));
        }
        for input in &m.inputs {
            args.push(arg(&mut p, reader(domain, &input.ty, input.nullable)));
        }
        for (_, ty) in &m.choices {
            args.push(arg(
                &mut p,
                reader(domain, &FieldType::Enum(ty.clone()), false),
            ));
        }
        p.push_str(&format!(
            "      const r = await port.prepare{}({});\n      return {{ ok: true, value: r.ok ? {{ ok: true, value: {{ action: {}, write: r.value }} }} : r }};\n    }}\n",
            m.name,
            args.join(", "),
            string_literal(&m.action)
        ));
    }
    p.push_str("    default:\n      return { ok: false, error: `no use case ${action ?? \"\"} in this port` };\n  }\n}\n\n");

    p.push_str("function $confCommit(port: ConformancePort, w: $ConfWrite): Promise<Result<void, unknown>> {\n  const action = w.action;\n  switch (action) {\n");
    for m in &port.methods {
        p.push_str(&format!(
            "    case {}:\n      return port.commit{}(w.write);\n",
            string_literal(&m.action),
            m.name
        ));
    }
    p.push_str("    default:\n      return assertNever(action);\n  }\n}\n\n");

    // The rows of an initial state.
    p.push_str("function $confSeedRow(seed: $ConfStore, toks: readonly string[]): Result<void, string> {\n");
    p.push_str("  const [entity, ...values] = toks;\n  switch (entity) {\n");
    for e in &port.entities {
        p.push_str(&format!("    case {}: {{\n", string_literal(&e.entity)));
        let mut parts = Vec::new();
        for (i, f) in e.columns.iter().enumerate() {
            p.push_str(&format!(
                "      const $a{i} = $confArg(values, {i}, {});\n      if (!$a{i}.ok) {{\n        return $a{i};\n      }}\n",
                reader(domain, &f.ty, f.nullable)
            ));
            parts.push(format!("{}: $a{i}.value", property(&f.wire)));
        }
        p.push_str(&format!(
            "      seed.{}.push({{ {} }});\n      return {{ ok: true, value: undefined }};\n    }}\n",
            property(&e.field),
            parts.join(", ")
        ));
    }
    p.push_str("    default:\n      return { ok: false, error: `no entity ${entity ?? \"\"} in this port` };\n  }\n}\n\n");

    // The abstraction.
    p.push_str(&doc(
        &[
            "The store as the traces spell a state: one line per row with the columns the model"
                .to_string(),
            "tracks, per pending run mark, and per raise a triggered use case reads.".to_string(),
        ],
        "",
    ));
    p.push_str("function $confAbstract(s: StoreSnapshot): readonly string[] {\n  const out: string[] = [];\n");
    for e in &port.entities {
        let mut line = format!("{} ${{row.{}}}", e.entity, property(&e.id.wire));
        for (field, how) in &e.tracked {
            line.push_str(&format!(" {}=${{{}}}", field.wire, abstracted(field, *how)));
        }
        p.push_str(&format!(
            "  for (const row of s.{}) {{\n    out.push(`{}`);\n  }}\n",
            property(&e.field),
            line
        ));
    }
    for r in &port.runs {
        p.push_str(&format!(
            "  for (const m of s.{}) {{\n    out.push(`mark {} {} ${{m.id}} ${{m.step}}`);\n  }}\n",
            property(&r.field),
            r.usecase,
            r.entity
        ));
    }
    for ev in &port.events {
        let tracked: Vec<_> = ev.ids.iter().filter(|(_, t)| *t).collect();
        if tracked.is_empty() {
            continue;
        }
        p.push_str(&format!(
            "  for (const ev of s.{}) {{\n",
            property(&ev.field)
        ));
        for (field, _) in tracked {
            p.push_str(&format!(
                "    out.push(`raised {} {} ${{ev.{}}}`);\n",
                ev.ty,
                field.entity,
                property(&field.name)
            ));
        }
        p.push_str("  }\n");
    }
    p.push_str("  return out;\n}\n\n");

    // Only the readers a call or a row uses, for `noUnusedLocals`.
    for (name, text) in CONF_READERS {
        if p.contains(name) {
            p.push_str(text);
        }
    }
    out.push_str(&p);
}

/// The readers of the traces' text that only some ports use.
const CONF_READERS: [(&str, &str); 4] = [
    (
        "$confText",
        r#"/** A reader of the traces' text through a parser of values. */
function $confText<T>(parse: (value: unknown) => Result<T, InvalidValue>): (text: string) => Result<T, string> {
  return (text) => {
    const r = parse(text);
    return r.ok ? r : { ok: false, error: `${text}: ${r.error.type}: ${r.error.reason}` };
  };
}

"#,
    ),
    (
        "$confNum",
        r#"/** A reader of an integer in the traces' text through a parser of values. */
function $confNum<T>(parse: (value: unknown) => Result<T, InvalidValue>): (text: string) => Result<T, string> {
  return (text) => {
    if (!/^-?[0-9]+$/.test(text)) {
      return { ok: false, error: `not an integer: ${text}` };
    }
    const r = parse(globalThis.Number(text));
    return r.ok ? r : { ok: false, error: `${text}: ${r.error.type}: ${r.error.reason}` };
  };
}

"#,
    ),
    (
        "$confBool",
        r#"function $confBool(text: string): Result<boolean, string> {
  return text === "true" ? { ok: true, value: true } : text === "false" ? { ok: true, value: false } : { ok: false, error: `not a bool: ${text}` };
}

"#,
    ),
    (
        "$confNullable",
        r#"/** A reader that takes `~` as null. */
function $confNullable<T>(read: (text: string) => Result<T, string>): (text: string) => Result<T | null, string> {
  return (text) => (text === "~" ? { ok: true, value: null } : read(text));
}

"#,
    ),
];

/// The reader of a value of `ty` from the traces' text: `(text: string) => Result<T, string>`.
fn reader(domain: &DomainModel, ty: &FieldType, nullable: bool) -> String {
    let read = match ty {
        FieldType::Scalar(Scalar::Int) => "$confNum($int)".to_string(),
        FieldType::Scalar(Scalar::Bool) => "$confBool".to_string(),
        FieldType::Scalar(Scalar::String) => "$confText($string)".to_string(),
        FieldType::Scalar(s) => format!("$confText({}.parse)", s.name()),
        FieldType::Enum(name) => format!("$confText({name}.parse)"),
        FieldType::Id(name) => {
            let repr = domain
                .ids
                .iter()
                .find(|i| i.name == *name)
                .map_or(Scalar::Int, |i| i.repr);
            if repr == Scalar::Int {
                format!("$confNum({name}.parse)")
            } else {
                format!("$confText({name}.parse)")
            }
        }
        FieldType::Rows(_) => unreachable!("a list of rows is no port argument"),
    };
    if nullable {
        format!("$confNullable({read})")
    } else {
        read
    }
}

/// The tracked column `field` of `row`, abstracted as the model holds it.
fn abstracted(field: &Field, how: Track) -> String {
    let place = format!("row.{}", property(&field.wire));
    match (how, field.nullable) {
        (Track::Presence, true) => format!("{place} === null ? \"~\" : \"+\""),
        (Track::Presence, false) => "\"+\"".to_string(),
        (Track::Exact, true) => format!("{place} === null ? \"~\" : {place}"),
        (Track::Exact, false) => place,
    }
}
