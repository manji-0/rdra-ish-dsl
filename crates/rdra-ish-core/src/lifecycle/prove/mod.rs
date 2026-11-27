//! Unbounded proofs: which state rules hold for any number of rows (docs/formal-verification.md,
//! "Proofs for any number of rows").
//!
//! Each rule is a candidate invariant over the encoding in `encode`. The prover keeps a set of
//! candidates that is inductive together (Houdini): every candidate holds in every initial
//! state, and no step from a state where all of them hold breaks one. A rule in that set is
//! proved for every slot count. Helper lemmas join the candidates and are kept only when they
//! survive too. Houdini over one fixed set of candidates, with every answer exact, keeps the
//! largest inductive subset of it; this one keeps an inductive set that may be smaller: an
//! `unknown` drops a candidate as a break does, a dropped candidate never returns, the lemmas
//! that wait join only once a rule falls, and each strengthening round starts Houdini over.
//! Only inductiveness matters for soundness; a smaller set costs proofs. A rule outside the set is unproved, with the step that breaks it
//! from a state where the set holds: a counterexample to induction, which need not be
//! reachable. The bounded engines, not this one, find counterexamples.

mod encode;
mod mine;
mod smt;
mod solver;
mod strengthen;

use std::collections::{BTreeMap, BTreeSet};

pub use solver::{Sat, Solver, SolverError, Z3};

use crate::lifecycle::cone::VarSet;
use crate::lifecycle::types::{ActId, CmpOp, Domain, EntId, Pred, Term, VarId, VarKind};
use crate::lifecycle::TransitionSystem;
use encode::{Enc, Formula, Frame, Unsupported};
use smt::{Command, Expr, Opt, Script, Sort};
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
    /// Holds in every system the model builds at any instance counts and any Int range
    /// (docs/theory/z3.md, Theorem 1 over `Sc(M, β)`); the CLI prints that claim
    /// (`prove.rs::Claim`).
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
    /// Some initial state of the encoding violates the rule. Those states hold any number of
    /// rows, and a column that starts at any value holds any Int value, so the violation may
    /// lie past the run's bounds, where the bounded engines do not see it; within them they
    /// report it with its trace.
    Initial,
    /// A step breaks the rule from a state where it and every kept candidate hold.
    NotInductive(Cti),
    /// The solver gave up on the step that `action` names.
    Unknown { action: String },
    /// The premise every check assumes, the declarations and the initial states, is
    /// inconsistent: the canary candidate `false` survived initiation (`premise`), so no rule
    /// is proved; the text says why.
    Premise(String),
}

/// The set a proof kept, for the next proof to offer first (`prove_offered`): which
/// candidates, by index, and the digest of the encoding they are indices into. It is a hint,
/// never a verdict: the next proof re-checks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    /// The hash of what the proof sent z3 before its rounds: the options, the declarations,
    /// the initial states, the steps, and each candidate. Equal digests mean equal rounds.
    pub digest: String,
    /// The candidates the proof kept, in order.
    pub kept: Vec<usize>,
}

/// A proof, the certificate it leaves, and whether the one offered was reused.
#[derive(Clone, Debug)]
pub struct Certified {
    pub proof: Proof,
    pub certificate: Certificate,
    pub reused: bool,
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
    /// The same for a lemma the prover adds (a helper, mined, or CTI lemma). A `.lemma()` rule
    /// of the model is a rule here and gets `rlimit`. A lemma that gives up leaves the set,
    /// which only costs proofs; a check that breaks a candidate is the slow kind, so a low
    /// limit saves the most.
    pub lemma_rlimit: u32,
}

/// The limits are set from `examples/prove_rlimit.rs` on every sample and engine-corpus case
/// under z3 5.1.0, on 1, 2, 3, and 8 solvers: no check that keeps a candidate spent more
/// than 42,000 units for a rule or 102,000 for a lemma, so the limits left about 480 and 20
/// times that. With the Houdini rounds run without MBQI (#421), on every sample on 1 and 4
/// solvers, no such check spent more than 169,000 and 146,000 units: about 120 and 14 times
/// below the limits. With the set asserted through activation literals (#421), on every sample
/// on 4 solvers, at most 158,000 and 144,000: about 120 and 14 times below. Only such a check
/// must fit: a lemma check past its limit drops the
/// lemma, as a step that breaks it does.
impl Default for ProveOptions {
    fn default() -> Self {
        ProveOptions {
            rlimit: 20_000_000,
            lemma_rlimit: 2_000_000,
        }
    }
}

/// The z3 versions the limits above were measured on, as `z3 --version` prints them; the
/// flake pins the first (`flake.nix`, `flake.lock`). A unit is z3's own count of solver work,
/// which a release may count differently or spend differently on the same query, a patch
/// release included, so a version is listed only once it has been measured. To add one: run
/// `examples/prove_rlimit.rs` under it on 1 and 4 solvers, check that no unsat check spends
/// more than a tenth of either limit and that every domain-sample rule is still proved (the
/// `proof_equivalence` tests assert the first on 1, 2, and 3 solvers, the second on 2), and
/// update the figures above.
pub const VERIFIED_Z3: &[&str] = &["5.1.0"];

/// The note `verify --prove` prints when `version`, the line `z3 --version` printed (as
/// `Z3 version 5.1.0 - 64 bit`), names no version in `VERIFIED_Z3`; `None` when it names one.
/// The version must match exactly: see `VERIFIED_Z3`.
pub fn unverified_z3_note(version: &str) -> Option<String> {
    let mut words = version.split_whitespace();
    let number = words
        .by_ref()
        .find(|w| w.eq_ignore_ascii_case("version"))
        .and_then(|_| words.next());
    let verified = VERIFIED_Z3.join(", ");
    match number {
        Some(n) if VERIFIED_Z3.contains(&n) => None,
        Some(n) => Some(format!(
            "note: z3 {n} is not a version the proof's resource limits were measured on ({verified}); a check may give up, or settle, where it would not on those"
        )),
        None => Some(format!(
            "note: cannot read the z3 version (`z3 --version` printed {version:?}); the proof's resource limits were measured on z3 {verified}"
        )),
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
    /// The formula over the pre-state, asserted.
    pre: Script,
    /// The candidate's activation literal in the Houdini rounds, and the commands, sent once
    /// per solver, that declare it and assert it implies the formula over the pre-state.
    act: Expr,
    activation: Script,
    /// z3 resource units a check of it may spend.
    rlimit: u32,
}

/// Why a candidate left the set, and the step that showed it.
#[derive(Clone, Debug)]
enum Fall {
    Initial,
    Step(ActId),
    /// z3 could not answer, and the reason is final: the initial states do not encode.
    Unknown(String),
    /// z3 gave up on a check of the Houdini rounds, which run without MBQI: in the initial
    /// states (`None`) or on a step. The CTI stage asks again with MBQI on.
    GaveUp(Option<ActId>),
}

/// The candidate set: what each candidate fell for, and which of the mined lemmas that wait
/// are in. A lemma that waits (`mine::Mined::wait`) stays outside the set until a rule stays
/// unproved without it: those of wait 1 join first, then those of wait 2.
struct Set<'a> {
    fell: Vec<Option<Fall>>,
    wait: &'a [u8],
    joined: u8,
}

impl Set<'_> {
    fn contains(&self, k: usize) -> bool {
        self.fell[k].is_none() && self.wait[k] <= self.joined
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
    prove_offered(ts, twin, rules, refuted, solvers, options, None).map(|c| c.proof)
}

/// `prove`, offered the certificate an earlier proof left (#421). It is reused when its
/// digest is this proof's, it holds every rule that is a candidate, and one round finds it
/// inductive: every candidate in it holds in the initial states, and no step from a state
/// where all of them hold breaks one. Its rules are then proved by that round, not by the
/// earlier proof. Equal digests mean the earlier proof ran the same rounds, so it kept the
/// same set and the `Proof` is the one Houdini gives; a certificate that fails the round
/// costs only the round, and the proof then runs as without one.
pub fn prove_offered<S: Solver + Send>(
    ts: &TransitionSystem,
    twin: Option<&TransitionSystem>,
    rules: &[usize],
    refuted: &BTreeSet<usize>,
    solvers: &mut [S],
    options: ProveOptions,
    offer: Option<&Certificate>,
) -> Result<Certified, SolverError> {
    assert!(!solvers.is_empty(), "a proof needs a solver");
    if let Some(why) = premise(&mut solvers[0], &Enc { ts }, options.rlimit)? {
        return Ok(unfounded(ts, rules, refuted, why));
    }
    let mined: Vec<(Origin, Formula, u8)> = mine::mine(ts, twin, rules)
        .into_iter()
        .map(|m| (Origin::Lemma(m.text), m.formula, m.wait))
        .collect();
    let mut extra: Vec<(String, Formula)> = Vec::new();
    for round in 0.. {
        let offer = if round == 0 { offer } else { None };
        let (certified, found) =
            prove_round(ts, rules, refuted, solvers, options, offer, &mined, &extra)?;
        let new: Vec<(String, Formula)> = found
            .into_iter()
            .filter(|(t, _)| !extra.iter().any(|(u, _)| u == t))
            .collect();
        if new.is_empty() || round == STRENGTHEN_ROUNDS {
            return Ok(certified);
        }
        extra.extend(new);
    }
    unreachable!("the rounds are bounded")
}

/// The canary of every proof: the candidate `false`, which must fall at initiation. It holds
/// in no state, so it survives initiation only when no initial state exists, when the
/// premise every check assumes, the declarations and the initial states, is inconsistent.
/// Then every check would be `unsat` and every candidate kept, a false rule too, so no rule is
/// proved, and the text says why. The encoding over-approximates the explorer, so the
/// explorer's initial state is one, and an inconsistent premise is a bug of the builder or the
/// encoder. The kept set needs nothing more: every candidate in it holds in every initial
/// state, so a state of the premise is one of the set too. A `.lemma()` rule is a candidate
/// like any other, not an assumption. Initial states the encoding cannot state are no
/// premise: they fail every candidate on their own (`initiation`).
///
/// The canary's initiation check is `check-sat` of the premise itself. It runs as the
/// Houdini rounds run, without MBQI, where z3 answers `unknown` for a premise with a state,
/// and `unknown` drops a candidate as `sat` does: only an `unsat` keeps the canary, so the
/// check cannot raise a false alarm. It runs on its own, under the rule limit, before the
/// rounds, and ends in a reset: a round starts with one too, so the rounds send and spend
/// what they would without it.
fn premise<S: Solver>(
    solver: &mut S,
    enc: &Enc,
    rlimit: u32,
) -> Result<Option<String>, SolverError> {
    let Ok(initial) = enc.initial() else {
        return Ok(None);
    };
    let mut s = Script::from([Command::Reset, Command::SetOption(Opt::Mbqi(false))]);
    s.append(&enc.declarations());
    s.append(&initial);
    solver.exec(s.text())?;
    let sat = solver.check_sat(rlimit)?;
    solver.exec(&Command::Reset.to_string())?;
    Ok(match sat {
        Sat::Sat | Sat::Unknown => None,
        Sat::Unsat => Some(
            "the canary `false` survived initiation: the encoding's declarations and initial states are inconsistent (they have no model), so every check would hold vacuously and no rule is proved; this is a bug in rdra-ish"
                .to_string(),
        ),
    })
}

/// The proof when the premise is inconsistent (`premise`): no rule proved. A rule BFS refuted
/// stays `Refuted` and one the encoding cannot state `OutOfScope`, as in a proof that runs.
fn unfounded(
    ts: &TransitionSystem,
    rules: &[usize],
    refuted: &BTreeSet<usize>,
    why: String,
) -> Certified {
    let enc = Enc { ts };
    let rules = rules
        .iter()
        .map(|&i| {
            let inv = &ts.invariants()[i];
            let formula = Formula {
                binders: inv.binders.clone(),
                body: inv.body.clone(),
                all_rows: false,
            };
            let verdict = match enc.holds(&formula, Frame::Pre) {
                _ if refuted.contains(&i) => Verdict::Refuted,
                Ok(_) => Verdict::Unproved(Unproved::Premise(why.clone())),
                Err(Unsupported(why)) => Verdict::OutOfScope(why),
            };
            RuleProof { inv: i, verdict }
        })
        .collect();
    let mut loose: Vec<String> = ts
        .actions()
        .iter()
        .flat_map(|a| enc.step(a).loose)
        .collect();
    loose.dedup();
    Certified {
        proof: Proof {
            rules,
            lemmas: Vec::new(),
            loose,
        },
        certificate: Certificate {
            digest: String::new(),
            kept: Vec::new(),
        },
        reused: false,
    }
}

/// How many times a proof is run again with the lemmas its CTIs gave (`strengthen`).
const STRENGTHEN_ROUNDS: usize = 2;

/// One proof over the rules, the helper and mined lemmas, and `extra`, the lemmas earlier
/// rounds took from CTIs; with the lemmas this round's CTIs give.
#[allow(clippy::too_many_arguments)]
fn prove_round<S: Solver + Send>(
    ts: &TransitionSystem,
    rules: &[usize],
    refuted: &BTreeSet<usize>,
    solvers: &mut [S],
    options: ProveOptions,
    offer: Option<&Certificate>,
    mined: &[(Origin, Formula, u8)],
    extra: &[(String, Formula)],
) -> Result<(Certified, Vec<(String, Formula)>), SolverError> {
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
            (Origin::Rule(i), f, 0)
        })
        .chain(lemmas(ts).into_iter().map(|(o, f)| (o, f, 0)))
        .chain(mined.iter().cloned())
        .chain(
            extra
                .iter()
                .map(|(t, f)| (Origin::Lemma(t.clone()), f.clone(), 0)),
        );
    let mut wait: Vec<u8> = Vec::new();
    for (origin, formula, waits) in formulas {
        match enc.holds(&formula, Frame::Pre) {
            Ok(holds) => {
                let act = Expr::sym(format!("act{}", cands.len()), Sort::Bool);
                let activation = Script::from([
                    Command::DeclareConst(act.clone()),
                    Command::Assert(smt::implies(act.clone(), holds.clone())),
                ]);
                let pre = Script::from([Command::Assert(holds)]);
                wait.push(waits);
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
                    act,
                    activation,
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

    // The Houdini rounds run without model-based quantifier instantiation (MBQI). Without
    // it z3 proves an `unsat` check as before, far faster, but answers `unknown` where it
    // would find a model: the candidate falls either way. The CTI stage turns MBQI back on
    // to find the step that breaks each rule. On every sample on one solver (#421): 150 s to
    // 68 s of proof, and the same `Proof` (rules proved, CTIs, lemmas kept) on each.
    let declarations = enc.declarations();
    let restart = |mbqi: bool| {
        let mut s = Script::from([Command::Reset, Command::SetOption(Opt::Mbqi(mbqi))]);
        s.append(&declarations);
        s
    };
    // The literals of the lemmas that wait are declared and asserted only when they join: a
    // proof that never needs them sends z3 the same commands whatever of them the model
    // offers, so a new kind of them changes no other proof's checks (#428).
    let last = wait.iter().copied().max().unwrap_or(0);
    let members = |t: u8| -> Vec<usize> { (0..cands.len()).filter(|k| wait[*k] == t).collect() };
    let mut houdini = restart(false);
    for k in members(0) {
        houdini.append(&cands[k].activation);
    }
    // Per wait from 1: the commands that declare and assert its lemmas' literals.
    let joining: Vec<Script> = (1..=last)
        .map(|t| {
            let mut s = Script::default();
            for k in members(t) {
                s.append(&cands[k].activation);
            }
            s
        })
        .collect();
    for solver in solvers.iter_mut() {
        solver.exec(houdini.text())?;
    }

    let initial = enc.initial();
    let digest = digest(
        &enc,
        &cands,
        &wait,
        &declarations,
        &initial,
        &steps,
        options,
    );
    let offered = match (offer, &initial) {
        (Some(offer), Ok(init)) if offer.digest == digest => {
            // A certificate that keeps a lemma that waits needs its literal.
            for (t, script) in (1..=last).zip(&joining) {
                if offer.kept.iter().any(|k| wait.get(*k) == Some(&t)) {
                    for solver in solvers.iter_mut() {
                        solver.exec(script.text())?;
                    }
                }
            }
            reusable(solvers, &enc, &cands, offer, init, &steps, &breaks)?
        }
        _ => None,
    };
    let reused = offered.is_some();
    if offer.is_some() && !reused {
        // The proof runs as without a certificate, on solvers as they were.
        for solver in solvers.iter_mut() {
            solver.exec(houdini.text())?;
        }
    }
    let mut set = Set {
        fell: offered.unwrap_or_else(|| vec![None; cands.len()]),
        wait: &wait,
        joined: if reused { last } else { 0 },
    };
    if !reused {
        // Houdini's special cases, each kept for a reason:
        // - Initiation is checked once per candidate, before the rounds it joins: no round adds
        //   a candidate the initial states have not been checked against.
        // - The mined lemmas that wait stay outside the set, and join (`Set::joined`) only when a
        //   rule fell after initiation: most models prove every rule without them, and they are
        //   many, so most proofs never check them. Until then z3 sees nothing of them, neither
        //   their literals nor their initiation checks. The fine ones join first; the ∀∃ ones
        //   only when a rule falls with the fine ones in.
        // - When some join, the rules that fell after initiation come back: the set that broke
        //   them lacked those lemmas, so its counterexamples may no longer stand.
        // - A step is checked only against the candidates that read what it writes (`breaks`):
        //   a step that writes none of a candidate's vars cannot break it.
        // - The set is asserted once per step, and every candidate the step may break is checked
        //   under it; one that falls still stands for that step's other checks, so the answers
        //   do not depend on the order of the checks or on how many solvers share them.
        // - Each candidate is asserted once per solver behind an activation literal, and a step
        //   asserts the literals of the set (`activate`): the same `Proof` on every sample as
        //   asserting the candidates in each step, with about a seventh of the text (#421).
        //   The CTI stage starts over without the literals, and asserts the set itself.
        // - `unknown` counts as a fall: the kept set stays inductive, and a rule z3 gave up on is
        //   never proved. Without MBQI every `sat` is `unknown` (see above).
        // - The CTIs come from a solver that starts over, with MBQI on: the model z3 gives
        //   depends on what the solver checked before.
        // - Rules and lemmas have their own rlimits (`ProveOptions`): a lemma that gives up only
        //   costs proofs, so its limit is lower.
        //
        // Initiation: every candidate but the lemmas that wait holds in every initial state.
        initiation(solvers, &enc, &cands, &members(0), &initial, &mut set.fell)?;

        // The lemmas that wait: most models prove every rule without them, and they are many.
        let at_start: BTreeSet<usize> = (0..cands.len())
            .filter(|k| set.fell[*k].is_some())
            .collect();
        // Consecution, until a round removes nothing: the kept set is then inductive. When a rule
        // fell after initiation, the next lemmas that wait join, the rules that fell come back
        // (the set that broke one lacked them), and it runs once more.
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
                    let mut scope = step.commands.clone();
                    scope.append(&activate(&cands, &set));
                    let answers = violated_each(solvers, &enc, &cands, &hit, &scope, Frame::Post)?;
                    for (k, sat) in hit.into_iter().zip(answers) {
                        let fall = match sat {
                            Sat::Unsat => continue,
                            Sat::Sat => Fall::Step(ActId::at_index(a)),
                            Sat::Unknown => Fall::GaveUp(Some(ActId::at_index(a))),
                        };
                        set.fell[k] = Some(fall);
                    }
                }
                if set.fallen() == before {
                    break;
                }
            }
            let open = cands.iter().enumerate().any(|(k, c)| {
                matches!(c.origin, Origin::Rule(_))
                    && set.fell[k].is_some()
                    && !at_start.contains(&k)
            });
            // The next wait with a lemma that holds initially joins; none left ends the proof.
            let mut joins = false;
            while open && !joins && set.joined < last {
                set.joined += 1;
                let these = members(set.joined);
                if these.is_empty() {
                    continue;
                }
                for solver in solvers.iter_mut() {
                    solver.exec(joining[usize::from(set.joined) - 1].text())?;
                }
                initiation(solvers, &enc, &cands, &these, &initial, &mut set.fell)?;
                joins = these.iter().any(|k| set.fell[*k].is_none());
            }
            if !joins {
                break;
            }
            for (k, c) in cands.iter().enumerate() {
                if matches!(c.origin, Origin::Rule(_)) && !at_start.contains(&k) {
                    set.fell[k] = None;
                }
            }
        }
    }

    // Each rule that fell: its step again from the kept set plus the rule, for the CTI. The
    // model z3 gives depends on what the solver has checked before, which depends on how many
    // solvers shared the checks, so the CTIs come from a solver that starts over, with MBQI
    // on. A rule z3 gave up on in the initial states is asked about them again.
    let solver = &mut solvers[0];
    solver.exec(restart(true).text())?;
    let mut found: Vec<(String, Formula)> = Vec::new();
    let with_writes: Vec<(Script, VarSet)> = steps
        .iter()
        .zip(&writes)
        .map(|(s, w)| (s.commands.clone(), w.clone()))
        .collect();
    for (k, c) in cands.iter().enumerate() {
        let Origin::Rule(i) = c.origin else { continue };
        let verdict = match &set.fell[k] {
            None => Verdict::Proved,
            Some(Fall::Initial) => Verdict::Unproved(Unproved::Initial),
            Some(Fall::Unknown(action)) => Verdict::Unproved(Unproved::Unknown {
                action: action.clone(),
            }),
            Some(Fall::GaveUp(None)) => {
                let init = initial.as_ref().expect("an initial check ran");
                let mut s = Script::from([Command::Push]);
                s.append(init);
                s.append(&violation(&enc, c, Frame::Pre));
                solver.exec(s.text())?;
                let sat = solver.check_sat(c.rlimit)?;
                solver.exec(&Command::Pop.to_string())?;
                match sat {
                    Sat::Sat => Verdict::Unproved(Unproved::Initial),
                    Sat::Unsat | Sat::Unknown => Verdict::Unproved(Unproved::Unknown {
                        action: "the initial states".to_string(),
                    }),
                }
            }
            Some(Fall::Step(first) | Fall::GaveUp(Some(first))) => {
                let mut cti = None;
                let order = std::iter::once(first.index())
                    .chain((0..steps.len()).filter(|a| *a != first.index()));
                for a in order.filter(|a| breaks(*a, c)) {
                    let mut s = Script::from([Command::Push]);
                    s.append(&steps[a].commands);
                    s.append(&assume(&cands, &set));
                    s.append(&c.pre);
                    s.push(Command::Push);
                    s.append(&violation(&enc, c, Frame::Post));
                    solver.exec(s.text())?;
                    let mut seen = None;
                    if solver.check_sat(c.rlimit)? == Sat::Sat {
                        cti = Some(read_cti(solver, ts, ActId::at_index(a), &c.formula)?);
                        seen = strengthen::diagram(
                            &mut *solver,
                            ts,
                            &cti_rows(ts, ActId::at_index(a), &c.formula),
                        )?;
                    }
                    solver.exec(Script::from([Command::Pop, Command::Pop]).line())?;
                    if let (Some((binders, lits)), Ok(init)) = (seen, &initial) {
                        let mut hyps = assume(&cands, &set);
                        hyps.append(&c.pre);
                        let frame = strengthen::Frame0 {
                            hyps: &hyps,
                            steps: &with_writes,
                            initial: init,
                            rlimit: options.lemma_rlimit,
                        };
                        if let Some(f) =
                            strengthen::generalize(&mut *solver, ts, &frame, &binders, lits)?
                        {
                            let text = format!(
                                "from the CTI of {}: {}",
                                ts.invariants()[i].name,
                                strengthen::text(ts, &f)
                            );
                            found.push((text, f));
                        }
                    }
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

    let kept: Vec<usize> = (0..cands.len()).filter(|k| set.contains(*k)).collect();
    let kept_lemmas = kept
        .iter()
        .filter_map(|k| match &cands[*k].origin {
            Origin::Lemma(text) => Some(text.clone()),
            Origin::Rule(_) => None,
        })
        .collect();
    let proof = Proof {
        rules: rules
            .iter()
            .map(|i| RuleProof {
                inv: *i,
                verdict: verdicts.remove(i).expect("every rule has a verdict"),
            })
            .collect(),
        lemmas: kept_lemmas,
        loose,
    };
    Ok((
        Certified {
            proof,
            certificate: Certificate { digest, kept },
            reused,
        },
        found,
    ))
}

/// The rows a CTI shows: the action's params, then the rule's binders.
fn cti_rows(ts: &TransitionSystem, act: ActId, formula: &Formula) -> Vec<(Expr, EntId)> {
    let action = &ts.actions()[act.index()];
    let mut rows: Vec<(Expr, EntId)> = action
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (encode::param(i, p.entity), p.entity))
        .collect();
    rows.extend(
        formula
            .binders
            .iter()
            .enumerate()
            .map(|(i, e)| (encode::bound(i, *e), *e)),
    );
    rows
}

/// The digest of a proof's rounds (`Certificate::digest`): every command a round may send.
fn digest(
    enc: &Enc,
    cands: &[Candidate],
    wait: &[u8],
    declarations: &Script,
    initial: &Result<Script, Unsupported>,
    steps: &[encode::Step],
    options: ProveOptions,
) -> String {
    let mut h = crate::fingerprint::Fnv::default();
    let mut add = |text: &str| {
        h.add(text.as_bytes());
        h.add(&[0]);
    };
    // Bump the tag with any change to which set the rounds keep that the text hashed here
    // does not show (the order of the checks, how a fall is decided): a certificate of the
    // older rounds then misses instead of standing for a set they no longer keep.
    add("rdra-ish proof certificate 3");
    add(env!("CARGO_PKG_VERSION"));
    add(&format!("{options:?}"));
    add(declarations.text());
    match initial {
        Ok(init) => add(init.text()),
        Err(Unsupported(why)) => add(&format!("unsupported: {why}")),
    }
    for step in steps {
        add(step.commands.text());
    }
    for (k, c) in cands.iter().enumerate() {
        add(&format!("{:?} {} {}", c.origin, c.rlimit, wait[k]));
        add(c.pre.text());
        add(violation(enc, c, Frame::Pre).text());
        add(violation(enc, c, Frame::Post).text());
    }
    h.hex()
}

/// Initiation for the candidates `which`: each that some initial state breaks, or that z3
/// gives up on there, falls.
fn initiation<S: Solver + Send>(
    solvers: &mut [S],
    enc: &Enc,
    cands: &[Candidate],
    which: &[usize],
    initial: &Result<Script, Unsupported>,
    fell: &mut [Option<Fall>],
) -> Result<(), SolverError> {
    match initial {
        Ok(init) => {
            let answers = violated_each(solvers, enc, cands, which, init, Frame::Pre)?;
            for (k, sat) in which.iter().zip(answers) {
                let fall = match sat {
                    Sat::Unsat => continue,
                    Sat::Sat => Fall::Initial,
                    Sat::Unknown => Fall::GaveUp(None),
                };
                fell[*k] = Some(fall);
            }
        }
        Err(Unsupported(why)) => {
            for k in which {
                fell[*k] = Some(Fall::Unknown(format!("the initial states ({why})")));
            }
        }
    }
    Ok(())
}

/// The candidates' falls with `offer`'s set kept, when it is one that holds every rule and one
/// round finds it inductive: initiation over it, then each step against the candidates in it
/// the step may break, under the set. `None` when it fails, which only costs the round.
fn reusable<S: Solver + Send>(
    solvers: &mut [S],
    enc: &Enc,
    cands: &[Candidate],
    offer: &Certificate,
    initial: &Script,
    steps: &[encode::Step],
    breaks: &dyn Fn(usize, &Candidate) -> bool,
) -> Result<Option<Vec<Option<Fall>>>, SolverError> {
    let kept: BTreeSet<usize> = offer.kept.iter().copied().collect();
    let holds_every_rule = (cands.iter().enumerate())
        .all(|(k, c)| !matches!(c.origin, Origin::Rule(_)) || kept.contains(&k));
    if kept.iter().any(|k| *k >= cands.len()) || !holds_every_rule {
        return Ok(None);
    }
    // A candidate outside the certificate is out of the set. None of them is a rule, so the
    // CTI stage, the only reader of why a candidate fell, never reads this `Fall`.
    let fell: Vec<Option<Fall>> = (0..cands.len())
        .map(|k| (!kept.contains(&k)).then_some(Fall::Initial))
        .collect();
    let set = Set {
        fell,
        wait: &vec![0; cands.len()],
        joined: 0,
    };
    let all: Vec<usize> = kept.iter().copied().collect();
    let answers = violated_each(solvers, enc, cands, &all, initial, Frame::Pre)?;
    if answers.iter().any(|sat| *sat != Sat::Unsat) {
        return Ok(None);
    }
    for (a, step) in steps.iter().enumerate() {
        let hit: Vec<usize> = all
            .iter()
            .copied()
            .filter(|k| breaks(a, &cands[*k]))
            .collect();
        if hit.is_empty() {
            continue;
        }
        let mut scope = step.commands.clone();
        scope.append(&activate(cands, &set));
        let answers = violated_each(solvers, enc, cands, &hit, &scope, Frame::Post)?;
        if answers.iter().any(|sat| *sat != Sat::Unsat) {
            return Ok(None);
        }
    }
    Ok(Some(set.fell))
}

/// Asserts the activation literal of every candidate still in the set: in a Houdini round,
/// where each candidate was asserted once behind its literal, the same as asserting the
/// candidates themselves, in a fraction of the text.
fn activate(cands: &[Candidate], set: &Set) -> Script {
    let mut s = Script::default();
    for (_, c) in cands.iter().enumerate().filter(|(k, _)| set.contains(*k)) {
        s.push(Command::Assert(c.act.clone()));
    }
    s
}

/// Asserts every candidate still in the set over the pre-state.
fn assume(cands: &[Candidate], set: &Set) -> Script {
    let mut s = Script::default();
    for (_, c) in cands.iter().enumerate().filter(|(k, _)| set.contains(*k)) {
        s.append(&c.pre);
    }
    s
}

/// The commands that assert some rows violate `c` in `frame`.
fn violation(enc: &Enc, c: &Candidate, frame: Frame) -> Script {
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
    scope: &Script,
    frame: Frame,
) -> Result<Vec<Sat>, SolverError> {
    let hands = solvers.len().min(ks.len()).max(1);
    let mut open = Script::from([Command::Push]);
    open.append(scope);
    let (open, pop) = (open.text(), Command::Pop.to_string());
    let deal = |solver: &mut S, hand: usize| -> Result<Vec<(usize, Sat)>, SolverError> {
        solver.exec(open)?;
        let mut answers = Vec::new();
        for (at, k) in ks.iter().enumerate().skip(hand).step_by(hands) {
            answers.push((at, violated(solver, enc, &cands[*k], frame)?));
        }
        solver.exec(&pop)?;
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
    let mut s = Script::from([Command::Push]);
    s.append(&violation(enc, c, frame));
    solver.exec(s.text())?;
    let sat = solver.check_sat(c.rlimit)?;
    solver.exec(&Command::Pop.to_string())?;
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
    let mut rows: Vec<(smt::Expr, EntId)> = action
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (encode::param(i, p.entity), p.entity))
        .collect();
    rows.extend(
        formula
            .binders
            .iter()
            .enumerate()
            .map(|(i, e)| (encode::bound(i, *e), *e)),
    );
    let enc = Enc { ts };
    let mut marks_read = VarSet::new();
    for g in &action.guards {
        ts.pred_reads(&g.pred, &mut marks_read);
    }
    ts.pred_reads(&formula.body, &mut marks_read);
    // Per row: its id, then each var before and after.
    let mut terms: Vec<smt::Expr> = Vec::new();
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
        .exec(&Command::GetValue(terms).to_string())?
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

#[cfg(test)]
mod tests {
    use super::unverified_z3_note;

    #[test]
    fn a_verified_z3_prints_no_note() {
        assert_eq!(unverified_z3_note("Z3 version 5.1.0 - 64 bit"), None);
    }

    #[test]
    fn any_other_z3_version_prints_a_note() {
        for (line, number) in [
            ("Z3 version 5.1.1 - 64 bit", "5.1.1"),
            ("Z3 version 5.2.0 - 64 bit", "5.2.0"),
            ("Z3 version 4.13.0 - 64 bit", "4.13.0"),
            ("Z3 version 5.1.0.1 - 64 bit", "5.1.0.1"),
        ] {
            assert_eq!(
                unverified_z3_note(line).as_deref(),
                Some(format!("note: z3 {number} is not a version the proof's resource limits were measured on (5.1.0); a check may give up, or settle, where it would not on those").as_str()),
                "{line}"
            );
        }
    }

    #[test]
    fn an_unreadable_z3_version_prints_a_note() {
        for line in ["", "z3", "Z3 version"] {
            let note = unverified_z3_note(line).unwrap_or_else(|| panic!("{line:?}"));
            assert!(
                note.starts_with("note: cannot read the z3 version"),
                "{note}"
            );
        }
    }
}
