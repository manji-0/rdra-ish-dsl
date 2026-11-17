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
