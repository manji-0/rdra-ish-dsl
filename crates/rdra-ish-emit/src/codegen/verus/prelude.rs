//! `prelude.rs`: per entity, a list of rows as the map of rows it stands for, and the lemmas
//! that a quantifier or a count over the list is the one over the map. The text is the same
//! for every model but the entity's names; nothing in it reads a rule or an action.

use super::{Ctx, MTy};

pub(super) fn print(ctx: &Ctx) -> String {
    let mut out = String::from(
        "use vstd::prelude::*;\nuse crate::types::*;\nuse crate::model::*;\n\nverus! {\n\n",
    );
    for e in &ctx.ents {
        let fields: Vec<String> = e
            .fields
            .iter()
            .map(|f| match f.ty {
                MTy::Key { .. } => format!("{0}: r.{0}.0 as int", f.name),
                _ => format!("{0}: r.{0}", f.name),
            })
            .collect();
        out.push_str(
            &TEMPLATE
                .replace("{e}", &e.snake)
                .replace("{M}", &e.m)
                .replace("{Row}", &e.row)
                .replace("{view}", &format!("{} {{ {} }}", e.m, fields.join(", "))),
        );
    }
    out.push_str("} // verus!\n");
    out
}

const TEMPLATE: &str = r#"// ── {Row} ──

/// A row as the spec holds it.
pub open spec fn {e}_row_view(r: {Row}) -> {M} {
    {view}
}

/// The rows a list stands for, by id (a later row of one id wins; `{e}_distinct` rules that out).
pub open spec fn {e}_rows_map(rows: Seq<{Row}>) -> Map<int, {M}>
    decreases rows.len()
{
    if rows.len() == 0 {
        Map::empty()
    } else {
        {e}_rows_map(rows.drop_last()).insert(rows.last().id.0 as int, {e}_row_view(rows.last()))
    }
}

pub open spec fn {e}_distinct(rows: Seq<{Row}>) -> bool {
    forall|i: int, j: int| 0 <= i < rows.len() && 0 <= j < rows.len() && i != j
        ==> #[trigger] rows[i].id.0 != #[trigger] rows[j].id.0
}

/// The list holds every row of the map, once each.
pub open spec fn {e}_snapshot(m: Map<int, {M}>, rows: Seq<{Row}>) -> bool {
    {e}_distinct(rows) && {e}_rows_map(rows) == m
}

/// The rows of the list for which `p` holds, counted.
pub open spec fn {e}_count_rows(rows: Seq<{Row}>, p: spec_fn({M}, int) -> bool) -> nat
    decreases rows.len()
{
    if rows.len() == 0 {
        0
    } else {
        {e}_count_rows(rows.drop_last(), p)
            + if p({e}_row_view(rows.last()), rows.last().id.0 as int) { 1nat } else { 0nat }
    }
}

pub proof fn {e}_lemma_distinct_prefix(rows: Seq<{Row}>)
    requires {e}_distinct(rows), rows.len() > 0
    ensures {e}_distinct(rows.drop_last())
{
    let d = rows.drop_last();
    assert forall|a: int, b: int| 0 <= a < d.len() && 0 <= b < d.len() && a != b
        implies #[trigger] d[a].id.0 != #[trigger] d[b].id.0 by {
        assert(d[a] == rows[a] && d[b] == rows[b]);
    }
}

pub proof fn {e}_lemma_dom(rows: Seq<{Row}>, x: int)
    ensures {e}_rows_map(rows).contains_key(x)
        <==> exists|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x
    decreases rows.len()
{
    if rows.len() > 0 {
        let d = rows.drop_last();
        {e}_lemma_dom(d, x);
        if {e}_rows_map(d).contains_key(x) {
            let i = choose|i: int| 0 <= i < d.len() && #[trigger] d[i].id.0 as int == x;
            assert(rows[i].id.0 as int == x);
        }
        if exists|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x {
            let i = choose|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x;
            if i < rows.len() - 1 {
                assert(d[i].id.0 as int == x);
            }
        }
    }
}

pub proof fn {e}_lemma_at(rows: Seq<{Row}>, i: int)
    requires {e}_distinct(rows), 0 <= i < rows.len()
    ensures
        {e}_rows_map(rows).contains_key(rows[i].id.0 as int),
        {e}_rows_map(rows)[rows[i].id.0 as int] == {e}_row_view(rows[i]),
    decreases rows.len()
{
    if i < rows.len() - 1 {
        {e}_lemma_distinct_prefix(rows);
        {e}_lemma_at(rows.drop_last(), i);
        assert(rows[i].id.0 != rows.last().id.0);
    }
}

/// `p` of every row of the list is `p` of every row of the map it stands for.
pub proof fn {e}_lemma_forall(rows: Seq<{Row}>, p: spec_fn({M}, int) -> bool)
    requires {e}_distinct(rows)
    ensures
        (forall|i: int| 0 <= i < rows.len() ==> #[trigger] p({e}_row_view(rows[i]), rows[i].id.0 as int))
        == (forall|x: int| #[trigger] {e}_rows_map(rows).contains_key(x) ==> p({e}_rows_map(rows)[x], x))
{
    if forall|i: int| 0 <= i < rows.len() ==> #[trigger] p({e}_row_view(rows[i]), rows[i].id.0 as int) {
        assert forall|x: int| #[trigger] {e}_rows_map(rows).contains_key(x)
            implies p({e}_rows_map(rows)[x], x) by {
            {e}_lemma_dom(rows, x);
            let i = choose|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x;
            {e}_lemma_at(rows, i);
            assert(p({e}_row_view(rows[i]), rows[i].id.0 as int));
        }
    }
    if forall|x: int| #[trigger] {e}_rows_map(rows).contains_key(x) ==> p({e}_rows_map(rows)[x], x) {
        assert forall|i: int| 0 <= i < rows.len()
            implies #[trigger] p({e}_row_view(rows[i]), rows[i].id.0 as int) by {
            {e}_lemma_at(rows, i);
        }
    }
}

/// The rows of the list for which `p` holds are as many as the ids of the map's.
pub proof fn {e}_lemma_count(rows: Seq<{Row}>, p: spec_fn({M}, int) -> bool)
    requires {e}_distinct(rows)
    ensures
        {e}_count_rows(rows, p)
            == {e}_rows_map(rows).dom().filter(|x: int| p({e}_rows_map(rows)[x], x)).len()
    decreases rows.len()
{
    if rows.len() == 0 {
        assert({e}_rows_map(rows).dom().filter(|x: int| p({e}_rows_map(rows)[x], x)) =~= Set::empty());
    } else {
        let d = rows.drop_last();
        {e}_lemma_distinct_prefix(rows);
        {e}_lemma_count(d, p);
        let k = rows.last().id.0 as int;
        assert(!{e}_rows_map(d).contains_key(k)) by {
            {e}_lemma_dom(d, k);
            if {e}_rows_map(d).contains_key(k) {
                let i = choose|i: int| 0 <= i < d.len() && #[trigger] d[i].id.0 as int == k;
                assert(rows[i].id.0 == rows[rows.len() - 1].id.0);
            }
        }
        let fd = {e}_rows_map(d).dom().filter(|x: int| p({e}_rows_map(d)[x], x));
        let fr = {e}_rows_map(rows).dom().filter(|x: int| p({e}_rows_map(rows)[x], x));
        if p({e}_row_view(rows.last()), k) {
            assert(fr =~= fd.insert(k));
        } else {
            assert(fr =~= fd);
        }
    }
}

/// Counting a prefix never counts more than the whole.
pub proof fn {e}_lemma_count_prefix(rows: Seq<{Row}>, n: int, p: spec_fn({M}, int) -> bool)
    requires 0 <= n <= rows.len()
    ensures {e}_count_rows(rows.subrange(0, n), p) <= {e}_count_rows(rows, p)
    decreases rows.len()
{
    if n < rows.len() {
        assert(rows.drop_last().subrange(0, n) =~= rows.subrange(0, n));
        {e}_lemma_count_prefix(rows.drop_last(), n, p);
    } else {
        assert(rows.subrange(0, n) =~= rows);
    }
}

/// At most one counted row per row of the list.
pub proof fn {e}_lemma_count_len(rows: Seq<{Row}>, p: spec_fn({M}, int) -> bool)
    ensures {e}_count_rows(rows, p) <= rows.len()
    decreases rows.len()
{
    if rows.len() > 0 {
        {e}_lemma_count_len(rows.drop_last(), p);
    }
}

"#;
