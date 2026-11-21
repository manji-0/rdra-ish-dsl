//! The extended sub-lifecycle of a cone: S with its inputs, and the rises.
use std::collections::{BTreeSet, HashMap};

use super::fold::{Fold, FoldKey};
use super::split::Split;
use super::*;
use crate::lifecycle::cone::VarSet;
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
        let mut acts: Vec<Option<ActId>> = system
            .actions
            .iter()
            .map(|a| {
                let at = cone
                    .actions
                    .iter()
                    .position(|b| b.source == a.source)
                    .expect("an action of the extended sub-lifecycle is an action of the cone");
                Some(ActId::at(at))
            })
            .collect();
        let mut bits: Vec<Vec<Fold>> = Vec::new();
        let mut bit_vars: Vec<VarId> = Vec::new();
        let mut variants: HashMap<ActId, Vec<ActId>> = HashMap::new();
        let mut groups: HashMap<String, usize> = HashMap::new();
        let mut drop: Vec<bool> = vec![false; system.actions.len()];
        let t: VarSet = cone.var_ids().filter(|v| !s.contains(v)).collect();
        let new_bit = |system: &mut TransitionSystem, entity: EntId, name: &str| {
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
        };
        let bit_guard = |var: VarId, key: u8| Guard {
            pred: Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Param(key),
                },
                CmpOp::Eq,
                Term::Lit(Value::TRUE),
            ),
            why: GuardWhy::Run { step: 0 },
            site: Site::bare(ModelRef::Synthesized),
        };
        for fold in folds {
            let ext_id = map.act[fold.act.index()].expect("an S action is kept");
            let renumber = |p: u8| match &map.params[fold.act.index()] {
                Some(m) => m[usize::from(p)].expect("an S param is kept"),
                None => p,
            };
            let name = system.actions[ext_id.index()].name.clone();
            let key = match &fold.key {
                FoldKey::Param(key) => *key,
                FoldKey::Linked {
                    guard,
                    entity,
                    link_var,
                    s_param,
                    body,
                } => {
                    let signature = format!("{:?}", (entity, link_var, body));
                    let var = match groups.get(&signature) {
                        Some(&g) => {
                            bits[g].push(fold.clone());
                            bit_vars[g]
                        }
                        None => {
                            let var =
                                new_bit(&mut system, map.e(*entity), &format!("{name}_{guard}"));
                            groups.insert(signature, bits.len());
                            bits.push(vec![fold.clone()]);
                            bit_vars.push(var);
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
                        .node_in(&g.pred, &t)
                        .expect("a linked fold's guard holds its node");
                    let rest = |p: &Pred| {
                        let mut p = map.pred(p);
                        if let Some(m) = &map.params[fold.act.index()] {
                            crate::lifecycle::restrict::renumber_pred(
                                &mut p,
                                &crate::lifecycle::restrict::renumber_by(m),
                            );
                        }
                        p
                    };
                    let mut stand_in: Vec<Pred> = node.outside.iter().map(rest).collect();
                    stand_in.push(Pred::Cmp(
                        Term::Slot {
                            var: map.v(*link_var),
                            at: At::Param(renumber(*s_param)),
                        },
                        CmpOp::Eq,
                        Term::Index(At::Param(q)),
                    ));
                    stand_in.push(bit_guard(var, q).pred);
                    let stand_in = Pred::And(stand_in);
                    let pred = cone.substitute_node(&g.pred, &t, &node, &stand_in, &rest);
                    a.guards.push(Guard {
                        pred,
                        why: g.why,
                        site: Site::bare(ModelRef::Synthesized),
                    });
                    variants.insert(fold.act, vec![fold.act]);
                    continue;
                }
            };
            let local_key = renumber(key);
            let a = &mut system.actions[ext_id.index()];
            if fold.t_params.iter().any(|(p, _)| *p == key) {
                a.params[usize::from(local_key)].role = ParamRole::Earlier { step: 0 };
            }
            let entity = a.params[usize::from(local_key)].entity;
            let signature = format!(
                "{:?}",
                (
                    &a.params,
                    a.guards.iter().map(|g| &g.pred).collect::<Vec<_>>(),
                    &a.effects,
                    local_key
                )
            );
            match groups.get(&signature) {
                Some(&g) => {
                    bits[g].push(fold.clone());
                    let first = bits[g][0].act;
                    variants
                        .get_mut(&first)
                        .expect("the group's first")
                        .push(fold.act);
                    drop[ext_id.index()] = true;
                }
                None => {
                    let var = new_bit(&mut system, entity, &name);
                    system.actions[ext_id.index()]
                        .guards
                        .push(bit_guard(var, local_key));
                    groups.insert(signature, bits.len());
                    bits.push(vec![fold.clone()]);
                    bit_vars.push(var);
                    variants.insert(fold.act, vec![fold.act]);
                }
            }
        }
        let kept: Vec<usize> = (0..system.actions.len()).filter(|i| !drop[*i]).collect();
        system.actions = kept.iter().map(|&i| system.actions[i].clone()).collect();
        acts = kept.iter().map(|&i| acts[i]).collect();
        // A bit starts where the cone starts: an entity no use case creates is present from
        // the first state, and a bit over it is raised there.
        for (g, &var) in bit_vars.iter().enumerate() {
            if bits[g].iter().any(|f| cone.fold_initial(f) == Some(true)) {
                system.vars[var.index()].base = Value::TRUE;
            }
        }
        for (k, &var) in bit_vars.iter().enumerate() {
            let VarKind::Run { entity, .. } = system.vars[var.index()].kind else {
                unreachable!("a bit is a run mark")
            };
            for (name, from, to) in [
                ("Rise", Value::FALSE, Value::TRUE),
                ("Fall", Value::TRUE, Value::FALSE),
            ] {
                system.actions.push(Action {
                    name: format!("{name}_in_{k}"),
                    source: ActId::at(cone.actions.len() + 2 * k + usize::from(to == Value::FALSE)),
                    origin: ActionOrigin::Clock,
                    site: Site::bare(ModelRef::Synthesized),
                    params: vec![Param {
                        entity,
                        role: ParamRole::Earlier { step: 0 },
                        via_api: None,
                    }],
                    guards: vec![Guard {
                        pred: Pred::Cmp(
                            Term::Slot {
                                var,
                                at: At::Param(0),
                            },
                            CmpOp::Eq,
                            Term::Lit(from),
                        ),
                        why: GuardWhy::Run { step: 0 },
                        site: Site::bare(ModelRef::Synthesized),
                    }],
                    effects: vec![Effect::Assign {
                        var,
                        param: 0,
                        rhs: Rhs::Lit(to),
                        site: Site::bare(ModelRef::Synthesized),
                    }],
                });
                acts.push(None);
            }
        }
        system.layout();
        let rise = acts
            .iter()
            .map(|a| a.is_none_or(|a| !s_actions.contains(&a)))
            .collect();
        let mut inputs_local: VarSet = inputs.iter().map(local).collect();
        inputs_local.extend(bit_vars);
        Extended {
            system,
            acts,
            rise,
            obs_local: obs.iter().map(local).collect(),
            inputs_local,
            bits,
            variants,
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
