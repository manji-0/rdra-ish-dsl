//! `--memory` and `memory` in `rdra.toml`: a BFS cone whose states pass the limit stops as it
//! does at the state cap, and `verify` sizes and queues its TLC runs to fit the limit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .env("TMPDIR", Path::new(env!("CARGO_TARGET_TMPDIR")))
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

const ORDER: &str = "samples/formal-verification/order.rdra";
const FLAGS: &str = "crates/rdra-ish-cli/tests/fixtures/memory/flags.rdra";

/// A project of its own: a repository root with `rdra.toml` and `order.rdra`.
fn project(test: &str, manifest: &str) -> PathBuf {
    let dir = scratch(test);
    std::fs::create_dir(dir.join(".git")).unwrap();
    std::fs::write(dir.join("rdra.toml"), manifest).unwrap();
    std::fs::copy(repo().join(ORDER), dir.join("order.rdra")).unwrap();
    dir
}

#[test]
fn a_cone_past_the_memory_limit_stops_as_at_the_state_cap() {
    for cmd in ["check", "states"] {
        let out = run(&repo(), &[cmd, "--memory", "1KiB", ORDER]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{cmd}\n{stderr}");
        assert!(
            stderr.contains(
                "  whether DeliverOrder can run is not decided: the Order cone stopped at the \
                 memory limit (1KiB; raise it with --memory or `memory` under [check] in rdra.toml)"
            ),
            "{cmd}\n{stderr}"
        );
    }
    // `check` names the stop as an error; `states` on the view it cut, on stdout.
    let out = run(&repo(), &["check", "--memory", "1KiB", ORDER]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "error[MemoryCapReached_Order]: state derivation for entity 'Order': memory limit \
             1KiB reached at 5 states; results are incomplete: the cone has 3 variables"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "raise it to 1MiB or more with --memory or `memory` under [check] in rdra.toml, or \
             narrow with --entity or --buc"
        ),
        "{stderr}"
    );
    let out = run(&repo(), &["states", "--memory", "1KiB", ORDER]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("memory limit 1KiB reached at 5 states"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    for args in [
        &["check", ORDER][..],
        &["states", ORDER],
        &["check", "--memory", "4KiB", ORDER],
    ] {
        let out = run(&repo(), args);
        assert!(
            out.status.success(),
            "{args:?}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// A run that stops at the memory limit says what it held, and, when the cone's variables
/// bound its states, what finishing needs and what to give `--memory`. `check` has no state
/// cap, so the bounds line says so and `--max-states` still caps.
#[test]
fn a_run_at_the_memory_limit_estimates_what_finishing_needs() {
    let out = run(&repo(), &["check", "--memory", "1KiB", ORDER]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with(
            "bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 1KiB;"
        ),
        "{stderr}"
    );
    // One layer past the peak says nothing yet, so the cone's variables bound what it needs.
    assert!(
        stderr.contains(
            "new states first reached by: CancelOrder 1, CapturePay 1, PlaceOrder 1, ShipOrder 1; \
             held at 177 B per state; the frontier peaked at depth 2, but by depth 3 it has not \
             fallen far enough to estimate the states left; the cone's variables allow at most 30 \
             states, so if all are reachable this cone alone needs at least about 1MiB; raise it \
             to 1MiB or more with --memory"
        ),
        "{stderr}"
    );
    // A cone still growing, whose variables allow far more states than 1TiB holds, gives no
    // figure.
    let out = run(
        &repo(),
        &["check", "--memory", "4MiB", "samples/insurance-claim-ops"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "the frontier still grows (346 new states at depth 11, 1.1x the depth before), so its \
             layers give no estimate of the states left; raise it with --memory or `memory` under \
             [check] in rdra.toml, or narrow with --entity or --buc"
        ),
        "{stderr}"
    );
    assert!(!stderr.contains("1TiB"), "{stderr}");
    // The estimate is a figure `--memory` takes: the run it names completes.
    let again = run(&repo(), &["check", "--memory", "1MiB", ORDER]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );

    let out = run(
        &repo(),
        &["check", "--max-states", "3", "--memory", "1GiB", ORDER],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("bounds: 1 instance per entity, Int 0..5; 3 states per cone;"),
        "{stderr}"
    );
    assert!(stderr.contains("error[StateCapReached_Order]"), "{stderr}");
}

/// A run stopped two layers or more past its frontier's peak estimates the states its cone
/// reaches from the layers. The `Task` cone of `flags.rdra` holds 257 states, its frontier
/// rising and falling with the number of flags on, and stopped at 256 the estimate is 258;
/// the `--memory` it names finishes the run.
#[test]
fn a_run_past_its_frontiers_peak_estimates_the_states_its_cone_reaches() {
    let out = run(&repo(), &["check", "--memory", "56KiB", FLAGS]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("memory limit 56KiB reached at 256 states; results are incomplete"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "the frontier peaked at depth 5 and has shrunk for 3 layers since, so the cone \
             probably holds about 258 states (its falling side taken to mirror its rising side: \
             an estimate, not a bound); with a quarter to spare, this cone alone probably needs \
             about 1MiB: try 1MiB with --memory or `memory` under [check] in rdra.toml, or narrow \
             with --entity or --buc"
        ),
        "{stderr}"
    );
    let again = run(&repo(), &["check", "--memory", "1MiB", FLAGS]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    // The cone holds 257 states: a state cap of 257 finishes it and one of 256 does not.
    let at = |cap: &str| {
        run(&repo(), &["check", "--max-states", cap, FLAGS])
            .status
            .success()
    };
    assert!(at("257") && !at("256"));
}

/// Cones explored side by side share the limit with the results of the cones before them,
/// and stop where they stop when run one at a time: the run is the same on one thread as on
/// eight.
#[test]
fn cones_side_by_side_stop_where_they_stop_one_at_a_time() {
    let run_on = |jobs: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
            .current_dir(repo())
            .env("RDRA_JOBS", jobs)
            .args(["check", "--memory", "4MiB", "samples/insurance-claim-ops"])
            .output()
            .expect("rdra-ish runs");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let one = run_on("1");
    let stops = one.matches("error[MemoryCapReached_").count();
    assert!((1..10).contains(&stops), "{stops} cones stopped\n{one}");
    assert_eq!(run_on("8"), one);
    assert_eq!(run_on("3"), one);
}

#[test]
fn the_flag_overrides_rdra_toml_which_overrides_the_default() {
    let dir = project(
        "memory_cli_precedence",
        "[model]\nname = \"order\"\n\n[check]\nmemory = \"1KiB\"\n",
    );
    let from_toml = run(&dir, &["check", "order.rdra"]);
    let stderr = String::from_utf8_lossy(&from_toml.stderr);
    assert_eq!(from_toml.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("memory limit 1KiB reached"), "{stderr}");

    let flag = run(&dir, &["check", "--memory", "4GiB", "order.rdra"]);
    assert!(
        flag.status.success(),
        "{}",
        String::from_utf8_lossy(&flag.stderr)
    );

    // `[check]` does not bound `verify`, which reads `[verify]`.
    let dir = project(
        "memory_cli_verify_section",
        "[model]\nname = \"order\"\n\n[check]\nmemory = \"1KiB\"\n\n[verify]\nmemory = \"1TiB\"\n",
    );
    let states = run(&dir, &["states", "order.rdra"]);
    assert_eq!(states.status.code(), Some(1));
}

#[test]
fn a_bad_memory_value_is_a_manifest_error_with_its_line() {
    let dir = project(
        "memory_cli_bad_manifest",
        "[model]\nname = \"order\"\n\n[verify]\nmemory = \"16\"\n",
    );
    let out = run(&dir, &["check", "order.rdra"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_ne!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("rdra.toml:5:"), "{stderr}");
    assert!(
        stderr.contains("invalid memory size \"16\": write a whole number and a unit"),
        "{stderr}"
    );

    let flag = run(&repo(), &["check", "--memory", "16", ORDER]);
    assert_eq!(flag.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&flag.stderr).contains("invalid memory size \"16\""));
}

/// Without `--with-tlc` BFS has all of `--memory`, and a cone past it leaves its findings
/// undecided.
#[test]
fn verify_leaves_a_cone_past_its_bfs_share_undecided() {
    let small = run(&repo(), &["verify", "--memory", "1MiB", FLAGS]);
    let stdout = String::from_utf8_lossy(&small.stdout);
    let stderr = String::from_utf8_lossy(&small.stderr);
    assert!(small.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains(
            "  whether some Task can get stuck at status = open is not decided: the Task cone \
             stopped at the memory limit (1MiB; raise it with --memory or `memory` under [verify] \
             in rdra.toml)"
        ),
        "{stdout}"
    );
    assert!(
        stderr.contains(
            "warning: 15 findings a cone stopped at the state cap or the memory limit left undecided"
        ),
        "{stderr}"
    );

    let denied = run(
        &repo(),
        &["verify", "--memory", "1MiB", "--deny-undecided", FLAGS],
    );
    assert_eq!(denied.status.code(), Some(1));

    let default = run(&repo(), &["verify", FLAGS]);
    let stdout = String::from_utf8_lossy(&default.stdout);
    assert!(default.status.success(), "{stdout}");
    assert!(!stdout.contains("memory limit"), "{stdout}");

    // Under `--with-tlc` the TLC runs take their share first, so 64MiB leaves BFS none.
    let too_small = run(
        &repo(),
        &["verify", "--with-tlc", "--memory", "64MiB", FLAGS],
    );
    assert!(String::from_utf8_lossy(&too_small.stderr)
        .contains("--memory 64MiB is too small for verify: a TLC run takes 64MiB"),);
}

/// Three entities, one rule each: three TLC cones that could run side by side.
#[cfg(unix)]
const THREE_CONES: &str = r#"
entity A "A" { id: Int @pk  status: Enum(x, y) @default(x) }
entity B "B" { id: Int @pk  status: Enum(x, y) @default(x) }
entity C "C" { id: Int @pk  status: Enum(x, y) @default(x) }
usecase MakeA "A"
usecase MakeB "B"
usecase MakeC "C"
creates(MakeA, A)
creates(MakeB, B)
creates(MakeC, C)
forbidden(A, status == y)
forbidden(B, status == y)
forbidden(C, status == y)
"#;

/// A `tlc` that records the `JAVA_TOOL_OPTIONS` of each run, and when it starts and ends.
#[cfg(unix)]
fn fake_tlc(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let tlc = bin.join("tlc");
    std::fs::write(
        &tlc,
        "#!/bin/sh\necho \"start $JAVA_TOOL_OPTIONS\" >> \"$RDRA_TEST_RECORD\"\nsleep 1\necho end >> \"$RDRA_TEST_RECORD\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&tlc, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// The `-Xmx` of every run, and the most runs that were going at once.
#[cfg(unix)]
fn tlc_runs(test: &str, memory: &str, env: &[(&str, &str)]) -> (Vec<String>, usize) {
    let dir = scratch(test);
    let model = dir.join("m.rdra");
    std::fs::write(&model, THREE_CONES).unwrap();
    let record = dir.join("record");
    let path = format!(
        "{}:{}",
        fake_tlc(&dir).display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    command
        .env("PATH", path)
        .env("RDRA_TEST_RECORD", &record)
        .env("TMPDIR", &dir)
        .env_remove("RDRA_TLC_HEAP")
        .env_remove("RDRA_TLC_JAR")
        .env_remove("JAVA_TOOL_OPTIONS")
        .args([
            "verify",
            "--with-tlc",
            "--memory",
            memory,
            model.to_str().unwrap(),
        ]);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("rdra-ish runs");
    let text = std::fs::read_to_string(&record).expect("tlc ran");
    let (mut going, mut most, mut heaps) = (0usize, 0usize, Vec::new());
    for line in text.lines() {
        if let Some(opts) = line.strip_prefix("start ") {
            going += 1;
            most = most.max(going);
            heaps.push(
                opts.split_whitespace()
                    .find(|o| o.starts_with("-Xmx"))
                    .expect("an -Xmx")
                    .to_string(),
            );
        } else {
            going -= 1;
        }
    }
    (heaps, most)
}

#[cfg(unix)]
#[test]
fn verify_sizes_and_queues_tlc_runs_to_fit_the_memory() {
    // 16GiB: TLC's half holds eight runs of 1g, so the three cones go side by side.
    let (heaps, most) = tlc_runs("memory_cli_tlc_default", "16GiB", &[]);
    assert!(heaps.iter().all(|h| h == "-Xmx1g"), "{heaps:?}");
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    assert!(most >= 2.min(cores), "{most} at once on {cores} cores");

    // 2GiB: TLC's half holds one run of 1g at a time.
    let (heaps, most) = tlc_runs("memory_cli_tlc_queued", "2GiB", &[]);
    assert!(heaps.iter().all(|h| h == "-Xmx1g"), "{heaps:?}");
    assert_eq!(most, 1);

    // 1GiB: TLC's half is less than one default heap, so the heap shrinks to it.
    let (heaps, most) = tlc_runs("memory_cli_tlc_shrunk", "1GiB", &[]);
    assert!(heaps.iter().all(|h| h == "-Xmx512m"), "{heaps:?}");
    assert_eq!(most, 1);

    // A heap the user names is kept, and the runs queue to fit it.
    let (heaps, most) = tlc_runs(
        "memory_cli_tlc_user_heap",
        "16GiB",
        &[("RDRA_TLC_HEAP", "4g")],
    );
    assert!(heaps.iter().all(|h| h == "-Xmx4g"), "{heaps:?}");
    assert!(most <= 2, "{most} at once");
}
