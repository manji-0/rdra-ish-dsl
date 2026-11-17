//! The reads a model declares with `reads(...).by(...).order(...)`: the indexes they want
//! and the `SELECT` each one is, as comments after the schema.

use super::{ident, idents, live_condition, Dialect, Script};
use rdra_ish_core::model::{Aggregate, OrderKey, Page, Query};

impl Script<'_> {
    pub(super) fn reads_section(&self) -> String {
        if self.model.queries.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        let suggestions = self.model.index_suggestions();
        if !suggestions.is_empty() {
            out.push_str(
                "\n-- Indexes the declared reads want and no key or @index gives (lint: query-index-missing).\n",
            );
            for s in suggestions {
                let entity = &self.model.entities[s.entity];
                let names: Vec<String> = s.columns.iter().map(|k| k.column.clone()).collect();
                let readers: Vec<&str> = s
                    .queries
                    .iter()
                    .filter_map(|&i| self.model.node_id(&self.model.queries[i].source))
                    .collect();
                out.push_str(&format!(
                    "-- CREATE INDEX {} ON {} ({}); -- {}\n",
                    ident(&format!("{}_{}_idx", entity.table_name(), names.join("_"))),
                    ident(entity.table_name()),
                    sorted(&s.columns),
                    readers.join(", ")
                ));
            }
        }
        out.push_str("\n-- Reads the model declares, as SQL.\n");
        for query in &self.model.queries {
            let source = self.model.node_id(&query.source).unwrap_or("?");
            let entity = &self.model.entities[query.entity];
            out.push_str(&format!("-- {source} reads {}:\n", entity.id));
            out.push_str(&format!("--   {}\n", self.select(query)));
        }
        out
    }

    /// The statement a read runs: the tenant and `.by` columns compared to parameters, live
    /// rows only on a soft-deleted entity, then the sort and the page.
    fn select(&self, query: &Query) -> String {
        let entity = &self.model.entities[query.entity];
        let (equal, order) = query.wanted_index(self.model);
        let mut n = 0;
        let mut param = || {
            n += 1;
            match self.dialect {
                Dialect::Postgres => format!("${n}"),
                Dialect::Sqlite => format!("?{n}"),
            }
        };
        let mut conds: Vec<String> = equal
            .iter()
            .map(|c| format!("{} = {}", ident(c), param()))
            .collect();
        if let Some(live) = entity
            .columns
            .iter()
            .find(|c| c.is_soft_delete)
            .and_then(live_condition)
        {
            conds.push(live);
        }
        if query.page == Some(Page::Cursor) {
            conds.push(keyset(&order, &mut param));
        }
        let what = match &query.aggregate {
            None => "*".to_string(),
            Some(Aggregate::Count) => "count(*)".to_string(),
            Some(Aggregate::Sum(c)) => format!("sum({})", ident(c)),
        };
        let mut sql = format!("SELECT {what} FROM {}", ident(entity.table_name()));
        if !conds.is_empty() {
            sql.push_str(&format!(" WHERE {}", conds.join(" AND ")));
        }
        if !order.is_empty() {
            sql.push_str(&format!(" ORDER BY {}", sorted(&order)));
        }
        match query.page {
            Some(Page::Cursor) => sql.push_str(&format!(" LIMIT {}", param())),
            Some(Page::Offset) => {
                let limit = param();
                sql.push_str(&format!(" LIMIT {limit} OFFSET {}", param()));
            }
            None => {}
        }
        sql.push(';');
        sql
    }
}

/// `"a", "b" DESC`
fn sorted(keys: &[OrderKey]) -> String {
    keys.iter()
        .map(|k| {
            if k.desc {
                format!("{} DESC", ident(&k.column))
            } else {
                ident(&k.column)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The rows after the last one of the previous page: a row comparison when the sort runs
/// one way, else one disjunct per key.
fn keyset(order: &[OrderKey], param: &mut impl FnMut() -> String) -> String {
    let op = |k: &OrderKey| if k.desc { "<" } else { ">" };
    let values: Vec<String> = order.iter().map(|_| param()).collect();
    if order.iter().all(|k| k.desc == order[0].desc) {
        let cols: Vec<String> = order.iter().map(|k| k.column.clone()).collect();
        return if order.len() == 1 {
            format!("{} {} {}", ident(&cols[0]), op(&order[0]), values[0])
        } else {
            format!(
                "({}) {} ({})",
                idents(&cols),
                op(&order[0]),
                values.join(", ")
            )
        };
    }
    let terms: Vec<String> = (0..order.len())
        .map(|i| {
            let mut parts: Vec<String> = (0..i)
                .map(|j| format!("{} = {}", ident(&order[j].column), values[j]))
                .collect();
            parts.push(format!(
                "{} {} {}",
                ident(&order[i].column),
                op(&order[i]),
                values[i]
            ));
            parts.join(" AND ")
        })
        .collect();
    format!("({})", terms.join(" OR "))
}
