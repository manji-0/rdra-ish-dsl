#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use rdra_ish_core::{analyze_project, analyze_workspace, load_project, RdraError};

    fn write_file(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn a_product_with_a_path_dependency_gets_the_cli_diagnostics_and_none_in_the_dependency() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/xsys/product-mistakes");
        let project = load_project(std::slice::from_ref(&root), None).unwrap();
        let analysis = analyze_project(project);

        let names: Vec<String> = analysis
            .diagnostics
            .iter()
            .map(|d| d.error.to_string().split(':').next().unwrap().to_string())
            .collect();
        for name in [
            "ForeignImport",
            "UnknownForeignName",
            "NotExposed",
            "ForeignWrite",
            "ForeignContained",
            "DirectEntityReference",
            "ForeignStateNeedsEnvironment",
        ] {
            assert!(
                names.contains(&name.to_string()),
                "{name} missing from {names:?}"
            );
        }
        for diag in &analysis.diagnostics {
            let path = diag
                .location
                .as_ref()
                .unwrap()
                .path(&analysis.program)
                .unwrap();
            assert!(
                path.ends_with("product-mistakes/mistakes.rdra"),
                "{} is located in {}",
                diag.error,
                path.display()
            );
        }
    }

    #[test]
    fn a_dependency_file_opened_under_the_product_root_is_its_own_model() {
        let xsys =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/xsys");
        let shop = xsys.join("shop-model");
        let auth_file = xsys.join("auth-model/auth/session.rdra");
        let groups = rdra_ish_core::group_by_model(&[shop.clone(), auth_file.clone()]);
        assert_eq!(groups, vec![vec![shop.clone()], vec![auth_file.clone()]]);

        for group in groups {
            let project = load_project(&group, None).unwrap();
            let analysis = analyze_project(project);
            let errors: Vec<String> = analysis
                .diagnostics
                .iter()
                .filter(|d| !d.is_warning)
                .map(|d| d.error.to_string())
                .collect();
            assert!(errors.is_empty(), "{group:?}: {errors:?}");
            let files: Vec<String> = analysis
                .program
                .sources
                .iter()
                .map(|(p, _, _)| p.display().to_string())
                .collect();
            let own = if group[0] == shop {
                "shop-model"
            } else {
                "auth-model"
            };
            assert!(files.iter().all(|f| f.contains(own)), "{files:?}");
        }
    }

    #[test]
    fn analyze_workspace_reports_located_type_mismatch() {
        let root = std::env::temp_dir().join(format!(
            "rdra_lsp_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();

        let file = root.join("bad.rdra");
        write_file(
            &file,
            r#"actor Customer "Customer"
entity Order "Order" { id: Int @pk }
reads(Customer, Order)
"#,
        );

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &HashMap::new(),
        );
        let mismatch = analysis
            .diagnostics
            .iter()
            .find(|diag| matches!(&diag.error, RdraError::TypeMismatch { .. }));
        assert!(mismatch.is_some(), "expected type mismatch diagnostic");
        assert!(
            mismatch.unwrap().location.is_some(),
            "expected located diagnostic"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn analyze_workspace_uses_overlay_text() {
        let root = std::env::temp_dir().join(format!(
            "rdra_lsp_overlay_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();

        let file = root.join("model.rdra");
        write_file(
            &file,
            r#"usecase OldName "Old"
"#,
        );

        let mut overlays = HashMap::new();
        overlays.insert(
            file.clone(),
            r#"usecase NewName "New"
"#
            .to_string(),
        );

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &overlays,
        );
        assert!(analysis
            .model
            .use_cases
            .iter()
            .any(|(_, uc)| uc.id == "NewName"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_manifest_above_the_workspace_does_not_change_its_diagnostics() {
        let messages = |with_manifest: bool| {
            let root = std::env::temp_dir().join(format!(
                "rdra_lsp_manifest_{with_manifest}_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            write_file(&root.join(".git/HEAD"), "");
            if with_manifest {
                write_file(&root.join("rdra.toml"), "[model]\nname = \"lsp\"\n");
            }
            write_file(
                &root.join("src/bad.rdra"),
                "actor Customer \"Customer\"\nentity Order \"Order\" { id: Int @pk }\nreads(Customer, Order)\n",
            );

            let project = load_project(&[root.join("src")], None).unwrap();
            assert_eq!(project.manifest.is_some(), with_manifest);
            let messages: Vec<String> = analyze_project(project)
                .diagnostics
                .iter()
                .map(|diag| diag.error.to_string().replace(root.to_str().unwrap(), ""))
                .collect();

            let _ = fs::remove_dir_all(root);
            messages
        };

        let without = messages(false);
        assert!(!without.is_empty());
        assert_eq!(without, messages(true));
    }
}
