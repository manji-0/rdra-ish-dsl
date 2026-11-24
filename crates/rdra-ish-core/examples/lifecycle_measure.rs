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
//! `RDRA_REDUCED_ONLY=1` skips the unreduced exploration. `RDRA_LAYERS=1` prints, under each
//! run of a thousand states and more, reduced or not, the states first reached at each depth
//! with the depth being expanded and how many of its states were, and for a complete run the
//! same for that cone rerun under a state cap of 10% to 90% of its states, with what the
//! estimate makes of each rerun.
//!
//! Under each temporal property's line it prints what the graph passes cost on the cone's
//! run: moves, self-loops, the time to build the adjacency without self-loops, and the time
//! of an iterative Tarjan pass over it with the number of components, once for the cone
//! unreduced and once reduced.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;

use rdra_ish_core::lifecycle::{
    store_bytes, Bounds, Caps, ComponentRun, Cone, Held, Layers, Scope, Seed, Target,
    TransitionSystem, VarId, VarKind,
};
use rdra_ish_core::memory::MemorySize;
use rdra_ish_core::project::load_project;

fn target_name(ts: &TransitionSystem, t: Target) -> String {
    match t {
        Target::Entity(e) => format!("view {}", ts.entity(e).id),
        Target::Invariant(i) => format!("inv {}", ts.invariants()[i].name),
        Target::AfterAssert(i) => format!("after {}", ts.after_asserts()[i].name),
        Target::Reach(i) => format!("can_reach {}", ts.reach_properties()[i].name),
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

/// The cost of the graph passes on a complete run: moves, self-loops, the adjacency, and the
/// components of the forward graph.
fn graph_stats(run: &ComponentRun) -> String {
    if !run.complete {
        return "capped".to_string();
    }
    let graph = run.graph();
    let loops = graph.edges().filter(|(from, to, _)| from == to).count();
    let start = Instant::now();
    let adjacency = graph.adjacency();
    let adjacency_ms = start.elapsed().as_secs_f64() * 1e3;
    let start = Instant::now();
    let (_, components) = adjacency.components();
    let components_ms = start.elapsed().as_secs_f64() * 1e3;
    format!(
        "{} states, {} edges, {} self-loops, adjacency {:.1} ms, components {} in {:.1} ms",
        run.states.len(),
        run.edges().len(),
        loops,
        adjacency_ms,
        components,
        components_ms
    )
}

/// The bytes `Caps::memory` counts for a complete run: states, edges, the state width, and the
/// peak store (with `seen`) and the kept store per state.
fn bytes_stats(run: &ComponentRun) -> String {
    if !run.complete {
        return "capped".to_string();
    }
    let n = run.states.len() as f64;
    let model = store_bytes(
        run.states.len() as u64,
        run.state_width(),
        run.edges().len() as u64,
    );
    format!(
        "bytes: {} states, {} edges ({:.2}/state), width {} B, peak {} B ({:.1} B/state, admitted {:.1}), kept {} B ({:.1} B/state), model {} B ({:+.1}%)",
        run.states.len(),
        run.edges().len(),
        run.edges().len() as f64 / n,
        run.state_width(),
        run.peak,
        run.peak as f64 / n,
        run.peak_admitted as f64 / n,
        run.kept_bytes(),
        run.kept_bytes() as f64 / n,
        model,
        (model as f64 / run.peak as f64 - 1.0) * 100.0
    )
}

/// `reading expanded | new states per depth`.
fn layers_line(layers: &Layers) -> String {
    let new: Vec<String> = layers.new.iter().map(u64::to_string).collect();
    format!("{} {} | {}", layers.reading, layers.expanded, new.join(" "))
}

/// The layers of a run of a thousand states and more, and, for a complete one, of the same
/// system rerun under a state cap of 10% to 90% of its states. Under each rerun: what
/// `Layers::estimate` makes of it, the store `MemoryCapReached` would name for it
/// (`Held::bytes_for` the states to cover, at the rerun's edges per expanded state), and the
/// complete run's peak store.
fn print_layers(name: &str, system: &TransitionSystem, run: &ComponentRun) {
    if run.states.len() < 1000 {
        return;
    }
    if !run.complete {
        println!("    layers {name} capped: {}", layers_line(&run.layers));
        return;
    }
    println!("    layers {name}: {}", layers_line(&run.layers));
    for pct in [10, 25, 50, 60, 75, 90] {
        let at = run.states.len() * pct / 100;
        let stopped = system.clone().run(Caps::new(usize::MAX, at));
        println!(
            "    layers {name} at {pct}%: {}",
            layers_line(&stopped.layers)
        );
        let estimate = stopped.layers.estimate();
        let held = Held {
            bytes: stopped.peak,
            width: stopped.state_width(),
            edges: stopped.edges().len() as u64,
            bound: None,
            layers: stopped.layers.clone(),
        };
        let need = estimate.states_to_cover().map_or("-".to_string(), |n| {
            MemorySize::covering(held.bytes_for(n)).bytes().to_string()
        });
        println!(
            "    estimate {name} at {pct}%: {estimate:?} need {need} peak {} states {}",
            run.peak,
            run.states.len()
        );
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
    let show_vars = std::env::var_os("RDRA_CONE_VARS").is_some();
    let reduced_only = std::env::var_os("RDRA_REDUCED_ONLY").is_some();
    let show_layers = std::env::var_os("RDRA_LAYERS").is_some();
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
        let built = TransitionSystem::build(&model, bounds.clone(), &Scope::ALL);
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
        let mut seen: HashMap<Cone, (usize, String, String, String)> = HashMap::new();
        let mut reduced_seen: HashMap<Cone, (String, String, String, String)> = HashMap::new();
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
            let (states, ms, graph, bytes) = match seen.get(&cone) {
                Some((n, _, g, b)) => (*n, "cached".to_string(), g.clone(), b.clone()),
                None if reduced_only => (0, "-".to_string(), "-".to_string(), "-".to_string()),
                None => {
                    let start = Instant::now();
                    let system = ts.restrict_to(&cone.vars, &cone.keep);
                    let run = if show_layers {
                        let run = system.clone().run(caps);
                        print_layers(&format!("unreduced {name}"), &system, &run);
                        run
                    } else {
                        system.run(caps)
                    };
                    let n = if run.complete {
                        run.states.len()
                    } else {
                        usize::MAX
                    };
                    let ms = start.elapsed().as_millis().to_string();
                    let graph = graph_stats(&run);
                    let bytes = bytes_stats(&run);
                    seen.insert(cone.clone(), (n, ms.clone(), graph.clone(), bytes.clone()));
                    (n, ms, graph, bytes)
                }
            };
            let (reduced, sub, rms, reduced_graph, reduced_bytes) = match reduced_seen.get(&cone) {
                Some((r, s, g, b)) => (
                    r.clone(),
                    s.clone(),
                    "cached".to_string(),
                    g.clone(),
                    b.clone(),
                ),
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
                    let run = if show_layers {
                        let run = system.clone().run(caps);
                        print_layers(&format!("reduced {name}"), &system, &run);
                        run
                    } else {
                        system.run(caps)
                    };
                    let n = if run.complete {
                        run.states.len().to_string()
                    } else {
                        format!(">{cap}")
                    };
                    let ms = start.elapsed().as_millis().to_string();
                    let graph = graph_stats(&run);
                    let bytes = bytes_stats(&run);
                    reduced_seen.insert(
                        cone.clone(),
                        (n.clone(), sub.clone(), graph.clone(), bytes.clone()),
                    );
                    (n, sub, ms, graph, bytes)
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
            println!("    unreduced {bytes}");
            println!("    reduced   {reduced_bytes}");
            if name.starts_with("property ") {
                println!("    unreduced: {graph}");
                println!("    reduced:   {reduced_graph}");
            }
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
