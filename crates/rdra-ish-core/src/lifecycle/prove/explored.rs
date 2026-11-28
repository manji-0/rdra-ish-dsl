//! The kept helper lemmas against explored states, on every proof (docs/theory/z3.md,
//! "The kept lemmas on explored states").
//!
//! A lemma the proof keeps is inductive in the encoding, so, the encoding over-approximating
//! the explorer (claim B1), it holds in every state the explorer reaches. This check runs the
//! cones the miner explores (`mine.rs`: each entity's cone and each rule's, at the run's
//! bounds, up to `STATE_CAP` states each) once more, one per thread, and evaluates each kept
//! lemma, with the native evaluator, on every state of one of those cones that holds every var
//! it reads. A state that breaks one shows the encoding let a false lemma survive
//! Houdini. It sees only those states: a proof that is wrong past them, the ones a run stopped
//! at a cap never reached, passes.
//!
//! A cone that leaves out a raised mark (`cone_leaves_marks`) runs a use case without its event
//! and may reach states no run of the whole system reaches, so it is not read. A cone that
//! frees slots reaches states of the whole system at more instances, where the lemmas hold too,
//! so it is read.

use crate::lifecycle::cone::{Cone, Target, VarSet};
use crate::lifecycle::explore::Caps;
use crate::lifecycle::expr::{Env, StateSlice};
use crate::lifecycle::types::{EntId, Pred};
use crate::lifecycle::TransitionSystem;
use crate::parallel::{in_parallel, jobs};

use super::encode::Formula;
use super::mine::STATE_CAP;

/// Each lemma of `lemmas` (its text and formula) that a state of a cone of `ts` breaks, as a
/// line naming the lemma, the cone and the rows. `rules` are the rules the proof was asked
/// about, whose cones the miner explores too. A lemma no cone that is read holds every var of
/// is not checked.
pub(super) fn lemma_faults(
    ts: &TransitionSystem,
    lemmas: &[(String, Formula)],
    rules: &[usize],
) -> Vec<String> {
    let mut targets: Vec<Target> = ts.entity_ids().map(Target::Entity).collect();
    targets.extend(rules.iter().map(|&i| Target::Invariant(i)));
    let reads: Vec<VarSet> = lemmas.iter().map(|(_, f)| reads(ts, f)).collect();
    let mut cones: Vec<(Target, Cone)> = Vec::new();
    for target in targets {
        let cone = ts.target_cone(target);
        if cones.iter().any(|(_, c)| *c == cone) {
            continue;
        }
        if ts.cone_leaves_marks(&cone) {
            continue;
        }
        cones.push((target, cone));
    }
    // Each lemma is read on one cone that holds every var it reads: a complete run of an exact
    // cone reaches every reachable state's projection onto its vars, so a second cone would
    // show the lemma nothing new. A run costs far more per state than a lemma does, so a lemma
    // goes to a cone already chosen when one holds it, and otherwise to the smallest that
    // does; the lemmas with the fewest such cones choose first.
    let holders: Vec<Vec<usize>> = reads
        .iter()
        .map(|r| {
            (0..cones.len())
                .filter(|&c| r.is_subset(&cones[c].1.vars))
                .collect()
        })
        .collect();
    let mut order: Vec<usize> = (0..lemmas.len()).collect();
    order.sort_by_key(|&k| holders[k].len());
    let mut held: Vec<Vec<usize>> = vec![Vec::new(); cones.len()];
    let size = |c: &usize| cones[*c].1.vars.len();
    for k in order {
        let chosen = holders[k]
            .iter()
            .filter(|c| !held[**c].is_empty())
            .min_by_key(|c| size(c));
        if let Some(&c) = chosen.or_else(|| holders[k].iter().min_by_key(|c| size(c))) {
            held[c].push(k);
        }
    }
    let work: Vec<(Target, Cone, Vec<usize>)> = cones
        .into_iter()
        .zip(held)
        .filter(|(_, h)| !h.is_empty())
        .map(|((t, c), h)| (t, c, h))
        .collect();
    let ran = in_parallel(work, jobs(), |(target, cone, held)| {
        let (sub, map) = ts.restrict_to_mapped(&cone.vars, &cone.keep, true);
        let mapped: Vec<(usize, Vec<EntId>, Pred, bool)> = held
            .iter()
            .map(|&k| {
                let f = &lemmas[k].1;
                let binders = f.binders.iter().map(|e| map.e(*e)).collect();
                (k, binders, map.pred(&f.body), f.all_rows)
            })
            .collect();
        let run = sub.run(Caps::new(usize::MAX, STATE_CAP));
        let sub = &run.system;
        let mut broken: Vec<String> = Vec::new();
        let mut open: Vec<bool> = vec![true; mapped.len()];
        for (n, s) in run.states.iter().enumerate() {
            for (j, (k, binders, body, all_rows)) in mapped.iter().enumerate() {
                if !open[j] {
                    continue;
                }
                if let Some(rows) = violation(sub, binders, body, &s, *all_rows) {
                    open[j] = false;
                    broken.push(format!(
                        "the kept lemma `{}` fails on state {n} BFS reached in the cone of {}, at rows {rows:?}",
                        lemmas[*k].0,
                        target_name(ts, target)
                    ));
                }
            }
        }
        broken
    });
    ran.into_iter().flatten().collect()
}

/// What a lemma reads: its vars and the existence of every entity it binds or quantifies.
fn reads(ts: &TransitionSystem, f: &Formula) -> VarSet {
    let mut out = VarSet::new();
    ts.pred_reads(&f.body, &mut out);
    out.extend(f.binders.iter().map(|e| ts.entity(*e).exists));
    out
}

fn target_name(ts: &TransitionSystem, target: Target) -> String {
    match target {
        Target::Entity(e) => ts.entity(e).id.clone(),
        Target::Invariant(i) => ts.invariants()[i].name.clone(),
        _ => unreachable!("the miner explores entity and rule cones only"),
    }
}

/// The first rows of `binders` under which `body` fails in `state`: present rows, or with
/// `all_rows` every slot, as `Enc::holds` reads the formula.
fn violation(
    ts: &TransitionSystem,
    binders: &[EntId],
    body: &Pred,
    state: &StateSlice,
    all_rows: bool,
) -> Option<Vec<u8>> {
    let mut env = Env::new(&[]);
    search(ts, binders, body, state, all_rows, &mut env)
}

fn search(
    ts: &TransitionSystem,
    binders: &[EntId],
    body: &Pred,
    state: &StateSlice,
    all_rows: bool,
    env: &mut Env,
) -> Option<Vec<u8>> {
    let Some((&first, rest)) = binders.split_first() else {
        return (!ts.eval(body, state, env)).then(|| env.bound.clone());
    };
    for slot in 0..ts.slots(first) {
        if !all_rows && !state.is_present(ts, first, slot) {
            continue;
        }
        env.bound
            .push(u8::try_from(slot).expect("at most 255 slots"));
        let found = search(ts, rest, body, state, all_rows, env);
        env.bound.pop();
        if found.is_some() {
            return found;
        }
    }
    None
}
