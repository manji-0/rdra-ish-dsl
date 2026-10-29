//! Every finding has a name that `error[<name>]:` can carry and the sample headers can match.

use crate::lifecycle::trace::{Inst, Trace};
use crate::lifecycle::types::{ActionOrigin, InvKind, InvText, ModelRef, Site};
use crate::lifecycle::{Blocked, Exit, StateDiag};

/// One value of each variant, with key fields that hold characters a name cannot.
fn one_of_each() -> Vec<StateDiag> {
    let site = || Site::bare(ModelRef::EntityInvariant(0));
    let all = vec![
        StateDiag::UnreachableEnumVariant {
            column: "status".into(),
            variant: "paid".into(),
        },
        StateDiag::ConflictingEffects {
            usecase: "Pay".into(),
            column: "status".into(),
            sites: vec![],
        },
        StateDiag::DoubleModeledEnum {
            column: "status".into(),
            usecase: "Pay".into(),
            site: site(),
        },
        StateDiag::NoCreationPath,
        StateDiag::PreExistingChildOfCreatedParent {
            parent: "Order".into(),
            site: site(),
        },
        StateDiag::MinCardOfPreExistingParent {
            child: "Line".into(),
            site: site(),
        },
        StateDiag::MinCardNeedsCreate {
            usecase: "Place".into(),
            child: "Line".into(),
            site: site(),
        },
        StateDiag::PatternCapReached { cap: 1, bound: 4 },
        StateDiag::StateCapReached { cap: 1 },
        StateDiag::ConstraintViolated {
            name: "Forbidden_Order_0".into(),
            kind: InvKind::Forbidden,
            text: InvText::Conditions("status=paid".into()),
            pattern_desc: "status=paid".into(),
            site: site(),
            trace: Trace::default(),
        },
        StateDiag::AfterAssertViolated {
            name: "AfterAssert_Pay_0".into(),
            anchor: "Pay".into(),
            requireds: "Order.status=paid".into(),
            actual: "Order(status=pending)".into(),
            site: site(),
            trace: Trace::default(),
        },
        StateDiag::NoWitness {
            entity: "Order".into(),
            column: "status".into(),
            value: "paid".into(),
            into: vec![],
        },
        StateDiag::NeverRuns {
            action: "Pay_2".into(),
            origin: ActionOrigin::Clock,
            site: site(),
            why: Blocked::Together,
        },
        StateDiag::Deadlock {
            entity: "Order".into(),
            column: "status".into(),
            value: "paid".into(),
            at: Inst {
                entity: "Order".into(),
                slot: 1,
            },
            exits: vec![Exit {
                action: "Ship".into(),
                origin: ActionOrigin::Clock,
                to: Some("shipped".into()),
                site: site(),
                why: Blocked::Together,
            }],
            trace: Trace::default(),
        },
        StateDiag::AssertOnInertUseCase {
            anchor: "Noop".into(),
            site: site(),
        },
        StateDiag::IntLiteralOutOfBounds {
            value: -7,
            max: 5,
            site: site(),
        },
        StateDiag::UnbindableScope {
            constraint: "cross forbidden(Order, Payment)".into(),
            reason: "no relate path".into(),
            site: site(),
        },
        StateDiag::UnbindableCondition {
            condition: "Settled".into(),
            usecase: "Pay".into(),
            reason: "nothing to read".into(),
            site: site(),
        },
        StateDiag::ContradictoryDrivenComparison {
            usecase: "Pay".into(),
            proposition: "Order.amount > 3".into(),
            driven: true,
            sites: vec![],
        },
        StateDiag::UnwrittenConstrainedVar {
            column: "delivered_at".into(),
            site: site(),
        },
        StateDiag::UnsupportedComparison {
            comparison: "Order.note < 3".into(),
            reason: "not Int".into(),
            site: site(),
        },
        StateDiag::DerivedColumnRead {
            column: "Audit.score".into(),
            site: site(),
        },
        StateDiag::UnresolvedPropertyAtom {
            property: "Eventually Paid".into(),
            column: "status".into(),
            site: site(),
        },
    ];
    // Fails to compile when a variant is added without a value above.
    for d in &all {
        match d {
            StateDiag::UnreachableEnumVariant { .. }
            | StateDiag::ConflictingEffects { .. }
            | StateDiag::DoubleModeledEnum { .. }
            | StateDiag::NoCreationPath
            | StateDiag::PreExistingChildOfCreatedParent { .. }
            | StateDiag::MinCardOfPreExistingParent { .. }
            | StateDiag::MinCardNeedsCreate { .. }
            | StateDiag::PatternCapReached { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::ConstraintViolated { .. }
            | StateDiag::AfterAssertViolated { .. }
            | StateDiag::NoWitness { .. }
            | StateDiag::NeverRuns { .. }
            | StateDiag::Deadlock { .. }
            | StateDiag::AssertOnInertUseCase { .. }
            | StateDiag::IntLiteralOutOfBounds { .. }
            | StateDiag::UnbindableScope { .. }
            | StateDiag::UnbindableCondition { .. }
            | StateDiag::ContradictoryDrivenComparison { .. }
            | StateDiag::UnwrittenConstrainedVar { .. }
            | StateDiag::UnsupportedComparison { .. }
            | StateDiag::DerivedColumnRead { .. }
            | StateDiag::UnresolvedPropertyAtom { .. } => {}
        }
    }
    all
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[test]
fn every_variant_has_a_name_of_word_characters() {
    for d in one_of_each() {
        for entities in [&[][..], &["Order"][..], &["Order", "Payment line"][..]] {
            let name = d.name(entities);
            assert!(is_name(&name), "{name:?} from {d:?}");
        }
    }
}

#[test]
fn names_keep_the_checks_own_names() {
    let names: Vec<String> = one_of_each().iter().map(|d| d.name(&["Order"])).collect();
    for kept in [
        "Forbidden_Order_0",
        "AfterAssert_Pay_0",
        "Witness_Order_status_paid",
        "Runs_Pay_2",
        "Deadlock_Order_status_paid",
        "DerivedColumnRead_Audit_score",
        "ContradictoryDrivenComparison_Order_Pay_Order_amount___3",
        "StateCapReached_Order",
    ] {
        assert!(names.iter().any(|n| n == kept), "{kept} not in {names:?}");
    }
}

#[test]
fn one_finding_on_two_entity_views_keeps_two_names() {
    let cap = StateDiag::StateCapReached { cap: 1 };
    assert_eq!(cap.name(&["Order"]), "StateCapReached_Order");
    assert_eq!(cap.name(&["Payment"]), "StateCapReached_Payment");
    assert_eq!(StateDiag::NoCreationPath.name(&[]), "NoCreationPath");
}
