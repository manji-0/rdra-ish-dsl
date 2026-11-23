//! `check --guards`: each guard dropped in turn, and the rules that then fail.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

/// The exit code and stdout of `rdra-ish <args>` run at the repository root, with sites
/// relative to it.
fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .output()
        .expect("rdra-ish runs");
    let text = |b: &[u8]| common::relative_to(&String::from_utf8_lossy(b), &repo());
    (
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr),
    )
}

const FIXTURES: &str = "crates/rdra-ish-cli/tests/fixtures/guards";

/// One guard keeps the rule; the other keeps none, and the report says what that means
/// without calling it wrong. The report is on stdout and leaves the exit code to `check`.
#[test]
fn a_guard_no_rule_depends_on_is_named_with_its_use_case() {
    let file = format!("{FIXTURES}/unprotected.rdra");
    let (code, stdout, stderr) = run(&["check", &file, "--guards"]);
    assert_eq!(code, Some(0), "{stderr}");
    let expected = format!(
        "\
guards: 2 guards: 1 protect a rule, 1 no rule depends on; 2 cones explored again
guard: condition PaidOnly on Ship   {file}:28:1
  protects: 「A shipped order was paid」
guard: condition StockChecked on Ship   {file}:32:1
  no rule depends on this guard; it is either a business rule nobody wrote as a rule, or not needed
rules:
  「A shipped order was paid」 depends on condition PaidOnly on Ship
OK: no errors within 1 instance per entity, Int 0..5
"
    );
    assert_eq!(stdout, expected);
    assert!(
        !stderr.contains("warning") && !stderr.contains("error"),
        "the report adds no finding: {stderr}"
    );
}

/// Without `--guards`, `check` prints no report.
#[test]
fn check_without_the_flag_prints_no_report() {
    let file = format!("{FIXTURES}/unprotected.rdra");
    let (code, stdout, _) = run(&["check", &file]);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        "OK: no errors within 1 instance per entity, Int 0..5\n"
    );
}

/// A cone that stops at the cap once a guard is gone decides nothing: the rule is not
/// decided without the guard, and the guard is neither protecting nor unprotected.
#[test]
fn a_cone_past_the_cap_without_the_guard_is_not_decided() {
    let file = format!("{FIXTURES}/not_decided.rdra");
    let (code, stdout, stderr) = run(&["check", &file, "--guards", "--max-states", "6"]);
    assert_eq!(code, Some(0), "{stderr}");
    let expected = format!(
        "\
guards: 2 guards: 0 protect a rule, 0 no rule depends on, 2 not decided; 2 cones explored again
guard: condition BFirst on FlipA   {file}:31:1
  not decided: 「c moves only after b」 (without the guard its cone stopped at the state cap (6 states))
guard: condition ReadyForC on FlipC   {file}:35:1
  not decided: 「c moves only after b」 (without the guard its cone stopped at the state cap (6 states))
rules:
  「c moves only after b」 depends on no guard, not decided without condition BFirst on FlipA; condition ReadyForC on FlipC
OK: no errors within 1 instance per entity, Int 0..5
"
    );
    assert_eq!(stdout, expected);
    // With room for the 7 states, the rule holds without either guard alone.
    let (_, stdout, _) = run(&["check", &file, "--guards", "--max-states", "7"]);
    assert!(
        stdout.starts_with("guards: 2 guards: 0 protect a rule, 2 no rule depends on;"),
        "{stdout}"
    );
}

/// A model that fails `check` gets no report: a rule that already fails depends on nothing.
#[test]
fn a_failing_model_gets_no_report() {
    let (code, stdout, stderr) = run(&[
        "check",
        "samples/formal-verification/deadlock.rdra",
        "--guards",
    ]);
    assert_eq!(code, Some(1));
    assert!(!stdout.contains("guards:"), "{stdout}");
    assert!(
        stderr.contains("note: --guards reports nothing on a model that fails check"),
        "{stderr}"
    );
}

/// The "Rules" table of samples/oidc-auth/README.md: each labelled rule with the guards
/// whose removal alone fails it. A rule the README protects by something other than a
/// guard (the token sets of the implicit responses, the use case `RejectCodeReplay`, the
/// `links` of `ApproveDevice`) or by a guard and a trigger together (`SessionEnded` and the
/// order of `EvSessionEnded`) depends on no guard here.
#[test]
fn oidc_auth_rules_depend_on_the_guards_its_readme_lists() {
    let (code, stdout, stderr) = run(&["check", "samples/oidc-auth", "--guards"]);
    assert_eq!(code, Some(0), "{stderr}");
    let rules: Vec<&str> = stdout
        .lines()
        .skip_while(|l| *l != "rules:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter(|l| l.contains('「'))
        .map(str::trim)
        .collect();
    let readme = [
        "「An ID Token from the authorization endpoint always answers a nonce」 depends on the .when values of AuthorizationRequest.nonce on RequestAuthentication",
        "「prompt=none never shows the login page」 depends on condition RequestValidated on AuthenticateEndUser",
        "「prompt=login never reuses the session without a login」 depends on condition SessionMayServe on ReuseSession; condition AccountChoiceAsked on SelectAccount",
        "「A poll-mode client is neither pinged nor pushed to」 depends on condition PingMode on SendPingCallback; condition PushMode on PushBackchannelTokens",
        "「A ping-mode client is not pushed the tokens」 depends on condition PushMode on PushBackchannelTokens",
        "「A push-mode client is neither pinged nor polls」 depends on the .when arms of BackchannelRequest.status on PollBackchannelToken; condition PingMode on SendPingCallback",
        "「A public client gets no code without an S256 code challenge」 depends on the .when arms of AuthorizationRequest.status on ValidateRequest",
        "「A request is authorized only after the End-User is authenticated」 depends on condition EndUserAuthenticated on GrantConsent",
        "「A request asking for a second factor is served only by a multi-factor session」 depends on the .when arms of Interaction.authentication on AuthenticateEndUser; the .when arms of Interaction.authentication on ReuseSession; the .when arms of Interaction.authentication on SelectAccount",
        "「A replayed code leaves its grant revoked」 depends on no guard",
        "「A Logout Token is sent only for a session that ended」 depends on no guard",
        "「The Implicit Flow issues no refresh token」 depends on no guard",
        "「An approved device authorization names the End-User who approved it」 depends on no guard",
    ];
    assert_eq!(rules, readme, "{stdout}");
    // Either the guard or the trigger keeps the Logout Token rule, so neither alone is a
    // dependency; the guard is listed as one no rule depends on.
    assert!(
        stdout.contains(
            "guard: condition SessionEnded on SendBackchannelLogout   samples/oidc-auth/buc/buc_logout.rdra:139:1\n  no rule depends on this guard;"
        ),
        "{stdout}"
    );
}
