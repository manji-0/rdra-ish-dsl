//! Translation validation of `Enc::compare` against the native comparison
//! (`expr.rs::compare`), on every `--prove` run (claim B1 of docs/theory/claims.md).
//!
//! Each comparison the encoded formulas and guards hold has a shape: its operator and, per
//! side, the kind of operand `Enc::term` makes (a var of a domain, a literal value, an integer,
//! `now`, or a row of an entity). For each shape the check builds the term `Enc::compare`
//! writes over fresh symbols, then, for every value of each side at the run's bounds (null
//! included, at most 257 per side), evaluates that term with the ground evaluator
//! `smt/eval.rs` and requires the native comparison of the same values to agree. A literal is
//! written as `Enc::value_term` writes it, and the check holds that term to the value it
//! stands for too. A value is read as the encoding reads it: a var's function gives the
//! domain index, a key's gives the row of its slot, or `nil` when it names none.
//!
//! Values past the run's bounds (numbers past `int_max`, rows past the slot count) are not
//! enumerated: the native comparison has no value there.

use super::{value_term, Enc, Op};
use crate::lifecycle::expr::{compare, Operand};
use crate::lifecycle::prove::smt::eval::{eval, Val, NIL};
use crate::lifecycle::prove::smt::{Expr, Sort};
use crate::lifecycle::types::{At, CmpOp, Domain, EntId, Pred, Term, Value};
use crate::lifecycle::TransitionSystem;

/// One side of a comparison, as `Enc::term` reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Side {
    Var(Domain),
    Lit(Value),
    Num(i64),
    Now,
    Row(EntId),
}

/// A comparison's operator and its two sides.
type Shape = (Side, CmpOp, Side);

/// The predicates the encoding states: each with the entities its params and binders name.
pub(in crate::lifecycle::prove) struct Encoded<'p> {
    pub pred: &'p Pred,
    pub params: &'p [EntId],
    pub bound: &'p [EntId],
}

/// Every comparison of `preds` whose term disagrees with the native comparison on some values,
/// as one line each; empty when all agree.
pub(in crate::lifecycle::prove) fn compare_faults(
    ts: &TransitionSystem,
    preds: &[Encoded<'_>],
) -> Vec<String> {
    let mut shapes: Vec<Shape> = Vec::new();
    for p in preds {
        let mut bound = p.bound.to_vec();
        collect(ts, p.pred, p.params, &mut bound, &mut shapes);
    }
    let enc = Enc { ts };
    shapes
        .iter()
        .filter_map(|shape| check(&enc, shape, encoded).err())
        .collect()
}

fn collect(
    ts: &TransitionSystem,
    pred: &Pred,
    params: &[EntId],
    bound: &mut Vec<EntId>,
    out: &mut Vec<Shape>,
) {
    let mut push = |a: &Term, op: CmpOp, b: &Term, bound: &[EntId]| {
        let side = |t: &Term| match t {
            Term::Slot { var, .. } => Side::Var(ts.var(*var).domain.clone()),
            Term::Lit(v) => Side::Lit(*v),
            Term::Int(n) => Side::Num(*n),
            Term::Now => Side::Now,
            Term::Index(At::Param(p) | At::Next(p)) => Side::Row(params[usize::from(*p)]),
            Term::Index(At::Bound(b)) => Side::Row(bound[usize::from(*b)]),
        };
        let shape = (side(a), op, side(b));
        if !out.contains(&shape) {
            out.push(shape);
        }
    };
    match pred {
        Pred::True | Pred::False => {}
        Pred::Cmp(a, op, b) => push(a, *op, b, bound),
        Pred::In(a, vs) => vs.iter().for_each(|v| push(a, CmpOp::Eq, v, bound)),
        Pred::Not(p) => collect(ts, p, params, bound, out),
        Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| collect(ts, p, params, bound, out)),
        Pred::Forall { entity, body }
        | Pred::Exists { entity, body }
        | Pred::Count { entity, body, .. } => {
            bound.push(*entity);
            collect(ts, body, params, bound, out);
            bound.pop();
        }
    }
}

/// What a side's operand is in the term, and its values: each as the native comparison reads
/// it and as the term's symbol takes it (`None` for a side with no symbol).
struct Sides<'d> {
    op: Op,
    values: Vec<(Operand<'d>, Option<Val>)>,
}

/// The value a var's function takes for `v`: the domain index, or for a key the row of its
/// slot (`nil` when it names none).
fn ground(domain: &Domain, v: Value) -> Val {
    match domain {
        Domain::Instance { .. } => Val::Row(domain.slot(v).map_or(NIL, i64::from)),
        _ => Val::Int(i64::from(v.0)),
    }
}

fn sides<'d>(ts: &TransitionSystem, side: &'d Side, left: bool) -> Sides<'d> {
    let name = if left { "l" } else { "r" };
    let sym = |sort| Expr::sym(name, sort);
    match side {
        Side::Var(d) => {
            let sort = match d {
                Domain::Instance { parent, .. } => Sort::Ent(*parent),
                _ => Sort::Int,
            };
            let values = (0..d.size(ts))
                .map(|i| {
                    let v = Value(u8::try_from(i).expect("a domain has at most 256 values"));
                    (Operand::Value(v, Some(d)), Some(ground(d, v)))
                })
                .collect();
            Sides {
                op: Op::Var(sym(sort), d.clone()),
                values,
            }
        }
        Side::Lit(v) => Sides {
            op: Op::Lit(*v),
            values: vec![(Operand::Value(*v, None), None)],
        },
        Side::Num(n) => Sides {
            op: Op::Num(Expr::int(*n)),
            values: vec![(Operand::Number(*n), None)],
        },
        Side::Now => Sides {
            op: Op::Num(Expr::sym("now", Sort::Int)),
            values: (0..=i64::from(ts.bounds.int_max))
                .map(|k| (Operand::Number(k), Some(Val::Int(k))))
                .collect(),
        },
        Side::Row(e) => Sides {
            op: Op::Row(sym(Sort::Ent(*e)), *e),
            values: (0..ts.slots(*e))
                .map(|s| {
                    let s = u8::try_from(s).expect("at most 255 slots");
                    (Operand::Slot(s), Some(Val::Row(i64::from(s))))
                })
                .collect(),
        },
    }
}

/// The symbols every term may name besides the sides': `nil<e>` and `zero<e>`, the rows
/// `Enc::value_term` writes for an unlinked key and for slot 0.
fn fixed(name: &str) -> Option<Val> {
    let entity = |prefix: &str| {
        name.strip_prefix(prefix)
            .is_some_and(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit()))
    };
    match () {
        _ if entity("nil") => Some(Val::Row(NIL)),
        _ if entity("zero") => Some(Val::Row(0)),
        _ => None,
    }
}

type Encoder = fn(&Enc<'_>, &Op, CmpOp, &Op) -> super::R<Expr>;

/// `Enc::compare` as an `Encoder`.
fn encoded(enc: &Enc<'_>, l: &Op, op: CmpOp, r: &Op) -> super::R<Expr> {
    enc.compare(l, op, r)
}

/// `shape` checked against `encode`, which is `Enc::compare` but for the tests that plant a
/// wrong one. A shape the encoder refuses is out of the proof's scope, so it passes.
fn check(enc: &Enc, shape: &Shape, encode: Encoder) -> Result<(), String> {
    let ts = enc.ts;
    let (l, op, r) = shape;
    let (ls, rs) = (sides(ts, l, true), sides(ts, r, false));
    let Ok(term) = encode(enc, &ls.op, *op, &rs.op) else {
        return Ok(());
    };
    // A literal's term must be the value it stands for, read against the other side's key.
    for (lit, other) in [(l, r), (r, l)] {
        if let (Side::Lit(v), Side::Var(d @ Domain::Instance { .. })) = (lit, other) {
            if let Ok(t) = value_term(d, *v) {
                let got = eval(&t, &fixed);
                if got.as_ref() != Ok(&ground(d, *v)) {
                    return Err(format!(
                        "the literal {} of {d:?} encodes as {t}, which reads {got:?}",
                        v.0
                    ));
                }
            }
        }
    }
    // Both sides `now` read one clock.
    let tied = *l == Side::Now && *r == Side::Now;
    for (ln, lv) in &ls.values {
        for (rn, rv) in &rs.values {
            if tied && lv != rv {
                continue;
            }
            let sym = |name: &str| match name {
                "l" => *lv,
                "r" => *rv,
                "now" if *l == Side::Now => *lv,
                "now" if *r == Side::Now => *rv,
                _ => fixed(name),
            };
            let want = compare(ln, *op, rn);
            let got = eval(&term, &sym);
            if got != Ok(Val::Bool(want)) {
                return Err(format!(
                    "`Enc::compare` of {l:?} {op:?} {r:?} disagrees with the native comparison at {} and {}: the term {term} reads {got:?}, the native {want}",
                    show(ln),
                    show(rn)
                ));
            }
        }
    }
    Ok(())
}

fn show(o: &Operand) -> String {
    match o {
        Operand::Value(v, _) => format!("value {}", v.0),
        Operand::Number(n) => format!("number {n}"),
        Operand::Slot(s) => format!("slot {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::prove::smt;
    use crate::lifecycle::types::Bounds;

    const LOANS: &str =
        "entity Member \"m\" {\n  id: Int @pk\n  score: Int @null\n  level: Int\n}\n\
        entity Loan \"l\" {\n  id: Int @pk\n  status: Enum(open, closed) @default(open)\n}\n\
        usecase Join \"j\"\nusecase Borrow \"b\"\n\
        creates(Join, Member)\ncreates(Borrow, Loan)\nupdates(Borrow, Member)\n\
        relate(Loan, Member, 0..3:1)\n";

    /// Every shape the comparisons of a system can take, so the check sees each path of
    /// `Enc::compare`: numbers with and without null, `now`, literals, keys, rows.
    fn every_shape(ts: &TransitionSystem) -> Vec<Shape> {
        let mut sides = vec![Side::Num(2), Side::Num(-1), Side::Now];
        for v in ts.var_ids() {
            let d = Side::Var(ts.var(v).domain.clone());
            if !sides.contains(&d) {
                sides.push(d);
            }
        }
        let parent = ts.entity_ids().next().expect("an entity");
        let domains = [
            Domain::Int,
            Domain::NullableInt,
            Domain::Bool,
            Domain::NullableBool,
            Domain::Nullable,
            Domain::Enum(vec!["a".into(), "b".into(), "c".into()]),
            Domain::NullableEnum(vec!["a".into(), "b".into()]),
            Domain::Instance {
                parent,
                optional: false,
            },
            Domain::Instance {
                parent,
                optional: true,
            },
        ];
        for d in domains {
            if !sides.contains(&Side::Var(d.clone())) {
                sides.push(Side::Var(d));
            }
        }
        for e in ts.entity_ids() {
            sides.push(Side::Row(e));
        }
        for v in 0..3 {
            sides.push(Side::Lit(Value(v)));
        }
        let ops = [
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::Lt,
            CmpOp::Le,
            CmpOp::Gt,
            CmpOp::Ge,
        ];
        let mut out = Vec::new();
        for l in &sides {
            for r in &sides {
                for op in ops {
                    out.push((l.clone(), op, r.clone()));
                }
            }
        }
        out
    }

    fn system() -> TransitionSystem {
        crate::lifecycle::tests::system_from(
            LOANS,
            Bounds::uniform(std::num::NonZeroU8::new(2).unwrap(), 3),
        )
    }

    #[test]
    fn the_encoding_agrees_with_the_native_comparison_on_every_shape() {
        let ts = system();
        let enc = Enc { ts: &ts };
        let shapes = every_shape(&ts);
        let checked = shapes
            .iter()
            .filter(|s| {
                let (ls, rs) = (sides(&ts, &s.0, true), sides(&ts, &s.2, false));
                enc.compare(&ls.op, s.1, &rs.op).is_ok()
            })
            .count();
        assert!(checked > 100, "only {checked} shapes encode");
        for shape in &shapes {
            check(&enc, shape, encoded).unwrap();
        }
    }

    /// A planted fault in each part of the encoding: the check names the shape it breaks.
    #[test]
    fn a_planted_wrong_compare_is_caught() {
        let ts = system();
        let enc = Enc { ts: &ts };
        let shapes = every_shape(&ts);
        let caught = |encode: Encoder| {
            shapes
                .iter()
                .filter_map(|s| check(&enc, s, encode).err())
                .next()
        };
        // `<=` written for `<`.
        let le_for_lt: Encoder = |enc, l, op, r| {
            let op = if op == CmpOp::Lt { CmpOp::Le } else { op };
            enc.compare(l, op, r)
        };
        // A nullable number read without its offset: `null` as 0.
        let no_null: Encoder = |enc, l, op, r| {
            let strip = |o: &Op| match o {
                Op::Var(v, Domain::NullableInt) => Op::Var(v.clone(), Domain::Int),
                Op::Var(v, d) => Op::Var(v.clone(), d.clone()),
                Op::Lit(v) => Op::Lit(*v),
                Op::Num(n) => Op::Num(n.clone()),
                Op::Row(r, e) => Op::Row(r.clone(), *e),
            };
            enc.compare(&strip(l), op, &strip(r))
        };
        // `null` ordered below every number.
        let null_ordered: Encoder = |enc, l, op, r| {
            let t = enc.compare(l, op, r)?;
            Ok(match op {
                CmpOp::Lt | CmpOp::Le => smt::or(vec![t, Expr::bool(true)]),
                _ => t,
            })
        };
        for (name, encode) in [
            ("le_for_lt", le_for_lt),
            ("no_null", no_null),
            ("null_ordered", null_ordered),
        ] {
            let fault = caught(encode).unwrap_or_else(|| panic!("{name} is not caught"));
            assert!(
                fault.contains("disagrees with the native comparison"),
                "{name}: {fault}"
            );
        }
        assert!(caught(encoded).is_none());
    }

    #[test]
    fn the_shapes_of_a_rule_are_collected() {
        let ts = system();
        let score = ts
            .var_ids()
            .find(|v| ts.var_name(*v).contains("score"))
            .unwrap();
        let member = ts
            .entity_ids()
            .find(|e| ts.entity(*e).id == "Member")
            .unwrap();
        let body = Pred::Forall {
            entity: member,
            body: Box::new(Pred::In(
                Term::Slot {
                    var: score,
                    at: At::Bound(1),
                },
                vec![Term::Int(3), Term::Lit(Value::NULL)],
            )),
        };
        let preds = [Encoded {
            pred: &body,
            params: &[],
            bound: &[member],
        }];
        let mut shapes = Vec::new();
        collect(&ts, preds[0].pred, &[], &mut vec![member], &mut shapes);
        assert_eq!(shapes.len(), 2, "{shapes:?}");
        assert!(compare_faults(&ts, &preds).is_empty());
    }
}
