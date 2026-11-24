---
name: rdra-ish-verify
description: >-
  Formal-verify RDRA-ish models with rdra-ish verify (native, no JVM; TLC as a
  second opinion). Use when the user asks about export --kind tla, verify,
  verify --with-tlc, temporal property / always / eventually / leads_to /
  stable / unchanged / precedes / can_reach, after.assert,
  invariant(E).when().none/has, Int/Money/Decimal/now arithmetic, multi-entity
  forbidden/invariant with .along, whether verify should complement rdra-ish
  states, a not decided result or the memory limit, or verify --prove (a rule
  for any number of rows, a counterexample to induction, a .lemma()).
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
| "このガードは必要？ / which rules does this guard protect?" | Run `rdra-ish check <model> --guards` and read each guard's line | `references/workflow.md` |
| "何行でも成り立つ？ / --prove / CTI / lemma" | Run `rdra-ish verify --prove`, read each `unproved:` CTI, and propose at most one `.lemma()` per CTI | `references/prove.md` |
| "サンプルや制限は？" | Open a file under `samples/`; note the bounds and approximations | `references/limits-and-samples.md` |

### Default Workflow

1. Work from the skill root (the directory that holds this `SKILL.md`). Check **one**
   sample file at a time: the samples reuse ids such as `Order`, so checking the whole
   `samples/` directory fails on duplicate declarations.
2. Run `rdra-ish check <file>` first. For a model split over several files, pass its
   root directory (`rdra-ish check ec-site/`), not a list of files. It explores the
   transition system by breadth-first search at one instance per entity, so it is the fast
   loop. It evaluates no `property`.
3. Decide whether `verify` is needed with `references/when-to-use.md`: temporal
   `property`, and rules that need two instances (quantifiers, `.along` over several rows,
   1:1 keys and `@unique` sets).
4. Run `rdra-ish verify <file>`. It needs no JVM and no TLC. For a second opinion on a
   result that matters, run `rdra-ish verify <file> --with-tlc` with `tlc` or `tlc2` on
   `PATH` (`nix develop` provides it in the repository; elsewhere
   `nix shell nixpkgs#tlaplus`) or `--tlc-jar <tla2tools.jar>`: TLC checks the same cones,
   and a check the two decide differently fails the run as a tool bug.
5. Report each failure by the name after its severity (`error[MultiForbidden_Order_0]`,
   `error[Deadlock_Order_status_paid]`, `error[NeverShipped]`) and its model-term
   counterexample, together with the `bounds:` line the result holds under. Paste the
   `bounds:` line (with its state cap or memory limit), `result:` and the last line
   exactly as printed, in a code block; do not paraphrase them. `result: not decided`
   with exit 0 means a run stopped at the memory limit, a `--max-states` cap, or
   `--temporal-budget`: the findings listed under
   `not checked:` and the cone lines that read `not decided` are open, not passed.

A passing run names its bounds first and last:

```text
$ rdra-ish verify samples/order.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
Cone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))
Cone_EventuallyTerminal [EventuallyTerminal]: not decided (31 states, Cone_EventuallyTerminal_WF: ok, needs the whole model's WF_vars(Next))
RdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)
RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)
result: ok
OK: no errors within 2 instances per entity, Int 0..5
```

### Quick Command Palette

Paths are relative to this skill directory:

```sh
rdra-ish check samples/order.rdra
rdra-ish states samples/order.rdra
rdra-ish verify samples/order.rdra
rdra-ish verify samples/partial_scope.rdra --entity Order
rdra-ish check samples/split_run_order.rdra --guards
rdra-ish export samples/order.rdra --kind tla -o /tmp/rdra-tla
```

### Related Skills

- `rdra-ish-buc-analyze` — BFS state-pattern review and broader BUC analysis
- `rdra-ish-write` — author lifecycle / rules that feed verification
- `rdra-ish-review` — checklist review including verification when claimed
- `rdra-ish-diagram` — state / event views before model checking

### Reference Files

- `references/when-to-use.md` — `check` / `states` vs `verify` decision table
- `references/dsl-surface.md` — `property`, `after.assert`, quantifiers, `.along`
- `references/workflow.md` — commands, output files, how to read each finding, and how
  to read `check --guards`
- `references/limits-and-samples.md` — bounds, approximations, and the bundled samples
- `references/prove.md` — `verify --prove`: proved / unproved lines, reading a CTI, and
  writing a `.lemma()`
- `samples/*.rdra` — standalone examples, each with its expected result in `// expect`
  lines
- `samples/fail/order.rdra` — intentionally unsafe negative example
