# Sequence and Access Views

Use this reference when reviewing screen/API/system boundaries, operation paths,
transaction boundaries, permissions, media, or actor grant coverage.

## Commands

```sh
# Sequence diagram — operations and transaction boundaries, whole model
rdra-ish diagram src/ --kind sequence --format mermaid

# Sequence diagram — scoped to a BUC
rdra-ish diagram src/ --kind sequence --buc <BucId> --format mermaid

# Sequence diagram — scoped to one or more use cases
rdra-ish diagram src/ --kind sequence --usecase <UseCaseId> --format mermaid

# API list and API × Entity CRUD matrix
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix

# Access/media review
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
```

## Reading Sequence Output

- Each use case block shows operations in order, including reads, writes, and screen
  returns.
- Participant lifelines are grouped by layer: system value, system boundary, and system.
- With `invokes`, the path renders as actor -> screen -> API -> entity; the API is the
  source of CRUD operations.
- Without `invokes`, legacy direct use-case CRUD renders through the `System` lane.
- Shaded `rect` blocks mark transaction groups. Notes distinguish `inferred from FK`
  from `API atomic boundary`.
- `Note right of ...: FK非連結` means entities are written outside a common FK chain;
  consider modeling the consistency boundary through an API.

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
- API diagnostics such as `ApiNeverInvoked` and `ApiInvokedButNoEntity` are printed to
  stderr when running `--kind sequence`.
- System diagnostics such as `CrossSystemEntityRelation`, `CoordinationMissingApi`,
  `CoordinationNotCrossSystem`, `ApiInMultipleSystems`, and `EntityInMultipleSystems`
  are printed by `check` and sequence output. Use them to verify `coordinates` and API
  calls.
