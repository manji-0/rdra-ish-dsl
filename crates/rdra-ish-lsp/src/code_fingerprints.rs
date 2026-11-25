//! The LSP's part of `check --code` (docs/design/code-check.md): it generates nothing and
//! starts no compiler, so it only matches the fingerprint line of each file `[code.<lang>]`
//! names against the hash of the model's source text, and says so as information on
//! rdra.toml. A comment-only edit of the model reads as a change here; `check --code` decides.

use std::path::{Path, PathBuf};

use rdra_ish_core::fingerprint;
use rdra_ish_core::project::Manifest;
use rdra_ish_core::ResolvedProgram;

/// A note on one line (0-based) of rdra.toml.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeNote {
    pub line: u32,
    pub message: String,
}

/// The notes on the files `manifest`'s `[code]` names, for the model `program` loaded.
pub fn code_notes(manifest: &Manifest, program: &ResolvedProgram) -> Vec<CodeNote> {
    let Some(code) = &manifest.code else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(&manifest.path).unwrap_or_default();
    let model = fingerprint::model_hash(program.sources.iter().map(|(_, t, _)| t.as_str()));
    let mut out = Vec::new();
    for (name, target) in code.targets() {
        let line = table_line(&text, name);
        let files: Vec<&PathBuf> = [
            Some(&target.domain),
            target.traces.as_ref(),
            target.probe.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        for file in files {
            if let Some(message) = note(&manifest.dir().join(file), file, &model) {
                out.push(CodeNote { line, message });
            }
        }
    }
    out
}

fn note(path: &Path, shown: &Path, model: &str) -> Option<String> {
    let shown = shown.display();
    match fingerprint::read_file(path) {
        Err(_) => Some(format!(
            "{shown} does not exist yet: `rdra-ish codegen <model> --code` writes it"
        )),
        Ok(None) => Some(format!(
            "{shown} has no fingerprint line: it was not written by `rdra-ish codegen`"
        )),
        Ok(Some(f)) if f.model != model => Some(format!(
            "the model changed since {shown} was generated: `rdra-ish check <model> --code` tells whether its code did"
        )),
        Ok(Some(_)) => None,
    }
}

/// The 0-based line of `[code.<name>]` in `text`, or of `[code]`, or 0.
fn table_line(text: &str, name: &str) -> u32 {
    let header = format!("[code.{name}]");
    let find = |h: &str| {
        text.lines()
            .position(|l| l.trim_start().starts_with(h))
            .map(|i| i as u32)
    };
    find(&header).or_else(|| find("[code")).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_missing_or_unstamped_file_is_noted_on_its_table() {
        let dir = std::env::temp_dir().join(format!("rdra-lsp-code-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.as_path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join("m.rdra"), "actor A \"A\"\n").unwrap();
        std::fs::write(
            root.join("rdra.toml"),
            "[model]\nname = \"m\"\n\n[code.rust]\ndomain = \"d.rs\"\ntraces = \"t.txt\"\n\n[code.go]\ndomain = \"d.go\"\n",
        )
        .unwrap();
        let project = rdra_ish_core::load_project(&[root.join("m.rdra")], None).unwrap();
        let manifest = project.manifest.clone().unwrap();
        let model =
            fingerprint::model_hash(project.program.sources.iter().map(|(_, t, _)| t.as_str()));
        std::fs::write(
            root.join("d.rs"),
            format!("// head\n// rdra-ish codegen fingerprint: model={model} code=0\n"),
        )
        .unwrap();
        std::fs::write(
            root.join("t.txt"),
            "# rdra-ish conformance traces 3\n# rdra-ish codegen fingerprint: model=00 code=0\n",
        )
        .unwrap();
        let notes = code_notes(&manifest, &project.program);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert_eq!(notes[0].line, 3);
        assert!(
            notes[0].message.contains("the model changed since t.txt"),
            "{notes:?}"
        );
        assert_eq!(notes[1].line, 7);
        assert!(
            notes[1].message.contains("d.go does not exist yet"),
            "{notes:?}"
        );
    }
}
