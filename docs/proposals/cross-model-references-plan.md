# Cross-model references plan

This plan delivers the accepted design in `docs/proposals/cross-model-references.md` as seven stacked PRs.
After the stack lands, a product model in rdra-ish references only what another system's model publishes. That is exposed use cases and events, published entities, and internal entities the owner consents to share.
The tool checks every reference on both sides, and the other model never merges into the product's own results.
The rule the program enforces is the design's Delivery list and its "Decided after review" section, both binding.
The PRs, in order, are XM-1 (one loader), XM-2 (publish), XM-3 (path dependency and references), XM-4 (lock), XM-5 (principals), XM-6 (environment), and XM-7 (git dependencies).

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a terminal transcript, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `skills/poteto-mode/playbooks/autopilot-stack.md` from the installed plugin. Each PR builds on the PR before it. No agent merges. Every PR, XM-1 through XM-7, is the operator's item and stops at merge-ready. The operator lands the stack bottom-up.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

The surface is a CLI, so every live lane saves a terminal transcript as a `.txt` file in place of a screenshot. Appendix C records this limitation.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] On the operator's go, write this exact text into the standing orders and restate it in your todolist. "Plan `docs/proposals/cross-model-references-plan.md`. PRs in order XM-1, XM-2, XM-3, XM-4, XM-5, XM-6, XM-7, each stacked on the one before, XM-1 on `main`. Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. No agent merges. Every PR stops at merge-ready, and the operator lands the stack bottom-up. The program is done when all seven PRs are verified and green at their current heads and handed to the operator."
- [ ] Read these from the installed plugin at program start. Re-read them at every tick.
  - [ ] `skills/poteto-mode/playbooks/autopilot-stack.md`
  - [ ] `skills/poteto-mode/playbooks/autopilot-full.md`
  - [ ] `skills/swarm/SKILL.md`
  - [ ] The built-in `run` skill for a CLI. Live lanes drive `target/release/rdra-ish`, and `target/debug/rdra-ish-lsp` through `/tmp/plan-xsys/lsp_probe.py` where a PR touches the LSP.
  - [ ] `skills/poteto-mode/playbooks/opening-a-pr.md`
  - [ ] `skills/poteto-mode/playbooks/shipping.md`
  - [ ] `skills/poteto-mode/references/bugbot-triage.md`
  - [ ] `skills/deslop/SKILL.md`
  - [ ] `skills/no-comments/SKILL.md`
  - [ ] `skills/benchmark-checklist/SKILL.md`
  - [ ] `skills/how/SKILL.md`
  - [ ] `skills/interrogate/SKILL.md`
  - [ ] `skills/show-me-your-work/SKILL.md`
- [ ] Copy `/tmp/plan-xsys/sweep.sh`, `/tmp/plan-xsys/lsp_probe.py`, and `/tmp/plan-xsys/explore.md` into `~/.claude/orchestrate/cross-model-references/tools/`. Every later reference to `/tmp/plan-xsys/` means this copy if `/tmp` was cleared.
- [ ] Build the trunk binary once per trunk SHA. Run `git worktree add /tmp/xm-trunk origin/main && cargo build --release -p rdra-ish-cli -p rdra-ish-lsp --manifest-path /tmp/xm-trunk/Cargo.toml`. Record the trunk SHA in the trail.
- [ ] Run the trunk sweep once per trunk SHA. Run `/tmp/plan-xsys/sweep.sh /tmp/xm-trunk/target/release/rdra-ish /tmp/xm-sweep-trunk /tmp/how-xsys/explorer1/p2`. Rerun it into `/tmp/xm-sweep-trunk-2` and confirm `diff -r` between the two prints nothing.
- [ ] Arm the hourly audit tick as a real `/loop` in dynamic mode, which schedules its own wake-up rather than blocking on a sleep. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from the installed plugin and the standing objective. Audit the operation against both and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then post a short status message to the operator in chat only when the audit found a tracked change that no earlier status message reported, such as a PR opened, a code-ready head, a round launched or closed, a verdict, a merge, a stuck agent and the action taken, a blocker added or cleared, or a decision only the operator can make. Name every such change and nothing else. Do not repeat a table, the merged list, or an unchanged blocker. If the audit found none, end the turn with no reply text. Either way, log this tick's row in your decision trail. The row names the items reported, or none."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one owner per PR with the full lifecycle the execution playbook names.
- [ ] Follow this dependency graph. Start dependent work only after its parent merges, or base it on the parent branch when the execution playbook stacks.
  - [ ] XM-1 is first. It branches from `main`.
  - [ ] XM-2 after XM-1, on the XM-1 branch.
  - [ ] XM-3 after XM-2, on the XM-2 branch.
  - [ ] XM-4 after XM-3, on the XM-3 branch.
  - [ ] XM-5 after XM-4, on the XM-4 branch.
  - [ ] XM-6 after XM-5, on the XM-5 branch.
  - [ ] XM-7 after XM-6, on the XM-6 branch.
- [ ] Hold the file boundaries. Each PR touches only the paths in its **Files.** block, plus `CHANGELOG.md` and the docs its **Build.** block names. Every PR keeps new fixtures under `tests/fixtures/xsys/` and never adds a root to `samples.txt`.
- [ ] Hold the review gate. No PR changes an interaction, because the stack changes the CLI only. No PR waits for a review in chat.

### PR mechanics, for every PR

- [ ] Resolve the forge once. The forge is GitHub, owner `manji-0`, repo `rdra-ish-dsl`, through the session's GitHub MCP tools. `gh` and Origin are not usable in this session, so record that fallback in the trail. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use the GitHub MCP tool `create_pull_request` with the PR's base branch. A stack child targets its parent branch. XM-1 targets `main`.
- [ ] Run the repo's lint and typecheck once before the PR-facing push. That is `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`. Push with hooks on.
- [ ] Keep every CI gate green on every PR. The gates are `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all -- --skip bfs_and_tlc_reach_the_same_states_and_verdicts`, `bash scripts/check-sample-artifacts.sh`, and `cargo build -p rdra-ish-cli && python3 -I scripts/check-skill-snippets.py target/debug/rdra-ish`. Locally the snippet check reports 3 failures that need `tlc`. Treat only those 3 as expected.
- [ ] Read the TLC verdict from CI only. `tlc` is not installed locally, so the `formal-verify` job is the sole TLC evidence. Link its run in the PR.
- [ ] Run `/deslop` before each commit and `/no-comments` before review.
- [ ] Triage every review-bot and security-reviewer comment per `../references/bugbot-triage.md`.
- [ ] Before the code-ready report and babysit, and again after merge prep, record the base and head SHAs prepared by the topology owner under the execution playbook.

### Verdict and merge, for every PR

- [ ] A full swarm runs once, at the code-ready head. Another full swarm runs only for new behavior, a redesign, or a large new diff. A rebase, a merge-conflict resolution, or a minor fix gets a single verifier at the new head, and a wording-only change gets none. Run each full swarm per `skills/swarm/SKILL.md`. One gates lane. The ten live lanes from the PR's **Verify, live** block. The perf lane from its **Verify, perf** block. Two or more audit lanes, each with its own focus, that read the diff and the receipts and distrust the PR body. The root audits the receipts in the merge-ready report before the verdict.
- [ ] Judge every finding by the execution playbook's blocking rule. The verdict is clean when none blocks. A failure on trunk behavior in a lane marked `Regression lane against trunk.` is recorded against trunk and does not block the PR. A new head gets a fresh verdict, except for results that stay valid under the patch-id rule in `playbooks/shipping.md`.
- [ ] Do not merge. Stop when each PR is verified and green at its current head, then hand the stack to the operator, who lands it bottom-up. After the handoff, nobody pushes without the operator's go. A restack that leaves a PR's `git patch-id` unchanged keeps its verdict, per step 3 of `playbooks/shipping.md`. Any other new head goes back through verification. Record each verdict's head SHA, base SHA, base branch, and patch-id.

### Boot recipe, for every live lane

Each live lane runs in its own worktree at the PR head. Drive through the skill path or exact commands recorded in this boot recipe.

- [ ] `git fetch origin <head-branch> && git checkout <head SHA>`.
- [ ] Run `cargo build --release -p rdra-ish-cli` and, for a lane that uses the LSP, `cargo build -p rdra-ish-lsp`. Ready means `target/release/rdra-ish --help` exits 0. The trunk binary is `/tmp/xm-trunk/target/release/rdra-ish`.
- [ ] Deliver input only through `target/release/rdra-ish <command>`, `target/debug/rdra-ish-lsp` through `python3 -I /tmp/plan-xsys/lsp_probe.py`, and `/tmp/plan-xsys/sweep.sh`. Scratch models live under `/tmp/swarm-<pr-id>/worker-<n>/`, never in the worktree. Read-only diagnostics are `diff -r`, `grep`, `strace -f -qq -e trace=execve`, `unshare -rn`, `sha256sum`, and `python3 -c 'import tomllib'`.
- [ ] Run every command through `script -q -c '<command>' <transcript>` or with `2>&1 | tee`, so the transcript holds the command, stdout, stderr, and `echo exit=$?`. Save every transcript to `/tmp/swarm-<pr-id>/worker-<n>/<slug>.txt` and return the paths with the report.

## Build one project loader for the CLI and the LSP (XM-1)

**Depends on.** None. XM-1 branches from `main`.

**Files.**

- [ ] Create `crates/rdra-ish-core/src/project.rs`.
- [ ] Edit `crates/rdra-ish-core/src/lib.rs`, `crates/rdra-ish-core/Cargo.toml`, and `Cargo.toml` (`toml` in `[workspace.dependencies]`, `walkdir` moved to core).
- [ ] Edit `crates/rdra-ish-cli/src/load.rs`, `crates/rdra-ish-cli/src/main.rs`, `crates/rdra-ish-cli/src/fmt_cmd.rs`, `crates/rdra-ish-cli/src/csv_cmd.rs`, `crates/rdra-ish-cli/src/diagram.rs`, `crates/rdra-ish-cli/src/states.rs`, `crates/rdra-ish-cli/src/next.rs`, and `crates/rdra-ish-cli/src/cli_tests.rs`.
- [ ] Edit `crates/rdra-ish-lsp/src/backend.rs` and `crates/rdra-ish-core/src/workspace.rs`.
- [ ] Edit the test loaders `crates/rdra-ish-cli/tests/engine_equivalence.rs`, `crates/rdra-ish-cli/tests/key_facts.rs`, and `crates/rdra-ish-core/src/lifecycle/tests/cone.rs`.
- [ ] Edit `docs/language-reference.md` (Modules and imports), `docs/cli-reference.md` (`files` and a manifest section), and `CHANGELOG.md`.
- [ ] Create `tests/fixtures/xsys/manifest-above/rdra.toml` and `tests/fixtures/xsys/manifest-above/src/*.rdra`.

**Build.**

- [ ] Move `collect_rdra_files` and `root_include_paths` from `crates/rdra-ish-cli/src/load.rs` into `project.rs`. Keep `eprint_diagnostic` and `reject_model_errors` in the CLI.
- [ ] Add `Manifest` in `project.rs` with `[model] name` and a parsed `[dependencies]` table that no code reads until XM-3.
- [ ] Add `discover` in `project.rs`. It walks up from each input as given, made absolute without resolving symlinks. It stops at the first directory that holds `.git`, inclusive, or at the filesystem root. Document this rule in `docs/cli-reference.md`.
- [ ] Keep today's include rule in `root_include_paths`, the parent and grandparent of the canonical input, so the symlinked `samples/formal-verification` roots load as today.
- [ ] Add `load_project(inputs, overlays) -> Project` with `program`, `model`, `diags`, `include_paths`, and `manifest`. It accepts the LSP's overlays and unifies include paths only. `analyze_workspace` keeps its consistency and lint layering.
- [ ] Route every `load_model` caller in `main.rs`, `csv_cmd.rs`, `diagram.rs`, `states.rs`, `next.rs`, and `cli_tests.rs` through `load_project`. Delete `load_model` once no caller is left. Route `files` and `fmt` through the core collector.
- [ ] Route `crates/rdra-ish-lsp/src/backend.rs::refresh_analysis` through `load_project`. Delete `include_paths_for` and `discover_rdra_files`. The LSP uses the CLI include rule. Any LSP diagnostic count that changes goes in the PR body with the probe output that shows it.
- [ ] Replace the three test copies of the include rule with calls to `load_project`.

**You see.**

- [ ] `diff -r /tmp/xm-sweep-trunk /tmp/xm-sweep-head` prints nothing across 1378 runs.
- [ ] `rdra-ish check tests/fixtures/xsys/manifest-above/src` prints the same output and exit code as the same files copied without `rdra.toml`.
- [ ] `python3 -I /tmp/plan-xsys/lsp_probe.py target/debug/rdra-ish-lsp samples/ec-site samples/ec-site/buc/buc_cart.rdra` prints 9, 9, 6, 0, and 0 diagnostics per file, as on trunk.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-core/src/project.rs` gains cases for a manifest above the inputs, no manifest, a stray manifest above `.git`, and a malformed `rdra.toml`. Run `cargo test -p rdra-ish-core project`.
- [ ] A core test runs one layout with and without a manifest and gets the same include paths and diagnostics through the CLI path and `analyze_workspace`. Run `cargo test -p rdra-ish-core project`.
- [ ] `crates/rdra-ish-core/src/resolver.rs` tests still pass with the moved include rule. Run `cargo test -p rdra-ish-core resolver`.
- [ ] `crates/rdra-ish-cli/tests/files_cli.rs` gains a manifest-above case and keeps the pinned file order. Run `cargo test -p rdra-ish-cli --test files_cli`.
- [ ] `crates/rdra-ish-lsp/src/workspace_test.rs` gains a manifest case. Run `cargo test -p rdra-ish-lsp`.
- [ ] Run every CI gate from **PR mechanics**. `bash scripts/check-sample-artifacts.sh` must leave `git diff` empty.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `/tmp/plan-xsys/sweep.sh target/release/rdra-ish /tmp/xm-sweep-head /tmp/how-xsys/explorer1/p2` at head and compare it with the trunk sweep using `diff -r /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Save `sweep-diff.txt`. Pass when the diff prints nothing.
- [ ] Lane 2. Run the head sweep twice into two directories and `diff -r` them. Save `sweep-repeat.txt`. Pass when the diff prints nothing.
- [ ] Lane 3. Copy `tests/fixtures/xsys/manifest-above` to the scratch dir, then make a second copy without `rdra.toml`. Run `rdra-ish check`, `lint --format json`, and `states` on each copy's `src`. Save `manifest-above.txt`. Pass when both copies give byte-identical output and the same exit codes.
- [ ] Lane 4. Put a `rdra.toml` with `[model] name = "stray"` in the scratch dir, then `git init` a child directory holding a copy of `samples/ec-site`. Run `rdra-ish check` on the copy with and without the stray file. Save `stray-manifest.txt`. Pass when the two outputs are byte-identical.
- [ ] Lane 5. Run `rdra-ish check`, `files`, and `export --kind tla` on `samples/formal-verification/order.rdra` and on `samples/formal-verification-fail/order.rdra` with the trunk and head binaries. Save `symlink-roots.txt`. Pass when trunk and head outputs are byte-identical.
- [ ] Lane 6. Run `python3 -I /tmp/plan-xsys/lsp_probe.py <lsp> samples/ec-site samples/ec-site/buc/buc_cart.rdra` with the trunk and head `rdra-ish-lsp` builds. Save `lsp-ec-site.txt`. Pass when both print 9, 9, 6, 0, and 0 diagnostics per file.
- [ ] Lane 7. Run the LSP probe with the trunk and head builds on `tests/fixtures/purchase` and on `/tmp/how-xsys/explorer1/p2`, opening one `.rdra` file each. Save `lsp-purchase-p2.txt`. Pass when trunk and head print the same files, counts, and first lines, or when every difference appears in the PR body as an intended change.
- [ ] Lane 8. Write a scratch `rdra.toml` with a TOML syntax error, then one with `[model]` and no `name`. Run `rdra-ish check` on each. Save `bad-manifest.txt`. Pass when each run exits 1, stderr names `rdra.toml` with a line number, and no output contains `panicked`.
- [ ] Lane 9. Run `rdra-ish files` and `rdra-ish fmt --check` on the manifest-above copy from lane 3. Save `files-fmt.txt`. Pass when `files` lists the same `.rdra` files in the same order as the copy without a manifest, `rdra.toml` is not listed, and `fmt --check` exits 0.
- [ ] Lane 10. Run `rdra-ish check` on `samples/ec-site` as a relative path, an absolute path, `./samples/ec-site/`, and `samples/ec-site/../ec-site`, with the trunk and head binaries. Save `path-forms.txt`. Pass when each head output equals the trunk output for the same path form.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and of `rdra-ish csv samples/clinic-ops --kind entity` (load only), release builds, at trunk and at head.
- [ ] Probe. Run the interleaved probe in Appendix D with `T=/tmp/xm-trunk/target/release/rdra-ish` and `H=target/release/rdra-ish` from the head worktree root. Both sides produce both numbers.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first. They were 123 ms for `check` and 121 ms for `csv` on this machine at plan time.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on either command. A failure most likely means the files are parsed twice, once to discover the manifest and once to resolve.

**Review gate.** None. XM-1 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-1 to the base-branch stack as its root on `main`, and the operator lands it bottom-up.

## Let an owner publish use cases, events, and entities (XM-2)

**Depends on.** XM-1.

**Files.**

- [ ] Edit `crates/rdra-ish-core/src/predicate.rs`, `crates/rdra-ish-core/src/analysis/predicate_process.rs`, `crates/rdra-ish-core/src/typed_predicate.rs`, `crates/rdra-ish-core/src/model/mod.rs`, and `crates/rdra-ish-core/src/diagnostics.rs`.
- [ ] Create `crates/rdra-ish-core/src/publication.rs` and `crates/rdra-ish-core/src/surface.rs`.
- [ ] Edit `crates/rdra-ish-core/src/workspace.rs`, `crates/rdra-ish-core/src/lint.rs`, `crates/rdra-ish-core/src/tx.rs`, `crates/rdra-ish-core/src/lifecycle/build/mod.rs`, and `crates/rdra-ish-core/src/access.rs`.
- [ ] Edit `crates/rdra-ish-cli/src/cli.rs` and `crates/rdra-ish-cli/src/list_output/mod.rs`.
- [ ] Edit `editors/vscode/syntaxes/rdra.tmLanguage.json` for `exposes` and `shares`.
- [ ] Edit `docs/language-reference.md`, `docs/cli-reference.md`, `docs/state-derivation.md`, and `CHANGELOG.md`.
- [ ] Create `tests/fixtures/xsys/auth-model/` and `tests/fixtures/xsys/owner-mistakes/`. Start from `/tmp/arena-xsys2/candidate-1/probes/proposed/auth-model/` and `/tmp/arena-xsys2/candidate-1/probes/proposed/errors/owner-mistakes.rdra`.

**Build.**

- [ ] Add the `exposes` signature `[["system"], ["usecase","event","entity"]]` and the `shares` signature `[["system"], ["entity"]]` in `predicate_signature`, and both names in `KNOWN_PREDICATES`.
- [ ] Add the chain methods `append`, `upsert`, and `caller(actor | system | any)` for `exposes` in `predicate_chain_methods`. The default caller is `actor`.
- [ ] Add `process_exposes_predicate` and `process_shares_predicate` in `predicate_process.rs`, `TypedPredicate::Exposes` and `TypedPredicate::Shares`, and a publication side table on `SemanticModel`.
- [ ] Add `publication_diagnostics` in `publication.rs`, driven by one table of published-entity uses. It reports `PublishedEntityMutated`, `PublishedEntityReadByOwner`, `PublishedEntityKeysInternal`, `PublishedNotInSourceTx`, `InternalEntityExposed`, `PublishedEntityNeverWritten`, and the `SharedInternalEntity` warning. Call it from `workspace::consistency_diagnostics` and list every name in `lint.rs::is_consistency_diag`.
- [ ] In `tx.rs::infer_usecase_transactions`, put a write to a published entity in the writing use case's group, and skip `SeparateTxInferred` for it.
- [ ] In `lifecycle/build/mod.rs`, skip published entities in the owner's lifecycle.
- [ ] In `access.rs::permission_diagnostics`, stop reporting a use case exposed with `.caller(system)` as having no actor.
- [ ] Add `Surface` and its canonical text in `surface.rs`, sorted by kind and id so declaration order cannot change it. Add `ListKind::Surface`. It prints the canonical text for the default format, and any other `--format` value exits 2 with a message that names the canonical text. Freeze the text format in this PR, because XM-4 pins it.

**You see.**

- [ ] `rdra-ish list tests/fixtures/xsys/auth-model --kind surface` prints the four lines of the design's canonical text example, byte for byte.
- [ ] `rdra-ish export <outbox probe with exposes(...).append()> --kind tla --out /tmp/t` writes one `Login` action in `RdraSpec.tla` where trunk writes `Login_1` and `Login_2`.
- [ ] `rdra-ish check tests/fixtures/xsys/owner-mistakes` exits 1 with one diagnostic per row of the design's owner table.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-core/src/lifecycle/tests/tx_groups.rs` gains a case where a published write joins the use case's group. Run `cargo test -p rdra-ish-core tx_groups`.
- [ ] `crates/rdra-ish-core/src/analysis/integration_tests/` gains one case per publication diagnostic, plus `SharedInternalEntity`. Run `cargo test -p rdra-ish-core integration_tests`.
- [ ] `crates/rdra-ish-core/src/surface.rs` gains a case that two declaration orders give the same text. Run `cargo test -p rdra-ish-core surface`.
- [ ] `crates/rdra-ish-cli/tests/check_cli.rs` extends `check_warns_about_transactions_apis_and_system_boundaries` so a published write gives no `SeparateTxInferred`. Run `cargo test -p rdra-ish-cli --test check_cli`.
- [ ] `crates/rdra-ish-emit/src/tla/tests.rs` gains a one-action snapshot. Run `INSTA_UPDATE=always cargo test -p rdra-ish-emit tla` once, review the `.snap` diff, then run `cargo test -p rdra-ish-emit tla`.
- [ ] `crates/rdra-ish-cli/tests/engine_equivalence.rs` gains an outbox fixture through `check_fixture`. Run `cargo test -p rdra-ish-cli --test engine_equivalence`. The TLC half runs in CI `formal-verify` only.
- [ ] `crates/rdra-ish-lsp/src/completion.rs` tests show `exposes` and `shares`. Run `cargo test -p rdra-ish-lsp completion`.
- [ ] Run every CI gate from **PR mechanics**.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep and `diff -r /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Trunk lacks `exposes`, so also run `rdra-ish export /tmp/arena-xsys2/candidate-1/probes/today/outbox --kind tla --out <dir>` with the trunk and head binaries on the unchanged probe. Save `sweep-and-outbox.txt`. Pass when the sweep diff prints nothing and the two `RdraSpec.tla` files are byte-identical.
- [ ] Lane 2. Run `rdra-ish check tests/fixtures/xsys/auth-model` and `rdra-ish states tests/fixtures/xsys/auth-model`. Save `auth-check-states.txt`. Pass when `check` exits 0 with no publication diagnostic and `states` lists `Session` and does not list `SessionView`.
- [ ] Lane 3. Run `rdra-ish list tests/fixtures/xsys/auth-model --kind surface` and diff it against the canonical text block in the design's Usage section. Save `surface-text.txt`. Pass when the diff prints nothing.
- [ ] Lane 4. Copy the auth fixture, reverse the order of its declarations and predicates, and run `list --kind surface` on both copies twice. Save `surface-stable.txt`. Pass when all four outputs are byte-identical.
- [ ] Lane 5. Run `rdra-ish check tests/fixtures/xsys/owner-mistakes` and `rdra-ish lint tests/fixtures/xsys/owner-mistakes --format json`. Save `owner-mistakes.txt`. Pass when `check` exits 1 with exactly one diagnostic for each of the six rows of the design's owner table, and `lint` files none of them as `semantic-warning`.
- [ ] Lane 6. Copy the outbox probe, add `exposes(..., LoginRecord).append()`, and run `export --kind tla` at head. Save `outbox-one-action.txt`. Pass when `RdraSpec.tla` has one `Login` action, no `Login_1` or `Login_2`, and one `Next` disjunct for `Login`.
- [ ] Lane 7. Copy the auth fixture, add `shares(AuthSystem, Session)`, and run `check` and `lint --format json`. Save `shares-warning.txt`. Pass when `check` exits 0 with one `SharedInternalEntity` warning and `lint` reports it as a consistency diagnostic.
- [ ] Lane 8. Copy the auth fixture, add a use case exposed with `.caller(system)` and no actor, and a second exposed use case with no chain and no actor. Run `check`. Save `caller-system.txt`. Pass when only the second use case gets the no-actor diagnostic.
- [ ] Lane 9. On copies of the auth fixture, add `updates` on an append entity, then `deletes` on the upsert entity, then `transitions` on a published column, then `updates` on the upsert entity. Run `check` on each. Save `published-writes.txt`. Pass when the first three each give one `PublishedEntityMutated` error and the fourth exits 0.
- [ ] Lane 10. Run the LSP probe on `tests/fixtures/xsys/owner-mistakes`, opening its `.rdra` file. Save `lsp-owner-mistakes.txt`. Pass when the LSP publishes the same diagnostic messages, line for line, as CLI `check` on the same file.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Trunk lacks publication, so also measure min-of-10 of `rdra-ish check tests/fixtures/xsys/auth-model` at head, the end state the owner waits for.
- [ ] Probe. Run the interleaved probe in Appendix D. Then run the auth fixture `check` 10 times at head with the same `date +%s%N` loop.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on either clinic-ops command. Fail when the auth fixture `check` minimum exceeds 50 ms.

**Review gate.** None. XM-2 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-2 to the base-branch stack on the XM-1 branch, and the operator lands it bottom-up.

## Bind a path dependency and check every reference (XM-3)

**Depends on.** XM-2.

**Files.**

- [ ] Edit `crates/rdra-ish-syntax/src/token.rs`, `crates/rdra-ish-syntax/src/parser.rs`, `crates/rdra-ish-syntax/src/ast.rs`, `crates/rdra-ish-syntax/src/format.rs`, and `crates/rdra-ish-syntax/src/snapshots/`.
- [ ] Edit `editors/vscode/syntaxes/rdra.tmLanguage.json` for `from`.
- [ ] Edit `crates/rdra-ish-core/src/project.rs`, `crates/rdra-ish-core/src/model/mod.rs`, `crates/rdra-ish-core/src/model/instances.rs`, and every caller of `len()` or `is_empty()` on `use_cases`, `events`, `entities`, and `permissions`.
- [ ] Create `crates/rdra-ish-core/src/binding.rs`.
- [ ] Edit `crates/rdra-ish-core/src/analysis/arg_resolve.rs`, `crates/rdra-ish-core/src/analysis/predicate_process.rs`, `crates/rdra-ish-core/src/predicate.rs`, `crates/rdra-ish-core/src/typed_predicate.rs`, `crates/rdra-ish-core/src/diagnostics.rs`, and `crates/rdra-ish-core/src/lint.rs`.
- [ ] Edit `crates/rdra-ish-core/src/resolver.rs`, `crates/rdra-ish-core/src/import_scope.rs`, `crates/rdra-ish-core/src/system.rs`, and `crates/rdra-ish-core/src/lifecycle/build/`.
- [ ] Edit `crates/rdra-ish-emit/src/tla/names.rs`, `crates/rdra-ish-core/src/lifecycle/build/naming.rs`, the context, event, usecase, and rdra diagram emitters, and the AsyncAPI emitter.
- [ ] Edit `crates/rdra-ish-cli/src/cli.rs`, `crates/rdra-ish-cli/src/list_output/`, and `crates/rdra-ish-cli/src/main.rs` (the `check` arm).
- [ ] Edit `crates/rdra-ish-lsp/src/rename.rs` and `crates/rdra-ish-lsp/src/refs.rs`.
- [ ] Create `crates/rdra-ish-cli/tests/cross_model.rs`.
- [ ] Create `tests/fixtures/xsys/shop-model/`, `tests/fixtures/xsys/payments-model/model/`, and `tests/fixtures/xsys/product-mistakes/`. Start from `/tmp/arena-xsys2/candidate-1/probes/proposed/shop-model/`, `.../payments-model/`, and `.../errors/product-mistakes.rdra`.
- [ ] Edit `docs/rdra-ish-interpretation.md` (Timers and external systems), `docs/language-reference.md`, `docs/cli-reference.md`, and `CHANGELOG.md`.

**Build.**

- [ ] Parse `extsystem X "..." from dep.System` in `parser.rs::instance_decl` into a `binding` field on `InstanceDecl`. Parse `usecase::Auth.Login` in `qref` and `Auth.SessionView.status` in `operand`. Round-trip all three in `format.rs`.
- [ ] Load `[dependencies]` path entries in `project.rs`. Check each dependency as its own model and fail closed on its errors. Detect cycles. Check transitive dependencies, and refuse to bind them.
- [ ] Add `Partitioned<K, V>` in `model/mod.rs` and use it for `use_cases`, `events`, `entities`, and `permissions`. Its `iter`, `values`, `keys`, and `len` cover local elements only, and `get` takes any key. Add `origin(&NodeRef)`. Review every `len()` and `is_empty()` caller on the four maps and record each decision in the PR body.
- [ ] Add `binding.rs`. In `arg_resolve.rs::resolve_qref`, resolve `Ext.X` against the bound surface before the alias lookup. Add `FOREIGN_OPERANDS`, one table of each predicate position that accepts a foreign operand, read by `validate_predicate_arg_types`.
- [ ] Add `UnknownForeignName` with the closest published names, `NotExposed` with the read or react hint, `ForeignWrite` with the exposed use case that makes the move, `ForeignContained`, `ForeignImport` in `resolver.rs::resolve_import_path` and `import_scope.rs::build_import_scopes`, `IdShadowsForeignSlug`, and the `DirectEntityReference` warning listed in `lint.rs::is_consistency_diag`.
- [ ] Widen `invokes` in `predicate.rs` and `typed_predicate.rs` to take a foreign use case. Keep `tx.rs` grouping on `api` targets only.
- [ ] Replace the `len() != 1` and `> 1` tests in `system.rs` with `SystemMembership { None, One, Many }`.
- [ ] Make stubs leaves in `resolver.rs::reachable_from_buc`.
- [ ] Add a lifecycle diagnostic in `lifecycle/build/` that refuses a guard on foreign data. XM-6 removes it.
- [ ] Emit `Auth__X` for foreign ids in `tla/names.rs` and `lifecycle/build/naming.rs`.
- [ ] Render opt-in references. The context, event, usecase, and rdra diagrams show the bound external system and its referenced elements as stubs. AsyncAPI lists foreign events as receive-only. `next` treats a foreign touchpoint as a satisfied boundary. Every other output excludes foreign elements.
- [ ] Add `ListKind::Dependency`, one row per foreign reference with its use, the local element, and the file and line.
- [ ] Print the environment once in the `check` arm of `main.rs`, before the `bounds` line, only when the manifest has dependencies.
- [ ] Stop LSP rename and goto in `rename.rs` and `refs.rs` from editing a dependency's files.
- [ ] Add the RDRA deviation note for `invokes` on an external system bound with `from` to `docs/rdra-ish-interpretation.md`.
- [ ] Write `crates/rdra-ish-cli/tests/cross_model.rs`. It runs every `diagram`, `export`, `csv`, and `list` kind, enumerated from `ValueEnum::value_variants()`, on the shop fixture, and fails when a sentinel id such as `Session`, `/sessions`, `Revoke`, or `BucSignIn` appears outside the allowed stubs.

**You see.**

- [ ] `rdra-ish check tests/fixtures/xsys/shop-model` prints `Auth (auth, path ../auth-model)` once, exits 0, and names no auth internal.
- [ ] `rdra-ish check tests/fixtures/xsys/product-mistakes` exits 1 with one diagnostic for each mistakes-table row this PR owns.
- [ ] `cargo test -p rdra-ish-cli --test cross_model` passes over every output kind.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-cli/tests/cross_model.rs` holds the leak test and one case per mistakes-table row this PR owns. Run `cargo test -p rdra-ish-cli --test cross_model`.
- [ ] `crates/rdra-ish-syntax/src/parser.rs` tests and snapshots gain the three forms. `crates/rdra-ish-cli/tests/fmt_cli.rs` gains their round trip. Run `INSTA_UPDATE=always cargo test -p rdra-ish-syntax` once, review the `.snap` diff, then run `cargo test -p rdra-ish-syntax && cargo test -p rdra-ish-cli --test fmt_cli`.
- [ ] `crates/rdra-ish-core/src/resolver.rs` and `import_scope.rs` tests gain `ForeignImport`. Run `cargo test -p rdra-ish-core resolver import_scope`.
- [ ] `crates/rdra-ish-core/src/system.rs` tests gain membership `None`. Run `cargo test -p rdra-ish-core system`.
- [ ] `crates/rdra-ish-core/src/project.rs` tests gain a missing path, a cycle, a dependency with errors, and a transitive bind. Run `cargo test -p rdra-ish-core project`.
- [ ] `crates/rdra-ish-lsp/src/workspace_test.rs` gains a product with a path dependency that gets the CLI's diagnostics. Run `cargo test -p rdra-ish-lsp`.
- [ ] Run every CI gate from **PR mechanics**. Keep cross-model examples out of `skills/`, or mark a fence with `<!-- expect-error: ... -->`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep with the extra root `/tmp/how-xsys/explorer1/p2` and `diff -r /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Save `sweep-diff.txt`. Pass when the diff prints nothing, which shows that a model with no manifest keeps today's import behavior.
- [ ] Lane 2. Run `rdra-ish check tests/fixtures/xsys/shop-model`. Save `shop-check.txt`. Pass when it exits 0, prints the environment line once before `bounds`, and no line names `Session`, `Revoke`, or `BucSignIn`.
- [ ] Lane 3. Run `rdra-ish check tests/fixtures/xsys/product-mistakes`. Save `product-mistakes.txt`. Pass when it exits 1 with one diagnostic each for `Auth.Logon`, `Auth.Session`, the `updates` on `Auth.SessionView`, the `contains` of `Auth.Login`, the cross-model `import`, the shared-entity `reads`, and the foreign guard refusal, each at its source line with the hint the design's table names.
- [ ] Lane 4. Run every `list`, `csv`, `export`, and `diagram` kind from `rdra-ish <command> --help` on `tests/fixtures/xsys/shop-model`, then `grep -rlE 'Session\b|/sessions|Revoke|BucSignIn'` over the outputs. Save `leak-scan.txt`. Pass when every hit is a stub in the context, event, usecase, or rdra diagram or a receive-only AsyncAPI event, and no other file has a hit.
- [ ] Lane 5. Run `rdra-ish list tests/fixtures/xsys/shop-model --kind dependency --format csv`. Save `list-dependency.txt`. Pass when there is one row per foreign reference in the shop sources, and each row names its use, its local element, and a file and line that exist.
- [ ] Lane 6. Copy `/tmp/how-xsys/explorer1/p2` and add an `rdra.toml` to the product and to auth with auth as a path dependency. Run `check`. Then replace the `import` with `extsystem Auth "Auth" from auth.AuthSystem` and run `check` again. Save `p2-manifest.txt`. Pass when the first run reports `ForeignImport` with the `extsystem ... from` hint and the second reports `ForeignWrite` on the `transitions` line.
- [ ] Lane 7. Run `rdra-ish fmt` twice on a scratch copy of `tests/fixtures/xsys/shop-model`, then `fmt --check`. Save `fmt-roundtrip.txt`. Pass when the second `fmt` changes no file, `fmt --check` exits 0, and the formatted file still holds `from auth.AuthSystem`, `usecase::Auth.Login`, and `Auth.SessionView.status`.
- [ ] Lane 8. On shop copies, point the auth path at a missing directory, add a cycle from auth back to shop, add an error to auth, and bind a transitive dependency. Run `check` on each. Save `dep-failures.txt`. Pass when each run exits 1 with one diagnostic that names the dependency and the cause, and no run panics.
- [ ] Lane 9. Run the LSP probe on `tests/fixtures/xsys/shop-model`, opening the checkout file. Save `lsp-shop.txt`. Pass when the LSP publishes the same diagnostics as CLI `check` for the shop files and publishes nothing for any file under `auth-model`.
- [ ] Lane 10. Run `rdra-ish export tests/fixtures/xsys/shop-model --kind tla --out <dir>` and `rdra-ish next tests/fixtures/xsys/shop-model`. Save `tla-next.txt`. Pass when `RdraSpec.tla` holds no `Auth.` and every foreign id appears as `Auth__X`, and `next` reports no missing boundary for the `invokes` of `Auth.Login`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Trunk cannot load two models, so also measure min-of-10 of `rdra-ish check tests/fixtures/xsys/shop-model` at head, the end state the product author waits for.
- [ ] Probe. Run the interleaved probe in Appendix D. Then run the shop fixture `check` 10 times at head with the same loop.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on either clinic-ops command. This PR has the largest risk, because `Partitioned::iter` filters on every hot loop. Fail when the shop fixture `check` minimum exceeds 60 ms.

**Review gate.** None. XM-3 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-3 to the base-branch stack on the XM-2 branch, and the operator lands it bottom-up.

## Lock each dependency's published surface (XM-4)

**Depends on.** XM-3.

**Files.**

- [ ] Edit `crates/rdra-ish-core/src/project.rs`, `crates/rdra-ish-core/src/surface.rs`, `crates/rdra-ish-core/src/diagnostics.rs`, `crates/rdra-ish-core/Cargo.toml`, and `Cargo.toml` (`sha2` in `[workspace.dependencies]`).
- [ ] Edit `crates/rdra-ish-cli/src/cli.rs` and `crates/rdra-ish-cli/src/main.rs`.
- [ ] Create `crates/rdra-ish-cli/src/deps.rs` and `crates/rdra-ish-cli/tests/deps_cli.rs`.
- [ ] Create `tests/fixtures/xsys/shop-model/rdra.lock`.
- [ ] Edit `scripts/check-skill-snippets.py`.
- [ ] Edit `docs/cli-reference.md` (a `## deps` section and the lock format) and `CHANGELOG.md`.

**Build.**

- [ ] Add `Lockfile` in `project.rs`. Each dependency records its source, its commit (empty for a path dependency), its canonical surface text, and the sha256 of that text. `rdra.lock` is TOML.
- [ ] Read and validate the lock in `load_project`. `check` and the LSP read each surface from the lock and need no network.
- [ ] Add `SurfaceChanged` as an error. Its message prints the diff of the locked against the current surface text and marks the lines the product uses. Any change fails, per item 6 of "Decided after review".
- [ ] Add `LockOutOfDate` as an error when `rdra.toml` and `rdra.lock` name different dependencies or sources.
- [ ] Add `LockTampered` as an error when a recorded sha256 does not match the recorded surface text. This is the meaning for a path dependency too, which has no commit.
- [ ] Hash the canonical text, not the file bytes, so CRLF checkouts give the same sha256.
- [ ] Add `Commands::Deps` with `DepsCommand::Lock` in `cli.rs`, dispatched to `deps.rs`. `deps lock` refuses and leaves `rdra.lock` unchanged when an owner model has errors. Two `deps lock` runs give a byte-identical file.
- [ ] Add `deps` to `SUBCOMMANDS` in `scripts/check-skill-snippets.py` and make its flag check read `rdra-ish deps lock --help`.

**You see.**

- [ ] `rdra-ish deps lock` in a shop copy writes `rdra.lock`, and `rdra-ish check` then exits 0.
- [ ] After a change to the Login label in the auth copy, `rdra-ish check` exits 1 and prints a diff with the `Login` line marked as used.
- [ ] `strace -f -qq -e trace=execve rdra-ish check <shop copy>` logs one `execve`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-cli/tests/deps_cli.rs` copies the shop and auth fixtures to a tempdir, as `export_gate.rs::scratch` does. It covers `deps lock`, a clean `check`, `SurfaceChanged` with the used line marked, a relock, `LockTampered`, and `LockOutOfDate`. Run `cargo test -p rdra-ish-cli --test deps_cli`.
- [ ] `crates/rdra-ish-core/src/project.rs` tests gain a CRLF case and a lock round trip. Run `cargo test -p rdra-ish-core project`.
- [ ] `crates/rdra-ish-cli/src/cli.rs` `mod tests` gains a parse case for `deps lock`. Run `cargo test -p rdra-ish-cli cli`.
- [ ] Run every CI gate from **PR mechanics**. Confirm `python3 -c 'import tomllib; tomllib.load(open("tests/fixtures/xsys/shop-model/rdra.lock","rb"))'` exits 0, because the pre-commit `check-toml` hook parses it.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep and `diff -r /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Trunk has no lock, so also run `rdra-ish deps lock` and `rdra-ish check` on a shop copy at head. Save `sweep-and-lock.txt`. Pass when the sweep diff prints nothing and the locked shop copy's `check` exits 0.
- [ ] Lane 2. Delete `rdra.lock` in a shop copy, run `rdra-ish deps lock`, parse the result with `tomllib`, and run `check`. Save `lock-write.txt`. Pass when `rdra.lock` parses, holds one entry per dependency with a surface and a sha256, and `check` exits 0.
- [ ] Lane 3. In the locked copy, change the label of `Login` in the auth copy and run `check`. Save `surface-changed-used.txt`. Pass when `check` exits 1 with `SurfaceChanged`, the diff shows the old and new `Login` line, and that line is marked as used.
- [ ] Lane 4. In a fresh locked copy, add an exposed use case that the shop does not reference and run `check`. Save `surface-changed-unused.txt`. Pass when `check` exits 1 with `SurfaceChanged` and the new line is not marked as used.
- [ ] Lane 5. Run `deps lock` twice and compare with `sha256sum`. Then convert every auth source to CRLF with `sed -i 's/$/\r/'` and run `check`. Save `lock-idempotent-crlf.txt`. Pass when the two lock files match and the CRLF `check` exits 0.
- [ ] Lane 6. Change one hex digit of a sha256 in `rdra.lock` and run `check`. Restore it, change one word of the locked surface text, and run `check`. Save `lock-tampered.txt`. Pass when both runs exit 1 with `LockTampered`.
- [ ] Lane 7. Add a dependency to `rdra.toml` without relocking and run `check`. Restore it, remove a dependency, and run `check`. Save `lock-out-of-date.txt`. Pass when both runs exit 1 with `LockOutOfDate`.
- [ ] Lane 8. Run `strace -f -qq -e trace=execve -o <log> rdra-ish check <locked copy>` and `unshare -rn rdra-ish check <locked copy>`. Save `check-offline.txt`. Pass when the log holds one line and the `unshare` run exits 0.
- [ ] Lane 9. Add an error to the auth copy, record `sha256sum rdra.lock`, and run `deps lock`. Save `lock-refuses.txt`. Pass when `deps lock` exits non-zero, names the auth error, and `rdra.lock` has the same sha256 as before.
- [ ] Lane 10. In the copy from lane 3, run the LSP probe, opening the shop checkout file. Save `lsp-surface-changed.txt`. Pass when the LSP publishes a `SurfaceChanged` diagnostic, as CLI `check` does.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Trunk has no lock, so also measure min-of-10 of `rdra-ish check` on the locked shop copy at head.
- [ ] Probe. Run the interleaved probe in Appendix D. Then run the locked shop `check` 10 times at head with the same loop.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on either clinic-ops command. Fail when the locked shop `check` minimum exceeds 60 ms.

**Review gate.** None. XM-4 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-4 to the base-branch stack on the XM-3 branch, and the operator lands it bottom-up.

## Check the principal of every foreign call (XM-5)

**Depends on.** XM-4.

**Files.**

- [ ] Edit `crates/rdra-ish-core/src/predicate.rs`, `crates/rdra-ish-core/src/model/refs.rs`, `crates/rdra-ish-core/src/access.rs`, `crates/rdra-ish-core/src/diagnostics.rs`, and `crates/rdra-ish-core/src/analysis/predicate_process.rs`.
- [ ] Edit `crates/rdra-ish-cli/src/main.rs` (`is_strict_access_gap`), `crates/rdra-ish-cli/src/list_output/access.rs`, `crates/rdra-ish-emit/src/csv.rs`, and `crates/rdra-ish-lsp/src/code_actions.rs`.
- [ ] Edit `tests/fixtures/xsys/shop-model/` and `tests/fixtures/xsys/product-mistakes/`.
- [ ] Edit `samples/clinic-ops/out/actor_permission_audit.csv`, `samples/incremental-order/step-{3,4,5,6}-*/out/actor_permission_audit.csv`, and `samples/incremental-order/step-3-interaction-boundary/design.md`, regenerated by `bash scripts/check-sample-artifacts.sh`.
- [ ] Edit `docs/language-reference.md` (Access constraints), `docs/cli-reference.md`, `skills/rdra-ish-buc-update/SKILL.md`, and `CHANGELOG.md`.

**Build.**

- [ ] Let `has_permission` take a `system` holder when the permission is foreign, through `FOREIGN_OPERANDS` or a pair table. A local permission on a `system` stays `TypeMismatch`.
- [ ] Add the `as_service` chain method to `invokes` in `predicate_chain_methods`. Make `unknown_chain_method_hint` point `.by(...)` on `invokes` to `.as_service`.
- [ ] Put the principal on the invokes relation in `refs.rs`, as `Delegated` or `Service(SystemKey)`.
- [ ] Extend `access.rs::collect_actor_permissions` to system principals. A delegated call adds the foreign use case's `requires` to the calling use case's actors.
- [ ] Add `CallerKindRejected` when the principal kind does not match the owner's `.caller(...)`. Add a missing-permission diagnostic that names the call and its principal. List the call gap in `main.rs::is_strict_access_gap`.
- [ ] Add a principal-kind column and an origin column to `ActorPermissionAudit` and to both renderers. The columns are always present, so every model has one CSV schema. Regenerate the committed sample artifacts.
- [ ] Add the `CapturePaymentForOrder` call with `.as_service(ShopBackend)` and both grants to the shop fixture. Add the caller-kind and missing-permission rows to `product-mistakes`.

**You see.**

- [ ] `rdra-ish check tests/fixtures/xsys/shop-model` exits 0 with the service call in place.
- [ ] `rdra-ish csv tests/fixtures/xsys/shop-model --kind actor-permission-audit --out /tmp/a.csv` has a `ShopBackend` row with principal kind `system` and origin `payments`.
- [ ] Without `has_permission(ShopBackend, Pay.CapturePayments)`, `check` reports exactly one missing-permission error, naming `CapturePaymentForOrder` and `ShopBackend`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-core/src/access.rs` tests gain delegated, service, rejected-kind, and missing-grant cases. Run `cargo test -p rdra-ish-core access`.
- [ ] `crates/rdra-ish-emit/src/csv.rs` tests gain the two columns. Run `cargo test -p rdra-ish-emit csv`.
- [ ] `crates/rdra-ish-cli/src/cli_tests.rs` gains a `--strict` case on a missing call grant. Run `cargo test -p rdra-ish-cli cli_tests`.
- [ ] `crates/rdra-ish-cli/tests/cross_model.rs` gains the removed-grant and missing-`.as_service` cases. Run `cargo test -p rdra-ish-cli --test cross_model`.
- [ ] Run every CI gate from **PR mechanics**. `bash scripts/check-sample-artifacts.sh` must pass with the regenerated artifacts committed.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep and `diff -rq /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Save `sweep-diff.txt`. Pass when every differing path ends in `csv-actor-permission-audit` or `list-actor-permission-audit`, and each of those diffs adds exactly the two columns.
- [ ] Lane 2. Run `rdra-ish check tests/fixtures/xsys/shop-model`. Save `shop-service-call.txt`. Pass when it exits 0 with no access diagnostic.
- [ ] Lane 3. In a shop copy, delete `has_permission(ShopBackend, Pay.CapturePayments)` and run `check`. Save `missing-service-grant.txt`. Pass when there is exactly one missing-permission error, at the `invokes` line, naming `CapturePaymentForOrder`, `Pay.CapturePayment`, and `ShopBackend`.
- [ ] Lane 4. In a shop copy, delete `.as_service(ShopBackend)` and run `check`. Save `missing-as-service.txt`. Pass when `check` exits 1 with `CallerKindRejected` naming the `caller(system)` requirement.
- [ ] Lane 5. In a shop copy, add `.as_service(ShopBackend)` to the `invokes` of `Auth.Login` and run `check`. Save `service-on-actor-call.txt`. Pass when `check` exits 1 with `CallerKindRejected` naming `caller(actor)`.
- [ ] Lane 6. In a shop copy, delete `has_permission(Shopper, Auth.CanLogin)` and run `check`. Save `missing-actor-grant.txt`. Pass when there is exactly one missing-permission error, naming `SignInAtCheckout` and `Shopper`.
- [ ] Lane 7. Run `rdra-ish csv tests/fixtures/xsys/shop-model --kind actor-permission-audit --out <file>` and `rdra-ish list tests/fixtures/xsys/shop-model --kind actor-permission-audit`. Save `audit-columns.txt`. Pass when both show principal kind `system` and origin `payments` for `ShopBackend`, and principal kind `actor` for `Shopper`.
- [ ] Lane 8. Run `/tmp/arena-xsys2/candidate-1/probes/today/service` through `rdra-ish check` with the trunk and head binaries. Save `local-permission-system.txt`. Pass when both report `TypeMismatch` for `has_permission` on a `system` with a local permission.
- [ ] Lane 9. In a shop copy, write `.by(ShopBackend)` on the payment call and run `check`. Then run `check --strict` on the copy from lane 3. Save `hint-and-strict.txt`. Pass when the first run reports `UnknownChainMethod` with a hint that names `.as_service`, and the strict run exits 1.
- [ ] Lane 10. Run the LSP probe on the copy from lane 3, opening the shop checkout file. Save `lsp-missing-grant.txt`. Pass when the LSP publishes the same missing-permission diagnostic at the same line as CLI `check`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Also measure min-of-10 of `rdra-ish csv samples/clinic-ops --kind actor-permission-audit`, which both trunk and head produce.
- [ ] Probe. Run the interleaved probe in Appendix D, then the same interleaved loop for the audit command.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on any of the three commands.

**Review gate.** None. XM-5 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-5 to the base-branch stack on the XM-4 branch, and the operator lands it bottom-up.

## Model the other system as the environment (XM-6)

**Depends on.** XM-5.

**Files.**

- [ ] Edit `crates/rdra-ish-core/src/lifecycle/types.rs`, `crates/rdra-ish-core/src/lifecycle/build/mod.rs`, `crates/rdra-ish-core/src/lifecycle/build/preds.rs`, `crates/rdra-ish-core/src/lifecycle/findings.rs`, `crates/rdra-ish-core/src/lifecycle/diag.rs`, `crates/rdra-ish-core/src/lifecycle/targets.rs`, `crates/rdra-ish-core/src/lifecycle/view.rs`, `crates/rdra-ish-core/src/lifecycle/select.rs`, and `crates/rdra-ish-core/src/lifecycle/restrict.rs`.
- [ ] Edit `crates/rdra-ish-core/src/refinement/findings.rs`, `crates/rdra-ish-core/src/analysis/fk.rs`, and `crates/rdra-ish-core/src/analysis/predicate_process.rs`.
- [ ] Edit `crates/rdra-ish-emit/src/tla/module.rs`, `crates/rdra-ish-emit/src/tla/mod.rs`, `crates/rdra-ish-emit/src/state_pattern.rs`, `crates/rdra-ish-emit/src/dbml.rs`, and `crates/rdra-ish-cli/src/verify.rs`.
- [ ] Edit `crates/rdra-ish-cli/src/list_output/state_findings.rs`.
- [ ] Edit `tests/fixtures/xsys/shop-model/` and `tests/fixtures/xsys/product-mistakes/`.
- [ ] Edit `docs/state-derivation.md`, `docs/formal-verification.md`, `docs/cli-reference.md`, and `CHANGELOG.md`.

**Build.**

- [ ] Add `ActionOrigin::Environment` with `Owner` and `Pace`, and `is_fair`. Match it exhaustively at every user, including the `[environment]` trace marker in `state_findings.rs`.
- [ ] In `lifecycle/build/mod.rs`, add one environment action with no fairness for each owner writer, hidden or exposed, of each published entity the product reads.
- [ ] Pace a product use case that only a foreign event starts at the environment's pace, by reading `triggers` from events of external systems bound with `from`. Unbound external systems keep today's results, per item 4 of "Decided after review".
- [ ] Treat every value of a shared column as reachable at any time in the product's lifecycle, per item 1 of "Decided after review".
- [ ] Run the stuck check again on fair edges only, in `findings.rs`, and report `StateDiag::StuckBehindEnvironment` when an instance can leave a state over all steps but not over product steps alone. Skip the second run when the model has no environment action.
- [ ] Add `StateDiag::UnreachableForeignValue` for a guard on a published value that no owner writer sets. Report no finding on a foreign subject.
- [ ] Emit `Next == ProductNext \/ EnvNext` and `WF_vars(ProductNext)` in `tla/module.rs` when an environment action exists. Keep `needs_fairness` as the condition, and make `Spec`, `verify.rs`, and the cfg agree. Models with no environment action keep byte-identical TLA+.
- [ ] Keep environment actions under `--buc` and `--entity` in `select.rs` and `restrict.rs`.
- [ ] Allow `relate(Order, Auth.SessionView, N:1)` in `fk.rs` and `process_relate_predicate`. Reject a key to a shared entity. Add the DBML note `external: auth.SessionView.session_id` in `dbml.rs`.
- [ ] Remove the XM-3 refusal of foreign guards.
- [ ] Add `relate(Order, Auth.SessionView, N:1).optional()`, the `SignedIn` condition with its `holds` guard, and `transitions(Order.status, MarkPaid, placed -> paid)` to the shop fixture. Add the `UnreachableForeignValue` row to `product-mistakes`.

**You see.**

- [ ] `rdra-ish check tests/fixtures/xsys/shop-model` prints `warning[StuckBehindEnvironment_Order_status_placed]` and exits 0.
- [ ] `grep -n 'EnvNext\|WF_vars' <out>/RdraSpec.tla` after `rdra-ish export tests/fixtures/xsys/shop-model --kind tla --out <out>` shows `Next == ProductNext \/ EnvNext` and `WF_vars(ProductNext)`.
- [ ] `rdra-ish states tests/fixtures/xsys/shop-model` shows an `Environment:` block.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-core/src/lifecycle/tests/deadlock.rs` and `never_runs.rs` gain environment cases. Run `cargo test -p rdra-ish-core lifecycle`.
- [ ] `crates/rdra-ish-emit/src/tla/tests.rs` gains a split snapshot, and every existing `tla/snapshots/*.snap` stays unchanged. Run `cargo test -p rdra-ish-emit tla`.
- [ ] `crates/rdra-ish-cli/tests/check_cli.rs` gains a stuck-behind-environment case modeled on `check_reports_an_instance_that_never_leaves_its_state`. Run `cargo test -p rdra-ish-cli --test check_cli`.
- [ ] `crates/rdra-ish-cli/tests/engine_equivalence.rs` gains `bfs_and_tlc_agree_on_environment` through `check_fixture`. Run `cargo test -p rdra-ish-cli --test engine_equivalence`. TLC runs in CI `formal-verify` with `RDRA_REQUIRE_TLC=1`, and the PR links that run.
- [ ] Run every CI gate from **PR mechanics**.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep and `diff -rq /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Save `sweep-diff.txt`. Pass when every differing path ends in `csv-actor-permission-audit` or `list-actor-permission-audit`, the XM-5 change, which shows that clinic-ops' unbound `raises` keep their results.
- [ ] Lane 2. Run `rdra-ish check tests/fixtures/xsys/shop-model`. Save `shop-stuck.txt`. Pass when it exits 0 with `warning[StuckBehindEnvironment_Order_status_placed]` and no `Deadlock` on `Order.status`.
- [ ] Lane 3. Run `rdra-ish export tests/fixtures/xsys/shop-model --kind tla --out <dir>` and grep `RdraSpec.tla`. Save `tla-split.txt`. Pass when it has `Next == ProductNext \/ EnvNext` and `WF_vars(ProductNext)`, and no `WF_vars(Next)`.
- [ ] Lane 4. Run `rdra-ish states tests/fixtures/xsys/shop-model`. Save `states-environment.txt`. Pass when the `Environment:` block lists the owner writers of `SessionView`, both `Login` and the unexposed writer, as environment actions.
- [ ] Lane 5. In a shop copy, change the guard to `holds(SignedIn, Auth.SessionView.status == expired)` and run `check`. Save `unreachable-foreign.txt`. Pass when `check` reports `UnreachableForeignValue` at that line.
- [ ] Lane 6. Run `rdra-ish export tests/fixtures/xsys/shop-model --kind dbml --out <dir>`. Then, in a copy where auth shares `Session`, write `relate(Order, Auth.Session, N:1)` and run `check`. Save `keys-published.txt`. Pass when the DBML has column `sessionview_session_id` with note `external: auth.SessionView.session_id` and no `SessionView` table, and the shared-key copy exits 1.
- [ ] Lane 7. Run the m4 probe in `/tmp/how-xsys/explorer3/m4` through `check` at trunk. Then rewrite it with manifests, a published `SessionView`, and `extsystem ... from`, and run `check` at head. Save `m4-rewrite.txt`. Pass when trunk reports `Deadlock` and head reports `StuckBehindEnvironment` or no lifecycle finding, and no `Deadlock`.
- [ ] Lane 8. Run `rdra-ish check --entity Order tests/fixtures/xsys/shop-model` and `rdra-ish states --buc <the shop BUC id> tests/fixtures/xsys/shop-model`. Save `buc-entity-env.txt`. Pass when `--entity Order` still reports `StuckBehindEnvironment_Order_status_placed` and `--buc` still shows the `Environment:` block.
- [ ] Lane 9. In a shop copy where auth shares `Session`, guard a product use case on `Auth.Session.status == revoked` and run `check`. Save `shared-havoc.txt`. Pass when `check` reports `DirectEntityReference` and no `UnreachableForeignValue` or `Witness` finding for the guard.
- [ ] Lane 10. Run `rdra-ish check` and `rdra-ish lint --format json` on `tests/fixtures/xsys/shop-model`. Save `no-foreign-findings.txt`. Pass when no lifecycle finding or lint row has a subject whose id starts with `Auth.` or `Pay.`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Also measure min-of-10 of `rdra-ish states samples/clinic-ops`, which both produce. Trunk has no environment, so also measure min-of-10 of `rdra-ish check tests/fixtures/xsys/shop-model` at head, the case where the second stuck run executes.
- [ ] Probe. Run the interleaved probe in Appendix D, then the same interleaved loop for `states`. Then run the shop `check` 10 times at head.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on any clinic-ops command. A failure there means the second stuck run is not skipped on models with no environment action. Fail when the shop `check` minimum exceeds 80 ms.

**Review gate.** None. XM-6 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-6 to the base-branch stack on the XM-5 branch, and the operator lands it bottom-up.

## Depend on a model in another git repository (XM-7)

**Depends on.** XM-6.

**Files.**

- [ ] Edit `crates/rdra-ish-core/src/project.rs` and `crates/rdra-ish-core/src/diagnostics.rs`.
- [ ] Edit `crates/rdra-ish-cli/src/cli.rs`, `crates/rdra-ish-cli/src/deps.rs`, and `crates/rdra-ish-cli/src/main.rs`.
- [ ] Create `crates/rdra-ish-cli/tests/deps_git.rs`.
- [ ] Edit `crates/rdra-ish-lsp/src/uri.rs`, `crates/rdra-ish-lsp/src/backend.rs`, and `editors/vscode/src/extension.ts`.
- [ ] Edit `flake.nix`.
- [ ] Edit `docs/cli-reference.md` (`## deps` and a CI recipe) and `CHANGELOG.md`.

**Build.**

- [ ] Add the `git` + `rev` + `dir` source in `project.rs`. A `rev` names a tag or a commit. A branch, found with `git ls-remote --heads`, is `GitRevIsBranch`, an error with the hint to pin a tag or a commit.
- [ ] Add `DepCache` under `$RDRA_ISH_HOME`, with bare repositories in `git/db/<url>` and checkouts in `checkouts/<url>/<commit>/`. Encode the URL into a stable, collision-free directory name. Build each checkout in a temp dir and rename it into place. Treat an existing destination as success.
- [ ] Spawn `git` only from `crates/rdra-ish-cli/src/deps.rs`, so `check` and the LSP stay offline.
- [ ] Add `DependencyNotFetched` and `GitFailed`.
- [ ] Add `DepsCommand::Fetch { locked }` and `DepsCommand::Verify`. `deps fetch --locked` fetches exactly the locked commits and fails when `rdra.toml` and `rdra.lock` disagree. A warm cache makes `deps fetch --locked` spawn no `git`. `deps verify` extracts the surface from the locked commit again and compares it with the lock.
- [ ] Make `uri.rs::watched_path_is_rdra` match `rdra.toml` and `rdra.lock`, refresh in `backend.rs::did_change_watched_files`, and widen the VS Code watcher to `**/{*.rdra,rdra.toml,rdra.lock}`. Exclude the cache directory from the watcher.
- [ ] Add `nativeCheckInputs = [ pkgs.git ]` to the package in `flake.nix`.
- [ ] Run git in tests with `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`, and `-c user.name=t -c user.email=t@t`.
- [ ] Document a CI recipe in `docs/cli-reference.md` that runs `deps fetch --locked` with a cache key of `hashFiles('rdra.lock')`.

**You see.**

- [ ] `rdra-ish check <shop copy>` prints `Pay (payments, git <short commit> v1.4.0)` in the environment line.
- [ ] `rm -rf "$RDRA_ISH_HOME" && unshare -rn rdra-ish check <shop copy>` exits 0.
- [ ] A manifest with `rev = "main"` makes `rdra-ish deps lock` exit 1 with `GitRevIsBranch`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/rdra-ish-cli/tests/deps_git.rs` builds a bare `file://` repository in a tempdir with the payments fixture under `model/` at tag `v1.4.0`. It covers `deps lock`, `deps fetch --locked`, `check` with the cache removed, `--locked` after a changed `rev`, `GitRevIsBranch`, and `deps verify`. Run `cargo test -p rdra-ish-cli --test deps_git`.
- [ ] `crates/rdra-ish-lsp/src/workspace_test.rs` or a new stdio test sends `workspace/didChangeWatchedFiles` with an `rdra.lock` URI and sees a refresh. Run `cargo test -p rdra-ish-lsp`.
- [ ] `crates/rdra-ish-lsp/src/uri.rs` tests gain `rdra.toml` and `rdra.lock`. Run `cargo test -p rdra-ish-lsp uri`.
- [ ] Run every CI gate from **PR mechanics**. If any skill mentions `rdra-ish deps fetch --locked`, confirm the snippet check passes on it.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe. Every lane exports `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 RDRA_ISH_HOME=/tmp/swarm-XM-7/worker-<n>/cache` and builds `/tmp/swarm-XM-7/worker-<n>/remote/payments.git` holding `model/` at tag `v1.4.0`.

- [ ] Lane 1. Regression lane against trunk. Run the head sweep and `diff -rq /tmp/xm-sweep-trunk /tmp/xm-sweep-head`. Trunk has no git dependency, so also run `deps lock`, `deps fetch --locked`, and `check` on a shop copy whose `rdra.toml` points `payments` at the `file://` remote. Save `sweep-and-git.txt`. Pass when every differing sweep path ends in `csv-actor-permission-audit` or `list-actor-permission-audit`, and the git shop `check` exits 0.
- [ ] Lane 2. Run `rdra-ish deps lock` and read `rdra.lock` and `check`. Save `git-lock.txt`. Pass when the lock records the full commit of `v1.4.0` and `check` prints `Pay (payments, git <short commit> v1.4.0)`.
- [ ] Lane 3. Run `deps fetch --locked`, then `rm -rf "$RDRA_ISH_HOME"`, then `unshare -rn rdra-ish check <shop copy>`. Save `offline-no-cache.txt`. Pass when the `unshare` run exits 0.
- [ ] Lane 4. Run `strace -f -qq -e trace=execve -o <log> rdra-ish check <shop copy>`. Save `check-execve.txt`. Pass when the log holds one line.
- [ ] Lane 5. Set `rev = "main"` and run `deps lock`. Save `rev-is-branch.txt`. Pass when it exits 1 with `GitRevIsBranch` and the hint to pin a tag or a commit, and `rdra.lock` is unchanged.
- [ ] Lane 6. Change `rev` in `rdra.toml` to another tag without relocking and run `deps fetch --locked`. Save `locked-mismatch.txt`. Pass when it exits non-zero, names the disagreement between `rdra.toml` and `rdra.lock`, and `rdra.lock` is unchanged.
- [ ] Lane 7. Run `deps verify` on the clean copy. Then change one word of the payments surface text in `rdra.lock`, recompute its sha256 so `check` passes, and run `deps verify` again. Save `deps-verify.txt`. Pass when the first run exits 0 and the second exits 1 and names the differing line.
- [ ] Lane 8. Run `deps fetch --locked` twice, the second time under `strace -f -qq -e trace=execve`, and hash the cache tree with `find "$RDRA_ISH_HOME" -type f | sort | xargs sha256sum` after each run. Then empty the cache and run two `deps fetch --locked` at once. Save `fetch-idempotent.txt`. Pass when the second run spawns no `git`, the two hashes match, both parallel runs exit 0, and the cache holds one checkout for the commit.
- [ ] Lane 9. Point `payments` at a `file://` path that does not exist and run `deps lock`. Save `git-failed.txt`. Pass when it exits 1 with `GitFailed` naming the URL, and no partial directory is left under `$RDRA_ISH_HOME/checkouts`.
- [ ] Lane 10. Copy `/tmp/plan-xsys/lsp_probe.py` to the lane dir and add one `workspace/didChangeWatchedFiles` notification for `rdra.lock` after the first diagnostics. Run it on the git shop copy, and change the lock's surface text between the open and the notification. Save `lsp-lock-reload.txt`. Pass when the diagnostics published after the notification differ from the first set and include `SurfaceChanged` or `LockTampered`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Min-of-10 wall time in ms of `rdra-ish check samples/clinic-ops` and `rdra-ish csv samples/clinic-ops --kind entity` at trunk and head. Trunk has no git dependency, so also measure the `execve` count of `check` on the git shop copy and min-of-10 wall time of `deps fetch --locked` on a warm cache at head.
- [ ] Probe. Run the interleaved probe in Appendix D. Then run `strace -f -qq -e trace=execve` on `check`, and run `deps fetch --locked` 10 times on a warm cache with the same `date +%s%N` loop.
- [ ] Baseline. Record the trunk `trunk_min_ms` values first. The trunk `execve` count for any command is 1.
- [ ] Rule. Fail when `head_min_ms` exceeds `trunk_min_ms` by more than 15 ms on either clinic-ops command. Fail when `check` on the git shop copy logs more than 1 `execve`. Fail when the warm `deps fetch --locked` minimum exceeds 100 ms.

**Review gate.** None. XM-7 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends XM-7 to the base-branch stack on the XM-6 branch, and the operator lands it bottom-up.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Reply to the operator with the report `playbooks/autopilot-stack.md` names. List each PR, XM-1 through XM-7, with its verdict, head SHA, base branch, patch-id, and PR link.

## Appendix A. Prototype evidence

No prototype questions remained when this plan was written. Every open question in the design was a product call, and the user answered each one. The answers are the design's "Decided after review" section, items 1 to 6, which this plan treats as binding. No prototype branch or SHA exists for this program.

The design rests on trunk facts that earlier probes measured at `a941a7d`. Its code is identical to `origin/main` at `dd3d6bd`, because the three commits above `dd3d6bd` touch only the proposal.

- `/tmp/arena-xsys2/candidate-1/probes/today/grammar.out.txt` shows that trunk rejects `usecase::Auth.Login`, `Auth.Rec.result == ok`, `extsystem Auth "Auth" from auth.AuthSystem`, and `.as(S)`. XM-3 and XM-5 add those forms.
- `/tmp/arena-xsys2/candidate-1/probes/today/outbox.out.txt` and `outbox-tla/` show the split `Login_1` and `Login_2` actions that XM-2 merges.
- `/tmp/arena-xsys2/candidate-1/probes/today/service.out.txt` shows that `has_permission` on a `system` with a local permission is a `TypeMismatch`. XM-5 lane 8 keeps it.
- `/tmp/arena-xsys/judge.md` confirms that `Auth.Login` parses today, that `ActionOrigin` has only `UseCase` and `Clock`, and that `tx.rs` groups only `invokes` with an `api` target.
- `/tmp/plan-xsys/explore.md` records the trunk perf baseline, the deterministic 1378-run sweep, and the working `file://`, `unshare -rn`, and `strace` checks.

Three claims stay unproven until the stack runs. TLC agreement with BFS on XM-2's merged action and XM-6's split is CI-only, because `tlc` is not installed locally. The cost of `Partitioned::iter` on hot loops in XM-3 is a guess until its perf lane runs. The LSP behavior after XM-1 routes through the CLI include rule is unmeasured until XM-1 lane 7.

## Appendix B. Alternatives rejected

The design went through two arena rounds. The round 1 judge, in `/tmp/arena-xsys/judge.md`, scored candidate 1 at 26, candidate 3 at 25, candidate 4 at 22, and candidate 2 at 21, and picked candidate 1 as the base. No round 2 judge file exists on disk. Its conclusions live in the "Why" column of the design's Decisions table, and the round 2 candidates are in `/tmp/arena-xsys2/candidate-{1..4}/design.md` against `/tmp/arena-xsys2/rubric.md`.

- **`NodeRef::Foreign` with separate tables** (round 1 candidate 3, round 2 candidate 4). It excludes relation loops by construction, at the cost of about 35 exhaustive match sites and a second entity-owner type. Partitioned slot maps reach the same default with a smaller migration. The `FOREIGN_OPERANDS` table and the leak test in XM-3 bound the relation-loop gap.
- **Side-table origin filters** (round 1 candidates 2 and 4). Nothing is excluded until each consumer is edited, and a missed consumer leaks with no error.
- **Exposing an internal entity with its state machine replayed as environment edges** (round 1). The user's rule against direct entity references rules it out, and every internal edge change would churn the product's lock.
- **A consumer-declared escape hatch for internal entities.** It bypasses the owner. The design keeps the owner-declared `shares` with warnings on both sides instead, per item 1 of "Decided after review".
- **Events with payloads in place of append entities.** It gives the event kind a body and keys, and leaves silver data without a home.
- **A new `calls` or `uses` predicate.** It duplicates the permission and transaction rules of `invokes`, and `uses` was removed on purpose.
- **A service principal as an actor or as a new `service` kind.** RDRA actors are people, and no check would read a separate kind. A product `system` with `.as_service` keeps the distinction where the access check reads it.
- **Semver version requirements in place of a surface hash.** A version string does not change when a surface does. The hash does.
- **Vendoring the dependency into the product repository.** It copies another team's model into the product's history. The lock plus the cache gives offline CI from one source.
- **Audit columns only for models with dependencies** (a plan-level choice in XM-5). It would keep every sample artifact unchanged, but the CSV schema would then depend on the input. XM-5 always emits the two columns and regenerates six artifacts and one embed.
- **Skipping the git tests in the `nix build` sandbox** (a plan-level choice in XM-7). A skip hides the tests from that build. XM-7 adds `pkgs.git` to `nativeCheckInputs` instead.
- **Watching `rdra.lock` in the LSP from XM-4.** The design's Delivery list puts LSP reload in XM-7, so XM-4 reads the lock on each existing refresh and adds no watcher.

## Appendix C. Risks

The live surface is a CLI and an LSP over stdio, so no lane can take a screenshot. Every lane saves a terminal transcript in its place, and the operator audits transcripts.

These eight explorer findings from `/tmp/plan-xsys/explore.md` are folded into the PR sections above.

1. **The manifest walk-up start on symlinked samples** (XM-1). `root_include_paths` canonicalizes, so the `samples/formal-verification` roots resolve under `skills/`. XM-1 walks up from the path as given and keeps the canonical include rule. Lane 5 watches it.
2. **The LSP and the CLI use different include roots** (XM-1). The LSP adds workspace roots, and the CLI does not. XM-1 picks the CLI rule. Lanes 6 and 7 watch the LSP counts.
3. **A stray `rdra.toml` in an ancestor directory** (XM-1). The walk-up stops at the directory that holds `.git`. Lane 4 watches it.
4. **Cross-model fixtures in `samples.txt`** (XM-3). `diagram_contract.rs`, `engine_equivalence.rs`, `scripts/verify-samples.sh`, and the sweep all read `samples.txt`. Every PR keeps fixtures under `tests/fixtures/xsys/` and out of that file.
5. **The skill snippet checker** (XM-3 and XM-4). A cross-model `rdra` fence in `skills/` fails, because the checker runs each fence as one file with no manifest. `SUBCOMMANDS` is hard-coded. XM-3 keeps cross-model fences out of `skills/`, and XM-4 adds `deps`.
6. **Audit columns change committed artifacts** (XM-5). Six CSV files and one embedded design doc change. XM-5 regenerates them with `scripts/check-sample-artifacts.sh`, and lane 1 allows only those sweep paths to differ. XM-6 and XM-7 carry the same allowance.
7. **The second stuck run costs every model** (XM-6). XM-6 skips it when no environment action exists. The clinic-ops perf rule watches it.
8. **The `nix build` sandbox has no `git`, and the user's git config leaks into tests** (XM-7). XM-7 adds `pkgs.git` to `nativeCheckInputs`. Tests set `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1`. CI does not run `nix build`, so the owner runs it once by hand if `nix` works.

Other risks, by PR:

- **TLC is CI-only** (XM-2 and XM-6). `nix develop` fails with HTTP 403 here, and neither `tlc` nor GNU `time` is installed. The owner links the `formal-verify` run as the TLC evidence.
- **`SeparateTxInferred` users** (XM-2). The lint mapping and two docs passages name it. The owner updates them with the rule.
- **The surface text is a contract** (XM-2 and XM-4). Unstable ordering or escaping would churn every lock. XM-2 sorts the text and tests two declaration orders. XM-4 hashes the text, not the bytes.
- **`len()` and `is_empty()` change meaning on about 45 call sites** (XM-3). The owner reviews each one and lists the decision in the PR body.
- **The 65 relation loops can meet a stub endpoint** (XM-3). `lint.rs` `relation_degree`, the diagrams, and `access.rs` are the known ones. The leak test and `FOREIGN_OPERANDS` bound them.
- **XM-3 is the largest PR.** It holds grammar, loading, the partition, resolution, diagnostics, and opt-in rendering. The design's Delivery list fixes the PR boundaries, so the owner keeps one commit per concern inside it so review can follow.
- **The `naming-id` lint rejects `Auth.Login`** if a stub becomes a lint subject (XM-3). The partition keeps stubs out of element lints.
- **LSP rename into a dependency** (XM-3). `rename.rs` and `refs.rs` stop at foreign refs.
- **`--strict` semantics change** with the call gap (XM-5). The owner documents it.
- **The 4096-state cap per cone can be hit sooner** (XM-6), because environment writers of an upsert entity add free column values. The shop perf budget watches it.
- **`--buc` and `--entity` can drop environment actions** (XM-6). Lane 8 watches it.
- **Cache paths on Windows and concurrent fetches** (XM-7). The URL encoding must be stable and collision-free. A rename into place makes parallel fetches converge. Lane 8 watches it.
- **LSP reload storms** if the watcher matches the cache directory (XM-7). The watcher excludes it.
- **Plan tools live in `/tmp`** (every PR). The Arm step copies `sweep.sh`, `lsp_probe.py`, and `explore.md` to the agent store.
- **The proposal is not on `main` yet.** The design and this plan sit on `claude/hopeful-archimedes-ar0tcg`. XM-1 needs no file from that branch, because the three commits are docs only.

## Appendix D. Links and reading list

Read these before editing.

- `docs/proposals/cross-model-references.md`, the accepted design. The Delivery list and "Decided after review" are binding.
- `/tmp/plan-xsys/explore.md`, the file and line map for each PR, the conventions, the test commands, and the trunk baseline.
- `docs/language-reference.md`, `docs/rdra-ish-interpretation.md`, `docs/vision.md`, `docs/state-derivation.md`, and `docs/cli-reference.md`.
- `/tmp/arena-xsys2/candidate-1/probes/proposed/`, the draft fixtures for auth, payments, shop, and the mistakes files.

Run `skills/how/SKILL.md` before XM-1 (loading), XM-3 (the partition and resolution), and XM-6 (the lifecycle and TLA+). Run `skills/interrogate/SKILL.md` on XM-3 (the partition and the leak bound), XM-4 (the lock format and its failure modes), and XM-6 (environment soundness and fairness).

Keep the decision trail per `skills/show-me-your-work/SKILL.md` in `~/.claude/orchestrate/cross-model-references/trail.tsv`, one row per decision and one per tick. It stays local unless the operator asks for it in a PR.

The perf probe is this interleaved loop. Run it from the head worktree root with `T` set to the trunk binary and `H` set to the head binary.

```sh
T=${T:?trunk binary} H=${H:?head binary}
for cmd in "check samples/clinic-ops" "csv samples/clinic-ops --kind entity --out /tmp/xm-perf.csv"; do
	: > /tmp/xm-perf-T.txt; : > /tmp/xm-perf-H.txt
	for i in $(seq 10); do
		for side in T H; do
			bin=${!side}; s=$(date +%s%N); $bin $cmd >/dev/null 2>&1; echo $(( ($(date +%s%N) - s) / 1000000 )) >> /tmp/xm-perf-$side.txt
		done
	done
	echo "$cmd trunk_min_ms=$(sort -n /tmp/xm-perf-T.txt | head -1) head_min_ms=$(sort -n /tmp/xm-perf-H.txt | head -1)"
done
```

The observed run-to-run spread is about 30 ms, so the 15 ms rule compares minimums only. Check every number against `skills/benchmark-checklist/SKILL.md` before reporting it.
