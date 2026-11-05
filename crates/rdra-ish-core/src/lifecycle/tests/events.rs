//! `triggers(Ev, UC)` orders the use case after an event: a latched mark per row that the
//! raiser sets and each `<UC>_by_<Ev>` variant reads on the row it is anchored to (#112).

use super::*;
use crate::lifecycle::{Severity, Trace};

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
        Caps::STATES,
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
/// them leaves its variant undecided rather than never run.
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
    for (states, complete) in [(6, false), (Caps::DEFAULT_STATES, true)] {
        let x = ts.explore_targets(&targets, Caps::new(Caps::DEFAULT_PATTERNS, states));
        assert_eq!(x.cone_of(targets[0]).run.complete, complete);
        let names: Vec<String> = x.verdicts().iter().map(|d| d.diag.name(&[])).collect();
        assert!(
            !names.iter().any(|n| n.starts_with("TriggerNeverRuns")),
            "{states}: {names:?}"
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
