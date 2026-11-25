---
name: rdra-ish-buc-create
description: Create a new BUC file from a requirements description, using staged refinement from BUC skeleton and business flow to data, UI/API contracts, lifecycle, rules, NFRs, and traceability
license: MIT
---

## Create a new BUC

### When to use

A requirement or feature description needs a new BUC file, and the shared declarations it
uses. Build only the stage the requirement supports: invented columns, endpoints, or state
machines read as decisions. Skill `rdra-ish-write` holds the stage table, the step file of
each stage, and the syntax.

### Steps

1. Classify the requirement by the stage it answers, with the stage table of
   `../rdra-ish-write/SKILL.md`. Ask only for what that stage adds.
2. List the business terms at that stage: actors and external systems, the business, the
   use cases and who performs each, the flow order, and, when given, screens, entities,
   APIs, permissions, NFRs, and lifecycle facts.
3. Split shared from BUC-local declarations with `../rdra-ish-write/references/layout.md`.
   Import what a shared file declares; never redeclare it.
4. Write `buc/buc_<name>.rdra` in this order: declarations, `belongs`, `contains`,
   `performs`, then one block per use case (CRUD, `displays`, `raises`). Add the later
   stages' predicates from their step files (`../rdra-ish-write/references/`).
5. Run `rdra-ish check src/` and fix every error, then run the validation of the stage's
   step file and `rdra-ish lint src/ --format table`. Paste the `bounds:` line and the
   last line of `check` exactly as printed, in a code block.
6. Hand off with the next stage's questions.

### Example

A BUC at the data-touchpoint stage, with the shared files it imports. Clerk cancels but
does not place orders, so Clerk performs CancelOrder, not the BUC.

```rdra
module shared.actors

actor Customer "Customer"
actor Clerk "Order Clerk"
```

```rdra
module shared.biz

business Commerce "Commerce"
```

```rdra
module shared.entities

entity Order "Order" {
  id: Int @pk
}
```

```rdra
module buc.order

import shared.actors
import shared.biz
import shared.entities

buc BucOrder "Process Order"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"

screen OrderScreen "Order Screen"

belongs(BucOrder, Commerce)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)

performs(Customer, PlaceOrder)
performs(Customer, CancelOrder)
performs(Clerk, CancelOrder)

creates(PlaceOrder, Order)
displays(PlaceOrder, OrderScreen)

updates(CancelOrder, Order)
displays(CancelOrder, OrderScreen)
```

### References

- `../rdra-ish-write/SKILL.md`: the stage table and the step files.
- `../rdra-ish-write/references/layout.md`: shared versus BUC-local, imports, performers.
- `../rdra-ish-write/references/syntax.md`: predicate signatures and common mistakes.
- Skill `rdra-ish-buc-update`: change the BUC once it exists.
