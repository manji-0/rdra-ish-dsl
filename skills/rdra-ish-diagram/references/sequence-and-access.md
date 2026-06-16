# Sequence and Access Views

Use this reference when reviewing actor-entered fields, screen/API/system boundaries,
operation paths, transaction boundaries, API contracts, permissions, media, or actor
grant coverage.

## Commands

```sh
# Sequence diagram — operations and transaction boundaries, whole model
rdra-ish diagram src/ --kind sequence --format mermaid

# Sequence diagram — scoped to a BUC
rdra-ish diagram src/ --kind sequence --buc <BucId> --format mermaid

# Sequence diagram — scoped to one or more use cases
rdra-ish diagram src/ --kind sequence --usecase <UseCaseId> --format mermaid

# Business area — actor-entered fields to use cases
rdra-ish diagram src/ --kind business-area --buc <BucId> --format mermaid
rdra-ish diagram src/ --kind business-area --usecase <UseCaseId> --format mermaid
rdra-ish csv src/ --kind business-inputs

# Technical area — systems containing APIs and operated entities
rdra-ish diagram src/ --kind technical-area --buc <BucId> --format mermaid

# API list and API × Entity CRUD matrix
rdra-ish list src/ --kind api --format table
rdra-ish list src/ --kind field --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish export src/ --kind openapi --out out/openapi.json

# Access/media review
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
```

## Reading Sequence Output

- Each use case block shows operations in order, including writes and screen returns;
  use CSV/list output for full field and contract audits.
- Participant lifelines are grouped by layer: system value, system boundary, and system.
- With `invokes`, the path renders as actor -> screen -> API -> entity; the API is the
  source of CRUD operations.
- Without `invokes`, legacy direct use-case CRUD renders through the `System` lane.
- Shaded `rect` blocks mark transaction groups. Notes distinguish `inferred from FK`
  from `API atomic boundary`.
- `Note right of ...: FK非連結` means entities are written outside a common FK chain;
  consider modeling the consistency boundary through an API.

## Reading Business/Technical Area Output

- `business-area` shows Actor -> input field -> UseCase paths. Input nodes come from
  modeled `field`/`maps_field` and inferred `business-inputs`, so missing rows usually
  mean no actor path, no C/U/W operation, no mapping, or fields are modeled as derived.
- `technical-area` shows each System as a container with only its APIs and the entities
  those APIs operate. Pair it with `api-matrix` and explicit `owns` diagnostics when
  checking CRUD coverage and intended ownership.

## Reading Access CSVs

- `screen-constraints` derives screen x use-case/API rows from `displays`,
  `invokes`, `requires_permission`, and `requires_medium`.
- `permission-callables` shows which use cases and APIs each permission enables.
- `actor-permission-audit` shows inferred actor x permission rows with `ok`,
  `missing`, or `excess`.

## Tips

- Use `--usecase` for concrete flow review and `--buc` for BUC-level review. Do not
  combine them.
- Access and medium requirements may appear as graph relationships, but sequence
  diagrams do not expand the full screen x UC/API constraint matrix.
- OpenAPI export requires API method/path plus DTO request/response/error links; do
  not treat a sequence diagram as the payload contract.
- API diagnostics such as `ApiNeverInvoked` and `ApiInvokedButNoEntity` are printed to
  stderr when running `--kind sequence`.
- System diagnostics such as `CrossSystemEntityRelation`, `CoordinationMissingApi`,
  `CoordinationNotCrossSystem`, `ApiInMultipleSystems`, and `EntityInMultipleSystems`
  are printed by `check` and sequence output. Use them to verify `coordinates` and API
  calls.
