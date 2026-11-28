//! Candidate lemmas mined from BFS (Houdini's guesses). Each entity cone and each rule's cone
//! is explored at the system's bounds, and again at one instance per entity, where a cone
//! the run's bounds cap often finishes; what every reached state shows becomes a candidate:
//! the patterns a row takes, the values and value pairs of its columns, the patterns a child
//! row takes together with its parent (whole, per parent column, one column of each, and one
//! child column with two parent columns), the patterns of two children of one parent, and
//! how many children a parent with a value a count guard's step writes has, at the guard's
//! count, and (at the run's bounds only) which value of another entity's rows is present
//! whenever a row holds a value (∀∃). A guess is never trusted: the prover keeps it only when
//! it is inductive with the others, so a capped or over-approximate exploration costs proofs,
//! not soundness.

use crate::lifecycle::narrow::id8;
use std::collections::{BTreeMap, BTreeSet};

use crate::lifecycle::cone::Target;
use crate::lifecycle::explore::{Caps, ComponentRun};
use crate::lifecycle::restrict::Remap;
use crate::lifecycle::types::{
    At, CmpOp, Domain, Effect, EntId, Pred, Rhs, Term, Value, VarId, VarKind,
};
use crate::lifecycle::TransitionSystem;
use crate::parallel::{in_parallel, jobs};

use super::encode::Formula;

/// The most states one mining run explores.
pub(super) const STATE_CAP: usize = 50_000;
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
    /// Per key and the row vars of child and parent: each present parent's pattern with the
    /// sorted patterns of the present children that name it, none included.
    families: BTreeMap<KeyAxes, BTreeSet<(Pattern, Vec<Pattern>)>>,
    /// Per row value `a` some present row took: the row vars of the cones that reached it, and
    /// the values `b` of those vars that some present row took in every state where a row
    /// held `a`, in each of those cones (`record`).
    exists: BTreeMap<Atom, (BTreeSet<VarId>, BTreeSet<Atom>)>,
}

/// One row var at one value.
type Atom = (VarId, u8);

/// One mined candidate and the text the report prints when the proof keeps it.
pub(super) struct Mined {
    pub text: String,
    pub formula: Formula,
    /// How many times the proof must leave a rule unproved before it joins: 0 from the start;
    /// 1 for the fine lemmas (one child column with one or two parent columns, and counts),
    /// which are many on a model with many columns; 2 for the ∀∃ lemmas, which are many too
    /// and cost z3 a quantifier alternation, so they wait for a rule the fine ones leave.
    pub wait: u8,
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
    let counted: BTreeSet<VarId> = quotas(ts).into_iter().map(|q| q.key).collect();
    let mut seen: Vec<Seen> = systems.iter().map(|_| Seen::default()).collect();
    for (k, map, run) in &runs {
        // At one row per entity, every row value goes with the one row of each other entity,
        // so the twin's ∀∃ guesses would be many and mostly false: they come from the run's.
        record(systems[*k], map, run, &counted, *k == 0, &mut seen[*k]);
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

/// What one cone's run reached, added to `seen` in `ts`'s ids. Families are kept only for the
/// keys in `counted`, the ones a guard counts children by, and ∀∃ facts only `with_exists`.
fn record(
    ts: &TransitionSystem,
    map: &Remap,
    run: &ComponentRun,
    counted: &BTreeSet<VarId>,
    with_exists: bool,
    seen: &mut Seen,
) {
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
    // Per row value some present row took (an atom, by index): the atoms of other entities
    // present in every state that showed it, in this cone, as bits.
    let mut universe: Vec<Atom> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    let mut offsets: Vec<Vec<usize>> = Vec::new();
    if with_exists {
        for (i, (_, _, sub_vars, whole_vars)) in entities.iter().enumerate() {
            let mut of_entity = Vec::new();
            for (v, w) in sub_vars.iter().zip(whole_vars) {
                of_entity.push(universe.len());
                for x in 0..sys.var(*v).domain.size(sys) {
                    universe.push((*w, id8(x)));
                    owner.push(i);
                }
            }
            offsets.push(of_entity);
        }
    }
    let words = universe.len().div_ceil(64);
    let mut own = vec![vec![0u64; words]; entities.len()];
    for (a, i) in owner.iter().enumerate() {
        own[*i][a / 64] |= 1 << (a % 64);
    }
    let mut along: Vec<Option<Vec<u64>>> = vec![None; universe.len()];
    for s in &run.states {
        if with_exists {
            let mut bits = vec![0u64; words];
            for (i, (se, _, sub_vars, _)) in entities.iter().enumerate() {
                for slot in (0..sys.slots(*se)).filter(|&k| s.is_present(sys, *se, k)) {
                    for (j, v) in sub_vars.iter().enumerate() {
                        let a = offsets[i][j] + usize::from(s.get(sys, *v, slot).0);
                        bits[a / 64] |= 1 << (a % 64);
                    }
                }
            }
            for (wi, word) in bits.iter().enumerate() {
                let mut rest = *word;
                while rest != 0 {
                    let a = wi * 64 + rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    match &mut along[a] {
                        Some(w) => w.iter_mut().zip(&bits).for_each(|(x, y)| *x &= y),
                        none => {
                            let of_row = &own[owner[a]];
                            *none = Some(bits.iter().zip(of_row).map(|(x, m)| x & !m).collect());
                        }
                    }
                }
            }
        }
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
            if counted.contains(&back[&o]) {
                let families = seen
                    .families
                    .entry((back[&o], c.3.clone(), p.3.clone()))
                    .or_default();
                for ps in (0..sys.slots(parent)).filter(|&k| s.is_present(sys, parent, k)) {
                    let mut kids: Vec<Pattern> = linked
                        .iter()
                        .filter(|(_, p)| usize::from(*p) == ps)
                        .map(|(k, _)| pattern(&s, &c.2, *k))
                        .collect();
                    kids.sort();
                    families.insert((pattern(&s, &p.2, ps), kids));
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
    if !with_exists {
        return;
    }
    let held: BTreeSet<VarId> = entities.iter().flat_map(|x| x.3.iter().copied()).collect();
    for (a, w) in along.iter().enumerate() {
        let Some(w) = w else { continue };
        let w: BTreeSet<Atom> = (0..universe.len())
            .filter(|b| w[b / 64] >> (b % 64) & 1 == 1)
            .map(|b| universe[b])
            .collect();
        let (vars, kept) = seen.exists.entry(universe[a]).or_default();
        // A value another cone kept stays only if this cone, when it holds its var, kept it
        // too; one this cone keeps joins only if no cone before held its var.
        kept.retain(|b| !held.contains(&b.0) || w.contains(b));
        kept.extend(w.into_iter().filter(|b| !vars.contains(&b.0)));
        vars.extend(held.iter().copied());
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
    let mut push = |text: String, formula: Formula, out: &mut Vec<Mined>, wait: u8| {
        if done.insert(text.clone()) {
            out.push(Mined {
                text,
                formula,
                wait,
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
                0,
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
                    0,
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
                        0,
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
                u8::from(is_fine),
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
            0,
        );
    }
    // Last, so the candidates before them and the checks they start are as without.
    for m in counts(ts, seen).into_iter().chain(exists(ts, seen)) {
        push(m.text, m.formula, &mut out, m.wait);
    }
    out
}

/// ∀∃ lemmas (#428, A2): while a present row of one entity holds a value, some present row of
/// another holds one, for each pair of values every reached state that showed the first
/// keeps (`Seen::exists`). No key need link the rows (z3_needs_exists_global: while a gate is
/// ajar, some guard is on duty). Only values a state showed are offered, one var each side.
fn exists(ts: &TransitionSystem, seen: &Seen) -> Vec<Mined> {
    let slot = |var: VarId, b: u8| Term::Slot {
        var,
        at: At::Bound(b),
    };
    let eq = |(var, x): Atom, b: u8| Pred::Cmp(slot(var, b), CmpOp::Eq, Term::Lit(Value(x)));
    let one = |(v, x): Atom| {
        format!(
            "{} == {}",
            ts.var_name(v),
            super::value_name(&ts.var(v).domain, i64::from(x))
        )
    };
    let mut out = Vec::new();
    for (a, (_, kept)) in &seen.exists {
        let Some(e) = ts.var_entity(a.0) else {
            continue;
        };
        for b in kept {
            let Some(f) = ts.var_entity(b.0).filter(|f| *f != e) else {
                continue;
            };
            let some = Pred::Exists {
                entity: f,
                body: Box::new(eq(*b, 1)),
            };
            out.push(Mined {
                text: format!(
                    "while a present {} row has {}, some present {} row has {}",
                    ts.entity(e).id,
                    one(*a),
                    ts.entity(f).id,
                    one(*b)
                ),
                formula: Formula {
                    binders: vec![e],
                    body: Pred::Or(vec![Pred::Not(Box::new(eq(*a, 0))), some]),
                    all_rows: false,
                },
                wait: 2,
            });
        }
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

/// The largest `n` a count lemma states; the proof unrolls a count into `n + 1` distinct rows,
/// exactly up to 8 (`.at_most(7, ..)` and `.exactly(7, ..)` are the largest of theirs).
const COUNT_MAX: u8 = 7;

/// A count a guard makes of the children of a row its step moves: the key, the child columns
/// the count pins (`var == value`), the bound the guard asks for, and the values the step
/// writes to the parent row. `.has` is at least 1.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Quota {
    key: VarId,
    pins: Vec<(VarId, u8)>,
    /// `true` for at most, `false` for at least.
    at_most: Vec<bool>,
    n: u8,
    after: Vec<(VarId, u8)>,
}

/// Each count a guard of the model makes of the children of a row its step writes. The fact
/// a proof needs is that the count still holds once the step has moved the row
/// (safety_count_guard_order: a shelf closed with at most one copy keeps at most one, since
/// copies come only to open shelves), so each quota keeps the values the step writes.
fn quotas(ts: &TransitionSystem) -> BTreeSet<Quota> {
    let mut out = BTreeSet::new();
    for action in ts.actions() {
        for node in action.guards.iter().flat_map(|g| g.pred.nodes()) {
            let (entity, body, at_most, n) = match node {
                Pred::Count {
                    entity,
                    body,
                    op,
                    n,
                } => {
                    let at_most = match op {
                        CmpOp::Le => vec![true],
                        CmpOp::Ge => vec![false],
                        _ => vec![true, false],
                    };
                    (*entity, body, at_most, *n)
                }
                Pred::Exists { entity, body } => (*entity, body, vec![false], 1),
                _ => continue,
            };
            let mut link = None;
            let mut pins = Vec::new();
            for atom in body.nodes() {
                let Pred::Cmp(Term::Slot { var, .. }, CmpOp::Eq, rhs) = atom else {
                    continue;
                };
                match (&ts.var(*var).kind, rhs) {
                    (VarKind::Owner { child, .. }, Term::Index(At::Param(p)))
                        if *child == entity =>
                    {
                        link = Some((*var, *p));
                    }
                    (_, Term::Lit(Value(x)))
                        if ts.var_entity(*var) == Some(entity) && is_row_value(ts, *var) =>
                    {
                        pins.push((*var, *x));
                    }
                    _ => {}
                }
            }
            let Some((key, param)) = link else {
                continue;
            };
            let mut after = Vec::new();
            for effect in &action.effects {
                match effect {
                    Effect::Assign {
                        var,
                        param: q,
                        rhs: Rhs::Lit(Value(x)),
                        ..
                    } if *q == param => after.push((*var, *x)),
                    Effect::Transition {
                        var,
                        param: q,
                        arms,
                    } if *q == param => {
                        after.extend(arms.iter().map(|arm| (*var, arm.to.0)));
                    }
                    _ => {}
                }
            }
            if after.is_empty() {
                continue;
            }
            pins.sort();
            pins.dedup();
            after.sort();
            after.dedup();
            out.insert(Quota {
                key,
                pins,
                at_most,
                n,
                after,
            });
        }
    }
    out
}

/// Count lemmas (#428, A1): a present parent row with a value a step guarded by a count of
/// its children writes has at most or at least the guard's `n` present children by that key,
/// all of them or those the count pins (`quotas`). Each is offered only when every family of
/// a parent and its children that BFS reached keeps it, which a parent value no state reached
/// does: the count a guard asks for often needs more rows than the run's bounds allow.
fn counts(ts: &TransitionSystem, seen: &Seen) -> Vec<Mined> {
    let mut out = Vec::new();
    for quota in quotas(ts) {
        let VarKind::Owner { child, parent, .. } = ts.var(quota.key).kind else {
            continue;
        };
        for ((o, cvars, pvars), families) in &seen.families {
            if *o != quota.key {
                continue;
            }
            // The pins as positions in the child's pattern; a pin on a column the cone does
            // not hold cannot be read from its states.
            let Some(pins): Option<Vec<(usize, u8)>> = quota
                .pins
                .iter()
                .map(|(v, x)| Some((cvars.iter().position(|c| c == v)?, *x)))
                .collect()
            else {
                continue;
            };
            let mut options = vec![Vec::new()];
            if !pins.is_empty() {
                options.push(pins);
            }
            for kid_pins in options {
                for &(var, x) in &quota.after {
                    let Some(j) = pvars.iter().position(|v| *v == var) else {
                        continue;
                    };
                    let held: Vec<usize> = families
                        .iter()
                        .filter(|(pp, _)| pp[j] == x)
                        .map(|(_, kids)| {
                            kids.iter()
                                .filter(|k| kid_pins.iter().all(|(i, x)| k[*i] == *x))
                                .count()
                        })
                        .collect();
                    for &at_most in &quota.at_most {
                        let n = quota.n;
                        let (op, fits, keeps) = if at_most {
                            let keeps = held.iter().all(|&c| c <= usize::from(n));
                            (CmpOp::Le, n <= COUNT_MAX, keeps)
                        } else {
                            let keeps = held.iter().all(|&c| c >= usize::from(n));
                            (CmpOp::Ge, (1..=COUNT_MAX + 1).contains(&n), keeps)
                        };
                        if !fits || !keeps {
                            continue;
                        }
                        let pinned: Vec<(VarId, u8)> =
                            kid_pins.iter().map(|(i, x)| (cvars[*i], *x)).collect();
                        let ends = (child, parent);
                        out.push(count_lemma(
                            ts,
                            ends,
                            quota.key,
                            Some((var, x)),
                            &pinned,
                            op,
                            n,
                        ));
                    }
                }
            }
        }
    }
    out
}

/// `∀ parent [with pvar == value]. count(child by key [with pins]) op n`, with its text.
fn count_lemma(
    ts: &TransitionSystem,
    (child, parent): (EntId, EntId),
    key: VarId,
    at: Option<(VarId, u8)>,
    pins: &[(VarId, u8)],
    op: CmpOp,
    n: u8,
) -> Mined {
    let slot = |var: VarId, b: u8| Term::Slot {
        var,
        at: At::Bound(b),
    };
    let eq = |var: VarId, b: u8, x: u8| Pred::Cmp(slot(var, b), CmpOp::Eq, Term::Lit(Value(x)));
    let mut kid = vec![Pred::Cmp(
        slot(key, 1),
        CmpOp::Eq,
        Term::Index(At::Bound(0)),
    )];
    kid.extend(pins.iter().map(|(v, x)| eq(*v, 1, *x)));
    let count = Pred::Count {
        entity: child,
        body: Box::new(Pred::And(kid)),
        op,
        n,
    };
    let body = match at {
        Some((v, x)) => Pred::Or(vec![Pred::Not(Box::new(eq(v, 0, x))), count]),
        None => count,
    };
    let one = |v: VarId, x: u8| {
        format!(
            "{} == {}",
            ts.var_name(v),
            super::value_name(&ts.var(v).domain, i64::from(x))
        )
    };
    let parent_text = match at {
        Some((v, x)) => format!(" with {}", one(v, x)),
        None => String::new(),
    };
    let pins_text = match pins {
        [] => String::new(),
        pins => format!(
            " with {}",
            pins.iter()
                .map(|(v, x)| one(*v, *x))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let bound = match op {
        CmpOp::Le => "at most",
        _ => "at least",
    };
    Mined {
        text: format!(
            "a present {} row{parent_text} has {bound} {n} present {} rows{pins_text} by {}",
            ts.entity(parent).id,
            ts.entity(child).id,
            ts.var_name(key)
        ),
        formula: Formula {
            binders: vec![parent],
            body,
            all_rows: false,
        },
        wait: 1,
    }
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
    use crate::lifecycle::tests::system_from;
    use crate::lifecycle::Bounds;

    /// The count lemmas `mine` offers on `src` at `verify`'s bounds, by text.
    fn count_lemmas(src: &str) -> Vec<String> {
        let ts = system_from(src, Bounds::VERIFY);
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let mut texts: Vec<String> = mine(&ts, None, &rules)
            .into_iter()
            .filter(|m| m.text.contains(" has at most ") || m.text.contains(" has at least "))
            .map(|m| {
                assert_eq!(m.wait, 1, "{}", m.text);
                m.text
            })
            .collect();
        texts.sort();
        texts
    }

    /// The guards' bounds give the counts, at the values their steps write, though no state at
    /// 2 instances has a third copy: Close (at most two) closes the shelf, Crowd (at least
    /// three) crowds it.
    #[test]
    fn a_count_guard_gives_a_count_lemma_past_the_states() {
        let src = include_str!("../../../../rdra-ish-cli/tests/engine_corpus/z3_needs_count.rdra");
        assert_eq!(
            count_lemmas(src),
            [
                "a present Shelf row with crowded == true has at least 3 present Copy rows by Copy._owner_Shelf",
                "a present Shelf row with status == closed has at most 2 present Copy rows by Copy._owner_Shelf",
            ]
        );
    }

    /// An `exists` guard is a count of at least one, of all the children and of those it
    /// pins, at the value its step writes (ready); the value a later step writes (done) gets
    /// none.
    #[test]
    fn an_exists_guard_gives_an_at_least_one_lemma() {
        let src = include_str!("../../../../rdra-ish-cli/tests/engine_corpus/z3_needs_exists.rdra");
        assert_eq!(
            count_lemmas(src),
            [
                "a present Order row with status == ready has at least 1 present Shipment rows by Shipment._owner_Order",
                "a present Order row with status == ready has at least 1 present Shipment rows with status == sent by Shipment._owner_Order",
            ]
        );
    }

    /// The ∀∃ lemmas `mine` offers on `src` at `verify`'s bounds, by text.
    fn exists_lemmas(src: &str) -> Vec<String> {
        let ts = system_from(src, Bounds::VERIFY);
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let mut texts: Vec<String> = mine(&ts, None, &rules)
            .into_iter()
            .filter(|m| m.text.starts_with("while "))
            .map(|m| {
                assert_eq!(m.wait, 2, "{}", m.text);
                m.text
            })
            .collect();
        texts.sort();
        texts
    }

    /// Every reached state with an ajar gate has a guard on duty, though no key links a gate
    /// to a guard; a shut gate has none in the states before the first guard is hired.
    #[test]
    fn rows_no_key_links_give_a_forall_exists_lemma() {
        let src = include_str!(
            "../../../../rdra-ish-cli/tests/engine_corpus/z3_needs_exists_global.rdra"
        );
        let texts = exists_lemmas(src);
        assert!(
            texts.contains(
                &"while a present Gate row has state == ajar, some present Guard row has state == on"
                    .to_string()
            ),
            "{texts:#?}"
        );
        assert!(
            !texts
                .iter()
                .any(|t| t.starts_with("while a present Gate row has state == shut")),
            "{texts:#?}"
        );
    }

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
