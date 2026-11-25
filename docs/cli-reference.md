# CLI Reference

The `rdra-ish` command-line tool parses `.rdra` sources, type-checks them, runs
model-consistency checks, and produces diagrams, CSV, listings, exports, and
state-pattern derivations.

```
rdra-ish <SUBCOMMAND> <INPUTS...> [OPTIONS]
```

`rdra-ish --version` prints the version, such as `rdra-ish 2.3.0`.

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
  Whether an Error-severity lifecycle finding also stops it depends on what the output
  reads, as [Lifecycle errors](#lifecycle-errors) lists.
- `fmt` only requires a parseable source; it does not run semantic analysis.

---

## Lifecycle errors

An output that prints a reachability result is **fail-closed**: it refuses while `check`
reports a lifecycle error. An output that prints none is written, and the exit status is 0.
An output of declared facts or the data layer does not explore the lifecycle, so it prints
nothing about it: run `rdra-ish check` for the lifecycle errors. The TLA+ export explores
it and prints one stderr note that gives the error count and points to `rdra-ish check`.
`crates/rdra-ish-cli/src/lifecycle_gate.rs` classifies every `--kind` by what it reads, and
`crates/rdra-ish-cli/tests/export_gate.rs` runs every kind against this table.

| Output | Reads | On a lifecycle error |
|---|---|---|
| `list`, every kind | declared facts | writes |
| `csv`, every kind | declared facts and the data layer | writes |
| `diagram --kind state` | each state model's start states and reachable states | refuses, unless every error is a deadlock, which it colours |
| `diagram`, every other kind | declared facts, `sequence` included | writes |
| `export --kind tla` | the transition system, built at `--instance-count` / `--int-range-max` | writes, with a note that the whole-model `.cfg` checks the rules only and names the `StateModel_` specs, which check the deadlocks and witnesses with TLC, and `verify`; a build error of the system refuses |
| `export`, every other kind | the data layer | writes |
| `states` | the derivation, which is its output | prints the report with its `[error]` rows and exits 1 |

[`codegen`](#codegen) refuses (exit 1) while `check` reports an error, lifecycle errors
included, unless `--allow-failing` is given.

The generators that explore the lifecycle (`codegen`, `next`, `diagram --kind state`, and
`export --kind tla`) explore it at the caps `check` runs at by default, within `memory` under
`[check]` (see [Memory limit](#memory-limit)). A cone past that limit is the lifecycle error
`MemoryCapReached_<E>`: it refuses `codegen` and the state diagram, and is counted in the
note of the TLA+ export.

`verify` refuses on a build error of the transition system it checks and reports every
other finding itself.

---

## `check`

Parse, type-check, and run whole-model consistency checks; produce no artifact output.

```
rdra-ish check <INPUTS...> [--strict] [--deny-deprecated] [--max-patterns <N>] [--max-states <N>] [--memory <SIZE>] [--guards] [--code[=fresh|build|run]] [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>] [--buc <ID>]... [--entity <ID>]... [--without-usecase <ID>]... [--without-buc <ID>]... [--actor <ID>]...
```

Prints each diagnostic to stderr. If any error (non-warning) is present, exits with
status `1` before running secondary consistency checks. Otherwise it reports unresolved
model warnings and state-pattern findings. State diagnostics share severity with
`states`: Error-severity findings print as `error:` and cause exit `1`; only a clean
run prints `OK: no errors within <bounds>` (for example `OK: no errors within 1 instance per entity, Int 0..5`), or `OK: no errors in the selected scope, within <bounds>` under `--buc` or `--entity` (see [Partial runs](#partial-runs)). A failed run ends with a summary line on stdout instead: `FAIL: <E> errors, <W> warnings within <bounds>` (for example `FAIL: 1 error, 0 warnings within 1 instance per entity, Int 0..5`), with `in the selected scope,` under a scope, or `FAIL: <E> errors, <W> warnings; the model did not load, so nothing was explored` when a load error stops the run before exploration. Info findings are not counted. When the model has temporal `property` declarations,
`check` prints `note: <n> temporal properties were not model-checked by check; use verify`
on stderr before the `OK:` or `FAIL:` line.

`check` and `states` do safety verification by state exploration only: the rules, the
`after` assertions, and the state-model findings. They evaluate no `property`, whatever its
form, including `can_reach`, `stable`, `unchanged`, and `precedes`, which BFS could decide. Temporal
properties and inductive proofs (`verify --prove`) are `verify`'s.

A violated constraint or `after` assertion prints once, with a shortest counterexample. Sites (`file:line:col`) print as the resolved absolute path; the examples in this page show them shortened.

```
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order(status=cancelled); Payment(status=captured)
  rule: cross_order_payment_split.rdra:44:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            cross_order_payment_split.rdra:17:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               cross_order_payment_split.rdra:18:1
         Payment#1.status: pending -> captured               cross_order_payment_split.rdra:39:1
    3. Cancel [Order#1]                                      cross_order_payment_split.rdra:19:1
         Order#1.status: pending -> cancelled                cross_order_payment_split.rdra:38:1
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
| `--deny-deprecated` | Promote each `DeprecatedSyntax` warning (a [deprecated form](./language-reference.md#deprecated-and-removed-forms) that still loads) to an error, so `check` prints it as `error:` and exits `1`. Without the flag a deprecated form does not change the exit status. |
| `--max-patterns <n>` | Cap on each entity's distinct patterns within the cone of its entity view; a cone of other targets gathers no pattern, so the cap does not stop it. Default: 2^(10 + 8 * (n - 1)) for `n` = `--instance-count` (`1024` at one instance, `262144` at two). Past it, the run records a `PatternCapReached` **error**. Same as [`states`](#states). |
| `--max-states <n>` | Cap on the joint states of one target cone, printed in the `bounds:` line. Default: none; `--memory` stops a run that outgrows the machine, and the `bounds:` line says `no state cap per cone, memory 4GiB`. Past a cap you give, the run records a `StateCapReached` **error** that names what the cone's states vary in. A larger `--int-range-max` or `--instance-count` makes more states per cone, so it reaches the cap sooner. Same as [`states`](#states). |
| `--memory <size>` | What the run may hold in memory: the states of the cones it explores at once. Default: `memory` under `[check]` in [`rdra.toml`](#project-manifest), else `4GiB`. A cone whose states pass it stops: the run records a `MemoryCapReached` **error** that names the limit, says how many states it held and what each took, and estimates what finishing needs; the findings the cone leaves are listed under `not checked:`. See [Memory limit](#memory-limit). Same as [`states`](#states). |
| `--guards` | After a clean run, print on stdout which rules each guard protects. See [Guard dependencies](#guard-dependencies). It adds no finding and does not change the exit status. |
| `--code[=<level>]` | After a clean run, check the service code the [`[code]` tables](#the-code-tables) of `rdra.toml` name: `fresh` (the default), `build` or `run`. Write the level with `=`. See [`check --code`](#check---code). |
| `--instance-count <n>` | Instances (slots) per entity, `1..=255` (default `1`). A violation that needs two rows of an entity (a uniqueness or a quantifier across rows) shows only from `2`. The `bounds:` line and the final `OK:` line print the value used. Same name and validation as [`verify`](#verify); the default differs (`verify` runs `2`). A larger value also raises the default `--max-patterns`. |
| `--instances <entity>=<n>` | One entity's slots in place of `--instance-count`, `1..=255`, as `Loan=4`; repeatable. Default: `instances` under `[check]` in [`rdra.toml`](#project-manifest). See [Per-entity instance counts](#per-entity-instance-counts). |
| `--int-range-max <n>` | The Int domain is `0..=n`, `1..=255` (default `5`). Printed in the `bounds:` line. Same as [`verify`](#verify). |
| `--buc <id>` | Keep only the use cases reachable from the BUC; repeatable. See [Partial runs](#partial-runs). |
| `--entity <id>` | Check only the entity's view and the invariants and after-asserts that read it; repeatable. See [Partial runs](#partial-runs). |
| `--without-usecase <id>` | Assume the use case never runs and leave it out; repeatable. See [Partial runs](#partial-runs). |
| `--without-buc <id>` | Assume the use cases the BUC reaches, and no other BUC does, never run; repeatable. See [Partial runs](#partial-runs). |
| `--actor <id>` | Keep only the use cases the actor performs and those no actor performs; repeatable. See [Partial runs](#partial-runs). |

### `check --code`
<!-- derived-from ./design/code-check.md -->

`check --code` asks whether the service code has what the model requires. Experimental, as
`codegen` is. rdra-ish parses none of the service code: it generates every file the
[`[code]` tables](#the-code-tables) name again and compares, and at `build` and `run` it
reads what the language's compiler reports. It runs after the model check; on a model that
fails `check` it checks nothing (a note says so) and the exit status is the model check's.

| Level | What it checks |
|---|---|
| `fresh` | Each file `codegen --code` writes is what the model generates now: the domain code by its text, as [`codegen --check`](#codegen) compares it, and the traces by hash. A missing file is an error. No compiler runs. |
| `build` | `fresh`, then the language's build command on the probe, with the errors mapped back to the model's use cases. |
| `run` | `build`, then the conformance test. |

`--code` with no level is `fresh`. Only `build` and `run` start a command, the language's
default or the `build` / `run` of its table; the LSP never does.

Each finding is one block on stderr, under the cause lines of `codegen --check`:

```
error: ../svc/src/domain.rs is not what `codegen` generates from the model
  model changed: its source text is not the text ../svc/src/domain.rs was generated from
  1 line(s) only in ../svc/src/domain.rs, 1 only in the generated code
  ...
  generate them again: rdra-ish codegen model --code
```

The last line on stdout, after the model check's `OK:` line, is
`OK: code: what the model requires, at --code=<level>`, or
`FAIL: code: <n> error(s) at --code=<level>` with exit status `1`.

At `build`, each compiler error goes back to a use case: by the probe line it is on (the
probe has one line per use case), or by the generated method or contract it names. In Rust
the build is `cargo check --test <probe> --message-format=json` in the table's `root`, and
`run` is `cargo test --test <probe> -- rdra_conformance`. One block per use case and kind:

```
error[CodeMissingUseCase_Ship]: use case Ship has no implementation in svc::service::Service
  model: model/order.rdra:22:1  usecase Ship "Ship"
  code:  src/service.rs:70:26  E0277: the trait bound `Service: ShipUseCase` is not satisfied
  fix:   impl ShipUseCase for svc::service::Service (prepare_ship, commit_ship)
```

| Name | When |
|---|---|
| `CodeMissingUseCase_<UseCase>` | The service does not implement the use case's contract (Rust E0277). |
| `CodeIncomplete_<UseCase>` | It implements the contract without some of its methods (E0046). |
| `CodeSignature_<UseCase>` | A method takes or returns other types than the contract's (E0053, E0050). |
| `CodeError_<UseCase>` | Another error on the use case's probe line or naming its methods. |
| `CodeUnmapped` | The count of the errors that belong to no use case, the first one quoted. |
| `CodeBuildFailed` | The build failed with no error rdra-ish reads. |
| `CodeConformance` | `run`: the conformance test failed; the message is the harness's first difference. |

At every level, `check --code` also checks the **obligation ledger**: each obligation the
generated code lists (`Unique_Payment_Order`, `Ref_Order_customer_id`, ...) needs an owner.
An owner is either a marker line in the service code,
`rdra: obligation <name> [file]` after any comment token (`//`, `#`, `--`), or an entry
`<name> = "<file>"` under `[code.obligations]`. A marker's file is relative to the root
the marker was found under; without one, the owner is the marker's own file. The check sees
only the declaration, so it says an obligation **has an owner**, never that it holds. It
confirms that the owning file exists.

| Name | When |
|---|---|
| `CodeObligationUnowned_<name>` | The obligation has no owner. Error. |
| `CodeObligationOwnerMissing_<name>` | Its owner names a file that does not exist. Error. |
| `CodeObligationUnknown_<name>` | An owner names an obligation the model does not have (renamed or gone). Warning. |

```
error[CodeObligationUnowned_Unique_Order_note]: no code owns the obligation Unique_Order_note
  model: model/order.rdra:6:1  no two Order rows share note.
  fix:   add `// rdra: obligation Unique_Order_note <file that keeps it>` or list it under [code.obligations]
```

The ledger is kept once there is a marker or a `[code.obligations]` entry. Until then,
`check --code` prints a note and diagnoses no obligation. Markers are searched under each
table's `root`, skipping hidden directories, `target`, `node_modules`, `vendor`, `dist`,
the generated files, and files over 1 MiB.

`--code` on a model without a `[code.rust]`, `[code.go]` or `[code.ts]` table exits `1`, so a
check that would check nothing never passes. `check` without `--code` on a model that has one
prints one line, `note: rdra.toml has [code]; `check --code` also checks the service code
against the model`, on stderr, and otherwise does what it did before.

### Partial runs

`check`, `states`, and `verify` take the same flags. Each id is checked against the
model, and an unknown one is an error.

- `--buc <id>` keeps only the use cases reachable from the BUC, when the system is built.
  A run without the other use cases cannot rule out what they would reach, enable, or
  unblock. So a state with no witness, a use case that never runs, and a deadlock are each
  **not decided**: they are listed under `not checked:` and are neither errors nor warnings.
- `--entity <id>` selects targets: the entity's view, each invariant that binds the entity,
  and each after-assert whose anchor or predicate binds it. Each target is explored on its
  own cone, as in a whole run, so every finding on a selected target is exact. A use case
  that no selected entity's cone keeps is not decided. One such use case gets its own line;
  two or more get one line that counts them and names the first three (`whether 39 use cases
  (A, B, C, ...) can run is not decided: ...`), and the `findings not decided` count still
  includes each.
- `--without-usecase <id>`, `--without-buc <id>`, and `--actor <id>` assume that some use
  cases never run and leave them out of the system. `--without-usecase` names one use
  case; `--without-buc` names the use cases the BUC reaches that no other BUC reaches;
  `--actor` keeps the use cases the actor performs and those no actor performs (started by
  an event or a timer, or with no `performs`), and leaves out every other one. Unlike
  `--buc`, the run decides witnesses, deadlocks, and use cases that never run under that
  assumption, so a deadlock that another actor's use case hides is an error here. For
  example, `check --actor Staff` finds the state a staff member cannot leave by their own
  use cases, though an operator's deletion leaves every state. The `scope:` line names the
  use cases left out.

A build error whose entities no selected cone reads is **deferred**. It is listed under
`not checked:` and does not fail the run. A build error that alters a use case (a guard
or an effect that the error dropped) also involves every entity that use case writes, so a
selection whose cones read that use case still fails on it. A build error that names no
entity always fails the run. A run without `--entity` reports every build error as an error,
so the whole model still fails:

```
$ rdra-ish check samples/formal-verification/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
scope: --entity Order
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
OK: no errors in the selected scope, within 1 instance per entity, Int 0..5
```

The `scope:` line follows the `bounds:` line and names each flag; for `--buc` it counts the
use cases outside the BUCs, and for `--without-usecase`, `--without-buc`, and `--actor` it
lists the use cases assumed not to run. The `not checked:` block lists the targets the scope leaves
out, the deferred build errors, and each finding a scope or a cap left undecided. A whole
run prints it only when a cap left something undecided. `verify` under a scope also lists
the temporal properties, `can_reach` ones included: a scoped run checks none.

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
  `required`, `exclusive`, multi-entity, quantifier, or `@unique` (`Unique_*`) rule, a
  failed `after` assertion, a rule or `after` assertion that holds vacuously
  (`Vacuous_<name>`), a condition body that no use case can bind, and a cone stopped at a
  cap or the memory limit. See
  [Diagnostics](./state-derivation.md#diagnostics).

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to load. |

### Guard dependencies

`check --guards` reports which rules each guard protects. A guard here is a condition the
model applies to a use case: a `holds` condition that `applies_to` it, or the `.when`
conditions on one column's `transitions` arms, `chooses` values, or `sets` values. For each
guard, the run removes it from every action of the use case (each transaction group and
each `<UC>_by_<Ev>` copy), explores again the cone of each rule (`forbidden`, `invariant`,
the other constraints, and `after` assertions) that keeps the use case, and lists the rules
that now fail. Triggers, `transitions` sources, keys, and links are not removed. A
condition with no `holds` body, or one applied to something other than a use case, puts no
guard on the lifecycle and is not listed. Removing the `.when` conditions of a column opens
every arm or value of it to the use case at once; that is not the same as deleting one
`.when(...)` from the source, which can leave two unguarded `sets` of one column that
conflict.

```
$ rdra-ish check crates/rdra-ish-cli/tests/fixtures/guards/unprotected.rdra --guards
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
guards: 2 guards: 1 protect a rule, 1 no rule depends on; 2 cones explored again
guard: condition PaidOnly on Ship   unprotected.rdra:28:1
  protects: 「A shipped order was paid」
guard: condition StockChecked on Ship   unprotected.rdra:32:1
  no rule depends on this guard; it is either a business rule nobody wrote as a rule, or not needed
rules:
  「A shipped order was paid」 depends on condition PaidOnly on Ship
OK: no errors within 1 instance per entity, Int 0..5
```

The report is on stdout, before the final `OK:` line. Each `guard:` line names the guard
and its site (the `holds` body, or the first guarded arm or value), then one line per rule
it protects, named by its `.label` or else by its check name. The `rules:` block turns the
table around: each rule with the guards it depends on, or `no guard`.

A guard no rule depends on is not wrong. Either it states a business rule that nobody
wrote as a `forbidden` or `invariant`, so `check` cannot tell when it is lost, or the
model does not need it. A guard that backs up another guard or a trigger lands here too:
removing one of the two keeps the rule.

The rule is `not decided` without a guard when, without the guard, its cone stops at
`--max-states` or `--memory` before a counterexample. Such a guard is
neither protecting nor unprotected, and the `rules:` line names it after `not decided
without`.

The run explores one set of cones per guard, one guard after another, under the same
caps and memory limit as `check`. Only the rule cones that keep the use case run again,
and rules that share a cone share a run. Under a scope (`--entity`, `--buc`, ...) only the
selected rules are explored. A model that fails `check` gets no report, only a `note:` to
fix the errors first. Temporal properties are `verify`'s and are not counted, so a guard
that serves only a `property` is one no rule depends on. The report is text only; it has
no `--format json`. The order is the build's order of the use
cases and their guards, so two runs print the same report.

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

It also emits these `info` rows:

- `coverage-score` is a lightweight 0-100 readiness signal. Empty models score `0`.
  Finding penalties are scaled by model size (`×25/elements`); each model layer with no
  element (`scope`, `business-flow`, `data`, `interaction`, `system-boundary`, `rules`)
  subtracts 8 (floor 0).
- `stage-readiness` gives the stage of each BUC and the stage after it, as
  [`next`](#next) decides them: `BucOrder stage 2 (next 3); BucPayment stage 6 (required
  stages complete)`.
- `requirement-no-goal` marks a `requirement` that achieves no `goal`, unless its status is
  `out-of-scope`. The hint is to add `achieves(Requirement, Goal)`, or to mark a non-goal
  `status "out-of-scope"` with a `rationale`.
- `goal-unmeasured` marks a `goal` with no `metric` or no `target`.
- `screen-no-transition` marks a `screen` that no `navigates` enters or leaves and that no
  business flow reaches in order, so it stands alone in `diagram --kind screen`. The hint is
  to add `navigates(FromScreen, ToScreen).via(UseCase)`.
- `rule-unlabeled` marks a `forbidden`, `invariant`, `required`, `exclusive`,
  `invariant(...).when(...).none/has(...)`, or `after(...).assert(...)` with no
  [`.label("...")`](./language-reference.md#labelling-a-rule), so `list --kind rule` shows it
  with an empty label. It is not shown in the editor.
- `triggered-usecase-unguarded` marks a use case that only an event or a timer starts (no
  `performs` names it), that writes something, and that no condition guards and no
  `transitions` gives a from-state. The event orders the use case after its raise, but the
  record of the raise is a latch, so once the event was raised on a row `check` and `verify`
  run the use case there in every state its guards allow, as often as they allow. The hint
  is to add the state the event follows as a `holds` body applied to the use case, or as a
  `transitions` arm.
- `condition-applied-without-body` marks a condition with no `holds` body and no `combines`
  that `applies_to` a use case or an API. It guards nothing and links no variation or state
  model: `check` and `verify` run the use case whether or not it holds. The hint is to write
  the body when it should guard the use case. A condition that `combines` is an RDRA trace
  link and is not marked.

These rules are `warning` rows:

- `flow-cycle` warns when `precedes` / `branches` edges form a cycle among steps.
- `enum-attribute` warns on an inline `Enum` column that no `transitions` and no `sets`
  moves. Its value is chosen by an actor, so the hint is to type the column by a
  [`variation`](./language-reference.md#variations).
- `field-column-unwritten` warns on a screen field mapped by `maps_field` to a `@null` column
  that no `sets` and no `chooses` gives a value. `states` keeps the column at `null`, so the
  field shows a value the model never produces. The hint is to add `sets(UseCase,
  Entity.column == present)`.
- `rdra-*` rules are the RDRA definition sheet's 不整合 columns that lint enforces: an actor,
  external system, entity, condition, variation, or timer that no BUC reaches
  (`rdra-actor-no-buc`, `rdra-extsystem-no-buc`, `rdra-entity-no-buc`,
  `rdra-condition-no-buc`, `rdra-variation-no-buc`, `rdra-timer-no-buc`), and a use case
  named by `transitions` that no BUC contains (`rdra-transition-uc-no-buc`). They replace
  `orphan-node` for those kinds. The "undefined" columns are `check` errors. See
  [RDRA lint rules](./language-reference.md#rdra-lint-rules).
- `condition-no-rdra-anchor` warns on a [condition](./language-reference.md#conditions) whose
  `holds` body names neither a variation-typed column nor a lifecycle column and guards no use
  case. A body that `applies_to` a use case or an API, or that a `transitions` arm or a
  `chooses` value names in `.when`, is a guard and is not flagged.

- `comparison-unsatisfiable-in-range` warns on a comparison of an Int, Money, or Decimal
  column with a literal that no value in `0..5`, the Int range `check` and `verify` explore,
  satisfies, such as `qty > 5` or `qty < 0`, in a rule or a `holds` body. What it guards or
  forbids never applies there.
- `comparison-trivial-in-range` warns on such a comparison that every value in `0..5`
  satisfies, such as `qty >= 0` or `qty <= 5`, on a column that is not `@null`. It constrains
  nothing there. The hint of both is to compare with a value inside the range, or to drop the
  comparison.

- `deprecated-syntax` warns on a
  [deprecated form](./language-reference.md#deprecated-and-removed-forms): the model loads
  with the form rewritten, and the message names the rewrite and the version that removes
  it. `fmt --write` rewrites it.

Lint warnings do not fail the command. Existing parse/model errors are emitted as
`semantic-error` rows and make the command exit with status `1`.

Every rule `lint` reports, by name:

| Rule | Severity | Marks |
|---|---|---|
| `api-contract-incomplete` | warning | An API that declares only one of `method` and `path`. |
| `api-contract-missing` | warning | An API with neither `method` nor `path`; the OpenAPI export omits it. |
| `api-unused` | warning | An API no use case `invokes`. |
| `buc-empty` | warning | A BUC that contains no use case; `info` while the BUC has not reached the skeleton stage. |
| `comparison-trivial-in-range` | warning | See above. |
| `comparison-unsatisfiable-in-range` | warning | See above. |
| `condition-applied-without-body` | info | See above. |
| `condition-no-rdra-anchor` | warning | See above. |
| `consistency-warning` | warning | A whole-model consistency diagnostic of `check`, such as a missing relation or metadata. |
| `constraint-unscoped` | warning | A `constraint` that `constrains` no use case, API, system, entity, or DTO. |
| `coverage-score` | info | See above. |
| `deprecated-syntax` | warning | See above. |
| `dto-unused` | warning | A DTO that no API names in `request`, `response`, or `error_response`. |
| `end-redundant` | warning | An `@end` value that no `transitions` arm leaves, which is an end state without `@end`. |
| `enum-attribute` | warning | See above. |
| `field-column-unwritten` | warning | See above. |
| `field-unmapped` | warning | A screen field that no `maps_field` maps to an entity column. |
| `field-unplaced` | warning | A screen field that no screen `contains`. |
| `flow-cycle` | warning | See above. |
| `flow-empty` | warning | A business flow that contains no step. |
| `goal-unmeasured` | info | See above. |
| `multi-rule-unscoped` | warning | A multi-entity `forbidden`, `invariant` or `property` with no `.along(...)` and no `.global()`, when keys relate its entities; the message names the `.along(...)` the shortest key path gives. |
| `naming-column` | warning | An entity column that is not snake_case. |
| `naming-dto-field` | warning | A DTO field that is not snake_case. |
| `naming-id` | warning | An element id that is not UpperCamelCase. |
| `nfr-unscoped` | warning | An `nfr` that `applies_to` no use case, API, or system. |
| `orphan-node` | warning | An element no predicate connects to another; the `rdra-*` rules replace it for the kinds they cover. |
| `query-index-missing` | warning | A lookup the model implies that no index serves; skipped for an entity under 10,000 `rows`. See [`export --kind sql`](#export). |
| `rdra-*` | warning | See above. |
| `requirement-no-goal` | info | See above. |
| `requirement-untraced` | warning | A requirement that `motivates` no BUC. |
| `rule-unlabeled` | info | See above. |
| `screen-no-fields` | warning | A screen that contains no field. |
| `screen-no-transition` | info | See above. |
| `semantic-error` | error | A parse or model error of `check`; the command exits with status `1`. |
| `semantic-warning` | warning | Any other warning of `check`. |
| `stage-readiness` | info | See above. |
| `step-no-cover` | warning | A business step that `covers` no use case, API, or event. |
| `triggered-usecase-unguarded` | info | See above. |

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `json` \| `csv` \| `markdown` | `table` | Output format. `markdown` is a Markdown table with the CSV columns. |

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
stage has a gate of one or more clauses. A blocking clause that fails is a gap, and the
stage is **open**; an advisory clause that fails is a review note, which never holds a BUC
back. A stage whose clauses pass but which owns an error of [`check`](#check) (a
`Witness_`, `Deadlock_`, or violated rule on an entity the BUC touches) is **blocked**: the
elements exist, but the model has to be fixed before the next stage's detail is added.
Otherwise the stage is **met**. A BUC is at the last stage of the met prefix of the
required stages, 0 to 6; the rule stages 7A to 7C are optional, so a BUC with a
lifecycle and no rule is complete, and `next` offers the rule stages instead of
demanding them:

<!-- stage-spec: gates -->
| Stage | The gate passes when | Review notes |
|---|---|---|
| 0 Scope sketch | the BUC `belongs` to a business |  |
| 1 BUC skeleton | an actor `performs` it or one of its use cases, or an event or timer `triggers` one; and it holds a use case (by `contains`, or through a flow's steps and `covers`), or an event or timer `triggers` the BUC itself |  |
| 2 Screens | the BUC holds a use case; and every use case `displays` a screen, or an event or timer `triggers` it |  |
| 3 Data touchpoints | the BUC holds a use case; and a use case creates, reads, updates, or deletes an entity, directly or through an API it `invokes` | once a use case touches data, each use case with no touchpoint is data-free on purpose; and once a use case touches data, each screen its use cases display with no `shows` is data-free on purpose |
| 4 API and access boundary | every API a use case `invokes` belongs to a `system` | when the model declares a `permission` or `medium`, each use case with no `requires_permission` / `requires_medium` on it or its APIs is open to every actor and device on purpose |
| 5 Entity structure | every touched entity has a column that is neither a primary key nor a foreign key | when the BUC touches two or more entities, each touched entity with no `relate` to another touched entity stands alone on purpose |
| 6 Lifecycle | a use case moves an Enum or Bool column, with `transitions` or with `sets` on it | a `sets` that only marks presence, an Int, or a comparison follows a lifecycle Enum or Bool |
| 7A Local guardrails (optional) | a `forbidden` or `exclusive` with no comparison reads a column this BUC moves |  |
| 7B Local obligations (optional) | an `invariant` or `required` with no comparison reads a column this BUC moves |  |
| 7C Advanced rules (optional) | a rule that holds a comparison or names more than one entity reads a column this BUC moves |  |
<!-- /stage-spec -->

A gate passing is evidence, not a verdict. The stage sections of
[Incremental Modeling Flow](./incremental-modeling.md) say when a stage is stable enough
to move on; `next` shows where the model stands, so you only need to ask the next stage's
questions.

The text output prints one block per BUC, after a summary table when there is more than
one: each stage with what its gate counted (`[x]` met, `[ ]` open, `[!]` blocked, and the
count of notes and `check` findings), then the first required stage that is not met, with
its main question, the elements that fail the gate (`Gaps:`, at most 8; `--format json`
lists every one), the review notes (`Notes:`), the `check` findings the stage owns
(`Findings:`, by name; `check` prints the traces), the questions to ask, the commands that
review the stage once it is added, and its section in the modeling guide:

```text
BucStoreRestock "Maintain Store Restock": stage 1 BUC skeleton
  [x] 0  Scope sketch             belongs to StoreOperations
  [x] 1  BUC skeleton             2 use cases, 1 `performs` / `triggers` starting them
  [ ] 2  Screens                  0/2 use cases display a screen or run on a trigger
  [ ] 3  Data touchpoints         0/2 use cases touch an entity
  ...

Next: stage 2 Screens
  What does the actor see and do in each use case?
  Gaps:
    - usecase ChangeNextRestockDate: add `displays(ChangeNextRestockDate, Screen)` for the screen the actor works on, or `triggers(Event, ChangeNextRestockDate)` when no actor starts it
    ...
  Ask:
    - Which screen does the actor work on in each use case?
    ...
  Then review with:
    rdra-ish check src
    rdra-ish diagram src --kind rdra --format mermaid --buc BucStoreRestock
    ...
  See docs/incremental-modeling.md#stage-2-screens
```

Once every required stage is met the block reads `stage 6 Lifecycle, required stages
complete`, and `Every required stage is met.` is followed by the met rule stages and an
`Optional: stage 7C Advanced rules` block for each open one. A blocked stage reads
`Next: stage 6 Lifecycle is blocked: the elements exist, but check rejects them`. Findings
of `check` that no BUC owns are printed before the BUC blocks.

`--format json` writes `{"bucs": [...], "unowned_findings": [...]}`. Each BUC has `id`,
`label`, `stage` (`"2"`, `"7A"`, or `null` before stage 0), `stage_name`, `slug` (a
stable id such as `data-touchpoints`, which a reorder of the stages does not change),
`required_complete`, `gates` (`stage`, `stage_name`, `slug`, `passed`, `verdict` as
`met`, `open`, or `blocked`, `evidence`, the counts `gaps` and `notes`, and `findings`),
and `next`. `next.kind` is `required` while a required stage is not met, with that stage's
`stage`, `stage_name`, `slug`, `verdict`, `question`, `doc`, `asks`, `validate`, `gaps`
and `notes` (`kind` as `buc`, `usecase`, `screen`, `api`, or `entity`, `id`, `message`), and `findings` (`name`, `severity`); once every
required stage is met it is `optional`, with the same fields for each rule stage under
`stages`. Skills key on `slug`, and read `next.asks` and `next.gaps` for what to ask.

`next` refuses a model with parse or model errors, as the generators do; run `check`
first. It derives the states as `check` does, so it takes as long. It exits 0 at every
stage and verdict.

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

`files` and `fmt` read no manifest, so a malformed `rdra.toml` does not stop them.

---

## Project manifest

A model may carry an `rdra.toml` at its root:

```toml
[model]
name = "shop"

[dependencies]
auth = { path = "../auth-model" }
```

`[model] name` is required. The `[dependencies]` table is parsed and not used yet. The
`[check]` and `[verify]` tables set the [memory limit](#memory-limit) and the
[per-entity instance counts](#per-entity-instance-counts) of those commands, and
`[verify]` sets [instance symmetry](#instance-symmetry) (`symmetry`, read by `verify` only;
under `[check]` it warns and is not read):

```toml
[check]           # check, states, and (memory only) the generators
memory = "4GiB"

[verify]          # verify and export --kind tla
memory = "16GiB"
instances = { Loan = 4 }
symmetry = "always"
```

A malformed manifest, one without `[model] name`, an unknown key in `[check]` or `[verify]`,
a `memory` value that is not a [size](#memory-limit), or an `instances` count outside
`1..=255`, makes every loading command exit `1` with a message that names `rdra.toml` and
the line.

A loading command finds the manifest like this:

1. Take each input as given, made absolute without resolving symlinks. A `..` component is
   collapsed before the walk.
2. Start in the input's directory, or in the directory of a file input, and look for
   `rdra.toml` there, then in each parent.
3. Stop after the first directory that holds `.git`, which is searched too, or at the
   filesystem root. A stray `rdra.toml` above a repository never applies.
4. With several inputs, every input's manifest is read, so a malformed one fails the command
   whatever the input order. When all are valid, the first input's manifest is the project's.
   A later change will tighten how inputs from different models combine.

### The code tables

`[code.rust]`, `[code.go]` and `[code.ts]` name the files `codegen --code` writes for a
service in that language, and that [`check --code`](#check---code) holds against the
model. Every path is relative to the directory of `rdra.toml`; rdra-ish does not decide
where they go. Only `domain` is required.

```toml
[code.rust]
domain = "../service/src/domain.rs"        # the codegen output
traces = "../service/tests/traces.txt"     # with it, domain holds the conformance harness
conformance_states = 65536                 # --conformance-states of the traces

[code.obligations]
Unique_Payment_Order = "../service/migrations/003_payment.sql"
```

| Key | Description |
|---|---|
| `domain` | The domain code `codegen` prints for the language. |
| `traces` | Rust and Go: the conformance traces. With it, `domain` also holds the conformance port and harness (`--conformance`). |
| `conformance_states` | The states the BFS behind the traces explores at most. Default `65536`. |
| `package` | Go: the package name of `domain` (`--package`). Default `domain`. |
| `probe` | The probe `codegen --code` (or `--probe`) writes: in Rust a file under the service crate's `tests/`, one line per use case stating that `service` implements that use case's contract (`<UseCase>UseCase`). `build` needs it. |
| `service` | The service type, as the probe names it: `svc::service::Service`. |
| `module` | The path of the generated module in the service's crate: `svc::domain`. |
| `new` | An expression that makes a fresh service. With `traces`, the probe also holds the conformance test, `rdra_conformance`, which `run` runs. |
| `root` | The directory the build command runs in. Default: the nearest directory above `domain` that holds `Cargo.toml`, `go.mod`, `package.json` or `tsconfig.json`. |
| `build`, `run` | The command `check --code=build` / `--code=run` runs in place of the language's default, as a list of arguments. It runs only when that level is given on the command line, never from the LSP. |

An unknown key under `[code]` or one of its tables makes every loading command exit `1`, as
under `[check]`.

The LSP does not run `codegen` or a build. When the model's source text is not the text a
file under `[code]` was generated from (its fingerprint's model hash), or the file is missing
or has no fingerprint, it shows an information diagnostic on the table's line of
`rdra.toml`. A comment-only edit of the model shows it too; `check --code` decides.

The LSP loads a workspace the same way. It lists every `.rdra` file under the workspace roots,
adds the open buffers, and derives the include paths from those files exactly as the CLI does.
The workspace roots are no longer include paths of their own, so `import m.a.shared` from
`m/a/b/c.rdra`, with no loaded file declaring `module m.a.shared`, reports "unknown module"
in the editor, as `check` does. An import resolves by the `module` a loaded file declares
before it is looked up by path, in both.
A malformed `rdra.toml` clears the LSP's diagnostics and shows the error in a message, and in
a diagnostic on an open `.rdra` file.

### Per-entity instance counts
<!-- constrained-by #project-manifest -->

`--instance-count` gives every entity the same number of slots. `--instances <Entity>=<n>`
gives one entity its own, `1..=255`, and is repeatable:

```sh
rdra-ish verify model/ --instances Loan=4
rdra-ish verify model/ --instance-count 1 --instances Loan=3 --instances Room=3
```

The first run explores two rows of every entity and four of `Loan`, so a rule that breaks
only with four loans shows without growing every other entity. `check`, `states`,
`verify`, and `export --kind tla` take the flag. `instances = { Loan = 4 }` under `[check]`
sets it for `check` and `states`, and under `[verify]` for `verify` and `export --kind
tla`, which writes the modules `verify` checks. A flag replaces the table's count of its
entity and keeps the others. An id that names no entity of the model is an error that
lists the entity ids. The generators (`codegen`, `next`, and the lifecycle gate of the
outputs) run at one instance per entity and read neither table.

The `bounds:` line and the final line name each count: `OK: no errors within 2 instances per
entity (Loan=4), Int 0..5`. The TLA+ module's header names them too, and the entity's
id set is `Loan_Ids == 1..4` while the others stay `1..InstanceCount`. The largest count
sets the default `--max-patterns` of `check` and `states`.

A pre-existing child that starts linked to the parent slot of its own index (a 1:1 key or
a `1..` range under a pre-existing parent) needs as many slots as its parent; other counts
are the `IdentityStartSlots` error. Without `--instances` and the tables every output is the
same as before the flag existed.

A counted rule names the count it needs. `.at_most(3, Loan)` and a card `0..3:1` break
only with four rows of `Loan`; below that they hold vacuously, and the `Vacuous_*` warning
ends `(Loan has 2 slots; --instances Loan=4 gives it 4)`. `.at_least(3, Loan)` and
`.exactly(3, Loan)` with fewer slots fail wherever they apply, and the
`CountAboveBound` warning names the same flag (see
[Counted quantifiers](./language-reference.md#counted-quantifiers)).

### Instance symmetry
<!-- constrained-by ./design/instance-symmetry.md -->

The slots of an entity are interchangeable unless the model tells them apart: two states that
differ only in which slot holds which row reach the same verdicts. `verify --symmetry always`
stores one state per class of such states, an orbit, so a cone that outgrows the memory limit
slot by slot can finish. [Instance symmetry](./design/instance-symmetry.md) covers the safety
cones (`Cone_<check>`), reduced or not, the cones of `always` properties that `verify`
leaves unreduced, the cones of `eventually` and `leads_to` properties, and the entity cones,
when no plain run decides them. Every other cone runs as before.

```sh
rdra-ish verify samples/clinic-ops --instance-count 3 --memory 8GiB --symmetry always
```

```text
Cone_Inv_Encounter_2 [Inv_Encounter_2]: ok (about 820M states)
```

- **Which cones.** A cone that a complete plain run of the entity pass decides keeps that
  result and prints what `never` prints, and so does a safety cone whose system is the cone
  of a property not yet decided, which its plain run decides too. Only the other cones run
  up to symmetry. Which runs are complete is what each gives alone, since a run that meets
  the memory limit beside others runs again alone, so `RDRA_JOBS` does not change the
  output. A cone whose group moves no row runs as under `never`.
- **The line.** `(about <m> states)`: the sum of the orbit sizes, rounded to two figures. The
  sum counts every image of a stored state under the group, and the model need not reach
  each one, so it can exceed the states a plain run stores: on the samples it was exact on
  three cones and 5% and 83% over on two. `RDRA_ISH_SYMMETRY_STATS=1` prints on stderr, per
  run up to symmetry, the orbits it stored, the estimate, the states the branch cap cut, and
  each entity the group leaves out and why (`Item (a comparison orders its row ids)`). That
  entity's slots stay fixed and the other entities still permute.
- **Counterexamples** name concrete slots: the stored path is lifted back through every
  permutation the run applied, and the lifted trace is replayed through the step relation,
  with the rows each step names, before it prints. A trace that does not replay is a tool bug
  and stops the run.
- **Reduced cones** (phase 2). The sub-lifecycle a reduced cone quotients runs up to `H`,
  the subgroup that leaves in place every entity whose rows the quotient reads (`obs`, the
  inputs, the fold bits, the stubs of T parents, and the rows a T key can name), under
  `always`, once its plain run stopped at the memory limit, alone. The
  classes are the plain run's, so the verdicts are too; their numbering, their
  representatives, the cone's TLA+ tables, and its lifted traces can differ. A cone whose
  sub-lifecycle fits keeps its output. A trace through
  such a cone runs the sub-lifecycle up to `H` again and lifts its path to concrete states.
- **Reduced safety cones** (phase 3). A safety cone `verify` reduces runs up to `G_T`, the
  group of its T rows: every S entity stays where it is, and so does every entity that holds
  an S var or an input, whose rows such a key names, that a bit is kept per row of, or that
  an S action binds. `RDRA_ISH_SYMMETRY_STATS` names them (`the reduced sub-lifecycle of
  its cone keys on its rows`). Its counterexample is lifted through the symmetry, then through
  the reduction, and replayed. An `always` property on a reduced cone runs plain.
- **Liveness** (phase 4). An `eventually` or `leads_to` cone, `.fair` included, runs up to
  symmetry with each edge's permutation kept, and its fair components are read per binding:
  an edge moves the binding, and each fair instance, by its permutation. Its counterexample
  repeats the stored loop until the concrete states close it, and replays and passes the
  fairness check before it prints. On a reduced cone whose run up to symmetry finds a
  counterexample, the property is decided as under `never`. A property whose reduced cone
  is a safety cone's system stays plain, so that one plain run decides both.
- **Entity cones** (phase 4). An entity cone whose plain run stops at the memory limit runs
  again up to symmetry, with each edge's permutation kept, in the room its plain run held.
  When that run completes, it decides the cone's state models: the witnesses, the use cases
  that never run, the `.when` arms taken, and the deadlocks. A deadlock's trace is lifted and
  replayed like a counterexample, and names the slot its instance holds at the end, which can
  be another instance than `never` names. A run that stops too leaves the plain run's
  findings, and a cone whose plain run left no finding open keeps them without running
  again. `RDRA_ISH_SYMMETRY_STATS` prints, per stopped entity cone, the states its plain run
  stopped at and the orbits of its run up to symmetry (`symmetry: entity Appointment: the
  plain run stopped at 81920 states; up to symmetry, 26929 orbits, about 242015 states,
  complete`).
- **Shared runs.** A cone that runs up to symmetry stops once every check has failed, and its
  run decides nothing for the other passes, which have no plain graph to read off it.
- **Branch cap.** The canonical form branches on rows it cannot tell apart that other rows
  name. Past 4096 branches a state is stored without its canonical form, which keeps the
  verdict and can store one orbit more than once; the line then adds `note: <n> states
  stored without their canonical form (branch cap); the count is an upper bound`.
- **What breaks the symmetry.** A comparison that orders row ids, a row id compared with a
  literal, a key set to a literal row or chosen among some rows, a start that pins a slot, a
  creation that leaves a key at its first row, a row id compared with another value, too
  many arrangements of rows that name each other. Each leaves the entity's class out, and
  `RDRA_ISH_SYMMETRY_STATS` says which. A deleted row's key moves with its parent like any other, since a
  deleted row keeps the row it names.

### Memory limit
<!-- constrained-by #project-manifest -->

> **Known issue: the limit does not yet bound the whole process** (#237, #242, #243).
> The limit bounds what the state stores are counted to hold, not what the process takes.
> On macOS the process has been measured well past it: `verify samples/oidc-auth
> --instance-count 2` peaked at 1.85 times a `2GiB` limit, and at the default `16GiB` it passed
> 19.5 GB. Until the fixes land, set `--memory` (or `memory` in `rdra.toml`) to about half the
> memory you can spare when cones stop at the limit. Runs whose cones finish below the limit,
> as every sample does at the default limits, are not affected.

`check`, `states`, and `verify` each run within a memory limit: `--memory`, else `memory` in
the command's `rdra.toml` table (`[check]` for `check` and `states`, `[verify]` for
`verify`), else `4GiB` for `check` and `states` and `16GiB` for `verify`.

The generators explore within the limit of `check`: `codegen`, `next`, `diagram --kind
state`, and `export --kind tla` take `memory` under `[check]`, else `4GiB`, and no
`--memory` flag. The other outputs of `diagram`, `export`, `list`, and `csv` do not explore. They run at the other caps of `check` at its defaults too, so a
model `check` passes with no flags explores the same in them.

A size is a whole number and a binary unit: `4GiB`, `512MiB`, `64KiB`, `1TiB`. `G` and `GB`
mean `GiB` too, and likewise `K`, `M`, and `T`, so `4G` and `4GB` are 4 × 2^30 bytes. A bare
number, `0`, and a fraction are refused.

What the limit counts:

- **BFS.** The states a cone has reached are counted in bytes as they are stored: each
  state once (in one buffer, with its number in the index of seen states), its parent, every
  edge, and the bindings, by the length and capacity of each collection. The count is the same on every
  machine and with any number of threads, so a cone stops at the same state wherever it
  runs. A finished cone keeps its states and edges until its findings are read, so they
  stay counted, and the next cone has the rest of the limit. `verify` reads the findings
  of an entity cone that stops at the limit at once, then gives back what it held, so the
  next cone has the room the stopped one had (#284). The cones run side by side,
  yet each stops where it would stop if they ran one at a time in order: a cone that met a
  pool the others had filled, or that grew past the room left to it, runs again on its own.
- **Shared quotients** (`verify`). Cones whose sub-lifecycles are equal share one quotient
  (#252): the first cone that needs it explores and partitions the sub-lifecycle, and the
  others reuse its classes. The classes stay counted while `verify` holds them, in the bytes
  their tables take, and are given back after the last cone is reduced. A cone reuses them
  only when the runs that computed them fit the room it has, asking the limit for that room
  as those runs did; otherwise it computes its own and stops where it would without the
  cache. The safety and property cones are reduced one at a time in order, so the classes
  held, and their count, are the same with any number of threads; the entity cones are
  reduced side by side, where each still stops as it would alone in its turn. The classes
  held shrink the room of the safety and property cones' quotient runs that come after
  them, so under a tight limit such a cone can stay unreduced where it would be reduced
  without the sharing. `RDRA_ISH_NO_QUOTIENT_CACHE=1` turns the sharing off.
- **What the count leaves out.** The transition system, the per-entity views, the states a
  worker holds while it expands them, and the memory that reading the findings of a cone
  takes. On `insurance-claim-ops` at two instances, where most cones stop at the limit, the
  process peaked at 1.2 to 1.4 times the limit run one cone at a time (`RDRA_JOBS=1`) and at
  up to twice the limit with eight. A single cone that stops at the limit can take 1.35 to
  1.6 times what it is counted while it grows (on macOS, memory the allocator keeps after a
  buffer is freed), and the cones that run again alone after a crowded pool can be counted
  past the limit together (#242, #237). The default limits stop no cone of the samples.
- **TLC** (`verify --with-tlc` only). Each TLC run's `-Xmx`, and `256MiB` per JVM for what
  it holds beside its heap. Without `--with-tlc` BFS has the whole limit. With it, `verify`
  gives the TLC cone runs half the limit (at least `64MiB`) and BFS the rest. The whole-model
  property runs start after BFS has finished and may take the whole limit.
  - A run's heap follows its module: 512 MiB per MiB of module text, in whole GiB, at least
    `1g` for a cone and `2g` for a whole-model run, and no more than the share. Every module
    under 2 MiB gets the default; the 7.6 MB cone of food-permit-ops'
    `AfterAssert_SuspendBusiness_0` at `--instance-count 3` gets `4g`. A heap shrinks to the
    share only when the default does not fit.
  - A run starts once its heap and 256 MiB fit in the share beside the runs going, or when
    no other run goes, so at `16GiB` six cone runs of `1g` go at once.
  - A run that runs out of memory runs once more at twice its heap when the share holds
    that, and its line says `rerun at -Xmx<2h> after -Xmx<h> ran out of memory`. Otherwise
    it is a `tool_error` with the hint.
  - A module that needs more heap than the share holds is not run: a `tool_error` whose
    report names the heap it needs and the hint.

  `RDRA_TLC_HEAP` and an `-Xmx` in `JAVA_TOOL_OPTIONS` still set the heap as given, with no
  rerun; the runs then queue so that their heaps fit, and run one at a time when one heap
  alone is past the share. Under `--with-tlc`, a `--memory` that leaves no BFS beside
  `64MiB` of TLC heap is refused.
- **z3** (`verify --prove`) is not counted. A z3 that reaches its own `memory_max_size`
  exits, which would end the proof, so `verify` leaves z3 unbounded.

A cone that reaches the limit stops as a cone that reaches `--max-states` does. `check`
reports a `MemoryCapReached` error that names the limit, says what the held states took, and
estimates what finishing needs, and exits `1`.
`states` marks the views it cut. `verify` lists the findings the cone leaves under `not
checked:`, ends with a warning, and exits `0`, or `1` under `--deny-undecided`; its
`result:` line says `not decided` either way. Examples of both close this section.

What `check` and `states` say when a cone stops at the limit: the states it held, the bytes
each took (the store counted above, with the state that did not fit), and what the cone's
BFS layers say about the states it reaches. BFS reaches the cone depth by depth, and the
run counts the new states of each depth (one number per depth, counted in the store). On
the sample cones that number rises to a peak and falls about as it rose, so:

- **Past the peak.** Once two whole depths lie past the largest one, the message takes the
  fall to mirror the rise: the cone probably holds twice the states before the peak plus
  the peak. It is an estimate, not a bound. On 73 sample cones at `check` and `verify`
  bounds, stopped at a tenth to nine tenths of their states, the 61 estimates came out at
  0.51 to 1.37 of the true count, and at 0.74 or more but once. The message names the
  memory for a quarter more states than the estimate; rerun at those caps, that size
  covered the store of all but one of the 61 and stood at most 2.6 times over it.
- **Still rising.** While the last whole depth is the largest yet, nothing says where the
  peak is, and the message says the frontier still grows and by how much. The peak of the
  sample cones came after 36% to 100% of their states (63% at the median), so a run that
  stops early usually gets no estimate: of the 73 cones, 1 got one when stopped at half its
  states, 10 at three quarters, and 49 at nine tenths. A cone that outgrows a machine is
  often still rising; the `Complaint` cone of `insurance-claim-ops` at two instances stops
  at the default `4GiB` with 1.7 million states, still growing 1.7 times per depth.
- **Just past the peak, or a long fall.** One depth past the peak, or a fall that already
  holds more states than the mirror gives, gets no estimate either, and the message says
  which.

Without an estimate from the layers, the message falls back on the cone's variables: the
most states the cone can have is the product of their domains, and if finishing at that
many takes no more than `1TiB` the message names that size as what finishing needs at the
least, if all of them are reachable. The bound is loose: most of the product is never
reached. A size is computed with `(w + 12)·p2(n) + 5·buckets(n) + (4 + b)·p2(e)` bytes for
`n` states of `w` bytes and `e` edges, where `b` is the 1, 2 or 4 bytes of a binding id
(`p2` rounds up to a power of two, `buckets` to a power of two of at least `n·8/7`): the run
keeps each state once, in one buffer, indexes it by number, and keeps an edge as its target
and binding id. A state is kept packed: each variable takes the bits its domain needs
(`ceil(log2(size))`, so a 3-value status takes 2), and `w` is their sum rounded up to whole
bytes. A cone that stops once every check fails keeps no edges, and its size is
`(w + 8 + b)·p2(n) + 5·buckets(n)`. The size is never under what the run counts at its
end. `e` is the edges per
expanded state seen so far times `n`; early depths have fewer, so that part is low. Every
figure is for the one cone; the cones share the limit, so allow more than it says.

```text
error[MemoryCapReached_Task]: state derivation for entity 'Task': memory limit 17KiB reached at 256 states; results are incomplete: the cone has 9 variables, 9 of them vary (...); new states first reached by: Flip7 127, Flip6 64, Flip5 32, Flip4 16, Flip3 8, Flip2 4, Flip1 2, Close 1; held at 71 B per state; the frontier peaked at depth 5 and has shrunk for 3 layers since, so the cone probably holds about 258 states (its falling side taken to mirror its rising side: an estimate, not a bound); with a quarter to spare, this cone alone probably needs about 1MiB: try 1MiB or more with --memory or `memory` under [check] in rdra.toml, or narrow with --entity or --buc
error[MemoryCapReached_Order]: state derivation for entity 'Order': memory limit 1KiB reached at 14 states; results are incomplete: the cone has 3 variables, 3 of them vary (values reached/domain: Order.status 5/5, Order._exists 2/3, Order.delivered_at 2/2); new states first reached by: PlaceOrder 5, CancelOrder 3, CapturePay 3, DeliverOrder 1, ShipOrder 1; held at 71 B per state; the frontier still grows (5 new states at depth 3, 1.7x the depth before), so its layers give no estimate of the states left; the cone's variables allow at most 900 states, so if all are reachable this cone alone needs at least about 1MiB; raise it to 1MiB or more with --memory or `memory` under [check] in rdra.toml, or narrow with --entity or --buc
```

The `Task` cone (`crates/rdra-ish-cli/tests/fixtures/memory/flags.rdra`) holds 257 states.

`verify` lists what the cones leave undecided. `verify samples/patent-ops --memory 64MiB`
prints (trimmed):

```text
not checked: 10 findings not decided
  whether some OfficeAction can get stuck at status = open is not decided: the OfficeAction cone stopped at the memory limit (64MiB; raise it with --memory or `memory` under [verify] in rdra.toml)
...
result: not decided
warning: 10 findings a cone stopped at the state cap or the memory limit left undecided; the OK line holds only for what was decided (--deny-undecided fails the run)
```

Under `--with-tlc` the reason also names the share BFS has: `BFS has 256MiB of it` at
`--memory 512MiB`.

Under `verify --prove`, an invariant cone that stops at the memory limit before a
violation is decided when z3 proves every rule in it for any number of rows. Its line reads
`ok`, it counts as decided for `result:` and `--deny-undecided`, and it still says where BFS
stopped. `samples/formal-verification/check_met.rdra` at `--instance-count 4 --memory 1KiB
--prove`:

```text
Cone_Forbidden_CartLine_0 [Forbidden_CartLine_0, Check_CartLine_qty_0]: ok (8 states, BFS stopped at the memory limit, decided by proof: --prove proved every check for any number of rows)
```

A cone with an `after` assertion, or with a rule z3 leaves unproved or out of scope, stays
`not decided`, as does a cone stopped at `--max-states`. See
[Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows).

---

## `fmt`

Format RDRA DSL source files.

```
rdra-ish fmt <INPUTS...> [--write | --check]
```

The formatter parses each `.rdra` file and prints a canonical source layout:

- The `module` line is followed by one blank line. Consecutive `import` lines form one
  block with no blank line inside it, and one blank line follows the block.
- A declaration whose only clause is `description` keeps it on the declaration's line
  while that line is at most 120 display columns wide; a wide character, such as most
  Japanese text, counts as two columns, and a trailing comment does not count. A longer
  declaration, or one with several clauses, puts each clause on its own line indented by
  two spaces. Entity and DTO columns and variation values are indented by two spaces.
- A declaration that prints on several lines is set off by one blank line on each side.
  Between two one-line items the source's grouping stands: a blank line there is kept
  (several become one), and none is added. Ids are not aligned into columns.
- Annotations use compact spacing, and a predicate call keeps its chain on one line.
- A comment stays where it was: a trailing comment at the end of its line, and an
  own-line comment above the line that follows it, so a citation above a declaration
  stays attached to it. A comment from a line that `fmt` joins, such as a metadata clause
  or a wrapped chain call, moves above its item.

This is an AST-based formatter. It preserves model structure, identifiers, labels,
metadata values, columns, predicates, chains, tuples, and comparison expressions, and a
second run changes nothing. A [deprecated form](./language-reference.md#deprecated-and-removed-forms)
prints in its current form, so `fmt --write` is the migration for it; a removed form is a
parse error, and `fmt` refuses the file.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to format. Directories are searched recursively for `.rdra` files. |
| `--write` | flag | off | Rewrite files in place. |
| `--check` | flag | off | Exit with status `1` and print `needs formatting:` rows when any file differs from the canonical format. |

Without `--write` or `--check`, formatted source is written to stdout and no file
changes: `rdra-ish fmt src/` only prints, and `rdra-ish fmt --write src/` rewrites the
files. With multiple input files, each output block is prefixed by a `// <path>` marker.

---

## `diagram`

Generate one RDRA diagram as Mermaid text (`.mmd`).

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [--buc <ID>]... [--usecase <ID>]... [--entity <ID>]... [--actor <ID>]... [--node-kind <KIND>]... [--edge-kind <KIND>]... [--title] [--legend] [-o <OUT>]
rdra-ish diagram <INPUTS...> --all [--title] [--legend] -o <DIR>
```

The `.mmd` is written for a reader that redraws it, a person or a tool such as an AI that
draws SVG. It carries the RDRA meaning in text, under the [encoding
contract](#encoding-contract): a `%%` header, node ids `<kind>__<DSL id>` that are the same
in every kind, a `:::<kind>` tag and a shape per kind on each node, the RDRA layer names,
and the DSL predicate names as edge labels where the ends do not already say the relation.
Colours and direction carry no meaning beyond the `:::<kind>` tag, so a renderer is free to
replace them.

### Kinds

Each kind is one diagram of RDRA 3.0, or `rdra`, the base graph of every element and
relation, from which the RDRAGraph views can be derived.

| Kind | RDRA diagram | Draws | Leaves out | With `--buc` |
|---|---|---|---|---|
| `rdra` (default) | RDRAGraph, the whole graph | Every element in its layer and every relation; `--node-kind` / `--edge-kind` narrow it | ADRs; columns (`er`), variation values and condition bodies (`condition`) | Everything reachable from the BUCs, and the states their use cases move between |
| `context` | システムコンテキスト | The system boundary: one edge per actor, labelled with the BUCs it performs, and one edge per external system and direction, labelled with the events exchanged; every edge derived, no RDRA layer subgraphs | BUCs, use cases, and events as nodes; `%% derived` names the BUCs and events behind each label | The actors and external systems with an edge through the BUCs, and the systems they reach |
| `requirement` | 要求モデル | A value tree, left to right: goals (with metric and `baseline → target`), the requirements that achieve them, and the BUCs those requirements `motivate`; a requirement with `status "out-of-scope"` is drawn apart in `やらないこと` with dashed edges | NFRs, constraints, qualities, entities, and systems (`list --kind nfr`, `list --kind constraint`), ADRs, and BUCs no requirement motivates; named in `omitted:` | The BUCs and the requirements that motivate them, with those requirements' goals |
| `business` | 業務の流れ図 (ビジネスコンテキスト, ビジネスユースケース) | The business flow: each business holding its BUCs, and an edge from a BUC to the BUC it hands off to, labelled with the event that passes it | Actors and external systems (`list --kind buc-roles`); use cases, steps, and events as nodes; a hand-off no event carries | The named BUCs and the BUCs they hand off to or from, with their businesses |
| `usecase` | UC複合図 | Each use case with its actors, screens and what they show, the entities it reads and writes, its events, external systems, and timers, the conditions on it, and its transitions | BUCs, steps, media, permissions, fields, APIs (their CRUD is folded into the use case), and DTOs | The use cases the BUCs contain, directly or through a step; `--usecase` names use cases instead |
| `robustness` | ロバストネス図 | The use cases as an ICONIX robustness diagram: actors, boundaries (screens, external systems, timers), controls (APIs, or the use case itself), and the entities each control reads and writes | BUCs, steps, events (named in edge labels), conditions, states, media, permissions, fields, and DTOs | The use cases the BUCs contain, directly or through a step; `--usecase` names use cases instead |
| `sequence` | シーケンス図 | One use case in order: the actors, screen, controls (APIs, or the use case itself), entities, and external systems that take part, the guards, each control's reads and writes with the `sets` values and the lifecycle move, and the events it raises | BUCs, steps, other use cases (a triggered one is one message to its control), permissions, media, fields, and DTOs | The BUCs' use cases one after another, each under a numbered note; without `--buc`, exactly one `--usecase` |
| `flow` | 業務フロー | Each flow with its steps, their `precedes`, `branches`, `excepts`, and `repeats` edges, what each step `covers`, and a lane per actor | Actors without `performs(Actor, Step)`; screens and entities | The flows the BUCs contain |
| `screen` | 画面遷移図 | Each screen; a solid edge per `navigates`, labelled with its `.via` use case; where none is declared, a dashed `業務上の順番` edge between the screens of consecutive steps in one actor's lane | Use cases, fields, and entities as nodes | The screens the BUCs' use cases display, and the business order of the BUCs' flows |
| `information` | 情報モデル | Entities with their `relate` links, state models, and the variations their columns take | Columns (`er`) and use cases | The entities `er` draws for the BUCs |
| `condition` | バリエーション・条件 | Each condition with the variations and state models it combines and what it applies to; each variation with its values | Entities (a state model stands for its column) | The conditions on the BUCs' use cases and their APIs, and what those combine |
| `state` | 状態モデル | Without a scope, an index: one composite state per state model holding one note, `<n> 状態 / <m> 遷移 (--entity <Entity>)`. With `--entity` or `--buc`, each state model whole: its states, the use case of each transition, start and end states, coloured by what the lifecycle check found | `Enum` columns that no `transitions` drive, named in `omitted:` | Each state model one of the BUCs' use cases moves, whole |
| `event` | システム間連携（イベント） | `raises` and `triggers` between use cases, BUCs, external systems, events, and timers | States and transitions (`state`) | The events reachable from the BUCs and their links |
| `er` | (RDRASpec logical data model) | Entities with their columns and `relate` cardinality, as an `erDiagram` | Variation values and state models, shown only as column types | The entities reachable from the BUCs |

`state` is the one kind that reads the lifecycle (for its start states and the colours of
its states). It refuses to write while the lifecycle has errors other than a deadlock,
which it colours; every other kind draws declared facts, explores nothing, and writes.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | one of the [kinds](#kinds) | `rdra` | The diagram kind. |
| `--format` | `mermaid` | `mermaid` | Output format. Mermaid text (`.mmd`) is the only one; to get an image, render the `.mmd` with `mmdc` or any Mermaid-aware tool, or hand it to a tool that redraws it. |
| `--buc <id>` | string (repeatable) | — (whole model) | Draw the part of the model the named BUCs reach, the union over every id. Applies to every kind; the [kinds](#kinds) table says what each one keeps. An unknown id is an error. |
| `--usecase <id>` | string (repeatable) | — (whole model) | Draw only the named use cases. Applies to `usecase`, `robustness`, and `sequence`; an unknown id is an error. `sequence` needs exactly one, or `--buc` for a BUC's use cases in order. One of `--buc`, `--usecase`, `--entity`, and `--actor` at a time. |
| `--entity <id>` | string (repeatable) | — (whole model) | Draw by entity; an unknown entity is an error. `state`: the named entities' state models, every lifecycle column, whole; the `omitted:` and `set-only:` header lines name only those entities' columns, and an entity with no state model is an error. `er` and `information`: the named entities and the ones a `relate` joins them to. `usecase` and `robustness`: the use cases that read or write them, an invoked API's operations included. |
| `--actor <id>` | string (repeatable) | — (whole model) | Draw the use cases the named actors perform, directly or through a BUC. Applies to `usecase` and `robustness`; an unknown actor, or one that performs no use case, is an error. |
| `--node-kind <kind>` | string (repeatable) | — (all nodes) | Draw only these node kinds. Applies to `rdra`. Examples: `actor`, `buc`, `flow`, `step`, `usecase`, `api`, `entity`, `field`. Alias: `--kind-filter`. |
| `--edge-kind <kind>` | string (repeatable) | — (all edges) | Draw only these relation kinds. Applies to `rdra`. Examples: `contains`, `invokes`, `reads`, `creates`, `maps-field`, `owns`, `relate`. Alias: `--edge-filter`. |
| `--all` | flag | off | Write the [review set](#the-review-set) under `--out`, a directory. Not together with a scope, `--node-kind`, or `--edge-kind`; `--kind` is not read. |
| `--title` | flag | off | Open the text with a Mermaid title (`---`, `title: "UC複合図: Order"`, `---`): the diagram's RDRA name and the labels of what it is scoped to. A renderer shows it, and drops the `%%` header that says the same. |
| `--legend` | flag | off | Add a `凡例` subgraph to a `flowchart` diagram, one node per node kind in its colour, before the `classDef` lines. The other diagram types have no node classes and stay as they are. |
| `-o`, `--out` | path | — (stdout) | Output file path. Without it the Mermaid text goes to stdout and no file is written, as `list`, `states`, and `next` print theirs. With it, the extension is always `.mmd`: a path without one gets it added (`-o out` writes `out.mmd`), and an existing one is replaced (`-o x.foo` writes `x.mmd`; `x.mmd` stays; `-o d.tar.gz` writes `d.tar.mmd`). With `--all`, the directory the files go to. |

#### The review set

`--all -o <dir>` writes one file per diagram:

- `<kind>.mmd` for the kinds that stay readable on a whole model: `context`, `requirement`,
  `business`, `information`, `condition`, `er`, `screen`, `event`, and `state` (its index).
- `<kind>__<BucId>.mmd` for each BUC: `usecase`, `robustness`, `flow`, and `sequence`.
- `state__<EntityId>.mmd` for each entity with a state model.

A diagram with nothing to draw is left out, and the last line counts them:
`wrote 58 diagrams to docs/diagrams (1 with nothing to draw left out)`. A model whose
lifecycle has errors gets no `state` diagrams, with a note; the rest are written.

#### What `diagram` says on stderr

- `warning: this diagram has 73725 characters and 475 edges; Mermaid's defaults stop at
  50000 characters (maxTextSize) and 500 edges (maxEdges), so it does not render as is.`
  The text is written all the same. The warning names the options that narrow that kind;
  the alternative is to raise both limits in the renderer's config.
- `note: nothing to draw: the model declares nothing --kind flow draws in this scope`. The
  text is the header alone, with `%% kinds: (none)`.

Notes:

- Removed names fail with a message that names the replacement: `--kind
  boundaryless-graph`, `business-area`, `technical-area`, `diff`, and
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
- For `--kind context`, the nodes sit in no layer subgraph, and the header has no
  `layers:` line. Every edge is derived and followed by its `%% derived ... via <ids>`
  line. An actor has one `-.->` edge to the system, labelled with the labels of the BUCs
  it performs, joined by `、`: a `performs` to a BUC counts that BUC, and a `performs` to
  a use case or step counts each BUC that contains it (the node itself when none does).
  An external system has one edge per direction, labelled with the labels of the events
  exchanged, joined by `、`: toward the system for each event it raises that triggers one
  of the system's use cases or BUCs, and from the system for each event one of its use
  cases raises that triggers the external system. `via` names the BUC or event ids behind
  the label. With one declared system every edge ends there. With
  several, an edge ends at each system that contains an API the use cases behind it
  invoke. When no system is declared, or several are and none can be traced, the edge
  ends at `system__Model["システム"]`, followed by
  `%% synthesized system__Model: <reason>`; it is the one node no other kind draws.
  The whole model draws every actor, external system, and system; `--buc` draws those
  with an edge through the BUC's use cases, steps, and APIs, and labels an edge with the
  scoped BUCs only.
- For `--kind requirement`, the nodes sit in no layer subgraph, and the header has no
  `layers:` line. The tree reads goal, requirement, BUC. A goal's label adds a second
  line (`<br/>`) with its `metric` and `baseline → target`, whichever of them it has. The
  edges are the declared `motivates` (requirement to BUC, unlabelled) and `achieves`, drawn from the
  goal as an unlabelled `-->` (`achieved_by`) to the requirement and followed by
  `%% derived <goal> -> <requirement> via <requirement>`. A requirement with
  `status "out-of-scope"` sits in `subgraph group_out_of_scope["やらないこと"]`, and its
  edges are dashed (`-.->`); every other edge is `-->`. The whole model draws every goal
  and requirement, linked or not, and the BUCs a requirement motivates; `--buc` draws the
  named BUCs, the requirements that motivate them, and those requirements' goals. NFRs,
  constraints, qualities, entities, and systems are not drawn: the `omitted:` line names
  them, and `list --kind nfr` and `list --kind constraint` write them as tables.
- For `--kind business`, the nodes sit in no layer subgraph, and the header has no
  `layers:` line. Each business is `subgraph business__<id>["<label>"]` holding its BUCs.
  A BUC that belongs to several businesses sits in the first by id, with a
  unlabelled `-->` (`belongs`) edge to each other; a BUC of no business sits outside every subgraph. An
  edge `buc__A -->|<event label>| buc__B` is a hand-off: a use case contained in A (directly
  or through a step) raises the event, and the event triggers a use case contained in B, or
  B itself (`triggers(Event, Buc)`). It is drawn once per `(A, B, event)` and followed by
  `%% derived <A> -> <B> via event__<Id>`. A hand-off with no event on it, such as one
  BUC feeding the next through data only, has no edge, so a BUC left unconnected is a gap
  to look at in the model. No actor or external-system node is drawn: `list --kind
  buc-roles` is the table of who takes part. `--buc` draws the named BUCs, the BUCs they
  hand off to or from, and their businesses.
- For `--kind usecase`, the use cases and what they touch sit in the `rdra` layers with
  the `rdra` ids, so a node matches its `rdra` node. An edge `rdra` does not draw is
  derived and followed by its `%% derived` line: an API's CRUD drawn from the use case
  that invokes it (`via api__<Api>`), and an actor drawn performing the use cases of the
  BUC or step it performs (`via buc__<Buc>` or `via step__<Step>`). A use case that an
  event or a timer starts gets no derived actor: only a `performs(Actor, UseCase)` draws
  who performs it. A condition is drawn with its `-.->|applies_to|` edge to a use case, and
  with a derived `-.->|when|` edge to a use case whose `transitions` arm or `chooses` value
  its `.when(Cond)` guards (the `%% derived ... via transitions ...` / `via chooses ...`
  line of `rdra`). A declared edge wins over a derived one. APIs, DTOs, permissions, media, fields, and BUCs are not drawn;
  `omitted:` names the ones linked to a drawn use case or screen.
- For `--kind flow`, the type line is `flowchart TD` and there are no layers. Each flow is
  `subgraph flow__<id>["<label>"]` holding its steps; the whole model also draws a step
  of no flow, outside any. The edges are the declared ones: `-->|precedes|`,
  `-->|branches|`, `-.->|excepts|`, and `-->|repeats|` between steps, and an unlabelled `-.->`
  (`covers`) from a step to the use cases, APIs, events, and timers it covers, drawn outside the
  flows. A step of a flow that exactly one actor performs sits in that actor's lane,
  `subgraph lane__<Flow>__<Actor>["<actor label>"]` inside the flow, whose first line is
  `%% lane lane__<Flow>__<Actor>: actor__<Actor>`. A lane is not an element, so its id
  is unique per flow; the `lane` kind appears in `kinds:`. A step several actors perform
  has an unlabelled `-.->` (`performs`) edge from each. Without `performs(Actor, Step)` there are no
  lanes. `--buc` draws the flows the BUCs contain; a BUC without a flow draws an empty
  diagram, its header only.
- For `--kind screen`, the type line is `flowchart LR` and there are no layers. Each
  screen of the scope is drawn, connected or not. Each `navigates(A, B)` is
  `screen__A -->|<use case label>| screen__B`, labelled with the use case of its `.via`,
  or `-->|navigates|` without one, and a `.via` adds `%% via screen__A -> screen__B
  usecase__<id>` after the edges. Where no `navigates` goes from A to B, two consecutive
  steps of one flow (`precedes`, `branches`, or `excepts`) that the same single actor
  performs give `screen__A -.->|業務上の順番| screen__B` for each screen A the first
  step's use cases display and each screen B the second step's display (A and B
  different), followed by `%% derived screen__A -> screen__B via <flow and step ids>`.
  A dashed edge is the order the work visits the screens, not a transition the UI offers.
  `--buc` keeps the screens the BUCs' use cases display, and the business order of the
  steps of their flows.
- For `--kind information`, there are no layers. Each entity of the scope is drawn (the
  entities `--kind er` draws), each `relate` between two of them as
  `entity__<Child> ---|<card>| entity__<Parent>` with the DSL cardinality (the same
  relates as `er`, without a child-side minimum), each state model, a column that
  `transitions` drive, as `statemodel__<Entity>_<column>` (the composite state id of
  `--kind state`) with an unlabelled `-.->` edge (`statemodel`), and each variation a column takes
  with an `-.->|variation <column>|` edge. An `Enum` column no `transitions` drive is no
  state model. `omitted:` starts with `columns`, which `er` draws.
- For `--kind condition`, there are no layers. Each condition is drawn with a
  `%% holds <id>: <body>` line under it when it has a `holds` body, and an
  unlabelled `-.->` (`combines`) edge to each variation and state model it combines
  (`statemodel__<Entity>_<column>`, the composite state id of `--kind state`). A link
  read from the `holds` body rather than a `combines` declaration is followed by
  `%% derived <from> -> <to> from holds`, as in `rdra`. Unlabelled `-.->` (`applies_to`) edges go to
  the APIs and systems it applies to, and `-.->|applies_to|` edges to the use cases. A
  condition that guards a `transitions` arm or a `chooses` value with `.when(Cond)` has a
  derived `-.->|when|` edge to that use case, followed by `%% derived <condition> ->
  <usecase> via transitions <Entity>.<column> <from>-><to>` or `via chooses
  <Entity>.<column> <value>` (`, `-joined when it guards more than one), as in `rdra` and
  `usecase`. Each variation is
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

- For `--kind sequence`, the file is a Mermaid `sequenceDiagram` of one use case, so
  `--usecase` must name exactly one. With `--buc` it plays the BUC's use cases one after
  another in the same diagram, each under a note that numbers and names it
  (`Note over screen__Cart: 2. Place order`), in the order the model first places them
  (`contains`, or a step's `covers`), a use case after the ones whose events trigger it. The header, `%% kinds:` line, and ids (`<kind>__<DSL id>`,
  as `participant actor__Clerk as Clerk`) are those of the other kinds, and every
  participant id is a node of `rdra`. Participants appear in this order, only those that
  take part: the actors that perform the use case (an event- or timer-triggered one has only
  its explicit `performs`) or the external system, timer, or raising use case that starts it;
  the screen it `displays`; each control, as in `robustness`; each entity the controls read
  and write; and the controls and external systems an event it raises reaches. Messages
  follow in order:
  1. The actor calls the screen with the use case label, or the starter sends its event
     to the control.
  2. The screen calls the control with the use case or API label.
  3. When the use case has guards, `alt <guards>` opens: each condition with a `holds` body
     that applies to it (the condition label, then its body), and for each column its
     `transitions` move, `Entity.column == <from>`, or `Entity.column in (<from>, ...)` when
     the arms leave from several states, unless a condition already says it. `else guard
     fails` ends with the control replying `rejected` to the screen.
  4. Each control `->>` its entities: `read`, then one transaction at a time in the
     order of the transaction inference, each write labelled `create`, `update`, or `delete`
     with the lifecycle move (`status <from label> -> <to label>`) and the other columns
     `sets` fixes (`paid_at=set`). Arms that leave one state for several are the actor's
     choice: after the write, `alt status <from> -> <to>` holds one branch per arm, each a
     message `status = <to>`, and `end` closes it. A transaction with more than one write, or
     inferred from foreign keys, opens with a `Note over <control>: transaction (...)`.
  5. For each event the use case `raises`, the control sends `-)` to the control of each use
     case it `triggers` (one message, never expanded) and to each external system it
     `triggers`; an event that triggers nothing is a `Note`.
  6. The control replies `done` to the screen, or to the starter when no screen is drawn.
- For `--kind robustness`, nodes sit in one flat list in the order actor, boundary, control,
  entity, each sorted by id, and the edge grammar is the whole point: an actor reaches a
  screen, a screen reaches a control, a control reaches an entity, and nothing skips a
  step. The kind is not layered, so the header has no `layers:` line.
  - Boundary: a `screen` a use case `displays`; an `extsystem` that `raises` an event
    starting a use case in scope, or that an event a use case in scope `raises` triggers;
    a `timer` that `triggers` a use case in scope.
  - Control: each `api` a use case `invokes`, and the use case itself when it reads or
    writes an entity directly or invokes no API. A use case with an API and direct CRUD
    draws both.
  - Entity: each entity a control `reads`, `creates`, `updates`, or `deletes`.
  - Edges, labelled so a reader finds the use case without a use case node:
    `actor --> screen` (`uses`, the one relation from an actor, so unlabelled) (the actor performs a use case that displays the screen;
    one an event or a timer starts takes only its declared performer, as in `usecase`);
    `screen -->|<use case label>| control` (the use case displays the screen and the
    control is its API, or it); `control -->|CRUD letters| entity` (the union of the
    control's operations on the entity); and the dotted
    `control -.->|<event label>| control`, `extsystem -.->|<event label>| control`,
    `control -.->|<event label>| extsystem`, and `timer -.-> control` (`triggers`, the one
    relation from a timer, so unlabelled) when both ends are in scope.
  - Never drawn: an edge from an actor to a control, or from a screen, an external system,
    or a timer to an entity. An actor whose use cases display no screen has no edge and
    is left out. Events, conditions, states, and `%% derived` lines are not drawn.

```
%% rdra-ish diagram=rdra scope=buc:BucPayment model=samples/ec-site
%% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム
%% kinds: actor extsystem business buc usecase screen event api entity state
%% omitted: (none)
flowchart LR
  subgraph layer_value["システム価値"]
    actor__Customer(["Customer"]):::actor
  end
  ...
  actor__Customer -.-> buc__BucPayment
  buc__BucPayment --> usecase__Capture
  usecase__Capture -.->|updates| entity__Order
  usecase__Capture -.->|transitions Order.status awaiting_capture->paid| state__Order_paid
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
```

- The header comes first. `scope` is `whole`, `buc:<id>[,<id>]`,
  `usecase:<id>[,<id>]` (also the use cases `--entity` or `--actor` keeps in `usecase` and
  `robustness`), or `entity:<id>[,<id>]` (`--entity` in `state`, `er`, and `information`); `model` is the input paths as given. `layers:` (in the kinds
  with RDRA layers: `rdra` and `usecase`) and `kinds:`
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

- Each node is `<id><open>"<label>"<close>:::<kind>`, in the Mermaid shape of its kind
  (`rdra_ish_emit::NODE_SHAPES`), so kinds tell apart in black and white too. The label
  is always quoted, with `"` written `#quot;`. The kinds one focused diagram draws together
  have different shapes (except a `value`, which sits inside its variation's subgraph
  and is a rectangle like `system` in `condition`); the `rdra` diagram has more kinds than Mermaid has shapes, so
  there a shape repeats across layers.

  | Shape | Mermaid | Kinds |
  |---|---|---|
  | stadium | `([..])` | `actor` |
  | rounded | `(..)` | `usecase` |
  | rectangle | `[..]` | `screen`, `buc`, `step`, `system`, `value`, `field`, `dto`, `flow`, `medium`, `permission`, `quality`, `adr` |
  | hexagon | `{{..}}` | `api` |
  | cylinder | `[(..)]` | `entity` |
  | subroutine | `[[..]]` | `extsystem`, `statemodel` |
  | asymmetric | `>..]` | `event` |
  | circle | `((..))` | `timer` |
  | double circle | `(((..)))` | `state` |
  | rhombus | `{..}` | `condition` |
  | parallelogram | `[/../]` | `goal` |
  | trapezoid | `[/..\]` | `requirement` |
  | parallelogram, leaning left | `[\..\]` | `nfr`, `constraint`, `variation` |
  | trapezoid, upside down | `[\../]` | `business` |

  A subgraph keeps `subgraph <id>["<label>"]` whatever its kind.
- Each node carries `:::<kind>`, and one `classDef <kind>` per kind closes the diagram.
  Each RDRA element family has its own light fill (actors red, BUCs and flows green, use
  cases blue, screens violet, events pink, information yellow, variations and conditions
  purple), so a plain Mermaid render tells the kinds apart. The colour says no more than
  the `:::<kind>` tag, and a renderer is free to replace it.
- Each layer is `subgraph layer_<token>["<RDRA name>"]`. A subgraph with an element id
  (`business__<id>`, `flow__<id>`, `variation__<id>`) is that element, holding its parts.
- An edge is `<from> <arrow>|<label>| <to>`, or `<from> <arrow> <to>` without a label. A
  label is left off when, in that kind, the kinds of the two ends have exactly one
  relation and the label would only name it: the relation is then read back from the ends
  (`rdra_ish_emit::implied_relation`, from the table `rdra_ish_emit::EDGE_RELATIONS`).
  So `performs`, `invokes`, `displays`, `shows`, `raises`, `triggers`, `contains`,
  `covers`, `belongs`, `motivates`, `achieves` / `achieved_by`, `applies_to`,
  `combines`, and the other single-relation pairs go unlabelled, as do the robustness
  diagram's actor-to-screen `uses` and timer-to-control `triggers`, and the information
  diagram's `statemodel`. A condition to
  a use case is two relations in `rdra`, `usecase`, and `condition`: the declared
  `applies_to` and the `when` derived from a `.when(Cond)` guard, so both are labelled there
  (`-.->|applies_to|`, `-.->|when|`); a condition to an API or a system stays unlabelled. A label stays
  where it says something the ends do not: the CRUD verb (`reads`, `creates`,
  `updates`, `deletes`) and the API's `request` / `response` / `error_response`, a
  step order (`precedes`, `branches`, `excepts`, `repeats`), a relate's cardinality
  (`N:1`), a transition, the robustness diagram's use case, CRUD letters, and events, the
  BUCs on a context actor edge (a `performs`) and the events on a context external-system
  or business edge, an information diagram's `variation <column>`,
  and every screen-diagram edge (its `.via` use case, `navigates`, or `業務上の順番`). A
  transition is drawn from the use case to the state it enters, labelled `transitions
  <Entity>.<column> <from>-><to>`. The direction is from the predicate's first argument to
  its second.
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
  | `%% derived <from> -> <to> via transitions <Entity>.<column> <from>-><to>` / `via chooses <Entity>.<column> <value>` | A `when` edge: the `transitions` arms and `chooses` values of the use case that the condition's `.when` guards, `, `-joined | `rdra`, `usecase`, `condition` |
  | `%% holds <id>: <body>` | A condition with a `holds` body | `condition` |
  | `%% lane <lane>: actor__<id>` | A lane subgraph's first line: the actor who performs its steps | `flow` |
  | `%% synthesized system__Model: <reason>` | The synthesized system node | `context` |

  An edge without a `%% derived` line is a declared relation, the same edge `rdra` draws.

`--kind state` is a `stateDiagram-v2`, where `classDef` and layers do not apply, so the
header and the id prefixes carry the kinds:

```
%% rdra-ish diagram=state scope=buc:BucPayment model=samples/ec-site
%% kinds: statemodel state usecase
%% omitted: (none)
%% set-only: Payment.status (Enum a `sets` writes and no `transitions` drive: a state axis of check, not a state model)
%% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out
stateDiagram-v2
  state "Order.status" as statemodel__Order_status {
    [*] --> state__Order_pending
    state "pending" as state__Order_pending
    ...
    %% transition usecase__Capture
    state__Order_awaiting_capture --> state__Order_paid : Capture Payment
    state__Order_delivered --> [*]
  }
```

- Each state model, a column that `transitions` drive, is one composite state
  `statemodel__<Entity>_<column>` labelled `<Entity>.<column>`. Its states follow the
  column's `Enum` order, with the ids `--kind rdra` gives them.
- Each transition is labelled with the label of the use case that drives it, then
  `[...]` with the label of each condition whose `holds` body guards that use case (UML
  guard notation). The line before it, `%% transition <usecase id>`, names the use case
  by the id `--kind rdra` gives it, for tools that read the diagram.
- `[*] -->` marks the start state the lifecycle gives the column. `--> [*]` marks an end
  state, one with no transition to another state.
- `omitted:` names the `Enum` columns in scope that no `transitions` drive and no `sets`
  writes. One that a `sets` writes is still a state axis of `states` and `check`, so it is
  named apart, on a `%% set-only: <Entity>.<column> ... (...)` line after `omitted:`, as
  `Payment.status` above. It has no composite state either: without `transitions` it has no
  states to draw.
- `--buc` draws each state model that one of the BUC's use cases moves, whole.
  `--entity <Entity>` (repeatable, not with `--buc`) draws the named entities' state
  models, every lifecycle column, whole. The header's scope reads `entity:<ids>`.
- With neither, the diagram is an index. Each state model is its composite state holding
  one note, `<n> 状態 / <m> 遷移 (--entity <Entity>)`, with `n` the declared values of
  the column and `m` its transitions, and nothing else: no transition, no start or end
  marker, no class. The header's kinds are `statemodel state`. Read a model's states with
  the `--entity` named in its note.
- A full model colours a state by what the lifecycle check found, one `class <state id>
  <class>` line each, with the `classDef` of each class used and a `%% legend:` comment
  before `stateDiagram-v2`:

  | Class | State | Found by |
  |---|---|---|
  | `stateunreachable` | A declared value no reachable state holds, drawn with no transition | `UnreachableEnumVariant` (a `warning`) |
  | `statedeadlock` | A state an instance cannot leave though it has a way out | `Deadlock_<Entity>_<column>_<value>` |
  | `stateend` | A state with no transition to another state, the one drawn `--> [*]` | The transitions |

  A state has one class, the first that applies in the order deadlock, unreachable, end.

---

## Sample Artifact Snapshots

`scripts/sample-artifacts.tsv` lists every committed `out/` file of `samples/ec-site`,
`samples/clinic-ops`, `samples/patent-ops`, `samples/food-permit-ops`,
`samples/insurance-claim-ops`, `samples/oidc-auth`, `samples/personal-info`, each `samples/incremental-order`
step, and `samples/api-contract`. Each row holds the output path and the `rdra-ish` command that
writes it. CI runs `scripts/check-sample-artifacts.sh` after the Rust snapshot tests. The
script builds the binary once and runs every row. It then rewrites the documents that
embed the artifacts: `scripts/embed-sample-artifacts.py` copies each file into the fenced
block under its `<!-- embed: <path> -->` line in the step `design.md` files and
`docs/diagram-sample-review.md`, and fails when a command those documents print is not
the manifest row for its `--out`; `scripts/gen-clinic-ops-docs.py` writes
`design-doc.md` and `prd.md` for `samples/clinic-ops`, `samples/patent-ops`,
`samples/food-permit-ops`, and `samples/insurance-claim-ops` (the sample is its second argument) from `list` and `csv` output and the committed diagrams. The script fails if a tracked file changes. A generator change
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
rdra-ish csv <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `matrix` \| `api` \| `api-matrix` \| `screen-constraints` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `entity` | CSV kind. `actor` = actor list; `entity` = entity/column list; `matrix` = use-case × entity CRUD matrix; `api` = API contract list including method/path/idempotency/mode/auth; `api-matrix` = API × entity CRUD matrix; `screen-constraints` = screen × UC/API permission/medium paths; `permission-callables` = permission × callable UC/API list derived from `requires_permission`, including `UseCase->Api` paths for API-level requirements; `actor-permission-audit` = actor × permission assignment audit inferred from UC/API requirements; `business-inputs` = Actor x inferred input field x UseCase rows derived from C/U paths. |
| `--format` | `csv` \| `markdown` | `csv` | `markdown` writes the same rows as a Markdown table, to paste into a design doc or a PRD, with the extension `.<kind>.md`. A `\|` in a cell is escaped and a line break becomes `<br>`. |
| `-o`, `--out` | path | `out` | Output file path. A path with an extension is used as is (`-o x.dat` writes `x.dat`). Otherwise `.<kind>.csv` is appended to the path: `-o out` (the default) writes `out.matrix.csv` for `--kind matrix` and `out.entity.csv` for `--kind entity`, and `-o foo` writes `foo.matrix.csv`. |

The command writes the CSV, or the Markdown table, to the output path and prints `wrote <path>`;
it prints no rows to stdout. Without `-o`, `rdra-ish csv src/ --kind matrix` writes
`out.matrix.csv` in the current directory.

---

## `export`

Export machine-readable or review artifacts from the model.

```
rdra-ish export <INPUTS...> [--kind <KIND>] [-o <OUT>] [--dialect <DIALECT>] [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `openapi` \| `asyncapi` \| `dbml` \| `sql` \| `json-schema` \| `mermaid-er` \| `tla` | `openapi` | Export kind. `openapi` emits an OpenAPI 3.0 JSON document from API `method`/`path` metadata and DTO `request` / `response` / `error_response` relations. `asyncapi` emits an AsyncAPI 3.1 event catalog from `event`, `raises`, `triggers`, `transitions`, and `outbox`. `dbml` emits a DBML schema from logical `entity` declarations, indexes, unique constraints, generated FK columns, and `relate` options. `sql` emits SQL DDL from the same model, with `CHECK` constraints from the entity-local rules (see [SQL export](#sql-export)). `json-schema` emits JSON Schema Draft 2020-12 definitions for DTO and Entity structures. `mermaid-er` emits a textual ER review artifact from the same logical data model projection used by `diagram --kind er`. `tla` emits a TLA+ module plus TLC config from entity lifecycles and state constraints (see [formal-verification.md](./formal-verification.md)). |
| `-o`, `--out` | path | `out` | Output file path. For every kind except `tla`, a path with an extension is used as is. Otherwise the kind's suffix is appended to the path: `-o out` (the default) writes `out.openapi.json`, `out.asyncapi.json`, `out.schema.dbml`, `out.schema.sql`, `out.json-schema.json`, or `out.er.mmd`, and `-o foo` writes `foo.openapi.json`. For `--kind tla`, a path ending in `.tla` is the TLA+ file, uses its file stem as the module name, and gets a sibling `.cfg` (`-o m.tla` writes `m.tla` and `m.cfg`). Any other path is a directory (created if missing) that receives `RdraSpec.tla` and `RdraSpec.cfg`: `-o out` (the default) writes `out/RdraSpec.tla`. A path with a non-`.tla` extension writes them into its parent directory. Beside the pair, `tla` writes `StateModel_<E>.tla`, `StateModel_<E>.cfg`, and `StateModel_<E>_witnesses.cfg` for each entity cone with a state model, which check its deadlocks and witnesses with TLC ([formal-verification.md](./formal-verification.md#state-models-with-tlc-alone)). |
| `--dialect` | `postgres` \| `sqlite` | `postgres` | SQL only: the dialect `--kind sql` writes. |
| `--instance-count` | unsigned integer | `2` | TLA+ only: multi-instance `InstanceCount` (`1..=255`). Value `1` is accepted with a warning (singleton ids cannot distinguish multi-entity instances). |
| `--instances` | `<entity>=<n>` (repeatable) | `instances` under `[verify]` in `rdra.toml` | TLA+ only: one entity's slots in place of `--instance-count`, as `Loan=4` (see [Per-entity instance counts](#per-entity-instance-counts)). |
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

#### SQL export

`export --kind sql` writes one script that creates the logical data model in an empty
database: `CREATE TABLE` per `entity`, in id order, then the indexes, then (PostgreSQL) the
foreign keys. Every sample's script loads into SQLite, and into PostgreSQL 16.

| Model | PostgreSQL | SQLite |
|---|---|---|
| `Int` | `INTEGER` | `INTEGER` |
| `String` / `String(n)` | `TEXT` / `VARCHAR(n)` | `TEXT` / `VARCHAR(n)` |
| `Money`, `Decimal` / with `(p, s)` | `NUMERIC` / `NUMERIC(p, s)` | `NUMERIC` / `NUMERIC(p, s)` |
| `DateTime`, `DateTime(tz)` / `DateTime(local)` | `TIMESTAMPTZ` / `TIMESTAMP` | `TEXT` (ISO 8601) |
| `Date` | `DATE` | `TEXT` (ISO 8601) |
| `Bool` | `BOOLEAN` | `BOOLEAN` |
| `Enum(...)` | `CREATE TYPE "<Entity>_<column>" AS ENUM (...)` | `TEXT` with `CHECK ("<col>" IN (...))`, named `<Entity>_<column>_values` |
| variation `V` | `CREATE TYPE "V" AS ENUM (...)` | the same as an `Enum` |

- **Keys.** `@pk` / `@pk(a, b)` is the `PRIMARY KEY`. `@unique`, `@unique(a, b)`, a junction
  pair, and a 1:1 key are `UNIQUE`. Each `relate` key is a `FOREIGN KEY` named
  `<Child>_<columns>_fkey` with its `.on_delete` / `.on_update` action, referring to the
  parent's primary key or to the columns `.references(...)` names. PostgreSQL adds it with
  `ALTER TABLE` after every table exists; SQLite declares it in the table.
- **Soft delete.** On an entity with a `@soft_delete` column, a declared unique set holds
  among live rows: `CREATE UNIQUE INDEX "<Entity>_<columns>_live_key" ... WHERE` the marker
  is null (or `FALSE` for a `Bool` marker), so a deleted row frees its value. The primary
  key, a key a `relate` makes unique, and a set a foreign key refers to stay a full
  `UNIQUE`.
- **Names.** A table takes the entity's `table` name when it has one, else the entity id,
  and so do its key, index, and Enum-check names. Rule constraints keep the entity id, the
  name `check` reports.
- **Computed values.** `@default(now())` is `DEFAULT CURRENT_TIMESTAMP` (`CURRENT_DATE` on a
  `Date`). `@generated(identity)` is `GENERATED BY DEFAULT AS IDENTITY` in PostgreSQL; in
  SQLite a single `INTEGER` primary key is the rowid, which SQLite numbers.
  `@version` is `DEFAULT 1` with a comment that each update checks it and adds 1.
  `@generated(uuid)` and `@default(uuid())` are `DEFAULT gen_random_uuid()::text` in
  PostgreSQL. What a dialect cannot compute (a UUID in SQLite, a ULID, `@on_update(now())`)
  stays as a comment on the column, for a trigger or the application.
- **Columns.** A column without `@null` is `NOT NULL`. `@default(v)` is `DEFAULT v`. A
  `@derived` column is `GENERATED ALWAYS AS (...)` (`STORED` in PostgreSQL, `VIRTUAL` in
  SQLite); a logical one keeps SQL's three-valued logic, as its value does in the model.
  `@label`, `@soft_delete`, `@history`, `@tenant`, and `@derived` stay as a comment on the
  column.
- **Indexes.** `@index` / `@index(a, b)` is `CREATE INDEX "<Entity>_<columns>_idx"`. Each
  foreign key gets one too, unless the primary key, a unique set, or an `@index` already
  starts with its columns.
- **Rules.** Each entity-local rule is a `CHECK` constraint named as `check` and `verify`
  name it: `@check` (`Check_<E>_<column>_<n>`), `forbidden(E, ...)` (`Forbidden_<E>_<n>`),
  `invariant(E).when(...).then(...)` (`Inv_<E>_<n>`), `required` (`Required_<E>_<n>`), and
  `exclusive` (`Exclusive_<E>_<n>`, at most one condition holds). A rule reads a null
  operand as false, except `!=` (see [Null](./language-reference.md#null)), while SQL reads
  it as unknown, which a `CHECK` accepts; so each comparison on a nullable column spells out
  its null case: `qty: Int @null @check(qty >= 1)` is
  `CHECK (("qty" IS NOT NULL AND "qty" >= 1))`, and a row with a null `qty` is rejected.
  `col in (a, b)` is `"col" IN ('a', 'b')` and `not in` is `NOT IN`, with the same null
  case. A rule's `.label("...")` is a comment on the line above its `CHECK`.
- **Reads.** The script ends with the model's reads as comments. First the indexes the
  lookups want and no key or `@index` gives, as `-- CREATE INDEX ... ; -- <readers>`: the
  `reads` [query shapes](./language-reference.md#query-shapes), each use case a timer
  starts that finds rows by their state, each `.has` / `.none` over an entity of
  `rows` 1,000,000 or more, and each guard that compares an entity no written row joins. An entity under 10,000 `rows` adds `small table, <n> rows: a
  scan is cheap` to the line. Then, when `reads` declare shapes, each read as the `SELECT` it runs:
  the tenant and `.by` columns compared to parameters (`$1` in PostgreSQL, `?1` in SQLite),
  live rows only on an entity with `@soft_delete`, then `ORDER BY` and the page. A cursor
  page continues after the last row: `("created_at", "id") < ($2, $3)` for a sort one way,
  one disjunct per key for a sort that mixes directions. The first page passes a key before
  every row. `.count()` and `.sum(column)` select `count(*)` and `sum("column")`. A read by an API
  selects the columns its response DTO fields map, else `*`.
- **What the schema does not enforce.** A rule over several entities (`MultiForbidden_*`,
  `MultiInv_*`), a `.has` / `.none` (`Quantifier_*`), a minimum cardinality (`MinCard_*`), a
  rule with a `.when(Condition)` guard, and a rule that reads `now` have no `CHECK` form. The
  script ends with a comment that lists each by name, with the reason, its label when it
  has one, and the rule, as an obligation of the application or a trigger.
- **Layout.** Comments head the parts of the script: `-- Types` (PostgreSQL), `-- Tables`,
  and `-- Foreign keys` (PostgreSQL). A variation's label and its values' labels are a
  comment above its type, `-- PaymentMethod (Payment method): cash = Cash, card`; SQLite,
  which has no enum type, lists those comments under `-- Value sets`.


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

`--kind typescript-states` (also `type-script-states`) was removed: it is a usage error
(exit 2) that writes nothing and names the rewrite, `codegen <INPUTS> --lang ts -o
<FILE>.ts` (see [`codegen`](#codegen)).

The command writes the artifact to the output path and prints `wrote <path>`.

---

## `verify`

Check every invariant, `after` assertion, and property of a model, each on its own cone,
with the native checker. No JVM starts. `--with-tlc` runs TLC beside it as a second
opinion. See [formal-verification.md](./formal-verification.md).

```
rdra-ish verify <INPUTS...> [--with-tlc] [-o <OUT>] [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>] [--max-patterns <N>] [--max-states <N>] [--temporal-budget <SECS>] [--deny-undecided] [--prove [--deny-unproved] [--z3 <PATH>]] [--tlc-jar <PATH>] [--memory <SIZE>] [--symmetry <MODE>] [--buc <ID>]... [--entity <ID>]... [--without-usecase <ID>]... [--without-buc <ID>]... [--actor <ID>]...
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--with-tlc` | flag | off | Also run TLC on every cone and property run the native checker makes, on each `stable`, `unchanged`, and `precedes` on its cone, and on the state models (deadlocks and witnesses) of each entity cone BFS explored to the end, up to 100,000 states, and fail the run (exit 1) when the two decide a check differently. `can_reach` has no TLC run. Requires `tlc` or `tlc2` on `PATH`, or `tla2tools.jar` named by `--tlc-jar` (see [Getting TLC](#getting-tlc)); without one the run exits 1. |
| `--backend` | `tlc` | none | Deprecated. `--backend tlc` is a spelling of `--with-tlc` that prints `warning: --backend tlc is deprecated: ...` on stderr. The next major release removes it. |
| `-o`, `--out` | path | none; under `--with-tlc` a temp dir, deleted when TLC passes every check and kept when it does not | Directory for the specs, for a TLC run by hand: one `Cone_<check>.tla` / `.cfg` pair per cone, named after the first invariant or after-assert that reads it, one `Cone_<Property>.tla` / `.cfg` pair per `always`, `eventually`, and `leads_to` with a `Cone_<Property>_WF` pair beside it for an `eventually` or `leads_to` and a `Cone_<Property>_Full` pair for a reduced cone, under `--with-tlc` a `Cone_<Property>` pair per `stable`, `unchanged`, and `precedes` and the `StateModel_<E>` specs of each entity cone TLC checks, `cones.txt` listing each module and the checks it answers, and, when the model declares temporal properties, the whole-model `RdraSpec.tla` / `.cfg` with every `PROPERTY` and a `RdraSpec_<Property>` pair for each property its cone leaves undecided. A path ending in `.tla` writes into its directory and names the whole-model module after its stem, and each whole-model property module `<stem>_<Property>`. With `-o`, stdout says `wrote <n> specs and <dir>/cones.txt`. The native checker reads none of these files. |
| `--instance-count` | unsigned integer | `2` | Multi-instance `InstanceCount` (`1..=255`; `1` warns). |
| `--instances` | `<entity>=<n>` (repeatable) | `instances` under `[verify]` in `rdra.toml` | One entity's slots in place of `--instance-count`, as `Loan=4` (see [Per-entity instance counts](#per-entity-instance-counts)). |
| `--int-range-max` | unsigned integer | `5` | Upper bound for `IntRange == 0..N` (`1..=255`; `0` rejected). |
| `--max-states` | unsigned integer | none | The state cap of every BFS run `verify` makes: each entity, safety, and property cone (`_Full` and `_WF` included) and the whole-model run. Without it BFS has no state cap: `--memory` stops a cone, and `--memory` and `--temporal-budget` stop the whole-model run, as `not decided`. The `bounds:` line says `no state cap per BFS cone, memory <limit>`, or the cap given. Under `--with-tlc` TLC's whole-model run needs a bound, and keeps 2,097,152 states unless `--max-states` is given; TLC's cone runs have no state cap. |
| `--max-patterns` | unsigned integer | `256` | The cap on each entity's distinct patterns in a run that builds an entity view, as `check` and `states` do. No run of `verify` builds one, so the flag stops none of them; it is accepted for scripts that pass it. |
| `--temporal-budget` | unsigned integer | `300` | Seconds a temporal property's whole-model run gets when its cone does not decide it. Past them, or past `--max-states` states or `--memory`, the run stops and, unless it had already found a counterexample, the property is `not decided`, which warns and does not fail the run. Under `--with-tlc` TLC's whole-model run gets the same budget, and its state count is read from its progress lines (about one a minute). |
| `--tlc-jar` | path | `RDRA_TLC_JAR`, else none | With `--with-tlc`: run TLC as `java -cp <PATH> tlc2.TLC` from `tla2tools.jar` instead of a `tlc` / `tlc2` launcher on `PATH`. Needs `java` on `PATH`. Without `--with-tlc` it prints a warning and starts no TLC. |
| `--memory` | size | `memory` under `[verify]` in `rdra.toml`, else `16GiB` | What the run may hold in memory, such as `16GiB` or `512MiB` (`G` and `GB` also mean GiB). BFS takes all of it; under `--with-tlc` BFS takes half and the TLC cone runs going at once the other half, at least `64MiB`. A BFS cone past its share stops and leaves its findings undecided. See [Memory limit](#memory-limit). |
| `--symmetry` | `never`, `always` | `symmetry` under `[verify]` in `rdra.toml`, else `never` | Experimental. Explore a safety cone (on a reduced one, its T rows), the unreduced cone of an `always` property, and the cone of an `eventually` or `leads_to`, up to instance symmetry: one stored state per class of states that differ only in which slots hold which rows. `always` does it on every such cone that no plain run decides; `never` changes nothing. A cone run up to symmetry prints `(about <m> states)`. See [Instance symmetry](#instance-symmetry). |
| `--deny-undecided` | flag | off | Fail the run (exit 1) when it leaves a finding undecided: a temporal property past `--temporal-budget`, or a check, state, deadlock, or use case a BFS cone left at its state cap or the memory limit. Findings a `--buc` or `--entity` scope leaves open do not count. Without it, `verify` prints `warning: <what> undecided; the OK line holds only for what was decided (--deny-undecided fails the run)` on stderr and passes. |
| `--prove` | flag | off | Experimental. Beside the cones, try to prove each selected state rule (`forbidden`, `invariant`, `required`, `@check`, quantifier rules, `Unique_*`, `MinCard_*`) for any number of rows and any non-negative Int value (`now` has no top) with z3, and print one `proved:`, `unproved:`, or `out of scope:` line per rule. The `OK:` line gains `; <n> of <m> rules proved for any number of rows and any Int value`. A rule the SQL export's schema does not enforce is marked `[not enforced by the SQL schema: <why>]`, and a `proof: the SQL schema leaves ...` line counts those rules. An invariant cone BFS stopped at the memory limit is `ok` when z3 proves every rule in it (see [Memory limit](#memory-limit)). Each z3 check has a fixed budget of z3 resource units, not of time, so the verdicts do not depend on the machine or its load. Needs z3 (see [Getting z3](#getting-z3)). See [Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows). |
| `--deny-unproved` | flag | off | With `--prove`: fail the run (exit 1) when a rule is unproved. A rule out of scope does not count. |
| `--z3` | path | `RDRA_Z3`, else `z3` on `PATH` | With `--prove`: the z3 binary to run. |
| `--buc <id>` | string (repeatable) | — (whole model) | Keep only the use cases reachable from the BUC. See [Partial runs](#partial-runs). |
| `--entity <id>` | string (repeatable) | — (all entities) | Check only the entity's view and the invariants and after-asserts that read it, each on its selected cone. See [Partial runs](#partial-runs). |
| `--without-usecase <id>` | string (repeatable) | — | Assume the use case never runs and leave it out. See [Partial runs](#partial-runs). |
| `--without-buc <id>` | string (repeatable) | — | Assume the use cases the BUC reaches, and no other BUC does, never run. See [Partial runs](#partial-runs). |
| `--actor <id>` | string (repeatable) | — (every actor) | Keep only the use cases the actor performs and those no actor performs. See [Partial runs](#partial-runs). |

A `holds` body applied to a use case is a guard of the step relation, and of the spec TLC
checks under `--with-tlc`. `verify` prints no notice for conditions.

Warnings and errors of the lifecycle transition system are printed on stderr, and an
error stops `verify` before any cone runs. Each invariant and `after(..).assert` is checked
on its own cone of influence, a restriction of the system whose reachable states are the
whole model's projected onto it (a superset where an action of the cone creates an entity
the cone does not read, since the cone does not bound that entity), with the cones explored
side by side. A `.lemma()` rule gets no cone: `check` checks it, and `--prove` uses it once
it proves it. Each `always`, `eventually`, and `leads_to` is checked on a cone of its own,
of what it reads and of the use cases its `.fair` lists, in one run (`Cone_<P>`) or two
(`Cone_<P>_WF` adds `WF_vars(Next)`), and only when neither decides it on the whole model
(`RdraSpec_<P>`), after the cone runs, under `--temporal-budget`; see
[formal-verification.md](./formal-verification.md#temporal-properties-on-a-cone) for what
decides. The labels name the runs and the modules `-o` writes, whichever engine runs them.
`stable`, `unchanged`, and `precedes` are decided by BFS on their cones, with no run of
their own: `verify` prints `order and stability properties: <n>, each decided by BFS on its
cone` and a `property <P> (BFS on its cone): ok|violated|not decided: ...` line per property
(see [formal-verification.md](./formal-verification.md#order-and-stability-properties)).
Under `--with-tlc` TLC checks each one on the same cone, module `Cone_<P>`, and the state
models of the entity cones, one `tlc: StateModel_<E> ...` line per cone (see
[formal-verification.md](./formal-verification.md#state-models-with-tlc-alone)).
`can_reach` is decided by BFS on its cone too, with no TLC run: `verify` prints
`can_reach properties: <n>, decided natively by BFS on each one's cone (TLC cannot check AG
EF)` and `can_reach (BFS on <k> cones): <n> checked, <v> violated`, with `, <u> not decided`
when a cone stopped at a cap or the memory limit; each such property is listed under
`not checked:` (see
[formal-verification.md](./formal-verification.md#recoverability-on-a-cone)).
Stdout names the bounds, the cone count, and each run's result. A whole-model run the budget
stops before it decides prints `not decided` and is listed under `not checked:`.

Every run that reaches its checks prints one `result: ok|not decided|counterexample|tool_error`
line, the verdict of the whole run: `ok` on stdout when the run exits 0 with nothing
undecided, otherwise on stderr. `counterexample` means a check is violated or a state-model
finding of the entity cones, below, fails the run (a deadlock, a state with no witness, a
`can_reach` error). `not decided` means nothing failed, but a cap, the budget, or
`--deny-unproved` left something open: the run exits 0, or 1 under `--deny-undecided` or
`--deny-unproved`, and never prints `ok`. `tool_error` means a native counterexample failed its
own check, the engines disagree, or a TLC run failed without a model violation: a tool bug
or a broken run. A run that fails or leaves a finding undecided never prints `result: ok`.
Under `--with-tlc` a `tlc_result: ok|counterexample|tool_error` line of the same form
follows it with TLC's verdict, and each TLC run prints its own line prefixed `tlc: `, after
the native lines.

`verify` also explores each entity's cone with BFS at the same bounds, with no state cap but `--memory` (or `--max-states`), and prints what it finds before the cone lines; the `bounds:` line names
the cap as `no state cap per BFS cone, memory 16GiB`, or `<N> states per BFS cone` under `--max-states`. It reads off each cone's run whether a rule, an
`after` assertion, or a property holds there and says nothing, and warns `Vacuous_<name>`
(see [Rules that hold vacuously](./state-derivation.md#rules-that-hold-vacuously)). A state
of a state model with no witness there, and a deadlock, print as in `check` and fail the run
even when every check holds: stderr carries `result: counterexample`, no `OK:` line
follows, and the exit status is 1. A deadlock at the bound and a use case that never runs print as in
`check`, as a `warning:` that does not fail the run. A cone that stops at the state cap or the memory limit lists each
state, each deadlock, and each use case it leaves undecided under `not checked:` on stdout.
Then one summary line follows on stdout:

```
state models (BFS on 2 entity cones): 2 state models, 5 states witnessed, 0 deadlocks, 0 use cases that never run
```

The line counts the distinct entity cones, the columns with a state model, the states with
a witness, the deadlocks (errors and warnings at the bound), and the `NeverRuns` warnings.

Each violated invariant or `after` assertion of a cone prints once in model terms, through
the renderer `check` uses, with BFS's shortest trace to it. A check several cones hold is
reported from the first cone that breaks it. The run fails with `verification failed: <n>
invariant or after-assert violation(s)`, and with `<n> property violation(s)` for the
properties:

```
$ rdra-ish verify samples/formal-verification/cross_order_payment_split.rdra
...
Cone_MultiForbidden_Order_0 [MultiForbidden_Order_0]: counterexample (31 states)
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order[1](status=cancelled); Payment[1](status=captured)
  rule: skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:44:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:17:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:18:1
         Payment#1.status: pending -> captured               skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:39:1
    3. Cancel [Order#1]                                      skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:19:1
         Order#1.status: pending -> cancelled                skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:38:1
result: counterexample
Error: verification failed: 1 invariant or after-assert violation(s)
```

A violated temporal property prints in model terms the same way, as
`error[<Property>]: property <Property> is violated: ...`. The head names the instance and
the step that shows the failure (for `leads_to`, the step after which the trigger holds).
An `always` counterexample ends at the state that breaks it. An `eventually` or `leads_to`
counterexample is a lasso: it ends in a loop, whose step back prints as the last numbered
step followed by `then loops back to step K` (the behavior repeats steps K through the last
one forever), or in a stop, `then nothing changes forever`. A native counterexample that
does not replay, does not violate the property, or loops unfairly prints
`error[LassoFault]: <module> [<P>]: ...; a tool bug, not a finding about the model`, and
the run fails with `result: tool_error`. An example is in
[formal-verification.md](./formal-verification.md#what-verify-checks).

### With TLC

Under `--with-tlc`, `verify` writes the specs and runs TLC on the modules above while BFS
runs, then the whole-model module of each property TLC's cones leave undecided. Each cone's
TLC runs with `-continue`, so every violated check of a cone prints its own counterexample.
TLC reports one violated check per state or step, the first in the cfg, so a cone run with a
violation is rerun without the checks it has reported (`<module>_rest<n>.cfg` next to the
spec) until a rerun reports no new one.

`verify` then compares the two engines check by check: each safety cone's verdict and the
checks its counterexample breaks, and each property's verdict on every run both made and in
the end. A check one engine leaves `not decided` is no disagreement. A check with two
verdicts prints `error[EngineDisagreement]: <module> [<checks>]: native <verdict>, TLC
<verdict>; one engine is wrong (a tool bug, not a finding about the model)`, and neither
verdict is the result: both lines read `tool_error`, the specs are kept, and the run fails
with `<n> check(s) the native checker and TLC decide differently (a tool bug)`.

TLC's failed runs print after the native findings, each under `tlc: <module> (<path>):`:
the counterexample in model terms, decoded from TLC's states and replayed through the step
relation BFS uses (the lowest binding when several give the same state), marked `from TLC`,
then TLC's own `violated: Error: ... is violated.` line and raw trace. A TLC run that fails
without a model violation fails the run with `<n> TLC run(s) that failed without a model
violation`. A run in which TLC does not pass, or the engines disagree, ends with
`specs: <dir>`; without `-o` it adds `(kept because a check failed)` or `(kept because the
engines disagree)`. No other run prints a `specs:` line.

```
$ rdra-ish verify samples/formal-verification/cross_order_payment_split.rdra --with-tlc
...
tlc: Cone_MultiForbidden_Order_0 [MultiForbidden_Order_0]: counterexample (31 states)
tlc: StateModel_Order (31 states): 1 deadlock and 3 witness checks; found none
tlc: StateModel_Payment (31 states): 1 deadlock and 2 witness checks; found none
tlc: Cone_MultiForbidden_Order_0 (/tmp/rdra-ish-tlc-4242/Cone_MultiForbidden_Order_0.tla):
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: ...
  rule: skills/rdra-ish-verify/samples/cross_order_payment_split.rdra:44:1
  counterexample (3 steps, from TLC):
    ...
violated: Error: Invariant MultiForbidden_Order_0 is violated.
counterexample:
  State 1: Initial predicate
    Order_exists[1] = "absent"
    ...
result: counterexample
tlc_result: counterexample
specs: /tmp/rdra-ish-tlc-4242 (kept because a check failed)
Error: verification failed: 1 invariant or after-assert violation(s)
```

If a TLC step matches no move of the cone, `verify` prints `error: TLC step <n> (label
<action>) matches no move of the cone; BFS and TLC disagree (a tool bug). Raw TLC trace
follows.` in place of the model-term block. TLC's lasso ends with `State N: Stuttering` or
`Back to state K: <action>`, and `verify` replays it the same way.

| Environment variable | Effect |
|---|---|
| `RDRA_TLC_HEAP` | With `--with-tlc`: heap of every TLC JVM, such as `4g`. Without it a run's heap follows its module: 512 MiB per MiB of module text in whole GiB, at least `1g` for a cone and `2g` for a whole-model run, at most what the [memory limit](#memory-limit) gives TLC, and twice that once after the run runs out of memory. The runs queue so that their heaps fit the limit. |
| `RDRA_TLC_JAR` | With `--with-tlc`: path to `tla2tools.jar`, used as `--tlc-jar` when the flag is absent. |
| `RDRA_Z3` | Path to the z3 binary, used as `--z3` when the flag is absent. |
| `RDRA_JOBS` | How many cones `check`, `states`, and `verify` explore at once by BFS, how many threads compute the moves of one large cone, and how many z3 processes `verify --prove` starts. It does not limit the TLC runs, which `verify --with-tlc` starts beside the BFS. Without it, the machine's cores, at most 8. Each cone holds its reached states, so a lower number uses less memory; `1` explores them one after another on one thread. The result does not depend on it. |
| `JAVA_TOOL_OPTIONS` | Appended after the options `verify --with-tlc` sets (`-Xmx`, `-Djava.io.tmpdir`, and `-XX:TieredStopAtLevel=1` on a state-model run of at most 10,000 states by BFS), so a value here wins. `verify` adds `-XX:TieredStopAtLevel=1` only after TLC started once with it, so a JVM that does not take it runs without it. |

A TLC run that runs out of memory runs once more at twice its heap when the memory limit
holds that; when it does not, or the rerun runs out too, the run is a `tool_error` whose
message names both variables.

#### Getting TLC

Only `--with-tlc` needs TLC. `verify` looks for it in this order: `--tlc-jar`,
`RDRA_TLC_JAR`, then `tlc` and `tlc2` on `PATH`. Homebrew has no formula that puts `tlc` on
`PATH` (the `tla+-toolbox` cask is the GUI). Use one of these:

- Nix: run `nix shell nixpkgs#tlaplus`, then run `verify` in that shell. If flakes are not
  enabled, add `--extra-experimental-features 'nix-command flakes'`. In this repository,
  `nix develop` provides `tlc`, and the flake's package puts the same `tlc` on the `PATH` of
  `rdra-ish`: TLC 1.8.0 on JDK 25, with a class-data-sharing archive of TLC's classes,
  which starts each run sooner. `nixpkgs#tlaplus` is still TLC 1.7.4 on JDK 8.
- The jar: download `tla2tools.jar` from the
  [TLA+ releases](https://github.com/tlaplus/tlaplus/releases), install a JDK, and pass
  `--tlc-jar <path>` or set `RDRA_TLC_JAR=<path>`. TLC 1.8.0 needs JDK 11 or later.

`verify` reads the output of TLC 1.7.x and of TLC 1.8.0. TLC 1.8.0 is a pre-release that
upstream rebuilds under the same tag. The flake pins the build of 2026-11-19 (`TLC2 Version
2026.11.19.180144`) by its hash and downloads it from this repository's release
[`tlc-1.8.0-bc1b506`](https://github.com/manji-0/rdra-ish-dsl/releases/tag/tlc-1.8.0-bc1b506)
first, so a rebuild upstream does not break it.

Use TLC 1.8.0, or run TLC 1.7.x on JDK 17 or earlier. On JDK 21 and later, TLC 1.7.x's
own buffered file reads a trace record back wrongly once the JDK's
`RandomAccessFile.readLong` goes through it, and a `-continue` run that prints many traces
stops with `java.lang.ArrayIndexOutOfBoundsException` (`arraycopy: length -N is negative`
or `Index N out of bounds for length M`) (#314).

- `verify` prints `warning: TLC 2.19 (TLC 1.7.x) runs on Java 21.0.11; …` once, when the
  first TLC run's banner names TLC 1.7.x (`TLC2 Version 2.x`) and a JVM of 21 or later.
- A run that stops on that exception is a `tool_error`, even when it had printed
  violations before it: it never reached the rest of the model. Its report starts with
  `hint: TLC stopped with an ArrayIndexOutOfBoundsException while it read a trace back, …`
  and says to use TLC 1.8.0 or to run TLC 1.7.x on JDK 17 or earlier.

#### Getting z3

`verify --prove` looks for z3 in this order: `--z3`, `RDRA_Z3`, then `z3` on `PATH`. Without
one, the run stops with an error. Use one of these:

- Nix: run `nix shell nixpkgs#z3`. In this repository, `nix develop` provides `z3`.
- A package manager: `apt install z3`, `brew install z3`, or `pip install z3-solver`, which
  installs a `z3` command.

---

## `list`

List model elements in a human-readable form.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format <FORMAT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `requirement` \| `adr` \| `adr-impact` \| `goal` \| `nfr` \| `quality` \| `constraint` \| `buc` \| `flow` \| `step` \| `usecase` \| `field` \| `system` \| `api` \| `dto` \| `variation` \| `screen` \| `event` \| `timer` \| `condition` \| `rule` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` \| `buc-roles` \| `transition` \| `surface` | `actor` | The element kind to list. `quality` / `flow` / `step` / `system` / `variation` / `screen` / `event` / `timer` list id+label; `actor` lists id, label, and description; `buc` lists id, label, the business it belongs to, the use cases it contains, and description; `usecase` lists UC preconditions, guards, postconditions, alternatives, errors, description, and the BUCs that contain it, the screens it displays, and the APIs it invokes; `requirement` lists requirement metadata; `adr` lists decision metadata plus impacted targets from `decides`; `adr-impact` lists one row per ADR-target pair; `nfr` and `constraint` list non-functional metadata; `field` lists screen-field metadata plus Entity.column mappings; `api` lists API contract metadata; `dto` lists DTO fields; `condition` lists the `holds` body, the variations and state models it combines (derived from the body, or from `combines` when there is none), and the targets of `applies_to`; `rule` lists the business rules as one table with the columns `kind,label,entities,rule,applies_to,site`: every `forbidden`, `invariant`, `required`, `exclusive`, `invariant(...).when(...).none/has(...)` (`quantifier`) and `after(...).assert(...)` with its `.label("...")` (empty when it has none), plus every condition with its label, `rule` is the formula, `applies_to` is the use case an `after` follows or the use cases and APIs a condition guards, and `site` is `file:line`; `entity` lists each column with type, PK/unique/index/FK flags, FK optionality/actions, check constraints, soft-delete/history/tenant markers, and derived expressions; `permission-callables` lists each permission with callable use case/API ids and API-level `UseCase->Api` paths derived from `requires_permission`; `actor-permission-audit` lists inferred actor-side assignment gaps; `business-inputs` lists inferred actor-entered fields; `surface` prints the published surface (`exposes` and `shares`) as canonical text. |
| `--format` | `table` \| `json` \| `csv` \| `markdown` | `table` | Output format. `markdown` is a Markdown table with the CSV columns, to paste into a design doc or a PRD. `--kind surface` has the canonical text only, and any other `--format` exits `2`. |

Output is written to stdout.

`--kind goal` has the columns `id,label,metric,baseline,target,window,description,requirements`.
`requirements` holds the ids of the requirements that achieve the goal, joined with `|`.
`--kind requirement` ends with the columns `rationale,goals,bucs,description`; `goals` holds
the ids of the goals the requirement achieves and `bucs` the BUCs it motivates, each joined
with `|`.
`--kind actor` has the columns `id,label,description`, and `--kind buc` has
`id,label,business,usecases,description`, with the use cases in declaration order.
`--kind usecase` ends with the columns `description,bucs,screens,apis`.

`--kind buc-roles` is a matrix. The rows are the actors, then the external systems, each
sorted by id; the columns are `role` (the label), `kind` (`actor` or `extsystem`), and one
column per BUC, headed by its label. The BUCs run business by business (businesses sorted by
id, each BUC under the first business it belongs to, BUCs of no business last). A cell says
how the row takes part in the BUC, and is empty when it does not: `担当` for an actor that
`performs` the BUC, a use case in it, or a step in it; `イベント送信` for an external system
that raises an event starting one of the BUC's use cases or the BUC; `イベント受信` for an
external system that an event raised by one of its use cases reaches. An external system
that does both has both, joined with `、`. Every `--format` works; `markdown` pastes into a
PRD or a design document, and the other formats take the BUC labels as column names.

`--kind transition` is the state-transition table behind `diagram --kind state`: one row
per transition the diagram draws, with the columns `entity,column,from,to,usecase,guards,actors,site`.
The rows run by state model (entity id, then column), then by the Enum order of `from` and
`to`, then by use case id. `from` and `to` are `label (value)`, such as `予約済
(scheduled)`; a value with no label shows the value twice. `usecase` is `label (id)`.
`guards` holds the labels of the conditions whose `holds` body guards the use case, the
same labels the diagram writes in `[...]`; a condition without a `holds` body guards
nothing. `actors` holds the labels of the actors that perform the use case: a direct
`performs`, plus the performers of each BUC that contains it, except that a use case an
event or a timer starts takes only its direct `performs`. `site` is the `transitions`
declaration as `file:line`. Several values in a cell are joined with `|`. Every `--format`
works.

For `--format table`, an empty result is explicit rather than silent, for example
`No APIs found.`, except for `--kind surface` (see below). CSV still prints only the
header row, Markdown prints the header row and its separator, and JSON prints `[]`.

### Surface text

`--kind surface` prints one line per published element, sorted by kind and then id, so
declaration order cannot change the text. The kinds come in a fixed order: `usecase`,
`event`, `entity`, `writer`, then `shared`. A model that publishes nothing prints nothing;
the surface is the one `--format table` result with no empty-result line, because the
text is the contract and an empty contract is empty. The format is frozen: a consumer's
lock records this text.

```text
usecase Login "Log in" caller(actor) requires(CanLogin) raises(EvLoggedIn)
entity SessionView "Session (published)" upsert { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }
writer SessionView Login: status == active
writer SessionView (unexposed): status == ended
```

- `usecase <Id> "<label>" caller(<kind>)`, then `requires(<permissions>)` and
  `raises(<events>)` when the use case has them, each list sorted.
- `event <Id> "<label>"` per exposed event.
- `entity <Id> "<label>" <append | upsert> { <columns> }`. Each column is
  `name: Type`, then `@pk`, `@unique`, and `@null` flags; two spaces separate columns.
- `writer <Entity> <UseCase>: <effect, ...>` per exposed use case that writes the
  entity, grouped per entity in entity order, with the use cases sorted by id. A `sets`
  on a column is `col == val`; a `sets` on a proposition is `<prop> == true` or
  `<prop> == false`; the effects are sorted. The writers that are not exposed collapse
  into one `writer <Entity> (unexposed)` line, last in the entity's group, that holds
  the union of their effects, so internal use case names never leak. A writer with no
  `sets` has no colon.
- `shared <Id> "<label>" { <columns> }` per `shares` line.
- A `<label>` is the declared label between double quotes, with a backslash written as
  `\\`, a double quote as `\"`, a newline as `\n`, and a carriage return as `\r`.
  Ids, column names, types, and values never need escaping.

---

## `states`

Derive the reachable state patterns per entity, aggregated across BUCs. See
[state-derivation.md](./state-derivation.md) for the algorithm.

```
rdra-ish states <INPUTS...> [--format <FORMAT>] [--buc <ID>]... [--max-patterns <N>] [--max-states <N>] [--memory <SIZE>] [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>] [--entity <ID>]... [--without-usecase <ID>]... [--without-buc <ID>]... [--actor <ID>]...
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `csv` \| `json` | `table` | Output format. For TypeScript state types, use [`codegen --lang ts`](#codegen): `--format typescript` (also `type-script`) was removed, and is a usage error (exit 2) that names that rewrite. |
| `--buc <id>` | string (repeatable) | — (whole model) | Keep only the use cases reachable from the named BUCs. See [Partial runs](#partial-runs). |
| `--max-patterns` | unsigned integer | 2^(10 + 8 * (n - 1)) for `n` = `--instance-count`: `1024`, `262144`, ... | Cap on each entity's distinct patterns within the cone of its entity view (each target is explored on the variables it reads; a cone of an invariant or after-assert alone gathers no pattern, so the cap does not stop it). Only entities the target reads are bounded to one instance: a use case that also creates an entity outside the cone (an audit row, say) never runs out of slots for it. When the cap is hit, the result is marked `truncated` and a fail-closed `PatternCapReached` **error** is recorded (exit non-zero for all formats). |
| `--max-states` | unsigned integer | none | Cap on the joint states of one target cone, printed in the `bounds:` line (`no state cap per cone, memory 4GiB` without it). When the cap is hit, the result is marked `truncated` and a fail-closed `StateCapReached` **error** is recorded (exit non-zero for all formats). The message names what the reached states vary in, so you can see which entity's columns a guard pulled into the cone (see the example in [Exploration by cone of influence](./state-derivation.md#exploration-by-cone-of-influence)). |
| `--memory` | size | `memory` under `[check]` in `rdra.toml`, else `4GiB` | What the run may hold in memory, such as `4GiB` or `512MiB` (`G` and `GB` also mean GiB). A cone whose states pass it stops, with a fail-closed `MemoryCapReached` error that estimates what finishing needs. See [Memory limit](#memory-limit). |
| `--instance-count` | unsigned integer | `1` | Instances (slots) per entity, `1..=255`. The `bounds:` line prints the value used. A larger value makes more states per cone and raises the default `--max-patterns` (see it above); a run that outgrows the machine stops at `--memory`. |
| `--instances` | `<entity>=<n>` (repeatable) | `instances` under `[check]` in `rdra.toml` | One entity's slots in place of `--instance-count`, as `Loan=4`. See [Per-entity instance counts](#per-entity-instance-counts). |
| `--int-range-max` | unsigned integer | `5` | The Int domain is `0..=N`, `1..=255` (`0` is rejected). |
| `--entity <id>` | string (repeatable) | — (all entities) | Derive only the named entities' views, and the invariants and after-asserts that read them, each on its own cone. Every output format holds only those views. Exit severity considers the selected targets and the build errors their cones read; a build error outside them is deferred and listed under `not checked:`. See [Partial runs](#partial-runs). |
| `--without-usecase <id>` | string (repeatable) | — | Assume the use case never runs and leave it out. See [Partial runs](#partial-runs). |
| `--without-buc <id>` | string (repeatable) | — | Assume the use cases the BUC reaches, and no other BUC does, never run. See [Partial runs](#partial-runs). |
| `--actor <id>` | string (repeatable) | — (every actor) | Keep only the use cases the actor performs and those no actor performs. See [Partial runs](#partial-runs). |

Output is written to stdout. In the table, each entity opens with an `axes:` line, the
value domain of each state axis, and a `labels:` line when an inline `Enum` axis has
labeled values (`status[pending=受付前|paid=支払済]`). Exit is based on structured severity
(all formats), not string-scanning `[error]`. stderr starts with a `bounds:` line: the instances per entity,
the Int range, the state cap per target cone (`no state cap per cone, memory 4GiB` without
`--max-states`), and that entities a check does not read are unbounded.
`check` prints the same line, and `verify` prints it on stdout with the TLA+ constants. Diagnostics that name no entity are printed
on stderr too, and so is the `not checked:` block, which lists each state of a state model,
each deadlock, and each use case the run does not decide (a capped cone, or `--buc`, which
leaves use cases out). With `--format json`, each pattern
carries a `witness`: a shortest path to the first state that holds it, as `start` and
`steps` objects with sites (see [Output formats](./state-derivation.md#output-formats)).

---

## `codegen`

Print the model's domain types as TypeScript, Rust, or Go code for an agent to finish.
Most of this subcommand is experimental. One part is covered by
[compatibility](./compatibility.md#the-typescript-types-of-codegen): the TypeScript types
that `codegen <INPUTS...> --lang ts [-o <FILE>]` prints for a model that `check` passes.
These types are the scalars, the id types, the value sets, the state types, and the
`<Entity>Row` row types. Everything else may change or be removed in any release: the
transition functions, the rule checks, the dispatchers, the header and its obligations,
`--allow-failing`, `--check` and the fingerprint line, and the Rust and Go output. [The design](./design/algebraic-codegen.md)
describes where `codegen` is going.

```
rdra-ish codegen <INPUTS...> --lang <LANG> [-o <FILE> | --check <FILE>] [--package <NAME>] [--allow-failing]
                 [--conformance <FILE> [--conformance-states <N>]]
rdra-ish codegen <INPUTS...> --format json [-o <FILE>] [--allow-failing]
rdra-ish codegen <INPUTS...> --code
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--lang` | `ts` \| `rust` \| `go` | — (required but with `--format json` or `--code`) | Language. `typescript` is an alias of `ts`. |
| `--format` | `code` \| `json` | `code` | `json` prints the contract as data instead of code: see [The JSON contract](#the-json-contract). Not with `--conformance` or `--check`. |
| `--code` | flag | off | Write every file `[code.<lang>]` in [`rdra.toml`](#the-code-tables) names (the domain code, the traces, the probe), with paths and sites relative to the directory of `rdra.toml`. Not with the other options. |
| `--probe` | flag | off | Write only the probe of each `[code.<lang>]` table that names one. Not with the other options. |
| `-o`, `--out` | path | — (stdout) | Write to this file instead of stdout. |
| `--package` | string | `domain` | The Go package name (`--lang go` only). Put the file in a directory of its own, as this package, and write the code that finishes it in another package: the fields of a state type are unexported, so only code outside the package is kept from building a state. |
| `--allow-failing` | flag | off | Generate even when `check` reports errors. The state types then narrow no column, and each error is listed as an obligation the model does not meet. |
| `--conformance` | path | — (no harness) | `--lang rust` and `go` only, on a model `check` passes: also print the conformance port and harness, and write to this file the traces the harness drives a finished service along. |
| `--conformance-states` | integer | `65536` | The states the BFS behind the traces explores at most. |
| `--check` | path | — | Generate again and compare the result with this file instead of printing it. Exit 1 when they differ. Not with `-o`. With `--conformance <TRACES>`, the traces in `TRACES` are compared too, by hash. |

The output is one compilation unit: a TypeScript module, a Rust module file, or a Go file.
It opens with a header and the **obligations**: what `check` and `verify` assume of
the data and no type enforces, each under a name and with its source site. The name is the
invariant that checks it where `check` and `verify` check one (`Unique_*`, `MinCard_*`,
`MaxCard_*`, `Check_*`, marked `[checked]`), and otherwise one built the same way from the
model's identifiers: `Ref_<Entity>_<columns>` (a key names a present parent),
`Unique_<Entity>_<columns>` (an `@unique` column or set `check` does not hold),
`OnDelete_*` / `OnUpdate_*`, `Derived_<Entity>_<column>`, `OneTransaction`,
`RunMarks_<UseCase>`, `CompleteRows_<Type>`, `RuleChecks`, and `NotMet_<error>` under
`--allow-failing`. A name does not move when the model gains an unrelated declaration, so
the [obligation ledger](#check---code) can name one. Then come the scalars a project may replace
(`Money`, `Decimal`, `Date`, `DateTime`), an id type per entity with a one-column primary
key, each variation and inline `Enum` as a value set, and each entity and DTO as a record. A
key column takes its parent's id type, and a `@null` column is `T | null`, `Option<T>`, or
`*T`.

An entity whose tag column is an inline `Enum` that `transitions` drive is a **state type**:
one variant per value of its tag column (of the columns `transitions` names, `status`, else
a `*_status` column, else the first by name), and the entity's type is their union, a
TypeScript union of interfaces, a Rust enum of structs with a `status()` method, or a Go
interface that only the generated variants implement. A variant narrows a nullable column
only where a rule says so: `invariant(E).when(status == v).then(x == present)` makes `x`
present in `v`, `forbidden(E, status == v, x == present)` makes it null there, and
`required(E, x == present)` makes it present in every state. A rule with another atom or a
`.when(Cond)` narrows nothing. A null column is `null` in TypeScript and left out of the
Rust and Go structs, with a doc line naming the rule. The Rust and Go variants hold no tag
field: the variant is the tag, and writing it to a row is the code's job. A variant's fields are
private in Rust and unexported in Go, and methods of the same name read them (`paid_at()`,
`PaidAt()`). Only a transition function and `from_row` (`<Entity>FromRow` in Go) make a
state, from the rows and the inputs the model leaves open, so code outside the generated
module or package can neither build a state by hand nor change a field a transition keeps.
In Go, a transition function or `<Entity>ToRow` given the zero value of a state (`OrderPaid{}`)
panics, `<Entity>FromRow` refuses a value outside a value set, a function refuses such a
value as an input, and every value set has `Parse<Enum>(s)`, which returns an
`InvalidValue` error for text that is no value. Records and `<Entity>Row` row types keep
public fields. An entity whose
`Enum` only `sets` writes, with no `transitions` (ec-site's `Payment`), is a plain record.

Each use case of one transaction gets **transition functions**, read from the transition
system `check` and `verify` explore. A use case gets one function per combination of the
source states of its rows (`cancelFromPending`, `cancelFromPaid`), so every argument and
result type is concrete: a call from another state does not compile. The function takes the
rows it reads, then an input for each value the model leaves open (a column `sets` makes
`present`, a variation the actor chooses, a column of a created row that no var holds). It
returns a `<Name>Result` record with the rows it writes or creates, the id of each row it
deletes, and a value of each event the use case raises. A condition applied to the use case
is checked first: when it does not hold, the function returns `Rejected` (TypeScript), an
`Err(Rejected)` (Rust), or a `*Rejected` error (Go). A row that breaks a rule the verified
model keeps (a column the target state keeps null that has a value, a column it keeps
present that is null) or a parent that is not the child's makes the function throw or
panic. A use case split into transactions gets one function per step (`openSettlement1`,
`openSettlement2`), linked by **run marks**: a step that leaves a row for the next one
returns a mark, `OpenSettlementClaimMark1 { id }`, and the next step takes it as an argument,
so it cannot be called without the step before it. A later step reads an earlier step's row
only through its mark (a child it creates under that row takes the mark's id), and the last
step returns the use case's events. The header's obligations say what the code that stores
the marks keeps: one mark of a use case per row, stored in the step's transaction; step 1
only on a row with none; and a run never abandoned, its step retried when it returns
`NotEnabled` or `Rejected`. An **event** a use case raises carries the id of each row the
raise marks (`EvOrderExecuted { certificate_order_id }`), as the event order of `check` and
`verify` records it. A use case the event triggers takes the event as an argument, after any
run marks, and checks that the row it runs on is the event's row; a row it reads only through
the event (`Triggered`) is an argument in its flat row type. Arms that leave one state for
several are a **choice**: each target is a function of its own (`decideFilingToDecidedFile`),
guarded by the `.when` conditions of its arm, and the dispatcher takes the chosen state as an
argument (`decision_to`). A **second lifecycle column** is a branch on its value: the field
takes the `to` of the arm its `from` matches, and a row in no `from` state is `Rejected` by
`TransitionFrom`. A condition that ends in `.has(..)` or `.none(..)` ranges over a **list of
related rows**, an input in the related entity's flat row type (`terminalCertAssignmentRows`):
the caller passes every present row of that entity, and the function keeps those the key
links. A use case whose vocabulary is not printed yet ( `now`, `.has` / `.none`, a second lifecycle column, a choice among arms, a
`.when`, a copy, `links`) gets no function. The header lists it with the reason, and the
code that finishes the output writes it.

Each record and state type gets a **rule check**: `orderPaidRuleViolations(row)` in
TypeScript, `rule_violations(&self)` in Rust, `RuleViolations()` in Go, which returns the names
`check` reports (`Forbidden_Order_0`, `Check_Order_qty_0`) of the entity-local rules the row
breaks, and an empty list when it keeps them all. A state type's union dispatches on its
tag. A rule its state type already guarantees is left out. A rule over several rows (a
multi-entity rule, a quantifier, `Unique_*`, `MinCard_*`) is listed as an obligation instead.

Each state type also has a **row type**, `<Entity>Row`, with every column as the data layer
declares it: the flat shape a store or the wire holds, with the same column names (and, in
Go, `json` tags). `orderFromRow(row)` / `Order::from_row(row)` / `OrderFromRow(row)` returns
the state of a row, or `InvalidRow` naming the column whose value its state rules out;
`orderToRow` / `into_row` / `OrderToRow` returns the flat row of a state.

Each function's result also carries, per row it writes, the source that row must still have
when the write commits: `order_expected` / `OrderExpected`, an `ExpectedSource` that is
`Absent` for a created row, `In { column, value }` (the tag column and the state the function
read) for a state type, and `Present` for a record. The header's obligations ask the code to
run each function in one transaction with its reads and to write each row only where it
still is what the result expects (`UPDATE .. WHERE id = ? AND status = ?`); a write that
matches no row rolls the transaction back and is `NotEnabled`.

Each use case whose functions take a state type gets a **dispatcher**, `runCancel` /
`run_cancel` / `RunCancel`, over the entity types: it calls the transition function of the
rows' states and returns its result as a `CancelOutcome` case, or `NotEnabled` when the use
case does not run from those states. In Rust every transition function and dispatcher returns
`Result<_, <UseCase>Error>` (`PayError`), an enum of `NotEnabled`, `Rejected` (when a condition
applies), and `InvalidArgument` (when the arguments can break what the model keeps), each a type
that implements `std::error::Error`; the generated Rust never panics. The code that finishes
the output loads rows, converts them, calls the dispatcher, and stores the rows it returns.
The Rust ids and scalars are opaque: `OrderId::from(7)`, `"12.50".parse::<Money>()`,
`"2026-01-01".parse::<Date>()`, which refuse text of another shape with `InvalidValue`
([Generated code style](design/algebraic-codegen.md#generated-code-style)).

In Rust, each use case with a function or an entry point also gets a **contract**, the trait
`<UseCase>UseCase` with its `prepare_*` / `commit_*` methods (and `run_<action>` for an action
with no printed function), printed with or without `--conformance`. A use case that actors
perform takes a **caller** first: `by: <UseCase>Caller`, an enum over the capabilities of those
actors (`EndUserCapability`, made only by `EndUserCapability::issue()`). Code without a
capability of an actor that performs the use case does not compile; whether the code that
calls `issue` authenticated the actor is not checked.

With `--conformance <FILE>` (Rust and Go), the output ends with a **conformance harness**
for the service that finishes it, and `FILE` gets the traces it runs. The service
implements `ConformancePort`: `seed` empties its store and stores the rows of an initial
state, `snapshot` returns a `StoreSnapshot` of every row (the `<Entity>Row` of a state type,
the record otherwise), every pending run mark (`<UseCase><Entity>Run { id, step }`), and
every event a triggered use case reads, and two methods per action run the use case by the
ids of its rows and its inputs: `prepare_pay` / `PreparePay` loads the rows and returns the
dispatcher's result, writing nothing, and `commit_pay` / `CommitPay` stores that result in one
transaction. Either returns an error when the use case does not run (in Rust, the use
case's `<UseCase>Error`). The agent's test calls `conform(|| MyService::new(), &traces)` in Rust or
`Conform(func() ConformancePort { return newService() }, traces)` in Go. The harness starts
a fresh port per trace, seeds the trace's initial state, makes each call, and after each
compares the store, cut down to the columns the model tracks (an enum's value, null or not,
an integer, a key's parent id), with the state BFS reached. A call the model refuses must
fail and leave the store as it was, and so must a commit after another call changed the
state of a row it writes (the harness prepares one call, runs another, then commits the
first). It returns the first difference, naming the traces line and the rows only the model
or only the store holds.

The traces are a text file, not code. They come from a BFS of the whole transition system
at `check`'s bounds (one row per entity), from its initial states, and take every move of a
use case with a printed function between the states those moves reach, in as few traces as
a greedy walk finds. In each state they also try every use case over one row that the model
does not run on a present row, as a call that must be refused. A BFS that stops at
`--conformance-states` covers only the states it reached and tries no refused call. The
header of the code says how many moves the traces take, and what the harness does not
compare (a counted trigger, a comparison proposition, the clock). A service whose store
abstracts to the model's state after every call passes even when it does not call the core;
the harness checks what is stored, not how.

The header also lists **candidate rules**: a nullable column that every reachable row of a
state holds present (or null) with no rule saying so, printed as the `invariant` or
`forbidden` that would narrow the state type. A column null in every reachable state is
listed once, as a column no use case writes. The modeler decides whether a candidate is a
rule of the business (state it, and the type narrows) or an accident of the model (a gap).
The list reads the states `check` reaches at its bounds.

Since the state types, functions, and rule checks rest on rules, `codegen` runs `check` first, at its bounds, and
prints them in the header. It refuses a model `check` rejects (exit 1, with the errors on
stderr) unless `--allow-failing` is given; the output then has no transition functions and
no rule checks. It refuses a model with model errors. Two declarations that generate one type name, such as an entity
`OrderStatus` beside the inline `Enum` of `Order.status`, are an error naming both.

The second line of the output is a **fingerprint**:
`// rdra-ish codegen fingerprint: model=<hash> code=<hash>`. The model hash covers the
source text of the loaded files. The code hash covers the output without this line.

`codegen --check <FILE>` checks a committed file in CI. It generates again with the same
inputs and options and compares the result with `<FILE>`, ignoring the fingerprint line.
Run it from the same directory with the same input paths, because the header names them.
`--check` writes nothing and prints nothing on stdout. When the two match, it prints
`ok: <FILE> is what the model generates` on stderr and exits 0. A model edit that changes
no generated code, such as a comment, still passes. When the two differ, it exits 1. On
stderr it prints the cause, how many lines differ, and the first difference. The causes are:

- `file edited`: the file no longer matches its own code hash. Someone changed it after it
  was generated.
- `model changed`: the model's source text differs from the text the fingerprint records.
- `generator changed`: neither the file nor the model changed, so the output of `codegen`
  did. Another rdra-ish version, other `--lang`, `--package` or `--allow-failing`, or other
  input paths cause this.
- No fingerprint line: the file was not generated by `codegen`, or the line was removed.

`file edited` and `model changed` can both appear. Restore or regenerate the file. Do not
edit it by hand: code that finishes the output belongs in other files.

A traces file carries a fingerprint too, as its second line,
`# rdra-ish codegen fingerprint: model=<hash> code=<hash>`, after the format line
`# rdra-ish conformance traces 3`. Its code hash covers the file's lines. `--check` with
`--conformance` compares the traces by that hash only, reading the file once without holding
it: a traces file can be hundreds of megabytes.

### The JSON contract

`codegen <INPUTS...> --format json` prints what the code is generated from, as one JSON
object, for an agent or a tool that generates in another language or checks code against the
model. It is experimental, as the rest of `codegen` is. `--lang` does not change it.

| Key | What it holds |
|---|---|
| `format` | `"rdra-ish codegen json 1"`; a reader refuses another. |
| `generator`, `model`, `check` | The rdra-ish version, the inputs, and whether `check` passed (`passed`, and `bounds` or `errors`). |
| `types` | The value sets (`kind: "enum"`, with `variation` and `site` for a variation), the id types (`"id"`, with `entity` and `repr`), and the records (`"entity"` or `"dto"`, with `id`, `site` and `fields`). A field has `name` (the column), `ident` (PascalCase), `type` (`{"kind": "scalar" \| "enum" \| "id" \| "rows", "name"}`) and `nullable`. |
| `states` | Each state type: `name`, `entity`, `site`, `row` (the `<Entity>Row` type), `tag`, and `variants` with their `fields` and `always_null` columns. |
| `usecases` | Every use case in model order: `id`, `label`, `site`, `kind` (`functions`, `unprinted` when an action has no function yet, or `read-only` when it writes nothing), and `actions`, each with its `functions`, its `dispatcher`, and why it is `unprinted`. |
| `functions` | Each transition function: `name`, `usecase`, `action`, `step`, `rows`, `marks`, `event`, `inputs`, `conditions`, `result`, `writes` and `raises`. |
| `obligations` | Each obligation: `name`, `checked`, `text` and `site`. |

Sites are `path:line:col`, relative to the working directory (to the directory of
`rdra.toml` under `--code`).

---

## See Also

- [language-reference.md](./language-reference.md) — the DSL syntax.
- [incremental-modeling.md](./incremental-modeling.md) — staged abstract-to-concrete modeling flow.
- [state-derivation.md](./state-derivation.md) — how `states` computes its output.
- [formal-verification.md](./formal-verification.md) — `export --kind tla` / `verify`.
- [compatibility.md](./compatibility.md) — which subcommands, flags, exit codes, and output formats are stable.
