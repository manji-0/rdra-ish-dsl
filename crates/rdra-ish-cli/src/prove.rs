//! `verify --prove`: the rules z3 proves for any number of rows, printed before TLC runs.

use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::prove::{self, Proof, ProveOptions, Unproved, Verdict, Z3};
use rdra_ish_core::lifecycle::{label_prefix, Selection, Target, TransitionSystem};

/// The env variable that names the z3 binary when `--z3` is not given.
pub(crate) const Z3_ENV: &str = "RDRA_Z3";

/// What the closing lines say about the proof.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProofTally {
    pub proved: usize,
    pub rules: usize,
    pub unproved: usize,
    pub deny_unproved: bool,
}

impl ProofTally {
    /// The clause the OK line ends with.
    pub(crate) fn clause(&self) -> String {
        format!(
            "{} of {} rules proved for any number of rows",
            self.proved, self.rules
        )
    }
}

/// `--z3` first, then `RDRA_Z3`, then `z3` on PATH.
fn find_z3(z3: Option<PathBuf>) -> Result<PathBuf> {
    let named = z3.or_else(|| {
        std::env::var_os(Z3_ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    });
    if let Some(path) = named {
        if !path.is_file() {
            bail!("z3 not found at {} (from --z3 or {Z3_ENV})", path.display());
        }
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("z3"))
            .find(|p| p.is_file())
    }) {
        return Ok(path);
    }
    bail!(
        "z3 not found on PATH (--prove needs it)\n\
         Get it one of these ways:\n  \
         - Nix: `nix develop` (the dev shell has it) or `nix shell nixpkgs#z3`\n  \
         - A package manager: `apt install z3`, `brew install z3`, or `pip install z3-solver`\n\
         Then pass `--z3 <path>` or set {Z3_ENV}=<path> if it is not on PATH."
    );
}

/// Proves the selected state rules of `ts` and prints one line per rule.
pub(crate) fn run_proof(
    ts: &TransitionSystem,
    selection: &Selection,
    z3: Option<PathBuf>,
    deny_unproved: bool,
) -> Result<ProofTally> {
    let binary = find_z3(z3)?;
    let version = Z3::version(&binary).unwrap_or_default();
    let rules: Vec<usize> = selection
        .targets
        .iter()
        .filter_map(|t| match t {
            Target::Invariant(i) => Some(*i),
            _ => None,
        })
        .collect();
    let mut solver = Z3::start(&binary)?;
    let proof = prove::prove(ts, &rules, &mut solver, ProveOptions::default())
        .map_err(|e| anyhow::anyhow!("z3 failed: {e}"))?;
    print_proof(ts, &proof, &version);
    Ok(ProofTally {
        proved: proof.proved(),
        rules: proof.rules.len(),
        unproved: proof.unproved(),
        deny_unproved,
    })
}

fn print_proof(ts: &TransitionSystem, proof: &Proof, version: &str) {
    let solver = match version {
        "" => String::new(),
        v => format!(" ({v})"),
    };
    println!(
        "proof{solver}: {} state rules, for any number of rows at the same value bounds",
        proof.rules.len()
    );
    for r in &proof.rules {
        let inv = &ts.invariants()[r.inv];
        let name = format!("{}{}", label_prefix(&inv.label), inv.name);
        match &r.verdict {
            Verdict::Proved => println!("proved: {name}"),
            Verdict::OutOfScope(why) => println!("out of scope: {name}: {why}"),
            Verdict::Unproved(Unproved::Initial) => {
                println!("unproved: {name}: an initial state violates it")
            }
            Verdict::Unproved(Unproved::Unknown { action }) => {
                println!("unproved: {name}: z3 gave up on {action}")
            }
            Verdict::Unproved(Unproved::NotInductive(cti)) => {
                println!(
                    "unproved: {name}: not inductive; {} breaks it from a state where it and every proved rule hold (not necessarily reachable)",
                    cti.step
                );
                for row in &cti.rows {
                    println!("  {row}");
                }
            }
        }
    }
    for lemma in &proof.lemmas {
        println!("lemma: {lemma}");
    }
    for loose in &proof.loose {
        println!("note: not encoded, so the proof allows any value or step there: {loose}");
    }
    println!(
        "proof: {} proved, {} unproved, {} out of scope",
        proof.proved(),
        proof.unproved(),
        proof.out_of_scope()
    );
}
