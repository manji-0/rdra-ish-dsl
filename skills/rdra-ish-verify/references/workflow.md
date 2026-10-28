# Export and Verify Workflow

<!-- derived-from ../../../docs/cli-reference.md#export -->
<!-- derived-from ../../../docs/cli-reference.md#verify -->
<!-- derived-from ../../../docs/formal-verification.md -->

## Commands

Prefer a bundled sample under this skill's `samples/` directory:

```sh
FILE="<skill-root>/samples/order.rdra"

rdra-ish check "$FILE"
rdra-ish states "$FILE"                 # optional BFS pass first
rdra-ish export "$FILE" --kind tla -o <OUT>
rdra-ish verify "$FILE" --backend tlc -o <OUT>
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `.tla` **and** sibling `.cfg` (module = file stem) |
| `verify --backend tlc` | Print one spec per cone of influence, then run `tlc` / `tlc2` from `PATH` |
| `states` | Local BFS (no TLC dependency) |

### Output Path Rules

- `-o` ends with `.tla` → that basename is the module; sibling `.cfg` next to it
- `-o` is a directory or has no extension → `<dir>/RdraSpec.tla` and
  `<dir>/RdraSpec.cfg`
- `verify` copies bare `Module.tla` / `Module.cfg` into its work dir for TLC
- `verify` without `-o` uses a temp dir: deleted on success, kept on failure and named
  by the closing `specs: <dir> (kept because a check failed)` line
- `verify` reports `tool_error` with a memory hint when TLC runs out of heap: set
  `RDRA_TLC_HEAP=4g` (default `1g` per cone, `2g` for the temporal run) or
  `JAVA_TOOL_OPTIONS=-Xmx4g`, which wins

## Suggested Order

1. `check` one target file (bundled sample or user model).
2. `states` for Enum/Bool/Nullable feedback.
3. `export --kind tla` and skim `Init` / `Next` / `INVARIANT` / `PROPERTY` in `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH`.
5. `verify --backend tlc` and capture counterexamples.

## Reporting Tips

- Cite TLC property names from the `.cfg` `PROPERTY` section.
- Distinguish invariants (`forbidden`, `invariant`, ...) from liveness
  (`eventually` / `leads_to` need fairness via `WF_vars`).
- If `.along` was intended but no `relate` path exists, note the stronger
  product quantification warning from the emitter.
- Prefer one coherent module per verify run.
