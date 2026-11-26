//! `rdra-ish export --kind sql` runs as printed: the SQLite script of every `samples.txt`
//! root loads into SQLite, and the `CHECK` constraints reject the rows the model's rules
//! forbid, null operands included. SQLite runs through Python's `sqlite3` module; without
//! `python3` on PATH each test is skipped with a printed reason. `RDRA_REQUIRE_SQLITE=1`
//! turns the skip into a failure.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[path = "common/samples.rs"]
mod samples;
#[path = "common/scratch.rs"]
mod scratch;
use samples::{repo, samples};
use scratch::scratch;

#[path = "common/tools.rs"]
mod tools;
use tools::found_or_skip;

fn have_python() -> bool {
    // sqlite3 is a module python3 may lack, so the probe imports it.
    let found = Command::new("python3")
        .args(["-c", "import sqlite3"])
        .output()
        .is_ok_and(|o| o.status.success());
    found_or_skip(
        found,
        "python3 with sqlite3 is not on PATH",
        "RDRA_REQUIRE_SQLITE",
    )
}

fn export_sqlite(input: &Path, out: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["export", "--kind", "sql", "--dialect", "sqlite", "-o"])
        .arg(out)
        .arg(input)
        .output()
        .expect("rdra-ish runs");
    assert!(
        output.status.success(),
        "{}: {}",
        input.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `script` (Python) with the DDL file as `sys.argv[1]`; returns its stdout, with
/// Windows line ends read as `\n` so a test compares the same text on every OS.
fn python(script: &str, ddl: &Path) -> String {
    let mut child = Command::new("python3")
        .arg("-I")
        .arg("-")
        .arg(ddl)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        ddl.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn every_sample_loads_into_sqlite() {
    if !have_python() {
        return;
    }
    let dir = scratch("sql_every_sample");
    let load = r#"
import sqlite3, sys
con = sqlite3.connect(":memory:")
con.execute("PRAGMA foreign_keys = ON")
con.executescript(open(sys.argv[1], encoding="utf-8").read())
"#;
    for (i, sample) in samples().iter().enumerate() {
        let ddl = dir.join(format!("sample_{i}.sql"));
        export_sqlite(&repo().join(sample), &ddl);
        python(load, &ddl);
    }
}

const RULES: &str = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  total: Int @check(total >= 0)
  discount: Int @default(0)
  paid_at: DateTime @null
  qty: Int @null @check(qty >= 1)
}
forbidden(Order, status == shipped, paid_at == null)
invariant(Order).when(status == paid).then(paid_at == present)
required(Order, total >= discount)
"#;

/// Each row the rules forbid is rejected by the constraint `check` names, and a row they
/// allow is stored.
#[test]
fn checks_reject_what_the_rules_forbid() {
    if !have_python() {
        return;
    }
    let dir = scratch("sql_checks");
    let model = dir.join("rules.rdra");
    std::fs::write(&model, RULES).unwrap();
    let ddl = dir.join("rules.sql");
    export_sqlite(&model, &ddl);
    let script = r#"
import sqlite3, sys
con = sqlite3.connect(":memory:")
con.executescript(open(sys.argv[1], encoding="utf-8").read())
rows = {
    "ok": "INSERT INTO \"Order\" (id, status, total, discount, paid_at, qty) VALUES (1, 'paid', 5, 1, '2026-01-01', 2)",
    "Check_Order_total_0": "INSERT INTO \"Order\" (id, total, qty) VALUES (2, -1, 1)",
    "Check_Order_qty_0": "INSERT INTO \"Order\" (id, total, qty) VALUES (3, 1, NULL)",
    "Forbidden_Order_0": "INSERT INTO \"Order\" (id, status, total, qty) VALUES (4, 'shipped', 1, 1)",
    "Inv_Order_0": "INSERT INTO \"Order\" (id, status, total, qty) VALUES (5, 'paid', 1, 1)",
    "Required_Order_0": "INSERT INTO \"Order\" (id, total, discount, qty) VALUES (6, 1, 2, 1)",
    "Order_status_values": "INSERT INTO \"Order\" (id, status, total, qty) VALUES (7, 'lost', 1, 1)",
}
for name, sql in rows.items():
    try:
        con.execute(sql)
        print(name, "stored")
    except sqlite3.IntegrityError as e:
        print(name, "rejected", e)
"#;
    let out = python(script, &ddl);
    assert!(out.contains("ok stored"), "{out}");
    for name in [
        "Check_Order_total_0",
        "Check_Order_qty_0",
        "Forbidden_Order_0",
        "Inv_Order_0",
        "Required_Order_0",
        "Order_status_values",
    ] {
        assert!(
            out.lines()
                .any(|l| l.starts_with(&format!("{name} rejected")) && l.contains(name)),
            "{name} not rejected by its own constraint:\n{out}"
        );
    }
}

/// A unique set on a soft-deleted entity holds among live rows: a deleted row frees its
/// value, and two live rows still conflict.
#[test]
fn a_live_unique_index_frees_the_value_of_a_deleted_row() {
    if !have_python() {
        return;
    }
    let dir = scratch("sql_live_unique");
    let model = dir.join("live.rdra");
    std::fs::write(
        &model,
        "entity Account \"Account\" {\n  id: Int @pk\n  email: String @unique\n  deleted_at: DateTime @null @soft_delete\n}\n",
    )
    .unwrap();
    let ddl = dir.join("live.sql");
    export_sqlite(&model, &ddl);
    let script = r#"
import sqlite3, sys
con = sqlite3.connect(":memory:")
con.executescript(open(sys.argv[1], encoding="utf-8").read())
con.execute("INSERT INTO Account (id, email, deleted_at) VALUES (1, 'a@x', '2026-01-01')")
con.execute("INSERT INTO Account (id, email) VALUES (2, 'a@x')")
print("reused")
try:
    con.execute("INSERT INTO Account (id, email) VALUES (3, 'a@x')")
    print("duplicate stored")
except sqlite3.IntegrityError:
    print("duplicate rejected")
"#;
    let out = python(script, &ddl);
    assert_eq!(out, "reused\nduplicate rejected\n");
}

/// Each suggested index and each `SELECT` the reads become runs in SQLite as written, and a
/// keyset page over a sort that mixes directions continues where the previous page ended.
#[test]
fn suggested_indexes_and_reads_run_in_sqlite() {
    if !have_python() {
        return;
    }
    let dir = scratch("sql_reads");
    let model = dir.join("reads.rdra");
    std::fs::write(
        &model,
        "entity Order \"Order\" {\n  id: Int @pk\n  status: Enum(open, paid)\n  amount: Int\n}\nusecase U \"u\"\nreads(U, Order).by(status).order(amount, id.desc).page(cursor)\nreads(U, Order).by(status).count()\n",
    )
    .unwrap();
    let ddl = dir.join("reads.sql");
    export_sqlite(&model, &ddl);
    let script = r#"
import re, sqlite3, sys
text = open(sys.argv[1], encoding="utf-8").read()
con = sqlite3.connect(":memory:")
con.executescript(text)
for line in text.splitlines():
    if line.startswith("-- CREATE INDEX"):
        con.execute(line[3:].split(" -- ")[0])
selects = [l[5:] for l in text.splitlines() if l.startswith("--   SELECT")]
rows = [(1, "open", 5), (2, "open", 5), (3, "open", 7), (4, "paid", 1), (5, "open", 9)]
con.executemany('INSERT INTO "Order" (id, status, amount) VALUES (?, ?, ?)', rows)
page = selects[0]
first = con.execute(page, {"1": "open", "2": -1, "3": 0, "4": 2}).fetchall()
last = first[-1]
second = con.execute(page, {"1": "open", "2": last[2], "3": last[0], "4": 2}).fetchall()
print([r[0] for r in first], [r[0] for r in second])
print(con.execute(selects[1], {"1": "open"}).fetchone()[0])
"#;
    let out = python(script, &ddl);
    assert_eq!(out, "[2, 1] [3, 5]\n4\n");
}
