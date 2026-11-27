//! `check --code[=fresh]` and `codegen --code` (docs/design/code-check.md, P1): `[code.<lang>]`
//! in rdra.toml names the generated files, `codegen --code` writes them, and `check --code`
//! generates them again and compares: the code by its text, the traces by hash. Each cause of a
//! difference (file edited, model changed, generator changed) is named, as `codegen --check`
//! names it. `check` without `--code` behaves as before, with one note on stderr.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/edit.rs"]
mod edit;
#[path = "common/output.rs"]
mod output;
use edit::edit;
use output::outcome;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen/conformance.rdra")
}

const MANIFEST: &str = r#"[model]
name = "shop"

[code.rust]
domain = "../svc/src/domain.rs"
traces = "../svc/tests/traces.txt"
conformance_states = 4096
"#;

/// A project of its own: `model/` with the fixture and `manifest`, `svc/` for the code.
fn project(name: &str, manifest: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-check-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("model")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::copy(fixture(), dir.join("model/order.rdra")).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    dir
}

fn rdra(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("runs rdra-ish")
}

fn generated(name: &str) -> PathBuf {
    let dir = project(name, MANIFEST);
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.contains("wrote svc/src/domain.rs"), "{stderr}");
    assert!(stderr.contains("wrote svc/tests/traces.txt"), "{stderr}");
    dir
}

#[test]
fn the_files_codegen_writes_are_fresh_wherever_check_runs() {
    let dir = generated("same");
    let traces = std::fs::read_to_string(dir.join("svc/tests/traces.txt")).unwrap();
    let mut lines = traces.lines();
    assert_eq!(lines.next(), Some("# rdra-ish conformance traces 3"));
    assert!(
        lines
            .next()
            .is_some_and(|l| l.starts_with("# rdra-ish codegen fingerprint: model=")),
        "{traces}"
    );
    for (cwd, input) in [(dir.clone(), "model"), (dir.join("model"), ".")] {
        let (code, stdout, stderr) = outcome(&rdra(&cwd, &["check", input, "--code"]));
        assert_eq!(code, Some(0), "{stderr}");
        assert!(
            stdout.ends_with("OK: code: what the model requires, at --code=fresh\n"),
            "{stdout}"
        );
        assert!(
            stderr.contains("domain.rs is what the model generates"),
            "{stderr}"
        );
        assert!(
            stderr.contains("traces.txt holds the traces the model generates"),
            "{stderr}"
        );
    }
}

#[test]
fn an_edited_domain_file_fails_as_edited() {
    let dir = generated("edited");
    edit(
        &dir.join("svc/src/domain.rs"),
        "pub struct Rejected",
        "pub struct Refusal",
    );
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=fresh"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("file edited"), "{stderr}");
    assert!(!stderr.contains("model changed"), "{stderr}");
    assert!(stderr.contains("rdra-ish codegen model --code"), "{stderr}");
    assert!(
        stdout.contains("FAIL: code: 1 error(s) at --code=fresh"),
        "{stdout}"
    );
}

#[test]
fn edited_traces_fail_as_edited_by_their_hash() {
    let dir = generated("traces");
    let path = dir.join("svc/tests/traces.txt");
    let text_before = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text_before}T 0\n")).unwrap();
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("traces.txt is not the traces `codegen` generates"),
        "{stderr}"
    );
    assert!(stderr.contains("file edited"), "{stderr}");
    assert!(
        stderr.contains("domain.rs is what the model generates"),
        "{stderr}"
    );
}

#[test]
fn a_changed_model_fails_as_changed() {
    let dir = generated("model");
    edit(&dir.join("model/order.rdra"), "qty > 4", "qty > 3");
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("model changed"), "{stderr}");
    assert!(!stderr.contains("file edited"), "{stderr}");
}

#[test]
fn other_options_in_rdra_toml_fail_as_a_generator_change() {
    let dir = generated("options");
    std::fs::write(
        dir.join("model/rdra.toml"),
        MANIFEST.replace("conformance_states = 4096", "conformance_states = 3"),
    )
    .unwrap();
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("generator changed"), "{stderr}");
}

#[test]
fn a_missing_file_fails_and_says_what_writes_it() {
    let dir = project("missing", MANIFEST);
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("svc/src/domain.rs does not exist: `codegen --code` writes it"),
        "{stderr}"
    );
}

#[test]
fn code_without_a_code_table_is_an_error() {
    let dir = project("no-table", "[model]\nname = \"shop\"\n");
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("--code needs a [code.rust], [code.go] or [code.ts] table"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK"), "{stdout}");
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
}

#[test]
fn check_without_code_prints_a_note_and_nothing_else_changes() {
    let with = project("note", MANIFEST);
    let without = project("plain", "[model]\nname = \"shop\"\n");
    let (code_with, stdout_with, stderr_with) = outcome(&rdra(&with, &["check", "model"]));
    let (code_without, stdout_without, stderr_without) =
        outcome(&rdra(&without, &["check", "model"]));
    assert_eq!(code_with, code_without);
    assert_eq!(stdout_with, stdout_without);
    let note: Vec<&str> = stderr_with
        .lines()
        .filter(|l| !stderr_without.lines().any(|w| w == *l))
        .collect();
    assert_eq!(
        note,
        ["note: model/rdra.toml has [code]; `check --code` also checks the service code against the model"],
        "{stderr_with}"
    );
}

#[test]
fn an_unknown_key_under_code_is_an_error() {
    let dir = project(
        "unknown",
        &format!("{MANIFEST}servise = \"shop::Service\"\n"),
    );
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("rdra.toml"), "{stderr}");
    assert!(stderr.contains("servise"), "{stderr}");
}

/// The level is one token with `=`: `--code model` reads `model` as an input, not a level.
#[test]
fn a_bare_code_flag_takes_no_level_from_the_next_argument() {
    let dir = generated("bare");
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "--code", "model"]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("at --code=fresh"), "{stdout}");
}

/// `codegen --format json` prints the contract as data, without `--lang`.
#[test]
fn the_json_contract_names_obligations_and_sites() {
    let dir = project("json", MANIFEST);
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--format", "json"]));
    assert_eq!(code, Some(0), "{stderr}");
    for want in [
        "\"format\": \"rdra-ish codegen json 1\"",
        "\"name\": \"Unique_Shipment_Order\"",
        "\"name\": \"Ref_Shipment_order_id\"",
        "\"site\": \"model/order.rdra:",
        "\"kind\": \"functions\"",
    ] {
        assert!(stdout.contains(want), "{want}:\n{stdout}");
    }
    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &["codegen", "model", "--format", "json", "--check", "x.json"],
    ));
    assert_eq!(code, Some(1), "{stderr}");
}

/// The obligation ledger (P5): with every obligation owned, by a marker or by
/// `[code.obligations]`, the check passes; adding `@unique` to the model adds exactly one
/// diagnostic, the new obligation with no owner. A marker naming a missing file fails it.
#[test]
fn the_ledger_wants_an_owner_per_obligation() {
    let dir = generated("ledger");
    let json = outcome(&rdra(&dir, &["codegen", "model", "--format", "json"])).1;
    let contract: serde_json::Value = serde_json::from_str(&json).expect("the JSON contract");
    let names: Vec<String> = contract["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.len() > 3, "{names:?}");
    // The first through the table, the rest by markers in one file.
    let (first, rest) = names.split_first().unwrap();
    std::fs::write(
        dir.join("model/rdra.toml"),
        format!("{MANIFEST}\n[code.obligations]\n{first} = \"../svc/src/owners.rs\"\n"),
    )
    .unwrap();
    let markers: String = rest
        .iter()
        .map(|n| format!("// rdra: obligation {n}\n"))
        .collect();
    std::fs::write(dir.join("svc/src/owners.rs"), &markers).unwrap();
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(!stderr.contains("CodeObligation"), "{stderr}");

    let model = dir.join("model/order.rdra");
    edit(&model, "  note: String\n", "  note: String @unique\n");
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(0), "{stderr}");
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    let unowned: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("error[CodeObligation"))
        .collect();
    assert_eq!(
        unowned,
        ["error[CodeObligationUnowned_Unique_Order_note]: no code owns the obligation Unique_Order_note"],
        "{stderr}"
    );
    assert!(stderr.contains("  model: model/order.rdra:"), "{stderr}");

    std::fs::write(
        dir.join("svc/src/owners.rs"),
        format!("{markers}// rdra: obligation Unique_Order_note migrations/none.sql\n"),
    )
    .unwrap();
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains(
            "error[CodeObligationOwnerMissing_Unique_Order_note]: the owner of Unique_Order_note does not exist"
        ),
        "{stderr}"
    );
}

/// `--seal` and the `[code.ts]` keys `seal` and `tsconfig` are TypeScript's: on another
/// language, `--seal` is a usage error (exit 2) and the keys an error naming the table.
#[test]
fn seal_is_for_typescript_only() {
    let dir = project("seal", MANIFEST);
    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &["codegen", "model/order.rdra", "--lang", "rust", "--seal"],
    ));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--seal is for --lang ts only"), "{stderr}");
    let dir = project("seal-key", &format!("{MANIFEST}seal = true\n"));
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains(
            "[code.rust] in model/rdra.toml: `seal` and `tsconfig` are for [code.ts] only"
        ),
        "{stderr}"
    );
}

/// `serde` is a `[code.rust]` key for the domain code: under another language's table, or in a
/// table that names `adapter`, it is an error naming the table.
#[test]
fn serde_is_for_rust_domain_code_only() {
    let go = MANIFEST.replace("[code.rust]", "[code.go]");
    let dir = project("serde-go", &format!("{go}serde = true\n"));
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("[code.go] in model/rdra.toml: `serde` is for [code.rust] only"),
        "{stderr}"
    );
    let adapter = MANIFEST.replace("domain = ", "adapter = ");
    let dir = project("serde-adapter", &format!("{adapter}serde = true\n"));
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("[code.rust] in model/rdra.toml: `serde` derives on the domain code, and this table names `adapter`"),
        "{stderr}"
    );
}

/// A manifest whose `build` and `run` are rdra-ish's own `--version`: the conformance run
/// passes with no toolchain, so what is tested is the statement of what it covered (with
/// `--mutants 0`: such a run fails the self-check) or the self-check itself.
fn noop_run(states: usize) -> String {
    let bin = env!("CARGO_BIN_EXE_rdra-ish");
    format!(
        "{}root = \"../svc\"\nbuild = ['{bin}', '--version']\nrun = ['{bin}', '--version']\n",
        MANIFEST.replace(
            "conformance_states = 4096",
            &format!("conformance_states = {states}")
        )
    )
}

fn run_project(name: &str, states: usize) -> PathBuf {
    let dir = project(name, &noop_run(states));
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(0), "{stderr}");
    dir
}

/// The lines of the coverage statement on stdout: from its title to the summary line.
fn statement(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .skip_while(|l| !l.starts_with("coverage of "))
        .take_while(|l| !l.starts_with("OK: code") && !l.starts_with("FAIL: code"))
        .collect()
}

/// #439: `--code=run` states what the traces cover, on stdout before the summary line: the
/// bound, the BFS, the moves run and left out, the refused calls, and the assumptions of
/// bounded conformance. A complete suite warns of nothing.
#[test]
fn a_conformance_run_states_its_coverage() {
    let dir = run_project("coverage", 4096);
    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run", "--mutants", "0"],
    ));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.ends_with("OK: code: what the model requires, at --code=run\n"),
        "{stdout}"
    );
    let lines = statement(&stdout);
    assert_eq!(
        lines[..5],
        [
            "coverage of [code.rust]: bounded conformance to the model, not equivalence",
            "  bound: 1 row created per entity, Int 0..5",
            "  BFS: complete, 6 states; 6 reached by the traces' calls",
            "  moves: 5 of 5 run",
            "  refused calls: 9 tried (calls on one existing row); interleaved commits: 7",
        ],
        "{stdout}"
    );
    assert!(lines[5].starts_with("  assumes: H1 "), "{stdout}");
    assert!(
        lines[6].starts_with("  cannot show: writes outside the port"),
        "{stdout}"
    );
    assert_eq!(lines.len(), 7, "{stdout}");
    assert!(!stderr.contains("CodeCoverageIncomplete"), "{stderr}");
    // The same statement whenever it runs, and none below --code=run.
    let again = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run", "--mutants", "0"],
    ))
    .1;
    assert_eq!(statement(&again), lines);
    let (_, build, _) = outcome(&rdra(&dir, &["check", "model", "--code=build"]));
    assert!(!build.contains("coverage of"), "{build}");
}

/// A suite whose BFS stopped at its cap tries no refused call: it warns and passes, and
/// `--deny-incomplete` fails it. The flag needs `--code=run`.
#[test]
fn a_capped_suite_warns_and_deny_incomplete_fails_it() {
    let dir = run_project("capped", 3);
    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run", "--mutants", "0"],
    ));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stderr.contains("warning[CodeCoverageIncomplete]: [code.rust]: the conformance BFS stopped at its cap of 3 states"),
        "{stderr}"
    );
    let lines = statement(&stdout);
    assert!(
        lines.contains(&"  BFS: stopped at its cap of 3 states; 3 reached by the traces' calls"),
        "{stdout}"
    );
    assert!(
        lines.contains(
            &"  refused calls: none tried (the BFS stopped at its cap); interleaved commits: 2"
        ),
        "{stdout}"
    );

    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &[
            "check",
            "model",
            "--code=run",
            "--deny-incomplete",
            "--mutants",
            "0",
        ],
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("error[CodeCoverageIncomplete]: [code.rust]:"),
        "{stderr}"
    );
    assert!(
        stdout.ends_with("FAIL: code: 1 error(s) at --code=run\n"),
        "{stdout}"
    );

    let complete = run_project("complete-deny", 4096);
    let (code, _, stderr) = outcome(&rdra(
        &complete,
        &[
            "check",
            "model",
            "--code=run",
            "--deny-incomplete",
            "--mutants",
            "0",
        ],
    ));
    assert_eq!(code, Some(0), "{stderr}");

    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=build", "--deny-incomplete"],
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("--deny-incomplete needs --code=run"),
        "{stderr}"
    );
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--deny-incomplete"]));
    assert_eq!(code, Some(2), "{stderr}");
}

/// `codegen` states the coverage of the traces it writes, on stderr, and warns of a cap.
#[test]
fn codegen_states_the_coverage_of_the_traces_it_writes() {
    let dir = project("codegen-coverage", &noop_run(3));
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stderr.contains(
            "coverage of svc/tests/traces.txt: bounded conformance to the model, not equivalence"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "warning: svc/tests/traces.txt: the conformance BFS stopped at its cap of 3 states"
        ),
        "{stderr}"
    );
    let dir = project("codegen-complete", MANIFEST);
    let (_, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"]));
    assert!(stderr.contains("  BFS: complete, 6 states"), "{stderr}");
    assert!(!stderr.contains("warning: "), "{stderr}");
}

/// #439, the self-check: a run command that never reads the traces (`rdra-ish --version`)
/// passes every mutant trace, and each is an error that says the command must run the
/// harness on them. `--mutants 0` runs none; the seed picks the same mutants every time.
#[test]
fn a_run_that_ignores_the_traces_fails_the_self_check() {
    let dir = run_project("self-check", 4096);
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=run"]));
    assert_eq!(code, Some(1), "{stderr}");
    let survived: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("error[CodeMutantSurvived]"))
        .collect();
    assert_eq!(survived.len(), 4, "{stderr}");
    for kind in [
        "refused-call-runs",
        "call-refused",
        "wrong-state",
        "absent-row",
    ] {
        assert!(
            survived
                .iter()
                .any(|l| l.contains(&format!("the {kind} mutant"))),
            "{kind}: {stderr}"
        );
    }
    // No harness ran, so even the absent row points at the command, not the service.
    assert_eq!(
        stderr
            .matches("must run the probe's conformance test on the traces RDRA_ISH_TRACES names")
            .count(),
        4,
        "{stderr}"
    );
    assert!(
        stdout
            .contains("  self-check: 0 of 4 mutant traces failed at their changed row (seed 0)\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("FAIL: code: 4 error(s) at --code=run\n"),
        "{stdout}"
    );
    let again = outcome(&rdra(&dir, &["check", "model", "--code=run"])).2;
    assert_eq!(again, stderr, "the same mutants on every run");

    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run", "--mutants", "0"],
    ));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stdout.contains("self-check"), "{stdout}");

    let seeded = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run", "--mutant-seed", "3"],
    ));
    assert!(seeded.1.contains("(seed 3)"), "{}", seeded.1);
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code", "--mutants", "2"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("--mutants needs --code=run"), "{stderr}");
}

/// A capped suite has no refused call to flip: the self-check says so, and never counts a
/// mutant it could not make.
#[test]
fn a_capped_suite_has_no_refused_call_to_flip() {
    let dir = run_project("self-check-capped", 3);
    let (_, stdout, _) = outcome(&rdra(&dir, &["check", "model", "--code=run"]));
    let line = stdout
        .lines()
        .find(|l| l.starts_with("  self-check:"))
        .unwrap_or_default();
    assert!(
        line.contains("; no refused-call-runs: the traces try no refused call"),
        "{stdout}"
    );
    assert!(line.contains(" of 3 mutant traces"), "{stdout}");
}
