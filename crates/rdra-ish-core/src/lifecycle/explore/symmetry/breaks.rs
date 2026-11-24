//! What breaks the group: a static scan over every guard, effect, invariant, after-assert,
//! property, premise, and `can_reach` goal of a system, and over the starts of its owners.
//! It never refuses a model. It names each entity whose class must stay where it is, and why
//! (`Rule`); leaving a class out keeps every run sound and loses only reduction.
//!
//! The builder emits none of these today: a guard reads row ids only by `=` and `≠` against
//! owners or other row ids, an effect writes an owner only from a param's slot or `null`,
//! and every choice and transition writes columns and marks. The scan guards against a
//! later builder change, and a hand-built system in the unit tests stands for each rule.

use std::collections::BTreeSet;

use crate::lifecycle::types::{
    At, Birth, CmpOp, Domain, Effect, EntId, Param, ParamRole, Pred, Rhs, Start, Term, Value,
    VarKind,
};
use crate::lifecycle::TransitionSystem;

use super::Rule;

/// A term that names a row of an entity: a row id, or an owner (with whether it is
/// optional, so `null` names no row).
#[derive(Clone, Copy)]
struct Id {
    entity: EntId,
    optional_owner: Option<bool>,
}

struct Scan<'a> {
    ts: &'a TransitionSystem,
    /// Per entity: its class, as `Group::of` couples them.
    class_of: &'a [usize],
    found: BTreeSet<(EntId, Rule)>,
}

/// Each entity whose class the group must leave out, with the rule that says so.
pub(super) fn scan(ts: &TransitionSystem, class_of: &[usize]) -> BTreeSet<(EntId, Rule)> {
    let mut scan = Scan {
        ts,
        class_of,
        found: BTreeSet::new(),
    };
    for action in ts.actions() {
        for guard in &action.guards {
            scan.pred(&guard.pred, &action.params, &mut Vec::new());
        }
        scan.effects(&action.params, &action.effects);
    }
    for inv in ts.invariants() {
        scan.pred(&inv.body, &[], &mut inv.binders.clone());
        for premise in &inv.premises {
            scan.pred(&premise.pred, &[], &mut inv.binders.clone());
        }
    }
    for a in ts.after_asserts() {
        for step in &a.steps {
            let params = &ts.actions()[step.act.index()].params;
            scan.pred(&step.post, params, &mut step.binders.clone());
        }
    }
    for p in ts.properties() {
        for pred in p.preds() {
            scan.pred(pred, &[], &mut p.binders.clone());
        }
        for term in p.formula.columns() {
            scan.lone(term, &[], &p.binders);
        }
    }
    for r in ts.reach_properties() {
        for pred in r.preds() {
            scan.pred(pred, &[], &mut r.binders.clone());
        }
    }
    scan.starts();
    scan.deleted_row_owners();
    scan.found
}

impl Scan<'_> {
    fn leave_out(&mut self, e: EntId, rule: Rule) {
        self.found.insert((e, rule));
    }

    /// The row `term` names, when it names one.
    fn id(&self, term: &Term, params: &[Param], bound: &[EntId]) -> Option<Id> {
        let at_entity = |at: &At| match at {
            At::Param(p) | At::Next(p) => params.get(usize::from(*p)).map(|p| p.entity),
            At::Bound(b) => bound.get(usize::from(*b)).copied(),
        };
        match term {
            Term::Index(at) => Some(Id {
                // A binder the scan cannot place names entity 0, and the comparison leaves
                // that out: an unresolved id is never taken as symmetric.
                entity: at_entity(at).unwrap_or(EntId::at(0)),
                optional_owner: None,
            }),
            Term::Slot { var, .. } => match &self.ts.var(*var).domain {
                Domain::Instance { parent, optional }
                    if matches!(self.ts.var(*var).kind, VarKind::Owner { .. }) =>
                {
                    Some(Id {
                        entity: *parent,
                        optional_owner: Some(*optional),
                    })
                }
                _ => None,
            },
            Term::Lit(_) | Term::Int(_) | Term::Now => None,
        }
    }

    fn same_class(&self, a: EntId, b: EntId) -> bool {
        self.class_of[a.index()] == self.class_of[b.index()]
    }

    /// A term read on its own, as `unchanged` reads its columns: a row id there is no value.
    fn lone(&mut self, term: &Term, params: &[Param], bound: &[EntId]) {
        if let (Term::Index(_), Some(id)) = (term, self.id(term, params, bound)) {
            self.leave_out(id.entity, Rule::MixedIds);
        }
    }

    fn pred(&mut self, pred: &Pred, params: &[Param], bound: &mut Vec<EntId>) {
        match pred {
            Pred::True | Pred::False => {}
            Pred::Cmp(a, op, b) => self.cmp(a, *op, b, params, bound),
            Pred::In(a, values) => {
                for term in std::iter::once(a).chain(values) {
                    if let Some(id) = self.id(term, params, bound) {
                        self.leave_out(id.entity, Rule::OrderedId);
                    }
                }
            }
            Pred::Not(body) => self.pred(body, params, bound),
            Pred::And(parts) | Pred::Or(parts) => {
                for part in parts {
                    self.pred(part, params, bound);
                }
            }
            Pred::Forall { entity, body }
            | Pred::Exists { entity, body }
            | Pred::Count { entity, body, .. } => {
                bound.push(*entity);
                self.pred(body, params, bound);
                bound.pop();
            }
        }
    }

    fn cmp(&mut self, a: &Term, op: CmpOp, b: &Term, params: &[Param], bound: &[EntId]) {
        let (ia, ib) = (self.id(a, params, bound), self.id(b, params, bound));
        if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
            for id in [ia, ib].into_iter().flatten() {
                self.leave_out(id.entity, Rule::OrderedId);
            }
            return;
        }
        match (ia, ib) {
            (None, None) => {}
            (Some(x), Some(y)) => {
                if !self.same_class(x.entity, y.entity) {
                    self.leave_out(x.entity, Rule::MixedIds);
                    self.leave_out(y.entity, Rule::MixedIds);
                }
            }
            (Some(id), None) => self.against(id, b),
            (None, Some(id)) => self.against(id, a),
        }
    }

    /// A row id compared by `=` or `≠` with a term that names no row.
    fn against(&mut self, id: Id, other: &Term) {
        match other {
            Term::Lit(v) if id.optional_owner == Some(true) && *v == Value::NULL => {}
            Term::Lit(_) | Term::Int(_) => self.leave_out(id.entity, Rule::IdAgainstLiteral),
            Term::Slot { .. } | Term::Now | Term::Index(_) => {
                self.leave_out(id.entity, Rule::MixedIds)
            }
        }
    }

    fn effects(&mut self, params: &[Param], effects: &[Effect]) {
        let ts = self.ts;
        let owner_parent =
            |var: crate::lifecycle::types::VarId| match (&ts.var(var).kind, &ts.var(var).domain) {
                (VarKind::Owner { .. }, Domain::Instance { parent, optional }) => {
                    Some((*parent, *optional))
                }
                _ => None,
            };
        for eff in effects {
            match eff {
                Effect::Assign { var, rhs, .. } => match (owner_parent(*var), rhs) {
                    (Some((_, true)), Rhs::Lit(v)) if *v == Value::NULL => {}
                    (Some((parent, _)), Rhs::Lit(_)) => self.leave_out(parent, Rule::OwnerLiteral),
                    (Some((parent, _)), Rhs::Param(q)) => {
                        if let Some(p) = params.get(usize::from(*q)) {
                            if !self.same_class(p.entity, parent) {
                                self.leave_out(parent, Rule::MixedIds);
                                self.leave_out(p.entity, Rule::MixedIds);
                            }
                        }
                    }
                    (None, Rhs::Param(q)) => {
                        if let Some(p) = params.get(usize::from(*q)) {
                            self.leave_out(p.entity, Rule::MixedIds);
                        }
                    }
                    (None, Rhs::Lit(_)) => {}
                },
                Effect::Transition { var, .. } => {
                    if let Some((parent, _)) = owner_parent(*var) {
                        self.leave_out(parent, Rule::OwnerChoice);
                    }
                }
                Effect::Choose { var, values, .. } => {
                    if let Some((parent, _)) = owner_parent(*var) {
                        let size = ts.var(*var).domain.size(ts);
                        let chosen: BTreeSet<Value> = values.iter().map(|c| c.value).collect();
                        if (0..size).any(|v| !chosen.contains(&Value(v as u8))) {
                            self.leave_out(parent, Rule::OwnerChoice);
                        }
                    }
                }
                Effect::Create { param } => {
                    let Some(created) = params.get(usize::from(*param)) else {
                        continue;
                    };
                    // A mandatory key the creation does not write holds slot 0.
                    for &var in &ts.entity(created.entity).owners {
                        let Some((parent, optional)) = owner_parent(var) else {
                            continue;
                        };
                        if optional && ts.var(var).base == Value::NULL {
                            continue;
                        }
                        let written = effects.iter().any(|e| match e {
                            Effect::Assign {
                                var: v,
                                param: p,
                                rhs: Rhs::Param(_),
                                ..
                            } => *v == var && p == param,
                            Effect::Choose {
                                var: v, param: p, ..
                            } => *v == var && p == param,
                            _ => false,
                        });
                        if !written {
                            self.leave_out(parent, Rule::UnlinkedCreate);
                        }
                    }
                }
                Effect::Delete { .. } | Effect::Tick => {}
            }
        }
    }

    /// An owner read through an `Earlier` param, which binds a row by its run mark and not
    /// by its existence, of an entity some step deletes: the row may be deleted, its owner
    /// then holds the base value, and the group does not relabel it, so the read names a
    /// fixed row of the parent while the parent's rows move.
    fn deleted_row_owners(&mut self) {
        let ts = self.ts;
        let deleted: BTreeSet<EntId> = ts
            .actions()
            .iter()
            .flat_map(|a| {
                a.effects.iter().filter_map(|e| match e {
                    Effect::Delete { param } => a.params.get(usize::from(*param)).map(|p| p.entity),
                    _ => None,
                })
            })
            .collect();
        let reads = |params: &[Param], pred: &Pred, found: &mut BTreeSet<(EntId, Rule)>| {
            for term in pred.terms() {
                let Term::Slot {
                    var,
                    at: At::Param(p) | At::Next(p),
                } = term
                else {
                    continue;
                };
                let (VarKind::Owner { child, .. }, Domain::Instance { parent, .. }) =
                    (&ts.var(*var).kind, &ts.var(*var).domain)
                else {
                    continue;
                };
                let earlier = params
                    .get(usize::from(*p))
                    .is_some_and(|p| matches!(p.role, ParamRole::Earlier { .. }));
                if earlier && deleted.contains(child) {
                    found.insert((*parent, Rule::DeletedRowOwner));
                }
            }
        };
        let mut found = BTreeSet::new();
        for action in ts.actions() {
            for guard in &action.guards {
                reads(&action.params, &guard.pred, &mut found);
            }
        }
        for a in ts.after_asserts() {
            for step in &a.steps {
                reads(
                    &ts.actions()[step.act.index()].params,
                    &step.post,
                    &mut found,
                );
            }
        }
        self.found.extend(found);
    }

    /// Starts that pin a slot: an identity start between unequal slot counts, and a present
    /// pre-existing row whose owner starts at its base, a row.
    fn starts(&mut self) {
        let ts = self.ts;
        for v in ts.var_ids() {
            let d = ts.var(v);
            let (VarKind::Owner { child, parent, .. }, Domain::Instance { optional, .. }) =
                (&d.kind, &d.domain)
            else {
                continue;
            };
            match d.start {
                Start::Any => {}
                Start::Identity => {
                    if ts.slots(*child) != ts.slots(*parent) {
                        self.leave_out(*child, Rule::PinnedStart);
                        self.leave_out(*parent, Rule::PinnedStart);
                    }
                }
                Start::Base => {
                    let pre = ts.entity(*child).birth == Birth::PreExisting;
                    if pre && !(*optional && d.base == Value::NULL) {
                        self.leave_out(*parent, Rule::PinnedStart);
                    }
                }
            }
        }
    }
}
