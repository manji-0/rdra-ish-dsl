//! `verify --prove`: the rules z3 proves for any number of rows, printed before TLC runs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::prove::{self, ProveOptions, RuleProof, Unproved, Verdict, Z3};
use rdra_ish_core::lifecycle::{label_prefix, Selection, Target, TransitionSystem};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::sql::{unenforced_rules, Dialect};

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
            "{} of {} rules proved for any number of rows and any Int value",
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

/// What the report says about a rule besides its verdict, by index into `ts.invariants()`.
pub(crate) struct Marks {
    /// The rules the model marks `.lemma()`: proved like any rule, printed as `lemma`, and not
    /// counted among the rules.
    pub lemmas: BTreeSet<usize>,
    /// The rules the SQL export's schema does not enforce, with why: the application keeps
    /// them, so a proof discharges that obligation.
    pub unenforced: BTreeMap<usize, &'static str>,
}

impl Marks {
    pub(crate) fn of(model: &SemanticModel, ts: &TransitionSystem) -> Marks {
        let sql: BTreeMap<String, &'static str> = unenforced_rules(model, Dialect::Postgres)
            .into_iter()
            .collect();
        Marks {
            lemmas: prove::lemma_invariants(model, ts),
            unenforced: ts
                .invariants()
                .iter()
                .enumerate()
                .filter_map(|(i, inv)| Some((i, *sql.get(&inv.name)?)))
                .collect(),
        }
    }
}

/// Proves the selected state rules of `ts` and prints one line per rule.
pub(crate) fn run_proof(
    ts: &TransitionSystem,
    selection: &Selection,
    marks: &Marks,
    z3: Option<PathBuf>,
    deny_unproved: bool,
) -> Result<ProofTally> {
    let lemmas = &marks.lemmas;
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
    let (yours, rules): (Vec<&RuleProof>, Vec<&RuleProof>) =
        proof.rules.iter().partition(|r| lemmas.contains(&r.inv));
    let count =
        |rs: &[&RuleProof], f: fn(&Verdict) -> bool| rs.iter().filter(|r| f(&r.verdict)).count();
    let proved = |v: &Verdict| matches!(v, Verdict::Proved);
    let unproved = |v: &Verdict| matches!(v, Verdict::Unproved(_));
    let out_of_scope = |v: &Verdict| matches!(v, Verdict::OutOfScope(_));

    let solver = match version.as_str() {
        "" => String::new(),
        v => format!(" ({v})"),
    };
    let with_lemmas = match yours.len() {
        0 => String::new(),
        n => format!(" and {n} of your lemmas"),
    };
    println!(
        "proof{solver}: {} state rules{with_lemmas}, for any number of rows and any Int value",
        rules.len()
    );
    for r in yours.iter().chain(&rules) {
        print_verdict(ts, r, marks);
    }
    for lemma in &proof.lemmas {
        println!("lemma: {lemma}");
    }
    for loose in &proof.loose {
        println!("note: not encoded, so the proof allows any value or step there: {loose}");
    }
    println!(
        "proof: {} proved, {} unproved, {} out of scope",
        count(&rules, proved),
        count(&rules, unproved),
        count(&rules, out_of_scope)
    );
    if !yours.is_empty() {
        println!(
            "proof: your lemmas: {} proved, {} unproved, {} out of scope",
            count(&yours, proved),
            count(&yours, unproved),
            count(&yours, out_of_scope)
        );
    }
    let app: Vec<&RuleProof> = rules
        .iter()
        .copied()
        .filter(|r| marks.unenforced.contains_key(&r.inv))
        .collect();
    if !app.is_empty() {
        println!(
            "proof: the SQL schema leaves {} rules to the application: {} proved, {} unproved, {} out of scope",
            app.len(),
            count(&app, proved),
            count(&app, unproved),
            count(&app, out_of_scope)
        );
    }
    Ok(ProofTally {
        proved: count(&rules, proved),
        rules: rules.len(),
        unproved: count(&rules, unproved),
        deny_unproved,
    })
}

/// One rule's line, and the CTI under an unproved one. A lemma the model marks reads
/// `lemma <name>`; a rule the SQL schema does not enforce ends its name with why.
fn print_verdict(ts: &TransitionSystem, r: &RuleProof, marks: &Marks) {
    let inv = &ts.invariants()[r.inv];
    let sql = match marks.unenforced.get(&r.inv) {
        Some(why) => format!(" [not enforced by the SQL schema: {why}]"),
        None => String::new(),
    };
    let name = format!(
        "{}{}{}{sql}",
        if marks.lemmas.contains(&r.inv) {
            "lemma "
        } else {
            ""
        },
        label_prefix(&inv.label),
        inv.name
    );
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
