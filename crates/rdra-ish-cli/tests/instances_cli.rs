//! `--instances <Entity>=<n>` and `instances` in `rdra.toml`: one entity's slot count, which
//! a flag sets over the command's table and every report states.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/output.rs"]
mod output;
#[path = "common/root.rs"]
mod root;
use output::text;
use root::repo;

/// A repository root of its own with `rdra.toml` and the order sample.
fn project(test: &str, manifest: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join("rdra.toml"), manifest).unwrap();
    std::fs::copy(
        repo().join("samples/formal-verification/order.rdra"),
        dir.join("order.rdra"),
    )
    .unwrap();
    dir
}

/// Runs `args`; a `verify` runs with `--no-prove`, since no test here is about the proof.
fn run(cwd: &Path, args: &[&str]) -> Output {
    let (command, rest) = args.split_first().expect("a command");
    let no_prove: &[&str] = match *command {
        "verify" => &["--no-prove"],
        _ => &[],
    };
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .arg(command)
        .args(no_prove)
        .args(rest)
        .output()
        .expect("rdra-ish runs")
}

#[test]
fn a_flag_sets_one_entity_over_the_table_of_its_command() {
    let dir = project(
        "instances_cli_precedence",
        "[model]\nname = \"order\"\n\n[check]\ninstances = { Order = 2 }\n\n[verify]\ninstances = { Order = 3 }\n",
    );
    let verify = text(&run(&dir, &["verify", "order.rdra"]));
    assert!(
        verify.contains("bounds: 2 instances per entity (Order=3), Int 0..5"),
        "{verify}"
    );
    let flag = text(&run(
        &dir,
        &["verify", "--instances", "Order=1", "order.rdra"],
    ));
    assert!(flag.contains("per entity (Order=1)"), "{flag}");
    // `check` reads `[check]`, not `[verify]`.
    let check = text(&run(&dir, &["check", "order.rdra"]));
    assert!(
        check.contains("1 instance per entity (Order=2), Int 0..5"),
        "{check}"
    );
    // The TLA+ export writes the modules `verify` checks, at `[verify]`'s counts.
    let out = dir.join("tla");
    let export = run(
        &dir,
        &["export", "--kind", "tla", "-o", "tla", "order.rdra"],
    );
    assert!(export.status.success(), "{}", text(&export));
    let tla = std::fs::read_to_string(out.join("RdraSpec.tla")).unwrap();
    assert!(tla.contains("InstanceCount=2 Order=3 IntRange"), "{tla}");
    assert!(tla.contains("Order_Ids == 1..3\n"), "{tla}");
}

#[test]
fn an_id_that_names_no_entity_is_an_error() {
    let dir = project("instances_cli_flag_id", "[model]\nname = \"order\"\n");
    let out = run(&dir, &["verify", "--instances", "Ordr=3", "order.rdra"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out).contains(
            "--instances names Ordr, which is no entity of the model; the entities are Order"
        ),
        "{}",
        text(&out)
    );

    let dir = project(
        "instances_cli_table_id",
        "[model]\nname = \"order\"\n\n[check]\ninstances = { Ordr = 2 }\n",
    );
    let out = run(&dir, &["states", "order.rdra"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out).contains("rdra.toml: `instances` under [check] names Ordr"),
        "{}",
        text(&out)
    );
}

#[test]
fn a_count_outside_one_to_255_is_refused() {
    let dir = project(
        "instances_cli_zero",
        "[model]\nname = \"order\"\n\n[verify]\ninstances = { Order = 0 }\n",
    );
    let out = run(&dir, &["verify", "order.rdra"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("rdra.toml:5"), "{}", text(&out));

    let out = run(
        &repo(),
        &[
            "check",
            "--instances",
            "Order=256",
            "samples/formal-verification/order.rdra",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("the count of Order must be between 1 and 255"),
        "{}",
        text(&out)
    );
}
