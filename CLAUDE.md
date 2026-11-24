# Working notes for agents

## Before a heavy check

A heavy check is anything that takes minutes: the full CI-equivalent suite,
`scripts/verify-samples.sh --with-tlc`, `scripts/reduction-diff.py run`, the
`engine_equivalence`, `proof_equivalence` and `engine_corpus` tests, a whole-output diff
between two builds, `verify` of oidc-auth, insurance-claim-ops or food-permit-ops at the
default bounds, and memory or timing measurements. Before you start one, go through this
list. Run it only when an item says it is needed, and say which item that was.

1. **GitHub Actions already runs it?** `.github/workflows/ci.yml` runs on every pull request:
   fmt, clippy, the tests (`scripts/diff-test.sh --all`), sample artifacts, skill snippets,
   `nix build`, `verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt`, reduced
   cones, the `reduction-diff.py` seeds (without TLC on a PR, with it on main),
   `engine_equivalence`, `proof_equivalence`, `engine_corpus`,
   the `shadow` run of the samples and the corpus, and the Kani harnesses (the last ones
   only when a change can reach them; see `scripts/verify-relevant.sh` and
   `scripts/kani-relevant.sh`). A pull request whose files are all prose
   (`scripts/code-relevant.sh`) runs fmt, clippy, the tests that read Markdown, and the
   skill snippets in place of the whole suite and the sample artifacts. The PR's checks,
   green on a head that contains the latest main, are the merge gate. Do not run these
   locally to gate a merge. Locally run only what Actions cannot: behaviour that differs on
   macOS (Actions runs on ubuntu).
2. **Can the change reach it?** Run a check only when a changed file can change its result:

   | Changed | Worth running locally |
   |---|---|
   | `docs/`, `skills/`, `README.md`, `CHANGELOG.md` | `scripts/check-skill-snippets.py`, `cargo test -p rdra-ish-cli --test export_gate` |
   | `crates/rdra-ish-emit/src/tla/` | the emit tests; one TLC case through `RDRA_CORPUS_CASE=<name>` |
   | `crates/rdra-ish-core/src/lifecycle/prove/` | the prove tests; `proof_equivalence` only if the encoding changed |
   | `crates/rdra-ish-core/src/lifecycle/{explore,reduce}/`, `cone.rs` | the core tests; for a speed-up, see [A speed-up change](#a-speed-up-change) |
   | `.github/workflows/` | nothing; the PR run is the test |

3. **A cheaper check answers the same question?** A targeted test, one corpus case
   (`RDRA_CORPUS_CASE=<name>`), one sample at the flags in its `// expect` header,
   `--instance-count 1`, instructions retired at `RDRA_JOBS=1` (they do not depend on load)
   rather than repeated wall times.
4. **The bounds are the ones that matter?** Compare samples at the flags in their `// expect`
   headers, as `verify-samples.sh` does. Do not run oidc-auth, insurance-claim-ops or
   food-permit-ops at the default `verify` bounds with `--with-tlc` for a diff: one such run
   takes over an hour. TLC output also varies between runs, so a `--with-tlc` diff says
   nothing about a change that does not touch the TLA+ emission.
5. **It already passed on a tree the change since cannot reach?** If main moved only in
   files from row 1 of the table, do not run it again after merging main.
6. **Someone else runs it anyway?** An agent does not repeat what the parent session or the
   PR run will do, and the parent does not repeat what an agent reported.
7. **The machine has room?** Check `uptime`. Above 18 (the core count), wait or lower `-j`.
   Run one heavy command at a time, under a timeout, and with a memory cap where the
   command can grow (`--memory`, `ulimit -v`).
8. **Estimate first.** Say how long you expect it to take. Over 10 minutes, ask whether the
   answer is worth that before starting.

## A speed-up change

A change that must make the checker faster without changing what it prints keeps the
function it replaces as that function's twin, in the same PR:

1. Move the old function, unchanged, into a `reference.rs` beside the fast one (as
   `lifecycle/explore/moves/reference.rs` and `lifecycle/reduce/partition/reference.rs` do), and call it from the
   fast path under the `shadow` feature: compare the two results and panic with a message
   that starts `shadow:` and names the call. Count the comparison so
   `RDRA_ISH_SHADOW_STATS=1` reports it.
2. Plant a bug in the fast side, show a `--features shadow` build catches it, and revert.
3. Show a passing shadow run: `cargo build --release -p rdra-ish-cli --features shadow`,
   then `scripts/verify-samples.sh target/release/rdra-ish` and
   `cargo test --release -p rdra-ish-cli --features shadow --test engine_corpus`, with
   every twin's count above zero. The `shadow` job in CI runs the same.

This replaces the byte-identical diff of every output between main and the branch. Do not
run that diff for a change whose fast paths all have twins.

Run the diff, at the `// expect` header flags and without `--with-tlc`, only for a
change no twin can hold: one that skips work rather than redoing it faster (the check
mask of #251, the early stop of #213), changes scheduling or threads, or replaces a
function whose reference would cost a second whole BFS. Say in the PR which part had no
twin and why.

To find what to speed up, build with `--features perf` and run `scripts/perf-tables.py
target/release/rdra-ish [sample ...]`: the passes, the BFS CPU per pass and the largest runs
of each `verify`, at `RDRA_JOBS=1` (#296). Under `RDRA_PERF=2` its run lines hash each run's
graph, which shows two runs of the same graph.

## GitHub API

Poll a workflow run every two minutes or more (`gh run view <id> --json status,conclusion`),
not with `gh run watch`, and keep one poller at a time. Parallel watchers used up the
account's hourly API limit on 2026-11-24.
