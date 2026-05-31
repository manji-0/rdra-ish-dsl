# Coverage Gap Analysis

Use this reference when the user asks what is missing or inconsistent in a BUC or the
whole model before getting into access, API boundaries, or lifecycle details.

## Concept

Coverage analysis asks whether the business story is connected enough to review:
actors perform BUCs or use cases, BUCs belong to a business domain, use cases belong to
BUCs, and use cases touch the business objects they are responsible for. Missing
coverage is often a modeling gap, but the severity depends on the current stage.

Good coverage does not mean the model is complete. It means the reviewer can follow
who does what, in which business area, and against which data objects.

## Commands

```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish list src/ --kind usecase --format table
rdra-ish list src/ --kind actor --format table
rdra-ish list src/ --kind entity --format json
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind rdra --format mermaid --buc <BucId>
```

## What To Look For

| Signal | Likely cause | Action |
|---|---|---|
| BUC has no `performs` path | Actor assignment is missing | Add `performs(Actor, Buc)` or `performs(Actor, UseCase)` when known |
| BUC has no `belongs` | Business domain is unclear | Add `business` and `belongs(Buc, Business)` |
| Use case has no `contains` | UC is orphaned | Attach it to the owning BUC or remove it |
| Use case has no CRUD predicate | Data touchpoint is missing or stage is still skeleton | Ask which entity the action touches |
| CRUD matrix has a row with all blanks | UC is declared but not connected to data | Add CRUD or defer with an explicit open question |
| CRUD matrix has overloaded rows | UC may combine multiple user-visible actions | Split UC or explain the transaction boundary |
| Entity appears in no CRUD matrix column | Entity may be premature or missing use cases | Add use-case touchpoints or remove the entity |
| BUC-local predicate appears in `shared/` | Ownership is blurred | Move the predicate to `buc/buc_<name>.rdra` |
| Module name does not match path | File layout drift | Fix `module` or move file |
| Same actor/entity/event is redeclared | Shared vocabulary duplication | Keep one declaration and import it |

## Directory Layout Checks

Small models usually start with:

```text
src/
  shared/
    actors.rdra
    biz.rdra
    entities.rdra
  buc/
    buc_<name>.rdra
```

Larger models may split shared files by responsibility, such as
`shared/entities/order.rdra`, `shared/lifecycle/order.rdra`, and `shared/rules.rdra`.
The important invariant is path/module correspondence and clear ownership.

## Severity Heuristics

- High: orphaned BUC/use case, type errors, duplicate definitions, or path/module
  mismatch blocking all analysis.
- Medium: missing `belongs`, missing actor assignment, empty CRUD row after the model has
  reached data touchpoints.
- Low: intentionally deferred screens, APIs, lifecycle, or rules in an early-stage BUC.

## Next Actions

- For missing actor/BUC links, ask who performs the business capability.
- For empty CRUD, ask which business object the use case creates, reads, updates, or
  deletes.
- For layout issues, propose the smallest file move or import change.
- For premature detail, recommend deferring it rather than inventing a deeper model.
