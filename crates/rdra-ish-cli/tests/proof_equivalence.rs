//! The prover and BFS agree on every sample: no rule `verify --prove` proves for any number
//! of rows and any Int value has a reachable violation at one, two, or three instances per
//! entity, or at two with a wider Int range than `verify`'s default. A proof
//! claims more than the bounded engines check, so this keeps the third semantics honest
//! against the explorer. A cone the BFS does not finish under `STATE_CAP` decides nothing and
//! is skipped.
//!
//! Without `z3` on PATH (or `RDRA_Z3`) the test is skipped with a printed reason;
//! `RDRA_REQUIRE_Z3=1` turns the skip into a failure.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rdra_ish_core::lifecycle::prove::{
    self, ProveOptions, Sat, Solver, SolverError, Unproved, Verdict, Z3,
};
use rdra_ish_core::lifecycle::{Bounds, Caps, Scope, StateDiag, Target, TransitionSystem};
use rdra_ish_core::load_project;

#[path = "common/samples.rs"]
mod samples;
use samples::repo;

#[path = "common/tools.rs"]
mod tools;
use tools::{found_or_skip, on_path};

const STATE_CAP: usize = 300_000;

fn samples() -> Vec<String> {
    let mut out = samples::samples();
    out.push("samples/formal-verification-fail/order.rdra".to_string());
    out
}

fn find_z3() -> Option<PathBuf> {
    let found = std::env::var_os("RDRA_Z3")
        .map(PathBuf::from)
        .or_else(|| on_path("z3"));
    found_or_skip(found.is_some(), "z3 is not on PATH", "RDRA_REQUIRE_Z3");
    found
}

fn system(sample: &str, bounds: Bounds) -> Option<TransitionSystem> {
    let project = load_project(&[repo().join(sample)], None).ok()?;
    let built = TransitionSystem::build(&project.model, bounds, &Scope::default());
    (!rdra_ish_core::lifecycle::has_errors(&built.diags)).then_some(built.ts)
}

/// The system `verify --prove` proves at its default bounds, and the twin it also mines
/// lemmas from (`prove::mining_twin`).
fn proving(sample: &str) -> Option<(TransitionSystem, Option<TransitionSystem>)> {
    let project = load_project(&[repo().join(sample)], None).ok()?;
    let built = TransitionSystem::build(&project.model, Bounds::VERIFY, &Scope::default());
    if rdra_ish_core::lifecycle::has_errors(&built.diags) {
        return None;
    }
    let twin = prove::mining_twin(&project.model, &Scope::default(), &built.ts);
    Some((built.ts, twin))
}

/// The rules some reachable state violates at `bounds`, and the rules whose cone BFS did not
/// finish.
fn bounded_verdicts(ts: &TransitionSystem) -> (Vec<String>, Vec<String>) {
    let targets: Vec<Target> = (0..ts.invariants().len()).map(Target::Invariant).collect();
    let run = ts.explore_targets(&targets, Caps::new(usize::MAX, STATE_CAP));
    let violated = run
        .verdicts()
        .iter()
        .filter_map(|d| match &d.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let open = targets
        .iter()
        .filter(|t| !run.cone_of(**t).run.complete)
        .map(|t| match t {
            Target::Invariant(i) => ts.invariants()[*i].name.clone(),
            _ => unreachable!(),
        })
        .collect();
    (violated, open)
}

fn instances(n: u8) -> Bounds {
    Bounds::uniform(NonZeroU8::new(n).unwrap(), Bounds::VERIFY.int_max)
}

/// `verify`'s default instance count with two more Int values than its default range. A
/// proof covers every Int value, so a rule it proves has no violation here either; a build
/// that reads the range into a rule (an `in` listing every value up to the top, #425) does.
fn wider_int() -> Bounds {
    Bounds::uniform(Bounds::VERIFY.instances, Bounds::VERIFY.int_max + 2)
}

/// The engine-corpus cases, by path.
fn corpus_cases() -> Vec<String> {
    let corpus = repo().join("crates/rdra-ish-cli/tests/engine_corpus");
    let mut cases: Vec<String> = std::fs::read_dir(&corpus)
        .expect("the engine corpus")
        .map(|e| e.expect("a corpus entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    cases.sort();
    cases
}

/// No rule `verdicts` proves has a violation BFS reaches at `bounds`. Returns the proved rules
/// it could not compare, those whose cone BFS did not finish under `STATE_CAP`, as
/// `sample: rule at bounds`, for the caller to report: a skipped comparison is no evidence.
fn assert_no_proved_rule_violated(
    sample: &str,
    verdicts: &BTreeMap<String, &Verdict>,
    bounds: Bounds,
) -> Vec<String> {
    let (instances, int_max) = (bounds.instances, bounds.int_max);
    let Some(bounded) = system(sample, bounds) else {
        return Vec::new();
    };
    let (violated, open) = bounded_verdicts(&bounded);
    for name in &violated {
        assert!(
            !matches!(verdicts.get(name), Some(Verdict::Proved)),
            "{sample}: {name} is proved for any number of rows and any Int value, but BFS at {instances} instances and Int 0..{int_max} reaches a violation"
        );
    }
    skipped(
        sample,
        verdicts,
        &open,
        &format!("{instances} instances, Int 0..{int_max}"),
    )
}

/// The proved rules among `open`, whose cone BFS did not finish, as `sample: rule at what`.
fn skipped(
    sample: &str,
    verdicts: &BTreeMap<String, &Verdict>,
    open: &[String],
    what: &str,
) -> Vec<String> {
    open.iter()
        .filter(|name| matches!(verdicts.get(*name), Some(Verdict::Proved)))
        .map(|name| format!("{sample}: {name} at {what}"))
        .collect()
}

/// Prints the comparisons a test skipped (`assert_no_proved_rule_violated`), so a run says
/// how much of its evidence it did not gather.
fn report_skipped(what: &str, skipped: &[String]) {
    println!(
        "{what}: {} comparisons of a proved rule skipped, its cone stopped at {STATE_CAP} states{}",
        skipped.len(),
        skipped
            .iter()
            .map(|s| format!("\n  {s}"))
            .collect::<String>()
    );
}

/// One `check-sat`: the limit it ran under, its answer, the z3 resource units it spent, and
/// for an `unknown` whether z3 ran out of its limit (`canceled`) rather than gave up on its own.
type Spent = (u32, Sat, u64, bool);

/// A z3 that records what each `check-sat` spent (`examples/prove_rlimit.rs` measures the
/// same way). `get-info :rlimit` reads the units the process has spent so far, and
/// `get-info :reason-unknown` why the last check answered `unknown`; neither changes what the
/// proof sees.
struct Meter {
    z3: Z3,
    log: Arc<Mutex<Vec<Spent>>>,
}

impl Meter {
    fn start(z3: &Path, log: &Arc<Mutex<Vec<Spent>>>) -> Meter {
        Meter {
            z3: Z3::start(z3).expect("z3 starts"),
            log: log.clone(),
        }
    }

    fn units(&mut self) -> Result<u64, SolverError> {
        let out = self.z3.exec("(get-info :rlimit)")?.join(" ");
        out.trim()
            .trim_start_matches("(:rlimit")
            .trim_end_matches(')')
            .trim()
            .parse()
            .map_err(|_| SolverError(format!("cannot read the rlimit: {out}")))
    }

    /// Whether the last `unknown` came from the resource limit: z3 then says `canceled`. One
    /// it gave up on by itself names its reason, `(incomplete quantifiers)` for one.
    fn ran_out(&mut self) -> Result<bool, SolverError> {
        let reason = self.z3.exec("(get-info :reason-unknown)")?.join(" ");
        Ok(reason.contains("canceled") || reason.contains("resource limit"))
    }
}

impl Solver for Meter {
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError> {
        self.z3.exec(commands)
    }

    fn check_sat(&mut self, rlimit: u32) -> Result<Sat, SolverError> {
        let before = self.units()?;
        let sat = self.z3.check_sat(rlimit)?;
        let units = self.units()? - before;
        let ran_out = sat == Sat::Unknown && self.ran_out()?;
        self.log.lock().unwrap().push((rlimit, sat, units, ran_out));
        Ok(sat)
    }
}

/// The headroom of `log` under the limits of `options`, as one line, or why it is too small:
/// every check that kept its candidate (unsat) spent under a tenth of its limit, for the rule
/// and the lemma limit, there was such a check of each kind, no check ran out of its limit,
/// and no check that answered spent nine tenths of it. The tenth is the margin the limits keep
/// for models larger than the samples. A check that runs out answers `unknown` and leaves the
/// unsat checks, so the tenth alone sees a z3 that spends more on the same queries only up to
/// the limit; z3 says `canceled` for a check that ran out, which fails here. A check z3 gave
/// up on by itself (a quantifier it cannot decide: the Houdini rounds run without MBQI) is
/// counted and its units printed, not asserted: what it spends before it stops depends on
/// what its solver checked before, not on the limit.
fn headroom(log: &Mutex<Vec<Spent>>, what: &str, options: ProveOptions) -> Result<String, String> {
    let log = log.lock().unwrap();
    let of = |limit: u32| log.iter().filter(move |(l, ..)| *l == limit);
    let mut line = format!("{what}: {} checks", log.len());
    for (kind, limit) in [("rule", options.rlimit), ("lemma", options.lemma_rlimit)] {
        let ran_out = of(limit).filter(|(.., r)| *r).count();
        if ran_out > 0 {
            return Err(format!(
                "{what}: {ran_out} {kind} check(s) ran out of their limit {limit}; re-measure the limits (`VERIFIED_Z3` in prove/mod.rs)"
            ));
        }
        let unsat = of(limit)
            .filter(|(_, s, ..)| *s == Sat::Unsat)
            .map(|(_, _, units, _)| *units)
            .max()
            .ok_or_else(|| format!("{what}: no {kind} check kept its candidate"))?;
        if unsat * 10 >= u64::from(limit) {
            return Err(format!(
                "{what}: a {kind} check that kept its candidate spent {unsat} z3 resource units, over a tenth of its limit {limit}; re-measure the limits (`VERIFIED_Z3` in prove/mod.rs)"
            ));
        }
        let answered = of(limit)
            .filter(|(_, s, ..)| *s != Sat::Unknown)
            .map(|(_, _, units, _)| *units)
            .max()
            .unwrap_or(0);
        if answered * 10 >= u64::from(limit) * 9 {
            return Err(format!(
                "{what}: a {kind} check spent {answered} z3 resource units, near its limit {limit}; re-measure the limits (`VERIFIED_Z3` in prove/mod.rs)"
            ));
        }
        let gave_up: Vec<u64> = of(limit)
            .filter(|(_, s, ..)| *s == Sat::Unknown)
            .map(|(_, _, units, _)| *units)
            .collect();
        line += &format!(
            "; {kind}: unsat at most {unsat} units (1/{} of the limit), none ran out, {} gave up by itself, at most {} units",
            u64::from(limit) / unsat.max(1),
            gave_up.len(),
            gave_up.iter().max().unwrap_or(&0)
        );
    }
    Ok(line)
}

/// `headroom` under `ProveOptions::default()`, printed, or a failure.
fn assert_headroom(log: &Mutex<Vec<Spent>>, what: &str) {
    match headroom(log, what, ProveOptions::default()) {
        Ok(line) => println!("{line}"),
        Err(why) => panic!("{why}"),
    }
}

/// A check z3 gave up on by itself does not count however much it spent, and one that ran
/// out does however little: the reason, not the units, tells them apart.
#[test]
fn only_a_check_that_ran_out_fails_the_headroom_check() {
    let options = ProveOptions::default();
    let (rule, lemma) = (options.rlimit, options.lemma_rlimit);
    let log = |last: Spent| {
        Mutex::new(vec![
            (rule, Sat::Unsat, 1_000, false),
            (lemma, Sat::Unsat, 1_000, false),
            last,
        ])
    };
    let gave_up = log((lemma, Sat::Unknown, u64::from(lemma) * 95 / 100, false));
    let line = headroom(&gave_up, "gave up", options).expect("a check z3 gave up on passes");
    assert!(line.contains("lemma: unsat at most 1000 units (1/2000 of the limit), none ran out, 1 gave up by itself, at most 1900000 units"), "{line}");
    let ran_out = log((lemma, Sat::Unknown, 10, true));
    let why = headroom(&ran_out, "ran out", options).expect_err("a check that ran out fails");
    assert!(
        why.contains("1 lemma check(s) ran out of their limit 2000000"),
        "{why}"
    );
    let near = log((lemma, Sat::Sat, u64::from(lemma) * 95 / 100, false));
    let why = headroom(&near, "near", options).expect_err("an answer near the limit fails");
    assert!(why.contains("near its limit 2000000"), "{why}");
}

/// The headroom check fails on a limit too low for the samples: with the lemma limit at
/// 50,000, a fortieth of the default, lemma checks run out (z3 says `canceled`) and the check
/// names them. The checks z3 gave up on by itself under the default limits do not count.
#[test]
fn the_headroom_check_fails_when_a_check_runs_out() {
    let Some(z3) = find_z3() else { return };
    let low = ProveOptions {
        lemma_rlimit: 50_000,
        ..ProveOptions::default()
    };
    let sample = "samples/insurance-claim-ops".to_string();
    let (ts, twin) = proving(&sample).expect("insurance-claim-ops builds");
    let rules: Vec<usize> = (0..ts.invariants().len()).collect();
    for (options, fails) in [(low, true), (ProveOptions::default(), false)] {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut solvers = [(); 2].map(|()| Meter::start(&z3, &log));
        prove::prove(
            &ts,
            twin.as_ref(),
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            options,
        )
        .unwrap_or_else(|e| panic!("{sample}: {e}"));
        let ran_out = log.lock().unwrap().iter().filter(|(.., r)| *r).count();
        let result = headroom(&log, &sample, options);
        println!(
            "lemma limit {}: {ran_out} ran out; {result:?}",
            options.lemma_rlimit
        );
        assert_eq!(result.is_err(), fails, "{result:?}");
        assert_eq!(ran_out > 0, fails, "{result:?}");
        if fails {
            let why = result.unwrap_err();
            assert!(
                why.contains("lemma check(s) ran out of their limit 50000"),
                "{why}"
            );
        }
    }
}

/// Also: every rule of a domain sample is proved at `verify`'s default two instances, as at
/// one. oidc-auth's browser flow passes the mining cap at two, and two of its rules were
/// proved only at one until the prover mined at one instance too (#231). And every check that
/// kept a candidate spent under a tenth of its limit and none ran out (`assert_headroom`), so a
/// z3 that spends much more on the same proofs fails here before a rule is left unproved.
#[test]
fn no_proved_rule_has_a_bounded_counterexample() {
    let Some(z3) = find_z3() else { return };
    let mut proved_total = 0;
    let mut not_compared = Vec::new();
    let mut domain_unproved = Vec::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    for sample in samples() {
        let Some((ts, twin)) = proving(&sample) else {
            continue;
        };
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let mut solvers = [(); 2].map(|()| Meter::start(&z3, &log));
        let proof = prove::prove(
            &ts,
            twin.as_ref(),
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            ProveOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{sample}: {e}"));
        let verdicts: BTreeMap<String, &Verdict> = proof
            .rules
            .iter()
            .map(|r| (ts.invariants()[r.inv].name.clone(), &r.verdict))
            .collect();
        proved_total += proof.proved();
        if !sample.contains("formal-verification") {
            domain_unproved.extend(
                verdicts
                    .iter()
                    .filter(|(_, v)| !matches!(v, Verdict::Proved))
                    .map(|(name, _)| format!("{sample}: {name}")),
            );
        }
        for n in 1..=3 {
            let Some(bounded) = system(&sample, instances(n)) else {
                continue;
            };
            let (violated, open) = bounded_verdicts(&bounded);
            not_compared.extend(skipped(
                &sample,
                &verdicts,
                &open,
                &format!("{n} instances"),
            ));
            for name in &violated {
                assert!(
                    !matches!(verdicts.get(name), Some(Verdict::Proved)),
                    "{sample}: {name} is proved for any number of rows, but BFS at {n} instances reaches a violation"
                );
            }
            // A violation in an initial state is one the bounded engines report too, once
            // there are enough rows for every binder.
            for (name, v) in verdicts.iter().filter(|_| n == 3) {
                if let Verdict::Unproved(Unproved::Initial) = v {
                    assert!(
                        violated.contains(name) || open.contains(name),
                        "{sample}: {name} fails in an initial state, but BFS at {n} instances does not report it"
                    );
                }
            }
        }
        not_compared.extend(assert_no_proved_rule_violated(
            &sample,
            &verdicts,
            wider_int(),
        ));
    }
    report_skipped("the samples", &not_compared);
    assert!(proved_total > 0, "no sample rule was proved");
    assert_eq!(
        domain_unproved,
        Vec::<String>::new(),
        "domain sample rules left unproved"
    );
    assert_headroom(&log, "the samples on 2 solvers");
}

/// The engine-corpus cases, proved at `verify`'s bounds without their flags, against BFS at
/// one, two, and three instances and at a wider Int range. The corpus holds the models built
/// to split the engines: `z3_int_listing_whole_range` passes BFS at 0..5 and fails at 0..7.
#[test]
fn no_proved_rule_has_a_bounded_counterexample_in_the_corpus() {
    let Some(z3) = find_z3() else { return };
    let mut proved_total = 0;
    let mut not_compared = Vec::new();
    for case in corpus_cases() {
        let Some((ts, twin)) = proving(&case) else {
            continue;
        };
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let mut solvers = [(); 2].map(|()| Z3::start(&z3).expect("z3 starts"));
        let proof = prove::prove(
            &ts,
            twin.as_ref(),
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            ProveOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{case}: {e}"));
        let verdicts: BTreeMap<String, &Verdict> = proof
            .rules
            .iter()
            .map(|r| (ts.invariants()[r.inv].name.clone(), &r.verdict))
            .collect();
        proved_total += proof.proved();
        for bounds in [instances(1), instances(2), instances(3), wider_int()] {
            not_compared.extend(assert_no_proved_rule_violated(&case, &verdicts, bounds));
        }
    }
    report_skipped("the corpus", &not_compared);
    assert!(proved_total > 0, "no corpus rule was proved");
}

/// The proof does not depend on how many solvers share its checks: the same verdicts, the
/// same counterexamples to induction, and the same kept lemmas on one solver, on three, and
/// (the formal-verification samples) on two.
/// A check's resource units depend on what its solver checked before, so this also holds
/// each check that keeps a candidate under a tenth of its limit (`assert_headroom`) on one
/// solver and on three. The
/// formal-verification samples cover proved rules, rules that fail in an initial state, and
/// rules with a CTI; insurance-claim-ops has the costliest lemma checks, and oidc-auth the
/// costliest rule checks.
#[test]
fn a_proof_is_the_same_on_any_number_of_solvers() {
    let Some(z3) = find_z3() else { return };
    let mut with_cti = 0;
    let logs = [(); 2].map(|()| Arc::new(Mutex::new(Vec::new())));
    for sample in samples().into_iter().filter(|s| {
        s.contains("formal-verification")
            || s.ends_with("insurance-claim-ops")
            || s.ends_with("oidc-auth")
    }) {
        let Some((ts, twin)) = proving(&sample) else {
            continue;
        };
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let proof = |solvers: usize| {
            let log = &logs[usize::from(solvers > 1)];
            let mut solvers: Vec<Meter> = (0..solvers).map(|_| Meter::start(&z3, log)).collect();
            let proof = prove::prove(
                &ts,
                twin.as_ref(),
                &rules,
                &BTreeSet::new(),
                &mut solvers,
                ProveOptions::default(),
            )
            .unwrap_or_else(|e| panic!("{sample}: {e}"));
            format!("{proof:?}")
        };
        let one = proof(1);
        with_cti += usize::from(one.contains("NotInductive"));
        assert_eq!(proof(3), one, "{sample}");
        // Two solvers deal the checks out another way again. The large samples take about
        // 40 s more on two, so only the small ones run there; `examples/prove_rlimit.rs`
        // compares every sample on 1, 2, and 3.
        if sample.contains("formal-verification") {
            assert_eq!(proof(2), one, "{sample} on 2 solvers");
        }
    }
    assert!(with_cti >= 3, "{with_cti} samples with a CTI");
    assert_headroom(&logs[0], "these samples on 1 solver");
    assert_headroom(&logs[1], "these samples on 2 and 3 solvers");
}

/// A rule BFS refutes is not tried (`verify --prove` passes the rules its safety pass
/// violated), and every other rule keeps its verdict: no inductive set holds a refuted rule,
/// so leaving it out proves the same rules. Tried anyway, the refuted rule is not proved.
/// The CTI rows and the fine lemmas kept may differ, so only the kind of each verdict is
/// compared. The cases: the formal-verification samples, with the rules BFS violates at
/// `verify`'s bounds, and the engine-corpus cases, with the rules their header says BFS
/// refutes (`z3=refuted`), some of them only under the case's flags.
///
/// The argument for it assumes z3's answers are exact. An `unknown` from a check that ran out
/// of its limit depends on what its solver checked before, which leaving a rule out changes,
/// so the test also asserts that no check of either proof ran out (z3 says `canceled`): the
/// verdicts it compares rest on no such answer. The checks z3 gives up on by itself (the
/// Houdini rounds run without MBQI, where every `sat` is `unknown`) are counted, not asserted.
/// It also assumes BFS's counterexample is real, which a cone that leaves a raised mark out
/// does not guarantee (P1 in docs/theory/README.md).
#[test]
fn a_refuted_rule_left_out_changes_no_other_verdict() {
    let Some(z3) = find_z3() else { return };
    let kind = |v: &Verdict| match v {
        Verdict::Proved => "proved",
        Verdict::Unproved(Unproved::Initial) => "initial",
        Verdict::Unproved(Unproved::NotInductive(_)) => "not inductive",
        Verdict::Unproved(Unproved::Unknown { .. }) => "unknown",
        Verdict::OutOfScope(_) => "out of scope",
        Verdict::Refuted => "refuted",
    };
    let cases = corpus_cases();
    let samples = samples()
        .into_iter()
        .filter(|s| s.contains("formal-verification"));
    let (mut refuted_total, mut beside) = (0, 0);
    let log = Arc::new(Mutex::new(Vec::new()));
    for sample in samples.chain(cases) {
        let Some((ts, twin)) = proving(&sample) else {
            continue;
        };
        let names: Vec<String> = match sample.ends_with(".rdra") && sample.contains("engine_corpus")
        {
            true => std::fs::read_to_string(&sample)
                .expect("a corpus case")
                .lines()
                .filter_map(|l| l.strip_prefix("// expect "))
                .filter(|l| l.contains(" z3=refuted"))
                .filter_map(|l| l.split_once(':').map(|(c, _)| c.to_string()))
                .collect(),
            false => bounded_verdicts(&ts).0,
        };
        let refuted: BTreeSet<usize> = (0..ts.invariants().len())
            .filter(|i| names.contains(&ts.invariants()[*i].name))
            .collect();
        if refuted.is_empty() {
            continue;
        }
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let proof = |refuted: &BTreeSet<usize>| {
            let mut solvers = [(); 2].map(|()| Meter::start(&z3, &log));
            prove::prove(
                &ts,
                twin.as_ref(),
                &rules,
                refuted,
                &mut solvers,
                ProveOptions::default(),
            )
            .unwrap_or_else(|e| panic!("{sample}: {e}"))
        };
        let (all, left_out) = (proof(&BTreeSet::new()), proof(&refuted));
        for (a, b) in all.rules.iter().zip(&left_out.rules) {
            let name = &ts.invariants()[a.inv].name;
            match refuted.contains(&a.inv) {
                true => {
                    refuted_total += 1;
                    assert!(
                        !matches!(a.verdict, Verdict::Proved),
                        "{sample}: {name} is refuted but proved"
                    );
                    assert!(matches!(b.verdict, Verdict::Refuted), "{sample}: {name}");
                }
                false => {
                    beside += 1;
                    assert_eq!(kind(&a.verdict), kind(&b.verdict), "{sample}: {name}");
                }
            }
        }
    }
    assert!(refuted_total > 0, "no case has a refuted rule");
    assert!(beside > 0, "no case has a rule beside a refuted one");
    let log = log.lock().unwrap();
    let ran_out = log.iter().filter(|(.., r)| *r).count();
    let gave_up = log.iter().filter(|(_, s, ..)| *s == Sat::Unknown).count();
    println!(
        "{} checks, {gave_up} gave up by themselves, {ran_out} ran out",
        log.len()
    );
    assert_eq!(
        ran_out, 0,
        "a check ran out of its limit, so the verdicts compared may rest on an answer that depends on what its solver checked before"
    );
}

/// A certificate (the set a proof kept) offered to the next proof is re-checked, and the
/// proof is the same whether the certificate is reused, refused, or not offered: it is reused
/// only when the encoding is the one it came from, it holds every rule, and one round finds it
/// inductive again; a stale one costs only that round. The formal-verification samples have
/// models whose rules are all proved, where it is reused, and models with an unproved rule,
/// where it never is.
#[test]
fn a_certificate_is_reused_only_when_it_rechecks_and_changes_no_proof() {
    let Some(z3) = find_z3() else { return };
    let (mut reused, mut refused) = (0, 0);
    let mut other: Option<prove::Certificate> = None;
    for sample in samples()
        .into_iter()
        .filter(|s| s.contains("formal-verification"))
    {
        let Some((ts, twin)) = proving(&sample) else {
            continue;
        };
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let run = |offer: Option<&prove::Certificate>| {
            let mut solvers = [(); 2].map(|()| Z3::start(&z3).expect("z3 starts"));
            prove::prove_offered(
                &ts,
                twin.as_ref(),
                &rules,
                &BTreeSet::new(),
                &mut solvers,
                ProveOptions::default(),
                offer,
            )
            .unwrap_or_else(|e| panic!("{sample}: {e}"))
        };
        let fresh = run(None);
        assert!(!fresh.reused, "{sample}");
        let expected = format!("{:?}", fresh.proof);
        // A model with no rule to prove reuses any certificate of its encoding; it counts
        // neither way.
        let unproved = fresh.proof.unproved() > 0;
        let again = run(Some(&fresh.certificate));
        assert_eq!(format!("{:?}", again.proof), expected, "{sample}");
        assert_eq!(again.reused, !unproved, "{sample}");
        assert_eq!(again.certificate, fresh.certificate, "{sample}");
        match (unproved, fresh.proof.proved()) {
            (true, _) => refused += 1,
            (false, 0) => {}
            (false, _) => reused += 1,
        }
        let mut stale = fresh.certificate.clone();
        stale.digest.push('0');
        // With no rule kept: refused, unless there is no rule to hold.
        let no_rule = (fresh.proof.proved() > 0).then(|| prove::Certificate {
            kept: Vec::new(),
            ..fresh.certificate.clone()
        });
        let mut past_end = fresh.certificate.clone();
        past_end.kept.push(usize::MAX);
        for damaged in [Some(stale), no_rule, Some(past_end), other.take()]
            .into_iter()
            .flatten()
        {
            let run = run(Some(&damaged));
            assert!(!run.reused, "{sample}: {damaged:?}");
            assert_eq!(format!("{:?}", run.proof), expected, "{sample}");
        }
        other = Some(fresh.certificate);
    }
    assert!(
        reused >= 3 && refused >= 3,
        "{reused} reused, {refused} refused"
    );
}

/// A z3 that answers `unknown` to its first `check-sat`.
struct GivesUpOnce {
    z3: Z3,
    gave_up: bool,
}

impl Solver for GivesUpOnce {
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError> {
        self.z3.exec(commands)
    }

    fn check_sat(&mut self, rlimit: u32) -> Result<Sat, SolverError> {
        let sat = self.z3.check_sat(rlimit)?;
        match std::mem::replace(&mut self.gave_up, true) {
            false => Ok(Sat::Unknown),
            true => Ok(sat),
        }
    }
}

/// A certificate whose round fails is refused, and the proof that follows on the same solvers
/// is the one Houdini gives without it: the round's first check gives up, so the certificate
/// falls after z3 has run part of the round.
#[test]
fn a_certificate_whose_round_fails_changes_no_proof() {
    let Some(z3) = find_z3() else { return };
    let sample = "samples/formal-verification/check_met.rdra";
    let (ts, twin) = proving(sample).expect("check_met builds");
    let rules: Vec<usize> = (0..ts.invariants().len()).collect();
    let run = |offer: Option<&prove::Certificate>, gave_up: bool| {
        let z3 = Z3::start(&z3).expect("z3 starts");
        let mut solvers = [GivesUpOnce { z3, gave_up }];
        prove::prove_offered(
            &ts,
            twin.as_ref(),
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            ProveOptions::default(),
            offer,
        )
        .expect("the proof runs")
    };
    let fresh = run(None, true);
    assert!(fresh.proof.proved() > 0, "{:?}", fresh.proof);
    assert!(run(Some(&fresh.certificate), true).reused);
    let refused = run(Some(&fresh.certificate), false);
    assert!(!refused.reused);
    assert_eq!(format!("{:?}", refused.proof), format!("{:?}", fresh.proof));
    assert_eq!(refused.certificate, fresh.certificate);
}
