//! `.role(..)`, `.key(..)`, composite parent keys and `.along(..).by(..)`.

use super::support::entity_key;
use crate::analysis::build_model;
use crate::diagnostics::RdraError;
use crate::model::{ColumnType, CrossConstraintScope, SemanticModel, UniqueSource};
use rdra_ish_syntax::parse;

fn build(src: &str) -> (SemanticModel, Vec<RdraError>) {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors = diags
        .into_iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error)
        .collect();
    (model, errors)
}

fn built(src: &str) -> SemanticModel {
    let (model, errors) = build(src);
    let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(errors.is_empty(), "model errors: {errors:?}");
    model
}

/// `(child column, parent key column)` pairs of every key of `child`, in fact order.
fn keys_of(model: &SemanticModel, child: &str) -> Vec<Vec<(String, String)>> {
    let key = entity_key(model, child);
    model
        .foreign_keys()
        .filter(|(_, fk)| fk.child == key)
        .map(|(_, fk)| fk.columns.clone())
        .collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(c, p)| (c.to_string(), p.to_string()))
        .collect()
}

const ADDRESSES: &str = r#"
entity Address "Address" { id: Int @pk }
entity Order "Order" { id: Int @pk }
"#;

#[test]
fn roles_name_two_keys_to_one_parent() {
    let model = built(&format!(
        "{ADDRESSES}
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to).optional()
"
    ));
    assert_eq!(
        keys_of(&model, "Order"),
        vec![
            pairs(&[("ship_to_id", "id")]),
            pairs(&[("bill_to_id", "id")])
        ]
    );
    let order = &model.entities[entity_key(&model, "Order")];
    let bill_to = order.columns.iter().find(|c| c.name == "bill_to_id");
    assert!(bill_to.is_some_and(|c| c.is_nullable));
}

#[test]
fn the_default_role_is_the_parent_id() {
    let model = built(&format!(
        "{ADDRESSES}
relate(Order, Address, N:1)
relate(Order, Address, N:1).role(bill_to)
"
    ));
    assert_eq!(
        keys_of(&model, "Order"),
        vec![
            pairs(&[("address_id", "id")]),
            pairs(&[("bill_to_id", "id")])
        ]
    );
}

#[test]
fn two_keys_to_one_parent_without_roles_are_a_duplicate_role() {
    let (model, errors) = build(&format!(
        "{ADDRESSES}
relate(Order, Address, N:1)
relate(Order, Address, N:1)
"
    ));
    assert!(
        matches!(
            errors.as_slice(),
            [RdraError::DuplicateRelateRole { child, role }] if child == "Order" && role == "Address"
        ),
        "{errors:?}"
    );
    assert!(errors[0].to_string().contains("add `.role(...)`"));
    assert_eq!(
        keys_of(&model, "Order"),
        vec![pairs(&[("address_id", "id")]), vec![]],
        "the second key keeps no columns"
    );
}

#[test]
fn roles_that_differ_only_in_case_are_a_duplicate_role() {
    let (_, errors) = build(&format!(
        "{ADDRESSES}
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(Ship_To)
"
    ));
    assert!(
        matches!(errors.as_slice(), [RdraError::DuplicateRelateRole { .. }]),
        "{errors:?}"
    );
}

#[test]
fn a_clash_with_an_author_column_hints_role_and_key() {
    let (_, errors) = build(
        r#"
entity Address "Address" { id: Int @pk }
entity Order "Order" { id: Int @pk  address_id: Int }
relate(Order, Address, N:1)
"#,
    );
    assert!(
        matches!(errors.as_slice(), [RdraError::FkConflict { col, .. }] if col == "address_id"),
        "{errors:?}"
    );
    let text = errors[0].to_string();
    assert!(
        text.contains("`.role(...)`") && text.contains("`.key(address_id)`"),
        "{text}"
    );
}

const SHIPMENT: &str = r#"
entity Order "Order" { id: Int @pk }
entity Shipment "Shipment" {
  id: Int @pk
  order_ref: Int @null
  code: String
}
"#;

#[test]
fn key_adopts_author_columns_as_the_key() {
    let model = built(&format!(
        "{SHIPMENT}\nrelate(Shipment, Order, 1:1).key(order_ref)\n"
    ));
    assert_eq!(
        keys_of(&model, "Shipment"),
        vec![pairs(&[("order_ref", "id")])]
    );
    let shipment = entity_key(&model, "Shipment");
    let column = model.entities[shipment]
        .columns
        .iter()
        .find(|c| c.name == "order_ref")
        .unwrap();
    assert!(column.is_unique, "a 1:1 key column is unique");
    assert!(!column.is_nullable, "a mandatory key is not null");
    assert!(model.column_fk(shipment, "order_ref").is_some());
    assert!(
        model.entities[shipment]
            .columns
            .iter()
            .all(|c| c.name != "order_id"),
        "no column is generated"
    );
}

fn key_error(relate: &str) -> RdraError {
    let (model, errors) = build(&format!("{SHIPMENT}\n{relate}\n"));
    assert_eq!(keys_of(&model, "Shipment"), vec![vec![]], "no key columns");
    match <[RdraError; 1]>::try_from(errors) {
        Ok([err]) => err,
        Err(errors) => panic!("{errors:?}"),
    }
}

#[test]
fn key_with_the_wrong_column_count_is_key_arity() {
    let err = key_error("relate(Shipment, Order, N:1).key(order_ref, code)");
    assert!(
        matches!(
            err,
            RdraError::KeyArity {
                got: 2,
                want: 1,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn key_naming_an_undeclared_column_is_key_column_undefined() {
    let err = key_error("relate(Shipment, Order, N:1).key(order_no)");
    assert!(
        matches!(&err, RdraError::KeyColumnUndefined { column, .. } if column == "order_no"),
        "{err:?}"
    );
}

#[test]
fn key_of_another_type_is_key_type_mismatch() {
    let err = key_error("relate(Shipment, Order, N:1).key(code)");
    assert!(
        matches!(&err, RdraError::KeyTypeMismatch { got, want, .. } if got == "String" && want == "Int"),
        "{err:?}"
    );
}

#[test]
fn a_column_holds_one_key_only() {
    let (_, errors) = build(&format!(
        "{SHIPMENT}
relate(Shipment, Order, N:1).key(order_ref)
relate(Shipment, Order, N:1).role(origin).key(order_ref)
"
    ));
    assert!(
        matches!(errors.as_slice(), [RdraError::KeyColumnShared { column, .. }] if column == "order_ref"),
        "{errors:?}"
    );
}

const BIN: &str = r#"
entity Bin "Bin" {
  site: String @pk(site, shelf)
  shelf: Int
}
entity Stock "Stock" {
  id: Int @pk
  alt_site: String
  alt_shelf: Int
}
"#;

#[test]
fn a_composite_parent_key_generates_one_column_per_key_column() {
    let model = built(&format!(
        "{BIN}
relate(Stock, Bin, N:1)
relate(Stock, Bin, N:1).role(alt).key(alt_site, alt_shelf)
"
    ));
    assert_eq!(
        keys_of(&model, "Stock"),
        vec![
            pairs(&[("bin_site", "site"), ("bin_shelf", "shelf")]),
            pairs(&[("alt_site", "site"), ("alt_shelf", "shelf")]),
        ]
    );
    let stock = &model.entities[entity_key(&model, "Stock")];
    let types: Vec<_> = stock
        .columns
        .iter()
        .filter(|c| c.name.starts_with("bin_"))
        .map(|c| c.col_type.clone())
        .collect();
    assert_eq!(types, [ColumnType::String, ColumnType::Int]);
}

#[test]
fn a_composite_one_to_one_key_is_a_unique_set_not_unique_columns() {
    let model = built(&format!("{BIN}\nrelate(Stock, Bin, 1:1)\n"));
    let stock = entity_key(&model, "Stock");
    assert!(model.entities[stock]
        .columns
        .iter()
        .filter(|c| c.name.starts_with("bin_"))
        .all(|c| !c.is_unique));
    let unique: Vec<_> = model
        .unique_keys(stock)
        .into_iter()
        .map(|k| (k.columns, k.source))
        .collect();
    assert!(
        matches!(
            unique.as_slice(),
            [(columns, UniqueSource::OneToOne(_))] if columns == &["bin_shelf", "bin_site"]
        ),
        "{unique:?}"
    );
}

#[test]
fn key_against_a_composite_parent_checks_arity_and_types_in_key_order() {
    let (_, errors) = build(&format!(
        "{BIN}\nrelate(Stock, Bin, N:1).key(alt_shelf, alt_site)\n"
    ));
    assert!(
        matches!(errors.as_slice(), [RdraError::KeyTypeMismatch { column, parent_column, .. }]
            if column == "alt_shelf" && parent_column == "site"),
        "{errors:?}"
    );
}

const TWO_ADDRESSES: &str = r#"
entity Address "Address" { id: Int @pk  status: Enum(open, closed) }
entity Order "Order" { id: Int @pk  status: Enum(pending, shipped) }
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
"#;

const FORBID: &str =
    "forbidden(Order, Address, Order.status == shipped, Address.status == closed).along(Order, Address)";

/// The keys each `.along` hop of the model's one cross forbidden resolved to, by role.
fn hop_roles(model: &SemanticModel) -> Vec<Option<String>> {
    let CrossConstraintScope::RelationPath { links, .. } =
        &model.cross_forbidden_constraints[0].scope_semantics
    else {
        panic!("expected an .along path");
    };
    links
        .iter()
        .map(|link| link.and_then(|id| model.foreign_key(id).role.clone()))
        .collect()
}

#[test]
fn along_between_two_keys_is_ambiguous_without_by() {
    let (model, errors) = build(&format!("{TWO_ADDRESSES}\n{FORBID}\n"));
    assert!(
        matches!(errors.as_slice(), [RdraError::AmbiguousOwnerLink { roles, .. }] if roles == "ship_to, bill_to"),
        "{errors:?}"
    );
    assert!(errors[0].to_string().contains("`.along(...).by(ship_to)`"));
    assert_eq!(hop_roles(&model), [None]);
}

#[test]
fn by_picks_the_key_with_its_role() {
    let model = built(&format!("{TWO_ADDRESSES}\n{FORBID}.by(bill_to)\n"));
    assert_eq!(hop_roles(&model), [Some("bill_to".to_string())]);
}

#[test]
fn along_with_one_key_needs_no_by() {
    let model = built(&format!(
        "{ADDRESSES}
relate(Order, Address, N:1)
forbidden(Order, Address, Order.id == 0, Address.id == 0).along(Order, Address)
"
    ));
    let CrossConstraintScope::RelationPath { links, .. } =
        &model.cross_forbidden_constraints[0].scope_semantics
    else {
        panic!("expected an .along path");
    };
    assert!(matches!(links.as_slice(), [Some(_)]), "{links:?}");
}

#[test]
fn by_naming_no_key_on_the_path_is_unknown_owner_role() {
    let (_, errors) = build(&format!("{TWO_ADDRESSES}\n{FORBID}.by(ship)\n"));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, RdraError::UnknownOwnerRole { role, .. } if role == "ship")),
        "{errors:?}"
    );
}

#[test]
fn by_without_along_is_an_error() {
    let (_, errors) = build(&format!(
        "{TWO_ADDRESSES}
forbidden(Order, Address, Order.status == shipped, Address.status == closed).by(ship_to)
"
    ));
    assert!(
        matches!(errors.as_slice(), [RdraError::ByWithoutAlong]),
        "{errors:?}"
    );
}
