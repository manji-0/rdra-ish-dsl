//! `rdra-ish export --kind sql` runs as printed: the SQLite script of every `samples.txt`
//! root loads into SQLite, and the `CHECK` constraints reject the rows the model's rules
//! forbid, null operands included. SQLite runs through Python's `sqlite3` module; without
//! `python3` on PATH each test is skipped with a printed reason.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn have_python() -> bool {
    let found = Command::new("python3")
        .args(["-c", "import sqlite3"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        println!("skipped: python3 with sqlite3 is not on PATH");
    }
    found
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// Runs `script` (Python) with the DDL file as `sys.argv[1]`; returns its stdout.
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
    String::from_utf8_lossy(&output.stdout).into_owned()
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
