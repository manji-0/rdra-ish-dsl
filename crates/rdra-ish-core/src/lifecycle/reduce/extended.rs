//! The extended sub-lifecycle of a cone: S with its inputs, and the rises.
use std::collections::{BTreeSet, HashMap};

use super::fold::{Fold, FoldKey};
use super::split::Split;
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::restrict::{renumber_by, renumber_pred, Remap};
use crate::lifecycle::types::{
    ActId, Action, ActionOrigin, At, CmpOp, Domain, Effect, EntId, Guard, GuardWhy, ModelRef,
    Param, ParamRole, Pred, Rhs, Site, Start, Term, Value, VarDecl, VarId, VarKind,
};
use crate::lifecycle::TransitionSystem;

/// The extended sub-lifecycle of a cone (`restrict_to(S ∪ inputs)`), with its actions sorted
/// into the S actions of the cone, whose steps are silent, and the rises: T actions that
/// write a raw input, kept with the guards they have on S and the inputs only, and the flips
/// of the input bits.
pub(super) struct Extended {
    pub(super) system: TransitionSystem,
    /// Per action of `system`, the cone's action it comes from; `None` for a bit's flip.
    pub(super) acts: Vec<Option<ActId>>,
    /// Per action of `system`, whether it is a rise.
    pub(super) rise: Vec<bool>,
    pub(super) obs_local: VarSet,
    /// The raw inputs and the bit vars, in `system`'s ids.
    pub(super) inputs_local: VarSet,
    /// The bit groups, in the cone's ids.
    pub(super) bits: Vec<Vec<Fold>>,
    pub(super) variants: HashMap<ActId, Vec<ActId>>,
    /// Per cone action, its param index to `system`'s, when the restriction dropped one.
    pub(super) params: Vec<Option<Vec<Option<u8>>>>,
}

impl Extended {
    pub(super) fn of(cone: &TransitionSystem, split: &Split) -> Self {
        Self::build(
            cone,
            &split.s,
            &split.inputs,
            &split.s_actions,
            &split.obs,
            &split.folds,
        )
    }

    /// The extended sub-lifecycle of a reduced cone, as `of` built it.
    pub(super) fn rebuild(cone: &TransitionSystem, r: &Reduction) -> Self {
        let folds: Vec<Fold> = r.bits.iter().flatten().cloned().collect();
        Self::build(
            cone,
            &r.vars.iter().copied().collect(),
            &r.inputs.iter().copied().collect(),
            &r.s_actions,
            &r.obs,
            &folds,
        )
    }

    /// `restrict_to(S ∪ inputs)` with one bit var per group of folded S actions, each such
    /// action reading its bit in place of its guards on T (which the restriction dropped
    /// with their T params) and with its key's T param bound to any slot, the variants of
    /// one group merged into one action, and per bit a rise and a fall that flip it freely.
    pub(super) fn build(
        cone: &TransitionSystem,
        s: &VarSet,
        inputs: &VarSet,
        s_actions: &BTreeSet<ActId>,
        obs: &VarSet,
        folds: &[Fold],
    ) -> Self {
        let all: VarSet = s.union(inputs).copied().collect();
        let (mut system, map) = cone.restrict_to_mapped(&all, &BTreeSet::new(), false);
        let local = |v: &VarId| VarId::at(all.iter().position(|x| x == v).expect("in S ∪ inputs"));
        let mut acts: Vec<Option<ActId>> = vec![None; system.actions.len()];
        for (from, to) in map.act.iter().enumerate() {
            if let Some(to) = to {
                acts[to.index()] = Some(ActId::at(from));
            }
        }
        let t: VarSet = cone.var_ids().filter(|v| !s.contains(v)).collect();
        let mut groups = BitGroups::default();
        let mut drop: Vec<bool> = vec![false; system.actions.len()];
        for fold in folds {
            match &fold.key {
                FoldKey::Param(key) => {
                    if groups.fold_param(&mut system, &map, fold, *key) {
                        drop[map.act[fold.act.index()]
                            .expect("an S action is kept")
                            .index()] = true;
                    }
                }
                FoldKey::Linked { .. } => groups.fold_linked(cone, &mut system, &map, &t, fold),
            }
        }
        let kept: Vec<usize> = (0..system.actions.len()).filter(|i| !drop[*i]).collect();
        system.actions = kept.iter().map(|&i| system.actions[i].clone()).collect();
        acts = kept.iter().map(|&i| acts[i]).collect();
        // A bit starts where the cone starts: an entity no use case creates is present from
        // the first state, and a bit over it is raised there.
        for (g, &var) in groups.vars.iter().enumerate() {
            if groups.bits[g]
                .iter()
                .any(|f| cone.fold_initial(f) == Some(true))
            {
                system.vars[var.index()].base = Value::TRUE;
            }
        }
        for (k, &var) in groups.vars.iter().enumerate() {
            let VarKind::Run { entity, .. } = system.vars[var.index()].kind else {
                unreachable!("a bit is a run mark")
            };
            system
                .actions
                .extend(flips(k, var, entity, cone.actions.len() + 2 * k));
            acts.extend([None, None]);
        }
        system.layout();
        let rise = acts
            .iter()
            .map(|a| a.is_none_or(|a| !s_actions.contains(&a)))
            .collect();
        let mut inputs_local: VarSet = inputs.iter().map(local).collect();
        inputs_local.extend(groups.vars);
        Extended {
            system,
            acts,
            rise,
            obs_local: obs.iter().map(local).collect(),
            inputs_local,
            bits: groups.bits,
            variants: groups.variants,
            params: map.params,
        }
    }

    /// A step's params in the cone's numbering: each param of cone action `act` takes the
    /// slot of the param it became in `system`. A param the restriction dropped (a T param
    /// whose guards fold into a bit) takes slot 0, as the extended sub-lifecycle binds a
    /// fold's key to any slot; params a fold added past the cone's are left out.
    pub(super) fn cone_params(
        &self,
        cone: &TransitionSystem,
        act: ActId,
        params: &[u8],
    ) -> Box<[u8]> {
        let n = cone.actions[act.index()].params.len();
        match &self.params[act.index()] {
            None => params[..n.min(params.len())].into(),
            Some(index) => index
                .iter()
                .map(|j| j.map_or(0, |j| params[usize::from(j)]))
                .collect(),
        }
    }

    /// The actions of `system` that are rises.
    pub(super) fn rise_actions(&self) -> BTreeSet<ActId> {
        (0..self.rise.len())
            .filter(|i| self.rise[*i])
            .map(ActId::at)
            .collect()
    }
}

/// The bit groups of an extended sub-lifecycle as its folds open and join them, in order.
#[derive(Default)]
struct BitGroups {
    /// A group's signature to its index.
    index: HashMap<String, usize>,
    /// Per group, its folds, the first one first.
    bits: Vec<Vec<Fold>>,
    /// Per group, its bit var.
    vars: Vec<VarId>,
    /// Per group's first action, the actions merged into it.
    variants: HashMap<ActId, Vec<ActId>>,
}

impl BitGroups {
    /// Adds `fold` to the group of `signature` and returns its index, if there is one.
    fn join(&mut self, signature: &str, fold: &Fold) -> Option<usize> {
        let g = *self.index.get(signature)?;
        self.bits[g].push(fold.clone());
        Some(g)
    }

    /// A new group of `signature` with `fold` only, and its bit `var`.
    fn open(&mut self, signature: String, fold: &Fold, var: VarId) {
        self.index.insert(signature, self.bits.len());
        self.bits.push(vec![fold.clone()]);
        self.vars.push(var);
    }

    /// Folds an S action keyed by one of its params: the action reads its bit at the key in
    /// place of its guards on T, and an action that matches an earlier group's first (in
    /// params, guards, effects, and key) merges into it. Returns whether it merged, so its
    /// own action is dropped.
    fn fold_param(
        &mut self,
        system: &mut TransitionSystem,
        map: &Remap,
        fold: &Fold,
        key: u8,
    ) -> bool {
        let ext_id = map.act[fold.act.index()].expect("an S action is kept");
        let local_key = renumber(map, fold, key);
        let a = &mut system.actions[ext_id.index()];
        if fold.t_params.iter().any(|(p, _)| *p == key) {
            a.params[usize::from(local_key)].role = ParamRole::Earlier { step: 0 };
        }
        let entity = a.params[usize::from(local_key)].entity;
        let name = a.name.clone();
        let signature = format!(
            "{:?}",
            (
                &a.params,
                a.guards.iter().map(|g| &g.pred).collect::<Vec<_>>(),
                &a.effects,
                local_key
            )
        );
        if let Some(g) = self.join(&signature, fold) {
            let first = self.bits[g][0].act;
            self.variants
                .get_mut(&first)
                .expect("the group's first")
                .push(fold.act);
            return true;
        }
        let var = new_bit(system, entity, &name);
        system.actions[ext_id.index()]
            .guards
            .push(bit_is(var, local_key, Value::TRUE));
        self.open(signature, fold, var);
        self.variants.insert(fold.act, vec![fold.act]);
        false
    }

    /// Folds an S action keyed by a T row a quantifier in one of its guards binds: the
    /// action gains a param for the row, and the guard reads the row's bit, its link to the
    /// S row, and the node's parts outside T in place of the node.
    fn fold_linked(
        &mut self,
        cone: &TransitionSystem,
        system: &mut TransitionSystem,
        map: &Remap,
        t: &VarSet,
        fold: &Fold,
    ) {
        let FoldKey::Linked {
            guard,
            entity,
            link_var,
            s_param,
            body,
        } = &fold.key
        else {
            unreachable!("a linked fold")
        };
        let ext_id = map.act[fold.act.index()].expect("an S action is kept");
        let signature = format!("{:?}", (entity, link_var, body));
        let var = match self.join(&signature, fold) {
            Some(g) => self.vars[g],
            None => {
                let name = format!("{}_{guard}", system.actions[ext_id.index()].name);
                let var = new_bit(system, map.e(*entity), &name);
                self.open(signature, fold, var);
                var
            }
        };
        let a = &mut system.actions[ext_id.index()];
        let q = a.params.len() as u8;
        a.params.push(Param {
            entity: map.e(*entity),
            role: ParamRole::Earlier { step: 0 },
            via_api: None,
        });
        let g = &cone.actions[fold.act.index()].guards[*guard];
        let node = cone
            .node_in(&g.pred, t)
            .expect("a linked fold's guard holds its node");
        let rest = |p: &Pred| {
            let mut p = map.pred(p);
            if let Some(m) = &map.params[fold.act.index()] {
                renumber_pred(&mut p, &renumber_by(m));
            }
            p
        };
        let mut stand_in: Vec<Pred> = node.outside.iter().map(rest).collect();
        stand_in.push(Pred::Cmp(
            Term::Slot {
                var: map.v(*link_var),
                at: At::Param(renumber(map, fold, *s_param)),
            },
            CmpOp::Eq,
            Term::Index(At::Param(q)),
        ));
        stand_in.push(bit_is(var, q, Value::TRUE).pred);
        let stand_in = Pred::And(stand_in);
        let pred = cone.substitute_node(&g.pred, t, &node, &stand_in, &rest);
        a.guards.push(Guard {
            pred,
            why: g.why,
            site: Site::bare(ModelRef::Synthesized),
        });
        self.variants.insert(fold.act, vec![fold.act]);
    }
}

/// Param `p` of a fold's cone action, in the extended sub-lifecycle's numbering.
fn renumber(map: &Remap, fold: &Fold, p: u8) -> u8 {
    match &map.params[fold.act.index()] {
        Some(m) => m[usize::from(p)].expect("an S param is kept"),
        None => p,
    }
}

/// A new bit var over `entity`, false at first.
fn new_bit(system: &mut TransitionSystem, entity: EntId, name: &str) -> VarId {
    let var = VarId::at(system.vars.len());
    system.vars.push(VarDecl {
        kind: VarKind::Run {
            entity,
            usecase: format!("in_{name}"),
        },
        domain: Domain::Bool,
        base: Value::FALSE,
        start: Start::Base,
        site: Site::bare(ModelRef::Synthesized),
        ends: Vec::new(),
    });
    var
}

/// The guard that bit `var` is `value` at param `at`.
fn bit_is(var: VarId, at: u8, value: Value) -> Guard {
    Guard {
        pred: Pred::Cmp(
            Term::Slot {
                var,
                at: At::Param(at),
            },
            CmpOp::Eq,
            Term::Lit(value),
        ),
        why: GuardWhy::Run { step: 0 },
        site: Site::bare(ModelRef::Synthesized),
    }
}

/// The rise and the fall of bit `k`, `var` over `entity`, whose sources start at `source`.
fn flips(k: usize, var: VarId, entity: EntId, source: usize) -> [Action; 2] {
    [
        ("Rise", Value::FALSE, Value::TRUE),
        ("Fall", Value::TRUE, Value::FALSE),
    ]
    .map(|(name, from, to)| Action {
        name: format!("{name}_in_{k}"),
        source: ActId::at(source + usize::from(to == Value::FALSE)),
        origin: ActionOrigin::Clock,
        site: Site::bare(ModelRef::Synthesized),
        params: vec![Param {
            entity,
            role: ParamRole::Earlier { step: 0 },
            via_api: None,
        }],
        guards: vec![bit_is(var, 0, from)],
        effects: vec![Effect::Assign {
            var,
            param: 0,
            rhs: Rhs::Lit(to),
            site: Site::bare(ModelRef::Synthesized),
        }],
    })
}
