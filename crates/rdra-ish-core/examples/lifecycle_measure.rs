//! Measurement gate for the lifecycle layer. For every root in `samples.txt` it builds at
//! `Bounds::STATES` (or `Bounds::VERIFY`, the bounds of `verify`'s BFS pass) and explores the cone of each target (each entity view, invariant, and
//! after-assert) under a state cap. It prints the cone size and reached-state count per
//! target and the largest per sample.
//!
//! Run: `cargo run --release -p rdra-ish-core --example lifecycle_measure [-- STATE_CAP [verify [SAMPLE]]]`

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rdra_ish_core::lifecycle::{Bounds, Caps, Cone, Scope, Target, TransitionSystem, VarKind};
use rdra_ish_core::{build_merged_model, resolve};

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

fn main() {
    let cap: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("state cap"))
        .unwrap_or(1_000_000);
    let (bounds, instances) = match std::env::args().nth(2).as_deref() {
        Some("verify") => (Bounds::VERIFY, Bounds::VERIFY.instances),
        _ => (Bounds::STATES, Bounds::STATES.instances),
    };
    let only = std::env::args().nth(3);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let list = std::fs::read_to_string(repo.join("samples.txt")).expect("samples.txt");
    let caps = Caps::new(usize::MAX, cap);
    println!("bounds: instances={instances} int_max=5  state cap per cone: {cap}");
    for sample in list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| only.as_deref().is_none_or(|o| o == *l))
    {
        let mut files = Vec::new();
        rdra_files(&repo.join(sample), &mut files);
        let includes = include_paths(&files);
        let (program, _) = resolve(&files, &includes);
        let (model, _) = build_merged_model(&program, &includes);
        let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
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
    }
}
