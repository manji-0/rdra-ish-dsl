//! The commit part in Rust: `StoredRow`, `StoredId`, `RowWrite`, `RunMark`, `MarkWrite`,
//! `RaisedEvent`, `Writes`, the trait `IntoWrites` on every result and outcome, and the helper
//! `commit_writes` over a `CommitStore`.

use super::super::ir::DomainModel;
use super::super::rust::{ident, is_copy, OPTION, RESULT};
use super::super::string_literal;
use super::{items, model, name, Item};

pub(in crate::codegen) fn print(domain: &DomainModel, out: &mut String) {
    if domain.fns.is_empty() {
        return;
    }
    let mut part = String::new();
    print_part(domain, &mut part);
    // An enum with no case (no run mark, no event) as rustfmt writes it.
    out.push_str(&part.replace(" {\n}\n", " {}\n"));
}

fn print_part(domain: &DomainModel, out: &mut String) {
    let m = model(domain);
    let [stored_row, stored_id, row_write, run_mark, mark_write, raised, writes, into, store, failed, commit] =
        [
            "StoredRow",
            "StoredId",
            "RowWrite",
            "RunMark",
            "MarkWrite",
            "RaisedEvent",
            "Writes",
            "IntoWrites",
            "CommitStore",
            "CommitFailed",
            "CommitWrites",
        ]
        .map(|n| name(domain, n));
    let commit = super::super::snake(&commit);
    let vec = "::std::vec::Vec";

    out.push_str("// ── Commit: what a result writes ──\n\n");

    // StoredRow.
    out.push_str(
        "/// A row of any entity, as a store holds it: the case names the entity.\n#[derive(Debug, Clone, PartialEq, Eq)]\n",
    );
    out.push_str(&format!("pub enum {stored_row} {{\n"));
    for s in &m.stored {
        out.push_str(&format!(
            "    /// A row of {}.\n    {}({}),\n",
            s.entity,
            case(s),
            s.row
        ));
    }
    out.push_str("}\n\n");
    out.push_str(&entity_fn(&stored_row, &m.stored, "(_)"));

    // StoredId.
    out.push_str(
        "/// The id of a row of any entity: the case names the entity.\n#[derive(Debug, Clone, PartialEq, Eq)]\n",
    );
    out.push_str(&format!("pub enum {stored_id} {{\n"));
    for s in &m.stored {
        match s.key {
            Some((ty, _)) => out.push_str(&format!(
                "    /// An id of {}.\n    {}({ty}),\n",
                s.entity,
                case(s)
            )),
            None => out.push_str(&format!(
                "    /// A row of {}: it has no one-column key, so no id names it.\n    {},\n",
                s.entity,
                case(s)
            )),
        }
    }
    out.push_str("}\n\n");
    out.push_str(&format!(
        "impl {stored_id} {{\n    /// The entity of the row, as the model names it.\n    pub fn entity(&self) -> &'static str {{\n        match *self {{\n"
    ));
    for s in &m.stored {
        let pattern = if s.key.is_some() { "(_)" } else { "" };
        out.push_str(&format!(
            "            Self::{}{pattern} => {},\n",
            case(s),
            string_literal(s.entity)
        ));
    }
    out.push_str("        }\n    }\n}\n\n");

    // RowWrite.
    out.push_str(&format!(
        "/// One row a commit writes, only where the row is still what `expected` names: when it is\n/// not, the commit writes nothing and the call is `NotEnabled`.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {row_write} {{\n    /// Store the row: insert it when `expected` is `Absent`, else update it.\n    Put {{\n        /// The row's id.\n        id: {stored_id},\n        /// The row.\n        row: {stored_row},\n        /// What the row must still be.\n        expected: ExpectedSource,\n    }},\n    /// Delete the row.\n    Delete {{\n        /// The row's id.\n        id: {stored_id},\n        /// What the row must still be.\n        expected: ExpectedSource,\n    }},\n}}\n\n"
    ));

    // RunMark.
    out.push_str(
        "/// A run mark of a use case split into transactions, on one row: the case names the use\n/// case and the entity.\n#[derive(Debug, Clone, PartialEq, Eq)]\n",
    );
    out.push_str(&format!("pub enum {run_mark} {{\n"));
    for k in &m.marks {
        out.push_str(&format!(
            "    /// A mark of {} on a {} row.\n    {} {{\n        /// The id of the marked row.\n        id: {},\n        /// The step of the run that last wrote the row.\n        step: u8,\n    }},\n",
            k.usecase, k.entity, k.case, k.id_ty
        ));
    }
    out.push_str("}\n\n");
    out.push_str(&format!("impl {run_mark} {{\n"));
    for (doc, method, ty, value) in [
        (
            "The use case of the run, as the model names it.",
            "usecase",
            "&'static str",
            0,
        ),
        (
            "The entity of the marked row, as the model names it.",
            "entity",
            "&'static str",
            1,
        ),
        (
            "The step of the run that last wrote the row.",
            "step",
            "u8",
            2,
        ),
    ] {
        out.push_str(&format!(
            "    /// {doc}\n    pub fn {method}(&self) -> {ty} {{\n        match *self {{\n"
        ));
        for k in &m.marks {
            let (pattern, v) = match value {
                0 => ("{ .. }", string_literal(k.usecase)),
                1 => ("{ .. }", string_literal(k.entity)),
                _ => ("{ step, .. }", "step".to_string()),
            };
            out.push_str(&format!("            Self::{} {pattern} => {v},\n", k.case));
        }
        out.push_str("        }\n    }\n\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out.push_str("}\n\n");

    // MarkWrite.
    out.push_str(&format!(
        "/// One run mark a commit writes.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {mark_write} {{\n    /// Store the mark: insert it at step 1, where the row holds no mark of the use case;\n    /// at a later step, advance the row's mark from the step before.\n    Put({run_mark}),\n    /// Delete the mark, which is at the step it names: the run ends on the row.\n    Delete({run_mark}),\n}}\n\n"
    ));

    // RaisedEvent.
    out.push_str(
        "/// An event of any type a use case raises: the case names the event.\n#[derive(Debug, Clone, PartialEq, Eq)]\n",
    );
    out.push_str(&format!("pub enum {raised} {{\n"));
    for e in &m.events {
        out.push_str(&format!("    /// The event {e}.\n    {e}({e}),\n"));
    }
    out.push_str("}\n\n");

    // Writes and IntoWrites.
    out.push_str(&format!(
        "/// What one commit stores, in order: the rows, then the run marks, then the events.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {writes} {{\n    /// The use case whose result it is.\n    pub usecase: &'static str,\n    /// The rows to write.\n    pub rows: {vec}<{row_write}>,\n    /// The run marks to store, advance, or delete.\n    pub marks: {vec}<{mark_write}>,\n    /// The events raised.\n    pub events: {vec}<{raised}>,\n}}\n\n"
    ));
    out.push_str(&format!(
        "/// A result or an outcome a commit stores: every row, run mark, and event it writes.\npub trait {into} {{\n    /// The writes, in the order a commit makes them.\n    fn into_writes(self) -> {writes};\n}}\n\n"
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
                    let local = format!("row_{}", rows.len());
                    let value = format!("self.{}", ident(&field));
                    let row = match s.sum {
                        Some(sum) => format!("{}::from({value}).into_row()", sum.name),
                        None => value,
                    };
                    lets.push_str(&format!("        let {local} = {row};\n"));
                    let id = match s.key {
                        Some((_, key)) => {
                            let clone = if is_copy(domain, &key.ty) {
                                ""
                            } else {
                                ".clone()"
                            };
                            format!(
                                "{stored_id}::{}({local}.{}{clone})",
                                case(s),
                                ident(&key.wire)
                            )
                        }
                        None => format!("{stored_id}::{}", case(s)),
                    };
                    rows.push(format!(
                        "{row_write}::Put {{ id: {id}, row: {stored_row}::{}({local}), expected: self.{} }}",
                        case(s),
                        ident(&expected)
                    ));
                }
                Item::Delete {
                    field,
                    expected,
                    stored,
                } => {
                    let s = &m.stored[stored];
                    let id = match (&field, s.key) {
                        (Some(field), Some(_)) => {
                            format!("{stored_id}::{}(self.{})", case(s), ident(field))
                        }
                        _ => format!("{stored_id}::{}", case(s)),
                    };
                    rows.push(format!(
                        "{row_write}::Delete {{ id: {id}, expected: self.{} }}",
                        ident(&expected)
                    ));
                }
                Item::Mark {
                    field,
                    mark,
                    step,
                    delete,
                } => {
                    let op = if delete { "Delete" } else { "Put" };
                    marks.push(format!(
                        "{mark_write}::{op}({run_mark}::{} {{ id: self.{}.id, step: {step} }})",
                        m.marks[mark].case,
                        ident(&field)
                    ));
                }
                Item::Event { field, ty } => {
                    events.push(format!("{raised}::{ty}(self.{})", ident(&field)));
                }
            }
        }
        let list = |items: Vec<String>| {
            if items.is_empty() {
                format!("{vec}::new()")
            } else {
                format!(
                    "::std::vec![\n{}            ]",
                    items
                        .iter()
                        .map(|i| format!("                {i},\n"))
                        .collect::<String>()
                )
            }
        };
        out.push_str(&format!(
            "impl {into} for {} {{\n    fn into_writes(self) -> {writes} {{\n{lets}        {writes} {{\n            usecase: {},\n            rows: {},\n            marks: {},\n            events: {},\n        }}\n    }}\n}}\n\n",
            f.result,
            string_literal(&f.usecase),
            list(rows),
            list(marks),
            list(events),
        ));
    }
    for d in &domain.dispatchers {
        out.push_str(&format!(
            "impl {into} for {} {{\n    fn into_writes(self) -> {writes} {{\n        match self {{\n",
            d.outcome
        ));
        for c in &d.cases {
            out.push_str(&format!(
                "            Self::{}(r) => r.into_writes(),\n",
                c.case
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
    }

    // CommitStore, CommitFailed, and the helper.
    out.push_str(&format!(
        "/// A store that commits a result, inside the transaction of the call: [`{commit}`] calls it\n/// once per write, in order. Each method writes one item only where it is still what it names.\npub trait {store} {{\n    /// The store's own error: a failure of the store, not an answer of the use case.\n    type Error;\n\n    /// Writes the row where it is still what its `expected` names: `Ok(false)`, and nothing\n    /// written, when it is not.\n    ///\n    /// # Errors\n    ///\n    /// The store's error when the store fails.\n    fn write_row(&mut self, write: {row_write}) -> {RESULT}<bool, Self::Error>;\n\n    /// Writes the run mark where the row's mark is still what the step needs: none of the\n    /// use case for a put at step 1, the step before for a later put, the mark's own step for\n    /// a delete. `Ok(false)`, and nothing written, when it is not.\n    ///\n    /// # Errors\n    ///\n    /// The store's error when the store fails.\n    fn write_mark(&mut self, write: {mark_write}) -> {RESULT}<bool, Self::Error>;\n\n    /// Records a raised event.\n    ///\n    /// # Errors\n    ///\n    /// The store's error when the store fails.\n    fn record_event(&mut self, event: {raised}) -> {RESULT}<(), Self::Error>;\n}}\n\n"
    ));
    out.push_str(&format!(
        "/// Why [`{commit}`] stopped. Either way the caller rolls the transaction back.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {failed}<E> {{\n    /// A row or a run mark is no longer what the result expects: another call changed it,\n    /// and the use case does not run from the state it left.\n    NotEnabled(NotEnabled),\n    /// The store failed.\n    Store(E),\n}}\n\n"
    ));
    out.push_str(&format!(
        "impl<E: ::core::fmt::Display> ::core::fmt::Display for {failed}<E> {{\n    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{\n        match self {{\n            Self::NotEnabled(e) => ::core::fmt::Display::fmt(e, f),\n            Self::Store(e) => write!(f, \"the store failed: {{e}}\"),\n        }}\n    }}\n}}\n\n"
    ));
    out.push_str(&format!(
        "impl<E: ::std::error::Error + 'static> ::std::error::Error for {failed}<E> {{\n    fn source(&self) -> {OPTION}<&(dyn ::std::error::Error + 'static)> {{\n        match self {{\n            Self::NotEnabled(e) => {OPTION}::Some(e),\n            Self::Store(e) => {OPTION}::Some(e),\n        }}\n    }}\n}}\n\n"
    ));
    out.push_str(&format!(
        "/// Commits `writes` through `store`: every row, then every run mark, then every event, and\n/// stops at the first row or mark that is no longer what it names. It begins and ends no\n/// transaction: call it inside the transaction of the call, and roll back on an error.\n///\n/// # Errors\n///\n/// `NotEnabled` when a row or a run mark is no longer what the result expects, and the store's\n/// error when the store fails.\npub fn {commit}<S: {store} + ?Sized>(store: &mut S, writes: impl {into}) -> {RESULT}<(), {failed}<S::Error>> {{\n    let writes = writes.into_writes();\n    let not_enabled = NotEnabled {{ usecase: writes.usecase }};\n    for write in writes.rows {{\n        if !store.write_row(write).map_err({failed}::Store)? {{\n            return {RESULT}::Err({failed}::NotEnabled(not_enabled));\n        }}\n    }}\n    for write in writes.marks {{\n        if !store.write_mark(write).map_err({failed}::Store)? {{\n            return {RESULT}::Err({failed}::NotEnabled(not_enabled));\n        }}\n    }}\n    for event in writes.events {{\n        store.record_event(event).map_err({failed}::Store)?;\n    }}\n    {RESULT}::Ok(())\n}}\n\n"
    ));
}

/// The case of an entity in `StoredRow` and `StoredId`: its type name.
fn case(s: &super::Stored<'_>) -> String {
    s.sum
        .map_or_else(|| s.row.to_string(), |sum| sum.name.clone())
}

/// `impl <ty> { fn entity(&self) }` over the stored entities, each case matched by `pattern`.
fn entity_fn(ty: &str, stored: &[super::Stored<'_>], pattern: &str) -> String {
    let mut out = format!(
        "impl {ty} {{\n    /// The entity of the row, as the model names it.\n    pub fn entity(&self) -> &'static str {{\n        match *self {{\n"
    );
    for s in stored {
        out.push_str(&format!(
            "            Self::{}{pattern} => {},\n",
            case(s),
            string_literal(s.entity)
        ));
    }
    out.push_str("        }\n    }\n}\n\n");
    out
}
