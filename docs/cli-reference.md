# CLI reference

`rdra-ish` loads `.rdra` sources, checks them, and writes diagrams, tables, exports, state
derivations, verification results, and code. This page says what each command does. The
[language reference](./language-reference.md) says what the DSL means.

```
rdra-ish <COMMAND> <INPUTS...> [OPTIONS]
rdra-ish --version        # or -V; prints "rdra-ish <version>"
rdra-ish <COMMAND> --help # or -h; every command takes it
```

| Command | What it does |
|---|---|
| [`check`](#check) | Type-check the model and explore its lifecycle. Writes no file. |
| [`lint`](#lint) | Report coverage and review-readiness findings. |
| [`next`](#next) | Show each BUC's modeling stage and what the next stage asks for. |
| [`files`](#files) | Print the `.rdra` files the inputs load. |
| [`fmt`](#fmt) | Format `.rdra` files. |
| [`diagram`](#diagram) | Write a Mermaid diagram. |
| [`csv`](#csv) | Write a CSV or Markdown table. |
| [`export`](#export) | Write OpenAPI, AsyncAPI, DBML, SQL, JSON Schema, Mermaid ER, or TLA+. |
| [`verify`](#verify) | Verify every rule and temporal property, each on its own cone. |
| [`list`](#list) | Print model elements as a table, JSON, CSV, or Markdown. |
| [`states`](#states) | Derive the reachable state patterns of each entity. |
| [`codegen`](#codegen) | Print the model's domain types and functions as TypeScript, Rust, or Go. |
| [`lsp`](#lsp) | Run the language server over stdin and stdout. |

[compatibility.md](./compatibility.md) says which commands, flags, exit codes, and output
formats are stable.

## Conventions

**Inputs.** `<INPUTS...>` is one or more files or directories; at least one is required. A
directory is searched recursively for `.rdra` files. Every loading command collects inputs
the same way: each file as given, then each directory walked with the entries of every
directory sorted by name, so the load order is the same on every platform. All reachable
files merge into one model. Imports resolve against include paths derived from the input
layout (see [Modules and imports](./language-reference.md#modules-and-imports)). Each
loading command reads the [project manifest](#project-manifest).

**Diagnostics** go to stderr. A load or consistency diagnostic is `error: ...` or
`warning: ...`. A lifecycle finding carries its stable name: `error[<Name>]: ...`,
`warning[<Name>]: ...`, or `info[<Name>]: ...`. [Diagnostics](./state-derivation.md#diagnostics)
lists the names. A source site prints as `file:line:col` with the resolved absolute path;
this page shortens it.

**Exit codes.**

| Code | Meaning |
|---|---|
| `0` | Success. Warnings and info findings do not change it. |
| `1` | The command failed: a load error, an error finding, a refused output, a failed `--check`, or a malformed `rdra.toml`. |
| `2` | Usage error: an unknown flag, a bad flag value, a removed value (its message names the replacement), or two flags that do not go together. |

Every generator refuses to write when the model has parse or resolve errors. `fmt` needs
only a parseable file and runs no analysis. `files` and `fmt` read no manifest.

---

## Lifecycle errors

An output that prints a reachability result is **fail-closed**: it refuses while `check`
reports a lifecycle error. Any other output is written and exits 0. An output of declared
facts or of the data layer does not explore the lifecycle and says nothing about it; run
`check` for the lifecycle errors. `crates/rdra-ish-cli/src/lifecycle_gate.rs` classifies
every `--kind` by what it reads, and `crates/rdra-ish-cli/tests/export_gate.rs` holds every
kind to this table.

| Output | Reads | On a lifecycle error |
|---|---|---|
| `list`, every kind | declared facts | writes |
| `csv`, every kind | declared facts and the data layer | writes |
| `diagram --kind state` | each state model's start states and reachable states | refuses, unless every error is a deadlock, which it colours |
| `diagram`, every other kind | declared facts, `sequence` included | writes |
| `export --kind tla` | the transition system, built at `--instance-count` / `--int-range-max` | writes, with a note: the error count, that the whole-model `.cfg` checks the rules only, that the `StateModel_` specs check the deadlocks and witnesses with TLC, and `verify`; a build error of the system refuses |
| `export`, every other kind | the data layer | writes |
| `states` | the derivation, which is its output | prints the report with its `[error]` rows and exits 1 |

[`codegen`](#codegen) refuses (exit 1) while `check` reports any error, unless
`--allow-failing` is given. `verify` refuses on a build error of its transition system and
reports every other finding itself.

The generators that explore the lifecycle (`codegen`, `next`, `diagram --kind state`,
`export --kind tla`) explore it at `check`'s default caps, within `memory` under `[check]`
(see [Memory limit](#memory-limit)). A cone past that limit is the error
`MemoryCapReached_<E>`: it refuses `codegen` and the state diagram, and the TLA+ export
counts it in its note.

---

## `check`

<!-- derived-from ./state-derivation.md#diagnostics -->

Type-check the model, run the consistency checks, and explore the lifecycle. Writes no file.

```
rdra-ish check <INPUTS...> [--strict] [--deny-deprecated] [--guards] [--code[=fresh|build|run]]
               [--deny-incomplete] [--mutants <N>] [--mutant-seed <SEED>]
               [--max-patterns <N>] [--max-states <N>] [--memory <SIZE>]
               [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>]
               [--buc <ID>]... [--entity <ID>]... [--without-usecase <ID>]...
               [--without-buc <ID>]... [--actor <ID>]...
```

`check` stops at the first load error. Otherwise it runs the consistency checks and
explores the lifecycle, one cone per rule and entity view. It checks safety only: the rules,
the `after` assertions, and the state-model findings. It evaluates no `property`; that is
[`verify`](#verify)'s, and `check` prints `note: <n> temporal properties were not
model-checked by check; use verify` when the model has some.

The consistency checks cover:

- actor permissions for `requires_permission` on use cases and the APIs they invoke,
  missing and excess, inferred from performer paths;
- API invocation and API entity-operation gaps;
- system boundaries and cross-system entity coordination;
- FK and API transaction boundaries;
- event links: an event never raised, a raised event that nothing consumes (`outbox(Event)`
  silences this one), a triggered use case in no BUC;
- the lifecycle findings of [Diagnostics](./state-derivation.md#diagnostics): unreachable
  values, states with no witness, deadlocks, use cases that never run, violated rules and
  `after` assertions, vacuous rules (`Vacuous_<name>`), unbindable conditions, and cones
  stopped at a cap.

| Flag | Default | Meaning |
|---|---|---|
| `--strict` | off | Make permission and access gaps (`ActorPermissionMissing`, a path with no actor, ...) errors. |
| `--deny-deprecated` | off | Make each `DeprecatedSyntax` warning (a [deprecated form](./language-reference.md#deprecated-and-removed-forms)) an error. |
| `--guards` | off | After a clean run, print which rules each guard protects. See [Guard dependencies](#guard-dependencies). |
| `--code[=<LEVEL>]` | off; `fresh` when given bare | After a clean run, check the service code the [`[code]` tables](#the-code-tables) name: `fresh`, `build`, or `run`. Write the level with `=`. See [`check --code`](#check---code). |
| `--mutants <N>` | `1` | With `--code=run` only (else exit 1): after a passing `--code=run`, run the run command again on `N` mutant traces of each kind and fail when one passes. `0` runs none. See [The self-check](#the-self-check). |
| `--mutant-seed <SEED>` | `0` | With `--code=run` only (else exit 1): the seed that picks the mutant traces. The same model and seed pick the same ones. |
| `--deny-incomplete` | off | With `--code=run` only (else exit 1; without `--code`, exit 2): fail when a table's traces are not complete: their BFS stopped at `conformance_states`, they leave refusals of the model untried, or they never run a use case of the port; and when calls reached the unimplemented port or the run did not say (`CodeUnimplemented`). See [What a run covers](#what-a-run-covers). |
| `--max-patterns <N>` | 2^(10 + 8·(n − 1)) for n = `--instance-count` (`1024`, `262144`, ...) | Cap on each entity's distinct patterns in its view's cone. Past it: the `PatternCapReached` error. A cone of a rule gathers no pattern, so the cap does not stop it. |
| `--max-states <N>` | none | Cap on the joint states of one cone. Past it: the `StateCapReached` error, which names what the states vary in. |
| `--memory <SIZE>` | `memory` under `[check]`, else `4GiB` | What the run may hold. A cone past it stops with the `MemoryCapReached` error. See [Memory limit](#memory-limit). |
| `--instance-count <N>` | `1` | Slots per entity, `1..=255`. A violation that needs two rows of an entity shows only from `2`. A larger value raises the default `--max-patterns`. |
| `--instances <ENTITY=N>` | `instances` under `[check]` | One entity's slots, `1..=255`; repeatable. See [Per-entity instance counts](#per-entity-instance-counts). |
| `--int-range-max <N>` | `5` | The Int domain is `0..=N`, `1..=255`. |
| `--buc`, `--entity`, `--without-usecase`, `--without-buc`, `--actor` `<ID>` | whole model | Check part of the model; each repeatable. See [Partial runs](#partial-runs). |

**Output.** stderr opens with a `bounds:` line (the slots per entity, the Int range, the state
cap or `no state cap per cone, memory 4GiB`, and that entities a check does not read are
unbounded). The last stdout line is the verdict:

| Line | When |
|---|---|
| `OK: no errors within <bounds>` | No error. |
| `OK: no errors in the selected scope, within <bounds>` | No error, under `--buc` or `--entity`. |
| `FAIL: <E> errors, <W> warnings within <bounds>` | Some error (`in the selected scope,` under a scope). Info findings are not counted. |
| `FAIL: <E> errors, <W> warnings; the model did not load, so nothing was explored` | A load error. |

`<bounds>` reads like `1 instance per entity, Int 0..5`.

**Exit codes.** `0` with no error; `1` on any error, including an Error-severity lifecycle
finding.

**Findings.** A violated rule or `after` assertion prints once, with a shortest
counterexample:

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

Each step names the use case (`Pay_1, transaction 1 of Pay` for a split one), the instances
it binds, and its site. Under it, `+ E#k` is a creation with the values the check reads,
`- E#k` a deletion, `now: a -> b` a clock tick, and `E#k.col: a -> b` a change with the
`transitions` arm or `sets` that wrote it. `start` lists the instances present before the
first step. The trace shows only what the rule's cone reads. When the cone does not count an
entity's slots and the trace creates more of them than the bound, a `note:` says the whole
model may not reach that state at this bound.

The state-model findings ([State models and witnesses](./state-derivation.md#state-models-and-witnesses),
[Use cases that never run](./state-derivation.md#use-cases-that-never-run),
[Deadlocks](./state-derivation.md#deadlocks)):

```
error[Witness_Order_status_refunded]: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it
  state model: Order.status, entered by Refund (shipped -> refunded)   unwitnessed_state.rdra:31:1

warning[Runs_Pay_2]: transaction 2 of Pay (Pay_2) can never run: in no reachable state do its guards hold
  declared: transaction 2 of Pay (Pay_2)   never_runs.rdra:5:1
  guard: condition Settled                 never_runs.rdra:20:1

error[Deadlock_Order_status_paid]: deadlock: Order#1 stays at status = paid forever, and paid is not an end state
  leaves paid: Ship (paid -> shipped)   deadlock.rdra:29:1
    guard: condition Packed             deadlock.rdra:33:1
  reached by (2 steps):
    1. Place [Order#1]                     deadlock.rdra:12:1
         + Order#1 (status=pending)
    2. Pay [Order#1]                       deadlock.rdra:13:1
         Order#1.status: pending -> paid   deadlock.rdra:28:1
```

- `Witness_*` (error): a state of a state model that no reachable state holds, with each arm
  that would enter it.
- `Runs_*` (warning): a use case, or one transaction of a split one, that no edge carries. It
  names only what blocks it: `needs:` for an instance no reachable state has (or a free slot
  the bound never leaves), `guard:` for each guard that holds nowhere, or a first line that
  says the guards never hold together.
- `Deadlock_*` (error): an instance at a state with a way out that no path takes. It lists
  each way out with what blocks it, and a shortest path into the state. When a way out lacks
  only a free slot of an entity it creates, the bound blocks it, not the model: it is the
  warning `deadlock at the bound`.

A cone stopped at a cap leaves these undecided. Each is listed under `not checked:`
(`state E.col = v is not decided: ...`, `whether Pay_2 can run is not decided: ...`, `whether
some E can get stuck at col = v is not decided: ...`) and is neither an error nor a warning.

### `check --code`
<!-- derived-from ./design/code-check.md -->

`check --code` asks whether the service code has what the model requires. It is
experimental, as `codegen` is. rdra-ish parses no service code: it generates every file the
[`[code]` tables](#the-code-tables) name again and compares, and at `build` and `run` it
reads the compiler's errors. It runs after the model check; on a model that fails `check` it
checks nothing, a note says so, and the exit status is the model check's.

| Level | What it checks |
|---|---|
| `fresh` | Each file `codegen --code` writes is what the model generates now: the domain code (or the [model adapter](#the-model-adapter)) by its text, as [`codegen --check`](#codegen) compares it, and the traces by hash. A missing file is an error. No compiler runs. |
| `build` | `fresh`, then the language's build command on the probe. Each compiler error maps back to a use case. |
| `run` | `build`, then the conformance test. |

Only `build` and `run` start a command: the language's default, or `build` / `run` from the
table. The LSP never does.

| Language | `build` | `run` |
|---|---|---|
| Rust | `cargo check --test <probe> --message-format=json` in `root` | `cargo test --test <probe> -- rdra_conformance --nocapture` |
| TypeScript | `tsc --noEmit --pretty false -p tsconfig.rdra.json` in `root` | `node <probe>` (Node 22.18 or later, by type stripping; the service imports name the `.ts` files; the `package.json` above the probe sets `"type": "module"`) |
| Go | `go test -count=1 -run ^$ -gcflags=-e ./<probe dir>` in `root` | `go test -count=1 -v -run ^TestRdraConformance$ ./<probe dir>` |

`tsconfig.rdra.json` is a generated file that extends the project's tsconfig and holds only
the probe. An older Node fails `run` with a `CodeConformance` that says so. The probe and
the domain code are ES modules, and npm 11's `npm init -y` writes `"type": "commonjs"`: under
`verbatimModuleSyntax` tsc then fails `build` with one `CodeCommonJs` in place of an error
per use case, and without it Node fails `run` with a `CodeConformance` that names the
setting. A project that runs TypeScript another way sets `run` (`["npx", "tsx", "src/rdra_contract.ts"]`). Go names
one missing method per error, so the Go probe names each method of a contract on a line of
its own.

The compiler's whole output comes first on stderr (for Rust, the text rustc prints for each
error of cargo's JSON). A compiler error then maps to a use case by the probe line it is on
(one line per use case), by the generated method, contract, dispatcher, or transition
function it names (Go `RunPay`, `PayFromOpen`), or, for an error about a call's number of
arguments that names no function (tsc TS2554, rustc E0061), by the generated function the
call on its line names. The `code:` line quotes the whole message, its further lines
indented under it (which member a TS2322 says differs, Go's `have` and `want`), with the
path relative to the working directory, as the other paths of the check are. One block per
use case and kind:

```
error[CodeMissingUseCase_Ship]: use case Ship has no implementation in svc::service::Service
  model: model/order.rdra:22:1  usecase Ship "Ship"
  code:  src/service.rs:70:26  E0277: the trait bound `Service: ShipUseCase` is not satisfied
  fix:   impl ShipUseCase for svc::service::Service (prepare_ship, commit_ship)
```

A stale file prints under the cause lines of `codegen --check`, and each stale file counts
as one error in the summary line. A `generator changed` names the settings of the table in
`rdra.toml` first (`the settings of [code.rust] in rdra.toml (paths, service, ...) changed`):
the fingerprint records the model and the file, not the settings, so a changed `service` or
path looks the same as another rdra-ish version.

```
error: ../svc/src/domain.rs is not what `codegen` generates from the model
  model changed: its source text is not the text ../svc/src/domain.rs was generated from
  1 line(s) only in ../svc/src/domain.rs, 1 only in the generated code
  ...
  generate them again: rdra-ish codegen model --code
```

**The obligation ledger.** At every level, each obligation the generated code lists
(`Unique_Payment_Order`, `Ref_Order_customer_id`, ...) needs an owner: a marker line
`rdra: obligation <name> [file]` after any comment token (`//`, `#`, `--`) in the service
code, or `<name> = "<file>"` under `[code.obligations]`. A name is letters, digits, and `_`,
as the generator builds them, and a marker after an odd number of `"` or `` ` `` on its line
is inside a string literal: neither owns anything, so code that prints markers
(`` `-- rdra: obligation ${name}` ``) is not read as one. A marker's file is relative to the
root it was found under; without one, the owner is the marker's own file. The check says an
obligation **has an owner**, never that it holds, and confirms the owning file exists.
Markers are searched under each table's `root`, skipping hidden directories, `target`,
`node_modules`, `vendor`, `dist`, the generated files, and files over 1 MiB. Until there is a
marker or a `[code.obligations]` entry, `check --code` prints a note and checks no
obligation.

```
error[CodeObligationUnowned_Unique_Order_note]: no code owns the obligation Unique_Order_note
  model: model/order.rdra:6:1  no two Order rows share note.
  fix:   add `// rdra: obligation Unique_Order_note <file that keeps it>` or list it under [code.obligations]
```

| Name | Severity | When |
|---|---|---|
| `CodeMissingUseCase_<UseCase>` | error | The service does not implement the use case's contract (Rust E0277; TS2739 / TS2740 / TS2741 naming every member; Go: every method missing). |
| `CodeIncomplete_<UseCase>` | error | It implements the contract without some methods (E0046; TS2739 / TS2740 / TS2741; Go `missing method`, `has no field or method`). |
| `CodeSignature_<UseCase>` | error | A method takes or returns other types than the contract's (E0053, E0050; TS2322; Go `wrong type for method`). |
| `CodeError_<UseCase>` | error | Another error on the use case's probe line or naming its methods. |
| `CodeServiceNotFound` | error | Errors on the probe that cannot resolve the service `service` names (Go `undefined:`, rustc E0433, tsc TS2305 / TS2307): one finding for them all, whose `fix:` points at `service` in the table and `codegen --probe`. A renamed service type or path gives this, not one `CodeError` per use case. |
| `CodeUnmapped` | error | The errors that belong to no use case: the count, and the first line of each (at most 20; the rest are in the compiler's output above). |
| `CodeBuildFailed` | error | The build failed with no error rdra-ish reads. |
| `CodeCommonJs` | error | TypeScript: tsc compiles the probe and the domain code as CommonJS under `verbatimModuleSyntax` (TS1286, TS1287, TS1295); the one finding of that build. Fix: `"type": "module"` in `package.json`. |
| `CodeConformance` | error | `run`: the conformance test failed. The `code:` line is the harness's first difference with its `model only:` and `store only:` rows; when the harness reported nothing, where the service panicked (Rust, Go) or what it threw (Node); else the output's last line. |
| `CodeMutantSurvived` | error | `run`: a mutant trace of the self-check passed. The `code:` line says whether the run passed or failed at another row. |
| `CodeUnimplemented` | warning; error with `--deny-incomplete` | `run`: the run passed, but calls reached the [unimplemented port](#the-unimplemented-port) (the message names their use cases and calls), or the run printed no line saying whether any did. |
| `CodeCoverageIncomplete` | warning; error with `--deny-incomplete` | `run`: the BFS behind the traces stopped at `conformance_states`, so the moves past it are not run and no call is tried as refused; or the traces leave refusals of the model untried, or never run a use case of the port. |
| `CodeObligationUnowned_<name>` | error | The obligation has no owner. |
| `CodeObligationOwnerMissing_<name>` | error | Its owner names a file that does not exist. |
| `CodeObligationUnknown_<name>` | warning | An owner names an obligation the model does not have. |

#### What a pass does not show

- `build` compiles the probe and what it imports: Rust `cargo check --test <probe>` (the
  library and that test), TypeScript the files `tsconfig.rdra.json` reaches from the probe,
  Go the probe's package. An `OK` at `build` says nothing of a binary, a handler, or a file
  the probe does not reach.
- Each trace runs on a fresh store: the harness calls `new` once per trace (181,095
  times on the clinic-ops sample at the default `conformance_states`) and `seed` empties the store before it stores the
  trace's rows. Make `new` cheap: share one database and let `seed` clear it, rather than
  open or migrate one per call. Migrations, and data an earlier version of the service
  stored, are outside what the check runs.
- The `OneTransaction` obligation's owner is a declaration, as every owner is. What the run
  shows of it is the interleaved commits: a commit prepared before another call changed its
  row must be refused. That is the optimistic write (`UPDATE .. WHERE id = ? AND <tag> = ?`,
  a write that matches no row rolls back as `NotEnabled`). The harness is one thread, so
  held reads (`SELECT .. FOR UPDATE`, serializable isolation) and races between real
  connections are not shown.
- A raised event is never consumed. Once a raiser ran on a row, the raise stays recorded on
  it (rule 3 of the [event order](./design/event-order.md#the-rules)): a delete does not
  clear it, and the use case it triggers may run on that row as often as its states and
  guards allow. `snapshot` lists each raise a triggered use case reads, and the harness
  compares them, so the store keeps them as long as the row's history matters, with no
  queue to drain. A `.counted(k)` trigger is the exception, and `codegen` prints no
  function for its use cases.

#### What a run covers

After `--code=run`, stdout states what the traces of each table cover, before the summary
line. The lines come in this order and hold counts only, so the text is the same on every
run of the same model. On `tests/fixtures/codegen/conformance.rdra`:

```
coverage of [code.rust]: bounded conformance to the model, not equivalence
  bound: 1 row created per entity, Int 0..5
  BFS: complete, 6 states; 6 reached by the traces' calls
  moves: 5 of 5 run
  refused calls: 11 tried (NotEnabled 10, Rejected 1); interleaved commits: 7
  seeded at a reached state: 2 traces, each then one call (H1: seed and path agree)
  never refused: 1 (Place)
  assumes: H1 the service answers from the tracked columns and the call alone; H2 every reachable call is tried; H3 seed and snapshot reach the real store; H4 every write goes through the port, one call one transaction; H5 the rules read tracked columns only
  cannot show: writes outside the port, locking, races, external effects, liveness, who issues a capability, data outside the model
```

When there is something to list, the `moves` line ends `; <n> left out (a tick, or a use case
with no function: <what> <n>, ...)`, counted by use case (`a tick` for the clock), and a
`not compared:` line after the refused calls names what the harness
does not compare (a counted trigger, a proposition, the clock).

- **Refused calls.** In each state the traces try every call the model refuses: each binding
  of the use case's rows to the rows there (a created row on the lowest free slot, never
  when none is free), for every use case of the port. A row names the kind of refusal the
  functions give, and the service must refuse with it: `NotEnabled` when no function runs
  from the rows' states, `InvalidArgument` when an argument check fails, `Rejected` when a
  condition does not hold. A call the functions would run, which the model refuses for a run
  mark, a raise, or a rule the service keeps outside them, expects any kind (`any kind` in
  the counts).
- **`untried refusals:`** counts the refusals no trace tries: those of a use case with more
  than 64 bindings in one state.
- **`seeded at a reached state:`** counts the traces that seed a state the calls reach (one
  with no run mark or raise, at most 16) and then make one call from it, which the other
  traces make from the same state reached by calls. Each trace after the first seeds other
  values in the columns the model does not track. Both check H1.
- **`never run:`** names the port's use cases no row runs, and **`never refused:`** those no
  row tries as refused.

Ids are banded by entity: the `k`-th entity of the port numbers its rows from `100k + 1`
(`Customer 1`, `Order 101`, `Shipment 201`), so a service that looks a row up by another
entity's id finds none.
A passing run means bounded conformance: under H1 to H5, the safety rules the model verified
over tracked columns hold for runs driven through the port, with at most that many rows
created per entity. It does not mean the service is equivalent to the model. When the BFS
stopped at its cap, the `BFS` line says `stopped at its cap of <N> states`, the refused calls
are `none tried`, and stderr gets `warning[CodeCoverageIncomplete]`: a service that runs a
call the model refuses passes such a suite. The same warning names untried refusals and
use cases never run. The exit status does not change unless `--deny-incomplete` is given.
Raise `conformance_states` and run `codegen --code` again to complete a capped suite. The statement describes the traces, so it is the same when `run` in the table
replaces the default command.

#### The unimplemented port

The domain code has a stand-in for the contracts, which a service takes while it is
unfinished, so it builds before every method exists: Go's `UnimplementedConformancePort` to
embed, TypeScript's `UnimplementedConformancePort` class to extend, and Rust's
`Unimplemented<Contract>` marker trait per contract (`impl UnimplementedPayUseCase for
Service {}`), which gives the service the contract. It stands in for each `prepare` and
`commit` method, and with `--conformance` Go's and TypeScript's also for `Seed` / `seed` and
`Snapshot` / `snapshot`. It stands in for no entry point (`Run<Action>`): the harness drives
none, so the service writes each, and Rust gives a contract with one no marker. Each method
fails as the language says "not implemented": Go returns `ErrUnimplemented`, TypeScript
rejects with `Unimplemented`, Rust panics with `unimplemented!`. A method the service
declares takes the place of the stand-in's.

`check --code=build` passes such a service: the build checks signatures, and cannot tell a
stand-in from a method the service wrote. The run tells them apart. The harness counts every
call that reached the stand-in. One the model runs (or either call of an interleaving) stops
its trace, and the next trace starts; a refused one is no refusal, and its trace goes on. A
`Seed` or `Snapshot` that reached it fails the run, as any store error does. The probe's
test prints `rdra-ish conformance: unimplemented: none`, or `... unimplemented: <n> traces
stopped; <action> <calls>, ...`. When calls reached the stand-in, the statement gains a line
and stderr a `CodeUnimplemented` warning, which `--deny-incomplete` makes an error:

```
  unimplemented: 1 use case (Cancel): 7 calls reached the unimplemented port; 4 traces stopped there
warning[CodeUnimplemented]: [code.go]: the service has not implemented Cancel (7 calls): a trace stops at such a call the model runs, so nothing after it is checked, and such a refused call is no refusal
```

A `run` command in the table that hides the test's output on a pass hides the line too: the
statement then says `unimplemented: not reported`, with the same warning. A mutant of the
self-check whose changed row the stand-in left unchecked (its trace stopped earlier, or the
row's own call reached it) is counted on the `self-check` line as `left unchecked by a call
that reached the unimplemented port`, not as a survivor.

A table with a [model adapter](#the-model-adapter) adds one line after the statement, read
from the line the adapter's conformance test prints on a pass:

```
  ran: 11 traces, 21 calls, 11 refused calls, 7 interleaved commits; 32 calls made again by prepare and commit
```

An adapter with `prepare` and `commit` runs the traces a second time with every call and
refused call made by `prepare` and then `commit`, so a `commit` that never stores fails. An
adapter without them runs each interleaved commit as a plain call, and
the line ends `; 7 interleavings run as plain calls: the adapter has no prepare and commit, so
conditional writes are not checked`. A `run` command that hides the test's output gets
`ran: not reported`.

#### The self-check

After a run passes, `--code=run` checks the check. It runs the run command again on mutant
traces, by default one of each kind. Each mutant is one trace of the real traces, cut after
one row and with that row changed into something the model never does:

| Kind | The row becomes |
|---|---|
| `refused-call-runs` | A call the model refuses (`X`), expected to run. |
| `call-refused` | A call the model runs (`C`), expected to be refused. |
| `wrong-state` | A call expected to reach another state, one whose rows differ. |
| `absent-row` | A call on one existing row that names an id no row has, expected to run as before. |
| `guard-removed` | A call one condition rejects (`X Rejected`), expected to run, as the model without that guard runs it. One per condition the traces reject a call with. |

The probe's conformance test reads its traces from the file `RDRA_ISH_TRACES` names, when the
variable is set and not empty, instead of the file it names. The self-check sets the variable
for each mutant, and the real run clears it. A mutant counts as failed only when the run fails
and its output names the changed row (`traces line <n>:`). The statement ends with one line:

```
  self-check: 5 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big
```

`guard-removed mutants:` names the conditions. When the traces give no mutant of a kind, the line says so (`; no refused-call-runs: the
traces try no refused call` on traces whose BFS stopped at its cap). A mutant that passes is
a `CodeMutantSurvived` error, and the exit code is 1. No mutant is equivalent to the real
traces, so a conforming service never passes one:

- Every kind but `absent-row` passes only when the run command does not run the harness on
  the traces it is given, or swallows its failure. For example, a `run` that tests something
  else, or a probe test that ignores the result. These kinds check the pipeline, not the
  service.
- `absent-row` passes when the service runs a call on a row it never looked up: it ignores
  the id. At one row per entity every id is `1`, so the real traces cannot show this.
- A service whose snapshot ignores the store fails the real traces first.

The service code is never changed. A table without `traces` or without a run command gets no
self-check.

**Output and exit codes.** The last stdout line, after the model check's `OK:` line, is
`OK: code: what the model requires, at --code=<level>`, or `FAIL: code: <n> error(s) at
--code=<level>` with exit 1. `--code` on a model with no `[code.rust]`, `[code.go]`, or
`[code.ts]` table exits 1, so a check of nothing never passes. Without `--code`, a model that
has one gets `note: rdra.toml has [code]; `check --code` also checks the service code against
the model` on stderr.

### Partial runs

`check`, `states`, and `verify` take the same scope flags, each repeatable. Each id is
checked against the model; an unknown one is an error.

| Flag | Keeps | Decides witnesses, deadlocks, use cases that never run |
|---|---|---|
| `--buc <ID>` | The use cases reachable from the BUC. | No: each is **not decided**, since the use cases left out could reach, enable, or unblock them. |
| `--entity <ID>` | The entity's view, each invariant that binds the entity, and each `after` assertion whose anchor or predicate binds it, each on its own cone as in a whole run. | Yes for the selected targets; a use case no selected cone keeps is not decided. |
| `--without-usecase <ID>` | Every use case but this one, which is assumed never to run. | Yes, under that assumption. |
| `--without-buc <ID>` | Every use case but those the BUC reaches and no other BUC reaches. | Yes, under that assumption. |
| `--actor <ID>` | The use cases the actor performs, and those no actor performs (started by an event or a timer, or with no `performs`). | Yes, under that assumption. |

A deadlock that another actor's use case hides is an error under `--actor`: `check --actor
Staff` finds a state a staff member cannot leave by their own use cases, even though an
operator's deletion leaves it.

A build error whose entities no selected cone reads is **deferred**: listed under `not
checked:`, it does not fail the run. A build error that alters a use case (a dropped guard or
effect) also involves every entity that use case writes. A build error that names no entity
always fails the run. Without `--entity`, every build error fails the run.

```
$ rdra-ish check samples/formal-verification/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
scope: --entity Order
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
OK: no errors in the selected scope, within 1 instance per entity, Int 0..5
```

The `scope:` line follows `bounds:` and names each flag. For `--buc` it counts the use cases
left out; for `--without-usecase`, `--without-buc`, and `--actor` it lists them. The `not
checked:` block lists the targets the scope leaves out, the deferred build errors, and each
finding a scope or a cap left undecided. Two or more use cases not decided share one line that
counts them and names the first three (`whether 39 use cases (A, B, C, ...) can run is not
decided: ...`). A whole run prints the block only when a cap left something undecided. A
scoped `verify` also lists its temporal properties there: it checks none.

### Guard dependencies

`check --guards` reports which rules each guard protects. A guard is a condition the model
applies to a use case: a `holds` condition that `applies_to` it, or the `.when` conditions of
one column's `transitions` arms, `chooses` values, or `sets` values. For each guard, the run
removes it from every action of the use case (each transaction and each `<UC>_by_<Ev>` copy),
explores again each rule cone that keeps the use case, and lists the rules that now fail.
Triggers, `transitions` sources, keys, and links are not removed. A condition with no `holds`
body, or one applied to something other than a use case, guards nothing and is not listed.
Removing a column's `.when` conditions opens every arm or value at once, which differs from
deleting one `.when(...)` from the source.

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

- The report is on stdout, before `OK:`, in the build's order of use cases and guards, so two
  runs print the same. It is text only.
- A `guard:` line names the guard and its site (the `holds` body, or the first guarded arm or
  value), then each rule it protects, by its `.label` or check name. The `rules:` block lists
  each rule with the guards it depends on, or `no guard`.
- A guard no rule depends on is not wrong. It states a business rule nobody wrote as a
  `forbidden` or `invariant`, or the model does not need it. A guard that backs up another
  guard or a trigger lands here too.
- A rule is `not decided without` a guard when its cone, without the guard, stops at
  `--max-states` or `--memory` before a counterexample.
- The runs use `check`'s caps and memory limit. Under a scope, only the selected rules run. A
  model that fails `check` gets only a `note:`. A guard that serves only a `property` is one
  no rule depends on. `--guards` adds no finding and does not change the exit status.

---

## `lint`

Report coverage and review-readiness findings beside the `check` diagnostics.

```
rdra-ish lint <INPUTS...> [--format table|json|csv|markdown]
```

| Flag | Default | Meaning |
|---|---|---|
| `--format` | `table` | `table`, `json`, `csv`, or `markdown` (a Markdown table with the CSV columns). |

Each row has a severity, a rule, the subject's kind and id, a message, and a hint. Load and
model errors are `semantic-error` rows. **Exit codes:** `1` when a `semantic-error` row is
present, else `0`; warnings do not fail.

| Rule | Severity | Marks |
|---|---|---|
| `api-contract-incomplete` | warning | An API that declares only one of `method` and `path`. |
| `api-contract-missing` | warning | An API with neither `method` nor `path`; the OpenAPI export omits it. |
| `api-unused` | warning | An API no use case `invokes`. |
| `buc-empty` | warning | A BUC that contains no use case; `info` while the BUC is before the skeleton stage. |
| `comparison-trivial-in-range` | warning | A comparison of an Int, Money, or Decimal column with a literal that every value in `0..5` meets (`qty >= 0`), on a column that is not `@null`. It constrains nothing. |
| `comparison-unsatisfiable-in-range` | warning | Such a comparison that no value in `0..5` meets (`qty > 5`), in a rule or a `holds` body. What it guards never applies. |
| `condition-applied-without-body` | info | A condition with no `holds` body and no `combines` that `applies_to` a use case or an API. It guards nothing. |
| `condition-no-rdra-anchor` | warning | A [condition](./language-reference.md#conditions) whose body names neither a variation-typed nor a lifecycle column and that guards no use case (by `applies_to` or `.when`). |
| `consistency-warning` | warning | A whole-model consistency diagnostic of `check`, such as a missing relation. |
| `constraint-unscoped` | warning | A `constraint` that `constrains` nothing. |
| `coverage-score` | info | A readiness signal from 0 to 100. An empty model scores `0`. The score is 100, minus the findings' penalty times 25 divided by the number of model elements, minus 8 for each empty layer (`scope`, `business-flow`, `data`, `interaction`, `system-boundary`, `rules`), and never below 0. |
| `deprecated-syntax` | warning | A [deprecated form](./language-reference.md#deprecated-and-removed-forms). The message names the rewrite and the version that removes it; `fmt --write` rewrites it. |
| `dto-unused` | warning | A DTO no API names in `request`, `response`, or `error_response`. |
| `end-redundant` | warning | An `@end` value that no `transitions` arm leaves: an end state already. |
| `enum-attribute` | warning | An inline `Enum` column that no `transitions` and no `sets` moves: an actor chooses it, so type it by a [`variation`](./language-reference.md#variations). |
| `field-column-unwritten` | warning | A field mapped to a `@null` column that no `sets` and no `chooses` fills, so it shows a value the model never produces. |
| `field-unmapped` | warning | A screen field that no `maps_field` maps. |
| `field-unplaced` | warning | A screen field that no screen `contains`. |
| `flow-cycle` | warning | `precedes` / `branches` edges that form a cycle. |
| `flow-empty` | warning | A flow that contains no step. |
| `goal-unmeasured` | info | A `goal` with no `metric` or no `target`. |
| `multi-rule-unscoped` | warning | A multi-entity `forbidden`, `invariant`, or `property` with no `.along(...)` and no `.global()`, when keys relate its entities. The message names the `.along(...)` of the shortest key path. |
| `naming-column` | warning | A column that is not snake_case. |
| `naming-dto-field` | warning | A DTO field that is not snake_case. |
| `naming-id` | warning | An element id that is not UpperCamelCase. |
| `nfr-unscoped` | warning | An `nfr` that `applies_to` nothing. |
| `orphan-node` | warning | An element no predicate connects; the `rdra-*` rules replace it for their kinds. |
| `query-index-missing` | warning | A lookup no index serves; skipped below 10,000 `rows`. See [Query shapes](./language-reference.md#query-shapes). |
| `rdra-*` | warning | The RDRA sheet's 不整合 columns that `lint` enforces: an actor, external system, entity, condition, variation, or timer no BUC reaches, and a `transitions` use case in no BUC. See [RDRA lint rules](./language-reference.md#rdra-lint-rules). |
| `requirement-no-goal` | info | A `requirement` that achieves no `goal`, unless its status is `out-of-scope`. |
| `requirement-untraced` | warning | A requirement that `motivates` no BUC. |
| `rule-unlabeled` | info | A rule with no [`.label("...")`](./language-reference.md#labelling-a-rule), so `list --kind rule` shows it unlabelled. Not shown in the editor. |
| `screen-no-fields` | warning | A screen that contains no field. |
| `screen-no-transition` | info | A screen that no `navigates` enters or leaves and that no flow reaches in order. |
| `semantic-error` | error | A load or model error of `check`. |
| `semantic-warning` | warning | Any other warning of `check`. |
| `stage-readiness` | info | Each BUC's stage and the stage after it, as [`next`](#next) decides them: `BucOrder stage 2 (next 3); BucPayment stage 6 (required stages complete)`. |
| `step-no-cover` | warning | A step that `covers` nothing. |
| `triggered-usecase-unguarded` | info | A use case only an event or timer starts, that writes, and that no condition guards and no `transitions` gives a from-state. Once the event was raised on a row, the use case may run there in every state its guards allow, as often as they allow. |

---

## `next`

Show each BUC's incremental modeling stage and what the next stage asks for.

```
rdra-ish next <INPUTS...> [--buc <ID>]... [--format text|json]
```

| Flag | Default | Meaning |
|---|---|---|
| `--buc <ID>` | every BUC | Show only these BUCs; repeatable. An unknown id is an error. |
| `--format` | `text` | `text` or `json`. |

`next` places each BUC on the [stage map](./incremental-modeling.md#stage-map). Each stage
has a gate of clauses. A blocking clause that fails is a gap, and the stage is **open**; an
advisory clause that fails is a review note and never holds a BUC back. A stage whose clauses
pass but which owns a [`check`](#check) error (a `Witness_`, `Deadlock_`, or violated rule on
an entity the BUC touches) is **blocked**. Otherwise it is **met**. A BUC is at the last stage
of the met prefix of the required stages, 0 to 6. The rule stages 7A to 7C are optional:
`next` offers them and does not demand them.

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

A gate is evidence, not a verdict; the stage sections of
[Incremental Modeling Flow](./incremental-modeling.md) say when a stage is stable enough to
move on.

**Text output.** A summary table when there are several BUCs, then one block per BUC: each
stage with what its gate counted (`[x]` met, `[ ]` open, `[!]` blocked, and the counts of notes
and findings), then the first required stage not met, with its question, `Gaps:` (at most 8;
JSON lists all), `Notes:`, `Findings:` (by name; `check` prints the traces), `Ask:`, the
commands that review the stage, and its guide section. Findings no BUC owns print first.

```text
BucStoreRestock "Maintain Store Restock": stage 1 BUC skeleton
  [x] 0  Scope sketch             belongs to StoreOperations
  [x] 1  BUC skeleton             2 use cases, 1 `performs` / `triggers` starting them
  [ ] 2  Screens                  0/2 use cases display a screen or run on a trigger
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
complete`, then `Every required stage is met.`, the met rule stages, and an `Optional: stage
7C Advanced rules` block per open one. A blocked stage reads `Next: stage 6 Lifecycle is
blocked: the elements exist, but check rejects them`.

**JSON output.** `{"bucs": [...], "unowned_findings": [...]}`. Each BUC has `id`, `label`,
`stage` (`"2"`, `"7A"`, or `null` before stage 0), `stage_name`, `slug` (a stable id such as
`data-touchpoints`), `required_complete`, `gates`, and `next`. A gate has `stage`,
`stage_name`, `slug`, `passed`, `verdict` (`met`, `open`, `blocked`), `evidence`, the counts
`gaps` and `notes`, and `findings`. `next.kind` is `required` while a required stage is not
met, with that stage's `stage`, `stage_name`, `slug`, `verdict`, `question`, `doc`, `asks`,
`validate`, `gaps` and `notes` (each with `kind`, one of `buc`, `usecase`, `screen`, `api`, or
`entity`; `id`; and `message`), and `findings` (`name`, `severity`). Once every required
stage is met it is `optional`, with the same fields per rule stage under `stages`. Skills
key on `slug` and read `next.asks` and `next.gaps`.

**Exit codes.** `0` at every stage and verdict; `1` when the model has load or model errors
(run `check` first). `next` explores the lifecycle as `check` does, so it takes as long.

---

## `files`

Print the `.rdra` files the inputs load, one path per line, in load order.

```
rdra-ish files <INPUTS...>
```

The order is the one every loading command uses (see [Conventions](#conventions)).
`scripts/verify-samples.sh` reads a sample's `// expect` header from the first path (see
[Sample expectations](./formal-verification.md#sample-expectations)). **Exit codes:** `1`
when the inputs hold no `.rdra` file, else `0`.

---

## Project manifest

A model may have an `rdra.toml`. Every loading command but `files` and `fmt` reads it.

```toml
[model]
name = "shop"                       # required

[dependencies]                      # parsed, not used yet; free-form
auth = { path = "../auth-model" }

[check]                             # check, states; memory also for the generators
memory = "4GiB"
instances = { Loan = 4 }

[verify]                            # verify and export --kind tla
memory = "16GiB"
instances = { Loan = 4 }
symmetry = "always"
```

| Table | Key | Meaning |
|---|---|---|
| `[model]` | `name` | The model's name. Required. |
| `[dependencies]` | any | Parsed and not used yet. |
| `[check]`, `[verify]` | `memory` | The command's [memory limit](#memory-limit), a size such as `4GiB`. |
| `[check]`, `[verify]` | `instances` | Per-entity slot counts, `1..=255`; see [Per-entity instance counts](#per-entity-instance-counts). |
| `[verify]` | `symmetry` | `never` or `always`; see [Instance symmetry](#instance-symmetry). Under `[check]` it is accepted but not read, and `verify` warns about it. |
| `[code]` | `obligations`, `rust`, `go`, `ts` | See [The code tables](#the-code-tables). |

A flag overrides its key. Every loading command exits 1, naming `rdra.toml` and the line, on a
malformed file, a missing `[model] name`, an unknown table or key (a misspelled `[verfiy]`;
only `[dependencies]` is free-form), a `memory` that is not a [size](#memory-limit), or an
`instances` count outside `1..=255`:

```
Error: /work/shop/rdra.toml:3: unknown field `verfiy`, expected one of `model`, `dependencies`, `check`, `verify`, `code`
```

**Discovery.**

1. Take each input as given, made absolute without resolving symlinks, with `..` collapsed.
2. Look for `rdra.toml` in the input's directory (a file input's directory), then in each
   parent.
3. Stop after the first directory that holds `.git` (searched too), or at the filesystem
   root. A stray `rdra.toml` above a repository never applies.
4. With several inputs, every input's manifest is read, so a malformed one fails the command
   in any order. When all are valid, the first input's manifest is the project's.

### The code tables

`[code.rust]`, `[code.go]`, and `[code.ts]` (also spelled `[code.typescript]`, as
`--lang typescript` is; one of the two) name the files `codegen --code` writes for a
service in that language, which [`check --code`](#check---code) holds against the model. Each
path is relative to the directory of `rdra.toml`. A table names `domain` or `adapter`, not
both.

```toml
[code.rust]
domain = "../service/src/domain.rs"
traces = "../service/tests/traces.txt"
conformance_states = 65536

[code.ts]
domain = "../web/src/domain.ts"
traces = "../web/src/traces.txt"
probe = "../web/src/rdra_contract.ts"
service = "../web/src/order_service.ts#OrderService"
seal = true

[code.go]
domain = "../shop/domain/domain.go"
traces = "../shop/svc/traces.txt"
probe = "../shop/svc/rdra_contract_test.go"    # a _test.go in the service's package
service = "example.com/shop/svc.Service"       # <import path>.<Type>
new = "svc.New()"

[code.obligations]
Unique_Payment_Order = "../service/migrations/003_payment.sql"
```

| Key | Meaning |
|---|---|
| `domain` | The file `codegen` writes for the language. |
| `adapter` | In place of `domain`: the [model adapter](#the-model-adapter) (`codegen --adapter`). Needs `traces`. `fresh` then compares only this file, the traces, and the probe, so the project may rename and edit the domain code it took. |
| `traces` | The conformance traces. With it, `domain` also holds the conformance port and harness (`--conformance`). |
| `conformance_states` | `--conformance-states` of the traces. Default `65536`. |
| `probe` | The file `codegen --code` (or `--probe`) writes: one line per use case stating that `service` implements its contract (`<UseCase>UseCase`). Rust: a file under the service crate's `tests/`. TypeScript: a `.ts` file in the project; `codegen --code` also writes `tsconfig.rdra.json` in `root`. Go: a `_test.go` in the service's package directory (`package <svc>_test`). `build` needs it. |
| `service` | The type that implements the contracts. Rust: its path, `svc::service::Service`. TypeScript: its file relative to `rdra.toml`, `#`, and the exported type, `../web/src/order_service.ts#OrderService`. Go: `<import path>.<Type>`. |
| `module` | Rust: the path of the generated module in the service's crate, `svc::domain`; the Rust probe needs it. Go: the import path of `domain`'s package; default: the service's import path moved as `domain`'s directory is from the probe's. |
| `new` | An expression that makes a fresh service. With `traces`, the probe also holds the conformance test (`rdra_conformance` in Rust, `TestRdraConformance` in Go) that `run` runs. Rust: required for the test. TypeScript: default `new service.<Type>()`. Go: default `new(<pkg>.<Type>)`. |
| `package` | Go: the package name of `domain` (`--package`), default `domain`. When set, the probe imports the domain package under this name, so an import path whose last element differs builds. |
| `root` | The directory the build runs in and the ledger scans. Default: the nearest directory above `domain` with `Cargo.toml`, `go.mod`, `package.json`, or `tsconfig.json`. |
| `build`, `run` | The command `--code=build` / `--code=run` runs in place of the default, as a list of arguments. Runs only when that level is given on the command line. |
| `seal` | TypeScript: `true` writes `domain` as `codegen --seal` does. Default `false`; set it in a table with `domain` (an `adapter` has no state types, and ignores it). Without it, a service can build a state as an object literal and skip the transition function and its rule checks, and `run`, which compares only what is stored, passes it wherever the traces store the same rows. With it, the literal fails `build` (TS2741). A service that makes states only through `fromRow` and the transition functions builds the same either way. |
| `tsconfig` | TypeScript: the tsconfig `tsconfig.rdra.json` extends. Default: `tsconfig.json` in `root` when it exists; else `tsconfig.rdra.json` sets `strict` itself. |
| `serde` | Rust: `true` adds `::serde::Serialize` and `::serde::Deserialize` to the derives of the data types in `domain`, so the crate needs `serde` with its `derive` feature. Ids go over the wire as their value, and scalars and value sets in the model's spelling (`"paid"`, `"12.50"`); a scalar or value set comes back through its parser, so text the parser refuses fails to deserialize. Rows, records, DTOs, events and run marks are plain structs with each column under its model name. An entity with states goes over the wire as its `<Entity>Row` and comes back through `from_row`, so a row that fits no state fails; a state struct gets no derive, and stays made only by a transition function and `from_row`. The errors, results and dispatcher outcomes, and the conformance types, get none. Default `false`, which leaves the output as it was before the key. |

An unknown key fails every loading command, as under `[check]`. `seal` or `tsconfig` under
`[code.rust]` or `[code.go]` makes `--code` exit 1, and so does `serde` under `[code.go]` or
`[code.ts]`, `serde = true` in a table with `adapter`, a table with neither `domain` nor `adapter`, one with
both, or one with `adapter` and no `traces`.

#### An async Rust service

The Rust contracts and `ConformancePort` are synchronous (`&mut self`). A service on an async
database crate (sqlx, tokio-postgres) keeps them that way and does its I/O around the domain
functions, which are pure: an async handler opens a transaction, loads the rows, converts each
with `from_row`, calls the dispatcher (`run_<use case>`), writes the result's rows where they
still are what `<row>_expected` names, and commits. Write that prepare and commit once, as
async methods of the service. For the conformance test, implement the contracts on a small
adapter that owns a runtime and the async service and calls `block_on` on those same methods,
so the check runs the code production runs. The test calls `new` once per trace, so share the
runtime and the connection pool across calls (a `static` `OnceLock`) instead of making them in
`new`. Do not put the async service behind a mutex to call it through the synchronous
contracts from a handler: that serialises every request. A service on a synchronous driver
(rusqlite) can implement the contracts directly and run them on `spawn_blocking`.

#### The model adapter
<!-- derived-from ./design/code-check.md#the-black-box-model-adapter -->

A project that changed the domain code it took (renamed it, split it, wrote its own) can no
longer pass `fresh` on it, and its service no longer implements the generated port. It binds
the service to the traces through a model adapter instead: `adapter` in the table, written
by `codegen --adapter`. The file holds plain data and no type of the domain code:

- `ModelState`: per entity, the rows with their id and the columns the model tracks (a
  column whose value the model ignores is a bool, set or not), the pending run marks, and
  the raises a triggered use case reads.
- `Call`: one case per use case action, with the ids, the event, the inputs and the choices
  the traces give.
- `ModelAdapter`: `reset`, `seed(ModelState)`, `observe() -> ModelState`, and
  `invoke(Call)`, which says the call ran or the kind of refusal (`NotEnabled`, `Rejected`,
  `InvalidArgument`). Optional, and only as a pair, `prepare(Call)` and `commit()` split a
  call for the traces' interleaved commits (Rust: `split()` returns the adapter as a
  `SplitAdapter`; Go: the adapter also implements `SplitAdapter`; TypeScript: optional
  members, and each method may return a promise). `invoke` must do what `prepare` then
  `commit` do: the traces' plain calls go through `invoke` only, so nothing else shows that a
  commit ever stores its write.
- `conform` / `Conform`: the harness. It reads the same traces as the port's harness and
  reports a difference in the same words.

The service implements `ModelAdapter` over its own code; `service` names that type, `module`
the adapter file's module (Rust) or import path (Go, which puts the file in a package of its
own named by `package`), and `new` makes one. The probe then holds only the conformance test,
and the self-check runs through it. The adapter picks the values of the columns the model
does not track when it seeds, and it must reach the real store (H3).

### Per-entity instance counts
<!-- constrained-by #project-manifest -->

`--instance-count` gives every entity the same number of slots. `--instances <Entity>=<n>`
gives one entity its own, `1..=255`, and is repeatable:

```sh
rdra-ish verify model/ --instances Loan=4      # two slots of everything, four of Loan
rdra-ish verify model/ --instance-count 1 --instances Loan=3 --instances Room=3
```

- `check`, `states`, `verify`, and `export --kind tla` take the flag. `instances` under
  `[check]` sets it for `check` and `states`; under `[verify]`, for `verify` and `export --kind
  tla`. A flag replaces the table's count of its entity and keeps the others. An id that names
  no entity is an error that lists the entity ids.
- The generators (`codegen`, `next`, and the lifecycle gate of the outputs) run at one slot
  per entity and read neither table.
- The `bounds:` and final lines name each count: `OK: no errors within 2 instances per entity
  (Loan=4), Int 0..5`. The TLA+ module's header names them, and the entity's id set is
  `Loan_Ids == 1..4`. The largest count sets the default `--max-patterns`.
- A pre-existing child that starts linked to the parent slot of its own index (a 1:1 key, or
  a `1..` range under a pre-existing parent) needs as many slots as its parent; other counts
  are the `IdentityStartSlots` error.
- A counted rule names the count it needs. `.at_most(3, Loan)` and a card `0..3:1` break only
  with four rows of `Loan`; below that the `Vacuous_*` warning ends `(Loan has 2 slots;
  --instances Loan=4 gives it 4)`. `.at_least(3, Loan)` and `.exactly(3, Loan)` with fewer
  slots draw the `CountAboveBound` warning with the same flag (see
  [Counted quantifiers](./language-reference.md#counted-quantifiers)).

### Instance symmetry
<!-- constrained-by ./design/instance-symmetry.md -->

The slots of an entity are interchangeable unless the model tells them apart: two states
that differ only in which slot holds which row reach the same verdicts. `verify --symmetry
always` (experimental) stores one state per class of such states, an orbit, so a cone that
outgrows the memory limit slot by slot can finish. [The design](./design/instance-symmetry.md)
has the group, the canonical form, and the soundness of each check.

```sh
rdra-ish verify samples/clinic-ops --instance-count 3 --memory 8GiB --symmetry always
```

```text
Cone_Inv_Encounter_2 [Inv_Encounter_2]: ok (about 820M states)
```

- **Which cones.** The safety cones (on a reduced cone, up to the group of its T rows, and
  its sub-lifecycle up to the group that keeps what the quotient reads), the cones of
  `always` properties left unreduced, the cones of `eventually` and `leads_to` (`.fair`
  included), and entity cones whose plain run stops at the memory limit. A cone that a
  complete plain run decides keeps that result and prints what `never` prints. Runs are
  judged alone, so `RDRA_JOBS` does not change the output. A cone whose group moves no row
  runs as under `never`.
- **The line.** `(about <m> states)`: the sum of the orbit sizes, to two figures. It counts
  every image of a stored state, reachable or not, so it can exceed what a plain run stores.
- **Counterexamples and deadlock traces** name concrete slots: the path is lifted back
  through every permutation the run applied and replayed through the step relation before it
  prints. A trace that does not replay is a tool bug and stops the run. A deadlock's trace
  may name another instance than `never` names.
- **Branch cap.** Past 4096 branches of the canonical form, a state is stored without it.
  The verdict stands, an orbit may be stored twice, and the line adds `note: <n> states stored
  without their canonical form (branch cap); the count is an upper bound`.
- **What breaks the symmetry** of an entity, which then keeps its slots fixed while the
  others permute: a comparison that orders row ids, a row id compared with a literal or
  another value, a key set to a literal row or chosen among some rows, a start that pins a
  slot, a creation that leaves a key at its first row, too many arrangements of rows that name
  each other. `RDRA_ISH_SYMMETRY_STATS=1` prints, per run up to symmetry, the orbits, the
  estimate, the states the branch cap cut, and each entity left out with the reason
  (`Item (a comparison orders its row ids)`); per stopped entity cone, the plain run's stop
  and the orbits of its symmetric run.
- **Shared runs.** A cone run up to symmetry stops once every check has failed, and decides
  nothing for the other passes.

### Memory limit
<!-- constrained-by #project-manifest -->

> **Known issue: the limit does not yet bound the whole process** (#237, #242, #243). It
> bounds what the state stores are counted to hold. On macOS the process has been measured
> past it: `verify samples/oidc-auth --instance-count 2` peaked at 1.85 times a `2GiB` limit,
> and passed 19.5 GB at the default `16GiB`. Until the fixes land, set `--memory` (or
> `memory`) to about half the memory you can spare when cones stop at the limit. Runs whose
> cones finish below it, as every sample does at the default limits, are not affected.

| Command | Limit |
|---|---|
| `check`, `states` | `--memory`, else `memory` under `[check]`, else `4GiB` |
| `verify` | `--memory`, else `memory` under `[verify]`, else `16GiB` |
| `codegen`, `next`, `diagram --kind state`, `export --kind tla` | `memory` under `[check]`, else `4GiB`; no flag |

The other outputs do not explore. A **size** is a whole number and a binary unit: `4GiB`,
`512MiB`, `64KiB`, `1TiB`. `G` and `GB` mean `GiB`, and likewise `K`, `M`, `T`. A bare
number, `0`, and a fraction are refused.

**What the limit counts.**

- **BFS.** Each cone's reached states, counted in bytes as stored: each state once, its
  parent, every edge, and the bindings. The count is the same on every machine and thread
  count, so a cone stops at the same state wherever it runs. A finished cone stays counted
  until its findings are read. `verify` gives back what a cone stopped at the limit held once
  it has read its findings. Cones run side by side, yet each stops where it would alone in
  order: a cone that met a pool the others filled runs again alone.
- **Shared quotients** (`verify`). Cones with equal sub-lifecycles share one quotient (#252),
  counted while held. A cone reuses it only when the runs that computed it fit its room;
  under a tight limit a later cone can stay unreduced. `RDRA_ISH_NO_QUOTIENT_CACHE=1` turns
  sharing off.
- **Not counted:** the transition system, the per-entity views, the states a worker holds
  while it expands them, and reading a cone's findings. On `insurance-claim-ops` at two
  instances the process peaked at 1.2 to 1.4 times the limit at `RDRA_JOBS=1` and up to twice it
  with eight. A cone can take 1.35 to 1.6 times what it is counted while it grows.
- **TLC** (`verify --with-tlc`). The TLC cone runs get half the limit (at least `64MiB`) and
  BFS the rest until BFS ends; a cone run that starts after that, and the whole-model property
  runs, may take all of it. Each
  run counts its `-Xmx` plus `256MiB`. A run's heap is 512 MiB per MiB of module text in whole
  GiB, at least `1g` (cone) or `2g` (whole model), at most the share. A run starts once its
  heap fits beside the running ones, or when none runs. A run that runs out of memory runs
  once more at twice its heap if the share holds it (`rerun at -Xmx<2h> after -Xmx<h> ran out
  of memory`), else it is a `tool_error`. A module that needs more than the share is not run:
  a `tool_error` naming the heap. `RDRA_TLC_HEAP` or an `-Xmx` in `JAVA_TOOL_OPTIONS` sets the
  heap as given, with no rerun. A `--memory` that leaves BFS nothing beside `64MiB` of TLC heap
  is refused.
- **TLC's cores** (`verify --with-tlc`). Each TLC run holds its `-workers` of the cores while
  it goes. A run gets one worker per 50,000 states BFS counted in its module, from 1 to 8. A
  run BFS has not counted gets 8 on 16 cores or more, and otherwise the cores divided among
  the runs left. When memory, not the number of runs waiting, bounds the
  runs at once, a run gets the cores divided among those that fit, up to 32. The largest runs
  start first. A run starts once the cores for its workers are free and its heap fits, or
  when none runs, and a smaller run does not pass it. Whole-model runs go 2 at once.
  `--tlc-workers` and `--tlc-jobs` set the workers and the most runs at once; see
  [docs/design/tlc-scheduling.md](./design/tlc-scheduling.md).
- **z3** (`verify --prove`). The proof starts once the safety cones are decided and runs
  beside the property and whole-model passes, on one z3 process per `RDRA_JOBS` (default:
  the cores, at most 8). Each process holds one core in the TLC runs' count while the proof
  runs, so under `--with-tlc` the runs that start meanwhile leave those cores free. Their
  memory is not counted and is not bounded by `--memory`: about 0.3 to 0.5 GiB per process
  (clinic-ops: about 1.3 GiB over four). Allow for it beside the limit, or lower `RDRA_JOBS`.
  z3 at its own `memory_max_size` exits, which would end the proof.

**A cone that reaches the limit** stops as one at `--max-states` does. `check` reports the
error `MemoryCapReached_<E>` and exits 1; `states` marks the views it cut; `verify` lists the
findings the cone leaves under `not checked:`, warns, and exits 0 (1 under
`--deny-undecided`), with `result: not decided`.

The `check` message names the limit, the states held, the bytes per state, and what the
cone's BFS layers say:

- **Past the peak** (two whole depths past the largest): the fall is taken to mirror the rise,
  and the message estimates the states and the memory for a quarter more. On 73 sample cones
  stopped at a tenth to nine tenths of their states, the 61 estimates came out at 0.51 to 1.37 times
  the true count.
- **Still rising:** no estimate; the message says how fast the frontier grows. Sample cones
  peaked after 36% to 100% of their states, so an early stop usually gets none.
- **Just past the peak, or a long fall:** no estimate, and the message says which.
- **No estimate from the layers:** the product of the variables' domains bounds the states,
  and when finishing at that many takes at most `1TiB` the message names it as a lower bound
  on what finishing needs. Most of the product is never reached.

A size is `(w + 12)·p2(n) + 5·buckets(n) + (4 + b)·p2(e)` bytes for `n` states of `w` bytes and
`e` edges, `b` the 1, 2, or 4 bytes of a binding id, `p2` rounding up to a power of two, and
`buckets` to a power of two of at least `n·8/7`. A state is packed: each variable takes
`ceil(log2(domain size))` bits, and `w` is their sum in whole bytes. A cone that stops once
every check fails keeps no edges: `(w + 8 + b)·p2(n) + 5·buckets(n)`. `e` is extrapolated
from the edges so far, so that part runs low. Every figure is for one cone; allow more.

```text
error[MemoryCapReached_Task]: state derivation for entity 'Task': memory limit 17KiB reached at 256 states; results are incomplete: the cone has 9 variables, 9 of them vary (...); new states first reached by: Flip7 127, Flip6 64, ...; held at 71 B per state; the frontier peaked at depth 5 and has shrunk for 3 layers since, so the cone probably holds about 258 states (its falling side taken to mirror its rising side: an estimate, not a bound); with a quarter to spare, this cone alone probably needs about 1MiB: try 1MiB or more with --memory or `memory` under [check] in rdra.toml, or narrow with --entity or --buc
```

(`check crates/rdra-ish-cli/tests/fixtures/memory/flags.rdra --memory 17KiB`; the cone holds
257 states.) `verify samples/patent-ops --memory 64MiB` prints, trimmed:

```text
not checked: 10 findings not decided
  whether some OfficeAction can get stuck at status = open is not decided: the OfficeAction cone stopped at the memory limit (64MiB; raise it with --memory or `memory` under [verify] in rdra.toml)
...
result: not decided
warning: 10 findings a cone stopped at the state cap or the memory limit left undecided; the OK line holds only for what was decided (--deny-undecided fails the run)
```

Under `--with-tlc` the reason adds the BFS share: `BFS has 256MiB of it` at `--memory
512MiB`.

Under `verify --prove`, an invariant cone stopped at the limit before a violation is decided
when z3 proves every rule in it for any number of rows. It reads `ok`, counts as decided, and
still says where BFS stopped. A cone with an `after` assertion, with a rule z3 leaves
unproved or out of scope, or stopped at `--max-states` stays `not decided`. See
[Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows).

```text
Cone_Forbidden_CartLine_0 [Forbidden_CartLine_0, Check_CartLine_qty_0]: ok (16 states, BFS stopped at the memory limit, decided by proof: --prove proved every check for any number of rows)
```

(`verify samples/formal-verification/check_met.rdra --instance-count 4 --memory 1KiB --prove`.)

---

## `fmt`

Format `.rdra` files.

```
rdra-ish fmt <INPUTS...> [--write | --check]
```

| Flag | Default | Meaning |
|---|---|---|
| `--write` | off | Rewrite the files in place. |
| `--check` | off | Change nothing; print a `needs formatting:` row per file that differs and exit 1. |

Without a flag, the formatted source goes to stdout, each file under a `// <path>` line when
there are several, and no file changes. **Exit codes:** `1` on a parse error or under
`--check` when a file differs, else `0`.

`fmt` reprints the parsed file. It keeps the model, identifiers, labels, metadata values,
columns, predicates, chains, tuples, and comparisons; a second run changes nothing. A
[deprecated form](./language-reference.md#deprecated-and-removed-forms) prints in its current
form, so `fmt --write` migrates it; a removed form is a parse error. The layout:

- The `module` line, one blank line, the `import` lines as one block, one blank line.
- A declaration whose only clause is `description` keeps it on its line while the line is at
  most 120 display columns (a wide character, such as most Japanese text, counts as two; a
  trailing comment does not count). Otherwise each clause gets its own line, indented two
  spaces, as do entity and DTO columns and variation values.
- A declaration on several lines is set off by one blank line each side. Between one-line
  items the source's grouping stands: a blank line is kept (several become one), none is
  added. Ids are not aligned.
- Annotations use compact spacing; a predicate call keeps its chain on one line.
- A trailing comment stays at the end of its line; an own-line comment stays above the line
  after it. A comment from a line `fmt` joins moves above its item.

---

## `diagram`

Write one RDRA diagram as Mermaid text (`.mmd`).

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--buc <ID>]... [--usecase <ID>]... [--entity <ID>]...
                 [--actor <ID>]... [--node-kind <KIND>]... [--edge-kind <KIND>]...
                 [--title] [--legend] [--format mermaid] [-o <OUT>]
rdra-ish diagram <INPUTS...> --all [--title] [--legend] -o <DIR>
```

The text is written for a reader that redraws it, a person or a tool, under the
[encoding contract](#encoding-contract): a `%%` header, node ids `<kind>__<DSL id>` that are the
same in every kind, a `:::<kind>` tag and a shape per kind, the RDRA layer names, and the
predicate names as edge labels where the ends do not already say the relation. Colours and
direction mean nothing beyond the tag.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <KIND>` | `rdra` | One of the [kinds](#kinds). |
| `--buc <ID>` | whole model | Draw what the BUCs reach, the union over every id; repeatable; every kind. The [kinds](#kinds) table says what each keeps. |
| `--usecase <ID>` | whole model | Draw these use cases; repeatable. `usecase`, `robustness`, and `sequence` (which needs exactly one, or `--buc`). |
| `--entity <ID>` | whole model | Repeatable. `state`: these entities' state models, every lifecycle column, whole (an entity with none is an error). `er`, `information`: these entities and those a `relate` joins to them. `usecase`, `robustness`: the use cases that read or write them, through an API included. |
| `--actor <ID>` | whole model | The use cases the actors perform, directly or through a BUC; repeatable. `usecase` and `robustness`; an actor that performs no use case is an error. |
| `--node-kind <KIND>` | all | `rdra` only: draw only these node kinds (`actor`, `buc`, `usecase`, `entity`, ...); repeatable. Alias `--kind-filter`. |
| `--edge-kind <KIND>` | all | `rdra` only: draw only these relations (`contains`, `invokes`, `reads`, `maps-field`, `relate`, ...); repeatable. Alias `--edge-filter`. |
| `--all` | off | Write the [review set](#the-review-set) into the directory `-o` names. Not with a scope, `--node-kind`, or `--edge-kind`; `--kind` is not read. |
| `--title` | off | Open with a Mermaid title (`---`, `title: "UC複合図: Order"`, `---`): the RDRA name and the labels of the scope. |
| `--legend` | off | Add a `凡例` subgraph to a `flowchart`, one node per node kind in its colour. Other diagram types are unchanged. |
| `--format` | `mermaid` | The only format. Render with `mmdc` or any Mermaid tool for an image. |
| `-o`, `--out <OUT>` | stdout | The output file. The extension is always `.mmd`: `-o out` writes `out.mmd`, `-o x.foo` writes `x.mmd`, `-o d.tar.gz` writes `d.tar.mmd`. With `--all`, the directory. |

`--usecase`, `--entity`, `--actor`, and `--buc` go one at a time, except where the table
pairs them. An unknown id is an error. `--node-kind` and `--edge-kind` hide nodes and the
edges whose ends are hidden; the `omitted:` line names what they hid.

**Removed values** fail (exit 2) with the replacement: `--kind boundaryless-graph`,
`business-area`, `technical-area`, `diff`, `event-flow`; `--format puml`, `svg`, `png`.
`--show-description`, `--view-preset`, and `--diff-base` are unknown arguments; descriptions
are always written as `%% desc` lines.

**stderr.**

- `warning: this diagram has 73725 characters and 475 edges; Mermaid's defaults stop at 50000
  characters (maxTextSize) and 500 edges (maxEdges), so it does not render as is.` The text is
  written anyway. The warning names the flags that narrow that kind; or raise both limits in
  the renderer.
- `note: nothing to draw: the model declares nothing --kind flow draws in this scope`. The
  text is the header alone, with `%% kinds: (none)`.

**Exit codes.** `0` when written; `1` on a load error, an unknown id, or a refused `state`
diagram ([Lifecycle errors](#lifecycle-errors)).

### Kinds

Each kind is one RDRA 3.0 diagram, or `rdra`, the base graph of every element and relation.

| Kind | RDRA diagram | Draws | Leaves out | With `--buc` |
|---|---|---|---|---|
| `rdra` | RDRAGraph | Every element in its layer and every relation | ADRs; columns (`er`), variation values and condition bodies (`condition`) | Everything reachable from the BUCs, and the states their use cases move between |
| `context` | システムコンテキスト | One derived edge per actor, labelled with the BUCs it performs, and one per external system and direction, labelled with the events exchanged | BUCs, use cases, and events as nodes | The actors and external systems with an edge through the BUCs, and the systems they reach |
| `requirement` | 要求モデル | Goals (with metric and `baseline → target`), the requirements that achieve them, and the BUCs they `motivate`; out-of-scope requirements apart in `やらないこと` | NFRs, constraints, qualities, entities, systems (`list --kind nfr`, `--kind constraint`), ADRs, BUCs no requirement motivates | The BUCs, the requirements that motivate them, and those requirements' goals |
| `business` | 業務の流れ図 | Each business with its BUCs, and a BUC-to-BUC hand-off edge labelled with the event that passes it | Actors and external systems (`list --kind buc-roles`); use cases, steps, events as nodes; a hand-off no event carries | The BUCs and the BUCs they hand off to or from, with their businesses |
| `usecase` | UC複合図 | Each use case with its actors, screens and what they show, entities read and written, events, external systems, timers, conditions, and transitions | BUCs, steps, media, permissions, fields, APIs (their CRUD folds into the use case), DTOs | The use cases the BUCs contain, directly or through a step |
| `robustness` | ロバストネス図 | Actors, boundaries (screens, external systems, timers), controls (APIs, or the use case), and the entities each control reads and writes | BUCs, steps, events (in edge labels), conditions, states, media, permissions, fields, DTOs | The use cases the BUCs contain |
| `sequence` | シーケンス図 | One use case in order: its starter, screen, controls, entities, external systems, guards, writes with `sets` values and the lifecycle move, and raised events | BUCs, steps, other use cases (a triggered one is one message), permissions, media, fields, DTOs | The BUCs' use cases one after another, each under a numbered note |
| `flow` | 業務フロー | Each flow with its steps, their `precedes`, `branches`, `excepts`, `repeats`, what each step `covers`, and a lane per actor | Actors without `performs(Actor, Step)`; screens; entities | The flows the BUCs contain |
| `screen` | 画面遷移図 | Each screen; a solid edge per `navigates`, labelled with its `.via` use case; where none is declared, a dashed `業務上の順番` edge between the screens of consecutive steps in one actor's lane | Use cases, fields, entities as nodes | The screens the BUCs' use cases display, and the order of the BUCs' flows |
| `information` | 情報モデル | Entities with their `relate` links, state models, and the variations their columns take | Columns (`er`), use cases | The entities `er` draws for the BUCs |
| `condition` | バリエーション・条件 | Each condition with the variations and state models it combines and what it applies to; each variation with its values | Entities (a state model stands for its column) | The conditions on the BUCs' use cases and their APIs, and what they combine |
| `state` | 状態モデル | Without a scope, an index of state models. With `--entity` or `--buc`, each state model whole, coloured by the lifecycle check | `Enum` columns no `transitions` drive (`omitted:`) | Each state model one of the BUCs' use cases moves |
| `event` | システム間連携（イベント） | `raises` and `triggers` between use cases, BUCs, external systems, events, and timers | States and transitions | The events reachable from the BUCs and their links |
| `er` | (RDRASpec logical data model) | Entities with columns and `relate` cardinality, as an `erDiagram` | Variation values and state models (column types only) | The entities reachable from the BUCs |

`state` is the one kind that reads the lifecycle. Every other kind draws declared facts.

**Per kind.**

- **`rdra`.** Layers: システム価値 (`actor`, `extsystem`, `requirement`, `nfr`), システム外部環境
  (`quality`, `constraint`, `business`, `buc`, `flow`, `step`, `timer`, `medium`, `permission`),
  システム境界 (`usecase`, `screen`, `field`, `event`), システム (`system`, `api`, `dto`,
  `entity`, `state`, `variation`, `condition`).
- **`context`.** No layers. An actor has one `-.->` edge to the system, labelled with the
  labels of the BUCs it performs, joined by `、`: a `performs` to a BUC counts it; one to a use
  case or step counts each BUC that contains it. An external system has one edge per
  direction, labelled with the events exchanged: toward the system for each event it raises
  that triggers a use case or BUC of the system, away for each event a use case raises that
  triggers it. Each edge is followed by `%% derived ... via <ids>`. With one declared system
  every edge ends there; with several, at each system that contains an API the use cases
  behind the edge invoke. With none, or none traceable, it ends at `system__Model["システム"]`,
  followed by `%% synthesized system__Model: <reason>`. `--buc` labels edges with the scoped
  BUCs only.
- **`requirement`.** No layers. A goal's label adds a line with its `metric` and `baseline →
  target`. Edges: `motivates` (requirement to BUC) and `achieves`, drawn goal to requirement
  and followed by `%% derived <goal> -> <requirement> via <requirement>`. A `status
  "out-of-scope"` requirement sits in `subgraph group_out_of_scope["やらないこと"]` with dashed
  edges. The whole model draws every goal and requirement, linked or not.
- **`business`.** No layers. Each business is `subgraph business__<id>["<label>"]`. A BUC in
  several businesses sits in the first by id, with a `belongs` edge to each other; a BUC of no
  business sits outside. `buc__A -->|<event label>| buc__B` is a hand-off: a use case of A
  (directly or through a step) raises the event, which triggers a use case of B or B itself.
  It is drawn once per `(A, B, event)` with `%% derived <A> -> <B> via event__<Id>`. A BUC with
  no edge is a gap to look at.
- **`usecase`.** Nodes keep their `rdra` layers and ids. Derived edges carry `%% derived`: an
  API's CRUD drawn from the invoking use case (`via api__<Api>`), and an actor performing the
  use cases of the BUC or step it performs (`via buc__<Buc>`, `via step__<Step>`). A use case
  an event or timer starts gets only its explicit `performs`. A condition has `-.->|applies_to|`
  to a use case, and a derived `-.->|when|` to a use case whose arm or `chooses` value its
  `.when` guards. A declared edge wins over a derived one. `omitted:` names the left-out kinds
  linked to a drawn node.
- **`robustness`.** One flat list, ordered actor, boundary, control, entity, each by id; no
  layers. Boundary: a displayed `screen`; an `extsystem` that raises an event starting a use
  case in scope, or that an event a use case in scope raises triggers; a `timer` that triggers
  a use case in scope. Control: each invoked `api`, and the use case itself when it reads or
  writes an entity directly or invokes no API. Edges: `actor --> screen`; `screen -->|<use case
  label>| control`; `control -->|<CRUD letters>| entity`; and dotted `-.->|<event label>|`
  between controls and external systems, and `timer -.-> control`, when both ends are in
  scope. Never drawn: actor to control, or screen, external system, or timer to entity. An
  actor whose use cases display no screen is left out.
- **`sequence`.** A `sequenceDiagram` of one use case; with `--buc`, the BUC's use cases in
  the order the model places them (`contains`, or a step's `covers`; a use case after those
  whose events trigger it), each under `Note over <screen>: <n>. <label>`. Participants, in
  order and only those taking part: the actors that perform it (an event- or timer-started
  one only its explicit `performs`) or the external system, timer, or raising use case that
  starts it; the screen; each control, as in `robustness`; the entities; and the controls and
  external systems its events reach. Messages:
  1. The actor calls the screen with the use case label, or the starter sends its event to
     the control.
  2. The screen calls the control with the use case or API label.
  3. With guards, `alt <guards>` opens: each applied condition with a body (label, then body),
     and per moved column `Entity.column == <from>` (or `in (<from>, ...)`) unless a condition
     says it. `else guard fails` ends with `rejected` to the screen.
  4. Each control `->>` its entities: `read`, then one transaction at a time, each write
     `create`, `update`, or `delete` with the lifecycle move (`status <from> -> <to>`) and the
     `sets` values (`paid_at=set`). Arms from one state to several are an `alt` with one branch
     per arm. A transaction of several writes, or inferred from keys, opens with `Note over
     <control>: transaction (...)`.
  5. For each raised event, `-)` to the control of each triggered use case and to each
     triggered external system; an event that triggers nothing is a `Note`.
  6. The control replies `done` to the screen, or to the starter.
- **`flow`.** `flowchart TD`, no layers. Each flow is `subgraph flow__<id>["<label>"]`; the
  whole model also draws steps of no flow. Edges: `-->|precedes|`, `-->|branches|`,
  `-.->|excepts|`, `-->|repeats|`, and `-.->` (`covers`) to the use cases, APIs, events, and
  timers a step covers. A step that exactly one actor performs sits in
  `subgraph lane__<Flow>__<Actor>`, whose first line is `%% lane lane__<Flow>__<Actor>:
  actor__<Actor>`. A step several actors perform has a `performs` edge from each. A BUC with no
  flow draws the header only.
- **`screen`.** `flowchart LR`, no layers, every screen in scope. `navigates(A, B)` is
  `screen__A -->|<use case label>| screen__B` (`-->|navigates|` without `.via`), and a `.via`
  adds `%% via screen__A -> screen__B usecase__<id>`. Where no `navigates` goes from A to B, two
  consecutive steps of one flow (`precedes`, `branches`, `excepts`) that the same single actor
  performs give `screen__A -.->|業務上の順番| screen__B` for each pair of screens their use cases
  display, with `%% derived ... via <flow and step ids>`. A dashed edge is the order the work
  visits screens, not a transition the UI offers.
- **`information`.** No layers. Each `relate` is `entity__<Child> ---|<card>| entity__<Parent>`
  (no child-side minimum). Each state model is `statemodel__<Entity>_<column>` with a
  `statemodel` edge; each variation a column takes has `-.->|variation <column>|`.
- **`condition`.** No layers. Each condition has `%% holds <id>: <body>` when it has a body,
  and `combines` edges to its variations and state models (`%% derived ... from holds` when read
  from the body). `applies_to` edges go to APIs and systems unlabelled and to use cases
  labelled. A `.when` guard adds a derived `-.->|when|` edge to the use case with `%% derived
  <condition> -> <usecase> via transitions <Entity>.<column> <from>-><to>` or `via chooses
  <Entity>.<column> <value>`. Each variation is `subgraph variation__<id>` with one
  `variation__<Variation>__<value>:::value` node per value. The whole model draws every
  variation.
- **`state`.** See the `stateDiagram-v2` part of the [encoding contract](#encoding-contract).
- **`event`.** Node ids `event__`, `usecase__`, `buc__`, `extsystem__`, `timer__`. Prints the
  event-integrity warnings (never raised, raised but never consumed, triggers a use case in no
  BUC); `outbox(Event)` silences the second. `%% omitted: state transitions` says a drawn use
  case drives one.
- **`er`.** The header only, then the `erDiagram`, whose entity names stay the DSL ids.
  `export --kind mermaid-er` writes the same text without `model=` in the header.

#### The review set

`--all -o <dir>` writes one file per diagram:

- `<kind>.mmd` for the kinds readable on a whole model: `context`, `requirement`, `business`,
  `information`, `condition`, `er`, `screen`, `event`, and `state` (the index).
- `<kind>__<BucId>.mmd` per BUC: `usecase`, `robustness`, `flow`, `sequence`.
- `state__<EntityId>.mmd` per entity with a state model.

A diagram with nothing to draw is left out; the last line counts them: `wrote 58 diagrams to
docs/diagrams (1 with nothing to draw left out)`. With lifecycle errors, the `state` diagrams
are left out with a note.

### Encoding contract

Every kind but `state` and `er` is a `flowchart` from which each fact it draws can be read
back:

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

- **Header.** `scope` is `whole`, `buc:<id>[,<id>]`, `usecase:<id>[,<id>]` (also what
  `--entity` or `--actor` keeps in `usecase` and `robustness`), or `entity:<id>[,<id>]`
  (`--entity` in `state`, `er`, `information`). `model` is the inputs as given. `layers:` (in
  `rdra` and `usecase`) and `kinds:` list only what the body draws. `omitted:` names the node
  kinds linked to a drawn node that the kind leaves out, and in `rdra` what a filter hid, or
  `(none)`.
- **Ids.** `<kind>__<DSL id>`, with the `--node-kind` tokens, so an element has one id in
  every kind. The label is the DSL label, always quoted, `"` written `#quot;`. Ids that name
  no DSL element of their own:

  | Id | `:::` | Kinds | What it is |
  |---|---|---|---|
  | `state__<Entity>_<variant>` | `state` | `rdra`, `usecase`, `state` | A state of a state model (`<Entity>_<column>_<variant>` with several lifecycle columns) |
  | `statemodel__<Entity>_<column>` | `statemodel` | `information`, `condition`, `state` | A state model |
  | `variation__<Variation>__<value>` | `value` | `condition` | One value, inside `subgraph variation__<Variation>` |
  | `lane__<Flow>__<Actor>` | `lane` (subgraph) | `flow` | One actor's lane in one flow |
  | `system__Model` | `system` | `context` | The system when no declared one stands for it |

- **Shapes** (`rdra_ish_emit::NODE_SHAPES`). Each node is `<id><open>"<label>"<close>:::<kind>`,
  so kinds differ in black and white too. The kinds one focused diagram draws have distinct
  shapes (a `value` sits inside its subgraph); `rdra` repeats shapes across layers. A subgraph
  is `subgraph <id>["<label>"]` whatever its kind.

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

- **Classes.** One `classDef <kind>` per kind closes the diagram. Each family has a light
  fill (actors red, BUCs and flows green, use cases blue, screens violet, events pink,
  information yellow, variations and conditions purple). A renderer may replace the colours.
- **Subgraphs.** A layer is `subgraph layer_<token>["<RDRA name>"]`. A subgraph with an element
  id (`business__<id>`, `flow__<id>`, `variation__<id>`) is that element.
- **Edges.** `<from> <arrow>|<label>| <to>`, from the predicate's first argument to its
  second. The label is left off when the two end kinds have exactly one relation in that kind;
  `rdra_ish_emit::implied_relation` reads it back from `rdra_ish_emit::EDGE_RELATIONS`. So
  `performs`, `invokes`, `displays`, `shows`, `raises`, `triggers`, `contains`, `covers`,
  `belongs`, `motivates`, `achieves` / `achieved_by`, `applies_to` (to an API or system),
  `combines`, robustness `uses` and timer `triggers`, and information `statemodel` go
  unlabelled. A condition to a use case is two relations in `rdra`, `usecase`, and
  `condition`, so `-.->|applies_to|` and `-.->|when|` are labelled. Labels stay where they add
  something: CRUD verbs, `request` / `response` / `error_response`, step order, a relate's
  cardinality (`N:1`), a transition (`transitions <Entity>.<column> <from>-><to>`, from the use
  case to the state it enters), robustness use cases, CRUD letters, and events, context and
  business labels, `variation <column>`, and every screen edge.
- **Line style** is the relation kind: `-->` for hierarchy (`contains`, `belongs`, `owns`) and
  flow order (`precedes`, `branches`, `repeats`), `---` for `relate`, `-.->` for every other
  association. A fact is drawn once.
- **Comment lines** say what shapes cannot; a renderer ignores them. An edge without a `%%
  derived` line is a declared relation.

  | Line | Follows | Kinds |
  |---|---|---|
  | `%% desc <id>: <text>` | A node with a `description`, lines joined by a space | every flowchart |
  | `%% derived <from> -> <to> via <ids>` | An edge derived through the named nodes | `business`, `usecase`, `context` |
  | `%% derived <from> -> <to> from holds` | A `combines` link read from a `holds` body | `rdra`, `condition` |
  | `%% derived <from> -> <to> via transitions <Entity>.<column> <from>-><to>` / `via chooses <Entity>.<column> <value>` | A `when` edge, `, `-joined | `rdra`, `usecase`, `condition` |
  | `%% holds <id>: <body>` | A condition with a body | `condition` |
  | `%% lane <lane>: actor__<id>` | A lane's first line | `flow` |
  | `%% synthesized system__Model: <reason>` | The synthesized system node | `context` |

**`--kind state`** is a `stateDiagram-v2`; the header and id prefixes carry the kinds:

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

- Each state model is a composite state `statemodel__<Entity>_<column>` labelled
  `<Entity>.<column>`, its states in the `Enum` order with their `rdra` ids.
- A transition is labelled with the use case label, then `[...]` with each condition whose
  body guards it. The line before, `%% transition <usecase id>`, names the use case.
- `[*] -->` marks the start state; `--> [*]` an end state (no transition out).
- `omitted:` names the in-scope `Enum` columns no `transitions` drive and no `sets` writes. One
  a `sets` writes is a state axis of `check`, not a state model, and is named on `%% set-only:`.
- `--buc` draws each state model a BUC's use case moves; `--entity` (not with `--buc`) the
  entities' state models. With neither, the diagram is an index: each composite state holds
  one note, `<n> 状態 / <m> 遷移 (--entity <Entity>)`, and nothing else; the header's kinds are
  `statemodel state`.
- A full diagram colours each state by the first class that applies, in this order, with a
  `class <state id> <class>` line, a `classDef` per class used, and the `%% legend:` line:

  | Class | State | Found by |
  |---|---|---|
  | `statedeadlock` | One an instance cannot leave though it has a way out | `Deadlock_<Entity>_<column>_<value>` |
  | `stateunreachable` | A declared value no reachable state holds, drawn with no transition | `UnreachableEnumVariant` (warning) |
  | `stateend` | One with no transition out, drawn `--> [*]` | The transitions |

---

## Sample artifact snapshots

`scripts/sample-artifacts.tsv` lists every committed `out/` file of the samples (`ec-site`,
`clinic-ops`, `patent-ops`, `food-permit-ops`, `insurance-claim-ops`, `oidc-auth`,
`personal-info`, each `incremental-order` step, `api-contract`), each with the command that
writes it. CI runs `scripts/check-sample-artifacts.sh <binary>`, which runs every row, then
`scripts/embed-sample-artifacts.py` (copies each file into the block under its `<!-- embed:
<path> -->` line in the step `design.md` files and `docs/diagram-sample-review.md`, and fails
when a printed command is not the row for its `--out`) and `scripts/gen-clinic-ops-docs.py`
(writes `design-doc.md` and `prd.md` of the four operations samples). It fails if a tracked
file changes, so a generator change shows as a diff. After an intended change, run the script
and commit the files.

`scripts/data-diff.sh LABEL` writes the DBML, JSON Schema, and entity CSV of every
`samples.txt` root, with exit codes and logs, under `target/data-diff/LABEL/`, using
`target/release/rdra-ish` (`RDRA_BIN` overrides). Compare two builds with `diff -r`.
`scripts/verify-samples.sh <binary>` runs `check` and `verify` on every `samples.txt` root
against its `// expect` header ([Sample expectations](./formal-verification.md#sample-expectations)).

---

## `csv`

Write one table as CSV or Markdown to a file.

```
rdra-ish csv <INPUTS...> [--kind <KIND>] [--format csv|markdown] [-o <OUT>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--kind <KIND>` | `entity` | The table; see below. |
| `--format` | `csv` | `markdown` writes the rows as a Markdown table, `\|` escaped and line breaks as `<br>`. |
| `-o`, `--out <OUT>` | `out` | A path with an extension is used as is. Otherwise `.<kind>.csv` (or `.<kind>.md`) is appended: `-o out` writes `out.matrix.csv` for `--kind matrix`. |

| Kind | Rows |
|---|---|
| `actor` | Actors. |
| `entity` | Each entity column with its type and key, index, check, and marker flags. |
| `matrix` | Use case × entity CRUD. |
| `api` | API contracts: `id,label,method,path,idempotency,mode,auth_scheme`. |
| `api-matrix` | API × entity CRUD. |
| `screen-constraints` | Screen × use case / API permission and medium paths. |
| `permission-callables` | Each permission with the use cases and APIs that require it, with `UseCase->Api` paths for API-level requirements. |
| `actor-permission-audit` | Actor × permission: `missing` and `excess` assignments inferred from the requirements. |
| `business-inputs` (alias `actor-inputs`) | Actor × inferred input field × use case, from C/U paths. |

The command prints `wrote <path>` and no rows. **Exit codes:** `0` when written; `1` on a
load error.

---

## `export`

Write one machine-readable artifact.

```
rdra-ish export <INPUTS...> [--kind <KIND>] [-o <OUT>] [--dialect postgres|sqlite]
                [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--kind <KIND>` | `openapi` | `openapi`, `asyncapi`, `dbml`, `sql`, `json-schema`, `mermaid-er`, or `tla`. |
| `-o`, `--out <OUT>` | `out` | See below. |
| `--dialect` | `postgres` | `sql` only: `postgres` or `sqlite`. |
| `--instance-count <N>` | `2` | `tla` only: slots per entity, `1..=255`; `1` warns (one id cannot tell instances apart). |
| `--instances <ENTITY=N>` | `instances` under `[verify]` | `tla` only: one entity's slots; repeatable. See [Per-entity instance counts](#per-entity-instance-counts). |
| `--int-range-max <N>` | `5` | `tla` only: `IntRange == 0..N`, `1..=255`. |

**Output path.** For every kind but `tla`, a path with an extension is used as is; otherwise
the kind's suffix is appended: `out.openapi.json`, `out.asyncapi.json`, `out.schema.dbml`,
`out.schema.sql`, `out.json-schema.json`, `out.er.mmd`. For `tla`, a path ending in `.tla` is
the module (its stem the module name) with a sibling `.cfg`; any other path is a directory,
created if missing, that receives `RdraSpec.tla` and `RdraSpec.cfg` (`-o out` writes
`out/RdraSpec.tla`); a path with another extension writes into its parent. Beside them, `tla`
writes `StateModel_<E>.tla`, `StateModel_<E>.cfg`, and `StateModel_<E>_witnesses.cfg` per
entity cone with a state model, which check its deadlocks and witnesses with TLC
([State models with TLC alone](./formal-verification.md#state-models-with-tlc-alone)). The
command prints `wrote <path>`.

**Removed kinds** fail (exit 2) with the replacement: `plantuml-er` (use `mermaid-er`), and
`typescript-states` / `type-script-states` (use `codegen <INPUTS> --lang ts -o <FILE>.ts`).

**Exit codes.** `0` when written; `1` on a load error or a refused `tla`
([Lifecycle errors](#lifecycle-errors)).

| Kind | What it writes |
|---|---|
| `openapi` | OpenAPI 3.0 JSON. Only APIs with both `method` and `path` become operations. `{name}` path segments are required `in: path` string parameters. `auth` `bearer` / `api_key` map to standard schemes; another value is an `apiKey` header `X-Auth` with `x-rdra-ish-auth-scheme`. `requires_permission(Api, P)` is `x-rdra-ish-requires-permission`, not `security`. DTOs are `components.schemas`. |
| `asyncapi` | AsyncAPI 3.1. Each `event` is a channel and message. `raises` and `outbox` are `send` operations; `triggers` are `receive` operations listing the triggered use cases' `transitions`. An external system that raises the event is `x-publisher`; one it triggers, `x-subscriber` (each only when non-empty). Protocols, servers, and payloads are not modeled; other facts are `x-rdra-ish-*`. |
| `dbml` | One table per `entity`, with its Enum types (one per inline Enum column, one per variation), indexes, unique sets, and a `Ref` per key (`-` for 1:1, the composite form `Ref: Stock.(bin_site, bin_shelf) > Bin.(site, shelf)`). `@check`, `@soft_delete`, `@history`, `@tenant`, and `@derived` are column notes; a table's rules are listed in its `Note` by name, label, and text (a multi-entity rule on its first entity). |
| `sql` | SQL DDL; see [SQL export](#sql-export). |
| `json-schema` | Draft 2020-12 definitions under `$defs` as `Dto.<id>` and `Entity.<id>`. Keys, indexes, checks, soft delete, history, tenant, and derived expressions are `x-rdra-ish-*`: `x-rdra-ish-foreign-key`, `x-rdra-ish-fk-target`, `x-rdra-ish-fk-target-column` (composite or `.references` keys), `x-rdra-ish-fk-role` (explicit `.role`), `x-rdra-ish-fk-min-children: 1`, `x-rdra-ish-version`, `x-rdra-ish-maps-to`, `x-rdra-ish-check-constraints`, `x-rdra-ish-derived`. |
| `mermaid-er` | The text of `diagram --kind er`, without `model=` in the header. |
| `tla` | A TLA+ module and TLC config of the lifecycle and rules. See [formal-verification.md](./formal-verification.md). |

### SQL export

`export --kind sql` writes one script that creates the data model in an empty database:
`CREATE TABLE` per entity in id order, the indexes, then (PostgreSQL) the foreign keys. Every
sample's script loads into SQLite and PostgreSQL 16.

| Model | PostgreSQL | SQLite |
|---|---|---|
| `Int` | `INTEGER` | `INTEGER` |
| `String` / `String(n)` | `TEXT` / `VARCHAR(n)` | `TEXT` / `VARCHAR(n)` |
| `Money`, `Decimal` / with `(p, s)` | `NUMERIC` / `NUMERIC(p, s)` | `NUMERIC` / `NUMERIC(p, s)` |
| `DateTime`, `DateTime(tz)` / `DateTime(local)` | `TIMESTAMPTZ` / `TIMESTAMP` | `TEXT` (ISO 8601) |
| `Date` | `DATE` | `TEXT` (ISO 8601) |
| `Bool` | `BOOLEAN` | `BOOLEAN` |
| `Enum(...)` | `CREATE TYPE "<Entity>_<column>" AS ENUM (...)` | `TEXT` with `CHECK ("<col>" IN (...))` named `<Entity>_<column>_values` |
| variation `V` | `CREATE TYPE "V" AS ENUM (...)` | as an `Enum` |

- **Keys.** `@pk` / `@pk(a, b)` is the `PRIMARY KEY`. `@unique`, `@unique(a, b)`, a junction pair,
  and a 1:1 key are `UNIQUE`. Each `relate` key is a `FOREIGN KEY` `<Child>_<columns>_fkey` with
  its `.on_delete` / `.on_update`, referring to the primary key or the `.references(...)`
  columns; PostgreSQL adds it by `ALTER TABLE`, SQLite in the table.
- **Soft delete.** With a `@soft_delete` column, a declared unique set holds among live rows:
  `CREATE UNIQUE INDEX "<Entity>_<columns>_live_key" ... WHERE` the marker is null (or `FALSE`).
  The primary key, a key a `relate` makes unique, and a set a foreign key refers to stay full
  `UNIQUE`.
- **Names.** A table takes the entity's `table` name, else its id, and so do its key, index,
  and Enum-check names. Rule constraints keep the entity id.
- **Computed values.** `@default(now())` is `DEFAULT CURRENT_TIMESTAMP` (`CURRENT_DATE` on a
  `Date`). `@generated(identity)` is `GENERATED BY DEFAULT AS IDENTITY` (SQLite: a single
  `INTEGER` primary key is the rowid). `@version` is `DEFAULT 1` with a comment.
  `@generated(uuid)` and `@default(uuid())` are `DEFAULT gen_random_uuid()::text` in PostgreSQL.
  What a dialect cannot compute (a UUID in SQLite, a ULID, `@on_update(now())`) is a comment.
- **Columns.** No `@null` is `NOT NULL`. `@default(v)` is `DEFAULT v`. `@derived` is `GENERATED
  ALWAYS AS (...)` (`STORED` in PostgreSQL, `VIRTUAL` in SQLite). `@label`, `@soft_delete`,
  `@history`, `@tenant`, and `@derived` are comments.
- **Indexes.** `@index` / `@index(a, b)` is `CREATE INDEX "<Entity>_<columns>_idx"`. Each
  foreign key gets one unless the primary key, a unique set, or an `@index` starts with its
  columns.
- **Rules.** Each entity-local rule is a `CHECK` named as `check` names it: `@check`
  (`Check_<E>_<column>_<n>`), `forbidden` (`Forbidden_<E>_<n>`), `invariant ... .then`
  (`Inv_<E>_<n>`), `required` (`Required_<E>_<n>`), `exclusive` (`Exclusive_<E>_<n>`). SQL reads
  a null operand as unknown, which a `CHECK` accepts, so each comparison on a nullable column
  spells out the [null](./language-reference.md#null) case: `qty: Int @null @check(qty >= 1)`
  is `CHECK (("qty" IS NOT NULL AND "qty" >= 1))`. `in` / `not in` are `IN` / `NOT IN` with the
  same null case. A rule's `.label` is a comment above its `CHECK`.
- **Reads.** The script ends with comments: first the indexes lookups want and nothing gives,
  as `-- CREATE INDEX ... ; -- <readers>` (the `reads` [query shapes](./language-reference.md#query-shapes),
  a timer-started use case that finds rows by state, a `.has` / `.none` over 1,000,000 or more
  `rows`, a guard over an unjoined entity), with `small table, <n> rows: a scan is cheap` under
  10,000 `rows`. Then each declared read as its `SELECT`: tenant and `.by` columns compared to
  parameters (`$1` / `?1`), live rows only under `@soft_delete`, `ORDER BY`, and the page. A
  cursor page continues after the last row (`("created_at", "id") < ($2, $3)`; one disjunct per
  key for mixed directions). `.count()` and `.sum(col)` select `count(*)` and `sum("col")`. A
  read by an API selects the columns its response DTO maps, else `*`.
- **Not enforced.** A multi-entity rule (`MultiForbidden_*`, `MultiInv_*`), a `.has` / `.none`
  (`Quantifier_*`), a minimum cardinality (`MinCard_*`), a rule with `.when(Condition)`, and a
  rule that reads `now` have no `CHECK`. A closing comment lists each by name, reason, label,
  and rule, as an obligation of the application or a trigger.
- **Layout.** `-- Types` (PostgreSQL), `-- Tables`, `-- Foreign keys` (PostgreSQL). A variation's
  label and value labels are a comment above its type, `-- PaymentMethod (Payment method): cash =
  Cash, card`; SQLite lists them under `-- Value sets`.

---

## `verify`

Check every invariant, `after` assertion, and temporal property, each on its own cone, with
the native checker. No JVM starts unless `--with-tlc` is given. See
[formal-verification.md](./formal-verification.md).

```
rdra-ish verify <INPUTS...> [--with-tlc [--tlc-jar <PATH>] [--tlc-jobs <N>] [--tlc-workers <N>]
                [--plan <FILE>]] [-o <DIR>]
                [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>]
                [--max-states <N>] [--max-patterns <N>] [--memory <SIZE>] [--symmetry never|always]
                [--temporal-budget <SECS>] [--deny-undecided]
                [--prove [--deny-unproved] [--z3 <PATH>]]
                [--buc <ID>]... [--entity <ID>]... [--without-usecase <ID>]...
                [--without-buc <ID>]... [--actor <ID>]...
```

| Flag | Default | Meaning |
|---|---|---|
| `--with-tlc` | off | Also run TLC on every cone and property run, on each `stable`, `unchanged`, and `precedes` cone, and on the state models of each entity cone BFS finished, up to 100,000 states. Fails (exit 1) when the engines decide a check differently. `can_reach` has no TLC run. Needs TLC ([Getting TLC](#getting-tlc)). |
| `--backend tlc` | none | Deprecated spelling of `--with-tlc`; prints `warning: --backend tlc is deprecated: ...`. The next major release removes it. |
| `-o`, `--out <DIR>` | none; a temp dir under `--with-tlc` | Write the specs for a TLC run by hand; see [Specs](#with-tlc). The native checker reads none of them. |
| `--instance-count <N>` | `2` | Slots per entity, `1..=255`; `1` warns. |
| `--instances <ENTITY=N>` | `instances` under `[verify]` | One entity's slots; repeatable. See [Per-entity instance counts](#per-entity-instance-counts). |
| `--int-range-max <N>` | `5` | `IntRange == 0..N`, `1..=255`. |
| `--max-states <N>` | none | State cap of every BFS run (entity, safety, and property cones, `_Full` and `_WF` included, and the whole-model run). A run past it is `not decided`. Under `--with-tlc` TLC's whole-model run keeps 2,097,152 states without it; its cone runs have no cap. |
| `--max-patterns <N>` | `256` | Accepted for scripts; no `verify` run builds an entity view, so it stops nothing. |
| `--memory <SIZE>` | `memory` under `[verify]`, else `16GiB` | BFS takes all of it; under `--with-tlc`, half, and the TLC runs the rest (at least `64MiB`). See [Memory limit](#memory-limit). |
| `--symmetry` | `symmetry` under `[verify]`, else `never` | Experimental. `always` explores cones up to instance symmetry; see [Instance symmetry](#instance-symmetry). |
| `--temporal-budget <SECS>` | `300` | Seconds a property's whole-model run gets when its cone does not decide it. Past them (or `--max-states`, or `--memory`) the property is `not decided` unless a counterexample was found. Under `--with-tlc`, TLC's whole-model run gets the same. |
| `--deny-undecided` | off | Exit 1 when a finding is left undecided: a property past the budget, or a check, state, deadlock, or use case a cone left at a cap. What `--buc` or `--entity` leaves open does not count. |
| `--prove` | off | Experimental. Also try to prove each selected state rule (`forbidden`, `invariant`, `required`, `@check`, quantifiers, `Unique_*`, `MinCard_*`) for any number of rows and any non-negative Int (`now` has no top) with z3. See [Proofs](#proofs). Needs z3 ([Getting z3](#getting-z3)). |
| `--deny-unproved` | off | With `--prove`: exit 1 when a rule is unproved. A rule out of scope, or one BFS refuted (already an error), does not count. |
| `--z3 <PATH>` | `RDRA_Z3`, else `z3` on `PATH` | The z3 binary for `--prove`. |
| `--tlc-jobs <N>` | `RDRA_TLC_JOBS`, else as many as the cores and memory allow | With `--with-tlc`: at most `N` TLC runs at once. The memory they share still bounds them. The verdicts do not change. Without `--with-tlc` it warns. |
| `--tlc-workers <N>` | `RDRA_TLC_WORKERS`, else by the states BFS counted | With `--with-tlc`: every TLC run gets `-workers N`; a safety cone that does not pass still reruns on one worker. See [Memory limit](#memory-limit). Without `--with-tlc` it warns. |
| `--plan <FILE>` | none | Experimental. With `--with-tlc` and `-o`: run the native checker as usual, write the specs, and write to `FILE` as JSON the TLC launches `--with-tlc` would start first, and start no TLC. Each launch gives its module, cfg, kind, checks, the cone it is checked in, BFS's verdict and states, and the heap, workers and arguments the scheduler would give it, with the runs that follow from what it prints. No `tlc:` or `tlc_result:` line prints. The format (`"version": 1`) may change in any release. |
| `--tlc-jar <PATH>` | `RDRA_TLC_JAR` | With `--with-tlc`: run `java -cp <PATH> tlc2.TLC` from `tla2tools.jar` instead of a `tlc` / `tlc2` launcher. Needs `java`. Without `--with-tlc` it warns and starts nothing. |
| `--buc`, `--entity`, `--without-usecase`, `--without-buc`, `--actor` `<ID>` | whole model | See [Partial runs](#partial-runs). A scoped run checks no property. |

**What runs.** Warnings and errors of the transition system print first; a build error stops
`verify` before any cone runs.

- Each invariant and `after(..).assert` is checked on its own **cone of influence**, a
  restriction of the system whose reachable states are the whole model's projected onto it (a
  superset where a cone action creates an entity the cone does not read). Cones run side by
  side. A `.lemma()` rule gets no cone: `check` checks it, and `--prove` uses it once proved.
- Each `always`, `eventually`, and `leads_to` is checked on a cone of what it reads and of the
  use cases its `.fair` lists: one run (`Cone_<P>`) or two (`Cone_<P>_WF` adds `WF_vars(Next)`).
  When neither decides it, the whole model decides it (`RdraSpec_<P>`) after the cones, under
  `--temporal-budget`. See [Temporal properties on a cone](./formal-verification.md#temporal-properties-on-a-cone).
- `stable`, `unchanged`, and `precedes` are decided by BFS on their cones: `order and stability
  properties: <n>, each decided by BFS on its cone`, then `property <P> (BFS on its cone):
  ok|violated|not decided: ...` per property
  ([Order and stability properties](./formal-verification.md#order-and-stability-properties)).
- `can_reach` is decided by BFS on its cone, with no TLC run: `can_reach properties: <n>,
  decided natively by BFS on each one's cone (TLC cannot check AG EF)`, then `can_reach (BFS on
  <k> cones): <n> checked, <v> violated` (and `, <u> not decided`)
  ([Recoverability on a cone](./formal-verification.md#recoverability-on-a-cone)).
- Each **entity cone** is explored with BFS at the same bounds, before the cone lines. It
  reports what `check` reports for state models: a state with no witness and a deadlock are
  errors that fail the run even when every check holds; a deadlock at the bound and a use case
  that never runs are warnings. It also warns `Vacuous_<name>` for a rule, assertion, or
  property that holds vacuously ([Rules that hold vacuously](./state-derivation.md#rules-that-hold-vacuously)).
  Then: `state models (BFS on 2 entity cones): 2 state models, 5 states witnessed, 0 deadlocks,
  0 use cases that never run`.

A condition's `holds` body applied to a use case is a guard of the step relation and of the
TLC spec; `verify` prints no notice for conditions.

**Output.** stdout names the bounds (`bounds: 2 instances per entity, Int 0..5 (TLA+
InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; ...`), the cone
count, and each run's result. A violation prints once in model terms, through `check`'s
renderer, with BFS's shortest trace, from the first cone that breaks it:

```
$ rdra-ish verify samples/formal-verification/cross_order_payment_split.rdra
...
Cone_MultiForbidden_Order_0 [MultiForbidden_Order_0]: counterexample (31 states)
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: ...
  rule: cross_order_payment_split.rdra:44:1
  counterexample (3 steps):
    ...
result: counterexample
Error: verification failed: 1 invariant or after-assert violation(s)
```

A violated property prints `error[<Property>]: property <Property> is violated: ...`. The head
names the instance and the step that shows the failure (for `leads_to`, the step after which
the trigger holds). An `always` counterexample ends at the breaking state. An `eventually` or
`leads_to` counterexample is a lasso: its last numbered step is followed by `then loops back to
step K` or `then nothing changes forever`. A native counterexample that does not replay, does
not violate the property, or loops unfairly prints `error[LassoFault]: <module> [<P>]: ...; a
tool bug, not a finding about the model` ([What verify checks](./formal-verification.md#what-verify-checks)).

**Result line.** Every run that reaches its checks prints `result:
ok|not decided|counterexample|tool_error`: `ok` on stdout only when the run exits 0 with
nothing undecided, otherwise on stderr.

| Result | Meaning | Exit |
|---|---|---|
| `ok` | Every check holds and nothing is undecided. | `0` |
| `not decided` | Nothing failed, but a cap, the budget, or `--deny-unproved` left something open. stderr: `warning: <what> undecided; the OK line holds only for what was decided (--deny-undecided fails the run)`. | `0`; `1` under `--deny-undecided` or `--deny-unproved` |
| `counterexample` | A check or property is violated, or a state-model finding fails (a deadlock, a state with no witness, a `can_reach` error). Ends `Error: verification failed: <n> invariant or after-assert violation(s)` and/or `<n> property violation(s)`. | `1` |
| `tool_error` | A native counterexample failed its own check, the engines disagree, or a TLC run failed without a model violation. | `1` |

Usage errors exit 2.

### With TLC

Under `--with-tlc`, `verify` writes the specs and runs TLC on them while BFS runs, then on
the whole-model module of each property TLC's cones leave undecided. Each cone run uses
`-continue`. TLC reports one violated check per state, so a cone run with a violation reruns
without the checks it reported (`<module>_rest<n>.cfg`) until no new one appears.

**Specs** (`-o`, or a temp dir kept when TLC fails or the engines disagree):

- `Cone_<check>.tla` / `.cfg` per safety cone, named after its first invariant or `after`
  assertion;
- `Cone_<Property>` per `always`, `eventually`, and `leads_to`, with `Cone_<Property>_WF` for an
  `eventually` or `leads_to` and `Cone_<Property>_Full` for a reduced cone;
- under `--with-tlc`, `Cone_<Property>` per `stable`, `unchanged`, and `precedes`, and the
  `StateModel_<E>` specs of each entity cone TLC checks;
- `cones.txt`, listing each module and the checks it answers;
- with temporal properties, `RdraSpec.tla` / `.cfg` with every `PROPERTY`, and
  `RdraSpec_<Property>` per property its cone leaves undecided.

A `-o` path ending in `.tla` writes into its directory and names the whole-model module after
its stem (`<stem>_<Property>` per property). With `-o`, stdout says `wrote <n> specs and
<dir>/cones.txt`.

**Comparison.** `verify` compares the engines check by check: each safety cone's verdict and
the checks its counterexample breaks, and each property's verdict on every run both made.
`not decided` on one side is no disagreement. Two verdicts print `error[EngineDisagreement]:
<module> [<checks>]: native <verdict>, TLC <verdict>; one engine is wrong (a tool bug, not a
finding about the model)`; both result lines read `tool_error`, and the run fails with `<n>
check(s) the native checker and TLC decide differently (a tool bug)`. Every TLC counterexample,
each trace of each run, is replayed through BFS's step relation, whatever the native verdict;
a run with a trace that does not replay prints `error[EngineDisagreement]: <module> [<checks>]:
<k> of TLC's <n> counterexample(s) do not replay through BFS's step relation (the first:
<why>); ...` and counts as one disagreement.

**TLC's output** prints after the native lines. Each TLC run prints a `tlc: ` line, and a
`tlc_result: ok|counterexample|tool_error` line follows `result:`. The line of a cone,
property, or whole-model run ends with what its TLC launches cost: their wall time added up
(the reruns, `_rest<n>`, `_Full`, and `_WF` included), the `-Xmx` of the last launch, and the
workers of the first, as `(31 states, 0.9 s, -Xmx1g, 8 workers)`. A cone checked in another
cone's run, and a module over the share, launch nothing and print no cost. The `tlc:` lines
are not covered by [compatibility.md](./compatibility.md#what-is-not-covered); `tlc_result:`
is. A failed run prints under
`tlc: <module> (<path>):` the counterexample decoded and replayed through BFS's step relation
(the lowest binding when several give the same state), marked `from TLC`, then TLC's
`violated: ...` line and raw trace. A TLC step that matches no move prints `error: TLC step <n>
(label <action>) matches no move of the cone; BFS and TLC disagree (a tool bug). Raw TLC trace
follows.` TLC's lasso ends `State N: Stuttering` or `Back to state K: <action>`. A TLC run that
fails without a model violation fails the run with `<n> TLC run(s) that failed without a model
violation`. When TLC does not pass or the engines disagree, the run ends `specs: <dir>`, with
`(kept because a check failed)` or `(kept because the engines disagree)` for a temp dir.

```
$ rdra-ish verify samples/formal-verification/cross_order_payment_split.rdra --with-tlc
...
tlc: Cone_MultiForbidden_Order_0 [MultiForbidden_Order_0]: counterexample (31 states, 1.6 s, -Xmx1g, 8 workers)
tlc: StateModel_Order (31 states): 1 deadlock and 3 witness checks; found none
tlc: StateModel_Payment (31 states): 1 deadlock and 2 witness checks; found none
tlc: Cone_MultiForbidden_Order_0 (/tmp/rdra-ish-tlc-4242/Cone_MultiForbidden_Order_0.tla):
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: ...
  counterexample (3 steps, from TLC):
    ...
violated: Error: Invariant MultiForbidden_Order_0 is violated.
counterexample:
  State 1: Initial predicate
    ...
result: counterexample
tlc_result: counterexample
specs: /tmp/rdra-ish-tlc-4242 (kept because a check failed)
Error: verification failed: 1 invariant or after-assert violation(s)
```

TLC's heap and the memory it shares are under [Memory limit](#memory-limit). A run that runs
out of memory twice, or cannot rerun, is a `tool_error` whose message names `RDRA_TLC_HEAP`
and `JAVA_TOOL_OPTIONS`.

#### Getting TLC

`verify` looks for TLC in this order: `--tlc-jar`, `RDRA_TLC_JAR`, then `tlc` and `tlc2` on
`PATH`. Without one, `--with-tlc` exits 1. Homebrew has no formula that puts `tlc` on `PATH`.

- **Nix:** `nix shell nixpkgs#tlaplus` (add `--extra-experimental-features 'nix-command flakes'`
  if flakes are off); this gives TLC 1.7.4 on JDK 8. In this repository `nix develop`, and the
  flake's package on the `PATH` of `rdra-ish`, give TLC 1.8.0 on JDK 25 with a class-data
  archive that starts runs sooner.
- **The jar:** download `tla2tools.jar` from the
  [TLA+ releases](https://github.com/tlaplus/tlaplus/releases), install a JDK (11 or later for
  TLC 1.8.0), and pass `--tlc-jar` or set `RDRA_TLC_JAR`.

`verify` reads TLC 1.7.x and 1.8.0. TLC 1.8.0 is a pre-release rebuilt under one tag; the flake
pins the build of 2026-11-19 (`TLC2 Version 2026.11.19.180144`) and downloads it from this
repository's release
[`tlc-1.8.0-bc1b506`](https://github.com/manji-0/rdra-ish-dsl/releases/tag/tlc-1.8.0-bc1b506).

Use TLC 1.8.0, or TLC 1.7.x on JDK 17 or earlier. On JDK 21 and later, TLC 1.7.x reads a trace
record back wrongly, and a `-continue` run that prints many traces stops with
`java.lang.ArrayIndexOutOfBoundsException` (#314). `verify` warns once (`warning: TLC 2.19 (TLC
1.7.x) runs on Java 21.0.11; …`) when the first banner shows that pair. A run that stops on the
exception is a `tool_error`, even after printing violations; its report starts `hint: TLC
stopped with an ArrayIndexOutOfBoundsException while it read a trace back, …`.

#### Getting z3

`verify --prove` looks for z3 in this order: `--z3`, `RDRA_Z3`, then `z3` on `PATH`. Without
one, the run stops with an error.

- **Nix:** `nix shell nixpkgs#z3`; `nix develop` here provides it.
- **A package manager:** `apt install z3`, `brew install z3`, or `pip install z3-solver`.

The proof's budgets were measured on z3 5.1.0, the version `nix develop` provides. Another
version, as a package manager may install, runs the same and prints a `note:` (see
[Proofs](#proofs)).

### Proofs

`--prove` prints one `proved:`, `unproved:`, `out of scope:`, or `refuted:` line per selected
rule. A rule left `unproved` that BFS passes is checked again at one and two instances more,
on its own cone: an `escalated: <rule>: ok at 3 instances, counterexample at 4 instances (BFS
on its cone; ok at 2)` line follows the proof's, and a counterexample there is an error that
fails the run, with its trace. The set of rules and lemmas a proof keeps is stored as a certificate under the user's cache
directory (`XDG_CACHE_HOME`), never in the project, and offered to the next run on the same
encoding, which re-checks it in one round and reuses it only when every rule is in it and the
round passes. A missing, stale, or damaged certificate costs only that round; the output is the
same with or without one. `RDRA_ISH_NO_PROOF_CACHE=1` turns it off. A rule BFS violated in the same run is not tried: it reads `refuted: <rule> (BFS
counterexample at <n> instances)`, with no CTI, and `--deny-unproved` does not count it. The
`OK:` line gains `; <n> of <m> rules proved for any number of rows and any Int value`. A rule
the SQL schema does not enforce is marked `[not enforced by the SQL schema: <why>]`, and a
`proof: the SQL schema leaves ...` line counts them. Each z3 check has a fixed budget of z3
resource units, not time, so the verdicts do not depend on the machine or its load. The
budgets were measured on z3 5.1.0; with another z3, or one whose `z3 --version` line cannot be
read, the run prints `note: z3 <version> is not a version the proof's resource limits were
measured on (5.1.0); a check may give up, or settle, where it would not on those` on stderr,
and nothing else changes. An
invariant cone stopped at the memory limit is `ok` when z3 proves every rule in it (see
[Memory limit](#memory-limit)). See
[Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows).

---

## `list`

Print model elements to stdout.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format table|json|csv|markdown]
```

| Flag | Default | Meaning |
|---|---|---|
| `--kind <KIND>` | `actor` | The kind to list; see below. |
| `--format` | `table` | `table`, `json`, `csv`, or `markdown` (a Markdown table with the CSV columns). `--kind surface` prints its canonical text only; another `--format` exits 2. |

An empty `table` result says so (`No APIs found.`), except for `surface`. CSV prints the header
only, Markdown the header and separator, JSON `[]`. **Exit codes:** `0`; `1` on a load error;
`2` on a usage error.

| Kind | Columns or content |
|---|---|
| `actor` | `id,label,description` |
| `buc` | `id,label,business,usecases,description` (use cases in declaration order) |
| `usecase` | Preconditions, guards, postconditions, alternatives, errors, then `description,bucs,screens,apis` |
| `goal` | `id,label,metric,baseline,target,window,description,requirements` (achieving requirements, `\|`-joined) |
| `requirement` | Requirement metadata, ending `rationale,goals,bucs,description` (`\|`-joined) |
| `adr` | Decision metadata and the targets of `decides` |
| `adr-impact` | One row per pair of an ADR and a target |
| `nfr`, `constraint` | Non-functional metadata |
| `quality`, `flow`, `step`, `system`, `variation`, `screen`, `event`, `timer` | Id and label |
| `field` | Field metadata and `Entity.column` mappings |
| `api` | API contract metadata: `api_id,api_label,method,path,idempotency,mode,auth_scheme` |
| `dto` | DTO fields |
| `entity` | Each column with type, PK / unique / index / FK flags, FK optionality and actions, checks, soft-delete / history / tenant markers, derived expressions |
| `condition` | The `holds` body, the variations and state models it combines (from the body, or `combines` when there is none), and the `applies_to` targets |
| `rule` | Every rule and condition: `kind,label,entities,rule,applies_to,site`. `kind` is `forbidden`, `invariant`, `required`, `exclusive`, `quantifier`, `after`, or `condition`; `label` is the `.label` (empty when none); `applies_to` is the use case an `after` follows or what a condition guards; `site` is `file:line` |
| `permission-callables` | Each permission with the use cases and APIs that require it, and `UseCase->Api` paths |
| `actor-permission-audit` | Inferred actor-side assignment gaps |
| `business-inputs` (alias `actor-inputs`) | Inferred actor-entered fields |
| `buc-roles` | See below |
| `transition` | See below |
| `surface` | See [Surface text](#surface-text) |

**`--kind buc-roles`** is a matrix: rows are the actors, then the external systems, each by
id; columns are `role` (the label), `kind` (`actor` or `extsystem`), and one per BUC, headed by
its label, ordered business by business (each BUC under its first business; BUCs of no
business last). A cell is `担当` for an actor that `performs` the BUC, a use case in it, or a
step in it; `イベント送信` for an external system that raises an event starting the BUC or one
of its use cases; `イベント受信` for one an event raised by its use cases reaches; both joined
with `、`; empty otherwise.

**`--kind transition`** is the table behind `diagram --kind state`, one row per drawn
transition: `entity,column,from,to,usecase,guards,actors,site`, ordered by state model, then the
Enum order of `from` and `to`, then use case id. `from` and `to` are `label (value)` (`予約済
(scheduled)`); `usecase` is `label (id)`; `guards` are the labels of conditions whose body
guards the use case; `actors` are the labels of its direct performers plus those of each
containing BUC (an event- or timer-started use case: direct `performs` only); `site` is the
`transitions` line. Several values are `|`-joined.

### Surface text

`--kind surface` prints one line per published element (`exposes` and `shares`; see
[Publishing to other models](./language-reference.md#publishing-to-other-models)), by kind in
the order `usecase`, `event`, `entity`, `writer`, `shared`, then by id. A model that publishes
nothing prints nothing. The format is frozen: a consumer's lock records this text.

```text
usecase Login "Log in" caller(actor) requires(CanLogin) raises(EvLoggedIn)
entity SessionView "Session (published)" upsert { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }
writer SessionView Login: status == active
writer SessionView (unexposed): status == ended
```

- `usecase <Id> "<label>" caller(<kind>)`, then `requires(<permissions>)` and `raises(<events>)`
  when present, each sorted.
- `event <Id> "<label>"`.
- `entity <Id> "<label>" <append | upsert> { <columns> }`: each column `name: Type` with `@pk`,
  `@unique`, `@null`, two spaces between columns.
- `writer <Entity> <UseCase>: <effects>` per exposed use case that writes the entity, use cases
  by id. A column `sets` is `col == val`; a proposition is `<prop> == true|false`; effects are
  sorted; a writer with no `sets` has no colon. The unexposed writers merge into one last line,
  `writer <Entity> (unexposed)`, so internal names never leak.
- `shared <Id> "<label>" { <columns> }` per `shares`.
- A `<label>` is quoted, with `\\`, `\"`, `\n`, and `\r` escapes. Ids, column names, types, and
  values never need escaping.

---

## `states`

Derive each entity's reachable state patterns, across BUCs. See
[state-derivation.md](./state-derivation.md).

```
rdra-ish states <INPUTS...> [--format table|csv|json]
                [--max-patterns <N>] [--max-states <N>] [--memory <SIZE>]
                [--instance-count <N>] [--instances <ENTITY=N>]... [--int-range-max <N>]
                [--buc <ID>]... [--entity <ID>]... [--without-usecase <ID>]...
                [--without-buc <ID>]... [--actor <ID>]...
```

| Flag | Default | Meaning |
|---|---|---|
| `--format` | `table` | `table`, `csv`, or `json` ([Output formats](./state-derivation.md#output-formats)). `typescript` / `type-script` were removed: exit 2, naming [`codegen --lang ts`](#codegen). |
| `--max-patterns <N>` | as `check` | As [`check`](#check). Past it, the result is `truncated` and `PatternCapReached` is an error. Only entities the target reads are bounded. |
| `--max-states <N>` | none | As `check`. Past it, `truncated` and `StateCapReached`, which names what the states vary in ([example](./state-derivation.md#exploration-by-cone-of-influence)). |
| `--memory <SIZE>` | `memory` under `[check]`, else `4GiB` | As `check`. |
| `--instance-count <N>` | `1` | As `check`. |
| `--instances <ENTITY=N>` | `instances` under `[check]` | As `check`. |
| `--int-range-max <N>` | `5` | As `check`. |
| `--buc`, `--entity`, `--without-usecase`, `--without-buc`, `--actor` `<ID>` | whole model | See [Partial runs](#partial-runs). Under `--entity`, every format holds only those views. |

The views go to stdout. In the table each entity opens with an `axes:` line, the domain of each
axis, and a `labels:` line when an inline `Enum` has labelled values
(`status[pending=受付前|paid=支払済]`). With `--format json`, each pattern carries a `witness`:
a shortest path to its first state, as `start` and `steps` with sites. stderr carries the
`bounds:` line, diagnostics that name no entity, and the `not checked:` block.

**Exit codes.** By structured severity, in every format: `1` when an error finding is present
(the report still prints), else `0`.

---

## `codegen`

Print the model's domain types and functions as TypeScript, Rust, or Go, for an agent to
finish.

```
rdra-ish codegen <INPUTS...> --lang ts|rust|go [-o <FILE> | --check <FILE>] [--package <NAME>]
                 [--allow-failing] [--conformance <FILE> [--conformance-states <N>] [--adapter]]
                 [--seal] [--verus <DIR>]
rdra-ish codegen <INPUTS...> --format json [-o <FILE>] [--allow-failing]
rdra-ish codegen <INPUTS...> --code
rdra-ish codegen <INPUTS...> --probe
```

Most of `codegen` is experimental. [compatibility.md](./compatibility.md#the-typescript-types-of-codegen)
covers one part: the TypeScript types `--lang ts` prints for a model `check` passes (`Result`,
`InvalidValue`, `InvalidRow`, the scalars, the id types, the value sets, the state types with
`<Entity>Kind`, the `<Entity>Row` types, and the signatures of their companion objects and of
`parse<Entity>Row`). Everything else may change in any release: the functions, rule checks,
dispatchers, header and obligations, `--allow-failing`, `--check` and the fingerprint, `--seal`,
and the Rust and Go output. [The design](./design/algebraic-codegen.md) describes where it is
going, and [Generated code style](./design/algebraic-codegen.md#generated-code-style) the shapes.

| Flag | Default | Meaning |
|---|---|---|
| `--lang` | none | `ts` (alias `typescript`), `rust`, or `go`. Required, except with `--format json`, `--code`, or `--probe`. |
| `--format` | `code` | `json` prints the contract as data; see [The JSON contract](#the-json-contract). Not with `--conformance` or `--check`. |
| `--code` | off | Write every file the [`[code.<lang>]` tables](#the-code-tables) name (domain code, traces, probe), paths and sites relative to `rdra.toml`. Not with other flags. |
| `--probe` | off | Write only the probe of each table that names one. Not with other flags. |
| `-o`, `--out <FILE>` | stdout | Write to this file. |
| `--package <NAME>` | `domain` | Go only: the package name. Put the file in its own directory and finish it in another package, so only code outside it is kept from building a state. |
| `--allow-failing` | off | Generate when `check` fails. The state types then narrow no column, there are no functions or rule checks, and each error is an obligation `NotMet_<error>`. |
| `--conformance <FILE>` | none | On a model `check` passes, also print the conformance port and harness, and write the traces to `FILE`. |
| `--conformance-states <N>` | `65536` | The most states the BFS behind the traces explores. After writing the traces, `codegen` prints their coverage statement on stderr (as [`check --code=run`](#what-a-run-covers) does), and `warning: <traces>: the conformance BFS stopped at its cap ...` when it stopped there. |
| `--seal` | off | `--lang ts` only (else exit 2): each state type gets a property keyed by an unexported `unique symbol`, so code outside the module cannot write a state as an object literal (TS2741). A type assertion still gets around it. |
| `--check <FILE>` | none | Generate again and compare with `FILE` instead of printing. Not with `-o`. With `--conformance <TRACES>`, also compares the traces by hash. |
| `--adapter` | off | Experimental, with `--conformance` only (else exit 2). Print the [model adapter](#the-model-adapter) in place of the domain code: plain data of the model's state and calls, `ModelAdapter`, and its harness. The traces are the same. Not with `--code`, `--probe`, `--seal` or `--verus`. |
| `--verus <DIR>` | none | Experimental, `--lang rust` only (else exit 2). Also write to `DIR` a Verus crate in which a modified core is checked against the verified model for any number of rows: check it with `verus DIR/main.rs`. The ordinary output does not change. Covers `Enum`, `Bool` and key columns, conditions with quantifiers and counts, creates and deletes; on another model it writes the ordinary output, prints one line per reason, and exits 1. Not with `--code`, `--probe`, `--check` or `--conformance`. See [the design](./design/verus.md). |

**Exit codes.** `0` when written or matched; `1` when `check` fails (without
`--allow-failing`), on model errors, on a name collision (two declarations that generate one
type name, such as an entity `OrderStatus` beside the inline `Enum` of `Order.status`), or when
`--check` finds a difference; `2` on a usage error.

`codegen` runs `check` first, at its bounds, and prints them in the header. Everything below
rests on the rules `check` verifies.

**Header and obligations.** The output is one compilation unit: a TypeScript module, a Rust
module file, or a Go file (`// Code generated by rdra-ish codegen. DO NOT EDIT.`, as `gofmt`
prints it). It opens with the **obligations**: what `check` and `verify` assume of the data and
no type enforces, each named and with its site. The name is the invariant's where `check`
checks one (`Unique_*`, `MinCard_*`, `MaxCard_*`, `Check_*`, marked `[checked]`), else
`Ref_<Entity>_<columns>`, `Unique_<Entity>_<columns>`, `OnDelete_*`, `OnUpdate_*`,
`Derived_<Entity>_<column>`, `OneTransaction`, `RunMarks_<UseCase>`, `CompleteRows_<Type>`,
`RuleChecks`, or `NotMet_<error>`. A name does not move when the model gains an unrelated
declaration, so the [obligation ledger](#check---code) can name it. The header also lists the
use cases with no function and why, and **candidate rules**: a nullable column that every
reachable row of a state holds present (or null) with no rule saying so, printed as the
`invariant` or `forbidden` that would narrow the type (a column null everywhere is listed once,
as one no use case writes).

**Types.** The scalars a project may replace (`Money`, `Decimal`, `Date`, `DateTime`), an id
type per entity with a one-column primary key, each variation and inline `Enum` as a value
set, and each entity and DTO as a record. A key column takes its parent's id type; a `@null`
column is `T | null`, `Option<T>`, or `*T`. Ids and scalars are opaque and made only by parsing:
`OrderId.parse(1)` / `OrderId::from(7)` / `ParseOrderId`, `"12.50".parse::<Money>()`,
`ParseMoney("12.50")`; text of another shape is `InvalidValue`.
TypeScript exports each scalar as a type and a companion object of the same name, so a
module that imports `Date` by name hides the global `Date` (as any model name that is also a
global's would): import it renamed (`import { Date as IsoDate } from "./domain.ts"`) or
import the module as a namespace (`import * as d`).

**State types.** An entity whose tag column is an inline `Enum` that `transitions` drive is a
state type: one variant per value of the tag (among the columns `transitions` names: `status`,
else a `*_status` column, else the first by name). It is a TypeScript union of `Readonly`
types told apart by `kind` (`"Pending"`, one value of `<Entity>Kind` per state), a Rust enum of
structs with `kind()`, or a Go interface per state implemented by one unexported struct, with
`Kind()` returning `<Entity>Kind` (`OrderKindPaid`). No variant holds the tag column; a column
named `kind` is `kind_` (TypeScript, Rust) or `Kind_()` (Go). A variant narrows a nullable
column only where a rule says so: `invariant(E).when(status == v).then(x == present)` makes `x`
present in `v`, `forbidden(E, status == v, x == present)` null there, `required(E, x ==
present)` present everywhere; a rule with another atom or a `.when(Cond)` narrows nothing.
Fields are private in Rust and unexported in Go, read by methods (`paid_at()`, `PaidAt()`).
Only a transition function and `from_row` make a state. In Go a state variable that holds
nothing is `nil`; a function refuses a `nil` state with `InvalidArgument`, and each state type
has a visitor (`OrderVisitor[R]`, `MatchOrder(state, v) (R, error)`), so a visitor that lacks a
new state's method no longer compiles. An entity whose `Enum` only `sets` writes is a plain
record.

**Row types.** Each state type has `<Entity>Row`, every column as the data layer declares it
(Go: with `json` tags). `Order.fromRow` / `Order::from_row` / `OrderFromRow` returns the state
or `InvalidRow` naming the column; `Order.toRow` / `into_row` / `OrderToRow` the row. A state
column marked `@null` keeps its declared values as states: its row column is nullable, and a
row whose state column is null is an `InvalidRow`. TypeScript's `parseOrderRow(value:
unknown)` reads a row from outside, each column through its type's parser.

The two serialize differently: a state names its variant in `kind` (`"Paid"`, the state's
name) and has no tag column, a row holds the tag column with the model's value (`status:
"paid"`). A result holds states, a store holds rows. An API that answers a write with the
result and a read with the stored row sends two shapes for one entity unless it converts one:
pass each state of the result through `Order.toRow` (it takes any state of the entity) to
answer both with rows, or each row through `Order.fromRow` to answer both with states.

**Transition functions.** Each use case gets one function per combination of its rows'
source states (`cancelFromPending`, `cancelFromPaid`), so a call from another state does not
compile. It takes the rows it reads, then an input per value the model leaves open (a column
`sets` makes `present`, a chosen variation value, a created row's column no var holds). A
logical `@derived` column of a written row is computed from the new values; one it cannot
compute (arithmetic, two columns, `now`, a nullable operand, an input) is an input, and the
doc says why. It returns `<Name>Result` with the rows written or created, the id of each
deleted row, the raised events, and per written row the source it must still have at commit
(`order_expected`: `Absent`, `In { column, value }`, or `Present`); the obligations ask for one
transaction per call and a guarded write (`UPDATE .. WHERE id = ? AND status = ?`), a write that
matches no row rolling back as `NotEnabled`. The last step of a split use case also returns
the run marks it ends (`ended_<row>_mark`). Every result and outcome lists what its commit
writes (`into_writes`, `Writes()`, `<Type>.writes`), and `commit_writes` / `CommitWrites` /
`commitWrites` writes that list through a `CommitStore` the service implements once
([design](design/algebraic-codegen.md#committing-a-result)).

- **Errors.** A condition applied to the use case is checked first; when it fails the
  function returns `Rejected` (TypeScript `{ kind: "Rejected", condition, label, site }`). A row
  that breaks what the model keeps, or a parent that is not the child's, is
  `InvalidArgument`. Rust returns `Result<_, <UseCase>Error>` (`NotEnabled`, `Rejected`,
  `InvalidArgument`, each `std::error::Error`); TypeScript `Result<<Name>Result,
  <UseCase>Error>`; Go `(<Result>, error)` with a `*<UseCase>Error` whose `Kind` is
  `<UseCase>ErrorNotEnabled`, `...Rejected`, or `...InvalidArgument`, reachable by `errors.As`
  and `errors.Is` (`ErrNotEnabled`, `ErrRejected`, `ErrInvalidArgument`). Nothing panics or
  throws but TypeScript's `assertNever`.
- **Split use cases** get one function per step (`openSettlement1`, `openSettlement2`), linked
  by **run marks**: a step that leaves a row for the next returns a mark
  (`OpenSettlementClaimMark1 { id }`) the next takes. The obligations say how to store marks:
  one per row, stored in the step's transaction, step 1 only on a row with none, and a run
  never abandoned.
- **Events** carry the id of each row the raise marks (`EvOrderExecuted {
  certificate_order_id }`). A triggered use case takes the event after any marks and checks
  its row is the event's.
- **Choices.** Arms from one state to several are one function per target
  (`decideFilingToDecidedFile`), guarded by the arm's `.when`; the dispatcher takes the chosen
  state. A second lifecycle column is a branch on its value; a row in no `from` state is
  `Rejected` by `TransitionFrom`. A choice among its arms is one function per target too
  (`determineFaultFromInvestigatingToAtFault`), and the dispatcher takes the target
  (`claimCaseLiabilityTo`). A `.has(..)` / `.none(..)` condition takes a list of related
  rows (`terminalCertAssignmentRows`), every present row of that entity; the function keeps
  those the key links.
- **Links and copies.** `links` / `relinks` / `unlinks` is the condition
  `Links_<Child>_<role>` (`Relinks_..`, `Unlinks_..`) on the child's key, which the function
  then sets to the parent's id or to null; a created row's optional key nobody links is null.
  `sets(UC, E.column == F.source)` copies the source row's value. A condition or `.when` on
  what the step writes reads the value it writes: the parent a created row's key names, a
  literal, or the input a chosen value becomes (`caseScope === "free"`). A nullable Enum or
  Bool column compared with a value takes a present value (`== Some(..)`, `!= nil && *x ==
  ..`), so null satisfies no `==` and every `!=`.
- **Not printed yet:** a use case that reads `now`, guards on a comparison proposition, joins
  by a key over several columns, or raises or handles a counted event. It gets
  no function; the header lists it with the reason, and the code that finishes the output
  writes it.

**Rule checks.** Each record and state type gets `orderPaidRuleViolations(row)` /
`rule_violations(&self)` / `RuleViolations()`: the names `check` reports (`Forbidden_Order_0`,
`Check_Order_qty_0`) of the entity-local rules the row breaks. A rule the state type
guarantees is left out; a rule over several rows is an obligation instead.

**Dispatchers.** Each use case whose functions take a state type gets `runCancel` /
`run_cancel` / `RunCancel`: it calls the function of the rows' states and returns its result
as a `CancelOutcome` case, or `NotEnabled`.

**Contracts and callers.** Each use case with a function or entry point gets a contract:
`<UseCase>UseCase` with `prepare_*` / `commit_*` (and `run_<action>` for an action with no
function). It is a Rust trait, a TypeScript type of `readonly` members returning `Promise`, a Go
interface (`Prepare*`, `Commit*`, `Run<Action>`). A use case actors perform takes a caller
first, `by: <UseCase>Caller`, over the capabilities of those actors: a Rust enum of
`EndUserCapability`-style types made only by `issue()`; a TypeScript union of capability types
with an unexported `unique symbol` (an object literal is refused, TS2741; `as unknown as`
still forges one); a Go interface with an unexported method only those capabilities implement,
made valid only by `Issue<Actor>Capability()`. Go cannot forbid the zero value or a nil
caller; `Valid()` and `CallerValid(by)` report them. Whether the code that calls `issue`
authenticated the actor is not checked.

**Conformance harness** (`--conformance <FILE>`). The service implements `ConformancePort`:
`seed` empties its store and stores an initial state; `snapshot` returns every row (the
`<Entity>Row` of a state type, else the record), every pending run mark (`<UseCase><Entity>Run
{ id, step }`), and every event a triggered use case reads; `prepare_pay` / `PreparePay` loads
the rows and returns the dispatcher's result, writing nothing, and `commit_pay` / `CommitPay`
stores it in one transaction; either returns the use case's error when it does not run. The
test calls `conform(|| MyService::new(), &traces)` (Rust), `Conform(func() ConformancePort {
return newService() }, traces)` (Go), or `await conform(() => newService(), traces)`
(TypeScript, whose port members return `Promise`s; it returns `Result<ConformanceReport,
string>`, and `traces` is the file's text or a function returning its lines as an
`AsyncIterable<string>`, which the probe uses so a file past Node's longest string, about
512 MB, still runs). Per trace, the harness starts a fresh port, seeds it, makes each call, and compares
the store, cut down to what the model tracks (an enum's value, null or not, an integer, a key's
parent), with the state BFS reached. A refused call must fail and change nothing, and so must
a commit after another call changed a row it writes. It returns the first difference, with
the traces line and the rows only one side holds. It checks what is stored, not how. In Go,
`Seed` returns an `error` and `Snapshot` a `(StoreSnapshot, error)`: a store that fails ends
the run with `the service's Seed failed: <error>` (or `Snapshot`) on the traces line, instead
of a panic.

A seeded state is the model's state as `check` reached it, row by row, and it can hold a child
whose parent row does not exist: an entity no use case creates starts present while its parent
starts absent (`check` warns `PreExistingChildOfCreatedParent_<Child>_<Parent>`, and clinic-ops
seeds an `AccountBalance` naming `PatientAccount` 1 before any `PatientAccount` exists). The
harness seeds such a row as it is, and `Snapshot` must return it. A store that enforces the
`Ref_<Child>_<key>` obligation as a foreign key at once refuses that seed; turn the check off
while `Seed` runs (SQLite: `PRAGMA foreign_keys = OFF` around the inserts; a deferred check
still fails at the commit), or give the model a use case that creates the child after its parent so the warning goes away.
The harness does not invent the missing parent, because the model's state has no such row.

**Traces.** A text file from a BFS of the whole system at `check`'s bounds, taking every move
of a use case with a function in as few traces as a greedy walk finds, and in each state
trying every call the model refuses, with the kind it must fail with (`X <kind> <call>`, `-`
for any kind; see [What a run covers](#what-a-run-covers)). A BFS that stops at
`--conformance-states` covers only what it reached and tries no refusal. The header says how
many moves are covered and what the harness does not compare (a counted trigger, a comparison
proposition, the clock). The file opens `# rdra-ish conformance traces 4`, then a fingerprint
line.

**Fingerprint.** The output's second line is `// rdra-ish codegen fingerprint: model=<hash>
code=<hash>` (`#` in a traces file). The model hash covers the loaded files' source text; the
code hash, the output without this line.

**`--check <FILE>`** generates with the same inputs and options and compares, ignoring the
fingerprint line. Run it from the same directory with the same input paths: the header names
them. It writes nothing to stdout. A match prints `ok: <FILE> is what the model generates` on
stderr and exits 0; a model edit that changes no code (a comment) passes. A difference exits 1
and prints the causes, the count of differing lines, and the first difference:

| Cause | Meaning |
|---|---|
| `file edited` | The file no longer matches its own code hash. |
| `model changed` | The model's source text differs from the text the fingerprint records. |
| `generator changed` | Neither changed, so `codegen`'s output did: other settings (`--lang`, `--package`, `--allow-failing`, `--conformance-states`, or other input paths; under `check --code`, the table's settings in `rdra.toml`), or another version. |
| no fingerprint line | The file was not generated by `codegen`, or the line was removed. |

Restore or regenerate the file; code that finishes the output belongs in other files. With
`--conformance`, the traces are compared by their hash, reading the file once without holding
it.

### The JSON contract

`codegen --format json` prints what the code is generated from as one JSON object, for a tool
that generates another language or checks code against the model. Experimental. `--lang` does
not change it.

| Key | What it holds |
|---|---|
| `format` | `"rdra-ish codegen json 1"`; a reader refuses another. |
| `generator`, `model`, `check` | The rdra-ish version, the inputs, and whether `check` passed (`passed`, and `bounds` or `errors`). |
| `types` | Value sets (`kind: "enum"`, with `variation` and `site` for a variation), id types (`"id"`, with `entity` and `repr`), records (`"entity"` or `"dto"`, with `id`, `site`, `fields`). A field has `name` (the column as the model and the row spell it), `domain_name` (the field's name in the generated state and record types, from which each language derives its own spelling; today the same as `name`), `ident` (PascalCase), `type` (`{"kind": "scalar" \| "enum" \| "id" \| "rows", "name"}`), and `nullable`. A field of an entity record also has `primary_key` and `unique` (`@unique` on the column; a key spanning several columns is a `unique` obligation). |
| `states` | Each state type: `name`, `entity`, `site`, `row`, `tag`, and `variants` with their `fields` and `always_null` columns. |
| `events` | Each event type a use case raises: `name`, `doc`, and `fields`, the id of each row the raise marks (`name`, `entity`, `type`). |
| `usecases` | Every use case in model order: `id`, `label`, `site`, `kind` (`functions`, `unprinted`, or `read-only`), `actors` (the ids of the actors that perform it, whose capabilities its `Caller` takes; empty for one an event or a timer starts), and `actions`, each with its `functions`, `dispatcher`, and why it is `unprinted`. |
| `functions` | Each function: `name`, `usecase`, `action`, `step`, `rows`, `marks`, `event`, `inputs`, `conditions`, `result`, `writes`, `raises`. |
| `obligations` | Each obligation: `name`, `checked`, `text`, `site`, and what it is about: `kind` (`reference`, `unique`, `min-children`, `max-children`, `on-delete`, `on-update`, `check`, `derived`, `invariant`, `rule-checks`, `transaction`, `run-marks`, `complete-rows`, or `not-met`), `entity` (or `null`), `columns` (the key, the unique set, or the column a check or a derivation is on; `[]` when none), and `parent` (the entity a key names, or `null`). |

Keys are only added under one `format`: a reader of format 1 ignores a key it does not know.

Sites are `path:line:col` relative to the working directory, or to `rdra.toml`'s directory under
`--code`.

---

## Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `RDRA_JOBS` | `check`, `states`, `verify`, the generators | A whole number above 0: the cones explored at once, the threads that compute one cone's moves, and the z3 processes of `--prove`. Default: cones and z3 processes are the cores, at most 8; one cone's threads are a third of the cores, at least 4 and at most 12, never more than the cores. `1` runs everything one after another on one thread. Lower uses less memory; the output does not change. TLC runs are not counted. |
| `RDRA_TLC_JAR` | `verify --with-tlc` | Path to `tla2tools.jar`, used as `--tlc-jar` when the flag is absent. |
| `RDRA_TLC_JOBS` | `verify --with-tlc` | A whole number above 0, used as `--tlc-jobs` when the flag is absent. |
| `RDRA_TLC_WORKERS` | `verify --with-tlc` | A whole number above 0, used as `--tlc-workers` when the flag is absent. |
| `RDRA_TLC_HEAP` | `verify --with-tlc` | The heap of every TLC JVM, such as `4g`, with no rerun. Runs queue so their heaps fit; see [Memory limit](#memory-limit). `states` instead raises a cone run's heap to 640 bytes per state BFS counted in its module, in whole GiB, within TLC's share (experimental; the heap then depends on whether BFS counted the cone before its run started). |
| `JAVA_TOOL_OPTIONS` | `verify --with-tlc` | Appended after the options `verify` sets (`-Xmx`, `-Djava.io.tmpdir`, and `-XX:TieredStopAtLevel=1` on a state-model run of at most 10,000 BFS states, added only after TLC started once with it), so a value here wins. |
| `RDRA_TLC_BATCH` | `verify --with-tlc` | `0` runs each TLC job as its own process instead of in the `tlc-batch` JVM the flake provides. |
| `RDRA_Z3` | `verify --prove` | The z3 binary, used as `--z3` when the flag is absent. |
| `XDG_CACHE_HOME` | `verify --prove` | Where the proof keeps the set it kept, for the next run to re-check first: `$XDG_CACHE_HOME/rdra-ish/proofs`, else `~/Library/Caches/rdra-ish/proofs` on macOS and `~/.cache/rdra-ish/proofs` elsewhere. See [Proofs](#proofs). |
| `RDRA_ISH_NO_PROOF_CACHE` | `verify --prove` | Set: read and write no certificate; every proof runs from the start. The output does not change. |
| `RDRA_BIN` | `scripts/data-diff.sh` | The binary to run (default `target/release/rdra-ish`). |

For developers. These change only diagnostics or turn an optimisation off; the verdicts stay:

| Variable | Effect |
|---|---|
| `RDRA_ISH_SYMMETRY_STATS` | Per run up to symmetry, a stderr line of orbits, estimate, branch-cap cuts, and entities left out; see [Instance symmetry](#instance-symmetry). |
| `RDRA_ISH_WF_REDUCED_STATS` | A stderr line per `WF_vars(Next)` run on a reduced cone. |
| `RDRA_ISH_TEST_QUOTIENT_HITS` | Test only: a `quotient-hits:` stderr line of how many cones took a shared quotient. |
| `RDRA_ISH_NO_QUOTIENT_CACHE` | Do not share quotients between cones. |
| `RDRA_ISH_NO_REDUCE` | Do not reduce cones to their quotient. |
| `RDRA_ISH_NO_WF_REDUCED` | Do not run `WF_vars(Next)` on a reduced cone first. |
| `RDRA_ISH_FORCE_ORBITS` | Test only: under `--symmetry always`, run every eligible job up to symmetry, sharing nothing. |

A `shadow` build also reads `RDRA_ISH_SHADOW_STATS`, `RDRA_ISH_DROP_RULE`,
`RDRA_ISH_FORCE_S_UP_TO_H`, and `RDRA_ISH_FORCE_PROBES`, and prints a
`shadow: compared/skipped/stopped` line at exit whenever a reference twin let a call through
unchecked, with or without `RDRA_ISH_SHADOW_STATS`; a `perf` build `RDRA_PERF` (`1` the pass and run lines, `2` the
costly columns too), `RDRA_PERF_EVAL_TWICE`, and the measurement switches the `perf` module
lists (`crates/rdra-ish-core/src/perf.rs`). The test suites read `RDRA_REQUIRE_*` and other
variables of their own.

---

## `lsp`

Run the language server over stdin and stdout.

```
rdra-ish lsp
```

`rdra-ish lsp` is the server the `rdra-ish-lsp` binary runs, built into the CLI, so a PyPI
install (`uv tool install rdra-ish`) has it too. It takes no arguments and reads no file
until a client opens a workspace. Stdout carries only protocol messages; nothing else is
written to it. It exits after the client's `exit` notification. The
[Claude Code plugin](../README.md#claude-code-plugin) starts it for `.rdra` files.
[Editor support](#editor-support) says what the server does.

---

## Editor support

`rdra-ish-lsp` is the language server. It offers diagnostics, go to definition, references,
rename, completion, hover, signature help, document and workspace symbols, formatting (as
`fmt`), semantic tokens, code actions, code lenses, inlay hints, linked editing, and folding.

- It loads a workspace as the CLI does: every `.rdra` file under the workspace roots plus the
  open buffers, with include paths derived from those files. An import resolves by the
  `module` a loaded file declares before its path, so an import no loaded file declares is
  "unknown module" in the editor, as in `check`.
- It publishes results under the URIs the editor opened the files by.
- A malformed `rdra.toml` clears the diagnostics, shows the error in a message, and adds a
  diagnostic on an open `.rdra` file; the server keeps running.
- It runs no `codegen` and no build. When a file under `[code]` is missing, has no
  fingerprint, or was generated from other source text, it shows an information diagnostic on
  the table's line of `rdra.toml` (a comment-only edit shows it too; `check --code` decides).
- A key's role is a symbol: see [Which key a join follows](./language-reference.md#which-key-a-join-follows).
- After each analysis of a model that loads, it runs the lifecycle exploration of `check` in
  the background, at one instance per entity, one cone at a time on one thread, within a
  time budget and a memory limit. Each finding is a diagnostic at the declaration it is
  about: a violated rule or a `Vacuous_` warning at the rule, a use case that never runs at
  the use case, and a deadlock or an unreachable state at the entity. Its source is
  `rdra-ish check (1 instance)` and its code is the name `check` prints. A cone that stopped
  at the budget or the limit, and each finding it left undecided, is information. Each
  invariant, `forbidden`, and `after(..).assert` gets a code lens: `check: ok (1 instance)`,
  `check: violated (1 instance)`, or `check: not decided (1 instance)`. `verify` explores two
  instances, so it may decide otherwise. A new edit drops a check that has not finished
  publishing, and the next analysis withdraws the last check's findings until its own check
  publishes.
- `lifecycleCheck` in the initialization options, or under the `rdra-ish` section of the
  settings, sets the check: `enabled` (default `true`), `timeBudgetMs` (`2000`), `memory`
  (`"512MiB"`), and `quietMs` (`200`, how long after an analysis it starts). The VS Code
  extension passes its `rdra-ish.lifecycleCheck.*` settings. The design and its open
  questions are in [lsp-lifecycle-check.md](./design/lsp-lifecycle-check.md).

---

## See also

- [language-reference.md](./language-reference.md): what the DSL means.
- [incremental-modeling.md](./incremental-modeling.md): the staged modeling flow `next` follows.
- [state-derivation.md](./state-derivation.md): how `check` and `states` explore, and every finding name.
- [formal-verification.md](./formal-verification.md): `verify` and `export --kind tla`.
- [compatibility.md](./compatibility.md): which commands, flags, exit codes, and formats are stable.
