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
        .map(|s| (shown(&s.columns), s.queries.clone()))
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
