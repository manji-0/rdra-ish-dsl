//! Joins over two keys (#73): every join a guard, a quantifier, or a copy makes follows the
//! key [`crate::model::SemanticModel::join_key`] picks, so a second key between the same two
//! entities never changes which rows a rule reads. Two keys from one child to one parent
//! (#75) bind a parent row each, and a parent the use case writes takes only the key its `.by`
//! names (#76).

use super::*;
use crate::lifecycle::types::{Effect, GuardWhy, ParamRole, Pred, Rhs, Term, VarKind};

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

/// An order has two mandatory keys to Address, `ship_to` and `bill_to`, and `Place` creates
/// it without writing an address. `Ship` runs while the `{SHIP}` address is in the east; the
/// rule forbids a shipped order billed to the west. The rule is violated only when the two
/// keys point at different rows.
const TWO_KEYS: &str = r#"
entity Address "住所" { id: Int @pk  region: Enum(east, west) }
entity Order "注文" { id: Int @pk  status: Enum(open, shipped, cancelled) @default(open) }
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
usecase AddEast "東の住所"
usecase AddWest "西の住所"
usecase Place "注文"
usecase Ship "出荷"
usecase Cancel "取消"
creates(AddEast, Address)
sets(AddEast, Address, region == east)
creates(AddWest, Address)
sets(AddWest, Address, region == west)
creates(Place, Order)
transitions(Ship, Order.status, open -> shipped)
transitions(Cancel, Order.status, open -> cancelled)
condition ShipsEast "東へ出荷する"
holds(ShipsEast, Address.region == east).by({SHIP})
applies_to(ShipsEast, Ship)
forbidden(Order, Address, Order.status == shipped, Address.region == west).along(Order, Address).by(bill_to)
"#;

fn order_violated(d: &Derived) -> bool {
    d.diags.iter().any(|x| {
        matches!(&x.diag, StateDiag::ConstraintViolated { name, .. } if name == "CrossForbidden_Order_0")
    })
}

/// The `Parent` param of the key with this role, and the owner var `Place` assigns it to.
fn place_parent(ts: &TransitionSystem, role: &str) -> Option<u8> {
    let place = ts
        .actions()
        .iter()
        .find(|a| a.name == "Place")
        .expect("Place");
    let owner = owner_of(ts, Some(role));
    place.effects.iter().find_map(|e| match e {
        Effect::Assign {
            var,
            rhs: Rhs::Param(p),
            ..
        } if *var == owner
            && matches!(place.params[usize::from(*p)].role, ParamRole::Parent { .. }) =>
        {
            Some(*p)
        }
        _ => None,
    })
}

/// Each key of a created child has its own parent param, so an order may ship to one address
/// and bill another (#75). With one shared param the two keys were equal, and the violation
/// through two rows was unreachable.
#[test]
fn two_keys_to_one_parent_point_at_rows_of_their_own() {
    let model = model_from(&TWO_KEYS.replace("{SHIP}", "ship_to"));
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let ship = place_parent(&ts, "ship_to").expect("ship_to parent");
    let bill = place_parent(&ts, "bill_to").expect("bill_to parent");
    assert_ne!(ship, bill);
    assert!(order_violated(&at_verify(
        &TWO_KEYS.replace("{SHIP}", "ship_to")
    )));
    assert!(!order_violated(&at_verify(
        &TWO_KEYS.replace("{SHIP}", "bill_to")
    )));
}

#[test]
fn two_keys_to_one_parent_keep_the_cone_exact() {
    let model = model_from(&TWO_KEYS.replace("{SHIP}", "ship_to"));
    let (exact, _) = super::cone::assert_cones_exact("two_keys", &model, Bounds::VERIFY);
    assert!(exact > 0);
}

/// `Place` guarded by a `holds` on the address `by` names, and the rule on the open order it
/// leaves.
fn placed_under(by: &str) -> String {
    TWO_KEYS
        .replace("{SHIP}", "ship_to")
        .replace("Order.status == shipped", "Order.status == open")
        .replace(
            "applies_to(ShipsEast, Ship)",
            &format!(
                "applies_to(ShipsEast, Ship)\ncondition PlacesEast \"東の住所で注文する\"\nholds(PlacesEast, Address.region == east){by}\napplies_to(PlacesEast, Place)"
            ),
        )
}

/// An atom on a parent the action binds reads the row its `.by` key joins (#75): the
/// `bill_to` guard keeps every placed order billed to the east, the `ship_to` guard does not.
#[test]
fn a_guard_on_a_parent_param_reads_the_key_its_by_names() {
    assert!(!order_violated(&at_verify(&placed_under(".by(bill_to)"))));
    assert!(order_violated(&at_verify(&placed_under(".by(ship_to)"))));
}

/// With two parent params of one entity and no `.by`, the atom's row is ambiguous.
#[test]
fn a_guard_on_two_parent_params_without_by_is_ambiguous() {
    let model = model_from(&placed_under(""));
    let diags = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).diags;
    assert!(
        diags.iter().any(|d| matches!(&d.diag,
            StateDiag::AmbiguousOwnerLink { statement, rewrite, .. }
            if statement == "holds(PlacesEast, ...)" && rewrite == "holds(PlacesEast, ...).by(ship_to)")),
        "{diags:?}"
    );
}

/// An `.optional()` key whose parent a guard compares links the created child to that parent
/// (#73) through a param of its own, beside the mandatory `ship_to`'s.
#[test]
fn a_guard_links_an_optional_key_beside_a_mandatory_one() {
    let src = placed_under(".by(bill_to)").replace(
        "relate(Order, Address, N:1).role(bill_to)",
        "relate(Order, Address, N:1).optional().role(bill_to)",
    );
    let model = model_from(&src);
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let ship = place_parent(&ts, "ship_to").expect("ship_to parent");
    let bill = place_parent(&ts, "bill_to").expect("bill_to parent");
    assert_ne!(ship, bill);
    assert!(!order_violated(&at_verify(&src)));
}

/// An after-assert has no `.by`: an entity the step binds through two parent params is bound
/// over every row, so an address in the west fails it even when both keys point east.
#[test]
fn an_after_assert_over_two_parent_params_reads_every_row() {
    let src = format!(
        "{}condition BillsEast \"東へ請求する\"\nholds(BillsEast, Address.region == east).by(bill_to)\napplies_to(BillsEast, Place)\nafter(Place).assert(Address.region == east)\n",
        placed_under(".by(ship_to)")
    );
    let d = at_verify(&src);
    assert!(
        d.diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::AfterAssertViolated { .. })),
        "{:?}",
        d.diags
    );
}

/// `ShipBilled` checks the billing address and ships: it writes an address and an order. The
/// guard reads the address it writes, joined to the order through `{KEY}`. The rule forbids a
/// shipped order whose `ship_to` address is in the west.
const WRITTEN_PARENT: &str = r#"
entity Address "住所" { id: Int @pk  region: Enum(east, west)  checked: Bool @default(false) }
entity Order "注文" { id: Int @pk  status: Enum(open, shipped, cancelled) @default(open) }
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
usecase AddEast "東の住所"
usecase AddWest "西の住所"
usecase Place "注文"
usecase ShipBilled "請求先を確認して出荷"
usecase Cancel "取消"
creates(AddEast, Address)
sets(AddEast, Address, region == east)
creates(AddWest, Address)
sets(AddWest, Address, region == west)
creates(Place, Order)
sets(ShipBilled, Address, checked == true)
transitions(ShipBilled, Order.status, open -> shipped)
transitions(Cancel, Order.status, open -> cancelled)
condition BillsEast "請求先が東"
holds(BillsEast, Address.region == east){KEY}
applies_to(BillsEast, ShipBilled)
forbidden(Order, Address, Order.status == shipped, Address.region == west).along(Order, Address).by(ship_to)
"#;

fn written_parent(key: &str) -> String {
    WRITTEN_PARENT.replace("{KEY}", key)
}

/// The roles of the owner guards `ShipBilled` puts on the order it updates.
fn owner_guards(ts: &TransitionSystem) -> Vec<String> {
    let ship = ts
        .actions()
        .iter()
        .find(|a| a.name == "ShipBilled")
        .expect("ShipBilled");
    let mut roles: Vec<String> = ship
        .guards
        .iter()
        .filter(|g| matches!(g.why, GuardWhy::Owner))
        .filter_map(|g| match &g.pred {
            Pred::Cmp(Term::Slot { var, .. }, ..) => match &ts.vars()[var.index()].kind {
                VarKind::Owner { role, .. } => role.clone(),
                _ => None,
            },
            _ => None,
        })
        .collect();
    roles.sort();
    roles
}

/// A use case that writes the parent binds the written row to the key a `.by` of it names,
/// not to every key of the child (#76). Billed to the checked address, the order may ship to
/// another, in the west: the violation is reachable. Before, both keys were on the written
/// row, and `verify` said ok.
#[test]
fn a_written_parent_takes_only_the_key_its_by_names() {
    let billed = model_from(&written_parent(".by(bill_to)"));
    let ts = TransitionSystem::build(&billed, Bounds::VERIFY, &Scope::ALL).ts;
    assert_eq!(owner_guards(&ts), ["bill_to"]);
    assert!(order_violated(&at_verify(&written_parent(".by(bill_to)"))));
    assert!(!order_violated(&at_verify(&written_parent(".by(ship_to)"))));
}

/// Two keys and no `.by`: the written row is the order's on neither key, so the guard reads an
/// address unrelated to the order and the violation stays reachable.
#[test]
fn a_written_parent_without_by_takes_neither_of_two_keys() {
    let src = written_parent("");
    let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
    assert!(owner_guards(&ts).is_empty());
    assert!(order_violated(&at_verify(&src)));
}

/// An order created with an address it creates takes the new address on the key the `.by`
/// names, and a row of its own on the other.
#[test]
fn a_created_parent_takes_the_named_key_of_a_created_child() {
    let src = TWO_KEYS.replace("{SHIP}", "ship_to").replace(
        "creates(Place, Order)",
        "creates(Place, Order)\ncreates(Place, Address)\nsets(Place, Address, region == east)\ncondition NewShipTo \"新しい配送先\"\nholds(NewShipTo, Address.region == east).by(ship_to)\napplies_to(NewShipTo, Place)",
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
    let place = ts
        .actions()
        .iter()
        .find(|a| a.name == "Place")
        .expect("Place");
    let role_of = |role: &str| {
        let owner = owner_of(&ts, Some(role));
        place.effects.iter().find_map(|e| match e {
            Effect::Assign {
                var,
                rhs: Rhs::Param(p),
                ..
            } if *var == owner => Some(place.params[usize::from(*p)].role),
            _ => None,
        })
    };
    assert_eq!(role_of("ship_to"), Some(ParamRole::New));
    assert!(
        matches!(role_of("bill_to"), Some(ParamRole::Parent { .. })),
        "{:?}",
        role_of("bill_to")
    );
}

#[test]
fn a_written_parent_on_one_key_keeps_the_cone_exact() {
    let model = model_from(&written_parent(".by(bill_to)"));
    let (exact, _) = super::cone::assert_cones_exact("written_parent", &model, Bounds::VERIFY);
    assert!(exact > 0);
}
