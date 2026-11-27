//! Folds: an S action's reads of T as one input bit, and the node folds of a guard.
use crate::lifecycle::narrow::id8;
use std::collections::BTreeSet;

use crate::lifecycle::cone::{named_params, VarSet};
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::{ActId, At, CmpOp, Effect, EntId, ParamRole, Pred, Rhs, Term, VarId};
use crate::lifecycle::TransitionSystem;

#[cfg(any(debug_assertions, feature = "shadow"))]
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
    pub(super) fn holds(&self, ts: &TransitionSystem, s: &StateSlice, c: u8) -> bool {
        let held = self.holds_fast(ts, s, c);
        #[cfg(any(debug_assertions, feature = "shadow"))]
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
            (None, None) => id8(a
                .params
                .iter()
                .position(|p| p.role.is_written())
                .expect("an S action writes a row")),
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
