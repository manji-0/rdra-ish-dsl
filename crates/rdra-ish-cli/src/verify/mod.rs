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
//!
//! This file holds the entry point `run` and its options. The rest is one file per seam:
//! `jobs` builds the cones and property runs, `bfs` explores the safety cones and the
//! `explored` properties, `native` decides the lasso properties, `tlc` and `tlc_tool` run
//! TLC, `specs` writes the modules for `-o`, `compare` finds where the engines differ, and
//! `report` and `end` print the plan, the property lines, and the closing tally.

mod bfs;
mod compare;
mod end;
mod jobs;
mod native;
mod report;
mod specs;
mod tlc;
mod tlc_tool;

use self::bfs::{bfs_cones, explored_properties, report_bfs_cones, state_models, Explored, Safety};
use self::compare::disagreements;
use self::end::{End, TlcEnd};
use self::jobs::{cone_jobs, property_jobs, Job, JobKind};
use self::native::{native_properties, native_wholes, Natives};
use self::report::{print_plan, report_properties, Temporal};
use self::specs::{out_names, spec_dir, write_specs_and_index, write_whole_specs};
use self::tlc::{run_cones, run_tlc_wholes, tlc_failures, Ran};
pub(crate) use self::tlc_tool::find_on_path;
use self::tlc_tool::Tlc;
use crate::export::{checked_system, tla_bounds, Modules};
use crate::memory::{Budgeted, VerifySplit};
use crate::prove::{find_z3, run_proof, Marks};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_output::Outcome;
use anyhow::Result;
use rdra_ish_core::lifecycle::{Caps, Scope, Target};
use rdra_ish_core::memory::MemoryPool;
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
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

/// Caps for a whole-model run that starts now: `WHOLE_MODEL_STATE_CAP` states, and the time
/// budget from now.
fn whole_caps(caps: Caps, budget: Duration) -> Caps {
    let states = usize::try_from(WHOLE_MODEL_STATE_CAP).expect("the cap fits in usize");
    Caps { states, ..caps }.until(Instant::now() + budget)
}
