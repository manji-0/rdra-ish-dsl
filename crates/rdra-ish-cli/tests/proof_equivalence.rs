//! The prover and BFS agree on every sample: no rule `verify --prove` proves for any number
//! of rows has a reachable violation at one, two, or three instances per entity. A proof
//! claims more than the bounded engines check, so this keeps the third semantics honest
//! against the explorer. A cone the BFS does not finish under `STATE_CAP` decides nothing and
//! is skipped.
//!
//! Without `z3` on PATH (or `RDRA_Z3`) the test is skipped with a printed reason;
//! `RDRA_REQUIRE_Z3=1` turns the skip into a failure.

use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};

use rdra_ish_core::lifecycle::prove::{self, ProveOptions, Unproved, Verdict, Z3};
use rdra_ish_core::lifecycle::{Bounds, Caps, Scope, StateDiag, Target, TransitionSystem};
use rdra_ish_core::load_project;

const STATE_CAP: usize = 300_000;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    out.push("samples/formal-verification-fail/order.rdra".to_string());
    out
}

fn find_z3() -> Option<PathBuf> {
    let named = std::env::var_os("RDRA_Z3").map(PathBuf::from);
    let found = named.or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join("z3"))
            .find(|p| p.is_file())
    });
    if found.is_none() {
        let reason = "z3 not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_Z3").as_deref() != Ok("1"),
            "RDRA_REQUIRE_Z3=1 but {reason}"
        );
        println!("skipped: {reason}");
    }
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
    Bounds {
        instances: NonZeroU8::new(n).unwrap(),
        int_max: Bounds::VERIFY.int_max,
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
    }
    assert!(proved_total > 0, "no sample rule was proved");
    assert_eq!(
        domain_unproved,
        Vec::<String>::new(),
        "domain sample rules left unproved"
    );
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
