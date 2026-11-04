use super::*;
use rdra_ish_core::analysis::build_model;
use rdra_ish_core::lifecycle::{has_errors, Bounds, Caps, Scope, Target};
use rdra_ish_syntax::parse;

fn system(src: &str) -> TransitionSystem {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    assert!(diags.iter().all(|d| d.is_warning), "{diags:?}");
    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL);
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

const PUBLISHED_OUTBOX: &str = r#"
system Auth "Auth"
actor User "User"
usecase Login "Log in"
performs(User, Login)
entity Session "Session" { id: Int @pk  status: Enum(active, revoked) @default(active) }
entity LoginRecord "Login record" { id: Int @pk  result: Enum(succeeded, failed) }
creates(Login, Session)
creates(Login, LoginRecord)
sets(Login, LoginRecord.result == succeeded)
exposes(Auth, LoginRecord).append()
"#;

#[test]
fn tla_published_outbox_is_one_action_snapshot() {
    let bundle = print(&system(PUBLISHED_OUTBOX), PrintMode::Check, "RdraSpec");
    assert!(bundle.tla.contains("\nLogin(p0) ==\n"), "{}", bundle.tla);
    assert!(!bundle.tla.contains("Login_1"), "{}", bundle.tla);
    insta::assert_snapshot!(bundle.tla);
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
fn a_property_cfg_checks_that_property_alone() {
    let ts = system(&sample("order.rdra"));
    let temporal = print(&ts, PrintMode::Temporal, "M");
    let one = print(&ts, PrintMode::Property(1), "M");
    let props = |cfg: &str| -> Vec<String> {
        cfg.lines()
            .filter(|l| l.starts_with("PROPERTY") || l.starts_with("INVARIANT"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        props(&temporal.cfg),
        [
            "INVARIANT TypeOK",
            "PROPERTY PaidLeadsToShipped",
            "PROPERTY EventuallyTerminal"
        ]
    );
    assert_eq!(
        props(&one.cfg),
        ["INVARIANT TypeOK", "PROPERTY EventuallyTerminal"]
    );
    assert_eq!(one.tla, temporal.tla, "the same module, fairness included");
}

/// The name BFS gives a failed after-assert is the property TLC checks and reports.
#[test]
fn the_bfs_after_assert_name_is_the_tla_property_name() {
    let src = sample("after_assert_unmet.rdra");
    let (ast, _) = parse(&src);
    let (model, _) = build_model(&ast);
    let report = rdra_ish_core::lifecycle::derive_states(
        &model,
        &Scope::ALL,
        rdra_ish_core::lifecycle::Caps::STATES,
    );
    let names: Vec<String> = report
        .diags
        .iter()
        .filter(|d| {
            matches!(
                d.diag,
                rdra_ish_core::lifecycle::StateDiag::AfterAssertViolated { .. }
            )
        })
        .map(|d| d.diag.name(&[]))
        .collect();
    assert_eq!(names, ["AfterAssert_DeliverOrder_0"]);
    let cfg = print(&system(&src), PrintMode::Check, "M").cfg;
    assert!(cfg.contains(&format!("PROPERTY {}\n", names[0])), "{cfg}");
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
fn a_none_guard_on_a_condition_is_a_universal_over_the_related_ids() {
    let tla = print(
        &system(&sample("quantifier_guard.rdra")),
        PrintMode::Check,
        "M",
    )
    .tla;
    let revoke = tla
        .split("\nRevoke(p0) ==\n")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .unwrap_or_else(|| panic!("no Revoke action in {tla}"));
    assert!(
        revoke.contains("\\A b0 \\in TerminalCertAssignment_Ids :"),
        "{revoke}"
    );
    assert!(
        revoke.contains("TerminalCertAssignment_owner_ClientCertificate[b0] = p0)"),
        "{revoke}"
    );
}

#[test]
fn a_none_guard_without_a_relate_ranges_over_every_row() {
    let src = r#"
entity Payment "P" { id: Int @pk  status: Enum(open, paid) }
entity Room "R" { id: Int @pk  state: Enum(free, busy) }
usecase Open "O"
usecase Pay "Pay"
usecase Occupy "Occ"
usecase Prepare "Prep"
creates(Open, Payment)
transitions(Pay, Payment.status, open -> paid)
creates(Prepare, Room)
transitions(Occupy, Room.state, free -> busy)
condition Quiet "Q"
holds(Quiet, Payment.status == open).none(Room.state == busy)
applies_to(Quiet, Pay)
"#;
    let tla = print(&system(src), PrintMode::Check, "M").tla;
    let pay = tla
        .split("\nPay(p0) ==\n")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .unwrap_or_else(|| panic!("no Pay action in {tla}"));
    assert!(
        pay.contains("\\A b0 \\in Room_Ids : (Room_exists[b0] = \"present\") => (~(Room_state[b0] = \"busy\"))"),
        "{pay}"
    );
}

#[test]
fn a_column_check_is_an_invariant_that_fails_on_null() {
    let bundle = print(&system(&sample("check_unmet.rdra")), PrintMode::Check, "M");
    assert!(bundle.tla.contains(
        "Check_CartLine_qty_0 == \\A b0 \\in CartLine_Ids : (CartLine_exists[b0] = \"present\") => (CartLine_qty[b0] >= 1)\n"
    ), "{}", bundle.tla);
    assert!(bundle.cfg.contains("INVARIANT Check_CartLine_qty_0\n"));
    let nullable = system(
        "entity L \"L\" { id: Int @pk  qty: Int @null @check(qty >= 1) }\nusecase A \"A\"\ncreates(A, L)\nsets(A, L.qty == 2)\n",
    );
    let tla = print(&nullable, PrintMode::Check, "M").tla;
    assert!(tla.contains(
        "Check_L_qty_0 == \\A b0 \\in L_Ids : (L_exists[b0] = \"present\") => (L_qty[b0] # -1 /\\ L_qty[b0] >= 1)\n"
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
sets(Fill, Item.qty == 3)
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
transitions(Next, E.status, a -> b)
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
transitions(Close, Address.status, open -> closed)
transitions(Ship, Order.status, pending -> shipped)
relate(Order, Address, N:1).role(ship_to)
relate(Order, Address, N:1).role(bill_to)
forbidden(Order, Address, Order.status == shipped, Address.status == closed)
  .along(Order, Address).role({role})
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

/// `decode` inverts `canon` on every reached state, and rejects a text that is no value of
/// its domain, a key the system does not have, and a missing key.
#[test]
fn decode_inverts_canon() {
    let caps = rdra_ish_core::lifecycle::Caps::new(usize::MAX, 100_000);
    for name in [
        "order.rdra",
        "cross_order_payment_split.rdra",
        "now_coupon.rdra",
        "int_stock.rdra",
        "one_to_one_shared.rdra",
    ] {
        let run = system(&sample(name)).run(caps);
        let ts = &run.system;
        let names = TlaNames::new(ts);
        for s in &run.states {
            let canon = names.canon(ts, s);
            assert_eq!(names.decode(ts, &canon).as_ref(), Some(s), "{name}");
        }
        let canon = names.canon(ts, &run.states[0]);
        let key = canon
            .keys()
            .find(|k| k.contains("_exists"))
            .unwrap()
            .clone();
        let mut bogus = canon.clone();
        bogus.insert(key, "\"bogus\"".to_string());
        assert_eq!(names.decode(ts, &bogus), None, "{name}");
        let mut extra = canon.clone();
        extra.insert("Ghost[1]".to_string(), "1".to_string());
        assert_eq!(names.decode(ts, &extra), None, "{name}");
        let mut missing = canon;
        missing.pop_first();
        assert_eq!(names.decode(ts, &missing), None, "{name}");
    }
}

/// A one-entity `invariant` ranging over related rows takes its scope from their columns, so it
/// prints the same spec as the form that names both entities.
#[test]
fn tla_one_entity_quantified_invariant_matches_the_two_entity_form() {
    let src = sample("quantifier_none.rdra");
    let line = src
        .lines()
        .find(|l| l.contains(".none("))
        .expect("the quantifier rule");
    let related = line.split(").none(").nth(1).expect("the related condition");
    let spec = |rule: String| {
        let model = src.replace(line, &rule);
        print(&system(&model), PrintMode::Check, "RdraSpec").tla
    };
    let one = spec(format!(
        "invariant(Cert).when(status == revoked).none({related}"
    ));
    let two = spec(format!(
        "invariant(Cert, Assign).when(Cert.status == revoked).none({related}"
    ));
    assert!(one.contains("Quantifier_Cert_0"), "{one}");
    assert_eq!(one, two);
}

const CHOICE: &str = r#"
entity Doc "Doc" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
}
usecase Submit "Submit"
usecase Decide "Decide"
creates(Submit, Doc)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, file -> drop)
"#;

/// Arms of one use case from one state are a choice: the post-value ranges over the set of
/// their targets, and the guard names each source state once. Without a choice the action
/// keeps the deterministic `CASE`.
#[test]
fn arms_from_one_state_are_a_choice_over_their_targets() {
    let tla = print(&system(CHOICE), PrintMode::Check, "M").tla;
    let decide = tla
        .split("Decide(p0) ==\n")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .expect("the Decide operator");
    assert_eq!(decide.matches("= \"searched\"").count(), 2, "{decide}");
    assert!(
        decide.contains(
            "  /\\ \\E v0 \\in (CASE Doc_status[p0] = \"searched\" -> {\"file\", \"secret\"} [] Doc_status[p0] = \"file\" -> {\"drop\"} [] OTHER -> {Doc_status[p0]}) : Doc_status' = [Doc_status EXCEPT ![p0] = v0]"
        ),
        "{decide}"
    );
    let single = print(&system(&sample("order.rdra")), PrintMode::Check, "M").tla;
    assert!(!single.contains("\\E v"), "{single}");
}

#[test]
fn a_set_membership_is_in_and_notin() {
    let src = r#"
entity Order "O" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  qty: Int @default(0)
}
usecase Place "P"
usecase Cancel "C"
usecase Bump "B"
creates(Place, Order)
transitions(Cancel, Order.status, pending -> cancelled)
sets(Bump, Order.qty == 2)
condition Unpaid "U"
holds(Unpaid, Order.status not in (paid))
applies_to(Unpaid, Cancel)
forbidden(Order, status in (paid, cancelled), qty in (1, 2))
"#;
    let tla = print(&system(src), PrintMode::Check, "M").tla;
    assert!(tla.contains(r#"Order_status[p0] \notin {"paid"}"#), "{tla}");
    assert!(
        tla.contains(r#"Order_status[b0] \in {"paid", "cancelled"}"#),
        "{tla}"
    );
    assert!(tla.contains(r#"Order_qty[b0] \in {1, 2}"#), "{tla}");
}

const FAIR: &str = r#"
entity App "App" { id: Int @pk  status: Enum(granted, registered) @default(granted) }
entity Patent "Patent" { id: Int @pk }
usecase File "File"
usecase Register "Register"
creates(File, App)
transitions(Register, App.status, granted -> registered)
creates(Register, Patent)
relate(Patent, App, 1:1)
property Ends leads_to(App.status == granted, App.status == registered).fair(Register)
property Plain leads_to(App.status == granted, App.status == registered)
"#;

/// `.fair(UC)` is weak fairness per instance the use case acts on: `\A` over the slots it
/// reads, `\E` over the slot it creates, so the lowest absent slot moving on does not reset an
/// instance's claim. The property assumes it; Spec, the cfg, and a property without `.fair`
/// stay as they are.
#[test]
fn a_fair_property_assumes_weak_fairness_per_instance() {
    let bundle = print(&system(FAIR), PrintMode::Check, "M");
    let tla = &bundle.tla;
    assert!(
        tla.contains(
            "Fair_Ends ==\n  /\\ \\A p0 \\in App_Ids : WF_vars(\\E p1 \\in Patent_Ids : Register(p0, p1))\n\
             Ends == Fair_Ends => (\\A b0 \\in App_Ids : "
        ),
        "{tla}"
    );
    assert!(tla.contains("\nPlain == \\A b0 \\in App_Ids : "), "{tla}");
    assert!(tla.contains("Spec == Init /\\ [][Next]_vars /\\ WF_vars(Next)\n"));
    assert!(bundle.cfg.contains("SPECIFICATION Spec\n"));
    assert!(bundle.cfg.contains("PROPERTY Ends\nPROPERTY Plain\n"));
}

/// A use case split into transaction groups is fair on each of its actions, the later ones
/// per run: the second step binds the application the first step moved (an `Earlier` param,
/// selected by its run mark), so a started run is fair to finish.
#[test]
fn a_fair_split_use_case_is_fair_on_each_action() {
    let split = FAIR.replace("relate(Patent, App, 1:1)\n", "");
    let tla = print(&system(&split), PrintMode::Check, "M").tla;
    assert!(
        tla.contains(
            "Fair_Ends ==\n  /\\ \\A p0 \\in App_Ids : WF_vars(Register_1(p0))\n  /\\ \\A p1 \\in App_Ids : WF_vars(\\E p0 \\in Patent_Ids : Register_2(p0, p1))\n"
        ),
        "{tla}"
    );
    assert!(
        tla.contains("App_run_Register \\in [App_Ids -> {\"none\", \"1\"}]"),
        "{tla}"
    );
    assert!(
        tla.contains("Register_1(p0) ==\n  /\\ App_exists[p0] = \"present\"\n  /\\ App_status[p0] = \"granted\"\n  /\\ App_run_Register[p0] = \"none\"\n"),
        "{tla}"
    );
    assert!(
        tla.contains("Register_2(p0, p1) ==\n  /\\ FirstAbsent(Patent_exists, p0)\n  /\\ App_run_Register[p1] = \"1\"\n"),
        "{tla}"
    );
}

/// `transitions(UC, E.col, (a, b) -> z)` exports the same module as one line per source
/// (#89), guard included, so TLC reads the two spellings as one model.
#[test]
fn a_transition_source_list_exports_as_its_expanded_lines() {
    const ORDER: &str = r#"
usecase Place "Place"
usecase Hold "Hold"
usecase Cancel "Cancel"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, held, paid, cancelled) @default(pending)
  flagged: Bool @default(false)
}
creates(Place, Order)
transitions(Hold, Order.status, pending -> held)
sets(Hold, Order.flagged == true)
condition Flagged "flagged"
holds(Flagged, Order.flagged == true)
"#;
    let listed = format!(
        "{ORDER}\ntransitions(Cancel, Order.status, (pending, held) -> cancelled).when(Flagged)"
    );
    let expanded = format!(
        "{ORDER}\ntransitions(Cancel, Order.status, pending -> cancelled).when(Flagged)\ntransitions(Cancel, Order.status, held -> cancelled).when(Flagged)"
    );
    let listed = print(&system(&listed), PrintMode::Check, "M");
    let expanded = print(&system(&expanded), PrintMode::Check, "M");
    assert!(listed.tla.contains("Cancel(p0) =="), "{}", listed.tla);
    assert_eq!(listed.tla, expanded.tla);
    assert_eq!(listed.cfg, expanded.cfg);
}

/// `.when(Cond)` on a rule exports the module the rule with the body's comparisons written in
/// place exports (#89), for an entity-local invariant and `forbidden`, a cross-entity
/// invariant, and a quantifier.
#[test]
fn a_rule_when_condition_exports_as_its_inline_form() {
    const COUPON: &str = r#"
entity Coupon "Coupon" { id: Int @pk  status: Enum(active, expired) @default(active)  expires_at: Date }
entity Handler "Handler" { id: Int @pk  busy: Bool @default(false) }
relate(Coupon, Handler, N:1)
usecase Issue "Issue"
usecase Expire "Expire"
usecase Free "Free"
creates(Issue, Coupon)
sets(Issue, Coupon.expires_at == 2)
transitions(Expire, Coupon.status, active -> expired)
sets(Free, Handler.busy == false)
condition PastDeadline "past the deadline"
holds(PastDeadline, Coupon.expires_at < now)
condition Busy "busy"
holds(Busy, Handler.busy == true)
"#;
    let with_conditions = format!(
        r#"{COUPON}
invariant(Coupon).when(PastDeadline).then(status == expired)
forbidden(Coupon, status == active).when(PastDeadline)
invariant(Coupon, Handler).along(Coupon, Handler).when(Busy).then(Coupon.status == expired)
invariant(Coupon).when(PastDeadline).none(Handler.busy == true)
"#
    );
    let inline = format!(
        r#"{COUPON}
invariant(Coupon).when(expires_at < now).then(status == expired)
forbidden(Coupon, status == active, expires_at < now)
invariant(Coupon, Handler).along(Coupon, Handler).when(Handler.busy == true).then(Coupon.status == expired)
invariant(Coupon).when(expires_at < now).none(Handler.busy == true)
"#
    );
    let with_conditions = print(&system(&with_conditions), PrintMode::Check, "M");
    let inline = print(&system(&inline), PrintMode::Check, "M");
    for name in [
        "Inv_Coupon_0",
        "Forbidden_Coupon_0",
        "CrossInv_Coupon_0",
        "Quantifier_Coupon_0",
    ] {
        assert!(
            with_conditions.cfg.contains(name),
            "{name}: {}",
            with_conditions.cfg
        );
    }
    assert_eq!(with_conditions.tla, inline.tla);
    assert_eq!(with_conditions.cfg, inline.cfg);
}

/// A gate with an unobserved latch and a key behind it; a job starts only at an open gate.
/// The job's cone reduces the gate and the key (#74).
const GATE: &str = r#"
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
entity Gate "門" { id: Int @pk  status: Enum(shut, half, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Gate, Key, N:1)
relate(Job, Gate, N:1)
usecase Unlock "解錠"
usecase Crack "少し開ける"
usecase Unlatch "掛け金を外す"
usecase Swing "開く"
usecase Submit "投入"
usecase Start "開始"
transitions(Unlock, Key.status, locked -> unlocked)
transitions(Crack, Gate.status, shut -> half)
sets(Unlatch, Gate.latch == true)
transitions(Swing, Gate.status, half -> open)
condition Unlatched "掛け金が外れている"
holds(Unlatched, Gate.latch == true)
applies_to(Unlatched, Swing)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
"#;

/// A reduced cone prints its sub-lifecycle as one variable over its classes: a table of the
/// representative's value per observed var, which the var's name reads, and a successor
/// table per class move, which steps the class and leaves the target side unchanged.
/// The Job view's cone of `src`, reduced.
fn reduced_job_cone(src: &str) -> TransitionSystem {
    let ts = system(src);
    let job = ts
        .targets()
        .into_iter()
        .find(|t| matches!(t, Target::Entity(e) if ts.entity(*e).id == "Job"))
        .expect("the Job view");
    let cone = ts.target_cone(job);
    ts.reduced_cone(&cone, [ts.target_seed(job)], Caps::VERIFY)
}

#[test]
fn a_reduced_cone_prints_its_sub_lifecycle_as_classes() {
    let reduced = reduced_job_cone(GATE);
    let r = reduced
        .reduction()
        .expect("the job's cone reduces the gate");
    assert!(r.classes() > 1);
    let bundle = print(&reduced, PrintMode::Safety, "Cone");
    let tla = &bundle.tla;
    assert!(tla.contains("EXTENDS Integers\n"), "{tla}");
    assert!(
        tla.contains(&format!("S_class \\in 1..{}", r.classes())),
        "{tla}"
    );
    assert!(tla.contains("\nRep_Gate_status == <<"), "{tla}");
    assert!(
        tla.contains("\nGate_status == Rep_Gate_status[S_class]"),
        "{tla}"
    );
    // Each table is one tuple with a cell per class: TLC's evaluation depth stays flat however
    // many classes there are, where a chain of `@@` nests once per entry (and overflowed).
    let swing = tla
        .lines()
        .find_map(|l| l.strip_prefix("Succ_Swing == "))
        .expect("the Swing table");
    assert!(
        swing.starts_with("<<{") && swing.ends_with("}>>") && !swing.contains("@@"),
        "{swing}"
    );
    assert_eq!(swing.matches('{').count(), r.classes(), "{swing}");
    assert!(tla.contains("S_class' \\in Succ_Swing[S_class]"), "{tla}");
    assert!(
        !tla.contains("Gate_latch") && !tla.contains("Key_status"),
        "unobserved sub-lifecycle vars are not state vars: {tla}"
    );
    insta::assert_snapshot!(tla);
}

/// A use case named like a reduction table keeps its name, and the table takes the next free
/// one; SANY rejected the module that defined both.
#[test]
fn a_reduction_table_a_use_case_spells_takes_the_next_free_name() {
    let src = GATE
        .replace("Submit", "Succ_Swing")
        .replace("Start", "Rep_Gate_status");
    let tla = print(&reduced_job_cone(&src), PrintMode::Safety, "Cone").tla;
    for line in [
        "\nRep_Gate_status_2 == <<",
        "\nGate_status == Rep_Gate_status_2[S_class]\n",
        "\nSucc_Swing_2 == <<",
        "\nRep_Gate_status(p0) ==\n",
        "\nSucc_Swing(p0, p1) ==\n",
        "S_class' \\in Succ_Swing_2[S_class]",
    ] {
        assert!(tla.contains(line), "{line:?} missing\n{tla}");
    }
}

/// A temporal property keeps its declared name beside a use case of that name, which the
/// export does not promise to keep (docs/compatibility.md).
#[test]
fn a_property_keeps_its_name_beside_a_use_case_of_that_name() {
    let src = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
usecase Submit "投入"
usecase Start "開始"
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
property Start "作業は待つ"
  always(Job.status == waiting)
"#;
    let bundle = print(&system(src), PrintMode::Check, "M");
    assert_eq!(bundle.names.property(0), "Start");
    assert!(bundle.tla.contains("\nStart_2(p0) ==\n"), "{}", bundle.tla);
    assert!(bundle.cfg.contains("PROPERTY Start\n"), "{}", bundle.cfg);
}

/// Columns named like the run mark, existence, and owner vars (`run_Go`, `exists`,
/// `owner_Gate`) give two vars one spelling; the later var takes the next free name, and a
/// state TLC prints decodes to the state it is.
#[test]
fn a_column_named_like_a_generated_var_decodes_apart() {
    let src = r#"
entity Gate "門" { id: Int @pk }
entity Note "記録" { id: Int @pk }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting)  run_Go: Bool @default(false)  exists: Bool @default(false)  owner_Gate: Bool @default(false) }
relate(Job, Gate, N:1)
usecase Go "開始"
usecase BuildGate "設置"
usecase Submit "投入"
usecase Mark "印"
api JobApi "作業 API"
api NoteApi "記録 API"
creates(BuildGate, Gate)
creates(Submit, Job)
invokes(Go, JobApi)
updates(JobApi, Job)
transitions(Go, Job.status, waiting -> running)
invokes(Go, NoteApi)
creates(NoteApi, Note)
sets(Mark, Job.run_Go == true)
sets(Mark, Job.exists == true)
sets(Mark, Job.owner_Gate == true)
"#;
    let run = system(src).run(Caps::new(usize::MAX, 100_000));
    let ts = &run.system;
    let names = TlaNames::new(ts);
    let mut vars: Vec<&str> = names.vars().iter().map(String::as_str).collect();
    vars.sort_unstable();
    for pair in [
        ["Job_exists", "Job_exists_2"],
        ["Job_owner_Gate", "Job_owner_Gate_2"],
        ["Job_run_Go", "Job_run_Go_2"],
    ] {
        assert!(pair.iter().all(|v| vars.contains(v)), "{vars:?}");
    }
    let mut unique = vars.clone();
    unique.dedup();
    assert_eq!(unique, vars);
    for s in &run.states {
        assert_eq!(names.decode(ts, &names.canon(ts, s)).as_ref(), Some(s));
    }
}

/// A property and a rule keep the names `check` prints beside a var or an action of that
/// spelling, which the export does not promise to keep (docs/compatibility.md): the var or
/// action takes the next free name, and a state TLC prints still decodes to the state it is.
#[test]
fn a_property_or_rule_keeps_its_name_beside_a_var_or_action() {
    let property_like_a_var = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running, done) @default(waiting) }
usecase Submit "投入"
usecase Start "開始"
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
property Job_status "never running"
  always(Job.status != running)
"#;
    let rule_like_a_var_and_an_action = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
entity Forbidden "x" { id: Int @pk  Job_0: Enum(a, b) @default(a) }
usecase Submit "投入"
usecase Forbidden_Job_0 "開始"
usecase Flip "flip"
creates(Submit, Job)
creates(Submit, Forbidden)
transitions(Forbidden_Job_0, Job.status, waiting -> running)
transitions(Flip, Forbidden.Job_0, a -> b)
forbidden(Job, status == running)
"#;
    let bundle = print(&system(property_like_a_var), PrintMode::Check, "M");
    assert_eq!(bundle.names.property(0), "Job_status");
    assert!(bundle.names.vars().iter().any(|v| v == "Job_status_2"));
    assert!(
        bundle.cfg.contains("PROPERTY Job_status\n"),
        "{}",
        bundle.cfg
    );

    let ts = system(rule_like_a_var_and_an_action);
    let bundle = print(&ts, PrintMode::Safety, "M");
    assert_eq!(ts.invariants()[0].name, "Forbidden_Job_0");
    assert_eq!(bundle.names.invariant(0), "Forbidden_Job_0");
    assert!(
        bundle.cfg.contains("INVARIANT Forbidden_Job_0\n"),
        "{}",
        bundle.cfg
    );
    let names = &bundle.names;
    let mut taken: Vec<&str> = names
        .vars()
        .iter()
        .map(String::as_str)
        .chain(ts.action_ids().map(|a| names.action(a)))
        .collect();
    taken.sort_unstable();
    assert_eq!(
        taken
            .iter()
            .filter(|n| n.starts_with("Forbidden_Job_0"))
            .collect::<Vec<_>>(),
        [&"Forbidden_Job_0_2", &"Forbidden_Job_0_3"]
    );
    let run = ts.run(Caps::new(usize::MAX, 100_000));
    let names = TlaNames::new(&run.system);
    for s in &run.states {
        assert_eq!(
            names
                .decode(&run.system, &names.canon(&run.system, s))
                .as_ref(),
            Some(s)
        );
    }
}

/// A renamed item's suffix skips a name another item asks for: beside a property `Start`,
/// the use case `Start` becomes `Start_3`, and a use case `Start_2` keeps its name.
#[test]
fn a_renamed_action_skips_another_actions_name() {
    let src = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running, done) @default(waiting) }
usecase Submit "投入"
usecase Start "開始"
usecase Start_2 "完了"
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
transitions(Start_2, Job.status, running -> done)
property Start "作業は待つ"
  always(Job.status == waiting)
"#;
    let ts = system(src);
    let names = TlaNames::new(&ts);
    let actions: Vec<(&str, &str)> = ts
        .action_ids()
        .map(|a| (ts.actions()[a.index()].name.as_str(), names.action(a)))
        .collect();
    assert!(actions.contains(&("Start", "Start_3")), "{actions:?}");
    assert!(actions.contains(&("Start_2", "Start_2")), "{actions:?}");
    assert_eq!(names.property(0), "Start");
}
