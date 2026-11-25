//! `triggers(Ev, UC)` orders the use case after an event: a latched mark per row that the
//! raiser sets and each `<UC>_by_<Ev>` variant reads on the row it is anchored to (#112).

use super::*;
use crate::lifecycle::{Severity, Subject, Trace};

/// Every finding of a run as `(name, severity)`, in report order.
fn findings(d: &Derived) -> Vec<(String, Severity)> {
    d.diags
        .iter()
        .map(|x| (x.diag.name(&[]), x.diag.severity()))
        .collect()
}

fn errors(d: &Derived) -> Vec<String> {
    findings(d)
        .into_iter()
        .filter(|(_, s)| *s == Severity::Error)
        .map(|(n, _)| n)
        .collect()
}

/// What `check --instance-count 2` derives: each view and verdict on its own cone.
fn derive_at_two(src: &str) -> Derived {
    let (report, _) = super::super::derive_states_built(
        &model_from(src),
        &Scope::ALL,
        Bounds::VERIFY,
        Caps::for_instances(1),
    );
    Derived {
        views: report.views,
        diags: report.diags,
    }
}

fn trace_of<'a>(d: &'a Derived, name: &str) -> &'a Trace {
    d.diags
        .iter()
        .find(|x| x.diag.name(&[]) == name)
        .and_then(|x| x.diag.trace())
        .unwrap_or_else(|| panic!("no trace for {name}"))
}

/// #30: the certificate is issued only after the order's execution, so an assertion after
/// the issue reads the executed order.
const CERT: &str = r#"
entity CertificateOrder "依頼" { id: Int @pk  status: Enum(requested, executed) @default(requested) }
entity ClientCertificate "証明書" { id: Int @pk }
usecase RequestCert "依頼"
usecase ExecuteCertIssue "発行実行"
usecase IssueCert "証明書作成"
event EvOrderExecuted "発行実行済"
creates(RequestCert, CertificateOrder)
transitions(ExecuteCertIssue, CertificateOrder.status, requested -> executed)
raises(ExecuteCertIssue, EvOrderExecuted)
triggers(EvOrderExecuted, IssueCert)
creates(IssueCert, ClientCertificate)
relate(ClientCertificate, CertificateOrder, N:1)
after(IssueCert).assert(CertificateOrder.status == executed)
"#;

#[test]
fn a_triggered_use_case_runs_only_after_its_event_on_its_row() {
    let d = derive_at_two(CERT);
    assert_eq!(errors(&d), Vec::<String>::new(), "{:?}", findings(&d));
    let without = CERT.replace("triggers(EvOrderExecuted, IssueCert)\n", "");
    assert_eq!(
        errors(&derive_at_two(&without)),
        ["AfterAssert_IssueCert_0"]
    );
}

/// The raiser's row is not among the handler's rows and no single key links them, so the
/// handler runs unordered and the model says so once; the forbidden state it reaches that
/// way is real.
#[test]
fn a_handler_that_binds_no_row_of_the_event_is_unjoined() {
    let d = derive_at_two(
        r#"
entity Account "口座" { id: Int @pk  status: Enum(pending, verified) @default(pending) }
entity Transfer "振込" { id: Int @pk  status: Enum(sent) @default(sent) }
relate(Transfer, Account, N:1).role(src)
relate(Transfer, Account, N:1).role(dst)
usecase CreateAccount "口座開設"
usecase VerifyAccount "口座確認"
usecase SendTransfer "振込"
event EvVerified "確認済"
creates(CreateAccount, Account)
transitions(VerifyAccount, Account.status, pending -> verified)
raises(VerifyAccount, EvVerified)
triggers(EvVerified, SendTransfer)
creates(SendTransfer, Transfer)
forbidden(Transfer, Account, Transfer.status == sent, Account.status == pending).along(Transfer, Account).role(dst).label("未確認の口座へ振り込まない")
"#,
    );
    let found = findings(&d);
    assert!(
        found.contains(&(
            "UnjoinedTrigger_EvVerified_SendTransfer".to_string(),
            Severity::Info
        )),
        "{found:?}"
    );
    assert_eq!(errors(&d), ["MultiForbidden_Transfer_0"]);
}

/// The event moves the ticket past the state the handler's guard needs, so the handler
/// never runs: a finding the order exposes.
#[test]
fn the_order_shows_a_guard_the_event_leaves_behind() {
    let d = derive_at_two(
        r#"
entity Ticket "票" { id: Int @pk  status: Enum(a, b, c) @default(a) }
entity Note "記録" { id: Int @pk }
relate(Note, Ticket, N:1)
usecase OpenTicket "起票"
usecase Bump "進める"
usecase Audit "監査"
event EvBumped "進めた"
creates(OpenTicket, Ticket)
transitions(Bump, Ticket.status, a -> b)
transitions(Bump, Ticket.status, b -> c)
raises(Bump, EvBumped)
triggers(EvBumped, Audit)
creates(Audit, Note)
condition StillA "まだ a"
holds(StillA, Ticket.status == a)
applies_to(StillA, Audit)
forbidden(Ticket, status == c).label("c に達しない")
"#,
    );
    let found = findings(&d);
    assert!(
        found.contains(&("Runs_Audit".to_string(), Severity::Warning)),
        "{found:?}"
    );
    assert_eq!(errors(&d), ["Forbidden_Ticket_0"]);
}

/// A use case two events start has one variant per event, judged as one use case: an arm
/// one variant takes is taken, and the findings carry the use case's name.
#[test]
fn variants_of_one_use_case_are_judged_together() {
    let arm = |ping: &str| {
        format!(
            r#"
entity Item "品" {{ id: Int @pk  status: Enum(x, y, z, w) @default(x) }}
usecase Make "作る"
usecase Ping "知らせる"
usecase Park "寄せる"
usecase Handle "処理"
event Ev1 "知らせ"
event Ev2 "寄せ"
creates(Make, Item)
transitions(Ping, Item.status, {ping})
raises(Ping, Ev1)
transitions(Park, Item.status, x -> z)
raises(Park, Ev2)
transitions(Handle, Item.status, x -> y).when(IsX)
transitions(Handle, Item.status, z -> w)
triggers(Ev1, Handle)
triggers(Ev2, Handle)
condition IsX "x である"
holds(IsX, Item.status == x)
"#
        )
    };
    let taken = derive_at_two(&arm("x -> x"));
    assert_eq!(
        errors(&taken),
        Vec::<String>::new(),
        "{:?}",
        findings(&taken)
    );
    let names: Vec<String> = findings(&taken).into_iter().map(|(n, _)| n).collect();
    assert!(!names.iter().any(|n| n.starts_with("Arm_")), "{names:?}");
    let untaken = derive_at_two(&arm("x -> z"));
    let found = findings(&untaken);
    assert!(
        found.contains(&("Arm_Handle_x_y".to_string(), Severity::Warning)),
        "{found:?}"
    );
    assert_eq!(errors(&untaken), ["Witness_Item_status_y"]);
}

/// One event of two never reaches the handler, so its variant never fires while the other
/// does: an info naming the use case and the event, not a `NeverRuns`.
#[test]
fn a_variant_no_event_reaches_is_an_info() {
    let d = derive_at_two(
        r#"
entity Item "品" { id: Int @pk  status: Enum(x, z, w) @default(x) }
usecase Make "作る"
usecase Ping "知らせる"
usecase Park "寄せる"
usecase Handle "処理"
event Ev1 "知らせ"
event Ev2 "寄せ"
creates(Make, Item)
transitions(Ping, Item.status, x -> z)
raises(Ping, Ev1)
transitions(Park, Item.status, x -> z).when(Never)
raises(Park, Ev2)
transitions(Handle, Item.status, z -> w)
triggers(Ev1, Handle)
triggers(Ev2, Handle)
condition Never "w である"
holds(Never, Item.status == w)
"#,
    );
    let found = findings(&d);
    assert!(
        found.contains(&("TriggerNeverRuns_Handle_Ev2".to_string(), Severity::Info)),
        "{found:?}"
    );
    assert!(
        found.contains(&("Runs_Park".to_string(), Severity::Warning)),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|(n, _)| n.starts_with("Runs_Handle")),
        "{found:?}"
    );
}

/// The second event comes four steps after the first, so a cap that stops the cone between
/// them leaves its variant undecided under its own name rather than never run.
#[test]
fn a_variant_a_capped_cone_never_reached_is_no_info() {
    let src = r#"
entity Item "品" { id: Int @pk  status: Enum(x, a, b, c, d, z, w) @default(x) }
usecase Make "作る"
usecase Ping "知らせる"
usecase Walk "進める"
usecase Park "寄せる"
usecase Handle "処理"
event Ev1 "知らせ"
event Ev2 "寄せ"
creates(Make, Item)
transitions(Ping, Item.status, x -> z)
raises(Ping, Ev1)
transitions(Walk, Item.status, x -> a)
transitions(Walk, Item.status, a -> b)
transitions(Walk, Item.status, b -> c)
transitions(Walk, Item.status, c -> d)
transitions(Park, Item.status, d -> z)
raises(Park, Ev2)
transitions(Handle, Item.status, z -> w)
triggers(Ev1, Handle)
triggers(Ev2, Handle)
"#;
    let ts = TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts;
    let targets = ts.targets();
    for (states, complete) in [(6, false), (Caps::for_instances(1).states, true)] {
        let x = ts.explore_targets(&targets, Caps::new(Caps::for_instances(1).patterns, states));
        assert_eq!(x.cone_of(targets[0]).run.complete, complete);
        let names: Vec<String> = x.verdicts().iter().map(|d| d.diag.name(&[])).collect();
        assert!(
            !names.iter().any(|n| n.starts_with("TriggerNeverRuns")),
            "{states}: {names:?}"
        );
        let undecided: Vec<&Subject> = x.undecided().iter().map(|u| &u.subject).collect();
        let open = Subject::Runs {
            action: "Handle_by_Ev2".to_string(),
        };
        assert_eq!(
            undecided.contains(&&open),
            !complete,
            "{states}: {undecided:?}"
        );
    }
}

/// The event's raisers bind no entity in common, so no row can carry its mark: the handler
/// runs free and the model says so.
#[test]
fn a_trigger_whose_raisers_share_no_entity_is_unanchored() {
    let d = derive_at_two(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Invoice "請求" { id: Int @pk  status: Enum(open, paid) @default(open) }
entity Report "報告" { id: Int @pk }
usecase Place "注文"
usecase Bill "請求"
usecase PayOrder "注文の支払"
usecase PayInvoice "請求の支払"
usecase FileReport "報告"
event EvPaid "支払済"
creates(Place, Order)
creates(Bill, Invoice)
transitions(PayOrder, Order.status, open -> paid)
transitions(PayInvoice, Invoice.status, open -> paid)
raises(PayOrder, EvPaid)
raises(PayInvoice, EvPaid)
triggers(EvPaid, FileReport)
creates(FileReport, Report)
"#,
    );
    let found = findings(&d);
    assert!(
        found.contains(&(
            "UnanchoredTrigger_EvPaid_FileReport".to_string(),
            Severity::Info
        )),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|(n, _)| n == "Runs_FileReport"),
        "{found:?}"
    );
}

/// A cone that holds the handler but not the raiser's writes leaves the mark out and runs
/// the handler unordered; its trace says which step ran without its event.
#[test]
fn a_cone_without_the_raiser_notes_the_unordered_step_on_its_trace() {
    let d = derive_at_two(
        r#"
entity Doc "文書" { id: Int @pk  status: Enum(draft, final) @default(draft)  seen: Bool @default(false) }
usecase Write "作成"
usecase Review "確認"
usecase Finalize "確定"
event EvReviewed "確認済"
creates(Write, Doc)
updates(Review, Doc)
sets(Review, Doc.seen == true)
raises(Review, EvReviewed)
triggers(EvReviewed, Finalize)
transitions(Finalize, Doc.status, draft -> final)
forbidden(Doc, status == final).label("確定しない")
"#,
    );
    assert_eq!(errors(&d), ["Forbidden_Doc_0"]);
    let trace = trace_of(&d, "Forbidden_Doc_0");
    assert_eq!(
        trace.free_triggers,
        [("Finalize".to_string(), "EvReviewed".to_string())]
    );
    let d = derive_at_two(
        r#"
entity Doc "文書" { id: Int @pk  status: Enum(draft, final) @default(draft)  seen: Bool @default(false) }
usecase Write "作成"
usecase Review "確認"
usecase Finalize "確定"
event EvReviewed "確認済"
creates(Write, Doc)
updates(Review, Doc)
sets(Review, Doc.seen == true)
raises(Review, EvReviewed)
triggers(EvReviewed, Finalize)
transitions(Finalize, Doc.status, draft -> final)
forbidden(Doc, status == final, seen == true).label("確認したら確定しない")
"#,
    );
    assert_eq!(errors(&d), ["Forbidden_Doc_0"]);
    assert_eq!(trace_of(&d, "Forbidden_Doc_0").free_triggers, []);
}

/// #206: the cone widened with the left-out mark holds the raiser, and a trace that runs the
/// handler before its event does not run on it or on the whole system, while a trace found on
/// the widened cone runs on the whole system step by step.
#[test]
fn a_trace_without_its_event_does_not_run_on_the_ordered_cone() {
    let ts = TransitionSystem::build(
        &model_from(
            r#"
entity Doc "文書" { id: Int @pk  status: Enum(draft, final) @default(draft)  seen: Bool @default(false) }
usecase Write "作成"
usecase Review "確認"
usecase Finalize "確定"
event EvReviewed "確認済"
creates(Write, Doc)
updates(Review, Doc)
sets(Review, Doc.seen == true)
raises(Review, EvReviewed)
triggers(EvReviewed, Finalize)
transitions(Finalize, Doc.status, draft -> final)
forbidden(Doc, status == final).label("確定しない")
"#,
        ),
        Bounds::VERIFY,
        &Scope::ALL,
    )
    .ts;
    let target = crate::lifecycle::Target::Invariant(0);
    let trace = |cone: &crate::lifecycle::Cone| {
        let run = ts.restrict_to(&cone.vars, &cone.keep).run(Caps::VERIFY);
        let mut d = run
            .diags
            .iter()
            .find(|x| x.diag.name(&[]) == "Forbidden_Doc_0")
            .expect("a violation")
            .clone();
        let trace = d.diag.trace_mut().expect("a trace");
        ts.annotate(cone, trace);
        trace.clone()
    };
    let cone = ts.target_cone(target);
    let ordered = ts.ordered_cone(target);
    assert!(ordered.vars.is_superset(&cone.vars) && ordered.vars != cone.vars);
    let unordered = trace(&cone);
    assert_eq!(unordered.free_triggers.len(), 1, "{unordered:?}");
    let widened = ts.restrict_to(&ordered.vars, &ordered.keep);
    assert!(!widened.runs_trace(&unordered));
    assert!(!ts.runs_trace(&unordered));
    let in_order = trace(&ordered);
    assert!(in_order.free_triggers.is_empty(), "{in_order:?}");
    assert_eq!(in_order.steps.len(), 3, "write, review, finalize");
    assert!(widened.runs_trace(&in_order));
    assert!(ts.runs_trace(&in_order), "the whole system runs it too");
}

/// A counted edge (#220): a box shipped twice, received once per shipment.
const COUNTED: &str = r#"
entity Box "箱" { id: Int @pk  sent: Enum(none, one, two) @default(none)  got: Enum(none, one, two) @default(none) }
usecase Pack "箱詰め"
usecase Ship "発送"
usecase Receive "入庫"
event EvShipped "発送済"
creates(Pack, Box)
transitions(Ship, Box.sent, none -> one)
transitions(Ship, Box.sent, one -> two)
raises(Ship, EvShipped)
triggers(EvShipped, Receive){EDGE}
transitions(Receive, Box.got, none -> one)
transitions(Receive, Box.got, one -> two)
forbidden(Box, sent == one, got == two)
"#;

/// The counter replaces the latch on its edge: the raiser adds one, the handler reads `!= 0`
/// and takes one, and the overflow rule follows the model's rules. Without `.counted` the
/// edge keeps the latch, and no counter or overflow rule exists.
#[test]
fn a_counted_edge_has_a_counter_in_place_of_the_latch() {
    let latch = model_from(&COUNTED.replace("{EDGE}", ""));
    let ts = TransitionSystem::build(&latch, Bounds::VERIFY, &Scope::ALL).ts;
    let names: Vec<String> = ts.var_ids().map(|v| ts.var_name(v)).collect();
    assert!(
        names.contains(&"Box._raised_EvShipped".to_string()),
        "{names:?}"
    );
    assert!(!names.iter().any(|n| n.contains("_pending_")), "{names:?}");
    assert!(!ts
        .invariants()
        .iter()
        .any(|i| i.name.starts_with("EventOverflow")));

    let counted = model_from(&COUNTED.replace("{EDGE}", ".counted(2)"));
    let ts = TransitionSystem::build(&counted, Bounds::VERIFY, &Scope::ALL).ts;
    let names: Vec<String> = ts.var_ids().map(|v| ts.var_name(v)).collect();
    assert!(!names.iter().any(|n| n.contains("_raised_")), "{names:?}");
    let counter = ts
        .var_ids()
        .find(|v| ts.var_name(*v) == "Box._pending_EvShipped_Receive")
        .expect("the counter");
    assert_eq!(
        ts.var(counter).domain,
        crate::lifecycle::Domain::Enum(vec!["0".into(), "1".into(), "2".into(), "over".into()])
    );
    let rules: Vec<&str> = ts.invariants().iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        rules,
        ["Forbidden_Box_0", "EventOverflow_Box_EvShipped_Receive"]
    );
    let d = derive_at_two(&COUNTED.replace("{EDGE}", ".counted(2)"));
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let d = derive_at_two(&COUNTED.replace("{EDGE}", ""));
    assert_eq!(
        errors(&d),
        ["Forbidden_Box_0"],
        "the latch runs Receive twice"
    );
}

/// `.counted(1)` on three shipments: the third raise finds one pending and breaks the
/// overflow rule, with a trace whose last step is the raise.
#[test]
fn a_raise_past_k_breaks_the_overflow_rule() {
    let d = derive_at_two(&COUNTED.replace("{EDGE}", ".counted(1)"));
    assert!(
        errors(&d).contains(&"EventOverflow_Box_EvShipped_Receive".to_string()),
        "{:?}",
        errors(&d)
    );
    let trace = trace_of(&d, "EventOverflow_Box_EvShipped_Receive");
    assert_eq!(
        trace.steps.last().map(|s| s.action.as_str()),
        Some("Ship"),
        "the raise overflows"
    );
}

/// A counted edge the order does not track is the error `UncountedTrigger`, naming why: an
/// event an external system raises, a handler that raises its own event.
#[test]
fn a_counted_edge_the_order_does_not_track_is_an_error() {
    let free = r#"
entity Box "箱" { id: Int @pk  got: Enum(none, one) @default(none) }
extsystem Carrier "運送"
usecase Pack "箱詰め"
usecase Receive "入庫"
event EvShipped "発送済"
creates(Pack, Box)
raises(Carrier, EvShipped)
triggers(EvShipped, Receive).counted(2)
transitions(Receive, Box.got, none -> one)
"#;
    let own = r#"
entity Box "箱" { id: Int @pk  got: Enum(none, one, two) @default(none) }
usecase Pack "箱詰め"
usecase Start "開始"
usecase Receive "入庫"
event EvShipped "発送済"
creates(Pack, Box)
transitions(Start, Box.got, none -> one)
raises(Start, EvShipped)
raises(Receive, EvShipped)
triggers(EvShipped, Receive).counted(2)
transitions(Receive, Box.got, one -> two)
"#;
    for (src, why) in [
        (free, "EvShipped is free"),
        (own, "Receive raises EvShipped itself"),
    ] {
        let d = derive_at_two(src);
        let found = d
            .diags
            .iter()
            .find(|x| matches!(x.diag, StateDiag::UncountedTrigger { .. }))
            .unwrap_or_else(|| panic!("no UncountedTrigger: {:?}", errors(&d)));
        assert_eq!(found.diag.severity(), Severity::Error);
        let text = found.diag.to_string();
        assert!(text.contains(why), "{text}");
    }
}

/// `.counted` takes one whole number from 1 to 16, once, on an edge from an event to a use
/// case; anything else is `CountedTriggerInvalid`.
#[test]
fn a_counted_argument_out_of_shape_is_rejected() {
    let base = r#"
entity Box "箱" { id: Int @pk  got: Enum(none, one) @default(none) }
buc Flow "流れ"
timer Daily "毎日"
usecase Pack "箱詰め"
usecase Receive "入庫"
event EvShipped "発送済"
creates(Pack, Box)
raises(Pack, EvShipped)
transitions(Receive, Box.got, none -> one)
"#;
    for line in [
        "triggers(EvShipped, Receive).counted(0)",
        "triggers(EvShipped, Receive).counted(17)",
        "triggers(EvShipped, Receive).counted(two)",
        "triggers(EvShipped, Receive).counted()",
        "triggers(EvShipped, Receive).counted(1).counted(2)",
        "triggers(EvShipped, Flow).counted(1)",
        "triggers(Daily, Receive).counted(1)",
    ] {
        let (ast, errors) = parse(&format!("{base}{line}\n"));
        assert!(errors.is_empty(), "{line}: {errors:?}");
        let (_, diags) = build_model(&ast);
        assert!(
            diags.iter().any(|d| !d.is_warning
                && matches!(
                    d.error,
                    crate::diagnostics::RdraError::CountedTriggerInvalid { .. }
                )),
            "{line}: {:?}",
            diags
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
    }
    let (ast, _) = parse(&format!("{base}triggers(EvShipped, Receive).counted(16)\n"));
    let (_, diags) = build_model(&ast);
    assert!(
        !diags.iter().any(|d| matches!(
            d.error,
            crate::diagnostics::RdraError::CountedTriggerInvalid { .. }
        )),
        "16 is allowed"
    );
}

/// Mixed edges: one event with a counted and a latch handler, and one handler with a counted
/// and a latch event. The latch handler keeps the shared latch; each `<UC>_by_<Ev>` variant
/// reads its own edge's mark, and only the counted variant takes one off its counter.
#[test]
fn counted_and_latch_edges_mix_by_edge() {
    let src = r#"
entity Box "箱" { id: Int @pk  sent: Enum(none, one) @default(none)  got: Enum(none, one, two) @default(none)  log: Enum(empty, logged) @default(empty) }
usecase Pack "箱詰め"
usecase Ship "発送"
usecase Return "返品"
usecase Receive "入庫"
usecase Log "記録"
event EvShipped "発送済"
event EvReturned "返品済"
creates(Pack, Box)
transitions(Ship, Box.sent, none -> one)
raises(Ship, EvShipped)
transitions(Return, Box.sent, one -> none)
raises(Return, EvReturned)
triggers(EvShipped, Receive).counted(1)
triggers(EvReturned, Receive)
triggers(EvShipped, Log)
transitions(Receive, Box.got, none -> one)
transitions(Receive, Box.got, one -> two)
transitions(Log, Box.log, empty -> logged)
"#;
    let model = model_from(src);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let var = |name: &str| {
        ts.var_ids()
            .find(|v| ts.var_name(*v) == name)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let pending = var("Box._pending_EvShipped_Receive");
    let shipped = var("Box._raised_EvShipped");
    let returned = var("Box._raised_EvReturned");
    let action = |name: &str| {
        ts.actions()
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let reads = |a: &crate::lifecycle::Action, v| {
        a.guards.iter().any(|g| {
            matches!(&g.pred, crate::lifecycle::Pred::Cmp(crate::lifecycle::Term::Slot { var, .. }, ..) if *var == v)
        })
    };
    let writes = |a: &crate::lifecycle::Action, v| {
        a.effects.iter().any(|e| {
            matches!(e, crate::lifecycle::Effect::Transition { var, .. } | crate::lifecycle::Effect::Assign { var, .. } if *var == v)
        })
    };
    let by_shipped = action("Receive_by_EvShipped");
    let by_returned = action("Receive_by_EvReturned");
    assert!(reads(by_shipped, pending) && writes(by_shipped, pending));
    assert!(!reads(by_shipped, shipped));
    assert!(reads(by_returned, returned) && !writes(by_returned, returned));
    assert!(!reads(by_returned, pending) && !writes(by_returned, pending));
    assert!(reads(action("Log"), shipped) && !writes(action("Log"), shipped));
    let ship = action("Ship");
    assert!(
        writes(ship, pending) && writes(ship, shipped),
        "Ship counts and latches"
    );
    // Ship, Return, Ship again before Receive: two raises against k = 1.
    let d = derive_at_two(src);
    assert_eq!(errors(&d), ["EventOverflow_Box_EvShipped_Receive"]);
}
