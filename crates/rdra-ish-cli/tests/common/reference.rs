//! The reference services of tests/fixtures/codegen as modules of a crate or package.

// A test binary that includes this file uses part of it.
#![allow(dead_code)]

use std::path::Path;

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/codegen"
    ))
}

/// The reference Rust service of codegen_conformance.rs as a module beside `domain`: its
/// `main` dropped, and `Service::new()` reading the fault from `FAULT`.
pub fn rust_service() -> String {
    let text = std::fs::read_to_string(fixtures().join("conformance_service.rs")).unwrap();
    let text = text
        .replace(
            "#[path = \"domain.rs\"]\n#[allow(dead_code)] // the service uses part of the module\nmod domain;\nuse domain::*;",
            "use crate::domain::*;",
        )
        .replace("struct Service {", "pub struct Service {");
    let main = text.find("fn main()").expect("the service has a main");
    format!(
        "{}\nimpl Service {{\n    /// A service with the fault `FAULT` names planted.\n    pub fn new() -> Self {{\n        Service {{ fault: std::env::var(\"FAULT\").unwrap_or_default(), ..Service::default() }}\n    }}\n}}\n",
        &text[..main]
    )
}
