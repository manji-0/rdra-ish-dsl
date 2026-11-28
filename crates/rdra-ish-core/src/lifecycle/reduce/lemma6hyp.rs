//! The run-time certificate of the syntactic hypotheses of Lemma 6 of docs/theory/cones.md:
//! every build checks the extended sub-lifecycle `X` that `Extended::of` built against the
//! fields of `proofs/lean/RdraIsh/Restrict.lean::Lemma6Hyp` that are facts about what an
//! action's conjuncts and effects read or name, before the reduction uses it. A fault makes
//! `reduced_with` decide the cone unreduced.
//!
//! The check reads the cone `C`, the split's sets and folds, and the built `X` (its actions,
//! guards, params and initial states). It does not read `fold_of`'s bookkeeping (`s_keys`,
//! `touched_t`, `linked`) or `restrict_to`'s (the `Remap`): it computes the `T` params, what a
//! guard or an effect names (`pred_params`, `effect_params`, after `Lemma6Syntax.lean`'s
//! `predPs` and `effPs`) and which params and guards the restriction keeps (`keepsP`, `keptOg`)
//! afresh from the actions' syntax. It shares `pred_reads`, `effect_writes`, `copy_target`,
//! `Fold::holds` and `initial_states` with the code it checks. Field by field, with the Lean
//! name of each (`Lemma6Hyp`'s, or `FoldSpec`'s for what `foldKey_spec` gives):
//!
//! - `kept_rd` (`Field::KeptRd`): every guard of every action of `X`, the bit guard a
//!   `FoldKey::Param` fold adds aside, reads only the vars of `S_v ∪ inputs`, which are `X`'s
//!   first vars. This is read off `X`'s own guards, not off the restriction's rule.
//! - `cover` (`Field::Cover`): every guard of an `S_a` action outside its fold reads only
//!   `S_v ∪ inputs` and names only params the restriction keeps (`keepsP`), so the
//!   restriction keeps it; every `S_a` action has an action of `X` or is a variant merged into
//!   one. The tie to `X` itself is by count: an action of `X` has as many params and guards as
//!   the restriction's rule, recomputed here, keeps, plus the fold's own. A count does not show
//!   which guard `X` kept; a guard `X` reads in place of a dropped one can only add steps,
//!   which the lift's replay of every counterexample through `C` rejects (N16).
//! - `folded_og` (`Field::FoldedOg`): each folded guard is an index into its action's guards.
//! - `FoldSpec.not_new`, `FoldSpec.no_next`, `tParam` (`Field::FoldData`): the key is a param
//!   of the action that is not `New` and that the restriction keeps, `Fold::t_params` are the
//!   params on entities whose existence is in `T_v`, each with `needsPresent`, every guard that
//!   names a `T` param reads no successor and is no copy, and the bit guard reads the bit at
//!   the key's place in `X`.
//! - `folded_names` (`Field::FoldedNames`, `FoldSpec.folded`): the folded guards name only `T`
//!   params and the key.
//! - `other_names` (`Field::OtherNames`, `FoldSpec.link`): no other guard of a folded action
//!   names a free `T` param (a `T` param that is not the key); the link `v[k] = Index(p)`
//!   names the key alone.
//! - `eff_names` (`Field::EffNames`, `FoldSpec.effs`): no effect of a folded action names a
//!   free `T` param.
//! - `eff_s` (`Field::EffS`): the values a folded action writes read only `S_v ∪ inputs`: a
//!   transition's source, the clock a tick reads, and what a copy guard reads.
//! - `rise_eff` (`Field::RiseEff`): the values a `T_a` action writes to an input read only
//!   `S_v ∪ inputs`, read the same way.
//! - The initial states, which the Lean model takes as given (`Ext.ext.init`) (`Field::Initial`):
//!   `X`'s initial states are exactly the projections of `C`'s onto `S_v ∪ inputs`, each with
//!   its bits at what `Fold::holds` gives at that state of `C` (`fold_initial`), the stubs
//!   left out (`X` reads them at one value, which no state of `C` gives).
//! - `X` holds the clock when it keeps a tick (`Field::Clock`), so its step relation has the
//!   var a tick writes.
//!
//! Node folds (`FoldKey::Linked`) and the groups of folds that share a bit are left to the
//! check of the bit itself (C2 of docs/design/certificate-plan.md): for a linked fold this
//! check reads only `cover`, `eff_s` and the count of `X`'s guards, and skips the guard the
//! fold puts in place of its node.

use std::collections::HashSet;
use std::fmt;

use super::extended::Extended;
use super::fold::FoldKey;
use super::split::Split;
use crate::lifecycle::cone::VarSet;
use crate::lifecycle::narrow::id8;
use crate::lifecycle::types::{
    ActId, Action, At, CmpOp, Effect, EntId, Param, ParamRole, Pred, Rhs, Term, Value, VarId,
    VarKind,
};
use crate::lifecycle::TransitionSystem;

/// The field of `Lemma6Hyp` (or of the initial states) that `X` fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::lifecycle) enum Field {
    KeptRd,
    Cover,
    FoldedOg,
    FoldData,
    FoldedNames,
    OtherNames,
    EffNames,
    EffS,
    RiseEff,
    Initial,
    Clock,
}

/// One fault of `X`: the field it fails and what breaks it.
#[derive(Debug)]
pub(in crate::lifecycle) struct Lemma6Fault {
    pub(in crate::lifecycle) field: Field,
    what: String,
}

impl fmt::Display for Lemma6Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = match self.field {
            Field::KeptRd => "kept_rd",
            Field::Cover => "cover",
            Field::FoldedOg => "folded_og",
            Field::FoldData => "the fold's data",
            Field::FoldedNames => "folded_names",
            Field::OtherNames => "other_names",
            Field::EffNames => "eff_names",
            Field::EffS => "eff_s",
            Field::RiseEff => "rise_eff",
            Field::Initial => "the initial states",
            Field::Clock => "the clock",
        };
        write!(f, "Lemma 6, {field}: {}", self.what)
    }
}

/// What the check read, for the walk that shows it is not vacuous.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::lifecycle) struct Seen {
    /// Actions of `X` whose guards `kept_rd` read.
    pub(in crate::lifecycle) actions: usize,
    /// `FoldKey::Param` folds checked.
    pub(in crate::lifecycle) param_folds: usize,
    /// Of those, folds with a free `T` param.
    pub(in crate::lifecycle) free: usize,
    /// Effects of a `T_a` action on an input.
    pub(in crate::lifecycle) rise_effects: usize,
    /// Initial states of `C` projected.
    pub(in crate::lifecycle) initial: usize,
}

/// The params `pred` names (`Lemma6Syntax.lean::predPs`): a slot or an index at a param or at
/// a param's successor.
pub(super) fn pred_params(pred: &Pred, out: &mut Vec<u8>) {
    match pred {
        Pred::True | Pred::False => {}
        Pred::Cmp(x, _, y) => {
            term_params(x, out);
            term_params(y, out);
        }
        Pred::In(x, ys) => {
            term_params(x, out);
            ys.iter().for_each(|y| term_params(y, out));
        }
        Pred::Not(p) => pred_params(p, out),
        Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| pred_params(p, out)),
        Pred::Forall { body, .. } | Pred::Exists { body, .. } | Pred::Count { body, .. } => {
            pred_params(body, out)
        }
    }
}

/// `termPs`.
fn term_params(term: &Term, out: &mut Vec<u8>) {
    match term {
        Term::Slot { at, .. } | Term::Index(at) => match at {
            At::Param(p) | At::Next(p) => out.push(*p),
            At::Bound(_) => {}
        },
        Term::Lit(_) | Term::Int(_) | Term::Now => {}
    }
}

/// Whether `pred` reads a successor (`predNext`).
fn pred_next(pred: &Pred) -> bool {
    let term = |t: &Term| match t {
        Term::Slot { at, .. } | Term::Index(at) => match at {
            At::Next(_) => true,
            At::Param(_) | At::Bound(_) => false,
        },
        Term::Lit(_) | Term::Int(_) | Term::Now => false,
    };
    match pred {
        Pred::True | Pred::False => false,
        Pred::Cmp(x, _, y) => term(x) || term(y),
        Pred::In(x, ys) => term(x) || ys.iter().any(term),
        Pred::Not(p) => pred_next(p),
        Pred::And(ps) | Pred::Or(ps) => ps.iter().any(pred_next),
        Pred::Forall { body, .. } | Pred::Exists { body, .. } | Pred::Count { body, .. } => {
            pred_next(body)
        }
    }
}

/// The params an effect names (`effPs`): its row, and the row an owner is set to.
pub(super) fn effect_params(eff: &Effect) -> Vec<u8> {
    match eff {
        Effect::Create { param } | Effect::Delete { param } => vec![*param],
        Effect::Assign { param, rhs, .. } => match rhs {
            Rhs::Param(q) => vec![*param, *q],
            Rhs::Lit(_) => vec![*param],
        },
        Effect::Transition { param, .. } | Effect::Choose { param, .. } => vec![*param],
        Effect::Tick => Vec::new(),
    }
}

/// `needsPresent`: a role that binds a present row.
fn needs_present(role: ParamRole) -> bool {
    match role {
        ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => true,
        ParamRole::New | ParamRole::Earlier { .. } => false,
    }
}

impl TransitionSystem {
    /// The vars the value an effect writes reads (`effRd`): a transition's source and the
    /// clock a tick reads; a literal, a slot index or a listed choice reads none.
    fn effect_value_reads(&self, eff: &Effect) -> Vec<VarId> {
        match eff {
            Effect::Transition { var, .. } => vec![*var],
            Effect::Tick => self.now.into_iter().collect(),
            Effect::Create { .. }
            | Effect::Delete { .. }
            | Effect::Assign { .. }
            | Effect::Choose { .. } => Vec::new(),
        }
    }

    /// The vars a copy guard of `a` onto a var `onto` accepts reads: the values the copy
    /// writes come from them.
    fn copy_value_reads(&self, a: &Action, onto: impl Fn(&VarId) -> bool) -> VarSet {
        let mut out = VarSet::new();
        for g in &a.guards {
            if g.copy_target().is_some_and(|t| onto(&t)) {
                self.pred_reads(&g.pred, &mut out);
            }
        }
        out
    }

    /// Every fault of `ext`, the extended sub-lifecycle `Extended::of` built from `split`, as
    /// the `X` of Lemma 6 for this cone system (the module doc lists the fields); empty when
    /// it meets them. Also returns what it read.
    pub(super) fn lemma6_faults(&self, split: &Split, ext: &Extended) -> (Vec<Lemma6Fault>, Seen) {
        let mut out = Vec::new();
        let mut seen = Seen::default();
        let mut fault = |field, what: String| out.push(Lemma6Fault { field, what });
        let x = &ext.system;
        let all: VarSet = split.s.union(&split.inputs).copied().collect();
        let in_x = |v: &VarId| all.contains(v);
        let exists = |e: EntId| self.entity(e).exists;
        let on_t = |e: EntId| !split.s.contains(&exists(e));
        // `stubE`: an entity outside `S_v ∪ inputs` that an owner var there names.
        let stub = |e: EntId| {
            !in_x(&exists(e))
                && all.iter().any(
                    |v| matches!(self.var(*v).kind, VarKind::Owner { parent, .. } if parent == e),
                )
        };
        // `keepsP`: a `New` param when its existence is kept, any other when its entity's is
        // or the entity is a stub.
        let keeps =
            |p: &Param| in_x(&exists(p.entity)) || (p.role != ParamRole::New && stub(p.entity));
        let reads = |p: &Pred| {
            let mut r = VarSet::new();
            self.pred_reads(p, &mut r);
            r
        };
        let named = |p: &Pred| {
            let mut n = Vec::new();
            pred_params(p, &mut n);
            n
        };
        let mut image: Vec<Option<usize>> = vec![None; self.actions.len()];
        for (k, a) in ext.acts.iter().enumerate() {
            if let Some(a) = a {
                image[a.index()] = Some(k);
            }
        }
        let merged = |id: ActId| ext.variants.values().any(|v| v.contains(&id));

        for (i, a) in self.actions.iter().enumerate() {
            let id = ActId::at(i);
            let act = a.name.as_str();
            let fold = split.folds.iter().find(|f| f.act == id);
            let folded = |g: usize| fold.is_some_and(|f| f.guards.contains(&g));
            let t_params: Vec<u8> = (a.params.iter().enumerate())
                .filter(|(_, p)| on_t(p.entity))
                .map(|(j, _)| id8(j))
                .collect();
            // The restriction's rule, recomputed: a guard is kept when what it pins and reads is
            // in `S_v ∪ inputs` and it names no param the restriction drops (`keptOg`).
            let kept = |g: &crate::lifecycle::types::Guard| {
                g.copy_target().is_none_or(|t| in_x(&t))
                    && reads(&g.pred).is_subset(&all)
                    && named(&g.pred)
                        .iter()
                        .all(|p| a.params.get(usize::from(*p)).is_some_and(keeps))
            };

            if split.s_actions.contains(&id) {
                for (gi, g) in a.guards.iter().enumerate().filter(|(gi, _)| !folded(*gi)) {
                    if !kept(g) {
                        fault(
                            Field::Cover,
                            format!("guard {gi} of {act} is neither folded nor kept"),
                        );
                    }
                }
                if image[i].is_none() && !merged(id) {
                    fault(Field::Cover, format!("no action of X stands for {act}"));
                }
                if let Some(f) = fold {
                    let mut eff_reads: VarSet = a
                        .effects
                        .iter()
                        .flat_map(|e| self.effect_value_reads(e))
                        .collect();
                    eff_reads.extend(self.copy_value_reads(a, |_| true));
                    for v in eff_reads.iter().filter(|v| !in_x(v)) {
                        fault(
                            Field::EffS,
                            format!("folded {act} writes a value read off {}", self.var_name(*v)),
                        );
                    }
                    if f.guards.iter().any(|g| *g >= a.guards.len()) {
                        fault(
                            Field::FoldedOg,
                            format!("a folded guard of {act} is no guard of it"),
                        );
                    }
                }
            }

            if let Some(f) = fold {
                if let FoldKey::Param(key) = f.key {
                    seen.param_folds += 1;
                    let free: Vec<u8> = t_params.iter().copied().filter(|p| *p != key).collect();
                    seen.free += usize::from(!free.is_empty());
                    let want: Vec<(u8, bool)> = (t_params.iter())
                        .map(|p| (*p, needs_present(a.params[usize::from(*p)].role)))
                        .collect();
                    if f.t_params != want {
                        fault(
                            Field::FoldData,
                            format!("the T params of {act}'s fold are not those of its action"),
                        );
                    }
                    match a.params.get(usize::from(key)) {
                        Some(p) if p.role != ParamRole::New && keeps(p) => {}
                        _ => fault(
                            Field::FoldData,
                            format!(
                                "the key {key} of {act}'s fold is no kept param that is not New"
                            ),
                        ),
                    }
                    for g in &a.guards {
                        let n = named(&g.pred);
                        if n.iter().any(|p| t_params.contains(p))
                            && (pred_next(&g.pred) || g.copy_target().is_some())
                        {
                            fault(
                                Field::FoldData,
                                format!("{act} names a T param in a guard on the successor"),
                            );
                        }
                    }
                    for (gi, g) in a.guards.iter().enumerate() {
                        let n = named(&g.pred);
                        if folded(gi) {
                            if n.iter().any(|p| *p != key && !t_params.contains(p)) {
                                fault(
                                    Field::FoldedNames,
                                    format!("folded guard {gi} of {act} names an S param besides the key"),
                                );
                            }
                        } else if n.iter().any(|p| free.contains(p)) {
                            fault(
                                Field::OtherNames,
                                format!("guard {gi} of {act} names a free T param"),
                            );
                        }
                    }
                    for e in &a.effects {
                        if effect_params(e).iter().any(|p| free.contains(p)) {
                            fault(
                                Field::EffNames,
                                format!("an effect of {act} names a free T param"),
                            );
                        }
                    }
                }
            }

            if split.t_actions.contains(&id) {
                let onto = |v: &VarId| split.inputs.contains(v);
                let mut values = VarSet::new();
                for e in &a.effects {
                    if self.effect_writes(a, e).iter().any(onto) {
                        seen.rise_effects += 1;
                        values.extend(self.effect_value_reads(e));
                    }
                }
                values.extend(self.copy_value_reads(a, onto));
                for v in values.iter().filter(|v| !in_x(v)) {
                    fault(
                        Field::RiseEff,
                        format!("{act} writes an input from {}", self.var_name(*v)),
                    );
                }
            }

            let Some(k) = image[i] else { continue };
            let xa = &x.actions[k];
            let own = usize::from(fold.is_some());
            let params = a.params.iter().filter(|p| keeps(p)).count();
            let linked = usize::from(fold.is_some_and(|f| matches!(f.key, FoldKey::Linked { .. })));
            let guards = a.guards.iter().filter(|g| kept(g)).count();
            if xa.params.len() != params + linked || xa.guards.len() != guards + own {
                fault(
                    Field::Cover,
                    format!(
                        "X's {act} has {} params and {} guards, the restriction keeps {} and {}",
                        xa.params.len(),
                        xa.guards.len(),
                        params + linked,
                        guards + own
                    ),
                );
                continue;
            }
            seen.actions += 1;
            let theirs = xa.guards.len() - own;
            for (gi, g) in xa.guards[..theirs].iter().enumerate() {
                let mut r = VarSet::new();
                x.pred_reads(&g.pred, &mut r);
                if r.iter().any(|v| v.index() >= all.len()) {
                    fault(
                        Field::KeptRd,
                        format!("guard {gi} of X's {act} reads outside S and the inputs"),
                    );
                }
            }
            if let Some(FoldKey::Param(key)) = fold.map(|f| &f.key) {
                let before = a.params.get(..usize::from(*key)).unwrap_or_default();
                let at = id8(before.iter().filter(|p| keeps(p)).count());
                let bit = match &xa.guards[theirs].pred {
                    Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(p),
                        },
                        CmpOp::Eq,
                        Term::Lit(Value::TRUE),
                    ) => ext.bit_vars.contains(var) && *p == at,
                    _ => false,
                };
                if !bit {
                    fault(
                        Field::FoldData,
                        format!("X's {act} does not read its bit at its key"),
                    );
                }
            }
        }

        if x.actions.iter().any(|a| a.effects.contains(&Effect::Tick)) && x.now.is_none() {
            fault(Field::Clock, "X keeps a tick and no clock".into());
        }

        // The initial states: `C`'s projected onto `S_v ∪ inputs` (`X`'s first vars, in their
        // order), and each bit at the disjunction of its group's `holds`.
        let far = ext
            .bit_vars
            .iter()
            .any(|b| b.index() < all.len() || b.index() >= x.vars.len());
        if ext.bits.len() != ext.bit_vars.len() || all.len() > x.vars.len() || far {
            fault(
                Field::Initial,
                "X does not hold S, the inputs and a bit per group of folds".into(),
            );
            return (out, seen);
        }
        let held: Vec<(usize, usize)> = (0..all.len())
            .map(VarId::at)
            .chain(ext.bit_vars.iter().copied())
            .map(|v| (x.offset(v), x.slots_of(&x.var(v).kind)))
            .collect();
        let cut = |s: &[u8]| -> Vec<u8> {
            held.iter()
                .flat_map(|&(at, w)| s[at..at + w].iter().copied())
                .collect()
        };
        let got: HashSet<Vec<u8>> = x.initial_states().iter().map(|s| cut(&s.0)).collect();
        let mut want: HashSet<Vec<u8>> = HashSet::new();
        for st in self.initial_states() {
            seen.initial += 1;
            let mut p = Vec::with_capacity(x.width);
            for (i, v) in all.iter().enumerate() {
                let w = self.slots_of(&self.var(*v).kind);
                if w != held[i].1 {
                    fault(
                        Field::Initial,
                        format!("{} is not as wide in X", self.var_name(*v)),
                    );
                    return (out, seen);
                }
                let at = self.offset(*v);
                p.extend_from_slice(&st.0[at..at + w]);
            }
            for (group, &(_, w)) in ext.bits.iter().zip(&held[all.len()..]) {
                for c in 0..w {
                    let on = group.iter().any(|f| f.holds(self, &st, id8(c)));
                    p.push(if on { Value::TRUE.0 } else { Value::FALSE.0 });
                }
            }
            want.insert(p);
        }
        if got != want {
            fault(
                Field::Initial,
                format!(
                    "X has {} initial states, which are not the {} projections of C's",
                    got.len(),
                    want.len()
                ),
            );
        }
        (out, seen)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::Field;
    use crate::lifecycle::cone::{Cone, Seed, VarSet};
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    use crate::lifecycle::explore::Caps;
    use crate::lifecycle::narrow::id8;
    use crate::lifecycle::reduce::extended::Extended;
    use crate::lifecycle::reduce::fold::FoldKey;
    use crate::lifecycle::reduce::partition::tests::model;
    use crate::lifecycle::reduce::split::Split;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::types::{
        At, CmpOp, Effect, EntId, Guard, GuardWhy, ModelRef, Param, ParamRole, Pred, Site, Term,
        Value, VarId,
    };
    use crate::lifecycle::{Bounds, Scope, Target, TransitionSystem};

    /// Issue, the handler of the order's paid event, issues a ticket: in the order's cone it
    /// folds keyed by its ticket, with its `Triggered` order param a free `T` param.
    const TRIGGERED: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(new, paid, shipped) @default(new) }
entity Voucher "券" { id: Int @pk  status: Enum(open, used) @default(open) }
entity Ticket "札" { id: Int @pk  status: Enum(draft, issued) @default(draft) }
relate(Voucher, Order, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Grant "発券"
usecase Draft "起票"
usecase Issue "発行"
usecase Use "使用"
event EvPaid "支払済"
creates(Place, Order)
transitions(Pay, Order.status, new -> paid)
raises(Pay, EvPaid)
triggers(EvPaid, Issue)
transitions(Ship, Order.status, paid -> shipped)
condition HasUsedVoucher "使用済の券"
holds(HasUsedVoucher, Voucher.status == used)
applies_to(HasUsedVoucher, Ship)
creates(Grant, Voucher)
creates(Draft, Ticket)
transitions(Issue, Ticket.status, draft -> issued)
transitions(Use, Voucher.status, open -> used)
condition TicketIssued "発行済の札"
holds(TicketIssued, Ticket.status == issued)
applies_to(TicketIssued, Use)
forbidden(Order, status == shipped)
"#;

    /// A claim is paid once its case is flagged: in the case's cone `Flag` is a rise that sets
    /// the flag, an input of the claims' sub-lifecycle.
    const FLAGGED: &str = r#"
entity Case "事案" { id: Int @pk  status: Enum(open, closed) @default(open)  flag: Bool @default(false) }
entity Claim "請求" { id: Int @pk  status: Enum(open, paid) @default(open) }
relate(Claim, Case, N:1)
usecase OpenCase "受付"
usecase File "請求"
usecase Flag "注記"
usecase Pay "支払"
usecase Close "終了"
creates(OpenCase, Case)
creates(File, Claim)
sets(Flag, Case.flag == true)
transitions(Pay, Claim.status, open -> paid).when(Flagged)
condition Flagged "事案に注記がある"
holds(Flagged, Case.flag == true)
transitions(Close, Case.status, open -> closed).when(NonePending)
condition NonePending "未処理の請求がない"
holds(NonePending, Case.status == open).none(Claim.status == open)
"#;

    /// Every target and property cone of the gate model (`partition::tests::model`), with and
    /// without the key's relock, of `TRIGGERED` and of `FLAGGED`, as cone systems with their
    /// seeds.
    fn cone_systems() -> Vec<(TransitionSystem, Vec<Seed>)> {
        let sources = [
            model(false),
            model(true),
            TRIGGERED.to_string(),
            FLAGGED.to_string(),
        ];
        let mut out = Vec::new();
        for src in sources {
            let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
            let targets = (ts.targets().into_iter())
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

    /// A guard `var[p] = v`.
    fn is(var: VarId, p: u8, v: Value) -> Pred {
        Pred::Cmp(
            Term::Slot {
                var,
                at: At::Param(p),
            },
            CmpOp::Eq,
            Term::Lit(v),
        )
    }

    /// The params `pred` names.
    fn named(pred: &Pred) -> Vec<u8> {
        let mut n = Vec::new();
        super::pred_params(pred, &mut n);
        n
    }

    /// A change to the cone, the split or `X`; whether it applied.
    type Plant<'a> = &'a dyn Fn(&mut TransitionSystem, &mut Split, &mut Extended) -> bool;

    /// Each field fails on an extended sub-lifecycle, a split or a cone planted to break it:
    /// what `reduced_with` builds with one thing changed. As built, every one passes. Then
    /// one fault planted end to end: with every bit started at the other value
    /// (`bit_starts`), every cone with a bit is decided unreduced.
    #[test]
    fn a_planted_fault_fails_the_field_it_breaks() {
        let mut fired: BTreeSet<Field> = BTreeSet::new();
        for (sys, seeds) in cone_systems() {
            for inputs in [true, false] {
                let split = sys.split_with(&seeds, inputs);
                if split.s.is_empty() || split.s.is_subset(&split.obs) {
                    continue;
                }
                let fields = |sys: &TransitionSystem, split: &Split, ext: &Extended| {
                    let (faults, _) = sys.lemma6_faults(split, ext);
                    faults.iter().map(|f| f.field).collect::<BTreeSet<Field>>()
                };
                let ext = Extended::of(&sys, &split);
                assert!(fields(&sys, &split, &ext).is_empty(), "X as built passes");
                let all: VarSet = split.s.union(&split.inputs).copied().collect();
                let t: VarSet = sys.var_ids().filter(|v| !split.s.contains(v)).collect();
                let outside = t.difference(&split.inputs).next().copied();
                let mut plant = |want: Field, change: Plant| {
                    let (mut sys, mut split) = (sys.clone(), sys.split_with(&seeds, inputs));
                    let mut ext = Extended::of(&sys, &split);
                    if change(&mut sys, &mut split, &mut ext) {
                        let got = fields(&sys, &split, &ext);
                        assert!(got.contains(&want), "{want:?} fails: {got:?}");
                        fired.insert(want);
                    }
                };
                let guarded = |ext: &Extended| {
                    (0..ext.system.actions.len()).find(|k| {
                        ext.acts[*k].is_some() && !ext.system.actions[*k].guards.is_empty()
                    })
                };
                // A guard of X that reads a var past S and the inputs (a stub's or a bit's).
                plant(Field::KeptRd, &|_, _, ext| {
                    let far = VarId::at(all.len());
                    match guarded(ext) {
                        Some(k) if far.index() < ext.system.vars.len() => {
                            ext.system.actions[k].guards[0].pred = is(far, 0, Value::TRUE);
                            true
                        }
                        _ => false,
                    }
                });
                // A guard X dropped.
                plant(Field::Cover, &|_, _, ext| {
                    guarded(ext).is_some_and(|k| ext.system.actions[k].guards.pop().is_some())
                });
                // A folded guard that is no guard of the action.
                plant(Field::FoldedOg, &|_, split, _| {
                    split.folds.first_mut().is_some_and(|f| {
                        f.guards.push(usize::MAX);
                        true
                    })
                });
                // A T param taken as needing a present row where it does not, or the reverse.
                plant(Field::FoldData, &|_, split, _| {
                    let f = split.folds.iter_mut().find(|f| !f.t_params.is_empty());
                    f.is_some_and(|f| {
                        f.t_params[0].1 = !f.t_params[0].1;
                        true
                    })
                });
                // A folded guard that names an S param besides the key: a new param on an S
                // entity, compared with itself.
                plant(Field::FoldedNames, &|sys, split, _| {
                    let s_entity = (0..sys.entities.len())
                        .map(EntId::at)
                        .find(|e| split.s.contains(&sys.entity(*e).exists));
                    let (Some(f), Some(entity)) = (split.folds.first(), s_entity) else {
                        return false;
                    };
                    let Some(&g) = f.guards.first() else {
                        return false;
                    };
                    let a = &mut sys.actions[f.act.index()];
                    let p = id8(a.params.len());
                    a.params.push(Param {
                        entity,
                        role: ParamRole::Existing,
                        via_api: None,
                    });
                    let own = Term::Index(At::Param(p));
                    let pred = std::mem::replace(&mut a.guards[g].pred, Pred::True);
                    a.guards[g].pred =
                        Pred::And(vec![pred, Pred::Cmp(own.clone(), CmpOp::Eq, own)]);
                    true
                });
                // A folded guard that names a free T param taken out of the fold.
                plant(Field::OtherNames, &|sys, split, _| {
                    split.folds.iter_mut().any(|f| {
                        let FoldKey::Param(key) = f.key else {
                            return false;
                        };
                        let a = &sys.actions[f.act.index()];
                        let free: Vec<u8> = (f.t_params.iter().map(|(p, _)| *p))
                            .filter(|p| *p != key)
                            .collect();
                        let at = (f.guards.iter()).position(|g| {
                            named(&a.guards[*g].pred).iter().any(|p| free.contains(p))
                        });
                        at.is_some_and(|at| {
                            f.guards.remove(at);
                            true
                        })
                    })
                });
                // An effect of a folded action on a free T param's row.
                plant(Field::EffNames, &|sys, split, _| {
                    let found = split.folds.iter().find_map(|f| {
                        let FoldKey::Param(key) = f.key else {
                            return None;
                        };
                        let p = f.t_params.iter().map(|(p, _)| *p).find(|p| *p != key)?;
                        Some((f.act, p))
                    });
                    found.is_some_and(|(act, p)| {
                        sys.actions[act.index()]
                            .effects
                            .push(Effect::Delete { param: p });
                        true
                    })
                });
                // A folded action that writes a value read off T outside the inputs.
                plant(Field::EffS, &|sys, split, _| {
                    let (Some(f), Some(v)) = (split.folds.first(), outside) else {
                        return false;
                    };
                    sys.actions[f.act.index()].effects.push(Effect::Transition {
                        var: v,
                        param: 0,
                        arms: Vec::new(),
                    });
                    true
                });
                // A rise that copies a T var outside the inputs into an input: a var a T
                // action writes, made an input when none is.
                plant(Field::RiseEff, &|sys, split, _| {
                    let found = split.t_actions.iter().find_map(|id| {
                        let a = &sys.actions[id.index()];
                        let writes = sys.action_writes(a);
                        let input = (writes.iter().find(|w| split.inputs.contains(w)))
                            .or_else(|| writes.first())
                            .copied()?;
                        let source = t
                            .iter()
                            .copied()
                            .find(|v| *v != input && !split.inputs.contains(v))?;
                        Some((*id, input, source))
                    });
                    found.is_some_and(|(id, input, source)| {
                        split.inputs.insert(input);
                        let source = Term::Slot {
                            var: source,
                            at: At::Param(0),
                        };
                        let target = Term::Slot {
                            var: input,
                            at: At::Next(0),
                        };
                        sys.actions[id.index()].guards.push(Guard {
                            pred: Pred::Cmp(target, CmpOp::Eq, source),
                            why: GuardWhy::Copy,
                            site: Site::bare(ModelRef::Synthesized),
                        });
                        true
                    })
                });
                // A bit that starts at the other value.
                plant(Field::Initial, &|_, _, ext| {
                    ext.bit_vars.first().is_some_and(|b| {
                        let var = &mut ext.system.vars[b.index()];
                        var.base = Value(var.base.0 ^ 1);
                        true
                    })
                });
                // A tick X keeps, with no clock.
                plant(Field::Clock, &|_, _, ext| {
                    let k = (0..ext.system.actions.len()).find(|k| ext.acts[*k].is_some());
                    k.is_some_and(|k| {
                        ext.system.now = None;
                        ext.system.actions[k].effects.push(Effect::Tick);
                        true
                    })
                });
            }
        }
        let all = [
            Field::KeptRd,
            Field::Cover,
            Field::FoldedOg,
            Field::FoldData,
            Field::FoldedNames,
            Field::OtherNames,
            Field::EffNames,
            Field::EffS,
            Field::RiseEff,
            Field::Initial,
            Field::Clock,
        ];
        assert_eq!(fired, all.into_iter().collect(), "every field fails once");

        let caps = Caps::new(usize::MAX, 200_000);
        let mut fell_back = 0;
        for (sys, seeds) in cone_systems() {
            let reduced = sys.reduced(&seeds, caps);
            let planted = dropping("bit_starts", || sys.reduced(&seeds, caps));
            if reduced.reduction().is_some_and(|r| !r.bits.is_empty()) {
                fell_back += 1;
                assert!(planted == sys, "the cone is decided unreduced");
            }
        }
        assert!(fell_back > 0, "a reduced cone with a bit falls back");
    }
}
