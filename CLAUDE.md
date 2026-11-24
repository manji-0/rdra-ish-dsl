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
   `nix build`, `verify-samples.sh` with and without `--with-tlc`, reduced cones, the
   `reduction-diff.py` seeds, `engine_equivalence`, `proof_equivalence`, and
   `engine_corpus`. A pull request whose files are all prose (`scripts/code-relevant.sh`)
   runs fmt, clippy, the tests that read Markdown, and the skill snippets in place of the
   whole suite and the sample artifacts. The PR's checks, green on a head that contains
   the latest main, are the merge gate. Do not run these locally to gate a merge. Locally run only what Actions
   cannot: behaviour that differs on macOS (Actions runs on ubuntu), and the byte-identical
   output diff a performance change must show.
2. **Can the change reach it?** Run a check only when a changed file can change its result:

   | Changed | Worth running locally |
   |---|---|
   | `docs/`, `skills/`, `README.md`, `CHANGELOG.md` | `scripts/check-skill-snippets.py`, `cargo test -p rdra-ish-cli --test export_gate` |
   | `crates/rdra-ish-emit/src/tla/` | the emit tests; one TLC case through `RDRA_CORPUS_CASE=<name>` |
   | `crates/rdra-ish-core/src/lifecycle/prove/` | the prove tests; `proof_equivalence` only if the encoding changed |
   | `crates/rdra-ish-core/src/lifecycle/{explore,reduce}/`, `cone.rs` | the core tests; the byte-identical native diff for a performance change |
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

## GitHub API

Poll a workflow run every two minutes or more (`gh run view <id> --json status,conclusion`),
not with `gh run watch`, and keep one poller at a time. Parallel watchers used up the
account's hourly API limit on 2026-11-24.
