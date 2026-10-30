# CLI Reference

The `rdra-ish` command-line tool parses `.rdra` sources, type-checks them, runs
model-consistency checks, and produces diagrams, CSV, listings, exports, and
state-pattern derivations.

```
rdra-ish <SUBCOMMAND> <INPUTS...> [OPTIONS]
```

`<INPUTS...>` is one or more files and/or directories. Directories are searched
recursively for `.rdra` files. At least one input is required. When loading, the tool
merges all reachable `.rdra` files into a single semantic model and resolves imports
against include paths derived from the input layout (it walks up from each input so that,
e.g., `shared/actors.rdra` resolves from a root containing both `shared/` and `buc/`).

Diagnostics are written to stderr. A load or consistency diagnostic is `error: ...` or
`warning: ...`. A lifecycle finding carries its stable name: `error[<Name>]: ...`,
`warning[<Name>]: ...`, or `info[<Name>]: ...`. The names are listed in
[Diagnostics](./state-derivation.md#diagnostics).

- `check` and `lint` exit non-zero when semantic errors (or lint errors) are present.
- Every generator subcommand refuses to emit when the model has parse or resolve errors.
  Whether Error-severity state diagnostics also stop it depends on whether the output
  prints the lifecycle. An output that prints it (`states`, `list`, `diagram --kind state`,
  `csv` of every kind except `entity`, `export --kind typescript-states`) is
  **fail-closed** and refuses. An output that reads only the data layer or the declared
  facts (`diagram` of every kind except `state`, `csv --kind entity`, and `export` of
  `dbml`, `json-schema`, `openapi`, `asyncapi`, and `mermaid-er`) is written, and the
  command prints one stderr note that gives the error count and points to `rdra-ish check`.
  `export --kind tla` and `verify` gate on parse errors plus the Error
  diagnostics of the lifecycle transition system they print (built at `--instance-count`
  / `--int-range-max`). The code decides this per kind in `ExportKind::reads_lifecycle()`
  and `DiagramKind::reads_lifecycle()`.
- `fmt` only requires a parseable source; it does not run semantic analysis.

---

## `check`

Parse, type-check, and run whole-model consistency checks; produce no artifact output.

```
rdra-ish check <INPUTS...> [--strict] [--buc <ID>]... [--entity <ID>]...
```

Prints each diagnostic to stderr. If any error (non-warning) is present, exits with
status `1` before running secondary consistency checks. Otherwise it reports unresolved
model warnings and state-pattern findings. State diagnostics share severity with
`states`: Error-severity findings print as `error:` and cause exit `1`; only a clean
run prints `OK: no errors within <bounds>` (for example `OK: no errors within 1 instance per entity, Int 0..5`), or `OK: no errors in the selected scope, within <bounds>` under `--buc` or `--entity` (see [Partial runs](#partial-runs)). When the model has temporal `property` declarations,
`check` prints a trailing `note:` that those were not model-checked (use
`verify --backend tlc`).

A violated constraint or `after` assertion prints once, with a shortest counterexample. Sites (`file:line:col`) print as the resolved absolute path; the examples in this page show them shortened.

```
error[CrossForbidden_Order_0]: invariant CrossForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order(status=cancelled); Payment(status=captured)
  rule: cross_order_payment_split.rdra:43:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            cross_order_payment_split.rdra:16:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               cross_order_payment_split.rdra:17:1
         Payment#1.status: pending -> captured               cross_order_payment_split.rdra:38:1
    3. Cancel [Order#1]                                      cross_order_payment_split.rdra:18:1
         Order#1.status: pending -> cancelled                cross_order_payment_split.rdra:37:1
```

A step names the use case (`Pay_1, transaction 1 of Pay` for a split one), the instances
it binds, and the use case's declaration. Under it, `+ E#k` is a creation with the values
the check reads, `- E#k` a deletion, `now: a -> b` a clock tick, and `E#k.col: a -> b` a
change with the `transitions` arm or `sets` that wrote it. `start` lists the instances
present before the first step. The trace shows only what the rule's cone reads. When the
cone does not count an entity's slots and the trace creates more of them than the bound,
a `note:` says that the whole model may not reach that state at this bound.

A state of a state model (a `transitions` endpoint, or a value a creating use case sets)
that no reachable state holds is one `error:` with each arm that would enter it:

```
error[Witness_Order_status_refunded]: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it
  state model: Order.status, entered by Refund (shipped -> refunded)   unwitnessed_state.rdra:31:1
```

When an entity's cone stops at a cap, its unwitnessed states are not decided, and each
is listed under `not checked:` as `state E.col = v is not decided: ...`, naming the cap.
Such a state is neither an error nor a warning. See
[State models and witnesses](./state-derivation.md#state-models-and-witnesses).

A use case, or one transaction group of a split use case, that no edge of its entity cones
carries is one `warning:`. It names the use case's declaration and only what blocks it:
`needs:` for an instance no reachable state has, or for a free slot when its guards hold
only in states where the instance bound leaves none, `guard:` with its site for each guard
that holds in no reachable state, or, when each guard holds somewhere, a first line that
says they never hold together:

```
warning[Runs_Pay_2]: transaction 2 of Pay (Pay_2) can never run: in no reachable state do its guards hold
  declared: transaction 2 of Pay (Pay_2)   never_runs.rdra:5:1
  guard: condition Settled                 never_runs.rdra:20:1
```

A capped cone leaves such an action undecided, listed under `not checked:` as `whether
Pay_2 can run is not decided: ...`. See
[Use cases that never run](./state-derivation.md#use-cases-that-never-run).

An instance at a state with a way out that no path ever moves is one `error:`. It names each
way out (a `transitions` arm, or a deletion) with what blocks it, read off the states the
instance stays stuck in, and a shortest path into the first such state:

```
error[Deadlock_Order_status_paid]: deadlock: Order#1 stays at status = paid forever, and paid is not an end state
  leaves paid: Ship (paid -> shipped)   deadlock.rdra:29:1
    guard: condition Packed             deadlock.rdra:33:1
  reached by (2 steps):
    1. Place [Order#1]                     deadlock.rdra:12:1
         + Order#1 (status=pending)
    2. Pay [Order#1]                       deadlock.rdra:13:1
         Order#1.status: pending -> paid   deadlock.rdra:28:1
```

When a way out lacks only a free slot of an entity it creates, the instance bound blocks
it, not the model, and the line is `warning[Deadlock_<E>_<col>_<v>]: deadlock at the bound: ...`. A capped cone
leaves each state with a way out undecided, listed under `not checked:` as `whether some
E can get stuck at col = v is not decided: ...`. See [Deadlocks](./state-derivation.md#deadlocks).

| Option | Description |
|---|---|
| `--strict` | Promote permission / access consistency gaps (`ActorPermissionMissing`, no-actor paths, etc.) from warnings to errors for exit. |
| `--buc <id>` | Keep only the use cases reachable from the BUC; repeatable. See [Partial runs](#partial-runs). |
| `--entity <id>` | Check only the entity's view and the invariants and after-asserts that read it; repeatable. See [Partial runs](#partial-runs). |

### Partial runs

`check`, `states`, and `verify` take the same two flags. Each id is checked against the
model, and an unknown one is an error.

- `--buc <id>` keeps only the use cases reachable from the BUC, when the system is built.
  A run without the other use cases cannot rule out what they would reach, enable, or
  unblock. So a state with no witness, a use case that never runs, and a deadlock are each
  **not decided**: they are listed under `not checked:` and are neither errors nor warnings.
- `--entity <id>` selects targets: the entity's view, each invariant that binds the entity,
  and each after-assert whose anchor or predicate binds it. Each target is explored on its
  own cone, as in a whole run, so every finding on a selected target is exact. A use case
  that no selected entity's cone keeps is not decided.

A build error whose entities no selected cone reads is **deferred**. It is listed under
`not checked:` and does not fail the run. A build error that alters a use case (a guard
or an effect that the error dropped) also involves every entity that use case writes, so a
selection whose cones read that use case still fails on it. A build error that names no
entity always fails the run. A run without `--entity` reports every build error as an error,
so the whole model still fails:

```
$ rdra-ish check samples/formal-verification/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
scope: --entity Order
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
OK: no errors in the selected scope, within 1 instance per entity, Int 0..5
```

The `scope:` line follows the `bounds:` line and names each flag; for `--buc` it counts the
use cases outside the BUCs. The `not checked:` block lists the targets the scope leaves
out, the deferred build errors, and each finding a scope or a cap left undecided. A whole
run prints it only when a cap left something undecided. `verify` under a scope also lists
the temporal properties: a scoped run has no whole model to check them on.

<!-- derived-from ./state-derivation.md#diagnostics -->
`check` includes the same broad review signals exposed elsewhere:

- actor permission coverage for `requires_permission` on use cases and invoked APIs,
  including missing and excess actor-side assignments inferred from performer paths;
- API invocation and API entity-operation gaps;
- system boundary derivation and cross-system entity coordination gaps;
- FK/API transaction-boundary warnings;
- event link gaps, including events that are never raised, raised events that trigger
  no transition/use case/BUC, or triggered use cases with no containing BUC. Mark
  intentional external publications with `outbox(Event)` to suppress only the
  raised-but-unconsumed warning;
- state diagnostics (same severity as `states`), including missing creation paths,
  unreachable enum variants, a state of a state model with no reachability witness, an
  instance that can never leave a state (deadlock), a use case that never runs, a reachable
  state that breaks a `forbidden`, `invariant`,
  `required`, `exclusive`, multi-entity, or quantifier rule, a failed `after` assertion,
  a condition body that no use case can bind, and pattern truncation. See
  [Diagnostics](./state-derivation.md#diagnostics).

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to load. |

---

## `lint`

Audit model coverage and review readiness.

```
rdra-ish lint <INPUTS...> [--format <FORMAT>]
```

`lint` is review-oriented rather than a replacement for `check`. It includes
parse/model diagnostics and whole-model consistency warnings, then adds modeling
coverage findings such as orphan nodes, untraced requirements, unscoped NFRs or
constraints, empty BUCs/flows, unmapped screen fields, unused APIs/DTOs, incomplete
API method/path contracts, and naming convention warnings.

It also emits two `info` rows:

- `coverage-score` is a lightweight 0-100 readiness signal. Empty models score `0`.
  Finding penalties are scaled by model size (`×25/elements`); each model layer with no
  element (`scope`, `business-flow`, `data`, `interaction`, `system-boundary`, `rules`)
  subtracts 8 (floor 0).
- `stage-readiness` gives the stage of each BUC and the stage after it, as
  [`next`](#next) decides them: `BucOrder stage 2 (next 3); BucPayment stage 5 (next 6A)`.

These rules are `warning` rows:

- `flow-cycle` warns when `precedes` / `branches` edges form a cycle among steps.
- `enum-attribute` warns on an inline `Enum` column that no `transitions` and no `sets`
  moves. Its value is chosen by an actor, so the hint is to type the column by a
  [`variation`](./language-reference.md#variations).
- `rdra-*` rules are the RDRA definition sheet's 不整合 columns that lint enforces: an actor,
  external system, entity, condition, variation, or timer that no BUC reaches
  (`rdra-actor-no-buc`, `rdra-extsystem-no-buc`, `rdra-entity-no-buc`,
  `rdra-condition-no-buc`, `rdra-variation-no-buc`, `rdra-timer-no-buc`), and a use case
  named by `transitions` that no BUC contains (`rdra-transition-uc-no-buc`). They replace
  `orphan-node` for those kinds. The "undefined" columns are `check` errors. See
  [RDRA lint rules](./language-reference.md#rdra-lint-rules).
- `condition-no-rdra-anchor` warns on a [condition](./language-reference.md#conditions) whose
  `holds` body names neither a variation-typed column nor a lifecycle column.

Lint warnings do not fail the command. Existing parse/model errors are emitted as
`semantic-error` rows and make the command exit with status `1`.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `json` \| `csv` | `table` | Output format. |

---

## `next`

Show the incremental modeling stage of each BUC, and what the next stage asks for.

```
rdra-ish next <INPUTS...> [--buc <ID>]... [--format <FORMAT>]
```

| Option | Default | Description |
|---|---|---|
| `--buc <ID>` | every BUC | Show only these BUCs; repeatable. An unknown id is an error |
| `--format <FORMAT>` | `text` | `text` or `json` |

`next` places each BUC on the [stage map](./incremental-modeling.md#stage-map). Every
stage has a gate, and a BUC is at the highest stage whose gate it passes together with
every earlier gate:

<!-- stage-spec: gates -->
| Stage | The gate passes when | Review notes |
|---|---|---|
| 0 Scope sketch | the BUC `belongs` to a business |  |
| 1 BUC skeleton | an actor `performs` it or one of its use cases, or an event or timer `triggers` one; and it holds a use case (by `contains`, or through a flow's steps and `covers`), or an event or timer `triggers` the BUC itself |  |
| 2 Data touchpoints | the BUC holds a use case; and a use case creates, reads, updates, or deletes an entity, directly or through an API it `invokes` | once a use case touches data, each use case with no touchpoint is data-free on purpose |
| 3 Interaction boundary | every use case `displays` a screen or `invokes` an API, or an event or timer `triggers` it | when the model declares a `permission` or `medium`, each use case with no `requires_permission` / `requires_medium` on it or its APIs is open to every actor and device on purpose |
| 4 Entity structure | every touched entity has a column that is neither a primary key nor a foreign key | when the BUC touches two or more entities, each touched entity with no `relate` to another touched entity stands alone on purpose |
| 5 Lifecycle | a use case moves an Enum or Bool column, with `transitions` or with `sets` on it | a `sets` that only marks presence, an Int, or a comparison follows a lifecycle Enum or Bool |
| 6A Local guardrails (optional) | a `forbidden` or `exclusive` with no comparison reads a column this BUC moves |  |
| 6B Local obligations (optional) | an `invariant` or `required` with no comparison reads a column this BUC moves |  |
| 6C Advanced rules (optional) | a rule that holds a comparison or names more than one entity reads a column this BUC moves |  |
<!-- /stage-spec -->

A gate passing is evidence, not a verdict. The stage sections of
[Incremental Modeling Flow](./incremental-modeling.md) say when a stage is stable enough
to move on; `next` shows where the model stands, so you only need to ask the next stage's
questions.

The text output prints one block per BUC, after a summary table when there is more than
one: the gate of each stage with what it counted, then the first failed gate as the next
stage, with its main question, the elements that fail the gate (`Gaps:`, at most 8;
`--format json` lists every gap), the questions to ask, the commands that review the
stage once it is added, and its section in the modeling guide:

```text
BucStoreRestock "Maintain Store Restock": stage 2 Data touchpoints
  [x] 0  Scope sketch          belongs to StoreOperations
  [x] 1  BUC skeleton          2 use cases, 1 `performs` / `triggers` starting them
  [x] 2  Data touchpoints      2/2 use cases touch an entity
  [ ] 3  Interaction boundary  0/2 use cases display a screen, invoke an API, or run on a trigger
  ...

Next: stage 3 Interaction boundary
  What UI/API boundary mediates the work, and with which authority/media constraints?
  Gaps:
    - usecase ChangeNextRestockDate: add `displays(ChangeNextRestockDate, Screen)` or `invokes(ChangeNextRestockDate, Api)` for the boundary it works through
    ...
  Ask:
    - Which screen or external interface does each use case expose?
    ...
  Then review with:
    rdra-ish check src
    rdra-ish diagram src --kind rdra --buc BucStoreRestock
    ...
  See docs/incremental-modeling.md#stage-3-interaction-boundary
```

`--format json` writes `{"bucs": [...]}`. Each BUC has `id`, `label`, `stage` (`"2"`,
`"6A"`, or `null` before stage 0), `stage_name`, `gates` (`stage`, `stage_name`,
`passed`, `evidence`, and the gap count `gaps`), and `next`: `null` once every gate
passes, otherwise `stage`, `stage_name`, `question`, `doc`, `asks`, `validate`, and
`gaps` (`kind`, `id`, `message`).

`next` refuses a model with parse or model errors, as the generators do; run `check`
first. It exits 0 at every stage.

---

## `files`

Print the `.rdra` files the inputs load, one path per line, in load order.

```
rdra-ish files <INPUTS...>
```

Every loading command collects its inputs the same way: each file as given, then each
directory walked recursively with the entries of every directory sorted by file name. So
the order is the same on every platform. Exits `1` when the inputs hold no `.rdra` file.
`scripts/verify-samples.sh` reads a sample's `// expect` header from the first path it
prints (see [Sample expectations](./formal-verification.md#sample-expectations)).

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to list. |

---

## `fmt`

Format RDRA DSL source files.

```
rdra-ish fmt <INPUTS...> [--write | --check]
```

The formatter parses each `.rdra` file and prints a canonical source layout:
top-level items are separated by one blank line, instance metadata is indented by
two spaces, entity/DTO columns are indented by two spaces, annotations use compact
spacing, and predicate calls keep their chain form on one line.

This is an AST-based formatter. It preserves model structure, identifiers, labels,
metadata values, columns, predicates, chains, tuples, and comparison expressions,
but it does not preserve comments or original blank-line grouping. Prefer
`--check` before `--write` when adopting it on an existing hand-commented file.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to format. Directories are searched recursively for `.rdra` files. |
| `--write` | flag | off | Rewrite files in place. |
| `--check` | flag | off | Exit with status `1` and print `needs formatting:` rows when any file differs from the canonical format. |

Without `--write` or `--check`, formatted source is written to stdout. With multiple
input files, each output block is prefixed by a `// <path>` marker.

---

## `diagram`

Generate one RDRA diagram as Mermaid text (`.mmd`).

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [--buc <ID>]... [--usecase <ID>]... [--node-kind <KIND>]... [--edge-kind <KIND>]... [-o <OUT>]
```

The `.mmd` is written for a reader that redraws it, a person or a tool such as an AI that
draws SVG. It carries the RDRA meaning in text, under the [encoding
contract](#encoding-contract): a `%%` header, node ids `<kind>__<DSL id>` that are the same
in every kind, a `:::<kind>` tag on each node, the RDRA layer names, and the DSL predicate
names as edge labels. Shapes, colours, and direction carry no meaning, so a renderer is
free to replace them.

### Kinds

Each kind is one diagram of RDRA 3.0, or `rdra`, the base graph of every element and
relation, from which the RDRAGraph views can be derived.

| Kind | RDRA diagram | Draws | Leaves out | With `--buc` |
|---|---|---|---|---|
| `rdra` (default) | RDRAGraph, the whole graph | Every element in its layer and every relation; `--node-kind` / `--edge-kind` narrow it | ADRs; columns (`er`), variation values and condition bodies (`condition`) | Everything reachable from the BUCs, and the states their use cases move between |
| `context` | システムコンテキスト | The system, the actors who use it, and the external systems it exchanges events with; every edge derived | BUCs, use cases, and events, which `%% derived` and the edge labels name | The actors and external systems with an edge through the BUCs, and the systems they reach |
| `requirement` | 要求モデル | Requirements and the BUCs they `motivate`; NFRs and constraints with what they `apply_to` or `constrain`, and the qualities they belong to | ADRs; a requirement-to-actor link, which the DSL does not have | What links to the BUCs, their use cases and APIs, the entities those read and write, and the systems that contain those APIs |
| `business` | ビジネスコンテキスト, ビジネスユースケース | Each business holding its BUCs, and the actors and external systems each BUC involves | Use cases, steps, and events, which `%% derived` names | The named BUCs and their businesses (the RDRA BUC diagram) |
| `usecase` | UC複合図 | Each use case with its actors, screens and what they show, the entities it reads and writes, its events, external systems, and timers, the conditions on it, and its transitions | BUCs, steps, media, permissions, fields, APIs (their CRUD is folded into the use case), and DTOs | The use cases the BUCs contain, directly or through a step; `--usecase` names use cases instead |
| `flow` | 業務フロー | Each flow with its steps, their `precedes`, `branches`, `excepts`, and `repeats` edges, what each step `covers`, and a lane per actor | Actors without `performs(Actor, Step)`; screens and entities | The flows the BUCs contain |
| `information` | 情報モデル | Entities with their `relate` links, state models, and the variations their columns take | Columns (`er`) and use cases | The entities `er` draws for the BUCs |
| `condition` | バリエーション・条件 | Each condition with the variations and state models it combines and what it applies to; each variation with its values | Entities (a state model stands for its column) | The conditions on the BUCs' use cases and their APIs, and what those combine |
| `state` | 状態モデル | One composite state per state model, its states, the use case of each transition, start and end states | `Enum` columns that no `transitions` drive, named in `omitted:` | Each state model one of the BUCs' use cases moves, whole |
| `event` | システム間連携（イベント） | `raises` and `triggers` between use cases, BUCs, external systems, events, and timers | States and transitions (`state`) | The events reachable from the BUCs and their links |
| `er` | (RDRASpec logical data model) | Entities with their columns and `relate` cardinality, as an `erDiagram` | Variation values and state models, shown only as column types | The entities reachable from the BUCs |

`state` is the one kind that reads the lifecycle (for its start states). It refuses to
write while the lifecycle has errors; every other kind draws declared facts and writes
with one stderr note.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | one of the [kinds](#kinds) | `rdra` | The diagram kind. |
| `--format` | `mermaid` | `mermaid` | Output format. Mermaid text (`.mmd`) is the only one; to get an image, render the `.mmd` with `mmdc` or any Mermaid-aware tool, or hand it to a tool that redraws it. |
| `--buc <id>` | string (repeatable) | — (whole model) | Draw the part of the model the named BUCs reach, the union over every id. Applies to every kind; the [kinds](#kinds) table says what each one keeps. An unknown id is an error. |
| `--usecase <id>` | string (repeatable) | — (whole model) | Draw only the named use cases. Applies to `usecase` only, and not together with `--buc`; an unknown id is an error. |
| `--node-kind <kind>` | string (repeatable) | — (all nodes) | Draw only these node kinds. Applies to `rdra`. Examples: `actor`, `buc`, `flow`, `step`, `usecase`, `api`, `entity`, `field`. Alias: `--kind-filter`. |
| `--edge-kind <kind>` | string (repeatable) | — (all edges) | Draw only these relation kinds. Applies to `rdra`. Examples: `contains`, `invokes`, `reads`, `writes`, `creates`, `maps-field`, `owns`, `relate`. Alias: `--edge-filter`. |
| `-o`, `--out` | path | `out` | Output file path. The extension is always `.mmd`: a path without one gets it added (`-o out` writes `out.mmd`), and an existing one is replaced (`-o x.foo` writes `x.mmd`; `x.mmd` stays; `-o d.tar.gz` writes `d.tar.mmd`). |

Notes:

- Removed names fail with a message that names the replacement: `--kind
  boundaryless-graph`, `business-area`, `technical-area`, `sequence`, `diff`, and
  `event-flow`, and `--format puml`, `svg`, and `png`. `--show-description`,
  `--view-preset`, and `--diff-base` are rejected as unknown arguments; descriptions are
  always written as `%% desc` lines. The CHANGELOG lists each replacement.
- `--node-kind` and `--edge-kind` hide nodes and any edges whose endpoints are hidden;
  they do not change the model or diagnostics. The header's `omitted:` line names the
  node kinds and relation kinds they hid.
- `--kind er` takes only the contract's header: the body is the `erDiagram`, whose
  entity names stay the DSL ids (a `__` prefix would read as part of a column), and the
  diagram type says each node is an entity. `export --kind mermaid-er` writes the same
  text, its header without `model=`.

The notes below say how each kind reads, in the order of the table.

- For `--kind rdra`, model objects are placed into the four RDRA layers:
  システム価値 (`actor`, `extsystem`, `requirement`, `nfr`), システム外部環境
  (`quality`, `constraint`, `business`, `buc`, `flow`, `step`, `timer`, `medium`,
  `permission`), システム境界 (`usecase`, `screen`, `field`, `event`), and システム
  (`system`, `api`, `dto`, `entity`, `state`, `variation`, `condition`). An `adr` is not
  drawn. A `--buc` scope also holds the states its use cases move between.
- For `--kind context`, actors and external systems sit in システム価値 and the system in
  システム. Every edge is derived and followed by its `%% derived ... via <ids>` line: an
  actor `-.->|performs|` the system through each BUC, use case, or step it performs; an
  external system `-.->|raises <Event>|` the system when the event triggers one of its use
  cases or BUCs; and the system `-.->|triggers <Event>|` an external system when one of
  its use cases raises the event. With one declared system every edge ends there. With
  several, an edge ends at each system that contains an API the use cases behind it
  invoke. When no system is declared, or several are and none can be traced, the edge
  ends at `system__Model["システム"]`, followed by
  `%% synthesized system__Model: <reason>`; it is the one node no other kind draws.
  The whole model draws every actor, external system, and system; `--buc` draws those
  with an edge through the BUC's use cases, steps, and APIs.
- For `--kind requirement`, the nodes sit in the `rdra` layers. The edges are the
  declared `motivates` (requirement to BUC), `applies_to` (NFR to use case, API, or
  system), `constrains` (constraint to use case, API, system, entity, or DTO), and
  `qualifies` (NFR or constraint to quality). The whole model draws every requirement,
  NFR, quality, and constraint, linked or not; `--buc` draws those linked to the BUC, its
  use cases and their APIs, the entities those read and write, and the systems that
  contain those APIs, with the qualities of the NFRs and constraints it draws. No `rdra-ish`
  predicate links a requirement to the actor who holds it, so the diagram draws none.
- For `--kind business`, actors and external systems sit in システム価値, and each
  business is `subgraph business__<id>["<label>"]` inside システム外部環境, holding its
  BUCs. A BUC of no business, or of several, sits beside them, the latter with its
  `belongs` edges. Only `performs(Actor, Buc)` is declared; every other edge is derived
  and followed by its `%% derived` line: an actor performing one of the BUC's use cases
  or steps (`via usecase__<Uc>` or `via step__<Step>`), an external system raising an
  event that triggers the BUC or one of its use cases (labelled `raises <Event>`), and
  the BUC triggering an external system through an event one of its use cases raises
  (labelled `triggers <Event>`). `--buc` draws the named BUCs and their businesses.
- For `--kind usecase`, the use cases and what they touch sit in the `rdra` layers with
  the `rdra` ids, so a node matches its `rdra` node. An edge `rdra` does not draw is
  derived and followed by its `%% derived` line: an API's CRUD drawn from the use case
  that invokes it (`via api__<Api>`), and an actor drawn performing the use cases of the
  BUC or step it performs (`via buc__<Buc>` or `via step__<Step>`). A declared edge wins
  over a derived one. APIs, DTOs, permissions, media, fields, and BUCs are not drawn;
  `omitted:` names the ones linked to a drawn use case or screen.
- For `--kind flow`, the type line is `flowchart TD` and there are no layers. Each flow is
  `subgraph flow__<id>["<label>"]` holding its steps; the whole model also draws a step
  of no flow, outside any. The edges are the declared ones: `-->|precedes|`,
  `-->|branches|`, `-.->|excepts|`, and `-->|repeats|` between steps, and `-.->|covers|`
  from a step to the use cases, APIs, events, and timers it covers, drawn outside the
  flows. A step of a flow that exactly one actor performs sits in that actor's lane,
  `subgraph lane__<Flow>__<Actor>["<actor label>"]` inside the flow, whose first line is
  `%% lane lane__<Flow>__<Actor>: actor__<Actor>`. A lane is not an element, so its id
  is unique per flow; the `lane` kind appears in `kinds:`. A step several actors perform
  has a `-.->|performs|` edge from each. Without `performs(Actor, Step)` there are no
  lanes. `--buc` draws the flows the BUCs contain; a BUC without a flow draws an empty
  diagram, its header only.
- For `--kind information`, there are no layers. Each entity of the scope is drawn (the
  entities `--kind er` draws), each `relate` between two of them as
  `entity__<Child> ---|<card>| entity__<Parent>` with the DSL cardinality (the same
  relates as `er`, without a child-side minimum), each state model, a column that
  `transitions` drive, as `statemodel__<Entity>_<column>` (the composite state id of
  `--kind state`) with an `-.->|statemodel|` edge, and each variation a column takes
  with an `-.->|variation <column>|` edge. An `Enum` column no `transitions` drive is no
  state model. `omitted:` starts with `columns`, which `er` draws.
- For `--kind condition`, there are no layers. Each condition is drawn with a
  `%% holds <id>: <body>` line under it when it has a `holds` body, and an
  `-.->|combines|` edge to each variation and state model it combines
  (`statemodel__<Entity>_<column>`, the composite state id of `--kind state`). A link
  read from the `holds` body rather than a `combines` declaration is followed by
  `%% derived <from> -> <to> from holds`, as in `rdra`. `-.->|applies_to|` edges go to
  the use cases, APIs, and systems it applies to. Each variation is
  `subgraph variation__<id>["<label>"]` holding one
  `variation__<Variation>__<value>["<value label>"]:::value` node per value, its label
  the value's label or else its name. The whole model draws every variation, a
  condition's or not (the RDRA variation sheet); `--buc` draws the conditions that
  apply to a use case of the BUC (one `--kind usecase --buc` draws) or to an API such a
  use case invokes, and the variations and state models they combine.
- For `--kind state`, see the `stateDiagram-v2` part of the [encoding
  contract](#encoding-contract).
- For `--kind event`, the tool runs event-integrity diagnostics and emits `warning:`
  lines for events that are never raised, raised but consume nothing, or trigger a use
  case belonging to no BUC. `outbox(Event)` suppresses the raised-but-unconsumed warning
  for events intentionally published outside the local model. Event targets may be either
  BUCs, use cases, or external systems. Node ids follow the encoding contract
  (`event__`, `usecase__`, `buc__`, `extsystem__`, `timer__`), so they do not collide
  when model elements of different kinds share a DSL identifier. The diagram draws no
  states; `%% omitted: state transitions` says a drawn use case drives one, which
  `--kind state` shows.

### Encoding contract

Every kind but `state` and `er` is a `flowchart`, written so that every fact it draws can
be read back from the text. The `rdra` diagram shows each part:

```
%% rdra-ish diagram=rdra scope=buc:BucPayment model=samples/ec-site
%% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム
%% kinds: actor extsystem business buc usecase screen event api entity state
%% omitted: (none)
flowchart LR
  subgraph layer_value["システム価値"]
    actor__Customer["Customer"]:::actor
  end
  ...
  actor__Customer -.->|performs| buc__BucPayment
  buc__BucPayment -->|contains| usecase__Capture
  usecase__Capture -.->|transitions Order.status awaiting_capture->paid| state__Order_paid
  classDef actor stroke:#333;
```

- The header comes first. `scope` is `whole`, `buc:<id>[,<id>]`, or
  `usecase:<id>[,<id>]`; `model` is the input paths as given. `layers:` (in the kinds
  with RDRA layers: `rdra`, `context`, `requirement`, `business`, `usecase`) and `kinds:`
  list only what the body draws. `omitted:` names the node kinds linked to a drawn node
  that the kind leaves out, and in `rdra` the node and relation kinds a filter hid; it
  says `(none)` when there are none.
- Each node id is `<kind>__<DSL id>`, with the kind tokens of `--node-kind`, so one
  element has one id in every kind. The label is the DSL label, unchanged. A few ids name
  what is not a DSL element of its own:

  | Id | `:::` | Kind | What it is |
  |---|---|---|---|
  | `state__<Entity>_<variant>` | `state` | `rdra`, `usecase`, `state` | A state of a state model |
  | `statemodel__<Entity>_<column>` | `statemodel` | `information`, `condition`, `state` | A state model, a column that `transitions` drive |
  | `variation__<Variation>__<value>` | `value` | `condition` | One value of a variation, inside its `subgraph variation__<Variation>` |
  | `lane__<Flow>__<Actor>` | `lane` (a subgraph) | `flow` | The swimlane of one actor in one flow |
  | `system__Model` | `system` | `context` | The system when no declared system can stand for it |

- Each node carries `:::<kind>`, and one `classDef <kind>` per kind closes the diagram.
  The style is a placeholder Mermaid needs; it carries no meaning.
- Each layer is `subgraph layer_<token>["<RDRA name>"]`. A subgraph with an element id
  (`business__<id>`, `flow__<id>`, `variation__<id>`) is that element, holding its parts.
- Each edge label is the DSL predicate name; a relate edge is labelled with its
  cardinality (`N:1`). A transition is drawn from the use case to the state it enters,
  labelled `transitions <Entity>.<column> <from>-><to>`. The direction is from the
  predicate's first argument to its second.
- The line style is the RDRAGraph relation kind: `-->` for hierarchy (`contains`,
  `belongs`, `owns`) and flow order (`precedes`, `branches`, `repeats`), `---` for
  `relate`, and `-.->` for every other association.
- A fact drawn more than once in the model is drawn once.
- A `%%` line in the body says what the shapes cannot. A renderer ignores it:

  | Line | Follows | Kinds |
  |---|---|---|
  | `%% desc <id>: <text>` | A node with a `description`, its lines joined by a space | every flowchart |
  | `%% derived <from> -> <to> via <ids>` | An edge rdra-ish derived through the nodes it names | `business`, `usecase`, `context` |
  | `%% derived <from> -> <to> from holds` | A condition's `combines` link read from its `holds` body | `rdra`, `condition` |
  | `%% holds <id>: <body>` | A condition with a `holds` body | `condition` |
  | `%% lane <lane>: actor__<id>` | A lane subgraph's first line: the actor who performs its steps | `flow` |
  | `%% synthesized system__Model: <reason>` | The synthesized system node | `context` |

  An edge without a `%% derived` line is a declared relation, the same edge `rdra` draws.

`--kind state` is a `stateDiagram-v2`, where `classDef` and layers do not apply, so the
header and the id prefixes carry the kinds:

```
%% rdra-ish diagram=state scope=whole model=samples/ec-site
%% kinds: statemodel state usecase
%% omitted: Payment.status (Enum without transitions)
stateDiagram-v2
  state "Order.status" as statemodel__Order_status {
    [*] --> state__Order_pending
    state "pending" as state__Order_pending
    ...
    state__Order_awaiting_capture --> state__Order_paid : usecase__Capture
    state__Order_delivered --> [*]
  }
```

- Each state model, a column that `transitions` drive, is one composite state
  `statemodel__<Entity>_<column>` labelled `<Entity>.<column>`. Its states follow the
  column's `Enum` order, with the ids `--kind rdra` gives them.
- Each transition is labelled with the id of the use case that drives it.
- `[*] -->` marks the start state the lifecycle gives the column. `--> [*]` marks an end
  state, one with no transition to another state.
- `omitted:` names the `Enum` columns in scope that no `transitions` drive.
- `--buc` draws each state model that one of the BUC's use cases moves, whole.

---

## Sample Artifact Snapshots

`scripts/sample-artifacts.tsv` lists every committed `out/` file of `samples/ec-site`,
`samples/clinic-ops`, `samples/personal-info`, each `samples/incremental-order` step, and
`samples/api-contract`. Each row holds the output path and the `rdra-ish` command that
writes it. CI runs `scripts/check-sample-artifacts.sh` after the Rust snapshot tests. The
script builds the binary once and runs every row. It then rewrites the documents that
embed the artifacts: `scripts/embed-sample-artifacts.py` copies each file into the fenced
block under its `<!-- embed: <path> -->` line in the step `design.md` files and
`docs/diagram-sample-review.md`, and fails when a command those documents print is not
the manifest row for its `--out`; `scripts/gen-clinic-ops-design-sample.py` writes
`samples/clinic-ops/design-sample.md`. The script fails if a tracked file changes. A generator change
therefore shows up as a diff of Mermaid diagrams, CSV review tables,
state-pattern output, and exports such as OpenAPI, AsyncAPI, DBML, and JSON Schema. After
an intended change, run the script and commit the regenerated files.

To compare the data layer of every sample across two builds, run
`scripts/data-diff.sh LABEL` with each build. The script writes the DBML, JSON Schema,
and entity CSV of every `samples.txt` root, with each exit code and log, under
`target/data-diff/LABEL/`. It uses `target/release/rdra-ish` (override with `RDRA_BIN`).
Compare two runs with `diff -r target/data-diff/A target/data-diff/B`.

`scripts/verify-samples.sh <binary>` runs `check` and `verify` on every `samples.txt` root
and compares each run with the root's `// expect` header. CI runs it on the release binary.
The header grammar is in
[Sample expectations](./formal-verification.md#sample-expectations).

---

## `csv`

Generate CSV output.

```
rdra-ish csv <INPUTS...> [--kind <KIND>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `matrix` \| `api` \| `api-matrix` \| `screen-constraints` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `entity` | CSV kind. `actor` = actor list; `entity` = entity/column list; `matrix` = use-case × entity CRUD matrix; `api` = API contract list including method/path/idempotency/mode/auth; `api-matrix` = API × entity CRUD matrix; `screen-constraints` = screen × UC/API permission/medium paths; `permission-callables` = permission × callable UC/API list derived from `requires_permission`, including `UseCase->Api` paths for API-level requirements; `actor-permission-audit` = actor × permission assignment audit inferred from UC/API requirements; `business-inputs` = Actor x inferred input field x UseCase rows derived from C/U/W paths. |
| `-o`, `--out` | path | `out` | Output file path. A path with an extension is used as is (`-o x.dat` writes `x.dat`). Otherwise `.<kind>.csv` is appended to the path: `-o out` (the default) writes `out.matrix.csv` for `--kind matrix` and `out.entity.csv` for `--kind entity`, and `-o foo` writes `foo.matrix.csv`. |

The command writes the CSV to the output path and prints `wrote <path>`.

---

## `export`

Export machine-readable or review artifacts from the model.

```
rdra-ish export <INPUTS...> [--kind <KIND>] [-o <OUT>] [--instance-count <N>] [--int-range-max <N>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `openapi` \| `asyncapi` \| `dbml` \| `json-schema` \| `typescript-states` \| `mermaid-er` \| `tla` | `openapi` | Export kind. `openapi` emits an OpenAPI 3.0 JSON document from API `method`/`path` metadata and DTO `request` / `response` / `error_response` relations. `asyncapi` emits an AsyncAPI 3.1 event catalog from `event`, `raises`, `triggers`, `transitions`, and `outbox`. `dbml` emits a DBML schema from logical `entity` declarations, indexes, unique constraints, generated FK columns, and `relate` options. `json-schema` emits JSON Schema Draft 2020-12 definitions for DTO and Entity structures. `mermaid-er` emits a textual ER review artifact from the same logical data model projection used by `diagram --kind er`. `tla` emits a TLA+ module plus TLC config from entity lifecycles and state constraints (see [formal-verification.md](./formal-verification.md)). |
| `-o`, `--out` | path | `out` | Output file path. For every kind except `tla`, a path with an extension is used as is. Otherwise the kind's suffix is appended to the path: `-o out` (the default) writes `out.openapi.json`, `out.asyncapi.json`, `out.schema.dbml`, `out.json-schema.json`, `out.entity-states.ts` (`typescript-states`), or `out.er.mmd`, and `-o foo` writes `foo.openapi.json`. For `--kind tla`, a path ending in `.tla` is the TLA+ file, uses its file stem as the module name, and gets a sibling `.cfg` (`-o m.tla` writes `m.tla` and `m.cfg`). Any other path is a directory (created if missing) that receives `RdraSpec.tla` and `RdraSpec.cfg`: `-o out` (the default) writes `out/RdraSpec.tla`. A path with a non-`.tla` extension writes them into its parent directory. |
| `--instance-count` | unsigned integer | `2` | TLA+ only: multi-instance `InstanceCount` (`1..=255`). Value `1` is accepted with a warning (singleton ids cannot distinguish multi-entity instances). |
| `--int-range-max` | unsigned integer | `5` | TLA+ only: upper bound for `IntRange == 0..N` (`1..=255`; `0` is rejected). |

For OpenAPI export, only APIs that declare both `method` and `path` become
`paths` operations. Path segments `{name}` become required `parameters` (`in: path`,
`schema: string`). Known `auth` values `bearer` / `api_key` map to standard security
schemes; unknown schemes fall back to an `apiKey` header `X-Auth` with
`x-rdra-ish-auth-scheme` holding the raw value. `requires_permission(Api, Permission)`
is projected as operation extension `x-rdra-ish-requires-permission` (not OpenAPI
`security`, so it is not mistaken for a runtime scheme). DTOs are emitted under
`components.schemas`.

For AsyncAPI export, each RDRA `event` becomes a channel and message. `raises`
and `outbox` produce `send` operations; `triggers` produce `receive` operations,
which list the `transitions` of the triggered use cases. An external system that
`raises` an event is listed as the channel's `x-publisher`, and one that the event
`triggers` (is delivered to) as its `x-subscriber`; both keys appear only when non-empty.
RDRA does not yet model broker protocol, server topology,
or event payload DTOs directly, so those details are left unspecified and RDRA
facts are preserved with `x-rdra-ish-*` extensions.

For DBML export, each `entity` becomes a table with its Enum types (one per inline
Enum column, plus one per `variation` shared by every column typed by it), indexes, and
foreign-key references. Every `relate` key prints as a `Ref`: a `1:1` key as `-` on a
`[unique]` column, a composite key as `Ref: Stock.(bin_site, bin_shelf) >
Bin.(site, shelf)`. DBML does not have native equivalents for every review annotation, so `@check`,
`@soft_delete`, `@history`, `@tenant`, and `@derived` are preserved as column notes.

For JSON Schema export, DTOs and Entities are emitted under `$defs` as
`Dto.<id>` and `Entity.<id>` so cross-kind ids cannot collide. Standard structural
shape is represented with JSON Schema keywords; RDRA-specific metadata such as PK,
FK, indexes, checks, soft delete, history, tenant scope, and derived expressions is
preserved with `x-rdra-ish-*` extensions.

For Mermaid ER export, the output is intentionally the same text-level
review artifact as `diagram --kind er`. `plantuml-er` was removed and fails with a
message that names `mermaid-er`. Use `diagram` when choosing a diagram view during
interactive review; use `export` when a downstream tool or CI job expects a named
artifact kind.

The command writes the artifact to the output path and prints `wrote <path>`.

---

## `verify`

Export a TLA+ specification and run TLC formal verification. See
[formal-verification.md](./formal-verification.md).

```
rdra-ish verify <INPUTS...> [--backend tlc] [-o <OUT>] [--instance-count <N>] [--int-range-max <N>] [--buc <ID>]... [--entity <ID>]...
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--backend` | `tlc` | `tlc` | Verification backend. Requires `tlc` or `tlc2` on `PATH`. |
| `-o`, `--out` | path | temp dir, deleted when every check passes and kept when one fails | Directory for the specs: one `Cone_<check>.tla` / `.cfg` pair per cone, named after the first invariant or after-assert that reads it, one `RdraSpec_<Property>.tla` / `.cfg` pair per temporal property, `cones.txt` listing each module TLC runs and the checks it answers, and, when the model declares temporal properties, the whole-model `RdraSpec.tla` / `.cfg` with every `PROPERTY` for a run by hand (TLC does not run it). A path ending in `.tla` writes into its directory and names the whole-model module after its stem, and each property module `<stem>_<Property>`. |
| `--instance-count` | unsigned integer | `2` | Multi-instance `InstanceCount` (`1..=255`; `1` warns). |
| `--int-range-max` | unsigned integer | `5` | Upper bound for `IntRange == 0..N` (`1..=255`; `0` rejected). |
| `--buc <id>` | string (repeatable) | — (whole model) | Keep only the use cases reachable from the BUC. See [Partial runs](#partial-runs). |
| `--entity <id>` | string (repeatable) | — (all entities) | Check only the entity's view and the invariants and after-asserts that read it, with one TLC job per selected cone. See [Partial runs](#partial-runs). |

A `holds` body applied to a use case is a guard in the spec, so TLC checks it. `verify`
prints no notice for conditions.

Warnings and errors of the lifecycle transition system are printed on stderr, and an
error stops `verify` before TLC runs. Each invariant and `after(..).assert` is checked on
its own cone of influence, a restriction of the system whose reachable states are exactly
the whole model's projected onto it, with the cones run in parallel. Temporal properties
need fairness on the whole system, so they are checked on the whole model, and only when
the model declares any, each in its own run after the cone runs, at most two at a time:
TLC does not say which property a temporal counterexample violates. Stdout names the
bounds, the cone count, and each run's result. Stdout or
stderr carries one `tlc_result: ok|counterexample|tool_error` line. Exits non-zero when
TLC is missing or reports a violation.

Before TLC runs, `verify` explores each entity's cone with BFS at the same bounds, capped
at 2,097,152 states per cone; the `bounds:` line names the cap as `2097152 states per BFS
cone`. A state of a state model with no witness there, and a deadlock, print as in `check`
and fail the run even when TLC passes: stdout ends with `tlc_result: ok`, no `OK:` line
follows, and the exit status is 1. A deadlock at the bound and a use case that never runs
print as in `check`, as a `warning:` that does not fail the run. A cone that stops at the
cap lists each state, each deadlock, and each use case it leaves undecided under `not
checked:` on stdout. Then one summary line follows on stdout:

```
state models (BFS on 2 entity cones): 2 state models, 5 states witnessed, 0 deadlocks, 0 use cases that never run
```

The line counts the distinct entity cones, the columns with a state model, the states with
a witness, the deadlocks (errors and warnings at the bound), and the `NeverRuns` warnings.

A failed run ends with `specs: <dir>`, the directory that holds the specs. Without `-o`
it adds `(kept because a check failed)`. Each cone's TLC runs with `-continue`, so every
violated check of a cone prints its own counterexample.

Each violated invariant or `after` assertion of a cone prints once in model terms, through
the renderer `check` uses, before TLC's own `violated: Error: ... is violated.` line and
raw trace. `verify` decodes TLC's states, replays them through the step relation BFS uses
(the lowest binding when several give the same state), and builds the violation message
with the code `check` uses. Only the header says `from TLC`:

```
$ rdra-ish verify samples/formal-verification/cross_order_payment_split.rdra
...
Cone_CrossForbidden_Order_0 [CrossForbidden_Order_0]: counterexample (43 states)
tlc_result: counterexample
Error: TLC verification failed
Cone_CrossForbidden_Order_0 (/tmp/rdra-ish-tlc-4242/Cone_CrossForbidden_Order_0.tla):
error[CrossForbidden_Order_0]: invariant CrossForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order[1](status=cancelled); Payment[1](status=captured)
  rule: skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:43:1
  counterexample (3 steps, from TLC):
    1. Place [Order#1, Payment#1]                            skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:16:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:17:1
         Payment#1.status: pending -> captured               skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:38:1
    3. Cancel [Order#1]                                      skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:18:1
         Order#1.status: pending -> cancelled                skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:37:1
violated: Error: Invariant CrossForbidden_Order_0 is violated.
counterexample:
  State 1: Initial predicate
    Order_exists[1] = "absent"
    ...
specs: /tmp/rdra-ish-tlc-4242 (kept because a check failed)
```

If a TLC step matches no move of the cone, the engines disagree. That is a tool bug, and
`verify` prints `error: TLC step <n> (label <action>) matches no move of the cone; BFS and
TLC disagree (a tool bug). Raw TLC trace follows.` in place of the model-term block. The
raw trace still follows, the result stays `counterexample`, and the exit status is 1.

A violated temporal property prints in model terms the same way, as
`error[<Property>]: property <Property> is violated: ...`, from the run of that property
alone. The head names the instance and the step that shows the failure (for `leads_to`,
the step after which the trigger holds). TLC's lasso ends in a stutter or a step back to
an earlier state: the step back prints as the last numbered step, then `then loops back
to step K` (the behavior repeats steps K through the last one forever), and a stutter
ends with `then nothing changes forever`. A lasso that does not replay, or that does not
violate the property, prints the same engine-disagreement line. TLC's raw trace follows
with its `State N: Stuttering` or `Back to state K: <action>` line. An example is in
[formal-verification.md](./formal-verification.md#what-verify-checks).

| Environment variable | Effect |
|---|---|
| `RDRA_TLC_HEAP` | Heap of every TLC JVM, such as `4g`. Without it a cone gets `1g` and each temporal property run `2g`. |
| `JAVA_TOOL_OPTIONS` | Appended after the options `verify` sets (`-Xmx`, `-Djava.io.tmpdir`), so a `-Xmx` here wins. |

A run that runs out of memory is a `tool_error` whose message names both variables.

---

## `list`

List model elements in a human-readable form.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format <FORMAT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `requirement` \| `adr` \| `adr-impact` \| `nfr` \| `quality` \| `constraint` \| `buc` \| `flow` \| `step` \| `usecase` \| `field` \| `system` \| `api` \| `dto` \| `variation` \| `condition` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `actor` | The element kind to list. `actor` / `quality` / `buc` / `flow` / `step` / `system` / `variation` list id+label; `usecase` lists UC preconditions, guards, postconditions, alternatives, errors, and description; `requirement` lists requirement metadata; `adr` lists decision metadata plus impacted targets from `decides`; `adr-impact` lists one row per ADR-target pair; `nfr` and `constraint` list non-functional metadata; `field` lists screen-field metadata plus Entity.column mappings; `api` lists API contract metadata; `dto` lists DTO fields; `condition` lists the `holds` body, the variations and state models it combines (derived from the body, or from `combines` when there is none), and the targets of `applies_to`; `entity` lists each column with type, PK/unique/index/FK flags, FK optionality/actions, check constraints, soft-delete/history/tenant markers, and derived expressions; `permission-callables` lists each permission with callable use case/API ids and API-level `UseCase->Api` paths derived from `requires_permission`; `actor-permission-audit` lists inferred actor-side assignment gaps; `business-inputs` lists inferred actor-entered fields. |
| `--format` | `table` \| `json` \| `csv` | `table` | Output format. |

Output is written to stdout.

For `--format table`, an empty result is explicit rather than silent, for example
`No APIs found.`. CSV still prints only the header row and JSON prints `[]`.

---

## `states`

Derive the reachable state patterns per entity, aggregated across BUCs. See
[state-derivation.md](./state-derivation.md) for the algorithm.

```
rdra-ish states <INPUTS...> [--format <FORMAT>] [--buc <ID>]... [--max-patterns <N>] [--entity <ID>]...
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `csv` \| `json` \| `typescript` | `table` | Output format. `typescript` (alias `type-script`) emits entity state unions. |
| `--buc <id>` | string (repeatable) | — (whole model) | Keep only the use cases reachable from the named BUCs. See [Partial runs](#partial-runs). |
| `--max-patterns` | unsigned integer | `256` | Cap on each entity's distinct patterns within one target cone (an entity view, invariant, or after-assert is explored on the variables it reads). The cone also stops after 16 joint states per allowed pattern (4096 at the default). Only entities the target reads are bounded to one instance: a use case that also creates an entity outside the cone (an audit row, say) never runs out of slots for it. When a cap is hit, the result is marked `truncated` and a fail-closed `PatternCapReached` or `StateCapReached` **error** is recorded (exit non-zero for all formats). |
| `--entity <id>` | string (repeatable) | — (all entities) | Derive only the named entities' views, and the invariants and after-asserts that read them, each on its own cone. Every output format holds only those views. Exit severity considers the selected targets and the build errors their cones read; a build error outside them is deferred and listed under `not checked:`. See [Partial runs](#partial-runs). |

Output is written to stdout. Exit is based on structured severity (all formats), not
string-scanning `[error]`. stderr starts with a `bounds:` line: one instance per entity,
the Int range, the state cap per target cone, and that entities a check does not read are unbounded.
`check` prints the same line, and `verify` prints it on stdout with the TLA+ constants. Diagnostics that name no entity are printed
on stderr too, and so is the `not checked:` block, which lists each state of a state model,
each deadlock, and each use case the run does not decide (a capped cone, or `--buc`, which
leaves use cases out). With `--format json`, each pattern
carries a `witness`: a shortest path to the first state that holds it, as `start` and
`steps` objects with sites (see [Output formats](./state-derivation.md#output-formats)).

---

## See Also

- [language-reference.md](./language-reference.md) — the DSL syntax.
- [incremental-modeling.md](./incremental-modeling.md) — staged abstract-to-concrete modeling flow.
- [state-derivation.md](./state-derivation.md) — how `states` computes its output.
- [formal-verification.md](./formal-verification.md) — `export --kind tla` / `verify --backend tlc`.
