//! A cone reduced by its independent sub-lifecycle decides what the cone decides: the same
//! states on the target side and what it reads of the sub-lifecycle, the same views and
//! verdicts, and traces that lift to real paths of the cone.

use std::collections::BTreeSet;

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::reduce::ParamCell;
use crate::memory::{MemoryPool, MemorySize};

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{
    ActId, Action, ActionOrigin, ComponentRun, Cone, EntId, Fold, FoldKey, QuotientCache,
    Reduction, ReplayError, Seed, State, Step, Target, TargetExploration, Value, VarSet,
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
pub(super) const GATE: &str = r#"
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

/// Order is created, paid (raising `EvPaid`) and shipped once a voucher is used; a voucher is
/// used once some ticket is issued; `Issue`, EvPaid's handler, issues a ticket. In the order's
/// cone S is Voucher and Ticket, and `Issue` folds keyed by its ticket with a `Triggered` Order
/// param beside the key. Voucher's owner keeps Order in the extended sub-lifecycle as a stub,
/// absent forever since `Place` creates it. The kept `Triggered` param binds a row of the stub
/// (claims A8, A11 of docs/theory/claims.md), so `Issue` runs there, a voucher is used, and the
/// reduced cone breaks the rule the cone breaks in 7 steps.
const TRIGGERED_ON_A_STUB: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(new, paid, shipped) @default(new) }
entity Voucher "券" { id: Int @pk  status: Enum(open, used) @default(open) }
entity Ticket "札" { id: Int @pk  status: Enum(draft, issued) @default(draft) }
relate(Voucher, Order, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Grant "発券"
usecase Draft "起票"
usecase Issue "発行"
usecase Use "使用"
event EvPaid "支払済"
creates(Place, Order)
transitions(Pay, Order.status, new -> paid)
raises(Pay, EvPaid)
triggers(EvPaid, Issue)
transitions(Ship, Order.status, paid -> shipped)
condition HasUsedVoucher "使用済の券"
holds(HasUsedVoucher, Voucher.status == used)
applies_to(HasUsedVoucher, Ship)
creates(Grant, Voucher)
creates(Draft, Ticket)
transitions(Issue, Ticket.status, draft -> issued)
transitions(Use, Voucher.status, open -> used)
condition TicketIssued "発行済の札"
holds(TicketIssued, Ticket.status == issued)
applies_to(TicketIssued, Use)
forbidden(Order, status == shipped)
"#;

/// `Note`, a use case of the order's cone T (it reads `Order.hold`, which goes both ways),
/// creates a sheet under an order; a voucher is used once some sheet and some ticket exist.
/// In the order's cone S is Voucher and Ticket, and Sheet's existence is an input that `Note`
/// raises. Voucher's owner keeps Order in the extended sub-lifecycle as a stub, so `Note`'s
/// rise there keeps its `Parent` Order param, which binds a row of the stub (claims A8, A11),
/// so the reduction finds the rise for the cone's `Note` step.
const RISE_ON_A_STUB: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(new, paid, shipped) @default(new)  hold: Bool @default(false) }
entity Voucher "券" { id: Int @pk  status: Enum(open, used) @default(open) }
entity Sheet "票" { id: Int @pk  status: Enum(made) @default(made) }
entity Ticket "札" { id: Int @pk  status: Enum(draft, issued) @default(draft) }
relate(Voucher, Order, N:1)
relate(Sheet, Order, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Grant "発券"
usecase Use "使用"
usecase Hold "保留"
usecase Release "解除"
usecase Note "記票"
usecase Draft "起票"
usecase Issue "発行"
creates(Place, Order)
transitions(Pay, Order.status, new -> paid)
transitions(Ship, Order.status, paid -> shipped)
sets(Hold, Order.hold == true)
sets(Release, Order.hold == false)
condition Held "保留中"
holds(Held, Order.hold == true)
creates(Note, Sheet)
applies_to(Held, Note)
condition HasUsedVoucher "使用済の券"
holds(HasUsedVoucher, Voucher.status == used)
applies_to(HasUsedVoucher, Ship)
creates(Grant, Voucher)
transitions(Use, Voucher.status, open -> used)
condition SheetMade "票がある"
holds(SheetMade, Sheet.status == made)
applies_to(SheetMade, Use)
creates(Draft, Ticket)
transitions(Issue, Ticket.status, draft -> issued)
condition TicketIssued "発行済の札"
holds(TicketIssued, Ticket.status == issued)
applies_to(TicketIssued, Use)
forbidden(Order, status == shipped)
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
            (
                act,
                params.into(),
                run.states.at(e.to as usize).into_state(),
            )
        })
        .collect();
    (run.system)
        .lifted(&run.states.at(first as usize), &steps)
        .expect("a reduced path lifts")
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
    let (n, _) = assert_reduction_exact_in(sample, model, bounds.clone(), false);
    assert_reduction_exact_in(sample, model, bounds, true);
    n
}

/// `assert_reduction_exact`, with S forced up to `H` at once when `up_to_h` says so. Without
/// it, each complete reduced safety cone also runs up to `G_T` (`assert_up_to_g_t`). Returns
/// the cones the quotient reduces, and the safety cones among them whose rows `G_T` moved.
fn assert_reduction_exact_in(
    sample: &str,
    model: &SemanticModel,
    bounds: Bounds,
    up_to_h: bool,
) -> (usize, usize) {
    let caps = Caps::new(usize::MAX, 200_000);
    let reduce_caps = match up_to_h {
        true => caps.forcing_s_up_to_h(),
        false => caps,
    };
    let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
    let targets = ts.targets();
    let plain = ts.explore_targets(&targets, caps);
    let reduced = ts.explore_targets_reduced(&targets, reduce_caps);
    let plain_views = plain.entity_views(&ts, model, &[]);
    let reduced_views = reduced.entity_views(&ts, model, &[]);
    let mut reductions = 0;
    let mut moved = 0;
    for target in &targets {
        let (p, r) = (plain.cone_of(*target), reduced.cone_of(*target));
        let safety = matches!(target, Target::Invariant(_) | Target::AfterAssert(_));
        if safety && !up_to_h && r.run.system.reduction().is_some() {
            moved += usize::from(assert_up_to_g_t(sample, *target, &r.run.system));
        }
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
                .map(|s| sys.project(&s, &readable))
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
                sys.project(&r.run.states.at(at as usize), &readable),
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
    (reductions, moved)
}

/// `system`, a reduced safety cone, run plainly and up to `G_T` (#212 phase 3), when its
/// plain run stays under 200,000 states. The group is a bisimulation: the canonical forms of
/// the successors of every plain state are those of the successors of its canonical form,
/// which a group that moves a row the reduction keys on fails. The stored states are the
/// canonical forms of the plain run's, the two runs report the same violated checks, each
/// with a trace the run lifted through the symmetry and then through #74 and replayed, and
/// every S entity, and so the class representative, stays where it is. Returns whether
/// `G_T` stored fewer states than the plain run.
fn assert_up_to_g_t(sample: &str, target: Target, system: &TransitionSystem) -> bool {
    let run = system.clone().run(Caps::new(usize::MAX, 200_000));
    if !run.complete {
        return false;
    }
    let orbits = system.clone().run_orbits(Caps::new(usize::MAX, usize::MAX));
    assert!(orbits.complete(), "{sample} {target:?}: up to G_T");
    let names = |diags: &[crate::lifecycle::diag::Diag]| -> BTreeSet<String> {
        diags.iter().map(|d| d.diag.name(&[])).collect()
    };
    assert_eq!(
        names(&run.diags),
        names(orbits.diags()),
        "{sample} {target:?}: violated checks up to G_T"
    );
    let successors = |s: &StateSlice| -> BTreeSet<State> {
        system.moves(s).map(|m| orbits.canonical(&m.next)).collect()
    };
    for s in run.states.iter() {
        let form = orbits.canonical(&s);
        assert!(
            successors(&s) == successors(&form),
            "{sample} {target:?}: G_T is no bisimulation at {:?}",
            &s.0
        );
    }
    let stored: BTreeSet<State> = orbits.states().iter().map(|s| s.into_state()).collect();
    let forms: BTreeSet<State> = run.states.iter().map(|s| orbits.canonical(&s)).collect();
    assert!(
        stored == forms,
        "{sample} {target:?}: {} plain states have {} forms, {} stored",
        run.states.len(),
        forms.len(),
        stored.len()
    );
    let r = system.reduction().expect("a reduced cone");
    for e in (0..system.entities().len()).map(EntId::at) {
        assert!(
            !r.is_s_entity(e) || !orbits.moves(e),
            "{sample} {target:?}: G_T moves the S rows of {}",
            system.entity(e).id
        );
    }
    orbits.orbits() < run.states.len()
}

/// The cones of `samples` the quotient reduces, and the safety cones among them whose rows
/// `G_T` moved (`assert_reduction_exact_in`).
fn reductions_in(samples: &[&str], up_to_h: bool) -> (usize, usize) {
    let (mut reductions, mut moved) = (0, 0);
    for sample in samples {
        let model = load(sample);
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let (r, m) = assert_reduction_exact_in(sample, &model, bounds, up_to_h);
            reductions += r;
            moved += m;
        }
    }
    (reductions, moved)
}

/// Most bundled sample cones quotient to themselves and stay unreduced (#101); two reduce.
/// Each reduced safety cone also runs up to `G_T` (#212 phase 3).
#[test]
fn reduced_cones_decide_what_the_cones_decide() {
    let (reductions, _) = reductions_in(SAMPLES, false);
    assert!(reductions >= 2, "{reductions} cones reduced");
}

/// With S run up to `H` (#212 phase 2), the reduced cones reach the same states on T and
/// `obs`, decide the same, and their traces lift to real paths; the `Classes::of` twin holds
/// each quotient to the plain one.
#[test]
fn reduced_cones_up_to_h_decide_what_the_cones_decide() {
    let (reductions, _) = reductions_in(SAMPLES, true);
    assert!(reductions >= 2, "{reductions} cones reduced");
}

/// The domain samples explore millions of states; CI runs this in release in formal-verify.
#[test]
fn reduced_domain_cones_decide_what_the_cones_decide() {
    let (reductions, moved) = reductions_in(DOMAIN_SAMPLES, false);
    assert!(reductions >= 53, "{reductions} cones reduced");
    assert!(moved >= 1, "{moved} reduced safety cones G_T moves rows of");
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
    let up = ts.explore_targets_reduced(&[job, rule], caps.forcing_s_up_to_h());
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

/// A reduced cone up to `G_T` (#212 phase 3): the job's cone reduces the gate, and at two
/// instances `G_T` swaps the jobs, which the reduction does not key on, while it leaves the
/// gate, the key and the card where they are. It stores fewer states than the plain reduced
/// run, finds the same violation, and lifts its trace through the swap and then through the
/// class moves, so the printed trace spells the gate's and the key's steps and ends in the
/// job's start, as the plain reduced run's does. The path to every stored state lifts, through
/// the permutations and then through both reductions, to a real path of the unreduced cone.
#[test]
fn a_reduced_cone_up_to_g_t_stores_fewer_states_and_lifts_its_trace() {
    let model = model_from(GATE);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let rule = Target::Invariant(0);
    let caps = Caps::new(usize::MAX, 200_000);
    let plain = ts.explore_targets_reduced(&[rule], caps);
    let cone = &plain.cone_of(rule).run;
    cone.system
        .reduction()
        .expect("the job's cone reduces the gate");
    // The cone's run stopped at its violation; the whole reduced cone, run plainly.
    let p = &cone.system.clone().run(caps);
    assert!(
        assert_up_to_g_t("gate", rule, &cone.system),
        "G_T stores fewer states"
    );
    let orbits = p.system.clone().run_orbits(caps);
    let job = (0..p.system.entities().len())
        .map(EntId::at)
        .find(|e| p.system.entity(*e).id == "Job")
        .expect("the job is in its cone");
    assert!(orbits.moves(job), "G_T swaps the jobs");
    let a = violated(&plain);
    let b: Vec<&StateDiag> = orbits
        .diags()
        .iter()
        .map(|d| &d.diag)
        .filter(|d| matches!(d, StateDiag::ConstraintViolated { .. }))
        .collect();
    assert_eq!((a.len(), b.len()), (1, 1));
    assert_eq!(a[0].name(&[]), b[0].name(&[]));
    let steps = |d: &StateDiag| -> Vec<String> {
        d.trace()
            .expect("a violation has a trace")
            .steps
            .iter()
            .map(|s| s.action.clone())
            .collect()
    };
    assert_eq!(steps(a[0]), steps(b[0]), "the same lifted trace");
    let lifted = steps(b[0]);
    assert!(
        lifted.contains(&"Unlatch".to_string()) && lifted.contains(&"Unlock".to_string()),
        "the lifted trace spells the gate's and the key's steps: {lifted:?}"
    );
    assert_eq!(lifted.last().map(String::as_str), Some("Start"));
    let unreduced = ts.restrict_to(
        &plain.cone_of(rule).cone.vars,
        &plain.cone_of(rule).cone.keep,
    );
    let graph = orbits.quotient_graph();
    for at in 0..graph.states.len() as u32 {
        let path = graph.path_to(at);
        let first = path.first().map_or(at, |e| e.from);
        let (start, steps) = orbits
            .lift_path(first, &path)
            .expect("a stored path replays");
        let (start, steps) = p
            .system
            .lifted(&start, &steps)
            .expect("a reduced path lifts");
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
/// (docs/design/counting-rules.md). The case fold's `.none` becomes each count in turn, at
/// the bounds `check` and `verify` use. Two instances already lets `at_most(1)` and
/// `at_least(2)` each hold and fail; a third instance repeats those.
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
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
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
                let before = red.input_bytes(sys, &s);
                for (act, _, next) in sys.plain_moves(&s) {
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

/// A gate whose Int level goes up and down, and a job that starts while it is up.
const SWINGING_LEVEL: &str = r#"
entity Gate "門" { id: Int @pk  level: Int @default(0) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase Build "設置"
usecase Raise "上げる"
usecase Lower "下げる"
usecase Submit "投入"
usecase Start "開始"
creates(Build, Gate)
updates(Raise, Gate)
sets(Raise, Gate.level == 1)
updates(Lower, Gate)
sets(Lower, Gate.level == 0)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateUp "門が上がっている"
holds(GateUp, Gate.level == 1)
applies_to(GateUp, Start)
forbidden(Job, status == running).label("作業は始まらない")
"#;

/// Claim A10: a column whose domain holds 256 values (a non-null Int at `--int-range-max 255`)
/// that a use case sets both up and down is not monotone, at 255 as at 254: `monotone_vars`
/// lists every value such a write starts from, all 256 of them.
#[test]
fn a_256_value_column_set_both_ways_is_not_monotone() {
    let model = model_from(SWINGING_LEVEL);
    for int_max in [254, 255] {
        let bounds = Bounds::uniform(std::num::NonZeroU8::MIN, int_max);
        let ts = TransitionSystem::build(&model, bounds.clone(), &Scope::ALL).ts;
        let level = ts
            .var_ids()
            .find(|v| ts.var_name(*v).contains("level"))
            .expect("the gate's level");
        assert!(
            !ts.monotone_vars()[level.index()],
            "int_max {int_max}: {} read as monotone",
            ts.var_name(level)
        );
        assert_reduction_exact("swinging level", &model, bounds);
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

/// Under a memory limit the entity pass releases a stopped entity cone once its findings are
/// read, and the cones after it run in the room it held (#284). Releasing moves findings from
/// undecided to decided and changes no decided one, and a reader of a released cone's graph
/// panics: nothing but `entity_findings` reads a stopped entity cone.
#[test]
fn a_stopped_entity_cone_leaves_its_room_to_the_cones_after_it() {
    let ts = TransitionSystem::build(&load("samples/patent-ops"), Bounds::VERIFY, &Scope::ALL).ts;
    let entities: Vec<Target> = (ts.targets().into_iter())
        .filter(|t| matches!(t, Target::Entity(_)))
        .collect();
    let explore = |release: bool| {
        let limit = MemorySize::from_bytes(1 << 20).expect("a size");
        let caps = Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
        ts.explore_targets_with(&entities, caps, Some(&QuotientCache::new()), &[], release)
    };
    let (kept, released) = (explore(false), explore(true));
    for v in kept.verdicts() {
        assert!(released.verdicts().contains(v), "decided alike: {v:?}");
    }
    for u in released.undecided() {
        assert!(
            kept.undecided().iter().any(|k| k.subject == u.subject),
            "left undecided only where the held runs left it: {u:?}"
        );
    }
    assert!(
        released.undecided().len() < kept.undecided().len(),
        "the room a stopped cone held decides more: {} undecided, {} with the runs held",
        released.undecided().len(),
        kept.undecided().len()
    );
    let stopped: Vec<&ComponentRun> = (released.cones.iter())
        .map(|c| &c.run)
        .filter(|run| !run.complete)
        .collect();
    assert!(!stopped.is_empty(), "some entity cone stops at 1MiB");
    for run in stopped {
        assert!(run.states.is_empty(), "a released run holds no state");
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.graph().complete));
        assert!(read.is_err(), "a reader of a released run's graph panics");
    }
}

/// Under `--symmetry always` an entity cone that stops at the memory limit runs again up to
/// instance symmetry in the room its plain run held, and a run that completes decides its
/// findings on the annotated quotient (#212 phase 4). It decides what the plain run left
/// open, and every check the plain runs decide keeps its verdict.
#[test]
fn an_entity_cone_up_to_symmetry_decides_what_its_plain_run_left_open() {
    let ts = TransitionSystem::build(&load("samples/clinic-ops"), Bounds::VERIFY, &Scope::ALL).ts;
    let entities: Vec<Target> = (ts.targets().into_iter())
        .filter(|t| matches!(t, Target::Entity(_)))
        .collect();
    let explore = |orbits: bool| {
        let limit = MemorySize::from_bytes(4 << 20).expect("a size");
        let caps = (Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory")))
            .with_entity_orbits(orbits);
        ts.explore_targets_reduced_in(&entities, caps, &QuotientCache::new(), &[])
    };
    let (plain, up) = (explore(false), explore(true));
    let names = |run: &TargetExploration| -> BTreeSet<String> {
        run.verdicts().iter().map(|d| d.diag.name(&[])).collect()
    };
    assert!(
        names(&plain).is_subset(&names(&up)),
        "every verdict stands: {:?} lost",
        names(&plain).difference(&names(&up)).collect::<Vec<_>>()
    );
    for u in up.undecided() {
        assert!(
            plain.undecided().iter().any(|p| p.subject == u.subject),
            "left undecided only where the plain runs left it: {u:?}"
        );
    }
    assert!(
        up.undecided().len() < plain.undecided().len(),
        "up to symmetry decides more: {} undecided, {} plain",
        up.undecided().len(),
        plain.undecided().len()
    );
}

/// The complete reduced cones of `models` under each of `bounds` whose reduction folds, each
/// with its plain cone, that cone's complete run, and the reduction; `f` sees each one.
fn each_folded_cone(
    models: &[(&str, SemanticModel)],
    bounds: &[Bounds],
    f: impl FnMut(&str, Target, &TransitionSystem, &ComponentRun, &Reduction),
) {
    each_reduced_cone(models, bounds, true, f);
}

/// `each_folded_cone`, with the cones whose reduction folds nothing too unless `folded_only`.
fn each_reduced_cone(
    models: &[(&str, SemanticModel)],
    bounds: &[Bounds],
    folded_only: bool,
    mut f: impl FnMut(&str, Target, &TransitionSystem, &ComponentRun, &Reduction),
) {
    let caps = Caps::new(usize::MAX, 200_000);
    for (name, model) in models {
        for bounds in bounds {
            let ts = TransitionSystem::build(model, bounds.clone(), &Scope::ALL).ts;
            let targets = ts.targets();
            let reduced = ts.explore_targets_reduced(&targets, caps);
            for target in &targets {
                let r = reduced.cone_of(*target);
                let Some(red) = r.run.system.reduction() else {
                    continue;
                };
                if folded_only && red.bits().iter().all(Vec::is_empty) {
                    continue;
                }
                let cone = ts.restrict_to(&r.cone.vars, &r.cone.keep);
                let run = cone.clone().run(caps);
                if run.complete {
                    f(name, *target, &cone, &run, red);
                }
            }
        }
    }
}

/// What a fold check saw: the folds, the moves or steps it checked, and those of them that
/// bind the bit's key past slot 0.
#[derive(Debug, Default)]
struct FoldsChecked {
    folds: usize,
    checked: usize,
    past_0: usize,
    /// The cone's S steps, folded or not, found among the extended sub-lifecycle's.
    s_steps: usize,
}

/// The fold half of Lemma 6 of docs/theory/cones.md (claims A8, A11), checked against the
/// cone's own step relation rather than against a second evaluation of the folded guards:
/// for every reachable state of every reduced cone of `models` whose reduction folds an S
/// action's reads of T into a bit, and every move of that action, the bit holds at the slot
/// the move binds its key to (or, for a node fold, at the row the folded quantifier is
/// satisfied by, which the link names). A bit that is false where the action runs would drop
/// that step from the extended sub-lifecycle.
fn fold_bits_hold_where_their_actions_run(
    models: &[(&str, SemanticModel)],
    bounds: &[Bounds],
) -> FoldsChecked {
    let mut seen = FoldsChecked::default();
    each_folded_cone(models, bounds, |name, target, cone, run, red| {
        cone_s_steps_are_silent_steps(name, target, cone, run, red, &mut seen);
    });
    seen
}

/// The body of `fold_bits_hold_where_their_actions_run` for one cone.
fn cone_s_steps_are_silent_steps(
    name: &str,
    target: Target,
    cone: &TransitionSystem,
    run: &ComponentRun,
    red: &Reduction,
    seen: &mut FoldsChecked,
) {
    let bits: Vec<&Fold> = red.bits().iter().flatten().collect();
    seen.folds += bits.len();
    let silent = red.silent_steps(cone);
    for s in run.states.iter() {
        let steps = silent.from(red, cone, &s);
        for m in cone.moves(&s) {
            // Every S step of the cone is a silent step of the extended sub-lifecycle to
            // the same state, by an action it stands for (claim A11): a true bit is not enough,
            // the step's other params must bind there too.
            if red.s_actions().contains(&m.act) {
                assert!(
                    steps
                        .iter()
                        .any(|x| x.next == m.next && x.acts.iter().any(|(a, _)| *a == m.act)),
                    "{name} {target:?}: the cone steps by {} {:?} where the extended \
                     sub-lifecycle has no such step, in {:?}",
                    cone.actions[m.act.index()].name,
                    m.params,
                    &s.0
                );
                seen.s_steps += 1;
            }
            for fold in bits.iter().filter(|f| f.act() == m.act) {
                let slot = match fold.key() {
                    FoldKey::Param(p) => m.params[usize::from(*p)],
                    FoldKey::Linked {
                        link_var, s_param, ..
                    } => {
                        // The link names the row through its domain: an optional link
                        // holds slot + 1, and 0 for no row; a required one the slot.
                        let row = usize::from(m.params[usize::from(*s_param)]);
                        let link = s.get(cone, *link_var, row);
                        match cone.var(*link_var).domain.slot(link) {
                            Some(slot) => slot,
                            None => continue,
                        }
                    }
                };
                assert!(
                    fold.holds(cone, &s, slot),
                    "{name} {target:?}: the bit of {} is false at slot {slot} where the \
                     action runs, in {:?}",
                    cone.actions[m.act.index()].name,
                    &s.0
                );
                seen.checked += 1;
                seen.past_0 += usize::from(slot > 0);
            }
        }
    }
}

/// The converse, the other half of the fold half of Lemma 6 (claim A11): a bit adds no step the
/// cone lacks. For every reachable state of every reduced cone of `models` whose reduction
/// folds, every silent step the extended sub-lifecycle takes from the state's projection (its
/// S part, raw inputs, and bits, as `Reduction::key` computes them) is a move of the cone: an
/// action the step stands for, with the params the step binds outside the folded T params,
/// leads the cone to the same state. A bit true where no binding of the folded guards holds
/// would add such a step. The steps of unfolded S actions are checked too, but not counted.
fn folded_steps_are_steps_of_the_cone(
    models: &[(&str, SemanticModel)],
    bounds: &[Bounds],
) -> FoldsChecked {
    let mut seen = FoldsChecked::default();
    each_folded_cone(models, bounds, |name, target, cone, run, red| {
        silent_steps_are_cone_steps(name, target, cone, run, red, &mut seen);
    });
    seen
}

/// The body of `folded_steps_are_steps_of_the_cone` for one cone.
fn silent_steps_are_cone_steps(
    name: &str,
    target: Target,
    cone: &TransitionSystem,
    run: &ComponentRun,
    red: &Reduction,
    seen: &mut FoldsChecked,
) {
    seen.folds += red.bits().iter().flatten().count();
    let silent = red.silent_steps(cone);
    for s in run.states.iter() {
        let moves: Vec<_> = cone.moves(&s).collect();
        for step in silent.from(red, cone, &s) {
            let realized = moves.iter().any(|m| {
                m.next == step.next
                    && step.acts.iter().any(|(a, kept)| {
                        *a == m.act
                            && (kept.iter().zip(m.params.iter()))
                                .all(|(k, p)| k.is_none_or(|k| k == *p))
                    })
            });
            assert!(
                realized,
                "{name} {target:?}: the extended sub-lifecycle steps by {} (bits at slots \
                 {:?}) where the cone has no such step, in {:?}",
                cone.actions[step.acts[0].0.index()].name,
                step.bit_slots,
                &s.0
            );
            if step.folded {
                seen.checked += 1;
                seen.past_0 += usize::from(step.bit_slots.iter().any(|c| *c > 0));
            }
        }
    }
}

/// What `lemma6_steps_hold` checked: the cells of the parameter matrix its cones fill, and
/// the cone's steps it matched in the extended sub-lifecycle.
#[derive(Debug, Default)]
struct StepsChecked {
    cells: BTreeSet<ParamCell>,
    /// The S half, as the two fold checks count it.
    s: FoldsChecked,
    /// The cone's T steps that change the input bytes, each matched stage by stage.
    rises: usize,
    /// Those steps with an S var of the successor moved, each found to be no rise.
    forged: usize,
}

/// Lemma 6 of docs/theory/cones.md step by step, on every complete reduced cone of `models`
/// under each of `bounds`, folded or not: every S step of the cone from a reachable state is
/// a silent step of the extended sub-lifecycle `X` from the state's projection to the
/// successor's (`cone_s_steps_are_silent_steps`), every silent step of `X` from there is a
/// step of the cone (`silent_steps_are_cone_steps`), and every T step that changes the input
/// bytes is a rise of `X` by the same action to the successor's raw inputs, then one flip per
/// bit slot that changes (`SilentSteps::rises_through`). Returns the cells of the parameter
/// matrix that the actions these checks ran on fill (`Reduction::param_cells`), for each
/// cell's test to assert it ran.
fn lemma6_steps_hold(models: &[(&str, SemanticModel)], bounds: &[Bounds]) -> StepsChecked {
    let mut seen = StepsChecked::default();
    each_reduced_cone(models, bounds, false, |name, target, cone, run, red| {
        cone_s_steps_are_silent_steps(name, target, cone, run, red, &mut seen.s);
        silent_steps_are_cone_steps(name, target, cone, run, red, &mut seen.s);
        let silent = red.silent_steps(cone);
        // The actions a check above or below ran on: every S step, every T step that changes
        // the inputs, and every step of a steady action, which `R` runs as the cone does.
        let mut stepped: BTreeSet<ActId> = BTreeSet::new();
        for s in run.states.iter() {
            let before = red.input_bytes(cone, &s);
            for m in cone.moves(&s) {
                if red.s_actions().contains(&m.act) || red.is_steady(m.act) {
                    stepped.insert(m.act);
                }
                if red.s_actions().contains(&m.act) || red.input_bytes(cone, &m.next) == before {
                    continue;
                }
                assert!(
                    silent.rises_through(red, cone, m.act, &s, &m.next),
                    "{name} {target:?}: the cone steps by {} {:?}, which changes the inputs, \
                     where the extended sub-lifecycle has no such rise, in {:?}",
                    cone.actions[m.act.index()].name,
                    m.params,
                    &s.0
                );
                seen.rises += 1;
                stepped.insert(m.act);
                // The check reads the state it is given: no rise changes S, so the same step
                // with an S var moved is no rise.
                if let Some(v) = cone
                    .var_ids()
                    .find(|v| red.is_s_var(*v) && cone.var(*v).domain.size(cone) > 1)
                {
                    let mut forged = m.next.clone();
                    let size = cone.var(v).domain.size(cone);
                    let moved = Value(
                        u8::try_from((usize::from(forged.get(cone, v, 0).0) + 1) % size).unwrap(),
                    );
                    forged.set(cone, v, 0, moved);
                    assert!(
                        !silent.rises_through(red, cone, m.act, &s, &forged),
                        "{name} {target:?}: a rise to a state with S moved"
                    );
                    seen.forged += 1;
                }
            }
        }
        seen.cells.extend(red.param_cells(cone, &stepped));
    });
    seen
}

/// The cells of the parameter matrix of Lemma 6 (docs/theory/cones.md, "Parameters") that
/// cannot occur, each for the reason the matrix gives: an S action's `New` or `Earlier` row
/// on T writes T; an unfolded S action reads the existence of every T row it binds, so no
/// such row is on a stub or dropped; a fold whose key the restriction would drop stays
/// unfolded (`Fold::key_kept`), and a node fold's row is on an input or a stub; a key is
/// never a `New` row; a rise writes no S, so it has no `New` or `Earlier` row
/// on S.
fn impossible_cell(cell: &ParamCell) -> bool {
    let (kind, side, role, key) = *cell;
    let s_action = kind == "folded" || kind == "unfolded";
    (s_action && side != "S" && matches!(role, "New" | "Earlier"))
        || (kind == "unfolded" && matches!(side, "stub" | "dropped"))
        || (kind == "folded" && side == "dropped" && key)
        || (kind == "folded" && side == "S" && role == "node row")
        || (role == "New" && key)
        || (kind == "rise" && side == "S" && matches!(role, "New" | "Earlier"))
}

/// `lemma6_steps_hold` on `models` at `Bounds::STATES` and `Bounds::VERIFY`, asserting that
/// the cones fill every cell of `cells` and no cell `impossible_cell` names.
fn assert_cells_step_as_the_cone_does(
    models: &[(&str, SemanticModel)],
    bounds: &[Bounds],
    cells: &[ParamCell],
) {
    let seen = lemma6_steps_hold(models, bounds);
    for cell in cells {
        assert!(seen.cells.contains(cell), "{cell:?} not filled: {seen:?}");
    }
    let impossible: Vec<&ParamCell> = seen.cells.iter().filter(|c| impossible_cell(c)).collect();
    assert!(
        impossible.is_empty(),
        "cells the matrix rules out: {impossible:?}"
    );
    assert!(
        seen.s.s_steps > 0 && seen.rises > 0 && seen.forged == seen.rises,
        "{seen:?}"
    );
}

/// The order's cone, with S the vouchers, booklets and tickets: Voucher's owner keeps Order as
/// a stub of the extended sub-lifecycle, and Shop is a T entity no var of S or the inputs
/// names, so the restriction drops it. Sheet, Memo, Pad and Tab are raw inputs that `Use`
/// reads, and every T use case that creates one runs only while its order is held, so it is a
/// T action and a rise. Each use case fills cells of the parameter matrix of Lemma 6:
///
/// - `Grant`: a fold keyed by a `Parent` row on the stub, with a `Parent` row of S, the
///   booklet, beside it;
/// - `UseOpen`: a node fold over the order its link names, on the stub;
/// - `UseNoted`: a fold keyed by an `Existing` row on the stub (it sets the order's `note`,
///   which the cone does not hold);
/// - `UseRung`: a fold with a `Triggered` row on Shop, dropped, beside its key;
/// - `SheetNote`, `RingNote`, `PaidNote`, `MarkNote`, `OpenNote`: rises with an `Existing`, a
///   `Parent`, a `Triggered`, an `Earlier` (`MarkNote`'s second step) and a `New` row on the
///   stub;
/// - `ShopNote`, `ShopOpenNote`, `MemoNote`, `RingNote`, `CloseNote`: rises with an
///   `Existing`, a `New`, a `Parent`, a `Triggered` and an `Earlier` row on Shop
///   (`CloseNote`'s second step).
///
/// In another of its targets' cones `RingNote` is an unfolded S action with a `Triggered` row
/// on an input.
const STUB_AND_DROPPED_ROWS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(new, paid, shipped) @default(new)  hold: Bool @default(false)  mark: Enum(a, b) @default(a)  note: Bool @default(false) }
entity Voucher "券" { id: Int @pk  status: Enum(open, used) @default(open) }
entity Booklet "冊子" { id: Int @pk  status: Enum(open, closed) @default(open) }
entity Ticket "札" { id: Int @pk  status: Enum(draft, issued) @default(draft) }
entity Sheet "票" { id: Int @pk  status: Enum(made) @default(made) }
entity Memo "メモ" { id: Int @pk  status: Enum(made) @default(made) }
entity Pad "パッド" { id: Int @pk  status: Enum(made) @default(made) }
entity Tab "タブ" { id: Int @pk  status: Enum(made) @default(made) }
entity Shop "店" { id: Int @pk  status: Enum(open, closed) @default(open) }
relate(Voucher, Order, N:1)
relate(Voucher, Booklet, N:1)
relate(Sheet, Order, N:1)
relate(Memo, Shop, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Hold "保留"
usecase Release "解除"
usecase Bind "製本"
usecase Grant "発券"
usecase Use "使用"
usecase UseOpen "開本で使用"
usecase UseRung "呼出で使用"
usecase UseNoted "注記して使用"
usecase Draft "起票"
usecase Issue "発行"
usecase OpenShop "開店"
usecase Ring "呼出"
usecase SheetNote "票と印"
usecase RingNote "呼出の票"
usecase PaidNote "支払のメモ"
usecase MarkNote "印とパッド"
usecase ShopNote "店とメモ"
usecase ShopOpenNote "開店とメモ"
usecase MemoNote "メモ"
usecase CloseNote "閉店とタブ"
event EvPaid "支払済"
event EvRing "呼出済"
creates(Place, Order)
transitions(Pay, Order.status, new -> paid)
raises(Pay, EvPaid)
transitions(Ship, Order.status, paid -> shipped)
condition HasUsedVoucher "使用済の券"
holds(HasUsedVoucher, Voucher.status == used)
applies_to(HasUsedVoucher, Ship)
sets(Hold, Order.hold == true)
sets(Release, Order.hold == false)
condition Held "保留中"
holds(Held, Order.hold == true)
creates(Bind, Booklet)
creates(Grant, Voucher)
transitions(Use, Voucher.status, open -> used)
condition Ready "揃っている"
holds(Ready, Ticket.status == issued, Sheet.status == made, Memo.status == made, Pad.status == made, Tab.status == made)
applies_to(Ready, Use)
condition BookOpen "冊子が開いている"
holds(BookOpen, Booklet.status == open)
condition OrderPaid "支払済"
holds(OrderPaid, Order.status == paid)
transitions(UseOpen, Voucher.status, open -> used)
applies_to(BookOpen, UseOpen)
applies_to(OrderPaid, UseOpen)
transitions(UseRung, Voucher.status, open -> used)
triggers(EvRing, UseRung)
transitions(UseNoted, Voucher.status, open -> used)
sets(UseNoted, Order.note == true)
creates(Draft, Ticket)
transitions(Issue, Ticket.status, draft -> issued)
creates(OpenShop, Shop)
transitions(Ring, Shop.status, open -> closed)
raises(Ring, EvRing)
creates(SheetNote, Sheet)
sets(SheetNote, Order.mark == b)
applies_to(Held, SheetNote)
creates(RingNote, Sheet)
triggers(EvRing, RingNote)
creates(PaidNote, Memo)
triggers(EvPaid, PaidNote)
creates(MarkNote, Pad)
sets(MarkNote, Order.mark == b)
applies_to(Held, MarkNote)
creates(ShopNote, Memo)
transitions(ShopNote, Shop.status, open -> closed)
applies_to(Held, ShopNote)
creates(ShopOpenNote, Memo)
creates(ShopOpenNote, Shop)
applies_to(Held, ShopOpenNote)
creates(MemoNote, Memo)
applies_to(Held, MemoNote)
creates(CloseNote, Tab)
transitions(CloseNote, Shop.status, open -> closed)
applies_to(Held, CloseNote)
usecase OpenNote "受付票"
creates(OpenNote, Order)
creates(OpenNote, Sheet)
forbidden(Order, status == shipped)
"#;

/// The order's cone, with S the vouchers and tickets, and every T row an S action binds on an
/// input: `Grant` copies its desk's kind, which no fold can stand for, so the desk's existence
/// and kind are raw inputs, and the order's existence with them. Each use case fills cells of
/// the parameter matrix of Lemma 6:
///
/// - `Grant`, `GrantNoted`: unfolded S actions with a `Parent` and an `Existing` row on an
///   input (`GrantNoted` sets the desk's `memo`, which the cone does not hold);
/// - `Stamp`: a fold keyed by a `Triggered` row on an input, the closed desk its link names;
/// - `StampNoted`: a fold keyed by an `Existing` row on an input;
/// - `StampIssued`: a fold with a `Triggered` row of S, the issued ticket, and a node fold
///   over the order its link names;
/// - `Check`: a node fold over the desk its link names;
/// - `SheetNote`, `ClosedNote`, `CloseNote`: rises with a `Parent`, a `Triggered` and an
///   `Earlier` row on an input (`CloseNote`'s second step).
///
/// In another of its targets' cones `GrantNoted` is a rise with an `Existing` row of S.
const INPUT_ROWS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(new, paid, shipped) @default(new)  hold: Bool @default(false)  note: Bool @default(false) }
entity Desk "窓口" { id: Int @pk  status: Enum(open, closed) @default(open)  kind: Enum(food, retail) @default(food)  memo: Bool @default(false) }
entity Voucher "券" { id: Int @pk  status: Enum(open, used) @default(open)  kind: Enum(food, retail) @default(food)  stamp: Bool @default(false) }
entity Ticket "札" { id: Int @pk  status: Enum(draft, issued) @default(draft) }
entity Sheet "票" { id: Int @pk  status: Enum(made) @default(made) }
entity Pad "パッド" { id: Int @pk  status: Enum(made) @default(made) }
relate(Voucher, Order, N:1)
relate(Voucher, Desk, N:1)
relate(Sheet, Desk, N:1)
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Hold "保留"
usecase Release "解除"
usecase OpenDesk "開設"
usecase Close "閉鎖"
usecase Grant "発券"
usecase GrantNoted "注記して発券"
usecase Use "使用"
usecase Check "確認"
usecase Stamp "押印"
usecase StampIssued "発行で押印"
usecase StampNoted "注記して押印"
usecase Draft "起票"
usecase Issue "発行"
usecase SheetNote "票"
usecase ClosedNote "閉鎖のパッド"
usecase CloseNote "閉鎖とパッド"
event EvClosed "閉鎖済"
event EvIssued "発行済"
creates(Place, Order)
transitions(Pay, Order.status, new -> paid)
transitions(Ship, Order.status, paid -> shipped)
condition HasUsedVoucher "使用済の券"
holds(HasUsedVoucher, Voucher.status == used)
applies_to(HasUsedVoucher, Ship)
sets(Hold, Order.hold == true)
sets(Release, Order.hold == false)
condition Held "保留中"
holds(Held, Order.hold == true)
creates(OpenDesk, Desk)
transitions(Close, Desk.status, open -> closed)
raises(Close, EvClosed)
applies_to(Held, Close)
creates(Grant, Voucher)
sets(Grant, Voucher.kind == Desk.kind)
creates(GrantNoted, Voucher)
sets(GrantNoted, Voucher.kind == Desk.kind)
sets(GrantNoted, Desk.memo == true)
transitions(Use, Voucher.status, open -> used)
condition Ready "準備済"
holds(Ready, Ticket.status == issued, Sheet.status == made, Pad.status == made, Voucher.kind == food, Voucher.stamp == true)
applies_to(Ready, Use)
sets(Check, Voucher.stamp == true)
condition DeskOpen "窓口が開いている"
holds(DeskOpen, Desk.status == open)
applies_to(DeskOpen, Check)
sets(Stamp, Voucher.stamp == true)
triggers(EvClosed, Stamp)
condition OrderPaid "支払済"
holds(OrderPaid, Order.status == paid)
sets(StampIssued, Voucher.stamp == true)
triggers(EvIssued, StampIssued)
applies_to(OrderPaid, StampIssued)
sets(StampNoted, Voucher.stamp == true)
sets(StampNoted, Order.note == true)
creates(Draft, Ticket)
transitions(Issue, Ticket.status, draft -> issued)
raises(Issue, EvIssued)
creates(SheetNote, Sheet)
applies_to(Held, SheetNote)
creates(ClosedNote, Pad)
triggers(EvClosed, ClosedNote)
creates(CloseNote, Pad)
transitions(CloseNote, Desk.status, open -> closed)
applies_to(Held, CloseNote)
forbidden(Order, status == shipped)
"#;

/// Lemma 6 step by step on the cells of its parameter matrix with a row on a stub or a
/// dropped entity (`STUB_AND_DROPPED_ROWS`): every S step of the cone is a silent step of the
/// extended sub-lifecycle and the converse, and every T step that changes the inputs is a
/// rise of the same action, stage by stage. Also on `INPUT_ROWS` with the voucher's kind and
/// stamp read by no one, so that neither `Grant`'s copy nor `GrantNoted`'s is in the cone:
/// both fold, and their desk, whose existence no S action then reads, is dropped beside the
/// key, as a `Parent` row and as an `Existing` one. At `Bounds::STATES` only: the order's
/// cone of `STUB_AND_DROPPED_ROWS` at `Bounds::VERIFY` passes the 200,000-state cap.
#[test]
fn stub_and_dropped_rows_bind_where_the_cone_binds_them() {
    let unread = INPUT_ROWS.replace(", Voucher.kind == food, Voucher.stamp == true", "");
    assert_ne!(unread, INPUT_ROWS);
    let models = [
        ("STUB_AND_DROPPED_ROWS", model_from(STUB_AND_DROPPED_ROWS)),
        ("INPUT_ROWS, voucher unread", model_from(&unread)),
    ];
    assert_cells_step_as_the_cone_does(
        &models,
        &[Bounds::STATES],
        &[
            ("folded", "S", "Parent", false),
            ("folded", "stub", "Parent", true),
            ("folded", "stub", "Existing", true),
            ("folded", "stub", "node row", true),
            ("folded", "dropped", "Existing", false),
            ("folded", "dropped", "Parent", false),
            ("folded", "dropped", "Triggered", false),
            ("rise", "stub", "New", false),
            ("rise", "stub", "Existing", false),
            ("rise", "stub", "Parent", false),
            ("rise", "stub", "Triggered", false),
            ("rise", "stub", "Earlier", false),
            ("rise", "dropped", "Existing", false),
            ("rise", "dropped", "New", false),
            ("rise", "dropped", "Parent", false),
            ("rise", "dropped", "Triggered", false),
            ("rise", "dropped", "Earlier", false),
            ("unfolded", "input", "Triggered", false),
        ],
    );
}

/// Lemma 6 step by step on the cells of its parameter matrix with a row on an input
/// (`INPUT_ROWS`), as `stub_and_dropped_rows_bind_where_the_cone_binds_them` checks them, and
/// on `STUB_AND_DROPPED_ROWS` with every voucher under a shop as well: in the voucher's own
/// cone `Grant` folds keyed by its `Parent` shop, an input there. Last on `INPUT_ROWS` with the
/// voucher unread, as `stub_and_dropped_rows_bind_where_the_cone_binds_them` has it, and with
/// `Issue` waiting for an open desk: that read makes the desk's existence and status raw
/// inputs, and `Grant` and `GrantNoted` still fold, keyed by the order, with the desk beside
/// the key as a `Parent` row and an `Existing` one on an input.
#[test]
fn input_rows_bind_where_the_cone_binds_them() {
    let open_desks = format!(
        "{}condition AnyOpenDesk \"開いた窓口\"\nholds(AnyOpenDesk, Desk.status == open)\n\
         applies_to(AnyOpenDesk, Issue)\n",
        INPUT_ROWS.replace(", Voucher.kind == food, Voucher.stamp == true", "")
    );
    let models = [
        ("INPUT_ROWS, desks read raw", model_from(&open_desks)),
        ("INPUT_ROWS", model_from(INPUT_ROWS)),
        (
            "STUB_AND_DROPPED_ROWS under shops",
            model_from(&format!(
                "{STUB_AND_DROPPED_ROWS}relate(Voucher, Shop, N:1)\n"
            )),
        ),
    ];
    assert_cells_step_as_the_cone_does(
        &models,
        &[Bounds::STATES],
        &[
            ("folded", "S", "Triggered", false),
            ("folded", "input", "Existing", true),
            ("folded", "input", "Existing", false),
            ("folded", "input", "Parent", true),
            ("folded", "input", "Parent", false),
            ("folded", "input", "Triggered", true),
            ("folded", "input", "node row", true),
            ("unfolded", "input", "Existing", false),
            ("unfolded", "input", "Parent", false),
            ("rise", "S", "Existing", false),
            ("rise", "input", "New", false),
            ("rise", "input", "Parent", false),
            ("rise", "input", "Triggered", false),
            ("rise", "input", "Earlier", false),
        ],
    );
}

/// Lemma 6 step by step on the models of the other reduction tests here, at one and two
/// instances, rises included: `RISE_ON_A_STUB`'s rise with a `Parent` row on the stub,
/// `CLAIM_CASE`'s fold with a `Triggered` row on an input beside its key and its rise with an
/// `Existing` row on an input, `ORDER_SHIP`'s fold keyed by a `Triggered` row on the stub,
/// `TRIGGERED_ON_A_STUB`'s fold with one beside its key, and the S rows of every role.
#[test]
fn the_reduction_models_bind_where_their_cones_bind() {
    let models: Vec<(&str, SemanticModel)> = [
        ("GATE", GATE),
        ("IDENTITY_QUOTIENT", IDENTITY_QUOTIENT),
        ("CLAIM_CASE", CLAIM_CASE),
        ("ORDER_SHIP", ORDER_SHIP),
        ("CASE_FOLD", CASE_FOLD),
        ("TRIGGERED_ON_A_STUB", TRIGGERED_ON_A_STUB),
        ("RISE_ON_A_STUB", RISE_ON_A_STUB),
    ]
    .into_iter()
    .map(|(name, src)| (name, model_from(src)))
    .collect();
    assert_cells_step_as_the_cone_does(
        &models,
        &[Bounds::STATES, Bounds::VERIFY],
        &[
            ("folded", "S", "New", false),
            ("folded", "S", "Existing", false),
            ("folded", "S", "Existing", true),
            ("folded", "input", "Triggered", false),
            ("folded", "stub", "Triggered", true),
            ("folded", "stub", "Triggered", false),
            ("unfolded", "S", "New", false),
            ("unfolded", "S", "Existing", false),
            ("unfolded", "S", "Parent", false),
            ("unfolded", "S", "Earlier", false),
            ("unfolded", "S", "Triggered", false),
            ("rise", "S", "Parent", false),
            ("rise", "S", "Triggered", false),
            ("rise", "input", "Existing", false),
            ("rise", "stub", "Parent", false),
        ],
    );
}

fn domain_samples() -> Vec<(&'static str, SemanticModel)> {
    DOMAIN_SAMPLES.iter().map(|s| (*s, load(s))).collect()
}

/// The bundled samples fold nothing; the domain samples' reduced cones hold 71 folds (about
/// 3,100 moves of folded actions, none past slot 0). CI runs this in release beside
/// `reduced_domain_cones_decide_what_the_cones_decide` (about 9 s).
#[test]
fn reduced_domain_cones_hold_each_fold_bit_where_its_action_runs() {
    let seen = fold_bits_hold_where_their_actions_run(
        &domain_samples(),
        &[Bounds::STATES, Bounds::VERIFY],
    );
    assert!(seen.folds >= 71, "{seen:?}");
    assert!(seen.checked >= 3_000, "{seen:?}");
}

/// The converse on the domain samples: on their 71 folds, every step a true bit lets the
/// extended sub-lifecycle take (about 3,100) is a step of the cone. CI runs this in release
/// beside the test above (about 9 s).
#[test]
fn reduced_domain_cones_step_by_a_true_fold_bit_only_where_the_cone_steps() {
    let seen =
        folded_steps_are_steps_of_the_cone(&domain_samples(), &[Bounds::STATES, Bounds::VERIFY]);
    assert!(seen.folds >= 71, "{seen:?}");
    assert!(seen.checked >= 3_000, "{seen:?}");
}

/// Both halves past slot 0. No checked move of the domain samples binds a fold's key past
/// slot 0, so a bit planted false at slot 1 alone passes the two tests above. The case fold's
/// folds (File's, and Pay's node fold keyed by the linked case) at two and three instances
/// bind keys at every slot, about 1,700 of their 2,800 moves past slot 0. A bit
/// planted false at slot 1 in `Fold::holds` fails the first half, on File; one planted true
/// at a present row 1 fails the second, on Pay (and the domain samples' converse too).
#[test]
fn case_fold_bits_hold_exactly_where_their_actions_run_past_slot_0() {
    let models = [("case fold", model_from(CASE_FOLD))];
    let bounds = [
        Bounds::VERIFY,
        Bounds::VERIFY.at(std::num::NonZeroU8::new(3).unwrap()),
    ];
    let held = fold_bits_hold_where_their_actions_run(&models, &bounds);
    assert!(held.past_0 > 0, "{held:?}");
    let stepped = folded_steps_are_steps_of_the_cone(&models, &bounds);
    assert!(stepped.past_0 > 0, "{stepped:?}");
}

/// A `Triggered` param beside a fold's key, on an entity the extended sub-lifecycle stubs,
/// binds there as the key does (claim A11): the reduced cone finds the violation the cone has, at
/// one and two instances, and both halves of the fold check pass on it.
#[test]
fn a_triggered_param_on_a_stub_binds_in_the_extended_sub_lifecycle() {
    let model = model_from(TRIGGERED_ON_A_STUB);
    let bounds = [Bounds::STATES, Bounds::VERIFY];
    let caps = Caps::new(usize::MAX, 200_000);
    for b in &bounds {
        let ts = TransitionSystem::build(&model, b.clone(), &Scope::ALL).ts;
        let targets = ts.targets();
        let reduced = ts.explore_targets_reduced(&targets, caps);
        let rule = targets
            .iter()
            .find(|t| matches!(t, Target::Invariant(_)))
            .expect("the rule");
        let r = reduced.cone_of(*rule);
        assert!(
            r.run.system.reduction().is_some(),
            "{b:?}: the rule's cone reduces"
        );
        assert!(
            !violated(&reduced).is_empty(),
            "{b:?}: the reduced cone finds the violation"
        );
        assert!(assert_reduction_exact("TRIGGERED_ON_A_STUB", &model, b.clone()) >= 1);
        // `Issue` binds its order freely in the extended sub-lifecycle; every state's path
        // lifts to a real path of the cone, the one where only Order#2 is paid included.
        let cone = ts.restrict_to(&r.cone.vars, &r.cone.keep);
        for at in 0..r.run.states.len() {
            let (start, steps) = lifted_path(&r.run, u32::try_from(at).unwrap());
            assert_real_path(&cone, &start, &steps);
        }
    }
    let models = [("TRIGGERED_ON_A_STUB", model)];
    let held = fold_bits_hold_where_their_actions_run(&models, &bounds);
    assert!(held.s_steps > 0, "{held:?}");
    let stepped = folded_steps_are_steps_of_the_cone(&models, &bounds);
    assert!(stepped.checked > 0, "{stepped:?}");
}

/// A rise whose `Parent` param is on an entity the extended sub-lifecycle stubs binds there
/// (claims A8, A11): the cone's `Note` step has its rise, and the reduced cone decides what the
/// cone decides.
#[test]
fn a_rise_with_a_param_on_a_stub_is_a_rise_of_the_extended_sub_lifecycle() {
    let model = model_from(RISE_ON_A_STUB);
    for b in [Bounds::STATES, Bounds::VERIFY] {
        assert!(assert_reduction_exact("RISE_ON_A_STUB", &model, b) >= 1);
    }
}

/// A fold keyed by a T param that `restrict_to(S ∪ inputs)` drops leaves its action unfolded
/// (claim A8; docs/theory/cones.md, Lemma 6, "Parameters"). In `cone_fold_key_dropped` the
/// rule's cone drops `GrantNoted`'s owner assignment and its `memo` effect, so its fold would
/// be keyed by its `Existing` Desk param, on an entity no var of S or the inputs names. The
/// split leaves `GrantNoted` unfolded with Desk's existence an input, the extended
/// sub-lifecycle passes its check, and the reduced cone decides what the cone decides, at one
/// and two instances. With the unfolding dropped the fold comes back, the build's check names
/// its key, and the cone is decided unreduced, with the cone's verdicts.
#[test]
fn a_fold_whose_key_the_restriction_drops_leaves_its_action_unfolded() {
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    let model = model_from(include_str!(
        "../../../../rdra-ish-cli/tests/engine_corpus/cone_fold_key_dropped.rdra"
    ));
    let caps = Caps::new(usize::MAX, 200_000);
    for b in [Bounds::STATES, Bounds::VERIFY] {
        let ts = TransitionSystem::build(&model, b.clone(), &Scope::ALL).ts;
        let rule = *ts
            .targets()
            .iter()
            .find(|t| matches!(t, Target::Invariant(_)))
            .expect("the rule");
        let (sys, seeds) = ts.cone_system(&ts.target_cone(rule), [ts.target_seed(rule)]);
        let folded = || -> Vec<String> {
            let split = sys.split_with(&seeds, true);
            let act = |a: crate::lifecycle::ActId| sys.actions()[a.index()].name.clone();
            split.folds.iter().map(|f| act(f.act())).collect()
        };
        let split = sys.split_with(&seeds, true);
        assert!(
            split
                .s_actions
                .iter()
                .any(|a| sys.actions()[a.index()].name == "GrantNoted"),
            "{b:?}: GrantNoted is an S action"
        );
        assert!(
            !folded().contains(&"GrantNoted".to_string()),
            "{b:?}: GrantNoted stays unfolded: {:?}",
            folded()
        );
        let inputs: Vec<String> = split.inputs.iter().map(|v| sys.var_name(*v)).collect();
        assert!(
            inputs.iter().any(|v| v.starts_with("Desk")),
            "{b:?}: Desk's existence is an input: {inputs:?}"
        );
        assert_eq!(
            sys.stub_check(&seeds, true).and_then(|(_, f)| f),
            None,
            "{b:?}: the check passes"
        );
        let reduced = sys.reduced(&seeds, caps);
        assert!(
            reduced.reduction().is_some(),
            "{b:?}: the rule's cone reduces"
        );
        // Planted: with the unfolding dropped, GrantNoted folds on its dropped key, the build's
        // check names it, and the cone is decided unreduced.
        let (planted, fault, fallback) = dropping("unfold_dropped_keys", || {
            (
                folded(),
                sys.stub_check(&seeds, true).and_then(|(_, f)| f),
                sys.reduced(&seeds, caps),
            )
        });
        assert!(
            planted.contains(&"GrantNoted".to_string()),
            "{b:?}: the planted split folds GrantNoted: {planted:?}"
        );
        let fault = fault.expect("the planted check fails");
        assert!(
            fault.contains("of S action GrantNoted keys its fold"),
            "{b:?}: {fault}"
        );
        assert!(fallback == sys, "{b:?}: the unreduced cone is the cone");
        let run = |s: &TransitionSystem| -> BTreeSet<String> {
            let run = s.clone().run(caps);
            assert!(run.complete, "{b:?}");
            run.diags.iter().map(|d| d.diag.name(&[])).collect()
        };
        assert_eq!(run(&reduced), run(&sys), "{b:?}: verdicts");
        let explored = ts.explore_targets_reduced(&ts.targets(), caps);
        assert!(
            !violated(&explored).is_empty(),
            "{b:?}: the reduced cone finds the violation"
        );
        assert!(assert_reduction_exact("cone_fold_key_dropped", &model, b) >= 1);
    }
}

/// A rise's guard that names a created row by its index alone goes with the row's `New`
/// param when the extended sub-lifecycle drops it (claim A8). In
/// `cone_rise_names_a_dropped_new_row`, `Place` creates an order and a sheet; Sheet's
/// existence is an input, Order's is neither in S nor an input, and `HasOpen` joins a voucher
/// (of S) to the new order. `restrict_to(S ∪ inputs)` keeps `Place` as a rise with its sheet
/// param alone and without that guard, and the reduced cone decides what the cone decides,
/// at one and two instances.
#[test]
fn a_rise_guard_on_a_dropped_new_row_goes_with_the_row() {
    let model = model_from(include_str!(
        "../../../../rdra-ish-cli/tests/engine_corpus/cone_rise_names_a_dropped_new_row.rdra"
    ));
    for b in [Bounds::STATES, Bounds::VERIFY] {
        let ts = TransitionSystem::build(&model, b.clone(), &Scope::ALL).ts;
        let rule = *ts
            .targets()
            .iter()
            .find(|t| matches!(t, Target::Invariant(_)))
            .expect("the rule");
        let (sys, seeds) = ts.cone_system(&ts.target_cone(rule), [ts.target_seed(rule)]);
        let split = sys.split_with(&seeds, true);
        let all: VarSet = split.s.union(&split.inputs).copied().collect();
        let place = sys
            .actions()
            .iter()
            .find(|a| a.name == "Place")
            .expect("Place");
        assert_eq!(
            place.params.len(),
            2,
            "{b:?}: Place binds an order and a sheet"
        );
        assert!(
            !place.guards.is_empty(),
            "{b:?}: Place has its condition in the cone"
        );
        let (ext, _) = sys.restrict_to_mapped(&all, &BTreeSet::new(), false);
        let place = ext
            .actions()
            .iter()
            .find(|a| a.name == "Place")
            .expect("Place is a rise of the extended sub-lifecycle");
        assert_eq!(place.params.len(), 1, "{b:?}: the order param is dropped");
        assert!(
            place.guards.is_empty(),
            "{b:?}: the guard that names the order goes with it: {:?}",
            place.guards
        );
        let reduced = sys.reduced(&seeds, Caps::new(usize::MAX, 200_000));
        assert!(
            reduced.reduction().is_some(),
            "{b:?}: the rule's cone reduces"
        );
        assert!(assert_reduction_exact("cone_rise_names_a_dropped_new_row", &model, b) >= 1);
    }
}

/// The corpus cases whose fold and rise keep a param on a stub (claim A8), each with the
/// action that keeps it.
const STUB_CASES: &[(&str, &str, &str)] = &[
    (
        "cone_triggered_param_on_a_stub",
        "Issue",
        include_str!(
            "../../../../rdra-ish-cli/tests/engine_corpus/cone_triggered_param_on_a_stub.rdra"
        ),
    ),
    (
        "cone_rise_param_on_a_stub",
        "Note",
        include_str!("../../../../rdra-ish-cli/tests/engine_corpus/cone_rise_param_on_a_stub.rdra"),
    ),
];

/// Every target and property cone of `ts`, with its seed.
fn cones_with_seeds(ts: &TransitionSystem) -> Vec<(Cone, Seed)> {
    let targets = ts
        .targets()
        .into_iter()
        .filter(|t| !matches!(t, Target::Reach(_)))
        .map(|t| (ts.target_cone(t), ts.target_seed(t)));
    let properties = (0..ts.properties().len()).map(|i| (ts.property_cone(i), ts.property_seed(i)));
    targets.chain(properties).collect()
}

/// The check every build makes on the extended sub-lifecycle (claim A8) fires when
/// `bind_stub_params` is dropped: on the two corpus cases whose fold (`Issue`'s `Triggered`
/// Order) and rise (`Note`'s `Parent` Order) keep a param on a stub, the check names that
/// param, and the cone is decided unreduced, as the cone decides. With the switch off the same
/// cones reduce and the check passes.
#[test]
fn a_param_left_on_a_stub_makes_the_cone_decide_unreduced() {
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    let caps = Caps::new(usize::MAX, 200_000);
    for (name, action, src) in STUB_CASES {
        let model = model_from(src);
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let (mut faults, mut fallbacks) = (0, 0);
        for (cone, seed) in cones_with_seeds(&ts) {
            let (sys, local) = ts.cone_system(&cone, [seed]);
            let kept = sys.stub_check(&local, true);
            let reduced = sys.reduced(&local, caps);
            let dropped = dropping("bind_stub_params", || {
                (sys.stub_check(&local, true), sys.reduced(&local, caps))
            });
            if let Some((_, fault)) = &kept {
                assert_eq!(fault, &None, "{name}: the check passes on main");
            }
            let Some((0, Some(fault))) = dropped.0 else {
                continue;
            };
            faults += 1;
            assert!(
                fault.contains(&format!(" of {action}")),
                "{name}: the fault names {action}'s param: {fault}"
            );
            if reduced.reduction().is_none() {
                continue;
            }
            fallbacks += 1;
            assert!(
                dropped.1.reduction().is_none(),
                "{name}: the cone is decided unreduced"
            );
            assert!(dropped.1 == sys, "{name}: the unreduced cone is the cone");
            let run = |s: &TransitionSystem| -> BTreeSet<String> {
                let run = s.clone().run(caps);
                assert!(run.complete, "{name}");
                run.diags.iter().map(|d| d.diag.name(&[])).collect()
            };
            assert_eq!(run(&dropped.1), run(&reduced), "{name}: verdicts");
        }
        assert!(faults > 0, "{name}: the check fires");
        assert!(fallbacks > 0, "{name}: a reduced cone falls back");
    }
}

/// The samples, the domain samples, and the engine corpus cases that build a model.
pub(super) fn walked_models() -> Vec<(String, SemanticModel)> {
    let corpus = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../rdra-ish-cli/tests/engine_corpus"
    );
    let cases = std::fs::read_to_string(format!("{corpus}/CASES")).expect("CASES");
    let mut models: Vec<(String, SemanticModel)> = SAMPLES
        .iter()
        .chain(DOMAIN_SAMPLES)
        .map(|s| (s.to_string(), load(s)))
        .collect();
    for case in cases
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let src = std::fs::read_to_string(format!("{corpus}/{case}.rdra")).expect(case);
        // A case whose model has errors (a `check` case) builds no system.
        let (ast, _) = rdra_ish_syntax::parse(&src);
        let (model, diags) = crate::analysis::build_model(&ast);
        if diags.iter().all(|d| d.is_warning) {
            models.push((case.to_string(), model));
        }
    }
    models
}

/// The check never fires on the samples, the domain samples, and the engine corpus cases that
/// build a model: for every
/// target and property cone, with and without the inputs of S, the extended sub-lifecycle
/// `reduced_with` builds binds no param on a stub that needs a present row. Only the top
/// level is built here; a nested one, built in `Classes::of`, goes through the same check in
/// every run, which a debug or `shadow` build turns into a panic.
#[test]
fn no_extended_sub_lifecycle_leaves_a_param_on_a_stub() {
    let models = walked_models();
    let (mut built, mut rebound) = (0, 0);
    for (name, model) in &models {
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
            for (cone, seed) in cones_with_seeds(&ts) {
                let (sys, local) = ts.cone_system(&cone, [seed]);
                for inputs in [true, false] {
                    let Some((n, fault)) = sys.stub_check(&local, inputs) else {
                        continue;
                    };
                    assert_eq!(fault, None, "{name}: a cone of {:?}", cone.vars);
                    built += 1;
                    rebound += n;
                }
            }
        }
    }
    assert!(
        built > 0 && rebound > 0,
        "{built} built, {rebound} params rebound"
    );
}

/// The split `split_with` chooses passes its check (Lemma 5 of docs/theory/cones.md,
/// `reduce/lemma5.rs`) on the samples, the domain samples, and the engine corpus cases that
/// build a model: for every target and property cone, with and without the inputs of S.
/// Only the top level is walked here; a nested split, chosen in `Classes::of`, goes through
/// the same check in every run, which a debug or `shadow` build turns into a panic.
#[test]
fn every_split_passes_its_check() {
    let (mut splits, mut reducing) = (0, 0);
    for (name, model) in &walked_models() {
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
            for (cone, seed) in cones_with_seeds(&ts) {
                let (sys, local) = ts.cone_system(&cone, [seed]);
                for inputs in [true, false] {
                    let split = sys.split_with(&local, inputs);
                    let faults = sys.split_faults(&local, &split);
                    let faults: Vec<String> = faults.iter().map(ToString::to_string).collect();
                    assert!(
                        faults.is_empty(),
                        "{name}: a cone of {:?}: {faults:?}",
                        cone.vars
                    );
                    splits += 1;
                    reducing += usize::from(!split.s.is_empty());
                }
            }
        }
    }
    assert!(reducing > 0, "{splits} splits, {reducing} with an S side");
}

/// Writes the sets of every split `every_split_passes_its_check` walks, one JSON certificate
/// per line, to the file `RDRA_LEAN_CERTS` names, for the Lean checker of proofs/lean
/// (`scripts/check-lean-certificates.sh` runs both). Equal certificates are written once.
#[test]
#[ignore = "run by scripts/check-lean-certificates.sh"]
fn dump_lemma5_certificates() {
    use std::io::Write;
    let path = std::env::var("RDRA_LEAN_CERTS").expect("RDRA_LEAN_CERTS names the output");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    let mut seen = std::collections::HashSet::new();
    let (mut splits, mut written, mut reducing) = (0, 0, 0);
    for (name, model) in &walked_models() {
        for (label, bounds) in [("states", Bounds::STATES), ("verify", Bounds::VERIFY)] {
            let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
            for (cone, seed) in cones_with_seeds(&ts) {
                let (sys, local) = ts.cone_system(&cone, [seed]);
                for inputs in [true, false] {
                    let split = sys.split_with(&local, inputs);
                    splits += 1;
                    let mut cert = sys.lemma5_certificate(&local, &split);
                    if !seen.insert(cert.to_string()) {
                        continue;
                    }
                    cert["name"] = format!("{name} {label} inputs={inputs}").into();
                    writeln!(out, "{cert}").expect("write");
                    written += 1;
                    reducing += usize::from(!split.s.is_empty());
                }
            }
        }
    }
    out.flush().expect("flush");
    eprintln!(
        "{splits} splits, {written} distinct certificates ({reducing} with an S side) written \
         to {path}"
    );
    assert!(reducing > 0, "a certificate with an S side");
}

/// Writes every flat run of `X` that `Classes::of_graph` partitions while the cones of
/// `every_quotient_passes_the_checks_of_its_hypotheses` are reduced, at `Bounds::STATES` and
/// `Bounds::VERIFY`, with each run of `X` capped at `RDRA_LEAN_RUNS_CAP` states (default
/// 200,000), to the file `RDRA_LEAN_RUNS` names, for the Lean checker of proofs/lean
/// (`scripts/check-lean-certificates.sh` runs both). A record holds the numbered run (each
/// state's `obs`, `S` and input values, the silent edges, the rises, the initial states), the
/// class of each state and each class's representative (`partition/dump.rs`). Equal runs are
/// written once. The runs left out are counted by reason (`Dump::left_out`): runs under an
/// inner reduction, runs up to `H`, partitions that are the identity, runs a cap stopped,
/// and unobserved sub-lifecycles.
#[test]
#[ignore = "run by scripts/check-lean-certificates.sh"]
fn dump_partition_certificates() {
    use crate::lifecycle::reduce::partition_dump as dump;
    use std::io::Write;
    let path = std::env::var("RDRA_LEAN_RUNS").expect("RDRA_LEAN_RUNS names the output");
    let cap = std::env::var("RDRA_LEAN_RUNS_CAP").map_or(200_000, |c| c.parse().expect("a cap"));
    let caps = Caps::new(usize::MAX, cap);
    let ((), dump) = dump::recording(|| {
        for (name, model) in &walked_models() {
            for (label, bounds) in [("states", Bounds::STATES), ("verify", Bounds::VERIFY)] {
                let ts = TransitionSystem::build(model, bounds, &Scope::ALL).ts;
                dump::name(&format!("{name} {label}"));
                for (cone, seed) in cones_with_seeds(&ts) {
                    let (sys, local) = ts.cone_system(&cone, [seed]);
                    sys.reduced(&local, caps);
                }
            }
        }
    });
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    let mut seen = std::collections::HashSet::new();
    let (mut written, mut states) = (0, 0usize);
    for record in &dump.records {
        let (head, body) = record.split_once('\n').expect("a name line");
        if !seen.insert(body) {
            continue;
        }
        write!(out, "{head}\n{body}").expect("write");
        written += 1;
        states += body[2..]
            .split(' ')
            .next()
            .map_or(0, |n| n.parse().unwrap_or(0));
    }
    out.flush().expect("flush");
    let left_out: Vec<String> = (dump.left_out.iter())
        .map(|(why, k)| format!("{k} {why}"))
        .collect();
    eprintln!(
        "{} runs, {written} distinct ({states} states, cap {cap}) written to {path}; left out: \
         {}",
        dump.records.len(),
        left_out.join(", ")
    );
    assert!(written > 0, "a run");
}

/// The checks of `reduce/hypotheses.rs` (`RiseTable` and the stages of `Lemma6Staged`,
/// `hloop`, the inner split) pass on the samples, the domain samples, and the engine corpus
/// cases that build a model: every target and property cone is reduced at `Bounds::STATES`,
/// where a fault panics in this debug build. The counts show each check ran: on rises, on
/// runs with a class that can loop, on nested reductions, and on unobserved sub-lifecycles.
#[test]
fn every_quotient_passes_the_checks_of_its_hypotheses() {
    let caps = Caps::new(usize::MAX, 50_000);
    let before = crate::lifecycle::reduce::CHECKED.with(|c| c.get());
    let mut reduced = 0;
    for (_, model) in &walked_models() {
        let ts = TransitionSystem::build(model, Bounds::STATES, &Scope::ALL).ts;
        for (cone, seed) in cones_with_seeds(&ts) {
            let (sys, local) = ts.cone_system(&cone, [seed]);
            reduced += usize::from(sys.reduced(&local, caps).reduction().is_some());
        }
    }
    let after = crate::lifecycle::reduce::CHECKED.with(|c| c.get());
    let got = crate::lifecycle::reduce::Checked {
        rises: after.rises - before.rises,
        loop_runs: after.loop_runs - before.loop_runs,
        looping: after.looping - before.looping,
        inner: after.inner - before.inner,
        unobserved: after.unobserved - before.unobserved,
        faults: [0; 4],
    };
    eprintln!("{reduced} cones reduced; checked {got:?}");
    assert!(
        reduced > 0
            && got.rises > 0
            && got.loop_runs > 0
            && got.looping > 0
            && got.inner > 0
            && got.unobserved > 0,
        "{reduced} cones reduced; checked {got:?}"
    );
    assert_eq!(after.faults, before.faults, "no check fails");
}

/// The reduction is a divergence-blind stuttering bisimulation, and `Deadlock` reads "no
/// path changes the column or deletes the row" (`can_leave`), a reachability the quotient
/// keeps, not "no step at all", which it does not: a state looping forever in its block's
/// silent cycle and a state with no step left are both stuck, on the cone and on the reduced
/// cone alike (claim A8). The gate's x3/x4 toggle is such a silent cycle; the job seen at x2 is
/// stuck with no move of the gate left.
#[test]
fn a_silent_cycle_and_a_stuck_state_give_the_same_deadlocks_reduced() {
    let src = include_str!(
        "../../../../rdra-ish-cli/tests/engine_corpus/cone_reduced_loop_unreached.rdra"
    );
    let model = model_from(src);
    for bounds in [Bounds::STATES, Bounds::VERIFY] {
        let ts = TransitionSystem::build(&model, bounds.clone(), &Scope::ALL).ts;
        let targets = ts.targets();
        let plain = ts.explore_targets(&targets, Caps::VERIFY);
        let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
        let job = entity_target(&ts, "Job");
        assert!(
            reduced.cone_of(job).run.system.reduction().is_some(),
            "{bounds:?}: the job's cone reduces the gate"
        );
        let (a, b) = (diag_names(&plain), diag_names(&reduced));
        assert_eq!(a, b, "{bounds:?}: verdicts");
        for e in ["Deadlock_Gate_phase_pa", "Deadlock_Job_status_waiting"] {
            assert!(a.contains(e), "{bounds:?}: {e} in {a:?}");
        }
    }
}

/// The lift checks every lifted step against the unreduced cone's step relation in every
/// build, not only under `debug_assert!`: a T step that is no move of the cone is the
/// `ReplayError` of that step, which the caller reports as a fault, never a panic (claim A16). Here
/// the first T step of a reduced path is forged to land on the state it left.
#[test]
fn a_lifted_step_that_is_no_move_of_the_cone_is_a_replay_error() {
    let model = model_from(CASE_FOLD);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let targets = ts.targets();
    let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
    let run = &reduced.cone_of(entity_target(&ts, "Case")).run;
    let r = run.system.reduction().expect("the Case view reduces");
    let start = run.states.at(0).into_state();
    let (act, params) = (run.edges().iter())
        .filter(|e| e.from == 0)
        .map(|e| run.step(&e))
        .find(|(act, _)| !r.skips(*act))
        .expect("a T step from the start");
    let forged = vec![(act, params.into(), start.clone())];
    assert!(matches!(
        run.system.lifted(&start, &forged),
        Err(ReplayError::NoMove { state: 2, .. })
    ));
}

/// A reduced path whose start is no initial state of the cone, here a stored state with its
/// sub-lifecycle part forged to values no class holds, is `NotInitial`, a fault, never a
/// panic (claim A16): the lift's look-ups of the start's class are checked in every build.
#[test]
fn a_lift_from_no_initial_state_is_a_replay_error() {
    let model = model_from(CASE_FOLD);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let targets = ts.targets();
    let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
    let run = &reduced.cone_of(entity_target(&ts, "Case")).run;
    let r = run.system.reduction().expect("the Case view reduces");
    let mut start = run.states.at(0).into_state();
    let key = r.key(&run.system, &start);
    r.write(&mut start, &vec![u8::MAX; key.len()]);
    assert!(
        r.class(&run.system, &start).is_none(),
        "no class holds the forged part"
    );
    assert_eq!(run.system.lifted(&start, &[]), Err(ReplayError::NotInitial));
}

/// A class move with no S path behind it, here a reduced path that takes a real class move
/// and then a forged one back to the class it left, is the `NoMove` of that step, a fault,
/// never a panic (claim A16): the claims' S actions only create, review and pay, so no S path leads
/// back, and the lift's path search is checked in every build like its look-up of the target.
#[test]
fn a_class_move_with_no_s_path_is_a_replay_error() {
    let model = model_from(CASE_FOLD);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let targets = ts.targets();
    let reduced = ts.explore_targets_reduced(&targets, Caps::VERIFY);
    let run = &reduced.cone_of(entity_target(&ts, "Case")).run;
    let r = run.system.reduction().expect("the Case view reduces");
    let class = |i: u32| r.class(&run.system, &run.states.at(i as usize).into_state());
    let edges: Vec<_> = run.edges().iter().collect();
    let moved = *(edges.iter())
        .find(|e| r.skips(run.step(&**e).0) && class(e.to) != class(e.from))
        .expect("a class move");
    // A path of the reduced run from its first state to the class move's source.
    let mut parent = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::from([0u32]);
    while let Some(x) = queue.pop_front() {
        for e in edges.iter().filter(|e| e.from == x) {
            if e.to != 0 && !parent.contains_key(&e.to) {
                parent.insert(e.to, *e);
                queue.push_back(e.to);
            }
        }
    }
    let mut prefix = Vec::new();
    let mut at = moved.from;
    while at != 0 {
        let e = parent[&at];
        prefix.push(e);
        at = e.from;
    }
    prefix.reverse();
    let step = |e: &crate::lifecycle::explore::Edge, to: u32| {
        let (act, params) = run.step(e);
        let params: Box<[u8]> = params.into();
        (act, params, run.states.at(to as usize).into_state())
    };
    let mut forged: Vec<_> = prefix.iter().map(|e| step(e, e.to)).collect();
    forged.push(step(&moved, moved.to));
    forged.push(step(&moved, moved.from));
    let start = run.states.at(0).into_state();
    assert_eq!(
        run.system
            .lifted(&start, &forged[..forged.len() - 1])
            .map(|_| ()),
        Ok(()),
        "the path up to the class move lifts"
    );
    let back = forged.len() + 1;
    assert!(
        matches!(
            run.system.lifted(&start, &forged),
            Err(ReplayError::NoMove { state, .. }) if state == back
        ),
        "the move back is state {back}"
    );
}
