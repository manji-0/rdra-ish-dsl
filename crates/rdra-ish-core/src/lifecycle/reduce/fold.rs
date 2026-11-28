//! Folds: an S action's reads of T as one input bit, and the node folds of a guard.
use crate::lifecycle::narrow::id8;
use std::collections::BTreeSet;

use crate::lifecycle::cone::{named_params, VarSet};
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::{ActId, At, CmpOp, Effect, EntId, ParamRole, Pred, Rhs, Term, VarId};
use crate::lifecycle::TransitionSystem;

/// The twin, left out under Kani: the harnesses below hold `holds` to their own spec, and a
/// planted bug must fail their assertion, not the twin's.
#[cfg(all(any(debug_assertions, feature = "shadow"), not(kani)))]
mod reference;

/// An S action's reads of T folded into one input bit (#112): the action keeps its S params
/// and guards and, in the extended sub-lifecycle, reads one bit per slot of its key in place
/// of its guards on T. For a `Param` key the bit holds when, for some binding of the other T
/// params, every folded guard holds, each T param whose binding needs a present row on one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    /// The cone's action.
    pub(super) act: ActId,
    pub(super) key: FoldKey,
    /// The T params, each with whether its row must be present.
    pub(super) t_params: Vec<(u8, bool)>,
    /// The folded guards, by index in the action.
    pub(super) guards: Vec<usize>,
}

/// What indexes a fold's bit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoldKey {
    /// A param of the action: the one T param a guard links to an S row or an effect
    /// references, or the one S param the folded guards name, or the acting row.
    Param(u8),
    /// A row of a T entity that a quantifier inside guard `guard` binds (the node fold): the
    /// node is a quantifier over `entity`, or an And/Or of such quantifiers, whose bodies
    /// read T except link atoms `link_var[s_param] = Index(row)`; it stands in the guard
    /// under And/Or alone, possibly more than once (the same node each time), so the
    /// quantifier can be pulled out to the action. The bit at slot `c` holds when row `c` of
    /// `entity` is present and the body holds for it; the extended sub-lifecycle binds the
    /// row as a param `q` of any slot and reads the node as `link_var[s_param] = q ∧ bit[q]`
    /// where the node stood.
    Linked {
        guard: usize,
        entity: EntId,
        link_var: VarId,
        s_param: u8,
        /// The folded node's body, with its quantifier over `entity` stripped and its link
        /// atoms replaced by `True`, read with the key slot as `Bound(0)`.
        body: Pred,
    },
}

impl Fold {
    pub fn act(&self) -> ActId {
        self.act
    }

    pub fn key(&self) -> &FoldKey {
        &self.key
    }

    /// The T params and whether each must be on a present row.
    pub fn t_params(&self) -> &[(u8, bool)] {
        &self.t_params
    }

    /// The folded guards, by index in the action.
    pub fn guards(&self) -> &[usize] {
        &self.guards
    }

    /// The entity whose slots index the bit: the key param's, or the node's.
    pub fn key_entity(&self, ts: &TransitionSystem) -> EntId {
        match &self.key {
            FoldKey::Param(key) => ts.actions[self.act.index()].params[usize::from(*key)].entity,
            FoldKey::Linked { entity, .. } => *entity,
        }
    }

    /// Whether `restrict_to(vars)` keeps every item of the cone the extended sub-lifecycle
    /// reads the bit through: for a `Param` key, the key param (`keeps_param`); for a linked
    /// fold, the folded node's entity, its link var, and the S param the link names.
    pub(super) fn key_kept(&self, ts: &TransitionSystem, vars: &VarSet) -> bool {
        let params = &ts.actions[self.act.index()].params;
        match &self.key {
            FoldKey::Param(key) => ts.keeps_param(vars, &params[usize::from(*key)]),
            FoldKey::Linked {
                entity,
                link_var,
                s_param,
                ..
            } => {
                ts.keeps_entity(vars, *entity)
                    && vars.contains(link_var)
                    && ts.keeps_param(vars, &params[usize::from(*s_param)])
            }
        }
    }

    /// The vars `holds` reads: a step that writes none of them leaves the bit as it was.
    pub(super) fn reads(&self, ts: &TransitionSystem) -> VarSet {
        let a = &ts.actions[self.act.index()];
        let mut reads = VarSet::new();
        match &self.key {
            FoldKey::Linked { entity, body, .. } => {
                reads.insert(ts.entity(*entity).exists);
                ts.pred_reads(body, &mut reads);
            }
            FoldKey::Param(_) => {
                for (p, _) in &self.t_params {
                    reads.insert(ts.entity(a.params[usize::from(*p)].entity).exists);
                }
                for g in &self.guards {
                    ts.pred_reads(&a.guards[*g].pred, &mut reads);
                }
            }
        }
        reads
    }

    /// Whether the bit holds for slot `c` of the key in `s`. A build with debug assertions or
    /// the `shadow` feature checks the answer against the reference twin
    /// (`fold/reference.rs`), which computes it with the allocations this one does without.
    pub(crate) fn holds(&self, ts: &TransitionSystem, s: &StateSlice, c: u8) -> bool {
        let held = self.holds_fast(ts, s, c);
        #[cfg(all(any(debug_assertions, feature = "shadow"), not(kani)))]
        reference::check_holds(self, ts, s, c, held);
        held
    }

    /// `holds` without an allocation per call: the params in a buffer on the stack for up to
    /// `MAX` of them, and each binding of the free T params decoded from its number by
    /// walking `t_params`, with the radix of each read off its entity's slots.
    fn holds_fast(&self, ts: &TransitionSystem, s: &StateSlice, c: u8) -> bool {
        const MAX: usize = 16;
        let a = &ts.actions[self.act.index()];
        let mut stack = [0u8; MAX];
        let mut heap = Vec::new();
        let params: &mut [u8] = match a.params.len() {
            n if n <= MAX => &mut stack[..n],
            n => {
                heap.resize(n, 0);
                &mut heap
            }
        };
        let key = match &self.key {
            FoldKey::Param(key) => *key,
            FoldKey::Linked { entity, body, .. } => {
                if !s.is_present(ts, *entity, usize::from(c)) {
                    return false;
                }
                let mut env = crate::lifecycle::expr::Env::new(params);
                env.bound.push(c);
                return ts.eval(body, s, &mut env);
            }
        };
        params[usize::from(key)] = c;
        let radix = |p: u8| ts.slots(a.params[usize::from(p)].entity);
        let free = || self.t_params.iter().map(|(p, _)| *p).filter(|p| *p != key);
        let bindings: usize = free().map(radix).product();
        (0..bindings).any(|binding| {
            let mut rest = binding;
            for p in free() {
                let n = radix(p);
                params[usize::from(p)] = id8(rest % n);
                rest /= n;
            }
            let present = self.t_params.iter().all(|(p, must)| {
                !must
                    || s.is_present(
                        ts,
                        a.params[usize::from(*p)].entity,
                        usize::from(params[usize::from(*p)]),
                    )
            });
            let mut env = crate::lifecycle::expr::Env::new(params);
            present
                && self
                    .guards
                    .iter()
                    .all(|g| ts.eval(&a.guards[*g].pred, s, &mut env))
        })
    }
}

/// A foldable node of a guard: the T entity its quantifier binds, the link `(S var, param)`
/// that ties the bound row to an S row, and the body with the quantifier stripped and the
/// links replaced by `True`.
pub(super) struct Node {
    pub(super) entity: EntId,
    pub(super) link: (VarId, u8),
    pub(super) body: Pred,
    /// Conjuncts of the quantifier's body that read S alone and name no bound row: they
    /// leave the quantifier and stand beside the link in the stand-in.
    pub(super) outside: Vec<Pred>,
}

impl Node {
    pub(super) fn same(&self, other: &Node) -> bool {
        self.entity == other.entity
            && self.link == other.link
            && self.body == other.body
            && self.outside == other.outside
    }
}

/// Whether `pred` reads a slot or index of a bound row.
pub(super) fn names_bound(pred: &Pred) -> bool {
    pred.terms().any(|t| {
        matches!(
            t,
            Term::Slot {
                at: At::Bound(_),
                ..
            } | Term::Index(At::Bound(_))
        )
    })
}

impl TransitionSystem {
    /// An S action's reads of T folded into a bit, when they can be (#112): at most one T
    /// param (a row of a T entity it binds) is referenced by an effect, and that one keys the
    /// bit; every guard that names a T param reads T only, or is a link guard that compares
    /// an S var with one T param's slot (that param is kept, bound to any slot, and keys the
    /// bit); the folded guards name at most one S param's slot, which keys the bit
    /// otherwise; no folded guard reads the successor. An action with no T param folds a
    /// node (`node_fold_of`).
    pub(super) fn fold_of(&self, id: ActId, t: &VarSet) -> Option<Fold> {
        let a = &self.actions[id.index()];
        let t_params: Vec<(u8, bool)> = a
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| t.contains(&self.entity(p.entity).exists))
            .map(|(i, p)| {
                let present = matches!(
                    p.role,
                    ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered
                );
                (id8(i), present)
            })
            .collect();
        if t_params.is_empty() {
            return self.node_fold_of(id, t);
        }
        let is_t = |p: &u8| t_params.iter().any(|(q, _)| q == p);
        // A T param an effect references (an owner set to it) stays a param of the extended
        // sub-lifecycle, bound to any slot, so it must be the bit's key.
        let mut touched_t: BTreeSet<u8> = BTreeSet::new();
        for eff in &a.effects {
            let touched: Vec<u8> = match eff {
                Effect::Create { param } | Effect::Delete { param } => vec![*param],
                Effect::Assign { param, rhs, .. } => match rhs {
                    Rhs::Param(q) => vec![*param, *q],
                    Rhs::Lit(_) => vec![*param],
                },
                Effect::Transition { param, .. } | Effect::Choose { param, .. } => vec![*param],
                Effect::Tick => Vec::new(),
            };
            touched_t.extend(touched.iter().filter(|p| is_t(p)));
        }
        if touched_t.len() > 1 {
            return None;
        }
        let mut guards = Vec::new();
        let mut linked: BTreeSet<u8> = BTreeSet::new();
        let mut s_keys: BTreeSet<u8> = BTreeSet::new();
        for (gi, g) in a.guards.iter().enumerate() {
            let named = named_params(&g.pred);
            if !named.iter().any(is_t) {
                continue;
            }
            if g.reads_successor() || g.copy_target().is_some() {
                return None;
            }
            let mut r = VarSet::new();
            self.pred_reads(&g.pred, &mut r);
            if r.is_subset(t) {
                s_keys.extend(named.iter().filter(|p| !is_t(p)));
                guards.push(gi);
                continue;
            }
            let link = match &g.pred {
                Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(k),
                    },
                    CmpOp::Eq,
                    Term::Index(At::Param(p)),
                )
                | Pred::Cmp(
                    Term::Index(At::Param(p)),
                    CmpOp::Eq,
                    Term::Slot {
                        var,
                        at: At::Param(k),
                    },
                ) if !t.contains(var) && !is_t(k) && is_t(p) => Some(*p),
                _ => None,
            };
            linked.insert(link?);
        }
        if linked.len() > 1 || s_keys.len() > 1 || (!linked.is_empty() && !s_keys.is_empty()) {
            return None;
        }
        if let Some(p) = touched_t.first() {
            if !s_keys.is_empty() || linked.iter().any(|l| l != p) {
                return None;
            }
            linked.insert(*p);
        }
        let key = match (linked.first(), s_keys.first()) {
            (Some(p), _) => *p,
            (None, Some(k)) => *k,
            (None, None) => id8(a.params.iter().position(|p| p.role.is_written())?),
        };
        // A row the step creates has no slot before the step, so no bit can be keyed by it.
        if a.params[usize::from(key)].role == ParamRole::New {
            return None;
        }
        Some(Fold {
            act: id,
            key: FoldKey::Param(key),
            t_params,
            guards,
        })
    }

    /// The bit's value in the cone's initial states, when every initial state and every slot
    /// of the key agree; `None` otherwise, and the read stays raw (a bit var has one base).
    pub(super) fn fold_initial(&self, fold: &Fold) -> Option<bool> {
        let initial = self.initial_states();
        let mut vals = initial.iter().flat_map(|st| {
            (0..id8(self.slots(fold.key_entity(self)))).map(move |c| fold.holds(self, st, c))
        });
        let first = vals.next()?;
        vals.all(|v| v == first).then_some(first)
    }

    /// The node fold of an S action with no T params (#112): exactly one guard reads T,
    /// through one node (`FoldKey::Linked`) under And/Or alone.
    pub(super) fn node_fold_of(&self, id: ActId, t: &VarSet) -> Option<Fold> {
        let a = &self.actions[id.index()];
        let mut found: Option<(usize, Node)> = None;
        for (gi, g) in a.guards.iter().enumerate() {
            let mut r = VarSet::new();
            self.pred_reads(&g.pred, &mut r);
            if r.is_disjoint(t) {
                continue;
            }
            if found.is_some() || g.copy_target().is_some() {
                return None;
            }
            found = Some((gi, self.node_in(&g.pred, t)?));
        }
        let (guard, node) = found?;
        Some(Fold {
            act: id,
            key: FoldKey::Linked {
                guard,
                entity: node.entity,
                link_var: node.link.0,
                s_param: node.link.1,
                body: node.body,
            },
            t_params: Vec::new(),
            guards: vec![guard],
        })
    }

    /// The one node `pred` reads T through: every part of `pred` that reads T, under And/Or
    /// alone, is that node. `None` when T is read elsewhere or through two nodes.
    pub(super) fn node_in(&self, pred: &Pred, t: &VarSet) -> Option<Node> {
        let mut found: Option<Node> = None;
        self.walk_nodes(pred, t, &mut |node| match &found {
            Some(f) if !f.same(&node) => false,
            Some(_) => true,
            None => {
                found = Some(node);
                true
            }
        })?;
        found
    }

    /// Calls `at` with each node of `pred` under And/Or; `None` when a part that reads T is no
    /// node, or `at` says no.
    pub(super) fn walk_nodes(
        &self,
        pred: &Pred,
        t: &VarSet,
        at: &mut dyn FnMut(Node) -> bool,
    ) -> Option<()> {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        if r.is_disjoint(t) {
            return Some(());
        }
        if let Some(node) = self.as_node(pred, t) {
            return at(node).then_some(());
        }
        match pred {
            Pred::And(parts) | Pred::Or(parts) => {
                for part in parts {
                    self.walk_nodes(part, t, at)?;
                }
                Some(())
            }
            _ => None,
        }
    }

    /// `pred` with each occurrence of `node` replaced by `stand_in` and every other part
    /// mapped by `rest`, which the parts under And/Or that read no T go through whole.
    pub(super) fn substitute_node(
        &self,
        pred: &Pred,
        t: &VarSet,
        node: &Node,
        stand_in: &Pred,
        rest: &dyn Fn(&Pred) -> Pred,
    ) -> Pred {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        if r.is_disjoint(t) {
            return rest(pred);
        }
        if let Some(found) = self.as_node(pred, t) {
            assert!(found.same(node), "the guard's nodes agree");
            return stand_in.clone();
        }
        let parts = |xs: &[Pred]| -> Vec<Pred> {
            xs.iter()
                .map(|x| self.substitute_node(x, t, node, stand_in, rest))
                .collect()
        };
        match pred {
            Pred::And(xs) => Pred::And(parts(xs)),
            Pred::Or(xs) => Pred::Or(parts(xs)),
            _ => unreachable!("a node fold's guard reads T through nodes under And/Or alone"),
        }
    }

    /// `pred` as a node: a quantifier over a T entity whose body reads T except link atoms
    /// `v[p] = Index(Bound(0))` (v an S var, p a param), all on one `(v, p)`, and no param
    /// or successor otherwise; or an And/Or of such quantifiers over one entity and link.
    pub(super) fn as_node(&self, pred: &Pred, t: &VarSet) -> Option<Node> {
        match pred {
            Pred::Exists { entity, body } if t.contains(&self.entity(*entity).exists) => {
                let mut link: Option<(VarId, u8)> = None;
                let mut outside = Vec::new();
                let mut inside = Vec::new();
                let parts: Vec<&Pred> = match &**body {
                    Pred::And(parts) => parts.iter().collect(),
                    other => vec![other],
                };
                for part in parts {
                    let mut r = VarSet::new();
                    self.pred_reads(part, &mut r);
                    if r.is_disjoint(t) && !names_bound(part) {
                        outside.push(part.clone());
                    } else {
                        inside.push(self.strip_links(part, t, &mut link, true)?);
                    }
                }
                let body = match inside.len() {
                    1 => inside.pop().expect("one part"),
                    _ => Pred::And(inside),
                };
                Some(Node {
                    entity: *entity,
                    link: link?,
                    body,
                    outside,
                })
            }
            Pred::And(parts) | Pred::Or(parts) if !parts.is_empty() => {
                let nodes: Vec<Node> = parts
                    .iter()
                    .map(|p| self.as_node(p, t))
                    .collect::<Option<_>>()?;
                let first = &nodes[0];
                if nodes.iter().any(|n| {
                    n.entity != first.entity || n.link != first.link || !n.outside.is_empty()
                }) {
                    return None;
                }
                let bodies: Vec<Pred> = nodes.iter().map(|n| n.body.clone()).collect();
                Some(Node {
                    entity: first.entity,
                    link: first.link,
                    body: match pred {
                        Pred::And(_) => Pred::And(bodies),
                        _ => Pred::Or(bodies),
                    },
                    outside: Vec::new(),
                })
            }
            _ => None,
        }
    }

    /// A node's body with its link atoms replaced by `True`, recording the one link; `None`
    /// when an atom reads S, a param, or the successor, when links differ, or when a link
    /// stands anywhere but as a conjunct of the body (under Or, Not, or a universal the
    /// stand-in `link ∧ bit` would not be the node). The node is under no quantifier, so
    /// its row is `Bound(0)` at every depth of its body.
    pub(super) fn strip_links(
        &self,
        pred: &Pred,
        t: &VarSet,
        link: &mut Option<(VarId, u8)>,
        conjunctive: bool,
    ) -> Option<Pred> {
        let is_link = |term: &Term, other: &Term| match (term, other) {
            (
                Term::Slot {
                    var,
                    at: At::Param(p),
                },
                Term::Index(At::Bound(0)),
            ) if !t.contains(var) => Some((*var, *p)),
            _ => None,
        };
        match pred {
            Pred::Cmp(x, CmpOp::Eq, y) => {
                if let Some(found) = is_link(x, y).or_else(|| is_link(y, x)) {
                    if !conjunctive || link.get_or_insert(found) != &found {
                        return None;
                    }
                    return Some(Pred::True);
                }
                self.t_atom(pred, t)
            }
            Pred::True | Pred::False | Pred::Cmp(..) | Pred::In(..) => self.t_atom(pred, t),
            Pred::Not(x) => Some(Pred::Not(Box::new(self.strip_links(x, t, link, false)?))),
            Pred::And(xs) => Some(Pred::And(
                xs.iter()
                    .map(|x| self.strip_links(x, t, link, conjunctive))
                    .collect::<Option<_>>()?,
            )),
            Pred::Or(xs) => Some(Pred::Or(
                xs.iter()
                    .map(|x| self.strip_links(x, t, link, false))
                    .collect::<Option<_>>()?,
            )),
            // A node pulls an existential out to a param; a count is no existential, so a
            // guard that reads T through one is never node-folded (docs/design/counting-rules.md).
            Pred::Count { .. } => None,
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                if !t.contains(&self.entity(*entity).exists) {
                    return None;
                }
                // Under an existential the link is still a conjunct of the node (`∃b1 (L ∧
                // T)` is `L ∧ ∃b1 T` when `L` names no `b1`); under a universal it is not.
                let body = Box::new(self.strip_links(
                    body,
                    t,
                    link,
                    conjunctive && matches!(pred, Pred::Exists { .. }),
                )?);
                Some(match pred {
                    Pred::Forall { .. } => Pred::Forall {
                        entity: *entity,
                        body,
                    },
                    _ => Pred::Exists {
                        entity: *entity,
                        body,
                    },
                })
            }
        }
    }

    /// An atom that reads T alone and names no param or successor, as is.
    pub(super) fn t_atom(&self, pred: &Pred, t: &VarSet) -> Option<Pred> {
        let mut r = VarSet::new();
        self.pred_reads(pred, &mut r);
        (r.is_subset(t) && named_params(pred).is_empty()).then(|| pred.clone())
    }

    /// What an S action reads outside its fold: the existence of its other `Existing` and
    /// `Parent` rows, its unfolded guards, its copies, and its transition sources.
    pub(super) fn raw_reads(&self, id: ActId, fold: Option<&Fold>) -> VarSet {
        let a = &self.actions[id.index()];
        let folded_param = |p: u8| fold.is_some_and(|f| f.t_params.iter().any(|(q, _)| *q == p));
        let folded_guard = |g: usize| fold.is_some_and(|f| f.guards.contains(&g));
        let mut out = VarSet::new();
        for (i, p) in a.params.iter().enumerate() {
            if matches!(p.role, ParamRole::Existing | ParamRole::Parent { .. })
                && !folded_param(id8(i))
            {
                out.insert(self.entity(p.entity).exists);
            }
        }
        for (gi, g) in a.guards.iter().enumerate() {
            if !folded_guard(gi) {
                self.pred_reads(&g.pred, &mut out);
            }
        }
        for eff in &a.effects {
            match eff {
                Effect::Transition { var, .. } => {
                    out.insert(*var);
                }
                Effect::Tick => out.extend(self.now),
                _ => {}
            }
        }
        out
    }
}

/// Step (a) of Lemma 6 (docs/theory/cones.md) for a `Param` fold, on a small system: the bit
/// holds exactly when some binding of the T params, the key at `c` when it is one of them,
/// puts each param on a candidate slot of its role and makes every folded guard true. The spec
/// reads the state's bytes and evaluates the folded guards itself, with no call into the
/// evaluator or the layout. The Kani harnesses prove it for every pick; the tests check that
/// `fold_of` gives the fold the harnesses take, and the spec against `holds` without Kani.
#[cfg(any(kani, test))]
mod proofs {
    use std::num::NonZeroU8;

    use super::*;
    use crate::lifecycle::types::{
        Action, ActionOrigin, Birth, Bounds, Domain, EntityDecl, Guard, GuardWhy, ModelRef, Param,
        Site, Start, Value, VarDecl, VarKind,
    };
    use crate::model::{EntityKey, FkId};

    /// The most slots of a T entity, and the slots of the S entity.
    const T_SLOTS: u8 = 3;
    const S_SLOTS: u8 = 2;
    /// Each T column is an `Enum` of this many values.
    const VALUES: u8 = 2;
    /// The state's width at the most slots: two vars per entity.
    const WIDTH: usize = 2 * (2 * T_SLOTS as usize + S_SLOTS as usize);

    /// The entities: the S entity, whose row is the action's own, and two T entities.
    const S: EntId = EntId(0);
    const T1: EntId = EntId(1);
    const T2: EntId = EntId(2);
    /// The vars, in layout order: each T entity's existence and one column, the S entity's
    /// existence, and its owner of a `T1` row, which the link guard reads.
    const T1_COL: VarId = VarId(1);
    const T2_COL: VarId = VarId(3);
    const S_OWNER: VarId = VarId(5);

    #[cfg(test)]
    const OPS: [CmpOp; 6] = [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Lt,
        CmpOp::Le,
        CmpOp::Gt,
        CmpOp::Ge,
    ];

    /// The roles `fold_of` reads of a T param: the three that need a present row, and
    /// `Earlier`, which binds any slot. (A T param of an `S_a` action is never `New` or
    /// `Earlier`, cones.md's folded table; `Earlier` is kept for the `false` mark.)
    fn roles() -> [ParamRole; 4] {
        [
            ParamRole::Existing,
            ParamRole::Parent { key: FkId::at(0) },
            ParamRole::Triggered,
            ParamRole::Earlier { step: 0 },
        ]
    }

    /// What varies: the slots of each T entity, the role of each T param, the values each of
    /// the first two folded guards admits (any set of the column's values: `VALUES`, no value
    /// of the column, leaves a place empty), and the third guard's operator.
    #[derive(Clone, Copy, Debug)]
    struct Pick {
        n1: u8,
        n2: u8,
        roles: [ParamRole; 2],
        sets: [[u8; VALUES as usize]; 2],
        op: CmpOp,
    }

    /// The action `A(s: S Existing, t1: T1, t2: T2)`, with the folded guards
    /// `t1.c in sets[0]`, `t2.c in sets[1]` and `t1.c op t2.c`, and, when
    /// `linked`, the link `s.owner = t1`, which makes `t1` the key. Without it the key is `s`,
    /// the acting row, and both T params are free.
    ///
    /// The system is built whole, its layout written out rather than computed (the tests check
    /// it is `layout`'s), so that CBMC unwinds no drop of the empty system's lists.
    fn system(pick: &Pick, linked: bool) -> TransitionSystem {
        let entity = |id: &str, slots: u8, exists: u16, columns: Vec<VarId>| EntityDecl {
            key: EntityKey::default(),
            id: id.to_owned(),
            label: String::new(),
            birth: Birth::Created,
            slots: NonZeroU8::new(slots).expect("an entity has a slot"),
            exists: VarId(exists),
            columns,
            owners: Vec::new(),
        };
        let entities = vec![
            entity("S", S_SLOTS, 4, vec![S_OWNER]),
            entity("T1", pick.n1, 0, vec![T1_COL]),
            entity("T2", pick.n2, 2, vec![T2_COL]),
        ];
        let var = |kind: VarKind, domain: Domain| VarDecl {
            kind,
            domain,
            base: Value(0),
            start: Start::Base,
            site: Site::bare(ModelRef::Synthesized),
            ends: Vec::new(),
        };
        let column = |entity: EntId| VarKind::Column {
            entity,
            column: String::new(),
            lifecycle: false,
            variation: false,
        };
        // `compare` never reads an `Enum`'s names, so the columns have none.
        let vars = vec![
            var(VarKind::Exists(T1), Domain::Existence),
            var(column(T1), Domain::Enum(Vec::new())),
            var(VarKind::Exists(T2), Domain::Existence),
            var(column(T2), Domain::Enum(Vec::new())),
            var(VarKind::Exists(S), Domain::Existence),
            var(
                VarKind::Owner {
                    child: S,
                    parent: T1,
                    fk: FkId::at(0),
                    role: None,
                },
                Domain::Instance {
                    parent: T1,
                    optional: true,
                },
            ),
        ];
        let (n1, n2, ns) = (
            usize::from(pick.n1),
            usize::from(pick.n2),
            usize::from(S_SLOTS),
        );
        let offsets = vec![
            0,
            n1,
            2 * n1,
            2 * n1 + n2,
            2 * (n1 + n2),
            2 * (n1 + n2) + ns,
        ];
        let width = 2 * (n1 + n2 + ns);
        let slot = |var: VarId, p: u8| Term::Slot {
            var,
            at: At::Param(p),
        };
        let guard = |pred: Pred, why: GuardWhy| Guard {
            pred,
            why,
            site: Site::bare(ModelRef::Synthesized),
        };
        let set = |k: usize| pick.sets[k].iter().map(|v| Term::Lit(Value(*v))).collect();
        let mut guards = Vec::with_capacity(4);
        guards.push(guard(
            Pred::In(slot(T1_COL, 1), set(0)),
            GuardWhy::TransitionFrom,
        ));
        guards.push(guard(
            Pred::In(slot(T2_COL, 2), set(1)),
            GuardWhy::TransitionFrom,
        ));
        guards.push(guard(
            Pred::Cmp(slot(T1_COL, 1), pick.op, slot(T2_COL, 2)),
            GuardWhy::TransitionFrom,
        ));
        if linked {
            guards.push(guard(
                Pred::Cmp(slot(S_OWNER, 0), CmpOp::Eq, Term::Index(At::Param(1))),
                GuardWhy::Link,
            ));
        }
        let param = |entity: EntId, role: ParamRole| Param {
            entity,
            role,
            via_api: None,
        };
        let actions = vec![Action {
            name: "A".to_owned(),
            source: ActId(0),
            origin: ActionOrigin::Clock,
            site: Site::bare(ModelRef::Synthesized),
            params: vec![
                param(S, ParamRole::Existing),
                param(T1, pick.roles[0]),
                param(T2, pick.roles[1]),
            ],
            guards,
            effects: Vec::new(),
        }];
        TransitionSystem {
            bounds: Bounds::VERIFY,
            entities,
            vars,
            actions,
            invariants: Vec::new(),
            after: Vec::new(),
            properties: Vec::new(),
            reach: Vec::new(),
            now: None,
            offsets,
            width,
            api_ids: std::collections::BTreeMap::new(),
            reduction: None,
            plans: crate::lifecycle::explore::Plans::default(),
        }
    }

    /// The fold `fold_of` gives the action (`fold_of_folds_the_harness_action`): keyed by
    /// `s`, or by `t1` when linked, with both T params, each marked by whether its role needs
    /// a row, and the three guards on T.
    fn fold(pick: &Pick, linked: bool) -> Fold {
        Fold {
            act: ActId(0),
            key: FoldKey::Param(u8::from(linked)),
            t_params: vec![(1, needs_row(pick.roles[0])), (2, needs_row(pick.roles[1]))],
            guards: vec![0, 1, 2],
        }
    }

    /// The T side of the cone: both T entities.
    #[cfg(test)]
    fn t_vars() -> VarSet {
        (0..4).map(VarId).collect()
    }

    /// Whether a param of this role binds only a present row (`candidate_slots`).
    fn needs_row(role: ParamRole) -> bool {
        matches!(
            role,
            ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered
        )
    }

    fn cmp(a: u8, op: CmpOp, b: u8) -> bool {
        match op {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
        }
    }

    /// The spec's reading of a state: each T entity's existence and column bytes, in the
    /// layout order of `system`.
    struct Rows<'a> {
        exists1: &'a [u8],
        col1: &'a [u8],
        exists2: &'a [u8],
        col2: &'a [u8],
    }

    impl<'a> Rows<'a> {
        fn of(pick: &Pick, bytes: &'a [u8]) -> Self {
            let (n1, n2) = (usize::from(pick.n1), usize::from(pick.n2));
            Rows {
                exists1: &bytes[..n1],
                col1: &bytes[n1..2 * n1],
                exists2: &bytes[2 * n1..2 * n1 + n2],
                col2: &bytes[2 * n1 + n2..2 * (n1 + n2)],
            }
        }

        /// Whether `t1` at `i` and `t2` at `j` are candidate slots of their roles and make
        /// every folded guard true.
        fn binds(&self, pick: &Pick, i: usize, j: usize) -> bool {
            (!needs_row(pick.roles[0]) || self.exists1[i] == Value::PRESENT.0)
                && (!needs_row(pick.roles[1]) || self.exists2[j] == Value::PRESENT.0)
                && pick.sets[0].contains(&self.col1[i])
                && pick.sets[1].contains(&self.col2[j])
                && cmp(self.col1[i], pick.op, self.col2[j])
        }

        /// The bit at slot `c` of the key: some binding of the T params, with `t1` at `c`
        /// when linked, binds.
        fn bit(&self, pick: &Pick, linked: bool, c: usize) -> bool {
            let mut found = false;
            for i in 0..usize::from(pick.n1) {
                for j in 0..usize::from(pick.n2) {
                    found |= (!linked || i == c) && self.binds(pick, i, j);
                }
            }
            found
        }
    }

    #[cfg(kani)]
    mod harness {
        use super::*;

        fn any_of<T: Copy>(xs: &[T]) -> T {
            let k: usize = kani::any();
            kani::assume(k < xs.len());
            xs[k]
        }

        /// Any pick with up to `most` slots per T entity.
        fn any_pick(most: u8) -> Pick {
            let n1: u8 = kani::any();
            let n2: u8 = kani::any();
            kani::assume((1..=most).contains(&n1) && (1..=most).contains(&n2));
            let sets: [[u8; VALUES as usize]; 2] = kani::any();
            for set in &sets {
                kani::assume(set.iter().all(|v| *v <= VALUES));
            }
            let roles = roles();
            Pick {
                n1,
                n2,
                roles: [any_of(&roles), any_of(&roles)],
                sets,
                // `expr.rs`'s harnesses prove `compare` for every operator.
                op: CmpOp::Le,
            }
        }

        /// `holds` of the action's fold at any slot `c` of its key, on any state whose columns
        /// hold values of their domain, is the spec's bit.
        ///
        /// The bound is one past the most bindings `holds` walks. The state's bytes come from
        /// one `u128` (`WIDTH` is 16), with no loop.
        fn check(linked: bool, most: u8) {
            let pick = any_pick(most);
            let ts = system(&pick, linked);
            let fold = fold(&pick, linked);
            let bytes: [u8; WIDTH] = kani::any::<u128>().to_le_bytes();
            let state = &bytes[..ts.width];
            let rows = Rows::of(&pick, state);
            kani::assume(rows.col1.iter().all(|v| *v < VALUES));
            kani::assume(rows.col2.iter().all(|v| *v < VALUES));
            let c: u8 = kani::any();
            kani::assume(usize::from(c) < ts.slots(fold.key_entity(&ts)));

            let held = fold.holds(&ts, StateSlice::new(state), c);
            assert_eq!(held, rows.bit(&pick, linked, usize::from(c)));

            // A true bit whose every binding is past slot 0 of each free T param, at equal
            // slot counts of 2 or more (where a wrong radix shows); and one bound to an absent
            // row by a param whose role needs none.
            let (n1, n2) = (usize::from(pick.n1), usize::from(pick.n2));
            let past_0 = (0..n1).all(|i| !rows.binds(&pick, i, 0))
                && (linked || (0..n2).all(|j| !rows.binds(&pick, 0, j)));
            kani::cover!(held);
            kani::cover!(!held);
            kani::cover!(held && past_0 && n1 == n2 && n1 >= 2);
            kani::cover!(
                held && !needs_row(pick.roles[1])
                    && rows.exists2.iter().all(|e| *e != Value::PRESENT.0)
            );
            std::mem::forget(fold);
            std::mem::forget(ts);
        }

        /// The key is the acting S row, and both T params range over entities of up to 2
        /// slots: up to 4 bindings. At 3 slots CBMC's symbolic execution alone passes ten
        /// minutes.
        #[kani::proof]
        #[kani::unwind(5)]
        fn a_fold_keyed_by_its_s_row_holds_where_some_t_binding_does() {
            check(false, 2);
        }

        /// The key is the linked T param, fixed at `c`, and the other T param ranges: up to 3
        /// bindings.
        #[kani::proof]
        #[kani::unwind(4)]
        fn a_fold_keyed_by_a_linked_t_row_holds_where_some_t_binding_does() {
            check(true, T_SLOTS);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every slot count, pair of roles, and key, at one choice of operators and literals.
        fn picks() -> Vec<(Pick, bool)> {
            let mut out = Vec::new();
            for n1 in 1..=T_SLOTS {
                for n2 in 1..=T_SLOTS {
                    for r1 in roles() {
                        for r2 in roles() {
                            for linked in [false, true] {
                                let pick = Pick {
                                    n1,
                                    n2,
                                    roles: [r1, r2],
                                    sets: [[0, VALUES], [1, 1]],
                                    op: CmpOp::Le,
                                };
                                out.push((pick, linked));
                            }
                        }
                    }
                }
            }
            out
        }

        /// The harnesses take the fold `fold_of` gives, on the layout `layout` gives.
        #[test]
        fn fold_of_folds_the_harness_action() {
            for (pick, linked) in picks() {
                let ts = system(&pick, linked);
                let got = ts.fold_of(ActId(0), &t_vars()).expect("the action folds");
                assert_eq!(got, fold(&pick, linked), "{pick:?}, linked {linked}");
                let mut laid = ts.clone();
                laid.layout();
                assert_eq!((&laid.offsets, laid.width), (&ts.offsets, ts.width));
            }
        }

        /// The spec and `holds` agree on sampled operators, literals and states of every
        /// pick of `picks`, and both values of the bit occur.
        #[test]
        fn the_spec_is_the_bit() {
            let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let small = |r: u64, shift: u32, n: u64| u8::try_from((r >> shift) % n).unwrap();
            let (mut held, mut not_held) = (0, 0);
            for (base, linked) in picks() {
                for _ in 0..200 {
                    let r = next();
                    let pick = Pick {
                        sets: [0, 1].map(|k| [0, 1].map(|v| small(r, 8 * k + 2 * v, 4))),
                        op: OPS[usize::from(small(r, 24, 6))],
                        ..base
                    };
                    let ts = system(&pick, linked);
                    let fold = ts.fold_of(ActId(0), &t_vars()).expect("the action folds");
                    let r = next();
                    let mut bytes = [0u8; WIDTH];
                    for (k, b) in (0u32..).zip(bytes.iter_mut()) {
                        *b = small(r, 2 * k, 4);
                    }
                    let (n1, n2) = (usize::from(pick.n1), usize::from(pick.n2));
                    let columns = (n1..2 * n1).chain(2 * n1 + n2..2 * (n1 + n2));
                    for k in columns {
                        bytes[k] %= VALUES;
                    }
                    let state = &bytes[..ts.width];
                    let rows = Rows::of(&pick, state);
                    for c in 0..ts.slots(fold.key_entity(&ts)) {
                        let got = fold.holds(&ts, StateSlice::new(state), id8(c));
                        let want = rows.bit(&pick, linked, c);
                        assert_eq!(got, want, "{pick:?}, linked {linked}, {state:?}, slot {c}");
                        match got {
                            true => held += 1,
                            false => not_held += 1,
                        }
                    }
                }
            }
            assert!(held > 0 && not_held > 0, "{held} true, {not_held} false");
        }
    }
}
