//! The commit part in Go: the sealed interfaces `StoredRow`, `StoredId` and `RaisedEvent`, the
//! structs `RowWrite`, `RunMark`, `MarkWrite` and `Writes`, the method `Writes()` on every
//! result and outcome with the interface `Committable`, and the helper `CommitWrites` over a
//! `CommitStore`.

use super::super::go::{struct_decl, Line};
use super::super::ir::{DomainModel, OutKind};
use super::super::{string_literal, type_ident};
use super::{items, model, name, Item};

pub(in crate::codegen) fn print(domain: &DomainModel, out: &mut String) {
    if domain.fns.is_empty() {
        return;
    }
    let m = model(domain);
    let [stored_row, stored_id, row_write, run_mark, mark_write, raised, writes, committable, store, commit] =
        [
            "StoredRow",
            "StoredId",
            "RowWrite",
            "RunMark",
            "MarkWrite",
            "RaisedEvent",
            "Writes",
            "Committable",
            "CommitStore",
            "CommitWrites",
        ]
        .map(|n| name(domain, n));
    let method = writes_method(domain);

    out.push_str("// ── Commit: what a result writes ──\n\n");

    out.push_str(&format!(
        "// {stored_row} is a row of any entity as a store holds it: its type names the entity.\ntype {stored_row} interface {{\n\tisStoredRow()\n}}\n\n"
    ));
    for s in &m.stored {
        out.push_str(&format!("func ({}) isStoredRow() {{}}\n\n", s.row));
    }
    out.push_str(&format!(
        "// {stored_id} is the id of a row of any entity: its type names the entity.\ntype {stored_id} interface {{\n\tisStoredId()\n}}\n\n"
    ));
    let mut ids: Vec<&str> = m.stored.iter().filter_map(|s| s.key.map(|k| k.0)).collect();
    ids.dedup();
    for id in ids {
        out.push_str(&format!("func ({id}) isStoredId() {{}}\n\n"));
    }

    out.push_str(&format!(
        "// {row_write} is one row a commit writes, only where the row is still what Expected names: when\n// it is not, the commit writes nothing and the call is NotEnabled. Row is stored (inserted when\n// Expected is Absent, else updated), or the row is deleted when Delete is set.\n"
    ));
    out.push_str(&struct_decl(
        &row_write,
        &[
            Line::Comment("Entity is the entity of the row, as the model names it.".into()),
            Line::Cells(vec!["Entity".into(), "string".into()]),
            Line::Comment("Id is the row's id: nil when the entity has no one-column key.".into()),
            Line::Cells(vec!["Id".into(), stored_id.clone()]),
            Line::Comment("Row is the row to store: nil for a delete.".into()),
            Line::Cells(vec!["Row".into(), stored_row.clone()]),
            Line::Comment("Delete says the row is deleted.".into()),
            Line::Cells(vec!["Delete".into(), "bool".into()]),
            Line::Comment("Expected is what the row must still be.".into()),
            Line::Cells(vec!["Expected".into(), "ExpectedSource".into()]),
        ],
    ));
    out.push_str(&format!(
        "// {run_mark} is a run mark of a use case split into transactions, on one row.\n"
    ));
    out.push_str(&struct_decl(
        &run_mark,
        &[
            Line::Comment("UseCase is the use case of the run, as the model names it.".into()),
            Line::Cells(vec!["UseCase".into(), "string".into()]),
            Line::Comment("Entity is the entity of the marked row, as the model names it.".into()),
            Line::Cells(vec!["Entity".into(), "string".into()]),
            Line::Comment("Id is the id of the marked row.".into()),
            Line::Cells(vec!["Id".into(), stored_id.clone()]),
            Line::Comment("Step is the step of the run that last wrote the row.".into()),
            Line::Cells(vec!["Step".into(), "int".into()]),
        ],
    ));
    out.push_str(&format!(
        "// {mark_write} is one run mark a commit writes. A put at step 1 inserts the mark, where the\n// row holds no mark of the use case; a put at a later step advances the row's mark from the\n// step before; a delete removes the mark at its step, and the run ends on the row.\n"
    ));
    out.push_str(&struct_decl(
        &mark_write,
        &[
            Line::Comment("Mark is the mark.".into()),
            Line::Cells(vec!["Mark".into(), run_mark.clone()]),
            Line::Comment("Delete says the mark is deleted.".into()),
            Line::Cells(vec!["Delete".into(), "bool".into()]),
        ],
    ));
    out.push_str(&format!(
        "// {raised} is an event of any type a use case raises: its type names the event.\ntype {raised} interface {{\n\tisRaisedEvent()\n}}\n\n"
    ));
    for e in &m.events {
        out.push_str(&format!("func ({e}) isRaisedEvent() {{}}\n\n"));
    }
    out.push_str(&format!(
        "// {writes} is what one commit stores, in order: the rows, then the run marks, then the events.\n"
    ));
    out.push_str(&struct_decl(
        &writes,
        &[
            Line::Comment("UseCase is the use case whose result it is.".into()),
            Line::Cells(vec!["UseCase".into(), "string".into()]),
            Line::Comment("Rows are the rows to write.".into()),
            Line::Cells(vec!["Rows".into(), format!("[]{row_write}")]),
            Line::Comment("Marks are the run marks to store, advance, or delete.".into()),
            Line::Cells(vec!["Marks".into(), format!("[]{mark_write}")]),
            Line::Comment("Events are the events raised.".into()),
            Line::Cells(vec!["Events".into(), format!("[]{raised}")]),
        ],
    ));
    out.push_str(&format!(
        "// {committable} is a result or an outcome a commit stores: every result and outcome type is one.\ntype {committable} interface {{\n\t// {method} returns every row, run mark, and event the commit writes, in order.\n\t{method}() {writes}\n}}\n\n"
    ));

    for f in &domain.fns {
        let mut lets = String::new();
        let (mut rows, mut marks, mut events) = (Vec::new(), Vec::new(), Vec::new());
        for item in items(domain, &m, f) {
            match item {
                Item::Put {
                    field,
                    expected,
                    stored,
                } => {
                    let s = &m.stored[stored];
                    let local = format!("row{}", rows.len());
                    let value = format!("r.{}", type_ident(&field));
                    let row = match s.sum {
                        Some(sum) => format!("{}ToRow({value})", sum.name),
                        None => value,
                    };
                    lets.push_str(&format!("\t{local} := {row}\n"));
                    let id = s
                        .key
                        .map(|(_, k)| format!("Id: {local}.{}, ", k.pascal))
                        .unwrap_or_default();
                    rows.push(format!(
                        "{{Entity: {}, {id}Row: {local}, Expected: r.{}}}",
                        string_literal(s.entity),
                        type_ident(&expected)
                    ));
                }
                Item::Delete {
                    field,
                    expected,
                    stored,
                } => {
                    let s = &m.stored[stored];
                    let id = field
                        .map(|f| format!("Id: r.{}, ", type_ident(&f)))
                        .unwrap_or_default();
                    rows.push(format!(
                        "{{Entity: {}, {id}Delete: true, Expected: r.{}}}",
                        string_literal(s.entity),
                        type_ident(&expected)
                    ));
                }
                Item::Mark {
                    field,
                    mark,
                    step,
                    delete,
                } => {
                    let k = &m.marks[mark];
                    let delete = if delete { ", Delete: true" } else { "" };
                    marks.push(format!(
                        "{{Mark: {run_mark}{{UseCase: {}, Entity: {}, Id: r.{}.Id, Step: {step}}}{delete}}}",
                        string_literal(k.usecase),
                        string_literal(k.entity),
                        type_ident(&field)
                    ));
                }
                Item::Event { ty, .. } => events.push(format!("r.{ty}")),
            }
        }
        let mut parts = vec![format!("UseCase: {}", string_literal(&f.usecase))];
        for (label, ty, list) in [
            ("Rows", &row_write, rows),
            ("Marks", &mark_write, marks),
            ("Events", &raised, events),
        ] {
            if !list.is_empty() {
                parts.push(format!("{label}: []{ty}{{{}}}", list.join(", ")));
            }
        }
        out.push_str(&format!(
            "// {method} returns what a commit of the result stores.\nfunc (r {}) {method}() {writes} {{\n{lets}\treturn {writes}{{{}}}\n}}\n\n",
            f.result,
            parts.join(", ")
        ));
    }
    for d in &domain.dispatchers {
        out.push_str(&format!(
            "// {method} returns what a commit of the case that ran stores: nothing when no case is set.\nfunc (o {}) {method}() {writes} {{\n",
            d.outcome
        ));
        for c in &d.cases {
            out.push_str(&format!(
                "\tif o.{0} != nil {{\n\t\treturn o.{0}.{method}()\n\t}}\n",
                c.case
            ));
        }
        out.push_str(&format!(
            "\treturn {writes}{{UseCase: {}}}\n}}\n\n",
            string_literal(&d.usecase)
        ));
    }

    out.push_str(&format!(
        "// {store} is a store that commits a result inside the transaction of the call: {commit} calls\n// it once per write, in order. Each method writes one item only where it is still what it names.\ntype {store} interface {{\n\t// WriteRow writes the row where it is still what w.Expected names: false, and nothing\n\t// written, when it is not. Its error is the store's own.\n\tWriteRow(w {row_write}) (bool, error)\n\t// WriteMark writes the run mark where the row's mark is still what the step needs: none of\n\t// the use case for a put at step 1, the step before for a later put, the mark's own step\n\t// for a delete. False, and nothing written, when it is not. Its error is the store's own.\n\tWriteMark(w {mark_write}) (bool, error)\n\t// RecordEvent records a raised event. Its error is the store's own.\n\tRecordEvent(e {raised}) error\n}}\n\n"
    ));
    out.push_str(&format!(
        "// {commit} commits what w writes through store: every row, then every run mark, then every\n// event, and stops at the first row or mark that is no longer what it names. It begins and\n// ends no transaction: call it inside the transaction of the call, and roll back on an error.\n// Its error is a *NotEnabled when a row or a run mark is no longer what the result expects,\n// or the store's error as the store returned it.\nfunc {commit}(store {store}, w {committable}) error {{\n\twrites := w.{method}()\n\tfor _, row := range writes.Rows {{\n\t\tok, err := store.WriteRow(row)\n\t\tif err != nil {{\n\t\t\treturn err\n\t\t}}\n\t\tif !ok {{\n\t\t\treturn &NotEnabled{{UseCase: writes.UseCase}}\n\t\t}}\n\t}}\n\tfor _, mark := range writes.Marks {{\n\t\tok, err := store.WriteMark(mark)\n\t\tif err != nil {{\n\t\t\treturn err\n\t\t}}\n\t\tif !ok {{\n\t\t\treturn &NotEnabled{{UseCase: writes.UseCase}}\n\t\t}}\n\t}}\n\tfor _, event := range writes.Events {{\n\t\tif err := store.RecordEvent(event); err != nil {{\n\t\t\treturn err\n\t\t}}\n\t}}\n\treturn nil\n}}\n\n"
    ));
}

/// The name of the method that returns a result's writes: `Writes`, or `Writes_` when a
/// result of the model has a field of that name, which Go does not allow beside a method.
fn writes_method(domain: &DomainModel) -> String {
    let clash = domain.fns.iter().any(|f| {
        let mut fields: Vec<String> = Vec::new();
        for o in &f.outputs {
            if !matches!(o.kind, OutKind::Deleted { id: None, .. }) {
                fields.push(type_ident(&o.name));
            }
            fields.push(type_ident(&o.expected_name()));
        }
        fields.extend(f.marks_out.iter().map(|m| type_ident(&m.name)));
        fields.extend(f.raises.iter().map(|e| e.ty.clone()));
        fields.extend(f.marks_ended().into_iter().map(|(_, n)| type_ident(&n)));
        fields.iter().any(|n| n == "Writes")
    });
    if clash { "Writes_" } else { "Writes" }.to_string()
}
