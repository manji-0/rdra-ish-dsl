//! Query shapes declared on `reads(UseCase | Api, Entity)` and the indexes they want.

use super::{EntityKey, NodeRef, RelKind, SemanticModel};

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

/// An index that serves one or more queries, and that no key or `@index` already gives.
#[derive(Debug, Clone)]
pub struct IndexSuggestion {
    pub entity: EntityKey,
    /// Equality columns first, then the sort columns with their direction.
    pub columns: Vec<OrderKey>,
    /// How many leading columns are compared for equality; the rest are the sort.
    pub equal: usize,
    /// Indices into `SemanticModel::queries` it serves.
    pub queries: Vec<usize>,
}

/// A column a read selects, and the response field that carries it.
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

/// Whether an index over `index` (each column with its direction) serves a query that
/// compares `equal` and sorts by `order`: its leading columns are the equality set in any
/// order, the next ones the sort. Scanned backward, an index serves the reverse sort too.
fn serves(index: &[OrderKey], equal: &[String], order: &[OrderKey]) -> bool {
    let n = equal.len();
    if index.len() < n + order.len() {
        return false;
    }
    if !index[..n].iter().all(|k| equal.contains(&k.column)) {
        return false;
    }
    let tail = &index[n..n + order.len()];
    if tail.iter().zip(order).any(|(i, o)| i.column != o.column) {
        return false;
    }
    let same = tail.iter().zip(order).all(|(i, o)| i.desc == o.desc);
    let reversed = tail.iter().zip(order).all(|(i, o)| i.desc != o.desc);
    same || reversed
}

impl IndexSuggestion {
    /// Whether the sort mixes directions, which only an index with the same mix serves.
    pub fn mixes_directions(&self) -> bool {
        let sort = &self.columns[self.equal..];
        sort.iter().any(|k| k.desc) && sort.iter().any(|k| !k.desc)
    }
}

impl SemanticModel {
    /// Column lists an entity's schema indexes: its primary key, each unique set, each
    /// `@index`, and each foreign key, all ascending.
    fn existing_indexes(&self, entity: EntityKey) -> Vec<Vec<OrderKey>> {
        let asc = |cols: &[String]| {
            cols.iter()
                .map(|c| OrderKey {
                    column: c.clone(),
                    desc: false,
                })
                .collect::<Vec<_>>()
        };
        let e = &self.entities[entity];
        let mut sets = vec![asc(&e.primary_key)];
        sets.extend(self.unique_keys(entity).iter().map(|u| asc(&u.columns)));
        sets.extend(e.indexes.iter().map(|i| asc(i)));
        for (_, fk) in self.foreign_keys() {
            if fk.child == entity {
                let cols: Vec<String> = fk.columns.iter().map(|(c, _)| c.clone()).collect();
                sets.push(asc(&cols));
            }
        }
        sets
    }

    /// The indexes the declared query shapes want and the schema does not have, in query
    /// order. A query an existing index or another suggestion serves adds none.
    pub fn index_suggestions(&self) -> Vec<IndexSuggestion> {
        let mut out: Vec<IndexSuggestion> = Vec::new();
        let wanted: Vec<(Vec<String>, Vec<OrderKey>)> =
            self.queries.iter().map(|q| q.wanted_index(self)).collect();
        // Longest first, so a shorter shape lands on the index a longer one asks for.
        let mut order: Vec<usize> = (0..self.queries.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(wanted[i].0.len() + wanted[i].1.len()));
        for i in order {
            let (equal, sort) = &wanted[i];
            if equal.is_empty() && sort.is_empty() {
                continue;
            }
            let entity = self.queries[i].entity;
            if self
                .existing_indexes(entity)
                .iter()
                .any(|idx| serves(idx, equal, sort))
            {
                continue;
            }
            if let Some(s) = out
                .iter_mut()
                .find(|s| s.entity == entity && serves(&s.columns, equal, sort))
            {
                s.queries.push(i);
                continue;
            }
            let mut columns: Vec<OrderKey> = equal
                .iter()
                .map(|c| OrderKey {
                    column: c.clone(),
                    desc: false,
                })
                .collect();
            columns.extend(sort.iter().cloned());
            out.push(IndexSuggestion {
                entity,
                columns,
                equal: equal.len(),
                queries: vec![i],
            });
        }
        for s in &mut out {
            s.queries.sort_unstable();
        }
        out.sort_by_key(|s| s.queries[0]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(spec: &[(&str, bool)]) -> Vec<OrderKey> {
        spec.iter()
            .map(|(c, d)| OrderKey {
                column: c.to_string(),
                desc: *d,
            })
            .collect()
    }

    #[test]
    fn an_index_serves_its_equality_set_in_any_order_then_the_sort() {
        let idx = keys(&[("b", false), ("a", false), ("t", false), ("id", false)]);
        let eq = vec!["a".to_string(), "b".to_string()];
        assert!(serves(&idx, &eq, &keys(&[("t", false)])));
        assert!(serves(&idx, &eq, &keys(&[("t", true), ("id", true)])));
        assert!(!serves(&idx, &eq, &keys(&[("t", true), ("id", false)])));
        assert!(!serves(&idx, &eq, &keys(&[("id", false)])));
        assert!(!serves(&idx, &["t".to_string()], &[]));
    }
}
