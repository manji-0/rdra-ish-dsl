#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use rdra_ish_core::{analyze_workspace, RdraError};

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
}
