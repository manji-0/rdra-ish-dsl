//! `reads(...).by(...).order(...).page(...)`: query shapes and the indexes they want.

use crate::build_model;
use crate::model::*;
use rdra_ish_syntax::parse;

const BASE: &str = r#"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" {
  id: Int @pk
  tenant_id: Int @tenant
  status: Enum(open, paid)
  amount: Money
  created_at: DateTime @index(tenant_id, status)
}
relate(Order, Customer, N:1)
usecase U "u"
api A "a"
"#;

fn build(reads: &str) -> (SemanticModel, Vec<String>) {
    let (ast, parse_errors) = parse(&format!("{BASE}{reads}\n"));
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    (model, errors)
}

fn shown(columns: &[OrderKey]) -> String {
    columns
        .iter()
        .map(|k| format!("{}{}", k.column, if k.desc { " desc" } else { "" }))
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn a_read_records_its_filter_sort_page_and_aggregate() {
    let (model, errors) = build(
        "reads(U, Order).by(customer_id, status).order(created_at.desc, id.asc).page(cursor)\nreads(A, Order).by(status).sum(amount)",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let [list, total] = model.queries.as_slice() else {
        panic!("{:?}", model.queries);
    };
    assert_eq!(list.filter, ["customer_id", "status"]);
    assert_eq!(shown(&list.order), "created_at desc, id");
    assert_eq!(list.page, Some(Page::Cursor));
    assert!(matches!(total.source, NodeRef::Api(_)));
    assert_eq!(total.aggregate, Some(Aggregate::Sum("amount".into())));
}

#[test]
fn a_malformed_read_is_an_invalid_query_shape() {
    for (reads, want) in [
        ("reads(U, Order).by(nope)", "has no column 'nope'"),
        (
            "reads(U, Order).order(created_at.up)",
            "takes a column, or `column.desc`",
        ),
        ("reads(U, Order).page(cursor)", "needs `.order(...)`"),
        ("reads(U, Order).page(all)", "takes `cursor` or `offset`"),
        (
            "reads(U, Order).sum(status)",
            "needs an Int, Money, or Decimal column",
        ),
        ("reads(U, Order).count().order(id)", "return one row"),
        (
            "reads(U, Order).by(status).by(amount)",
            "takes one `.by(...)`",
        ),
        (
            "reads(U, Order).count().sum(amount)",
            "one `.count()` or `.sum(...)`",
        ),
    ] {
        let (model, errors) = build(reads);
        assert_eq!(errors.len(), 1, "{reads}: {errors:?}");
        assert!(errors[0].contains(want), "{reads}: {errors:?}");
        assert!(model.queries.is_empty(), "{reads}");
    }
}

/// The tenant column leads, a cursor page ends in the primary key, and a shape an existing
/// key, an `@index`, or a longer suggestion serves adds no index.
#[test]
fn index_suggestions_skip_what_an_index_already_serves() {
    let (model, errors) = build(
        "reads(U, Order).by(status).order(created_at.desc).page(cursor)\n\
         reads(U, Order).by(status)\n\
         reads(U, Order).by(customer_id)\n\
         reads(A, Order).by(status).order(created_at)\n\
         reads(A, Order).order(amount, created_at.desc)",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let got: Vec<(String, Vec<usize>)> = model
        .index_suggestions()
        .iter()
        .map(|s| {
            let reads = s
                .readers
                .iter()
                .map(|r| match r {
                    Reader::Query(i) => *i,
                    other => panic!("{other:?}"),
                })
                .collect();
            (shown(&s.columns), reads)
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                "tenant_id, status, created_at desc, id desc".to_string(),
                vec![0, 3]
            ),
            ("tenant_id, customer_id".to_string(), vec![2]),
            ("tenant_id, amount, created_at desc".to_string(), vec![4]),
        ]
    );
    assert!(model.index_suggestions()[2].mixes_directions());
}

const MAPPED: &str = r#"
dto OrderView "Order view" {
  order_id: Int
  customer_id: Int
  total: Money
  note: String
}
response(A, OrderView)
"#;

/// A DTO field maps to a column, a generated key column included, and a read by an API
/// selects the columns its response fields map, in field order.
#[test]
fn a_read_by_an_api_selects_what_its_response_fields_map() {
    let (model, errors) = build(&format!(
        "{MAPPED}maps_field(OrderView.total, Order.amount)\nmaps_field(OrderView.order_id, Order.id)\nmaps_field(OrderView.customer_id, Order.customer_id)\nreads(A, Order).by(status)\nreads(U, Order).by(status)"
    ));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(model.dto_mappings.len(), 3);
    let selected: Vec<(String, String)> = model.queries[0]
        .selected(&model)
        .into_iter()
        .map(|s| (s.column, s.field))
        .collect();
    assert_eq!(
        selected,
        [
            ("id".to_string(), "order_id".to_string()),
            ("customer_id".to_string(), "customer_id".to_string()),
            ("amount".to_string(), "total".to_string()),
        ]
    );
    assert!(model.queries[1].selected(&model).is_empty());
}

#[test]
fn a_dto_mapping_names_a_field_and_a_column_that_exist_once() {
    for (mapping, want) in [
        (
            "maps_field(OrderView.nope, Order.id)",
            "has no column 'nope'",
        ),
        (
            "maps_field(OrderView.total, Order.nope)",
            "has no column 'nope'",
        ),
        (
            "maps_field(OrderView.total, Order.amount)\nmaps_field(OrderView.total, Order.id)",
            "'OrderView.total' already maps to a column",
        ),
        (
            "maps_field(Order.total, Order.amount)",
            "expected field or dto",
        ),
    ] {
        let (model, errors) = build(&format!("{MAPPED}{mapping}"));
        assert_eq!(errors.len(), 1, "{mapping}: {errors:?}");
        assert!(errors[0].contains(want), "{mapping}: {errors:?}");
        assert!(model.dto_mappings.len() <= 1, "{mapping}");
    }
}

const SWEPT: &str = r#"
entity Ticket "Ticket" {
  id: Int @pk
  tenant_id: Int @tenant
  state: Enum(open, held, closed)
}
timer Tick "tick"
usecase Sweep "sweep"
usecase Close "close"
triggers(Tick, Sweep)
transitions(Sweep, Ticket.state, open -> closed)
transitions(Sweep, Ticket.state, held -> closed)
transitions(Close, Ticket.state, open -> closed)
"#;

/// A use case a timer starts finds every row at the states it moves off, so it wants the
/// tenant column and the state column; its arms on one column are one lookup. A use case
/// an actor starts moves the row it already holds, which wants no index.
#[test]
fn a_timer_started_move_wants_an_index_on_its_state_column() {
    let (ast, parse_errors) = parse(SWEPT);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let (model, _) = build_model(&ast);
    let suggestions = model.index_suggestions();
    let [s] = suggestions.as_slice() else {
        panic!("{suggestions:?}");
    };
    assert_eq!(shown(&s.columns), "tenant_id, state");
    let [Reader::Batch { column, from, .. }] = s.readers.as_slice() else {
        panic!("{:?}", s.readers);
    };
    assert_eq!(column, "state");
    assert_eq!(from, &["open".to_string(), "held".to_string()]);
    assert_eq!(model.reader_name(&s.readers[0]), "Sweep");
    assert!(model.reader_site(&s.readers[0]).is_some());
}

const QUANTIFIED: &str = r#"
entity Case "Case"ROWS {
  id: Int @pk
  open: Bool
}
entity Step "Step"ROWS {
  id: Int @pk
  state: Enum(todo, done)
}
relate(Step, Case, N:1)
condition Busy "Busy"
holds(Busy, Case.open == true).has(Step.state == todo)
"#;

fn quantified(rows: &str) -> SemanticModel {
    let (ast, parse_errors) = parse(&QUANTIFIED.replace("ROWS", rows));
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    build_model(&ast).0
}

/// A `.has` reads one row's related rows through the foreign key index, which is enough
/// until the related entity declares a million rows or more; then it wants the key and
/// the compared column together.
#[test]
fn a_quantifier_wants_an_index_only_over_a_large_entity() {
    assert!(quantified("").index_suggestions().is_empty());
    assert!(quantified(" rows 999999").index_suggestions().is_empty());
    let model = quantified(" rows \"1e6\"");
    let suggestions = model.index_suggestions();
    let [s] = suggestions.as_slice() else {
        panic!("{suggestions:?}");
    };
    assert_eq!(shown(&s.columns), "case_id, state");
    assert!(matches!(&s.readers[..], [Reader::Quantifier { on }] if on == "Busy"));
    assert_eq!(s.rows, Some(1_000_000));
    assert!(!s.is_small());
}

/// `rows` takes a whole number, with `_` or `,` separators or an exponent; anything else
/// is an error on the entity.
#[test]
fn a_row_estimate_parses_or_is_reported() {
    let rows = |written: &str| {
        let (ast, _) = parse(&format!(
            "entity E \"E\" rows {written} {{\n  id: Int @pk\n}}\n"
        ));
        let (model, diags) = build_model(&ast);
        let errors: Vec<String> = diags
            .iter()
            .filter(|d| !d.is_warning)
            .map(|d| d.error.to_string())
            .collect();
        (model.entities.values().next().unwrap().rows, errors)
    };
    assert_eq!(rows("5000"), (Some(5000), vec![]));
    assert_eq!(rows("\"1_000_000\"").0, Some(1_000_000));
    assert_eq!(rows("\"2,500\"").0, Some(2500));
    assert_eq!(rows("\"3e4\"").0, Some(30_000));
    let (none, errors) = rows("many");
    assert_eq!(none, None);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("many"), "{errors:?}");
}

const GUARDED: &str = r#"
entity Shop "Shop" {
  id: Int @pk
  mode: Enum(open, closed)
}
entity Sale "Sale" {
  id: Int @pk
  state: Enum(draft, done)
}
entity Notice "Notice" {
  id: Int @pk
  level: Enum(low, high)
}
LINK
usecase Sell "sell"
usecase Refund "refund"
creates(Sell, Sale)
updates(Refund, Sale)
condition Calm "Calm"
holds(Calm, Notice.level == low, Shop.mode == open)
applies_to(Calm, Sell)
applies_to(Calm, Refund)
"#;

fn guarded(link: &str) -> SemanticModel {
    let src = GUARDED.replace("LINK", link);
    let (ast, parse_errors) = parse(&src);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    build_model(&ast).0
}

/// A guard that compares an entity no written row joins by a key looks for any matching row
/// across the table, so it wants the compared columns; an entity one key joins to a written
/// row is read through that key.
#[test]
fn a_guard_over_an_unjoined_entity_wants_an_index() {
    let model = guarded("relate(Sale, Shop, N:1)");
    let suggestions = model.index_suggestions();
    let [s] = suggestions.as_slice() else {
        panic!("{suggestions:?}");
    };
    assert_eq!(model.entities[s.entity].id, "Notice");
    assert_eq!(shown(&s.columns), "level");
    let [r @ Reader::Guard { usecases, .. }] = s.readers.as_slice() else {
        panic!("{:?}", s.readers);
    };
    assert_eq!(model.reader_name(r), "Calm");
    assert_eq!(usecases.len(), 2);
    assert!(model.reader_site(r).is_some());

    let both = guarded("relate(Sale, Shop, N:1)\nrelate(Sale, Notice, N:1)");
    assert!(both.index_suggestions().is_empty());
}
