# Access and API Views

Use this reference when reviewing actor-entered fields, screen/API/system boundaries,
operation paths, transaction boundaries, API contracts, permissions, media, or actor
grant coverage.

## Commands

```sh
# Transaction, API, and system-boundary warnings
rdra-ish check src/

# Actor -> screen -> API -> entity paths of one BUC
rdra-ish diagram src/ --kind rdra --buc <BucId> --format mermaid

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

## Reading Transaction Warnings

- `check` groups each use case's writes the way `states` and `verify` run them: the
  writes of an invoked API form one atomic group, direct writes group by FK
  connectivity, and a write with no FK link runs alone (`SeparateTxInferred`). Consider
  modeling such a consistency boundary through an API.

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

- Use `--buc` for BUC-level review.
- Access and medium requirements may appear as graph relationships, but diagrams do
  not expand the full screen x UC/API constraint matrix.
- OpenAPI export requires API method/path plus DTO request/response/error links; do
  not treat a diagram as the payload contract.
- API diagnostics such as `ApiNeverInvoked` and `ApiInvokedButNoEntity` are printed to
  stderr by `check`.
- System diagnostics such as `CrossSystemEntityRelation`, `CoordinationMissingApi`,
  `CoordinationNotCrossSystem`, `ApiInMultipleSystems`, and `EntityInMultipleSystems`
  are printed by `check`. Use them to verify `coordinates` and API
  calls.
