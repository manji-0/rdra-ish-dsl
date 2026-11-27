//! `proved` against the reference at larger Int ranges (P43's shape). A rule z3 proves at
//! `--int-range-max m` holds, by claim B2 of docs/theory/claims.md, for every slot count
//! and every `m'` at which the model builds. The reference explores the model at `m' > m`
//! without reading the builder's system, so a `proved` that depends on the run's own bound
//! (P43: a `.when`-guarded `present` write listed only the values up to `m`) shows as a
//! reachable violation there.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::interp::RefModel;
use super::{bounds, gen, try_model, Scope, TransitionSystem};
use crate::lifecycle::prove::{prove, ProveOptions, Verdict, Z3};
use crate::model::SemanticModel;

/// `RDRA_Z3`, else `z3` on PATH.
fn z3() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RDRA_Z3") {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("z3"))
        .find(|p| p.is_file())
}

/// The rules z3 proves of `model` at `int_max`, by name.
fn proved(z3: &Path, model: &SemanticModel, int_max: u8) -> BTreeSet<String> {
    let ts = TransitionSystem::build(model, bounds(2, int_max), &Scope::ALL).ts;
    let rules: Vec<usize> = (0..ts.invariants().len()).collect();
    let mut solvers = [Z3::start(z3).expect("z3 starts")];
    let proof = prove(
        &ts,
        None,
        &rules,
        &BTreeSet::new(),
        &mut solvers,
        ProveOptions::default(),
    )
    .expect("z3 answers");
    (proof.rules.iter())
        .filter(|r| matches!(r.verdict, Verdict::Proved))
        .map(|r| ts.invariants()[r.inv].name.clone())
        .collect()
}

/// The proved rules the reference finds broken at one or two instances and `m + 1` or
/// `m + 3`.
fn broken_later(model: &SemanticModel, int_max: u8, proved: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for wider in [1, 3] {
        let m = i64::from(int_max) + wider;
        for n in [1, 2] {
            let Ok(r) = RefModel::read(model, |_| n, m) else {
                continue;
            };
            let Some(x) = r.explore(super::MOST_STATES) else {
                continue;
            };
            for name in x.violated.intersection(proved) {
                out.push(format!(
                    "{name}: proved at {int_max}, broken at {n} instances and {m}"
                ));
            }
        }
    }
    out
}

const GUARDED_PRESENT: &str =
    "entity Item \"item\" {\n  id: Int @pk\n  n: Int @null\n  ok: Bool\n}\n\
    usecase Make \"make\"\nusecase Fill \"fill\"\n\
    creates(Make, Item)\nupdates(Fill, Item)\n\
    condition Open \"open\"\nholds(Open, Item.ok == false)\n\
    sets(Fill, Item.n == present).when(Open)\n\
    forbidden(Item, n > 5)\n";

/// P43: `forbidden(Item, n > 5)` is no theorem, since `Fill` may write any Int, so it must
/// not be proved at 5; the reference breaks it at 6.
#[test]
fn a_rule_proved_at_one_int_range_holds_at_wider_ones() {
    let Some(z3) = z3() else {
        eprintln!("skipped: no z3 on PATH or RDRA_Z3");
        return;
    };
    let model = try_model(GUARDED_PRESENT).expect("the model builds");
    let got = proved(&z3, &model, 5);
    let wrong = broken_later(&model, 5, &got);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The same on random models of the subset, on main only (.config/nextest.toml).
#[test]
fn rules_proved_on_random_models_hold_at_wider_int_ranges() {
    let Some(z3) = z3() else {
        eprintln!("skipped: no z3 on PATH or RDRA_Z3");
        return;
    };
    let (mut checked, mut proofs) = (0, 0);
    let mut wrong = Vec::new();
    for seed in 0..200 {
        let (src, b) = gen::model(seed);
        let Some(model) = try_model(&src) else {
            continue;
        };
        let built = TransitionSystem::build(&model, b.clone(), &Scope::ALL);
        if built
            .diags
            .iter()
            .any(|d| d.diag.severity() == super::Severity::Error)
        {
            continue;
        }
        let got = proved(&z3, &model, b.int_max);
        checked += 1;
        proofs += got.len();
        wrong.extend(
            broken_later(&model, b.int_max, &got)
                .into_iter()
                .map(|w| format!("seed {seed}: {w}")),
        );
    }
    eprintln!("proofs: {checked} models, {proofs} proved rules checked at wider Int ranges");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(
        checked >= 150 && proofs >= 100,
        "{checked} models, {proofs} proofs"
    );
}
