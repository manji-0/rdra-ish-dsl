use crate::analysis::build_model;
use crate::diagnostics::RdraError;
use crate::model::{ColumnType, EffectValue, SemanticModel};
use crate::state_pattern::{derive_state_patterns, AxisKind, DEFAULT_PATTERN_CAP};
use rdra_ish_syntax::parse;

fn build(src: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    build_model(&ast)
}

fn errors(diags: &[crate::Diagnostic]) -> Vec<&RdraError> {
    diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| &d.error)
        .collect()
}

const PAYMENT: &str = r#"
variation PaymentMethod "Payment Method" {
  cash "Cash"
  card
}
entity Payment "Payment" {
  id: Int @pk
  method: PaymentMethod @default(card)
  status: Enum(pending, posted) @default(pending)
}
"#;

#[test]
fn variation_typed_column_resolves_to_the_variation_and_its_values() {
    let (model, diags) = build(PAYMENT);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let entity = model.entities.values().next().unwrap();
    let method = entity.columns.iter().find(|c| c.name == "method").unwrap();
    assert!(matches!(method.col_type, ColumnType::Variation(_)));
    assert_eq!(
        model.column_values(method),
        Some(&["cash".to_string(), "card".to_string()][..])
    );
    let status = entity.columns.iter().find(|c| c.name == "status").unwrap();
    assert_eq!(
        model.column_values(status),
        Some(&["pending".to_string(), "posted".to_string()][..])
    );
    let id = entity.columns.iter().find(|c| c.name == "id").unwrap();
    assert_eq!(model.column_values(id), None);
}

#[test]
fn column_typed_by_an_undeclared_name_is_unknown_column_type() {
    let (_, diags) = build("entity E \"E\" { id: Int @pk\n method: PaymentMethd }\n");
    let errs = errors(&diags);
    assert!(
        errs.iter().any(|e| matches!(e,
            RdraError::UnknownColumnType { entity, col, name }
                if entity == "E" && col == "method" && name == "PaymentMethd")),
        "{errs:?}"
    );
}

#[test]
fn default_outside_the_variation_values_is_rejected() {
    let src = PAYMENT.replace("@default(card)", "@default(crypto)");
    let (_, diags) = build(&src);
    let errs = errors(&diags);
    assert!(
        errs.iter().any(|e| matches!(e,
            RdraError::InvalidVariationValue { variation, value, .. }
                if variation == "PaymentMethod" && value == "crypto")),
        "{errs:?}"
    );
}

#[test]
fn sets_value_is_checked_against_the_variation_values() {
    let ok = format!("{PAYMENT}usecase Pay \"Pay\"\nsets(Pay, Payment, method == cash)\n");
    let (model, diags) = build(&ok);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    assert!(model
        .column_effects
        .iter()
        .any(|e| e.column == "method" && e.value == EffectValue::EnumVariant("cash".into())));

    let bad = format!("{PAYMENT}usecase Pay \"Pay\"\nsets(Pay, Payment, method == crypto)\n");
    let (_, diags) = build(&bad);
    let errs = errors(&diags);
    assert!(
        errs.iter().any(|e| matches!(e,
            RdraError::InvalidVariationValue { value, allowed, .. }
                if value == "crypto" && allowed == "cash, card")),
        "{errs:?}"
    );
}

#[test]
fn transitions_on_a_variation_column_is_an_error() {
    let src =
        format!("{PAYMENT}usecase Pay \"Pay\"\ntransitions(Payment.method, Pay, cash -> card)\n");
    let (_, diags) = build(&src);
    let errs = errors(&diags);
    assert!(
        errs.iter().any(
            |e| matches!(e, RdraError::TransitionsColumnNotEnum { col, .. } if col == "method")
        ),
        "{errs:?}"
    );
}

#[test]
fn variation_column_adds_no_state_axis_while_an_inline_enum_does() {
    let lifecycle = r#"
usecase Pay "Pay"
transitions(Payment.status, Pay, pending -> posted)
"#;
    let as_variation = format!("{PAYMENT}{lifecycle}").replace(
        "status: Enum",
        "backup_method: PaymentMethod @null\n  status: Enum",
    );
    let (model, diags) = build(&as_variation);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let result = &derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP)[0];
    let columns: Vec<_> = result.axes.iter().map(|a| a.column.as_str()).collect();
    assert_eq!(columns, vec!["status"], "{:?}", result.axes);
    assert!(result
        .axes
        .iter()
        .all(|a| matches!(a.kind, AxisKind::Enum(_))));

    let as_enum = as_variation.replace(
        "method: PaymentMethod @default(card)",
        "method: Enum(cash, card) @default(card)",
    );
    let (model, diags) = build(&as_enum);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let result = &derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP)[0];
    let mut columns: Vec<_> = result.axes.iter().map(|a| a.column.as_str()).collect();
    columns.sort();
    assert_eq!(columns, vec!["method", "status"]);
}
