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
