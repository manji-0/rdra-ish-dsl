use super::*;
use rdra_ish_core::build_model;
use rdra_ish_syntax::parse;

fn model_from(src: &str) -> SemanticModel {
    let (ast, errors) = parse(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "diagnostics: {errors:?}");
    model
}

fn sql(src: &str, dialect: Dialect) -> String {
    SqlEmitter { dialect }
        .emit(&model_from(src), &View::whole())
        .unwrap()
}

const SHOP: &str = r#"
entity Customer "Customer" {
  id: Int @pk
  email: String @unique
}
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  total: Money @check(total >= 0)
  discount: Money @default(0)
  paid_at: DateTime @null
  qty: Int @null @check(qty == null or qty >= 1)
  net_total: Money @derived(total - discount)
  method: PaymentMethod
}
variation PaymentMethod "Payment method" {
  card
  cash
}
relate(Order, Customer, N:1).on_delete(cascade)
forbidden(Order, status == shipped, paid_at == null).label("a shipped order is paid")
invariant(Order).when(status == paid).then(paid_at == present)
required(Order, total >= discount)
exclusive(Order, status == pending, paid_at == present)
"#;

#[test]
fn postgres_declares_enum_types_and_adds_foreign_keys_after_the_tables() {
    let out = sql(SHOP, Dialect::Postgres);
    assert!(out.contains("CREATE TYPE \"Order_status\" AS ENUM ('pending', 'paid', 'shipped');"));
    assert!(out.contains("CREATE TYPE \"PaymentMethod\" AS ENUM ('card', 'cash');"));
    assert!(out.contains("  \"status\" \"Order_status\" NOT NULL DEFAULT 'pending',"));
    assert!(out.contains("  \"method\" \"PaymentMethod\" NOT NULL,"));
    assert!(out.contains("  \"paid_at\" TIMESTAMPTZ,"));
    assert!(out.contains("  \"discount\" NUMERIC NOT NULL DEFAULT 0,"));
    assert!(out.contains("  UNIQUE (\"email\")"));
    let alter = "ALTER TABLE \"Order\" ADD CONSTRAINT \"Order_customer_id_fkey\" FOREIGN KEY (\"customer_id\") REFERENCES \"Customer\" (\"id\") ON DELETE CASCADE;";
    assert!(out.contains(alter), "{out}");
    let tables_end = out.rfind("CREATE TABLE").unwrap();
    assert!(out.find(alter).unwrap() > tables_end, "{out}");
}

#[test]
fn sqlite_checks_enum_values_and_declares_foreign_keys_inline() {
    let out = sql(SHOP, Dialect::Sqlite);
    assert!(!out.contains("CREATE TYPE"), "{out}");
    assert!(out.contains("  \"status\" TEXT NOT NULL DEFAULT 'pending',"));
    assert!(out.contains(
        "  CONSTRAINT \"Order_status_values\" CHECK (\"status\" IN ('pending', 'paid', 'shipped'))"
    ));
    assert!(out.contains("  CONSTRAINT \"Order_customer_id_fkey\" FOREIGN KEY (\"customer_id\") REFERENCES \"Customer\" (\"id\") ON DELETE CASCADE"));
    assert!(!out.contains("ALTER TABLE"), "{out}");
}

/// A rule reads a null operand as false except `!=`, so each comparison on a nullable
/// column says what a null means; a `NOT NULL` column needs no guard.
#[test]
fn entity_rules_become_two_valued_checks_with_the_names_check_reports() {
    let out = sql(SHOP, Dialect::Postgres);
    for line in [
        "CONSTRAINT \"Check_Order_total_0\" CHECK (\"total\" >= 0)",
        "CONSTRAINT \"Check_Order_qty_0\" CHECK ((\"qty\" IS NULL) OR ((\"qty\" IS NOT NULL AND \"qty\" >= 1)))",
        "CONSTRAINT \"Forbidden_Order_0\" CHECK (NOT (\"status\" = 'shipped' AND \"paid_at\" IS NULL))",
        "CONSTRAINT \"Inv_Order_0\" CHECK (NOT (\"status\" = 'paid') OR (\"paid_at\" IS NOT NULL))",
        "CONSTRAINT \"Required_Order_0\" CHECK (\"total\" >= \"discount\")",
        "CONSTRAINT \"Exclusive_Order_0\" CHECK ((CASE WHEN \"status\" = 'pending' THEN 1 ELSE 0 END) + (CASE WHEN \"paid_at\" IS NOT NULL THEN 1 ELSE 0 END) <= 1)",
    ] {
        assert!(out.contains(line), "missing {line}\n{out}");
    }
}

#[test]
fn a_derived_column_is_generated_and_the_foreign_key_is_indexed() {
    let out = sql(SHOP, Dialect::Postgres);
    assert!(
        out.contains("  \"net_total\" NUMERIC GENERATED ALWAYS AS (\"total\" - \"discount\") STORED /* derived: total - discount */,"),
        "{out}"
    );
    assert!(out.contains("CREATE INDEX \"Order_customer_id_idx\" ON \"Order\" (\"customer_id\");"));
    let sqlite = sql(SHOP, Dialect::Sqlite);
    assert!(sqlite.contains("GENERATED ALWAYS AS (\"total\" - \"discount\") VIRTUAL"));
}

/// A key that the primary key, a unique set, or an `@index` already leads needs no index of
/// its own.
#[test]
fn a_foreign_key_an_index_already_leads_gets_no_second_index() {
    let out = sql(
        r#"
entity Order "Order" { id: Int @pk }
entity Line "Line" {
  id: Int @pk
  no: Int @index(order_id, no)
}
entity Note "Note" { id: Int @pk }
relate(Line, Order, N:1)
relate(Note, Order, 1:1)
"#,
        Dialect::Postgres,
    );
    assert!(
        out.contains("CREATE INDEX \"Line_order_id_no_idx\" ON \"Line\" (\"order_id\", \"no\");")
    );
    assert!(!out.contains("\"Line_order_id_idx\""), "{out}");
    assert!(out.contains("UNIQUE (\"order_id\")"), "{out}");
    assert!(!out.contains("\"Note_order_id_idx\""), "{out}");
}

/// What a `CHECK` cannot say closes the script, by the name `check` reports.
#[test]
fn rules_the_schema_cannot_enforce_are_listed_as_obligations() {
    let out = sql(
        r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
  due_at: DateTime @null
}
entity Payment "Payment" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
relate(Payment, Order, N:1)
relate(Line, Order, 1..*:1)
entity Line "Line" { id: Int @pk }
forbidden(Order, status == open, due_at < now)
forbidden(Order, Payment, Order.status == open, Payment.status == captured).along(Order, Payment)
condition Late "late"
holds(Late, Order.status == open)
forbidden(Order, status == paid).when(Late)
"#,
        Dialect::Postgres,
    );
    let tail = &out[out
        .find("-- Not enforced by this schema")
        .expect("an obligations block")..];
    for line in [
        "-- Forbidden_Order_0 (reads now): status == open, due_at < now",
        "-- Forbidden_Order_1 (guarded by a condition): status == paid",
        "-- MultiForbidden_Order_0 (several entities):",
        "-- MinCard_Line_Order (minimum cardinality):",
    ] {
        assert!(tail.contains(line), "missing {line}\n{tail}");
    }
    assert!(!out.contains("CONSTRAINT \"Forbidden_Order_0\""), "{out}");
}
