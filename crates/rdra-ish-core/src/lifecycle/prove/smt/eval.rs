//! A ground evaluator of the quantifier-free fragment of `Expr` that `encode.rs::Enc::compare`
//! writes, for the translation validation of comparisons (`encode/compare_tv.rs`): numerals,
//! `true`/`false`, `and`, `or`, `not`, `=>`, `=`, `distinct`, `ite`, the orders, `+`, binary
//! and unary `-`, and symbols the caller gives a value. Anything else fails, so a term outside
//! the fragment is a fault, never a value. This code is trusted: it is the reading of SMT-LIB
//! the check compares the native comparison with.

use super::{Expr, Node};

/// A ground value: a Boolean, an integer, or a row of an entity's sort (`NIL` for `nil<e>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::lifecycle::prove) enum Val {
    Bool(bool),
    Int(i64),
    Row(i64),
}

/// The row `nil<e>` names: no slot.
pub(in crate::lifecycle::prove) const NIL: i64 = -1;

/// `e` with each symbol read through `sym`; `Err` names what lies outside the fragment.
pub(in crate::lifecycle::prove) fn eval(
    e: &Expr,
    sym: &dyn Fn(&str) -> Option<Val>,
) -> Result<Val, String> {
    let bool_of = |x: &Expr| match eval(x, sym)? {
        Val::Bool(b) => Ok(b),
        v => Err(format!("{x} is {v:?}, not a Boolean")),
    };
    let int_of = |x: &Expr| match eval(x, sym)? {
        Val::Int(n) => Ok(n),
        v => Err(format!("{x} is {v:?}, not an integer")),
    };
    let (head, args) = match &*e.node {
        Node::Num(n) => {
            return i64::try_from(*n)
                .map(Val::Int)
                .map_err(|_| format!("the numeral {n} is past i64"))
        }
        Node::App(head, args) => (head.as_ref(), &args[..]),
        Node::Quant(..) | Node::Let(..) => return Err(format!("{e} is outside the fragment")),
    };
    Ok(match (head, args) {
        ("true", []) => Val::Bool(true),
        ("false", []) => Val::Bool(false),
        (name, []) => sym(name).ok_or_else(|| format!("the symbol {name} has no value"))?,
        ("and", xs) => Val::Bool(
            xs.iter()
                .map(bool_of)
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .all(|b| *b),
        ),
        ("or", xs) => Val::Bool(
            xs.iter()
                .map(bool_of)
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|b| *b),
        ),
        ("not", [x]) => Val::Bool(!bool_of(x)?),
        ("=>", [p, q]) => Val::Bool(!bool_of(p)? || bool_of(q)?),
        ("=", [a, b]) => Val::Bool(eval(a, sym)? == eval(b, sym)?),
        ("distinct", xs) if xs.len() > 1 => {
            let vs = xs
                .iter()
                .map(|x| eval(x, sym))
                .collect::<Result<Vec<_>, _>>()?;
            Val::Bool((0..vs.len()).all(|i| vs[i + 1..].iter().all(|w| *w != vs[i])))
        }
        ("ite", [c, t, f]) => match bool_of(c)? {
            true => eval(t, sym)?,
            false => eval(f, sym)?,
        },
        ("<", [a, b]) => Val::Bool(int_of(a)? < int_of(b)?),
        ("<=", [a, b]) => Val::Bool(int_of(a)? <= int_of(b)?),
        (">", [a, b]) => Val::Bool(int_of(a)? > int_of(b)?),
        (">=", [a, b]) => Val::Bool(int_of(a)? >= int_of(b)?),
        ("+", [a, b]) => Val::Int(checked(int_of(a)?.checked_add(int_of(b)?))?),
        ("-", [a, b]) => Val::Int(checked(int_of(a)?.checked_sub(int_of(b)?))?),
        ("-", [a]) => Val::Int(checked(int_of(a)?.checked_neg())?),
        _ => return Err(format!("{e} is outside the fragment")),
    })
}

fn checked(n: Option<i64>) -> Result<i64, String> {
    n.ok_or_else(|| "an integer past i64".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::{and, distinct, eq, ite, not, or, rel, sub, Rel, Sort};
    use super::*;

    fn at(x: i64) -> impl Fn(&str) -> Option<Val> {
        move |s| (s == "x").then_some(Val::Int(x))
    }

    #[test]
    fn evaluates_the_fragment_and_refuses_the_rest() {
        let x = Expr::sym("x", Sort::Int);
        let t = and(vec![
            rel(Rel::Le, Expr::int(-2), x.clone()),
            or(vec![
                eq(sub(x.clone(), Expr::int(1)), Expr::int(2)),
                not(distinct(vec![x.clone(), Expr::int(0)])),
            ]),
        ]);
        let holds = |v| eval(&t, &at(v)).unwrap();
        assert_eq!(holds(3), Val::Bool(true));
        assert_eq!(holds(0), Val::Bool(true));
        assert_eq!(holds(1), Val::Bool(false));
        assert_eq!(holds(-3), Val::Bool(false));
        let pick = ite(eq(x.clone(), Expr::int(0)), Expr::int(7), x.clone());
        assert_eq!(eval(&pick, &at(0)).unwrap(), Val::Int(7));
        assert!(eval(&Expr::sym("y", Sort::Int), &at(0)).is_err());
        let q = super::super::forall(vec![x.clone()], Expr::bool(true));
        assert!(eval(&q, &at(0)).is_err());
    }
}
