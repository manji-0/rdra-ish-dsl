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

## When to use

Use this skill when a question goes past what `check` decides at one instance per entity:
a temporal `property`, a rule that needs two rows, a `not decided` result, the memory
limit, or a proof for any number of rows (`verify --prove`).

The `.rdra` files under `samples/` ship with the skill. Run every command from the skill
root, one sample file at a time: the samples reuse ids such as `Order`, so the whole
directory fails on duplicate declarations.

## Steps

1. Run `rdra-ish check <model>` first. Pass a model's root directory, not a list of its
   files. It is the fast loop: one instance per entity, no `property`.
2. Decide whether `verify` is needed with `references/when-to-use.md`.
3. Run `rdra-ish verify <model>`. It needs no JVM. It also proves the state rules for any
   number of rows with z3 when `z3` is on `PATH`; without it a `note:` says the proof was
   skipped, and `--no-prove` turns the proof off. For a second opinion on a result that
   matters, add `--with-tlc` (`tlc` or `tlc2` on `PATH`, or `--tlc-jar <tla2tools.jar>`).
4. Read the run with `references/workflow.md`.
5. Report each failure by the name after its severity (`error[MultiForbidden_Order_0]`)
   with its counterexample. Paste the `bounds:` line, the `result:` line, and the last line
   exactly as printed, in a code block. `result: not decided` is open, not passed.

A passing run names its bounds first and last:

```text
$ rdra-ish verify samples/order.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
proof (Z3 version 5.1.0 - 64 bit): 2 state rules, for any number of rows, in the whole model
proved: Forbidden_Order_0 (for any number of rows, in the whole model)
proved: Inv_Order_0 (for any number of rows, in the whole model)
lemma: an absent Order row holds base values (for any number of rows, in the whole model)
lemma: no Order row is ever deleted (for any number of rows, in the whole model)
lemma: every present Order row is one of 5 reached patterns of (status, delivered_at) (for any number of rows, in the whole model)
lemma: a present Order row's (status, delivered_at) is one of {(pending, null), (paid, null), (shipped, null), (delivered, present), (cancelled, null)} (for any number of rows, in the whole model)
proof: 2 proved, 0 unproved, 0 out of scope
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
Cone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))
Cone_EventuallyTerminal [EventuallyTerminal]: not decided (31 states, Cone_EventuallyTerminal_WF: ok, needs the whole model's WF_vars(Next))
RdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)
RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)
result: ok
OK: no errors within 2 instances per entity, Int 0..5; 2 of 2 rules proved for any number of rows, in the whole model
```

| The question | Load |
|---|---|
| Is `check` enough, or do I need `verify`? | `references/when-to-use.md` |
| How do I write a `property`, `after.assert`, quantifier, or `.along` rule? | `references/dsl-surface.md` |
| What does this output mean? Which rules does a guard protect? | `references/workflow.md` |
| Does a rule hold for any number of rows? What is this CTI? | `references/prove.md` |
| What do the bounds leave out? Which sample shows X? | `references/limits-and-samples.md` |

## References

- `references/when-to-use.md`: `check` and `states` against `verify`, by concern.
- `references/dsl-surface.md`: `property` forms, `after.assert`, quantifiers, multi-entity
  rules, and their TLA+ forms.
- `references/workflow.md`: the commands, the spec files, reading a run, scoped runs, the
  memory limit, and `check --guards`.
- `references/prove.md`: the proof `verify` runs, reading a CTI, and writing a `.lemma()`.
- `references/limits-and-samples.md`: what the bounds approximate, and the bundled samples
  with their expected results.
- `samples/*.rdra`: standalone models, each with its expected result in `// expect` lines.
- `../rdra-ish-buc-analyze/references/state-patterns.md`: the lifecycle findings `check`
  and `verify` share (`Witness_*`, `Deadlock_*`, `Runs_*`, `Vacuous_*`).
