//! The index advisor: the lookups each reader makes, and the indexes that would serve them.

use crate::model::*;

/// One lookup a reader makes: the entity, its equality columns, its sort, and, for a
/// lookup through a key, the key columns an index must lead with to serve it alone.
struct Want {
    reader: Reader,
    entity: EntityKey,
    equal: Vec<String>,
    order: Vec<OrderKey>,
    key: Option<Vec<String>>,
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

    /// The name a reader goes by: a use case's id, or the rule or condition a quantifier is
    /// written on.
    pub fn reader_name(&self, reader: &Reader) -> String {
        match reader {
            Reader::Query(i) => self
                .node_id(&self.queries[*i].source)
                .unwrap_or("?")
                .to_string(),
            Reader::Batch { usecase, .. } => self.use_cases[*usecase].id.clone(),
            Reader::Quantifier { on } => on.clone(),
            Reader::Guard { condition, .. } => self.conditions[*condition].id.clone(),
        }
    }

    /// Where a reader is written, when the model knows.
    pub fn reader_site(&self, reader: &Reader) -> Option<crate::location::LocatedSpan> {
        match reader {
            Reader::Query(i) => self.queries[*i].site.clone(),
            Reader::Batch {
                usecase, column, ..
            } => self
                .state_transitions
                .iter()
                .find(|t| t.usecase == *usecase && t.column == *column)
                .and_then(|t| t.site.clone()),
            Reader::Quantifier { .. } => None,
            Reader::Guard { condition, .. } => self.conditions[*condition]
                .body
                .as_ref()
                .and_then(|b| b.site.clone()),
        }
    }

    /// Every lookup that wants an index: each declared read, each state a timer's use case
    /// moves rows off, and each `.has` / `.none` on a guard or a rule.
    fn wants(&self) -> Vec<Want> {
        let mut wants: Vec<Want> = self
            .queries
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let (equal, order) = q.wanted_index(self);
                Want {
                    reader: Reader::Query(i),
                    entity: q.entity,
                    equal,
                    order,
                    key: None,
                }
            })
            .collect();
        let timer_started: Vec<UseCaseKey> = self
            .relations
            .iter()
            .filter_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
                (RelKind::Triggers, NodeRef::Timer(_), NodeRef::UseCase(uc)) => Some(*uc),
                _ => None,
            })
            .collect();
        for t in &self.state_transitions {
            if !timer_started.contains(&t.usecase) {
                continue;
            }
            // The arms of one use case on one column are one lookup, by every state they leave.
            let same = |w: &&mut Want| {
                matches!(&w.reader, Reader::Batch { usecase, column, .. }
                    if *usecase == t.usecase && *column == t.column)
            };
            if let Some(w) = wants.iter_mut().find(same) {
                if let Reader::Batch { from, .. } = &mut w.reader {
                    if !from.contains(&t.from) {
                        from.push(t.from.clone());
                    }
                }
                continue;
            }
            let reader = Reader::Batch {
                usecase: t.usecase,
                column: t.column.clone(),
                from: vec![t.from.clone()],
            };
            let mut equal: Vec<String> = self.entities[t.entity]
                .columns
                .iter()
                .filter(|c| c.is_tenant_scope)
                .map(|c| c.name.clone())
                .collect();
            if !equal.contains(&t.column) {
                equal.push(t.column.clone());
            }
            wants.push(Want {
                reader,
                entity: t.entity,
                equal,
                order: Vec::new(),
                key: None,
            });
        }
        self.guard_wants(&mut wants);
        let mut quantified = Vec::new();
        for c in self.conditions.values() {
            if let Some(body) = &c.body {
                let from: Vec<EntityKey> = body.conds.iter().filter_map(condition_entity).collect();
                for q in &body.quantifiers {
                    quantified.push((c.id.clone(), from.clone(), q));
                }
            }
        }
        for (i, rule) in self.quantifier_constraints.iter().enumerate() {
            let name = format!("Quantifier_{}_{i}", self.entities[rule.anchor].id);
            quantified.push((name, vec![rule.anchor], &rule.quantifier));
        }
        for (on, from, q) in quantified {
            // The related rows of one row of `from`, through the key the related entity holds;
            // a quantifier over a parent reads one row by its primary key.
            let Some(fk) = self
                .foreign_keys()
                .map(|(_, fk)| fk)
                .find(|fk| fk.child == q.related && from.contains(&fk.parent))
            else {
                continue;
            };
            let key: Vec<String> = fk.columns.iter().map(|(c, _)| c.clone()).collect();
            let mut equal = key.clone();
            for cond in &q.conditions {
                if let CrossEntityCondition::Equals { column, .. } = cond {
                    if column.entity == q.related && !equal.contains(&column.column) {
                        equal.push(column.column.clone());
                    }
                }
            }
            if equal.len() == key.len() {
                continue;
            }
            wants.push(Want {
                reader: Reader::Quantifier { on },
                entity: q.related,
                equal,
                order: Vec::new(),
                key: Some(key),
            });
        }
        wants
    }

    /// The global reads of guards: a `holds` body without `.along` compares an entity the
    /// use case neither writes nor joins by one key to a row it writes, so the guard looks
    /// for any row at the compared values. It wants the tenant column and the columns the
    /// body compares with `==`; one want per condition and entity, for every use case it
    /// guards that way.
    fn guard_wants(&self, wants: &mut Vec<Want>) {
        let txs = crate::tx::infer_usecase_transactions(self);
        let keys: Vec<(EntityKey, EntityKey)> = crate::tx::fk_edges(self).collect();
        for b in crate::analysis::applied_bodies(self) {
            if !matches!(b.body.along, CrossConstraintScope::GlobalProduct) {
                continue;
            }
            let bound: Vec<EntityKey> = txs
                .iter()
                .filter(|t| t.usecase == b.usecase)
                .flat_map(|t| t.writes().map(|w| w.entity))
                .collect();
            let joined = |ek: EntityKey| {
                bound.contains(&ek)
                    || keys.iter().any(|&(parent, child)| {
                        (parent == ek && bound.contains(&child))
                            || (child == ek && bound.contains(&parent))
                    })
            };
            let mut read: Vec<EntityKey> = Vec::new();
            for ek in b.body.conds.iter().filter_map(condition_entity) {
                if !read.contains(&ek) {
                    read.push(ek);
                }
            }
            for ek in read.into_iter().filter(|ek| !joined(*ek)) {
                let mut equal: Vec<String> = self.entities[ek]
                    .columns
                    .iter()
                    .filter(|c| c.is_tenant_scope)
                    .map(|c| c.name.clone())
                    .collect();
                let scoped = equal.len();
                for cond in &b.body.conds {
                    if let CrossEntityCondition::Equals { column, .. } = cond {
                        if column.entity == ek && !equal.contains(&column.column) {
                            equal.push(column.column.clone());
                        }
                    }
                }
                if equal.len() == scoped {
                    continue;
                }
                let same = |w: &&mut Want| {
                    w.entity == ek
                        && matches!(&w.reader, Reader::Guard { condition, .. } if *condition == b.cond)
                };
                if let Some(w) = wants.iter_mut().find(same) {
                    if let Reader::Guard { usecases, .. } = &mut w.reader {
                        if !usecases.contains(&b.usecase) {
                            usecases.push(b.usecase);
                        }
                    }
                    continue;
                }
                wants.push(Want {
                    reader: Reader::Guard {
                        condition: b.cond,
                        usecases: vec![b.usecase],
                    },
                    entity: ek,
                    equal,
                    order: Vec::new(),
                    key: None,
                });
            }
        }
    }

    /// The indexes the lookups want and the schema does not have: declared reads first, in
    /// query order, then batch moves and quantifiers. A lookup an existing index or another
    /// suggestion serves adds none. A quantifier reads one parent's rows, which an index
    /// that leads with its key serves, unless the related entity declares
    /// `LARGE_TABLE_ROWS` or more.
    pub fn index_suggestions(&self) -> Vec<IndexSuggestion> {
        let wants = self.wants();
        let mut out: Vec<IndexSuggestion> = Vec::new();
        // Longest first, so a shorter shape lands on the index a longer one asks for.
        let mut order: Vec<usize> = (0..wants.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(wants[i].equal.len() + wants[i].order.len()));
        for i in order {
            let w = &wants[i];
            if w.equal.is_empty() && w.order.is_empty() {
                continue;
            }
            let rows = self.entities[w.entity].rows;
            let existing = self.existing_indexes(w.entity);
            let by_key = w.key.as_ref().is_some_and(|key| {
                rows.is_none_or(|n| n < LARGE_TABLE_ROWS)
                    && existing.iter().any(|idx| {
                        idx.len() >= key.len()
                            && idx[..key.len()].iter().all(|k| key.contains(&k.column))
                    })
            });
            if by_key || existing.iter().any(|idx| serves(idx, &w.equal, &w.order)) {
                continue;
            }
            if let Some(s) = out
                .iter_mut()
                .find(|s| s.entity == w.entity && serves(&s.columns, &w.equal, &w.order))
            {
                s.readers.push(w.reader.clone());
                continue;
            }
            let mut columns: Vec<OrderKey> = w
                .equal
                .iter()
                .map(|c| OrderKey {
                    column: c.clone(),
                    desc: false,
                })
                .collect();
            columns.extend(w.order.iter().cloned());
            out.push(IndexSuggestion {
                entity: w.entity,
                columns,
                equal: w.equal.len(),
                readers: vec![w.reader.clone()],
                rows,
            });
        }
        let rank = |r: &Reader| {
            wants
                .iter()
                .position(|w| w.reader == *r)
                .unwrap_or(usize::MAX)
        };
        for s in &mut out {
            s.readers.sort_by_key(rank);
        }
        out.sort_by_key(|s| rank(&s.readers[0]));
        out
    }
}

/// The entity a guard comparison reads.
fn condition_entity(cond: &CrossEntityCondition) -> Option<EntityKey> {
    match cond {
        CrossEntityCondition::Equals { column, .. } => Some(column.entity),
        CrossEntityCondition::Comparison(c) => Some(c.lhs.entity),
        CrossEntityCondition::Mark { .. } => None,
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
