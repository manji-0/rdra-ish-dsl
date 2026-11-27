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
sets(CloseStore, Store.status == closed)
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
transitions(usecase::Pay, Order.status, pending -> paid)
transitions(usecase::Cancel, Order.status, pending -> cancelled)
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
transitions(usecase::DeliverUc, Order.status, pending -> delivered)
sets(usecase::DeliverUc, Order.delivered_at == present)
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
transitions(Pay, Order.status, pending -> paid)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Cancel, Order.status, paid -> cancelled)
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
sets(usecase::SetA, Multi.a == present)
sets(usecase::SetB, Multi.b == present)
sets(usecase::SetC, Multi.c == present)
sets(usecase::SetD, Multi.d == present)
"#,
    );
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let run = built
        .ts
        .explore_targets(&built.selection.targets, Caps::new(5, 1000));
    let views = run.entity_views(&built.ts, &model, &built.diags);
    assert!(views[0].truncated);
    assert!(views[0].patterns.len() <= 5);
    assert!(views[0]
        .diagnostics
        .contains(&StateDiag::PatternCapReached { cap: 5, bound: 16 }));
}

#[test]
fn state_cap_names_what_the_reached_states_vary_in() {
    let model = model_from(
        r#"
entity Multi "マルチ" { id: Int @pk  a: String @null  b: String @null  n: Int @default("1") }
usecase Create "生成"
usecase SetA   "Aセット"
usecase SetB   "Bセット"
creates(Create, Multi)
sets(usecase::SetA, Multi.a == present)
sets(usecase::SetB, Multi.b == present)
sets(usecase::SetB, Multi.n == 3)
"#,
    );
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let run = built
        .ts
        .explore_targets(&built.selection.targets, Caps::new(usize::MAX, 3));
    let views = run.entity_views(&built.ts, &model, &built.diags);
    let growth = views
        .iter()
        .flat_map(|v| &v.diagnostics)
        .find_map(|d| match d {
            StateDiag::StateCapReached { cap: 3, growth } => Some(growth.clone()),
            _ => None,
        })
        .expect("the cap stops the run");
    let spread = |name: &str| {
        let v = growth.vars.iter().find(|v| v.name == name).expect(name);
        (v.reached, v.domain, v.range.clone())
    };
    // absent at the start, then present after Create, then a set by SetA.
    assert_eq!(spread("Multi._exists"), (2, 3, None));
    assert_eq!(spread("Multi.a"), (2, 2, None));
    assert_eq!(spread("Multi.b"), (1, 2, None));
    assert_eq!(spread("Multi.n"), (1, 6, Some("Int 0..5".into())));
    assert_eq!(growth.vars[0].reached, 2, "most values first");
    assert_eq!(
        growth.usecases,
        vec![("Create".to_string(), 1), ("SetA".to_string(), 1)]
    );
    assert_eq!(
        growth.to_string(),
        "the cone has 4 variables, 2 of them vary (values reached/domain: Multi._exists 2/3, \
         Multi.a 2/2); new states first reached by: Create 1, SetA 1"
    );
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
sets(usecase::Publish, Doc.metadata == present)
"#,
    );
    assert_eq!(d.rows("Doc"), vec!["metadata=null", "metadata=present"]);
    assert_eq!(d.entity("Doc").axes[0].kind, AxisKind::Nullable);
}

/// An Enum or Bool column marked `@null` holds null: a created row starts there unless a
/// `@default` names a value, and `sets` writes null, a value, or any value (`present`)
/// (#359).
#[test]
fn nullable_enum_and_bool_columns_start_null() {
    let d = derive_src(
        r#"
entity Item "品目" {
  id: Int @pk
  k: Enum(red, blue) @null
  f: Bool @null
  d: Enum(red, blue) @null @default(blue)
  e: Bool @null @default(true)
}
usecase Make  "作成"
usecase Paint "塗装"
usecase Clear "消去"
usecase Flag  "印付け"
creates(Make, Item)
sets(Paint, Item.k == red)
sets(Clear, Item.k == null)
sets(Flag, Item.f == present)
"#,
    );
    let item = d.entity("Item");
    let kinds: Vec<&AxisKind> = item.axes.iter().map(|a| &a.kind).collect();
    let rb = vec!["red".to_string(), "blue".to_string()];
    assert_eq!(
        kinds,
        vec![
            &AxisKind::NullableEnum(rb.clone()),
            &AxisKind::NullableBool,
            &AxisKind::NullableEnum(rb),
            &AxisKind::NullableBool,
        ]
    );
    assert_eq!(item.bound(), 3 * 3 * 3 * 3);
    let rows = d.rows("Item");
    assert_eq!(rows[0], "d=blue, e=true, f=null, k=null", "{rows:?}");
    for want in [
        "d=blue, e=true, f=null, k=red",
        "d=blue, e=true, f=false, k=null",
        "d=blue, e=true, f=true, k=null",
    ] {
        assert!(rows.iter().any(|r| r == want), "{want} not in {rows:?}");
    }
    assert!(
        !rows.iter().any(|r| r.contains("k=blue")),
        "nothing writes blue: {rows:?}"
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
sets(usecase::Enable, Switch.enabled == true)
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
transitions(Deactive, Item.status, active -> inactive)
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

/// A pattern's provenance names the BUC of each use case, whether the BUC contains it or
/// holds it through a step of one of its flows (#161); of several BUCs, the first by id.
#[test]
fn provenance_names_the_buc_a_use_case_enters_through_a_flow_step() {
    let d = derive_src(
        r#"
buc Zeta "後"
buc Billing "請求業務"
flow BillingFlow "フロー"
step IssueStep "発行"
step CloseStep "締め"
usecase IssueBill "発行する"
usecase CloseBill "締める"
entity Bill "請求" { id: Int @pk  status: Enum(draft, issued, closed) @default(draft) }
contains(Zeta, IssueBill)
contains(Billing, BillingFlow)
contains(Billing, IssueBill)
contains(BillingFlow, IssueStep)
contains(BillingFlow, CloseStep)
precedes(IssueStep, CloseStep)
covers(IssueStep, IssueBill)
covers(CloseStep, CloseBill)
creates(IssueBill, Bill)
updates(CloseBill, Bill)
sets(IssueBill, Bill.status == issued)
transitions(CloseBill, Bill.status, issued -> closed)
"#,
    );
    let via: Vec<(Option<&str>, &str)> = d
        .entity("Bill")
        .patterns
        .iter()
        .flat_map(|p| p.provenance.via.iter())
        .map(|(buc, uc)| (buc.as_deref(), uc.as_str()))
        .collect();
    assert!(via.contains(&(Some("Billing"), "CloseBill")), "{via:?}");
    assert!(via.contains(&(Some("Billing"), "IssueBill")), "{via:?}");
    assert!(via.iter().all(|(buc, _)| buc.is_some()), "{via:?}");
}

/// A `deletes` removes one row; a child under an `.on_delete(cascade)` key stays, so every
/// rule still sees it (#85). The use case that deletes such a parent draws the info
/// `DeleteLeavesChildren`, once per key; a key with no cascade, or a use case that deletes
/// the child too, draws nothing.
#[test]
fn a_delete_leaves_the_children_of_a_cascade_key_and_says_so() {
    let src = |on_delete: &str, extra: &str| {
        format!(
            r#"
entity Parent "親" {{ id: Int @pk }}
entity Child "子" {{ id: Int @pk  alive: Bool @default(true) }}
relate(Child, Parent, N:1){on_delete}
usecase CreateParent "親作成"
usecase CreateChild "子作成"
usecase DeleteParent "親削除"
creates(CreateParent, Parent)
creates(CreateChild, Child)
deletes(DeleteParent, Parent)
{extra}
invariant(Child).when(alive == true).has(Parent).label("every child has a parent")
"#
        )
    };
    let d = derive_src(&src(".on_delete(cascade)", ""));
    let left: Vec<(String, String, String)> = d
        .diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::DeleteLeavesChildren {
                usecase,
                parent,
                child,
                ..
            } => Some((usecase.clone(), parent.clone(), child.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        left,
        vec![("DeleteParent".into(), "Parent".into(), "Child".into())]
    );
    assert_eq!(
        left_diag(&d).severity(),
        crate::lifecycle::Severity::Warning
    );
    assert!(d.has("Child", |x| matches!(
        x,
        StateDiag::ConstraintViolated {
            text: crate::lifecycle::InvText::Quantifier { .. },
            ..
        }
    )));
    let silent = |on_delete: &str, extra: &str| {
        !derive_src(&src(on_delete, extra))
            .diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::DeleteLeavesChildren { .. }))
    };
    assert!(silent("", ""));
    assert!(silent(
        ".on_delete(cascade)",
        "deletes(DeleteParent, Child)"
    ));
}

/// The export's `ON DELETE SET NULL` / `SET DEFAULT` clears a child's key in the database,
/// which the lifecycle does not do: the child keeps naming the deleted row. The use case that
/// deletes such a parent draws the warning `DeleteLeavesChildKeys`, once per key, naming the
/// action; a use case that also deletes the child, or moves its key (`unlinks`, `relinks`),
/// and a `@soft_delete` parent, which no SQL delete removes, draw nothing (#P63).
#[test]
fn a_delete_leaves_the_keys_of_a_set_null_or_set_default_key_and_says_so() {
    let src = |key: &str, extra: &str, soft: &str| {
        format!(
            r#"
entity Parent "親" {{ id: Int @pk {soft} }}
entity Child "子" {{ id: Int @pk }}
entity Other "他" {{ id: Int @pk }}
relate(Child, Parent, N:1){key}
relate(Child, Other, N:1).optional()
usecase CreateParent "親作成"
usecase CreateChild "子作成"
usecase Attach "紐付け"
usecase DeleteParent "親削除"
creates(CreateParent, Parent)
creates(CreateChild, Child)
links(Attach, Child, Parent)
deletes(DeleteParent, Parent)
{extra}
"#
        )
    };
    let keys = |key: &str, extra: &str, soft: &str| -> Vec<(String, String, String, String)> {
        derive_src(&src(key, extra, soft))
            .diags
            .iter()
            .filter_map(|x| match &x.diag {
                StateDiag::DeleteLeavesChildKeys {
                    usecase,
                    parent,
                    child,
                    action,
                    ..
                } => Some((
                    usecase.clone(),
                    parent.clone(),
                    child.clone(),
                    action.as_str().to_string(),
                )),
                _ => None,
            })
            .collect()
    };
    for (key, action) in [
        (".optional().on_delete(set_null)", "set_null"),
        (".optional().on_delete(set_default)", "set_default"),
    ] {
        assert_eq!(
            keys(key, "", ""),
            vec![(
                "DeleteParent".into(),
                "Parent".into(),
                "Child".into(),
                action.into()
            )],
            "{key}"
        );
        let d = derive_src(&src(key, "", ""));
        let diag = d
            .diags
            .iter()
            .map(|x| &x.diag)
            .find(|x| matches!(x, StateDiag::DeleteLeavesChildKeys { .. }))
            .expect("DeleteLeavesChildKeys");
        assert_eq!(diag.severity(), crate::lifecycle::Severity::Warning);
        assert!(diag.to_string().contains(action), "{diag}");
        assert!(
            keys(key, "deletes(DeleteParent, Child)", "").is_empty(),
            "{key}"
        );
        assert!(
            keys(key, "unlinks(DeleteParent, Child, Parent)", "").is_empty(),
            "{key}"
        );
        assert!(
            keys(key, "relinks(DeleteParent, Child, Parent)", "").is_empty(),
            "{key}"
        );
        assert!(
            keys(key, "", "deleted_at: DateTime @null @soft_delete").is_empty(),
            "{key}"
        );
    }
    // A cascade key draws DeleteLeavesChildren instead, and a restrict key neither.
    assert!(keys(".optional().on_delete(cascade)", "", "").is_empty());
    assert!(keys(".optional().on_delete(restrict)", "", "").is_empty());
}

fn left_diag(d: &Derived) -> &StateDiag {
    d.diags
        .iter()
        .map(|x| &x.diag)
        .find(|x| matches!(x, StateDiag::DeleteLeavesChildren { .. }))
        .expect("DeleteLeavesChildren")
}

#[test]
fn creation_status_is_a_start_even_off_the_transition_start() {
    let d = derive_src(
        r#"
entity Order "O" { id: Int @pk  status: Enum(draft, placed, shipped) @default(draft) }
usecase Place "place"
usecase Ship "ship"
creates(Place, Order)
sets(Place, Order.status == shipped)
updates(Ship, Order)
transitions(Ship, Order.status, placed -> shipped)
"#,
    );
    let o = d.entity("Order");
    assert_eq!(d.rows("Order"), vec!["status=shipped"]);
    assert!(o.patterns[0].is_initial);
    assert!(!d.has("Order", |x| matches!(
        x,
        StateDiag::DoubleModeledEnum { .. }
    )));
    // `placed` is a transition endpoint that nothing enters: the one error is its witness.
    let errors: Vec<String> = d
        .diags
        .iter()
        .filter(|x| x.diag.severity() == crate::lifecycle::Severity::Error)
        .map(|x| x.diag.name(&[]))
        .collect();
    assert_eq!(errors, ["Witness_Order_status_placed"]);
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
transitions(Close, Order.status, open -> closed)
sets(Reopen, Order.status == open)
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
sets(Restock, Item.stock == 1)
sets(Restock, Item.stock == 2)
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
sets(Flag, Order.flagged == true)
"#,
    );
    assert_eq!(d.rows("Order"), vec!["flagged=false", "flagged=true"]);
}

#[test]
fn shared_use_case_is_atomic_within_a_transaction_group() {
    let src = |relate: &str| {
        format!(
            r#"
entity Order "注文" {{ id: Int @pk  status: Enum(pending, paid) @default(pending) }}
entity Payment "決済" {{ id: Int @pk  status: Enum(pending, paid) @default(pending) }}
usecase Init "初期化"
usecase Flip "更新"
creates(Init, Order)
creates(Init, Payment)
updates(Flip, Order)
updates(Flip, Payment)
transitions(Flip, Order.status, pending -> paid)
transitions(Flip, Payment.status, paid -> pending)
{relate}
"#
        )
    };
    let d = derive_src(&src("relate(Payment, Order, N:1)"));
    assert_eq!(d.rows("Payment"), vec!["status=pending"]);
    // One atomic Flip needs Payment paid, which nothing reaches, so Order stays pending.
    assert_eq!(d.rows("Order"), vec!["status=pending"]);
    // Unrelated, the two writes are two transaction groups, so Flip_2 pays the Order alone.
    let d = derive_src(&src(""));
    assert_eq!(d.rows("Payment"), vec!["status=pending"]);
    assert_eq!(d.rows("Order"), vec!["status=pending", "status=paid"]);
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
sets(Fill, Item.stock == 3)
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
    let d = derive_src(&src("sets(Make, Item.size == large)"));
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
sets(Make, Item.stock == 9)
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

/// A numeric `@default` the bounds cannot hold is the same error as a literal there, never a
/// start of 0 (#180). The bound is `0..=int_max`, so the top itself is a valid default.
#[test]
fn a_default_outside_the_bound_is_an_error_and_the_top_is_not() {
    let out_of_bounds = |column: &str, default: &str, int_max: u8| {
        let src = format!(
            r#"
entity Item "商品" {{ id: Int @pk  {column} @default("{default}") }}
usecase Make "作る"
creates(Make, Item)
forbidden(Item, v == 0)
"#
        );
        let bounds = Bounds {
            int_max,
            ..Bounds::STATES
        };
        let d = derive_scoped(&model_from(&src), bounds, &Scope::ALL);
        d.entity("Item").diagnostics.iter().find_map(|x| match x {
            StateDiag::IntLiteralOutOfBounds { value, max, .. } => Some((*value, *max)),
            _ => None,
        })
    };
    assert_eq!(out_of_bounds("v: Int", "5", 5), None, "the top of 0..5");
    assert_eq!(out_of_bounds("v: Int", "6", 5), Some((6, 5)));
    assert_eq!(out_of_bounds("v: Int", "6", 6), None, "the top of 0..6");
    assert_eq!(out_of_bounds("v: Int", "7", 6), Some((7, 6)));
    assert_eq!(out_of_bounds("v: Int", "0", 5), None);
    assert_eq!(out_of_bounds("v: Int", "-1", 5), Some((-1, 5)));
    assert_eq!(out_of_bounds("v: Int @null", "7", 5), Some((7, 5)));
    assert_eq!(out_of_bounds("v: Date", "9", 5), Some((9, 5)));
    assert_eq!(out_of_bounds("v: Money", "100", 5), Some((100, 5)));
}

/// A driven comparison is a variable of its own, which only `drives` moves (#89). A use case
/// that `sets` a column the comparison reads and does not drive it leaves the comparison at its
/// last truth: the warning `DrivenComparisonColumnWritten` names it, once per column. A use
/// case that drives the comparison itself draws nothing.
#[test]
fn a_sets_of_a_column_a_driven_comparison_reads_says_the_comparison_goes_stale() {
    let src = |restock: &str| {
        format!(
            r#"
entity Stock "在庫" {{ id: Int @pk  stock: Int @default(3)  selling: Int @default(1) }}
usecase Sell "販売"
usecase Restock "入荷"
updates(Sell, Stock)
drives(Sell, Stock.stock < selling, true)
updates(Restock, Stock)
sets(Restock, Stock.stock == 5)
{restock}
forbidden(Stock, stock == 5, stock < selling)
"#
        )
    };
    let written = |restock: &str| {
        derive_src(&src(restock))
            .diags
            .iter()
            .filter_map(|x| match &x.diag {
                StateDiag::DrivenComparisonColumnWritten {
                    usecase,
                    column,
                    drivers,
                    ..
                } => Some((usecase.clone(), column.clone(), drivers.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        written(""),
        vec![("Restock".into(), "stock".into(), vec!["Sell".into()])]
    );
    assert!(written("drives(Restock, Stock.stock < selling, false)").is_empty());
    let d = derive_src(&src(""));
    let diag = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .find(|x| matches!(x, StateDiag::DrivenComparisonColumnWritten { .. }))
        .expect("DrivenComparisonColumnWritten");
    assert_eq!(diag.severity(), crate::lifecycle::Severity::Warning);
    assert!(
        diag.to_string()
            .contains("drives(Restock, Stock.stock < selling, true)"),
        "{diag}"
    );
}

/// A variation column starts at any of its values, so its `@default` picks nothing (#89): the
/// info `DefaultOnVariationIgnored` says so on a row no use case creates and on a created row
/// some creation leaves unset. When every creation sets the column, nothing is said.
#[test]
fn a_default_on_a_variation_column_says_it_does_not_pick_the_start() {
    let src = |extra: &str| {
        format!(
            r#"
variation Method "支払方法" {{
  cash "現金"
  card "カード"
}}
entity Account "口座" {{ id: Int @pk  method: Method @default(cash)  active: Bool @default(true) }}
entity Ticket "伝票" {{ id: Int @pk  method: Method @default(cash)  open: Bool @default(true) }}
usecase Close "解約"
usecase Open "起票"
updates(Close, Account)
sets(Close, Account.active == false)
creates(Open, Ticket)
{extra}
forbidden(Account, method == card, active == false)
forbidden(Ticket, method == card, open == false)
"#
        )
    };
    let ignored = |extra: &str| {
        let d = derive_src(&src(extra));
        let mut out: Vec<(String, bool)> = Vec::new();
        for entity in ["Account", "Ticket"] {
            out.extend(d.entity(entity).diagnostics.iter().filter_map(|x| match x {
                StateDiag::DefaultOnVariationIgnored {
                    column, created, ..
                } => Some((format!("{entity}.{column}"), *created)),
                _ => None,
            }));
        }
        out
    };
    assert_eq!(
        ignored(""),
        vec![
            ("Account.method".to_string(), false),
            ("Ticket.method".to_string(), true)
        ]
    );
    assert_eq!(
        ignored("sets(Open, Ticket.method == cash)"),
        vec![("Account.method".to_string(), false)]
    );
    let d = derive_src(&src(""));
    let info = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .find(|x| {
            matches!(
                x,
                StateDiag::DefaultOnVariationIgnored { created: true, .. }
            )
        })
        .expect("DefaultOnVariationIgnored");
    assert_eq!(info.severity(), crate::lifecycle::Severity::Info);
}

/// Each action writes each slot at most once, the invariant the native step and the TLA+
/// printer both rely on: `transitions`, `sets`, and `chooses` on one column of one use case
/// give one write. A second write of a slot is what `Action::slots_written_twice` names, and
/// `build` fails such a model with `ConflictingEffects`.
#[test]
fn an_action_writes_each_slot_once() {
    let built = built_from(
        r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  note: Enum(x, y) @default(x)
}
usecase Place "注文確定"
usecase Pay "支払い"
creates(Place, Order)
updates(usecase::Pay, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Pay, Order.status, pending -> cancelled)
transitions(Pay, Order.status, paid -> cancelled)
sets(Pay, Order.note == y)
"#,
        Bounds::VERIFY,
    );
    assert!(
        !built
            .diags
            .iter()
            .any(|d| matches!(d.diag, StateDiag::ConflictingEffects { .. })),
        "{:?}",
        built.diags
    );
    for a in built.ts.actions() {
        assert!(a.slots_written_twice().is_empty(), "{}", a.name);
    }
    let pay = (built.ts.actions().iter())
        .find(|a| a.name == "Pay")
        .expect("Pay");
    let mut twice = pay.clone();
    let write = (pay.effects.iter())
        .find(|e| matches!(e, crate::lifecycle::types::Effect::Transition { .. }))
        .expect("a transition")
        .clone();
    twice.effects.push(write);
    assert_eq!(twice.slots_written_twice().len(), 1);
}

/// A guarded `sets` of a created row's lifecycle column and a `transitions` of the same use
/// case write one slot twice: a `Choose` and then a `Transition` with a choice. The native
/// step applied both and kept the chosen value; the printed TLA+ let the transition's
/// choice replace it, so on this model the native checker found the forbidden state and TLC
/// did not (P24 of docs/theory/README.md). The build now fails the model instead.
#[test]
fn a_created_rows_guarded_set_beside_its_transitions_conflicts() {
    let built = built_from(
        r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
}
entity Flag "旗" { id: Int @pk  on: Bool @default(false) }
usecase Raise "旗上げ"
usecase Place "注文確定"
creates(Raise, Flag)
creates(Place, Order)
condition FlagUp "旗が上がっている"
holds(FlagUp, Flag.on == false)
sets(Place, Order.status == cancelled).when(FlagUp)
transitions(Place, Order.status, pending -> paid)
transitions(Place, Order.status, paid -> cancelled)
transitions(Place, Order.status, paid -> pending)
forbidden(Order, status == cancelled)
"#,
        Bounds::VERIFY,
    );
    assert!(
        built.diags.iter().any(|d| matches!(
            &d.diag,
            StateDiag::ConflictingEffects { usecase, column, .. }
                if usecase == "Place" && column.contains("status")
        )),
        "{:?}",
        built.diags
    );
}
