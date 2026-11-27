//! The obligation ledger (docs/design/code-check.md, proposal D and decision 8): who owns each
//! obligation no type holds. An owner is declared by a marker line in the service code,
//! `rdra: obligation <name> [path]` after a comment token, or by `[code.obligations]` in
//! rdra.toml. The check confirms only that an owner is declared and its file exists: an
//! obligation "has an owner", never "holds". Language-neutral: a marker is found by its text in
//! any file under the service roots.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rdra_ish_emit::codegen::ir::Obligation;

use super::CodeDiagnostic;

/// The text a marker line holds.
pub(crate) const MARKER: &str = "rdra: obligation ";

/// One marker: the obligation it names, the file it names (if any), and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
    pub name: String,
    /// The owning file, relative to the root the marker was found under; the marker's own
    /// file when the marker names none.
    pub path: Option<String>,
    /// The file the marker is in, as shown.
    pub file: String,
    /// 1-based.
    pub line: usize,
}

/// Whether `name` can be an obligation's name: the generator builds each from model
/// identifiers, so letters, digits and `_` only.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The markers in `text`, the content of `file`. A marker inside a string literal (an odd
/// number of `"` or `` ` `` before it on its line) is code that prints one, not a marker, and
/// so is a name that is no identifier (`${ob}`).
pub(crate) fn scan(file: &str, text: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let Some((before, rest)) = line.split_once(MARKER) else {
            continue;
        };
        let quoted = |q: char| before.chars().filter(|c| *c == q).count() % 2 == 1;
        if quoted('"') || quoted('`') {
            continue;
        }
        let mut words = rest.split_whitespace();
        let Some(name) = words.next().filter(|n| is_name(n)) else {
            continue;
        };
        out.push(Marker {
            name: name.to_string(),
            path: words.next().map(str::to_string),
            file: file.to_string(),
            line: i + 1,
        });
    }
    out
}

/// What the ledger's owners are checked against.
pub(crate) struct Ledger<'a> {
    pub obligations: &'a [Obligation],
    /// The markers found, with the root each was found under.
    pub markers: &'a [(PathBuf, Marker)],
    /// `[code.obligations]`: name -> path relative to rdra.toml.
    pub table: &'a BTreeMap<String, PathBuf>,
    /// The directory of rdra.toml.
    pub manifest_dir: &'a Path,
    /// A path as the check shows it.
    pub shown: &'a dyn Fn(&Path) -> String,
    /// An obligation's site as the check shows it.
    pub site: &'a dyn Fn(&str) -> String,
}

impl Ledger<'_> {
    /// Whether the project keeps a ledger: a marker or a `[code.obligations]` entry.
    pub fn active(&self) -> bool {
        !self.markers.is_empty() || !self.table.is_empty()
    }
}

/// The ledger's findings: each obligation with no owner, each owner whose file does not
/// exist, and (as warnings) each owner of a name the model has no obligation of.
pub(crate) fn check(ledger: &Ledger<'_>) -> Vec<CodeDiagnostic> {
    let mut out = Vec::new();
    // The owners: (name, owning file, where it is declared).
    let mut owners: Vec<(&str, PathBuf, String)> = Vec::new();
    for (name, path) in ledger.table {
        owners.push((
            name,
            ledger.manifest_dir.join(path),
            "[code.obligations]".to_string(),
        ));
    }
    for (root, m) in ledger.markers {
        let file = match &m.path {
            Some(p) => root.join(p),
            None => PathBuf::from(&m.file),
        };
        owners.push((
            &m.name,
            file,
            format!("{}:{}", (ledger.shown)(Path::new(&m.file)), m.line),
        ));
    }
    for (name, file, at) in &owners {
        if !ledger.obligations.iter().any(|o| o.name == *name) {
            out.push(CodeDiagnostic {
                name: format!("CodeObligationUnknown_{name}"),
                message: format!("the model has no obligation {name}"),
                model: None,
                code: Some(at.clone()),
                fix: Some(
                    "rename it to an obligation the generated code lists, or remove it".to_string(),
                ),
                warning: true,
            });
        } else if !file.exists() {
            out.push(CodeDiagnostic {
                name: format!("CodeObligationOwnerMissing_{name}"),
                message: format!(
                    "the owner of {name} does not exist: {}",
                    (ledger.shown)(file)
                ),
                model: None,
                code: Some(at.clone()),
                fix: Some("name the file that keeps it".to_string()),
                warning: false,
            });
        }
    }
    for o in ledger.obligations {
        if owners.iter().any(|(name, _, _)| *name == o.name) {
            continue;
        }
        let text = o.text.lines().next().unwrap_or_default();
        out.push(CodeDiagnostic {
            name: format!("CodeObligationUnowned_{}", o.name),
            message: format!("no code owns the obligation {}", o.name),
            model: Some(match &o.site {
                Some(site) => format!("{}  {text}", (ledger.site)(site)),
                None => text.to_string(),
            }),
            code: None,
            fix: Some(format!(
                "add `// rdra: obligation {} <file that keeps it>` or list it under [code.obligations]",
                o.name
            )),
            warning: false,
        });
    }
    out
}

/// The files under `root` a marker may be in: not hidden, not a build or dependency
/// directory, at most 1 MiB.
pub(crate) fn files_under(root: &Path) -> Vec<PathBuf> {
    const SKIP: [&str; 4] = ["target", "node_modules", "vendor", "dist"];
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !SKIP.contains(&name) {
                    stack.push(path);
                }
            } else if path.metadata().is_ok_and(|m| m.len() <= 1 << 20) {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation(name: &str) -> Obligation {
        Obligation {
            name: name.to_string(),
            checked: false,
            text: format!("{name} text"),
            site: Some("m.rdra:3:1".to_string()),
        }
    }

    #[test]
    fn markers_are_read_after_any_comment_token() {
        let text = "fn a() {}\n// rdra: obligation Unique_Payment_Order migrations/003.sql\n-- rdra: obligation Ref_Order_customer_id\n# rdra: obligation\n";
        let m = scan("src/a.rs", text);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].name.as_str(), m[0].path.as_deref(), m[0].line),
            ("Unique_Payment_Order", Some("migrations/003.sql"), 2)
        );
        assert_eq!(
            (m[1].name.as_str(), m[1].path.as_deref()),
            ("Ref_Order_customer_id", None)
        );
    }

    #[test]
    fn a_marker_built_in_a_string_is_no_marker() {
        // svc/scripts/gen-schema.mjs of a dogfooding run: a script that prints markers.
        let text = "for (const ob of obligations) {\n  out.push(`-- rdra: obligation ${ob}`);\n}\nconst s = \"// rdra: obligation Unique_A x\";\n// rdra: obligation Ref_B_c_id \n";
        let m = scan("scripts/gen-schema.mjs", text);
        let names: Vec<&str> = m.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Ref_B_c_id"]);
    }

    #[test]
    fn every_obligation_needs_an_owner_whose_file_exists() {
        let dir = std::env::temp_dir().join(format!("rdra-ish-ledger-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("svc/src")).unwrap();
        std::fs::write(dir.join("svc/src/a.rs"), "").unwrap();
        let obligations = [
            obligation("A"),
            obligation("B"),
            obligation("C"),
            obligation("D"),
        ];
        let markers = [
            (
                dir.join("svc"),
                Marker {
                    name: "A".to_string(),
                    path: None,
                    file: dir.join("svc/src/a.rs").display().to_string(),
                    line: 1,
                },
            ),
            (
                dir.join("svc"),
                Marker {
                    name: "B".to_string(),
                    path: Some("migrations/none.sql".to_string()),
                    file: dir.join("svc/src/a.rs").display().to_string(),
                    line: 2,
                },
            ),
        ];
        let table: BTreeMap<String, PathBuf> = [
            ("C".to_string(), PathBuf::from("svc/src/a.rs")),
            ("Gone".to_string(), PathBuf::from("svc/src/a.rs")),
        ]
        .into();
        let shown = |p: &Path| p.display().to_string();
        let site = |s: &str| format!("model/{s}");
        let ledger = Ledger {
            obligations: &obligations,
            markers: &markers,
            table: &table,
            manifest_dir: &dir,
            shown: &shown,
            site: &site,
        };
        assert!(ledger.active());
        let found = check(&ledger);
        let names: Vec<(&str, bool)> = found.iter().map(|d| (d.name.as_str(), d.warning)).collect();
        assert_eq!(
            names,
            [
                ("CodeObligationUnknown_Gone", true),
                ("CodeObligationOwnerMissing_B", false),
                ("CodeObligationUnowned_D", false),
            ]
        );
        assert_eq!(
            found[2].to_string(),
            "error[CodeObligationUnowned_D]: no code owns the obligation D\n  \
             model: model/m.rdra:3:1  D text\n  \
             fix:   add `// rdra: obligation D <file that keeps it>` or list it under [code.obligations]"
        );
        let none = Ledger {
            markers: &[],
            table: &BTreeMap::new(),
            ..ledger
        };
        assert!(!none.active());
    }
}
