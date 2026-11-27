//! What a result commits, as data (docs/design/algebraic-codegen.md, "Committing a result"):
//! the model-wide types of a row, an id, a run mark, and an event of any kind, and per result
//! type the rows, marks, and events it writes. Computed here once, so the three printers list
//! the same writes in the same order.

pub(super) mod go;
pub(super) mod rust;
pub(super) mod ts;

use super::ir::{DomainModel, Field, FieldType, FnDecl, OutKind, RecordKind, SumDecl};

/// The names the commit part declares, claimed after the model's names (`_` appended on a
/// collision): the types, Rust's trait and error, Go's interface, and the helper.
const NAMES: [&str; 12] = [
    "StoredRow",
    "StoredId",
    "RowWrite",
    "RunMark",
    "MarkWrite",
    "RaisedEvent",
    "Writes",
    "IntoWrites",
    "Committable",
    "CommitStore",
    "CommitFailed",
    "CommitWrites",
];

/// The derived names of the commit part, for every language.
pub(super) fn derived_names() -> Vec<super::DerivedName> {
    NAMES
        .iter()
        .map(|n| super::DerivedName {
            key: format!("writes:{n}"),
            name: (*n).to_string(),
            by: format!("the commit type {n}"),
        })
        .collect()
}

/// The name claimed for `base`, one of [`NAMES`].
pub(super) fn name(domain: &DomainModel, base: &str) -> String {
    domain
        .derived(&format!("writes:{base}"))
        .unwrap_or(base)
        .to_string()
}

/// An entity as a store holds it.
pub(super) struct Stored<'d> {
    /// The entity's id, as the model spells it.
    pub entity: &'d str,
    /// Its row type: `<Entity>Row`, or the record itself.
    pub row: &'d str,
    /// The sum of a state type, `None` for a record.
    pub sum: Option<&'d SumDecl>,
    /// The id type and the row field that holds it; `None` when the entity has no
    /// one-column key.
    pub key: Option<(&'d str, &'d Field)>,
}

/// The run marks of one split use case on one entity.
pub(super) struct Mark<'d> {
    /// `<UseCase><Entity>`: the case of the mark types.
    pub case: String,
    pub usecase: &'d str,
    pub entity: &'d str,
    pub id_ty: &'d str,
}

/// Every entity, run mark, and event a commit can write.
pub(super) struct Model<'d> {
    pub stored: Vec<Stored<'d>>,
    pub marks: Vec<Mark<'d>>,
    pub events: Vec<&'d str>,
}

/// One write of a result, by its result field.
pub(super) enum Item {
    /// The row in `field` (a state or a record of `stored`) is stored where it is still what
    /// `expected` names.
    Put {
        field: String,
        expected: String,
        stored: usize,
    },
    /// The row whose id `field` holds is deleted; `None` when its entity has no id type.
    Delete {
        field: Option<String>,
        expected: String,
        stored: usize,
    },
    /// The mark in `field` (of `marks[mark]`, at `step`) is stored or advanced, or deleted.
    Mark {
        field: String,
        mark: usize,
        step: u8,
        delete: bool,
    },
    /// The event in `field`, of type `ty`, is raised.
    Event { field: String, ty: String },
}

/// The entities, marks, and events of `domain`.
pub(super) fn model<'d>(domain: &'d DomainModel) -> Model<'d> {
    fn key<'d>(
        domain: &'d DomainModel,
        entity: &str,
        fields: &'d [Field],
    ) -> Option<(&'d str, &'d Field)> {
        let id = domain.ids.iter().find(|i| i.entity == entity)?;
        let field = fields
            .iter()
            .find(|f| f.ty == FieldType::Id(id.name.clone()))?;
        Some((id.name.as_str(), field))
    }
    let key = |entity: &str, fields: &'d [Field]| key(domain, entity, fields);
    let mut stored: Vec<Stored<'_>> = domain
        .records
        .iter()
        .filter(|r| r.kind == RecordKind::Entity)
        .map(|r| Stored {
            entity: &r.id,
            row: &r.name,
            sum: None,
            key: key(&r.id, &r.fields),
        })
        .collect();
    stored.extend(domain.sums.iter().map(|s| Stored {
        entity: &s.entity,
        row: &s.row,
        sum: Some(s),
        key: key(&s.entity, &s.row_fields),
    }));
    let mut marks: Vec<Mark<'_>> = Vec::new();
    for m in &domain.marks {
        if !marks
            .iter()
            .any(|k| k.usecase == m.usecase && k.entity == m.entity)
        {
            marks.push(Mark {
                case: format!(
                    "{}{}",
                    super::type_ident(&m.usecase),
                    super::type_ident(&m.entity)
                ),
                usecase: &m.usecase,
                entity: &m.entity,
                id_ty: &m.id_ty,
            });
        }
    }
    Model {
        stored,
        marks,
        events: domain.events.iter().map(|e| e.name.as_str()).collect(),
    }
}

/// The writes of the result of `f`, in order: rows in the order of its fields, then marks,
/// then events.
pub(super) fn items(domain: &DomainModel, model: &Model<'_>, f: &FnDecl) -> Vec<Item> {
    let stored_of = |entity: &str| model.stored.iter().position(|s| s.entity == entity);
    let mark_of = |ty: &str| {
        let decl = domain.marks.iter().find(|m| m.name == ty)?;
        let at = model
            .marks
            .iter()
            .position(|k| k.usecase == decl.usecase && k.entity == decl.entity)?;
        Some((at, decl.step))
    };
    let mut items = Vec::new();
    for o in &f.outputs {
        let expected = o.expected_name();
        match &o.kind {
            OutKind::Row { ty, .. } => {
                let entity = domain
                    .sums
                    .iter()
                    .find(|s| s.variants.iter().any(|v| v.name == *ty))
                    .map(|s| s.entity.as_str())
                    .or_else(|| {
                        domain
                            .records
                            .iter()
                            .find(|r| r.name == *ty)
                            .map(|r| r.id.as_str())
                    });
                let stored = entity
                    .and_then(stored_of)
                    .expect("a written row is of an entity's state or record");
                items.push(Item::Put {
                    field: o.name.clone(),
                    expected,
                    stored,
                });
            }
            OutKind::Deleted { row, id } => {
                let stored = stored_of(&f.row(*row).entity).expect("a deleted row is of an entity");
                items.push(Item::Delete {
                    field: id.as_ref().map(|_| o.name.clone()),
                    expected,
                    stored,
                });
            }
        }
    }
    for m in &f.marks_out {
        let (mark, _) = mark_of(&m.ty).expect("a mark a step leaves is declared");
        items.push(Item::Mark {
            field: m.name.clone(),
            mark,
            step: m.step,
            delete: false,
        });
    }
    for (m, field) in f.marks_ended() {
        let (mark, step) = mark_of(&m.ty).expect("a mark a step takes is declared");
        items.push(Item::Mark {
            field,
            mark,
            step,
            delete: true,
        });
    }
    for e in &f.raises {
        items.push(Item::Event {
            field: super::snake(&e.ty),
            ty: e.ty.clone(),
        });
    }
    items
}
