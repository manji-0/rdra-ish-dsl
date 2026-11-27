//! Lemmas from a rule's counterexample to induction (#428, B), in the manner of IC3's
//! generalization: the CTI's pre-state over its rows becomes a conjunction of literals (its
//! diagram), and the clause that forbids it is weakened, one literal dropped at a time, while
//! it holds in the initial states and no step breaks it from a state where it, the rule, and
//! the kept set hold. A clause that survives is only a guess: it joins the next proof as a
//! candidate, which keeps it only when it is inductive with the others, so `proved` stays
//! sound. Deterministic: the literals are tried in a fixed order, and the rounds and the
//! literals are bounded.

use std::collections::BTreeSet;

use crate::lifecycle::cone::VarSet;
use crate::lifecycle::types::{At, CmpOp, EntId, Pred, Term, Value, VarId, VarKind};
use crate::lifecycle::TransitionSystem;

use super::encode::{self, Enc, Formula, Frame};
use super::smt::{Command, Expr, Script};
use super::solver::{Sat, Sexp, Solver, SolverError};

/// The most literals a diagram may have; a larger one is not generalized.
const LITERAL_CAP: usize = 40;

/// The diagram's rows, by entity, and its literals over them (`At::Bound`).
pub(super) type Diagram = (Vec<EntId>, Vec<Pred>);

/// A row of the diagram: its entity, its id in the model, and its pre-state values.
struct Row {
    entity: EntId,
    id: Sexp,
    values: Vec<(VarId, Sexp)>,
}

/// The diagram of the open sat scope's pre-state over `rows`: the present rows among them,
/// each once, and the literals that pin their values, their keys to one another, and that two
/// rows of one entity differ. `None` when the model cannot be read as one.
pub(super) fn diagram(
    solver: &mut dyn Solver,
    ts: &TransitionSystem,
    rows: &[(Expr, EntId)],
) -> Result<Option<Diagram>, SolverError> {
    let enc = Enc { ts };
    let mut terms: Vec<Expr> = Vec::new();
    let vars_of = |e: EntId| -> Vec<VarId> {
        ts.var_ids()
            .filter(|v| ts.var_entity(*v) == Some(e) && !matches!(ts.var(*v).kind, VarKind::Now))
            .collect()
    };
    for (row, e) in rows {
        terms.push(row.clone());
        for v in vars_of(*e) {
            terms.push(enc.read(v, Frame::Pre, row));
        }
    }
    for e in ts.entity_ids() {
        terms.push(encode::nil(e));
    }
    let out = solver
        .exec(&Command::GetValue(terms).to_string())?
        .join(" ");
    let Some(Sexp::List(pairs)) = Sexp::parse(&out) else {
        return Ok(None);
    };
    let mut values = pairs.into_iter().map(|p| match p {
        Sexp::List(mut kv) if kv.len() == 2 => Some(kv.remove(1)),
        _ => None,
    });
    let mut read: Vec<Row> = Vec::new();
    for (_, e) in rows {
        let Some(id) = values.next().flatten() else {
            return Ok(None);
        };
        let mut row = Row {
            entity: *e,
            id,
            values: Vec::new(),
        };
        for v in vars_of(*e) {
            let Some(x) = values.next().flatten() else {
                return Ok(None);
            };
            row.values.push((v, x));
        }
        read.push(row);
    }
    let nils: Vec<Sexp> = values.flatten().collect();
    let nil_of = |e: EntId| nils.get(e.index());
    // The present rows, each once.
    let mut kept: Vec<Row> = Vec::new();
    for row in read {
        let present = row
            .values
            .iter()
            .any(|(v, x)| matches!(ts.var(*v).kind, VarKind::Exists(_)) && x.int() == Some(1));
        let same = kept
            .iter()
            .any(|k| k.entity == row.entity && k.id == row.id);
        if present && !same && nil_of(row.entity) != Some(&row.id) {
            kept.push(row);
        }
    }
    let slot = |var: VarId, b: usize| Term::Slot {
        var,
        at: At::Bound(b as u8),
    };
    let mut lits = Vec::new();
    for (i, a) in kept.iter().enumerate() {
        for (j, b) in kept.iter().enumerate().skip(i + 1) {
            if a.entity == b.entity {
                lits.push(Pred::Cmp(
                    Term::Index(At::Bound(i as u8)),
                    CmpOp::Ne,
                    Term::Index(At::Bound(j as u8)),
                ));
            }
        }
    }
    for (i, row) in kept.iter().enumerate() {
        for (v, x) in &row.values {
            match ts.var(*v).kind {
                VarKind::Exists(_) | VarKind::Now => {}
                VarKind::Owner { parent, .. } => {
                    if let Some(j) = kept.iter().position(|k| k.entity == parent && &k.id == x) {
                        lits.push(Pred::Cmp(
                            slot(*v, i),
                            CmpOp::Eq,
                            Term::Index(At::Bound(j as u8)),
                        ));
                    } else if nil_of(parent) == Some(x) {
                        lits.push(Pred::Cmp(slot(*v, i), CmpOp::Eq, Term::Lit(Value::NULL)));
                    }
                }
                _ => {
                    if let Some(n) = x.int().and_then(|n| u8::try_from(n).ok()) {
                        lits.push(Pred::Cmp(slot(*v, i), CmpOp::Eq, Term::Lit(Value(n))));
                    }
                }
            }
        }
    }
    if kept.is_empty() || lits.is_empty() {
        return Ok(None);
    }
    Ok(Some((kept.iter().map(|r| r.entity).collect(), lits)))
}

/// The clause that forbids `lits` over rows of `binders`.
fn clause(binders: &[EntId], lits: &[Pred]) -> Formula {
    Formula {
        binders: binders.to_vec(),
        body: Pred::Not(Box::new(Pred::And(lits.to_vec()))),
        all_rows: false,
    }
}

/// What a generalization checks a clause against: the hypotheses every step check asserts
/// (the kept set and the rule, over the pre-state), each step with what it writes, and the
/// initial states.
pub(super) struct Frame0<'a> {
    pub hyps: &'a Script,
    pub steps: &'a [(Script, VarSet)],
    pub initial: &'a Script,
    pub rlimit: u32,
}

/// Whether the clause holds initially and no step breaks it from a state of `frame.hyps`
/// where it holds.
fn inductive(
    solver: &mut dyn Solver,
    ts: &TransitionSystem,
    frame: &Frame0,
    f: &Formula,
) -> Result<bool, SolverError> {
    let enc = Enc { ts };
    let (Ok(pre), Ok(post), Ok(at_init)) = (
        enc.holds(f, Frame::Pre),
        enc.violation(f, Frame::Post),
        enc.violation(f, Frame::Pre),
    ) else {
        return Ok(false);
    };
    let mut reads = VarSet::new();
    ts.pred_reads(&f.body, &mut reads);
    reads.extend(f.binders.iter().map(|e| ts.entity(*e).exists));
    let check = |s: Script, solver: &mut dyn Solver| -> Result<bool, SolverError> {
        let mut open = Script::from([Command::Push]);
        open.append(&s);
        solver.exec(open.text())?;
        let sat = solver.check_sat(frame.rlimit)?;
        solver.exec(&Command::Pop.to_string())?;
        Ok(sat == Sat::Unsat)
    };
    let mut init = frame.initial.clone();
    init.append(&at_init);
    if !check(init, solver)? {
        return Ok(false);
    }
    for (step, writes) in frame.steps {
        if !reads.iter().any(|v| writes.contains(v)) {
            continue;
        }
        let mut s = step.clone();
        s.append(frame.hyps);
        s.push(Command::Assert(pre.clone()));
        s.append(&post);
        if !check(s, solver)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The diagram's clause with each literal dropped in turn while it stays inductive; `None`
/// when the whole clause is not inductive, or the diagram is past `LITERAL_CAP`.
pub(super) fn generalize(
    solver: &mut dyn Solver,
    ts: &TransitionSystem,
    frame: &Frame0,
    binders: &[EntId],
    lits: Vec<Pred>,
) -> Result<Option<Formula>, SolverError> {
    if lits.len() > LITERAL_CAP {
        return Ok(None);
    }
    if !inductive(solver, ts, frame, &clause(binders, &lits))? {
        return Ok(None);
    }
    let mut kept = lits;
    let mut i = 0;
    while i < kept.len() {
        let mut fewer = kept.clone();
        fewer.remove(i);
        if !fewer.is_empty() && inductive(solver, ts, frame, &clause(binders, &fewer))? {
            kept = fewer;
        } else {
            i += 1;
        }
    }
    Ok(Some(trim(binders, &kept)))
}

/// The clause over only the binders its literals read, renumbered.
fn trim(binders: &[EntId], lits: &[Pred]) -> Formula {
    let mut used = BTreeSet::new();
    for l in lits {
        for t in l.terms() {
            if let Term::Slot {
                at: At::Bound(b), ..
            }
            | Term::Index(At::Bound(b)) = t
            {
                used.insert(*b);
            }
        }
    }
    let order: Vec<u8> = used.into_iter().collect();
    let at = |b: &u8| order.iter().position(|x| x == b).expect("a used binder") as u8;
    let renumber = |t: &Term| match t {
        Term::Slot {
            var,
            at: At::Bound(b),
        } => Term::Slot {
            var: *var,
            at: At::Bound(at(b)),
        },
        Term::Index(At::Bound(b)) => Term::Index(At::Bound(at(b))),
        other => other.clone(),
    };
    let lits: Vec<Pred> = lits
        .iter()
        .map(|l| match l {
            Pred::Cmp(a, op, b) => Pred::Cmp(renumber(a), *op, renumber(b)),
            other => other.clone(),
        })
        .collect();
    let binders: Vec<EntId> = order.iter().map(|b| binders[usize::from(*b)]).collect();
    clause(&binders, &lits)
}

/// The clause as `.lemma()` text a user can copy into the model, when the language states
/// it: rows of distinct entities, each literal a column, a run mark, a raised mark, or a key
/// between them. Otherwise a plain reading of it.
pub(super) fn text(ts: &TransitionSystem, f: &Formula) -> String {
    let Pred::Not(inner) = &f.body else {
        return format!("{:?}", f.body);
    };
    let Pred::And(lits) = inner.as_ref() else {
        return format!("{:?}", f.body);
    };
    let ids: Vec<&str> = f
        .binders
        .iter()
        .map(|e| ts.entity(*e).id.as_str())
        .collect();
    let distinct = ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
    let one = f.binders.len() == 1;
    let mut conds = Vec::new();
    let mut along = Vec::new();
    let mut plain = true;
    for l in lits {
        match l {
            Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Bound(b),
                },
                CmpOp::Eq,
                rhs,
            ) => {
                let e = ids[usize::from(*b)];
                let prefix = if one { String::new() } else { format!("{e}.") };
                let decl = ts.var(*var);
                match (&decl.kind, rhs) {
                    (VarKind::Column { column, .. }, Term::Lit(Value(x))) => conds.push(format!(
                        "{prefix}{column} == {}",
                        super::value_name(&decl.domain, i64::from(*x))
                    )),
                    (VarKind::Owner { parent, .. }, Term::Index(At::Bound(_))) => {
                        along.push(format!("{e}, {}", ts.entity(*parent).id));
                    }
                    (VarKind::Run { usecase, .. }, Term::Lit(Value(x))) if *x > 0 => {
                        conds.push(format!("{e}.in_run({usecase}, {x})"));
                    }
                    (
                        VarKind::Raised {
                            event,
                            handler: None,
                            ..
                        },
                        Term::Lit(Value(1)),
                    ) => {
                        conds.push(format!("{e}.raised({event})"));
                    }
                    _ => plain = false,
                }
            }
            _ => plain = false,
        }
    }
    if !plain || !distinct || conds.is_empty() {
        let lits: Vec<String> = lits.iter().map(|l| format!("{l:?}")).collect();
        return format!(
            "no present rows of ({}) with {}",
            ids.join(", "),
            lits.join(" and ")
        );
    }
    let scope = match (one, along.as_slice()) {
        (true, _) => String::new(),
        (false, []) => ".global()".to_string(),
        (false, keys) => keys.iter().map(|k| format!(".along({k})")).collect(),
    };
    format!(
        "forbidden({}, {}){scope}.lemma()",
        ids.join(", "),
        conds.join(", ")
    )
}
