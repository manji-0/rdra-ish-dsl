//! Every finding has a name that `error[<name>]:` can carry and the sample headers can match.

use super::model_from;
use crate::lifecycle::trace::{Inst, Trace};
use crate::lifecycle::types::{ActionOrigin, InvKind, InvText, LtlKind, ModelRef, Site};
use crate::lifecycle::{
    Blocked, Bounds, CapGrowth, Exit, Loop, Scope, StateDiag, TransitionSystem,
};

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
        StateDiag::StateCapReached {
            cap: 1,
            growth: CapGrowth::default(),
        },
        StateDiag::ConstraintViolated {
            name: "Forbidden_Order_0".into(),
            kind: InvKind::Forbidden,
            text: InvText::Conditions("status=paid".into()),
            label: None,
            pattern_desc: "status=paid".into(),
            site: site(),
            trace: Trace::default(),
        },
        StateDiag::AfterAssertViolated {
            name: "AfterAssert_Pay_0".into(),
            anchor: "Pay".into(),
            requireds: "Order.status=paid".into(),
            label: None,
            actual: "Order(status=pending)".into(),
            site: site(),
            trace: Trace::default(),
        },
        StateDiag::PropertyViolated {
            name: "NeverShipped".into(),
            kind: LtlKind::Always,
            instance: vec![Inst {
                entity: "Order".into(),
                slot: 1,
            }],
            witness: 3,
            lasso: Loop::Stutter,
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
            unlinked: vec![],
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
        StateDiag::FairOnInertUseCase {
            property: "Ends".into(),
            usecase: "Noop".into(),
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
        StateDiag::UnboundCopySource {
            usecase: "Register".into(),
            column: "Permit.kind".into(),
            source: "Line.kind".into(),
            reason: "several Line rows".into(),
            site: site(),
        },
        StateDiag::AmbiguousOwnerLink {
            statement: ".none(Permit)".into(),
            a: "Application".into(),
            b: "Permit".into(),
            roles: "application, renews".into(),
            rewrite: ".none(Permit).role(application)".into(),
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
        StateDiag::ArmNeverTaken {
            action: "Decide".into(),
            column: "Doc.status".into(),
            from: "searched".into(),
            to: "file".into(),
            conditions: vec!["Ready".into()],
            site: site(),
        },
        StateDiag::ValueNeverChosen {
            action: "Draft".into(),
            column: "Notice.scope".into(),
            value: "free".into(),
            conditions: vec!["NotFinal".into()],
            site: site(),
        },
        StateDiag::UnjoinedGuardRead {
            condition: "SettlementSigned".into(),
            usecase: "CompleteClaim".into(),
            entity: "Settlement".into(),
            fix: "add `.along(Claim, ClaimCase, Settlement)`".into(),
            site: site(),
        },
        StateDiag::UnboundAfterRead {
            anchor: "Pay".into(),
            entity: "Desk".into(),
            site: site(),
        },
        StateDiag::DeleteLeavesChildren {
            usecase: "DeleteParent".into(),
            parent: "Parent".into(),
            child: "Child".into(),
            site: site(),
        },
        StateDiag::GuardReadsLaterStep {
            condition: "Captured".into(),
            usecase: "Pay".into(),
            entity: "Payment".into(),
            step: 2,
            apis: vec!["PaymentApi".into()],
            site: site(),
        },
        StateDiag::DrivenComparisonColumnWritten {
            usecase: "Restock".into(),
            column: "stock".into(),
            proposition: "Stock.stock < selling".into(),
            drivers: vec!["Sell".into()],
            sites: vec![],
        },
        StateDiag::DefaultOnVariationIgnored {
            column: "method".into(),
            value: "cash".into(),
            created: true,
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
            | StateDiag::PropertyViolated { .. }
            | StateDiag::NoWitness { .. }
            | StateDiag::NeverRuns { .. }
            | StateDiag::ArmNeverTaken { .. }
            | StateDiag::ValueNeverChosen { .. }
            | StateDiag::Deadlock { .. }
            | StateDiag::AssertOnInertUseCase { .. }
            | StateDiag::FairOnInertUseCase { .. }
            | StateDiag::IntLiteralOutOfBounds { .. }
            | StateDiag::UnbindableScope { .. }
            | StateDiag::UnbindableCondition { .. }
            | StateDiag::UnboundCopySource { .. }
            | StateDiag::AmbiguousOwnerLink { .. }
            | StateDiag::ContradictoryDrivenComparison { .. }
            | StateDiag::UnwrittenConstrainedVar { .. }
            | StateDiag::UnsupportedComparison { .. }
            | StateDiag::DerivedColumnRead { .. }
            | StateDiag::UnresolvedPropertyAtom { .. }
            | StateDiag::UnjoinedGuardRead { .. }
            | StateDiag::UnboundAfterRead { .. }
            | StateDiag::DeleteLeavesChildren { .. }
            | StateDiag::GuardReadsLaterStep { .. }
            | StateDiag::DrivenComparisonColumnWritten { .. }
            | StateDiag::DefaultOnVariationIgnored { .. } => {}
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
        "NeverShipped",
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
    let cap = StateDiag::StateCapReached {
        cap: 1,
        growth: CapGrowth::default(),
    };
    assert_eq!(cap.name(&["Order"]), "StateCapReached_Order");
    assert_eq!(cap.name(&["Payment"]), "StateCapReached_Payment");
    assert_eq!(StateDiag::NoCreationPath.name(&[]), "NoCreationPath");
}

fn check_names(src: &str) -> Vec<String> {
    let ts = TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts;
    ts.invariants()
        .iter()
        .map(|i| i.name.clone())
        .chain(ts.after_asserts().iter().map(|a| a.name.clone()))
        .collect()
}

/// Ids may hold `_`, so `Check_Cart_Line_qty_0` is both `Cart.Line_qty`'s check and
/// `Cart_Line.qty`'s, and `MinCard_A_B_C` both `A_B`'s key to `C` and `A`'s to `B_C`: the
/// later check takes the next free name, and each finding names one rule.
#[test]
fn a_check_name_an_earlier_check_spells_takes_the_next_free_name() {
    let src = r#"
entity Cart "カート" { id: Int @pk  Line_qty: Int @default("1") @check(Line_qty >= 1) }
entity Cart_Line "明細" { id: Int @pk  qty: Int @default("1") @check(qty >= 1) }
entity A "A" { id: Int @pk }
entity A_B "AB" { id: Int @pk }
entity B_C "BC" { id: Int @pk }
entity C "C" { id: Int @pk }
usecase Make "作成"
creates(Make, Cart)
creates(Make, Cart_Line)
creates(Make, A)
creates(Make, A_B)
creates(Make, B_C)
creates(Make, C)
sets(Make, Cart.Line_qty == 1)
sets(Make, Cart_Line.qty == 1)
relate(A_B, C, 1..*:1)
relate(A, B_C, 1..*:1)
"#;
    let mut names = check_names(src);
    names.sort();
    assert_eq!(
        names,
        [
            "Check_Cart_Line_qty_0",
            "Check_Cart_Line_qty_0_2",
            "MinCard_A_B_C",
            "MinCard_A_B_C_2"
        ]
    );
}

/// A temporal property or a use case named like a generated check keeps its id, as a use case
/// does beside a split use case's steps; the check takes the next free name.
#[test]
fn a_check_name_a_model_id_spells_takes_the_next_free_name() {
    let src = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
usecase Submit "投入"
usecase Forbidden_Job_1 "開始"
creates(Submit, Job)
transitions(Forbidden_Job_1, Job.status, waiting -> running)
forbidden(Job, status == running)
forbidden(Job, status == waiting)
property Forbidden_Job_0 "作業は待つ"
  always(Job.status == waiting)
"#;
    assert_eq!(check_names(src), ["Forbidden_Job_0_2", "Forbidden_Job_1_2"]);
    let ts = TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts;
    assert_eq!(ts.properties()[0].name, "Forbidden_Job_0");
    assert!(ts.actions().iter().any(|a| a.name == "Forbidden_Job_1"));
}
