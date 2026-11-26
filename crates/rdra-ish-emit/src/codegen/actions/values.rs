//! Literal and input helpers `lower` shares: what a model value writes as, the start value of a
//! state tag, and the inputs a function gains.

use super::*;

/// What an effect writes to a column.
#[derive(Clone, Debug)]
pub(super) enum Write {
    Lit(Lit),
    /// An enum value; its type is the target field's.
    Enum(EnumValue),
    Null,
    /// A second lifecycle column: `(from, to)` per arm.
    Switch(Vec<(String, String)>),
    /// A value the model leaves open: the caller passes it.
    Input,
    /// A value the model leaves open, null among them: the caller passes it, or null.
    InputOrNull,
    /// The key of the parent param, by its key column.
    Parent(usize, String),
    /// The value of another param's column (`sets(UC, E.column == F.source)`).
    Copy(usize, String),
}

/// The write of a literal value of `domain`.
pub(super) fn literal(domain: &Domain, value: Value) -> Option<Write> {
    Some(match domain {
        Domain::NullableEnum(_) | Domain::NullableBool | Domain::Nullable
            if value == Value::NULL =>
        {
            Write::Null
        }
        Domain::Enum(_) | Domain::NullableEnum(_) => {
            let wire = domain.name(value)?.to_string();
            Write::Enum(EnumValue {
                ident: type_ident(&wire),
                wire,
                label: None,
            })
        }
        Domain::Bool | Domain::NullableBool => Write::Lit(Lit::Bool(domain.truth(value)?)),
        Domain::Nullable => Write::Input,
        Domain::Int | Domain::NullableInt => match domain.number(value) {
            Some(n) => Write::Lit(Lit::Int(n)),
            None => Write::Null,
        },
        Domain::Existence | Domain::Instance { .. } => return None,
    })
}

/// The start value of a created row's column, by its var: `Some(Some(lit))`, `Some(None)` for
/// null, `None` when no var holds the column (the caller passes it).
pub(super) fn start_literal(
    ts: &TransitionSystem,
    e: EntId,
    column: &str,
    target: &Field,
) -> Option<Option<Lit>> {
    let var = ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Column { entity, column: c, .. } if *entity == e && c == column)
    })?;
    let decl = ts.var(var);
    match literal(&decl.domain, decl.base)? {
        Write::Enum(value) => Some(Some(enum_lit(value, target)?)),
        Write::Lit(lit) => Some(Some(lit)),
        Write::Null => Some(None),
        Write::Input
        | Write::InputOrNull
        | Write::Parent(..)
        | Write::Copy(..)
        | Write::Switch(_) => None,
    }
}

/// The start value of a created row's tag.
pub(super) fn start_value(ts: &TransitionSystem, e: EntId, tag: &str) -> Option<String> {
    let var = ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Column { entity, column, .. } if *entity == e && column == tag)
    })?;
    let decl = ts.var(var);
    decl.domain.name(decl.base).map(str::to_string)
}

/// The id type and id field of an entity's type, when it has one.
pub(super) fn declared_id(entity: &str, fields: &[Field]) -> Option<(String, Field)> {
    fields.iter().find_map(|f| match &f.ty {
        FieldType::Id(ty) if *ty == format!("{}Id", type_ident(entity)) => {
            Some((ty.clone(), f.clone()))
        }
        _ => None,
    })
}

/// An enum value as a literal of its target field's enum type; none when the field is not an enum.
pub(super) fn enum_lit(value: EnumValue, target: &Field) -> Option<Lit> {
    match &target.ty {
        FieldType::Enum(ty) => Some(Lit::Enum {
            ty: ty.clone(),
            value,
        }),
        _ => None,
    }
}

/// The input named `name`, added when the function has none by that name yet.
pub(super) fn input_index(
    inputs: &mut Vec<FnInput>,
    name: String,
    ty: InputType,
    doc: String,
) -> usize {
    if let Some(i) = inputs.iter().position(|x| x.name == name) {
        return i;
    }
    inputs.push(FnInput {
        name,
        ty,
        nullable: false,
        doc,
    });
    inputs.len() - 1
}
