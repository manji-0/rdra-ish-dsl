//! What `codegen_equivalence.rs` and `codegen_differential.rs` share: the samples `check`
//! passes, BFS's run of each at `Bounds::STATES`, the IR built at the same caps, the
//! function of the IR each BFS edge runs, and the IR's conditions read on a BFS state.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rdra_ish_core::lifecycle::{
    Action, Bounds, Caps, ComponentRun, Scope, StateSlice, TransitionSystem,
};
use rdra_ish_core::{load_project, SemanticModel};
use rdra_ish_emit::codegen::build;
use rdra_ish_emit::codegen::ir::{DomainModel, FnDecl, OutKind};

/// The caps the IR is built at, and the printed code generated at.
pub fn caps() -> Caps {
    Caps::for_instances(1)
}

/// A sample `check` passes: its model, the IR, and BFS's run of the whole system.
pub struct Explored {
    pub model: SemanticModel,
    pub domain: DomainModel,
    pub run: ComponentRun,
}

pub fn explore(sample: &str) -> Explored {
    let project = load_project(&[repo().join(sample)], None).expect("loads");
    let model = project.model;
    let domain = build(&model, Some(caps()), &|_| None).expect("builds");
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    // The whole system, not a cone, and charged to no pool: its first 65,536 states
    // sample the moves.
    let run = built.ts.clone().run(Caps::new(caps().patterns, 1 << 16));
    Explored { model, domain, run }
}

/// The `(entity id, slot)` of each row of `f`, as the edge's params bind them.
pub fn row_slots(
    ts: &TransitionSystem,
    action: &Action,
    params: &[u8],
    f: &FnDecl,
) -> Vec<(String, usize)> {
    f.rows
        .iter()
        .map(|r| {
            let param = r.param.expect("a function row binds a param");
            let e = action.params[param].entity;
            (ts.entity(e).id.clone(), usize::from(params[param]))
        })
        .collect()
}

/// The function of `domain` a BFS move of `action` with `params` from `pre` to `post` runs:
/// the one whose source states are the rows' states and, among several arms from one state,
/// whose target BFS took. `Err` says why there is not exactly one.
pub fn pick<'f>(
    domain: &'f DomainModel,
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    params: &[u8],
    pre: &StateSlice,
    post: &StateSlice,
) -> Result<&'f FnDecl, String> {
    // The rows as the first function of the action names them; every function of one
    // action binds the same params.
    let Some(first) = domain.fns.iter().find(|f| f.action == action.name) else {
        return Err(format!(
            "BFS runs {} but the generator printed no function and listed no skip",
            action.name
        ));
    };
    let before = Rows::new(
        ts,
        model,
        pre,
        row_slots(ts, action, params, first),
        Vec::new(),
    );
    let mut fns = matching(&domain.fns, &action.name, &before);
    // Several arms from one state: the function whose target BFS took.
    if fns.len() > 1 {
        fns.retain(|f| {
            f.outputs.iter().all(|o| match &o.kind {
                OutKind::Row {
                    tag: Some((column, value)),
                    ..
                } => {
                    let e = action.params[o.param].entity;
                    let slot = usize::from(params[o.param]);
                    column_var(ts, &ts.entity(e).id, column)
                        .and_then(|v| enum_name(ts, v, post.get(ts, v, slot)))
                        .is_some_and(|n| n == value.wire)
                }
                _ => true,
            })
        });
    }
    match fns.as_slice() {
        [f] => Ok(f),
        _ => Err(format!(
            "{} from {:?} matches {} functions",
            action.name,
            first.rows.iter().map(|r| &r.state).collect::<Vec<_>>(),
            fns.len()
        )),
    }
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub fn check_passes(sample: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["check", sample])
        .output()
        .expect("runs rdra-ish")
        .status
        .success()
}

/// The evaluator of the IR's conditions on a BFS state, shared with the conformance traces.
pub use rdra_ish_emit::codegen::conformance::eval::{column_var, enum_name, matching, Rows};

/// The type name codegen gives an id: PascalCase, `_` and `-` splitting words.
pub fn type_name(id: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in id.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}
