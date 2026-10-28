//! `.role(..)`, `.key(..)`, composite parent keys and `.along(..).by(..)`.

use super::support::entity_key;
use crate::analysis::build_model;
use crate::diagnostics::RdraError;
use crate::model::SemanticModel;
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
