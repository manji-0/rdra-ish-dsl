# Code Generation

<!-- constrained-by ./cli-reference.md#codegen -->
<!-- constrained-by ./cli-reference.md#check---code -->

This guide is for developers who build a service from a model. It says what
`rdra-ish codegen` writes, what `check --code` checks against it, and what a passing check
does and does not show. Every flag and every finding name is in
[cli-reference.md](./cli-reference.md#codegen); this page links there and does not repeat
the tables.

Most of the output is experimental. [compatibility.md](./compatibility.md#the-typescript-types-of-codegen)
covers one part: the TypeScript types `--lang ts` prints for a model `check` passes. The
functions, the contracts, the harness, the Rust and Go output, the model adapter, the JSON
contract and the Verus crate may change in any release.

## The commands

| Command | What it does | Reference |
|---|---|---|
| `codegen <INPUTS> --lang ts\|rust\|go` | Prints one file of domain code: types, transition functions, dispatchers, contracts. `-o` writes it to a file. | [`codegen`](./cli-reference.md#codegen) |
| `codegen ... --conformance <FILE>` | Also prints the conformance port and harness into the code, and writes the traces to `FILE`. `--conformance-states` sets the BFS cap of the traces. | [`codegen`](./cli-reference.md#codegen) |
| `codegen ... --adapter` | With `--conformance`: prints the model adapter in place of the domain code. | [The model adapter](./cli-reference.md#the-model-adapter) |
| `codegen ... --lang rust --verus <DIR>` | Also writes a Verus crate to `DIR`. | [`codegen`](./cli-reference.md#codegen) |
| `codegen <INPUTS> --format json` | Prints the contract the code is generated from, as data. | [The JSON contract](./cli-reference.md#the-json-contract) |
| `codegen <INPUTS> --code` | Writes every file the `[code.<lang>]` tables of `rdra.toml` name: domain code (or adapter), traces, probe. `--probe` writes only the probes. | [The code tables](./cli-reference.md#the-code-tables) |
| `codegen ... --check <FILE>` | Generates again and compares with `FILE`, by the fingerprint on its second line. | [`codegen`](./cli-reference.md#codegen) |
| `check <INPUTS> --code[=fresh\|build\|run]` | Checks the service code the `[code]` tables name: the files are fresh, the probe compiles, the conformance test passes. `--deny-incomplete`, `--mutants` and `--mutant-seed` go with `run`. | [`check --code`](./cli-reference.md#check---code) |

`codegen` runs `check` first. On a model `check` rejects it exits 1, unless
`--allow-failing` is given. Then the state types narrow no column, no function is printed,
and each error becomes an obligation `NotMet_<error>`.

## What is generated

### Files

`codegen` writes one compilation unit per language: a TypeScript module, a Rust module file,
or a Go file in its own package (`--package`, default `domain`). The code opens with a header:
the bounds `check` passed at, the obligations, the use cases that got no function and why,
and the candidate rules. Line 2 is the fingerprint
(`// rdra-ish codegen fingerprint: model=<hash> code=<hash>`).

With `--code`, the `[code.<lang>]` table decides the paths, relative to `rdra.toml`:

| Key | File |
|---|---|
| `domain` (or `adapter`) | The domain code (or the model adapter). |
| `traces` | The conformance traces, a text file. With it, `domain` also holds the port and the harness. |
| `probe` | One line per use case that states `service` implements its contract, and the conformance test. Rust: a file under the crate's `tests/`. TypeScript: a `.ts` file, plus `tsconfig.rdra.json` in `root`. Go: a `_test.go` in the service's package. |

For example, this table:

```toml
[code.rust]
domain = "../svc/src/domain.rs"
traces = "../svc/tests/traces.txt"
probe = "../svc/tests/rdra_contract.rs"
service = "svc::Service"
module = "svc::domain"
new = "svc::Service::default()"
```

makes `codegen model --code` write three files:

```
wrote svc/src/domain.rs
wrote svc/tests/traces.txt: 4 of 4 moves in 9 traces, 7 refused calls, 5 interleaved commits
wrote svc/tests/rdra_contract.rs
```

The generated files are never edited by hand. Code that finishes the output goes in other
files.

The examples below come from this model:

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  paid_at: DateTime @null
}

usecase Place "Place an order"
usecase Pay "Pay for an order"
usecase Cancel "Cancel an order"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.paid_at == present)
transitions(Cancel, Order.status, (pending, paid) -> cancelled)
invariant(Order).when(status == paid).then(paid_at == present)
forbidden(Order, status == pending, paid_at == present)
```

For it, `--lang ts` prints 650 lines, `--lang rust` 1,338 and `--lang go` 1,023. With
`--conformance`, the Rust file has 1,706 lines and the traces 57.

### Types

- **Scalars.** `Money`, `Decimal`, `Date` and `DateTime` are opaque. They are made only by
  parsing text of the right shape; other text is `InvalidValue`. A project may replace them.
- **Ids.** Each entity with a one-column primary key gets an id type (`OrderId`). A key
  column takes its parent's id type.
- **Value sets.** Each variation and inline `Enum` (`OrderStatus`).
- **Records.** Each entity without states, and each DTO.
- **Null.** A `@null` column is `T | null` (TypeScript), `Option<T>` (Rust) or `*T` (Go).

### State types

An entity whose tag column is an inline `Enum` that `transitions` drive gets one type per
value of the tag. A state type holds no tag column. It narrows a nullable column only where a
rule says so: here `paid_at` is `null` in `pending` and present in `paid`, by the two rules.

```ts
/** Order in the state `paid` of status. */
export type OrderPaid = Readonly<{
  kind: "Paid";
  /** Primary key. */
  id: OrderId;
  /** Present in this state by a rule (order.rdra:14:1). */
  paid_at: DateTime;
}>;
```

Rust has an enum of structs with private fields (`pub enum Order { Pending(OrderPending),
Paid(OrderPaid), Cancelled(OrderCancelled) }`). Go has one interface per state, implemented
by one unexported struct, and a visitor per entity:

```go
type OrderPaid interface {
	Order
	// Id returns id.
	Id() OrderId
	// PaidAt returns paid_at.
	PaidAt() DateTime
	isOrderPaid()
}
```

Each state type also has `<Entity>Row`, every column as stored. `fromRow` / `from_row` /
`OrderFromRow` turns a row into a state, or returns `InvalidRow` naming the column the state
rules out; `toRow` turns a state back into a row. Only a transition function and `from_row`
make a state.

### Transition functions, dispatchers and contracts

Each use case gets one function per combination of its rows' source states. A call from
another state does not compile. The function takes the rows it reads, then one input per
value the model leaves open. It returns the rows it writes and, per row, the source the row
must still have at commit:

```rust
pub fn pay(order: OrderPending, order_paid_at: DateTime) -> ::core::result::Result<PayResult, PayError> {
    ::core::result::Result::Ok(PayResult {
        order: OrderPaid {
            id: order.id,
            paid_at: order_paid_at,
        },
        order_expected: ExpectedSource::In { column: "status", value: "pending" },
    })
}
```

`Cancel` gets `cancel_from_pending` and `cancel_from_paid`. A dispatcher (`run_cancel`) takes
an `Order` in any state, calls the function of its state, or returns `NotEnabled`. Each
record and state type gets a rule check (`rule_violations`), which names the entity-local
rules a row breaks. Each result lists what its commit writes, and `commit_writes` writes that list
through a `CommitStore` the service implements.

The errors are values; no function throws or panics, except TypeScript's `assertNever`.
Every `<UseCase>Error` has `NotEnabled`, `NotFound` and `StoreFailed`, and `Rejected` and
`InvalidArgument` where the use case can give them.

Each use case also gets a contract, which the service implements: `prepare*` loads the rows
by id and runs the function, writing nothing; `commit*` stores the result in one transaction.
It is a Rust trait, a Go interface, or a TypeScript type whose members return `Promise`s:

```ts
export type PayUseCase = {
  /** Runs Pay (the action Pay). */
  readonly preparePay: (order: OrderId, orderPaidAt: DateTime) => Promise<Result<PayOutcome, PayError>>;
  readonly commitPay: (write: PayOutcome) => Promise<Result<void, PayError>>;
};
```

A use case that actors perform takes a caller first, made from the capabilities of those
actors. Split use cases get one function per step, linked by run marks. Events carry the id
of each row they mark. Choices, links, copies and `@derived` columns are in
[`codegen`](./cli-reference.md#codegen).

Some use cases get no function. The header lists each with the reason, and the contract
gives it an entry point (`run_<action>`) that the service writes whole. The reasons include:
a use case that reads `now`, guards on or drives a comparison proposition, joins by a key
over several columns, raises or handles a counted event, chooses among some values of a
column, or computes an arithmetic `@derived` column.

### Obligations

What `check` and `verify` assume of the data, and no type enforces, is listed in the header
as named obligations, each with its site: `OneTransaction`, `Ref_<Entity>_<columns>`,
`Unique_*`, `OnDelete_*`, `RunMarks_<UseCase>`, and the others
[`codegen`](./cli-reference.md#codegen) lists. For the model above the only one is
`OneTransaction`: each function runs in one transaction, and writes a row only where it still
is what `<row>_expected` names (`UPDATE .. WHERE id = ? AND status = ?`).

### The conformance port, harness and traces

With `--conformance` (or `traces` in the table), the code also holds `ConformancePort`, which
is every contract plus `seed` (empty the store, store a state) and `snapshot` (return every
row, run mark and read raise). It also holds the harness, `conform` / `Conform`.

The traces are a text file from a BFS of the whole system at `check`'s bounds. They take
every move of a use case that has a function, and in each state try every call the model
refuses, with the kind of error it must fail with:

```
S 2
A Order 1 status=paid paid_at=+
T 0
G
X NotFound Pay 101 2026-01-01T00:00:00Z
X NotFound Cancel 101
C 1 Place 1
C 2 Pay 1 2026-01-01T00:00:00Z
X NotEnabled Pay 1 2026-01-01T00:00:00Z
```

`S`/`A` define a state and its rows, `T` starts a trace, `G` ends its seed, `C` is a call
and the state it reaches, and `X` is a call that must be refused. Per trace, the harness
makes a fresh port, seeds it, makes each call, and compares the store with the state, cut
down to what the model tracks: an enum's value, null or present, an integer, a key's parent.

### The model adapter

A project that changed the domain code it took can bind its own code to the traces through
`codegen --adapter` (`adapter` in the table). The adapter file uses no type of the domain
code: `ModelState`, `Call`, and `ModelAdapter` (`reset`, `seed`, `observe`, `invoke`, and
optionally `prepare` and `commit`). It runs the same traces and reports differences in the
same words. See [The model adapter](./cli-reference.md#the-model-adapter).

### The Verus crate

`--lang rust --verus <DIR>` writes, beside the ordinary Rust output, a crate checked with
`verus DIR/main.rs`. The ordinary output does not change. The header of each file names the
Verus release and Rust toolchain it was checked with (`0.2026.11.22.1ce6a59`, Rust
`1.100.0`).

| File | Edited by | Holds |
|---|---|---|
| `main.rs` | rdra-ish | the module list and the harness's entry |
| `types.rs` | rdra-ish | the data types |
| `prelude.rs` | rdra-ish | rows as a map, and the lemmas for quantifiers and counts |
| `model.rs` | rdra-ish | the state, one step per action, the rules, the inductive set, the preservation lemmas |
| `contract.rs` | rdra-ish | what each function must do, as a trait |
| `core.rs` | the user | the functions, as the trait's methods |
| `port.rs` | rdra-ish | the trusted store, and one verified port call per action |
| `harness.rs` | rdra-ish | a differential test against BFS successors (not verified) |

When Verus reports no error, the model keeps its rules for any number of rows, the core
meets its contracts, and each port call moves the store as one step of the model or not at
all. What it trusts (the store, the printer, Verus itself) is in
[the Verus design note](./design/verus.md#the-trust-boundary).

The crate covers a model whose tracked columns are all `Enum`, `Bool` or keys, and whose use
cases are one step that creates, updates, transitions or deletes rows, guarded by conditions
on their rows and by quantifiers and counts over related rows. For any other model, `codegen`
writes the ordinary output, prints one line per reason, and exits 1. On the model above:

```
  Order.paid_at: nullable columns are not covered
  Order.paid_at: only Enum, Bool and key columns are covered
  Pay: the input order_paid_at is not covered (only ids of created rows and related rows are)
Error: --verus does not cover this model yet (3 reason(s) above); the ordinary output is unaffected
```

### Per language

| | TypeScript | Rust | Go |
|---|---|---|---|
| State type | union of `Readonly` types told apart by `kind` | enum of structs with private fields | interface per state, one unexported struct each, a visitor |
| Errors | `Result<T, E>` values | `Result<T, E>`, each error a `std::error::Error` | `(T, error)`, `*<UseCase>Error`, `errors.Is` / `errors.As` |
| Contracts | type of `readonly` members returning `Promise` | synchronous trait (`&mut self`) | interface |
| Sealing a state | `--seal` (`seal = true`): a literal fails with TS2741 | always: private fields | always: unexported struct |
| Serialization | `parse<Entity>Row` reads a row from `unknown` | `serde = true` derives serde on the data types | `json` tags on the rows |
| `check --code=build` | `tsc --noEmit -p tsconfig.rdra.json` | `cargo check --test <probe>` | `go test -count=1 -run ^$ -gcflags=-e` |
| `check --code=run` | `node <probe>` (Node 22.18 or later) | `cargo test --test <probe> -- rdra_conformance` | `go test -count=1 -run ^TestRdraConformance$` |

## What the output guarantees

### What it rests on

The state types and functions rest on the rules `check` verified. That `check` explores at
one instance per entity, `Int 0..5`, with no state cap and the memory limit of `[check]`
(`memory`, default `4GiB`; `codegen` takes no `--memory`). The state refinements explore at
the same limits. The header prints these bounds. A rule `check` cannot see at those bounds
can still fail at larger ones: run [`verify`](./formal-verification.md) for those.

The traces come from a second BFS. That one stops at `--conformance-states` states
(`conformance_states` in the table, default 65,536) and is not charged to the memory limit.

### What the types enforce

- A transition function takes only rows in its source states, so a call from another state
  does not compile.
- A state holds only the columns its state allows, narrowed by the model's rules.
- Outside the generated module or package, code cannot build a state or change its fields:
  Rust fields are private, Go structs are unexported, and TypeScript states are sealed under
  `--seal`.

The types do not stop code that goes around them:

- In TypeScript without `--seal`, a state can be written as an object literal, which skips
  the transition function and its rule checks. With `--seal`, a type assertion
  (`as unknown as`) still forges one, and also forges a caller capability.
- In Go, the zero value and a `nil` state or caller cannot be forbidden. The functions refuse
  a `nil` state with `InvalidArgument`; `Valid()` and `CallerValid(by)` report the rest.
- Whether the code that calls `issue` authenticated the actor is not checked.
- Rules over several rows, keys and uniqueness are obligations, not types.

### What `check --code` verifies

rdra-ish parses no service code. Each level includes the one before it.

- **`fresh`**: each generated file is what the model generates now. The code is compared by
  its text and the traces by hash. No compiler runs.
- **`build`**: the language's compiler builds the probe. Each compiler error maps back to a
  use case and its model location (`CodeMissingUseCase_<UseCase>`, `CodeIncomplete_*`,
  `CodeSignature_*`). A pass says nothing of a binary, a handler, or a file the probe does
  not reach.
- **`run`**: the probe's conformance test passes along the traces. Then a self-check runs the
  test again on mutant traces, each wrong at one row (`refused-call-runs`, `call-refused`,
  `wrong-state`, `absent-row`, `guard-removed`). A mutant that passes is
  `CodeMutantSurvived`.

At every level, each obligation needs an owner: a `rdra: obligation <name>` marker in the
service code, or an entry under `[code.obligations]`. The check says that an obligation has
an owner, never that it holds. Until the first owner exists, it prints a note and checks no
obligation.

### What a conformance run shows

A passing run is bounded conformance, not equivalence. Under five assumptions, the safety
rules the model verified hold for runs driven through the port, with at most one row created
per entity:

- H1: the service answers from the tracked columns and the call alone.
- H2: every reachable call is tried.
- H3: `seed` and `snapshot` reach the real store.
- H4: every write goes through the port, one call one transaction.
- H5: the rules read tracked columns only.

The run prints these, and what it covers, after each table
([What a run covers](./cli-reference.md#what-a-run-covers)). The harness checks what is
stored, not how. A refused call must fail with its kind and change nothing. A commit
prepared before another call changed its row must be refused, which shows the guarded write
of `OneTransaction`.

### What is not checked

- **Code outside the port.** Writes that skip the port, migrations, data an earlier version
  stored, external effects, liveness.
- **Concurrency.** The harness is one thread. Held reads (`SELECT .. FOR UPDATE`) and races
  between real connections are not shown.
- **Columns the model does not track.** The harness compares only the tracked part of each
  row.
- **Moves past the cap.** When the traces' BFS stops at `--conformance-states`, the moves past
  it are not run, and no call on an existing row is tried as refused. A service that runs a
  call the model refuses then passes. The run prints `warning[CodeCoverageIncomplete]` and
  still exits 0; `--deny-incomplete` makes it an error. The same holds for use cases the
  traces never run, refusals they leave untried (more than 64 bindings in one state), and
  calls that reached the unimplemented port (`CodeUnimplemented`).
- **What the harness does not compare.** A counted trigger, a comparison proposition, and the
  clock. The traces' header and the run's `not compared:` line name them.
- **Use cases with no function.** Their moves are left out of the traces; the `moves:` line
  counts them.

On the larger samples the cap is reached at the default. On `samples/clinic-ops`, the BFS
stops at 65,536 states: the traces run all 308,918 moves it found, but try only the 48
refused calls on rows that do not exist, and never run 22 of the port's use cases. On
`samples/ec-site` the BFS completes at 54 states. Raise `conformance_states` and run
`codegen --code` again to complete a capped suite.

### Costs

Counts from `codegen --lang rust --conformance <FILE>` on rdra-ish 3.0.0:

| Sample | BFS | Moves | Traces | Traces file | Rust code |
|---|---|---|---|---|---|
| `samples/ec-site` | complete, 54 states | 189 | 88 | 61,766 bytes | 183 KiB |
| `samples/clinic-ops` | stopped at 65,536 states | 308,918 | 181,111 | 236,971,795 bytes | 1.4 MiB |

Because a traces file can be this large, `codegen --check` and `check --code=fresh` compare
it by hash, reading it once without holding it. The TypeScript harness reads the file as an
`AsyncIterable` of lines, so a file past Node's longest string (about 512 MB) still runs.

The harness makes a fresh port once per trace and seeds it, so `new` and `seed` dominate the
run time of a large suite. Make `new` cheap: share one database and let `seed` clear it.

## Known limitations

- The Verus crate does not check which error a function returns. A contract asks for `Err`
  exactly when the guards fail; `Rejected` and `InvalidArgument` are not told apart.
- The Verus crate covers no `Int` or nullable column, `now`, run mark, split use case, event,
  choice, entity no use case creates, optional key, deleted parent, nested quantifier, or
  condition on a successor row. Verus may also reject a model in scope: a rule that holds but
  is not inductive with the key lemmas alone fails its preservation lemma.
- The contracts and the conformance port need every entity to have a one-column primary
  key that is not `Bool`. For another model the code has no contracts, and `--conformance`
  fails with `no conformance harness for this model`.
- Use cases outside the printed vocabulary get no function (see
  [Transition functions](#transition-functions-dispatchers-and-contracts)).
- The Rust contracts are synchronous. An async service implements them on a small adapter
  that calls `block_on` ([An async Rust service](./cli-reference.md#an-async-rust-service)).
- The `fresh` check records the model and the file, not the table's settings, so a changed
  path or `service` in `rdra.toml` shows as `generator changed`.

## How the generator is tested

These tests under `crates/rdra-ish-cli/tests/` hold the generator to the transition system
that `check` and `verify` explore:

- `codegen_equivalence.rs`: the functions in the IR do what BFS does, forward and converse, on
  every sample `check` passes.
- `codegen_differential.rs`: the printed Rust and Go functions, run as printed, return what
  BFS reaches on every move. The printed TypeScript is not run on every move.
- `codegen_behaviour.rs`: a driver in each language runs the code for one fixture, and code
  outside the module cannot build a state.
- `codegen_builds.rs`: the output of every sample builds with `tsc --strict`,
  `clippy-driver -D warnings -D missing_docs`, `go vet` and `gofmt -l`.
- `codegen_conformance.rs`, `codegen_code_build.rs`, `codegen_code_build_ts.rs`,
  `codegen_code_check_go.rs`: a reference service passes, and planted faults fail the harness
  and `check --code`.
- `codegen_contract.rs`: a snapshot of the TypeScript declarations the compatibility contract
  covers.
- `codegen_verus.rs`: the Verus crates verify, reject a broken rule or core, and agree with
  BFS. It needs `verus` on `PATH`, and skips without it unless `RDRA_REQUIRE_VERUS=1`.

## Where it connects to verification

- What `check` and `verify` decide, and what a passing run proves:
  [formal-verification.md](./formal-verification.md).
- The guarantees, trust base and assumptions of the checker:
  [theory/README.md](./theory/README.md).
- The transition system all of them read: [state-derivation.md](./state-derivation.md).
