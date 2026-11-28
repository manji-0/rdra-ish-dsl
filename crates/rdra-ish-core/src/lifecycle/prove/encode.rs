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

use super::smt::{self, and, eq, not, or, Command, Expr, Fun, Rel, Script, Sort};
use crate::lifecycle::types::{
    Action, At, Birth, CmpOp, Domain, Effect, EntId, GuardWhy, Numbers, ParamRole, Pred, Rhs,
    Start, Term, Value, VarId, VarKind,
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
    pub commands: Script,
    pub loose: Vec<String>,
}

pub(super) struct Enc<'a> {
    pub ts: &'a TransitionSystem,
}

enum Op {
    /// A var read: its SMT term and domain.
    Var(Expr, Domain),
    Lit(Value),
    /// An integer term (a literal or `now`).
    Num(Expr),
    /// A row of an entity (`Term::Index`).
    Row(Expr, EntId),
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

pub(super) fn nil(e: EntId) -> Expr {
    Expr::sym(format!("nil{}", e.index()), Sort::Ent(e))
}

fn zero(e: EntId) -> Expr {
    Expr::sym(format!("zero{}", e.index()), Sort::Ent(e))
}

fn now(frame: Frame) -> Expr {
    let name = match frame {
        Frame::Pre => "now",
        Frame::Post => "now_n",
    };
    Expr::sym(name, Sort::Int)
}

/// The row of entity `e` that a step's param `p` names.
pub(super) fn param(p: usize, e: EntId) -> Expr {
    Expr::sym(format!("p{p}"), Sort::Ent(e))
}

/// The row of entity `e` that a formula's binder `b` names.
pub(super) fn bound(b: usize, e: EntId) -> Expr {
    Expr::sym(format!("b{b}"), Sort::Ent(e))
}

impl Enc<'_> {
    /// `term` lies in `var`'s domain: `0..size` for a finite domain, any non-negative number
    /// for an Int domain.
    fn in_domain(&self, var: VarId, term: &Expr) -> Expr {
        let domain = &self.ts.var(var).domain;
        let from_zero = smt::rel(Rel::Le, Expr::int(0), term.clone());
        match domain.is_numeric() {
            true => from_zero,
            false => {
                let size = Expr::int(domain.size(self.ts) as i64);
                and(vec![from_zero, smt::rel(Rel::Lt, term.clone(), size)])
            }
        }
    }

    /// The parent and optionality of a key var.
    fn key(&self, var: VarId) -> Option<(EntId, bool)> {
        match self.ts.var(var).domain {
            Domain::Instance { parent, optional } => Some((parent, optional)),
            _ => None,
        }
    }

    fn var_sort(&self, var: VarId) -> Sort {
        match self.key(var) {
            Some((parent, _)) => Sort::Ent(parent),
            None => Sort::Int,
        }
    }

    /// The entity whose rows a var is a function of.
    pub(super) fn owner(&self, var: VarId) -> Option<EntId> {
        self.ts.var_entity(var)
    }

    /// The function that holds a row var in `frame`.
    fn func(&self, var: VarId, frame: Frame) -> Fun {
        let e = self.owner(var).expect("a row var has an entity");
        let name = match frame {
            Frame::Pre => format!("v{}", var.index()),
            Frame::Post => format!("v{}_n", var.index()),
        };
        Fun {
            name,
            args: vec![Sort::Ent(e)],
            ret: self.var_sort(var),
        }
    }

    /// The row `x` of `var`'s entity that a fact or a successor function ranges over.
    fn x(&self, var: VarId) -> Expr {
        let e = self.owner(var).expect("a row var has an entity");
        Expr::sym("x", Sort::Ent(e))
    }

    /// `var` of `row` in `frame`.
    pub(super) fn read(&self, var: VarId, frame: Frame, row: &Expr) -> Expr {
        match self.ts.var(var).kind {
            VarKind::Now => now(frame),
            _ => self.func(var, frame).apply(vec![row.clone()]),
        }
    }

    fn exists(&self, e: EntId, frame: Frame, row: &Expr) -> Expr {
        self.read(self.ts.entity(e).exists, frame, row)
    }

    fn present(&self, e: EntId, frame: Frame, row: &Expr) -> Expr {
        eq(self.exists(e, frame, row), Expr::int(1))
    }

    /// A value of `var`'s domain as a term of its sort.
    fn value(&self, var: VarId, v: Value) -> R<Expr> {
        match self.key(var) {
            None => Ok(Expr::int(i64::from(v.0))),
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
    pub(super) fn declarations(&self) -> Script {
        let ts = self.ts;
        let mut out = Script::default();
        for e in ts.entity_ids() {
            out.push(Command::DeclareSort(e));
            out.push(Command::DeclareConst(nil(e)));
            out.push(Command::DeclareConst(zero(e)));
            out.push(Command::Assert(smt::distinct(vec![nil(e), zero(e)])));
        }
        for v in ts.var_ids() {
            let decl = ts.var(v);
            if let VarKind::Now = decl.kind {
                let now = now(Frame::Pre);
                out.push(Command::DeclareConst(now.clone()));
                out.push(Command::Assert(smt::rel(Rel::Le, Expr::int(0), now)));
                continue;
            }
            let (f, x) = (self.func(v, Frame::Pre), self.x(v));
            let fx = f.apply(vec![x.clone()]);
            out.push(Command::DeclareFun(f));
            match self.key(v) {
                None => {
                    let fact = smt::forall(vec![x], self.in_domain(v, &fx));
                    out.push(Command::Assert(fact));
                }
                Some((parent, false)) => {
                    let fact = smt::forall(vec![x], not(eq(fx, nil(parent))));
                    out.push(Command::Assert(fact));
                }
                Some((_, true)) => {}
            }
        }
        for e in ts.entity_ids() {
            let absent = eq(self.exists(e, Frame::Pre, &nil(e)), Expr::int(0));
            out.push(Command::Assert(absent));
        }
        out
    }

    /// The initial states: every var at base, except that each slot of a pre-existing entity
    /// is present and its columns and keys start as `Start` says.
    pub(super) fn initial(&self) -> R<Script> {
        let ts = self.ts;
        let mut out = Script::default();
        for v in ts.var_ids() {
            let decl = ts.var(v);
            let base = self.value(v, decl.base)?;
            let Some(e) = self.owner(v) else {
                out.push(Command::Assert(eq(now(Frame::Pre), base)));
                continue;
            };
            let ent = ts.entity(e);
            let (f, x) = (self.func(v, Frame::Pre), self.x(v));
            let fx = f.apply(vec![x.clone()]);
            let at_base = eq(fx.clone(), base);
            let start = match (ent.birth, &decl.kind) {
                (Birth::Created, _) => None,
                (Birth::PreExisting, VarKind::Exists(_)) => Some(eq(fx, Expr::int(1))),
                (Birth::PreExisting, _) if ent.columns.contains(&v) || ent.owners.contains(&v) => {
                    match decl.start {
                        Start::Base => None,
                        Start::Any => Some(Expr::bool(true)),
                        Start::Identity => {
                            let (parent, _) = self.key(v).ok_or_else(|| {
                                Unsupported("an identity start of a non-key".to_string())
                            })?;
                            let y = Expr::sym("y", Sort::Ent(e));
                            let fy = f.apply(vec![y.clone()]);
                            let same = and(vec![
                                not(eq(x.clone(), nil(e))),
                                not(eq(y.clone(), nil(e))),
                                eq(fx.clone(), fy),
                            ]);
                            let injective = smt::implies(same, eq(x.clone(), y.clone()));
                            out.push(Command::Assert(smt::forall(vec![x.clone(), y], injective)));
                            Some(not(eq(fx, nil(parent))))
                        }
                    }
                }
                (Birth::PreExisting, _) => None,
            };
            let fact = match start {
                None => at_base,
                Some(start) => smt::ite(eq(x.clone(), nil(e)), at_base, start),
            };
            out.push(Command::Assert(smt::forall(vec![x], fact)));
        }
        Ok(out)
    }

    /// The formula in `frame`, closed over its binders.
    pub(super) fn holds(&self, f: &Formula, frame: Frame) -> R<Expr> {
        let scope = Scope {
            frame,
            params: &[],
            bound: f.binders.clone(),
        };
        let mut body = self.pred(&f.body, &scope)?;
        for (i, e) in f.binders.iter().enumerate().rev() {
            let b = bound(i, *e);
            let row = match f.all_rows {
                true => not(eq(b.clone(), nil(*e))),
                false => self.present(*e, frame, &b),
            };
            body = smt::forall(vec![b], smt::implies(row, body));
        }
        Ok(body)
    }

    /// Commands that declare one row per binder (`b0`, ..) and assert that they violate the
    /// formula in `frame`.
    pub(super) fn violation(&self, f: &Formula, frame: Frame) -> R<Script> {
        let scope = Scope {
            frame,
            params: &[],
            bound: f.binders.clone(),
        };
        let body = self.pred(&f.body, &scope)?;
        let mut out = Script::default();
        let mut parts = Vec::new();
        for (i, e) in f.binders.iter().enumerate() {
            let b = bound(i, *e);
            out.push(Command::DeclareConst(b.clone()));
            parts.push(not(eq(b.clone(), nil(*e))));
            if !f.all_rows {
                parts.push(self.present(*e, frame, &b));
            }
        }
        parts.push(not(body));
        out.push(Command::Assert(and(parts)));
        Ok(out)
    }

    /// One step of `action` from the pre-state to the successor (`moves.rs`): params bound
    /// by role, guards on both states, effects in order, deletes, then the choices.
    pub(super) fn step(&self, action: &Action) -> Step {
        let ts = self.ts;
        let mut cmds = Script::default();
        let mut loose = Vec::new();
        let entities: Vec<EntId> = action.params.iter().map(|p| p.entity).collect();
        let row = |p: u8| param(usize::from(p), entities[usize::from(p)]);
        for (i, p) in action.params.iter().enumerate() {
            let (name, e) = (param(i, p.entity), p.entity);
            cmds.push(Command::DeclareConst(name.clone()));
            cmds.push(Command::Assert(not(eq(name.clone(), nil(e)))));
            let exists = self.exists(e, Frame::Pre, &name);
            match p.role {
                ParamRole::New => cmds.push(Command::Assert(eq(exists, Expr::int(0)))),
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                    cmds.push(Command::Assert(eq(exists, Expr::int(1))))
                }
                ParamRole::Earlier { .. } => {}
            }
        }

        // The successor of each var, as a term over the row `x`, built in effect order.
        let mut next: BTreeMap<VarId, Expr> = BTreeMap::new();
        let mut extra = 0usize;
        let mut fresh = |cmds: &mut Script, sort: Sort| {
            let c = Expr::sym(format!("c{extra}"), sort);
            extra += 1;
            cmds.push(Command::DeclareConst(c.clone()));
            c
        };
        let cur = |next: &BTreeMap<VarId, Expr>, v: VarId| {
            next.get(&v)
                .cloned()
                .unwrap_or_else(|| self.func(v, Frame::Pre).apply(vec![self.x(v)]))
        };
        let set =
            |next: &mut BTreeMap<VarId, Expr>, v: VarId, p: u8, cond: Option<Expr>, val: Expr| {
                let old = cur(next, v);
                let at = eq(self.x(v), row(p));
                let when = match cond {
                    None => at,
                    Some(c) => and(vec![at, c]),
                };
                next.insert(v, smt::ite(when, val, old));
            };
        let mut now_next = now(Frame::Pre);
        let mut deleted: Vec<u8> = Vec::new();
        // Choices fork after the other effects: (var, param, condition, choice).
        let mut forks: Vec<(VarId, u8, Option<Expr>, Expr)> = Vec::new();
        for eff in &action.effects {
            match eff {
                Effect::Create { param: p } => {
                    let e = entities[usize::from(*p)];
                    set(&mut next, ts.entity(e).exists, *p, None, Expr::int(1));
                }
                Effect::Delete { param: p } => deleted.push(*p),
                Effect::Assign {
                    var, param: p, rhs, ..
                } => {
                    let val = match rhs {
                        Rhs::Lit(v) => self.value(*var, *v),
                        Rhs::Param(q) => match self.key(*var) {
                            Some(_) => Ok(row(*q)),
                            None => Err(Unsupported("a slot index into a non-key".to_string())),
                        },
                    };
                    match val {
                        Ok(val) => set(&mut next, *var, *p, None, val),
                        Err(Unsupported(why)) => {
                            loose.push(format!("{} of {}: {why}", ts.var_name(*var), action.name));
                            let h = self.havoc(&mut cmds, &mut fresh, *var);
                            set(&mut next, *var, *p, None, h);
                        }
                    }
                }
                Effect::Transition {
                    var,
                    param: p,
                    arms,
                    ..
                } => {
                    let from = self.read(*var, Frame::Pre, &row(*p));
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
                            set(&mut next, *var, *p, None, h);
                            continue;
                        };
                        let cond = eq(from.clone(), fv);
                        match &tvs[..] {
                            [to] => set(&mut next, *var, *p, Some(cond), to.clone()),
                            _ => multi.push((cond, tvs)),
                        }
                    }
                    if !multi.is_empty() {
                        let c = fresh(&mut cmds, self.var_sort(*var));
                        let when = or(multi.iter().map(|(cond, _)| cond.clone()).collect());
                        let pick = multi
                            .into_iter()
                            .map(|(cond, tvs)| {
                                let any = tvs.into_iter().map(|t| eq(c.clone(), t)).collect();
                                and(vec![cond, or(any)])
                            })
                            .collect();
                        cmds.push(Command::Assert(smt::implies(when.clone(), or(pick))));
                        forks.push((*var, *p, Some(when), c));
                    }
                }
                Effect::Choose {
                    var,
                    param: p,
                    values,
                    numbers,
                } => match values
                    .iter()
                    .map(|c| Ok((c.value, self.value(*var, c.value)?)))
                    .collect::<R<Vec<_>>>()
                {
                    Ok(vals) => {
                        let c = fresh(&mut cmds, self.var_sort(*var));
                        cmds.push(Command::Assert(Self::one_of(numbers, vals, &c)));
                        forks.push((*var, *p, None, c));
                    }
                    Err(Unsupported(why)) => {
                        loose.push(format!("{} of {}: {why}", ts.var_name(*var), action.name));
                        let h = self.havoc(&mut cmds, &mut fresh, *var);
                        forks.push((*var, *p, None, h));
                    }
                },
                Effect::Tick => now_next = smt::add(now(Frame::Pre), Expr::int(1)),
            }
        }
        for &p in &deleted {
            let decl = ts.entity(entities[usize::from(p)]);
            // A deleted row keeps its keys, as in the native step.
            for &v in &decl.columns {
                match self.value(v, ts.var(v).base) {
                    Ok(base) => set(&mut next, v, p, None, base),
                    Err(_) => unreachable!("a base value is always expressible"),
                }
            }
            set(&mut next, decl.exists, p, None, Expr::int(2));
        }
        for (v, p, cond, c) in forks {
            if !deleted.contains(&p) {
                set(&mut next, v, p, cond, c);
            }
        }

        for v in ts.var_ids() {
            if self.owner(v).is_none() {
                let f = Fun {
                    name: "now_n".to_string(),
                    args: Vec::new(),
                    ret: Sort::Int,
                };
                cmds.push(Command::DefineFun(f, Vec::new(), now_next.clone()));
                continue;
            }
            cmds.push(Command::DefineFun(
                self.func(v, Frame::Post),
                vec![self.x(v)],
                cur(&next, v),
            ));
        }

        let scope = Scope {
            frame: Frame::Pre,
            params: &entities,
            bound: Vec::new(),
        };
        // The clock's guard is the bound's top; without a top, `Tick` always runs.
        for g in action.guards.iter().filter(|g| g.why != GuardWhy::Clock) {
            match self.pred(&g.pred, &scope) {
                Ok(p) => cmds.push(Command::Assert(p)),
                Err(Unsupported(why)) => loose.push(format!("a guard of {}: {why}", action.name)),
            }
        }
        Step {
            commands: cmds,
            loose,
        }
    }

    /// `c` is one of the values a choice lists (each with its term), or, for a choice of
    /// every number (`Numbers`), any index from its `from` or one of the listed values below
    /// it: the list holds the numbers only up to the run's top, and a proof holds for every
    /// Int range. A value below `from` is one a `.when`-guarded line of the same column
    /// asks for beside the numbers, as `null` beside `present` (`build/sets.rs::guarded_set`),
    /// which the step must still be able to write.
    fn one_of(numbers: &Option<Numbers>, listed: Vec<(Value, Expr)>, c: &Expr) -> Expr {
        let is = |t: Expr| eq(c.clone(), t);
        match numbers {
            Some(n) => {
                let every = smt::rel(Rel::Le, Expr::int(i64::from(n.from.0)), c.clone());
                let below = listed.into_iter().filter(|(v, _)| *v < n.from);
                any(std::iter::once(every)
                    .chain(below.map(|(_, t)| is(t)))
                    .collect())
            }
            None => any(listed.into_iter().map(|(_, t)| is(t)).collect()),
        }
    }

    /// A fresh constant of `var`'s domain.
    fn havoc(
        &self,
        cmds: &mut Script,
        fresh: &mut impl FnMut(&mut Script, Sort) -> Expr,
        var: VarId,
    ) -> Expr {
        let h = fresh(cmds, self.var_sort(var));
        match self.key(var) {
            None => cmds.push(Command::Assert(self.in_domain(var, &h))),
            Some((parent, false)) => cmds.push(Command::Assert(not(eq(h.clone(), nil(parent))))),
            Some((_, true)) => {}
        }
        h
    }

    fn pred(&self, pred: &Pred, scope: &Scope) -> R<Expr> {
        Ok(match pred {
            Pred::True => Expr::bool(true),
            Pred::False => Expr::bool(false),
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
                any(parts)
            }
            Pred::Not(p) => not(self.pred(p, scope)?),
            Pred::And(ps) => all(ps.iter().map(|p| self.pred(p, scope)).collect::<R<_>>()?),
            Pred::Or(ps) => any(ps.iter().map(|p| self.pred(p, scope)).collect::<R<_>>()?),
            Pred::Forall { entity, body } | Pred::Exists { entity, body } => {
                let b = bound(scope.bound.len(), *entity);
                let mut inner = Scope {
                    frame: scope.frame,
                    params: scope.params,
                    bound: scope.bound.clone(),
                };
                inner.bound.push(*entity);
                let body = self.pred(body, &inner)?;
                let present = self.present(*entity, scope.frame, &b);
                match pred {
                    Pred::Forall { .. } => smt::forall(vec![b], smt::implies(present, body)),
                    _ => smt::exists(vec![b], and(vec![present, body])),
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
                    CmpOp::Le => not(at_least(n + 1)?),
                    CmpOp::Lt => not(at_least(n)?),
                    CmpOp::Eq => and(vec![at_least(n)?, not(at_least(n + 1)?)]),
                    CmpOp::Ne => not(and(vec![at_least(n)?, not(at_least(n + 1)?)])),
                }
            }
        })
    }

    /// "At least `k` present rows of `entity` satisfy `body`": `k` distinct rows, each
    /// present and satisfying it. The body reads its row as `b<depth>`; each witness binds that
    /// name with `let`. That is the count, not an approximation of it, in either polarity.
    /// Past `COUNT_WITNESSES` witnesses the formula is out of scope.
    fn at_least(&self, entity: EntId, body: &Pred, k: u16, scope: &Scope) -> R<Expr> {
        if k == 0 {
            return Ok(Expr::bool(true));
        }
        if k > COUNT_WITNESSES {
            return Err(Unsupported(format!(
                "a count of {k} or more rows (at most {COUNT_WITNESSES} unroll)"
            )));
        }
        let depth = scope.bound.len();
        let b = bound(depth, entity);
        let mut inner = Scope {
            frame: scope.frame,
            params: scope.params,
            bound: scope.bound.clone(),
        };
        inner.bound.push(entity);
        let body = self.pred(body, &inner)?;
        let present = self.present(entity, scope.frame, &b);
        let witnesses: Vec<Expr> = (0..k)
            .map(|i| Expr::sym(format!("b{depth}_{i}"), Sort::Ent(entity)))
            .collect();
        let mut parts: Vec<Expr> = Vec::new();
        if k > 1 {
            parts.push(smt::distinct(witnesses.clone()));
        }
        let witnessed = and(vec![present, body]);
        parts.extend(
            witnesses
                .iter()
                .map(|w| smt::let_in(vec![(b.clone(), w.clone())], witnessed.clone())),
        );
        Ok(smt::exists(witnesses, all(parts)))
    }

    fn row(&self, at: At, scope: &Scope) -> (Expr, EntId, Frame) {
        let (name, e, frame) = match at {
            At::Param(p) => (usize::from(p), scope.params[usize::from(p)], Frame::Pre),
            At::Next(p) => (usize::from(p), scope.params[usize::from(p)], Frame::Post),
            At::Bound(b) => {
                let e = scope.bound[usize::from(b)];
                return (bound(usize::from(b), e), e, scope.frame);
            }
        };
        (param(name, e), e, frame)
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
            Term::Int(n) => Op::Num(Expr::int(*n)),
            Term::Index(at) => {
                let (row, e, _) = self.row(*at, scope);
                Op::Row(row, e)
            }
            Term::Now => Op::Num(now(scope.frame)),
        })
    }

    /// `expr::compare` over terms: a key against a row, then numbers (where `null` equals only
    /// `null` and is never ordered), then domain indices.
    fn compare(&self, lhs: &Op, op: CmpOp, rhs: &Op) -> R<Expr> {
        fn unsupported<T>(what: &str) -> R<T> {
            Err(Unsupported(what.to_string()))
        }
        if let (Op::Row(row, e), Op::Var(v, d)) | (Op::Var(v, d), Op::Row(row, e)) = (lhs, rhs) {
            return match d {
                Domain::Instance { parent, .. } if parent == e => {
                    let same = eq(v.clone(), row.clone());
                    Ok(match op {
                        CmpOp::Eq => same,
                        CmpOp::Ne => not(same),
                        _ => Expr::bool(false),
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
            let num = |o: &Op| -> R<(Expr, Expr)> {
                let no = Expr::bool(false);
                Ok(match o {
                    Op::Num(n) => (no, n.clone()),
                    Op::Row(..) => return unsupported("a row compared as a number"),
                    // A var reads through its own numeric domain, else the other side's.
                    Op::Var(_, Domain::Instance { .. }) => {
                        return unsupported("a key compared as a number")
                    }
                    Op::Var(v, _) => match numeric(o).or(domain.clone()) {
                        Some(Domain::NullableInt) => (
                            eq(v.clone(), Expr::int(0)),
                            smt::sub(v.clone(), Expr::int(1)),
                        ),
                        _ => (no, v.clone()),
                    },
                    Op::Lit(v) => match &domain {
                        Some(Domain::NullableInt) if v.0 == 0 => (Expr::bool(true), Expr::int(0)),
                        Some(Domain::NullableInt) => (no, Expr::int(i64::from(v.0) - 1)),
                        _ => (no, Expr::int(i64::from(v.0))),
                    },
                })
            };
            let ((ln, l), (rn, r)) = (num(lhs)?, num(rhs)?);
            let both = and(vec![not(ln.clone()), not(rn.clone())]);
            let equal = || {
                let nulls = and(vec![ln.clone(), rn.clone()]);
                or(vec![
                    nulls,
                    and(vec![both.clone(), eq(l.clone(), r.clone())]),
                ])
            };
            return Ok(match op {
                CmpOp::Eq => equal(),
                CmpOp::Ne => not(equal()),
                _ => and(vec![both.clone(), ordered(op, l.clone(), r.clone())]),
            });
        }
        // Domain indices; a key's values are rows of its parent.
        let key = |o: &Op| match o {
            Op::Var(_, Domain::Instance { parent, optional }) => Some((*parent, *optional)),
            _ => None,
        };
        let id = |o: &Op, other: Option<(EntId, bool)>| -> R<Option<(Expr, EntId, bool)>> {
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
                let same = eq(l, r);
                match op {
                    CmpOp::Eq => Ok(same),
                    CmpOp::Ne => Ok(not(same)),
                    _ => unsupported("rows compared by order"),
                }
            }
            (None, None) => {
                let index = |o: &Op| match o {
                    Op::Var(v, _) => v.clone(),
                    Op::Lit(v) => Expr::int(i64::from(v.0)),
                    Op::Num(n) => n.clone(),
                    Op::Row(r, _) => r.clone(),
                };
                Ok(match op {
                    CmpOp::Ne => not(eq(index(lhs), index(rhs))),
                    _ => ordered(op, index(lhs), index(rhs)),
                })
            }
            _ => unsupported("a key compared with a plain value"),
        }
    }
}

fn ordered(op: CmpOp, a: Expr, b: Expr) -> Expr {
    match op {
        CmpOp::Eq => eq(a, b),
        CmpOp::Ne => smt::distinct(vec![a, b]),
        CmpOp::Lt => smt::rel(Rel::Lt, a, b),
        CmpOp::Le => smt::rel(Rel::Le, a, b),
        CmpOp::Gt => smt::rel(Rel::Gt, a, b),
        CmpOp::Ge => smt::rel(Rel::Ge, a, b),
    }
}

/// The conjunction of `parts`: `true` for none, the part itself for one.
fn all(parts: Vec<Expr>) -> Expr {
    match parts.len() {
        0 => Expr::bool(true),
        1 => parts.into_iter().next().unwrap(),
        _ => and(parts),
    }
}

/// The disjunction of `parts`: `false` for none, the part itself for one.
fn any(parts: Vec<Expr>) -> Expr {
    match parts.len() {
        0 => Expr::bool(false),
        1 => parts.into_iter().next().unwrap(),
        _ => or(parts),
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
        let parent = EntId::at(0);
        let key = Op::Var(
            Expr::sym("k", Sort::Ent(parent)),
            Domain::Instance {
                parent,
                optional: true,
            },
        );
        let n = Op::Var(Expr::sym("n", Sort::Int), Domain::NullableInt);
        for (l, r) in [(&key, &n), (&n, &key)] {
            assert!(enc.compare(l, CmpOp::Eq, r).is_err());
        }
    }

    /// Every count operator, and an Int literal below zero, which no sample reaches: the text
    /// the `format!` encoder wrote for them (#426).
    #[test]
    fn counts_and_negative_numbers_print_as_before() {
        let ts = crate::lifecycle::tests::system_from(
            "entity Member \"m\" {\n  id: Int @pk\n}\n\
             entity Loan \"l\" {\n  id: Int @pk\n  status: Enum(open, closed) @default(open)\n}\n\
             usecase Join \"j\"\nusecase Borrow \"b\"\n\
             creates(Join, Member)\ncreates(Borrow, Loan)\nupdates(Borrow, Member)\n\
             relate(Loan, Member, 0..3:1)\n\
             invariant(Member).at_most(3, Loan.status == open)\n",
            Bounds::VERIFY,
        );
        let inv = ts
            .invariants()
            .iter()
            .find(|i| matches!(i.body, Pred::Count { .. }))
            .expect("a count rule");
        let Pred::Count { entity, body, .. } = &inv.body else {
            unreachable!()
        };
        let enc = Enc { ts: &ts };
        let scope = Scope {
            frame: Frame::Pre,
            params: &[],
            bound: inv.binders.clone(),
        };
        let al = |k| enc.at_least(*entity, body, k, &scope).unwrap().to_string();
        let count = |op| {
            let p = Pred::Count {
                entity: *entity,
                body: body.clone(),
                op,
                n: 2,
            };
            enc.pred(&p, &scope).unwrap().to_string()
        };
        assert_eq!(count(CmpOp::Ge), al(2));
        assert_eq!(count(CmpOp::Gt), al(3));
        assert_eq!(count(CmpOp::Le), format!("(not {})", al(3)));
        assert_eq!(count(CmpOp::Lt), format!("(not {})", al(2)));
        assert_eq!(count(CmpOp::Eq), format!("(and {} (not {}))", al(2), al(3)));
        assert_eq!(
            count(CmpOp::Ne),
            format!("(not (and {} (not {})))", al(2), al(3))
        );
        assert!(al(2).starts_with("(exists ((b1_0 E"), "{}", al(2));
        assert_eq!(al(0), "true");
        assert!(enc.at_least(*entity, body, 9, &scope).is_err());
        let Op::Num(n) = enc.term(&Term::Int(-5), &scope).unwrap() else {
            panic!()
        };
        assert_eq!(n.to_string(), "(- 5)");
    }

    #[test]
    fn an_empty_disjunction_is_false() {
        assert_eq!(any(Vec::new()).to_string(), "false");
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;
    use crate::lifecycle::types::{ModelRef, Site};

    /// The largest domain and the largest `from`.
    const SIZE: u8 = 4;
    const FROM: u8 = 3;

    /// What the step may write for a choice: within the run's domain the values the native
    /// step forks on (`explore/moves.rs::each_successor`, one successor per value listed), and
    /// past its top every number from `from` (`Numbers`), since a proof holds for every Int
    /// range.
    fn admitted(values: &[bool], numbers: &Option<Numbers>, size: u8, c: u8) -> bool {
        match c < size {
            true => values[usize::from(c)],
            false => numbers.as_ref().is_some_and(|n| Value(c) >= n.from),
        }
    }

    /// `one_of` holds of a candidate exactly when the choice admits it. The choice is one the
    /// builder makes for a non-key column of a domain up to `SIZE` values: any non-empty set of
    /// listed values and, for a choice of every number from `from` (`present` from 1, a copy from 0),
    /// beside them the expansion of the numbers from `from` to the top, as
    /// `build/sets.rs::guarded_set` joins a `.when`-guarded `null` and a `.when`-guarded
    /// `present` of one column. The `.when` conditions are guards of the action, not part of
    /// the choice, and a key column's values are not numbers.
    #[kani::proof]
    #[kani::unwind(6)]
    fn one_of_admits_what_the_step_forks_on() {
        let size: u8 = kani::any();
        kani::assume((1..=SIZE).contains(&size));
        let numbers = kani::any::<bool>().then(|| {
            let from: u8 = kani::any();
            kani::assume(from <= FROM);
            Numbers {
                from: Value(from),
                conds: Vec::new(),
                site: Site {
                    fact: ModelRef::ColumnEffect(0),
                    span: None,
                },
            }
        });
        // The list: `SIZE` values of the domain, so a set of up to `SIZE` values with
        // repeats, holding the expansion of the numbers. A list of fixed length keeps CBMC
        // from tracking a list of every length; a repeat changes no set `one_of` reads.
        let mut values = [false; SIZE as usize];
        let mut terms = Vec::with_capacity(usize::from(SIZE));
        for _ in 0..SIZE {
            let v: u8 = kani::any();
            kani::assume(v < size);
            values[usize::from(v)] = true;
            terms.push((Value(v), Expr::int(i64::from(v))));
        }
        if let Some(n) = &numbers {
            kani::assume((n.from.0..size).all(|v| values[usize::from(v)]));
        }
        let listed_below = numbers
            .as_ref()
            .is_some_and(|n| (0..n.from.0.min(size)).any(|v| values[usize::from(v)]));
        let encoded = Enc::one_of(&numbers, terms, &Expr::sym("c", Sort::Int));

        let c: u8 = kani::any();
        let holds = encoded.holds_at("c", i64::from(c));
        assert_eq!(holds, admitted(&values, &numbers, size, c));

        kani::cover!(listed_below && holds && numbers.as_ref().is_some_and(|n| Value(c) < n.from));
        kani::cover!(numbers.is_none() && holds);
        kani::cover!(numbers.is_none() && !holds);
        kani::cover!(c >= size && holds);
        kani::cover!(c < size && !holds);
    }
}
