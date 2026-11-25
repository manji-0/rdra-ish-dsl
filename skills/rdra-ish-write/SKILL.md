---
name: rdra-ish-write
description: Write RDRA DSL files from requirements using correct syntax and file structure, including staged abstract-to-concrete refinement, business flows, requirements/NFRs, screen fields, API contracts, data modeling annotations, ADR links, lint/format checks, and exports
license: MIT
---

## When to use

Write or extend `.rdra` files from requirements. Model one refinement stage at a time: a
guessed column, endpoint, or state machine reads as a decision to reviewers. To start or
change one BUC, skills `rdra-ish-buc-create` and `rdra-ish-buc-update` add their steps to
these.

## Steps

1. Find the model root and its stage. `rdra-ish next src/` prints each BUC's stage and the
   gaps that block the next one; `rdra-ish next src/ --buc <BucId> --format json` gives
   the next stage's `asks` and `gaps`.
2. Load the step file of that stage (table below). If the stage is unclear, read the
   previous, current, and next step files, classify, then keep one.
3. Ask only that stage's questions, unless the code already answers them.
4. Edit the fewest `.rdra` files. Place each declaration as `references/layout.md` says.
   Add `nfr`, `quality`, `constraint`, `goal`, and `adr` at any stage once the source gives
   them, scoped with `applies_to`, `qualifies`, `constrains`, `achieves`, or `decides`.
5. Run the step file's validation. Before handoff, also run `rdra-ish lint src/` and
   `rdra-ish fmt --check src/`.
6. Stop when the step's conditions hold, or list what is open. Paste the `bounds:` line
   and the last line of `check` exactly as printed, in a code block; never paraphrase them
   as "OK".

<!-- stage-spec: skill-table -->
| Stage | Concern | Main question | Add |
|---|---|---|---|
| 0 Scope sketch | Biz intent | What business area are we modeling? | `business`, rough `buc`, `belongs`, candidate `actor`, optional `requirement` and `motivates` |
| 1 BUC skeleton | Biz value | Who gets value from each BUC? | `performs`, `contains`, rough `usecase`, `flow` / `step` when order matters |
| 2 Screens | Biz actor boundary | What does the actor see and do in each use case? | `screen`, `displays`, `navigates`, optional `field` |
| 3 Data touchpoints | Biz object touchpoints | Which data objects does each use case touch, and which does each screen show? | coarse `entity`, CRUD predicates (`creates`, `reads`, `updates`, `deletes`), `shows`, optional `maps_field` |
| 4 API and access boundary | Tech system boundary | Which use cases work through an API, which system owns it, and which permission and medium does the work need? | optional `api` / `invokes`, `dto`, `system`, `permission`, `medium`, `has_permission`, `requires_*` |
| 5 Entity structure | Tech data design | What structure and ownership does the data need? | columns, `@pk`, `variation`, `relate`, cardinality, optional `owns` / `coordinates` |
| 6 Lifecycle | Tech lifecycle design | Which states and event-triggered BUC entries are reachable through the BUCs? | `Enum`, `Bool`, `@null`, `event`, `transitions`, `raises`, `triggers`, `timer`, `sets`, `condition` |
| 7A Local guardrails | Tech-enforced rules | Which simple states or facts must never co-occur? | `forbidden`, `exclusive` over Enum / Bool / nullable axes |
| 7B Local obligations | Tech-enforced rules | Which reachable states imply or require other facts? | `invariant`, narrow `required` |
| 7C Advanced rules | Tech-enforced rules | Which comparison or cross-entity facts are reviewable from the abstract state space? | comparison propositions, multi-entity `forbidden` / `invariant` |
<!-- /stage-spec -->

## References

- `references/00-scope.md`, `references/01-buc-skeleton.md`, `references/02-screens.md`,
  `references/03-data-touchpoints.md`, `references/04-api-access-boundary.md`,
  `references/05-entity-structure.md`, `references/06-lifecycle.md`,
  `references/07-rules.md`: one step file per stage (7A to 7C share `07-rules.md`).
- `references/layout.md`: files, where each declaration goes, who performs a use case.
- `references/syntax.md`: declarations, predicate signatures, common mistakes.
