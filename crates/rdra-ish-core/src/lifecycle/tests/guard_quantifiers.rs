//! `holds(..).has(..)` and `.none(..)`: a condition guard over the related rows of its entities.

use super::conditions::{clinic, reaches_paid};
use super::*;
use crate::lifecycle::InvKind;

/// The certificate model of issue 2: revoking needs no active assignment.
fn certificates(revoke_guard: &str, extra: &str) -> String {
    format!(
        r#"
entity ClientCertificate "証明書" {{ id: Int @pk  status: Enum(active, revoked) @default(active) }}
entity TerminalCertAssignment "端末割当" {{ id: Int @pk  status: Enum(active, ended) @default(active) }}
usecase Issue "発行"
usecase Assign "割当"
usecase End "割当終了"
usecase Revoke "失効"
creates(Issue, ClientCertificate)
creates(Assign, TerminalCertAssignment)
transitions(TerminalCertAssignment.status, End, active -> ended)
transitions(ClientCertificate.status, Revoke, active -> revoked)
relate(TerminalCertAssignment, ClientCertificate, N:1)
condition CertActive "有効な証明書にだけ割り当てる"
holds(CertActive, ClientCertificate.status == active)
applies_to(CertActive, Assign)
{revoke_guard}
when(ClientCertificate, status == revoked).none(TerminalCertAssignment.status == active)
{extra}
"#
    )
}

const NO_ACTIVE_ASSIGNMENT: &str = r#"condition NoActiveAssignment "有効な割当がない証明書だけ失効できる"
holds(NoActiveAssignment, ClientCertificate.status == active).none(TerminalCertAssignment.status == active)
applies_to(NoActiveAssignment, Revoke)"#;

fn quantifier_violated(d: &Derived) -> bool {
    d.has("ClientCertificate", |x| {
        matches!(
            x,
            StateDiag::ConstraintViolated {
                kind: InvKind::Quantifier,
                ..
            }
        )
    })
}

#[test]
fn a_none_guard_keeps_the_revoke_from_running_while_a_linked_row_matches() {
    let without = derive_src(&certificates("", ""));
    assert!(quantifier_violated(&without));
    let guarded = derive_src(&certificates(NO_ACTIVE_ASSIGNMENT, ""));
    assert!(
        !quantifier_violated(&guarded),
        "{:?}",
        guarded.entity("ClientCertificate").diagnostics
    );
    let two = derive_scoped(
        &model_from(&certificates(NO_ACTIVE_ASSIGNMENT, "")),
        Bounds::VERIFY,
        &Scope::ALL,
    );
    assert!(!quantifier_violated(&two), "{:?}", two.diags);
}

#[test]
fn a_none_guard_still_lets_the_use_case_run_once_the_linked_rows_end() {
    let d = derive_src(&certificates(NO_ACTIVE_ASSIGNMENT, ""));
    assert!(d
        .rows("ClientCertificate")
        .contains(&"status=revoked".to_string()));
}

#[test]
fn a_has_guard_needs_some_linked_row_to_match() {
    let end_when = |status: &str| {
        certificates(
            &format!(
                "condition Linked \"有効な証明書の割当だけ終了できる\"\nholds(Linked, TerminalCertAssignment.status == active).has(ClientCertificate.status == {status})\napplies_to(Linked, End)"
            ),
            "",
        )
    };
    let ended = |d: &Derived| {
        d.rows("TerminalCertAssignment")
            .contains(&"status=ended".to_string())
    };
    assert!(ended(&derive_src(&end_when("active"))));
    let revoked_only = end_when("revoked").replace(
        "transitions(ClientCertificate.status, Revoke, active -> revoked)",
        "",
    );
    assert!(!ended(&derive_src(&revoked_only)));
}

#[test]
fn a_guard_over_an_unrelated_entity_quantifies_every_row() {
    let guarded = |quantifier: &str, occupy: bool| {
        let model = clinic(
            &format!("holds(Ready, Payment.status == open).{quantifier}(Room.state == busy)"),
            "",
        );
        if occupy {
            model
        } else {
            model.replace("transitions(Room.state, Occupy, free -> busy)", "")
        }
    };
    assert!(reaches_paid(&derive_src(&guarded("none", true))));
    assert!(reaches_paid(&derive_src(&guarded("none", false))));
    assert!(reaches_paid(&derive_src(&guarded("has", true))));
    assert!(!reaches_paid(&derive_src(&guarded("has", false))));
}
