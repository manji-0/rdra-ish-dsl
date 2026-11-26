//! Calibration of the prover's resource limits (`ProveOptions`). For every root in
//! `samples.txt` (or the roots given) it runs the proof at `Bounds::VERIFY` on every rule, with
//! the rule and lemma limits given, and records each z3 check: the z3 resource units
//! (`rlimit`) it spent, its wall time, and its answer. It prints, per root, the proof's digest
//! (the full `Proof`, so two runs compare by it), the verdict counts, and per kind of check the
//! most units an unsat and a sat check spent and how many checks gave up; then the same over
//! all roots. Run it on 1, 2, and more solvers: what a check spends depends on what its solver
//! checked before.
//!
//! Run: `cargo run --release -p rdra-ish-core --example prove_rlimit -- SOLVERS RULE LEMMA [ROOT...]`
//! `RULE` and `LEMMA` are the limits in units, distinct so a check's kind reads off its limit.
//! `RDRA_CHECKS=1` prints every check. z3 comes from `RDRA_Z3`, else `z3` on PATH.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rdra_ish_core::lifecycle::prove::{self, ProveOptions, Sat, Solver, SolverError, Verdict, Z3};
use rdra_ish_core::lifecycle::{Bounds, Scope, TransitionSystem};
use rdra_ish_core::project::load_project;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Rule,
    Lemma,
}

#[derive(Clone, Debug)]
struct Check {
    kind: Kind,
    sat: Sat,
    units: u64,
    ms: u128,
}

/// A z3 that records each `check-sat`: the units it spent and its time.
struct Meter {
    z3: Z3,
    limits: (u32, u32),
    log: Arc<Mutex<Vec<Check>>>,
}

impl Meter {
    fn units(&mut self) -> Result<u64, SolverError> {
        let out = self.z3.exec("(get-info :rlimit)")?.join(" ");
        out.trim()
            .trim_start_matches("(:rlimit")
            .trim_end_matches(')')
            .trim()
            .parse()
            .map_err(|_| SolverError(format!("cannot read the rlimit: {out}")))
    }
}

impl Solver for Meter {
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError> {
        self.z3.exec(commands)
    }

    fn check_sat(&mut self, rlimit: u32) -> Result<Sat, SolverError> {
        let kind = if rlimit == self.limits.0 {
            Kind::Rule
        } else {
            assert_eq!(rlimit, self.limits.1, "a limit that is neither");
            Kind::Lemma
        };
        let before = self.units()?;
        let start = Instant::now();
        let sat = self.z3.check_sat(rlimit)?;
        let ms = start.elapsed().as_millis();
        let units = self.units()? - before;
        self.log.lock().unwrap().push(Check {
            kind,
            sat,
            units,
            ms,
        });
        Ok(sat)
    }
}

fn z3_binary() -> PathBuf {
    std::env::var_os("RDRA_Z3")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|d| d.join("z3"))
                .find(|p| p.is_file())
        })
        .expect("z3 on PATH or RDRA_Z3")
}

fn roots(repo: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_to_string(repo.join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    out.push("samples/formal-verification-fail/order.rdra".to_string());
    out
}

/// The most units a check of `kind` spent that kept its candidate (unsat) and that broke it
/// (sat), with the longest time of each, and the checks that gave up. Only an unsat check
/// must fit the limit: a lemma whose check gives up falls as a broken one does.
fn summary(checks: &[Check], kind: Kind) -> String {
    let of: Vec<&Check> = checks.iter().filter(|c| c.kind == kind).collect();
    let most = |sat: Sat| {
        let answered = of.iter().filter(|c| c.sat == sat);
        let units = answered.clone().map(|c| c.units).max().unwrap_or(0);
        let ms = answered.map(|c| c.ms).max().unwrap_or(0);
        format!("{units} units ({ms} ms)")
    };
    let unknown = of.iter().filter(|c| c.sat == Sat::Unknown).count();
    format!(
        "{} checks, unsat at most {}, sat at most {}, {unknown} gave up",
        of.len(),
        most(Sat::Unsat),
        most(Sat::Sat)
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [solvers, rule, lemma, rest @ ..] = &args[..] else {
        eprintln!("usage: prove_rlimit SOLVERS RULE LEMMA [ROOT...]");
        std::process::exit(2);
    };
    let solvers: usize = solvers.parse().expect("SOLVERS");
    let options = ProveOptions {
        rlimit: rule.parse().expect("RULE"),
        lemma_rlimit: lemma.parse().expect("LEMMA"),
    };
    assert_ne!(
        options.rlimit, options.lemma_rlimit,
        "RULE and LEMMA differ"
    );
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let roots = match rest {
        [] => roots(&repo),
        given => given.to_vec(),
    };
    let z3 = z3_binary();
    let every = std::env::var_os("RDRA_CHECKS").is_some();
    let mut all = Vec::new();
    for root in roots {
        let Ok(project) = load_project(&[repo.join(&root)], None) else {
            println!("{root}: does not load");
            continue;
        };
        let built = TransitionSystem::build(&project.model, Bounds::VERIFY, &Scope::default());
        if rdra_ish_core::lifecycle::has_errors(&built.diags) {
            println!("{root}: build errors, skipped");
            continue;
        }
        let ts = built.ts;
        let twin = prove::mining_twin(&project.model, &Scope::default(), &ts);
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut meters: Vec<Meter> = (0..solvers)
            .map(|_| Meter {
                z3: Z3::start(&z3).expect("z3 starts"),
                limits: (options.rlimit, options.lemma_rlimit),
                log: log.clone(),
            })
            .collect();
        let start = Instant::now();
        let proof = prove::prove(
            &ts,
            twin.as_ref(),
            &rules,
            &Default::default(),
            &mut meters,
            options,
        )
        .unwrap_or_else(|e| panic!("{root}: {e}"));
        let secs = start.elapsed().as_secs_f64();
        let mut h = DefaultHasher::new();
        format!("{proof:?}").hash(&mut h);
        let checks = log.lock().unwrap().clone();
        println!(
            "{root}: digest {:016x}, {} proved, {} unproved, {} out of scope, {} lemmas kept, {secs:.1} s",
            h.finish(),
            proof.proved(),
            proof.unproved(),
            proof.out_of_scope(),
            proof.lemmas.len()
        );
        println!("  rules:  {}", summary(&checks, Kind::Rule));
        println!("  lemmas: {}", summary(&checks, Kind::Lemma));
        for r in &proof.rules {
            let v = match &r.verdict {
                Verdict::Proved => "proved",
                Verdict::Unproved(_) => "unproved",
                Verdict::OutOfScope(_) => "out_of_scope",
                Verdict::Refuted => "refuted",
            };
            println!("  {v}: {}", ts.invariants()[r.inv].name);
        }
        for l in &proof.lemmas {
            println!("  lemma: {l}");
        }
        if every {
            for c in &checks {
                println!(
                    "  check {:?} {:?} {} units {} ms",
                    c.kind, c.sat, c.units, c.ms
                );
            }
        }
        all.extend(checks);
    }
    println!("all rules:  {}", summary(&all, Kind::Rule));
    println!("all lemmas: {}", summary(&all, Kind::Lemma));
}
