use super::*;
use std::fs;
use std::path::Path;

#[test]
fn a_deprecated_form_is_a_located_warning() {
    let mut parsed = parse_with_deprecations("performs(Clerk, Pay)\n");
    parsed.deprecations.push(rdra_ish_syntax::Deprecation {
        span: 0..20,
        message: "write `new(...)`".into(),
    });
    let mut diags = Vec::new();
    let ast = take_parsed(parsed, 3, Path::new("m.rdra"), &mut diags);
    assert_eq!(ast.items.len(), 1);
    let [diag] = diags.as_slice() else {
        panic!("{diags:?}");
    };
    assert!(diag.is_warning);
    assert_eq!(
        diag.error.to_string(),
        "deprecated syntax in 'm.rdra': write `new(...)`"
    );
    let location = diag.location.as_ref().expect("located");
    assert_eq!((location.source_id, location.span.clone()), (3, 0..20));
}

/// Write a file, creating parent directories as needed.
fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Create a unique temp directory under the system temp root.
fn make_temp_dir(prefix: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "rdra_test_{}_{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

#[test]
fn test_resolve_single_file() {
    let dir = make_temp_dir("single");
    let entry = dir.join("main.rdra");
    write_file(
        &entry,
        r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#,
    );

    let (program, diags) = resolve(&[entry], std::slice::from_ref(&dir));
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "unexpected errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(program.sources.len(), 1);
}

#[test]
fn test_resolve_two_files_with_import() {
    let dir = make_temp_dir("two_files");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    write_file(
        &shared_dir.join("actors.rdra"),
        r#"
module shared.actors
actor Customer "顧客"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors
usecase Browse "商品を探す"
performs(Customer, Browse)
"#,
    );

    let (program, diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "unexpected errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(program.sources.len(), 2, "expected 2 source files");

    // Verify edge exists in import graph.
    assert_eq!(program.import_graph.edge_count(), 1);

    // Verify model builds correctly.
    let (model, model_diags) = build_merged_model(&program, &[dir]);
    let model_errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        model_errors.is_empty(),
        "model errors: {:?}",
        model_errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(model.actors.len(), 1);
    assert_eq!(model.use_cases.len(), 1);
}

#[test]
fn selective_import_alias_resolves_and_hides_original() {
    let dir = make_temp_dir("import_select_alias");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    write_file(
        &shared_dir.join("actors.rdra"),
        r#"
module shared.actors
actor Staff "職員"
actor Customer "顧客"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors.{Staff as S}
usecase Work "作業"
performs(S, Work)
performs(Staff, Work)
performs(Customer, Work)
"#,
    );

    let (program, _) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    let errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();

    assert!(
        !errors.iter().any(|d| matches!(
            &d.error,
            RdraError::UndefinedSymbol { id, .. } if id == "S"
        )),
        "S should resolve: {errors:?}"
    );
    assert!(
        errors.iter().any(|d| matches!(
            &d.error,
            RdraError::UndefinedSymbol { id, .. } if id == "Staff"
        )),
        "Staff should be hidden: {errors:?}"
    );
    assert!(
        errors.iter().any(|d| matches!(
            &d.error,
            RdraError::UndefinedSymbol { id, .. } if id == "Customer"
        )),
        "Customer should be hidden: {errors:?}"
    );
}

#[test]
fn namespaced_import_requires_alias_prefix() {
    let dir = make_temp_dir("import_ns");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    write_file(
        &shared_dir.join("actors.rdra"),
        r#"
module shared.actors
actor Staff "職員"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors as a
usecase Work "作業"
performs(a.Staff, Work)
performs(x.Staff, Work)
performs(Staff, Work)
"#,
    );

    let (program, _) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    let errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();

    assert!(
        errors.iter().any(|d| matches!(
            &d.error,
            RdraError::UndefinedSymbol { id, .. } if id.contains("x.")
        )),
        "x.Staff should fail: {errors:?}"
    );
    assert!(
        errors.iter().any(|d| matches!(
            &d.error,
            RdraError::UndefinedSymbol { id, .. } if id == "Staff"
        )),
        "bare Staff should fail under alias import: {errors:?}"
    );
    // a.Staff + Work should leave at most the two expected undefined errors.
    assert!(
        errors.len() >= 2,
        "expected undefined for x.Staff and Staff; got {errors:?}"
    );
}

#[test]
fn same_id_allowed_in_different_modules() {
    let dir = make_temp_dir("cross_module_same_id");
    let a_dir = dir.join("mod_a");
    let b_dir = dir.join("mod_b");
    fs::create_dir_all(&a_dir).unwrap();
    fs::create_dir_all(&b_dir).unwrap();

    write_file(
        &a_dir.join("actors.rdra"),
        r#"
module mod_a.actors
actor Staff "A側職員"
"#,
    );
    write_file(
        &b_dir.join("actors.rdra"),
        r#"
module mod_b.actors
actor Staff "B側職員"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import mod_a.actors as a
import mod_b.actors as b
usecase Work "作業"
performs(a.Staff, Work)
performs(b.Staff, Work)
"#,
    );

    let (program, resolve_diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    assert!(
        resolve_diags.iter().all(|d| d.is_warning),
        "{resolve_diags:?}"
    );
    let (model, model_diags) = build_merged_model(&program, &[dir]);
    let errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "cross-module same id should be allowed: {errors:?}"
    );
    assert_eq!(model.actors.len(), 2);
}

#[test]
fn sibling_without_import_keeps_open_world() {
    let dir = make_temp_dir("sibling_open_world");
    let shared = dir.join("shared");
    fs::create_dir_all(&shared).unwrap();
    write_file(
        &shared.join("actors.rdra"),
        r#"
module shared.actors
actor Staff "職員"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors as a
usecase Work "作業"
performs(a.Staff, Work)
"#,
    );
    write_file(
        &dir.join("sibling.rdra"),
        r#"
usecase Other "他"
actor Local "ローカル"
performs(Staff, Other)
"#,
    );

    let entries = [
        dir.join("main.rdra"),
        dir.join("sibling.rdra"),
        shared.join("actors.rdra"),
    ];
    let (program, _) = resolve(&entries, std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    let errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "sibling without import should keep open-world Staff: {errors:?}"
    );
}

#[test]
fn import_all_collision_is_duplicate() {
    let dir = make_temp_dir("import_all_collision");
    let a_dir = dir.join("mod_a");
    let b_dir = dir.join("mod_b");
    fs::create_dir_all(&a_dir).unwrap();
    fs::create_dir_all(&b_dir).unwrap();
    write_file(
        &a_dir.join("actors.rdra"),
        r#"
module mod_a.actors
actor Staff "A"
"#,
    );
    write_file(
        &b_dir.join("actors.rdra"),
        r#"
module mod_b.actors
actor Staff "B"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import mod_a.actors
import mod_b.actors
usecase Work "作業"
performs(Staff, Work)
"#,
    );

    let (program, _) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    assert!(
        model_diags.iter().any(|d| matches!(
            &d.error,
            RdraError::DuplicateDefinition { id } if id == "Staff"
        )),
        "All×All Staff collision expected: {model_diags:?}"
    );
}

#[test]
fn duplicate_import_alias_is_error() {
    let dir = make_temp_dir("dup_import_alias");
    let a_dir = dir.join("mod_a");
    let b_dir = dir.join("mod_b");
    fs::create_dir_all(&a_dir).unwrap();
    fs::create_dir_all(&b_dir).unwrap();
    write_file(
        &a_dir.join("actors.rdra"),
        r#"
module mod_a.actors
actor Staff "A"
"#,
    );
    write_file(
        &b_dir.join("actors.rdra"),
        r#"
module mod_b.actors
actor Staff "B"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import mod_a.actors as x
import mod_b.actors as x
usecase W "w"
performs(x.Staff, W)
"#,
    );

    let (program, _) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    assert!(
        model_diags.iter().any(|d| matches!(
            &d.error,
            RdraError::DuplicateAlias { alias } if alias == "x"
        )),
        "expected DuplicateAlias: {model_diags:?}"
    );
}

#[test]
fn unknown_import_module_is_error() {
    let dir = make_temp_dir("unknown_import_module");
    write_file(
        &dir.join("main.rdra"),
        r#"
import missing.actors as a
usecase Work "作業"
"#,
    );
    let (program, _) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    assert!(
        model_diags.iter().any(|d| matches!(
            &d.error,
            RdraError::UnknownImportModule { module, .. } if module == "missing.actors"
        )),
        "expected UnknownImportModule: {model_diags:?}"
    );
}

/// `buc/buc_registration.rdra` declares `module buc.registration`: no file sits at
/// `buc/registration.rdra`, but the loaded file that declares the module answers the import.
#[test]
fn an_import_resolves_by_the_declared_module_of_a_loaded_file() {
    let dir = make_temp_dir("module_by_declaration");
    write_file(
        &dir.join("buc/buc_registration.rdra"),
        "module buc.registration\nscreen FeePaymentScreen \"納付\"\n",
    );
    write_file(
        &dir.join("buc/buc_annuity.rdra"),
        "module buc.annuity\nimport buc.registration.{FeePaymentScreen}\nscreen AnnuityScreen \"年金\"\nusecase Pay \"納付\"\nnavigates(AnnuityScreen, FeePaymentScreen).via(Pay)\n",
    );
    let entries = [
        dir.join("buc/buc_annuity.rdra"),
        dir.join("buc/buc_registration.rdra"),
    ];
    let (program, diags) = resolve(&entries, std::slice::from_ref(&dir));
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(
        program.import_graph.edge_count(),
        1,
        "the import is an edge"
    );
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    assert!(model_diags.is_empty(), "{model_diags:?}");
}

/// An import no loaded file declares and no file path holds is one error, and it names the
/// loaded files that declare a module like it.
#[test]
fn an_unknown_module_names_the_files_that_declare_a_similar_one() {
    let dir = make_temp_dir("module_similar");
    write_file(
        &dir.join("buc/buc_registration.rdra"),
        "module buc.registration\nscreen FeePaymentScreen \"納付\"\n",
    );
    write_file(
        &dir.join("buc/buc_annuity.rdra"),
        "module buc.annuity\nimport buc.registraton.{FeePaymentScreen}\nscreen AnnuityScreen \"年金\"\n",
    );
    let entries = [
        dir.join("buc/buc_annuity.rdra"),
        dir.join("buc/buc_registration.rdra"),
    ];
    let (program, diags) = resolve(&entries, std::slice::from_ref(&dir));
    assert!(
        diags.is_empty(),
        "the miss is reported once, below: {diags:?}"
    );
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    let messages: Vec<String> = model_diags
        .iter()
        .filter(|d| matches!(d.error, RdraError::UnknownImportModule { .. }))
        .map(|d| d.error.to_string())
        .collect();
    assert_eq!(
        messages,
        ["unknown module 'buc.registraton'\n  hint: declare `module buc.registraton` in an included .rdra file, or fix the import path; similar modules: buc.registration (buc/buc_registration.rdra)"]
    );
}

#[test]
fn duplicate_module_path_is_error() {
    let dir = make_temp_dir("dup_module_path");
    let a = dir.join("a");
    let b = dir.join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    write_file(
        &a.join("actors.rdra"),
        r#"
module shared.actors
actor StaffA "A"
"#,
    );
    write_file(
        &b.join("actors.rdra"),
        r#"
module shared.actors
actor StaffB "B"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors as s
usecase Work "作業"
performs(s.StaffA, Work)
"#,
    );

    // Both module files must be entry sources so DuplicateModule is visible
    // even when import path resolution would only load one of them.
    let entries = [
        dir.join("main.rdra"),
        a.join("actors.rdra"),
        b.join("actors.rdra"),
    ];
    let (program, _) = resolve(&entries, std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);
    assert!(
        model_diags.iter().any(|d| matches!(
            &d.error,
            RdraError::DuplicateModule { module } if module == "shared.actors"
        )),
        "expected DuplicateModule: {model_diags:?}"
    );
}

#[test]
fn test_circular_import_warning() {
    let dir = make_temp_dir("circular");
    let a = dir.join("a.rdra");
    let b_dir = dir.join("b");
    fs::create_dir_all(&b_dir).unwrap();
    let b = b_dir.join("mod.rdra");

    // a imports b.mod, b.mod imports... but b.mod can't import "a" (dotted name would be "a"),
    // so simulate the simplest possible cycle: two files that each import the other.
    // We use a directory structure: dir/a.rdra imports dir/b/mod.rdra,
    // and dir/b/mod.rdra imports dir/a.rdra (via dotted name "a").
    write_file(
        &a,
        r#"
module a
import b.mod
actor Customer "顧客"
"#,
    );
    write_file(
        &b,
        r#"
module b.mod
import a
actor Staff "スタッフ"
"#,
    );

    let (program, diags) = resolve(&[a], std::slice::from_ref(&dir));
    let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
    assert!(
        warnings
            .iter()
            .any(|d| matches!(&d.error, RdraError::CircularImport { .. })),
        "expected CircularImport warning, got: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    // Sources are still collected (2 files).
    assert_eq!(program.sources.len(), 2);
    // a.rdra loads first and imports b.mod, so b.mod's `import a` closes the cycle.
    let closing = warnings.iter().find_map(|d| match &d.error {
        RdraError::CircularImport {
            closing,
            closing_file,
            ..
        } => Some((closing.as_str(), closing_file.as_str(), d.location.clone())),
        _ => None,
    });
    let (module, file, at) = closing.expect("CircularImport");
    assert_eq!(module, "a");
    assert!(file.ends_with("mod.rdra"), "{file}");
    assert_eq!(at.map(|at| at.source_id), Some(1));
}

#[test]
fn an_undefined_name_another_module_declares_names_the_import_once_per_file() {
    let dir = make_temp_dir("missing_import");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();
    write_file(
        &shared_dir.join("actors.rdra"),
        "module shared.actors\nactor Customer \"顧客\"\n",
    );
    write_file(
        &shared_dir.join("entities.rdra"),
        "module shared.entities\nentity Order \"注文\" { id: Int @pk }\n",
    );
    write_file(
        &dir.join("main.rdra"),
        "import shared.actors\nusecase Buy \"購入\"\nperforms(Customer, Buy)\ncreates(Buy, Order)\nreads(Buy, Order)\nreads(Buy, Ghost)\n",
    );
    let (program, _) = resolve(
        &[dir.join("main.rdra"), shared_dir.join("entities.rdra")],
        std::slice::from_ref(&dir),
    );
    let (_, diags) = build_merged_model(&program, &[dir]);
    let undefined: Vec<(String, Option<String>)> = diags
        .iter()
        .filter_map(|d| match &d.error {
            RdraError::UndefinedSymbol { id, import } => Some((id.clone(), import.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        undefined,
        vec![
            ("Order".into(), Some("shared.entities".into())),
            ("Order".into(), None),
            ("Ghost".into(), None),
        ]
    );
    let hinted = diags
        .iter()
        .find(|d| {
            matches!(
                &d.error,
                RdraError::UndefinedSymbol {
                    import: Some(_),
                    ..
                }
            )
        })
        .map(|d| d.error.to_string())
        .unwrap_or_default();
    assert!(hinted.contains("add `import shared.entities`"), "{hinted}");
}

#[test]
fn test_duplicate_definition_across_files() {
    let dir = make_temp_dir("dup_def");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    write_file(
        &shared_dir.join("actors.rdra"),
        r#"
module shared.actors
actor Customer "顧客"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors
actor Customer "重複定義"
"#,
    );

    let (program, resolve_diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);

    let all_diags: Vec<_> = resolve_diags.iter().chain(model_diags.iter()).collect();
    let dup_errors: Vec<_> = all_diags
        .iter()
        .filter(|d| matches!(&d.error, RdraError::DuplicateDefinition { .. }))
        .collect();
    assert!(!dup_errors.is_empty(), "expected DuplicateDefinition error");
}

#[test]
fn test_duplicate_definition_has_location() {
    let dir = make_temp_dir("dup_loc");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();

    write_file(
        &shared_dir.join("actors.rdra"),
        r#"
module shared.actors
actor Customer "顧客"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
import shared.actors
actor Customer "重複定義"
"#,
    );

    let (program, resolve_diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (_, model_diags) = build_merged_model(&program, &[dir]);

    let dup = resolve_diags
        .iter()
        .chain(model_diags.iter())
        .find(|d| matches!(&d.error, RdraError::DuplicateDefinition { .. }))
        .expect("expected DuplicateDefinition");

    let loc = dup
        .location
        .as_ref()
        .expect("duplicate definition should carry a source location");
    assert_eq!(
        loc.source_id, 0,
        "duplicate should be attributed to main.rdra (entry source)"
    );
    let pos = loc
        .start_position(&program)
        .expect("position should resolve against program sources");
    assert_eq!(pos.line, 3, "duplicate actor line in main.rdra");
}

#[test]
fn cross_kind_same_id_import_allows_kind_qualifiers() {
    // event Capture in shared + usecase Capture locally must coexist;
    // kind::Capture must resolve each side.
    let dir = make_temp_dir("cross_kind_capture");
    let shared_dir = dir.join("shared");
    fs::create_dir_all(&shared_dir).unwrap();
    write_file(
        &shared_dir.join("entities.rdra"),
        r#"
module shared.entities
entity Payment "Payment" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
event Capture "Capture Payment"
"#,
    );
    write_file(
        &dir.join("main.rdra"),
        r#"
module buc.payment
import shared.entities
usecase Capture "Capture Payment"
raises(usecase::Capture, event::Capture)
sets(usecase::Capture, Payment.status == captured)
"#,
    );

    let (program, resolve_diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
    let (model, model_diags) = build_merged_model(&program, &[dir]);
    let errors: Vec<_> = resolve_diags
        .iter()
        .chain(model_diags.iter())
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "expected cross-kind Capture to resolve, got: {errors:?}"
    );
    assert!(
        model.use_cases.values().any(|u| u.id == "Capture"),
        "usecase Capture missing"
    );
    assert!(
        model.events.values().any(|e| e.id == "Capture"),
        "event Capture missing"
    );
}

#[test]
fn test_resolve_fixture_purchase() {
    // Tests the full purchase fixture under tests/fixtures/purchase/.
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/purchase");

    if !fixture_root.exists() {
        // Skip if fixtures haven't been created yet.
        return;
    }

    let entry = fixture_root.join("buc/buc_purchase.rdra");
    let (program, diags) = resolve(&[entry], std::slice::from_ref(&fixture_root));

    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "resolve errors: {:?}",
        errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(program.sources.len(), 3, "expected 3 source files");

    let (model, model_diags) = build_merged_model(&program, &[fixture_root]);
    let model_errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        model_errors.is_empty(),
        "model errors: {:?}",
        model_errors
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
    assert!(model.actors.len() >= 2);
    assert!(model.entities.len() >= 4);
    assert!(!model.bucs.is_empty());
}

#[test]
fn a_condition_enters_the_closure_of_the_buc_whose_use_case_it_applies_to() {
    let src = r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  id: Int @pk
  method: Method
}
buc InBuc "In"
buc OtherBuc "Other"
usecase Collect "Collect"
usecase Refund "Refund"
contains(InBuc, Collect)
contains(OtherBuc, Refund)
condition Here "Here"
holds(Here, Pay.method == cash)
applies_to(Here, Collect)
condition There "There"
holds(There, Pay.method == card)
applies_to(There, Refund)
"#;
    let (ast, errors) = rdra_ish_syntax::parse(src);
    assert!(errors.is_empty(), "{errors:?}");
    let (model, diags) = crate::analysis::build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    let reached = reachable_from_buc(&model, "InBuc");
    let id_of = |name: &str| model.symbols.get(name).cloned().unwrap();
    assert!(reached.contains(&id_of("Here")));
    assert!(!reached.contains(&id_of("There")));
    assert!(reached.contains(&id_of("Method")));
}
