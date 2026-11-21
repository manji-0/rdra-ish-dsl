//! Literal and input helpers `lower` shares: what a model value writes as, the start value of a
//! state tag, and the inputs a function gains.

use super::*;

/// What an effect writes to a column.
#[derive(Clone, Debug)]
pub(super) enum Write {
    Lit(Lit),
    Null,
    /// A second lifecycle column: `(from, to)` per arm.
    Switch(Vec<(String, String)>),
    /// A value the model leaves open: the caller passes it.
    Input,
    /// The key of the parent param, by its key column.
    Parent(usize, String),
}

/// The write of a literal value of `domain`.
pub(super) fn literal(domain: &Domain, value: Value) -> Option<Write> {
    Some(match domain {
        Domain::Enum(values) => {
            let wire = values.get(usize::from(value.0))?.clone();
            Write::Lit(Lit::Enum {
                ty: String::new(),
                value: EnumValue {
                    ident: type_ident(&wire),
                    wire,
                    label: None,
                },
            })
        }
        Domain::Bool => Write::Lit(Lit::Bool(value == Value::TRUE)),
        Domain::Nullable if value == Value::NULL => Write::Null,
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
        Write::Lit(Lit::Enum { value, .. }) => {
            let FieldType::Enum(ty) = &target.ty else {
                return None;
            };
            Some(Some(Lit::Enum {
                ty: ty.clone(),
                value,
            }))
        }
        Write::Lit(lit) => Some(Some(lit)),
        Write::Null => Some(None),
        Write::Input | Write::Parent(..) | Write::Switch(_) => None,
    }
}

/// The start value of a created row's tag.
pub(super) fn start_value(ts: &TransitionSystem, e: EntId, tag: &str) -> Option<String> {
    let var = ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Column { entity, column, .. } if *entity == e && column == tag)
    })?;
    let decl = ts.var(var);
    match &decl.domain {
        Domain::Enum(values) => values.get(usize::from(decl.base.0)).cloned(),
        _ => None,
    }
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

/// A literal typed for its target field: an enum value takes the field's enum type.
pub(super) fn typed(lit: &Lit, target: &Field) -> Option<Lit> {
    match (lit, &target.ty) {
        (Lit::Enum { value, .. }, FieldType::Enum(ty)) => Some(Lit::Enum {
            ty: ty.clone(),
            value: value.clone(),
        }),
        (Lit::Enum { .. }, _) => None,
        (other, _) => Some(other.clone()),
    }
}

/// The input named `name`, added when the function has none by that name yet.
pub(super) fn input_index(
    inputs: &mut Vec<FnInput>,
    name: String,
    ty: FieldType,
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
