use super::bfs::{BfsCone, Explored};
use super::jobs::{Job, JobKind};
use super::native::Natives;
use super::tlc::{Decision, Ran};
use crate::property_cone::ConeRun;
use crate::tlc_output::Outcome;
use crate::tlc_replay::{explain_property, explain_violation};
use rdra_ish_core::lifecycle::TransitionSystem;
use rdra_ish_emit::tla::TlaNames;
use std::collections::BTreeSet;

/// Under `--with-tlc`, each check the two engines decide differently, as an
/// `error[EngineDisagreement]` line: a safety cone's verdict and the checks its counterexample
/// breaks, a property's verdict on each run both made and in the end, and the final verdict
/// of a `stable`, `unchanged`, or `precedes`. A check one engine
/// leaves undecided, or a TLC run that failed as a tool, is no disagreement.
pub(super) fn disagreements(
    whole_modules: &[String],
    jobs: &[Job],
    (natives, explored): (&Natives, &Explored),
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
                // A trace that does not replay is reported by `unreplayed`, whatever the
                // native verdict; the sets are compared only when every trace replays.
                let theirs: Option<BTreeSet<String>> = (tlc.run.violations.iter())
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
            JobKind::Explored(e) => {
                // The final verdicts: the whole-model run's where a cone left it open.
                let tlc_whole = match tlc.decision {
                    Decision::Undecided { .. } => tlc_wholes.next(),
                    Decision::Decided(_) => None,
                };
                let (a, b) = (
                    explored.verdict(e.index),
                    tlc_whole.unwrap_or(tlc).decision.verdict(),
                );
                if differ(a, b) {
                    let module = match tlc_whole {
                        Some(_) => &whole_modules[e.index],
                        None => &job.module,
                    };
                    out.push(line(module, &job.checks, as_str(a), as_str(b)));
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
                    if differ(*mine, theirs) {
                        out.push(line(
                            &module(*run),
                            &job.checks,
                            as_str(*mine),
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

/// Under `--with-tlc`, one `error[EngineDisagreement]` line per job with a TLC counterexample
/// that does not replay through BFS's step relation, any of its traces: TLC reached a state,
/// or took a step, the native checker's system does not have, so one engine is wrong whatever
/// the native verdict. `jobs` and `ran` hold the cone, property, and whole-model jobs.
pub(super) fn unreplayed(
    (ts, names): (&TransitionSystem, &TlaNames),
    (jobs, ran): (&[&Job], &[&Ran]),
) -> Vec<String> {
    let mut out = Vec::new();
    for (job, tlc) in jobs.iter().zip(ran) {
        let failed: Vec<String> = (tlc.run.violations.iter())
            .filter_map(|v| {
                let replayed = match &job.kind {
                    JobKind::Cone(cs) => explain_violation(&cs.system, &cs.names, v).map(drop),
                    JobKind::PropertyCone(p) => {
                        explain_property(&p.cone.system, &p.names, p.cone.index, v).map(drop)
                    }
                    JobKind::Explored(e) => {
                        explain_property(&e.system, &e.names, e.cone_index, v).map(drop)
                    }
                    JobKind::Whole(i) => explain_property(ts, names, *i, v).map(drop),
                };
                replayed.err()
            })
            .collect();
        let Some(first) = failed.first() else {
            continue;
        };
        out.push(format!(
            "error[EngineDisagreement]: {} [{}]: {} of TLC's {} counterexample(s) do not replay through BFS's step relation (the first: {first}); one engine is wrong (a tool bug, not a finding about the model)",
            job.module,
            job.checks.join(", "),
            failed.len(),
            tlc.run.violations.len(),
        ));
    }
    out
}

fn as_str(outcome: Option<Outcome>) -> &'static str {
    outcome.map_or("not decided", Outcome::as_str)
}

#[cfg(test)]
mod tests {
    use super::super::jobs::whole_job;
    use super::super::tlc::Ran;
    use super::unreplayed;
    use crate::tlc_output::{TlcRun, TraceStep, Violation, ViolationKind};
    use rdra_ish_core::lifecycle::{Bounds, Scope, TransitionSystem};
    use rdra_ish_emit::tla::TlaNames;
    use std::path::Path;

    /// A TLC counterexample whose trace leaves the native step relation is a disagreement,
    /// on any job, and one that replays is none. Such a trace was once only printed, and
    /// on a safety cone it silently skipped the comparison of the violated sets (P12 of
    /// docs/theory/README.md).
    #[test]
    fn a_tlc_trace_that_does_not_replay_is_a_disagreement() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/formal-verification/temporal_unmet.rdra");
        let model = rdra_ish_core::load_project(&[sample], None)
            .expect("the sample loads")
            .model;
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let names = TlaNames::new(&ts);
        let i = (ts.properties().iter())
            .position(|p| p.formula.kind().is_liveness())
            .expect("a liveness property");
        let job = whole_job(&ts, &names, "RdraSpec_P", i);
        let stranger = Violation {
            kind: ViolationKind::Temporal,
            name: None,
            line: "Error: Temporal properties were violated.".to_string(),
            trace: vec![
                TraceStep {
                    label: "Initial predicate".to_string(),
                    state: Default::default(),
                },
                TraceStep {
                    label: "NoSuchAction".to_string(),
                    state: Default::default(),
                },
            ],
            lasso: None,
        };
        let ran = Ran::counterexample(TlcRun {
            violations: vec![stranger],
            ..TlcRun::default()
        });
        let lines = unreplayed((&ts, &names), (&[&job], &[&ran]));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("error[EngineDisagreement]: RdraSpec_P ["),
            "{lines:?}"
        );
        let none = Ran::counterexample(TlcRun::default());
        assert!(unreplayed((&ts, &names), (&[&job], &[&none])).is_empty());
    }
}
