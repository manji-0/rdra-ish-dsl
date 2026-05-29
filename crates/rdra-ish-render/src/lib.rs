//! rdra-render: PlantUML → SVG/PNG renderer.
//!
//! Default backend: local `plantuml.jar` via stdin/stdout pipe.
//! Optional backend: Kroki HTTP API (feature `kroki`).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error(
        "plantuml not found: {msg}\nHint: set PLANTUML_JAR=/path/to/plantuml.jar or JAVA_HOME"
    )]
    NotFound { msg: String },

    #[error("plantuml exited with status {code}:\n{stderr}")]
    ProcessFailed { code: i32, stderr: String },

    #[error("plantuml process error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "kroki")]
    #[error("kroki HTTP error: {0}")]
    Http(String),
}

/// Output format for rendered diagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Svg,
    Png,
}

impl RenderFormat {
    pub fn flag(&self) -> &'static str {
        match self {
            RenderFormat::Svg => "-tsvg",
            RenderFormat::Png => "-tpng",
        }
    }
    pub fn extension(&self) -> &'static str {
        match self {
            RenderFormat::Svg => "svg",
            RenderFormat::Png => "png",
        }
    }
}

/// Common trait for diagram renderers.
pub trait DiagramRenderer {
    /// Render PlantUML source text to bytes (SVG/PNG).
    fn render(&self, puml: &str, format: RenderFormat) -> Result<Vec<u8>, RenderError>;
}

/// Renderer that invokes a local `plantuml.jar` via stdin/stdout.
#[derive(Debug)]
pub struct PlantumlCliRenderer {
    pub java: std::path::PathBuf,
    pub jar: std::path::PathBuf,
}

impl PlantumlCliRenderer {
    /// Create from explicit paths.
    pub fn new(java: std::path::PathBuf, jar: std::path::PathBuf) -> Self {
        Self { java, jar }
    }

    /// Auto-discover java and plantuml.jar.
    /// Search order:
    ///   1. `PLANTUML_JAR` env var (for jar) + `JAVA_HOME/bin/java` or `java` in PATH
    ///   2. `java` in PATH
    pub fn discover() -> Result<Self, RenderError> {
        let jar = discover_jar()?;
        let java = discover_java()?;
        Ok(Self { java, jar })
    }
}

impl DiagramRenderer for PlantumlCliRenderer {
    fn render(&self, puml: &str, format: RenderFormat) -> Result<Vec<u8>, RenderError> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&self.java)
            .arg("-jar")
            .arg(&self.jar)
            .arg(format.flag())
            .arg("-pipe")        // read from stdin, write to stdout
            .arg("-charset")
            .arg("UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write PlantUML source to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(puml.as_bytes())?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(RenderError::ProcessFailed { code, stderr });
        }

        Ok(output.stdout)
    }
}

fn discover_jar() -> Result<std::path::PathBuf, RenderError> {
    // 1. PLANTUML_JAR env var
    if let Ok(jar) = std::env::var("PLANTUML_JAR") {
        let p = std::path::PathBuf::from(&jar);
        if p.exists() {
            return Ok(p);
        }
    }
    // 2. Common locations
    let candidates = [
        "/usr/local/lib/plantuml.jar",
        "/usr/share/plantuml/plantuml.jar",
        "/opt/homebrew/opt/plantuml/libexec/plantuml.jar",
    ];
    for c in &candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() {
            return Ok(p);
        }
    }
    // 3. `plantuml` binary in PATH that wraps the jar
    if let Ok(output) = std::process::Command::new("plantuml")
        .arg("-version")
        .output()
    {
        if output.status.success() {
            // plantuml binary found; we'll use it differently — just record a sentinel
            // Actually for our stdin/stdout approach we need java + jar.
            // Fall through to error.
        }
    }
    Err(RenderError::NotFound {
        msg: "plantuml.jar not found. Set PLANTUML_JAR=/path/to/plantuml.jar".to_string(),
    })
}

fn discover_java() -> Result<std::path::PathBuf, RenderError> {
    // 1. JAVA_HOME/bin/java
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let p = std::path::PathBuf::from(java_home).join("bin").join("java");
        if p.exists() {
            return Ok(p);
        }
    }
    // 2. `java` in PATH
    let which = std::process::Command::new("which").arg("java").output();
    if let Ok(out) = which {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Ok(std::path::PathBuf::from(s));
            }
        }
    }
    // 3. `java` as bare command (let OS resolve PATH at spawn time)
    Ok(std::path::PathBuf::from("java"))
}

/// Convenience: render puml text to a file.
pub fn render_to_file(
    renderer: &dyn DiagramRenderer,
    puml: &str,
    out_path: &std::path::Path,
    format: RenderFormat,
) -> Result<(), RenderError> {
    let bytes = renderer.render(puml, format)?;
    std::fs::write(out_path, bytes)?;
    Ok(())
}

#[cfg(feature = "kroki")]
pub mod kroki {
    use super::*;

    pub struct KrokiRenderer {
        pub endpoint: String,
    }

    impl KrokiRenderer {
        pub fn new(endpoint: impl Into<String>) -> Self {
            Self {
                endpoint: endpoint.into(),
            }
        }
        pub fn default_saas() -> Self {
            Self::new("https://kroki.io")
        }
    }

    impl DiagramRenderer for KrokiRenderer {
        fn render(&self, puml: &str, format: RenderFormat) -> Result<Vec<u8>, RenderError> {
            // POST to https://kroki.io/plantuml/svg  (or png)
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            use flate2::{write::ZlibEncoder, Compression};
            use std::io::Write;

            let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
            enc.write_all(puml.as_bytes())
                .map_err(RenderError::Io)?;
            let compressed = enc.finish().map_err(RenderError::Io)?;
            let encoded = STANDARD.encode(&compressed);

            let fmt = match format {
                RenderFormat::Svg => "svg",
                RenderFormat::Png => "png",
            };
            let url = format!("{}/plantuml/{}/{}", self.endpoint, fmt, encoded);

            let response =
                reqwest::blocking::get(&url).map_err(|e| RenderError::Http(e.to_string()))?;
            if !response.status().is_success() {
                return Err(RenderError::Http(format!("HTTP {}", response.status())));
            }
            Ok(response
                .bytes()
                .map_err(|e| RenderError::Http(e.to_string()))?
                .to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_returns_err_without_jar() {
        // PLANTUML_JAR が設定されておらず、デフォルトパスにもない環境では Err が返る
        // CI環境では必ずそうなるはず
        // ただし discover_java() は "java" バイナリとして成功するので、
        // discover_jar() だけをテストする
        use std::env;
        let saved = env::var("PLANTUML_JAR").ok();
        env::remove_var("PLANTUML_JAR");
        // jarが存在しない場合のみ
        let candidates_exist = [
            "/usr/local/lib/plantuml.jar",
            "/usr/share/plantuml/plantuml.jar",
            "/opt/homebrew/opt/plantuml/libexec/plantuml.jar",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).exists());
        if !candidates_exist {
            let result = PlantumlCliRenderer::discover();
            assert!(result.is_err(), "expected Err when no jar found");
            let err = result.unwrap_err().to_string();
            assert!(err.contains("plantuml.jar") || err.contains("not found"));
        }
        if let Some(v) = saved {
            env::set_var("PLANTUML_JAR", v);
        }
    }

    #[test]
    fn test_render_format_flags() {
        assert_eq!(RenderFormat::Svg.flag(), "-tsvg");
        assert_eq!(RenderFormat::Png.flag(), "-tpng");
        assert_eq!(RenderFormat::Svg.extension(), "svg");
        assert_eq!(RenderFormat::Png.extension(), "png");
    }
}
