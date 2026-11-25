---
name: rdra-ish-buc-update
description: Update an existing BUC by adding or modifying use cases, business flows, requirements, screens, fields, API contracts, NFRs, events, entities, ADR links, and predicates while preserving staged refinement
license: MIT
---

## Update an existing BUC

### When to use

A BUC file exists and a request adds, changes, or removes part of it. Keep the BUC at its
stage: when it is a skeleton, ask for the next stage's facts instead of adding columns,
APIs, or rules. Skill `rdra-ish-write` holds the stage table, the step files, and the
syntax.

### Steps

1. Read `buc/buc_<name>.rdra` and the shared files it imports: its use cases and
   performers (direct, or through `performs(Actor, Buc)`), entities, screens, events, and
   whether `shared/` is split.
2. Run `rdra-ish check src/` before editing and keep its findings, so you know which ones
   the change adds.
3. Run `rdra-ish next src/ --buc <BucId> --format json`. Its `next.asks` and `next.gaps`
   name what the next stage needs; add no more than one stage unless the user gave the
   facts.
4. Place each new declaration with `../rdra-ish-write/references/layout.md`, and write it
   as the step file of its stage shows (`../rdra-ish-write/references/`). Do not remove or
   rename a predicate unless the request deletes that behavior.
5. Run `rdra-ish check src/` and `rdra-ish lint src/ --format table`. A new
   `error[...]` or `warning[...]` is the change's: fix it. Paste the `bounds:` line and the
   last line of each `check` or `verify` run exactly as printed, in a code block (for
   `verify`, also `result:`).

### Adding a use case

```rdra
usecase ChangeAddress "Change Delivery Address"

contains(BucOrder, ChangeAddress)
performs(Customer, ChangeAddress)
updates(ChangeAddress, Order)
displays(ChangeAddress, OrderScreen)
```

If the BUC has `performs(Actor, Buc)`, that actor now performs the new use case and needs
its permissions. When that is wrong, move the actor to `performs` on the use cases it runs.

### Removing a use case

1. Delete the `usecase` and every predicate that names it: `contains`, `performs`, CRUD,
   `displays`, `navigates(...).via`, `invokes`, `raises`, `triggers`, `transitions`,
   `sets`, `applies_to`, `after`, `compensates`, `covers`.
2. Remove an `import` whose last use you deleted.
3. Run `check`: a state only that use case entered, which another arm still leaves, now
   fails as `Witness_*`.

### After the change

- Each new use case belongs to a BUC and has a performer, or an event or timer starts it.
- Each changing column has a `transitions` arm or a `sets`.
- A use case that writes two entities related only through a third also writes the one
  between, or `check` warns `UnjoinedWrite_<UC>_<A>_<B>`.
- For a review of the whole BUC, use skill `rdra-ish-review`.

### References

- `../rdra-ish-write/SKILL.md`: the stage table and the step files.
- `../rdra-ish-write/references/layout.md`: where each declaration goes.
- `../rdra-ish-write/references/syntax.md`: predicate signatures and common mistakes.
