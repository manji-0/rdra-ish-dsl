//! `codegen --lang rust --verus` (experimental, docs/design/verus.md): the Verus crate beside
//! the Rust code.
//!
//! Without Verus these tests check what the generator writes and refuses. With `verus` on
//! PATH they also check the crates verify, that Verus rejects a model whose rule fails and a
//! core that drops a check, that an `assert(false)` fails in every proof (the proofs are not
//! vacuous), and that the verified port moves as BFS does (the differential harness).
//! Without `verus` on PATH those are skipped with a printed reason; `RDRA_REQUIRE_VERUS=1`
//! turns the skip into a failure.

use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rdra_ish_core::lifecycle::Caps;
use rdra_ish_core::load_project;
use rdra_ish_emit::codegen::{build, verus::bfs_dump};

#[path = "common/root.rs"]
mod root;
use root::repo;

#[path = "common/tools.rs"]
mod tools;
use tools::tool;

const REQUIRE: &str = "RDRA_REQUIRE_VERUS";

/// The models the crate verifies, and the BFS check of the port runs on.
const COVERED: &[&str] = &[
    "crates/rdra-ish-cli/tests/fixtures/verus/library.rdra",
    "samples/formal-verification/quantifier_guard.rdra",
    "samples/formal-verification/counting_rules.rdra",
    "samples/formal-verification/min_card_met.rdra",
];

fn dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("codegen_verus")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creates the test directory");
    dir
}

fn stem(model: &str) -> &str {
    Path::new(model).file_stem().unwrap().to_str().unwrap()
}

/// `codegen --lang rust -o <out>/domain.rs`, with `--verus <out>/verus` when `verus`.
fn codegen(model: &str, out: &Path, verus: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    cmd.current_dir(repo())
        .args(["codegen", "--lang", "rust", "-o"])
        .arg(out.join("domain.rs"));
    if verus {
        cmd.arg("--verus").arg(out.join("verus"));
    }
    cmd.arg(model).output().expect("rdra-ish runs")
}

/// The Verus crate of `model`, in a directory of its own.
fn crate_of(model: &str, name: &str) -> PathBuf {
    let out = dir(name);
    let o = codegen(model, &out, true);
    assert!(
        o.status.success(),
        "{model}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    out.join("verus")
}

/// `verus main.rs` in `krate`, with `args`, on at most four threads.
fn verus(krate: &Path, args: &[&str]) -> Output {
    Command::new("verus")
        .current_dir(krate)
        .args(["--num-threads", "4", "main.rs"])
        .args(args)
        .output()
        .expect("verus runs")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn the_ordinary_output_is_the_same_with_verus() {
    for model in COVERED {
        let plain = dir(&format!("plain_{}", stem(model)));
        let with = dir(&format!("with_{}", stem(model)));
        assert!(codegen(model, &plain, false).status.success());
        let o = codegen(model, &with, true);
        assert!(o.status.success(), "{model}: {}", text(&o));
        assert_eq!(
            std::fs::read(plain.join("domain.rs")).unwrap(),
            std::fs::read(with.join("domain.rs")).unwrap(),
            "{model}: --verus changed the ordinary output"
        );
        let mut files: Vec<String> = std::fs::read_dir(with.join("verus"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(
            files,
            [
                "contract.rs",
                "core.rs",
                "harness.rs",
                "main.rs",
                "model.rs",
                "port.rs",
                "prelude.rs",
                "types.rs"
            ]
        );
    }
}

#[test]
fn a_model_outside_the_subset_is_refused_with_its_reasons() {
    for (model, reason) in [
        (
            "samples/formal-verification/int_stock.rdra",
            "Item.stock: Int columns are not covered",
        ),
        (
            "samples/formal-verification/event_order.rdra",
            "events (raises / triggers) are not covered",
        ),
        (
            "samples/formal-verification/split_run_order.rdra",
            "choices are not covered",
        ),
        (
            "samples/formal-verification/unique_booking_guarded.rdra",
            "Room: no use case creates it",
        ),
    ] {
        let out = dir(&format!("refused_{}", stem(model)));
        let o = codegen(model, &out, true);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{model}: --verus accepted it");
        assert!(err.contains(reason), "{model}: no `{reason}` in\n{err}");
        assert!(
            err.contains("--verus does not cover this model yet"),
            "{model}: {err}"
        );
        assert!(!out.join("verus").exists(), "{model}: wrote a crate");
        assert!(
            out.join("domain.rs").exists(),
            "{model}: the ordinary output was not written"
        );
    }
}

#[test]
fn verus_is_for_rust_only() {
    let out = dir("ts");
    let o = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["codegen", "--lang", "ts", "--verus"])
        .arg(out.join("verus"))
        .arg(COVERED[0])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--verus is for --lang rust only"));
}

#[test]
fn the_generated_crates_verify() {
    if !tool("verus", REQUIRE) {
        return;
    }
    for model in COVERED {
        let krate = crate_of(model, &format!("verify_{}", stem(model)));
        let o = verus(&krate, &[]);
        let out = text(&o);
        assert!(o.status.success(), "{model}:\n{out}");
        assert!(out.contains(" 0 errors"), "{model}:\n{out}");
    }
}

/// A rule `verify` refutes is not inductive: its preservation lemma fails, at the action that
/// breaks it.
#[test]
fn verus_rejects_a_model_whose_rule_fails() {
    if !tool("verus", REQUIRE) {
        return;
    }
    for (model, lemma) in [
        (
            "samples/formal-verification/one_to_one_shared.rdra",
            "ship_preserves",
        ),
        (
            "samples/formal-verification/none_any_child_bypassed.rdra",
            "reissue_preserves",
        ),
    ] {
        let krate = crate_of(model, &format!("refuted_{}", stem(model)));
        let o = verus(&krate, &[]);
        let out = text(&o);
        assert!(!o.status.success(), "{model}: verified");
        let model_rs = std::fs::read_to_string(krate.join("model.rs")).unwrap();
        // The line of each failed postcondition, and the lemma it is in.
        let failed: Vec<String> = out
            .lines()
            .filter_map(|l| l.trim().strip_prefix("--> model.rs:"))
            .filter_map(|at| at.split(':').next()?.parse::<usize>().ok())
            .map(|line| {
                model_rs
                    .lines()
                    .take(line)
                    .filter(|l| l.starts_with("pub proof fn "))
                    .last()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert!(
            failed.iter().any(|f| f.contains(lemma)),
            "{model}: no failure in {lemma}:\n{out}"
        );
    }
}

/// The claim: a core that drops a check its contract asks for does not verify, and the error
/// names the core.
#[test]
fn a_core_that_drops_a_condition_fails_its_contract() {
    if !tool("verus", REQUIRE) {
        return;
    }
    let krate = crate_of(COVERED[0], "weakened");
    let core = std::fs::read_to_string(krate.join("core.rs")).unwrap();
    // Borrow's RoomForLoan check: `if !(...) { return Err(...Rejected...) }`, dropped.
    let at = core.find("condition: \"RoomForLoan\"").expect("the check");
    let check = core[..at].rfind("        if !").expect("its test");
    let end = at + core[at..].find("        }\n").unwrap() + "        }\n".len();
    let weakened = format!("{}{}", &core[..check], &core[end..]);
    std::fs::write(krate.join("core.rs"), weakened).unwrap();
    let o = verus(&krate, &[]);
    let out = text(&o);
    assert!(!o.status.success(), "verified without the check:\n{out}");
    // The failed postcondition is the method's contract, and the body is the core's.
    assert!(out.contains("borrow_from_active_spec"), "{out}");
    assert!(out.contains("::: core.rs:"), "{out}");
    assert!(out.contains(" 1 errors"), "{out}");
}

/// Not vacuous: `assert(false)` at the top of every preservation lemma, every core method and
/// every port call fails, each on its own.
#[test]
fn an_assert_false_fails_in_every_proof() {
    if !tool("verus", REQUIRE) {
        return;
    }
    let krate = crate_of(COVERED[0], "vacuity");
    let mut planted = 0;
    for (file, starts) in [
        ("model.rs", &["pub proof fn "][..]),
        ("core.rs", &["    fn "][..]),
        ("port.rs", &["pub fn port_"][..]),
    ] {
        let path = krate.join(file);
        let mut out = String::new();
        let mut inside = false;
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            if starts.iter().any(|s| line.starts_with(s)) {
                inside = true;
            }
            out.push_str(line);
            out.push('\n');
            if inside && line.trim_end().ends_with('{') && !line.contains("match") {
                out.push_str("    assert(false);\n");
                planted += 1;
                inside = false;
            }
        }
        std::fs::write(&path, out).unwrap();
    }
    assert!(planted >= 15, "planted {planted}");
    let o = verus(&krate, &[]);
    let out = text(&o);
    let failed = out.matches("error: assertion failed").count();
    assert_eq!(failed, planted, "{out}");
}

/// The port, run on the in-memory store, moves as BFS does at two rows per entity: from every
/// state BFS reaches, every call on every slot.
#[test]
fn the_verified_port_moves_as_bfs_does() {
    if !tool("verus", REQUIRE) {
        return;
    }
    for model in COVERED {
        let krate = crate_of(model, &format!("harness_{}", stem(model)));
        let o = verus(&krate, &["--compile", "-o", "harness"]);
        assert!(o.status.success(), "{model}:\n{}", text(&o));
        let project = load_project(&[repo().join(model)], None).expect("loads");
        let domain = build(&project.model, Some(Caps::for_instances(1)), &|_| None).unwrap();
        let dump = bfs_dump(&project.model, &domain, NonZeroU8::new(2).unwrap())
            .unwrap_or_else(|w| panic!("{model}: {w:?}"));
        std::fs::write(krate.join("bfs.txt"), dump).unwrap();
        let o = Command::new(krate.join("harness"))
            .arg(krate.join("bfs.txt"))
            .output()
            .unwrap();
        let out = text(&o);
        assert!(o.status.success(), "{model}:\n{out}");
        assert!(out.contains("mismatch 0"), "{model}:\n{out}");
        println!("{model}: {out}");
    }
}
