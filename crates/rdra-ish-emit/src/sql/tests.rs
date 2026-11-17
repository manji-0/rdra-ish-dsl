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

const DETAILED: &str = r#"
entity Customer "Customer" {
  id: String @pk @generated(uuid)
  email: String(255) @unique
  created_at: DateTime(tz) @default(now())
  updated_at: DateTime(local) @default(now()) @on_update(now())
}
entity Order "Order" {
  id: Int @pk @generated(identity)
  total: Decimal(10, 2)
  ordered_on: Date @default(now())
}
relate(Order, Customer, N:1)
"#;

#[test]
fn type_detail_and_computed_defaults_reach_postgres() {
    let out = sql(DETAILED, Dialect::Postgres);
    for line in [
        "  \"id\" TEXT NOT NULL DEFAULT gen_random_uuid()::text,",
        "  \"email\" VARCHAR(255) NOT NULL,",
        "  \"created_at\" TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,",
        "  \"updated_at\" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP /* on update: now() (trigger or application) */,",
        "  \"id\" INTEGER NOT NULL GENERATED BY DEFAULT AS IDENTITY,",
        "  \"total\" NUMERIC(10, 2) NOT NULL,",
        "  \"ordered_on\" DATE NOT NULL DEFAULT CURRENT_DATE,",
        // A key column stores what the parent's key stores.
        "  \"customer_id\" TEXT NOT NULL",
    ] {
        assert!(out.contains(line), "missing {line}\n{out}");
    }
}

/// SQLite numbers a single INTEGER primary key itself and has no UUID function.
#[test]
fn sqlite_leaves_identity_to_the_rowid_and_uuid_to_the_application() {
    let out = sql(DETAILED, Dialect::Sqlite);
    assert!(out.contains("  \"id\" INTEGER NOT NULL,"), "{out}");
    assert!(
        out.contains("  \"id\" TEXT NOT NULL /* uuid generated by the application */,"),
        "{out}"
    );
    assert!(out.contains("  \"email\" VARCHAR(255) NOT NULL,"), "{out}");
}

const SCHEMA_NAMES: &str = r#"
entity Product "Product" table "products" {
  id: Int @pk
  sku: String(32) @unique
  code: String @unique
  tenant_id: Int @tenant
  name: String @unique(tenant_id, name)
  version: Int @version
  deleted_at: DateTime @null @soft_delete
}
entity Line "Line" table "order_lines" { id: Int @pk }
entity Note "Note" { id: Int @pk }
relate(Line, Product, N:1).references(sku)
relate(Note, Product, 1:1)
"#;

/// On an entity with a soft-delete marker a declared unique set holds among live rows; a
/// set a foreign key references stays a full constraint.
#[test]
fn soft_deleted_entities_keep_declared_uniques_among_live_rows() {
    let out = sql(SCHEMA_NAMES, Dialect::Postgres);
    for line in [
        "CREATE UNIQUE INDEX \"products_code_live_key\" ON \"products\" (\"code\") WHERE \"deleted_at\" IS NULL;",
        "CREATE UNIQUE INDEX \"products_tenant_id_name_live_key\" ON \"products\" (\"tenant_id\", \"name\") WHERE \"deleted_at\" IS NULL;",
        "  UNIQUE (\"sku\")",
    ] {
        assert!(out.contains(line), "missing {line}\n{out}");
    }
    assert!(!out.contains("\"products_sku_live_key\""), "{out}");
    assert!(!out.contains("  UNIQUE (\"code\")"), "{out}");
}

#[test]
fn physical_names_references_and_version_reach_the_schema() {
    let out = sql(SCHEMA_NAMES, Dialect::Postgres);
    for line in [
        "CREATE TABLE \"products\" (",
        "CREATE TABLE \"order_lines\" (",
        "  \"product_sku\" VARCHAR(32) NOT NULL,",
        "ALTER TABLE \"order_lines\" ADD CONSTRAINT \"order_lines_product_sku_fkey\" FOREIGN KEY (\"product_sku\") REFERENCES \"products\" (\"sku\");",
        "  \"version\" INTEGER NOT NULL DEFAULT 1 /* version: each update checks it and adds 1 */,",
    ] {
        assert!(out.contains(line), "missing {line}\n{out}");
    }
}

const READS: &str = r#"
entity Order "Order" table "orders" {
  id: Int @pk
  status: Enum(open, paid)
  amount: Money
  created_at: DateTime
  deleted_at: DateTime @null @soft_delete
}
usecase ListOrders "List orders"
usecase CountOrders "Count orders"
reads(ListOrders, Order).by(status).order(created_at.desc).page(cursor)
reads(ListOrders, Order).order(amount, created_at.desc).page(offset)
reads(CountOrders, Order).by(status).count()
"#;

/// The indexes the reads want close the script as comments, then each read as a `SELECT`
/// over live rows, its keyset continuing after the last row of the previous page.
#[test]
fn declared_reads_become_suggested_indexes_and_selects() {
    let out = sql(READS, Dialect::Postgres);
    for line in [
        "-- CREATE INDEX \"orders_status_created_at_id_idx\" ON \"orders\" (\"status\", \"created_at\" DESC, \"id\" DESC); -- ListOrders, CountOrders",
        "-- CREATE INDEX \"orders_amount_created_at_idx\" ON \"orders\" (\"amount\", \"created_at\" DESC); -- ListOrders",
        "-- ListOrders reads Order:",
        "--   SELECT * FROM \"orders\" WHERE \"status\" = $1 AND \"deleted_at\" IS NULL AND (\"created_at\", \"id\") < ($2, $3) ORDER BY \"created_at\" DESC, \"id\" DESC LIMIT $4;",
        "--   SELECT * FROM \"orders\" WHERE \"deleted_at\" IS NULL ORDER BY \"amount\", \"created_at\" DESC LIMIT $1 OFFSET $2;",
        "--   SELECT count(*) FROM \"orders\" WHERE \"status\" = $1 AND \"deleted_at\" IS NULL;",
    ] {
        assert!(out.contains(line), "missing {line}\n{out}");
    }
    assert!(!sql(SHOP, Dialect::Postgres).contains("Reads the model declares"));
}

/// A read by an API selects the columns its response fields map, under the field names.
#[test]
fn a_read_by_an_api_selects_its_mapped_response_fields() {
    let out = sql(
        r#"
entity Order "Order" {
  id: Int @pk
  total: Money
}
dto OrderView "Order view" {
  order_id: Int
  total: Money
}
api GetOrder "Get order"
response(GetOrder, OrderView)
maps_field(OrderView.order_id, Order.id)
maps_field(OrderView.total, Order.total)
reads(GetOrder, Order).by(id)
"#,
        Dialect::Postgres,
    );
    assert!(
        out.contains(
            "--   SELECT \"id\" AS \"order_id\", \"total\" FROM \"Order\" WHERE \"id\" = $1;"
        ),
        "{out}"
    );
}
