use rdra_core::{build_merged_model, resolve, RdraError};
use rdra_emit::{
    csv::{EntityListCsvEmitter, RelationMatrixCsvEmitter},
    plantuml::{ErPlantUmlEmitter, RdraPlantUmlEmitter, StateDiagramEmitter},
    Emitter, View,
};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/purchase")
}

#[test]
fn test_purchase_fixture_resolves_three_files() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, diags) = resolve(&[entry], &[root.clone()]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "resolve errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    // buc_purchase.rdra + shared/actors.rdra + shared/entities.rdra
    assert_eq!(program.sources.len(), 3, "expected exactly 3 source files");

    // buc_purchase imports both shared modules → 2 edges
    assert_eq!(program.import_graph.edge_count(), 2);
}

#[test]
fn test_purchase_fixture_builds_model() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    // Actors: Customer, Staff
    assert_eq!(model.actors.len(), 2);

    // Entities: Customer_info, Order, OrderLine, Product, Category
    assert_eq!(model.entities.len(), 5);

    // BUCs: Purchase
    assert_eq!(model.bucs.len(), 1);

    // UseCases: Browse, AddToCart, Checkout
    assert_eq!(model.use_cases.len(), 3);

    // Screens: ProductList, CartView, CheckoutForm
    assert_eq!(model.screens.len(), 3);
}

#[test]
fn test_buc_diagram() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    // Purchase BUC に絞った図を生成
    let view = View::bucs(vec!["Purchase".to_string()]);
    let puml = RdraPlantUmlEmitter.emit(&model, &view).unwrap();

    // Purchase BUC に関連する Customer と Browse は含まれる
    assert!(
        puml.contains("Customer"),
        "Customer should be visible in Purchase BUC diagram:\n{puml}"
    );
    assert!(
        puml.contains("Browse"),
        "Browse should be visible in Purchase BUC diagram:\n{puml}"
    );

    // Staff は Purchase BUC に関連しないので含まれない
    assert!(
        !puml.contains("Staff"),
        "Staff should NOT be visible in Purchase BUC diagram:\n{puml}"
    );
}

#[test]
fn test_relation_matrix_csv() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    let view = View::whole();
    let csv = RelationMatrixCsvEmitter.emit(&model, &view).unwrap();

    // ヘッダに UseCase が含まれる
    assert!(
        csv.contains("UseCase"),
        "header should contain UseCase:\n{csv}"
    );

    // Browse（reads=R）が含まれる
    assert!(
        csv.contains("Browse"),
        "Browse should appear in matrix:\n{csv}"
    );

    // AddToCart（writes=W）が含まれる
    assert!(
        csv.contains("AddToCart"),
        "AddToCart should appear in matrix:\n{csv}"
    );

    // Checkout（creates=C）が含まれる
    assert!(
        csv.contains("Checkout"),
        "Checkout should appear in matrix:\n{csv}"
    );

    // CRUD 文字が含まれる
    assert!(
        csv.contains('R'),
        "R (reads) should appear in matrix:\n{csv}"
    );
    assert!(
        csv.contains('C'),
        "C (creates) should appear in matrix:\n{csv}"
    );
    assert!(
        csv.contains('W'),
        "W (writes) should appear in matrix:\n{csv}"
    );
}

#[test]
fn test_state_diagram() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    let view = View::whole();
    let puml = StateDiagramEmitter.emit(&model, &view).unwrap();

    assert!(
        puml.contains("@startuml"),
        "should start with @startuml:\n{puml}"
    );
    assert!(
        puml.contains("[*] -->"),
        "should have initial state:\n{puml}"
    );
    assert!(
        puml.contains("OrderDraft"),
        "OrderDraft state should appear:\n{puml}"
    );
    assert!(
        puml.contains("OrderPaid"),
        "OrderPaid state should appear:\n{puml}"
    );
    assert!(
        puml.contains("OrderShipped"),
        "OrderShipped state should appear:\n{puml}"
    );
    assert!(puml.contains("@enduml"), "should end with @enduml:\n{puml}");
}

#[test]
fn test_circular_import_emits_warning() {
    use std::fs;

    let dir = std::env::temp_dir().join(format!(
        "rdra_integ_circular_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    let b_dir = dir.join("b");
    fs::create_dir_all(&b_dir).unwrap();

    // a.rdra imports b/mod.rdra, b/mod.rdra imports a.rdra
    fs::write(
        dir.join("a.rdra"),
        "module a\nimport b.mod\nactor Customer \"顧客\"\n",
    )
    .unwrap();
    fs::write(
        b_dir.join("mod.rdra"),
        "module b.mod\nimport a\nactor Staff \"スタッフ\"\n",
    )
    .unwrap();

    let (program, diags) = resolve(&[dir.join("a.rdra")], &[dir.clone()]);

    let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
    assert!(
        warnings
            .iter()
            .any(|d| matches!(&d.error, RdraError::CircularImport { .. })),
        "expected CircularImport warning, got diags: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    // Files are still collected despite cycle.
    assert_eq!(program.sources.len(), 2);

    // Clean up.
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_purchase_fixture_er_snapshot() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    let view = View::er();
    let puml = ErPlantUmlEmitter.emit(&model, &view).unwrap();
    insta::assert_snapshot!(puml);
}

#[test]
fn test_purchase_fixture_entity_csv_snapshot() {
    let root = fixture_root();
    let entry = root.join("buc/buc_purchase.rdra");

    let (program, _) = resolve(&[entry], &[root.clone()]);
    let (model, diags) = build_merged_model(&program, &[root]);

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "model errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );

    let view = View::whole();
    let csv = EntityListCsvEmitter.emit(&model, &view).unwrap();
    insta::assert_snapshot!(csv);
}

fn errors_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/errors")
}

#[test]
fn test_error_type_mismatch() {
    let path = errors_fixture_root().join("type_mismatch.rdra");
    let (program, _) = resolve(&[path.clone()], &[errors_fixture_root()]);
    let (_, diags) = build_merged_model(&program, &[errors_fixture_root()]);

    let has_type_mismatch = diags
        .iter()
        .any(|d| matches!(&d.error, RdraError::TypeMismatch { .. }));
    assert!(
        has_type_mismatch,
        "expected TypeMismatch diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_error_nm_relation() {
    let path = errors_fixture_root().join("nm_relation.rdra");
    let (program, _) = resolve(&[path.clone()], &[errors_fixture_root()]);
    let (_, diags) = build_merged_model(&program, &[errors_fixture_root()]);

    let has_nm = diags
        .iter()
        .any(|d| matches!(&d.error, RdraError::NMRelation { .. }));
    assert!(
        has_nm,
        "expected NMRelation diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_error_duplicate() {
    let path = errors_fixture_root().join("duplicate.rdra");
    let (program, _) = resolve(&[path.clone()], &[errors_fixture_root()]);
    let (_, diags) = build_merged_model(&program, &[errors_fixture_root()]);

    let has_dup = diags
        .iter()
        .any(|d| matches!(&d.error, RdraError::DuplicateDefinition { .. }));
    assert!(
        has_dup,
        "expected DuplicateDefinition diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_duplicate_definition_across_files() {
    use std::fs;

    let dir = std::env::temp_dir().join(format!(
        "rdra_integ_dup_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    fs::write(
        shared_dir.join("actors.rdra"),
        "module shared.actors\nactor Customer \"顧客\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.rdra"),
        "import shared.actors\nactor Customer \"重複定義\"\n",
    )
    .unwrap();

    let (program, resolve_diags) = resolve(&[dir.join("main.rdra")], &[dir.clone()]);
    let (_, model_diags) = build_merged_model(&program, &[dir.clone()]);

    let all: Vec<_> = resolve_diags.iter().chain(model_diags.iter()).collect();
    let dup: Vec<_> = all
        .iter()
        .filter(|d| matches!(&d.error, RdraError::DuplicateDefinition { .. }))
        .collect();
    assert!(!dup.is_empty(), "expected DuplicateDefinition error");

    // Clean up.
    let _ = fs::remove_dir_all(&dir);
}
