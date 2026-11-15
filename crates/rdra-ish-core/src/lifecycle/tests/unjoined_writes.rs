//! `UnjoinedWrite` (#117): a use case writes rows of two entities that a key path relates
//! through an entity the use case does not bind, so one run may write rows that belong to
//! different rows of the entity between.

use super::*;

/// Account and Store meet only through Device; `{EXTRA}` is what `Register` also declares.
const DEVICE: &str = r#"
entity Account "Account" {
  id: Int @pk
  status: Enum(unregistered, registered) @default(unregistered)
}
entity Device "Device" {
  id: Int @pk
}
entity Store "Store" {
  id: Int @pk
  status: Enum(unregistered, registered) @default(unregistered)
}
relate(Device, Account, 1:1)
relate(Store, Device, 1:1)
usecase Create "Create"
usecase Register "Register"
creates(Create, Account)
creates(Create, Device)
creates(Create, Store)
updates(Register, Account)
transitions(Register, Account.status, unregistered -> registered)
transitions(Register, Store.status, unregistered -> registered)
{EXTRA}
"#;

fn unjoined(extra: &str) -> Vec<(String, String)> {
    let model = model_from(&DEVICE.replace("{EXTRA}", extra));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    built
        .diags
        .iter()
        .filter(|d| matches!(d.diag, StateDiag::UnjoinedWrite { .. }))
        .map(|d| (d.diag.name(&[]), d.diag.to_string()))
        .collect()
}

#[test]
fn a_use_case_writing_both_ends_of_an_unwritten_path_is_warned() {
    let found = unjoined("updates(Register, Store)");
    assert_eq!(
        found,
        [(
            "UnjoinedWrite_Register_Account_Store".to_string(),
            "Register writes Account and Store, and no row it writes joins them, so one run may write Account and Store rows that belong to different Device rows; write Device in Register (`updates(Register, Device)`) to write joined rows, or mark a write meant as a row of its own with `.apart()`".to_string()
        )]
    );
    let model = model_from(&DEVICE.replace("{EXTRA}", "updates(Register, Store)"));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let d = built
        .diags
        .iter()
        .find(|d| matches!(d.diag, StateDiag::UnjoinedWrite { .. }))
        .unwrap();
    assert_eq!(d.diag.severity(), crate::lifecycle::Severity::Warning);
}

#[test]
fn writing_the_entity_between_joins_the_rows() {
    assert!(unjoined("updates(Register, Store)\nupdates(Register, Device)").is_empty());
}

#[test]
fn a_write_marked_apart_is_not_warned() {
    assert!(unjoined("updates(Register, Store).apart()").is_empty());
}

/// A created row binds its parent, so creating Store under a written Device joins it.
#[test]
fn a_directly_related_pair_is_left_to_the_owner_binding() {
    let src = DEVICE
        .replace("{EXTRA}", "updates(Register, Device)")
        .replace(
            "transitions(Register, Store.status, unregistered -> registered)\n",
            "",
        );
    let model = model_from(&src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    assert!(!built
        .diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::UnjoinedWrite { .. })));
}
