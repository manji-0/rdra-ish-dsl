//! An oracle for `can_reach` and deadlocks (claims N8 and N14 of docs/theory/claims.md): TLC
//! cannot state `can_reach`, and its deadlock check is not the one `verify` makes, so each
//! has a naive reference here. The reference explores the whole system on its own, from
//! `initial_states` through `moves` into a map of its own, and decides each check by a
//! forward search from every state, where the checker runs one backward pass per binding
//! (`reach.rs::reach_failure`) or per column and slot (`findings.rs::can_leave`) over the
//! graph its BFS stored. What the two share is the step relation and the evaluator of
//! predicates, which TLC holds to its own reading of the TLA+ module on every other check.
//!
//! Every corpus case and formal-verification sample that builds, at one and two instances
//! per entity, with a `can_reach` added per value of each state model column, up to
//! `MOST_STATES` states.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::num::NonZeroU8;

use super::cone::load;
use super::*;
use crate::lifecycle::expr::{Env, State, StateSlice};
use crate::lifecycle::findings::Progress;
use crate::lifecycle::types::{Pred, Value, VarId};
use crate::lifecycle::Target;

/// The largest system the oracle explores; its searches are quadratic.
const MOST_STATES: usize = 3_000;

/// The whole system, explored by the oracle: each state and the states its moves reach.
struct Naive {
    states: Vec<State>,
    succ: Vec<Vec<usize>>,
    index: HashMap<State, usize>,
}

impl Naive {
    /// `None` past `MOST_STATES` states.
    fn explore(ts: &TransitionSystem) -> Option<Naive> {
        let mut naive = Naive {
            states: Vec::new(),
            succ: Vec::new(),
            index: HashMap::new(),
        };
        let mut queue = VecDeque::new();
        for s in ts.initial_states() {
            if let Some(i) = naive.intern(s) {
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let next: Vec<State> = ts.moves(&naive.states[i]).map(|m| m.next).collect();
            for s in next {
                let j = match naive.index.get(&s) {
                    Some(&j) => j,
                    None => {
                        let j = naive.intern(s).expect("a new state");
                        queue.push_back(j);
                        j
                    }
                };
                naive.succ[i].push(j);
            }
            if naive.states.len() > MOST_STATES {
                return None;
            }
        }
        Some(naive)
    }

    fn intern(&mut self, s: State) -> Option<usize> {
        if self.index.contains_key(&s) {
            return None;
        }
        let i = self.states.len();
        self.index.insert(s.clone(), i);
        self.states.push(s);
        self.succ.push(Vec::new());
        Some(i)
    }

    /// Whether some path from `from` through states where `keep` holds reaches a state where
    /// `done` holds, `from` included; `done` reads the state a step leaves and the one it
    /// enters (`None` for `from` itself).
    fn finds(
        &self,
        from: usize,
        keep: impl Fn(usize) -> bool,
        done: impl Fn(Option<usize>, usize) -> bool,
    ) -> bool {
        if done(None, from) {
            return true;
        }
        let mut seen = vec![false; self.states.len()];
        let mut stack = vec![from];
        seen[from] = true;
        while let Some(x) = stack.pop() {
            for &y in &self.succ[x] {
                if done(Some(x), y) {
                    return true;
                }
                if keep(y) && !seen[y] {
                    seen[y] = true;
                    stack.push(y);
                }
            }
        }
        false
    }

    /// The states from which `can_reach` `i` fails: a binding's instance is present and
    /// linked, and no path on which it stays present reaches a present state with the goal.
    fn reach_failures(&self, ts: &TransitionSystem, i: usize) -> Vec<usize> {
        let r = &ts.reach[i];
        let holds = |pred: &Pred, binding: &[u8], s: &StateSlice| {
            let mut env = Env::new(&[]);
            env.bound = binding.to_vec();
            ts.eval(pred, s, &mut env)
        };
        let mut out = Vec::new();
        for binding in ts.binder_slots(&r.binders) {
            let present = |at: usize| {
                (r.binders.iter().zip(&binding))
                    .all(|(e, k)| self.states[at].is_present(ts, *e, usize::from(*k)))
            };
            for at in 0..self.states.len() {
                if !present(at) || !holds(&r.link, &binding, &self.states[at]) {
                    continue;
                }
                let goal = |_: Option<usize>, y: usize| {
                    present(y) && holds(&r.goal, &binding, &self.states[y])
                };
                if !self.finds(at, present, goal) {
                    out.push(at);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Per state-model value of `e` that can deadlock, whether a reachable state has a
    /// present slot at it from which no path on which the slot stays present takes a step
    /// that deletes the slot or changes the column there.
    fn deadlocks(&self, ts: &TransitionSystem, e: EntId) -> Vec<((VarId, Value), bool)> {
        let slots = ts.slots(e);
        (ts.state_model_values(e).into_iter())
            .filter(|&(var, value)| {
                !ts.var(var).ends.contains(&value)
                    && TransitionSystem::has_way_on(&ts.exits(e, var, value))
            })
            .map(|(var, value)| {
                let stuck = (0..slots).any(|k| {
                    let present = |at: usize| self.states[at].is_present(ts, e, k);
                    let at_value = |at: usize| self.states[at].get(ts, var, k);
                    (0..self.states.len()).any(|at| {
                        present(at)
                            && at_value(at) == value
                            && !self.finds(at, present, |x, y| {
                                x.is_some_and(|x| !present(y) || at_value(y) != at_value(x))
                            })
                    })
                });
                ((var, value), stuck)
            })
            .collect()
    }
}

/// A `can_reach` per value of each state model column of `ts`, as `property` lines.
fn reach_lines(ts: &TransitionSystem) -> String {
    let mut out = String::new();
    for e in ts.entity_ids() {
        for (var, value) in ts.state_model_values(e) {
            out.push_str(&format!(
                "\nproperty OracleReach{} \"oracle\" can_reach({}.{} == {})\n",
                out.len(),
                ts.entity(e).id,
                ts.var_name(var),
                ts.value_text(var, value)
            ));
        }
    }
    out
}

/// The model `src` builds, if it parses and builds without an error.
fn try_model(src: &str) -> Option<SemanticModel> {
    let (ast, errors) = parse(src);
    if !errors.is_empty() {
        return None;
    }
    let (model, diags) = build_model(&ast);
    diags.iter().all(|d| d.is_warning).then_some(model)
}

/// The models the oracle checks: every corpus case and formal-verification sample.
fn models() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in [
        "crates/rdra-ish-cli/tests/engine_corpus",
        "samples/formal-verification",
    ] {
        let mut paths: Vec<_> = std::fs::read_dir(repo_root().join(dir))
            .expect("a model directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
            .collect();
        paths.sort();
        for path in paths {
            let src = std::fs::read_to_string(&path).expect("a model");
            out.push((path.display().to_string(), src));
        }
    }
    out
}

/// What one system gave: how many checks the oracle compared, and each disagreement.
#[derive(Default)]
struct Tally {
    reach: usize,
    deadlocks: usize,
    stuck: usize,
    failing: usize,
    systems: usize,
    wrong: Vec<String>,
}

fn compare(name: &str, model: &SemanticModel, n: u8, tally: &mut Tally) {
    let bounds = Bounds::uniform(NonZeroU8::new(n).expect("n > 0"), 5);
    let ts = TransitionSystem::build(model, bounds.clone(), &Scope::ALL).ts;
    let Some(naive) = Naive::explore(&ts) else {
        return;
    };
    tally.systems += 1;
    let run = ts.clone().run(Caps::new(usize::MAX, 1 << 20));
    assert!(run.complete, "{name} at {n}: the checker's run stopped");
    let graph = run.graph();
    assert_eq!(
        graph.states.len(),
        naive.states.len(),
        "{name} at {n}: the two explorations reach different numbers of states"
    );
    let naive_at = |at: u32| {
        let s = graph.states.at(at as usize);
        naive.index[&*(*s).to_owned()]
    };
    for i in 0..ts.reach.len() {
        let want = naive.reach_failures(&ts, i);
        let label = format!("{name} at {n}: can_reach {}", ts.reach[i].name);
        tally.reach += 1;
        tally.failing += usize::from(!want.is_empty());
        // The checker's run of the whole system, and the cone `verify` decides it on.
        match run.system.reach_failure(i, &graph) {
            None if want.is_empty() => {}
            Some((at, _)) if want.contains(&naive_at(at)) => {}
            got => tally.wrong.push(format!(
                "{label}: whole system {:?}, oracle {} failing states",
                got.map(|(at, _)| at),
                want.len()
            )),
        }
        let cone = ts.explore_targets(&[Target::Reach(i)], Caps::new(usize::MAX, 1 << 20));
        let fails = (cone.verdicts().iter()).any(|d| {
            matches!(&d.diag, StateDiag::GoalUnreachable { name, .. } if *name == ts.reach[i].name)
        });
        if fails == want.is_empty() {
            tally.wrong.push(format!(
                "{label}: its cone says {}, the oracle {}",
                if fails { "violated" } else { "ok" },
                if want.is_empty() { "ok" } else { "violated" }
            ));
        }
    }
    // What `check` prints: the deadlocks of each entity's own cone (`derive_states`). A cone
    // that takes slots of an entity it leaves out counts that entity unbounded ("entities a
    // check does not read are unbounded"), so a deadlock that only the bound on that entity
    // causes is not one `check` reports: those entities are left out of this comparison.
    let exact = |e: EntId| !ts.cone_frees_slots(&ts.target_cone(Target::Entity(e)));
    let caps = Caps::new(usize::MAX, 1 << 20);
    let printed: BTreeSet<(String, String, String)> =
        (super::super::derive_states_built(model, &Scope::ALL, bounds, caps).0)
            .diags
            .iter()
            .filter_map(|d| match &d.diag {
                StateDiag::Deadlock {
                    entity,
                    column,
                    value,
                    ..
                } => Some((entity.clone(), column.clone(), value.clone())),
                _ => None,
            })
            .filter(|(entity, ..)| exact(ent(&ts, entity)))
            .collect();
    let mut stuck_here = BTreeSet::new();
    for e in ts.entity_ids() {
        let found = ts.state_model_findings(e, &graph);
        for ((var, value), stuck) in naive.deadlocks(&ts, e) {
            if stuck && exact(e) {
                stuck_here.insert((
                    ts.entity(e).id.clone(),
                    ts.var_name(var),
                    ts.value_text(var, value),
                ));
            }
            tally.deadlocks += 1;
            tally.stuck += usize::from(stuck);
            let got = (found.deadlocks.iter())
                .find(|d| d.var == var && d.value == value)
                .map(|d| &d.progress);
            let agrees = match got {
                Some(Progress::Leaves) => !stuck,
                Some(Progress::Stuck { .. }) => stuck,
                _ => false,
            };
            if !agrees {
                tally.wrong.push(format!(
                    "{name} at {n}: deadlock of {}.{} = {}: the checker {got:?}, the oracle {}",
                    ts.entity(e).id,
                    ts.var_name(var),
                    ts.value_text(var, value),
                    if stuck { "stuck" } else { "leaves" }
                ));
            }
        }
    }
    if printed != stuck_here {
        tally.wrong.push(format!(
            "{name} at {n}: `check` prints the deadlocks {printed:?}, the oracle finds {stuck_here:?}"
        ));
    }
}

/// The checker's `can_reach` and deadlock verdicts equal the oracle's on every model it
/// explores, at one and two instances, and the comparison covers failing and passing
/// checks of both kinds.
#[test]
fn can_reach_and_deadlocks_agree_with_a_naive_oracle() {
    let mut tally = Tally::default();
    for (name, src) in models() {
        let Some(model) = try_model(&src) else {
            continue;
        };
        let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
        let with_reach = format!("{src}\n{}", reach_lines(&ts));
        let model = try_model(&with_reach).unwrap_or(model);
        for n in [1, 2] {
            compare(&name, &model, n, &mut tally);
        }
    }
    eprintln!(
        "oracle: {} can_reach compared ({} failing), {} deadlock values ({} stuck), {} systems",
        tally.reach, tally.failing, tally.deadlocks, tally.stuck, tally.systems
    );
    assert!(
        tally.wrong.is_empty(),
        "{} disagreement(s):\n{}",
        tally.wrong.len(),
        tally.wrong.join("\n")
    );
    // A comparison that never saw a failure would not show the oracle can tell.
    assert!(
        tally.reach >= 100 && tally.failing >= 10,
        "can_reach: {} compared, {} failing",
        tally.reach,
        tally.failing
    );
    assert!(
        tally.deadlocks >= 50 && tally.stuck >= 5,
        "deadlocks: {} compared, {} stuck",
        tally.deadlocks,
        tally.stuck
    );
}

/// The samples the oracle must reach: the two `can_reach` samples and the deadlock one.
#[test]
fn the_oracle_explores_the_can_reach_and_deadlock_samples() {
    for rel in [
        "samples/formal-verification/can_reach_met.rdra",
        "samples/formal-verification/can_reach_unmet.rdra",
        "samples/formal-verification/deadlock.rdra",
    ] {
        let ts = TransitionSystem::build(&load(rel), Bounds::STATES, &Scope::ALL).ts;
        assert!(Naive::explore(&ts).is_some(), "{rel}");
    }
}
