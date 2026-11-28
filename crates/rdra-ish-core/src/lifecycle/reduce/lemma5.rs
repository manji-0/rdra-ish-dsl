//! The run-time certificate of Lemma 5 of docs/theory/cones.md: every build checks the split
//! `split_with` chose before the reduction uses it. The check reads the split's sets (`T_v`,
//! `T_a`, `S_v`, `S_a`, `obs`, the inputs and the folds) and the seeds, and nothing else of
//! `split_at`'s work: it computes each action's reads (mark reads and copy reads included)
//! and writes afresh from the action, and monotonicity afresh with `monotone_vars`. A fault
//! makes `reduced_with` decide the cone unreduced.

use std::fmt;

use super::split::Split;
use crate::lifecycle::cone::{Seed, VarSet};
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

/// The clause of Lemma 5 a split fails, or `Split` when its sets are not a split at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::lifecycle) enum Clause {
    /// `S_v` is not exactly the vars outside `T_v`, `S_a` is not exactly the writing actions
    /// outside `T_a`, an input is not in `T_v`, an `obs` var is not in `S_v`, or a fold is of
    /// an action outside `S_a`.
    Split,
    /// (a) an `S_a` action writes a `T_v` var.
    A,
    /// (b) a `T_a` action writes an `S_v` var, or reads one outside `obs`.
    B,
    /// (c) an `S_a` action reads a `T_v` var that is neither an input nor read by its fold.
    C,
    /// (d) an input is not monotone.
    D,
    /// (e) a seed reads an `S_v` var outside `obs`, or a kept action is outside `T_a`.
    E,
    /// A var and its entity's existence are on different sides (Definition 3).
    Sides,
}

/// One fault of a split: its clause and what breaks it.
#[derive(Debug)]
pub(in crate::lifecycle) struct SplitFault {
    pub(in crate::lifecycle) clause: Clause,
    what: String,
}

impl fmt::Display for SplitFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clause = match self.clause {
            Clause::Split => "not a split",
            Clause::A => "Lemma 5a",
            Clause::B => "Lemma 5b",
            Clause::C => "Lemma 5c",
            Clause::D => "Lemma 5d",
            Clause::E => "Lemma 5e",
            Clause::Sides => "a var and its existence on two sides",
        };
        write!(f, "{clause}: {}", self.what)
    }
}

impl TransitionSystem {
    /// Per action, the reads Lemma 5 checks (`action_reads` with `copy_reads`) and the
    /// writes (`action_writes`).
    fn read_write_sets(&self) -> (Vec<VarSet>, Vec<VarSet>) {
        let reads = self
            .actions
            .iter()
            .map(|a| {
                let mut r = self.action_reads(a);
                for (_, c) in self.copy_reads(a) {
                    r.extend(c);
                }
                r
            })
            .collect();
        let writes = self.actions.iter().map(|a| self.action_writes(a)).collect();
        (reads, writes)
    }

    /// The sets `split_faults` checks, as one certificate for the Lean checker of
    /// proofs/lean (`lake exe lemma5-check`, `RdraIsh/Lemma5Check.lean`): the reads and
    /// writes per action, `monotone_vars`, the split's sets, each `S_a` action's folded reads,
    /// and the seeds. Ids are `VarId::index` and `ActId::index`.
    #[cfg(test)]
    pub(in crate::lifecycle) fn lemma5_certificate(
        &self,
        seeds: &[Seed],
        split: &Split,
    ) -> serde_json::Value {
        use serde_json::json;
        let vars = |s: &VarSet| s.iter().map(|v| v.index()).collect::<Vec<_>>();
        let acts =
            |s: &std::collections::BTreeSet<ActId>| s.iter().map(|a| a.index()).collect::<Vec<_>>();
        let (reads, writes) = self.read_write_sets();
        let monotone: Vec<usize> = self
            .monotone_vars()
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.then_some(i))
            .collect();
        let folded: Vec<Vec<usize>> = self
            .action_ids()
            .map(|id| {
                let fold = split.folds.iter().find(|f| f.act == id);
                fold.map(|f| vars(&f.reads(self))).unwrap_or_default()
            })
            .collect();
        let keep: Vec<usize> = seeds.iter().flat_map(|(_, kept)| acts(kept)).collect();
        json!({
            "nv": self.vars.len(),
            "na": self.actions.len(),
            "rd": reads.iter().map(vars).collect::<Vec<_>>(),
            "wr": writes.iter().map(vars).collect::<Vec<_>>(),
            "mono": monotone,
            "t": vars(&split.t),
            "s": vars(&split.s),
            "ta": acts(&split.t_actions),
            "sa": acts(&split.s_actions),
            "obs": vars(&split.obs),
            "inputs": vars(&split.inputs),
            "folded": folded,
            "seeds": seeds.iter().map(|(seed, _)| vars(seed)).collect::<Vec<_>>(),
            "keep": keep,
        })
    }

    /// Every fault of `split` as the split of this cone system for `seeds` (Lemma 5 of
    /// docs/theory/cones.md, and the side rule of Definition 3); empty when it is one.
    pub(in crate::lifecycle) fn split_faults(
        &self,
        seeds: &[Seed],
        split: &Split,
    ) -> Vec<SplitFault> {
        let mut out = Vec::new();
        let mut fault = |clause, what: String| out.push(SplitFault { clause, what });
        let var = |v: &_| self.var_name(*v);
        let act = |a: &ActId| self.actions[a.index()].name.as_str();
        let (reads, writes) = self.read_write_sets();
        let (t, s) = (&split.t, &split.s);

        for v in self.var_ids() {
            if t.contains(&v) == s.contains(&v) {
                fault(
                    Clause::Split,
                    format!("{} is in both or neither of T and S", var(&v)),
                );
            }
        }
        if t.len() + s.len() != self.vars.len() {
            fault(Clause::Split, "T and S hold a var the cone does not".into());
        }
        for id in self.action_ids() {
            let (in_t, in_s) = (split.t_actions.contains(&id), split.s_actions.contains(&id));
            let writing = !writes[id.index()].is_empty();
            if (in_t && in_s) || (writing && !in_t && !in_s) || (in_s && !writing) {
                fault(
                    Clause::Split,
                    format!("{} is misplaced among T_a and S_a", act(&id)),
                );
            }
        }
        for v in split.inputs.difference(t) {
            fault(Clause::Split, format!("the input {} is not in T", var(v)));
        }
        for v in split.obs.difference(s) {
            fault(
                Clause::Split,
                format!("the observed {} is not in S", var(v)),
            );
        }
        for f in split
            .folds
            .iter()
            .filter(|f| !split.s_actions.contains(&f.act))
        {
            fault(
                Clause::Split,
                format!("{} is folded but not in S_a", act(&f.act)),
            );
        }

        for id in &split.s_actions {
            for v in writes[id.index()].difference(s) {
                fault(Clause::A, format!("{} writes {}", act(id), var(v)));
            }
        }
        for id in &split.t_actions {
            for v in writes[id.index()].difference(t) {
                fault(Clause::B, format!("{} writes {}", act(id), var(v)));
            }
            for v in reads[id.index()].intersection(s) {
                if !split.obs.contains(v) {
                    fault(
                        Clause::B,
                        format!("{} reads {} outside obs", act(id), var(v)),
                    );
                }
            }
        }
        for id in &split.s_actions {
            let fold = split.folds.iter().find(|f| f.act == *id);
            let folded = fold.map(|f| f.reads(self)).unwrap_or_default();
            for v in reads[id.index()].difference(s) {
                if !split.inputs.contains(v) && !folded.contains(v) {
                    fault(Clause::C, format!("{} reads {}", act(id), var(v)));
                }
            }
            // A T var the fold reads and the action also reads outside it is an input.
            for v in self.raw_reads(*id, fold).intersection(t) {
                if !split.inputs.contains(v) {
                    fault(
                        Clause::C,
                        format!("{} reads {} outside its fold", act(id), var(v)),
                    );
                }
            }
        }
        if !split.inputs.is_empty() {
            let monotone = self.monotone_vars();
            for v in split.inputs.iter().filter(|v| !monotone[v.index()]) {
                fault(Clause::D, format!("the input {} is not monotone", var(v)));
            }
        }
        for (seed, kept) in seeds {
            for v in seed.intersection(s) {
                if !split.obs.contains(v) {
                    fault(Clause::E, format!("a seed reads {} outside obs", var(v)));
                }
            }
            for id in kept.iter().filter(|id| !split.t_actions.contains(id)) {
                fault(Clause::E, format!("the kept {} is not in T_a", act(id)));
            }
        }
        for v in self.var_ids() {
            let Some(e) = self.own_existence(v) else {
                continue;
            };
            if t.contains(&v) != t.contains(&e) {
                fault(Clause::Sides, format!("{} and {}", var(&v), var(&e)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::Clause;
    use crate::lifecycle::cone::{Cone, Seed, VarSet};
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    use crate::lifecycle::explore::Caps;
    use crate::lifecycle::reduce::partition::tests::model;
    use crate::lifecycle::reduce::split::Split;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::types::{ActId, VarId};
    use crate::lifecycle::{Bounds, Scope, Target, TransitionSystem};

    /// Every target and property cone of the gate model (`partition::tests::model`), with
    /// and without the key's relock, as cone systems with their seeds.
    fn cone_systems() -> Vec<(TransitionSystem, Vec<Seed>)> {
        let mut out = Vec::new();
        for key_cycles in [false, true] {
            let model = model_from(&model(key_cycles));
            let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
            let targets = ts
                .targets()
                .into_iter()
                .filter(|t| !matches!(t, Target::Reach(_)))
                .map(|t| (ts.target_cone(t), ts.target_seed(t)));
            let properties =
                (0..ts.properties().len()).map(|i| (ts.property_cone(i), ts.property_seed(i)));
            let cones: Vec<(Cone, Seed)> = targets.chain(properties).collect();
            for (cone, seed) in cones {
                out.push(ts.cone_system(&cone, [seed]));
            }
        }
        out
    }

    /// The first var of `vars` in `set`.
    fn first_in(vars: &VarSet, set: &VarSet) -> Option<VarId> {
        vars.intersection(set).next().copied()
    }

    /// Each clause of Lemma 5, and the side rule, fails on a split planted to break it: the
    /// split `split_with` chose with one set changed, or, for the side rule, the split
    /// `split_at` makes without it, on which the cone is decided unreduced. The splits as
    /// chosen pass.
    #[test]
    fn a_planted_split_fails_the_clause_it_breaks() {
        let mut fired: BTreeSet<Clause> = BTreeSet::new();
        for (sys, seeds) in cone_systems() {
            let writes = |a: &ActId| sys.action_writes(&sys.actions[a.index()]);
            let reads = |a: &ActId| sys.action_reads(&sys.actions[a.index()]);
            let monotone = sys.monotone_vars();
            for inputs in [true, false] {
                let clauses = |split: &Split| -> BTreeSet<Clause> {
                    let faults = sys.split_faults(&seeds, split);
                    faults.iter().map(|f| f.clause).collect()
                };
                assert!(
                    clauses(&sys.split_with(&seeds, inputs)).is_empty(),
                    "the split as chosen passes"
                );
                let mut plant = |want: Clause, change: &dyn Fn(&mut Split) -> bool| {
                    let mut split = sys.split_with(&seeds, inputs);
                    if change(&mut split) {
                        let got = clauses(&split);
                        assert!(got.contains(&want), "{want:?} fails: {got:?}");
                        fired.insert(want);
                    }
                };
                // A T var put in S too.
                plant(Clause::Split, &|s| {
                    let v = s.t.first().copied();
                    v.is_some_and(|v| s.s.insert(v))
                });
                // A var an S action writes moved to T.
                plant(Clause::A, &|s| {
                    let v = s.s_actions.iter().find_map(|a| writes(a).first().copied());
                    v.is_some_and(|v| s.s.remove(&v) && s.t.insert(v))
                });
                // A var a T action writes moved to S.
                plant(Clause::B, &|s| {
                    let v = s.t_actions.iter().find_map(|a| writes(a).first().copied());
                    v.is_some_and(|v| s.t.remove(&v) && s.s.insert(v))
                });
                // An S var a T action reads dropped from obs.
                plant(Clause::B, &|s| {
                    let v = s.t_actions.iter().find_map(|a| first_in(&s.obs, &reads(a)));
                    v.is_some_and(|v| s.obs.remove(&v))
                });
                // An input taken from the S actions that read it.
                plant(Clause::C, &|s| {
                    let v = s.inputs.first().copied();
                    v.is_some_and(|v| s.inputs.remove(&v))
                });
                // An S var an S action reads moved to T, where no fold or input reads it.
                plant(Clause::C, &|s| {
                    let v = s.s_actions.iter().find_map(|a| first_in(&s.s, &reads(a)));
                    v.is_some_and(|v| s.s.remove(&v) && s.t.insert(v))
                });
                // A T var that is not monotone made an input.
                plant(Clause::D, &|s| {
                    let v = s.t.iter().copied().find(|v| !monotone[v.index()]);
                    v.is_some_and(|v| s.inputs.insert(v))
                });
                // A seed's S var dropped from obs.
                plant(Clause::E, &|s| {
                    let v = seeds.iter().find_map(|(seed, _)| first_in(&s.obs, seed));
                    v.is_some_and(|v| s.obs.remove(&v))
                });
                // A kept action moved to S_a.
                plant(Clause::E, &|s| {
                    let a = seeds.iter().find_map(|(_, kept)| kept.first().copied());
                    a.is_some_and(|a| s.t_actions.remove(&a) && s.s_actions.insert(a))
                });
            }
        }
        let caps = Caps::new(usize::MAX, 200_000);
        let mut fell_back = 0;
        for (sys, seeds) in cone_systems() {
            let reduced = sys.reduced(&seeds, caps);
            let (faults, planted) = dropping("split_sides", || {
                let split = sys.split_with(&seeds, true);
                let faults = sys.split_faults(&seeds, &split);
                let clauses: Vec<Clause> = faults.iter().map(|f| f.clause).collect();
                (clauses, sys.reduced(&seeds, caps))
            });
            if !faults.contains(&Clause::Sides) {
                continue;
            }
            fired.insert(Clause::Sides);
            if reduced.reduction().is_some() {
                fell_back += 1;
                assert!(planted == sys, "the cone is decided unreduced");
            }
        }
        let all = [
            Clause::Split,
            Clause::A,
            Clause::B,
            Clause::C,
            Clause::D,
            Clause::E,
            Clause::Sides,
        ];
        assert_eq!(fired, all.into_iter().collect(), "every clause fails once");
        assert!(fell_back > 0, "a reduced cone falls back");
    }
}
