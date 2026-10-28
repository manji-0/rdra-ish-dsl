//! Effects, starts, and view flags, ported from the `states` behaviour tests.

use super::*;
use crate::lifecycle::AxisKind;

#[test]
fn entity_without_state_columns_has_one_trivial_pattern() {
    let d = derive_src(r#"entity Product "商品" { id: Int @pk  name: String  price: Decimal }"#);
    let p = d.entity("Product");
    assert!(p.axes.is_empty());
    assert_eq!(p.patterns.len(), 1);
    assert!(p.patterns[0].is_initial && p.patterns[0].is_terminal);
}

#[test]
fn api_crud_carries_the_use_case_sets() {
    let d = derive_src(
        r#"
entity Store "店舗" { id: Int @pk  status: Enum(open, closed) @default(open) }
usecase CloseStore "店舗を閉じる"
api StoreAdminApi "店舗管理API"
invokes(CloseStore, StoreAdminApi)
updates(StoreAdminApi, Store)
sets(CloseStore, Store, status == closed)
"#,
    );
    assert_eq!(d.rows("Store"), vec!["status=open", "status=closed"]);
}

#[test]
fn enum_chain_reaches_every_target() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
usecase Place  "注文確定"
usecase Pay    "支払い"
usecase Cancel "キャンセル"
creates(Place, Order)
updates(usecase::Pay,    Order)
updates(usecase::Cancel, Order)
transitions(Order.status, usecase::Pay, pending -> paid)
transitions(Order.status, usecase::Cancel, pending -> cancelled)
"#,
    );
    assert_eq!(
        d.rows("Order"),
        vec!["status=pending", "status=paid", "status=cancelled"]
    );
    assert!(!d.has("Order", |x| matches!(
        x,
        StateDiag::UnreachableEnumVariant { .. }
    )));
}

#[test]
fn transition_use_case_sets_apply_in_the_same_step() {
    let d = derive_src(
        r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place      "注文確定"
usecase DeliverUc  "配達確認"
creates(Place,         Order)
updates(usecase::DeliverUc, Order)
transitions(Order.status, usecase::DeliverUc, pending -> delivered)
sets(usecase::DeliverUc, Order, delivered_at == present)
"#,
    );
    assert_eq!(
        d.rows("Order"),
        vec![
            "delivered_at=null, status=pending",
            "delivered_at=present, status=delivered"
        ]
    );
}

#[test]
fn one_use_case_with_two_transitions_on_a_column_takes_whichever_source_holds() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) }
usecase Place "注文"
usecase Pay "支払"
usecase Cancel "取消"
creates(Place, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Cancel, pending -> cancelled)
transitions(Order.status, Cancel, paid -> cancelled)
"#,
    );
    let rows = d.rows("Order");
    assert_eq!(
        rows,
        vec!["status=pending", "status=paid", "status=cancelled"]
    );
    let o = d.entity("Order");
    assert!(!o.patterns[1].is_terminal, "Cancel leaves paid too");
    assert!(o.patterns[2].is_terminal);
}

#[test]
fn no_creates_starts_at_defaults() {
    let d = derive_src(r#"entity Flag "フラグ" { id: Int @pk  active: Bool }"#);
    let f = d.entity("Flag");
    assert!(f.no_creation_path);
    assert_eq!(d.rows("Flag"), vec!["active=false"]);
    assert!(f.patterns[0].is_initial);
}

#[test]
fn pattern_cap_truncates_and_reports() {
    let model = model_from(
        r#"
entity Multi "マルチ" { id: Int @pk  a: String @null  b: String @null  c: String @null  d: String @null }
usecase Create "生成"
usecase SetA   "Aセット"
usecase SetB   "Bセット"
usecase SetC   "Cセット"
usecase SetD   "Dセット"
creates(Create, Multi)
sets(usecase::SetA, Multi, a == present)
sets(usecase::SetB, Multi, b == present)
sets(usecase::SetC, Multi, c == present)
sets(usecase::SetD, Multi, d == present)
"#,
    );
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::All);
    let run = built.ts.explore(Caps::new(5, 1000));
    let views = run.entity_views(&model, &built.diags);
    assert!(views[0].truncated);
    assert!(views[0].patterns.len() <= 5);
    assert!(views[0]
        .diagnostics
        .contains(&StateDiag::PatternCapReached { cap: 5, bound: 16 }));
}

#[test]
fn nullable_column_reaches_present() {
    let d = derive_src(
        r#"
entity Doc "文書" { id: Int @pk  metadata: String @null }
usecase Create  "作成"
usecase Publish "公開"
creates(Create,  Doc)
updates(Publish, Doc)
sets(usecase::Publish, Doc, metadata == present)
"#,
    );
    assert_eq!(d.rows("Doc"), vec!["metadata=null", "metadata=present"]);
    assert_eq!(
        d.entity("Doc").axes[0].kind,
        AxisKind::Nullable { pg_type: None }
    );
}

#[test]
fn bool_axis_reaches_both_values() {
    let d = derive_src(
        r#"
entity Switch "スイッチ" { id: Int @pk  enabled: Bool }
usecase Create "作成"
usecase Enable "有効化"
creates(Create, Switch)
updates(Enable, Switch)
sets(usecase::Enable, Switch, enabled == true)
"#,
    );
    assert_eq!(d.rows("Switch"), vec!["enabled=false", "enabled=true"]);
}

#[test]
fn delete_marks_its_source_terminal() {
    let d = derive_src(
        r#"
entity Item "アイテム" { id: Int @pk  status: Enum(active, inactive) @default(active) }
usecase Create   "作成"
usecase Deactive "非アクティブ化"
usecase Remove   "削除"
creates(Create, Item)
updates(Deactive, Item)
deletes(Remove,   Item)
transitions(Item.status, Deactive, active -> inactive)
"#,
    );
    let item = d.entity("Item");
    assert_eq!(d.rows("Item"), vec!["status=active", "status=inactive"]);
    assert!(item.patterns[0].is_terminal, "Remove is enabled on active");
    let via: Vec<&str> = item.patterns[0]
        .provenance
        .via
        .iter()
        .map(|(_, uc)| uc.as_str())
        .collect();
    assert_eq!(via, vec!["Create", "Remove"]);
}

#[test]
fn creation_status_is_a_start_even_off_the_transition_start() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(draft, placed, shipped) @default(draft) }
usecase Place "place"
usecase Ship "ship"
creates(Place, Order)
sets(Place, Order, status == shipped)
updates(Ship, Order)
transitions(Order.status, Ship, placed -> shipped)
"#,
    );
    let o = d.entity("Order");
    assert_eq!(d.rows("Order"), vec!["status=shipped"]);
    assert!(o.patterns[0].is_initial);
    assert!(!d.has("Order", |x| matches!(
        x,
        StateDiag::DoubleModeledEnum { .. }
    )));
    assert!(!crate::lifecycle::has_errors(&d.diags), "{:?}", d.diags);
}

#[test]
fn status_sets_beside_transitions_is_an_error() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(open, closed) }
usecase Place "place"
usecase Close "close"
usecase Reopen "reopen"
creates(Place, Order)
transitions(Order.status, Close, open -> closed)
sets(Reopen, Order, status == open)
"#,
    );
    assert!(d.has(
        "Order",
        |x| matches!(x, StateDiag::DoubleModeledEnum { usecase, .. } if usecase == "Reopen")
    ));
    assert!(crate::lifecycle::has_errors(&d.diags));
}

#[test]
fn conflicting_int_sets_are_an_error_with_both_sites() {
    let d = derive_src(
        r#"
entity Item "商品" { id: Int @pk  status: Enum(active, archived) @default(active)  stock: Int @default(0) }
usecase Restock "補充"
creates(Restock, Item)
updates(Restock, Item)
sets(Restock, Item, stock == 1)
sets(Restock, Item, stock == 2)
"#,
    );
    let conflict = d.entity("Item").diagnostics.iter().find_map(|x| match x {
        StateDiag::ConflictingEffects { column, sites, .. } if column == "stock" => {
            Some(sites.len())
        }
        _ => None,
    });
    assert_eq!(conflict, Some(2));
    assert!(
        d.rows("Item").iter().any(|r| r.contains("stock=2")),
        "last write wins"
    );
}

#[test]
fn sets_without_a_crud_edge_is_still_a_write() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  flagged: Bool }
usecase Place "place"
usecase Flag "flag"
creates(Place, Order)
sets(Flag, Order, flagged == true)
"#,
    );
    assert_eq!(d.rows("Order"), vec!["flagged=false", "flagged=true"]);
}

#[test]
fn shared_use_case_is_atomic_and_transitions_do_not_leak() {
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
usecase Init "初期化"
usecase Flip "更新"
creates(Init, Order)
creates(Init, Payment)
updates(Flip, Order)
updates(Flip, Payment)
transitions(Order.status, Flip, pending -> paid)
transitions(Payment.status, Flip, paid -> pending)
"#,
    );
    assert_eq!(d.rows("Payment"), vec!["status=pending"]);
    // One atomic Flip needs Payment paid, which nothing reaches, so Order stays pending.
    assert_eq!(d.rows("Order"), vec!["status=pending"]);
}

#[test]
fn int_and_variation_columns_that_something_names_are_axes() {
    let d = derive_src(
        r#"
variation Size "サイズ" {
  small "S"
  large "L"
}
entity Item "商品" { id: Int @pk  stock: Int @default(0)  size: Size  note: String }
usecase Make "作る"
usecase Fill "補充"
creates(Make, Item)
sets(Fill, Item, stock == 3)
required(Item, size == small)
"#,
    );
    let item = d.entity("Item");
    let axes: Vec<&str> = item.axes.iter().map(|a| a.column.as_str()).collect();
    assert_eq!(axes, vec!["stock", "size"]);
    assert_eq!(item.axes[0].kind, AxisKind::Int { max: 5 });
    assert_eq!(
        d.rows("Item"),
        vec![
            "size=small, stock=0",
            "size=large, stock=0",
            "size=small, stock=3",
            "size=large, stock=3"
        ],
        "creation chooses the variation freely"
    );
    assert!(d.has("Item", |x| matches!(
        x,
        StateDiag::ConstraintViolated { .. }
    )));
}

#[test]
fn a_default_does_not_pin_a_variation_choice() {
    let src = |extra: &str| {
        format!(
            r#"
variation Size "サイズ" {{
  small "S"
  large "L"
}}
entity Item "商品" {{ id: Int @pk  size: Size @default(large) }}
entity Stock "在庫" {{ id: Int @pk  size: Size @default(large) }}
usecase Make "作る"
creates(Make, Item)
required(Item, size == small)
required(Stock, size == small)
{extra}
"#
        )
    };
    let d = derive_src(&src(""));
    assert_eq!(d.rows("Item"), vec!["size=small", "size=large"]);
    assert_eq!(
        d.rows("Stock"),
        vec!["size=small", "size=large"],
        "a pre-existing instance starts at any value"
    );
    let d = derive_src(&src("sets(Make, Item, size == large)"));
    assert_eq!(
        d.rows("Item"),
        vec!["size=large"],
        "the creating use case sets it"
    );
}

#[test]
fn int_literal_outside_the_bound_is_an_error() {
    let d = derive_src(
        r#"
entity Item "商品" { id: Int @pk  stock: Int @default(0) }
usecase Make "作る"
creates(Make, Item)
sets(Make, Item, stock == 9)
"#,
    );
    assert!(d.has("Item", |x| matches!(
        x,
        StateDiag::IntLiteralOutOfBounds {
            value: 9,
            max: 5,
            ..
        }
    )));
}
