//! `holds(..).along(A, B, C)` (#77): a guard reads the row a key path joins to the rows the
//! use case binds, and a guard that reads an entity no key joins to them is
//! `UnjoinedGuardRead`.

use super::*;

/// A claim and a settlement are both children of a case, which has one settlement at most:
/// opening it moves the case. `Complete` runs while `{GUARD}`; the rule forbids a completed
/// claim whose case's settlement is not agreed.
const CASE: &str = r#"
entity Case "案件" { id: Int @pk  status: Enum(open, negotiating) @default(open) }
entity Claim "請求" { id: Int @pk  status: Enum(submitted, completed) @default(submitted) }
entity Settlement "示談" { id: Int @pk  status: Enum(negotiating, agreed) @default(negotiating) }
relate(Claim, Case, N:1)
relate(Settlement, Case, N:1)
usecase Open "受付"
usecase Submit "請求"
usecase Negotiate "示談開始"
usecase Agree "示談成立"
usecase Complete "請求完了"
creates(Open, Case)
creates(Submit, Claim)
creates(Negotiate, Settlement)
transitions(Negotiate, Case.status, open -> negotiating)
transitions(Agree, Settlement.status, negotiating -> agreed)
transitions(Complete, Claim.status, submitted -> completed)
condition Signed "示談書がある"
holds(Signed, Settlement.status == agreed){GUARD}
applies_to(Signed, Complete)
forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement)
"#;

fn case(guard: &str) -> SemanticModel {
    model_from(&CASE.replace("{GUARD}", guard))
}

fn violated(model: &SemanticModel) -> bool {
    derive_scoped(model, Bounds::VERIFY, &Scope::ALL)
        .diags
        .iter()
        .any(|x| matches!(&x.diag, StateDiag::ConstraintViolated { name, .. } if name == "CrossForbidden_Claim_0"))
}

fn unjoined(model: &SemanticModel) -> Vec<StateDiag> {
    TransitionSystem::build(model, Bounds::STATES, &Scope::ALL)
        .diags
        .into_iter()
        .map(|d| d.diag)
        .filter(|d| matches!(d, StateDiag::UnjoinedGuardRead { .. }))
        .collect()
}

/// Along the case, the guard reads the settlement of the claim's own case, so the rule holds at
/// two instances. Without it, an agreed settlement of another case completes the claim.
#[test]
fn a_guard_along_a_path_reads_the_row_of_its_own_case() {
    assert!(!violated(&case(".along(Claim, Case, Settlement)")));
    assert!(violated(&case("")));
}

/// The guard with no path reads a settlement anywhere, and says so: the entity, the use case,
/// and the `.along` that joins it.
#[test]
fn a_guard_on_an_unjoined_entity_warns_with_the_path() {
    let warned = unjoined(&case(""));
    let [StateDiag::UnjoinedGuardRead {
        condition,
        usecase,
        entity,
        fix,
        ..
    }] = warned.as_slice()
    else {
        panic!("{warned:?}");
    };
    assert_eq!(
        (condition.as_str(), usecase.as_str(), entity.as_str()),
        ("Signed", "Complete", "Settlement")
    );
    assert!(fix.contains("`.along(Claim, Case, Settlement)`"), "{fix}");
    assert!(fix.contains("write Settlement in Complete"), "{fix}");
    assert_eq!(
        StateDiag::UnjoinedGuardRead {
            condition: condition.clone(),
            usecase: usecase.clone(),
            entity: entity.clone(),
            fix: fix.clone(),
            site: crate::lifecycle::Site::bare(crate::lifecycle::ModelRef::Synthesized),
        }
        .severity(),
        crate::lifecycle::Severity::Warning
    );
    assert!(unjoined(&case(".along(Claim, Case, Settlement)")).is_empty());
}

/// An entity one key joins to a row the use case binds needs no path.
#[test]
fn a_guard_one_key_joins_does_not_warn() {
    let src = CASE
        .replace("{GUARD}", "")
        .replace("Settlement.status == agreed", "Case.status == open");
    assert!(unjoined(&model_from(&src)).is_empty());
}

/// A path that leaves out a hop no key joins says why the guard binds nothing.
#[test]
fn a_path_with_an_unrelated_hop_is_unbindable() {
    let model = case(".along(Claim, Settlement)");
    let diags = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).diags;
    assert!(
        diags.iter().any(|d| matches!(&d.diag,
            StateDiag::UnbindableCondition { reason, .. } if reason.contains("binds Claim and Settlement"))),
        "{diags:?}"
    );
}

/// A claim can move to another case (`moved_from`), so two keys join a claim and a case; a
/// `.by` after the `.along` picks the hop's key, and the verdict follows it.
#[test]
fn a_by_after_along_picks_the_key_of_a_hop() {
    let src = CASE
        .replace(
            "relate(Claim, Case, N:1)",
            "relate(Claim, Case, N:1).role(home)\nrelate(Claim, Case, N:1).role(moved_from)",
        )
        .replace(
            "forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement)",
            "forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement).by(home)",
        );
    let along = |by: &str| {
        model_from(&src.replace(
            "{GUARD}",
            &format!(".along(Claim, Case, Settlement).by({by})"),
        ))
    };
    assert!(!violated(&along("home")));
    assert!(violated(&along("moved_from")));
}

/// `list --kind condition` and the diagrams print the path.
#[test]
fn the_body_display_prints_the_path() {
    let model = case(".along(Claim, Case, Settlement)");
    let ck = model
        .conditions
        .iter()
        .find(|(_, c)| c.id == "Signed")
        .map(|(k, _)| k)
        .expect("Signed");
    assert_eq!(
        crate::analysis::condition_body_display(&model, ck).as_deref(),
        Some("(Settlement.status == agreed).along(Claim, Case, Settlement)")
    );
}

#[test]
fn a_guard_along_a_path_keeps_the_cone_exact() {
    let model = case(".along(Claim, Case, Settlement)");
    let (exact, _) = super::cone::assert_cones_exact("along_guard", &model, Bounds::VERIFY);
    assert!(exact > 0);
}

/// A use case that creates the child of an `.optional()` key links it to the parent its
/// guard's path goes through, as a guard that compares the parent does; with no path the child
/// is created unlinked.
#[test]
fn a_path_through_an_optional_key_links_the_created_child() {
    use crate::lifecycle::types::{Effect, ParamRole, Rhs};
    let linked = |guard: &str| {
        let src = CASE
            .replace("{GUARD}", guard)
            .replace(
                "relate(Claim, Case, N:1)",
                "relate(Claim, Case, N:1).optional()",
            )
            .replace("applies_to(Signed, Complete)", "applies_to(Signed, Submit)");
        let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
        let submit = ts
            .actions()
            .iter()
            .find(|a| a.name == "Submit")
            .expect("Submit");
        submit.effects.iter().find_map(|e| match e {
            Effect::Assign { rhs, .. } => Some(match rhs {
                Rhs::Param(p) => matches!(
                    submit.params[usize::from(*p)].role,
                    ParamRole::Parent { .. }
                ),
                _ => false,
            }),
            _ => None,
        })
    };
    assert_eq!(linked(".along(Claim, Case, Settlement)"), Some(true));
    assert_eq!(linked(""), Some(false));
}
