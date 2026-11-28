//! The reach certificate of a safety `ok` (`certificate.rs`, #594 T15): a complete run's states
//! check against the initial states, the step relation and the invariants, and a certificate with
//! a state left out, or of a model whose rule fails, does not.

use super::*;
use crate::lifecycle::{CertFault, ReachCertificate};

fn whole(rel: &str) -> TransitionSystem {
    TransitionSystem::build(&sample_model(rel), Bounds::VERIFY, &Scope::ALL).ts
}

fn certificate(ts: &TransitionSystem) -> ReachCertificate {
    let run = ts.clone().run(Caps::new(usize::MAX, 1_000_000));
    ts.reach_certificate(&run).expect("a complete run")
}

#[test]
fn a_complete_run_certifies_its_rules_and_reads_back_as_written() {
    let ts = whole("samples/formal-verification/order.rdra");
    let cert = certificate(&ts);
    let mut bytes = Vec::new();
    cert.write(&mut bytes).unwrap();
    let read = ReachCertificate::read(&bytes[..]).unwrap();
    assert_eq!(read, cert);
    let stats = ts.check_reach_certificate(&read).expect("the rules hold");
    assert_eq!(stats.states, cert.states.len());
    assert!(stats.moves > 0 && stats.initial > 0 && stats.invariants > 0);
}

#[test]
fn a_certificate_with_a_state_left_out_fails() {
    let ts = whole("samples/formal-verification/order.rdra");
    let mut cert = certificate(&ts);
    for drop in [cert.states.len() - 1, 0] {
        let mut short = cert.clone();
        short.states.remove(drop);
        assert!(
            matches!(
                ts.check_reach_certificate(&short),
                Err(CertFault::SuccessorMissing { .. } | CertFault::InitialMissing { .. })
            ),
            "state {drop} left out"
        );
    }
    cert.states.clear();
    assert!(matches!(
        ts.check_reach_certificate(&cert),
        Err(CertFault::InitialMissing { initial: 0 })
    ));
}

#[test]
fn the_certificate_of_a_broken_rule_names_it() {
    let ts = whole("samples/formal-verification-fail/order.rdra");
    let cert = certificate(&ts);
    match ts.check_reach_certificate(&cert) {
        Err(CertFault::Violated { invariant, .. }) => assert_eq!(invariant, "Inv_Order_0"),
        other => panic!("expected Inv_Order_0 violated, got {other:?}"),
    }
}

#[test]
fn a_stopped_run_has_no_certificate() {
    let ts = whole("samples/formal-verification/order.rdra");
    let run = ts.clone().run(Caps::new(usize::MAX, 2));
    assert_eq!(ts.reach_certificate(&run), Err(CertFault::Incomplete));
}
