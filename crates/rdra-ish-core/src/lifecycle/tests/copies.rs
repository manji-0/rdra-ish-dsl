//! `sets(UC, E, column == F.source)` (#69): the step leaves in `column` the value the row of
//! `F` it binds holds, read as a condition atom on `F` reads it.

use super::*;

const PERMIT: &str = r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Application "申請" {
  id: Int @pk
  kind: Kind
  status: Enum(submitted, approved) @default(submitted)
  staffed: Bool @default(false)
  fee: Int @default(0)
}
entity Permit "許可" {
  id: Int @pk
  kind: Kind
  staffed: Bool @default(false)
  fee: Money @default(0)
}
relate(Permit, Application, N:1)
usecase Apply "申請"
usecase Staff "責任者"
usecase Pay "納付"
usecase Approve "承認"
usecase Register "登録"
usecase Refresh "更新"
creates(Apply, Application)
sets(Staff, Application, staffed == true)
sets(Pay, Application, fee == 3)
transitions(Approve, Application.status, submitted -> approved)
creates(Register, Permit)
updates(Refresh, Permit)
condition Approved "承認済み"
holds(Approved, Application.status == approved)
applies_to(Approved, Register)
forbidden(Permit, Application, Permit.kind != Application.kind).along(Permit, Application)
forbidden(Permit, Application, Permit.staffed != Application.staffed).along(Permit, Application)
forbidden(Permit, Application, Permit.fee != Application.fee).along(Permit, Application)
"#;

const COPIES: &str = r#"
sets(Register, Permit, kind == Application.kind)
sets(Register, Permit, staffed == Application.staffed)
sets(Register, Permit, fee == Application.fee)
sets(Refresh, Permit, staffed == Application.staffed)
sets(Refresh, Permit, fee == Application.fee)
"#;

fn violated(d: &Derived) -> Vec<String> {
    let mut out: Vec<String> = d
        .diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn build_diags(src: &str) -> Vec<StateDiag> {
    let model = model_from(src);
    TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL)
        .diags
        .into_iter()
        .map(|d| d.diag)
        .collect()
}

/// Without a copy the permit's kind is a free choice, and the Bool and Int columns keep their
/// default while the application's move on: every rule breaks.
#[test]
fn without_a_copy_the_created_row_does_not_follow_its_parent() {
    let d = derive_src(PERMIT);
    assert_eq!(
        violated(&d),
        [
            "CrossForbidden_Permit_0",
            "CrossForbidden_Permit_1",
            "CrossForbidden_Permit_2"
        ],
        "{:?}",
        d.diags
    );
}

/// The creation copies from the parent it is created under, and the update from the parent
/// its owner names, so no permit differs from its application until the application moves
/// again, and `Refresh` copies it back.
#[test]
fn a_copy_reads_the_parent_the_row_belongs_to() {
    let d = derive_src(&format!(
        "{PERMIT}{COPIES}\nafter(Refresh).assert(Permit.staffed == true)\n"
    ));
    let v = violated(&d);
    assert!(
        !v.contains(&"CrossForbidden_Permit_0".to_string()),
        "the kind is copied at creation and never changes: {v:?}"
    );
    // `Staff` and `Pay` move the application after the permit exists, so the Bool and Int
    // rules still break until `Refresh` copies them again.
    assert!(v.contains(&"CrossForbidden_Permit_1".to_string()), "{v:?}");
    let kinds = d.rows("Permit");
    assert!(
        kinds.iter().any(|r| r.contains("kind=drink"))
            && kinds.iter().any(|r| r.contains("kind=food")),
        "the copy takes either value of the source: {kinds:?}"
    );
    let unmet = d.diags.iter().any(
        |x| matches!(&x.diag, StateDiag::AfterAssertViolated { anchor, .. } if anchor == "Refresh"),
    );
    assert!(
        unmet,
        "Refresh leaves staffed false while the application is unstaffed: {:?}",
        d.diags
    );
}

/// A use case that creates both rows reads the source in the successor, as a condition on a
/// created row does: the invoice takes the kind the order was created with.
#[test]
fn a_copy_from_a_row_created_in_the_same_step_reads_the_successor() {
    let d = derive_src(
        r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Order "注文" { id: Int @pk  kind: Kind }
entity Invoice "請求" { id: Int @pk  kind: Kind }
relate(Invoice, Order, N:1)
usecase Open "開始"
creates(Open, Order)
creates(Open, Invoice)
sets(Open, Invoice, kind == Order.kind)
forbidden(Invoice, Order, Invoice.kind != Order.kind)
"#,
    );
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    assert_eq!(d.rows("Invoice").len(), 2, "{:?}", d.rows("Invoice"));
}

/// Two writes no relate joins are two steps. A run takes them in order, so a step reads a
/// row an earlier step of the run wrote, and cannot read one a later step writes.
#[test]
fn a_source_in_a_later_transaction_group_is_unbound() {
    let model = |first_api: &str, second_api: &str| {
        format!(
            r#"
variation Kind "種類" {{
  food "食品"
  drink "飲料"
}}
entity Order "注文" {{ id: Int @pk  kind: Kind }}
entity Invoice "請求" {{ id: Int @pk  kind: Kind }}
usecase Open "開始"
api OrderApi "注文 API"
api InvoiceApi "請求 API"
invokes(Open, {first_api})
invokes(Open, {second_api})
creates(OrderApi, Order)
creates(InvoiceApi, Invoice)
sets(Open, Invoice, kind == Order.kind)
forbidden(Invoice, Order, Invoice.kind != Order.kind)
"#
        )
    };
    let diags = build_diags(&model("InvoiceApi", "OrderApi"));
    assert!(
        diags.iter().any(|d| matches!(d, StateDiag::UnboundCopySource { reason, .. } if reason.contains("later transaction group"))),
        "{diags:?}"
    );
    // The order is written first, so the invoice's step copies from the run's order: with
    // two orders of different kinds, the rule still holds.
    let d = derive_src(&model("OrderApi", "InvoiceApi"));
    assert!(
        !d.diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::UnboundCopySource { .. })),
        "{:?}",
        d.diags
    );
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
}

/// A copy within one row reads the row's pre-state on an update.
#[test]
fn a_copy_within_one_row_reads_the_pre_state() {
    let d = derive_src(
        r#"
entity Flag "旗" { id: Int @pk  on: Bool @default(false)  shadow: Bool @default(false) }
usecase Make "作成"
usecase Flip "切替"
usecase Mirror "写し"
creates(Make, Flag)
sets(Flip, Flag, on == true)
sets(Mirror, Flag, shadow == Flag.on)
forbidden(Flag, on == false, shadow == true)
"#,
    );
    assert!(violated(&d).is_empty(), "{:?}", d.diags);
    assert!(
        d.rows("Flag").contains(&"on=true, shadow=true".to_string()),
        "{:?}",
        d.rows("Flag")
    );
}

/// A row with several candidates, a row no relate binds, and an unlinked created child are
/// `UnboundCopySource`, and the copy is not applied.
#[test]
fn a_source_the_step_does_not_bind_is_an_error() {
    let src = r#"
variation Kind "種類" {
  food "食品"
  drink "飲料"
}
entity Order "注文" { id: Int @pk  kind: Kind }
entity Line "明細" { id: Int @pk  kind: Kind }
entity Note "メモ" { id: Int @pk  kind: Kind }
entity Tag "札" { id: Int @pk  kind: Kind }
relate(Line, Order, N:1)
relate(Tag, Order, N:1).optional()
usecase Place "注文"
usecase AddLine "明細追加"
usecase Fix "修正"
usecase Jot "メモ"
usecase Label "札付け"
creates(Place, Order)
creates(AddLine, Line)
updates(Fix, Order)
sets(Fix, Order, kind == Line.kind)
creates(Jot, Note)
sets(Jot, Note, kind == Order.kind)
creates(Label, Tag)
sets(Label, Tag, kind == Order.kind)
"#;
    let unbound: Vec<(String, String)> = build_diags(src)
        .into_iter()
        .filter_map(|d| match d {
            StateDiag::UnboundCopySource {
                usecase, reason, ..
            } => Some((usecase, reason)),
            _ => None,
        })
        .collect();
    let reason = |uc: &str| {
        unbound
            .iter()
            .find(|(u, _)| u == uc)
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| panic!("no UnboundCopySource for {uc}: {unbound:?}"))
    };
    assert!(reason("Fix").contains("several Line rows"), "{unbound:?}");
    assert!(reason("Jot").contains("no relate"), "{unbound:?}");
    assert!(reason("Label").contains("optional"), "{unbound:?}");
    assert_eq!(unbound.len(), 3, "{unbound:?}");
}

/// A literal and a copy of one column in one use case conflict; a copy into a lifecycle column
/// is the double modeling a literal `sets` on an update is, on a creation too: a state model
/// starts at the values its creations name.
#[test]
fn a_copy_conflicts_with_a_literal_and_with_transitions() {
    let src = r#"
entity Parent "親" { id: Int @pk  on: Bool @default(false)  status: Enum(a, b) @default(a) }
entity Child "子" { id: Int @pk  on: Bool @default(false)  status: Enum(a, b) @default(a) }
relate(Child, Parent, N:1)
usecase Make "作成"
usecase Born "誕生"
usecase Both "両方"
usecase Move "移動"
usecase Spawn "派生"
creates(Make, Parent)
creates(Born, Child)
updates(Both, Child)
sets(Both, Child, on == true)
sets(Both, Child, on == Parent.on)
transitions(Move, Child.status, a -> b)
updates(Move, Child)
sets(Move, Child, status == Parent.status)
creates(Spawn, Child)
sets(Spawn, Child, status == Parent.status)
"#;
    let diags = build_diags(src);
    assert!(
        diags.iter().any(|d| matches!(d, StateDiag::ConflictingEffects { usecase, sites, .. } if usecase == "Both" && sites.len() == 2)),
        "{diags:?}"
    );
    assert!(
        diags.iter().any(
            |d| matches!(d, StateDiag::DoubleModeledEnum { usecase, .. } if usecase == "Move")
        ),
        "{diags:?}"
    );
}

/// A copy reads its source through a successor guard, so the source joins the cone of the
/// target it writes: each cone reaches the projection of the whole system.
#[test]
fn a_copy_keeps_the_cone_exact() {
    let model = model_from(&format!("{PERMIT}{COPIES}"));
    let (exact, _) = super::cone::assert_cones_exact("copies", &model, Bounds::STATES);
    assert!(exact > 0);
}
