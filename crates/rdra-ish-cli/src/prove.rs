//! `verify --prove`: the rules z3 proves for any number of rows, printed before TLC runs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::prove::{self, ProveOptions, RuleProof, Unproved, Verdict, Z3};
use rdra_ish_core::lifecycle::{
    has_errors, label_prefix, Bounds, Scope, Selection, StateDiag, Target, TransitionSystem,
};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::sql::{unenforced_rules, Dialect};

/// `println!` onto the `String` `$text`.
macro_rules! out {
    ($text:expr, $($arg:tt)*) => {{
        $text.push_str(&format!($($arg)*));
        $text.push('\n');
    }};
}

/// The env variable that names the z3 binary when `--z3` is not given.
pub(crate) const Z3_ENV: &str = "RDRA_Z3";

/// What the closing lines say about the proof.
#[derive(Clone, Debug)]
pub(crate) struct ProofTally {
    pub proved: usize,
    pub rules: usize,
    pub unproved: usize,
    pub deny_unproved: bool,
    pub claim: Claim,
}

impl ProofTally {
    /// The clause the OK line ends with.
    pub(crate) fn clause(&self) -> String {
        format!(
            "{} of {} rules proved {}",
            self.proved, self.rules, self.claim.0
        )
    }
}

/// What a proof claims, the one statement every line that says a rule or lemma is proved
/// prints (`for any number of rows and every --int-range-max from 4 to 254, in the whole
/// model`): the bounds `Sc(M, β)` of docs/theory/z3.md, Definition 3, over which Theorem 1
/// says each proved rule holds, and the scope `σ` the system was built under.
///
/// - Rows: the encoding has no bound on them.
/// - Int: every `--int-range-max` at which the build reports no error. The build errors that
///   depend on it each hold on one side of the run's own: below the least one an Int literal
///   of the model is past the range (`IntLiteralOutOfBounds`), above the greatest a column
///   holds more values than a state does (`DomainTooLarge`, past 254 with a nullable Int).
///   A model with no Int column has no range to name.
/// - Scope: a scope flag drops use cases from the system the prover encodes, so a proof there
///   holds in that system, not in the whole model. A counterexample or a CTI under a scope is
///   one of the whole model too, so those lines name no scope.
#[derive(Clone, Debug)]
pub(crate) struct Claim(String);

impl Claim {
    pub(crate) fn of(model: &SemanticModel, scope: &Scope, ts: &TransitionSystem) -> Claim {
        let ints = match int_range(model, scope, ts) {
            Some((least, most)) => {
                format!(" and every --int-range-max from {least} to {most}")
            }
            None => String::new(),
        };
        let sigma = match scope.flags(model) {
            Some(flags) => format!("in the selected scope: {flags}"),
            None => "in the whole model".to_string(),
        };
        Claim(format!("for any number of rows{ints}, {sigma}"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The statement in parentheses, as a `proved:` or `lemma:` line ends with it.
    fn suffix(&self) -> String {
        format!(" ({})", self.0)
    }
}

/// The least and the greatest `--int-range-max` at which the model builds under `scope`, on
/// each side of the run's own, at which it built. `None` when the system has no Int-valued
/// var. Each is first guessed, the least from the literals a build at 0 reports past its
/// range and the greatest as 255 or 254, and the guess is confirmed by a build on each side
/// of it; bisection decides only when a guess fails. So a proof costs about five builds more.
fn int_range(model: &SemanticModel, scope: &Scope, ts: &TransitionSystem) -> Option<(u8, u8)> {
    if !ts.vars().iter().any(|v| v.domain.is_numeric()) {
        return None;
    }
    let build = |int_max: u8| {
        let bounds = Bounds {
            int_max,
            ..ts.bounds.clone()
        };
        TransitionSystem::build(model, bounds, scope).diags
    };
    let builds = |int_max: u8| !has_errors(&build(int_max));
    let m = ts.bounds.int_max;
    let past = build(0)
        .iter()
        .filter_map(|d| match d.diag {
            StateDiag::IntLiteralOutOfBounds { value, .. } => u8::try_from(value).ok(),
            _ => None,
        })
        .max();
    let guess = past.unwrap_or(1).clamp(1, m);
    if builds(guess) && (guess == 1 || !builds(guess - 1)) {
        let most = match (builds(u8::MAX), builds(u8::MAX - 1)) {
            (true, _) => Some(u8::MAX),
            (false, true) => Some(u8::MAX - 1),
            (false, false) => None,
        };
        if let Some(most) = most {
            return Some((guess, most));
        }
    }
    // The least: `builds` is false below it and true from it up to `m`.
    let (mut lo, mut hi) = (1u8, m);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if builds(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let least = lo;
    // The greatest: `builds` is true from `m` up to it and false above.
    let (mut lo, mut hi) = (m, u8::MAX);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if builds(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some((least, lo))
}

/// `--z3` first, then `RDRA_Z3`, then `z3` on PATH.
pub(crate) fn find_z3(z3: Option<PathBuf>) -> Result<PathBuf> {
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
    if let Some(path) = crate::verify::find_on_path("z3") {
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

/// The z3 a proof runs, and the line its `--version` printed (empty when it printed none).
#[derive(Clone, Copy)]
pub(crate) struct Z3Tool<'a> {
    pub binary: &'a Path,
    pub version: &'a str,
}

/// Proves the selected state rules of `ts` and writes one line per rule. Lemmas are mined
/// from `twin` too (`prove::mining_twin`). A rule in `refuted`, which BFS violated at the
/// instances it maps to, is not tried: it reads `refuted:`, and is an error of BFS's, not an
/// unproved rule. Returns what `ProofRun` holds; the caller prints its lines where the proof's
/// place in the report is, whenever the proof ran.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_proof(
    ts: &TransitionSystem,
    twin: Option<&TransitionSystem>,
    selection: &Selection,
    marks: &Marks,
    refuted: &BTreeMap<usize, usize>,
    z3: Z3Tool<'_>,
    deny_unproved: bool,
    claim: Claim,
) -> Result<ProofRun> {
    let within = claim.suffix();
    let scoped = claim.0.contains("in the selected scope");
    let lemmas = &marks.lemmas;
    let mut text = String::new();
    let (binary, version) = (z3.binary, z3.version.to_string());
    let rules: Vec<usize> = selection
        .targets
        .iter()
        .filter_map(|t| match t {
            Target::Invariant(i) => Some(*i),
            _ => None,
        })
        .collect();
    // One z3 per thread `RDRA_JOBS` allows; the proof is the same on any number.
    let mut solvers = (0..rdra_ish_core::jobs())
        .map(|_| Z3::start(binary))
        .collect::<Result<Vec<_>, _>>()?;
    let skip: BTreeSet<usize> = refuted.keys().copied().collect();
    let cache = certificate::file(ts, &rules);
    let offer = cache
        .as_deref()
        .and_then(|file| certificate::read(file, &version));
    let certified = prove::prove_offered(
        ts,
        twin,
        &rules,
        &skip,
        &mut solvers,
        ProveOptions::default(),
        offer.as_ref(),
    )
    .map_err(|e| anyhow::anyhow!("z3 failed: {e}"))?;
    if let (Some(file), false) = (&cache, offer.as_ref() == Some(&certified.certificate)) {
        certificate::write(file, &version, &certified.certificate);
    }
    let proof = certified.proof;
    let (yours, rules): (Vec<&RuleProof>, Vec<&RuleProof>) =
        proof.rules.iter().partition(|r| lemmas.contains(&r.inv));
    let count =
        |rs: &[&RuleProof], f: fn(&Verdict) -> bool| rs.iter().filter(|r| f(&r.verdict)).count();
    let proved = |v: &Verdict| matches!(v, Verdict::Proved);
    let unproved = |v: &Verdict| matches!(v, Verdict::Unproved(_));
    let out_of_scope = |v: &Verdict| matches!(v, Verdict::OutOfScope(_));
    // The counts, and the rules BFS refuted when there are any.
    let counts = |rs: &[&RuleProof]| {
        let in_scope = if scoped { " in the selected scope" } else { "" };
        let refuted = match count(rs, |v| matches!(v, Verdict::Refuted)) {
            0 => String::new(),
            n => format!(", {n} refuted"),
        };
        format!(
            "{} proved{in_scope}, {} unproved, {} out of scope{refuted}",
            count(rs, proved),
            count(rs, unproved),
            count(rs, out_of_scope)
        )
    };

    let solver = match version.as_str() {
        "" => String::new(),
        v => format!(" ({v})"),
    };
    let with_lemmas = match yours.len() {
        0 => String::new(),
        n => format!(" and {n} of your lemmas"),
    };
    out!(
        text,
        "proof{solver}: {} state rules{with_lemmas}, {}",
        rules.len(),
        claim.0
    );
    for r in yours.iter().chain(&rules) {
        print_verdict(&mut text, ts, r, marks, refuted, &within);
    }
    for lemma in &proof.lemmas {
        out!(text, "lemma: {lemma}{within}");
    }
    for loose in &proof.loose {
        out!(
            text,
            "note: not encoded, so the proof allows any value or step there: {loose}"
        );
    }
    out!(text, "proof: {}", counts(&rules));
    if !yours.is_empty() {
        out!(text, "proof: your lemmas: {}", counts(&yours));
    }
    let app: Vec<&RuleProof> = rules
        .iter()
        .copied()
        .filter(|r| marks.unenforced.contains_key(&r.inv))
        .collect();
    if !app.is_empty() {
        out!(
            text,
            "proof: the SQL schema leaves {} rules to the application: {}",
            app.len(),
            counts(&app)
        );
    }
    let tally = ProofTally {
        proved: count(&rules, proved),
        rules: rules.len(),
        unproved: count(&rules, unproved),
        deny_unproved,
        claim,
    };
    let with = |f: fn(&Verdict) -> bool| -> BTreeSet<usize> {
        proof
            .rules
            .iter()
            .filter(|r| f(&r.verdict))
            .map(|r| r.inv)
            .collect()
    };
    let unproved_rules = with(unproved)
        .into_iter()
        .filter(|i| !lemmas.contains(i))
        .collect();
    Ok(ProofRun {
        tally,
        proved: with(proved),
        unproved: unproved_rules,
        text,
    })
}

/// What `run_proof` found.
pub(crate) struct ProofRun {
    pub tally: ProofTally,
    /// The rules proved, lemmas included, by index into `ts.invariants()`.
    pub proved: BTreeSet<usize>,
    /// The rules left unproved, lemmas left out, by index into `ts.invariants()`.
    pub unproved: BTreeSet<usize>,
    /// The lines to print.
    pub text: String,
}

/// One rule's line, and the CTI under an unproved one, onto `text`. A lemma the model marks reads
/// `lemma <name>`; a rule the SQL schema does not enforce ends its name with why.
fn print_verdict(
    text: &mut String,
    ts: &TransitionSystem,
    r: &RuleProof,
    marks: &Marks,
    refuted: &BTreeMap<usize, usize>,
    within: &str,
) {
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
        Verdict::Proved => out!(text, "proved: {name}{within}"),
        Verdict::OutOfScope(why) => out!(text, "out of scope: {name}: {why}"),
        Verdict::Refuted => {
            let n = refuted[&r.inv];
            let s = if n == 1 { "" } else { "s" };
            out!(
                text,
                "refuted: {name} (BFS counterexample at {n} instance{s})"
            )
        }
        Verdict::Unproved(Unproved::Initial) => {
            out!(text, "unproved: {name}: an initial state violates it")
        }
        Verdict::Unproved(Unproved::Premise(why)) => {
            out!(text, "unproved: {name}: {why}")
        }
        Verdict::Unproved(Unproved::Unknown { action }) => {
            out!(text, "unproved: {name}: z3 gave up on {action}")
        }
        Verdict::Unproved(Unproved::NotInductive(cti)) => {
            out!(
            text,
                "unproved: {name}: not inductive; {} breaks it from a state where it and every proved rule hold (not necessarily reachable)",
                cti.step
            );
            for row in &cti.rows {
                out!(text, "  {row}");
            }
        }
    }
}

/// Where the proof keeps its certificate between runs (`prove::Certificate`, #421): one file
/// per model and rule selection, under the user's cache directory, never in the project. A
/// missing, damaged, or stale file only costs the round that re-checks it; the proof's lines
/// are the same with it or without. `RDRA_ISH_NO_PROOF_CACHE` set turns it off.
mod certificate {
    use std::path::{Path, PathBuf};

    use rdra_ish_core::fingerprint;
    use rdra_ish_core::lifecycle::prove::Certificate;
    use rdra_ish_core::lifecycle::TransitionSystem;

    const HEADER: &str = "rdra-ish proof certificate 1";

    /// `$XDG_CACHE_HOME/rdra-ish/proofs`, else the platform's cache directory, with a file
    /// named by the hash of the model's rule names and the rules asked. Two models with the
    /// same names share it; the digest inside tells them apart.
    pub(super) fn file(ts: &TransitionSystem, rules: &[usize]) -> Option<PathBuf> {
        if std::env::var_os("RDRA_ISH_NO_PROOF_CACHE").is_some() {
            return None;
        }
        let xdg = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let root = match xdg {
            Some(dir) => dir,
            None if cfg!(target_os = "macos") => home()?.join("Library/Caches"),
            None => home()?.join(".cache"),
        };
        let mut key = String::new();
        for inv in ts.invariants() {
            key.push_str(&inv.name);
            key.push('\n');
        }
        for i in rules {
            key.push_str(&format!("{i} "));
        }
        let name = format!("{}.txt", fingerprint::hash(key.as_bytes()));
        Some(root.join("rdra-ish").join("proofs").join(name))
    }

    /// The certificate in `file`, when it is whole and from this z3.
    pub(super) fn read(file: &Path, z3: &str) -> Option<Certificate> {
        let text = std::fs::read_to_string(file).ok()?;
        let (body, check) = text.rsplit_once("check ")?;
        if fingerprint::hash(body.as_bytes()) != check.trim_end() {
            return None;
        }
        let mut lines = body.lines();
        (lines.next()? == HEADER).then_some(())?;
        (lines.next()?.strip_prefix("z3 ")? == z3).then_some(())?;
        let digest = lines.next()?.strip_prefix("digest ")?.to_string();
        let kept = lines.next()?.strip_prefix("kept")?;
        let kept = kept
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<usize>, _>>()
            .ok()?;
        lines
            .next()
            .is_none()
            .then_some(Certificate { digest, kept })
    }

    /// Writes `cert` to `file` whole or not at all: a run beside this one reads the old file
    /// or the new. A failure is ignored; the next run only proves without it.
    pub(super) fn write(file: &Path, z3: &str, cert: &Certificate) {
        let kept: String = cert.kept.iter().map(|k| format!(" {k}")).collect();
        let body = format!("{HEADER}\nz3 {z3}\ndigest {}\nkept{kept}\n", cert.digest);
        let text = format!("{body}check {}\n", fingerprint::hash(body.as_bytes()));
        let Some(dir) = file.parent() else { return };
        let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
        let written = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&tmp, text))
            .and_then(|()| std::fs::rename(&tmp, file));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}
