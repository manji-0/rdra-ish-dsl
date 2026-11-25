//! `verify --with-tlc --plan <FILE>` (#384, experimental): the TLC launches `--with-tlc` would
//! start first, as JSON, for a caller that runs TLC itself. No TLC starts. Each launch names
//! its module, cfg, kind, checks, the cone it is checked in when it is not run, what BFS
//! decided and counted, and the heap, workers and arguments the scheduler would give it
//! (`schedule::target`), with the runs that follow from what it prints. The format is
//! `"version": 1` and carries no compatibility promise
//! (docs/design/tlc-scheduling.md, "A caller that runs TLC itself").

use super::bfs::{BfsCone, Explored};
use super::jobs::{Job, JobKind};
use super::native::Natives;
use super::schedule::{target, Free, Machine, Want};
use super::tlc::covering;
use crate::memory::TlcHeap;
use crate::tlc_env::{current_user_heap, heap_from_states, xmx, Heap};
use crate::tlc_output::Outcome;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// What BFS decided of one job: its verdict (`None`: not decided), the states its run
/// stored, and the states of a complete plain run, which size the TLC run.
struct Bfs {
    verdict: Option<Outcome>,
    states: Option<u64>,
    counted: Option<u64>,
}

fn bfs_of(job: &Job, k: usize, natives: &Natives, explored: &Explored) -> Bfs {
    match &job.kind {
        JobKind::Cone(_) => {
            let cone = natives.safety[k].as_ref();
            Bfs {
                verdict: cone.and_then(BfsCone::verdict),
                states: cone.and_then(|c| match c {
                    BfsCone::Ran { states, .. } | BfsCone::Proved { states } => {
                        Some(*states as u64)
                    }
                    BfsCone::Covered(_) => None,
                }),
                counted: cone.and_then(BfsCone::counted),
            }
        }
        JobKind::PropertyCone(_) => {
            let native = natives.properties[k].as_ref();
            Bfs {
                verdict: native.and_then(|n| n.decision.verdict()),
                states: native.and_then(|n| n.states),
                counted: native.and_then(|n| n.counted),
            }
        }
        JobKind::Explored(e) => Bfs {
            verdict: explored.verdict(e.index),
            states: None,
            counted: None,
        },
        JobKind::Whole(_) => Bfs {
            verdict: None,
            states: None,
            counted: None,
        },
    }
}

/// The runs that follow a launch, by what it prints, as `run_job` makes them.
fn follow_ups(job: &Job, whole: Option<&str>, workers: usize) -> Vec<Value> {
    let mut out = Vec::new();
    match &job.kind {
        JobKind::Cone(_) => {
            if workers > 1 {
                out.push(json!({
                    "when": "the run does not pass and did not run out of memory",
                    "run": "the same module and cfg on -workers 1; that run is the one reported",
                }));
            }
            out.push(json!({
                "when": "a violation, with checks of the cfg left unreported",
                "run": format!("the same module with {}_rest<n>.cfg, without the checks reported so far, until a run reports no new one", job.module),
            }));
        }
        JobKind::PropertyCone(p) => {
            if let Some(full) = &p.full {
                out.push(json!({
                    "when": "a counterexample",
                    "module": full.module,
                    "cfg": format!("{}.cfg", full.module),
                }));
            }
            if let Some(wf) = &p.wf {
                out.push(json!({
                    "when": "the verdict so far asks for WF_vars(Next)",
                    "module": wf.module,
                    "cfg": format!("{}.cfg", wf.module),
                }));
            }
        }
        JobKind::Explored(_) | JobKind::Whole(_) => {}
    }
    if let Some(whole) = whole {
        out.push(json!({
            "when": "the cone leaves the property not decided",
            "module": whole,
            "cfg": format!("{whole}.cfg"),
            "note": "written by verify -o once a run leaves it open; runs under --temporal-budget and the whole-model state cap",
        }));
    }
    out.push(json!({
        "when": "the run runs out of memory and the share holds twice its heap",
        "run": "the same launch at twice the heap, once",
    }));
    out
}

/// Writes the plan of `jobs` to `path`: every launch `--with-tlc` would start first, in job
/// order, with the workers the scheduler would give each once BFS has ended. Returns how many
/// launches it lists.
pub(super) fn write_plan(
    path: &Path,
    dir: &Path,
    (jobs, whole_modules): (&[Job], &[String]),
    (natives, explored): (&Natives, &Explored),
    (machine, share): (Machine, u64),
) -> Result<usize> {
    let covers = covering(jobs);
    let set = current_user_heap();
    let by_states = heap_from_states();
    let launched: Vec<usize> = (0..jobs.len())
        .filter(|k| !matches!(jobs[*k].kind, JobKind::Whole(_)))
        .collect();
    let free = Free {
        cores: machine.cores,
        bytes: share,
        share,
        running: 0,
        holding: 0,
        counted: true,
    };
    let launches: Vec<Value> = launched
        .iter()
        .map(|&k| {
            let job = &jobs[k];
            let bfs = bfs_of(job, k, natives, explored);
            let heap = TlcHeap::of(job.module_bytes(), Heap::Cone, share, set);
            let heap = match (by_states, bfs.counted) {
                (true, Some(states)) => heap.with_states(states, share),
                _ => heap,
            };
            let want = Want {
                bytes: heap.charge(),
                states: bfs.counted,
                rank: (heap.charge(), job.weight()),
            };
            let workers = target(&want, &machine, &free, launched.len());
            let cfg = format!("{}.cfg", job.module);
            let tla = format!("{}.tla", job.module);
            let mut args: Vec<String> = vec!["-workers".into(), workers.to_string()];
            if matches!(job.kind, JobKind::Cone(_)) {
                args.push("-continue".into());
            }
            args.extend(["-config".into(), cfg.clone(), tla.clone()]);
            let kind = match &job.kind {
                JobKind::Cone(_) => "cone",
                JobKind::PropertyCone(_) => "property",
                JobKind::Explored(_) => "explored",
                JobKind::Whole(_) => "whole",
            };
            let whole = match &job.kind {
                JobKind::PropertyCone(p) => Some(whole_modules[p.index].as_str()),
                JobKind::Explored(e) => Some(whole_modules[e.index].as_str()),
                JobKind::Cone(_) | JobKind::Whole(_) => None,
            };
            json!({
                "module": job.module,
                "tla": tla,
                "cfg": cfg,
                "kind": kind,
                "checks": job.checks,
                "checked_in": covers[k].map(|by| jobs[by].module.as_str()),
                "bfs": {
                    "verdict": bfs.verdict.map_or("not decided", Outcome::as_str),
                    "states": bfs.states,
                    "counted": bfs.counted,
                },
                "heap": heap.bytes().map(xmx),
                "not_run": match heap {
                    TlcHeap::OverShare { need } => Some(format!("needs a {} heap, more than TLC's share", xmx(need))),
                    TlcHeap::Sized(_) | TlcHeap::User(_) => None,
                },
                "workers": workers,
                "java_tool_options": heap.bytes().map(|b| format!("-Xmx{}", xmx(b))),
                "args": args,
                "follow_ups": follow_ups(job, whole, workers),
            })
        })
        .collect();
    let plan = json!({
        "version": 1,
        "dir": dir.display().to_string(),
        "cores": machine.cores,
        "share": share,
        "launches": launches,
        "not_listed": "the StateModel_ runs of the entity cones, which run after the cone runs",
    });
    let text = serde_json::to_string_pretty(&plan).expect("a plan serializes");
    std::fs::write(path, text + "\n")
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(launches.len())
}
