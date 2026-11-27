//! The commit part in TypeScript: the unions `RowWrite`, `RunMark`, `MarkWrite` and
//! `RaisedEvent`, the record `Writes`, a companion with `writes` per result and outcome type,
//! and the helper `commitWrites` over a `CommitStore`.

use super::super::ir::{DomainModel, FnDecl};
use super::super::string_literal;
use super::super::ts::{doc1, fn_name, property};
use super::{items, model, name, Item, Model};

pub(in crate::codegen) fn print(domain: &DomainModel, out: &mut String) {
    if domain.fns.is_empty() {
        return;
    }
    let m = model(domain);
    let [row_write, run_mark, mark_write, raised, writes, store, commit] = [
        "RowWrite",
        "RunMark",
        "MarkWrite",
        "RaisedEvent",
        "Writes",
        "CommitStore",
        "CommitWrites",
    ]
    .map(|n| name(domain, n));
    let commit = fn_name(&commit);

    out.push_str("// ── Commit: what a result writes ──\n\n");

    out.push_str(&doc1(
        "One row a commit writes, only where the row is still what `expected` names: when it is not, the commit writes nothing and the call is NotEnabled. A put stores the row (inserts it when `expected` is absent, else updates it); a delete removes it. `id` is null for an entity with no one-column key.",
    ));
    let mut cases = Vec::new();
    for s in &m.stored {
        let entity = string_literal(s.entity);
        let id = s.key.map_or("null", |(ty, _)| ty);
        cases.push(format!(
            "Readonly<{{ kind: \"put\"; entity: {entity}; id: {id}; row: {}; expected: ExpectedSource }}>",
            s.row
        ));
        cases.push(format!(
            "Readonly<{{ kind: \"delete\"; entity: {entity}; id: {id}; expected: ExpectedSource }}>"
        ));
    }
    out.push_str(&union(&row_write, &cases));

    out.push_str(&doc1(
        "A run mark of a use case split into transactions, on one row: `step` is the step of the run that last wrote the row.",
    ));
    let cases: Vec<String> = m
        .marks
        .iter()
        .map(|k| {
            format!(
                "Readonly<{{ usecase: {}; entity: {}; id: {}; step: number }}>",
                string_literal(k.usecase),
                string_literal(k.entity),
                k.id_ty
            )
        })
        .collect();
    out.push_str(&union(&run_mark, &cases));
    out.push_str(&doc1(
        "One run mark a commit writes. A put at step 1 inserts the mark, where the row holds no mark of the use case; a put at a later step advances the row's mark from the step before; a delete removes the mark at its step, and the run ends on the row.",
    ));
    out.push_str(&format!(
        "export type {mark_write} = Readonly<{{ kind: \"put\" | \"delete\"; mark: {run_mark} }}>;\n\n"
    ));
    out.push_str(&doc1("An event of any type a use case raises."));
    let cases: Vec<String> = m.events.iter().map(|e| (*e).to_string()).collect();
    out.push_str(&union(&raised, &cases));
    out.push_str(&doc1(
        "What one commit stores, in order: the rows, then the run marks, then the events.",
    ));
    out.push_str(&format!(
        "export type {writes} = Readonly<{{\n  usecase: string;\n  rows: readonly {row_write}[];\n  marks: readonly {mark_write}[];\n  events: readonly {raised}[];\n}}>;\n\n"
    ));

    for f in &domain.fns {
        out.push_str(&companion(domain, &m, f, &f.result, &f.usecase, &writes));
    }
    for d in &domain.dispatchers {
        // The cases of one action write the same fields; only their states differ, so the
        // union's fields are read without telling the cases apart.
        let Some(f) = d
            .cases
            .first()
            .and_then(|c| domain.fns.iter().find(|f| f.name == c.function))
        else {
            continue;
        };
        out.push_str(&companion(domain, &m, f, &d.outcome, &d.usecase, &writes));
    }

    out.push_str(&doc1(
        "A store that commits a result inside the transaction of the call: `commitWrites` calls it once per write, in order. Each method writes one item only where it is still what it names; `E` is the store's own error.",
    ));
    out.push_str(&format!(
        "export type {store}<E> = Readonly<{{\n  /** Writes the row where it is still what `expected` names: false, and nothing written, when it is not. */\n  writeRow: (write: {row_write}) => Promise<Result<boolean, E>>;\n  /** Writes the run mark where the row's mark is still what the step needs: none of the use case for a put at step 1, the step before for a later put, the mark's own step for a delete. False, and nothing written, when it is not. */\n  writeMark: (write: {mark_write}) => Promise<Result<boolean, E>>;\n  /** Records a raised event. */\n  recordEvent: (event: {raised}) => Promise<Result<void, E>>;\n}}>;\n\n"
    ));
    out.push_str(&doc1(
        "Commits `writes` through `store`: every row, then every run mark, then every event, and stops at the first row or mark that is no longer what it names. It begins and ends no transaction: call it inside the transaction of the call, and roll back on an error. NotEnabled when a row or a run mark is no longer what the result expects; the store's error when the store fails.",
    ));
    out.push_str(&format!(
        "export async function {commit}<E>(store: {store}<E>, writes: {writes}): Promise<Result<void, NotEnabled | E>> {{\n  const notEnabled: NotEnabled = {{ kind: \"NotEnabled\", usecase: writes.usecase }};\n  for (const write of writes.rows) {{\n    const written = await store.writeRow(write);\n    if (!written.ok) {{\n      return written;\n    }}\n    if (!written.value) {{\n      return {{ ok: false, error: notEnabled }};\n    }}\n  }}\n  for (const write of writes.marks) {{\n    const written = await store.writeMark(write);\n    if (!written.ok) {{\n      return written;\n    }}\n    if (!written.value) {{\n      return {{ ok: false, error: notEnabled }};\n    }}\n  }}\n  for (const event of writes.events) {{\n    const recorded = await store.recordEvent(event);\n    if (!recorded.ok) {{\n      return recorded;\n    }}\n  }}\n  return {{ ok: true, value: undefined }};\n}}\n\n"
    ));
}

/// `export type <name> = a | b;`, or `never` with no case.
fn union(name: &str, cases: &[String]) -> String {
    match cases {
        [] => format!("export type {name} = never;\n\n"),
        [one] => format!("export type {name} = {one};\n\n"),
        _ => format!(
            "export type {name} =\n{};\n\n",
            cases
                .iter()
                .map(|c| format!("  | {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// The companion of the result or outcome type `ty`, whose fields are those of `f`'s result:
/// `writes` lists what a commit of it stores.
fn companion(
    domain: &DomainModel,
    m: &Model<'_>,
    f: &FnDecl,
    ty: &str,
    usecase: &str,
    writes: &str,
) -> String {
    let items = items(domain, m, f);
    let r = if items.is_empty() { "_r" } else { "r" };
    let mut lets = String::new();
    let (mut rows, mut marks, mut events) = (Vec::new(), Vec::new(), Vec::new());
    for item in items {
        match item {
            Item::Put {
                field,
                expected,
                stored,
            } => {
                let s = &m.stored[stored];
                let local = format!("row{}", rows.len());
                let value = format!("r.{}", property(&field));
                let row = match s.sum {
                    Some(sum) => format!("{}.toRow({value})", sum.name),
                    None => value,
                };
                lets.push_str(&format!("    const {local} = {row};\n"));
                let id = s.key.map_or("null".to_string(), |(_, k)| {
                    format!("{local}.{}", property(&k.wire))
                });
                rows.push(format!(
                    "{{ kind: \"put\", entity: {}, id: {id}, row: {local}, expected: r.{} }}",
                    string_literal(s.entity),
                    property(&expected)
                ));
            }
            Item::Delete {
                field,
                expected,
                stored,
            } => {
                let s = &m.stored[stored];
                let id = match (field, s.key) {
                    (Some(field), Some(_)) => format!("r.{}", property(&field)),
                    _ => "null".to_string(),
                };
                rows.push(format!(
                    "{{ kind: \"delete\", entity: {}, id: {id}, expected: r.{} }}",
                    string_literal(s.entity),
                    property(&expected)
                ));
            }
            Item::Mark {
                field,
                mark,
                step,
                delete,
            } => {
                let k = &m.marks[mark];
                marks.push(format!(
                    "{{ kind: \"{}\", mark: {{ usecase: {}, entity: {}, id: r.{}.id, step: {step} }} }}",
                    if delete { "delete" } else { "put" },
                    string_literal(k.usecase),
                    string_literal(k.entity),
                    property(&field)
                ));
            }
            Item::Event { field, .. } => events.push(format!("r.{}", property(&field))),
        }
    }
    let list = |items: Vec<String>| format!("[{}]", items.join(", "));
    format!(
        "{}export const {ty} = {{\n  /** What a commit of it stores: every row, run mark, and event it writes, in order. */\n  writes: ({r}: {ty}): {writes} => {{\n{lets}    return {{ usecase: {}, rows: {}, marks: {}, events: {} }};\n  }},\n}};\n\n",
        doc1(&format!("The companion of {ty}.")),
        string_literal(usecase),
        list(rows),
        list(marks),
        list(events),
    )
}
