//! Measurement gate for the lifecycle layer. For every root in `samples.txt` it builds at
//! `Bounds::STATES` (or `Bounds::VERIFY`, the bounds of `verify`'s BFS pass) and explores the
//! cone of each target (each entity view, invariant, and after-assert) and of each temporal
//! property under a state cap. It prints the cone size and reached-state count per target and
//! the largest per sample, and beside them the cone with its independent sub-lifecycle reduced
//! as `verify` explores it (`TransitionSystem::reduced`): its reached states, and the states
//! and classes of the sub-lifecycle.
//!
//! Run: `cargo run --release -p rdra-ish-core --example lifecycle_measure -- STATE_CAP [verify [SAMPLE]]`
//! `SAMPLE` is a line of `samples.txt` or a path to any project. `RDRA_CONE_VARS=1` prints the
//! vars of each cone under its line, and the vars of its sub-lifecycle where it reduces.
//! `RDRA_REDUCED_ONLY=1` skips the unreduced exploration.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;

use rdra_ish_core::lifecycle::{
    Bounds, Caps, Cone, Scope, Seed, Target, TransitionSystem, VarId, VarKind,
};
use rdra_ish_core::project::load_project;

fn target_name(ts: &TransitionSystem, t: Target) -> String {
    match t {
        Target::Entity(e) => format!("view {}", ts.entity(e).id),
        Target::Invariant(i) => format!("inv {}", ts.invariants()[i].name),
        Target::AfterAssert(i) => format!("after {}", ts.after_asserts()[i].name),
    }
}

fn var_names(ts: &TransitionSystem, vars: impl IntoIterator<Item = VarId>) -> String {
    let names: Vec<String> = vars
        .into_iter()
        .map(|v| match &ts.var(v).kind {
            VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => {
                format!("{}.{}", ts.entity(*entity).id, ts.var_name(v))
            }
            _ => ts.var_name(v),
        })
        .collect();
    names.join(" ")
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
    let show_vars = std::env::var_os("RDRA_CONE_VARS").is_some();
    let reduced_only = std::env::var_os("RDRA_REDUCED_ONLY").is_some();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let list = std::fs::read_to_string(repo.join("samples.txt")).expect("samples.txt");
    let caps = Caps::new(usize::MAX, cap);
    println!("bounds: instances={instances} int_max=5  state cap per cone: {cap}");
    let listed: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| only.as_deref().is_none_or(|o| o == *l))
        .collect();
    let samples: Vec<&str> = match (&only, listed.is_empty()) {
        (Some(path), true) => vec![path.as_str()],
        _ => listed,
    };
    for sample in samples {
        let model = load_project(&[repo.join(sample)], None)
            .expect("sample loads")
            .model;
        let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
        let ts = &built.ts;
        println!("\n{sample}");
        println!(
            "  {:<44} {:>5} {:>8} {:>10} {:>7} {:>10} {:>14} {:>7}",
            "target", "vars", "entities", "states", "ms", "reduced", "S states/cls", "ms"
        );
        // The seed and keep of each target, by cone, as `verify` unites them.
        let mut shared: HashMap<Cone, Vec<Seed>> = HashMap::new();
        for t in ts.targets() {
            shared
                .entry(ts.target_cone(t))
                .or_default()
                .push(ts.target_seed(t));
        }
        for i in 0..ts.properties().len() {
            shared
                .entry(ts.property_cone(i))
                .or_default()
                .push(ts.property_seed(i));
        }
        let mut seen: HashMap<Cone, (usize, String)> = HashMap::new();
        let mut reduced_seen: HashMap<Cone, (String, String)> = HashMap::new();
        let mut sub_vars: Option<String> = None;
        let mut worst: (usize, String) = (0, String::new());
        let targets = ts
            .targets()
            .into_iter()
            .map(|t| (ts.target_cone(t), target_name(ts, t)));
        let properties = (0..ts.properties().len()).map(|i| {
            (
                ts.property_cone(i),
                format!("property {}", ts.properties()[i].name),
            )
        });
        for (cone, name) in targets.chain(properties) {
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
                None if reduced_only => (0, "-".to_string()),
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
            let (reduced, sub, rms) = match reduced_seen.get(&cone) {
                Some((r, s)) => (r.clone(), s.clone(), "cached".to_string()),
                None => {
                    let start = Instant::now();
                    let system = ts.reduced_cone(&cone, shared[&cone].iter().cloned(), caps);
                    let sub = match system.reduction() {
                        Some(r) => format!("{}/{}", r.states, r.classes()),
                        None => "-".to_string(),
                    };
                    sub_vars = system
                        .reduction()
                        .map(|r| var_names(&system, r.vars().iter().copied()));
                    let run = system.run(caps);
                    let n = if run.complete {
                        run.states.len().to_string()
                    } else {
                        format!(">{cap}")
                    };
                    reduced_seen.insert(cone.clone(), (n.clone(), sub.clone()));
                    (n, sub, start.elapsed().as_millis().to_string())
                }
            };
            let shown = if reduced_only {
                "-".to_string()
            } else if states == usize::MAX {
                format!(">{cap}")
            } else {
                states.to_string()
            };
            println!(
                "  {:<44} {:>5} {:>8} {:>10} {:>7} {:>10} {:>14} {:>7}",
                name,
                cone.vars.len(),
                entities.len(),
                shown,
                ms,
                reduced,
                sub,
                rms
            );
            if show_vars {
                println!("    vars: {}", var_names(ts, cone.vars.iter().copied()));
                if let Some(s) = sub_vars.take() {
                    println!("    sub-lifecycle: {s}");
                }
            }
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
