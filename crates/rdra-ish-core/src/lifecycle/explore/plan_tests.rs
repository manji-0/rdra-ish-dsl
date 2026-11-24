//! The planned move generator (#253) against `eval` and its definition on every shipped model.

use crate::lifecycle::expr::{Env, State, StateSlice};
use crate::lifecycle::{Bounds, Caps, Scope, TransitionSystem, Value};
use crate::model::SemanticModel;

use super::plan::{conjuncts, one_row};

/// Every model the repository ships: the roots `samples.txt` lists and the engine corpus.
fn shipped_models() -> Vec<(String, SemanticModel)> {
    let root = crate::lifecycle::tests::repo_root();
    let listed = std::fs::read_to_string(root.join("samples.txt")).expect("samples.txt");
    let mut corpus: Vec<std::path::PathBuf> =
        std::fs::read_dir(root.join("crates/rdra-ish-cli/tests/engine_corpus"))
            .expect("the engine corpus")
            .map(|e| e.expect("a corpus entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
            .collect();
    corpus.sort();
    listed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| root.join(l))
        .chain(corpus)
        .map(|path| {
            let model = crate::load_project(std::slice::from_ref(&path), None)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .model;
            (path.display().to_string(), model)
        })
        .collect()
}

/// SplitMix64, so the random states are the same on every run.
struct Mix(u64);

impl Mix {
    fn below(&mut self, n: usize) -> u8 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) % n.max(1) as u64) as u8
    }
}

/// A state of `ts` with every byte drawn from its var's domain, reachable or not.
fn random_state(ts: &TransitionSystem, mix: &mut Mix) -> State {
    let mut state = ts.base_state();
    for var in ts.var_ids() {
        let size = ts.var(var).domain.size(ts);
        for slot in 0..ts.slots_of(&ts.var(var).kind) {
            state.set(ts, var, slot, Value(mix.below(size)));
        }
    }
    state
}

/// The planned move generator against its definition on one state: each row test agrees
/// with `eval` on the conjunct it stands for under every binding, and the enabled bindings
/// are the `bindings` whose pre-guards hold, in order. Returns how many bindings are enabled.
fn assert_planned_agrees(name: &str, ts: &TransitionSystem, state: &StateSlice) -> usize {
    let mut enabled = 0;
    for (action, plan) in ts.actions().iter().zip(ts.plans()) {
        let all = ts.bindings(action, state);
        for guard in action.guards.iter().filter(|g| !g.reads_successor()) {
            for part in conjuncts(&guard.pred) {
                let Some((param, var)) = one_row(part) else {
                    continue;
                };
                let test = ts.row_test(action, part, param, var);
                for params in &all {
                    let slot = usize::from(params[usize::from(param)]);
                    assert_eq!(
                        test.admits(state.get(ts, var, slot).0),
                        ts.eval(part, state, &mut Env::new(params)),
                        "{name}: {} {part:?} on {params:?} in {state:?}",
                        action.name
                    );
                }
            }
        }
        let want: Vec<Box<[u8]>> = all
            .into_iter()
            .filter(|params| ts.pre_guards_hold(action, state, params))
            .map(Vec::into_boxed_slice)
            .collect();
        let mut got: Vec<Box<[u8]>> = Vec::new();
        ts.each_planned(action, plan, state, |params| got.push(params.into()));
        assert_eq!(got, want, "{name}: {} in {state:?}", action.name);
        enabled += got.len();
    }
    enabled
}

/// The row tests and the planned enabled bindings agree with `eval` and the definition on
/// every shipped model: on each system `verify` explores (the whole system and every
/// target's cone, unreduced and reduced), on the states a capped run of each cone reaches,
/// and on random states.
#[test]
fn planned_enabled_bindings_agree_with_eval_on_every_shipped_model() {
    let caps = Caps {
        states: 300,
        ..Caps::VERIFY
    };
    let mut mix = Mix(253);
    let (mut models, mut systems, mut states, mut enabled) = (0, 0, 0, 0);
    for (name, model) in shipped_models() {
        models += 1;
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let targets = ts.targets();
        let runs = [
            ts.explore_targets(&targets, caps),
            ts.explore_targets_reduced(&targets, caps),
        ];
        let cones = || runs.iter().flat_map(|r| &r.cones);
        for system in std::iter::once(&ts).chain(cones().map(|c| &c.run.system)) {
            systems += 1;
            for _ in 0..20 {
                states += 1;
                enabled += assert_planned_agrees(&name, system, &random_state(system, &mut mix));
            }
        }
        for cone in cones() {
            for state in &cone.run.states {
                states += 1;
                enabled += assert_planned_agrees(&name, &cone.run.system, state);
            }
        }
    }
    eprintln!("{models} models, {systems} systems, {states} states, {enabled} enabled bindings");
    assert!(models >= 80, "{models} models");
    assert!(
        systems > 500 && states > 50_000 && enabled > 50_000,
        "{systems} systems, {states} states, {enabled} enabled bindings"
    );
}
