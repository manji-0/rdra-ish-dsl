//! Measurement gate for the lifecycle layer. For every root in `samples.txt` it builds at
//! `Bounds::STATES` and explores the cone of each target (each entity view, invariant, and
//! after-assert) under a state cap. It prints the cone size and reached-state count per
//! target and the largest per sample, then each entity whose cone view differs from today's
//! `states`.
//!
//! Run: `cargo run --release -p rdra-ish-core --example lifecycle_measure [-- STATE_CAP]`

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rdra_ish_core::lifecycle::{Bounds, Caps, Cone, Scope, Target, TransitionSystem, VarKind};
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

fn target_name(ts: &TransitionSystem, t: Target) -> String {
    match t {
        Target::Entity(e) => format!("view {}", ts.entity(e).id),
        Target::Invariant(i) => format!("inv {}", ts.invariants()[i].name),
        Target::AfterAssert(i) => format!("after {}", ts.after_asserts()[i].name),
    }
}

fn rows(r: &rdra_ish_core::lifecycle::EntityStateResult) -> String {
    r.patterns
        .iter()
        .map(|p| {
            p.pattern
                .values
                .iter()
                .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn old_rows(r: &rdra_ish_core::EntityStateResult) -> String {
    r.patterns
        .iter()
        .map(|p| {
            p.pattern
                .values
                .iter()
                .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn main() {
    let cap: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("state cap"))
        .unwrap_or(1_000_000);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let list = std::fs::read_to_string(repo.join("samples.txt")).expect("samples.txt");
    let caps = Caps::new(usize::MAX, cap);
    println!("bounds: instances=1 int_max=5  state cap per cone: {cap}");
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
        let ts = &built.ts;
        println!("\n{sample}");
        println!(
            "  {:<44} {:>5} {:>8} {:>10} {:>7}",
            "target", "vars", "entities", "states", "ms"
        );
        let mut seen: HashMap<Cone, (usize, String)> = HashMap::new();
        let mut worst: (usize, String) = (0, String::new());
        for t in ts.targets() {
            let cone = ts.target_cone(t);
            let entities: BTreeSet<_> = cone
                .vars
                .iter()
                .filter_map(|v| match ts.var(*v).kind {
                    VarKind::Exists(e) => Some(e),
                    _ => None,
                })
                .collect();
            let (states, ms) = match seen.get(&cone) {
                Some((n, _)) => (*n, "cached".to_string()),
                None => {
                    let start = Instant::now();
                    let run = ts.restrict_to(&cone.vars, &cone.keep).run(caps);
                    let n = if run.complete {
                        run.states.len()
                    } else {
                        usize::MAX
                    };
                    let ms = start.elapsed().as_millis().to_string();
                    seen.insert(cone.clone(), (n, ms.clone()));
                    (n, ms)
                }
            };
            let shown = if states == usize::MAX {
                format!(">{cap}")
            } else {
                states.to_string()
            };
            let name = target_name(ts, t);
            println!(
                "  {:<44} {:>5} {:>8} {:>10} {:>7}",
                name,
                cone.vars.len(),
                entities.len(),
                shown,
                ms
            );
            if states >= worst.0 {
                let ids: Vec<&str> = entities.iter().map(|e| ts.entity(*e).id.as_str()).collect();
                worst = (states, format!("{name} [{}]", ids.join(",")));
            }
        }
        let shown = if worst.0 == usize::MAX {
            format!(">{cap}")
        } else {
            worst.0.to_string()
        };
        println!("  max: {shown} states, {}", worst.1);

        let exploration = ts.explore_targets(caps);
        let views = exploration.entity_views(ts, &model, &built.diags);
        let old = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for new in &views {
            let Some(old) = old.iter().find(|o| o.entity_id == new.entity_id) else {
                continue;
            };
            let (o, n) = (old_rows(old), rows(new));
            if o != n {
                view_diffs.push(format!(
                    "{sample} {}: states {} patterns, lifecycle {} patterns\n    states:    {o}\n    lifecycle: {n}",
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
