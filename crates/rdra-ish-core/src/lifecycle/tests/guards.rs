//! `guard_dependencies`: each guard is one (use case, condition) across every action of the
//! use case, and a rule that fails without it depends on it.

use super::*;
use crate::lifecycle::{GuardKind, Target};

/// `Handle` runs on either event, so it is two actions (`Handle_by_Ev1`, `Handle_by_Ev2`),
/// each with the `Approved` guard. The report names the guard once and drops it from both:
/// the rule fails without it. `Approve` carries no guard.
const VARIANTS: &str = r#"
entity Item "品" {
  id: Int @pk
  status: Enum(x, z, w) @default(x)
  approved: Bool @default(false)
}
usecase Make "作る"
usecase Ping "知らせる"
usecase Park "寄せる"
usecase Approve "承認"
usecase Handle "処理"
event Ev1 "知らせ"
event Ev2 "寄せ"
creates(Make, Item)
transitions(Ping, Item.status, x -> z)
raises(Ping, Ev1)
transitions(Park, Item.status, x -> z)
raises(Park, Ev2)
sets(Approve, Item.approved == true)
transitions(Handle, Item.status, z -> w)
triggers(Ev1, Handle)
triggers(Ev2, Handle)
condition Approved "承認済み"
holds(Approved, Item.approved == true)
applies_to(Approved, Handle)
forbidden(Item, status == w, approved == false)
"#;

#[test]
fn a_guard_on_every_variant_of_a_use_case_is_one_guard_the_rule_depends_on() {
    let model = model_from(VARIANTS);
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let handles = ts
        .actions()
        .iter()
        .filter(|a| a.name.starts_with("Handle_by_"))
        .count();
    assert_eq!(handles, 2, "the use case runs as two variants");
    let deps = ts.guard_dependencies(&ts.targets(), Caps::for_instances(1));
    assert_eq!(deps.len(), 1, "{deps:?}");
    let d = &deps[0];
    assert!(
        matches!(d.guard.kind, GuardKind::Condition(ck) if model.conditions[ck].id == "Approved")
    );
    assert_eq!(model.use_cases[d.guard.usecase].id, "Handle");
    assert_eq!(d.rerun, [Target::Invariant(0)]);
    assert_eq!(d.protects, [Target::Invariant(0)]);
    assert!(d.undecided.is_empty());
}

/// A rule whose cone does not keep the guarded use case is not explored again: the `Note`
/// rule reads nothing `Handle` writes.
#[test]
fn a_rule_whose_cone_does_not_keep_the_use_case_is_not_explored_again() {
    let src = format!(
        "{VARIANTS}{}",
        r#"
entity Note "メモ" { id: Int @pk  kind: Enum(a, b, c) @default(a) }
usecase Jot "書く"
usecase Edit "直す"
creates(Jot, Note)
transitions(Edit, Note.kind, a -> b)
forbidden(Note, kind == c)
"#
    );
    let model = model_from(&src);
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let rules: Vec<Target> = ts
        .targets()
        .into_iter()
        .filter(|t| matches!(t, Target::Invariant(_)))
        .collect();
    assert_eq!(rules.len(), 2, "{rules:?}");
    let deps = ts.guard_dependencies(&ts.targets(), Caps::for_instances(1));
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].rerun.len(), 1, "{:?}", deps[0].rerun);
    assert_eq!(deps[0].protects, deps[0].rerun);
    assert_eq!(deps[0].cones, 1);
}
