//! Measurement gate for the lifecycle layer: for every root in `samples.txt`, build at
//! `Bounds::STATES`, explore each component under a state cap, and print the entity count and
//! reached-state count per component. Then compare each entity's view with today's `states`.
//!
//! Run: `cargo run --release -p rdra-ish-core --example lifecycle_measure [-- STATE_CAP]`

use std::path::{Path, PathBuf};
use std::time::Instant;

use rdra_ish_core::lifecycle::{Bounds, Caps, Scope, TransitionSystem};
use rdra_ish_core::{build_merged_model, derive_state_patterns, resolve, DEFAULT_PATTERN_CAP};

fn rdra_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .expect("readable sample dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rdra_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rdra") {
            out.push(path);
        }
    }
}

/// The CLI's include rule: each file's parent and grandparent.
fn include_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for f in files {
        let canon = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        if let Some(parent) = canon.parent() {
            for a in [parent, parent.parent().unwrap_or(parent)] {
                if !paths.iter().any(|p| p == a) {
                    paths.push(a.to_path_buf());
                }
            }
        }
    }
    paths
}

fn main() {
    let cap: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("state cap"))
        .unwrap_or(1_000_000);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let list = std::fs::read_to_string(repo.join("samples.txt")).expect("samples.txt");
    println!("bounds: instances=1 int_max=5  state cap per component: {cap}");
    println!(
        "{:<58} {:>4} {:>8} {:>10} {:>9}  entities",
        "sample", "comp", "entities", "states", "ms"
    );
    let mut view_diffs: Vec<String> = Vec::new();
    for sample in list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut files = Vec::new();
        rdra_files(&repo.join(sample), &mut files);
        let includes = include_paths(&files);
        let (program, _) = resolve(&files, &includes);
        let (model, _) = build_merged_model(&program, &includes);
        let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
        let mut runs = Vec::new();
        for (i, comp) in built.ts.components().iter().enumerate() {
            let sub = built.ts.restrict(comp);
            let ids: Vec<String> = sub.entities().iter().map(|e| e.id.clone()).collect();
            let start = Instant::now();
            let run = sub.run(Caps::new(usize::MAX, cap));
            let ms = start.elapsed().as_millis();
            let states = if run.complete {
                run.states.len().to_string()
            } else {
                format!(">{}", run.states.len())
            };
            let clock = if comp.now { " +now" } else { "" };
            println!(
                "{:<58} {:>4} {:>8} {:>10} {:>9}  {}{clock}",
                sample,
                i,
                ids.len(),
                states,
                ms,
                ids.join(",")
            );
            runs.push(run);
        }
        let exploration = rdra_ish_core::lifecycle::Exploration { runs };
        let views = exploration.entity_views(&model, &built.diags);
        let old = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for new in &views {
            let Some(old) = old.iter().find(|o| o.entity_id == new.entity_id) else {
                continue;
            };
            let render = |values: Vec<String>| values.join(" | ");
            let old_rows = render(
                old.patterns
                    .iter()
                    .map(|p| {
                        p.pattern
                            .values
                            .iter()
                            .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect(),
            );
            let new_rows = render(
                new.patterns
                    .iter()
                    .map(|p| {
                        p.pattern
                            .values
                            .iter()
                            .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect(),
            );
            if old_rows != new_rows {
                view_diffs.push(format!(
                    "{sample} {}: states {} patterns, lifecycle {} patterns\n    states:    {old_rows}\n    lifecycle: {new_rows}",
                    new.entity_id,
                    old.patterns.len(),
                    new.patterns.len()
                ));
            }
        }
    }
    println!(
        "\nentities whose view differs from today's `states` ({}):",
        view_diffs.len()
    );
    for d in view_diffs {
        println!("  {d}");
    }
}
