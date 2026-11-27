//! The TypeScript model adapter: `ModelState`, `Call`, `ModelAdapter` and `conform`. An
//! adapter's methods may return promises: a real store is asynchronous.

use super::super::conformance::{TRACES_ENV, TRACES_FORMAT};
use super::super::string_literal;
use super::super::ts::{doc, property};
use super::{AdColumn, Adapter, ArgKind, RowToken, Val, REPORT};

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
    print_enums(a, &mut out);
    print_state(a, &mut out);
    print_calls(a, &mut out);
    print_adapter(&mut out);
    print_conform(&mut out);
    print_seed_rows(a, &mut out);
    print_call_parser(a, &mut out);
    print_abstraction(a, &mut out);
    out.push('\n');
    for (name, text) in READERS {
        if out.contains(&format!("{name}(")) || out.contains(&format!("{name})")) {
            out.push_str(text);
        }
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn ty(a: &Adapter, val: Val) -> String {
    match val {
        Val::Int => "number".to_string(),
        Val::Bool => "boolean".to_string(),
        Val::Text => "string".to_string(),
        Val::Enum(i) => a.enums[i].name.clone(),
    }
}

fn column_ty(a: &Adapter, c: &AdColumn) -> String {
    let t = ty(a, c.val);
    if c.nullable {
        format!("{t} | null")
    } else {
        t
    }
}

/// The values of enum `i`, a constant of the file.
fn values(a: &Adapter, i: usize) -> String {
    format!("adValues{}", a.enums[i].name)
}

/// `obj.name`, or `obj["name"]` when the name is no identifier.
fn access(obj: &str, name: &str) -> String {
    let p = property(name);
    if p.starts_with('"') {
        format!("{obj}[{p}]")
    } else {
        format!("{obj}.{p}")
    }
}

/// The property of a call's argument: `kind` discriminates the calls, so an argument of
/// that name is `kind_`.
fn arg_property(name: &str) -> String {
    if name == "kind" {
        "kind_".to_string()
    } else {
        name.to_string()
    }
}

fn print_enums(a: &Adapter, out: &mut String) {
    for (i, e) in a.enums.iter().enumerate() {
        out.push_str(&doc(
            &[format!("A value of {}, as the model spells it.", e.name)],
            "",
        ));
        out.push_str(&format!(
            "export type {} = {};\n",
            e.name,
            e.values
                .iter()
                .map(|(w, _)| string_literal(w))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        out.push_str(&format!(
            "const {}: readonly {}[] = [{}];\n\n",
            values(a, i),
            e.name,
            e.values
                .iter()
                .map(|(w, _)| string_literal(w))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn print_state(a: &Adapter, out: &mut String) {
    for e in &a.entities {
        out.push_str(&doc(
            &[format!(
                "A {} row as the model sees it: its id and the columns the model tracks. The adapter picks the other columns' values when it seeds.",
                e.entity
            )],
            "",
        ));
        out.push_str(&format!("export type {} = Readonly<{{\n", e.name));
        out.push_str(&doc(&[format!("The primary key `{}`.", e.id.wire)], "  "));
        out.push_str(&format!(
            "  {}: {};\n",
            property(&e.id.wire),
            column_ty(a, &e.id)
        ));
        for c in &e.tracked {
            out.push_str(&doc(
                &[if c.presence {
                    format!("Whether `{}` is set: the model tracks only that.", c.wire)
                } else if c.nullable {
                    format!("`{}`; null for null.", c.wire)
                } else {
                    format!("`{}`.", c.wire)
                }],
                "  ",
            ));
            out.push_str(&format!("  {}: {};\n", property(&c.wire), column_ty(a, c)));
        }
        out.push_str("}>;\n\n");
    }
    for r in &a.runs {
        out.push_str(&doc(
            &[format!(
                "A pending run mark of {} on a {} row: the last step of the run that wrote the row.",
                r.usecase, r.entity
            )],
            "",
        ));
        out.push_str(&format!(
            "export type {} = Readonly<{{ id: {}; step: number }}>;\n\n",
            r.name,
            ty(a, r.id)
        ));
    }
    for ev in &a.events {
        out.push_str(&doc(
            &[format!(
                "The payload of a {} event: the ids of the rows it was raised on.",
                ev.ty
            )],
            "",
        ));
        let fields: Vec<String> = ev
            .ids
            .iter()
            .map(|(name, _, val, _)| format!("{}: {}", property(name), ty(a, *val)))
            .collect();
        out.push_str(&format!(
            "export type {} = Readonly<{{ {} }}>;\n\n",
            ev.name,
            fields.join("; ")
        ));
        if ev.field.is_some() {
            out.push_str(&doc(
                &[format!(
                    "A raise of {} the model remembers: the ids of the rows a triggered use case reads.",
                    ev.ty
                )],
                "",
            ));
            let fields: Vec<String> = ev
                .ids
                .iter()
                .filter(|(_, _, _, tracked)| *tracked)
                .map(|(name, _, val, _)| format!("{}: {}", property(name), ty(a, *val)))
                .collect();
            out.push_str(&format!(
                "export type {} = Readonly<{{ {} }}>;\n\n",
                ev.raised,
                fields.join("; ")
            ));
        }
    }
    let mut fields = Vec::new();
    for e in &a.entities {
        fields.push((
            e.field.clone(),
            e.name.clone(),
            format!("The {} rows.", e.entity),
        ));
    }
    for r in &a.runs {
        fields.push((
            r.field.clone(),
            r.name.clone(),
            format!(
                "The pending run marks of {} on {} rows.",
                r.usecase, r.entity
            ),
        ));
    }
    for ev in &a.events {
        if let Some(field) = &ev.field {
            fields.push((
                field.clone(),
                ev.raised.clone(),
                format!("The {} events raised.", ev.ty),
            ));
        }
    }
    out.push_str(&doc(
        &["Everything a store holds, as the model sees it: the rows of each entity, the pending run marks, and the raises a triggered use case reads. The order of a list does not matter.".to_string()],
        "",
    ));
    out.push_str("export type ModelState = Readonly<{\n");
    for (name, t, what) in &fields {
        out.push_str(&doc(std::slice::from_ref(what), "  "));
        out.push_str(&format!("  {}: readonly {t}[];\n", property(name)));
    }
    out.push_str("}>;\n\n");
    // The state the harness builds a row at a time.
    out.push_str("type AdSeed = {\n");
    for (name, t, _) in &fields {
        out.push_str(&format!("  {}: {t}[];\n", property(name)));
    }
    out.push_str("};\n\n");
    let empty: Vec<String> = fields
        .iter()
        .map(|(name, _, _)| format!("{}: []", property(name)))
        .collect();
    out.push_str(&format!(
        "function adEmpty(): AdSeed {{\n  return {{ {} }};\n}}\n\n",
        empty.join(", ")
    ));
}

fn print_calls(a: &Adapter, out: &mut String) {
    out.push_str(&doc(
        &["One call of a use case, by its action in `kind`: the ids of its rows and run marks, its event, its inputs and its choices, as the traces give them.".to_string()],
        "",
    ));
    if a.calls.is_empty() {
        out.push_str("export type Call = never;\n\n");
    } else {
        out.push_str("export type Call =\n");
        for (k, c) in a.calls.iter().enumerate() {
            let mut fields = vec![format!("kind: {}", string_literal(&c.action))];
            for arg in &c.args {
                let t = match &arg.kind {
                    ArgKind::Value { val, nullable } => {
                        let t = ty(a, *val);
                        if *nullable {
                            format!("{t} | null")
                        } else {
                            t
                        }
                    }
                    ArgKind::Event(i) => a.events[*i].name.clone(),
                };
                fields.push(format!("{}: {t}", property(&arg_property(&arg.name))));
            }
            let end = if k + 1 == a.calls.len() { ";" } else { "" };
            out.push_str(&format!(
                "  | Readonly<{{ {} }}>{end} // the action {} of the use case {}\n",
                fields.join("; "),
                c.action,
                c.usecase
            ));
        }
        out.push('\n');
    }
}

fn print_adapter(out: &mut String) {
    out.push_str(&format!(
        r#"/** Why a use case did not run. The store is unchanged. */
export type Refusal =
  | "notEnabled" // the call does not run from its rows' states, names a row that does not exist, or a commit's rows changed since its prepare
  | "rejected" // a condition the model applies to the use case rejects the call
  | "invalidArgument"; // an argument breaks what the verified model keeps

/** What a call did: it ran, or it was refused, with the store unchanged. */
export type Outcome = Readonly<{{ kind: "ran" }}> | Readonly<{{ kind: "refused"; refusal: Refusal }}>;

/** The outcome of a call that ran. */
export const ran: Outcome = {{ kind: "ran" }};

/** Each refusal as the traces spell its kind. */
const adKinds: Readonly<Record<Refusal, string>> = {{ notEnabled: "NotEnabled", rejected: "Rejected", invalidArgument: "InvalidArgument" }};

/** The outcome of a call refused for `refusal`. */
export function refused(refusal: Refusal): Outcome {{
  return {{ kind: "refused", refusal }};
}}

/**
 * What a service offers the conformance harness: a store it can empty, seed and read back as
 * the model sees it, and its use cases by call. It uses no type of the generated domain code,
 * so the service may be written, renamed and restructured freely. Each method may return a
 * promise.
 */
export type ModelAdapter = {{
  /** Empties the store: a trace starts here. */
  reset(): void | Promise<void>;
  /** Stores these rows in the empty store, with a value of the adapter's choosing for each column the model does not track. */
  seed(state: ModelState): void | Promise<void>;
  /** What the store holds now, as the model sees it. */
  observe(): ModelState | Promise<ModelState>;
  /** Makes the call in one transaction: it ran, or why it did not, with the store unchanged. */
  invoke(call: Call): Outcome | Promise<Outcome>;
  /**
   * Optional, for the interleaved rows of the traces: reads the call's rows and holds its
   * write for `commit`, writing nothing. Without both, the harness runs those rows as plain
   * calls and counts them as skipped, so conditional writes go unchecked.
   */
  prepare?(call: Call): Outcome | Promise<Outcome>;
  /** Stores the write `prepare` holds in one transaction, only where its rows are still what the prepare read. */
  commit?(): Outcome | Promise<Outcome>;
}};

/** What `conform` checked; `skipped` counts the interleaved rows run as plain calls. */
export type AdapterReport = Readonly<{{
  traces: number;
  calls: number;
  refusals: number;
  interleavings: number;
  skipped: number;
  /** The calls and refused calls made again by `prepare` and then `commit`. */
  split: number;
}}>;

/** The line a conformance test prints on a pass, which `check --code=run` reads. */
export function reportLine(r: AdapterReport): string {{
  return {report} + `${{r.traces}} traces, ${{r.calls}} calls, ${{r.refusals}} refusals, ${{r.interleavings}} interleavings, ${{r.skipped}} interleavings skipped, ${{r.split}} split calls`;
}}

/** A value, or what is wrong. */
type AdResult<T> = Readonly<{{ ok: true; value: T }}> | Readonly<{{ ok: false; error: string }}>;

"#,
        report = string_literal(REPORT),
    ));
}

fn print_conform(out: &mut String) {
    out.push_str(&format!(
        r##"/**
 * Drives `adapter` along `traces` (the file `codegen --conformance` writes): resets and seeds
 * each trace's initial state, makes each call, and after each compares what the store holds
 * with the state the model reaches. A call the model refuses must be refused, with the kind the
 * traces name, and change nothing, and so must a commit whose rows another call changed after
 * its prepare. An adapter with `prepare` and `commit` runs the traces again with each call made
 * by them. Returns the first difference.
 */
export async function conform(adapter: ModelAdapter, traces: string): Promise<AdResult<AdapterReport>> {{
  const report = await adPass(adapter, traces, false);
  if (!report.ok || adapter.prepare === undefined || adapter.commit === undefined) {{
    return report;
  }}
  const again = await adPass(adapter, traces, true);
  if (!again.ok) {{
    return again;
  }}
  return {{ ok: true, value: {{ ...report.value, split: again.value.calls + again.value.refusals }} }};
}}

/** Makes `call`: by `invoke`, or, with `split`, by the adapter's `prepare` and then its `commit`. */
async function adRun(adapter: ModelAdapter, call: Call, split: boolean): Promise<Outcome> {{
  const {{ prepare, commit }} = adapter;
  if (!split || prepare === undefined || commit === undefined) {{
    return adapter.invoke(call);
  }}
  const o = await prepare.call(adapter, call);
  return o.kind === "ran" ? commit.call(adapter) : o;
}}

/** One run of `traces`, with each call made by `adRun`. */
async function adPass(adapter: ModelAdapter, traces: string, viaSplit: boolean): Promise<AdResult<AdapterReport>> {{
  const how = viaSplit ? " through prepare and commit" : "";
  const lines = traces.split("\n");
  if (lines[0]?.trimEnd() !== {format}) {{
    return {{ ok: false, error: "not a traces file of this version: generate the adapter and the traces again" }};
  }}
  const states = new Map<string, string[]>();
  let defining: string[] | null = null;
  let report: AdapterReport = {{ traces: 0, calls: 0, refusals: 0, interleavings: 0, skipped: 0, split: 0 }};
  let inTrace = false;
  let seed = adEmpty();
  let want: readonly string[] = [];
  const observe = async (): Promise<readonly string[]> => adAbstract(await adapter.observe());
  for (let n = 1; n < lines.length; n++) {{
    const toks = (lines[n] ?? "").split(/\s+/).filter((t) => t !== "");
    const [kind, ...rest] = toks;
    const at = (what: string): AdResult<never> => ({{ ok: false, error: `traces line ${{n + 1}}: ${{what}}` }});
    if (kind === undefined || kind === "#") {{
      continue;
    }}
    const state = (id: string | undefined): readonly string[] | undefined => (id === undefined ? undefined : states.get(id));
    if (!inTrace && (kind === "G" || kind === "C" || kind === "X" || kind === "I")) {{
      return at("a call outside a trace");
    }}
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
        await adapter.reset();
        inTrace = true;
        seed = adEmpty();
        report = {{ ...report, traces: report.traces + 1 }};
        break;
      }}
      case "R": {{
        const row = adSeedRow(seed, rest);
        if (!row.ok) {{
          return at(row.error);
        }}
        break;
      }}
      case "G": {{
        await adapter.seed(seed);
        seed = adEmpty();
        const diff = adCompare(await observe(), want, "after seeding the initial state");
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
        const text = rest.slice(1);
        const call = adCall(text);
        if (!call.ok) {{
          return at(call.error);
        }}
        const o = await adRun(adapter, call.value, viaSplit);
        if (o.kind === "refused") {{
          return at(`the model runs \`${{text.join(" ")}}\` here, and the service refused it${{how}}: ${{o.refusal}}`);
        }}
        const diff = adCompare(await observe(), next, `after \`${{text.join(" ")}}\``);
        if (diff !== null) {{
          return at(diff);
        }}
        want = next;
        report = {{ ...report, calls: report.calls + 1 }};
        break;
      }}
      case "X": {{
        const [kind, ...words] = rest;
        const call = adCall(words);
        if (!call.ok || kind === undefined) {{
          return at(call.ok ? "a refused call without its kind" : call.error);
        }}
        const o = await adRun(adapter, call.value, viaSplit);
        if (o.kind === "ran") {{
          return at(`the model refuses \`${{words.join(" ")}}\` here, and the service ran it${{how}}`);
        }}
        if (kind !== "-" && adKinds[o.refusal] !== kind) {{
          return at(`the model refuses \`${{words.join(" ")}}\` here with ${{kind}}, and the service refused it with ${{o.refusal}}`);
        }}
        const diff = adCompare(await observe(), want, `after the refused \`${{words.join(" ")}}\``);
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
        const bar = rest.indexOf("|");
        if (bar < 1) {{
          return at("an interleaving without `|`");
        }}
        const first = rest.slice(1, bar);
        const second = rest.slice(bar + 1);
        const early = adCall(first);
        if (!early.ok) {{
          return at(early.error);
        }}
        const late = adCall(second);
        if (!late.ok) {{
          return at(late.error);
        }}
        const {{ prepare, commit }} = adapter;
        const split = prepare !== undefined && commit !== undefined;
        if (split) {{
          const o = await prepare.call(adapter, early.value);
          if (o.kind === "refused") {{
            return at(`the model runs \`${{first.join(" ")}}\` here, and the service refused it: ${{o.refusal}}`);
          }}
        }}
        const o = await adapter.invoke(late.value);
        if (o.kind === "refused") {{
          return at(`the model runs \`${{second.join(" ")}}\` here, and the service refused it: ${{o.refusal}}`);
        }}
        const between = adCompare(await observe(), next, `after \`${{second.join(" ")}}\``);
        if (between !== null) {{
          return at(between);
        }}
        if (split) {{
          if ((await commit.call(adapter)).kind === "ran") {{
            return at(`the service committed \`${{first.join(" ")}}\`, prepared before \`${{second.join(" ")}}\` changed a row it writes: the write must be conditional on what the row was`);
          }}
          const diff = adCompare(await observe(), next, `after the refused commit of \`${{first.join(" ")}}\``);
          if (diff !== null) {{
            return at(diff);
          }}
          report = {{ ...report, interleavings: report.interleavings + 1 }};
        }} else {{
          report = {{ ...report, skipped: report.skipped + 1 }};
        }}
        want = next;
        break;
      }}
      default:
        return at(`unknown record ${{kind}}`);
    }}
  }}
  return {{ ok: true, value: report }};
}}

/** The store against the model's state, both as sorted sets of lines: what one holds and the other does not, or null. */
function adCompare(got: readonly string[], want: readonly string[], when: string): string | null {{
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

/** The value at `values[i]`, read by `read`. */
function adArg<T>(values: readonly string[], i: number, read: (text: string) => AdResult<T>): AdResult<T> {{
  const text = values[i];
  return text === undefined ? {{ ok: false, error: "too few values" }} : read(text);
}}

"##,
        format = string_literal(TRACES_FORMAT),
    ));
}

/// The readers of the traces' text that only some adapters use.
const READERS: [(&str, &str); 6] = [
    (
        "adInt",
        "function adInt(text: string): AdResult<number> {\n  return /^-?[0-9]+$/.test(text) ? { ok: true, value: globalThis.Number(text) } : { ok: false, error: `not an integer: ${text}` };\n}\n\n",
    ),
    (
        "adBool",
        "function adBool(text: string): AdResult<boolean> {\n  return text === \"true\" ? { ok: true, value: true } : text === \"false\" ? { ok: true, value: false } : { ok: false, error: `not a bool: ${text}` };\n}\n\n",
    ),
    (
        "adText",
        "function adText(text: string): AdResult<string> {\n  return { ok: true, value: text };\n}\n\n",
    ),
    (
        "adSet",
        "/** Whether a column is set: `~` is null. */\nfunction adSet(text: string): AdResult<boolean> {\n  return { ok: true, value: text !== \"~\" };\n}\n\n",
    ),
    (
        "adEnum",
        "/** A reader of a value of `values`. */\nfunction adEnum<T extends string>(values: readonly T[]): (text: string) => AdResult<T> {\n  return (text) => {\n    const v = values.find((x) => x === text);\n    return v === undefined ? { ok: false, error: `not a value of the model: ${text}` } : { ok: true, value: v };\n  };\n}\n\n",
    ),
    (
        "adNullable",
        "/** A reader that takes `~` as null. */\nfunction adNullable<T>(read: (text: string) => AdResult<T>): (text: string) => AdResult<T | null> {\n  return (text) => (text === \"~\" ? { ok: true, value: null } : read(text));\n}\n\n",
    ),
];

/// The reader of a value of `val`: `(text: string) => AdResult<T>`.
fn reader(ad: &Adapter, val: Val, nullable: bool, presence: bool) -> String {
    let read = match val {
        _ if presence => "adSet".to_string(),
        Val::Int => "adInt".to_string(),
        Val::Bool => "adBool".to_string(),
        Val::Text => "adText".to_string(),
        Val::Enum(i) => format!("adEnum({})", values(ad, i)),
    };
    if nullable {
        format!("adNullable({read})")
    } else {
        read
    }
}

/// A read of `values[i]` into `$a{i}`, returning its error; the value's expression.
fn arg(out: &mut String, i: usize, reader: String) -> String {
    out.push_str(&format!(
        "      const $a{i} = adArg(values, {i}, {reader});\n      if (!$a{i}.ok) {{\n        return $a{i};\n      }}\n"
    ));
    format!("$a{i}.value")
}

fn print_seed_rows(ad: &Adapter, out: &mut String) {
    out.push_str("/** A row `R` of the traces into the state to seed, its untracked columns skipped. */\nfunction adSeedRow(seed: AdSeed, toks: readonly string[]): AdResult<void> {\n  const [entity, ...values] = toks;\n  switch (entity) {\n");
    for e in &ad.entities {
        out.push_str(&format!("    case {}: {{\n", string_literal(&e.entity)));
        let mut fields = Vec::new();
        for (i, token) in e.tokens.iter().enumerate() {
            match token {
                RowToken::Id => {
                    let v = arg(out, i, reader(ad, e.id.val, false, false));
                    fields.push(format!("{}: {v}", property(&e.id.wire)));
                }
                RowToken::Tracked(k) => {
                    let c = &e.tracked[*k];
                    let v = arg(out, i, reader(ad, c.val, c.nullable, c.presence));
                    fields.push(format!("{}: {v}", property(&c.wire)));
                }
                RowToken::Skip => {}
            }
        }
        out.push_str(&format!(
            "      {}.push({{ {} }});\n      return {{ ok: true, value: undefined }};\n    }}\n",
            access("seed", &e.field),
            fields.join(", ")
        ));
    }
    out.push_str("    default:\n      return { ok: false, error: `no entity ${entity ?? \"\"} in this adapter` };\n  }\n}\n\n");
}

fn print_call_parser(ad: &Adapter, out: &mut String) {
    // A model whose every action is an entry point has no call: the harness names none.
    if ad.calls.is_empty() {
        out.push_str("/** A call of the traces as a `Call`: this model has none. */\nfunction adCall(call: readonly string[]): AdResult<Call> {\n  const [action] = call;\n  return { ok: false, error: `no use case ${action ?? \"\"} in this adapter` };\n}\n\n");
        return;
    }
    out.push_str("/** A call of the traces as a `Call`. */\nfunction adCall(call: readonly string[]): AdResult<Call> {\n  const [action, ...values] = call;\n  switch (action) {\n");
    for c in &ad.calls {
        out.push_str(&format!("    case {}: {{\n", string_literal(&c.action)));
        let mut fields = vec![format!("kind: {}", string_literal(&c.action))];
        let mut i = 0usize;
        for a in &c.args {
            let value = match &a.kind {
                ArgKind::Value { val, nullable } => {
                    i += 1;
                    arg(out, i - 1, reader(ad, *val, *nullable, false))
                }
                ArgKind::Event(e) => {
                    let mut ids = Vec::new();
                    for (name, _, val, _) in &ad.events[*e].ids {
                        i += 1;
                        let v = arg(out, i - 1, reader(ad, *val, false, false));
                        ids.push(format!("{}: {v}", property(name)));
                    }
                    format!("{{ {} }}", ids.join(", "))
                }
            };
            fields.push(format!("{}: {value}", property(&arg_property(&a.name))));
        }
        out.push_str(&format!(
            "      return {{ ok: true, value: {{ {} }} }};\n    }}\n",
            fields.join(", ")
        ));
    }
    out.push_str("    default:\n      return { ok: false, error: `no use case ${action ?? \"\"} in this adapter` };\n  }\n}\n\n");
}

/// A tracked column of `row`, as the traces spell it.
fn abstracted(c: &AdColumn) -> String {
    let place = access("row", &c.wire);
    if c.presence {
        format!("({place} ? \"+\" : \"~\")")
    } else if c.nullable {
        format!("({place} === null ? \"~\" : String({place}))")
    } else {
        format!("String({place})")
    }
}

fn print_abstraction(ad: &Adapter, out: &mut String) {
    out.push_str("/** The state as the traces spell it: one line per row with the columns the model tracks, per pending run mark, and per raise a triggered use case reads; sorted. */\nfunction adAbstract(s: ModelState): string[] {\n  const out: string[] = [];\n");
    for e in &ad.entities {
        out.push_str(&format!(
            "  for (const row of {}) {{\n    let line = {} + String({});\n",
            access("s", &e.field),
            string_literal(&format!("{} ", e.entity)),
            access("row", &e.id.wire)
        ));
        for c in &e.tracked {
            out.push_str(&format!(
                "    line += {} + {};\n",
                string_literal(&format!(" {}=", c.wire)),
                abstracted(c)
            ));
        }
        out.push_str("    out.push(line);\n  }\n");
    }
    for r in &ad.runs {
        out.push_str(&format!(
            "  for (const m of {}) {{\n    out.push({} + `${{m.id}} ${{m.step}}`);\n  }}\n",
            access("s", &r.field),
            string_literal(&format!("mark {} {} ", r.usecase, r.entity))
        ));
    }
    for ev in &ad.events {
        let Some(field) = &ev.field else {
            continue;
        };
        out.push_str(&format!("  for (const ev of {}) {{\n", access("s", field)));
        for (name, entity, _, tracked) in &ev.ids {
            if *tracked {
                out.push_str(&format!(
                    "    out.push({} + String({}));\n",
                    string_literal(&format!("raised {} {entity} ", ev.ty)),
                    access("ev", name)
                ));
            }
        }
        out.push_str("  }\n");
    }
    out.push_str("  out.sort();\n  return out.filter((l, i) => i === 0 || l !== out[i - 1]);\n}\n");
}

/// The adapter probe: a script that drives the adapter `new` makes along the traces and
/// prints what it ran.
pub(super) fn probe(
    binding: &super::super::ProbeBinding,
    new: &str,
    traces: &str,
) -> Option<super::super::probe::Probe> {
    let (specifier, ty) = binding.service.rsplit_once('#')?;
    let mut w = super::super::probe::ProbeWriter::new(
        "// Generated by rdra-ish codegen (experimental): the adapter probe of `check --code`.",
    );
    w.line("// Never edited by hand: `rdra-ish codegen --code` writes it again.");
    w.line(&format!(
        "import * as model from {};",
        string_literal(&binding.module)
    ));
    w.line(&format!(
        "import * as service from {};",
        string_literal(specifier)
    ));
    w.line("");
    w.line(&format!("type Adapter = service.{ty};"));
    w.line("");
    w.line("// The adapter against the model's traces, by the generated harness. What it needs of");
    w.line("// Node is declared here, so the probe builds with or without @types/node.");
    w.line("declare const process: {");
    w.line("  exitCode: number | undefined;");
    w.line("  env: { readonly [name: string]: string | undefined };");
    w.line(
        "  getBuiltinModule: (id: \"node:fs\") => { readFileSync: (path: string, encoding: \"utf8\") => string };",
    );
    w.line("};");
    w.line(
        "declare const console: { log: (line: string) => void; error: (line: string) => void };",
    );
    w.line(&format!(
        "const traces = process.getBuiltinModule(\"node:fs\").readFileSync(process.env[{}] || {}, \"utf8\");",
        string_literal(TRACES_ENV),
        string_literal(traces)
    ));
    w.line(&format!(
        "const adapter: model.ModelAdapter = ((): Adapter => {new})();"
    ));
    w.line("model.conform(adapter, traces).then(");
    w.line("  (r) => {");
    w.line("    if (r.ok) {");
    w.line("      console.log(model.reportLine(r.value));");
    w.line("    } else {");
    w.line("      console.error(`rdra-ish conformance: ${r.error}`);");
    w.line("      process.exitCode = 1;");
    w.line("    }");
    w.line("  },");
    w.line("  (e: unknown) => {");
    w.line("    console.error(`rdra-ish conformance: the adapter threw: ${String(e)}`);");
    w.line("    process.exitCode = 1;");
    w.line("  },");
    w.line(");");
    Some(w.finish())
}
