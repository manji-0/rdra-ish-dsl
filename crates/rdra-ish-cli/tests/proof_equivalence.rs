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
use std::path::PathBuf;

use rdra_ish_core::lifecycle::prove::{self, ProveOptions, Unproved, Verdict, Z3};
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

/// No rule `verdicts` proves has a violation BFS reaches at `bounds`.
fn assert_no_proved_rule_violated(
    sample: &str,
    verdicts: &BTreeMap<String, &Verdict>,
    bounds: Bounds,
) {
    let (instances, int_max) = (bounds.instances, bounds.int_max);
    let Some(bounded) = system(sample, bounds) else {
        return;
    };
    for name in &bounded_verdicts(&bounded).0 {
        assert!(
            !matches!(verdicts.get(name), Some(Verdict::Proved)),
            "{sample}: {name} is proved for any number of rows and any Int value, but BFS at {instances} instances and Int 0..{int_max} reaches a violation"
        );
    }
}

/// Also: every rule of a domain sample is proved at `verify`'s default two instances, as at
/// one. oidc-auth's browser flow passes the mining cap at two, and two of its rules were
/// proved only at one until the prover mined at one instance too (#231).
#[test]
fn no_proved_rule_has_a_bounded_counterexample() {
    let Some(z3) = find_z3() else { return };
    let mut proved_total = 0;
    let mut domain_unproved = Vec::new();
    for sample in samples() {
        let Some((ts, twin)) = proving(&sample) else {
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
        assert_no_proved_rule_violated(&sample, &verdicts, wider_int());
    }
    assert!(proved_total > 0, "no sample rule was proved");
    assert_eq!(
        domain_unproved,
        Vec::<String>::new(),
        "domain sample rules left unproved"
    );
}

/// The engine-corpus cases, proved at `verify`'s bounds without their flags, against BFS at
/// one, two, and three instances and at a wider Int range. The corpus holds the models built
/// to split the engines: `z3_int_listing_whole_range` passes BFS at 0..5 and fails at 0..7.
#[test]
fn no_proved_rule_has_a_bounded_counterexample_in_the_corpus() {
    let Some(z3) = find_z3() else { return };
    let mut proved_total = 0;
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
            assert_no_proved_rule_violated(&case, &verdicts, bounds);
        }
    }
    assert!(proved_total > 0, "no corpus rule was proved");
}

/// The proof does not depend on how many solvers share its checks: the same verdicts, the
/// same counterexamples to induction, and the same kept lemmas on one solver and on three.
/// A check's resource units depend on what its solver checked before, so this also holds
/// each check that keeps a candidate within its limit (`ProveOptions`). The
/// formal-verification samples cover proved rules, rules that fail in an initial state, and
/// rules with a CTI; insurance-claim-ops has the costliest lemma checks, and oidc-auth the
/// costliest rule checks.
#[test]
fn a_proof_is_the_same_on_any_number_of_solvers() {
    let Some(z3) = find_z3() else { return };
    let mut with_cti = 0;
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
            let mut solvers: Vec<Z3> = (0..solvers)
                .map(|_| Z3::start(&z3).expect("z3 starts"))
                .collect();
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
    }
    assert!(with_cti >= 3, "{with_cti} samples with a CTI");
}

/// A rule BFS refutes is not tried (`verify --prove` passes the rules its safety pass
/// violated), and every other rule keeps its verdict: no inductive set holds a refuted rule,
/// so leaving it out proves the same rules. Tried anyway, the refuted rule is not proved.
/// The CTI rows and the fine lemmas kept may differ, so only the kind of each verdict is
/// compared. The cases: the formal-verification samples, with the rules BFS violates at
/// `verify`'s bounds, and the engine-corpus cases, with the rules their header says BFS
/// refutes (`z3=refuted`), some of them only under the case's flags.
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
            let mut solvers = [(); 2].map(|()| Z3::start(&z3).expect("z3 starts"));
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
