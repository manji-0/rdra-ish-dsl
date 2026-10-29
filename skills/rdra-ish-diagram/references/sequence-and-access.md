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

# Actor-entered fields per use case
rdra-ish csv src/ --kind business-inputs

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
  The actors of a use case are its direct performers plus the performers of every BUC
  that contains it.
- With `invokes`, the path renders as actor -> screen -> API -> entity; the API is the
  source of CRUD operations.
- Without `invokes`, legacy direct use-case CRUD renders through the `System` lane.
- Shaded `rect` blocks mark transaction groups. Notes distinguish `inferred from FK`
  from `API atomic boundary`.
- The writes and groups are the ones `states` and `verify` run: a `writes(..)` edge and an
  entity the use case only `sets` or transitions both draw as `update`.
- `Note right of ...: FK非連結` means entities are written outside a common FK chain;
  consider modeling the consistency boundary through an API.

## Reading Business Inputs and the API Matrix

- `business-inputs` lists actor -> input field -> use case rows. Rows come from modeled
  `field`/`maps_field` and inferred CRUD, so a missing row usually means no actor path,
  no C/U/W operation, no mapping, or a field modeled as derived.
- `api-matrix` shows which entities each API operates. Pair it with explicit `owns`
  diagnostics when checking CRUD coverage and intended system ownership.

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
