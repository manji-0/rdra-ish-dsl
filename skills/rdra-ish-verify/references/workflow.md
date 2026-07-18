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
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` |
| `verify --backend tlc` | Export, then run `tlc` / `tlc2` from `PATH` |
| `states` | Local BFS (no TLC dependency) |

### Output Path Rules

- `-o` ends with `.tla` → sibling `.cfg` next to it
- `-o` is a directory or has no extension → `<dir>/RdraSpec.tla` and
  `<dir>/RdraSpec.cfg`

## Suggested Order

1. `check` one target file (bundled sample or user model).
2. `states` for Enum/Bool/Nullable feedback.
3. `export --kind tla` and skim `Init` / `Next` / `Safety` / `PROPERTY` in `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH`.
5. `verify --backend tlc` and capture counterexamples.

## Reporting Tips

- Cite TLC property names from the `.cfg` `PROPERTY` section.
- Distinguish Safety (invariants / forbidden) from liveness
  (`eventually` / `leads_to` need fairness via `WF_vars`).
- If `.along` was intended but no `relate` path exists, note the stronger
  product quantification warning from the emitter.
- Prefer one coherent module per verify run.
