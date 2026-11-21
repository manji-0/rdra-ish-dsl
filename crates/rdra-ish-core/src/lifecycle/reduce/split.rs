//! The split of a cone into the target side T and the independent sub-lifecycle S.
use std::collections::{BTreeSet, HashMap};

use super::fold::Fold;
use crate::lifecycle::cone::{Seed, VarSet};
use crate::lifecycle::types::{ActId, Effect, Rhs, Value, VarId};
use crate::lifecycle::TransitionSystem;

/// The split of a cone.
pub(in crate::lifecycle) struct Split {
    pub(super) t: VarSet,
    pub(in crate::lifecycle) s: VarSet,
    pub(super) t_actions: BTreeSet<ActId>,
    pub(in crate::lifecycle) s_actions: BTreeSet<ActId>,
    pub(super) obs: VarSet,
    /// T vars the S actions read raw (#112), each monotone, with the existence of each one's
    /// entity: the raw inputs of S.
    pub(in crate::lifecycle) inputs: VarSet,
    /// The S actions whose reads of T fold into a bit.
    pub(in crate::lifecycle) folds: Vec<Fold>,
}

impl TransitionSystem {
    /// Per var, whether no action lowers it (#112): the writes of the system, as edges
    /// between the var's values, form no cycle. A create takes an existence from absent to
    /// present and a delete from present to deleted; a delete resets the entity's columns
    /// and owners, which makes them cyclic; an assignment or a choice in the action that
    /// creates the row starts from base, elsewhere from any value; a transition is its arms;
    /// the clock ticks up. Such a var changes at most as often as its domain has values, so
    /// a sub-lifecycle that reads it as a free input sees finitely many input histories.
    pub(crate) fn monotone_vars(&self) -> Vec<bool> {
        let n = self.vars.len();
        let mut edges: Vec<BTreeSet<(u8, u8)>> = vec![BTreeSet::new(); n];
        let mut cyclic = vec![false; n];
        for a in &self.actions {
            let creates: BTreeSet<u8> = a
                .effects
                .iter()
                .filter_map(|e| match e {
                    Effect::Create { param } => Some(*param),
                    _ => None,
                })
                .collect();
            let entity = |p: &u8| self.entity(a.params[usize::from(*p)].entity);
            let froms = |var: VarId, param: &u8| -> Vec<u8> {
                let decl = self.var(var);
                if creates.contains(param) {
                    vec![decl.base.0]
                } else {
                    (0..decl.domain.size(self.bounds) as u8).collect()
                }
            };
            for eff in &a.effects {
                match eff {
                    Effect::Create { param } => {
                        edges[entity(param).exists.index()]
                            .insert((Value::ABSENT.0, Value::PRESENT.0));
                    }
                    Effect::Delete { param } => {
                        let d = entity(param);
                        edges[d.exists.index()].insert((Value::PRESENT.0, Value::DELETED.0));
                        for v in d.columns.iter().chain(&d.owners) {
                            cyclic[v.index()] = true;
                        }
                    }
                    Effect::Assign {
                        var, param, rhs, ..
                    } => {
                        let tos: Vec<u8> = match rhs {
                            Rhs::Lit(v) => vec![v.0],
                            Rhs::Param(_) => (0..self.bounds.slots() as u8)
                                .map(|c| self.var(*var).domain.of_slot(c).0)
                                .collect(),
                        };
                        for f in froms(*var, param) {
                            edges[var.index()].extend(tos.iter().map(|t| (f, *t)));
                        }
                    }
                    Effect::Transition { var, arms, .. } => {
                        edges[var.index()].extend(arms.iter().map(|(f, t)| (f.0, t.0)));
                    }
                    Effect::Choose {
                        var, param, values, ..
                    } => {
                        for f in froms(*var, param) {
                            edges[var.index()].extend(values.iter().map(|v| (f, v.0)));
                        }
                    }
                    Effect::Tick => {
                        if let Some(now) = self.now {
                            edges[now.index()].extend((0..self.bounds.int_max).map(|i| (i, i + 1)));
                        }
                    }
                }
            }
        }
        (0..n).map(|v| !cyclic[v] && acyclic(&edges[v])).collect()
    }

    /// The split for one anchor: T holds the anchor and the writes of the kept actions, and
    /// any action that writes a T var, or that writes into the cone while reading a T var
    /// that is not monotone, has every write in T. So S is closed up to its inputs: its
    /// writers read S and monotone T vars, and write only S. An entity's vars share a side
    /// (`own_existence`): a T entity's view is observed in full, and S is a system of whole
    /// entities, whose owner vars may name a parent in T (`restrict_to`).
    pub(super) fn split_at(
        &self,
        anchor: Option<VarId>,
        keep: &BTreeSet<ActId>,
        reads: &[VarSet],
        writes: &[VarSet],
        monotone: &[bool],
    ) -> Seed {
        let own: Vec<Option<VarId>> = self.var_ids().map(|v| self.own_existence(v)).collect();
        let mut t: VarSet = anchor.into_iter().collect();
        let mut t_actions = BTreeSet::new();
        loop {
            let mut grew = false;
            for i in 0..self.actions.len() {
                let id = ActId::at(i);
                if t_actions.contains(&id) || (writes[i].is_empty() && !keep.contains(&id)) {
                    continue;
                }
                let reads_t = reads[i]
                    .iter()
                    .any(|v| t.contains(v) && !monotone[v.index()]);
                if keep.contains(&id) || !writes[i].is_disjoint(&t) || reads_t {
                    t_actions.insert(id);
                    t.extend(writes[i].iter().copied());
                    grew = true;
                }
            }
            for v in self.var_ids() {
                let Some(e) = own[v.index()] else { continue };
                grew |= (t.contains(&v) && t.insert(e)) || (t.contains(&e) && t.insert(v));
            }
            if !grew {
                return (t, t_actions);
            }
        }
    }

    /// The split of the cone for its targets' seeds and kept actions: of the splits anchored
    /// at each var the seeds read, the one with the largest S, the first on a tie. What the
    /// seeds read of S is observed, so every target's check reads T and `obs` only. With
    /// `inputs`, an S action may read monotone T vars, which become the inputs of S; without,
    /// every reader of a T var joins T.
    pub(in crate::lifecycle) fn split_with(&self, seeds: &[Seed], inputs: bool) -> Split {
        // A copy's reads count here too: an S action that copies from T keeps the copy's
        // guard through its inputs, or joins T.
        let reads: Vec<VarSet> = self
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
        let writes: Vec<VarSet> = self.actions.iter().map(|a| self.action_writes(a)).collect();
        let monotone: Vec<bool> = if inputs {
            self.monotone_vars()
        } else {
            vec![false; self.vars.len()]
        };
        let keep: BTreeSet<ActId> = seeds.iter().flat_map(|(_, k)| k.iter().copied()).collect();
        let mut anchors: Vec<Option<VarId>> = seeds
            .iter()
            .flat_map(|(s, _)| s.iter().copied().map(Some))
            .collect();
        anchors.dedup();
        if anchors.is_empty() {
            anchors.push(None);
        }
        let (t, t_actions) = anchors
            .into_iter()
            .map(|a| self.split_at(a, &keep, &reads, &writes, &monotone))
            .min_by_key(|(t, _)| t.len())
            .expect("an anchor");
        let s: VarSet = self.var_ids().filter(|v| !t.contains(v)).collect();
        let s_actions: BTreeSet<ActId> = self
            .action_ids()
            .filter(|id| !t_actions.contains(id) && !writes[id.index()].is_empty())
            .collect();
        let mut obs = VarSet::new();
        for id in &t_actions {
            obs.extend(reads[id.index()].intersection(&s).copied());
        }
        for (seed, _) in seeds {
            obs.extend(seed.intersection(&s).copied());
        }
        let folds: Vec<Fold> = s_actions
            .iter()
            .filter_map(|id| self.fold_of(*id, &t))
            .filter(|f| self.fold_initial(f).is_some())
            .collect();
        let mut inputs = VarSet::new();
        for id in &s_actions {
            let fold = folds.iter().find(|f| f.act == *id);
            inputs.extend(self.raw_reads(*id, fold).intersection(&t).copied());
        }
        for v in inputs.clone() {
            inputs.extend(self.existences(v));
        }
        inputs.retain(|v| t.contains(v));
        assert!(
            inputs.iter().all(|v| monotone[v.index()]),
            "every input of S is monotone"
        );
        Split {
            t,
            s,
            t_actions,
            s_actions,
            obs,
            inputs,
            folds,
        }
    }
}

/// Whether a var's write edges form no cycle; a self-loop is not one.
pub(super) fn acyclic(edges: &BTreeSet<(u8, u8)>) -> bool {
    pub(super) fn visit(v: u8, edges: &BTreeSet<(u8, u8)>, done: &mut HashMap<u8, bool>) -> bool {
        if let Some(finished) = done.get(&v) {
            return *finished;
        }
        done.insert(v, false);
        for &(a, b) in edges {
            if a == v && b != v && !visit(b, edges, done) {
                return false;
            }
        }
        done.insert(v, true);
        true
    }
    let mut done = HashMap::new();
    edges
        .iter()
        .flat_map(|(a, b)| [*a, *b])
        .all(|v| visit(v, edges, &mut done))
}
