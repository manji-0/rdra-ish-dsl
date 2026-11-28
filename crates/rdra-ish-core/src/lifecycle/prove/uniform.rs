//! Lemma U of docs/theory/z3.md, checked on the systems a proof's claim names: two builds of
//! one model and scope at Int bounds `m` and `m'` must be the same system except for the
//! three items the lemma lists. The comparator is written from the lemma's statement, not
//! from the builder:
//!
//! 1. the sizes of the `Int`/`NullableInt` domains and of the key domains, which a `Domain`
//!    does not hold (it is sized by the system's bounds), so the domains compare equal;
//! 2. a `Choose` that lists every value of an Int-typed domain from `Numbers::from`: below
//!    `from` the two lists are the same, and from it each holds exactly the values up to its
//!    own top, the values both hold alike and each value only one holds as the numbers' own
//!    (`Numbers::conds`, as `build/transitions.rs::arm_guards` folds it into the numbers' arm);
//! 3. the `Tick` guard, `now < m` on one side and `now < m'` on the other.
//!
//! Everything else the prover reads must be equal: the entities, the vars with their domains,
//! bases and starts, `now`, each action's name, origin, params, guards and effects, and each
//! rule's name, binders and body. A rule's premises and text are not compared: the prover
//! reads neither. A digest of the systems would not do: the bound enters at several build
//! sites, so no one literal can be masked out.

use std::collections::BTreeMap;

use crate::lifecycle::types::{
    Action, Bounds, Choice, CmpOp, Effect, Guard, GuardWhy, Pred, Term, Value,
};
use crate::lifecycle::TransitionSystem;

/// `Ok` when `a` and `b` are the same system up to Lemma U's three items; else the first
/// difference.
pub fn same_up_to_bound(a: &TransitionSystem, b: &TransitionSystem) -> Result<(), String> {
    let unbounded = |bounds: &Bounds| Bounds {
        int_max: 0,
        ..bounds.clone()
    };
    if unbounded(&a.bounds) != unbounded(&b.bounds) {
        return Err("the instance counts differ".to_string());
    }
    if a.entities != b.entities {
        return Err("the entities differ".to_string());
    }
    if a.vars.len() != b.vars.len() {
        return Err("the number of vars differs".to_string());
    }
    for (i, (x, y)) in a.vars.iter().zip(&b.vars).enumerate() {
        if x != y {
            return Err(format!(
                "var {} differs: {x:?} and {y:?}",
                a.var_name(crate::lifecycle::types::VarId::at(i))
            ));
        }
    }
    if a.now != b.now {
        return Err("the clock differs".to_string());
    }
    if a.actions.len() != b.actions.len() {
        return Err("the number of actions differs".to_string());
    }
    for (x, y) in a.actions.iter().zip(&b.actions) {
        same_action(a, x, b, y).map_err(|why| format!("action {}: {why}", x.name))?;
    }
    if a.invariants.len() != b.invariants.len() {
        return Err("the number of rules differs".to_string());
    }
    for (x, y) in a.invariants.iter().zip(&b.invariants) {
        if (&x.name, &x.binders, &x.body) != (&y.name, &y.binders, &y.body) {
            return Err(format!("rule {} differs", x.name));
        }
    }
    Ok(())
}

fn same_action(
    a: &TransitionSystem,
    x: &Action,
    b: &TransitionSystem,
    y: &Action,
) -> Result<(), String> {
    if (&x.name, x.source, &x.origin, &x.site, &x.params)
        != (&y.name, y.source, &y.origin, &y.site, &y.params)
    {
        return Err("its name, origin or params differ".to_string());
    }
    if x.guards.len() != y.guards.len() {
        return Err("the number of guards differs".to_string());
    }
    for (g, h) in x.guards.iter().zip(&y.guards) {
        let same = match (g.why, h.why) {
            // Item 3: the clock's guard reads each side's own bound, and nothing else.
            (GuardWhy::Clock, GuardWhy::Clock) => {
                g.site == h.site && is_tick(g, a.bounds.int_max) && is_tick(h, b.bounds.int_max)
            }
            _ => g == h,
        };
        if !same {
            return Err(format!("a guard differs: {:?} and {:?}", g.pred, h.pred));
        }
    }
    if x.effects.len() != y.effects.len() {
        return Err("the number of effects differs".to_string());
    }
    for (e, f) in x.effects.iter().zip(&y.effects) {
        let same = match (e, f) {
            (
                Effect::Choose {
                    var,
                    param,
                    values,
                    numbers: Some(n),
                },
                Effect::Choose {
                    var: var2,
                    param: param2,
                    values: values2,
                    numbers: Some(n2),
                },
            ) if (var, param, n) == (var2, param2, n2) => {
                let domain = &a.var(*var).domain;
                same_numbers(values, domain.size(a), values2, domain.size(b), n)
            }
            _ => e == f,
        };
        if !same {
            let what = match e {
                Effect::Assign { var, .. }
                | Effect::Transition { var, .. }
                | Effect::Choose { var, .. } => format!("its write of {}", a.var_name(*var)),
                other => format!("{other:?}"),
            };
            return Err(format!("an effect differs: {what}"));
        }
    }
    Ok(())
}

/// `now < int_max`, as the build writes the clock's guard.
fn is_tick(g: &Guard, int_max: u8) -> bool {
    g.pred == Pred::Cmp(Term::Now, CmpOp::Lt, Term::Int(i64::from(int_max)))
}

/// Item 2: two lists of one choice of every number from `n.from`, of domains of `size` and
/// `size2` values.
fn same_numbers(
    values: &[Choice],
    size: usize,
    values2: &[Choice],
    size2: usize,
    n: &crate::lifecycle::types::Numbers,
) -> bool {
    let below = |vs: &[Choice]| -> Vec<Choice> {
        vs.iter().filter(|c| c.value < n.from).cloned().collect()
    };
    if below(values) != below(values2) {
        return false;
    }
    let tail = |vs: &[Choice]| -> BTreeMap<Value, Choice> {
        vs.iter()
            .filter(|c| c.value >= n.from)
            .map(|c| (c.value, c.clone()))
            .collect()
    };
    let (t, t2) = (tail(values), tail(values2));
    // Each list holds every value from `from` to its own top, once.
    let whole = |t: &BTreeMap<Value, Choice>, vs: &[Choice], size: usize| {
        let from = usize::from(n.from.0);
        t.len() == vs.iter().filter(|c| c.value >= n.from).count()
            && t.keys().map(|v| usize::from(v.0)).eq(from..size.max(from))
    };
    if !whole(&t, values, size) || !whole(&t2, values2, size2) {
        return false;
    }
    let of_numbers =
        |c: &Choice| c.conds.len() == n.conds.len() && c.conds.iter().all(|k| n.conds.contains(k));
    t.iter()
        .all(|(v, c)| t2.get(v).map_or(of_numbers(c), |c2| c == c2))
        && t2.iter().all(|(v, c)| t.contains_key(v) || of_numbers(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::types::Bounds;
    use std::num::NonZeroU8;

    /// A nullable Int set to `present` under a condition and to a literal under another, a
    /// copy, a rule over a number, and the clock: each of Lemma U's items appears.
    const SRC: &str = r#"
entity Account "口座" {
  id: Int @pk
  limit: Int @null
  spare: Int @null
  mode: Enum(open, held) @default(open)
  expires_at: Date @default("3")
}
usecase Open "開設"
usecase Hold "保留"
usecase Raise "引上げ"
usecase Copy "写し"
creates(Open, Account)
transitions(Hold, Account.mode, open -> held)
updates(Raise, Account)
condition Held "保留中"
holds(Held, Account.mode == held)
condition Free "通常"
holds(Free, Account.mode == open)
sets(Raise, Account.limit == present).when(Held)
sets(Raise, Account.limit == 3).when(Free)
updates(Copy, Account)
sets(Copy, Account.spare == Account.limit)
forbidden(Account, mode == held, limit == null)
forbidden(Account, mode == open, expires_at < now)
"#;

    fn at(int_max: u8) -> TransitionSystem {
        let bounds = Bounds::uniform(NonZeroU8::new(2).unwrap(), int_max);
        crate::lifecycle::tests::system_from(SRC, bounds)
    }

    #[test]
    fn builds_at_two_bounds_are_the_same_up_to_lemma_u() {
        let (a, b) = (at(4), at(9));
        assert!(
            a.actions.iter().any(|x| x.effects.iter().any(|e| matches!(
                e,
                Effect::Choose {
                    numbers: Some(_),
                    ..
                }
            ))),
            "the model has a choice of every number"
        );
        same_up_to_bound(&a, &b).unwrap();
        same_up_to_bound(&b, &a).unwrap();
        same_up_to_bound(&a, &a).unwrap();
    }

    /// Planted faults, one per kind of leak of the bound into the system: each is caught.
    #[test]
    fn a_planted_bound_leak_is_caught() {
        let a = at(4);
        let leak = |f: &dyn Fn(&mut TransitionSystem)| {
            let mut b = at(9);
            f(&mut b);
            same_up_to_bound(&a, &b).expect_err("the leak is caught")
        };
        // A guard that reads the bound as a literal.
        let why = leak(&|b| {
            let x = b.actions.iter_mut().find(|x| !x.guards.is_empty()).unwrap();
            x.guards[0].pred = Pred::Cmp(Term::Now, CmpOp::Le, Term::Int(9));
        });
        assert!(why.contains("a guard differs"), "{why}");
        // A choice of every number missing the values past the smaller top.
        let why = leak(&|b| {
            for x in &mut b.actions {
                for e in &mut x.effects {
                    if let Effect::Choose {
                        values,
                        numbers: Some(_),
                        ..
                    } = e
                    {
                        values.retain(|c| c.value.0 <= 5);
                    }
                }
            }
        });
        assert!(why.contains("an effect differs"), "{why}");
        // A choice listing every value without the numbers that stand for them.
        let why = leak(&|b| {
            for x in &mut b.actions {
                for e in &mut x.effects {
                    if let Effect::Choose { numbers, .. } = e {
                        *numbers = None;
                    }
                }
            }
        });
        assert!(why.contains("an effect differs"), "{why}");
        // A rule whose literal moved with the bound.
        let why = leak(&|b| {
            let inv = &mut b.invariants[0];
            inv.body = Pred::Cmp(Term::Now, CmpOp::Le, Term::Int(9));
        });
        assert!(why.contains("rule"), "{why}");
        // The clock's guard at the other side's bound.
        let mut clocked = at(4);
        if let Some(tick) = clocked.actions.iter_mut().find(|x| x.name == "Tick") {
            tick.guards[0].pred = Pred::Cmp(Term::Now, CmpOp::Lt, Term::Int(9));
            assert!(same_up_to_bound(&clocked, &at(9)).is_err());
        }
    }
}
