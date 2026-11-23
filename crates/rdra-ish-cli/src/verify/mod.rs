//! `rdra-ish verify`: check every invariant, after-assert, and temporal property on its own
//! cone, and a temporal property on the whole system only when its cone does not decide it.
//! The native checker decides each one on the graph BFS explores: a safety cone by the
//! explorer (`bfs_cones`), `stable`, `unchanged`, and `precedes` on their cones
//! (`TransitionSystem::decide_property`), and `always`, `eventually`, and `leads_to` by the
//! runs `property_cone` defines (`native_property`), the whole-model run included
//! (`native_wholes`), under `--temporal-budget` and `WHOLE_MODEL_STATE_CAP`.
//!
//! A cone (`TransitionSystem::target_cone`) is closed: its restricted system reaches exactly
//! the projection of the whole system's states, so a safety verdict on the cone is the
//! verdict on the whole model. A property's cone (`property_cone`) also keeps the actions its
//! `.fair` names whole, so their weak fairness restricts to it; the whole model's
//! `WF_vars(Next)` does not, which is what `property_cone::Verdict` decides.
//!
//! `--with-tlc` also prints a TLA+ module per run and has TLC run the same cones and the
//! same ladder of property runs beside the native checker, each property alone, because TLC
//! does not say which property its lasso violates. A check the two engines decide
//! differently is a tool bug (`disagreements`): the run fails and keeps its specs. A check
//! one engine leaves undecided is no disagreement.

mod bfs;
mod jobs;
mod specs;
mod tlc;
mod tlc_tool;

use self::bfs::{
    bfs_cones, crowded, explored_properties, report_bfs_cones, state_models, Bfs, BfsCone,
    Explored, Safety,
};
use self::jobs::{cone_jobs, property_jobs, Job, JobKind, PropertyJob};
use self::specs::{out_names, spec_dir, write_specs_and_index, write_whole_specs};
use self::tlc::{run_cones, run_tlc_wholes, tlc_failures, undecided_hint, Decision, Ran, Stop};
pub(crate) use self::tlc_tool::find_on_path;
use self::tlc_tool::Tlc;
use crate::export::{checked_system, tla_bounds, Modules};
use crate::memory::{Budgeted, VerifySplit};
use crate::property_cone::{AfterPlain, ConeRun, Verdict};
use crate::prove::{find_z3, run_proof, Marks, ProofTally};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_output::Outcome;
use crate::tlc_replay::explain_violation;
use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::{
    Bounds, BoundsLine, Caps, ComponentRun, Diag, Fairness, LassoFault, PropertyVerdict, Scope,
    Selection, StateDiag, Target, TransitionSystem,
};
use rdra_ish_core::memory::MemoryPool;
use rdra_ish_core::{in_parallel, ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Distinct states a whole-model temporal run may reach before it is stopped, natively and
/// under TLC. Separate from `Caps::VERIFY`, the BFS cap per cone: the run's memory and time
/// grow with this one, and `--temporal-budget` bounds only the time.
const WHOLE_MODEL_STATE_CAP: u64 = 1 << 21;

/// `--with-tlc`: TLC runs beside the native checker.
pub struct WithTlc {
    /// `--tlc-jar`: run TLC from `tla2tools.jar`; else `RDRA_TLC_JAR`, else `tlc` on PATH.
    pub jar: Option<PathBuf>,
}

/// What `verify` spends on a check it may not finish, and whether one it leaves undecided
/// fails the run.
pub struct Budget {
    /// Seconds a temporal property's whole-model run gets.
    pub temporal: Duration,
    /// `--deny-undecided`: an undecided finding is an error, not a warning.
    pub deny_undecided: bool,
    /// `--prove`: z3 tries each state rule for any number of rows.
    pub prove: Option<Prove>,
    /// `--memory`, split between BFS and the TLC runs under `--with-tlc`.
    pub memory: VerifySplit,
}

/// `verify --prove`.
pub struct Prove {
    /// `--z3`: the binary; else `RDRA_Z3`, else `z3` on PATH.
    pub z3: Option<PathBuf>,
    /// `--deny-unproved`: a rule left unproved fails the run.
    pub deny_unproved: bool,
}

/// A run line: `<module> [<checks>]: <verdict> (<states> states, <notes>)`, the same for
/// either engine.
fn run_line(
    module: &str,
    checks: &[String],
    verdict: &str,
    states: Option<u64>,
    notes: &[String],
) -> String {
    let inside: Vec<String> = states
        .map(|n| format!("{n} states"))
        .into_iter()
        .chain(notes.iter().cloned())
        .collect();
    let inside = match inside.is_empty() {
        true => String::new(),
        false => format!(" ({})", inside.join(", ")),
    };
    format!("{module} [{}]: {verdict}{inside}", checks.join(", "))
}

/// Runs `verify`. `bounds` is `(--instance-count, --int-range-max)`; `with_tlc` is
/// `--with-tlc` and its `--tlc-jar`.
pub fn run(
    program: &ResolvedProgram,
    model: &SemanticModel,
    out: Option<PathBuf>,
    (instance_count, int_range_max): (u32, u32),
    scope: &Scope,
    Budget {
        temporal: budget,
        deny_undecided,
        prove,
        memory,
    }: Budget,
    with_tlc: Option<WithTlc>,
) -> Result<()> {
    let bounds = tla_bounds(instance_count, int_range_max)?;
    let built = checked_system(model, bounds, scope)?;
    let (ts, selection) = (built.ts, built.selection);
    let (out, whole_name) = out_names(out);
    let keep = out.is_some();
    let names = TlaNames::new(&ts);

    // A scope checks no property: `--buc` drops use cases fairness would read, and an error
    // `--entity` defers may sit in a property's cone.
    let temporal = !ts.properties().is_empty() && scope.is_whole();
    // The whole-model module keeps the name `-o` gives it; the generated names yield.
    let mut modules = Modules::default();
    let whole_stem = whole_name.clone();
    let whole_name = match temporal {
        true => modules.claim(&whole_name),
        false => whole_name,
    };
    // A `.lemma()` rule only helps `--prove`, which keeps a lemma only when it proves it, so
    // no cone runs for one: a lemma that joins two large cones would cost more than the rules
    // it helps. `check` still checks it.
    let lemmas = rdra_ish_core::lifecycle::prove::lemma_invariants(model, &ts);
    let targets: Vec<Target> = selection
        .targets
        .iter()
        .copied()
        .filter(|t| !matches!(t, Target::Invariant(i) if lemmas.contains(i)))
        .collect();
    // Every BFS of the run, the ones that reduce a cone included, is charged to one pool:
    // all of `--memory`, or BFS's half of it under `--with-tlc`.
    let raise = if memory.bfs == memory.limit {
        Budgeted::Verify.raise().to_string()
    } else {
        format!("{}; BFS has {} of it", Budgeted::Verify.raise(), memory.bfs)
    };
    let caps = Caps::VERIFY.charged_to(MemoryPool::share_of(
        memory.limit,
        memory.bfs,
        Box::leak(raise.into_boxed_str()),
    ));
    let mut jobs = cone_jobs(&ts, &names, &targets, &mut modules, caps);
    let cones = jobs.len();
    let line = print_plan(
        model,
        &ts,
        scope,
        &selection,
        bounds,
        (cones, temporal, with_tlc.is_some()),
        budget,
    );
    let mut whole_modules = Vec::new();
    if temporal {
        jobs.extend(property_jobs(&ts, &names, &mut modules, caps));
        whole_modules = (0..ts.properties().len())
            .map(|i| modules.claim(&format!("{whole_stem}_{}", names.property(i))))
            .collect();
    }
    // The modules are written for `-o`, a run by hand, and for TLC under `--with-tlc`.
    let dir = match keep || with_tlc.is_some() {
        true => Some(spec_dir(out)?),
        false => None,
    };
    // The whole-model module with every property, for a run by hand; a property runs on it
    // only when its cone leaves it undecided.
    let whole =
        (temporal && dir.is_some()).then(|| tla::print(&ts, PrintMode::Temporal, &whole_name));
    // TLC starts on the cones while BFS and the proof run here: none reads what another
    // finds. A failure to write the specs or to find a tool is reported after what BFS and
    // the proof print, and starts no TLC run.
    let written = dir
        .as_ref()
        .map(|dir| write_specs_and_index(dir, &jobs, &whole));
    let tlc = with_tlc.map(|w| Tlc::find(w.jar));
    let z3 = prove.as_ref().map(|p| find_z3(p.z3.clone()));
    let start = match (&dir, &written, &tlc, &z3) {
        (Some(dir), Some(Ok(_)), Some(Ok(tlc)), None | Some(Ok(_))) => Some((tlc, dir)),
        _ => None,
    };
    let (bfs, natives, explored, proof, cones_ran) = std::thread::scope(|threads| {
        let cones = start
            .map(|(tlc, dir)| threads.spawn(|| run_cones(tlc, dir, &modules, &jobs, memory.cones)));
        let bfs = state_models(program, model, &ts, scope, &selection, caps);
        let (safety, mut vacuous) = bfs_cones(&ts, &jobs, caps);
        let (properties, property_vacuous) = native_properties(&ts, &jobs, caps);
        // A `--buc` drops use cases that may reach a premise.
        if scope.bucs.is_empty() {
            vacuous.extend(property_vacuous);
            for d in vacuous {
                if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                    eprintln!("{}", Finding::new(&d.diag, &[], text));
                }
            }
        }
        let explored = match temporal {
            true => explored_properties(program, model, &ts, caps, budget),
            false => Explored::default(),
        };
        let open: Vec<usize> = jobs
            .iter()
            .zip(&properties)
            .filter_map(|(job, native)| match (&job.kind, native) {
                (JobKind::PropertyCone(p), Some(n)) if n.goes_whole() => Some(p.index),
                _ => None,
            })
            .collect();
        let wholes = native_wholes(&ts, &open, caps, budget);
        let proof = match (&prove, z3) {
            (Some(Prove { deny_unproved, .. }), Some(z3)) => z3.and_then(|z3| {
                let marks = Marks::of(model, &ts);
                run_proof(&ts, &selection, &marks, &z3, *deny_unproved).map(Some)
            }),
            _ => Ok(None),
        };
        let cones_ran = cones.map(|cones| match cones.join() {
            Ok(ran) => ran,
            Err(panic) => std::panic::resume_unwind(panic),
        });
        let natives = Natives {
            safety,
            properties,
            wholes: open.into_iter().zip(wholes).collect(),
        };
        (bfs, natives, explored, proof, cones_ran)
    });
    let mut end = End {
        bfs,
        explored,
        safety: Safety::default(),
        temporal: Temporal::default(),
        tlc: TlcEnd::default(),
        deny_undecided,
        proof: proof?,
    };

    let mut index = match written {
        Some(written) => Some(written?),
        None => None,
    };
    if let (true, Some((_, index_path))) = (keep, &index) {
        println!(
            "wrote {} specs and {}",
            jobs.len() + usize::from(whole.is_some()),
            index_path.display()
        );
    }
    if jobs.is_empty() && end.explored.checked == 0 && ts.reach_properties().is_empty() {
        println!("nothing to check");
    }
    end.safety = report_bfs_cones(program, model, &jobs, &natives.safety);
    end.temporal = report_properties(program, model, &ts, &jobs, &natives, &whole_modules);
    let undecided = natives.undecided_lines(&names);
    // Under `-o` the whole-model module of each property its cone leaves undecided is
    // written, for a run by hand, as TLC's whole-model run writes it.
    if let (true, Some(dir), Some((index, index_path))) = (keep, &dir, &mut index) {
        let open: Vec<usize> = natives.wholes.iter().map(|(i, _)| *i).collect();
        write_whole_specs(
            dir,
            (index, index_path),
            (&ts, &names),
            &whole_modules,
            &open,
        )?;
    }

    let mut disagreed = Vec::new();
    let mut tlc_outcome = None;
    if let Some(tlc) = tlc {
        let tlc = tlc?;
        let dir = dir.as_deref().expect("--with-tlc writes the specs");
        let (index, index_path) = index.as_mut().expect("--with-tlc writes the index");
        let ran = cones_ran.expect("TLC started once the specs and the tools were there")?;
        for (job, r) in jobs.iter().zip(&ran) {
            println!("tlc: {}", r.line(&job.module, &job.checks));
        }
        let (wholes, whole_ran) = run_tlc_wholes(
            (&tlc, dir),
            (&ts, &names),
            &whole_modules,
            (budget, memory.wholes),
            (&jobs, &ran),
            (index, index_path.as_path()),
        )?;
        disagreed = disagreements(&whole_modules, &jobs, &natives, (&ran, &whole_ran));
        let all_jobs: Vec<&Job> = jobs.iter().chain(&wholes).collect();
        let all_ran: Vec<&Ran> = ran.iter().chain(&whole_ran).collect();
        let outcome = worst(all_ran.iter().map(|r| r.decision.outcome()));
        end.tlc.tool_errors = all_ran
            .iter()
            .filter(|r| r.decision.outcome() == Outcome::ToolError)
            .count();
        if outcome != Outcome::Ok {
            tlc_failures(program, model, (&ts, &names), dir, (&all_jobs, &all_ran));
        }
        tlc_outcome = Some(outcome);
    }
    for d in &disagreed {
        eprintln!("{d}");
    }
    end.tlc.disagreements = disagreed.len();

    // A disagreement reports neither engine's verdict as the result.
    let result = match disagreed.is_empty() {
        true => end.outcome(),
        false => Outcome::ToolError,
    };
    let say = |label: &str, outcome: Outcome| match outcome {
        Outcome::Ok => println!("{label}: ok"),
        other => eprintln!("{label}: {}", other.as_str()),
    };
    say("result", result);
    if let Some(outcome) = tlc_outcome {
        say(
            "tlc_result",
            match disagreed.is_empty() {
                true => outcome,
                false => Outcome::ToolError,
            },
        );
    }
    if let Some(dir) = &dir {
        let failed = tlc_outcome.is_some_and(|o| o != Outcome::Ok);
        let kept = match (!disagreed.is_empty(), failed) {
            (true, _) => Some("the engines disagree"),
            (false, true) => Some("a check failed"),
            (false, false) => None,
        };
        match (keep, kept) {
            (true, Some(_)) => eprintln!("specs: {}", dir.display()),
            (true, None) => {}
            (false, Some(why)) => eprintln!("specs: {} (kept because {why})", dir.display()),
            (false, None) => {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }
    end.finish(&line, &undecided)
}

/// The worst of `outcomes`: a counterexample over a tool error over `ok`.
fn worst(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
    outcomes.into_iter().fold(Outcome::Ok, |a, b| match (a, b) {
        (Outcome::Counterexample, _) | (_, Outcome::Counterexample) => Outcome::Counterexample,
        (Outcome::ToolError, _) | (_, Outcome::ToolError) => Outcome::ToolError,
        _ => Outcome::Ok,
    })
}

/// The header of a run: the bounds line, the cone and property counts, and the fairness
/// assumptions. Returns the bounds line for the closing summary.
fn print_plan(
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    bounds: Bounds,
    (cones, temporal, with_tlc): (usize, bool, bool),
    budget: Duration,
) -> BoundsLine {
    let line =
        BoundsLine::verify(bounds, Some(Caps::VERIFY)).with_scope(scope.line(model, selection));
    println!("{line}");
    let checked = |pick: fn(&Target) -> bool| selection.targets.iter().filter(|t| pick(t)).count();
    println!(
        "cones: {cones} for {} invariant(s) and {} after-assert(s)",
        checked(|t| matches!(t, Target::Invariant(_))),
        checked(|t| matches!(t, Target::AfterAssert(_)))
    );
    let explored = ts
        .properties()
        .iter()
        .filter(|p| p.formula.kind().is_explored())
        .count();
    match (temporal, ts.properties().len() - explored, explored) {
        (_, 0, 0) => println!("temporal properties: none"),
        (true, n, m) => {
            if n > 0 {
                println!(
                    "temporal properties: {n}, each checked on its cone, and on the whole model only when the cone does not decide it (budget {} s)",
                    budget.as_secs()
                );
            }
            if m > 0 {
                println!("order and stability properties: {m}, each decided by BFS on its cone");
            }
        }
        (false, n, m) => println!(
            "temporal properties: {}, not checked: a scoped run checks none",
            n + m
        ),
    }
    match (scope.is_whole(), ts.reach_properties().len()) {
        (_, 0) => {}
        (true, n) => println!(
            "can_reach properties: {n}, decided natively by BFS on each one's cone (TLC cannot check AG EF)"
        ),
        (false, n) => println!("can_reach properties: {n}, not checked: a scoped run checks none"),
    }
    if with_tlc {
        println!("tlc: runs the same cones beside the native checker (--with-tlc); a check the two decide differently fails the run");
    }
    // A verdict under `.fair(...)` holds only on the runs that do not starve those use cases.
    for p in ts.properties().iter().filter(|p| !p.fair.is_empty()) {
        let ucs: Vec<&str> = p
            .fair
            .iter()
            .map(|uk| model.use_cases[*uk].id.as_str())
            .collect();
        println!(
            "  {} assumes weak fairness per instance on {}",
            p.name,
            ucs.join(", ")
        );
    }
    line
}

/// Caps for a whole-model run that starts now: `WHOLE_MODEL_STATE_CAP` states, and the time
/// budget from now.
fn whole_caps(caps: Caps, budget: Duration) -> Caps {
    let states = usize::try_from(WHOLE_MODEL_STATE_CAP).expect("the cap fits in usize");
    Caps { states, ..caps }.until(Instant::now() + budget)
}

/// What the native checker decides of one `always`, `eventually`, or `leads_to`, on its cone
/// or on the whole model.
struct Native {
    decision: Decision,
    /// The states of the run the line reports: the plain cone's, or the whole model's.
    states: Option<u64>,
    notes: Vec<String>,
    /// Each cone run and what it said, for `--with-tlc` to compare.
    runs: Vec<(ConeRun, Outcome)>,
    /// The counterexample of a `counterexample` verdict.
    found: Option<Box<StateDiag>>,
    /// A counterexample that failed its own check, or a reduced cone that disagrees with its
    /// cone: a tool bug, never a finding about the model.
    fault: Option<String>,
    /// A cone run stopped at the state cap or the memory limit: a finding the cone leaves
    /// undecided, which no whole-model run takes up.
    cone_capped: bool,
    /// A run met the memory limit, maybe only because the runs beside it held the rest.
    crowded: bool,
}

impl Native {
    fn line(&self, module: &str, checks: &[String]) -> String {
        run_line(
            module,
            checks,
            self.decision.as_str(),
            self.states,
            &self.notes,
        )
    }

    /// The cone leaves the property to the whole model.
    fn goes_whole(&self) -> bool {
        matches!(self.decision, Decision::Undecided { .. }) && !self.cone_capped
    }
}

/// One run's answer on a graph.
enum Answer {
    Verdict(Outcome, Option<Box<StateDiag>>),
    /// The run stopped at a cap or the budget before it found a counterexample.
    Stopped,
    Fault(LassoFault),
}

impl Answer {
    /// `properties()[i]` of `run`'s system on its graph: an `always` by its states, an
    /// `eventually` or `leads_to` by its fair components under `fairness`.
    fn of(run: &ComponentRun, i: usize, fairness: Fairness) -> Answer {
        let verdict = match run.system.properties()[i].formula.kind().is_liveness() {
            true => run.decide_liveness(i, fairness),
            false => Ok(run.decide_property(i).expect("an always")),
        };
        match verdict {
            Ok(PropertyVerdict::Holds) => Answer::Verdict(Outcome::Ok, None),
            Ok(PropertyVerdict::Violated(d)) => Answer::Verdict(Outcome::Counterexample, Some(d)),
            Ok(PropertyVerdict::Undecided) => Answer::Stopped,
            Err(fault) => Answer::Fault(fault),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Answer::Verdict(outcome, _) => outcome.as_str(),
            Answer::Stopped => "not decided",
            Answer::Fault(_) => "tool_error",
        }
    }

    fn outcome(&self) -> Outcome {
        match self {
            Answer::Verdict(outcome, _) => *outcome,
            Answer::Stopped => Outcome::Ok,
            Answer::Fault(_) => Outcome::ToolError,
        }
    }
}

/// Explores `system` under `caps`, and marks `crowded` when the run met the memory limit.
fn explore(system: &TransitionSystem, caps: Caps, crowded_run: &mut bool) -> ComponentRun {
    let run = system.clone().run(caps);
    *crowded_run |= crowded(&run);
    run
}

/// `p` on its cone by the runs `property_cone` defines, each on a graph BFS explores: `Plain`
/// on the reduced cone, `Full` on the unreduced cone after a counterexample there, and
/// `WfNext` on the same unreduced graph when `after_plain` asks for it. Also returns the
/// vacuity the plain run shows.
fn native_property(ts: &TransitionSystem, p: &PropertyJob, caps: Caps) -> (Native, Option<Diag>) {
    let cone = &p.cone;
    let mut out = Native {
        decision: Decision::Decided(Outcome::Ok),
        states: None,
        notes: Vec::new(),
        runs: Vec::new(),
        found: None,
        fault: None,
        cone_capped: false,
        crowded: false,
    };
    let mut crowded_run = false;
    let plain_run = explore(&cone.reduced, caps, &mut crowded_run);
    let vacuous = ts.property_vacuity(p.index, &plain_run);
    out.states = Some(plain_run.states.len() as u64);
    let mut last = Answer::of(&plain_run, cone.reduced_index, Fairness::Plain);
    drop(plain_run);
    out.runs.push((ConeRun::Plain, last.outcome()));
    let mut unreduced: Option<ComponentRun> = None;
    // A counterexample on the reduced cone is real, but its lasso is read on the unreduced
    // cone, whose run gives the verdict from here on.
    if let (Answer::Verdict(Outcome::Counterexample, _), Some(full)) = (&last, &p.full) {
        let run = explore(&cone.system, caps, &mut crowded_run);
        let again = Answer::of(&run, cone.index, Fairness::Plain);
        unreduced = Some(run);
        out.runs.push((ConeRun::Full, again.outcome()));
        out.notes
            .push(format!("{}: {}", full.module, again.as_str()));
        last = match again {
            Answer::Verdict(Outcome::Ok, _) => {
                out.fault =
                    Some("the reduced cone finds a counterexample the cone does not".to_string());
                Answer::Verdict(Outcome::ToolError, None)
            }
            other => other,
        };
    }
    out.crowded = crowded_run;
    let (plain, found) = match last {
        Answer::Verdict(outcome, found) => (outcome, found),
        Answer::Stopped => return (out.capped(), vacuous),
        Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
    };
    out.found = found;
    let verdict = match cone.after_plain(plain) {
        AfterPlain::Decided(verdict) => verdict,
        AfterPlain::NeedsWfNext => {
            let wf =
                p.wf.as_ref()
                    .expect("an eventually or leads_to has a WF module");
            let run = match unreduced {
                Some(run) => run,
                None => explore(&cone.system, caps, &mut crowded_run),
            };
            out.crowded = crowded_run;
            let second = Answer::of(&run, cone.index, Fairness::WfNext);
            out.runs.push((ConeRun::WfNext, second.outcome()));
            out.notes
                .push(format!("{}: {}", wf.module, second.as_str()));
            match second {
                Answer::Verdict(outcome, found) => {
                    out.found = found.or(out.found.take());
                    cone.after_wf(plain, outcome)
                }
                Answer::Stopped => return (out.capped(), vacuous),
                Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
            }
        }
    };
    out.decision = match verdict {
        Verdict::Ok => Decision::Decided(Outcome::Ok),
        Verdict::Counterexample => Decision::Decided(Outcome::Counterexample),
        Verdict::ToolError => Decision::Decided(Outcome::ToolError),
        Verdict::Undecided(why) => {
            out.notes.push(why.as_str().to_string());
            Decision::Undecided {
                why: why.as_str().to_string(),
                stop: None,
            }
        }
    };
    if out.decision.verdict() != Some(Outcome::Counterexample) {
        out.found = None;
    }
    (out, vacuous)
}

impl Native {
    /// A cone run stopped at the state cap or the memory limit before it decided.
    fn capped(mut self) -> Native {
        let why = "stopped at the state cap or the memory limit".to_string();
        self.notes.push(why.clone());
        self.decision = Decision::Undecided { why, stop: None };
        self.cone_capped = true;
        self.found = None;
        self
    }

    /// A counterexample that failed its own check.
    fn faulty(mut self, fault: &LassoFault) -> Native {
        self.fault = Some(format!(
            "the counterexample fails its own check ({fault:?})"
        ));
        self.decision = Decision::Decided(Outcome::ToolError);
        self.found = None;
        self
    }
}

/// `native_property` for every property job, side by side, `None` for a safety cone. A
/// property whose runs met the memory limit beside others is decided again alone. Also
/// returns the vacuous properties.
fn native_properties(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
) -> (Vec<Option<Native>>, Vec<Diag>) {
    let at: Vec<usize> = (0..jobs.len())
        .filter(|&k| matches!(jobs[k].kind, JobKind::PropertyCone(_)))
        .collect();
    let decide = |k: usize| match &jobs[k].kind {
        JobKind::PropertyCone(p) => native_property(ts, p, caps),
        JobKind::Cone(_) | JobKind::Whole(_) => unreachable!("a property's job"),
    };
    let ran = in_parallel(at.clone(), rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<Native>> = jobs.iter().map(|_| None).collect();
    let mut vacuous = Vec::new();
    for (k, (native, vacuity)) in at.into_iter().zip(ran) {
        let (native, vacuity) = match native.crowded {
            true => decide(k),
            false => (native, vacuity),
        };
        vacuous.extend(vacuity);
        out[k] = Some(native);
    }
    (out, vacuous)
}

/// The properties `open` names, which their cones leave undecided, on the whole model: one
/// exploration under the budget and `WHOLE_MODEL_STATE_CAP`, and each property decided on
/// its graph with `WF_vars(Next)`. A run that stopped decides a property only by a
/// counterexample, which is real on the states it reached, as TLC's verdict printed before a
/// kill is.
fn native_wholes(
    ts: &TransitionSystem,
    open: &[usize],
    caps: Caps,
    budget: Duration,
) -> Vec<Native> {
    if open.is_empty() {
        return Vec::new();
    }
    let run = ts.clone().run(whole_caps(caps, budget));
    let states = run.states.len() as u64;
    let stop = run.diags.iter().find_map(|d| match &d.diag {
        StateDiag::StateCapReached { cap, .. } => Some((
            Stop::StateCap,
            format!("stopped at the state cap of {cap} states"),
        )),
        StateDiag::MemoryCapReached { limit, .. } => Some((
            Stop::Memory,
            format!("stopped at the memory limit of {limit}"),
        )),
        StateDiag::TimeBudgetReached { .. } => Some((
            Stop::Budget,
            format!("stopped at the time budget of {} s", budget.as_secs()),
        )),
        _ => None,
    });
    open.iter()
        .map(|&i| {
            let mut native = Native {
                decision: Decision::Decided(Outcome::Ok),
                states: (states > 0).then_some(states),
                notes: Vec::new(),
                runs: Vec::new(),
                found: None,
                fault: None,
                cone_capped: false,
                crowded: false,
            };
            match Answer::of(&run, i, Fairness::WfNext) {
                Answer::Verdict(outcome, found) => {
                    native.decision = Decision::Decided(outcome);
                    native.found = found;
                }
                Answer::Stopped => {
                    let (stop, why) = stop.clone().expect("a run that stopped says why");
                    native.notes.push(why.clone());
                    native.decision = Decision::Undecided {
                        why,
                        stop: Some(stop),
                    };
                }
                Answer::Fault(fault) => native = native.faulty(&fault),
            }
            native
        })
        .collect()
}

/// What the native checker decided: per job, a safety cone's verdict or a property's on its
/// cone, and per property the cone left open, its whole-model verdict.
struct Natives {
    safety: Vec<Option<BfsCone>>,
    properties: Vec<Option<Native>>,
    wholes: Vec<(usize, Native)>,
}

impl Natives {
    /// The whole-model verdict of property `i`, when its cone left it open.
    fn whole(&self, i: usize) -> Option<&Native> {
        self.wholes.iter().find(|(at, _)| *at == i).map(|(_, n)| n)
    }

    /// The properties the whole-model run leaves undecided, one line each.
    fn undecided_lines(&self, names: &TlaNames) -> Vec<String> {
        self.wholes
            .iter()
            .filter_map(|(i, native)| {
                let Decision::Undecided {
                    why,
                    stop: Some(stop),
                } = &native.decision
                else {
                    return None;
                };
                let states = native
                    .states
                    .map_or(String::new(), |n| format!(" after {n} states"));
                Some(format!(
                    "  {}: the whole-model run {why}{states}; {}",
                    names.property(*i),
                    undecided_hint(*stop)
                ))
            })
            .collect()
    }
}

/// What the native checker decided of the `always`, `eventually`, and `leads_to` properties.
#[derive(Default)]
struct Temporal {
    violated: usize,
    /// Counterexamples that failed their own check: a tool bug.
    faults: usize,
    /// Properties a cone run stopped at the state cap or the memory limit left undecided.
    capped: usize,
}

/// Prints each property's cone line and then its whole-model line, renders each
/// counterexample, and reports each fault.
fn report_properties(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    jobs: &[Job],
    natives: &Natives,
    whole_modules: &[String],
) -> Temporal {
    let mut out = Temporal::default();
    let mut report = |module: &str, checks: &[String], native: &Native| {
        println!("{}", native.line(module, checks));
        if let Some(diag) = &native.found {
            let text = render_violation(program, model, diag, TraceSource::Bfs)
                .expect("a property violation renders");
            eprintln!("{}", Finding::new(diag, &[], text));
            out.violated += 1;
        }
        if let Some(fault) = &native.fault {
            eprintln!("error[LassoFault]: {module} [{}]: {fault}; a tool bug, not a finding about the model", checks.join(", "));
            out.faults += 1;
        }
    };
    let mut capped = 0;
    for (job, native) in jobs.iter().zip(&natives.properties) {
        if let Some(native) = native {
            report(&job.module, &job.checks, native);
            capped += usize::from(native.cone_capped);
        }
    }
    for (i, native) in &natives.wholes {
        let checks = [ts.properties()[*i].name.clone()];
        report(&whole_modules[*i], &checks, native);
    }
    out.capped = capped;
    out
}

/// Under `--with-tlc`, each check the two engines decide differently, as an
/// `error[EngineDisagreement]` line: a safety cone's verdict and the checks its counterexample
/// breaks, and a property's verdict on each run both made and in the end. A check one engine
/// leaves undecided, or a TLC run that failed as a tool, is no disagreement.
fn disagreements(
    whole_modules: &[String],
    jobs: &[Job],
    natives: &Natives,
    (ran, whole_ran): (&[Ran], &[Ran]),
) -> Vec<String> {
    let line = |module: &str, checks: &[String], native: &str, tlc: &str| {
        format!(
            "error[EngineDisagreement]: {module} [{}]: native {native}, TLC {tlc}; one engine is wrong (a tool bug, not a finding about the model)",
            checks.join(", ")
        )
    };
    let differ = |a: Option<Outcome>, b: Option<Outcome>| match (a, b) {
        (Some(a), Some(b)) => a != b && a != Outcome::ToolError && b != Outcome::ToolError,
        _ => false,
    };
    let mut out = Vec::new();
    let mut tlc_wholes = whole_ran.iter();
    for (k, (job, tlc)) in jobs.iter().zip(ran).enumerate() {
        match &job.kind {
            JobKind::Cone(cs) => {
                let Some(native) = &natives.safety[k] else {
                    continue;
                };
                let (a, b) = (native.verdict(), tlc.decision.verdict());
                if differ(a, b) {
                    out.push(line(&job.module, &job.checks, as_str(a), as_str(b)));
                    continue;
                }
                if (a, b) != (Some(Outcome::Counterexample), Some(Outcome::Counterexample)) {
                    continue;
                }
                let BfsCone::Ran { found, .. } = native else {
                    continue;
                };
                let mine: BTreeSet<String> = found.iter().map(|d| d.diag.name(&[])).collect();
                let theirs: Option<BTreeSet<String>> = tlc
                    .run
                    .violations
                    .iter()
                    .map(|v| explain_violation(&cs.system, &cs.names, v).map(|d| d.name(&[])))
                    .collect::<Result<_, _>>()
                    .ok();
                if let Some(theirs) = theirs.filter(|t| *t != mine) {
                    let list =
                        |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(" ");
                    out.push(line(
                        &job.module,
                        &job.checks,
                        &format!("violates {}", list(&mine)),
                        &format!("violates {}", list(&theirs)),
                    ));
                }
            }
            JobKind::PropertyCone(p) => {
                let Some(native) = &natives.properties[k] else {
                    continue;
                };
                let module = |run: ConeRun| match run {
                    ConeRun::Plain => job.module.clone(),
                    ConeRun::Full => p
                        .full
                        .as_ref()
                        .map_or(job.module.clone(), |s| s.module.clone()),
                    ConeRun::WfNext => {
                        p.wf.as_ref()
                            .map_or(job.module.clone(), |s| s.module.clone())
                    }
                };
                for (run, mine) in &native.runs {
                    let theirs = tlc.runs.iter().find(|(r, _)| r == run).map(|(_, o)| *o);
                    if differ(Some(*mine), theirs) {
                        out.push(line(
                            &module(*run),
                            &job.checks,
                            mine.as_str(),
                            as_str(theirs),
                        ));
                    }
                }
                // The final verdict: the whole-model run's where a cone left the property open.
                let tlc_whole = match tlc.decision {
                    Decision::Undecided { .. } => tlc_wholes.next(),
                    Decision::Decided(_) => None,
                };
                let mine = natives.whole(p.index).unwrap_or(native);
                let theirs = tlc_whole.unwrap_or(tlc);
                let (a, b) = (mine.decision.verdict(), theirs.decision.verdict());
                if differ(a, b) {
                    let module = match (natives.whole(p.index), tlc_whole) {
                        (None, None) => &job.module,
                        _ => &whole_modules[p.index],
                    };
                    out.push(line(module, &job.checks, as_str(a), as_str(b)));
                }
            }
            JobKind::Whole(_) => {}
        }
    }
    out
}

fn as_str(outcome: Option<Outcome>) -> &'static str {
    outcome.map_or("not decided", Outcome::as_str)
}

/// What TLC adds to the end of a `--with-tlc` run.
#[derive(Default)]
struct TlcEnd {
    /// TLC runs that failed without a model violation.
    tool_errors: usize,
    /// Checks the two engines decide differently.
    disagreements: usize,
}

/// How a run ends.
struct End {
    bfs: Bfs,
    explored: Explored,
    /// What BFS decided of the safety cones.
    safety: Safety,
    /// What the native checker decided of `always`, `eventually`, and `leads_to`.
    temporal: Temporal,
    tlc: TlcEnd,
    deny_undecided: bool,
    /// What `--prove` proved, when it ran.
    proof: Option<ProofTally>,
}

impl End {
    /// The verdict the `result:` line states: of the invariants, the after-asserts, and the
    /// properties, every form and `can_reach` included. The state-model findings of the
    /// entity cones fail the run beside it and do not move it.
    fn outcome(&self) -> Outcome {
        if self.temporal.faults > 0 {
            return Outcome::ToolError;
        }
        let violated = self.safety.violated
            + self.explored.violated
            + self.temporal.violated
            + self.bfs.reach_errors;
        match violated {
            0 => Outcome::Ok,
            _ => Outcome::Counterexample,
        }
    }

    /// A violation, an error of BFS, or a tool error fails the run. A temporal property the
    /// budget left undecided is listed, as the BFS cap lists the findings it leaves; what the
    /// bounds left undecided is a warning, and an error under `--deny-undecided`.
    fn finish(&self, line: &BoundsLine, undecided: &[String]) -> Result<()> {
        if !undecided.is_empty() {
            println!(
                "not checked: {} not decided",
                match undecided.len() {
                    1 => "1 temporal property".to_string(),
                    n => format!("{n} temporal properties"),
                }
            );
            for u in undecided {
                println!("{u}");
            }
        }
        let failed: Vec<String> = [
            (
                self.bfs.errors,
                "state-model error(s) from BFS on the entity cones",
            ),
            (
                self.bfs.reach_errors,
                "can_reach error(s) from BFS on their cones",
            ),
            (
                self.safety.violated,
                "invariant or after-assert violation(s)",
            ),
            (
                self.explored.violated + self.temporal.violated,
                "property violation(s)",
            ),
            (
                self.temporal.faults,
                "counterexample(s) that fail their own check (a tool bug)",
            ),
            (
                self.tlc.disagreements,
                "check(s) the native checker and TLC decide differently (a tool bug)",
            ),
            (
                self.tlc.tool_errors,
                "TLC run(s) that failed without a model violation",
            ),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !failed.is_empty() {
            bail!("verification failed: {}", failed.join(" and "));
        }
        let open = [
            (
                self.bfs.capped + self.explored.capped + self.safety.capped + self.temporal.capped,
                "finding a cone stopped at the state cap or the memory limit left",
                "findings a cone stopped at the state cap or the memory limit left",
            ),
            (
                undecided.len(),
                "temporal property the budget left",
                "temporal properties the budget left",
            ),
        ]
        .iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect::<Vec<_>>();
        if !open.is_empty() {
            let what = format!("{} undecided", open.join(" and "));
            if self.deny_undecided {
                bail!("verification incomplete: {what} (--deny-undecided)");
            }
            eprintln!("warning: {what}; the OK line holds only for what was decided (--deny-undecided fails the run)");
        }
        match self.proof {
            Some(p) if p.unproved > 0 && p.deny_unproved => {
                bail!(
                    "verification incomplete: {} of {} rules unproved for any number of rows and any Int value (--deny-unproved)",
                    p.unproved,
                    p.rules
                );
            }
            Some(p) => println!("{}; {}", line.ok(), p.clause()),
            None => println!("{}", line.ok()),
        }
        Ok(())
    }
}
