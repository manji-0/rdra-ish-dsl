//! The step-level check of Lemma 6 of docs/theory/cones.md and of the stuttering
//! bisimulation of Theorem A8, on a reduced cone whose unreduced cone `C` was explored to the
//! end. A test-only checker: it needs the complete run of `C`, which the reduction exists to
//! avoid.
//!
//! It explores the extended sub-lifecycle `X` (`Extended::rebuild`) plainly, without its
//! inner reduction, and maps each state of `C` to its `X` state `x(t, s)` by the projection
//! (`Reduction::key`). Then:
//!
//! - `x(Reach C) ⊆ Reach X`, and each initial state of `C` maps to an initial state of `X`;
//! - every step of `C` maps to a step of `X` or a stutter: an `S_a` step to a silent edge of
//!   `X` by an action that stands for it, a `T_a` step that leaves the input bytes alone to no
//!   change of `x`, and one that changes them to one rise of `X` per stage of
//!   `Reduction::stages`, ending at `x(t', s)`; a `T_a` step leaves `S` alone (Lemma 5b);
//! - every silent edge of `X` from `x(t, s)`, a self-loop aside, is the image of an `S_a`
//!   step of `C` from `(t, s)`. Rises are not held to `C`: `X` flips a bit and raises an input
//!   wherever its guards on `S` and the inputs let it, which over-approximates `C`'s `T`.
//!
//! The class level reads no class table of the reduction. It computes the blocks of `Reach X`
//! afresh, with the sweeps of `partition/reference.rs` from the projection on `obs`, the raw
//! inputs and the bits, and labels each state of `C` and of the reduced run `R` by its `T`
//! part and the block of its `X` state. The two runs must then have the same labelled states,
//! the same initial labels, and the same labelled edges up to stuttering: an edge whose two
//! ends have one label is a stutter, and the others are labelled by their action, an `S`
//! action as one silent label. The class moves, the rises table and the representatives are
//! held to `C` through `R`'s run. Where a state of `R` maps to no state of `Reach X` (its
//! representative is in the inner reduction's canonical form), the class level is skipped and
//! counted. A nested reduction is checked the same way, with `X` as its cone.
//!
//! What it shares with the code under test: `Reduction::key` (the spans, the stub tail and
//! `push_bits`, so `Fold::holds`), `Extended::build`, which is `X` itself, and the step
//! relation `moves` of both systems.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::extended::Extended;
use super::partition::reference::quotient_blocks;
use super::Reduction;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::{Caps, Checks, ComponentRun};
use crate::lifecycle::expr::{State, StateSlice};
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

/// What the check saw, summed over the cones it ran on.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct StepCheck {
    /// Reductions checked, nested ones included.
    pub(crate) reductions: usize,
    /// Of those, nested ones.
    pub(crate) nested: usize,
    /// States of `C` mapped into `Reach X`.
    pub(crate) states: usize,
    /// `S_a` steps of `C` matched by a silent edge of `X`.
    pub(crate) s_steps: usize,
    /// Steps of `C` that leave `x` where it is.
    pub(crate) stutters: usize,
    /// `T_a` steps of `C` that change the input bytes, matched stage by stage.
    pub(crate) rises: usize,
    /// The stages of those steps, each a rise edge of `X`.
    pub(crate) stages: usize,
    /// Silent edges of `X` from an image state, each the image of a step of `C`.
    pub(crate) silent_back: usize,
    /// Reductions whose class level ran: labelled states and edges of `C` and `R` compared.
    pub(crate) classes: usize,
    /// Labelled edges of `C` compared with `R`'s, stutters left out.
    pub(crate) class_edges: usize,
    /// Reductions whose class level was skipped: a state of `R` maps to no state of
    /// `Reach X`.
    pub(crate) classes_skipped: usize,
    /// Reductions whose `X`, or whose `R`, stopped at the caps: not checked.
    pub(crate) stopped: usize,
    /// Reduced cones whose unreduced cone stopped at the caps, counted by the caller: not
    /// checked.
    pub(crate) cones_stopped: usize,
}

impl std::ops::AddAssign for StepCheck {
    fn add_assign(&mut self, o: Self) {
        self.reductions += o.reductions;
        self.nested += o.nested;
        self.states += o.states;
        self.s_steps += o.s_steps;
        self.stutters += o.stutters;
        self.rises += o.rises;
        self.stages += o.stages;
        self.silent_back += o.silent_back;
        self.classes += o.classes;
        self.class_edges += o.class_edges;
        self.classes_skipped += o.classes_skipped;
        self.stopped += o.stopped;
        self.cones_stopped += o.cones_stopped;
    }
}

impl TransitionSystem {
    /// This system with `r` as its reduction in place of its own: an extended sub-lifecycle
    /// with its inner reduction, or a cone with tables a test planted a bug in.
    pub(crate) fn with_reduction(&self, r: Reduction) -> TransitionSystem {
        let mut out = self.clone();
        out.reduction = Some(std::sync::Arc::new(r));
        out
    }
}

impl Reduction {
    /// This reduction with every class move of `class` removed: a planted bug the class level
    /// of `check` must catch.
    pub(crate) fn without_class_moves(&self, class: u32) -> Reduction {
        let mut out = self.clone();
        out.moves[class as usize].clear();
        out
    }

    /// `x` with its raw inputs and bits replaced by `inputs`, in `input_bytes`' order.
    fn with_inputs(&self, x: &[u8], inputs: &[u8]) -> State {
        let mut out = x.to_vec();
        let (mut at, mut i) = (0, 0);
        for &(_, w, in_s) in &self.spans {
            if !in_s {
                out[at..at + w].copy_from_slice(&inputs[i..i + w]);
                i += w;
            }
            at += w;
        }
        let bits = inputs.len() - i;
        let n = out.len();
        out[n - bits..].copy_from_slice(&inputs[i..]);
        State(out.into())
    }

    /// The bytes of a cone state outside S: its T part, which a label keeps.
    fn t_part(&self, s: &StateSlice) -> Box<[u8]> {
        let mut out = s.0.to_vec();
        for &(at, w, in_s) in &self.spans {
            if in_s {
                out[at..at + w].fill(0);
            }
        }
        out.into()
    }
}

/// Checks Lemma 6 and then the class level on `reduced`, the reduced cone `R`, against `run`,
/// the complete run of `cone`, the unreduced cone `C`. Every run of `X` and of `R` goes under
/// `caps`; `R` runs after Lemma 6 holds, since a missing rise panics in its run. Returns what
/// it checked, or the first step, state or edge that breaks the lemma.
pub(crate) fn check(
    cone: &TransitionSystem,
    run: &ComponentRun,
    reduced: &TransitionSystem,
    caps: Caps,
) -> Result<StepCheck, String> {
    let red = reduced.reduction().expect("a reduced cone");
    let mut seen = StepCheck {
        reductions: 1,
        ..StepCheck::default()
    };
    let ext = Extended::rebuild(cone, red);
    let xrun = ext
        .system
        .clone()
        .run_checking(caps.without_views(), Checks::none());
    if !xrun.complete {
        seen.stopped += 1;
        return Ok(seen);
    }
    let index: HashMap<State, u32> = (xrun.states.iter().enumerate())
        .map(|(i, s)| (s.into_state(), u32::try_from(i).expect("a state index")))
        .collect();
    let x_initial: HashSet<State> = ext.system.initial_states().into_iter().collect();
    let name = |a: ActId| cone.actions[a.index()].name.as_str();
    let stands_for = |x_act: ActId, a: ActId| {
        ext.acts[x_act.index()].is_some_and(|first| {
            first == a || ext.variants.get(&first).is_some_and(|v| v.contains(&a))
        })
    };
    let key = |s: &StateSlice| State(red.key(cone, s).into());
    for (i, c) in run.states.iter().enumerate() {
        let x = key(&c);
        if !index.contains_key(&x) {
            return Err(format!(
                "the state {:?} of C maps to {:?}, which X does not reach",
                &c.0, x.0
            ));
        }
        if run.parent(u32::try_from(i).expect("an index")).is_none() && !x_initial.contains(&x) {
            return Err(format!(
                "the initial state {:?} of C maps to {:?}, no initial state of X",
                &c.0, x.0
            ));
        }
        seen.states += 1;
        let x_moves = ext.system.plain_moves(&x);
        let c_moves: Vec<_> = cone.moves(&c).collect();
        for m in &c_moves {
            let x2 = key(&m.next);
            if red.s_actions.contains(&m.act) {
                if x2 == x {
                    seen.stutters += 1;
                    continue;
                }
                let matched = x_moves.iter().any(|(a, _, post)| {
                    !ext.rise[a.index()] && *post == x2 && stands_for(*a, m.act)
                });
                if !matched {
                    return Err(format!(
                        "C steps by {} {:?} from {:?} to {:?}, and X has no silent edge from {:?} \
                         to {:?} by an action that stands for it",
                        name(m.act),
                        m.params,
                        &c.0,
                        m.next.0,
                        x.0,
                        x2.0
                    ));
                }
                seen.s_steps += 1;
                continue;
            }
            let s_of = |s: &StateSlice| -> Vec<u8> {
                red.spans
                    .iter()
                    .filter(|(_, _, in_s)| *in_s)
                    .flat_map(|&(at, w, _)| s.0[at..at + w].to_vec())
                    .collect()
            };
            if s_of(&c) != s_of(&m.next) {
                return Err(format!(
                    "the T step {} {:?} from {:?} changes S",
                    name(m.act),
                    m.params,
                    &c.0
                ));
            }
            let (before, after) = (red.input_bytes(cone, &c), red.input_bytes(cone, &m.next));
            if before == after {
                assert_eq!(x2, x, "the same S and inputs give the same X state");
                seen.stutters += 1;
                continue;
            }
            let mut cur = x.clone();
            for stage in red.stages(&before, &after) {
                let next = red.with_inputs(&cur.0, &stage);
                let rises = ext.system.plain_moves(&cur);
                if !rises
                    .iter()
                    .any(|(a, _, post)| ext.rise[a.index()] && *post == next)
                {
                    return Err(format!(
                        "C steps by {} {:?} from {:?}, changing the inputs from {before:?} to \
                         {after:?}, and X has no rise from {:?} to the stage {stage:?}",
                        name(m.act),
                        m.params,
                        &c.0,
                        cur.0
                    ));
                }
                seen.stages += 1;
                cur = next;
            }
            if cur != x2 {
                return Err(format!(
                    "the stages of {} from {:?} end at {:?}, not at {:?}",
                    name(m.act),
                    &c.0,
                    cur.0,
                    x2.0
                ));
            }
            seen.rises += 1;
        }
        for (a, _, post) in &x_moves {
            if ext.rise[a.index()] || *post == x {
                continue;
            }
            let realized = c_moves.iter().any(|m| {
                red.s_actions.contains(&m.act) && stands_for(*a, m.act) && key(&m.next) == *post
            });
            if !realized {
                return Err(format!(
                    "X has a silent edge by {} from {:?} to {:?}, the image of no step of C \
                     from {:?}",
                    ext.system.actions[a.index()].name,
                    x.0,
                    post.0,
                    &c.0
                ));
            }
            seen.silent_back += 1;
        }
    }
    if let Some(inner) = red.inner() {
        let sub = ext.system.with_reduction(inner.clone());
        let nested = check(&ext.system, &xrun, &sub, caps).map_err(|e| format!("nested: {e}"))?;
        seen += nested;
        seen.nested += nested.reductions;
    }
    let r_run = (reduced.clone()).run_checking(caps.without_views(), Checks::none());
    if !r_run.complete {
        seen.stopped += 1;
        return Ok(seen);
    }
    class_level(cone, run, &r_run, red, &ext, &xrun, &index, &mut seen)?;
    Ok(seen)
}

/// The class level of `check`: the states and edges of `C` and of `R`, each labelled by its T
/// part and the block of its X state in a partition of `Reach X` computed afresh, agree up to
/// stuttering.
#[allow(clippy::too_many_arguments)]
fn class_level(
    cone: &TransitionSystem,
    run: &ComponentRun,
    reduced: &ComponentRun,
    red: &Reduction,
    ext: &Extended,
    xrun: &ComponentRun,
    index: &HashMap<State, u32>,
    seen: &mut StepCheck,
) -> Result<(), String> {
    let n = xrun.states.len();
    let anchor: VarSet = ext.obs_local.union(&ext.inputs_local).copied().collect();
    let mut labels: HashMap<State, u32> = HashMap::new();
    let initial: Vec<u32> = (xrun.states.iter())
        .map(|s| {
            let k = ext.system.project(&s, &anchor);
            let len = u32::try_from(labels.len()).expect("a label");
            *labels.entry(k).or_insert(len)
        })
        .collect();
    let (mut silent, mut rises) = (Vec::new(), Vec::new());
    for e in xrun.edges() {
        let (act, _) = xrun.step(e);
        let pair = (e.from, e.to);
        match ext.rise[act.index()] {
            true => rises.push(pair),
            false => silent.push(pair),
        }
    }
    let blocks = quotient_blocks(n, &silent, &rises, &initial);
    type Label = (Box<[u8]>, u32);
    let label = |s: &StateSlice| -> Option<Label> {
        let x = State(red.key(cone, s).into());
        index.get(&x).map(|i| (red.t_part(s), blocks[*i as usize]))
    };
    // The labelled states, initial states, and edges of a run; `None` when a state maps to no
    // state of Reach X.
    type Labelled = (
        BTreeSet<Label>,
        BTreeSet<Label>,
        BTreeSet<(Label, Option<ActId>, Label)>,
    );
    let labelled = |r: &ComponentRun| -> Option<Labelled> {
        let mut at = Vec::with_capacity(r.states.len());
        for s in r.states.iter() {
            at.push(label(&s)?);
        }
        let states: BTreeSet<Label> = at.iter().cloned().collect();
        let initial = (0..at.len())
            .filter(|i| r.parent(u32::try_from(*i).expect("an index")).is_none())
            .map(|i| at[i].clone())
            .collect();
        let mut edges = BTreeSet::new();
        for e in r.edges() {
            let (act, _) = r.step(e);
            let (a, b) = (&at[e.from as usize], &at[e.to as usize]);
            if a == b {
                continue;
            }
            let act = (!red.s_actions.contains(&act)).then_some(act);
            edges.insert((a.clone(), act, b.clone()));
        }
        Some((states, initial, edges))
    };
    let c = labelled(run).expect("a state of C maps into Reach X, checked above");
    let Some(r) = labelled(reduced) else {
        seen.classes_skipped += 1;
        return Ok(());
    };
    let name =
        |a: &Option<ActId>| a.map_or("an S action", |a| cone.actions[a.index()].name.as_str());
    if let Some(l) = c.0.symmetric_difference(&r.0).next() {
        return Err(format!(
            "C and R label their states differently: {l:?} is in one alone ({} and {} labels)",
            c.0.len(),
            r.0.len()
        ));
    }
    if c.1 != r.1 {
        return Err("C and R start in different labels".into());
    }
    if let Some((a, act, b)) = c.2.difference(&r.2).next() {
        return Err(format!(
            "C steps by {} from the label {a:?} to {b:?}, and R has no such edge",
            name(act)
        ));
    }
    if let Some((a, act, b)) = r.2.difference(&c.2).next() {
        return Err(format!(
            "R steps by {} from the label {a:?} to {b:?}, and C has no such edge",
            name(act)
        ));
    }
    seen.classes += 1;
    seen.class_edges += c.2.len();
    Ok(())
}
