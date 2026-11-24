//! The transition system in SMT-LIB 2, for any number of rows.
//!
//! Each entity's slots become an uninterpreted sort `E<i>` with two named elements: `nil<i>`,
//! which stands for an unlinked optional key and is never a row, and `zero<i>`, the slot a
//! non-optional key holds at base (slot 0). Every var but `now` is a function from its
//! entity's sort to an `Int` (a domain index, as `Value` holds it) or, for a key, to the
//! parent's sort. `now` is an `Int` constant. An Int column and `now` range over every
//! non-negative integer, not the bounds' `0..=int_max`: the clock ticks without a top, and a
//! choice of every value of an Int domain (`sets(.., col == present)`, a copy) takes any
//! number. Enum domains are finite and keep their values.
//!
//! The encoding over-approximates the explorer in two ways: a `New` param takes any absent
//! slot, not the lowest one, and numbers have no top. Every state and step the explorer
//! reaches, at any slot count and any Int range, is then a model of the encoding, so a
//! property proved here holds at every bound.

use std::collections::BTreeMap;

use crate::lifecycle::types::{
    Action, At, Birth, Choice, CmpOp, Domain, Effect, EntId, GuardWhy, ParamRole, Pred, Rhs, Start,
    Term, Value, VarId, VarKind,
};
use crate::lifecycle::TransitionSystem;

/// Which state a function name reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Frame {
    Pre,
    Post,
}

/// A construct the encoding does not express. The prover reports a rule that needs one as
/// out of scope; it never approximates a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Unsupported(pub String);

type R<T> = Result<T, Unsupported>;

/// The most witnesses a count unrolls into: `.at_most(7, ..)` needs 8.
const COUNT_WITNESSES: u16 = 8;

/// A state formula: `body` over every present row of `binders` (every row but `nil`, with
/// `all_rows`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Formula {
    pub binders: Vec<EntId>,
    pub body: Pred,
    pub all_rows: bool,
}

/// One action as SMT-LIB: the commands that declare its params and choices, define the
/// successor, and constrain both. `loose` names the guards and effects it could not express;
/// a dropped guard and a havocked write only add steps, so a proof stays sound.
pub(super) struct Step {
    pub commands: String,
    pub loose: Vec<String>,
}

pub(super) struct Enc<'a> {
    pub ts: &'a TransitionSystem,
}

enum Op {
    /// A var read: its SMT term and domain.
    Var(String, Domain),
    Lit(Value),
    /// An integer term (a literal or `now`).
    Num(String),
    /// A row of an entity (`Term::Index`).
    Row(String, EntId),
}

/// What a predicate's slot references resolve to.
struct Scope<'s> {
    /// The state `Bound` reads, quantifiers range over, and `now` comes from.
    frame: Frame,
    /// Each param's entity; `At::Param` reads the pre-state, `At::Next` the successor.
    params: &'s [EntId],
    /// The entity of each bound row, outermost first.
    bound: Vec<EntId>,
}

pub(super) fn sort(e: EntId) -> String {
    format!("E{}", e.index())
}

pub(super) fn nil(e: EntId) -> String {
    format!("nil{}", e.index())
}

fn zero(e: EntId) -> String {
    format!("zero{}", e.index())
}

pub(super) fn func(var: VarId, frame: Frame) -> String {
    match frame {
        Frame::Pre => format!("v{}", var.index()),
        Frame::Post => format!("v{}_n", var.index()),
    }
}

pub(super) fn now(frame: Frame) -> &'static str {
    match frame {
        Frame::Pre => "now",
        Frame::Post => "now_n",
    }
}

pub(super) fn param(p: usize) -> String {
    format!("p{p}")
}

pub(super) fn bound(b: usize) -> String {
    format!("b{b}")
}

impl Enc<'_> {
    /// `term` lies in `var`'s domain: `0..size` for a finite domain, any non-negative number
    /// for an Int domain.
    fn in_domain(&self, var: VarId, term: &str) -> String {
        let domain = &self.ts.var(var).domain;
        match domain.is_numeric() {
            true => format!("(<= 0 {term})"),
            false => format!("(and (<= 0 {term}) (< {term} {}))", domain.size(self.ts)),
        }
    }

    /// The parent and optionality of a key var.
    fn key(&self, var: VarId) -> Option<(EntId, bool)> {
        match self.ts.var(var).domain {
            Domain::Instance { parent, optional } => Some((parent, optional)),
            _ => None,
        }
    }

    fn var_sort(&self, var: VarId) -> String {
        match self.key(var) {
            Some((parent, _)) => sort(parent),
            None => "Int".to_string(),
        }
    }

    /// The entity whose rows a var is a function of.
    pub(super) fn owner(&self, var: VarId) -> Option<EntId> {
        self.ts.var_entity(var)
    }

    /// `var` of `row` in `frame`.
    pub(super) fn read(&self, var: VarId, frame: Frame, row: &str) -> String {
        match self.ts.var(var).kind {
            VarKind::Now => now(frame).to_string(),
            _ => format!("({} {row})", func(var, frame)),
        }
    }

    fn exists(&self, e: EntId, frame: Frame, row: &str) -> String {
        self.read(self.ts.entity(e).exists, frame, row)
    }

    fn present(&self, e: EntId, frame: Frame, row: &str) -> String {
        format!("(= {} 1)", self.exists(e, frame, row))
    }

    /// A value of `var`'s domain as a term of its sort.
    fn value(&self, var: VarId, v: Value) -> R<String> {
        match self.key(var) {
            None => Ok(v.0.to_string()),
            Some((parent, true)) if v == Value::NULL => Ok(nil(parent)),
            Some((parent, false)) if v.0 == 0 => Ok(zero(parent)),
            Some(_) => Err(Unsupported(format!(
                "a fixed slot of {} as a value",
                self.ts.var_name(var)
            ))),
        }
    }

    /// The sorts, the pre-state functions, and the facts every state of the system has: each
    /// var stays in its domain, `nil` is never a row, and a non-optional key is never `nil`.
    pub(super) fn declarations(&self) -> String {
        let ts = self.ts;
        let mut out = String::new();
        for e in ts.entity_ids() {
            let s = sort(e);
            out += &format!(
                "(declare-sort {s} 0)\n(declare-const {n} {s})\n(declare-const {z} {s})\n(assert (distinct {n} {z}))\n",
                n = nil(e),
                z = zero(e)
            );
        }
        for v in ts.var_ids() {
            let decl = ts.var(v);
            if let VarKind::Now = decl.kind {
                out += "(declare-const now Int)\n(assert (<= 0 now))\n";
                continue;
            }
            let e = self.owner(v).expect("a row var has an entity");
            let (s, f) = (sort(e), func(v, Frame::Pre));
            out += &format!("(declare-fun {f} ({s}) {})\n", self.var_sort(v));
            match self.key(v) {
                None => {
                    out += &format!(
                        "(assert (forall ((x {s})) {}))\n",
                        self.in_domain(v, &format!("({f} x)"))
                    );
                }
                Some((parent, false)) => {
                    out += &format!(
                        "(assert (forall ((x {s})) (not (= ({f} x) {}))))\n",
                        nil(parent)
                    );
                }
                Some((_, true)) => {}
            }
        }
        for e in ts.entity_ids() {
            out += &format!("(assert (= {} 0))\n", self.exists(e, Frame::Pre, &nil(e)));
        }
        out
    }

    /// The initial states: every var at base, except that each slot of a pre-existing entity
    /// is present and its columns and keys start as `Start` says.
    pub(super) fn initial(&self) -> R<String> {
        let ts = self.ts;
        let mut out = String::new();
        for v in ts.var_ids() {
            let decl = ts.var(v);
            let base = self.value(v, decl.base)?;
            let Some(e) = self.owner(v) else {
                out += &format!("(assert (= now {base}))\n");
                continue;
            };
            let ent = ts.entity(e);
            let s = sort(e);
            let at_base = format!("(= ({} x) {base})", func(v, Frame::Pre));
            let start = match (ent.birth, &decl.kind) {
                (Birth::Created, _) => None,
                (Birth::PreExisting, VarKind::Exists(_)) => {
                    Some(format!("(= ({} x) 1)", func(v, Frame::Pre)))
                }
                (Birth::PreExisting, _) if ent.columns.contains(&v) || ent.owners.contains(&v) => {
                    match decl.start {
                        Start::Base => None,
                        Start::Any => Some("true".to_string()),
                        Start::Identity => {
                            let (parent, _) = self.key(v).ok_or_else(|| {
                                Unsupported("an identity start of a non-key".to_string())
                            })?;
                            let f = func(v, Frame::Pre);
                            out += &format!(
                                "(assert (forall ((x {s}) (y {s})) (=> (and (not (= x {n})) (not (= y {n})) (= ({f} x) ({f} y))) (= x y))))\n",
                                n = nil(e)
                            );
                            Some(format!("(not (= ({f} x) {}))", nil(parent)))
                        }
                    }
                }
                (Birth::PreExisting, _) => None,
            };
            out += &match start {
                None => format!("(assert (forall ((x {s})) {at_base}))\n"),
                Some(start) => format!(
                    "(assert (forall ((x {s})) (ite (= x {}) {at_base} {start})))\n",
                    nil(e)
                ),
            };
        }
        Ok(out)
    }

    /// The formula in `frame`, closed over its binders.
    pub(super) fn holds(&self, f: &Formula, frame: Frame) -> R<String> {
        let scope = Scope {
            frame,
            params: &[],
            bound: f.binders.clone(),
        };
        let mut body = self.pred(&f.body, &scope)?;
        for (i, e) in f.binders.iter().enumerate().rev() {
            let b = bound(i);
            body = match f.all_rows {
                true => format!(
                    "(forall (({b} {})) (=> (not (= {b} {})) {body}))",
                    sort(*e),
                    nil(*e)
                ),
                false => format!(
                    "(forall (({b} {})) (=> {} {body}))",
                    sort(*e),
                    self.present(*e, frame, &b)
                ),
            };
        }
        Ok(body)
    }

    /// Commands that declare one row per binder (`b0`, ..) and assert that they violate the
    /// formula in `frame`.
    pub(super) fn violation(&self, f: &Formula, frame: Frame) -> R<String> {
        let scope = Scope {
            frame,
            params: &[],
            bound: f.binders.clone(),
        };
        let body = self.pred(&f.body, &scope)?;
        let mut out = String::new();
        let mut parts = Vec::new();
        for (i, e) in f.binders.iter().enumerate() {
            let b = bound(i);
            out += &format!("(declare-const {b} {})\n", sort(*e));
            parts.push(format!("(not (= {b} {}))", nil(*e)));
            if !f.all_rows {
                parts.push(self.present(*e, frame, &b));
            }
        }
        parts.push(format!("(not {body})"));
        out += &format!("(assert (and {}))\n", parts.join(" "));
        Ok(out)
    }

    /// One step of `action` from the pre-state to the successor (`moves.rs`): params bound
    /// by role, guards on both states, effects in order, deletes, then the choices.
    pub(super) fn step(&self, action: &Action) -> Step {
        let ts = self.ts;
        let mut cmds = String::new();
        let mut loose = Vec::new();
        let entities: Vec<EntId> = action.params.iter().map(|p| p.entity).collect();
        for (i, p) in action.params.iter().enumerate() {
            let (name, e) = (param(i), p.entity);
            cmds += &format!(
                "(declare-const {name} {})\n(assert (not (= {name} {})))\n",
                sort(e),
                nil(e)
            );
            let exists = self.exists(e, Frame::Pre, &name);
            match p.role {
                ParamRole::New => cmds += &format!("(assert (= {exists} 0))\n"),
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                    cmds += &format!("(assert (= {exists} 1))\n")
                }
                ParamRole::Earlier { .. } => {}
            }
        }

        // The successor of each var, as a term over the row `x`, built in effect order.
        let mut next: BTreeMap<VarId, String> = BTreeMap::new();
        let mut extra = 0usize;
        let mut fresh = |cmds: &mut String, sort: String| {
            let name = format!("c{extra}");
            extra += 1;
            *cmds += &format!("(declare-const {name} {sort})\n");
            name
        };
        let cur = |next: &BTreeMap<VarId, String>, v: VarId| {
            next.get(&v)
                .cloned()
                .unwrap_or_else(|| format!("({} x)", func(v, Frame::Pre)))
        };
        let set = |next: &mut BTreeMap<VarId, String>, v: VarId, p: u8, cond: &str, val: &str| {
            let old = cur(next, v);
            let at = format!("(= x {})", param(usize::from(p)));
            let when = match cond {
                "" => at,
                c => format!("(and {at} {c})"),
            };
            next.insert(v, format!("(ite {when} {val} {old})"));
        };
        let mut now_next = now(Frame::Pre).to_string();
        let mut deleted: Vec<u8> = Vec::new();
        // Choices fork after the other effects: (var, param, condition, choice).
        let mut forks: Vec<(VarId, u8, String, String)> = Vec::new();
        for eff in &action.effects {
            match eff {
                Effect::Create { param: p } => {
                    let e = entities[usize::from(*p)];
                    set(&mut next, ts.entity(e).exists, *p, "", "1");
                }
                Effect::Delete { param: p } => deleted.push(*p),
                Effect::Assign {
                    var, param: p, rhs, ..
                } => {
                    let val = match rhs {
                        Rhs::Lit(v) => self.value(*var, *v),
                        Rhs::Param(q) => match self.key(*var) {
                            Some(_) => Ok(param(usize::from(*q))),
                            None => Err(Unsupported("a slot index into a non-key".to_string())),
                        },
                    };
                    match val {
                        Ok(val) => set(&mut next, *var, *p, "", &val),
                        Err(Unsupported(why)) => {
                            loose.push(format!("{} of {}: {why}", ts.var_name(*var), action.name));
                            let h = self.havoc(&mut cmds, &mut fresh, *var);
                            set(&mut next, *var, *p, "", &h);
                        }
                    }
                }
                Effect::Transition {
                    var,
                    param: p,
                    arms,
                    ..
                } => {
                    let from = self.read(*var, Frame::Pre, &param(usize::from(*p)));
                    let mut tos: BTreeMap<Value, Vec<Value>> = BTreeMap::new();
                    for arm in arms {
                        tos.entry(arm.from).or_default().push(arm.to);
                    }
                    let mut multi = Vec::new();
                    for (f, ts_) in &tos {
                        let (Ok(fv), Ok(tvs)) = (
                            self.value(*var, *f),
                            ts_.iter()
                                .map(|t| self.value(*var, *t))
                                .collect::<R<Vec<_>>>(),
                        ) else {
                            loose.push(format!("{} of {}", ts.var_name(*var), action.name));
                            let h = self.havoc(&mut cmds, &mut fresh, *var);
                            set(&mut next, *var, *p, "", &h);
                            continue;
                        };
                        let cond = format!("(= {from} {fv})");
                        match &tvs[..] {
                            [to] => set(&mut next, *var, *p, &cond, to),
                            _ => multi.push((cond, tvs)),
                        }
                    }
                    if !multi.is_empty() {
                        let c = fresh(&mut cmds, self.var_sort(*var));
                        let when = format!(
                            "(or {})",
                            multi
                                .iter()
                                .map(|(c, _)| c.as_str())
                                .collect::<Vec<_>>()
                                .join(" ")
                        );
                        let pick = multi
                            .iter()
                            .map(|(cond, tvs)| {
                                let any: Vec<String> =
                                    tvs.iter().map(|t| format!("(= {c} {t})")).collect();
                                format!("(and {cond} (or {}))", any.join(" "))
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        cmds += &format!("(assert (=> {when} (or {pick})))\n");
                        forks.push((*var, *p, when, c));
                    }
                }
                Effect::Choose {
                    var,
                    param: p,
                    values,
                } => match values
                    .iter()
                    .map(|c| self.value(*var, c.value))
                    .collect::<R<Vec<_>>>()
                {
                    Ok(vals) => {
                        let c = fresh(&mut cmds, self.var_sort(*var));
                        cmds += &format!("(assert {})\n", self.one_of(*var, values, &vals, &c));
                        forks.push((*var, *p, String::new(), c));
                    }
                    Err(Unsupported(why)) => {
                        loose.push(format!("{} of {}: {why}", ts.var_name(*var), action.name));
                        let h = self.havoc(&mut cmds, &mut fresh, *var);
                        forks.push((*var, *p, String::new(), h));
                    }
                },
                Effect::Tick => now_next = format!("(+ {} 1)", now(Frame::Pre)),
            }
        }
        for &p in &deleted {
            let decl = ts.entity(entities[usize::from(p)]);
            // A deleted row keeps its keys, as in the native step.
            for &v in &decl.columns {
                match self.value(v, ts.var(v).base) {
                    Ok(base) => set(&mut next, v, p, "", &base),
                    Err(_) => unreachable!("a base value is always expressible"),
                }
            }
            set(&mut next, decl.exists, p, "", "2");
        }
        for (v, p, cond, c) in forks {
            if !deleted.contains(&p) {
                set(&mut next, v, p, &cond, &c);
            }
        }

        for v in ts.var_ids() {
            let Some(e) = self.owner(v) else {
                cmds += &format!("(define-fun now_n () Int {now_next})\n");
                continue;
            };
            cmds += &format!(
                "(define-fun {} ((x {})) {} {})\n",
                func(v, Frame::Post),
                sort(e),
                self.var_sort(v),
                cur(&next, v)
            );
        }

        let scope = Scope {
            frame: Frame::Pre,
            params: &entities,
            bound: Vec::new(),
        };
        // The clock's guard is the bound's top; without a top, `Tick` always runs.
        for g in action.guards.iter().filter(|g| g.why != GuardWhy::Clock) {
            match self.pred(&g.pred, &scope) {
                Ok(p) => cmds += &format!("(assert {p})\n"),
                Err(Unsupported(why)) => loose.push(format!("a guard of {}: {why}", action.name)),
            }
        }
        Step {
            commands: cmds,
            loose,
        }
    }

    /// `c` is one of `values` (whose terms are `terms`). A list of every index from 1 of an Int
    /// domain at the bounds stands for every index from 1 (every number, or every non-null
    /// one), and with index 0 for every index.
    fn one_of(&self, var: VarId, values: &[Choice], terms: &[String], c: &str) -> String {
        let domain = &self.ts.var(var).domain;
        let size = domain.size(self.ts);
        let has = |v: Value| values.iter().any(|c| c.value == v);
        let listed = |from: usize| (from..size).all(|i| has(Value(i as u8)));
        // The non-null numbers start at index 1 of a nullable domain; `present` on an Int
        // column lists 1.. as well.
        if domain.is_numeric() && listed(1) {
            let from = match has(Value(0)) {
                true => 0,
                false => 1,
            };
            return format!("(<= {from} {c})");
        }
        or(terms.iter().map(|t| format!("(= {c} {t})")).collect())
    }

    /// A fresh constant of `var`'s domain.
    fn havoc(
        &self,
        cmds: &mut String,
        fresh: &mut impl FnMut(&mut String, String) -> String,
        var: VarId,
    ) -> String {
        let h = fresh(cmds, self.var_sort(var));
        match self.key(var) {
            None => *cmds += &format!("(assert {})\n", self.in_domain(var, &h)),
            Some((parent, false)) => *cmds += &format!("(assert (not (= {h} {})))\n", nil(parent)),
            Some((_, true)) => {}
        }
        h
    }

    fn pred(&self, pred: &Pred, scope: &Scope) -> R<String> {
        Ok(match pred {
            Pred::True => "true".to_string(),
            Pred::False => "false".to_string(),
            Pred::Cmp(a, op, b) => {
                let (a, b) = (self.term(a, scope)?, self.term(b, scope)?);
                self.compare(&a, *op, &b)?
            }
            Pred::In(a, vs) => {
                let a = self.term(a, scope)?;
                let parts = vs
                    .iter()
                    .map(|v| self.compare(&a, CmpOp::Eq, &self.term(v, scope)?))
                    .collect::<R<Vec<_>>>()?;
                or(parts)
            }
            Pred::Not(p) => format!("(not {})", self.pred(p, scope)?),
            Pred::And(ps) => and(ps.iter().map(|p| self.pred(p, scope)).collect::<R<_>>()?),
            Pred::Or(ps) => or(ps.iter().map(|p| self.pred(p, scope)).collect::<R<_>>()?),
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                let b = bound(scope.bound.len());
                let mut inner = Scope {
                    frame: scope.frame,
                    params: scope.params,
                    bound: scope.bound.clone(),
                };
                inner.bound.push(*entity);
                let body = self.pred(body, &inner)?;
                let present = self.present(*entity, scope.frame, &b);
                let s = sort(*entity);
                match pred {
                    Pred::Forall { .. } => format!("(forall (({b} {s})) (=> {present} {body}))"),
                    _ => format!("(exists (({b} {s})) (and {present} {body}))"),
                }
            }
            Pred::Count {
                entity,
                body,
                op,
                n,
            } => {
                let at_least = |k: u16| self.at_least(*entity, body, k, scope);
                let n = u16::from(*n);
                match op {
                    CmpOp::Ge => at_least(n)?,
                    CmpOp::Gt => at_least(n + 1)?,
                    CmpOp::Le => format!("(not {})", at_least(n + 1)?),
                    CmpOp::Lt => format!("(not {})", at_least(n)?),
                    CmpOp::Eq => format!("(and {} (not {}))", at_least(n)?, at_least(n + 1)?),
                    CmpOp::Ne => format!("(not (and {} (not {})))", at_least(n)?, at_least(n + 1)?),
                }
            }
        })
    }

    /// "At least `k` present rows of `entity` satisfy `body`": `k` distinct rows, each
    /// present and satisfying it. The body reads its row as `b<depth>`; each witness binds that
    /// name with `let`. That is the count, not an approximation of it, in either polarity.
    /// Past `COUNT_WITNESSES` witnesses the formula is out of scope.
    fn at_least(&self, entity: EntId, body: &Pred, k: u16, scope: &Scope) -> R<String> {
        if k == 0 {
            return Ok("true".to_string());
        }
        if k > COUNT_WITNESSES {
            return Err(Unsupported(format!(
                "a count of {k} or more rows (at most {COUNT_WITNESSES} unroll)"
            )));
        }
        let b = bound(scope.bound.len());
        let mut inner = Scope {
            frame: scope.frame,
            params: scope.params,
            bound: scope.bound.clone(),
        };
        inner.bound.push(entity);
        let body = self.pred(body, &inner)?;
        let present = self.present(entity, scope.frame, &b);
        let s = sort(entity);
        let witnesses: Vec<String> = (0..k).map(|i| format!("{b}_{i}")).collect();
        let decls: Vec<String> = witnesses.iter().map(|w| format!("({w} {s})")).collect();
        let mut parts: Vec<String> = Vec::new();
        if k > 1 {
            parts.push(format!("(distinct {})", witnesses.join(" ")));
        }
        parts.extend(
            witnesses
                .iter()
                .map(|w| format!("(let (({b} {w})) (and {present} {body}))")),
        );
        Ok(format!("(exists ({}) {})", decls.join(" "), and(parts)))
    }

    fn row(&self, at: At, scope: &Scope) -> (String, EntId, Frame) {
        match at {
            At::Param(p) => (
                param(usize::from(p)),
                scope.params[usize::from(p)],
                Frame::Pre,
            ),
            At::Next(p) => (
                param(usize::from(p)),
                scope.params[usize::from(p)],
                Frame::Post,
            ),
            At::Bound(b) => (
                bound(usize::from(b)),
                scope.bound[usize::from(b)],
                scope.frame,
            ),
        }
    }

    fn term(&self, term: &Term, scope: &Scope) -> R<Op> {
        Ok(match term {
            Term::Slot { var, at } => {
                let (row, _, frame) = self.row(*at, scope);
                Op::Var(
                    self.read(*var, frame, &row),
                    self.ts.var(*var).domain.clone(),
                )
            }
            Term::Lit(v) => Op::Lit(*v),
            Term::Int(n) if *n >= 0 => Op::Num(n.to_string()),
            Term::Int(n) => Op::Num(format!("(- {})", n.unsigned_abs())),
            Term::Index(at) => {
                let (row, e, _) = self.row(*at, scope);
                Op::Row(row, e)
            }
            Term::Now => Op::Num(now(scope.frame).to_string()),
        })
    }

    /// `expr::compare` over terms: a key against a row, then numbers (where `null` equals only
    /// `null` and is never ordered), then domain indices.
    fn compare(&self, lhs: &Op, op: CmpOp, rhs: &Op) -> R<String> {
        fn unsupported<T>(what: &str) -> R<T> {
            Err(Unsupported(what.to_string()))
        }
        if let (Op::Row(row, e), Op::Var(v, d)) | (Op::Var(v, d), Op::Row(row, e)) = (lhs, rhs) {
            return match d {
                Domain::Instance { parent, .. } if parent == e => {
                    let eq = format!("(= {v} {row})");
                    Ok(match op {
                        CmpOp::Eq => eq,
                        CmpOp::Ne => format!("(not {eq})"),
                        _ => "false".to_string(),
                    })
                }
                _ => unsupported("a row compared with a value that is not its key"),
            };
        }
        let numeric = |o: &Op| match o {
            Op::Var(_, d) if d.is_numeric() => Some(d.clone()),
            _ => None,
        };
        let domain = numeric(lhs).or_else(|| numeric(rhs));
        let is_num = |o: &Op| matches!(o, Op::Num(_));
        if domain.is_some() || is_num(lhs) || is_num(rhs) {
            // (is null, number)
            let num = |o: &Op| -> R<(String, String)> {
                Ok(match o {
                    Op::Num(n) => ("false".to_string(), n.clone()),
                    Op::Row(..) => return unsupported("a row compared as a number"),
                    // A var reads through its own numeric domain, else the other side's.
                    Op::Var(_, Domain::Instance { .. }) => {
                        return unsupported("a key compared as a number")
                    }
                    Op::Var(v, _) => match numeric(o).or(domain.clone()) {
                        Some(Domain::NullableInt) => (format!("(= {v} 0)"), format!("(- {v} 1)")),
                        _ => ("false".to_string(), v.clone()),
                    },
                    Op::Lit(v) => match &domain {
                        Some(Domain::NullableInt) if v.0 == 0 => ("true".to_string(), "0".into()),
                        Some(Domain::NullableInt) => ("false".to_string(), (v.0 - 1).to_string()),
                        _ => ("false".to_string(), v.0.to_string()),
                    },
                })
            };
            let ((ln, l), (rn, r)) = (num(lhs)?, num(rhs)?);
            let both = format!("(and (not {ln}) (not {rn}))");
            return Ok(match op {
                CmpOp::Eq => format!("(or (and {ln} {rn}) (and {both} (= {l} {r})))"),
                CmpOp::Ne => format!("(not (or (and {ln} {rn}) (and {both} (= {l} {r}))))"),
                _ => format!("(and {both} ({} {l} {r}))", ordered(op)),
            });
        }
        // Domain indices; a key's values are rows of its parent.
        let key = |o: &Op| match o {
            Op::Var(_, Domain::Instance { parent, optional }) => Some((*parent, *optional)),
            _ => None,
        };
        let id = |o: &Op, other: Option<(EntId, bool)>| -> R<Option<(String, EntId, bool)>> {
            Ok(match (o, other) {
                (Op::Var(v, Domain::Instance { parent, optional }), _) => {
                    Some((v.clone(), *parent, *optional))
                }
                (Op::Row(r, e), _) => Some((r.clone(), *e, false)),
                (Op::Lit(v), Some((parent, true))) if *v == Value::NULL => {
                    Some((nil(parent), parent, true))
                }
                (Op::Lit(v), Some((parent, opt))) if v.0 == 0 && !opt => {
                    Some((zero(parent), parent, false))
                }
                (Op::Lit(_), Some(_)) => return unsupported("a fixed slot as a value"),
                _ => None,
            })
        };
        let (li, ri) = (id(lhs, key(rhs))?, id(rhs, key(lhs))?);
        match (li, ri) {
            (Some((l, le, lo)), Some((r, re, ro))) => {
                if le != re || lo != ro {
                    return unsupported("keys of different shapes compared");
                }
                let eq = format!("(= {l} {r})");
                match op {
                    CmpOp::Eq => Ok(eq),
                    CmpOp::Ne => Ok(format!("(not {eq})")),
                    _ => unsupported("rows compared by order"),
                }
            }
            (None, None) => {
                let index = |o: &Op| match o {
                    Op::Var(v, _) => v.clone(),
                    Op::Lit(v) => v.0.to_string(),
                    Op::Num(n) => n.clone(),
                    Op::Row(r, _) => r.clone(),
                };
                Ok(match op {
                    CmpOp::Ne => format!("(not (= {} {}))", index(lhs), index(rhs)),
                    _ => format!("({} {} {})", ordered(op), index(lhs), index(rhs)),
                })
            }
            _ => unsupported("a key compared with a plain value"),
        }
    }
}

fn ordered(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "=",
        CmpOp::Ne => "distinct",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
    }
}

fn and(parts: Vec<String>) -> String {
    match parts.len() {
        0 => "true".to_string(),
        1 => parts.into_iter().next().unwrap(),
        _ => format!("(and {})", parts.join(" ")),
    }
}

fn or(parts: Vec<String>) -> String {
    match parts.len() {
        0 => "false".to_string(),
        1 => parts.into_iter().next().unwrap(),
        _ => format!("(or {})", parts.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::types::Bounds;

    #[test]
    fn a_key_against_a_nullable_number_is_unsupported() {
        let ts = TransitionSystem::empty(Bounds::VERIFY);
        let enc = Enc { ts: &ts };
        let key = Op::Var(
            "(v0 p0)".to_string(),
            Domain::Instance {
                parent: EntId::at(0),
                optional: true,
            },
        );
        let n = Op::Var("(v1 p1)".to_string(), Domain::NullableInt);
        for (l, r) in [(&key, &n), (&n, &key)] {
            assert!(enc.compare(l, CmpOp::Eq, r).is_err());
        }
    }

    #[test]
    fn an_empty_disjunction_is_false() {
        assert_eq!(or(Vec::new()), "false");
    }
}
