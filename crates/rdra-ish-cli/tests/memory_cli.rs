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
        let out = run(&repo(), &[cmd, "--memory", "4KiB", FLAGS]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{cmd}\n{stderr}");
        assert!(
            stderr.contains(
                "  whether some Task can get stuck at status = open is not decided: the Task cone \
                 stopped at the memory limit (4KiB; raise it with --memory or `memory` under \
                 [check] in rdra.toml)"
            ),
            "{cmd}\n{stderr}"
        );
    }
    // `check` names the stop as an error; `states` on the view it cut, on stdout.
    let out = run(&repo(), &["check", "--memory", "4KiB", FLAGS]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "error[MemoryCapReached_Task]: state derivation for entity 'Task': memory limit \
             4KiB reached at 64 states; results are incomplete: the cone has 9 variables"
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
    let out = run(&repo(), &["states", "--memory", "4KiB", FLAGS]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("memory limit 4KiB reached at 64 states"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    for args in [
        &["check", FLAGS][..],
        &["states", FLAGS],
        &["check", "--memory", "1MiB", FLAGS],
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
    let out = run(
        &repo(),
        &["check", "--memory", "1KiB", "--instance-count", "2", ORDER],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with(
            "bounds: 2 instances per entity, Int 0..5; no state cap per cone, memory 1KiB;"
        ),
        "{stderr}"
    );
    // A frontier still growing says nothing yet, so the cone's variables bound what it needs.
    assert!(
        stderr.contains(
            "new states first reached by: PlaceOrder 5, CancelOrder 3, CapturePay 3, DeliverOrder \
             1, ShipOrder 1; held at 71 B per state; the frontier still grows (5 new states at \
             depth 3, 1.7x the depth before), so its layers give no estimate of the states left; \
             the cone's variables allow at most 900 states, so if all are reachable this cone \
             alone needs at least about 1MiB; raise it to 1MiB or more with --memory"
        ),
        "{stderr}"
    );
    // A cone still growing, whose variables allow far more states than 1TiB holds, gives no
    // figure.
    let out = run(
        &repo(),
        &["check", "--memory", "1MiB", "samples/insurance-claim-ops"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "the frontier still grows (172 new states at depth 8, 1.3x the depth before), so its \
             layers give no estimate of the states left; raise it with --memory or `memory` under \
             [check] in rdra.toml, or narrow with --entity or --buc"
        ),
        "{stderr}"
    );
    assert!(!stderr.contains("1TiB"), "{stderr}");
    // The estimate is a figure `--memory` takes: the run it names completes.
    let again = run(
        &repo(),
        &["check", "--memory", "1MiB", "--instance-count", "2", ORDER],
    );
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
    let out = run(&repo(), &["check", "--memory", "17KiB", FLAGS]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("memory limit 17KiB reached at 256 states; results are incomplete"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "the frontier peaked at depth 5 and has shrunk for 3 layers since, so the cone \
             probably holds about 258 states (its falling side taken to mirror its rising side: \
             an estimate, not a bound); with a quarter to spare, this cone alone probably needs \
             about 1MiB: try 1MiB or more with --memory or `memory` under [check] in rdra.toml, or \
             narrow with --entity or --buc"
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
            .args(["check", "--memory", "1MiB", "samples/insurance-claim-ops"])
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

/// `verify` releases an entity cone that stops at the memory limit once its findings are
/// read, and the cones after it run in the room it held (#284). The run is the same on one
/// thread as on eight, and every pass after the entity pass reads past the released cones: a
/// reader of a released cone's graph panics.
#[test]
fn a_stopped_entity_cone_leaves_its_room_on_any_number_of_threads() {
    let dir = scratch("memory_cli_released_entity_cones");
    let run_on = |jobs: &str| {
        let spec = dir.join("spec");
        let _ = std::fs::remove_dir_all(&spec);
        let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
            .current_dir(repo())
            .env("RDRA_JOBS", jobs)
            .args(["verify", "--memory", "1MiB", "samples/patent-ops", "-o"])
            .arg(&spec)
            .output()
            .expect("rdra-ish runs");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!stderr.contains("panicked"), "{jobs} threads\n{stderr}");
        String::from_utf8_lossy(&out.stdout).into_owned() + &stderr
    };
    let one = run_on("1");
    assert!(
        one.contains("the OfficeAction cone stopped at the memory limit"),
        "an entity cone stops\n{one}"
    );
    assert_eq!(run_on("4"), one, "four threads");
    assert_eq!(run_on("8"), one, "eight threads");
}

/// Cones with equal sub-lifecycles share one quotient, which stays counted while `verify`
/// holds it (#252). Under limits where cones stop, the run is the same on one thread as on
/// eight: a cone takes a shared quotient only where computing its own would have fit, and the
/// shared quotients come and are charged in cone order. At these limits the quotients held
/// leave every cone where it stops with the sharing off.
#[test]
fn shared_quotients_stop_no_cone_elsewhere_on_any_number_of_threads() {
    let dir = scratch("memory_cli_shared_quotients");
    let run_on = |jobs: &str, memory: &str, sharing: bool| {
        let spec = dir.join("spec");
        let _ = std::fs::remove_dir_all(&spec);
        let mut command = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
        command
            .current_dir(repo())
            .env("RDRA_JOBS", jobs)
            .env("RDRA_ISH_QUOTIENT_STATS", "1")
            .args(["verify", "--memory", memory, "samples/patent-ops", "-o"])
            .arg(&spec);
        if !sharing {
            command.env("RDRA_ISH_NO_QUOTIENT_CACHE", "1");
        }
        let out = command.output().expect("rdra-ish runs");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let hits = stderr
            .lines()
            .find_map(|l| l.strip_prefix("quotient-cache: "))
            .and_then(|l| l.split(' ').find_map(|kv| kv.strip_prefix("hits=")))
            .map_or(0, |h| h.parse::<usize>().unwrap());
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &stderr
                .lines()
                .filter(|l| !l.starts_with("quotient-cache: "))
                .collect::<Vec<_>>()
                .join("\n");
        (text, hits)
    };
    for memory in ["2MiB", "3MiB", "8MiB"] {
        let (one, hits) = run_on("1", memory, true);
        assert!(
            one.contains("not decided"),
            "{memory}: some cone stops\n{one}"
        );
        assert!(hits > 0, "{memory}: some cone takes a shared quotient");
        assert_eq!(run_on("8", memory, true).0, one, "{memory}: eight threads");
        assert_eq!(run_on("1", memory, false).0, one, "{memory}: no sharing");
    }
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

const WIDE: &str = "crates/rdra-ish-cli/tests/fixtures/memory/wide.rdra";

/// The generators explore at the caps `check` runs at by default: a cone past 65,536 states
/// that `check` finishes, the generators finish too.
#[test]
fn the_generators_finish_a_cone_check_finishes() {
    for args in [
        &["check", WIDE][..],
        &["codegen", "--lang", "ts", WIDE],
        &["diagram", "--kind", "state", WIDE],
    ] {
        let out = run(&repo(), args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}\n{stderr}");
        assert!(!stderr.contains("StateCapReached"), "{args:?}\n{stderr}");
    }
    // The cone is past the 65,536 states the generators used to stop at.
    let out = run(&repo(), &["check", "--max-states", "65536", WIDE]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error[StateCapReached_Task]"), "{stderr}");
}

/// The generators take `memory` under `[check]` and stop where `check` stops.
#[test]
fn the_generators_stop_at_the_check_memory_limit() {
    let dir = project(
        "memory_cli_generators",
        "[model]\nname = \"order\"\n\n[check]\nmemory = \"1KiB\"\n",
    );
    for args in [
        &["codegen", "--lang", "ts", "order.rdra"][..],
        &["diagram", "--kind", "state", "order.rdra"],
    ] {
        let out = run(&dir, args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{args:?}\n{stderr}");
        assert!(
            stderr.contains("error[MemoryCapReached_Order]") && stderr.contains("1KiB"),
            "{args:?}\n{stderr}"
        );
    }
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

/// `two_queues.rdra` at 4KiB: the entity cones fit and the `can_reach` cone, their product,
/// does not. That `can_reach` is not decided, as at the state cap: listed under `not checked:`
/// with the limit, `not decided` on the `result:` line, and an error under `--deny-undecided`
/// (#246).
#[test]
fn a_can_reach_whose_cone_stops_at_the_memory_limit_is_not_decided() {
    const TWO_QUEUES: &str = "crates/rdra-ish-cli/tests/fixtures/reach/two_queues.rdra";
    let small = run(&repo(), &["verify", "--memory", "4KiB", TWO_QUEUES]);
    let stdout = String::from_utf8_lossy(&small.stdout);
    let stderr = String::from_utf8_lossy(&small.stderr);
    assert!(small.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains(
            "\nnot checked: 1 finding not decided\n  can_reach BothCanFinish is not decided: the \
             BothCanFinish cone stopped at the memory limit (4KiB; raise it with --memory or \
             `memory` under [verify] in rdra.toml)\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l == "can_reach (BFS on 1 cone): 1 checked, 0 violated, 1 not decided"),
        "{stdout}"
    );
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );

    let denied = run(
        &repo(),
        &["verify", "--memory", "4KiB", "--deny-undecided", TWO_QUEUES],
    );
    let stdout = String::from_utf8_lossy(&denied.stdout);
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert_eq!(denied.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK:"), "{stdout}");
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

/// A `tlc` that records the `JAVA_TOOL_OPTIONS` and arguments of each run, and when it starts
/// and ends.
/// With `oom_at`, a run whose options hold that `-Xmx` prints TLC's out-of-memory error and
/// exits 255, and any other run passes.
#[cfg(unix)]
fn fake_tlc(dir: &Path, oom_at: Option<&str>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let tlc = bin.join("tlc");
    let outcome = match oom_at {
        Some(xmx) => format!(
            "case \"$JAVA_TOOL_OPTIONS \" in\n\
             *\"-Xmx{xmx} \"*)\n\
               echo 'Error: Java ran out of memory.  Running Java with a larger maximum heap size may allow the model checker to complete.'\n\
               echo end >> \"$RDRA_TEST_RECORD\"\n\
               exit 255;;\n\
             esac\n\
             echo '4 states generated, 2 distinct states found, 0 states left on queue.'\n\
             echo 'Model checking completed. No error has been found.'\n"
        ),
        None => String::new(),
    };
    std::fs::write(
        &tlc,
        format!(
            "#!/bin/sh\necho \"start $JAVA_TOOL_OPTIONS -- $*\" >> \"$RDRA_TEST_RECORD\"\nsleep 1\n{outcome}echo end >> \"$RDRA_TEST_RECORD\"\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&tlc, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// What `verify --with-tlc` did with the TLC runs of `THREE_CONES` at `memory`.
#[cfg(unix)]
struct TlcRuns {
    /// The `-Xmx` of every run, in start order.
    heaps: Vec<String>,
    /// The `-workers` of every run, in start order.
    workers: Vec<String>,
    /// The most runs that were going at once.
    most: usize,
    /// Each start at which the runs going held more than the cone runs' share of `memory`,
    /// counting each JVM at its `-Xmx` and 256 MiB, with more than one run going.
    over: Vec<String>,
    stdout: String,
    stderr: String,
}

/// `-Xmx` as bytes: `1g`, `512m`.
#[cfg(unix)]
fn xmx_bytes(xmx: &str) -> u64 {
    let (digits, unit) = xmx.split_at(xmx.len() - 1);
    let n: u64 = digits.parse().expect("an -Xmx size");
    match unit {
        "g" => n << 30,
        "m" => n << 20,
        other => panic!("unit {other}"),
    }
}

#[cfg(unix)]
fn tlc_runs(test: &str, memory: &str, env: &[(&str, &str)], oom_at: Option<&str>) -> TlcRuns {
    let dir = scratch(test);
    let model = dir.join("m.rdra");
    std::fs::write(&model, THREE_CONES).unwrap();
    let record = dir.join("record");
    let path = format!(
        "{}:{}",
        fake_tlc(&dir, oom_at).display(),
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
    let output = command.output().expect("rdra-ish runs");
    // BFS on these cones ends within the first run, and from then on the cone runs may hold
    // all of --memory (#384); the half they hold while BFS runs is `schedule::pick`'s to test.
    let share: u64 = memory.trim_end_matches("GiB").parse::<u64>().expect("GiB") << 30;
    let text = std::fs::read_to_string(&record).expect("tlc ran");
    let mut runs = TlcRuns {
        heaps: Vec::new(),
        workers: Vec::new(),
        most: 0,
        over: Vec::new(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    // The fake runs do not say which run ends, so the run that ends is charged the largest
    // heap going: what is left going is then the least it can be.
    let mut going: Vec<u64> = Vec::new();
    for line in text.lines() {
        if let Some(opts) = line.strip_prefix("start ") {
            let xmx = opts
                .split_whitespace()
                .find_map(|o| o.strip_prefix("-Xmx"))
                .expect("an -Xmx");
            runs.heaps.push(format!("-Xmx{xmx}"));
            let args: Vec<&str> = opts.split_whitespace().collect();
            let at = args
                .iter()
                .position(|a| *a == "-workers")
                .expect("-workers");
            runs.workers.push(args[at + 1].to_string());
            going.push(xmx_bytes(xmx) + (256 << 20));
            runs.most = runs.most.max(going.len());
            let held: u64 = going.iter().sum();
            if going.len() > 1 && held > share {
                runs.over
                    .push(format!("{} runs hold {held} of {share}", going.len()));
            }
        } else {
            let largest = (0..going.len()).max_by_key(|i| going[*i]).expect("a run");
            going.remove(largest);
        }
    }
    runs
}

#[cfg(unix)]
#[test]
fn verify_sizes_and_queues_tlc_runs_to_fit_the_memory() {
    // 16GiB: TLC's share holds six runs of 1g and 256MiB each, so the three cones go side by
    // side.
    let runs = tlc_runs("memory_cli_tlc_default", "16GiB", &[], None);
    assert!(runs.heaps.iter().all(|h| h == "-Xmx1g"), "{:?}", runs.heaps);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    assert!(
        runs.most >= 2.min(cores),
        "{} at once on {cores} cores",
        runs.most
    );
    assert!(runs.over.is_empty(), "{:?}", runs.over);

    // 2GiB: neither TLC's half nor, once BFS has ended, all of it holds two runs of 1g and
    // the 256MiB each JVM holds beside it.
    let runs = tlc_runs("memory_cli_tlc_charged", "2GiB", &[], None);
    assert!(runs.heaps.iter().all(|h| h == "-Xmx1g"), "{:?}", runs.heaps);
    assert_eq!(runs.most, 1);

    // 1GiB: TLC's half is less than one default heap, so the heap shrinks to it, and stays so
    // once BFS has ended: a heap is sized by the half whenever its run starts.
    let runs = tlc_runs("memory_cli_tlc_shrunk", "1GiB", &[], None);
    assert!(
        runs.heaps.iter().all(|h| h == "-Xmx512m"),
        "{:?}",
        runs.heaps
    );
    assert_eq!(runs.most, 1);

    // A heap the user names is kept, and the runs queue to fit it: two of 4g and 256MiB
    // each are past all of 8GiB.
    let runs = tlc_runs(
        "memory_cli_tlc_user_heap",
        "8GiB",
        &[("RDRA_TLC_HEAP", "4g")],
        None,
    );
    assert!(runs.heaps.iter().all(|h| h == "-Xmx4g"), "{:?}", runs.heaps);
    assert_eq!(runs.most, 1);
}

/// `--tlc-workers` gives every run its workers, and `RDRA_TLC_JOBS` bounds the runs at once
/// below what the memory allows; a value that is not a whole number above 0 is refused (#384).
#[cfg(unix)]
#[test]
fn the_tlc_knobs_fix_the_workers_and_the_runs_at_once() {
    // The fake passes every run that does not ask for 99g, so no cone reruns on one worker.
    let runs = tlc_runs(
        "memory_cli_tlc_knobs",
        "16GiB",
        &[("RDRA_TLC_WORKERS", "3"), ("RDRA_TLC_JOBS", "1")],
        Some("99g"),
    );
    assert_eq!(
        runs.workers,
        ["3", "3", "3"],
        "{}{}",
        runs.stdout,
        runs.stderr
    );
    assert_eq!(runs.most, 1);
    assert!(runs.stdout.contains(", 3 workers)"), "{}", runs.stdout);

    // `RDRA_TLC_HEAP=states` names no heap: a cone of a few states keeps the module's 1g.
    let runs = tlc_runs(
        "memory_cli_tlc_heap_states",
        "16GiB",
        &[("RDRA_TLC_HEAP", "states")],
        Some("99g"),
    );
    assert!(runs.heaps.iter().all(|h| h == "-Xmx1g"), "{:?}", runs.heaps);
    assert!(runs.stdout.contains("\ntlc_result: ok"), "{}", runs.stdout);

    let dir = scratch("memory_cli_tlc_bad_jobs");
    let model = dir.join("m.rdra");
    std::fs::write(&model, THREE_CONES).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .env("RDRA_TLC_JOBS", "0")
        .args(["verify", "--with-tlc", model.to_str().unwrap()])
        .output()
        .expect("rdra-ish runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("RDRA_TLC_JOBS must be a whole number above 0, not `0`"),
        "{stderr}"
    );
}

/// A run that runs out of memory is run once more at twice the heap when TLC's share holds
/// that, and its line says so; when the share does not, it is a tool error with the hint.
/// It is not run again on one worker first, as a run that fails otherwise is (#309).
#[cfg(unix)]
#[test]
fn a_run_out_of_memory_reruns_once_at_twice_the_heap_within_the_share() {
    let runs = tlc_runs("memory_cli_tlc_oom_retry", "16GiB", &[], Some("1g"));
    assert_eq!(
        runs.heaps.iter().filter(|h| *h == "-Xmx1g").count(),
        3,
        "{:?}",
        runs.heaps
    );
    assert_eq!(
        runs.heaps.iter().filter(|h| *h == "-Xmx2g").count(),
        3,
        "{:?}",
        runs.heaps
    );
    assert!(runs.over.is_empty(), "{:?}", runs.over);
    // The line ends with what both launches cost (#384): the fake TLC sleeps a second in
    // each, the heap is the rerun's, and the workers the first launch's.
    let rerun = "(2 states, rerun at -Xmx2g after -Xmx1g ran out of memory, ";
    let reruns: Vec<&str> = runs.stdout.lines().filter(|l| l.contains(rerun)).collect();
    assert_eq!(reruns.len(), 3, "{}", runs.stdout);
    for line in reruns {
        let cost = line.split_once(rerun).unwrap().1;
        let (secs, rest) = cost.split_once(" s, ").expect("seconds");
        assert!(secs.parse::<f64>().unwrap() >= 2.0, "{line}");
        assert!(rest.starts_with("-Xmx2g, "), "{line}");
        assert!(
            rest.ends_with(" workers)") || rest.ends_with(" worker)"),
            "{line}"
        );
    }
    assert!(
        runs.stdout.contains("\ntlc_result: ok"),
        "{}{}",
        runs.stdout,
        runs.stderr
    );

    // 2GiB: TLC's half is 1GiB, which holds no 2g rerun.
    let runs = tlc_runs("memory_cli_tlc_oom_no_room", "2GiB", &[], Some("1g"));
    assert!(runs.heaps.iter().all(|h| h == "-Xmx1g"), "{:?}", runs.heaps);
    assert_eq!(runs.heaps.len(), 3, "{:?}", runs.heaps);
    assert!(
        runs.stderr
            .contains("hint: TLC ran out of memory; set the heap with RDRA_TLC_HEAP=4g"),
        "{}",
        runs.stderr
    );
    assert!(
        runs.stderr.contains("\ntlc_result: tool_error\n"),
        "{}",
        runs.stderr
    );
}
