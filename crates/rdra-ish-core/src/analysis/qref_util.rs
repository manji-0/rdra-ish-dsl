//! Qualified reference display helpers shared by comparison and constraint logic.

use rdra_ish_syntax::ast::{QRef, QualifiedColumnRef};

pub(crate) fn qref_id(qref: &QRef) -> Option<String> {
    if qref.parts.len() == 1 {
        Some(qref.parts[0].clone())
    } else {
        None
    }
}

pub(crate) fn qref_display(qref: &QRef) -> String {
    let id = qref.parts.join(".");
    match &qref.kind_qualifier {
        Some(kind) => format!("{}::{}", kind.name(), id),
        None => id,
    }
}

pub(crate) fn qualified_column_display(qcol: &QualifiedColumnRef) -> String {
    format!("{}.{}", qref_display(&qcol.entity), qcol.column)
}
