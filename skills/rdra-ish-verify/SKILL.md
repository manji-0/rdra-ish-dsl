---
name: rdra-ish-verify
description: >-
  Formal-verify RDRA-ish models with TLA+/TLC. Use when the user asks about
  export --kind tla, verify --backend tlc, temporal property / always /
  eventually / leads_to, after.assert, when().none/has, Int/Money/Decimal/now
  arithmetic, multi-entity forbidden/invariant with .along, or whether TLC
  should complement rdra-ish states.
license: MIT
---

## Choose Formal Verification

Use this skill when the question is about model checking beyond BFS `states`.
Keep this file as the routing layer; load one reference for the concrete task.

Bundled samples live under **`samples/`** in this skill directory (installable with
`npx skills` / `gh skill`). Prefer those paths so the workflow works outside the
monorepo. In the `rdra-ish-dsl` checkout, `samples/formal-verification/` is a
symlink mirror of the same files.

### Routing Guide

| User intent | Do first | Load |
|---|---|---|
| "states で足りる？ / TLA が必要？" | Compare BFS axes vs TLC arithmetic / temporal | `references/when-to-use.md` |
| "property / after.assert / when / .along を書きたい" | Author the DSL surface correctly | `references/dsl-surface.md` |
| "export / verify を回したい" | Run check → export → (optional) TLC | `references/workflow.md` |
| "サンプルや制限は？" | Open a file under `samples/`; note approximations | `references/limits-and-samples.md` |

### Default Workflow

1. Resolve the skill root (directory that contains this `SKILL.md`). Run
   `rdra-ish check` on **one** sample file under `samples/` (ids collide across
   files — never check the whole `samples/` directory at once).
2. Decide BFS vs TLA using `references/when-to-use.md`. Prefer `states` for
   Enum / Bool / Nullable reachability; use TLA for Int/`now`, temporal
   `property`, `after.assert`, quantifiers, and multi-instance `.along`.
3. Export: `rdra-ish export <file> --kind tla -o <OUT>` (writes `.tla` and `.cfg`).
4. If `tlc` / `tlc2` is on `PATH`: `rdra-ish verify <file> --backend tlc -o <OUT>`.
   Prefer **expected-pass** samples from `references/limits-and-samples.md`; use
   `samples/fail/` only for negative TLC checks (`check` may exit 0 with warnings).
5. Report evidence from TLC property names / Safety failures, not only intuition.
   Heed stderr `warning: tla export: …` lines (skipped mappings / approximations).
6. For full mapping tables in the monorepo, see `docs/formal-verification.md`.

### Quick Command Palette

Paths are relative to this skill directory:

```sh
SKILL_ROOT="$(dirname "$0")"   # or the installed skill path containing SKILL.md
FILE="$SKILL_ROOT/samples/order.rdra"

rdra-ish check "$FILE"
rdra-ish states "$FILE"
rdra-ish export "$FILE" --kind tla -o /tmp/rdra-tla
rdra-ish verify "$FILE" --backend tlc -o /tmp/rdra-tla
```

Monorepo mirror (same content via symlink):

```sh
rdra-ish check samples/formal-verification/order.rdra
rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla
```

### Related Skills

- `rdra-ish-buc-analyze` — BFS state-pattern review and broader BUC analysis
- `rdra-ish-write` — author lifecycle / rules that feed verification
- `rdra-ish-review` — checklist review including TLA export when claimed
- `rdra-ish-diagram` — state / event-flow views before model checking

### Reference Files

- `references/when-to-use.md` — BFS `states` vs TLA+/TLC decision table
- `references/dsl-surface.md` — `property`, `after.assert`, quantifiers, `.along`
- `references/workflow.md` — export / verify commands and interpretation tips
- `references/limits-and-samples.md` — emitter approximations and bundled samples
- `samples/*.rdra` — standalone positive examples
- `samples/fail/order.rdra` — intentionally unsafe negative example
