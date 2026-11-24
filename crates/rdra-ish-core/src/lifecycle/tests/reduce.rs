//! A cone reduced by its independent sub-lifecycle decides what the cone decides: the same
//! states on the target side and what it reads of the sub-lifecycle, the same views and
//! verdicts, and traces that lift to real paths of the cone.

use std::collections::BTreeSet;

use crate::lifecycle::Symmetry;

use crate::memory::{MemoryPool, MemorySize};

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{
    Action, ActionOrigin, ComponentRun, Fold, FoldKey, QuotientCache, State, Step, Target,
    TargetExploration, VarSet,
};

const DOMAIN_SAMPLES: &[&str] = &[
    "samples/clinic-ops",
    "samples/patent-ops",
    "samples/food-permit-ops",
    "samples/insurance-claim-ops",
];

/// A gate with an unobserved latch, a key behind it, and a card behind the key; a job starts
/// only at an open gate. The job's cone reduces the gate, whose own cone reduces the key, in
/// which the card is folded away.
const GATE: &str = r#"
entity Card "札" { id: Int @pk  status: Enum(blank, issued) @default(blank) }
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Issue "札を出す"
transitions(Issue, Card.status, blank -> issued)
condition CardIssued "札が出ている"
holds(CardIssued, Card.status == issued)
applies_to(CardIssued, Unlock)
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
/// share a name, and only the action's identity tells them apart. The key behind `Go_1` is
/// what the reduction folds: the gate's states alone are each a class (#101).
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
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Unlock "解錠"
transitions(Unlock, Key.status, locked -> unlocked)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Go_1)
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
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Unlock "解錠"
transitions(Unlock, Key.status, locked -> unlocked)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Go_1)
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
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Unlock "解錠"
transitions(Unlock, Key.status, locked -> unlocked)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Go_1)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Go)
"#;

/// A random model, minimized (#101): every one of the 1294 states of `P0`'s independent
/// sub-lifecycle is a class of its own, so quotienting it would reduce nothing.
const IDENTITY_QUOTIENT: &str = r#"
entity E0 "e0" {
  status: Enum(s0, s1, s2) @default(s0)
}
entity E1 "e1" {
  status: Enum(s0, s1) @default(s0)
}
entity E2 "e2" {
  status: Enum(s0, s1) @default(s0)
}
entity E3 "e3" {
  status: Enum(s0, s1, s2) @default(s0)
}
usecase New3 "New3"
creates(New3, E3)
usecase T0_0 "T0_0"
usecase T1_0 "T1_0"
usecase T1_1 "T1_1"
usecase T3_0 "T3_0"
transitions(T0_0, E0.status, s1 -> s2)
transitions(T0_0, E0.status, s0 -> s2)
transitions(T1_0, E1.status, s0 -> s1)
condition C0 "C0"
holds(C0, E0.status in (s1, s2)).none(E1.status == s0)
applies_to(C0, T1_0)
transitions(T1_1, E1.status, s0 -> s1)
condition C1 "C1"
holds(C1, E2.status == s0).none(E3.status == s0)
applies_to(C1, T1_1)
transitions(T3_0, E3.status, s0 -> s1)
condition C6 "C6"
holds(C6, E2.status == s1)
transitions(T3_0, E3.status, s1 -> s2).when(C6)
usecase S "S"
transitions(S, E0.status, s2 -> s1)
transitions(S, E2.status, s1 -> s0)
usecase S_1 "S_1"
transitions(S_1, E2.status, s0 -> s1)
property P0
  leads_to(E1.status == s1, E1.status == s1)
"#;

/// A case's `CloseCase` is triggered by either of its claims' events (#112). In the claim's
/// cone the case is the sub-lifecycle: `CloseCase_by_<Ev>` reads the claim's row (its
/// existence, its owner key, its mark) through the `Triggered` param, so both variants fold
/// into one input bit per case; `DetermineFault` reads the claims through a quantifier the
/// node fold refuses (its body names the case param), so the claims' existence and status
/// are raw inputs. A claim stuck at
/// `open` in a case another claim closed is a deadlock that exists only after an input rose;
/// a claim at `paid` whose case was never determined is one that exists only in the
/// interleaving where the input rose before the case moved.
const CLAIM_CASE: &str = r#"
entity Case "事案" { id: Int @pk  status: Enum(open, closed) @default(open)  liability: Enum(undetermined, determined) @default(undetermined) }
entity Claim "請求" { id: Int @pk  status: Enum(open, paid, denied, closed) @default(open) }
relate(Claim, Case, N:1)
usecase OpenCase "受付"
usecase File "請求"
usecase Pay "支払"
usecase Deny "否認"
usecase DetermineFault "過失判定"
usecase CloseCase "事案終了"
usecase Archive "保管"
event EvPaid "支払済"
event EvDenied "否認済"
creates(OpenCase, Case)
creates(File, Claim)
transitions(Pay, Claim.status, open -> paid).when(CaseOpen)
raises(Pay, EvPaid)
transitions(Deny, Claim.status, open -> denied).when(CaseOpen)
raises(Deny, EvDenied)
condition CaseOpen "事案が開いている"
holds(CaseOpen, Case.status == open)
transitions(DetermineFault, Case.liability, undetermined -> determined).when(NoPaid)
condition NoPaid "支払済の請求がない"
holds(NoPaid, Case.status == open).none(Claim.status == paid)
transitions(CloseCase, Case.status, open -> closed).when(Determined)
condition Determined "過失判定済み"
holds(Determined, Case.liability == determined)
triggers(EvPaid, CloseCase)
triggers(EvDenied, CloseCase)
transitions(Archive, Claim.status, paid -> closed).when(CaseClosed)
transitions(Archive, Claim.status, denied -> closed).when(CaseClosed)
condition CaseClosed "事案が終了している"
holds(CaseClosed, Case.status == closed)
forbidden(Claim, status == denied).label("否認は起きない")
"#;

/// The order's payment triggers its shipment's `Ship`, whose `Triggered` param is the
/// shipment's parent, so the link guard keeps it and keys the bit by the order's slot;
/// `Prepare` creates a shipment under an order, and the parent its effect names keys its
/// bit, so the order's existence is no raw input (#112). The order never reads whether a
/// shipment is packed, so the sub-lifecycle reduces.
const ORDER_SHIP: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid, closed, cancelled) @default(open) }
entity Shipment "出荷" { id: Int @pk  status: Enum(waiting, shipped) @default(waiting)  packed: Bool @default(false) }
relate(Shipment, Order, N:1)
usecase Place "注文"
usecase Prepare "準備"
usecase Pack "梱包"
usecase Pay "支払"
usecase Ship "出荷"
usecase Close "完了"
usecase Cancel "取消"
event EvPaid "支払済"
creates(Place, Order)
creates(Prepare, Shipment)
sets(Pack, Shipment.packed == true)
transitions(Pay, Order.status, open -> paid)
raises(Pay, EvPaid)
transitions(Ship, Shipment.status, waiting -> shipped).when(Packed)
condition Packed "梱包済み"
holds(Packed, Shipment.packed == true)
triggers(EvPaid, Ship)
transitions(Close, Order.status, paid -> closed).when(Shipped)
condition Shipped "出荷済み"
holds(Shipped, Order.status == paid).has(Shipment.status == shipped)
transitions(Cancel, Order.status, open -> cancelled)
forbidden(Order, status == cancelled).label("取消は起きない")
"#;

/// `Pay` reads the case through a quantifier with no T param (the condition joins the claim
/// to its case by the owner), so in the case's view the read is a node fold: one bit per
/// case, "the case is open", keyed by the row the link names; `reviewed` is read by no one
/// on the case's side, so the claims' sub-lifecycle reduces. A claim still open when its
/// case closes can never be paid (the bit fell), so the case is never archived.
const CASE_FOLD: &str = r#"
entity Case "事案" { id: Int @pk  status: Enum(open, closed, archived) @default(open) }
entity Claim "請求" { id: Int @pk  status: Enum(open, paid) @default(open)  reviewed: Bool @default(false) }
relate(Claim, Case, N:1)
usecase OpenCase "受付"
usecase File "請求"
usecase Review "査読"
usecase Pay "支払"
usecase CloseCase "事案終了"
usecase ArchiveCase "事案保管"
event EvPaid "支払済"
creates(OpenCase, Case)
creates(File, Claim)
sets(Review, Claim.reviewed == true)
transitions(Pay, Claim.status, open -> paid).when(PayReady)
condition PayReady "査読済みで事案が開いている"
holds(PayReady, Claim.reviewed == true, Case.status == open)
raises(Pay, EvPaid)
transitions(CloseCase, Case.status, open -> closed)
triggers(EvPaid, CloseCase)
transitions(ArchiveCase, Case.status, closed -> archived).when(AllSettled)
condition AllSettled "請求がすべて片付いている"
holds(AllSettled, Case.status == closed).none(Claim.status == open)
"#;

/// A case closes once none of its claims is open, and a claim is paid once its case is
/// flagged (#112): in the case's cone the claims read the case's flag and existence, which
/// only rise, so they stay a sub-lifecycle with those two vars as its inputs.
const FLAGGED_CASE: &str = r#"
entity Case "事案" { id: Int @pk  status: Enum(open, closed) @default(open)  flag: Bool @default(false) }
entity Claim "請求" { id: Int @pk  status: Enum(open, paid) @default(open) }
relate(Claim, Case, N:1)
usecase OpenCase "受付"
usecase File "請求"
usecase Flag "注記"
usecase Pay "支払"
usecase Close "終了"
creates(OpenCase, Case)
creates(File, Claim)
sets(Flag, Case.flag == true)
transitions(Pay, Claim.status, open -> paid).when(Flagged)
condition Flagged "事案に注記がある"
holds(Flagged, Case.flag == true)
transitions(Close, Case.status, open -> closed).when(NonePending)
condition NonePending "未処理の請求がない"
holds(NonePending, Case.status == open).none(Claim.status == open)
"#;

/// The same model with the flag cleared by a use case, so it no longer only rises.
const UNFLAGGED_CASE: &str = r#"
usecase Unflag "注記解除"
sets(Unflag, Case.flag == false)
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
            (act, params.into(), run.states[e.to as usize].to_owned())
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

/// The reduced cones of a model decide what its cones decide, and their traces lift, with S
/// run plainly and up to `H` (#212 phase 2). Returns the cones the plain quotient reduces.
fn assert_reduction_exact(sample: &str, model: &SemanticModel, bounds: Bounds) -> usize {
    let n = assert_reduction_exact_in(sample, model, bounds.clone(), Symmetry::Never);
    assert_reduction_exact_in(sample, model, bounds, Symmetry::Always);
    n
}

/// `assert_reduction_exact` with S run as `s_up_to_h` says.
fn assert_reduction_exact_in(
    sample: &str,
    model: &SemanticModel,
    bounds: Bounds,
    s_up_to_h: Symmetry,
) -> usize {
    let caps = Caps::new(usize::MAX, 200_000);
    let reduce_caps = caps.with_s_up_to_h(s_up_to_h);
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let targets = ts.targets();
    let plain = ts.explore_targets(&targets, caps);
    let reduced = ts.explore_targets_reduced(&targets, reduce_caps);
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

fn reductions_in(samples: &[&str], s_up_to_h: Symmetry) -> usize {
    let mut reductions = 0;
    for sample in samples {
        let model = load(sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            reductions += assert_reduction_exact_in(sample, &model, bounds, s_up_to_h);
        }
    }
    reductions
}

/// Most bundled sample cones quotient to themselves and stay unreduced (#101); two reduce.
#[test]
fn reduced_cones_decide_what_the_cones_decide() {
    let reductions = reductions_in(SAMPLES, Symmetry::Never);
    assert!(reductions >= 2, "{reductions} cones reduced");
}

/// With S run up to `H` (#212 phase 2), the reduced cones reach the same states on T and
/// `obs`, decide the same, and their traces lift to real paths; the `Classes::of` twin holds
/// each quotient to the plain one.
#[test]
fn reduced_cones_up_to_h_decide_what_the_cones_decide() {
    let reductions = reductions_in(SAMPLES, Symmetry::Always);
    assert!(reductions >= 2, "{reductions} cones reduced");
}

/// The domain samples explore millions of states; CI runs this in release in formal-verify.
#[test]
fn reduced_domain_cones_decide_what_the_cones_decide() {
    let reductions = reductions_in(DOMAIN_SAMPLES, Symmetry::Never);
    assert!(reductions >= 53, "{reductions} cones reduced");
}

/// Every target and property cone of the samples, reduced through one shared quotient cache,
/// is the system a fresh reduction gives, its reduction's tables included; returns the cache's
/// hits.
fn shared_quotients_reduce_as_fresh_ones(models: &[(&str, SemanticModel)]) -> usize {
    let cache = QuotientCache::new();
    for (sample, model) in models {
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
            let pool = MemoryPool::for_command(MemorySize::VERIFY, "--memory");
            let capped = Caps::new(usize::MAX, 200_000);
            for caps in [capped, capped.charged_to(pool)] {
                let cones = ts.targets().into_iter().map(|t| {
                    let seed = ts.target_seed(t);
                    (ts.target_cone(t), seed)
                });
                let properties =
                    (0..ts.properties().len()).map(|i| (ts.property_cone(i), ts.property_seed(i)));
                for (cone, seed) in cones.chain(properties) {
                    let shared = ts.reduced_cone_in(&cone, [seed.clone()], caps, &cache);
                    let fresh = ts.reduced_cone(&cone, [seed], caps);
                    assert!(shared == fresh, "{sample}: a cone of {:?}", cone.vars);
                }
            }
        }
    }
    cache.hits()
}

#[test]
fn reduced_cones_through_a_shared_cache_are_fresh_ones() {
    let mut models: Vec<(&str, SemanticModel)> = SAMPLES.iter().map(|s| (*s, load(s))).collect();
    let counted = COUNTED_GATE.replace("{EDGE}", ".counted(1)");
    for (name, src) in [
        ("GATE", GATE),
        ("CLAIM_CASE", CLAIM_CASE),
        ("ORDER_SHIP", ORDER_SHIP),
        ("CASE_FOLD", CASE_FOLD),
        ("COUNTED_GATE", &counted),
    ] {
        models.push((name, model_from(src)));
    }
    let hits = shared_quotients_reduce_as_fresh_ones(&models);
    assert!(hits > 0, "some cones share a quotient");
}

/// CI runs this in release beside `reduced_domain_cones_decide_what_the_cones_decide`, whose
/// name it extends.
#[test]
fn reduced_domain_cones_decide_what_the_cones_decide_through_a_shared_cache() {
    let models: Vec<(&str, SemanticModel)> = DOMAIN_SAMPLES.iter().map(|s| (*s, load(s))).collect();
    let hits = shared_quotients_reduce_as_fresh_ones(&models);
    assert!(hits > 0, "some cones share a quotient");
}

#[test]
fn a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace() {
    let model = model_from(GATE);
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let job = entity_target(&ts, "Job");
    let rule = Target::Invariant(0);
    let caps = Caps::for_instances(1);
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

/// S up to `H` on a sub-lifecycle `H` permutes (#212 phase 2): at two instances the key's
/// sub-lifecycle, nested in the gate's, holds two cards that nothing the quotient reads names,
/// so `H` swaps them (the gate's own S holds a nested reduction and runs plain), and it
/// stores fewer states than its plain run while its quotient has the same classes. The
/// reduced cone decides the same, and every trace of it, the violation's included, lifts
/// through both reductions and the permutations to a real path of the unreduced cone.
#[test]
fn a_sub_lifecycle_up_to_h_stores_fewer_states_and_lifts_its_traces() {
    let model = model_from(GATE);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let job = entity_target(&ts, "Job");
    let rule = Target::Invariant(0);
    let caps = Caps::new(usize::MAX, 200_000);
    let plain = ts.explore_targets_reduced(&[job, rule], caps);
    let up = ts.explore_targets_reduced(&[job, rule], caps.with_s_up_to_h(Symmetry::Always));
    let (p, u) = (plain.cone_of(rule), up.cone_of(rule));
    let (pr, ur) = (
        p.run
            .system
            .reduction()
            .expect("the plain quotient reduces the gate"),
        u.run
            .system
            .reduction()
            .expect("the quotient up to H reduces the gate"),
    );
    let (pi, ui) = (
        pr.inner().expect("the gate's S nests the key's reduction"),
        ur.inner().expect("the gate's S nests the key's reduction"),
    );
    assert!(
        ui.states < pi.states,
        "H moves the cards: {} states up to H, {} plain",
        ui.states,
        pi.states
    );
    assert_eq!(ui.classes(), pi.classes(), "the same inner classes");
    assert_eq!(ur.classes(), pr.classes(), "the same classes");
    assert_eq!(
        u.run.states.len(),
        p.run.states.len(),
        "the same reduced cone"
    );
    let (a, b) = (violated(&plain), violated(&up));
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].name(&[]), b[0].name(&[]));
    let unreduced = ts.restrict_to(&u.cone.vars, &u.cone.keep);
    for at in 0..u.run.states.len() as u32 {
        let (start, steps) = lifted_path(&u.run, at);
        assert_real_path(&unreduced, &start, &steps);
    }
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
            let ts = TransitionSystem::build(&model, bounds.clone(), &Scope::ALL).ts;
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
    let inner = red.inner().expect("the gate's cone reduces the key");
    let key_vars: Vec<String> = inner
        .vars()
        .iter()
        .map(|v| sys.var_name(red.sub_vars()[v.index()]))
        .collect();
    assert_eq!(
        key_vars,
        ["Card._exists", "status", "Key._exists", "status"],
        "the gate's sub-lifecycle is the key and its card"
    );
    assert!(
        inner.classes() < inner.states,
        "the key's cone folds the card"
    );
    assert!(
        red.classes() < red.states,
        "{} classes of {} states",
        red.classes(),
        red.states
    );
}

/// A cone whose independent sub-lifecycle quotients to itself is not reduced: it keeps its
/// checks, and its run is the cone's, states and verdicts alike.
#[test]
fn an_identity_quotient_leaves_the_cone_unreduced() {
    let model = model_from(IDENTITY_QUOTIENT);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let (sys, seeds) = ts.cone_system(&ts.property_cone(0), [ts.property_seed(0)]);
    let s = sys.split_with(&seeds, true).s;
    assert_eq!(
        s.len(),
        7,
        "the property's cone has an independent sub-lifecycle"
    );
    assert!(
        sys.reduced(&seeds, Caps::VERIFY).reduction().is_none(),
        "the property's cone is not reduced"
    );
    let e1 = entity_target(&ts, "E1");
    let plain = ts.explore_targets(&[e1], Caps::VERIFY);
    let reduced = ts.explore_targets_reduced(&[e1], Caps::VERIFY);
    let cone = &reduced.cone_of(e1).run;
    assert!(
        cone.system.reduction().is_none(),
        "E1's cone is not reduced"
    );
    assert_eq!(cone.states.len(), 5176);
    assert_eq!(cone.states, plain.cone_of(e1).run.states);
    assert_eq!(diag_names(&plain), diag_names(&reduced));
}

/// An S action that reads T vars which only rise stays in the sub-lifecycle, with those
/// reads as its inputs, here folded into a bit each (#112); one that reads a var some action
/// lowers joins T, and with it the entity it writes. Without inputs every reader of T joins T.
#[test]
fn a_sub_lifecycle_keeps_a_reader_of_monotone_vars_and_loses_a_reader_of_cyclic_ones() {
    let split_of = |src: &str, inputs: bool| {
        let model = model_from(src);
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let case = entity_target(&ts, "Case");
        let (sys, seeds) = ts.cone_system(&ts.target_cone(case), [ts.target_seed(case)]);
        let split = sys.split_with(&seeds, inputs);
        let names =
            |vars: &VarSet| -> Vec<String> { vars.iter().map(|v| sys.var_name(*v)).collect() };
        let actions: Vec<String> = split
            .s_actions
            .iter()
            .map(|a| sys.actions()[a.index()].name.clone())
            .collect();
        let folded: Vec<String> = split
            .folds
            .iter()
            .map(|f| sys.actions()[f.act().index()].name.clone())
            .collect();
        (names(&split.s), names(&split.inputs), actions, folded)
    };
    let (s, inputs, actions, folded) = split_of(FLAGGED_CASE, true);
    assert_eq!(
        s,
        ["Claim._exists", "status", "Claim._owner_Case"],
        "the claims are the sub-lifecycle"
    );
    assert_eq!(
        actions,
        ["Pay", "File"],
        "the claims' steps read the case's flag and existence"
    );
    assert_eq!(folded, ["Pay", "File"], "each read folds into a bit");
    assert!(inputs.is_empty(), "no raw input is left: {inputs:?}");
    let (s, inputs, _, folded) = split_of(&format!("{FLAGGED_CASE}{UNFLAGGED_CASE}"), true);
    assert!(
        s.is_empty(),
        "a cleared flag is no input, so Pay joins T: {s:?}"
    );
    assert!(
        inputs.is_empty() && folded.is_empty(),
        "{inputs:?} {folded:?}"
    );
    let (s, _, _, _) = split_of(FLAGGED_CASE, false);
    assert!(s.is_empty(), "without inputs File and Pay join T: {s:?}");
}

/// A sub-lifecycle that reads inputs of T (#112) is still reduced, and the reduced cone
/// decides what the cone decides: the deadlocks that exist only after an input rose or only
/// in one interleaving of the rise with the sub-lifecycle, and the forbidden state.
#[test]
fn a_sub_lifecycle_that_reads_inputs_of_t_is_reduced_and_decides_the_same() {
    for (name, src, anchor, expected) in [
        (
            "claim case",
            CLAIM_CASE,
            "Claim",
            vec!["Deadlock_Claim_status_open", "Deadlock_Claim_status_paid"],
        ),
        ("order ship", ORDER_SHIP, "Order", vec![]),
    ] {
        let model = model_from(src);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let reductions = assert_reduction_exact(name, &model, bounds.clone());
            assert!(
                reductions >= 1,
                "{name} at {bounds:?}: {reductions} cones reduced"
            );
            let ts = TransitionSystem::build(&model, bounds.clone(), &Scope::ALL).ts;
            let target = entity_target(&ts, anchor);
            let targets = ts.targets();
            let plain = ts.explore_targets(&targets, Caps::VERIFY);
            let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
            let cone = reduced.cone_of(target);
            // At one instance the order's sub-lifecycle has a class per state (#101).
            let Some(red) = cone.run.system.reduction() else {
                assert!(
                    bounds != Bounds::VERIFY,
                    "{name} at {bounds:?}: the {anchor} view reduces"
                );
                continue;
            };
            if name == "claim case" {
                assert_eq!(
                    red.bits().len(),
                    1,
                    "{name}: one bit for CloseCase's variants"
                );
                assert_eq!(red.bits()[0].len(), 2, "{name}: both variants share it");
                assert!(
                    red.inputs().len() >= 2,
                    "{name}: the claims' status and existence are raw inputs"
                );
            } else {
                assert_eq!(
                    red.bits().len(),
                    2,
                    "{name}: one bit for Ship, keyed by the order, and one for Prepare"
                );
                assert!(
                    red.inputs().is_empty(),
                    "{name}: no raw input: {:?}",
                    red.inputs()
                );
            }
            let (a, b) = (diag_names(&plain), diag_names(&reduced));
            assert_eq!(a, b, "{name} at {bounds:?}: verdicts");
            // A claim stuck in a case another claim closed needs two claims.
            let expected: Vec<&str> = match bounds == Bounds::VERIFY {
                true => expected.clone(),
                false => expected
                    .iter()
                    .copied()
                    .filter(|e| !e.ends_with("_open"))
                    .collect(),
            };
            for e in &expected {
                assert!(a.contains(*e), "{name} at {bounds:?}: {e} in {a:?}");
            }
            assert_eq!(
                violated(&plain).len(),
                1,
                "{name} at {bounds:?}: the forbidden state"
            );
            assert_eq!(
                violated(&reduced).len(),
                1,
                "{name} at {bounds:?}: the forbidden state"
            );
            assert!(
                cone.run.states.len() < plain.cone_of(target).run.states.len(),
                "{name} at {bounds:?}: the reduced cone is smaller"
            );
        }
    }
}

/// A reduced cone counts the cells its TLA+ tables print (#112): one per class for each
/// observed var and each S action, and one per rise. The case's view of `CASE_FOLD` prints
/// 3,696 cells (336 classes, 1,344 rises), under the budget a cone may hand TLC.
#[test]
fn a_reduced_cone_counts_the_cells_its_tla_tables_print() {
    let model = model_from(CASE_FOLD);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let case = entity_target(&ts, "Case");
    let (sys, seeds) = ts.cone_system(&ts.target_cone(case), [ts.target_seed(case)]);
    let reduced = sys.reduced(&seeds, Caps::VERIFY);
    let red = reduced
        .reduction()
        .expect("the claims reduce under the case's bit");
    let rises: usize = (0..red.classes() as u32)
        .map(|c| red.rises_from(c).len())
        .sum();
    assert_eq!(
        red.tla_cells(),
        red.classes() * (red.obs().len() + red.s_actions().len()) + rises
    );
    assert_eq!(
        red.tla_cells(),
        3696,
        "{} classes, {rises} rises",
        red.classes()
    );
}

/// A cone whose tables print more than the old budget of 200,000 cells and no more than
/// `TLA_CELL_BUDGET` keeps its reduction with inputs (#284). The model is reduction-diff seed
/// 28 (`scripts/reduction-diff.py gen 28`): the `Forbidden_E4_0` cone at 2 instances reduces
/// its 74,064 sub-lifecycle states to 23,376 classes that print 439,968 cells, where the old
/// budget left the cone to the retry without inputs, which reduces nothing.
#[test]
fn a_cone_between_the_old_and_the_new_cell_budget_is_reduced() {
    let model = model_from(include_str!("wide_quotient.rdra"));
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let at = ts
        .invariants()
        .iter()
        .position(|inv| inv.name == "Forbidden_E4_0")
        .expect("the forbidden state of E4");
    let target = Target::Invariant(at);
    let (sys, seeds) = ts.cone_system(&ts.target_cone(target), [ts.target_seed(target)]);
    let reduced = sys.reduced(&seeds, Caps::VERIFY);
    let red = reduced.reduction().expect("the cone is reduced");
    assert!(
        !red.inputs().is_empty() || !red.bits().is_empty(),
        "with inputs"
    );
    assert_eq!(red.classes(), 23_376);
    assert_eq!(red.tla_cells(), 439_968);
    assert!(
        200_000 < red.tla_cells() && red.tla_cells() <= crate::lifecycle::reduce::TLA_CELL_BUDGET
    );
}

/// A quantifier over T with no T param folds into a bit keyed by the linked row (#112), the
/// sub-lifecycle has no raw input left, and the reduced cone decides what the cone decides:
/// the deadlock at `closed` exists only because the bit fell when the case closed.
#[test]
fn a_quantifier_over_t_folds_into_a_bit_keyed_by_the_linked_row() {
    let model = model_from(CASE_FOLD);
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        assert_reduction_exact("case fold", &model, bounds);
    }
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let target = entity_target(&ts, "Case");
    let targets = ts.targets();
    let plain = ts.explore_targets(&targets, Caps::VERIFY);
    let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
    let cone = reduced.cone_of(target);
    let red = cone.run.system.reduction().expect("the Case view reduces");
    assert!(red.inputs().is_empty(), "no raw input: {:?}", red.inputs());
    let linked: Vec<&Fold> = red
        .bits()
        .iter()
        .flatten()
        .filter(|f| matches!(f.key(), FoldKey::Linked { .. }))
        .collect();
    assert_eq!(linked.len(), 1, "Pay's node is the one linked fold");
    assert_eq!(
        cone.run.system.actions()[linked[0].act().index()].name,
        "Pay"
    );
    let (a, b) = (diag_names(&plain), diag_names(&reduced));
    assert_eq!(a, b, "verdicts");
    for e in ["Deadlock_Case_status_open", "Deadlock_Case_status_closed"] {
        assert!(a.contains(e), "{e} in {a:?}");
    }
    assert!(
        cone.run.states.len() < plain.cone_of(target).run.states.len(),
        "the reduced cone is smaller"
    );
}

/// A count over T in a guard is read raw: no node pulls it out as it pulls an existential,
/// so the reduced cones decide what the cones decide, whichever way the count points
/// (docs/design/counting-rules.md). The case fold's `.none` becomes each count in turn.
#[test]
fn a_count_over_t_in_a_guard_reduces_exactly() {
    for count in [
        ".at_most(1, Claim.status == open)",
        ".at_least(2, Claim.status == paid)",
        ".exactly(1, Claim)",
    ] {
        let src = CASE_FOLD.replace(".none(Claim.status == open)", count);
        assert_ne!(src, CASE_FOLD);
        let model = model_from(&src);
        let mut reductions = 0;
        for bounds in [
            Bounds::STATES,
            Bounds::VERIFY,
            Bounds::VERIFY.at(std::num::NonZeroU8::new(3).unwrap()),
        ] {
            reductions += assert_reduction_exact(count, &model, bounds);
        }
        assert!(reductions > 0, "{count}: some cone reduces");
    }
}

/// A steady action (`Reduction::is_steady`) writes nothing the inputs of the sub-lifecycle
/// read, so its steps skip the class lookup in BFS and print `UNCHANGED` for the class in
/// TLA+. On every reduced cone of the case fold and of insurance-claim-ops, each step of a
/// steady action from each reached state leaves the input bytes as they were. The cones
/// have steps of both kinds: steady ones, and ones that change an input.
#[test]
fn a_steady_action_never_changes_the_inputs() {
    let (mut steady, mut moving) = (0, 0);
    for model in [model_from(CASE_FOLD), load("samples/insurance-claim-ops")] {
        let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
        let targets = ts.targets();
        let reduced = ts.explore_targets_reduced(&targets, Caps::for_instances(1));
        for cone in &reduced.cones {
            let sys = &cone.run.system;
            let Some(red) = sys.reduction() else { continue };
            for s in &cone.run.states {
                let before = red.input_bytes(sys, s);
                for (act, _, next) in sys.plain_moves(s) {
                    if red.skips(act) {
                        continue;
                    }
                    let changed = red.input_bytes(sys, &next) != before;
                    if red.is_steady(act) {
                        steady += 1;
                        let name = &sys.actions()[act.index()].name;
                        assert!(!changed, "{name} changed the inputs");
                    } else {
                        moving += usize::from(changed);
                    }
                }
            }
        }
    }
    assert!(
        steady > 0 && moving > 0,
        "{steady} steady steps, {moving} that change an input"
    );
}

/// A class move's params are the extended sub-lifecycle's, which drops a folded action's T
/// params. In clinic-ops, `ReviewPatientResponse` updates a `CarePlan` (S) and its
/// `FollowUpMessage` (T) on the cone of `Unique_FollowUpMessage_CarePlan`: the BFS takes the
/// cone's step into the same class, so it binds both rows, decides what the plain cone
/// decides, and its traces lift to real paths.
#[test]
fn a_class_move_of_a_folded_action_binds_the_cones_params() {
    let model = load("samples/clinic-ops");
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let target = ts
        .targets()
        .into_iter()
        .find(|t| {
            matches!(t, Target::Invariant(i) if ts.invariants()[*i].name == "Unique_FollowUpMessage_CarePlan")
        })
        .expect("the 1:1 key's invariant");
    let caps = Caps::for_instances(1);
    let plain = ts.explore_targets(&[target], caps);
    let reduced = ts.explore_targets_reduced(&[target], caps);
    let cone = reduced.cone_of(target);
    assert!(cone.run.system.reduction().is_some(), "the cone reduces");
    assert_eq!(diag_names(&plain), diag_names(&reduced));
    let unreduced = ts.restrict_to(&cone.cone.vars, &cone.cone.keep);
    for at in 0..cone.run.states.len() as u32 {
        let (start, steps) = lifted_path(&cone.run, at);
        assert_real_path(&unreduced, &start, &steps);
    }
}

/// A job starts only at an open gate. Crack raises `EvCracked` and Swing, which the event
/// triggers, opens the gate; `{EDGE}` is `.counted(1)` or nothing (the latch). The job's view
/// observes the gate's status only, so under the latch the gate is the job cone's independent
/// sub-lifecycle and its mark is an unobserved var of it.
const COUNTED_GATE: &str = r#"
entity Gate "門" { id: Int @pk  status: Enum(shut, half, open) @default(shut) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Build "設置"
usecase Crack "少し開ける"
usecase Swing "開く"
usecase Submit "投入"
usecase Start "開始"
event EvCracked "少し開いた"
creates(Build, Gate)
transitions(Crack, Gate.status, shut -> half)
raises(Crack, EvCracked)
triggers(EvCracked, Swing){EDGE}
transitions(Swing, Gate.status, half -> open)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// A counted mark is never reduced across (docs/design/counted-events.md, "The reduction"):
/// the split keeps the counter, its raiser, and its handler in T, names it in no input and
/// no fold, and the reduced cones decide what the cones decide. The latch copy of the same
/// model puts the gate in S, so it is the counted rule, not the model, that keeps it in T.
#[test]
fn a_counted_mark_keeps_its_raiser_and_handler_out_of_the_sub_lifecycle() {
    let split_of = |edge: &str| {
        let model = model_from(&COUNTED_GATE.replace("{EDGE}", edge));
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let job = entity_target(&ts, "Job");
        let (sys, seeds) = ts.cone_system(&ts.target_cone(job), [ts.target_seed(job)]);
        let split = sys.split_with(&seeds, true);
        let names =
            |vars: &VarSet| -> Vec<String> { vars.iter().map(|v| sys.var_name(*v)).collect() };
        let act = |a: crate::lifecycle::ActId| sys.actions()[a.index()].name.clone();
        (
            names(&split.s),
            names(&split.inputs),
            split.s_actions.iter().map(|a| act(*a)).collect::<Vec<_>>(),
            split.folds.iter().map(|f| act(f.act())).collect::<Vec<_>>(),
        )
    };
    let (s, _, actions, _) = split_of("");
    assert!(
        s.contains(&"Gate._raised_EvCracked".to_string()),
        "under the latch the gate is the sub-lifecycle: {s:?}"
    );
    assert!(
        actions.contains(&"Crack".to_string()) && actions.contains(&"Swing".to_string()),
        "under the latch Crack and Swing are S: {actions:?}"
    );
    let (s, inputs, actions, folded) = split_of(".counted(1)");
    assert!(
        !s.iter().any(|v| v.contains("_pending_")),
        "the counter is never in S: {s:?}"
    );
    for uc in ["Crack", "Swing"] {
        assert!(
            !actions.iter().any(|a| a == uc),
            "{uc} writes the counter, so it is T: {actions:?}"
        );
        assert!(!folded.iter().any(|a| a == uc), "{uc} folds: {folded:?}");
    }
    assert!(
        !inputs.iter().any(|v| v.contains("_pending_")),
        "the counter is no input: {inputs:?}"
    );
    let model = model_from(&COUNTED_GATE.replace("{EDGE}", ".counted(1)"));
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        assert_reduction_exact("counted gate", &model, bounds);
    }
}

/// A counted mark is not monotone even in a system that holds its raiser and not its
/// handler, where its arms alone (`0 -> 1 -> over`) form no cycle; the latch there is.
#[test]
fn a_counted_mark_is_not_monotone_without_its_handler() {
    for (edge, monotone) in [("", true), (".counted(1)", false)] {
        let model = model_from(&COUNTED_GATE.replace("{EDGE}", edge));
        let mut ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        ts.actions.retain(|a| a.name != "Swing");
        let mark = ts
            .var_ids()
            .find(|v| matches!(ts.var(*v).kind, crate::lifecycle::VarKind::Raised { .. }))
            .expect("the gate's mark");
        assert_eq!(
            ts.monotone_vars()[mark.index()],
            monotone,
            "{edge:?}: {} monotone",
            ts.var_name(mark)
        );
    }
}
