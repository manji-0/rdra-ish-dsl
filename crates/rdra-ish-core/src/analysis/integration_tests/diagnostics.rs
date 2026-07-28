use crate::analysis::build_model;
use crate::diagnostics::RdraError;
use crate::model::{NodeRef, RelKind};
use rdra_ish_syntax::parse;

#[test]
fn test_duplicate_definition_same_kind() {
    let src = r#"
actor Customer "顧客"
actor Customer "重複"
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty());
    assert!(errors[0].error.to_string().contains("duplicate definition"));
}

#[test]
fn test_same_name_different_kind_allowed() {
    // `actor Add` and `usecase Add` must coexist without error when
    // references are qualified.
    let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(actor::Add, usecase::Add)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "unexpected errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    assert_eq!(model.actors.len(), 1);
    assert_eq!(model.use_cases.len(), 1);
    assert_eq!(model.relations.len(), 1);
}

#[test]
fn test_ambiguous_unqualified_reference() {
    let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(Add, Add)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty());
    assert!(errors[0].error.to_string().contains("ambiguous reference"));
}

#[test]
fn test_type_mismatch() {
    let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Browse, Customer)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty());
    assert!(errors[0].error.to_string().contains("type mismatch"));
}

#[test]
fn test_nm_relation_requires_via() {
    let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
relate(A, B, N:M)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty());
    assert!(errors[0].error.to_string().contains("N:M relation"));
    assert!(errors[0].error.to_string().contains(".via"));
}

#[test]
fn test_nm_relation_via_generates_fks() {
    let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
entity AB "AB" { id: Int @pk }
relate(A, B, N:M).via(AB)
"#;
    let (ast, _) = parse(src);
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "{errors:?}");

    let ab = model.entities.values().find(|e| e.id == "AB").unwrap();
    let fk_names: Vec<_> = ab
        .columns
        .iter()
        .filter(|c| c.is_fk)
        .map(|c| c.name.as_str())
        .collect();
    assert!(fk_names.contains(&"a_id"), "{fk_names:?}");
    assert!(fk_names.contains(&"b_id"), "{fk_names:?}");

    let nm = model
        .relations
        .iter()
        .find(|r| matches!(r.kind, RelKind::RelateManyToMany))
        .expect("N:M relation");
    assert!(matches!(nm.via, Some(NodeRef::Entity(_))));

    let a_ek = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "A")
        .map(|(k, _)| k)
        .unwrap();
    let b_ek = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "B")
        .map(|(k, _)| k)
        .unwrap();
    let ab_ek = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "AB")
        .map(|(k, _)| k)
        .unwrap();
    let synth_n1: Vec<_> = model
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelKind::RelateManyToOne))
        .collect();
    assert_eq!(
        synth_n1.len(),
        2,
        "expected synthesized J→A and J→B: {synth_n1:?}"
    );
    assert!(synth_n1
        .iter()
        .any(|r| r.from == NodeRef::Entity(ab_ek) && r.to == NodeRef::Entity(a_ek)));
    assert!(synth_n1
        .iter()
        .any(|r| r.from == NodeRef::Entity(ab_ek) && r.to == NodeRef::Entity(b_ek)));
}

#[test]
fn test_via_only_for_nm() {
    let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
entity J "J" { id: Int @pk }
relate(A, B, N:1).via(J)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors
            .iter()
            .any(|d| matches!(d.error, RdraError::ViaOnlyForNM)),
        "{:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_via_requires_entity() {
    let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
relate(A, B, N:M).via("not-an-entity")
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors
            .iter()
            .any(|d| matches!(d.error, RdraError::ViaRequiresEntity)),
        "{:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_nm_via_fk_conflict() {
    let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
entity AB "AB" {
  id: Int @pk
  a_id: Int
}
relate(A, B, N:M).via(AB)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors
            .iter()
            .any(|d| matches!(d.error, RdraError::FkConflict { .. })),
        "{:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_missing_pk_error() {
    let src = r#"
entity A "A" { name: String }
entity B "B" { id: Int @pk }
relate(B, A, N:1)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty());
    assert!(errors[0].error.to_string().contains("missing @pk"));
}

#[test]
fn test_one_to_many_fk_on_to_side() {
    let src = r#"
entity Customer "顧客" { id: Int @pk }
entity Order "注文" { id: Int @pk }
relate(Customer, Order, 1:N)
"#;
    let (ast, _) = parse(src);
    let (model, diags) = build_model(&ast);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "unexpected errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    let order = model.entities.values().find(|e| e.id == "Order").unwrap();
    let fk = order.columns.iter().find(|c| c.name == "customer_id");
    assert!(fk.is_some(), "customer_id FK not found in Order");
    assert!(fk.unwrap().is_fk);
}

#[test]
fn test_sets_false_on_equals_is_rejected() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  stock: Int
  selling: Int
}
usecase Pay "支払"
usecase Refund "返金"
sets(Pay, Order, status == paid, false)
sets(Refund, Order, stock < selling, false)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert_eq!(errors.len(), 1, "diags: {diags:?}");
    assert!(
        errors[0]
            .error
            .to_string()
            .contains("sets(..., status == paid, false)"),
        "{}",
        errors[0].error
    );
    assert!(
        model.column_effects.is_empty(),
        "equals+false must not register a column effect"
    );
    assert_eq!(model.proposition_effects.len(), 1);
    assert!(!model.proposition_effects[0].truth);
}

#[test]
fn test_multiple_lifecycle_columns_warns() {
    let src = r#"
entity Ticket "チケット" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  phase: Enum(intake, work) @default(intake)
}
event Close "閉じる"
event StartWork "作業開始"
transitions(Ticket.status, Close, open -> closed)
transitions(Ticket.phase, StartWork, intake -> work)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (_, diags) = build_model(&ast);
    let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
    assert!(
        warnings.iter().any(|d| d
            .error
            .to_string()
            .contains("already has transitions on column")),
        "expected MultipleLifecycleColumns warning, got: {:?}",
        warnings
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}
