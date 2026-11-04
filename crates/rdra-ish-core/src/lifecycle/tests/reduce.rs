//! A cone reduced by its independent sub-lifecycle decides what the cone decides: the same
//! states on the target side and what it reads of the sub-lifecycle, the same views and
//! verdicts, and traces that lift to real paths of the cone.

use std::collections::BTreeSet;

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{
    Action, ActionOrigin, ComponentRun, State, Step, Target, TargetExploration, VarSet,
};

const DOMAIN_SAMPLES: &[&str] = &[
    "samples/clinic-ops",
    "samples/patent-ops",
    "samples/food-permit-ops",
    "samples/insurance-claim-ops",
];

/// A gate with an unobserved latch and a key behind it; a job starts only at an open gate.
/// The job's cone reduces the gate, whose own cone reduces the key.
const GATE: &str = r#"
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
entity Gate "門" { id: Int @pk  status: Enum(shut, half, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Gate, Key, N:1)
relate(Job, Gate, N:1)
usecase Unlock "解錠"
usecase Crack "少し開ける"
usecase Unlatch "掛け金を外す"
usecase Swing "開く"
usecase Submit "投入"
usecase Start "開始"
transitions(Unlock, Key.status, locked -> unlocked)
transitions(Crack, Gate.status, shut -> half)
sets(Unlatch, Gate.latch == true)
transitions(Swing, Gate.status, half -> open)
condition Unlatched "掛け金が外れている"
holds(Unlatched, Gate.latch == true)
applies_to(Unlatched, Swing)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// A split use case `Go` (two APIs, so `Go_1` and `Go_2`) beside a use case named `Go_1`
/// that moves the gate the job's cone reduces: the gate's action and the job's first step
/// share a name, and only the action's identity tells them apart.
const COLLIDING_T_FIRST: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Go_1 "門を開ける"
usecase BuildGate "設置"
usecase Submit "投入"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Go_1, Gate.status, shut -> open)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// The same model with the gate's use case declared first.
const COLLIDING_S_FIRST: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Go_1 "門を開ける"
usecase Go "開始"
usecase BuildGate "設置"
usecase Submit "投入"
api JobApi "作業 API"
api NoteApi "記録 API"
transitions(Go_1, Gate.status, shut -> open)
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// The collision with a second entity the first step writes, so the colliding actions
/// differ in their params as well.
const COLLIDING_TWO_PARAMS: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut) }
entity Note "記録" { id: Int @pk  status: Enum(written) @default(written) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
entity Crew "班" { id: Int @pk  status: Enum(idle, busy) @default(idle) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase Go_1 "門を開ける"
usecase BuildGate "設置"
usecase Submit "投入"
usecase Hire "雇う"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
creates(Hire, Crew)
invokes(Go, JobApi)
updates(JobApi, Job)
updates(JobApi, Crew)
transitions(Go, Job.status, waiting -> running)
transitions(Go, Crew.status, idle -> busy)
invokes(Go, NoteApi)
creates(NoteApi, Note)
transitions(Go_1, Gate.status, shut -> open)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
"#;

fn entity_target(ts: &TransitionSystem, id: &str) -> Target {
    ts.targets()
        .into_iter()
        .find(|t| matches!(t, Target::Entity(e) if ts.entity(*e).id == id))
        .unwrap_or_else(|| panic!("entity {id}"))
}

fn view_set(v: &EntityStateResult) -> BTreeSet<String> {
    v.patterns
        .iter()
        .map(|p| {
            format!(
                "{:?} initial={} terminal={} via={:?}",
                p.pattern.values, p.is_initial, p.is_terminal, p.provenance.via
            )
        })
        .collect()
}

fn diag_names(run: &TargetExploration) -> BTreeSet<String> {
    run.verdicts().iter().map(|d| d.diag.name(&[])).collect()
}

fn violated(run: &TargetExploration) -> Vec<&StateDiag> {
    run.verdicts()
        .iter()
        .map(|d| &d.diag)
        .filter(|d| matches!(d, StateDiag::ConstraintViolated { .. }))
        .collect()
}

/// The BFS path to `states[at]`, lifted to a path of the unreduced cone.
fn lifted_path(run: &ComponentRun, at: u32) -> (State, Vec<Step>) {
    let path = run.path_to(at);
    let first = path.first().map_or(at, |e| e.from);
    let steps: Vec<Step> = path
        .iter()
        .map(|e| {
            let (act, params) = run.step(e);
            (act, params.into(), run.states[e.to as usize].clone())
        })
        .collect();
    run.system.lifted(&run.states[first as usize], &steps)
}

/// Every step of a lifted path is a move of the unreduced cone system from the state before
/// it, and its start is an initial state of that system.
fn assert_real_path(plain: &TransitionSystem, start: &State, steps: &[Step]) {
    assert!(
        plain.initial_states().contains(start),
        "the lifted path starts at an initial state"
    );
    let mut cur = start.clone();
    for (act, params, next) in steps {
        let found = plain
            .plain_moves(&cur)
            .into_iter()
            .any(|(a, p, n)| a == *act && p == *params && n == *next);
        assert!(
            found,
            "step {} of the lifted path is a real move",
            plain.actions()[act.index()].name
        );
        cur = next.clone();
    }
}

/// The reduced cones of a model decide what its cones decide, and their traces lift.
fn assert_reduction_exact(sample: &str, model: &SemanticModel, bounds: Bounds) -> usize {
    let caps = Caps::new(usize::MAX, 200_000);
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let targets = ts.targets();
    let plain = ts.explore_targets(&targets, caps);
    let reduced = ts.explore_targets_reduced(&targets, caps);
    let plain_views = plain.entity_views(&ts, model, &[]);
    let reduced_views = reduced.entity_views(&ts, model, &[]);
    let mut reductions = 0;
    for target in &targets {
        let (p, r) = (plain.cone_of(*target), reduced.cone_of(*target));
        if !p.run.complete || !r.run.complete {
            continue;
        }
        let Some(red) = r.run.system.reduction() else {
            continue;
        };
        reductions += 1;
        let sys = &r.run.system;
        let readable: VarSet = sys
            .var_ids()
            .filter(|v| !red.is_s_var(*v) || red.obs().contains(v))
            .collect();
        let project = |run: &ComponentRun| -> BTreeSet<State> {
            run.states
                .iter()
                .map(|s| sys.project(s, &readable))
                .collect()
        };
        assert_eq!(
            project(&p.run),
            project(&r.run),
            "{sample} {target:?}: states on T and obs"
        );
        if let Target::Entity(e) = target {
            let id = &ts.entity(*e).id;
            let a = plain_views.iter().find(|v| v.entity_id == *id).unwrap();
            let b = reduced_views.iter().find(|v| v.entity_id == *id).unwrap();
            assert_eq!(view_set(a), view_set(b), "{sample} {id}: view");
        }
        // Every trace of the reduced run lifts to a real path of the cone.
        let unreduced = ts.restrict_to(&r.cone.vars, &r.cone.keep);
        for at in (0..r.run.states.len() as u32).rev().take(8) {
            let (start, steps) = lifted_path(&r.run, at);
            assert_real_path(&unreduced, &start, &steps);
            let last = steps.last().map_or(&start, |s| &s.2);
            assert_eq!(
                sys.project(last, &readable),
                sys.project(&r.run.states[at as usize], &readable),
                "{sample} {target:?}: the lifted path ends in the state's class"
            );
        }
    }
    // A cone the cap stops decides less; the reduced one may decide it.
    let (a, b) = (diag_names(&plain), diag_names(&reduced));
    if targets.iter().all(|t| plain.cone_of(*t).run.complete) {
        assert_eq!(a, b, "{sample}: verdicts");
    } else {
        let decided: BTreeSet<&String> = a.iter().filter(|n| *n != "StateCapReached").collect();
        assert!(
            decided.iter().all(|n| b.contains(*n)),
            "{sample}: {a:?} vs {b:?}"
        );
    }
    reductions
}

fn reductions_in(samples: &[&str]) -> usize {
    let mut reductions = 0;
    for sample in samples {
        let model = load(sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            reductions += assert_reduction_exact(sample, &model, bounds);
        }
    }
    reductions
}

#[test]
fn reduced_cones_decide_what_the_cones_decide() {
    let reductions = reductions_in(SAMPLES);
    assert!(reductions >= 30, "{reductions} cones reduced");
}

/// The domain samples explore millions of states; CI runs this in release in formal-verify.
#[test]
fn reduced_domain_cones_decide_what_the_cones_decide() {
    let reductions = reductions_in(DOMAIN_SAMPLES);
    assert!(reductions >= 100, "{reductions} cones reduced");
}

#[test]
fn a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace() {
    let model = model_from(GATE);
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let job = entity_target(&ts, "Job");
    let rule = Target::Invariant(0);
    let caps = Caps::STATES;
    let plain = ts.explore_targets(&[job, rule], caps);
    let reduced = ts.explore_targets_reduced(&[job, rule], caps);
    let cone = reduced.cone_of(rule);
    let red = cone
        .run
        .system
        .reduction()
        .expect("the job's cone reduces the gate");
    let s_names: Vec<String> = red
        .vars()
        .iter()
        .map(|v| cone.run.system.var_name(*v))
        .collect();
    assert!(
        s_names.iter().any(|n| n == "latch") && s_names.iter().any(|n| n == "Key._exists"),
        "S holds the gate and the key: {s_names:?}"
    );
    assert!(
        cone.run.states.len() < plain.cone_of(rule).run.states.len(),
        "the reduced cone is smaller"
    );
    let (a, b) = (violated(&plain), violated(&reduced));
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    let steps = |d: &StateDiag| -> Vec<String> {
        d.trace()
            .expect("a violation has a trace")
            .steps
            .iter()
            .map(|s| s.action.clone())
            .collect()
    };
    let lifted = steps(b[0]);
    assert!(
        lifted.contains(&"Unlatch".to_string()) && lifted.contains(&"Unlock".to_string()),
        "the lifted trace spells the gate's and the key's steps: {lifted:?}"
    );
    assert_eq!(lifted.last().map(String::as_str), Some("Start"));
    assert!(
        lifted.len() > steps(a[0]).len() - 1,
        "{lifted:?} vs {:?}",
        steps(a[0])
    );
    assert_eq!(a[0].name(&[]), b[0].name(&[]));
}

/// Two actions of one name are two actions: the reduced cone relates the gate's action to
/// itself, not to the job's first step, so it reaches what the cone reaches.
#[test]
fn a_sub_lifecycle_action_that_shares_a_name_with_a_target_step_keeps_its_identity() {
    for (name, src, violated_count) in [
        ("T first", COLLIDING_T_FIRST, 1),
        ("S first", COLLIDING_S_FIRST, 1),
        ("two params", COLLIDING_TWO_PARAMS, 0),
    ] {
        let model = model_from(src);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let ts = TransitionSystem::build(&model, bounds, &Scope::ALL).ts;
            let targets = ts.targets();
            let plain = ts.explore_targets(&targets, Caps::VERIFY);
            let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
            let job = entity_target(&ts, "Job");
            let cone = reduced.cone_of(job);
            let sys = &cone.run.system;
            let red = sys.reduction().expect("the job's cone reduces the gate");
            let is_gate_opener = |a: &Action| matches!(&a.origin, ActionOrigin::UseCase { uc, .. } if model.use_cases[*uc].id == "Go_1");
            assert!(
                red.s_actions()
                    .iter()
                    .any(|a| is_gate_opener(&sys.actions()[a.index()])),
                "{name}: the gate's Go_1 is an S action"
            );
            let mut names: Vec<&str> = sys
                .actions()
                .iter()
                .filter(|a| a.name.starts_with("Go_1"))
                .map(|a| a.name.as_str())
                .collect();
            names.sort_unstable();
            assert_eq!(names, ["Go_1", "Go_1_2"], "{name}: the step named apart");
            assert_eq!(
                diag_names(&plain),
                diag_names(&reduced),
                "{name} at {bounds:?}"
            );
            assert_eq!(
                violated(&reduced).len(),
                violated_count,
                "{name} at {bounds:?}"
            );
            // Every cone finishes, so no action's run is left undecided, whichever action of
            // the two of one name a cone kept.
            for run in [&plain, &reduced] {
                assert!(
                    run.undecided().is_empty(),
                    "{name} at {bounds:?}: {:?}",
                    run.undecided()
                );
            }
        }
    }
}

#[test]
fn the_job_cone_nests_two_reductions() {
    let model = model_from(GATE);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let job = entity_target(&ts, "Job");
    let run = ts.explore_targets_reduced(&[job], Caps::VERIFY);
    let sys = &run.cone_of(job).run.system;
    let red = sys.reduction().expect("reduced");
    let sub = sys.sub_system(red);
    let inner = sub.reduction().expect("the gate's cone reduces the key");
    let key_vars: Vec<String> = inner.vars().iter().map(|v| sub.var_name(*v)).collect();
    assert!(
        key_vars
            .iter()
            .all(|n| n.starts_with("Key") || n == "status"),
        "{key_vars:?}"
    );
    assert!(
        red.classes() < red.states,
        "{} classes of {} states",
        red.classes(),
        red.states
    );
}
