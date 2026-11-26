//! Unbounded proofs: which state rules hold for any number of rows (docs/formal-verification.md,
//! "Proofs for any number of rows").
//!
//! Each rule is a candidate invariant over the encoding in `encode`. The prover keeps the
//! largest set of candidates that is inductive together (Houdini): every candidate holds in
//! every initial state, and no step from a state where all of them hold breaks one. A rule in
//! that set is proved for every slot count. Helper lemmas join the candidates and are kept
//! only when they survive too. A rule outside the set is unproved, with the step that breaks it
//! from a state where the set holds: a counterexample to induction, which need not be
//! reachable. The bounded engines, not this one, find counterexamples.

mod encode;
mod mine;
mod solver;

use std::collections::{BTreeMap, BTreeSet};

pub use solver::{Sat, Solver, SolverError, Z3};

use crate::lifecycle::cone::VarSet;
use crate::lifecycle::types::{ActId, CmpOp, Domain, EntId, Pred, Term, VarId, VarKind};
use crate::lifecycle::TransitionSystem;
use encode::{Enc, Formula, Frame, Unsupported};
use solver::Sexp;

/// What the prover says about each rule it was asked about.
#[derive(Clone, Debug)]
pub struct Proof {
    /// One per rule, in the order asked.
    pub rules: Vec<RuleProof>,
    /// The helper lemmas the proof kept, as text a reader can check.
    pub lemmas: Vec<String>,
    /// Guards and writes the encoding could not express: guards it dropped and writes it
    /// left free. Both only add steps, so a proof stays sound; a rule may fail to prove.
    pub loose: Vec<String>,
}

impl Proof {
    pub fn proved(&self) -> usize {
        self.rules
            .iter()
            .filter(|r| matches!(r.verdict, Verdict::Proved))
            .count()
    }

    pub fn unproved(&self) -> usize {
        self.rules
            .iter()
            .filter(|r| matches!(r.verdict, Verdict::Unproved(_)))
            .count()
    }

    pub fn out_of_scope(&self) -> usize {
        self.rules
            .iter()
            .filter(|r| matches!(r.verdict, Verdict::OutOfScope(_)))
            .count()
    }
}

#[derive(Clone, Debug)]
pub struct RuleProof {
    /// Index into `TransitionSystem::invariants`.
    pub inv: usize,
    pub verdict: Verdict,
}

#[derive(Clone, Debug)]
pub enum Verdict {
    /// Holds for any number of rows, at the stated value bounds.
    Proved,
    Unproved(Unproved),
    /// The encoding cannot state the rule; the reason names the construct.
    OutOfScope(String),
    /// The caller's bounded engine reached a state that violates the rule, so no inductive
    /// set holds it: the prover did not try it.
    Refuted,
}

#[derive(Clone, Debug)]
pub enum Unproved {
    /// Some initial state violates the rule: a real violation, which the bounded engines
    /// report with its trace.
    Initial,
    /// A step breaks the rule from a state where it and every kept candidate hold.
    NotInductive(Cti),
    /// The solver gave up on the step that `action` names.
    Unknown { action: String },
}

/// A counterexample to induction: one step, the rows it reads and writes, and the rule's rows.
#[derive(Clone, Debug)]
pub struct Cti {
    /// The action's name and its rows, as `Ship(Order#1)`.
    pub step: String,
    /// One line per row: its values before, and what the step changed.
    pub rows: Vec<String>,
}

/// Per-query solver settings.
#[derive(Clone, Copy, Debug)]
pub struct ProveOptions {
    /// z3 resource units (`rlimit`) a check of a rule may spend before it counts as unknown.
    /// A unit counts solver work, not time, so the answer does not depend on the machine or
    /// its load; it does depend on the z3 version.
    pub rlimit: u32,
    /// The same for a lemma. A lemma that gives up leaves the set, which only costs proofs;
    /// a check that breaks a candidate is the slow kind, so a low limit saves the most.
    pub lemma_rlimit: u32,
}

/// The limits are set from `examples/prove_rlimit.rs` on every sample and engine-corpus case
/// under z3 5.1.0, on 1, 2, 3, and 8 solvers: no check that keeps a candidate spent more
/// than 42,000 units for a rule or 102,000 for a lemma, so the limits leave about 480 and 20
/// times that. Only such a check must fit: a lemma check past its limit drops the lemma, as
/// a step that breaks it does.
impl Default for ProveOptions {
    fn default() -> Self {
        ProveOptions {
            rlimit: 20_000_000,
            lemma_rlimit: 2_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    Rule(usize),
    Lemma(String),
}

struct Candidate {
    origin: Origin,
    formula: Formula,
    reads: VarSet,
    /// The formula over the pre-state.
    pre: String,
    /// z3 resource units a check of it may spend.
    rlimit: u32,
}

/// Why a candidate left the set, and the step that showed it.
#[derive(Clone, Debug)]
enum Fall {
    Initial,
    Step(ActId),
    Unknown(String),
}

/// The candidate set: what each candidate fell for, and whether the fine mined lemmas are in.
/// A fine lemma waits outside the set until a rule stays unproved without it.
struct Set<'a> {
    fell: Vec<Option<Fall>>,
    fine: &'a BTreeSet<usize>,
    fine_joined: bool,
}

impl Set<'_> {
    fn contains(&self, k: usize) -> bool {
        self.fell[k].is_none() && (self.fine_joined || !self.fine.contains(&k))
    }

    fn fallen(&self) -> usize {
        self.fell.iter().filter(|f| f.is_some()).count()
    }
}

/// The invariants of `ts` that come from a rule the model marks `.lemma()`.
pub fn lemma_invariants(
    model: &crate::model::SemanticModel,
    ts: &TransitionSystem,
) -> BTreeSet<usize> {
    use crate::lifecycle::types::ModelRef;
    use crate::model::RuleRef;
    ts.invariants()
        .iter()
        .enumerate()
        .filter(|(_, inv)| {
            let rule = match inv.site.fact {
                ModelRef::Forbidden(i) => RuleRef::Forbidden(i),
                ModelRef::EntityInvariant(i) => RuleRef::Invariant(i),
                ModelRef::CrossForbidden(i) => RuleRef::CrossForbidden(i),
                ModelRef::CrossInvariant(i) => RuleRef::CrossInvariant(i),
                ModelRef::Quantifier(i) => RuleRef::Quantifier(i),
                _ => return false,
            };
            model.lemma_rules.contains(&rule)
        })
        .map(|(i, _)| i)
        .collect()
}

/// `ts`'s model built again at one instance per entity, for `prove` to mine lemmas from:
/// a cone that passes the mining cap at `ts`'s bounds often finishes there. `None` when `ts`
/// already has one instance, or when the build differs from `ts` in more than its bounds
/// (errors, or other entities, vars, or rules), so the twin's ids are always `ts`'s.
pub fn mining_twin(
    model: &crate::model::SemanticModel,
    scope: &crate::lifecycle::Scope,
    ts: &TransitionSystem,
) -> Option<TransitionSystem> {
    if ts.most_slots() == 1 {
        return None;
    }
    let bounds = ts.bounds.at(std::num::NonZeroU8::MIN);
    let built = TransitionSystem::build(model, bounds, scope);
    let twin = built.ts;
    let same = !crate::lifecycle::has_errors(&built.diags)
        && twin.entity_ids().count() == ts.entity_ids().count()
        && ts
            .entity_ids()
            .all(|e| twin.entity(e).id == ts.entity(e).id)
        && twin.var_ids().count() == ts.var_ids().count()
        && ts
            .var_ids()
            .all(|v| twin.var(v).kind == ts.var(v).kind && twin.var(v).domain == ts.var(v).domain)
        && twin.invariants().len() == ts.invariants().len()
        && ts
            .invariants()
            .iter()
            .zip(twin.invariants())
            .all(|(a, b)| a.name == b.name);
    same.then_some(twin)
}

/// Proves `rules` (indices into `ts.invariants()`) for any number of rows. Lemmas are mined
/// from `ts` and from `twin` (`mining_twin`), when given. The checks of one scope are shared
/// out over `solvers`, at least one; the proof does not depend on how many there are
/// (`violated_each`).
///
/// A rule in `refuted` has a reachable violation, found by a bounded engine: no inductive set
/// that holds in the initial states holds it, so it is no candidate and its verdict is
/// `Refuted`. Leaving it out proves the same rules, since the set Houdini keeps never had it,
/// and spares its checks and the pass with the fine lemmas its fall would start; fewer of
/// those lemmas may then be kept. Its cone is still mined, so the candidates do not change.
pub fn prove<S: Solver + Send>(
    ts: &TransitionSystem,
    twin: Option<&TransitionSystem>,
    rules: &[usize],
    refuted: &BTreeSet<usize>,
    solvers: &mut [S],
    options: ProveOptions,
) -> Result<Proof, SolverError> {
    assert!(!solvers.is_empty(), "a proof needs a solver");
    let enc = Enc { ts };
    let mut verdicts: BTreeMap<usize, Verdict> = BTreeMap::new();
    let mut cands = Vec::new();
    for &i in rules.iter().filter(|i| refuted.contains(i)) {
        verdicts.insert(i, Verdict::Refuted);
    }
    let formulas = rules
        .iter()
        .filter(|i| !refuted.contains(i))
        .map(|&i| {
            let inv = &ts.invariants()[i];
            let f = Formula {
                binders: inv.binders.clone(),
                body: inv.body.clone(),
                all_rows: false,
            };
            (Origin::Rule(i), f, false)
        })
        .chain(lemmas(ts).into_iter().map(|(o, f)| (o, f, false)))
        .chain(
            mine::mine(ts, twin, rules)
                .into_iter()
                .map(|m| (Origin::Lemma(m.text), m.formula, m.fine)),
        );
    let mut fine = BTreeSet::new();
    for (origin, formula, is_fine) in formulas {
        match enc.holds(&formula, Frame::Pre) {
            Ok(pre) => {
                if is_fine {
                    fine.insert(cands.len());
                }
                let mut reads = VarSet::new();
                ts.pred_reads(&formula.body, &mut reads);
                reads.extend(formula.binders.iter().map(|e| ts.entity(*e).exists));
                let rlimit = match origin {
                    Origin::Rule(_) => options.rlimit,
                    Origin::Lemma(_) => options.lemma_rlimit,
                };
                cands.push(Candidate {
                    origin,
                    formula,
                    reads,
                    pre,
                    rlimit,
                });
            }
            Err(Unsupported(why)) => {
                if let Origin::Rule(i) = origin {
                    verdicts.insert(i, Verdict::OutOfScope(why));
                }
            }
        }
    }

    let steps: Vec<_> = ts.actions().iter().map(|a| enc.step(a)).collect();
    let writes: Vec<VarSet> = ts.actions().iter().map(|a| ts.action_writes(a)).collect();
    // A step that writes nothing a candidate reads cannot break it.
    let breaks = |a: usize, c: &Candidate| c.reads.iter().any(|v| writes[a].contains(v));
    let mut loose: Vec<String> = steps.iter().flat_map(|s| s.loose.clone()).collect();
    loose.dedup();

    let declarations = format!("(reset)\n{}", enc.declarations());
    for solver in solvers.iter_mut() {
        solver.exec(&declarations)?;
    }

    let mut set = Set {
        fell: vec![None; cands.len()],
        fine: &fine,
        fine_joined: false,
    };
    // Initiation: every candidate holds in every initial state.
    match enc.initial() {
        Ok(init) => {
            let all: Vec<usize> = (0..cands.len()).collect();
            let answers = violated_each(solvers, &enc, &cands, &all, &init, Frame::Pre)?;
            for (k, sat) in all.into_iter().zip(answers) {
                let fall = match sat {
                    Sat::Unsat => continue,
                    Sat::Sat => Fall::Initial,
                    Sat::Unknown => Fall::Unknown("the initial states".to_string()),
                };
                set.fell[k] = Some(fall);
            }
        }
        Err(Unsupported(why)) => {
            for fall in &mut set.fell {
                *fall = Some(Fall::Unknown(format!("the initial states ({why})")));
            }
        }
    }

    // The fine lemmas wait: most models prove every rule without them, and they are many.
    let at_start: BTreeSet<usize> = (0..cands.len())
        .filter(|k| set.fell[*k].is_some())
        .collect();
    // Consecution, until a round removes nothing: the kept set is then inductive. When a rule
    // fell after initiation, the fine lemmas join, the rules that fell come back (the set that
    // broke one lacked them), and it runs once more.
    loop {
        loop {
            let before = set.fallen();
            for (a, step) in steps.iter().enumerate() {
                let hit: Vec<usize> = (0..cands.len())
                    .filter(|k| set.contains(*k) && breaks(a, &cands[*k]))
                    .collect();
                if hit.is_empty() {
                    continue;
                }
                // The set is asserted once for the step, so a candidate that falls here
                // still stands for the step's other checks, on one solver or on several.
                let scope = format!("{}{}", step.commands, assume(&cands, &set));
                let answers = violated_each(solvers, &enc, &cands, &hit, &scope, Frame::Post)?;
                for (k, sat) in hit.into_iter().zip(answers) {
                    let fall = match sat {
                        Sat::Unsat => continue,
                        Sat::Sat => Fall::Step(ActId::at_index(a)),
                        Sat::Unknown => Fall::Unknown(ts.actions()[a].name.clone()),
                    };
                    set.fell[k] = Some(fall);
                }
            }
            if set.fallen() == before {
                break;
            }
        }
        let open = cands.iter().enumerate().any(|(k, c)| {
            matches!(c.origin, Origin::Rule(_)) && set.fell[k].is_some() && !at_start.contains(&k)
        });
        let waiting = !set.fine_joined && fine.iter().any(|k| !at_start.contains(k));
        if !open || !waiting {
            break;
        }
        set.fine_joined = true;
        for (k, c) in cands.iter().enumerate() {
            if matches!(c.origin, Origin::Rule(_)) && !at_start.contains(&k) {
                set.fell[k] = None;
            }
        }
    }

    // Each rule that fell: its step again from the kept set plus the rule, for the CTI. The
    // model z3 gives depends on what the solver has checked before, which depends on how many
    // solvers shared the checks, so the CTIs come from a solver that starts over.
    let solver = &mut solvers[0];
    solver.exec(&declarations)?;
    for (k, c) in cands.iter().enumerate() {
        let Origin::Rule(i) = c.origin else { continue };
        let verdict = match &set.fell[k] {
            None => Verdict::Proved,
            Some(Fall::Initial) => Verdict::Unproved(Unproved::Initial),
            Some(Fall::Unknown(action)) => Verdict::Unproved(Unproved::Unknown {
                action: action.clone(),
            }),
            Some(Fall::Step(first)) => {
                let mut cti = None;
                let order = std::iter::once(first.index())
                    .chain((0..steps.len()).filter(|a| *a != first.index()));
                for a in order.filter(|a| breaks(*a, c)) {
                    solver.exec(&format!(
                        "(push)\n{}{}(assert {})\n(push)\n{}",
                        steps[a].commands,
                        assume(&cands, &set),
                        c.pre,
                        violation(&enc, c, Frame::Post)
                    ))?;
                    if solver.check_sat(c.rlimit)? == Sat::Sat {
                        cti = Some(read_cti(solver, ts, ActId::at_index(a), &c.formula)?);
                    }
                    solver.exec("(pop)\n(pop)")?;
                    if cti.is_some() {
                        break;
                    }
                }
                match cti {
                    Some(cti) => Verdict::Unproved(Unproved::NotInductive(cti)),
                    None => Verdict::Unproved(Unproved::Unknown {
                        action: ts.actions()[first.index()].name.clone(),
                    }),
                }
            }
        };
        verdicts.insert(i, verdict);
    }

    let kept_lemmas = cands
        .iter()
        .enumerate()
        .filter(|(k, _)| set.contains(*k))
        .filter_map(|(_, c)| match &c.origin {
            Origin::Lemma(text) => Some(text.clone()),
            Origin::Rule(_) => None,
        })
        .collect();
    Ok(Proof {
        rules: rules
            .iter()
            .map(|i| RuleProof {
                inv: *i,
                verdict: verdicts.remove(i).expect("every rule has a verdict"),
            })
            .collect(),
        lemmas: kept_lemmas,
        loose,
    })
}

/// Asserts every candidate still in the set over the pre-state.
fn assume(cands: &[Candidate], set: &Set) -> String {
    cands
        .iter()
        .enumerate()
        .filter(|(k, _)| set.contains(*k))
        .map(|(_, c)| format!("(assert {})\n", c.pre))
        .collect()
}

/// The commands that assert some rows violate `c` in `frame`.
fn violation(enc: &Enc, c: &Candidate, frame: Frame) -> String {
    enc.violation(&c.formula, frame)
        .expect("a candidate that encodes has a violation that encodes")
}

/// Whether some state of `scope` violates each of `cands[ks]` in `frame`: one answer per
/// candidate, in the order of `ks`. The candidates are dealt out to the solvers, each of
/// which opens `scope` for itself. A check leaves nothing in its solver that another reads,
/// so the answers are those one solver gives.
fn violated_each<S: Solver + Send>(
    solvers: &mut [S],
    enc: &Enc,
    cands: &[Candidate],
    ks: &[usize],
    scope: &str,
    frame: Frame,
) -> Result<Vec<Sat>, SolverError> {
    let hands = solvers.len().min(ks.len()).max(1);
    let deal = |solver: &mut S, hand: usize| -> Result<Vec<(usize, Sat)>, SolverError> {
        solver.exec(&format!("(push)\n{scope}"))?;
        let mut answers = Vec::new();
        for (at, k) in ks.iter().enumerate().skip(hand).step_by(hands) {
            answers.push((at, violated(solver, enc, &cands[*k], frame)?));
        }
        solver.exec("(pop)")?;
        Ok(answers)
    };
    let dealt: Vec<_> = if hands == 1 {
        vec![deal(&mut solvers[0], 0)]
    } else {
        std::thread::scope(|threads| {
            let running: Vec<_> = solvers
                .iter_mut()
                .take(hands)
                .enumerate()
                .map(|(hand, solver)| threads.spawn(move || deal(solver, hand)))
                .collect();
            running
                .into_iter()
                .map(|thread| match thread.join() {
                    Ok(answers) => answers,
                    Err(panic) => std::panic::resume_unwind(panic),
                })
                .collect()
        })
    };
    let mut sat = vec![Sat::Unknown; ks.len()];
    for answers in dealt {
        for (at, answer) in answers? {
            sat[at] = answer;
        }
    }
    Ok(sat)
}

/// Whether some state of the open scope violates `c` in `frame`.
fn violated(
    solver: &mut dyn Solver,
    enc: &Enc,
    c: &Candidate,
    frame: Frame,
) -> Result<Sat, SolverError> {
    solver.exec(&format!("(push)\n{}", violation(enc, c, frame)))?;
    let sat = solver.check_sat(c.rlimit)?;
    solver.exec("(pop)")?;
    Ok(sat)
}

/// The helper lemmas every model gets as candidates: an absent row holds base values, since
/// no step writes a row before it is created; no row of an entity that nothing deletes is
/// deleted; and a present row's key never names a parent row that does not exist yet.
fn lemmas(ts: &TransitionSystem) -> Vec<(Origin, Formula)> {
    use crate::lifecycle::types::{At, Value};
    let slot = |var: VarId, b: u8| Term::Slot {
        var,
        at: At::Bound(b),
    };
    let mut out = Vec::new();
    for e in ts.entity_ids() {
        let decl = ts.entity(e);
        let row_vars: Vec<VarId> = ts
            .var_ids()
            .filter(|v| *v != decl.exists && ts.var_entity(*v) == Some(e))
            .collect();
        let body = Pred::Or(vec![
            Pred::Cmp(slot(decl.exists, 0), CmpOp::Ne, Term::Lit(Value::ABSENT)),
            Pred::And(
                row_vars
                    .iter()
                    .map(|v| Pred::Cmp(slot(*v, 0), CmpOp::Eq, Term::Lit(ts.var(*v).base)))
                    .collect(),
            ),
        ]);
        out.push((
            Origin::Lemma(format!("an absent {} row holds base values", decl.id)),
            Formula {
                binders: vec![e],
                body,
                all_rows: true,
            },
        ));
    }
    // No step deletes a row of an entity no action deletes.
    for e in ts.entity_ids() {
        let deleted = ts.actions().iter().any(|a| {
            a.effects.iter().any(|eff| {
                matches!(eff, crate::lifecycle::types::Effect::Delete { param }
                    if a.params[usize::from(*param)].entity == e)
            })
        });
        if deleted {
            continue;
        }
        let decl = ts.entity(e);
        out.push((
            Origin::Lemma(format!("no {} row is ever deleted", decl.id)),
            Formula {
                binders: vec![e],
                body: Pred::Cmp(slot(decl.exists, 0), CmpOp::Ne, Term::Lit(Value::DELETED)),
                all_rows: true,
            },
        ));
    }
    for v in ts.var_ids() {
        let VarKind::Owner { child, parent, .. } = ts.var(v).kind else {
            continue;
        };
        let body = Pred::Or(vec![
            Pred::Cmp(
                slot(ts.entity(child).exists, 0),
                CmpOp::Ne,
                Term::Lit(Value::PRESENT),
            ),
            Pred::Cmp(slot(v, 0), CmpOp::Ne, Term::Index(At::Bound(1))),
            Pred::Cmp(
                slot(ts.entity(parent).exists, 1),
                CmpOp::Ne,
                Term::Lit(Value::ABSENT),
            ),
        ]);
        out.push((
            Origin::Lemma(format!(
                "a present {} row's {} never names an absent {} row",
                ts.entity(child).id,
                ts.var_name(v),
                ts.entity(parent).id
            )),
            Formula {
                binders: vec![child, parent],
                body,
                all_rows: true,
            },
        ));
    }
    out
}

/// The model of an open sat scope, as a CTI: the action's rows and the rule's rows, before and
/// after the step.
fn read_cti(
    solver: &mut dyn Solver,
    ts: &TransitionSystem,
    act: ActId,
    formula: &Formula,
) -> Result<Cti, SolverError> {
    let action = &ts.actions()[act.index()];
    let mut rows: Vec<(String, EntId)> = action
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (encode::param(i), p.entity))
        .collect();
    rows.extend(
        formula
            .binders
            .iter()
            .enumerate()
            .map(|(i, e)| (encode::bound(i), *e)),
    );
    let enc = Enc { ts };
    let mut marks_read = VarSet::new();
    for g in &action.guards {
        ts.pred_reads(&g.pred, &mut marks_read);
    }
    ts.pred_reads(&formula.body, &mut marks_read);
    // Per row: its id, then each var before and after.
    let mut terms: Vec<String> = Vec::new();
    for (row, e) in &rows {
        terms.push(row.clone());
        for v in ts.var_ids().filter(|v| enc.owner(*v) == Some(*e)) {
            terms.push(enc.read(v, Frame::Pre, row));
            terms.push(enc.read(v, Frame::Post, row));
        }
    }
    for e in ts.entity_ids() {
        terms.push(encode::nil(e));
    }
    let out = solver
        .exec(&format!("(get-value ({}))", terms.join(" ")))?
        .join(" ");
    let parsed =
        Sexp::parse(&out).ok_or_else(|| SolverError(format!("cannot read the model: {out}")))?;
    let Sexp::List(pairs) = parsed else {
        return Err(SolverError(format!("cannot read the model: {out}")));
    };
    let values: Vec<Sexp> = pairs
        .into_iter()
        .map(|p| match p {
            Sexp::List(mut kv) if kv.len() == 2 => Ok(kv.remove(1)),
            other => Err(SolverError(format!("cannot read a model value: {other:?}"))),
        })
        .collect::<Result<_, _>>()?;
    let mut names = RowNames::default();
    let nils: Vec<(EntId, Sexp)> = ts
        .entity_ids()
        .zip(values[values.len() - ts.entities().len()..].iter().cloned())
        .collect();
    names.nils = nils;
    let mut at = 0;
    let mut lines = Vec::new();
    let mut seen = BTreeSet::new();
    let mut row_names = Vec::new();
    for (_, e) in &rows {
        let id = values[at].clone();
        at += 1;
        let name = names.name(ts, *e, &id);
        row_names.push(name.clone());
        let mut before = Vec::new();
        let mut changed = Vec::new();
        let mut state = String::new();
        for v in ts.var_ids().filter(|v| enc.owner(*v) == Some(*e)) {
            let (pre, post) = (values[at].clone(), values[at + 1].clone());
            at += 2;
            let decl = ts.var(v);
            let show = |s: &Sexp, names: &mut RowNames| show(ts, &decl.domain, s, names);
            let (pre_s, post_s) = (show(&pre, &mut names), show(&post, &mut names));
            if let VarKind::Exists(_) = decl.kind {
                state = if pre_s == post_s {
                    pre_s
                } else {
                    format!("{pre_s} -> {post_s}")
                };
                continue;
            }
            let label = var_label(ts, v);
            // A run or raised mark is no view column; it shows when the step or the rule
            // reads it, since the CTI may turn on it.
            let in_view = match decl.kind {
                VarKind::Column { .. } | VarKind::Owner { .. } => true,
                VarKind::Run { .. } | VarKind::Raised { .. } => marks_read.contains(&v),
                _ => false,
            };
            if in_view {
                before.push(format!("{label}={pre_s}"));
            }
            if pre_s != post_s {
                changed.push(format!("{label}={post_s}"));
            }
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        let mut line = format!("{name} ({state})");
        if !before.is_empty() {
            line += &format!(" {}", before.join(", "));
        }
        if !changed.is_empty() {
            line += &format!(" -> {}", changed.join(", "));
        }
        lines.push(line);
    }
    let params: Vec<&str> = row_names[..action.params.len()]
        .iter()
        .map(String::as_str)
        .collect();
    Ok(Cti {
        step: format!("{}({})", action.name, params.join(", ")),
        rows: lines,
    })
}

/// Names the solver's row elements `Entity#1`, `Entity#2`, .. in order of first mention.
#[derive(Default)]
struct RowNames {
    seen: BTreeMap<(usize, String), usize>,
    nils: Vec<(EntId, Sexp)>,
}

impl RowNames {
    fn name(&mut self, ts: &TransitionSystem, e: EntId, id: &Sexp) -> String {
        if self.nils.iter().any(|(n, s)| *n == e && s == id) {
            return "null".to_string();
        }
        let key = (e.index(), format!("{id:?}"));
        let next = self.seen.keys().filter(|(k, _)| *k == e.index()).count() + 1;
        let n = *self.seen.entry(key).or_insert(next);
        format!("{}#{n}", ts.entity(e).id)
    }
}

fn show(ts: &TransitionSystem, domain: &Domain, value: &Sexp, names: &mut RowNames) -> String {
    if let Domain::Instance { parent, .. } = domain {
        return names.name(ts, *parent, value);
    }
    match value.int() {
        Some(i) => value_name(domain, i),
        None => format!("{value:?}"),
    }
}

/// A domain index as the model writes the value.
pub(super) fn value_name(domain: &Domain, i: i64) -> String {
    let idx = usize::try_from(i).unwrap_or(usize::MAX);
    let named = |names: &[&str]| names.get(idx).map_or(i.to_string(), |s| s.to_string());
    match domain {
        Domain::Existence => named(&["absent", "present", "deleted"]),
        Domain::Enum(values) => values.get(idx).cloned().unwrap_or(i.to_string()),
        Domain::NullableEnum(values) => match idx {
            0 => "null".to_string(),
            n => values.get(n - 1).cloned().unwrap_or(i.to_string()),
        },
        Domain::Bool => named(&["false", "true"]),
        Domain::NullableBool => named(&["null", "false", "true"]),
        Domain::Nullable => named(&["null", "present"]),
        Domain::Int | Domain::Instance { .. } => i.to_string(),
        Domain::NullableInt => match i {
            0 => "null".to_string(),
            n => (n - 1).to_string(),
        },
    }
}

/// A var as a CTI line names it: the column, or the key's role or parent.
fn var_label(ts: &TransitionSystem, v: VarId) -> String {
    match &ts.var(v).kind {
        VarKind::Column { column, .. } => column.clone(),
        VarKind::Owner { parent, role, .. } => role
            .clone()
            .unwrap_or_else(|| ts.entity(*parent).id.clone()),
        _ => {
            let name = ts.var_name(v);
            match name.split_once('.') {
                Some((_, rest)) => rest.to_string(),
                None => name,
            }
        }
    }
}
