//! `rdra-ish verify`: check every invariant, after-assert, and temporal property on its own
//! cone, and a temporal property on the whole system only when its cone does not decide it.
//! The native checker decides each one on the graph BFS explores: a safety cone by the
//! explorer (`bfs_cones`), `stable`, `unchanged`, and `precedes` on their cones
//! (`TransitionSystem::decide_property`), and `always`, `eventually`, and `leads_to` by the
//! runs `property_cone` defines (`native_property`), the whole-model run included
//! (`native_wholes`), under `--temporal-budget` and `WHOLE_MODEL_STATE_CAP` (under TLC, or `--max-states`).
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
mod escalate;
mod jobs;
mod native;
mod plan;
mod refuted;
mod report;
mod schedule;
mod shared;
mod specs;
mod state_models;
mod tlc;
mod tlc_tool;

use self::bfs::{
    bfs_cones, decide_by_proof, explored_properties, recheck_cones, report_bfs_cones, state_models,
    BfsCone, Explored, Safety, STATE_MODEL_TLC_STATES,
};
use self::compare::disagreements;
use self::end::{End, TlcEnd};
use self::jobs::{cone_jobs, explored_jobs, property_jobs, Job, JobKind};
use self::native::{native_properties, native_wholes, Native, Natives};
use self::report::{print_plan, report_properties, Temporal};
use self::schedule::{Machine, Pool};
use self::shared::Shared;
use self::specs::{out_names, spec_dir, write_specs_and_index, write_whole_specs};
use self::state_models::{run_state_models, state_model_disagreements, state_model_jobs};
use self::tlc::{covering, run_cones, run_tlc_wholes, tlc_failures, Ran};
pub(crate) use self::tlc_tool::find_on_path;
use self::tlc_tool::Tlc;
use crate::export::{checked_system, Modules};
use crate::memory::{Budgeted, VerifySplit};
use crate::prove::{find_z3, run_proof, Marks, Z3Tool};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_output::Outcome;
use anyhow::Result;
use rdra_ish_core::lifecycle::prove::{unverified_z3_note, Z3};
use rdra_ish_core::lifecycle::{Caps, QuotientCache, Scope, Symmetry, Target};
use rdra_ish_core::memory::MemoryPool;
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::tla::{self, PrintMode, TlaNames};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Distinct states a whole-model temporal run under TLC may reach before it is stopped,
/// unless `--max-states` replaces it. TLC's heap is not `--memory`'s pool, so it needs a bound;
/// the native runs have none by default, and the memory limit and `--temporal-budget` stop them.
const WHOLE_MODEL_STATE_CAP: usize = 1 << 21;

/// What a whole-model run gets: `--temporal-budget` seconds, and a state cap.
#[derive(Clone, Copy)]
pub(super) struct WholeRun {
    pub(super) budget: Duration,
    /// `--max-states`; `None`: no cap natively, `WHOLE_MODEL_STATE_CAP` under TLC.
    pub(super) states: Option<usize>,
}

impl WholeRun {
    /// `caps` for a run that starts now: this cap on states, and the time budget from now.
    pub(super) fn caps(self, caps: Caps) -> Caps {
        caps.with_states(self.states.unwrap_or(usize::MAX))
            .until(Instant::now() + self.budget)
    }
}

/// `--with-tlc`: TLC runs beside the native checker.
pub struct WithTlc {
    /// `--tlc-jar`: run TLC from `tla2tools.jar`; else `RDRA_TLC_JAR`, else `tlc` on PATH.
    pub jar: Option<PathBuf>,
    /// `--tlc-jobs`, else `RDRA_TLC_JOBS`: the most TLC runs at once.
    pub jobs: Option<usize>,
    /// `--tlc-workers`, else `RDRA_TLC_WORKERS`: every TLC run's workers.
    pub workers: Option<usize>,
    /// `--plan`: write the TLC launches to this file and start no TLC.
    pub plan: Option<PathBuf>,
}

/// What `verify` spends on a check it may not finish, and whether one it leaves undecided
/// fails the run.
pub struct Budget {
    /// Seconds a temporal property's whole-model run gets.
    pub temporal: Duration,
    /// `--max-patterns`; default `Caps::VERIFY`'s. It caps a run that builds an entity
    /// view, and `verify` builds none (#251), so it stops none of its runs.
    pub max_patterns: Option<usize>,
    /// `--max-states`: the state cap of every BFS run, the whole-model one and TLC's included.
    /// Without it BFS has no state cap; TLC's whole-model run keeps `WHOLE_MODEL_STATE_CAP`.
    pub max_states: Option<usize>,
    /// `--deny-undecided`: an undecided finding is an error, not a warning.
    pub deny_undecided: bool,
    /// `--prove`: z3 tries each state rule for any number of rows.
    pub prove: Option<Prove>,
    /// `--memory`, split between BFS and the TLC runs under `--with-tlc`.
    pub memory: VerifySplit,
    /// `--symmetry`: which safety cones run up to instance symmetry.
    pub symmetry: Symmetry,
}

/// `verify --prove`.
pub struct Prove {
    /// `--z3`: the binary; else `RDRA_Z3`, else `z3` on PATH.
    pub z3: Option<PathBuf>,
    /// `--deny-unproved`: a rule left unproved fails the run.
    pub deny_unproved: bool,
}

/// Runs `verify` at `bounds` (`export::verify_bounds`); `with_tlc` is
/// `--with-tlc` and its `--tlc-jar`.
pub fn run(
    program: &ResolvedProgram,
    model: &SemanticModel,
    out: Option<PathBuf>,
    bounds: rdra_ish_core::lifecycle::Bounds,
    scope: &Scope,
    Budget {
        temporal: whole_time,
        max_patterns,
        max_states,
        deny_undecided,
        prove,
        memory,
        symmetry,
    }: Budget,
    with_tlc: Option<WithTlc>,
) -> Result<()> {
    rdra_ish_core::perf::pass("build_ts");
    let built = checked_system(model, bounds.clone(), scope)?;
    let (ts, selection) = (built.ts, built.selection);
    #[cfg(feature = "perf")]
    ts.perf_guards_line();
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
    // No `--max-states`: no state cap, and the memory limit stops a cone. No run of `verify`
    // reads an entity view, so none gathers the patterns the pattern cap counts (#251).
    let caps = Caps::new(
        max_patterns.unwrap_or(Caps::VERIFY.patterns),
        max_states.unwrap_or(usize::MAX),
    )
    .without_views()
    // Under `always`, a sub-lifecycle whose run stops at the memory limit runs
    // again up to `H` (#212 phase 2): only a cone that stopped there changes.
    .with_s_up_to_h(symmetry != Symmetry::Never)
    // Under `always`, an entity cone that stops at the memory limit runs again up to
    // symmetry (#212 phase 4): only a cone that stopped there changes.
    .with_entity_orbits(symmetry != Symmetry::Never)
    .charged_to(MemoryPool::share_of(
        memory.limit,
        memory.bfs,
        Box::leak(raise.into_boxed_str()),
    ));
    let budget = WholeRun {
        budget: whole_time,
        states: max_states,
    };
    // The quotients the passes that reduce cones share: a sub-lifecycle many cones have is
    // explored and partitioned once (#252). The entity pass, the last of them, drops it.
    let quotients = QuotientCache::new();
    rdra_ish_core::perf::pass("cone_jobs");
    let mut jobs = cone_jobs(&ts, &names, &targets, &mut modules, caps, &quotients);
    let cones = jobs.len();
    let line = print_plan(
        model,
        &ts,
        scope,
        &selection,
        (bounds, caps),
        (cones, temporal, with_tlc.is_some()),
        budget.budget,
    );
    let mut whole_modules = Vec::new();
    if temporal {
        rdra_ish_core::perf::pass("property_jobs");
        jobs.extend(property_jobs(&ts, &names, &mut modules, caps, &quotients));
        whole_modules = (0..ts.properties().len())
            .map(|i| modules.claim(&format!("{whole_stem}_{}", names.property(i))))
            .collect();
        // TLC's second opinion on `stable`, `unchanged`, and `precedes`; the native checker
        // decides them without a job.
        if with_tlc.is_some() {
            jobs.extend(explored_jobs(&ts, &names, &mut modules));
        }
    }
    // The modules are written for `-o`, a run by hand, and for TLC under `--with-tlc`.
    rdra_ish_core::perf::pass("emit_setup");
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
    let machine = (with_tlc.as_ref()).map_or(Machine::here(None, None), |w| {
        Machine::here(w.workers, w.jobs)
    });
    // The cone runs pick their jobs as BFS counts them (#384), on half of `--memory` until
    // BFS ends and all of it after.
    let pool = Pool::new(machine, memory.cones, jobs.len(), false);
    // `--plan` writes the launches for a caller that runs TLC itself, and starts none.
    let plan = with_tlc.as_ref().and_then(|w| w.plan.clone());
    let tlc = with_tlc
        .filter(|_| plan.is_none())
        .map(|w| Tlc::find(w.jar));
    let z3 = prove.as_ref().map(|p| find_z3(p.z3.clone()));
    let start = match (&dir, &written, &tlc, &z3) {
        (Some(dir), Some(Ok(_)), Some(Ok(tlc)), None | Some(Ok(_))) => Some((tlc, dir)),
        _ => None,
    };
    let (bfs, mut natives, explored, proof, cones_ran) = std::thread::scope(|threads| {
        let cones =
            start.map(|(tlc, dir)| threads.spawn(|| run_cones(tlc, dir, &modules, &jobs, &pool)));
        // A cone the entity, safety and property passes share runs once (#209): the pass that
        // runs it first decides the later passes' jobs on its run.
        let covers = covering(&jobs);
        let mut shared = Shared::new(&jobs, symmetry);
        let wanted = shared.wanted(&jobs);
        rdra_ish_core::perf::pass("entity_pass");
        let bfs = state_models(
            program,
            model,
            &ts,
            scope,
            &selection,
            caps,
            start.is_some(),
            quotients,
            &wanted,
            |run| shared.decide_on(&ts, &jobs, &covers, run),
        );
        let shared_safety: Vec<Option<&BfsCone>> = (shared.safety.iter())
            .map(|s| s.as_ref().map(|(c, _)| c))
            .collect();
        pool.learn(counted(&shared_safety, &shared, &[]));
        rdra_ish_core::perf::pass("safety_pass");
        let (mut safety, mut vacuous) = bfs_cones(&ts, &jobs, caps, &covers, &mut shared);
        // A violation a cone cannot vouch for is decided on its exact cone before the proof
        // reads what BFS refuted.
        recheck_cones(&ts, &jobs, &mut safety, caps);
        let safety_ran: Vec<Option<&BfsCone>> = safety.iter().map(Option::as_ref).collect();
        pool.learn(counted(&safety_ran, &shared, &[]));
        // The proof needs only the rules the safety pass refuted, so it starts here, beside
        // the property and whole-model passes, its z3 processes on cores the TLC runs leave
        // (#421). Its lines print where they always have.
        let proof = match (&prove, z3) {
            (Some(Prove { deny_unproved, .. }), Some(z3)) => {
                let refuted = refuted::refuted(&ts, &jobs, &safety);
                let (ts, selection, pool, deny) = (&ts, &selection, &pool, *deny_unproved);
                Some(threads.spawn(move || {
                    let _cores = pool.hold_cores(rdra_ish_core::jobs());
                    let z3 = z3.map(|z3| {
                        let version = Z3::version(&z3).unwrap_or_default();
                        (z3, version)
                    });
                    // A z3 the limits were not measured on: the note prints even when the
                    // proof then fails.
                    let note = z3.as_ref().ok().and_then(|(_, v)| unverified_z3_note(v));
                    let ran = z3.and_then(|(z3, version)| {
                        let marks = Marks::of(model, ts);
                        let twin = rdra_ish_core::lifecycle::prove::mining_twin(model, scope, ts);
                        let tool = Z3Tool {
                            binary: &z3,
                            version: &version,
                        };
                        run_proof(ts, twin.as_ref(), selection, &marks, &refuted, tool, deny)
                    });
                    (note, ran)
                }))
            }
            _ => None,
        };
        rdra_ish_core::perf::pass("property_pass");
        let (properties, property_vacuous) = native_properties(&ts, &jobs, caps, &mut shared);
        pool.learn(counted(&[], &shared, &properties));
        // A `--buc` drops use cases that may reach a premise.
        if scope.bucs.is_empty() {
            vacuous.extend(property_vacuous);
            for d in vacuous {
                if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                    eprintln!("{}", Finding::new(&d.diag, &[], text));
                }
            }
        }
        rdra_ish_core::perf::pass("explored_props");
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
        rdra_ish_core::perf::pass("wholes");
        let wholes = native_wholes(&ts, &open, caps, budget);
        // BFS has let go of its half: the cone runs that start from here take all of it.
        pool.bfs_ended(memory.limit.bytes());
        rdra_ish_core::perf::pass("proof");
        let proof = match proof.map(|p| p.join()) {
            Some(Ok((note, ran))) => {
                if let Some(note) = note {
                    eprintln!("{note}");
                }
                ran.map(|run| {
                    print!("{}", run.text);
                    Some(run)
                })
            }
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            None => Ok(None),
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
    rdra_ish_core::perf::pass("report");
    let proof = proof?;
    // A cone BFS left open at the memory limit is decided by the rules the proof proved.
    if let Some(run) = &proof {
        natives.safety = decide_by_proof(&ts, &jobs, natives.safety, &run.proved);
    }
    // A rule the proof left unproved and BFS passed is checked again at more instances, on
    // its own cone (#421); a counterexample there is an error.
    let escalated = match &proof {
        Some(run) => escalate::escalate(
            (program, model),
            (&ts, &ts.bounds, scope),
            (&jobs, &natives.safety),
            &run.unproved,
            caps,
        ),
        None => 0,
    };
    let mut end = End {
        bfs,
        explored,
        safety: Safety::default(),
        temporal: Temporal::default(),
        tlc: TlcEnd::default(),
        deny_undecided,
        proof: proof.map(|run| run.tally),
        escalated,
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

    if let (Some(path), Some(dir)) = (&plan, &dir) {
        let n = plan::write_plan(
            path,
            dir,
            (&jobs, &whole_modules),
            (&natives, &end.explored),
            (machine, memory.cones),
        )?;
        println!("plan: {} ({n} TLC launches; no TLC ran)", path.display());
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
            (budget, memory.wholes, machine),
            (&jobs, &ran),
            (index, index_path.as_path()),
        )?;
        // The state models of the entity cones BFS explored to the end, after the cone runs,
        // whose memory share they take.
        let models = &end.bfs.models;
        let model_jobs = state_model_jobs(dir, &mut modules, models, (index, index_path))?;
        let model_ran = run_state_models(&tlc, dir, &modules, &model_jobs, memory.cones)?;
        for (job, r) in model_jobs.iter().zip(&model_ran) {
            println!("tlc: {}", r.line(job));
        }
        for (module, states) in &models.skipped {
            println!(
                "tlc: {module} ({states} states by BFS): not run, over the limit of {STATE_MODEL_TLC_STATES} states"
            );
        }
        disagreed = disagreements(
            &whole_modules,
            &jobs,
            (&natives, &end.explored),
            (&ran, &whole_ran),
        );
        disagreed.extend(state_model_disagreements(models, &model_jobs, &model_ran));
        let all_jobs: Vec<&Job> = jobs.iter().chain(&wholes).collect();
        let all_ran: Vec<&Ran> = ran.iter().chain(&whole_ran).collect();
        let model_outcomes: Vec<Outcome> = model_ran.iter().map(|r| r.outcome()).collect();
        let outcome = worst(
            all_ran
                .iter()
                .map(|r| r.decision.outcome())
                .chain(model_outcomes.iter().copied()),
        );
        end.tlc.tool_errors = all_ran
            .iter()
            .map(|r| r.decision.outcome())
            .chain(model_outcomes.iter().copied())
            .filter(|o| *o == Outcome::ToolError)
            .count();
        if outcome != Outcome::Ok {
            tlc_failures(program, model, (&ts, &names), dir, (&all_jobs, &all_ran));
        }
        for (job, r) in model_jobs.iter().zip(&model_ran) {
            if let Some(why) = &r.error {
                eprintln!(
                    "tlc: {} ({}):\n{why}",
                    job.module,
                    dir.join(format!("{}.tla", job.module)).display()
                );
            }
        }
        tlc_outcome = Some(outcome);
    }
    for d in &disagreed {
        eprintln!("{d}");
    }
    end.tlc.disagreements = disagreed.len();

    // `result:` states the verdict of the whole run; a disagreement reports neither engine's
    // verdict, and counts as a tool error.
    let result = end.result(undecided.len());
    let say = |label: &str, verdict: &str| match verdict {
        "ok" => println!("{label}: ok"),
        other => eprintln!("{label}: {other}"),
    };
    say("result", result.as_str());
    if let Some(outcome) = tlc_outcome {
        say(
            "tlc_result",
            match disagreed.is_empty() {
                true => outcome,
                false => Outcome::ToolError,
            }
            .as_str(),
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

/// The states BFS counted of each safety and property cone job (`BfsCone::counted`,
/// `Plain::counted`, `Native::counted`), for the TLC scheduler (#384).
fn counted(
    safety: &[Option<&BfsCone>],
    shared: &self::shared::Shared,
    properties: &[Option<Native>],
) -> Vec<(usize, u64)> {
    let safety = (safety.iter().enumerate()).filter_map(|(k, c)| Some((k, c.as_ref()?.counted()?)));
    let plain =
        (shared.plain.iter().enumerate()).filter_map(|(k, p)| Some((k, p.as_ref()?.counted()?)));
    let native =
        (properties.iter().enumerate()).filter_map(|(k, n)| Some((k, n.as_ref()?.counted?)));
    safety.chain(plain).chain(native).collect()
}

/// The worst of `outcomes`: a counterexample over a tool error over `ok`.
fn worst(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
    outcomes.into_iter().fold(Outcome::Ok, |a, b| match (a, b) {
        (Outcome::Counterexample, _) | (_, Outcome::Counterexample) => Outcome::Counterexample,
        (Outcome::ToolError, _) | (_, Outcome::ToolError) => Outcome::ToolError,
        _ => Outcome::Ok,
    })
}
