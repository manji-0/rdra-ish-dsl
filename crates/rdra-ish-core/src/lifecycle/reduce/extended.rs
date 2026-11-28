//! The extended sub-lifecycle of a cone: S with its inputs, and the rises.
use crate::lifecycle::narrow::id8;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::fold::{Fold, FoldKey};
use super::split::Split;
use super::*;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::explore::symmetry::dropped;
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
    /// The entities of `system` that `H` leaves where they are (#212 phase 2), each with
    /// every reason found for it: one that holds an `obs` var ("obs"), an input or a bit
    /// ("input"), or whose rows such a key names ("named by obs", "named by an input"); a
    /// stub of a T parent ("stub"); and one whose rows a key of the cone's T can name ("named
    /// by T"). `fixed` gives the set `Group::fixing` leaves out.
    pub(super) fixed_why: BTreeMap<EntId, Vec<&'static str>>,
    /// The check of claim A8 every build makes on `system` (`StubCheck`).
    pub(super) stubs: StubCheck,
}

/// What every build checks on the extended sub-lifecycle once it is built (claim A8 of
/// docs/theory/claims.md, Lemma 6 of docs/theory/cones.md): no param of any action, a folded
/// or unfolded S action, a rise, or a bit's flip, needs a present row (`Existing`, `Parent`,
/// `Triggered`) of a stub, which is absent in every state and so would never bind. A fault
/// makes `reduced_with` decide the cone unreduced.
#[derive(Debug, Default)]
pub(super) struct StubCheck {
    /// The params on a stub that `bind_stub_params` bound to any slot.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) rebound: usize,
    /// The first param found that breaks the check.
    pub(super) fault: Option<StubParam>,
}

/// A param on a stub that needs a present row of it, or that `bind_stub_params` would bind
/// freely though neither a fold nor a rise stands for it: the param `param` of action
/// `action` of the extended sub-lifecycle.
#[derive(Debug)]
pub(super) struct StubParam {
    pub(super) action: String,
    pub(super) param: usize,
    /// It is an unfolded S action's, or a T param of a folded one outside the fold.
    pub(super) unfolded: bool,
}

impl std::fmt::Display for StubParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (action, param) = (&self.action, self.param);
        match self.unfolded {
            true => write!(
                f,
                "param {param} of S action {action} is on a stub, and no fold stands for it"
            ),
            false => write!(
                f,
                "param {param} of {action} needs a present row of a stub, which never binds"
            ),
        }
    }
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
    /// with their T params) and with its key's T param bound to any slot, every param on a
    /// stub bound to any slot too (`bind_stub_params`), the variants of one group merged into
    /// one action, and per bit a rise and a fall that flip it freely.
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
        let mut stubs = match dropped("bind_stub_params") {
            true => StubCheck::default(),
            false => bind_stub_params(&mut system, &map, &acts, all.len(), s_actions, folds),
        };
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
        for (k, (&var, &entity)) in groups.vars.iter().zip(&groups.entities).enumerate() {
            system
                .actions
                .extend(flips(k, var, entity, cone.actions.len() + 2 * k));
            acts.extend([None, None]);
        }
        system.layout();
        if stubs.fault.is_none() {
            stubs.fault = stub_param(&system, all.len());
        }
        let rise = acts
            .iter()
            .map(|a| a.is_none_or(|a| !s_actions.contains(&a)))
            .collect();
        let mut inputs_local: VarSet = inputs.iter().map(local).collect();
        inputs_local.extend(groups.vars);
        let obs_local: VarSet = obs.iter().map(local).collect();
        // Each reason is one a `shadow` build or a test can drop (`dropped`,
        // `Observed:<reason>`): a dropped reason is not added.
        let mut fixed_why: BTreeMap<EntId, Vec<&'static str>> = BTreeMap::new();
        let mut add = |e: EntId, why: &'static str| {
            if dropped(&format!("Observed:{why}")) {
                return;
            }
            let reasons = fixed_why.entry(e).or_default();
            if !reasons.contains(&why) {
                reasons.push(why);
            }
        };
        for (vars, held, named) in [
            (&obs_local, "obs", "named by obs"),
            (&inputs_local, "input", "named by an input"),
        ] {
            for v in vars {
                let d = &system.vars[v.index()];
                if let Some(e) = d.kind.entity() {
                    add(e, held);
                }
                if let Domain::Instance { parent, .. } = d.domain {
                    add(parent, named);
                }
            }
        }
        // The stubs come after the entities whose existence is in S or the inputs.
        for e in (0..system.entities.len()).map(EntId::at) {
            if system.entities[e.index()].exists.index() >= all.len() {
                add(e, "stub");
            }
        }
        for v in &t {
            if let Domain::Instance { parent, .. } = cone.vars[v.index()].domain {
                if let Some(e) = map.ent_of(parent) {
                    add(e, "named by T");
                }
            }
        }
        Extended {
            system,
            acts,
            rise,
            obs_local,
            inputs_local,
            bits: groups.bits,
            variants: groups.variants,
            params: map.params,
            fixed_why,
            stubs,
        }
    }

    /// The entities of `system` that `H` leaves where they are: those of `fixed_why`.
    pub(super) fn fixed(&self) -> Vec<EntId> {
        self.fixed_why.keys().copied().collect()
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

/// What two folds share to share a bit group. The two kinds never equal each other.
#[derive(PartialEq, Eq, Hash)]
enum Signature {
    /// A fold keyed by a param: the folded action's params, guards and effects (their sites
    /// included, so variants written on different lines never merge), and the key.
    Param {
        params: Vec<Param>,
        guards: Vec<Pred>,
        effects: Vec<Effect>,
        key: u8,
    },
    /// A linked fold: the T entity, the link var, and the folded node's body.
    Linked {
        entity: EntId,
        link_var: VarId,
        body: Pred,
    },
}

/// The bit groups of an extended sub-lifecycle as its folds open and join them, in order.
#[derive(Default)]
struct BitGroups {
    /// A group's signature to its index.
    index: HashMap<Signature, usize>,
    /// Per group, its folds, the first one first.
    bits: Vec<Vec<Fold>>,
    /// Per group, its bit var.
    vars: Vec<VarId>,
    /// Per group, the entity its bit var is over.
    entities: Vec<EntId>,
    /// Per group's first action, the actions merged into it.
    variants: HashMap<ActId, Vec<ActId>>,
}

impl BitGroups {
    /// Adds `fold` to the group of `signature` and returns its index, if there is one.
    fn join(&mut self, signature: &Signature, fold: &Fold) -> Option<usize> {
        let g = *self.index.get(signature)?;
        self.bits[g].push(fold.clone());
        Some(g)
    }

    /// A new group of `signature` with `fold` only, and its bit `var` over `entity`.
    fn open(&mut self, signature: Signature, fold: &Fold, var: VarId, entity: EntId) {
        self.index.insert(signature, self.bits.len());
        self.bits.push(vec![fold.clone()]);
        self.vars.push(var);
        self.entities.push(entity);
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
        let signature = Signature::Param {
            params: a.params.clone(),
            guards: a.guards.iter().map(|g| g.pred.clone()).collect(),
            effects: a.effects.clone(),
            key: local_key,
        };
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
        self.open(signature, fold, var, entity);
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
        let signature = Signature::Linked {
            entity: *entity,
            link_var: *link_var,
            body: body.clone(),
        };
        let var = match self.join(&signature, fold) {
            Some(g) => self.vars[g],
            None => {
                let name = format!("{}_{guard}", system.actions[ext_id.index()].name);
                let var = new_bit(system, map.e(*entity), &name);
                self.open(signature, fold, var, map.e(*entity));
                var
            }
        };
        let a = &mut system.actions[ext_id.index()];
        let q = id8(a.params.len());
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

/// Binds every param on a stub to any slot (claims A8, A11 of docs/theory/claims.md). A stub is
/// an entity of T whose existence `system` does not hold: `restrict_to` keeps it for an owner
/// var of S, and no action writes it, so a param whose role needs a present row of it
/// (`Existing`, `Parent`, `Triggered`) would never bind, and its action would never run,
/// although it runs in the cone. Two kinds of action keep such a param:
///
/// - a folded S action, whose T params the bit stands for: the bit holds at the key when some
///   binding of them is on rows that are present where they must be and makes every folded
///   guard hold, and no effect names a T param but the key (`fold_of`), so the step's
///   successor is the same at every binding and binding them freely adds no step and drops
///   none;
/// - a rise, which keeps only its guards on S and the inputs, and so may already run where
///   the cone's T step cannot; binding freely keeps every T step's rise.
///
/// An unfolded S action keeps none: its raw reads hold the existence of each entity it binds
/// (`raw_reads`, and for a `Triggered` param that of its mark, through `existences`), so that
/// entity is no stub. One that does keep one, or a folded S action's T param outside its
/// fold, is left as it is and returned as the check's fault, in every build. Returns how many
/// params it bound, and that fault.
fn bind_stub_params(
    system: &mut TransitionSystem,
    map: &Remap,
    acts: &[Option<ActId>],
    width: usize,
    s_actions: &BTreeSet<ActId>,
    folds: &[Fold],
) -> StubCheck {
    let mut out = StubCheck::default();
    for (k, act) in acts.iter().enumerate() {
        let Some(act) = *act else { continue };
        let fold = folds.iter().find(|f| f.act == act);
        let local = |q: u8| match &map.params[act.index()] {
            Some(m) => m[usize::from(q)],
            None => Some(q),
        };
        let folded = |p: usize| {
            fold.is_some_and(|f| {
                f.t_params
                    .iter()
                    .any(|(q, _)| local(*q).map(usize::from) == Some(p))
            })
        };
        for p in 0..system.actions[k].params.len() {
            let param = &system.actions[k].params[p];
            let on_stub = system.entities[param.entity.index()].exists.index() >= width;
            if !on_stub || param.role.binds_exists() != Some(Value::PRESENT) {
                continue;
            }
            if s_actions.contains(&act) && !folded(p) {
                out.fault.get_or_insert_with(|| StubParam {
                    action: system.actions[k].name.clone(),
                    param: p,
                    unfolded: true,
                });
                continue;
            }
            system.actions[k].params[p].role = ParamRole::Earlier { step: 0 };
            out.rebound += 1;
        }
    }
    out
}

/// The first param of `system`'s actions on a stub (an entity whose existence is not among
/// its first `width` vars) whose role needs a present row of it.
fn stub_param(system: &TransitionSystem, width: usize) -> Option<StubParam> {
    system.actions.iter().find_map(|a| {
        let param = a.params.iter().position(|p| {
            system.entities[p.entity.index()].exists.index() >= width
                && p.role.binds_exists() == Some(Value::PRESENT)
        })?;
        Some(StubParam {
            action: a.name.clone(),
            param,
            unfolded: false,
        })
    })
}

/// Param `p` of a fold's cone action, in the extended sub-lifecycle's numbering.
fn renumber(map: &Remap, fold: &Fold, p: u8) -> u8 {
    match &map.params[fold.act.index()] {
        Some(m) => m[usize::from(p)].expect("an S param is kept"),
        None => p,
    }
}

/// A new bit var over `entity`, false at first. A bit is no run mark, but it is encoded as
/// one: its var is a `VarKind::Run` of a use case named `in_<name>`, a guard reads it as a
/// `GuardWhy::Run` at step 0, a fold binds its row as `ParamRole::Earlier { step: 0 }`, and
/// its flips are `ActionOrigin::Clock` actions. Those give a bit the binding and the printing
/// it needs; a reader of any of the three may meet a bit.
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
