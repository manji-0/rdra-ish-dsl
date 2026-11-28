//! The "no cone" column of the paper's ablation (docs/paper/eval/ablation.md, #594 T6): BFS on
//! the whole transition system `T_β(M)` with no cone, no reduction and no symmetry, under a
//! state cap.
//!
//! Run: `cargo run --release -p rdra-ish-core --example paper_whole -- PROJECT INSTANCES STATE_CAP`
//!
//! Prints one tab-separated line: project, instances, variables, actions, states (or
//! `>STATE_CAP` when the run stopped at the cap), complete, and the wall time in ms. The bounds
//! are `INSTANCES` slots per entity and Int `0..=5`, as `verify --instance-count INSTANCES`.

use std::num::NonZeroU8;
use std::path::Path;
use std::time::Instant;

use rdra_ish_core::lifecycle::{Bounds, Caps, Scope, TransitionSystem};
use rdra_ish_core::project::load_project;

fn main() {
    let mut args = std::env::args().skip(1);
    let project = args.next().expect("PROJECT");
    let instances: u8 = args
        .next()
        .map(|s| s.parse().expect("INSTANCES"))
        .unwrap_or(2);
    let cap: usize = args
        .next()
        .map(|s| s.parse().expect("STATE_CAP"))
        .unwrap_or(10_000_000);
    let model = load_project(&[Path::new(&project).to_path_buf()], None)
        .expect("project loads")
        .model;
    let bounds = Bounds::uniform(NonZeroU8::new(instances).expect("INSTANCES >= 1"), 5);
    let built = TransitionSystem::build(&model, bounds, &Scope::ALL);
    let ts = built.ts;
    let vars = ts.vars().len();
    let actions = ts.actions().len();
    let start = Instant::now();
    let mut caps = Caps::new(usize::MAX, cap);
    caps.views = false;
    let run = ts.run(caps);
    let ms = start.elapsed().as_millis();
    let states = if run.complete {
        run.states.len().to_string()
    } else {
        format!(">{cap}")
    };
    println!(
        "{project}\t{instances}\t{vars}\t{actions}\t{states}\t{}\t{ms}",
        run.complete
    );
}
