//! Candidate lemmas mined from BFS (Houdini's guesses). Each entity cone and each rule's cone
//! is explored at the system's bounds, and again at one instance per entity, where a cone
//! the run's bounds cap often finishes; what every reached state shows becomes a candidate:
//! the patterns a row takes, the values and value pairs of its columns, the patterns a child
//! row takes together with its parent (whole, per parent column, one column of each, and one
//! child column with two parent columns), and the patterns of two children of one parent. A guess is never trusted: the prover keeps it only
//! when it is inductive with the others, so a capped or over-approximate exploration costs
//! proofs, not soundness.

use std::collections::{BTreeMap, BTreeSet};

use crate::lifecycle::cone::Target;
use crate::lifecycle::explore::{Caps, ComponentRun};
use crate::lifecycle::restrict::Remap;
use crate::lifecycle::types::{At, CmpOp, Domain, EntId, Pred, Term, Value, VarId, VarKind};
use crate::lifecycle::TransitionSystem;
use crate::parallel::{in_parallel, jobs};

use super::encode::Formula;

/// The most states one mining run explores.
const STATE_CAP: usize = 50_000;
/// The most patterns one pattern lemma lists.
const PATTERN_CAP: usize = 64;

type Pattern = Vec<u8>;

/// Pairs of patterns two related rows took together.
type Pairs = BTreeSet<(Pattern, Pattern)>;

/// A key, the child's row vars, and the parent's row vars.
type KeyAxes = (VarId, Vec<VarId>, Vec<VarId>);

/// What the reached states showed, in the whole system's ids.
#[derive(Default)]
struct Seen {
    /// Per entity and the row vars a cone holds: the patterns of present rows.
    rows: BTreeMap<(EntId, Vec<VarId>), BTreeSet<Pattern>>,
    /// Per key and the row vars of child and parent: the patterns of a present child and the
    /// present parent its key names.
    keys: BTreeMap<KeyAxes, Pairs>,
    /// Per key and the child's row vars: the patterns of two present children that name one
    /// parent row. An empty set says no two ever do.
    siblings: BTreeMap<(VarId, Vec<VarId>), Pairs>,
}

/// One mined candidate and the text the report prints when the proof keeps it.
pub(super) struct Mined {
    pub text: String,
    pub formula: Formula,
    /// A pair of one child column and one parent column. There are many of them on a model
    /// with many columns, so the prover adds them only when a rule stays unproved without.
    pub fine: bool,
}

/// Mines `ts` and, when given, `twin`: the same model built at one instance per entity, with
/// `ts`'s ids (`super::mining_twin`). A cone past `STATE_CAP` at the run's bounds often
/// finishes at one instance, so the twin offers shapes the capped run never reached; the
/// run's own shapes come first, and a twin's candidate that is one of them is dropped.
pub(super) fn mine(
    ts: &TransitionSystem,
    twin: Option<&TransitionSystem>,
    rules: &[usize],
) -> Vec<Mined> {
    let systems: Vec<&TransitionSystem> = std::iter::once(ts).chain(twin).collect();
    let mut cones = Vec::new();
    for (k, sys) in systems.iter().enumerate() {
        let mut of_sys = BTreeSet::new();
        for e in sys.entity_ids() {
            of_sys.insert(sys.target_cone(Target::Entity(e)));
        }
        for &i in rules {
            of_sys.insert(sys.target_cone(Target::Invariant(i)));
        }
        cones.extend(of_sys.into_iter().map(|cone| (k, cone)));
    }
    // The cones are explored side by side; what they reach is gathered in cone order into
    // sets, so the candidates do not depend on the threads.
    let runs = in_parallel(cones, jobs(), |(k, cone)| {
        let (sub, map) = systems[k].restrict_to_mapped(&cone.vars, &cone.keep, true);
        (k, map, sub.run(Caps::new(usize::MAX, STATE_CAP)))
    });
    let mut seen: Vec<Seen> = systems.iter().map(|_| Seen::default()).collect();
    for (k, map, run) in &runs {
        record(systems[*k], map, run, &mut seen[*k]);
    }
    let mut out = candidates(ts, &seen[0]);
    if let (Some(twin), Some(seen)) = (twin, seen.get(1)) {
        let at = twin.bounds.instances;
        for m in candidates(ts, seen) {
            if !out.iter().any(|o| o.formula == m.formula) {
                out.push(Mined {
                    text: format!("{} (mined at {at} instance per entity)", m.text),
                    ..m
                });
            }
        }
    }
    out
}

/// What one cone's run reached, added to `seen` in `ts`'s ids.
fn record(ts: &TransitionSystem, map: &Remap, run: &ComponentRun, seen: &mut Seen) {
    let back: BTreeMap<VarId, VarId> = ts
        .var_ids()
        .filter_map(|v| map.var_of(v).map(|s| (s, v)))
        .collect();
    let sys = &run.system;
    // Per sub entity: the whole system's entity and its row vars, sub and whole ids.
    let entities: Vec<(EntId, EntId, Vec<VarId>, Vec<VarId>)> = sys
        .entity_ids()
        .filter_map(|se| {
            let whole = ts.entity_ids().find(|e| map.ent_of(*e) == Some(se))?;
            let sub_vars: Vec<VarId> = sys
                .var_ids()
                .filter(|v| sys.var_entity(*v) == Some(se) && is_row_value(sys, *v))
                .collect();
            let whole_vars = sub_vars.iter().map(|v| back[v]).collect();
            Some((se, whole, sub_vars, whole_vars))
        })
        .collect();
    let pattern = |s: &crate::lifecycle::StateSlice, vars: &[VarId], slot: usize| -> Pattern {
        vars.iter().map(|v| s.get(sys, *v, slot).0).collect()
    };
    for s in &run.states {
        for (se, whole, sub_vars, whole_vars) in &entities {
            for slot in (0..sys.slots(*se)).filter(|&k| s.is_present(sys, *se, k)) {
                seen.rows
                    .entry((*whole, whole_vars.clone()))
                    .or_default()
                    .insert(pattern(&s, sub_vars, slot));
            }
        }
        for o in sys.var_ids() {
            let VarKind::Owner { child, parent, .. } = sys.var(o).kind else {
                continue;
            };
            let (Some(c), Some(p)) = (
                entities.iter().find(|x| x.0 == child),
                entities.iter().find(|x| x.0 == parent),
            ) else {
                continue;
            };
            let pairs = seen.siblings.entry((back[&o], c.3.clone())).or_default();
            let linked: Vec<(usize, u8)> = (0..sys.slots(child))
                .filter(|&k| s.is_present(sys, child, k))
                .filter_map(|k| Some((k, sys.var(o).domain.slot(s.get(sys, o, k))?)))
                .collect();
            for (i, (k1, p1)) in linked.iter().enumerate() {
                for (k2, p2) in &linked[i + 1..] {
                    if p1 == p2 {
                        let (a, b) = (pattern(&s, &c.2, *k1), pattern(&s, &c.2, *k2));
                        pairs.insert((a.clone(), b.clone()));
                        pairs.insert((b, a));
                    }
                }
            }
            for slot in (0..sys.slots(child)).filter(|&k| s.is_present(sys, child, k)) {
                let Some(ps) = sys.var(o).domain.slot(s.get(sys, o, slot)) else {
                    continue;
                };
                let ps = usize::from(ps);
                if !s.is_present(sys, parent, ps) {
                    continue;
                }
                seen.keys
                    .entry((back[&o], c.3.clone(), p.3.clone()))
                    .or_default()
                    .insert((pattern(&s, &c.2, slot), pattern(&s, &p.2, ps)));
            }
        }
    }
}

/// A column, proposition, run mark, or raised mark: a row value a lemma can pin.
fn is_row_value(ts: &TransitionSystem, v: VarId) -> bool {
    !matches!(
        ts.var(v).kind,
        VarKind::Exists(_) | VarKind::Owner { .. } | VarKind::Now
    ) && !matches!(ts.var(v).domain, Domain::Instance { .. })
}

fn candidates(ts: &TransitionSystem, seen: &Seen) -> Vec<Mined> {
    let mut out = Vec::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut push = |text: String, formula: Formula, out: &mut Vec<Mined>, fine: bool| {
        if done.insert(text.clone()) {
            out.push(Mined {
                text,
                formula,
                fine,
            });
        }
    };
    let names = |vars: &[VarId]| {
        vars.iter()
            .map(|v| ts.var_name(*v))
            .collect::<Vec<_>>()
            .join(", ")
    };
    for ((e, vars), patterns) in &seen.rows {
        if vars.is_empty() {
            continue;
        }
        let id = &ts.entity(*e).id;
        if patterns.len() <= PATTERN_CAP {
            push(
                format!(
                    "every present {id} row is one of {} reached patterns of ({})",
                    patterns.len(),
                    names(vars)
                ),
                Formula {
                    binders: vec![*e],
                    body: one_of(vars, patterns, 0),
                    all_rows: false,
                },
                &mut out,
                false,
            );
        }
        for (i, a) in vars.iter().enumerate() {
            let values: BTreeSet<Pattern> = patterns.iter().map(|p| vec![p[i]]).collect();
            if values.len() < ts.var(*a).domain.size(ts) {
                push(
                    format!(
                        "a present {id} row's {} is one of {}",
                        names(&[*a]),
                        show(ts, &[*a], &values)
                    ),
                    Formula {
                        binders: vec![*e],
                        body: one_of(&[*a], &values, 0),
                        all_rows: false,
                    },
                    &mut out,
                    false,
                );
            }
            for (j, b) in vars.iter().enumerate().skip(i + 1) {
                let pairs: BTreeSet<Pattern> = patterns.iter().map(|p| vec![p[i], p[j]]).collect();
                let full = ts.var(*a).domain.size(ts) * ts.var(*b).domain.size(ts);
                if pairs.len() < full && pairs.len() <= PATTERN_CAP {
                    push(
                        format!(
                            "a present {id} row's ({}) is one of {}",
                            names(&[*a, *b]),
                            show(ts, &[*a, *b], &pairs)
                        ),
                        Formula {
                            binders: vec![*e],
                            body: one_of(&[*a, *b], &pairs, 0),
                            all_rows: false,
                        },
                        &mut out,
                        false,
                    );
                }
            }
        }
    }
    for ((o, cvars, pvars), pairs) in &seen.keys {
        let VarKind::Owner { child, parent, .. } = ts.var(*o).kind else {
            continue;
        };
        // The whole patterns, then each parent column with the whole child, then one column
        // of each, then one child column with two parent columns: a row with many columns
        // reaches too many whole pairs to list, and one column of each keeps the fact a step
        // preserves when the rest of the row does not. Two parent columns state what a child
        // column does under a combination of its parent's (insurance-claim-ops: a liability
        // case before negotiation has no completed claim, #428); one is offered only when the
        // pairs it is made of do not already imply it, which keeps their count down.
        let mut views: Vec<(Vec<VarId>, Vec<VarId>, Pairs, bool)> =
            vec![(cvars.clone(), pvars.clone(), pairs.clone(), false)];
        for (j, pv) in pvars.iter().enumerate() {
            let projected = pairs
                .iter()
                .map(|(cp, pp)| (cp.clone(), vec![pp[j]]))
                .collect();
            views.push((cvars.clone(), vec![*pv], projected, false));
            for (i, cv) in cvars.iter().enumerate() {
                let projected = pairs
                    .iter()
                    .map(|(cp, pp)| (vec![cp[i]], vec![pp[j]]))
                    .collect();
                views.push((vec![*cv], vec![*pv], projected, true));
                for (k, pv2) in pvars.iter().enumerate().skip(j + 1) {
                    let projected: Pairs = pairs
                        .iter()
                        .map(|(cp, pp)| (vec![cp[i]], vec![pp[j], pp[k]]))
                        .collect();
                    if !implied_by_pairs(&projected) {
                        views.push((vec![*cv], vec![*pv, *pv2], projected, true));
                    }
                }
            }
        }
        for (cvars, pvars, pairs, is_fine) in views {
            let full = cvars
                .iter()
                .chain(&pvars)
                .map(|v| ts.var(*v).domain.size(ts))
                .product::<usize>();
            if pairs.len() > PATTERN_CAP
                || pairs.len() >= full
                || (cvars.is_empty() && pvars.is_empty())
            {
                continue;
            }
            let linked = Pred::Cmp(
                Term::Slot {
                    var: *o,
                    at: At::Bound(0),
                },
                CmpOp::Eq,
                Term::Index(At::Bound(1)),
            );
            let options = pairs
                .iter()
                .map(|(cp, pp)| {
                    Pred::And(vec![
                        one_of(&cvars, &BTreeSet::from([cp.clone()]), 0),
                        one_of(&pvars, &BTreeSet::from([pp.clone()]), 1),
                    ])
                })
                .collect();
            push(
                format!(
                    "a present {} row and the present {} its {} names are one of {} reached pairs of ({}) and ({})",
                    ts.entity(child).id,
                    ts.entity(parent).id,
                    ts.var_name(*o),
                    pairs.len(),
                    names(&cvars),
                    names(&pvars)
                ),
                Formula {
                    binders: vec![child, parent],
                    body: Pred::Or(vec![Pred::Not(Box::new(linked)), Pred::Or(options)]),
                    all_rows: false,
                },
                &mut out,
                is_fine,
            );
        }
    }
    for ((o, cvars), pairs) in &seen.siblings {
        let VarKind::Owner { child, parent, .. } = ts.var(*o).kind else {
            continue;
        };
        if pairs.len() > PATTERN_CAP {
            continue;
        }
        let key = |b: u8| Term::Slot {
            var: *o,
            at: At::Bound(b),
        };
        let mut body = vec![
            Pred::Cmp(
                Term::Index(At::Bound(0)),
                CmpOp::Eq,
                Term::Index(At::Bound(1)),
            ),
            Pred::Cmp(key(0), CmpOp::Ne, key(1)),
        ];
        if let Domain::Instance { optional: true, .. } = ts.var(*o).domain {
            body.push(Pred::Cmp(key(0), CmpOp::Eq, Term::Lit(Value::NULL)));
        }
        body.extend(pairs.iter().map(|(a, b)| {
            Pred::And(vec![
                one_of(cvars, &BTreeSet::from([a.clone()]), 0),
                one_of(cvars, &BTreeSet::from([b.clone()]), 1),
            ])
        }));
        let text = match pairs.len() {
            0 => format!(
                "no two present {} rows name one {} row by {}",
                ts.entity(child).id,
                ts.entity(parent).id,
                ts.var_name(*o)
            ),
            n => format!(
                "two present {} rows that name one {} row by {} are one of {n} reached pairs of ({})",
                ts.entity(child).id,
                ts.entity(parent).id,
                ts.var_name(*o),
                names(cvars)
            ),
        };
        push(
            text,
            Formula {
                binders: vec![child, child],
                body: Pred::Or(body),
                all_rows: false,
            },
            &mut out,
            false,
        );
    }
    out
}

/// Whether the triples of one child column and two parent columns say nothing their pairs do
/// not: every triple whose child value goes with each parent value, and whose two parent
/// values go together, was reached.
fn implied_by_pairs(triples: &Pairs) -> bool {
    let first: BTreeSet<(u8, u8)> = triples.iter().map(|(c, p)| (c[0], p[0])).collect();
    let second: BTreeSet<(u8, u8)> = triples.iter().map(|(c, p)| (c[0], p[1])).collect();
    let parents: BTreeSet<(u8, u8)> = triples.iter().map(|(_, p)| (p[0], p[1])).collect();
    first.iter().all(|&(c, a)| {
        second
            .iter()
            .filter(|&&(c2, b)| c2 == c && parents.contains(&(a, b)))
            .all(|&(_, b)| triples.contains(&(vec![c], vec![a, b])))
    })
}

/// The row bound at `at` holds one of `patterns` over `vars`.
fn one_of(vars: &[VarId], patterns: &BTreeSet<Pattern>, at: u8) -> Pred {
    Pred::Or(
        patterns
            .iter()
            .map(|p| {
                Pred::And(
                    vars.iter()
                        .zip(p)
                        .map(|(v, x)| {
                            Pred::Cmp(
                                Term::Slot {
                                    var: *v,
                                    at: At::Bound(at),
                                },
                                CmpOp::Eq,
                                Term::Lit(Value(*x)),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn show(ts: &TransitionSystem, vars: &[VarId], patterns: &BTreeSet<Pattern>) -> String {
    let one = |v: VarId, x: u8| super::value_name(&ts.var(v).domain, i64::from(x));
    let items: Vec<String> = patterns
        .iter()
        .map(|p| match &p[..] {
            [x] => one(vars[0], *x),
            xs => format!(
                "({})",
                vars.iter()
                    .zip(xs)
                    .map(|(v, x)| one(*v, *x))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
        .collect();
    format!("{{{}}}", items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triples(xs: &[(u8, u8, u8)]) -> Pairs {
        xs.iter().map(|&(c, a, b)| (vec![c], vec![a, b])).collect()
    }

    /// A triple set is implied when each child value goes with every parent pair whose
    /// values each go with it; a missing triple of that join is a fact of its own.
    #[test]
    fn two_parent_columns_are_offered_only_past_their_pairs() {
        // Child 1 goes with parent values (0, x) and (y, 0) only: the join is all there.
        assert!(implied_by_pairs(&triples(&[
            (0, 0, 0),
            (0, 1, 1),
            (1, 0, 0)
        ])));
        // (1, 0, 1) is in the join of (1, 0), (1, 1) and the parent pair (0, 1), but no
        // state reached it: the liability case that is investigating has no completed claim.
        assert!(!implied_by_pairs(&triples(&[
            (0, 0, 1),
            (1, 0, 0),
            (1, 1, 1)
        ])));
    }
}
