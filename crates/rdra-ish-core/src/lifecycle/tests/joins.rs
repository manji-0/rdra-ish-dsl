//! Joins over two keys (#73): every join a guard, a quantifier, or a copy makes follows the
//! key [`crate::model::SemanticModel::join_key`] picks, so a second key between the same two
//! entities never changes which rows a rule reads.

use super::*;
use crate::lifecycle::types::{Effect, Rhs, VarKind};

/// A permit is registered under its application (1:1); a renewal application `renews` a
/// permit. `{EXPIRY}` is the key the expiry's quantifier follows.
const RENEWAL: &str = r#"
entity Permit "許可" { id: Int @pk  status: Enum(valid, expired) @default(valid) }
entity App "申請" { id: Int @pk  kind: Enum(new, renewal)  status: Enum(open, done) @default(open) }
relate(Permit, App, 1:1)
relate(App, Permit, N:1).optional().role(renews)
usecase Issue "新規申請"
usecase Register "登録"
usecase Apply "更新申請"
usecase Renew "更新"
usecase Expire "失効"
creates(Issue, App)
sets(Issue, App, kind == new)
creates(Register, Permit)
condition NoPermitYet "許可がまだない新規申請"
holds(NoPermitYet, App.kind == new).none(Permit).by(App)
applies_to(NoPermitYet, Register)
creates(Apply, App)
sets(Apply, App, kind == renewal)
reads(Apply, Permit)
condition Renewable "更新する許可が有効"
holds(Renewable, Permit.status == valid).by(renews)
applies_to(Renewable, Apply)
applies_to(Renewable, Renew)
transitions(Renew, App.status, open -> done)
transitions(Expire, Permit.status, valid -> expired)
condition NotRenewed "更新済みの申請がない"
holds(NotRenewed, Permit.status == valid).none(App.status == done).by({EXPIRY})
applies_to(NotRenewed, Expire)
forbidden(App, Permit, App.status == done, Permit.status == expired).along(App, Permit).by(renews)
"#;

fn at_verify(src: &str) -> Derived {
    derive_scoped(&model_from(src), Bounds::VERIFY, &Scope::ALL)
}

fn violated(d: &Derived) -> bool {
    d.diags.iter().any(|x| {
        matches!(&x.diag, StateDiag::ConstraintViolated { name, .. } if name == "CrossForbidden_App_0")
    })
}

fn never_runs(d: &Derived, uc: &str) -> bool {
    d.diags
        .iter()
        .any(|x| matches!(&x.diag, StateDiag::NeverRuns { action, .. } if action == uc))
}

/// Through `renews` the expiry waits while a renewal of the permit is done; through the
/// registration key it reads the new application, which no renewal moves, so a permit expires
/// under a done renewal.
#[test]
fn the_key_a_quantifier_names_decides_the_verdict() {
    let renews = at_verify(&RENEWAL.replace("{EXPIRY}", "renews"));
    assert!(!violated(&renews), "{:?}", renews.diags);
    assert!(!never_runs(&renews, "Renew"), "{:?}", renews.diags);
    let registered = at_verify(&RENEWAL.replace("{EXPIRY}", "App"));
    assert!(violated(&registered), "{:?}", registered.diags);
}

/// Without `.by`, every join between App and Permit is ambiguous: the body's witness, the
/// quantifiers, and the analysis's `.along`. The lifecycle names the candidates and the
/// rewrite, and links nothing.
#[test]
fn a_join_two_keys_could_follow_is_ambiguous() {
    let src = RENEWAL
        .replace(".by(App)", "")
        .replace(
            ".by(renews)\napplies_to(Renewable",
            "\napplies_to(Renewable",
        )
        .replace(".by({EXPIRY})", "");
    let model = model_from(&src);
    let diags: Vec<StateDiag> = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL)
        .diags
        .into_iter()
        .map(|d| d.diag)
        .collect();
    let mut ambiguous: Vec<(String, String)> = diags
        .iter()
        .filter_map(|d| match d {
            StateDiag::AmbiguousOwnerLink {
                statement,
                roles,
                rewrite,
                ..
            } => {
                assert_eq!(roles, "App, renews");
                Some((statement.clone(), rewrite.clone()))
            }
            _ => None,
        })
        .collect();
    ambiguous.sort();
    ambiguous.dedup();
    assert_eq!(
        ambiguous,
        [
            (".none(...)".to_string(), ".none(...).by(App)".to_string()),
            (
                ".none(Permit)".to_string(),
                ".none(Permit).by(App)".to_string()
            ),
            (
                "holds(Renewable, ...)".to_string(),
                "holds(Renewable, ...).by(App)".to_string()
            ),
        ]
    );
}

/// The owner var of the key with this role (`App` names the registration key).
fn owner_of(ts: &TransitionSystem, role: Option<&str>) -> crate::lifecycle::types::VarId {
    let at = ts
        .vars()
        .iter()
        .position(|d| matches!(&d.kind, VarKind::Owner { role: r, .. } if r.as_deref() == role))
        .expect("owner var");
    crate::lifecycle::types::VarId::at(at)
}

/// `Register` creates a permit under an application it only reads (a parent param). The
/// application is the child of the `renews` key, and a parent param is never relinked, so the
/// new application does not renew the permit it was just granted. `Apply` creates a renewal
/// that a guard links through `renews` to the valid permit it reads.
#[test]
fn a_parent_param_is_not_relinked_and_a_guard_links_an_optional_key() {
    let model = model_from(&RENEWAL.replace("{EXPIRY}", "renews"));
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let renews = owner_of(&ts, Some("renews"));
    let assigns = |name: &str| -> Vec<Rhs> {
        let action = ts.actions().iter().find(|a| a.name == name).expect(name);
        action
            .effects
            .iter()
            .filter_map(|e| match e {
                Effect::Assign { var, rhs, .. } if *var == renews => Some(*rhs),
                _ => None,
            })
            .collect()
    };
    assert!(assigns("Register").is_empty());
    assert!(
        matches!(assigns("Apply").as_slice(), [Rhs::Param(_)]),
        "{:?}",
        assigns("Apply")
    );
}

/// A copy between two entities two keys join names the key to its source with `.by`.
#[test]
fn a_copy_through_two_keys_names_its_key() {
    let src = r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Site "施設" { id: Int @pk  kind: Kind }
entity Permit "許可" { id: Int @pk  kind: Kind }
relate(Permit, Site, N:1).role(home)
relate(Permit, Site, N:1).role(branch)
usecase Fix "修正"
updates(Fix, Permit)
sets(Fix, Permit, kind == Site.kind)
"#;
    let build = |src: &str| -> Vec<StateDiag> {
        let model = model_from(src);
        TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL)
            .diags
            .into_iter()
            .map(|d| d.diag)
            .collect()
    };
    let ambiguous = build(src);
    assert!(
        ambiguous.iter().any(
            |d| matches!(d, StateDiag::AmbiguousOwnerLink { rewrite, .. }
            if rewrite == "sets(Fix, Permit, kind == Site.kind).by(home)")
        ),
        "{ambiguous:?}"
    );
    let named = build(&format!("{src}.by(branch)\n").replace("\n.by(", ".by("));
    assert!(
        !named.iter().any(|d| matches!(
            d,
            StateDiag::AmbiguousOwnerLink { .. } | StateDiag::UnboundCopySource { .. }
        )),
        "{named:?}"
    );
}
