//! Query shapes declared on `reads(UseCase | Api, Entity)` and the indexes they want.

use super::{ConditionKey, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

/// `reads(Source, Entity).by(...).order(...).page(...)` with an optional `.count()` or
/// `.sum(column)`: what a read filters on, how it sorts, and how it pages.
#[derive(Debug, Clone)]
pub struct Query {
    /// The use case or API that reads.
    pub source: NodeRef,
    pub entity: EntityKey,
    /// `.by(a, b)`: columns the read compares for equality, as written.
    pub filter: Vec<String>,
    /// `.order(a, b.desc)`: the sort, first key first.
    pub order: Vec<OrderKey>,
    pub page: Option<Page>,
    pub aggregate: Option<Aggregate>,
    pub site: Option<crate::location::LocatedSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderKey {
    pub column: String,
    pub desc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Keyset paging: each page starts after the sort key of the last row.
    Cursor,
    Offset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aggregate {
    Count,
    Sum(String),
}

/// Fewer rows than this and a scan is as cheap as an index: `query-index-missing` keeps
/// quiet about an entity whose `rows` says so.
pub const SMALL_TABLE_ROWS: u64 = 10_000;

/// At least this many rows and the rows of one parent are many too: a `.has` / `.none` that
/// filters them wants the filter in the index, not the key alone.
pub const LARGE_TABLE_ROWS: u64 = 1_000_000;

/// What looks rows up, and so wants an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reader {
    /// `SemanticModel::queries[i]`: a declared `reads(...).by(...)`.
    Query(usize),
    /// A use case a timer starts, which moves every row at one of `from` of the entity: the
    /// rows are found by their state, not picked by a person.
    Batch {
        usecase: UseCaseKey,
        column: String,
        from: Vec<String>,
    },
    /// A `.has(...)` / `.none(...)` that reads the rows of one parent that match its
    /// comparisons: on a guard (`holds`) or on a rule, by the name it is written under.
    Quantifier { on: String },
    /// A `holds` body that compares an entity the guarded use cases neither write nor join
    /// by a key to a row they write: the guard looks for any row that matches, across the
    /// table.
    Guard {
        condition: ConditionKey,
        usecases: Vec<UseCaseKey>,
    },
}

/// An index that serves one or more readers, and that no key or `@index` already gives.
#[derive(Debug, Clone)]
pub struct IndexSuggestion {
    pub entity: EntityKey,
    /// Equality columns first, then the sort columns with their direction.
    pub columns: Vec<OrderKey>,
    /// How many leading columns are compared for equality; the rest are the sort.
    pub equal: usize,
    /// What it serves, declared reads first.
    pub readers: Vec<Reader>,
    /// The entity's `rows`, when it says.
    pub rows: Option<u64>,
}

impl IndexSuggestion {
    /// An entity declared smaller than `SMALL_TABLE_ROWS`: a scan is cheap.
    pub fn is_small(&self) -> bool {
        self.rows.is_some_and(|n| n < SMALL_TABLE_ROWS)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub column: String,
    pub field: String,
}

impl Query {
    /// The columns a read by an API selects: each field of its `response` DTOs that a
    /// `maps_field(Dto.field, Entity.column)` maps to a column of the entity read, in field
    /// order. Empty when the reader is a use case or no field maps, which selects every column.
    pub fn selected(&self, model: &SemanticModel) -> Vec<Selected> {
        let mut out: Vec<Selected> = Vec::new();
        for rel in &model.relations {
            if rel.kind != RelKind::Response || rel.from != self.source {
                continue;
            }
            let NodeRef::Dto(dto) = rel.to else {
                continue;
            };
            for field in &model.dtos[dto].fields {
                let mapped = model
                    .dto_mappings
                    .iter()
                    .find(|m| m.dto == dto && m.field == field.name && m.entity == self.entity);
                if let Some(m) = mapped {
                    let s = Selected {
                        column: m.column.clone(),
                        field: field.name.clone(),
                    };
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// The index this query wants: the tenant column, the `.by` columns, then the sort, and
    /// for cursor paging the primary key columns the sort does not name, so the key is
    /// unique. Empty when the query neither filters nor sorts.
    pub fn wanted_index(&self, model: &SemanticModel) -> (Vec<String>, Vec<OrderKey>) {
        let entity = &model.entities[self.entity];
        let mut equal: Vec<String> = Vec::new();
        if !self.filter.is_empty() || !self.order.is_empty() {
            if let Some(tenant) = entity.columns.iter().find(|c| c.is_tenant_scope) {
                equal.push(tenant.name.clone());
            }
        }
        for column in &self.filter {
            if !equal.contains(column) {
                equal.push(column.clone());
            }
        }
        let mut order: Vec<OrderKey> = self
            .order
            .iter()
            .filter(|k| !equal.contains(&k.column))
            .cloned()
            .collect();
        if self.page == Some(Page::Cursor) && !order.is_empty() {
            let desc = order.last().is_some_and(|k| k.desc);
            for column in &entity.primary_key {
                if !equal.contains(column) && !order.iter().any(|k| &k.column == column) {
                    order.push(OrderKey {
                        column: column.clone(),
                        desc,
                    });
                }
            }
        }
        (equal, order)
    }
}

impl IndexSuggestion {
    /// Whether the sort mixes directions, which only an index with the same mix serves.
    pub fn mixes_directions(&self) -> bool {
        let sort = &self.columns[self.equal..];
        sort.iter().any(|k| k.desc) && sort.iter().any(|k| !k.desc)
    }
}
