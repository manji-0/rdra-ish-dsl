use super::*;

/// `src` formats to `expected`, and formatting `expected` again changes nothing.
fn assert_formats_to(src: &str, expected: &str) {
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source(&formatted).unwrap(),
        formatted,
        "not idempotent"
    );
}

/// A lemma's marks format as written, and a column named like a mark method stays a column.
#[test]
fn formats_the_marks_of_a_lemma() {
    assert_formats_to(
        "forbidden(Light,Door,Light.state==off,Door.in_run( Open ,1)).global().lemma()\n",
        "forbidden(Light, Door, Light.state == off, Door.in_run(Open, 1)).global().lemma()\n",
    );
    assert_formats_to(
        "invariant(Sign).when(Sign.raised(EvShut)).then(Sign.state==dark).lemma()\n",
        "invariant(Sign).when(Sign.raised(EvShut)).then(Sign.state == dark).lemma()\n",
    );
    assert_formats_to(
        "forbidden(Door, Door.in_run == true)\n",
        "forbidden(Door, Door.in_run == true)\n",
    );
}

#[test]
fn formats_the_order_and_stability_forms() {
    assert_formats_to(
        "property S stable( Order.status==cancelled )\n",
        "property S\n  stable(Order.status == cancelled)\n",
    );
    assert_formats_to(
        "property U unchanged(Order.amount,total ).when(Order.status==approved).along(Order)\n",
        "property U\n  unchanged(Order.amount, total).when(Order.status == approved).along(Order)\n",
    );
    assert_formats_to(
        "property P \"paid first\" precedes(Order.paid==true,Order.status==shipped)\n",
        "property P \"paid first\"\n  precedes(Order.paid == true, Order.status == shipped)\n",
    );
}

#[test]
fn keeps_the_use_case_first_and_entity_column_forms() {
    assert_formats_to(
        "transitions( Pay,Order.status, pending->paid )\n",
        "transitions(Pay, Order.status, pending -> paid)\n",
    );
    assert_formats_to(
        "transitions(Cancel, Order.status, ( pending,held )->cancelled)\n",
        "transitions(Cancel, Order.status, (pending, held) -> cancelled)\n",
    );
    assert_formats_to(
        "maps_field( Total ,Order.total )\n",
        "maps_field(Total, Order.total)\n",
    );
    assert_formats_to(
        "requirement R \"R\"\n  acceptance_criteria \"Paid orders ship.\"\n  priority must\n",
        "requirement R \"R\"\n  priority must\n  acceptance_criteria \"Paid orders ship.\"\n",
    );
}

#[test]
fn a_removed_form_does_not_format() {
    for src in [
        "transitions(Order.status, Pay, pending -> paid)\n",
        "maps_field(Total, Order, \"total\")\n",
        "requirement R \"R\"\n  acceptance criteria \"x\"\n",
        "writes(Pay, Order)\n",
    ] {
        assert!(format_source(src).is_err(), "{src}");
    }
}

#[test]
fn formats_a_goal_and_keeps_its_measure_in_order() {
    assert_formats_to(
        "goal   ShortWait \"待ちを短く\"\n  description \"患者が帰る。\"\n  window   monthly\n  target \"30分\"\n  baseline \"45分\"\n  metric \"中央値\"\n",
        "goal ShortWait \"待ちを短く\"\n  description \"患者が帰る。\"\n  metric \"中央値\"\n  baseline \"45分\"\n  target \"30分\"\n  window monthly\n",
    );
}

#[test]
fn prints_consecutive_imports_as_one_block() {
    assert_formats_to(
        "module m\nimport a\n\nimport b.{X}\n// why c\nimport c as C\nactor A \"a\"\n",
        "module m\n\nimport a\nimport b.{X}\n// why c\nimport c as C\n\nactor A \"a\"\n",
    );
}

#[test]
fn keeps_a_description_on_the_line_of_its_declaration_while_it_fits() {
    let fits = format!("usecase U \"u\" description \"{}\"\n", "x".repeat(92));
    assert_eq!(fits.trim_end().len(), MAX_WIDTH);
    assert_formats_to(&fits.replace(" description", "\n  description"), &fits);

    let over = format!("usecase U \"u\" description \"{}\"\n", "x".repeat(93));
    assert_formats_to(&over, &over.replace(" description", "\n  description"));
}

#[test]
fn counts_a_wide_character_as_two_columns() {
    // 46 wide characters are 92 columns: the line is exactly MAX_WIDTH wide.
    let fits = format!("usecase U \"u\" description \"{}\"\n", "会".repeat(46));
    assert_formats_to(&fits, &fits);
    let over = format!("usecase U \"u\" description \"{}\"\n", "会".repeat(47));
    assert_formats_to(&over, &over.replace(" description", "\n  description"));
}

#[test]
fn keeps_a_clause_beside_another_on_its_own_line_and_opens_a_body_after_it() {
    assert_formats_to(
        "requirement R \"r\" description \"d\" priority must\n",
        "requirement R \"r\"\n  description \"d\"\n  priority must\n",
    );
    assert_formats_to(
        "entity E \"e\" description \"d\" {\n  id: Int @pk\n}\n",
        "entity E \"e\" description \"d\" {\n  id: Int @pk\n}\n",
    );
    let long = "x".repeat(110);
    assert_formats_to(
        &format!("entity E \"e\" description \"{long}\" {{\n  id: Int @pk\n}}\n"),
        &format!("entity E \"e\"\n  description \"{long}\" {{\n  id: Int @pk\n}}\n"),
    );
}

#[test]
fn keeps_runs_of_one_line_declarations_and_their_citations_together() {
    assert_formats_to(
        "// a.txt:1\nusecase A \"a\" description \"x\"\n// b.txt:2\nusecase B  \"b\"\n\n\nscreen S \"s\"\nusecase C \"c\" // c.txt:3\n  description \"y\"\nrequirement R \"r\"\n  priority must\n  status draft\nperforms(P, A)\n",
        "// a.txt:1\nusecase A \"a\" description \"x\"\n// b.txt:2\nusecase B \"b\"\n\nscreen S \"s\"\nusecase C \"c\" description \"y\" // c.txt:3\n\nrequirement R \"r\"\n  priority must\n  status draft\n\nperforms(P, A)\n",
    );
}

#[test]
fn keeps_a_trailing_comment_on_the_header_of_a_wrapped_description() {
    let long = "x".repeat(110);
    assert_formats_to(
        &format!("actor A \"a\" description \"{long}\" // note\nactor B \"b\"\n"),
        &format!("actor A \"a\" // note\n  description \"{long}\"\n\nactor B \"b\"\n"),
    );
}

#[test]
fn keeps_an_own_line_comment_above_the_item_it_precedes() {
    assert_formats_to(
        "module a\n// heading comment\nactor A \"a\"\n",
        "module a\n\n// heading comment\nactor A \"a\"\n",
    );
}

#[test]
fn keeps_a_trailing_comment_on_the_line_it_ends() {
    assert_formats_to(
        "module a // the module\nactor A \"a\"   // trailing  \n",
        "module a // the module\n\nactor A \"a\" // trailing\n",
    );
}

#[test]
fn keeps_block_comments_inline_and_on_their_own_lines() {
    assert_formats_to(
        "/* file\n   header */\nactor A \"a\" /* inline */\nactor B \"b\" /* both */ // kinds\n",
        "/* file\n   header */\nactor A \"a\" /* inline */\nactor B \"b\" /* both */ // kinds\n",
    );
}

#[test]
fn keeps_comments_between_predicates() {
    assert_formats_to(
        "performs(A, U1)\n// sources/a.txt:12\nperforms(A, U2) // sources/b.txt:3\n// sources/c.txt:4\nperforms(A, U3)\n",
        "performs(A, U1)\n// sources/a.txt:12\nperforms(A, U2) // sources/b.txt:3\n// sources/c.txt:4\nperforms(A, U3)\n",
    );
}

#[test]
fn keeps_comments_at_the_end_of_the_file() {
    assert_formats_to(
        "actor A \"a\"\n// footer one\n\n// footer two",
        "actor A \"a\"\n\n// footer one\n\n// footer two\n",
    );
}

#[test]
fn keeps_a_file_of_only_comments() {
    assert_formats_to("// nothing yet\n", "// nothing yet\n");
}

#[test]
fn keeps_comments_inside_an_entity_body() {
    assert_formats_to(
        "entity E \"e\" { // columns\n  id: Int @pk // key\n// a status\n  status: Enum(a, b)\n  // closing note\n} // end\n",
        "entity E \"e\" { // columns\n  id: Int @pk // key\n  // a status\n  status: Enum(a, b)\n  // closing note\n} // end\n",
    );
}

#[test]
fn keeps_comments_between_variation_values() {
    assert_formats_to(
        "variation V \"v\" {\n  a \"A\"\n  // why b\n  b \"B\" // b note\n}\n",
        "variation V \"v\" {\n  a \"A\"\n  // why b\n  b \"B\" // b note\n}\n",
    );
}

#[test]
fn keeps_the_blank_line_between_a_detached_comment_and_the_next_item() {
    assert_formats_to(
        "// section\n\nactor A \"a\"\n\n// group\n// of two\n\n\nactor B \"b\"\n",
        "// section\n\nactor A \"a\"\n\n// group\n// of two\n\nactor B \"b\"\n",
    );
}

#[test]
fn moves_a_comment_from_a_line_without_its_own_anchor_above_its_item() {
    assert_formats_to(
        "requirement R \"r\"\n  priority \"must\"\n  // why the source\n  source \"Interview\" // who said it\n\ninvariant(Order)\n  // only when paid\n  .when(status == paid).then(total > 0) // wrapped call\n",
        "// why the source\n// who said it\nrequirement R \"r\"\n  priority \"must\"\n  source \"Interview\"\n\n// only when paid\n// wrapped call\ninvariant(Order).when(status == paid).then(total > 0)\n",
    );
}

#[test]
fn does_not_read_a_slash_pair_inside_a_string_as_a_comment() {
    assert_formats_to(
        "actor A \"see http://x /* y */\"\n",
        "actor A \"see http://x /* y */\"\n",
    );
}

#[test]
fn formats_set_membership() {
    assert_formats_to(
        "forbidden(Order,status  in(a,b))\n",
        "forbidden(Order, status in (a, b))\n",
    );
    assert_formats_to(
        "holds(C, Order.status not   in (a), Order.qty in (1,2))\n",
        "holds(C, Order.status not in (a), Order.qty in (1, 2))\n",
    );
}

#[test]
fn formats_check_and_derived_unquoted() {
    let src = "entity E \"E\" {\n  qty: Int @check(qty >= 1 and not qty > 4)\n  net: Int @derived(qty * 2)\n}\n";
    let formatted = format_source(src).unwrap();
    assert!(
        formatted.contains("@check((qty >= 1) and (not (qty > 4)))"),
        "{formatted}"
    );
    assert!(formatted.contains("@derived(qty * 2)"), "{formatted}");
    assert_eq!(format_source(&formatted).unwrap(), formatted);
}

#[test]
fn formats_a_property_fair_chain() {
    let src = "property GrantEnds\n  leads_to(A.status == granted, A.status == registered).fair(Pay, Register)\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, src);
}

#[test]
fn formats_a_can_reach_property() {
    let src = "property OrderCanFinish \"every order can finish\"\n  can_reach(Order.status == delivered)\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, src);
}

#[test]
fn formats_nm_via_chain() {
    let src = "relate(Order, Product, N:M).via(OrderProduct)\n";
    let formatted = format_source(src).unwrap();
    assert!(
        formatted.contains("relate(Order, Product, N:M).via(OrderProduct)"),
        "{formatted}"
    );
    let (_ast, errors) = parse(&formatted);
    assert!(
        errors.is_empty(),
        "formatted output should parse: {errors:?}"
    );
}

#[test]
fn formats_a_holds_quantifier_chain() {
    let src = "holds(C, A.status == active).none(B.status == active).has(B.status == ended)\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, src);
}

#[test]
fn formats_a_rule_label_as_written() {
    let src = "invariant(Appointment).when(status == checkedin).then(checked_in_at == present).label(\"受付済の予約には受付時刻がある\")\n\nforbidden(A, B, A.status == x).along(A, B).label(\"x\")\n\nafter(Pay).assert(Order.status == paid).label(\"支払後は入金済\")\n\ninvariant(Cert).when(status == revoked).none(Assign, status == active).label(\"失効後は割当なし\")\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, src);
    let (_ast, errors) = parse(&formatted);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn formats_a_screen_transition_as_written() {
    let src =
        "navigates(CheckInScreen, EligibilityScreen).via(CheckInPatient)\n\nnavigates(A, B)\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(formatted, src);
}

#[test]
fn formats_a_cardinality_range_as_written() {
    let formatted =
        format_source("relate(Line, Order, 1 .. *:1)\nrelate(Note, Order, 0..1:1)\n").unwrap();
    assert_eq!(
        formatted,
        "relate(Line, Order, 1..*:1)\nrelate(Note, Order, 0..1:1)\n"
    );
}

#[test]
fn formats_variation_values_and_named_column_types() {
    let src = "variation PaymentMethod \"Payment Method\" {cash \"Cash\" card\n adjustment \"Adj\"}\nentity E \"E\" {method:PaymentMethod @default(card)}\n";
    let formatted = format_source(src).unwrap();
    assert_eq!(
        formatted,
        "variation PaymentMethod \"Payment Method\" {\n  cash \"Cash\"\n  card\n  adjustment \"Adj\"\n}\n\nentity E \"E\" {\n  method: PaymentMethod @default(card)\n}\n"
    );
    let (_ast, errors) = parse(&formatted);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn keeps_the_labels_of_enum_values() {
    let formatted =
        format_source("entity E \"E\" {status:Enum(open \"受付中\",closed)}\n").unwrap();
    assert_eq!(
        formatted,
        "entity E \"E\" {\n  status: Enum(open \"受付中\", closed)\n}\n"
    );
}

#[test]
fn canonicalizes_source_and_preserves_parseability() {
    let src = r#"module shop.checkout
import shared.actors.{Customer as Buyer, Staff}
requirement ReqCheckout "Checkout reliable" priority "must" source "Interview"
adr AdrOutbox "Use outbox" adr_status accepted decision "Use transactional outbox." reason "Avoid synchronous callbacks."
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {customer_id:Int note:String @null}
invariant(Order).when(status == paid).then(total > 0)
"#;

    let formatted = format_source(src).unwrap();

    assert_eq!(
        formatted,
        r#"module shop.checkout

import shared.actors.{Customer as Buyer, Staff}

requirement ReqCheckout "Checkout reliable"
  priority "must"
  source "Interview"

adr AdrOutbox "Use outbox"
  adr_status accepted
  decision "Use transactional outbox."
  reason "Avoid synchronous callbacks."

api CreateOrder "Create order"
  method POST
  path "/orders"
  auth bearer

dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}

invariant(Order).when(status == paid).then(total > 0)
"#
    );

    let (_ast, errors) = parse(&formatted);
    assert!(
        errors.is_empty(),
        "formatted output should parse: {errors:?}"
    );
}

#[test]
fn keeps_an_end_annotation() {
    assert_formats_to(
        "entity E \"E\" {\n  status: Enum(free, used) @end( free,used )\n}\n",
        "entity E \"E\" {\n  status: Enum(free, used) @end(free, used)\n}\n",
    );
}

#[test]
fn keeps_type_arguments_and_computed_defaults() {
    assert_formats_to(
        "entity E \"E\" {\n  id: Int @pk @generated( identity )\n  name: String( 255 )\n  at: DateTime(tz) @default(now( )) @on_update( now() )\n}\n",
        "entity E \"E\" {\n  id: Int @pk @generated(identity)\n  name: String(255)\n  at: DateTime(tz) @default(now()) @on_update(now())\n}\n",
    );
}

#[test]
fn keeps_a_table_name_and_a_version_column() {
    assert_formats_to(
        "entity E \"E\" table   \"es\" {\n  v: Int  @version\n}\n",
        "entity E \"E\"\n  table \"es\" {\n  v: Int @version\n}\n",
    );
}

#[test]
fn keeps_a_row_estimate_bare_or_quoted() {
    assert_formats_to(
        "entity E \"E\" table \"es\"   rows   5000 {\n  id: Int @pk\n}\nentity F \"F\" rows \"1e6\" {\n  id: Int @pk\n}\n",
        "entity E \"E\"\n  table \"es\"\n  rows 5000 {\n  id: Int @pk\n}\n\nentity F \"F\"\n  rows \"1e6\" {\n  id: Int @pk\n}\n",
    );
}

#[test]
fn keeps_a_default_bare_or_quoted_as_written() {
    assert_formats_to(
        "entity E \"E\" {\n  n: Int @default(0)\n  m: Money @default(\"3\")\n  s: Enum(a, b) @default(a)\n}\n",
        "entity E \"E\" {\n  n: Int @default(0)\n  m: Money @default(\"3\")\n  s: Enum(a, b) @default(a)\n}\n",
    );
}

#[test]
fn keeps_a_counted_trigger() {
    assert_formats_to(
        "triggers( EvPaid,Post ).counted( 2 )\n",
        "triggers(EvPaid, Post).counted(2)\n",
    );
}
