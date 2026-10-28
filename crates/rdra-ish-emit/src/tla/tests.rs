use super::*;
use rdra_ish_core::analysis::build_model;
use rdra_ish_core::lifecycle::{has_errors, Bounds, Scope};
use rdra_ish_syntax::parse;

fn system(src: &str) -> TransitionSystem {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::All);
    assert!(!has_errors(&built.diags), "{:?}", built.diags);
    built.ts
}

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("../../samples/formal-verification/{name}"))
        .expect("formal-verification sample")
}

#[test]
fn tla_order_snapshot() {
    let bundle = print(&system(&sample("order.rdra")), PrintMode::Check, "RdraSpec");
    insta::assert_snapshot!(bundle.tla);
    insta::assert_snapshot!("order_cfg", bundle.cfg);
}

#[test]
fn check_cfg_names_each_invariant_and_explore_cfg_checks_nothing() {
    let ts = system(&sample("order.rdra"));
    let check = print(&ts, PrintMode::Check, "M").cfg;
    assert!(
        check.contains("INVARIANT TypeOK\nINVARIANT Forbidden_Order_0\nINVARIANT Inv_Order_0\n")
    );
    assert!(check.contains("PROPERTY AfterAssert_DeliverOrder_0\n"));
    assert!(!check.contains("Safety"));
    let explore = print(&ts, PrintMode::Explore, "M").cfg;
    assert_eq!(explore, "SPECIFICATION Spec\nCHECK_DEADLOCK FALSE\n");
}

#[test]
fn a_one_to_one_key_is_a_unique_invariant_over_two_binders() {
    let bundle = print(
        &system(&sample("one_to_one_shared.rdra")),
        PrintMode::Check,
        "M",
    );
    assert!(bundle.tla.contains(
        "Unique_Shipment_Order == \\A b0 \\in Shipment_Ids, b1 \\in Shipment_Ids : (Shipment_exists[b0] = \"present\" /\\ Shipment_exists[b1] = \"present\") => ((b0 = b1) \\/ (Shipment_owner_Order[b0] # Shipment_owner_Order[b1]))\n"
    ), "{}", bundle.tla);
    assert!(bundle.cfg.contains("INVARIANT Unique_Shipment_Order\n"));
    let optional = system(
        "entity A \"A\" { id: Int @pk }\nentity B \"B\" { id: Int @pk }\nusecase U \"U\"\ncreates(U, B)\nrelate(B, A, 1:1).optional()\n",
    );
    assert!(print(&optional, PrintMode::Check, "M")
        .tla
        .contains("((b0 = b1) \\/ (B_owner_A[b0] # B_owner_A[b1]) \\/ (B_owner_A[b0] = 0))"));
}

#[test]
fn a_child_side_minimum_is_an_invariant_over_a_parent_and_a_witness_child() {
    let bundle = print(
        &system(&sample("min_card_unmet.rdra")),
        PrintMode::Check,
        "M",
    );
    assert!(bundle.tla.contains(
        "MinCard_OrderLine_Order == \\A b0 \\in Order_Ids : (Order_exists[b0] = \"present\") => (\\E b1 \\in OrderLine_Ids : (OrderLine_exists[b1] = \"present\") /\\ (OrderLine_owner_Order[b1] = b0))\n"
    ), "{}", bundle.tla);
    assert!(bundle.cfg.contains("INVARIANT MinCard_OrderLine_Order\n"));
    let master = system(
        "entity A \"A\" { id: Int @pk }\nentity B \"B\" { id: Int @pk }\nrelate(B, A, 1..*:1)\n",
    );
    assert!(print(&master, PrintMode::Check, "M")
        .tla
        .contains("  /\\ B_owner_A = [i \\in B_Ids |-> i]\n"));
}

#[test]
fn a_column_check_is_an_invariant_that_passes_on_null() {
    let bundle = print(&system(&sample("check_unmet.rdra")), PrintMode::Check, "M");
    assert!(bundle.tla.contains(
        "Check_CartLine_qty_0 == \\A b0 \\in CartLine_Ids : (CartLine_exists[b0] = \"present\") => (CartLine_qty[b0] >= 1)\n"
    ), "{}", bundle.tla);
    assert!(bundle.cfg.contains("INVARIANT Check_CartLine_qty_0\n"));
    let nullable = system(
        "entity L \"L\" { id: Int @pk  qty: Int @null @check(qty >= 1) }\nusecase A \"A\"\ncreates(A, L)\nsets(A, L, qty == 2)\n",
    );
    let tla = print(&nullable, PrintMode::Check, "M").tla;
    assert!(tla.contains(
        "Check_L_qty_0 == \\A b0 \\in L_Ids : (L_exists[b0] = \"present\") => ((L_qty[b0] = -1) \\/ (L_qty[b0] # -1 /\\ L_qty[b0] >= 1))\n"
    ), "{tla}");
}

#[test]
fn a_pre_existing_child_owner_starts_linked() {
    let src = |card: &str| {
        format!("entity A \"A\" {{ id: Int @pk }}\nentity B \"B\" {{ id: Int @pk }}\nrelate(B, A, {card})\n")
    };
    let n_to_one = print(&system(&src("N:1")), PrintMode::Check, "M").tla;
    assert!(
        n_to_one.contains("  /\\ B_owner_A \\in [B_Ids -> A_Ids]\n"),
        "{n_to_one}"
    );
    let one_to_one = print(&system(&src("1:1")), PrintMode::Check, "M").tla;
    assert!(
        one_to_one.contains("  /\\ B_owner_A = [i \\in B_Ids |-> i]\n"),
        "{one_to_one}"
    );
}

#[test]
fn after_assert_reads_the_post_state_of_its_action() {
    let tla = print(&system(&sample("order.rdra")), PrintMode::Check, "M").tla;
    assert!(tla.contains(
        "AfterAssert_DeliverOrder_0 == [][(\\A p0 \\in Order_Ids : DeliverOrder(p0) => ((Order_status'[p0] = \"delivered\") /\\ (Order_delivered_at'[p0] = \"present\")))]_vars"
    ), "{tla}");
}

#[test]
fn clock_ticks_by_one_inside_the_int_range_and_needs_no_fairness() {
    let tla = print(&system(&sample("now_coupon.rdra")), PrintMode::Check, "M").tla;
    assert!(
        tla.contains("Tick ==\n  /\\ now < 5\n  /\\ now' = now + 1\n"),
        "{tla}"
    );
    assert!(tla.contains("  \\/ Tick\n"));
    assert!(tla.contains("Spec == Init /\\ [][Next]_vars\n"));
    assert!(tla.contains("(~(Coupon_expired_at[b0] < now))"));
}

const NULLABLE: &str = r#"
entity Item "Item" {
  id: Int @pk
  qty: Int @null
}
usecase Make "Make"
usecase Fill "Fill"
usecase Drop "Drop"
creates(Make, Item)
updates(Fill, Item)
deletes(Drop, Item)
sets(Fill, Item, qty == 3)
forbidden(Item, qty < 2)
"#;

#[test]
fn null_int_is_minus_one_and_never_ordered() {
    let tla = print(&system(NULLABLE), PrintMode::Check, "M").tla;
    assert!(
        tla.contains("Item_qty \\in [Item_Ids -> IntRange \\cup {-1}]"),
        "{tla}"
    );
    assert!(tla.contains("Item_qty = [i \\in Item_Ids |-> -1]"));
    assert!(
        tla.contains("~(Item_qty[b0] # -1 /\\ Item_qty[b0] < 2)"),
        "{tla}"
    );
}

#[test]
fn delete_resets_the_slot_and_marks_it_deleted() {
    let tla = print(&system(NULLABLE), PrintMode::Check, "M").tla;
    assert!(
        tla.contains("Item_exists' = [Item_exists EXCEPT ![p0] = \"deleted\"]"),
        "{tla}"
    );
    assert!(
        tla.contains("Item_qty' = [Item_qty EXCEPT ![p0] = -1]"),
        "{tla}"
    );
}

#[test]
fn names_avoid_the_module_own_identifiers() {
    let src = r#"
entity E "E" {
  id: Int @pk
  status: Enum(a, b) @default(a)
}
usecase Init "Init"
usecase Next "Next"
creates(Init, E)
updates(Next, E)
transitions(E.status, Next, a -> b)
"#;
    let tla = print(&system(src), PrintMode::Check, "M").tla;
    assert!(tla.contains("Init_2(p0) =="), "{tla}");
    assert!(tla.contains("Next_2(p0) =="), "{tla}");
    assert!(tla.contains("\\E p0 \\in E_Ids : Next_2(p0)"));
}

#[test]
fn values_render_as_tlc_prints_them() {
    let enumd = Domain::Enum(vec!["a\"b".to_string()]);
    assert_eq!(render_value(&enumd, Value(0)), "\"a\\\"b\"");
    assert_eq!(render_value(&Domain::NullableInt, Value(0)), "-1");
    assert_eq!(render_value(&Domain::NullableInt, Value(4)), "3");
    let parent = system(&sample("order.rdra")).entity_ids().next().unwrap();
    let owner = |optional| Domain::Instance { parent, optional };
    assert_eq!(render_value(&owner(false), Value(0)), "1");
    assert_eq!(render_value(&owner(true), Value(0)), "0");
}

#[test]
fn module_name_is_sanitized() {
    let ts = system(&sample("order.rdra"));
    assert_eq!(
        print(&ts, PrintMode::Check, "my-spec.tla").module_name,
        "my_spec"
    );
    assert_eq!(print(&ts, PrintMode::Check, "").module_name, "RdraSpec");
}

#[test]
fn owner_vars_take_the_role_only_when_one_is_given() {
    let ts = system(
        r#"
entity Address "Address" { id: Int @pk }
entity Order "Order" { id: Int @pk }
usecase Place "Place"
creates(Place, Order)
relate(Order, Address, N:1)
relate(Order, Address, N:1).role(bill_to).optional()
"#,
    );
    let names = TlaNames::new(&ts);
    let owners: Vec<&str> = names
        .vars()
        .iter()
        .map(String::as_str)
        .filter(|n| n.contains("_owner_"))
        .collect();
    assert_eq!(owners, ["Order_owner_Address", "Order_owner_bill_to"]);
}

#[test]
fn along_by_links_through_the_owner_of_that_role() {
    let src = |role: &str| {
        format!(
            r#"
entity Address "Address" {{ id: Int @pk  status: Enum(open, closed) }}
entity Order "Order" {{ id: Int @pk  status: Enum(pending, shipped) }}
usecase Place "Place"
usecase Close "Close"
usecase Ship "Ship"
creates(Place, Order)
transitions(Address.status, Close, open -> closed)
transitions(Order.status, Ship, pending -> shipped)
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
forbidden(Order, Address, Order.status == shipped, Address.status == closed)
  .along(Order, Address).by({role})
"#
        )
    };
    for (role, other) in [("ship_to", "bill_to"), ("bill_to", "ship_to")] {
        let tla = print(&system(&src(role)), PrintMode::Check, "M").tla;
        let line = tla
            .lines()
            .find(|l| l.starts_with("CrossForbidden_"))
            .expect("the forbidden prints as an invariant");
        assert!(line.contains(&format!("Order_owner_{role}")), "{line}");
        assert!(!line.contains(&format!("Order_owner_{other}")), "{line}");
    }
}
