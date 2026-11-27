//! The TypeScript contracts, conformance port and harness: one `<UseCase>UseCase` type per use
//! case (function-property notation, so a narrower parameter does not pass), `StoreSnapshot`,
//! `ConformancePort` as their composition, and `conform`, which drives a port along the traces
//! `conformance::traces` writes. The contracts are async (`Promise<Result<..>>`): a TypeScript
//! service reads its store asynchronously; the Rust and Go ones are sync.

use super::conformance::{TRACES_ENV, TRACES_FORMAT};
use super::ir::{DomainModel, Field, FieldType, PortArg, PortDecl, PortMethod, Scalar, Track};
use super::probe::{Probe, ProbeWriter};
use super::string_literal;
use super::ts::{column_type, doc, doc1, local, property};

/// `name: type` per argument of a method's `prepare`.
fn params(m: &PortMethod) -> Vec<(String, String)> {
    m.args()
        .map(|a| match a {
            PortArg::Id(a) | PortArg::Event(a) | PortArg::Choice(a) => {
                (local(&a.name), a.ty.clone())
            }
            PortArg::Input(i) => (local(&i.name), column_type(&i.ty, i.nullable)),
        })
        .collect()
}

/// The contract types, one per use case with printed functions.
fn print_contracts(domain: &DomainModel, port: &PortDecl, out: &mut String) {
    if port.contracts.is_empty() {
        return;
    }
    print_capabilities(domain, port, out);
    out.push_str("// ── Contracts: one per use case, which the service implements ──\n\n");
    for c in &port.contracts {
        let mut lines = vec![format!(
            "What a service offers for the use case {} \"{}\"{}: per action, `prepare*` loads its rows",
            c.usecase,
            c.label,
            c.site
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default()
        )];
        lines.push("by id and returns what the core returns, writing nothing; `commit*` stores that result in".to_string());
        lines.push("one transaction, each row only where it still is what the result expects. Either returns".to_string());
        lines.push("an error and changes nothing when the use case does not run. Async: a TypeScript store is.".to_string());
        out.push_str(&doc(&lines, ""));
        // `readonly` members rather than `Readonly<{..}>`: tsc then names the contract in its
        // messages ("missing ... from type 'ShipUseCase'"), which `check --code` reads.
        out.push_str(&format!("export type {} = {{\n", c.name));
        for m in c.methods.iter().map(|&i| &port.methods[i]) {
            out.push_str(&doc1_indent(&format!(
                "Runs {} (the action {}).",
                m.usecase, m.action
            )));
            let mut ps: Vec<String> = params(m)
                .iter()
                .map(|(n, ty)| format!("{n}: {ty}"))
                .collect();
            if let Some(caller) = &c.caller {
                ps.insert(0, format!("by: {}", caller.name));
            }
            let [prepare, commit] = method_names(m);
            out.push_str(&format!(
                "  readonly {prepare}: ({}) => Promise<Result<{}, {}>>;\n",
                ps.join(", "),
                m.write,
                c.error
            ));
            out.push_str(&format!(
                "  readonly {commit}: (write: {}) => Promise<Result<void, {}>>;\n",
                m.write, c.error
            ));
        }
        for e in &c.entries {
            out.push_str(&doc1_indent(&format!(
                "Runs the action {} of {} whole, by the ids of its rows: the generator printed no function for it (#335), so the service writes it and keeps the obligations.",
                e.action, c.usecase
            )));
            let mut ps: Vec<String> = e
                .rows
                .iter()
                .map(|(n, ty)| format!("{}: {ty}", local(n)))
                .collect();
            if let Some(caller) = &c.caller {
                ps.insert(0, format!("by: {}", caller.name));
            }
            out.push_str(&format!(
                "  readonly {}: ({}) => Promise<Result<void, {}>>;\n",
                entry_name(&e.name),
                ps.join(", "),
                c.error
            ));
        }
        out.push_str("};\n\n");
    }
}

/// The capability of each actor and the caller type of each use case (#337): a prepare or an
/// entry point takes the caller first, so code with no capability of an actor that performs
/// the use case cannot run it. A capability carries a module-private `unique symbol`, so an
/// object literal cannot stand in for one; its companion's `issue` is the only way to make one
/// without a type assertion.
fn print_capabilities(domain: &DomainModel, port: &PortDecl, out: &mut String) {
    if domain.capabilities.is_empty() {
        return;
    }
    out.push_str("// ── Capabilities: who may run each use case ──\n\n");
    out.push_str(&doc1(
        "The brand of the capabilities: the module does not export it, so code outside cannot write one.",
    ));
    out.push_str("const $capability: unique symbol = Symbol(\"rdra-ish capability\");\n\n");
    for cap in &domain.capabilities {
        out.push_str(&doc(
            &[
                format!(
                    "The capability of the actor {} ({}): the use cases it performs take it.",
                    cap.actor, cap.label
                ),
                format!(
                    "Made only by `{}.issue()`, which the code calls once it has authenticated a {}; rdra-ish",
                    cap.name, cap.actor
                ),
                "does not check who calls it, and a type assertion (`as`) still forges one.".to_string(),
            ],
            "",
        ));
        let kind = string_literal(&super::type_ident(&cap.actor));
        out.push_str(&format!(
            "export type {} = Readonly<{{ kind: {kind}; [$capability]: true }}>;\n\n",
            cap.name
        ));
        out.push_str(&doc1(&format!(
            "Makes the capability of {}, for code that has authenticated one.",
            cap.actor
        )));
        out.push_str(&format!(
            "export const {0} = {{\n  issue: (): {0} => ({{ kind: {kind}, [$capability]: true }}),\n}} as const;\n\n",
            cap.name
        ));
    }
    for c in &port.contracts {
        let Some(caller) = &c.caller else {
            continue;
        };
        out.push_str(&doc1(&format!(
            "Who may run {}: an actor that performs it, by its capability.",
            c.usecase
        )));
        let caps: Vec<&str> = caller
            .actors
            .iter()
            .map(|&i| domain.capabilities[i].name.as_str())
            .collect();
        out.push_str(&format!(
            "export type {} = {};\n\n",
            caller.name,
            caps.join(" | ")
        ));
    }
}

/// The member of an entry point (#335): `run<Action>`.
pub(super) fn entry_name(name: &str) -> String {
    format!("run{name}")
}

/// A port method's two members: `prepare<Action>` and `commit<Action>`.
pub(super) fn method_names(m: &PortMethod) -> [String; 2] {
    [format!("prepare{}", m.name), format!("commit{}", m.name)]
}

/// The probe of `check --code` (docs/design/code-check.md): one line per contract, a function
/// that takes the service and returns it as the contract, so tsc reports a missing or
/// mistyped member on that line; and with `binding.conformance`, the conformance test, which
/// Node runs as it is (type stripping). In TypeScript `binding.service` is
/// `<import specifier>#<exported type>` and `binding.module` the domain file's specifier, both
/// relative to the probe; the `new` expression sees the service module as `service`.
pub(super) fn probe(domain: &DomainModel, binding: &super::ProbeBinding) -> Option<Probe> {
    let port = domain.port_with_contracts()?;
    let (specifier, ty) = binding.service.rsplit_once('#')?;
    let test = binding
        .conformance
        .as_ref()
        .filter(|_| domain.harness().is_some());
    // Type-only imports unless the test runs: the probe alone loads nothing.
    let import = if test.is_some() {
        "import"
    } else {
        "import type"
    };
    let mut w = ProbeWriter::new(
        "// Generated by rdra-ish codegen (experimental): the contract probe of `check --code`.",
    );
    w.line("// Never edited by hand: `rdra-ish codegen --code` writes it again. A type error on a");
    w.line("// use case's line is that use case not implemented by the service.");
    w.line(&format!(
        "{import} * as domain from {};",
        string_literal(&binding.module)
    ));
    w.line(&format!(
        "{import} * as service from {};",
        string_literal(specifier)
    ));
    w.line("");
    w.line(&format!("type Service = service.{ty};"));
    w.line("");
    for c in &port.contracts {
        w.usecase(
            &format!(
                "export const _{} = (it: Service): domain.{} => it; // usecase {}",
                c.usecase, c.name, c.usecase
            ),
            c,
        );
    }
    if let Some((new, traces)) = test {
        w.line("");
        w.line(
            "// The service against the model's traces, by the generated harness. What it needs of",
        );
        w.line("// Node is declared here, so the probe builds with or without @types/node.");
        for line in probe_traces(traces) {
            w.line(&line);
        }
        w.line(&format!(
            "domain.conform((): Service => {new}, traces).then("
        ));
        w.line("  (r) => {");
        w.line("    if (r.ok) {");
        w.line("      const { traces: t, calls, refusals, interleavings } = r.value;");
        w.line("      console.log(`rdra-ish conformance: ok ${t} traces, ${calls} calls, ${refusals} refusals, ${interleavings} interleavings`);");
        w.line("      console.log(domain.unimplementedLine(r.value));");
        w.line("    } else {");
        w.line("      console.error(`rdra-ish conformance: ${r.error}`);");
        w.line("      process.exitCode = 1;");
        w.line("    }");
        w.line("  },");
        w.line("  (e: unknown) => {");
        w.line("    console.error(`rdra-ish conformance: the service threw: ${String(e)}`);");
        w.line("    process.exitCode = 1;");
        w.line("  },");
        w.line(");");
    }
    Some(w.finish())
}

/// `Unimplemented` and `UnimplementedConformancePort` (docs/design/algebraic-codegen.md, "The
/// unimplemented port"): a class with every contract's member, and `seed` and `snapshot` with
/// the harness, each rejecting with `Unimplemented`, for a service class to extend while it is
/// unfinished. Members are properties, as a service's are, so a subclass overrides one with
/// its own arrow function.
fn print_unimplemented(port: &PortDecl, out: &mut String) {
    if port.contracts.is_empty() {
        return;
    }
    let ty = super::conformance::UNIMPLEMENTED_PORT;
    out.push_str(
        "// ── The unimplemented port: extend it to build before every member exists ──\n\n",
    );
    out.push_str(&doc(
        &[
            format!(
                "What the members of `{ty}` reject with: the service has not implemented the call"
            ),
            "yet. `conform` stops a trace at such a call and reports the calls by use case;"
                .to_string(),
            "`rdra-ish check --code=run` lists them, and `--deny-incomplete` fails on any."
                .to_string(),
        ],
        "",
    ));
    out.push_str("export class Unimplemented extends globalThis.Error {\n  constructor(member: string) {\n    super(`rdra-ish: not implemented: ${member}`);\n    this.name = \"Unimplemented\";\n  }\n}\n\n");
    out.push_str(&doc(
        &[
            format!(
                "The `prepare*` and `commit*` members of every contract{}, each rejecting with `Unimplemented`.",
                if port.harness { ", `seed`, and `snapshot`" } else { "" }
            ),
            "Extend it to build before every member exists: a member the service declares takes".to_string(),
            "the place of this one, and `check --code=run` reports each call that still reaches it.".to_string(),
            "An entry point's `run*` member is not here: the harness drives none, so the service".to_string(),
            "writes each.".to_string(),
        ],
        "",
    ));
    out.push_str(&format!("export class {ty} {{\n"));
    let member = |name: &str, ps: &[String], ret: &str| -> String {
        let ps: Vec<String> = ps.iter().map(|p| format!("_{p}")).collect();
        format!(
            "  {name} = ({}): Promise<{ret}> => Promise.reject(new Unimplemented({}));\n",
            ps.join(", "),
            string_literal(name)
        )
    };
    for c in &port.contracts {
        let by: Vec<String> = c
            .caller
            .iter()
            .map(|caller| format!("by: {}", caller.name))
            .collect();
        for m in c.methods.iter().map(|&i| &port.methods[i]) {
            let [prepare, commit] = method_names(m);
            let ps: Vec<String> = by
                .iter()
                .cloned()
                .chain(params(m).iter().map(|(n, ty)| format!("{n}: {ty}")))
                .collect();
            out.push_str(&member(
                &prepare,
                &ps,
                &format!("Result<{}, {}>", m.write, c.error),
            ));
            out.push_str(&member(
                &commit,
                &[format!("write: {}", m.write)],
                &format!("Result<void, {}>", c.error),
            ));
        }
    }
    if port.harness {
        out.push_str(&member(
            "seed",
            &["rows: StoreSnapshot".to_string()],
            "void",
        ));
        out.push_str(&member("snapshot", &[], "StoreSnapshot"));
    }
    out.push_str("}\n\n");
}

/// The probe's declarations of what it needs of Node, and `traces`, which reads the traces
/// file line by line: a file past Node's longest string (about 512 MB) cannot be read whole.
pub(super) fn probe_traces(path: &str) -> Vec<String> {
    vec![
        "declare const process: {".to_string(),
        "  exitCode: number | undefined;".to_string(),
        "  env: { readonly [name: string]: string | undefined };".to_string(),
        "  getBuiltinModule: ((id: \"node:fs\") => { createReadStream: (path: string) => unknown }) &".to_string(),
        "    ((id: \"node:readline\") => { createInterface: (options: { input: unknown; crlfDelay: number }) => AsyncIterable<string> });".to_string(),
        "};".to_string(),
        "declare const console: { log: (line: string) => void; error: (line: string) => void };".to_string(),
        format!(
            "const tracesPath = process.env[{}] || {};",
            string_literal(TRACES_ENV),
            string_literal(path)
        ),
        "const traces = (): AsyncIterable<string> =>".to_string(),
        "  process.getBuiltinModule(\"node:readline\").createInterface({ input: process.getBuiltinModule(\"node:fs\").createReadStream(tracesPath), crlfDelay: Infinity });".to_string(),
    ]
}

fn doc1_indent(line: &str) -> String {
    doc(&[line.to_string()], "  ")
}

/// The contracts, and with `--conformance` the port and the harness.
pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    print_contracts(domain, port, out);
    print_unimplemented(port, out);
    if !port.harness {
        return;
    }
    let mut p = String::new();
    let fields = print_snapshot(port, &mut p);
    print_port_trait(port, &mut p);
    print_conform(port, &fields, &mut p);
    print_calls(domain, port, &mut p);
    print_seed_rows(domain, port, &mut p);
    print_abstraction(port, &mut p);

    // Only the readers a call or a row uses, for `noUnusedLocals`.
    for (name, text) in CONF_READERS {
        if p.contains(name) {
            p.push_str(text);
        }
    }
    out.push_str(&p);
}

/// The run marks and `StoreSnapshot`: everything a store holds, as the harness reads it.
/// Returns the snapshot's fields, `(property, row type)`.
fn print_snapshot(port: &PortDecl, p: &mut String) -> Vec<(String, String)> {
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
    fields
}

/// `ConformancePort`, what a finished service offers the harness, and `ConformanceReport`.
fn print_port_trait(port: &PortDecl, p: &mut String) {
    p.push_str(&doc(
        &[
            "What a finished service offers the conformance harness: `seed` empties the store and stores".to_string(),
            "the rows of a trace's initial state, `snapshot` returns everything it holds, and the".to_string(),
            "contract of every use case runs the calls.".to_string(),
        ],
        "",
    ));
    p.push_str("export type ConformancePort = Readonly<{\n  seed: (rows: StoreSnapshot) => Promise<void>;\n  snapshot: () => Promise<StoreSnapshot>;\n}>");
    for c in &port.contracts {
        p.push_str(&format!(" &\n  {}", c.name));
    }
    p.push_str(";\n\n");

    p.push_str(&doc(
        &[
            "What `conform` checked, and the calls that reached the unimplemented port, by action,"
                .to_string(),
            "with the number of traces stopped at one (a refused call does not stop its trace)."
                .to_string(),
        ],
        "",
    ));
    p.push_str("export type ConformanceReport = Readonly<{\n  traces: number;\n  calls: number;\n  refusals: number;\n  interleavings: number;\n  unimplemented: Readonly<Record<string, number>>;\n  stopped: number;\n}>;\n\n");
    p.push_str(&doc1(
        "The line `rdra-ish check --code=run` reads for the calls that reached the unimplemented port: `none` when none did.",
    ));
    p.push_str(&format!(
        "export function unimplementedLine(report: ConformanceReport): string {{\n  if (Object.keys(report.unimplemented).length === 0) {{\n    return `${{{0}}} none`;\n  }}\n  const calls = Object.keys(report.unimplemented).sort().map((a) => `${{a}} ${{report.unimplemented[a] ?? 0}}`);\n  return `${{{0}}} ${{report.stopped}} traces stopped; ${{calls.join(\", \")}}`;\n}}\n\n",
        string_literal(super::conformance::UNIMPLEMENTED_REPORT)
    ));
}

/// `conform`, which drives a fresh port per trace, and the helpers it shares with every model.
/// `fields` are the snapshot's, as `print_snapshot` returns them.
fn print_conform(port: &PortDecl, fields: &[(String, String)], p: &mut String) {
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
    for (f, ty) in fields {
        p.push_str(&format!("  {f}: {ty}[];\n"));
    }
    p.push_str("};\n\n");

    p.push_str(&format!(
        r##"/**
 * Drives a fresh port per trace along `traces` (the file `codegen --conformance` writes): seeds
 * each trace's initial state, makes each call, and after each compares what the store holds with
 * the state the model reaches. A call the model refuses must return an error and change nothing,
 * and so must a commit whose rows another call changed after its prepare. Returns the first
 * difference. `traces` is the file's text, or a function that reads its lines (a file too large
 * for one string: `readline` over `createReadStream`).
 */
export async function conform(newPort: () => ConformancePort, traces: string | (() => AsyncIterable<string>)): Promise<Result<ConformanceReport, string>> {{
  const stale: Result<never, string> = {{ ok: false, error: "not a traces file of this version: generate the code and the traces again" }};
  const states = new Map<string, string[]>();
  let defining: string[] | null = null;
  let report: ConformanceReport = {{ traces: 0, calls: 0, refusals: 0, interleavings: 0, unimplemented: {{}}, stopped: 0 }};
  let stopped = false;
  const count = (call: readonly string[]): void => {{
    const action = call[0] ?? "";
    report = {{ ...report, unimplemented: {{ ...report.unimplemented, [action]: (report.unimplemented[action] ?? 0) + 1 }} }};
  }};
  const stop = (call: readonly string[]): void => {{
    count(call);
    report = {{ ...report, stopped: report.stopped + 1 }};
    stopped = true;
  }};
  let port: ConformancePort | null = null;
  let seed = $confEmpty();
  let want: readonly string[] = [];
  let n = 0;
  for await (const line of typeof traces === "string" ? traces.split("\n") : traces()) {{
    n++;
    if (n === 1) {{
      if (line.trimEnd() !== {format}) {{
        return stale;
      }}
      continue;
    }}
    const toks = line.split(/\s+/).filter((t) => t !== "");
    const [kind, ...rest] = toks;
    const at = (what: string): Result<never, string> => ({{ ok: false, error: `traces line ${{n}}: ${{what}}` }});
    if (kind === undefined || kind === "#") {{
      continue;
    }}
    const state = (id: string | undefined): readonly string[] | undefined => (id === undefined ? undefined : states.get(id));
    if (stopped && "RGCXI".includes(kind)) {{
      continue;
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
        port = newPort();
        seed = $confEmpty();
        stopped = false;
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
        if (!ran.value.ok && ran.value.error === $confUnimplemented) {{
          stop(call);
          continue;
        }}
        if (!ran.value.ok) {{
          return at($confRefused(call, ran.value.error));
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
        const [kind, ...call] = rest;
        if (port === null || kind === undefined) {{
          return at("a call outside a trace");
        }}
        const ran = await $confRun(port, call);
        if (!ran.ok) {{
          return at(ran.error);
        }}
        if (!ran.value.ok && ran.value.error === $confUnimplemented) {{
          count(call);
          continue;
        }}
        if (ran.value.ok) {{
          return at(`the model refuses \`${{call.join(" ")}}\` here, and the service ran it`);
        }}
        const got = $confKind(ran.value.error);
        if (got === "StoreFailed") {{
          return at($confRefused(call, ran.value.error));
        }}
        if (kind !== "-" && got !== kind) {{
          return at(`the model refuses \`${{call.join(" ")}}\` here with ${{kind}}, and the service refused it with ${{got}}`);
        }}
        const diff = $confCompare($confAbstract(await port.snapshot()), want, `after the refused \`${{call.join(" ")}}\``);
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
        if (!early.value.ok && early.value.error === $confUnimplemented) {{
          stop(first);
          continue;
        }}
        if (!early.value.ok) {{
          return at($confRefused(first, early.value.error));
        }}
        const ran = await $confRun(port, second);
        if (!ran.ok) {{
          return at(ran.error);
        }}
        if (!ran.value.ok && ran.value.error === $confUnimplemented) {{
          stop(second);
          continue;
        }}
        if (!ran.value.ok) {{
          return at($confRefused(second, ran.value.error));
        }}
        const between = $confCompare($confAbstract(await port.snapshot()), next, `after \`${{second.join(" ")}}\``);
        if (between !== null) {{
          return at(between);
        }}
        const late = await $confCommit(port, early.value.value);
        if (!late.ok && late.error === $confUnimplemented) {{
          stop(first);
          continue;
        }}
        if (late.ok) {{
          return at(`the service committed \`${{first.join(" ")}}\`, prepared before \`${{second.join(" ")}}\` changed a row it writes: the write must be conditional on what the row was`);
        }}
        if ($confKind(late.error) === "StoreFailed") {{
          return at($confRefused(first, late.error));
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
  return n === 0 ? stale : {{ ok: true, value: report }};
}}

/** The refusal `$confTry` turns a rejection with `Unimplemented` into. */
const $confUnimplemented: unique symbol = Symbol("rdra-ish unimplemented port");

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

/** What is wrong when the service did not run `call`, which the model runs: a refusal, or, for a StoreFailed, a failure of the service. */
function $confRefused(call: readonly string[], error: unknown): string {{
  return $confKind(error) === "StoreFailed"
    ? `the service failed on \`${{call.join(" ")}}\`: ${{$confShow(error)}}`
    : `the model runs \`${{call.join(" ")}}\` here, and the service refused it: ${{$confShow(error)}}`;
}}

/** The kind of a refusal: the `kind` of the error the service returned. */
function $confKind(error: unknown): string {{
  return typeof error === "object" && error !== null && "kind" in error && typeof error.kind === "string" ? error.kind : "an error of no kind";
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
}

/// The calls: `$confPrepare` reads a call's values and prepares it; `$confCommit` commits it.
fn print_calls(domain: &DomainModel, port: &PortDecl, p: &mut String) {
    // A model whose every action is an entry point has no call: the harness names none.
    if port.methods.is_empty() {
        p.push_str("async function $confPrepare(_port: ConformancePort, call: readonly string[]): Promise<Result<Result<$ConfWrite, unknown>, string>> {\n  const [action] = call;\n  return { ok: false, error: `no use case ${action ?? \"\"} in this port` };\n}\n\n");
        p.push_str("function $confCommit(_port: ConformancePort, w: $ConfWrite): Promise<Result<void, unknown>> {\n  return assertNever(w);\n}\n\n");
        return;
    }
    // The calls.
    p.push_str(
        r##"/** What the service returned, or `$confUnimplemented` when it rejected with `Unimplemented`. */
function $confTry<T>(f: () => Promise<Result<T, unknown>>): Promise<Result<T, unknown>> {
  return f().then(undefined, (e: unknown): Result<T, unknown> | Promise<never> => (e instanceof Unimplemented ? { ok: false, error: $confUnimplemented } : Promise.reject(e)));
}

"##,
    );
    p.push_str("async function $confPrepare(port: ConformancePort, call: readonly string[]): Promise<Result<Result<$ConfWrite, unknown>, string>> {\n");
    p.push_str("  const [action, ...values] = call;\n  switch (action) {\n");
    for (k, m) in port.methods.iter().enumerate() {
        p.push_str(&format!("    case {}: {{\n", string_literal(&m.action)));
        let mut args = Vec::new();
        // The harness runs each call as the first actor that performs the use case.
        if let Some(caller) = port
            .contracts
            .iter()
            .find(|c| c.methods.contains(&k))
            .and_then(|c| c.caller.as_ref())
        {
            args.push(format!(
                "{}.issue()",
                domain.capabilities[caller.actors[0]].name
            ));
        }
        let mut i = 0usize;
        let mut arg = |p: &mut String, reader: String| -> String {
            let name = format!("$a{i}");
            p.push_str(&format!(
                "      const {name} = $confArg(values, {i}, {reader});\n      if (!{name}.ok) {{\n        return {name};\n      }}\n"
            ));
            i += 1;
            format!("{name}.value")
        };
        for a in m.args() {
            let value = match a {
                PortArg::Id(a) => arg(&mut *p, reader(domain, &FieldType::Id(a.ty.clone()), false)),
                PortArg::Event(a) => {
                    let event = domain.events.iter().find(|e| e.name == a.ty);
                    let mut parts = vec![format!("kind: {}", string_literal(&a.ty))];
                    for f in event.map(|e| e.fields.clone()).unwrap_or_default() {
                        let v = arg(
                            &mut *p,
                            reader(domain, &FieldType::Id(f.id_ty.clone()), false),
                        );
                        parts.push(format!("{}: {v}", property(&f.name)));
                    }
                    format!("{{ {} }}", parts.join(", "))
                }
                PortArg::Input(i) => arg(&mut *p, reader(domain, &i.ty, i.nullable)),
                PortArg::Choice(a) => arg(
                    &mut *p,
                    reader(domain, &FieldType::Enum(a.ty.clone()), false),
                ),
            };
            args.push(value);
        }
        p.push_str(&format!(
            "      const r = await $confTry(() => port.{}({}));\n      return {{ ok: true, value: r.ok ? {{ ok: true, value: {{ action: {}, write: r.value }} }} : r }};\n    }}\n",
            method_names(m)[0],
            args.join(", "),
            string_literal(&m.action)
        ));
    }
    p.push_str("    default:\n      return { ok: false, error: `no use case ${action ?? \"\"} in this port` };\n  }\n}\n\n");

    p.push_str("function $confCommit(port: ConformancePort, w: $ConfWrite): Promise<Result<void, unknown>> {\n  const action = w.action;\n  switch (action) {\n");
    for m in &port.methods {
        p.push_str(&format!(
            "    case {}:\n      return $confTry(() => port.{}(w.write));\n",
            string_literal(&m.action),
            method_names(m)[1]
        ));
    }
    p.push_str("    default:\n      return assertNever(action);\n  }\n}\n\n");
}

/// `$confSeedRow`: the rows of an initial state.
fn print_seed_rows(domain: &DomainModel, port: &PortDecl, p: &mut String) {
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
}

/// `$confAbstract`: the store as the traces spell a state.
fn print_abstraction(port: &PortDecl, p: &mut String) {
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
            let repr = domain.id_repr(name);
            if repr == Scalar::Int {
                format!("$confNum({name}.parse)")
            } else {
                format!("$confText({name}.parse)")
            }
        }
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
