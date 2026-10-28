//! Relate bindings: mandatory owners need a present parent, optional owners may stay unlinked.

use super::*;
use crate::lifecycle::{InvKind, ParamRole};

/// clinic-ops' Charge/Claim shape: the claim's creator updates the charge it bills.
fn billing(relate: &str) -> String {
    format!(
        r#"
entity Claim "請求" {{ id: Int @pk  status: Enum(draft, sent) }}
entity Charge "料金" {{ id: Int @pk  status: Enum(posted, billed) }}
usecase CreateCharge "計上"
usecase GenerateClaim "請求作成"
usecase SendClaim "送付"
{relate}
creates(CreateCharge, Charge)
creates(GenerateClaim, Claim)
updates(GenerateClaim, Charge)
transitions(Charge.status, GenerateClaim, posted -> billed)
transitions(Claim.status, SendClaim, draft -> sent)
"#
    )
}

#[test]
fn mandatory_owner_needs_a_present_parent_so_the_cycle_creates_nothing() {
    let model = model_from(&billing("relate(Charge, Claim, N:1)"));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
    let create = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "CreateCharge")
        .unwrap();
    assert!(create.params.iter().any(|p| p.role == ParamRole::Parent));
    let d = derive(&model);
    assert!(d.entity("Charge").patterns.is_empty());
    assert!(d.entity("Claim").patterns.is_empty());
}

#[test]
fn optional_owner_starts_unlinked_and_the_claim_links_it() {
    let model = model_from(&billing("relate(Charge, Claim, N:1).optional()"));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
    let create = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "CreateCharge")
        .unwrap();
    assert_eq!(
        create.params.len(),
        1,
        "no implied parent for an optional relate"
    );
    let d = derive(&model);
    assert_eq!(d.rows("Charge"), vec!["status=posted", "status=billed"]);
    assert_eq!(d.rows("Claim"), vec!["status=draft", "status=sent"]);
    let charge = d.entity("Charge");
    let axes: Vec<&str> = charge.axes.iter().map(|a| a.column.as_str()).collect();
    assert_eq!(
        axes,
        vec!["status"],
        "the foreign key is the owner var, not an axis"
    );
}

#[test]
fn an_unlinked_optional_owner_is_not_linked_for_along() {
    let src = format!(
        "{}\ninvariant(Charge, Claim).along(Charge, Claim).when(Claim.status == sent).then(Charge.status == billed)\nforbidden(Charge, Claim, Charge.status == posted, Claim.status == sent).along(Charge, Claim)",
        billing("relate(Charge, Claim, N:1).optional()")
    );
    // Two slots: the second charge is created after the claim and stays unlinked.
    let d = derive_scoped(&model_from(&src), Bounds::VERIFY, &Scope::All);
    assert!(
        !d.has("Charge", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "a posted charge is never linked to a claim: {:?}",
        d.entity("Charge").diagnostics
    );
    let global = format!(
        "{}\nforbidden(Charge, Claim, Charge.status == posted, Claim.status == sent)",
        billing("relate(Charge, Claim, N:1).optional()")
    );
    let d = derive_scoped(&model_from(&global), Bounds::VERIFY, &Scope::All);
    assert!(
        d.has("Charge", |x| matches!(
            x,
            StateDiag::ConstraintViolated {
                kind: InvKind::CrossForbidden,
                ..
            }
        )),
        "without .along any pair counts: {:?}",
        d.entity("Charge").diagnostics
    );
}

#[test]
fn quantifier_ignores_unlinked_related_rows() {
    let src = r#"
entity Cert "証明書" { id: Int @pk  status: Enum(active, revoked) @default(active) }
entity Assign "割当" { id: Int @pk  status: Enum(active, ended) @default(active) }
usecase Issue "発行"
usecase Revoke "失効"
usecase Spare "予備"
creates(Issue, Cert)
creates(Spare, Assign)
transitions(Cert.status, Revoke, active -> revoked)
relate(Assign, Cert, N:1).optional()
when(Cert, status == revoked).none(Assign.status == active)
"#;
    let d = derive_src(src);
    assert!(
        !d.has("Cert", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "the spare assignment belongs to no cert: {:?}",
        d.entity("Cert").diagnostics
    );
}

#[test]
fn owner_var_names_carry_the_role_only_when_one_is_given() {
    let model = model_from(
        r#"
entity Address "Address" { id: Int @pk }
entity Order "Order" { id: Int @pk }
usecase Place "Place"
creates(Place, Order)
relate(Order, Address, N:1)
relate(Order, Address, N:1).role(bill_to).optional()
"#,
    );
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::All).ts;
    let owners: Vec<String> = ts
        .var_ids()
        .map(|v| ts.var_name(v))
        .filter(|n| n.contains("._owner_"))
        .collect();
    assert_eq!(owners, ["Order._owner_Address", "Order._owner_bill_to"]);
}
