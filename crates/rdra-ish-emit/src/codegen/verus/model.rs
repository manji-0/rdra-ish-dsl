//! `model.rs`: the transition system as Verus spec. The state holds each entity's present rows
//! by id, with their tracked columns and keys; each action is an `enabled` and a `step`; each
//! state rule is a spec function; the inductive set is the rules and one key lemma per key;
//! and each action has a lemma that it keeps the set, with the hints a count needs.
//!
//! Unlike the BFS, the spec does not number rows by slot: a created row takes any absent id,
//! and a deleted id may be created again. Every BFS move is a move of the spec up to the ids
//! (the differential harness checks it); the spec's extra moves are the same moves at other ids.

use std::cell::RefCell;

use rdra_ish_core::lifecycle::{Action, At, CmpOp, Effect, EntId, Pred, Term, Value, VarId};

use super::{Ctx, MTy};

/// A quantifier of a guard or a rule, printed once as named spec functions over a row of its
/// entity (`r`, with id `x`) and its free terms (`f0`, ..).
#[derive(Clone)]
pub(super) struct Atom {
    pub name: String,
    pub kind: Q,
    pub ent: EntId,
    /// The body over `r`, `x`, and `f<i>`.
    pub body: String,
    /// The free terms, as the outer predicate reads them, and their spec types.
    pub frees: Vec<(Term, String)>,
    /// The body as the transition system holds it, and its binder's index.
    pub pred: Pred,
    pub depth: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Q {
    Forall,
    Exists,
    Count,
}

/// Where a predicate's rows and quantifiers are read.
pub(super) struct Env<'e> {
    /// Per action param: its row and its id.
    pub param: &'e dyn Fn(u8) -> (String, String),
    /// Per binder of a rule (and of an atom's body): its row and its id.
    pub bound: Vec<(String, String)>,
    /// Terms printed as given (an atom's free terms).
    pub subst: Vec<(Term, String)>,
    /// How a quantifier over an entity prints, given the atom and its free terms' values.
    pub quant: &'e dyn Fn(&Atom, &[String]) -> String,
}

/// The atoms every printer shares, numbered in the order they are met.
#[derive(Default)]
pub(super) struct Atoms {
    pub list: RefCell<Vec<Atom>>,
}

enum Ty {
    Var(VarId),
    Id,
    Lit(Value),
}

impl Ctx<'_> {
    fn term(&self, t: &Term, env: &Env) -> Result<(String, Ty), String> {
        if let Some((_, s)) = env.subst.iter().find(|(x, _)| x == t) {
            let ty = match t {
                Term::Slot { var, .. } => Ty::Var(*var),
                _ => Ty::Id,
            };
            return Ok((s.clone(), ty));
        }
        let at = |a: &At| -> Result<(String, String), String> {
            match a {
                At::Param(k) => Ok((env.param)(*k)),
                At::Bound(j) => env
                    .bound
                    .get(usize::from(*j))
                    .cloned()
                    .ok_or_else(|| "a bound row out of scope".to_string()),
                At::Next(_) => Err("a condition on the successor is not covered".to_string()),
            }
        };
        match t {
            Term::Slot { var, at: a } => {
                let (row, _) = at(a)?;
                let (_, f) = self.field(*var);
                Ok((format!("{row}.{}", f.name), Ty::Var(*var)))
            }
            Term::Index(a) => Ok((at(a)?.1, Ty::Id)),
            Term::Lit(v) => Ok((String::new(), Ty::Lit(*v))),
            Term::Int(_) | Term::Now => Err("Int values and `now` are not covered".to_string()),
        }
    }

    fn cmp(&self, a: &Term, op: CmpOp, b: &Term, env: &Env) -> Result<String, String> {
        let (x, tx) = self.term(a, env)?;
        let (y, ty) = self.term(b, env)?;
        let (x, y) = match (tx, ty) {
            (Ty::Var(v), Ty::Lit(l)) => (x, self.lit(v, l)),
            (Ty::Lit(l), Ty::Var(v)) => (self.lit(v, l), y),
            (Ty::Lit(_), Ty::Lit(_)) => return Err("a comparison of two values".to_string()),
            _ => (x, y),
        };
        let op = match op {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            _ => return Err("an ordering comparison is not covered".to_string()),
        };
        Ok(format!("{x} {op} {y}"))
    }

    /// `p` as a spec expression.
    pub(super) fn pred(&self, p: &Pred, env: &Env, atoms: &Atoms) -> Result<String, String> {
        Ok(match p {
            Pred::True => "true".to_string(),
            Pred::False => "false".to_string(),
            Pred::Cmp(a, op, b) => format!("({})", self.cmp(a, *op, b, env)?),
            Pred::In(t, values) => {
                let parts: Result<Vec<String>, String> = values
                    .iter()
                    .map(|v| self.cmp(t, CmpOp::Eq, v, env))
                    .collect();
                format!("({})", parts?.join(" || "))
            }
            Pred::Not(inner) => format!("!{}", self.pred(inner, env, atoms)?),
            Pred::And(parts) => join(self, parts, " && ", "true", env, atoms)?,
            Pred::Or(parts) => join(self, parts, " || ", "false", env, atoms)?,
            Pred::Forall { entity, body } => self.quant(Q::Forall, *entity, body, env, atoms)?,
            Pred::Exists { entity, body } => self.quant(Q::Exists, *entity, body, env, atoms)?,
            Pred::Count {
                entity,
                body,
                op,
                n,
            } => {
                let op = match op {
                    CmpOp::Eq => "==",
                    CmpOp::Ne => "!=",
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                };
                format!(
                    "({} {op} {n})",
                    self.quant(Q::Count, *entity, body, env, atoms)?
                )
            }
        })
    }

    fn quant(
        &self,
        kind: Q,
        ent: EntId,
        body: &Pred,
        env: &Env,
        atoms: &Atoms,
    ) -> Result<String, String> {
        let depth = env.bound.len();
        let inner = |a: &At| matches!(a, At::Bound(j) if usize::from(*j) == depth);
        let mut frees: Vec<Term> = Vec::new();
        for t in body.terms() {
            let free = match t {
                Term::Slot { at, .. } | Term::Index(at) => !inner(at),
                _ => false,
            };
            if free && !frees.contains(t) {
                frees.push(t.clone());
            }
        }
        let mut free_specs = Vec::new();
        let mut values = Vec::new();
        for t in &frees {
            let (v, ty) = self.term(t, env)?;
            values.push(v);
            free_specs.push(match ty {
                Ty::Var(var) => self.field(var).1.ty.spec(),
                _ => "int".to_string(),
            });
        }
        let e = self.ent(ent);
        let no_param = |_: u8| -> (String, String) { unreachable!("an atom's params are free") };
        let mut bound: Vec<(String, String)> = vec![(String::new(), String::new()); depth];
        bound.push(("r".to_string(), "x".to_string()));
        let fail = |_: &Atom, _: &[String]| -> String { String::new() };
        let inner_env = Env {
            param: &no_param,
            bound,
            subst: frees
                .iter()
                .enumerate()
                .map(|(i, t)| (t.clone(), format!("f{i}")))
                .collect(),
            quant: &fail,
        };
        let text = self.pred(body, &inner_env, atoms)?;
        let found = {
            let list = atoms.list.borrow();
            list.iter()
                .find(|a| {
                    a.kind == kind
                        && a.ent == ent
                        && a.body == text
                        && a.frees.iter().map(|f| &f.1).eq(free_specs.iter())
                })
                .cloned()
        };
        let atom = match found {
            Some(a) => a,
            None => {
                let mut list = atoms.list.borrow_mut();
                let name = match kind {
                    Q::Count => format!("{}_cnt{}", e.snake, list.len()),
                    _ => format!("{}_q{}", e.snake, list.len()),
                };
                let a = Atom {
                    name,
                    kind,
                    ent,
                    body: text,
                    frees: frees.into_iter().zip(free_specs).collect(),
                    pred: body.clone(),
                    depth,
                };
                list.push(a.clone());
                a
            }
        };
        Ok((env.quant)(&atom, &values))
    }

    /// The row param `k` of `a` holds after the action, as a ghost row literal: `pre` is its
    /// row before (`None` for a created row, which starts at the base values), `id(j)` the id
    /// of param `j`.
    pub(super) fn row_after(
        &self,
        a: &Action,
        k: u8,
        pre: Option<&str>,
        id: &dyn Fn(u8) -> String,
    ) -> String {
        let e = self.ent(a.params[usize::from(k)].entity);
        let mut fields = Vec::new();
        for f in &e.fields {
            let before = match pre {
                Some(row) => format!("{row}.{}", f.name),
                None => match f.ty {
                    MTy::Key { .. } => "0".to_string(),
                    _ => self.lit(f.var, self.ts.var(f.var).base),
                },
            };
            let mut value = before.clone();
            for eff in &a.effects {
                match eff {
                    Effect::Assign {
                        var, param, rhs, ..
                    } if *var == f.var && *param == k => {
                        value = match rhs {
                            rdra_ish_core::lifecycle::Rhs::Lit(v) => self.lit(f.var, *v),
                            rdra_ish_core::lifecycle::Rhs::Param(j) => id(*j),
                        };
                    }
                    Effect::Transition { var, param, arms } if *var == f.var && *param == k => {
                        let mut chain = String::new();
                        for arm in arms {
                            chain.push_str(&format!(
                                "if {before} == {} {{ {} }} else ",
                                self.lit(f.var, arm.from),
                                self.lit(f.var, arm.to)
                            ));
                        }
                        value = format!("{chain}{{ {before} }}");
                    }
                    _ => {}
                }
            }
            fields.push(format!("{}: {value}", f.name));
        }
        format!("{} {{ {} }}", e.m, fields.join(", "))
    }
}

fn join(
    ctx: &Ctx,
    parts: &[Pred],
    sep: &str,
    empty: &str,
    env: &Env,
    atoms: &Atoms,
) -> Result<String, String> {
    if parts.is_empty() {
        return Ok(empty.to_string());
    }
    let parts: Result<Vec<String>, String> =
        parts.iter().map(|p| ctx.pred(p, env, atoms)).collect();
    Ok(format!("({})", parts?.join(sep)))
}

/// How the model reads a quantifier: over the state `s`.
pub(super) fn quant_in(state: &str) -> impl Fn(&Atom, &[String]) -> String + '_ {
    move |a: &Atom, values: &[String]| {
        let args = std::iter::once(state.to_string())
            .chain(values.iter().cloned())
            .collect::<Vec<_>>()
            .join(", ");
        match a.kind {
            Q::Count => format!("{}({args}).len()", a.name),
            _ => format!("{}({args})", a.name),
        }
    }
}

/// The spec name of action `a`: snake_case of its name.
pub(super) fn act_name(a: &Action) -> String {
    super::snake(&a.name)
}

/// The `Call` variant of action `a`.
pub(super) fn call_variant(a: &Action) -> String {
    crate::codegen::type_ident(&a.name)
}

/// `p0: int, p1: int` for the params of `a`.
pub(super) fn param_decls(a: &Action) -> String {
    (0..a.params.len())
        .map(|k| format!("p{k}: int"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn param_args(a: &Action) -> String {
    (0..a.params.len())
        .map(|k| format!("p{k}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Everything `model.rs` holds after the header, and the atoms its rules and guards made.
pub(super) fn print(ctx: &Ctx, atoms: &Atoms) -> Result<String, Vec<String>> {
    let mut why = Vec::new();
    let mut out = String::from(
        "use vstd::prelude::*;\nuse crate::types::*;\nuse crate::prelude::*;\n\nverus! {\n\n",
    );

    // ── A. State ──
    out.push_str(
        "// ── State: each entity's present rows by id, with their tracked columns and keys ──\n\n",
    );
    for e in &ctx.ents {
        out.push_str(&format!("pub ghost struct {} {{\n", e.m));
        for f in &e.fields {
            out.push_str(&format!("    pub {}: {},\n", f.name, f.ty.spec()));
        }
        out.push_str("}\n\n");
    }
    out.push_str("pub ghost struct MState {\n");
    for e in &ctx.ents {
        out.push_str(&format!("    pub {}: Map<int, {}>,\n", e.snake, e.m));
    }
    out.push_str("}\n\n");
    out.push_str(&format!(
        "/// No row of any entity.\npub open spec fn init() -> MState {{\n    MState {{ {} }}\n}}\n\n",
        ctx.ents
            .iter()
            .map(|e| format!("{}: Map::empty()", e.snake))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // ── B. Actions ──
    let state_q = quant_in("s");
    let mut actions_text =
        String::from("// ── Actions: when each runs, and the state it leaves ──\n\n");
    for a in ctx.ts.actions() {
        let name = act_name(a);
        let param = |k: u8| {
            let e = ctx.ent(a.params[usize::from(k)].entity);
            (format!("s.{}[p{k}]", e.snake), format!("p{k}"))
        };
        let env = Env {
            param: &param,
            bound: Vec::new(),
            subst: Vec::new(),
            quant: &state_q,
        };
        let mut conds = Vec::new();
        for (k, p) in a.params.iter().enumerate() {
            let e = ctx.ent(p.entity);
            if p.role == rdra_ish_core::lifecycle::ParamRole::New {
                conds.push(format!("!s.{}.contains_key(p{k})", e.snake));
            } else {
                conds.push(format!("s.{}.contains_key(p{k})", e.snake));
            }
        }
        let mut guards = Vec::new();
        for g in &a.guards {
            match ctx.pred(&g.pred, &env, atoms) {
                Ok(t) => guards.push(t),
                Err(w) => why.push(format!("{}: {w}", a.name)),
            }
        }
        actions_text.push_str(&format!(
            "/// The guards of {} on its rows.\npub open spec fn {name}_guard(s: MState, {}) -> bool {{\n    {}\n}}\n\n",
            a.name,
            param_decls(a),
            if guards.is_empty() { "true".to_string() } else { guards.join("\n    && ") }
        ));
        actions_text.push_str(&format!(
            "/// {} runs: its rows are present (absent for a created one) and its guards hold.\npub open spec fn {name}_enabled(s: MState, {}) -> bool {{\n    {}\n    && {name}_guard(s, {})\n}}\n\n",
            a.name,
            param_decls(a),
            conds.join("\n    && "),
            param_args(a)
        ));
        let id = |j: u8| format!("p{j}");
        let mut maps = Vec::new();
        for e in &ctx.ents {
            let mut m = format!("s.{}", e.snake);
            for (k, p) in a.params.iter().enumerate() {
                if p.entity != e.eid || !p.role.is_written() {
                    continue;
                }
                let deleted = a
                    .effects
                    .iter()
                    .any(|x| matches!(x, Effect::Delete { param } if usize::from(*param) == k));
                if deleted {
                    m = format!("{m}.remove(p{k})");
                } else {
                    let pre = (p.role != rdra_ish_core::lifecycle::ParamRole::New)
                        .then(|| format!("s.{}[p{k}]", e.snake));
                    m = format!(
                        "{m}.insert(p{k}, {})",
                        ctx.row_after(a, k as u8, pre.as_deref(), &id)
                    );
                }
            }
            maps.push(format!("        {}: {m},", e.snake));
        }
        actions_text.push_str(&format!(
            "/// The state {} leaves.\npub open spec fn {name}_step(s: MState, {}) -> MState {{\n    MState {{\n{}\n    }}\n}}\n\n",
            a.name,
            param_decls(a),
            maps.join("\n")
        ));
    }
    let mut call =
        String::from("/// A call of an action, by the ids of its rows.\npub enum Call {\n");
    let mut enabled = String::new();
    let mut step = String::new();
    for a in ctx.ts.actions() {
        let ints = vec!["int"; a.params.len()].join(", ");
        call.push_str(&format!("    {}({ints}),\n", call_variant(a)));
        let pat = format!("Call::{}({})", call_variant(a), param_args(a));
        enabled.push_str(&format!(
            "        {pat} => {}_enabled(s, {}),\n",
            act_name(a),
            param_args(a)
        ));
        step.push_str(&format!(
            "        {pat} => {}_step(s, {}),\n",
            act_name(a),
            param_args(a)
        ));
    }
    call.push_str("}\n\n");
    actions_text.push_str(&call);
    actions_text.push_str(&format!(
        "pub open spec fn enabled(s: MState, c: Call) -> bool {{\n    match c {{\n{enabled}    }}\n}}\n\n\
         pub open spec fn step(s: MState, c: Call) -> MState {{\n    match c {{\n{step}    }}\n}}\n\n\
         /// One call: the action's step when it runs, no change when it is refused.\n\
         pub open spec fn next(s: MState, c: Call) -> MState {{\n    if enabled(s, c) {{ step(s, c) }} else {{ s }}\n}}\n\n"
    ));

    // ── C. Rules ──
    let mut rules_text = String::from(
        "// ── State rules (`check` and `verify` check them) and the inductive set ──\n\n",
    );
    let mut rule_names = Vec::new();
    for inv in ctx.ts.invariants() {
        let name = format!("rule_{}", super::snake(&inv.name).to_lowercase());
        let no_param = |_: u8| -> (String, String) { unreachable!("a rule has no params") };
        let bound: Vec<(String, String)> = inv
            .binders
            .iter()
            .enumerate()
            .map(|(j, e)| (format!("s.{}[b{j}]", ctx.ent(*e).snake), format!("b{j}")))
            .collect();
        let env = Env {
            param: &no_param,
            bound,
            subst: Vec::new(),
            quant: &state_q,
        };
        let body = match ctx.pred(&inv.body, &env, atoms) {
            Ok(b) => b,
            Err(w) => {
                why.push(format!("{}: {w}", inv.name));
                continue;
            }
        };
        let vars: Vec<String> = (0..inv.binders.len())
            .map(|j| format!("b{j}: int"))
            .collect();
        let present: Vec<String> = inv
            .binders
            .iter()
            .enumerate()
            .map(|(j, e)| format!("s.{}.contains_key(b{j})", ctx.ent(*e).snake))
            .collect();
        let text = if vars.is_empty() {
            body
        } else {
            format!(
                "forall|{}| #![trigger {}]\n        {} ==>\n        {body}",
                vars.join(", "),
                present.join(", "),
                present.join(" && ")
            )
        };
        let label = inv
            .label
            .as_ref()
            .map(|l| format!(" ({})", super::super::one_line(l)))
            .unwrap_or_default();
        rules_text.push_str(&format!(
            "/// {}{label}.\npub open spec fn {name}(s: MState) -> bool {{\n    {text}\n}}\n\n",
            inv.name
        ));
        rule_names.push(name);
    }
    let mut keys = Vec::new();
    for e in &ctx.ents {
        for f in &e.fields {
            if let MTy::Key { parent } = f.ty {
                let p = ctx.ent(parent);
                let name = format!("key_{}_{}", e.snake, f.name);
                rules_text.push_str(&format!(
                    "/// {}.{} names a present {}: kept by every action, and needed to keep a count.\npub open spec fn {name}(s: MState) -> bool {{\n    forall|x: int| #[trigger] s.{}.contains_key(x) ==> s.{}.contains_key(s.{}[x].{})\n}}\n\n",
                    e.id, f.name, p.id, e.snake, p.snake, e.snake, f.name
                ));
                keys.push(name);
            }
        }
    }
    let conj = |names: &[String]| {
        if names.is_empty() {
            "true".to_string()
        } else {
            names
                .iter()
                .map(|n| format!("{n}(s)"))
                .collect::<Vec<_>>()
                .join("\n    && ")
        }
    };
    rules_text.push_str(&format!(
        "/// Every state rule.\npub open spec fn rules(s: MState) -> bool {{\n    {}\n}}\n\n",
        conj(&rule_names)
    ));
    rules_text.push_str(&format!(
        "/// The inductive set: the rules and the key lemmas.\npub open spec fn inv(s: MState) -> bool {{\n    rules(s)\n    && {}\n}}\n\n",
        conj(&keys)
    ));

    if !why.is_empty() {
        return Err(why);
    }

    // ── Atoms ──
    let mut atoms_text = String::from(
        "// ── Quantifiers over rows, each once: its body, over the state, and over a list of rows ──\n\n",
    );
    for a in atoms.list.borrow().iter() {
        let e = ctx.ent(a.ent);
        let frees_decl: String = a
            .frees
            .iter()
            .enumerate()
            .map(|(i, (_, t))| format!(", f{i}: {t}"))
            .collect();
        let frees_args: String = (0..a.frees.len()).map(|i| format!(", f{i}")).collect();
        atoms_text.push_str(&format!(
            "pub open spec fn {}_body(r: {}, x: int{frees_decl}) -> bool {{\n    {}\n}}\n\n",
            a.name, e.m, a.body
        ));
        let (over_state, over_rows) = match a.kind {
            Q::Forall => (
                format!("bool {{\n    forall|x: int| #[trigger] s.{0}.contains_key(x) ==> {1}_body(s.{0}[x], x{frees_args})\n}}", e.snake, a.name),
                format!("bool {{\n    forall|i: int| 0 <= i < rows.len() ==> #[trigger] {1}_body({0}_row_view(rows[i]), rows[i].id.0 as int{frees_args})\n}}", e.snake, a.name),
            ),
            Q::Exists => (
                format!("bool {{\n    exists|x: int| #[trigger] s.{0}.contains_key(x) && {1}_body(s.{0}[x], x{frees_args})\n}}", e.snake, a.name),
                format!("bool {{\n    exists|i: int| 0 <= i < rows.len() && #[trigger] {1}_body({0}_row_view(rows[i]), rows[i].id.0 as int{frees_args})\n}}", e.snake, a.name),
            ),
            Q::Count => (
                format!("Set<int> {{\n    s.{0}.dom().filter(|x: int| {1}_body(s.{0}[x], x{frees_args}))\n}}", e.snake, a.name),
                format!("nat {{\n    {0}_count_rows(rows, |r: {2}, x: int| {1}_body(r, x{frees_args}))\n}}", e.snake, a.name, e.m),
            ),
        };
        atoms_text.push_str(&format!(
            "pub open spec fn {}(s: MState{frees_decl}) -> {over_state}\n\n",
            a.name
        ));
        atoms_text.push_str(&format!(
            "pub open spec fn {}_rows(rows: Seq<{}>{frees_decl}) -> {over_rows}\n\n",
            a.name, e.row
        ));
        // The link: on a list of every row, the two agree.
        let link_body = match a.kind {
            Q::Forall => FORALL_LINK
                .replace("{e}", &e.snake)
                .replace("{q}", &a.name)
                .replace("{f}", &frees_args),
            Q::Exists => EXISTS_LINK
                .replace("{e}", &e.snake)
                .replace("{q}", &a.name)
                .replace("{f}", &frees_args),
            Q::Count => format!(
                "    let p = |r: {2}, x: int| {1}_body(r, x{frees_args});\n    {0}_lemma_count(rows, p);\n    assert({1}(s{frees_args}) =~= {0}_rows_map(rows).dom().filter(|x: int| p({0}_rows_map(rows)[x], x)));",
                e.snake, a.name, e.m
            ),
        };
        let (lhs, rhs) = match a.kind {
            Q::Count => (
                format!("{}_rows(rows{frees_args})", a.name),
                format!("{}(s{frees_args}).len()", a.name),
            ),
            _ => (
                format!("{}_rows(rows{frees_args})", a.name),
                format!("{}(s{frees_args})", a.name),
            ),
        };
        atoms_text.push_str(&format!(
            "/// On a list of every {} row, the quantifier over the list is the one over the state.\npub proof fn {}_link(s: MState, rows: Seq<{}>{frees_decl})\n    requires {}_snapshot(s.{}, rows)\n    ensures {lhs} == {rhs}\n{{\n{link_body}\n}}\n\n",
            e.id, a.name, e.row, e.snake, e.snake
        ));
    }

    // ── D. Preservation ──
    let preserve = preservation(ctx, atoms);

    out.push_str(&atoms_text);
    out.push_str(&actions_text);
    out.push_str(&rules_text);
    out.push_str(&preserve);
    out.push_str("} // verus!\n");
    Ok(out)
}

/// The lemmas that each action keeps `inv`, with the hints a count needs, and the run lemma.
fn preservation(ctx: &Ctx, atoms: &Atoms) -> String {
    let list = atoms.list.borrow();
    let mut out = String::from(
        "// ── Preservation: every action keeps the inductive set, so every run keeps the rules ──\n\n\
         pub proof fn init_inv()\n    ensures inv(init())\n{\n}\n\n",
    );
    for a in ctx.ts.actions() {
        let name = act_name(a);
        let mut hints = Vec::new();
        for at in list.iter().filter(|x| x.kind == Q::Count) {
            let e = ctx.ent(at.ent);
            let decl: Vec<String> = at
                .frees
                .iter()
                .enumerate()
                .map(|(i, (_, t))| format!("f{i}: {t}"))
                .collect();
            let args: String = (0..at.frees.len()).map(|i| format!(", f{i}")).collect();
            // How the counted set moves.
            let written = a
                .params
                .iter()
                .enumerate()
                .find(|(_, p)| p.entity == at.ent && p.role.is_written());
            let after = match written {
                None => format!("{}(s{args})", at.name),
                Some((k, _)) => {
                    let deleted = a
                        .effects
                        .iter()
                        .any(|x| matches!(x, Effect::Delete { param } if usize::from(*param) == k));
                    if deleted {
                        format!("{}(s{args}).remove(p{k})", at.name)
                    } else {
                        format!(
                            "if {}_body(t.{}[p{k}], p{k}{args}) {{ {}(s{args}).insert(p{k}) }} else {{ {}(s{args}).remove(p{k}) }}",
                            at.name, e.snake, at.name, at.name
                        )
                    }
                }
            };
            let q = if decl.is_empty() {
                format!("    assert({}(t{args}) =~= {after});", at.name)
            } else {
                format!(
                    "    assert forall|{}| #[trigger] {}(t{args}) == {after} by {{\n        assert({}(t{args}) =~= {after});\n    }}",
                    decl.join(", "),
                    at.name,
                    at.name
                )
            };
            hints.push(q);
            // A created parent has no counted child yet: no key names an absent row.
            for (k, p) in a.params.iter().enumerate() {
                if p.role != rdra_ish_core::lifecycle::ParamRole::New {
                    continue;
                }
                for (i, (t, _)) in at.frees.iter().enumerate() {
                    if !matches!(t, Term::Index(_)) || !keyed_by(ctx, at, i, p.entity) {
                        continue;
                    }
                    let mut vars = Vec::new();
                    let fargs: String = (0..at.frees.len())
                        .map(|j| {
                            if j == i {
                                format!(", p{k}")
                            } else {
                                vars.push(format!("f{j}: {}", at.frees[j].1));
                                format!(", f{j}")
                            }
                        })
                        .collect();
                    if vars.is_empty() {
                        hints.push(format!(
                            "    assert({}(s{fargs}) =~= Set::<int>::empty());",
                            at.name
                        ));
                    } else {
                        hints.push(format!(
                            "    assert forall|{}| #[trigger] {}(s{fargs}) == Set::<int>::empty() by {{\n        assert({}(s{fargs}) =~= Set::<int>::empty());\n    }}",
                            vars.join(", "),
                            at.name,
                            at.name
                        ));
                    }
                }
            }
        }
        // An existential keeps its witness when the action leaves that row alone, and gains the
        // written row when it matches.
        for at in list.iter().filter(|x| x.kind == Q::Exists) {
            let Some((k, _)) = a
                .params
                .iter()
                .enumerate()
                .find(|(_, p)| p.entity == at.ent && p.role.is_written())
            else {
                continue;
            };
            let e = ctx.ent(at.ent);
            let decl: Vec<String> = at
                .frees
                .iter()
                .enumerate()
                .map(|(i, (_, t))| format!("f{i}: {t}"))
                .collect();
            let args: String = (0..at.frees.len()).map(|i| format!(", f{i}")).collect();
            let deleted = a
                .effects
                .iter()
                .any(|x| matches!(x, Effect::Delete { param } if usize::from(*param) == k));
            let keep = format!(
                "{q}(s{args}) implies {q}(t{args}) || (s.{c}.contains_key(p{k}) && {q}_body(s.{c}[p{k}], p{k}{args})) by {{\n        let w = choose|x: int| #[trigger] s.{c}.contains_key(x) && {q}_body(s.{c}[x], x{args});\n        if w != p{k} {{\n            assert(t.{c}.contains_key(w) && t.{c}[w] == s.{c}[w]);\n        }}\n    }}",
                q = at.name,
                c = e.snake
            );
            let gain = format!(
                "{q}_body(t.{c}[p{k}], p{k}{args}) implies {q}(t{args}) by {{\n        assert(t.{c}.contains_key(p{k}));\n    }}",
                q = at.name,
                c = e.snake
            );
            let trig = format!("#![trigger {}(t{args})]", at.name);
            let wrap = |body: &str| format!("    assert forall|{}| {trig} {body}", decl.join(", "));
            if decl.is_empty() {
                continue; // a closed existential has no free row to follow
            }
            hints.push(wrap(&keep));
            if !deleted {
                hints.push(wrap(&gain));
            }
        }
        // A count bounded by another: when one counted set is inside another, its size is too.
        let counts: Vec<&super::model::Atom> = list.iter().filter(|x| x.kind == Q::Count).collect();
        for x in &counts {
            for y in &counts {
                if x.name == y.name || x.ent != y.ent {
                    continue;
                }
                let tx: Vec<&String> = x.frees.iter().map(|f| &f.1).collect();
                let ty: Vec<&String> = y.frees.iter().map(|f| &f.1).collect();
                if tx != ty {
                    continue;
                }
                let decl: Vec<String> = x
                    .frees
                    .iter()
                    .enumerate()
                    .map(|(i, (_, t))| format!("f{i}: {t}"))
                    .collect();
                let args: String = (0..x.frees.len()).map(|i| format!(", f{i}")).collect();
                let (sx, sy) = (
                    format!("{}(s{args})", x.name),
                    format!("{}(s{args})", y.name),
                );
                if decl.is_empty() {
                    hints.push(format!(
                        "    if {sx}.subset_of({sy}) {{ vstd::set_lib::lemma_len_subset({sx}, {sy}); }}"
                    ));
                } else {
                    hints.push(format!(
                        "    assert forall|{}| {sx}.subset_of({sy}) implies #[trigger] {sx}.len() <= #[trigger] {sy}.len() by {{\n        vstd::set_lib::lemma_len_subset({sx}, {sy});\n    }}",
                        decl.join(", ")
                    ));
                }
            }
        }
        out.push_str(&format!(
            "pub proof fn {name}_preserves(s: MState, {})\n    requires inv(s), {name}_enabled(s, {})\n    ensures inv({name}_step(s, {}))\n{{\n    let t = {name}_step(s, {});\n{}\n}}\n\n",
            param_decls(a),
            param_args(a),
            param_args(a),
            param_args(a),
            hints.join("\n")
        ));
    }
    let mut cases = String::new();
    for a in ctx.ts.actions() {
        cases.push_str(&format!(
            "            Call::{}({}) => {}_preserves(s, {}),\n",
            call_variant(a),
            param_args(a),
            act_name(a),
            param_args(a)
        ));
    }
    out.push_str(&format!(
        "pub proof fn next_preserves(s: MState, c: Call)\n    requires inv(s)\n    ensures inv(next(s, c))\n{{\n    if enabled(s, c) {{\n        match c {{\n{cases}        }}\n    }}\n}}\n\n"
    ));
    out.push_str(
        "/// The state a list of calls leaves, from no rows.\n\
         pub open spec fn run(cs: Seq<Call>) -> MState\n    decreases cs.len()\n{\n    if cs.len() == 0 { init() } else { next(run(cs.drop_last()), cs.last()) }\n}\n\n\
         /// Every state a run of calls reaches keeps the inductive set.\n\
         pub proof fn run_inv(cs: Seq<Call>)\n    ensures inv(run(cs))\n    decreases cs.len()\n{\n    if cs.len() == 0 {\n        init_inv();\n    } else {\n        run_inv(cs.drop_last());\n        next_preserves(run(cs.drop_last()), cs.last());\n    }\n}\n\n\
         /// Every state a run of calls reaches keeps every state rule, for any number of rows.\n\
         pub proof fn run_keeps_rules(cs: Seq<Call>)\n    ensures rules(run(cs))\n{\n    run_inv(cs);\n}\n\n",
    );
    out
}

/// Whether atom `at`'s body has, as a conjunct, the counted row's key to `parent` equal to
/// its free term `i`.
fn keyed_by(ctx: &Ctx, at: &Atom, i: usize, parent: EntId) -> bool {
    let free = &at.frees[i].0;
    let mut conj = vec![&at.pred];
    let mut found = false;
    while let Some(p) = conj.pop() {
        match p {
            Pred::And(parts) => conj.extend(parts.iter()),
            Pred::Cmp(a, CmpOp::Eq, b) => {
                for (x, y) in [(a, b), (b, a)] {
                    if let Term::Slot {
                        var,
                        at: At::Bound(j),
                    } = x
                    {
                        if usize::from(*j) == at.depth
                            && y == free
                            && matches!(ctx.field(*var).1.ty, MTy::Key { parent: p } if p == parent)
                        {
                            found = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    found
}

const FORALL_LINK: &str = r#"    if {q}_rows(rows{f}) {
        assert forall|x: int| #[trigger] s.{e}.contains_key(x) implies {q}_body(s.{e}[x], x{f}) by {
            {e}_lemma_dom(rows, x);
            let i = choose|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x;
            {e}_lemma_at(rows, i);
        }
    }
    if {q}(s{f}) {
        assert forall|i: int| 0 <= i < rows.len()
            implies #[trigger] {q}_body({e}_row_view(rows[i]), rows[i].id.0 as int{f}) by {
            {e}_lemma_at(rows, i);
        }
    }"#;

const EXISTS_LINK: &str = r#"    if {q}_rows(rows{f}) {
        let i = choose|i: int| 0 <= i < rows.len() && #[trigger] {q}_body({e}_row_view(rows[i]), rows[i].id.0 as int{f});
        {e}_lemma_at(rows, i);
        assert(s.{e}.contains_key(rows[i].id.0 as int));
    }
    if {q}(s{f}) {
        let x = choose|x: int| #[trigger] s.{e}.contains_key(x) && {q}_body(s.{e}[x], x{f});
        {e}_lemma_dom(rows, x);
        let i = choose|i: int| 0 <= i < rows.len() && #[trigger] rows[i].id.0 as int == x;
        {e}_lemma_at(rows, i);
        assert({q}_body({e}_row_view(rows[i]), rows[i].id.0 as int{f}));
    }"#;
