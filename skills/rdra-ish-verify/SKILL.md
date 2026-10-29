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

Use this skill when the question is about model checking beyond `check` and `states`.
Keep this file as the routing layer; load one reference for the concrete task.

Bundled samples live under `samples/` in this skill directory, so the workflow works
after `npx skills add` / `gh skill install`. In the `rdra-ish-dsl` repository,
`samples/formal-verification/` is a symlink mirror of the same files.

### Routing Guide

| User intent | Do first | Load |
|---|---|---|
| "states で足りる？ / TLA が必要？" | Compare `check` (one instance, no temporal properties) with `verify` (two instances, temporal) | `references/when-to-use.md` |
| "property / after.assert / when / .along を書きたい" | Author the DSL surface correctly | `references/dsl-surface.md` |
| "export / verify を回したい", "what does this output mean?" | Run check, then verify, and read the result | `references/workflow.md` |
| "サンプルや制限は？" | Open a file under `samples/`; note the bounds and approximations | `references/limits-and-samples.md` |

### Default Workflow

1. Work from the skill root (the directory that holds this `SKILL.md`). Check **one**
   sample file at a time: the samples reuse ids such as `Order`, so checking the whole
   `samples/` directory fails on duplicate declarations.
2. Run `rdra-ish check <file>` first. It explores the same transition system as TLC by
   breadth-first search at one instance per entity, so it is the fast loop.
3. Decide whether TLC is needed with `references/when-to-use.md`: temporal `property`,
   and rules that need two instances (quantifiers, `.along` over several rows, 1:1 keys).
4. With `tlc` or `tlc2` on `PATH` (`nix develop` provides it in the repository), run
   `rdra-ish verify <file>`. `--backend tlc` is the default.
5. Report each failure by the name after its severity (`error[CrossForbidden_Order_0]`,
   `error[Deadlock_Order_status_paid]`, `error[NeverShipped]`) and its model-term
   counterexample, together with the `bounds:` line the result holds under.

A passing run names its bounds first and last:

```text
$ rdra-ish verify samples/order.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, each checked in its own run on the whole model (fairness does not restrict to a cone)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
RdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)
RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)
tlc_result: ok
OK: no errors within 2 instances per entity, Int 0..5
```

### Quick Command Palette

Paths are relative to this skill directory:

```sh
rdra-ish check samples/order.rdra
rdra-ish states samples/order.rdra
rdra-ish verify samples/order.rdra
rdra-ish verify samples/partial_scope.rdra --entity Order
rdra-ish export samples/order.rdra --kind tla -o /tmp/rdra-tla
```

### Related Skills

- `rdra-ish-buc-analyze` — BFS state-pattern review and broader BUC analysis
- `rdra-ish-write` — author lifecycle / rules that feed verification
- `rdra-ish-review` — checklist review including verification when claimed
- `rdra-ish-diagram` — state / event-flow views before model checking

### Reference Files

- `references/when-to-use.md` — `check` / `states` vs `verify` decision table
- `references/dsl-surface.md` — `property`, `after.assert`, quantifiers, `.along`
- `references/workflow.md` — commands, output files, and how to read each finding
- `references/limits-and-samples.md` — bounds, approximations, and the bundled samples
- `samples/*.rdra` — standalone examples, each with its expected result in `// expect`
  lines
- `samples/fail/order.rdra` — intentionally unsafe negative example
