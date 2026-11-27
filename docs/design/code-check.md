# Code check: does the service code have what the model requires?

<!-- derived-from ./algebraic-codegen.md#the-conformance-harness -->
<!-- constrained-by ./algebraic-codegen.md#goals-and-non-goals -->
<!-- constrained-by ../compatibility.md -->

Status (2026-11-26): design study of 2026-11-25 (against `14de5558` on main), accepted by
the owner the same day with the [decisions](#decisions) below. Tracking issue: #363 (open).

- **Landed.** One pull request per language. Rust with the language-neutral base: P0, P1,
  P2, P4 and P5 (#366). TypeScript: P3 and P4, `--seal` included (#368). Go: P2, P4 and P7
  (#369). `check --code=fresh|build|run` works for all three languages. P6's static plan, a
  capability type per actor in the contract arguments, is in all three.
- **Open.** P6's refused trace for a call by another actor, in every language (#337).
- **Added (#439).** The black-box [model adapter](#the-black-box-model-adapter), beside the
  port.
- **Superseded.** Nothing. [What exists today](#what-exists-today) describes main before P0.

## Summary

**It can be built. A static check can only show that a declaration exists and that its types
agree; behaviour stays with the conformance harness.**

rdra-ish does not read the user's code. From the model it generates one trait or interface
per use case, and a small **probe** file that names the service type written in `rdra.toml`.
The language's compiler then does the checking (`cargo check`, `go test -run '^$'`,
`tsc --noEmit`). `check --code` checks, in order, that the generated files are fresh (the
existing `codegen --check`) and that the probe compiles, and it maps each compiler error back
to a model location. The spikes showed, in all three languages, that a missing use case, an
input of the wrong type, and stale generated code each become an error with a model location.

Three things stay out of reach: code that declares an entry point and does nothing (`todo!()`),
an HTTP handler that writes the store directly instead of going through the contract, and
obligations a store or an operator keeps (uniqueness, atomicity, timers). The first two show
up only when the conformance harness runs; for the last one the check can confirm only that
someone declared they own it.

## What exists today

[As of `14de5558`, before P0. `codegen --format json`, the TypeScript port and harness, and the
probes have landed since; see the status above.]

- The TypeScript output already has transition functions, rule checks, `orderFromRow` and the
  dispatchers (`runCancel`). It lacks sealed state types and the conformance port and harness.
  Compatibility covers only the TypeScript types.
- `codegen --format json`, which [the codegen design](./algebraic-codegen.md#architecture)
  announces, does not exist yet.
- The Rust and Go `ConformancePort` is already a generated trait / interface with a
  `prepare_*` and a `commit_*` per action, so half of what a static check needs exists. Four
  things are missing: (a) something that forces a service to implement it, (b) a way back
  from a compiler error to a model location, (c) a TypeScript version, and (d) an entry point
  for use cases whose functions are not printed (#335).
- The port's unit is the action, not the use case. clinic-ops has 86 method pairs: a step of
  a split use case is `prepare_check_in_patient1`, and an event-triggered one is
  `prepare_send_appointment_notice_by_ev_appointment_cancelled`. ec-site has neither.
- ec-site has 17 use cases. Functions and port methods are printed for the 11 that write; the
  other 6 (`ViewOrderHistory` and so on) only read and have no entry point.
- Generating and `codegen --check`, `check` included, took 10 ms on ec-site and 39 ms on
  clinic-ops.
- Trace files had no fingerprint (line 1 was only `# rdra-ish conformance traces 2`). The
  clinic-ops traces reach 178 MB at the cap, so comparing them by regenerating and diffing
  the text is not an option.
- The LSP crate depends only on `rdra-ish-core` and `rdra-ish-syntax`. It runs neither the
  lifecycle checks nor `codegen`; `state_findings`, which `codegen` uses, lives in the CLI.
- A `[code.rust]` table in `rdra.toml` passed `check` silently: the top-level `Manifest` did
  not reject unknown tables.
- ec-site has no `permission`, timer, or `.fair`. clinic-ops, ec-site and oidc-auth together
  have 35 `permission` and 19 `timer`.

## What can be required (property x method x language)

**S** the compiler checks it from declarations. **R** it takes a run of the conformance
harness. **D** only the declaration that someone owns it can be checked. **N** it cannot be
checked. "now" holds on the output before this work, "plan" after it.

| # | What the model requires | Rust | Go | TypeScript | Note |
|---|---|---|---|---|---|
| 1 | Entity and column types (records, `<Entity>Row`) | S now | S now | S now | As long as the code uses the generated types; own structs need the binding of [Binding](#binding-the-code-to-the-model) |
| 2 | `@null` or not | S now `Option<T>` | S now `*T` | S now `T \| null` | Go cannot stop a nil dereference by type |
| 3 | The value sets of enums and states | S now | S now | S now | |
| 4 | Every value handled | S now `match` | S now through the generated visitor (`<Entity>Visitor`, `Match<Entity>`, P7); N for a plain `switch` | S now, only with a `never` check | Go lets a `switch` miss a case; a state added to the model fails every visitor implementation to compile |
| 5 | The columns a state narrows (state types) | S now, sealed | S now, but a zero value can be built | S now, unsealed; S plan with a private-symbol brand | |
| 6 | Id and key types | S now | S now | S now (brand) | |
| 7 | A referenced row exists | D / R (partly) | D / R (partly) | D | The harness keeps one row per entity, so it never tries an orphan |
| 8 | Uniqueness | D | D | D | No trace makes a second row; `export --kind sql` constraints against the real schema is another way |
| 9 | Each use case has one entry point | S plan; now only if the port is implemented | same | S plan; nothing now | Today nobody objects when the port is not implemented |
| 10 | The entry point's input and result types | S now (port implemented) | same | S plan | TypeScript needs property-style interfaces |
| 11 | Effects (rows written, target states, columns set) | R now; S as long as the generated functions are called | same | N now, R plan (TS harness) | Sealing means a state comes only from a generated function or `from_row`, but code that writes rows directly is not stopped by types |
| 12 | Guards and conditions (`Rejected` / `NotEnabled`) | R now | R now | N now, R plan | The `X` lines of the traces try the calls that must be refused |
| 13 | Use cases with no printed function (#335) | S plan (entry point only), R plan (pre/post) | same | same | Today only a header line names them; none is left on the samples (see [algebraic-codegen](./algebraic-codegen.md#use-cases-without-a-function)) |
| 14 | One-row rules (`forbidden`, `invariant`, `@check`) | S (what the state type ensures), R (the rest) | same | same (R is plan) | `rule_violations` is generated, not enforced |
| 15 | Multi-row rules (#336) | D now, R plan | same | D | Needs a generated validator and two-row traces |
| 16 | Permissions (#337) | S plan (a capability argument per actor), R plan (refusal for another actor) | same | same | Whether the code that makes a capability authenticates is **N** |
| 17 | Atomicity | R partly now, the rest D | same | D now, R plan | The `I` lines see it only when a written row's state changed; the lock itself is **N** |
| 18 | Run marks of split use cases | S (type), R (stored) | same | S, N now | |
| 19 | Events raised and the use cases they trigger | S (the result holds the event), R (what the snapshot holds) | same | S, N now | "Raised after the last step commits" is D |
| 20 | Timers and fairness (#338) | S plan (an entry point per timer), rest D | same | same | "Eventually runs" cannot be shown by finite traces; a scheduler's existence is **N** |
| 21 | `@derived` columns | D now, S plan (only the generated function sets it), R (tracked columns) | same | same | Since #362 a generated function computes a logical derived column from what it writes (a literal folds; a copied operand is read from the argument row); any other derived column stays an input, and the function's doc says why |
| 22 | `Enum @null` / `Bool @null` (#359) | the type is `Option`, the model has no null | same | same | Decided before P2: null is a value |
| 23 | The generated code is the model's latest | S now (`codegen --check`) | same | same | Language-neutral, by fingerprint |
| 24 | The whole service goes through the contract (no handler bypass) | **N** | **N** | **N** | See [What it cannot detect](#what-it-cannot-detect) |

Rows 1 to 6, 9, 10 and 23 are missing declarations, the centre of the request; the compiler
checks them in all three languages. From row 11 on, a property can break even when every
declaration is there.

## Binding the code to the model

| Method | Survives refactoring | How hard to satisfy without meaning it | False positives | False negatives |
|---|---|---|---|---|
| **Naming convention** (a function `place_order` exists) | Low: a rename breaks it; three naming styles | Low: an empty function of that name passes | Many: same names elsewhere, wrappers | Many: no types, wrong arguments pass |
| **Markers in code** (`// rdra: usecase PlaceOrder`, attributes, struct tags) | Medium: markers move with the code, but a comment disappears silently | Lowest: write the comment | Few | Many: what the marker points at is not looked at |
| **A mapping table in `rdra.toml`** (one symbol per use case) | Low: every rename edits the table; it grows with the use cases | Low; resolving it needs a code reader anyway | Few | Many, unless the target's type is checked |
| **Import and implement the generated code** (the compiler checks the generated trait / interface is implemented) | High: the compiler follows renames; the binding is a type reference | Medium: the signature must match; an empty implementation passes, but the same type is the harness's entry point, so a run fails it | Almost none: the compiler decides | Nothing happens when nobody implements it, which the probe closes |

The fourth is the core, and the table shrinks to **one service type** in `rdra.toml`:

```toml
[code.rust]
domain  = "src/domain.rs"              # codegen output; its freshness is checked
probe   = "tests/rdra_contract.rs"     # generated by rdra-ish; never edited by hand
service = "ecsvc::service::EcService"  # the type that implements the contract
```

The probe is a generated file that states, one line per use case, that `service` implements
that use case's trait. "Nobody implements it" becomes a compile error, and since rdra-ish knows
which line is which use case, an error's line number gives back the model location. rdra-ish
parses none of the user's code.

Markers are used only for the ledger of obligations no type can hold (proposal D below).

## How each language's code is read

| Means | Cost to depend on | Speed | Shared by the three languages | Precision |
|---|---|---|---|---|
| **The compiler as the checker** (generated traits + probe) | None for rdra-ish; the user needs the language's toolchain, which a service in that language has anyway | 0.4 s `cargo check`, 0.2 s `go vet`, 0.12 s `tsc` on the spike crates; a real project takes its own check time | Deriving the contract, the probe's shape, the line map, the diagnostic format; only printing and error extraction differ | Complete: aliases, re-exports, generics are the compiler's |
| **tree-sitter inside rdra-ish** | 29 more lock entries for three grammars (`cc` among them), a C compiler at build time, 4.4 MB stripped spike binary (rdra-ish release: 17.7 MB), 8.8 s build | 86 KB of generated Rust parsed in 8 ms | Queries per language; matching shared | Syntax only: no aliases, imports, or Go's implicit interface satisfaction |
| **`syn`** (Rust only) | Already in the lock for proc-macros; a runtime dependency lengthens the build | Fast | Rust only | Syntax only |
| **rustdoc JSON** | Nightly, unstable format | As slow as `cargo doc` | Rust only | Types resolved |
| **`go/packages`, the TypeScript compiler API** | Not one binary; the helper must run under the user's `go run` / `node`, and TS must resolve `typescript` | Medium | A whole helper per language | Complete |
| **Scanning text** (lines with `// rdra:`) | None | Fast | Identical | Only that a marker exists |

The first keeps the single binary and the PyPI distribution, is the most precise, and shares
the most between languages. Its two weak points: the compiler cannot run inside the LSP, and an
unrelated compile error in the user's code can bury the contract's result. The second is
handled by keeping only errors on probe lines and errors that name a generated method, and
counting the rest.

## Architectures

**A: conventions and parsing** (rdra-ish reads the code with tree-sitter). No compiler, runs in
the LSP, works on code that does not import the generated output. But it cannot resolve types,
Go's interface satisfaction has to be matched by hand, it overturns the non-goal "rdra-ish does
not read code back", and it adds up to about 30 crates.

**B: the compiler as the checker** (generated contracts + probe). Recommended core.

1. `codegen` splits `ConformancePort` into one trait / interface per use case (`CancelUseCase`;
   `ConformancePort` stays as their composition), TypeScript included. A trait has the methods
   of every action of its use case: each step of a split use case, each event copy of a
   triggered one. A probe line is a use case; a method name is an action. A read-only use case
   gets no trait, since the model states no effect of it.
2. `codegen --probe` writes the probe that names `service` from `rdra.toml`, with a fingerprint.
3. `check --code` checks (i) the freshness of the generated files and the probe, (ii) that it
   compiles, (iii) optionally the conformance run.

rdra-ish reads no user code, the precision is the compiler's, and the probe and the harness
share one binding. It needs a compiler run, and the generated traits become an interface a user
build depends on, outside compatibility. A probe can pass silently in three ways, all closed:

- **The probe is not in the build.** The default command names it: Rust `cargo check --test
  rdra_contract` (cargo fails on a test name it does not have), Go `go test -run '^$'` on the
  probe's package, TypeScript `tsc --noEmit -p tsconfig.rdra.json`, a generated file that
  `extends` the project's `tsconfig.json` and includes only the probe.
- **`--code` without `[code]`.** An error, not a silent pass.
- **A Rust service in a `main.rs`-only crate.** A probe under `tests/` can name only a lib
  crate's public path. Put the service type in a lib (recommended), or put the probe in the
  crate as a `#[cfg(test)] mod`, which reopens the first hole since the user writes the `mod`.

**C: semantic helpers per language** (`go/packages`, the TS compiler API, rustdoc JSON dump
declarations as JSON for rdra-ish to match). Works on code that does not import the output,
with types. Three implementations, no single binary, and costly for what B already covers.

**D: an obligation ledger** (marker scanning). Obligations no type holds (uniqueness,
references, atomicity, timers, who issues a permission) get stable names, and the code or
`rdra.toml` says where each is owned:

```
// rdra: obligation Unique_Payment_Order migrations/003_payment.sql
```

`check --code` reports each obligation of the model with no owner. It notices when the model
gains an obligation (a new `relate`, a new `@unique`) that the code did not follow. It sees
only the declaration, which one line satisfies, so the diagnostics say an obligation "has an
owner", never that it "holds". It needs the `O1`, `O2` numbering replaced by names that do not
move when the model changes.

**Recommendation: B as the core, a thin D on top; not A, not C.** tree-sitter can be
reconsidered if the LSP ever wants "go to this use case's implementation"; that needs only
positions, which the probe's line map or the marker scan likely gives.

## Example on ec-site

Generated, never edited by hand: `src/domain.rs` (`codegen --lang rust --conformance`, with the
contract traits, one per use case whose functions are printed, 11 on ec-site):

```rust
/// usecase ShipOrder  samples/ec-site/buc/buc_order.rdra:14:1
pub trait ShipOrderUseCase {
    fn prepare_ship_order(&mut self, order: OrderId) -> Result<ShipOrderOutcome, ShipOrderError>;
    fn commit_ship_order(&mut self, write: ShipOrderOutcome) -> Result<(), ShipOrderError>;
}
```

and the probe `tests/rdra_contract.rs`, which cargo picks up with no `mod` declaration:

```rust
type Service = ecsvc::service::EcService;   // [code.rust] service
const _: fn() = || { fn uc<T: CancelUseCase>() {} uc::<Service>(); };     // usecase Cancel
const _: fn() = || { fn uc<T: ShipOrderUseCase>() {} uc::<Service>(); };  // usecase ShipOrder
```

Written by hand or by an agent: `src/service.rs`, `EcService` and its 11 impls, which load rows
through `Order::from_row`, call the dispatchers (`run_cancel`), and write each result where
`<row>_expected` still holds. In Go, `var _ domain.ShipOrderUseCase = (*svc.Service)(nil)`
lines in `svc/rdra_contract_test.go` do the same; in TypeScript,
`export const _shipOrder: ShipOrderUseCase = service;` lines in `rdra_contract.ts`.

What `check --code` reports for each omission (the `code:` lines are real compiler output from
the spikes):

**ShipOrder not implemented**

```
error[CodeMissingUseCase_ShipOrder]: use case ShipOrder has no implementation in ecsvc::service::EcService
  model: samples/ec-site/buc/buc_order.rdra:14:1  usecase ShipOrder "Ship Order"
  code:  tests/rdra_contract.rs:5:59  E0277: the trait `ShipOrderUseCase` is not implemented for `EcService`
  fix:   impl ShipOrderUseCase for EcService (prepare_ship_order, commit_ship_order)
```

Go: `svc/rdra_contract_test.go:10:33: *svc.Service does not implement domain.ShipOrderUseCase
(missing method CommitShipOrder)`. TypeScript: `rdra_contract.ts(6,14): error TS2739: Type
'EcService' is missing the following properties from type 'ShipOrderUseCase':
prepareShipOrder, commitShipOrder`.

**An input of another type than the model's** (a bare integer for `OrderId`)

```
error[CodeSignature_Cancel]: use case Cancel takes other inputs than the model's
  model: samples/ec-site/buc/buc_order.rdra:13:1  usecase Cancel "Cancel Order"
  code:  src/service.rs:23:41  E0053: method `prepare_cancel` has an incompatible type for trait: expected `domain::OrderId`, found `i64`
```

Go reports `wrong type for method PrepareDeliverOrder` with have / want on the probe line;
TypeScript `TS2322 ... Types of property 'prepareDeliverOrder' are incompatible`.

**A value added to `Order.status` and the code not generated again**: `codegen --check` answers
as it does today (`model changed`, and the line counts). Once generated again, Rust fails each
`match` that lacks `Order::Returned` (E0004), Go passes, TypeScript fails only where a `never`
check is written.

**A state built without a transition function**: Rust fails with E0451 (private fields), today.
Go compiles `domain.OrderShipped{}` (a function given the zero value panics). TypeScript
compiles it today; a private-symbol brand fails it with TS2741, though `as` still passes.

**Nobody owns `Unique_Payment_Order`** (one Payment per Order), with D:

```
error[CodeObligationUnowned_Unique_Payment_Order]: no code owns the obligation Unique_Payment_Order
  model: samples/ec-site/shared/entities.rdra:90:1  relate(Payment, Order, 1:1)
  fix:   add `// rdra: obligation Unique_Payment_Order <file that keeps it>` or list it under [code.obligations]
```

**Not caught statically: `commit_ship_order` is `todo!()`.** The spike service passed `cargo
check` that way. The conformance harness fails it (`codegen_conformance.rs` fails a service
that drops a returned row with "no shipment row").

## Where it goes, and what it adds

### Commands

- `[code.rust]`, `[code.go]`, `[code.ts]` in `rdra.toml`.
- `rdra-ish check --code[=fresh|build|run]`:
  - `fresh`: the generated files and the probe are what the model generates now. No compiler.
    It generates again, which includes running `check` (39 ms on clinic-ops; a model whose
    `check` is slow takes that long). Trace files carry a fingerprint after the format line,
    and only hashes are compared (the format goes to 3).
  - `build`: `fresh`, then the language's default command, with the errors mapped to model
    locations.
  - `run`: `build`, then the conformance test the probe file holds.
- `codegen --check` and `codegen --conformance` stay. `check --code=fresh` reads `[code]` and
  runs `codegen --check` on each file.
- No separate command (`codegen --verify-impl`): "is the model right" and "does the code
  follow the model" are one check to the user, and one CI line is better.

### Diagnostics

Each diagnostic has a model location (`model:`), a code location (`code:`, the compiler's
whole message, its further lines indented, with the path relative to the working directory),
and a fix (`fix:`). The compiler's own output is printed first (for Rust, the `rendered` text
of each error in cargo's JSON). The extraction relies on two things: `file:line:col`
and the generated names. An error on a probe line goes back to its use case by the line
map; an error in the user's code goes back to an action by the method name in its message
(`prepare_cancel`, `PrepareDeliverOrder`), which is unique per action, or to a use case by
the dispatcher or transition function it names (`RunPay`, `PayFromOpen`). E0053 and E0046
name the method, not the trait, so nothing relies on trait names. Rust's
`--message-format=json` gives the spans directly. Errors that map to nothing are listed, the
first line of each, under one `CodeUnmapped`.

Two cases read more than the message (dogfooding, 2026-11-27):

- An error about the number of a call's arguments names no function in tsc (TS2554) or rustc
  (E0061). For such a message only, the check reads the one source line the compiler points
  at and maps it by the generated function the line calls. This is the one place the check
  reads the user's code, and it reads it as text: a line, never parsed.
- A probe that cannot resolve the service (`service` renamed in the code, not in
  `rdra.toml`) fails every line of the probe. The messages on the probe that cannot resolve a
  name and name a part of `service` become one `CodeServiceNotFound` pointing at `service`
  and `codegen --probe`, in place of one `CodeError` per use case.

A stale file counts one error per file. The fingerprint records the model and the file, not
the settings, so `generator changed` names the table's settings in `rdra.toml` first, then
another version; recording a settings hash would change the fingerprint line of every
generated file, and is not done.

### CI and the LSP

- CI adds one line, `rdra-ish check <model> --code=run`. A pull request that changes the model
  then fails, each with its own diagnostic, on code not generated again, a new use case not
  implemented, and conformance broken.
- The LSP starts no compiler; it must not run a command from `rdra.toml` on its own. `fresh`
  cannot run there either: the LSP depends neither on `rdra-ish-emit` nor on the CLI's
  `state_findings`. It only matches fingerprints: the line holds `model=<hash of the model's
  source> code=<hash of the body>`, so "the model changed" and "the file was edited" show without
  generating. That needs the hash in `rdra-ish-core`. A comment-only model edit also reads as
  "changed", so the LSP reports it as information on `rdra.toml`, and the CLI's `fresh` decides.

### What it adds to what exists

| Existing | What it looks at | What this adds |
|---|---|---|
| `codegen --check` | The generated file was not edited, the model did not change | Which files to check (`[code]`), with the probe and the traces |
| `ConformancePort` | Missing or mistyped methods, once implemented | An error when it is not implemented (probe), diagnostics per use case, a TypeScript version, entry points for use cases with no printed function |
| `codegen --conformance` | A finished service's behaviour | `check --code=run` runs it; the binding is shared with the static check |
| `codegen_differential.rs` | The printed Rust and Go agree with BFS (rdra-ish's own test) | Nothing: that is the generator's correctness, not the user's code |

Nothing is replaced. It fills the gap between `--check` and `--conformance`: the generated code
is fresh, but the service does not use it, or implements only part of it.

## What it cannot detect

1. **Code that declares and does something else.** `todo!()`, a `commit_*` that always returns
   `Ok(())`, rows built by hand instead of by the generated functions. Every static check passes;
   the harness fails the tracked columns.
2. **Code that bypasses the contract.** A handler that rewrites `status` in SQL without going
   through `EcService` is seen by no check. Sealing protects the values of state types, but
   `OrderRow` is a public struct and raw store writes are outside the types. Limiting which
   module may import the database driver is a discipline outside rdra-ish.
3. **Behaviour outside the harness's bounds.** One row per entity, untracked columns (strings,
   the values of `present` times), the clock, counted triggers, a BFS cut at the state cap. A
   service that breaks only with two rows passes; uniqueness is invisible for that reason.
4. **Concurrency.** The harness is single-threaded: it checks the write conditions, not the
   store's locks.
5. **Liveness.** That a timer fires, that a `.fair` action eventually runs, that a `Blocked`
   step keeps being retried. An entry point can be required, not that it keeps being called.
6. **Who issues a permission.** "No `ship_order` without the admin capability" is a type; whether
   the code that makes the capability authenticates is not checkable.
7. **What a ledger owner does.** Only that an owner is declared.
8. **A wrong model.** That is `verify`'s job; this check says only that the code has not left the
   verified model.
9. **The store's history.** Every trace starts from `seed` on an emptied store, so migrations,
   and rows an earlier version of the service wrote, are never run. `build` compiles the probe
   and what it imports, not the rest of the application.

1, 3 and 4 narrow as the harness grows. 2, 5, 6, 7 and 9 stay.

### What a passing run claims

<!-- constrained-by #what-it-cannot-detect -->

Decided by the owner on 2026-11-27 (#439): a passing `--code=run` is **bounded conformance**,
never equivalence. If the service passes the traces, the safety rules the model verified
(invariant, forbidden, `@check`, after-assert) over tracked columns hold for every run driven
through the port, with at most N rows created per entity (the traces' bound; deleted slots
are not reused). The claim rests on five assumptions:

- **H1 Deterministic.** The answer depends only on the tracked columns, ids, run marks and
  raised marks, and on the call; not on hidden state, untracked columns or the clock.
- **H2 Complete coverage.** Every reachable state and every call in the finite label domain
  is tried, run and refused. This needs the BFS to finish.
- **H3 Real store.** `seed` and `snapshot` reach the production store.
- **H4 Port-only writes.** Every write goes through the port, one call is one transaction,
  and writes are conditional.
- **H5 Tracked columns.** The rule reads tracked columns only.

The C rows give only model ⊑ implementation; the safety direction needs the X rows, every
call the model refuses refused with the store unchanged. A BFS stopped at its cap has no X
rows, so H2 fails there and the safety direction is not checked at all.

`--code=run` prints this on stdout before its summary line, one statement per table: the
bound, whether the BFS completed or stopped at its cap, the moves run and left out, the
refused calls tried, what the harness does not compare, H1 to H5 in one line, and what no
test can show (the [list above](#what-it-cannot-detect), shortened) in another. A capped suite
gets `warning[CodeCoverageIncomplete]` and the exit status does not change; `--deny-incomplete`
makes it an error. `codegen` prints the same statement after it writes the traces.

Since PR 4 of #439 the traces try every refusal of the model in each reached state (gap 2):
every binding of a use case's rows, creates on a free slot, and calls the functions would
run but the model refuses for a mark, a raise or a rule. Each `X` row names the kind the
functions refuse with, or `-` where they would run the call. `--deny-incomplete` fails on
the cap, on refusals no trace tries (a use case with more than 64 bindings in a state), and
on use cases of the port no trace runs (owner decision, 2026-11-27).

A passing run claims nothing of a call that reached the
[unimplemented port](./algebraic-codegen.md#the-unimplemented-port) (dogfood G6, owner
decision 2026-11-27): the harness stops the trace at such a call the model runs and does not
count such a refused call as a refusal, so the statement adds `unimplemented: <n> use cases
(...)`, stderr gets `CodeUnimplemented`, and `--deny-incomplete` fails on it as on the gaps
above. A run that does not print the harness's line (a `run` in the table that hides the
test's output) cannot say, and gets the same finding.

### The self-check by mutant traces

<!-- constrained-by #decisions -->

A pass says nothing if the run never read the traces. After a passing `--code=run`, rdra-ish
runs the same command on a few mutant traces, by default one of each kind and picked by a
seed. Each mutant fails a conforming service at the row it changes. The service code is
never changed (decision 1). The run is evidence only as a CI run (decision 11).

- **Kinds.** Each kind changes the last row of a cut trace into something the model never
  does: `X` to `C` (`refused-call-runs`), `C` to `X` (`call-refused`), `C` to a state whose
  rows differ (`wrong-state`), and `C` on a one-row call naming an absent id
  (`absent-row`).
- **No equivalent mutants.** No mutant is equivalent to the real traces, so a survivor is
  never a false alarm. A survivor is therefore an error by default, and `--mutants 0` turns
  the self-check off.
- **What the first three show.** A deterministic service that passed the real traces does
  again what it did there, so it fails these three whether or not its adapter is honest. Their
  kill rate checks the pipeline: whether the command runs the harness on `RDRA_ISH_TRACES`
  and fails when it fails.
- **What `absent-row` shows.** It is the one kind about the service. At one row per entity
  every id is `1`, so an invoke that ignores the id passes the real traces; it fails only
  this kind.
- **Constant snapshots.** A snapshot that returns what was seeded fails the real traces at the
  first call that changes a row, before the self-check runs.
- **Kill criterion.** A mutant counts as killed only when the run fails and its output names
  the changed row (`traces line <n>:`). All three harnesses number the lines of the file the
  same way.

**Guard removal.** Traces from a model with one guard removed die at their first row the
model refuses, a call the condition rejects. With every refusal tried (PR 4), each such call
is an `X Rejected` row of the real traces, so the `guard-removed` mutant is that row expected
to run: one per condition, named in the `self-check` line. The owner decided on 2026-11-27
that guard removal comes with the wider refusals, and that a surviving mutant is an error by
default.

**Seed and path (H1).** A few reached states are seeded directly and followed by one call,
which other traces make from the same state reached by calls; each trace after the first
seeds other values in the columns the model does not track. A model adapter picks those
values itself, so for it only the seeded states apply. Ids are banded by entity (`Order 101`,
`Shipment 201`), so a lookup by another entity's id finds no row.

### The black-box model adapter

<!-- constrained-by #decisions -->
<!-- constrained-by #what-a-passing-run-claims -->

Gap 8 of #439: the port is written in the generated core's types, and `fresh` requires the
domain file unedited, so a project that renamed or restructured the core it took cannot use
the harness at all. The owner decided on 2026-11-27 that a black-box adapter sits beside the
port. `codegen --adapter` (with `--conformance`) prints it in place of the domain code, and
`adapter` in a `[code.<lang>]` table names it in place of `domain`.

- **Plain data.** `ModelState` holds per entity the rows with their id and the tracked
  columns (a column tracked only by presence is a bool), the pending run marks, and the
  raises a triggered use case reads. `Call` holds one case per action with functions: row
  and mark ids, the event, inputs and choices. Ids are integers or text, scalars other than
  Int and Bool are their text, and value sets are the adapter file's own enums. Nothing names
  a type of the core, so a project maps its own code to the model in a small adapter.
- **The interface.** `reset`, `seed(ModelState)`, `observe() -> ModelState`, and
  `invoke(Call)`, which returns that the call ran or a refusal kind (`NotEnabled`,
  `NotFound`, `Rejected`, `InvalidArgument`, `StoreFailed`, the variants of the core's
  use-case errors), which the harness checks against the kind an `X` row names. A
  `StoreFailed` fails the run as a failure of the service. `prepare(Call)` and `commit()` are
  optional, and only as a pair (in Rust a
  `SplitAdapter` that `split()` returns, so a forgotten `commit` does not compile). Without
  them the harness runs an interleaved row (`I`) as its second call alone, which reaches the
  same state, and counts it as skipped: conditional writes go unchecked, and `--code=run`
  says so. `commit` takes no token of its prepare (the study had `commit(Pending)`): the
  adapter holds one prepared write at a time, which is all an `I` row needs; the owner
  decided on 2026-11-27 to revisit that only if a test holds more than one. A split adapter
  runs the traces a second time with every `C` and `X` row made by `prepare` and then
  `commit` (owner decision, 2026-11-27), so a `commit` that never stores fails there. Skipped
  `I` rows do not fail `--deny-incomplete`: the `ran:` line counts them.
- **The same traces, the same words.** The harness reads the traces the port's harness
  reads, abstracts `ModelState` to the same lines as the port's snapshot, and reports a
  difference in the same text (`traces line <n>:`). So the self-check runs through it
  unchanged, and the faults of the reference service fail the same way.
- **Its own namespace.** The file declares only its own names, so it lives beside the core
  in its own module (Rust, TypeScript) or package (Go). Its fixed names are claimed before the
  model's, and a model name that meets one gets a suffix.
- **`fresh` is black-box.** It compares the adapter file, the traces, and the probe; the
  domain code is the project's. The probe holds only the conformance test, which prints the
  counts it ran (`rdra-ish conformance: ok …`); `--code=run` reads them into a `ran:` line,
  because a pass of an adapter without `prepare` must not claim the interleavings.
- **What it does not change.** Decision 1 (rdra-ish reads no user code) and the claim:
  H3 is now about the adapter, which picks the untracked columns' values when it seeds and
  must reach the real store. The port, its harness, and every generated file of a table with
  `domain` are as before.

## Phases

Each phase ships and is verified on its own.

| Phase | Delivers | Verified by | Shares with |
|---|---|---|---|
| **P0** a machine-readable contract | `codegen --format json`: types, states, use cases, functions and obligations, with model locations and stable names. `O1`… become stable names: the invariant names that exist (`Unique_Payment_Order`) and new ones for the rest (`Ref_Product_category_id`) | A snapshot; a test that an unrelated declaration added to the model moves no existing name | #339 (intent and location in doc comments, the same data) |
| **P1** `[code]` and `check --code=fresh` | `[code.<lang>]` in `rdra.toml`; `codegen --check` on the configured files; `--code` without `[code]` is an error; a fingerprint on trace files; the LSP matches fingerprints only | CLI tests shaped like `codegen_check.rs` (same, edited, model changed, generator changed) | — |
| **P2** per-use-case contracts and probe (Rust, Go) | `ConformancePort` split into one trait / interface per use case; `codegen --probe`; `check --code=build` with the errors mapped | A reference service with one omission planted at a time, each giving a named diagnostic (as the faults of `codegen_conformance.rs`); the probes of every sample compiled in `codegen_builds.rs` | #335 (entry points for use cases with no printed function) |
| **P3** TypeScript contracts | Property-style interfaces, the probe, `tsconfig.rdra.json`; sealing by brand behind an opt-in flag | The P2 omission tests; the brand under `tsc` | #335 |
| **P4** `check --code=run`, TypeScript harness | The conformance test generated into the probe file, run by `run`; the TypeScript port and harness | The 4 existing faults fail through it, in TypeScript too | #336 (multi-row rule validators, which need two-row traces) |
| **P5** obligation ledger | `// rdra: obligation <name>` markers or `[code.obligations]`; obligations with no owner diagnosed | Adding `@unique` to the model adds one diagnostic | #338 (timer and fairness assumptions as named obligations), #336 |
| **P6** permissions | A capability type per actor in the contract trait arguments; a refused trace for a call by another actor | On clinic-ops or oidc-auth, a call without the capability does not compile; the refusal trace | #337 |
| **P7** Go's gap (done in the Go pull request of #363) | A generated state visitor (`MatchOrder`) so a missed value fails to compile | A model with one more value fails the visitor's implementation (`go_visitor_fails_to_compile_on_a_new_state`) | — |

#359 is a prerequisite, not a phase: whether `Enum @null` has null as a value changes the
generated types, so it is decided before P2 fixes the contracts as an interface. It was decided
on 2026-11-25: null is a value of a nullable Enum or Bool.

## Decisions

The owner decided these on 2026-11-25, each as the study recommended. They are final.

1. **Round trip stays a non-goal.** rdra-ish does not parse the user's code. It reads its own
   generated files, the compiler's output, and, for the obligation ledger, the
   `rdra: obligation` marker lines ([algebraic-codegen.md](./algebraic-codegen.md#goals-and-non-goals)).
2. **No file layout.** rdra-ish does not decide where the generated code or the service lives;
   the user writes the paths in `rdra.toml`. The probe's placement constraints are accepted:
   Rust under `tests/`, with the service type public from a lib crate; Go as a `_test.go` in the
   service's package; TypeScript visible from the generated `tsconfig.rdra.json`.
3. **User builds may depend on the generated traits.** The Rust and Go output stays experimental:
   when a release changes its shape, `codegen --check` says "generator changed", and the
   compiler shows the difference after generating again. Whether to put it under compatibility is
   revisited once P2 to P4 are stable.
4. **`check --code[=fresh|build|run]`.** `check` without the flag does not change; with `[code]`
   present it prints a one-line note.
5. **Binding: one service type in `rdra.toml`.** No per-use-case mapping table for now (a
   `service = { Cancel = "…", default = "…" }` form is deferred).
6. **One contract trait / interface per use case.** Diagnostics are per use case, and one probe
   line is one model location. `ConformancePort` stays as their composition.
7. **`--code=build` has a default command per language, overridable in `rdra.toml`.** The command
   runs only when `--code=build` (or `run`) is given on the command line. The LSP never runs it.
8. **The obligation ledger comes in P5.** Diagnostics and docs say an obligation "has an owner",
   never that it "holds". An owner names a file path, and the check confirms the file exists.
9. **TypeScript gets the contracts and the probe (P3) and the harness (P4).** The study proposed
   waiting with the harness; the owner put it in P4. Brand sealing breaks
   [compatibility](../compatibility.md) (a state type is an interface of `<tag>: "<v>"` and one
   property per column, and a required brand fails code that builds states from object
   literals), so it is an opt-in flag; making it the default is a next-major decision.
10. **No tree-sitter.** It worked in the spike, but resolves no types, adds up to about 30 crates,
    and needs a C compiler to build.
11. **No run receipts.** Nothing that can be forged counts as evidence; CI runs `--code=run`.
12. **#359 before P2.** Done: null is a value of a nullable Enum or Bool.
13. **Read-only use cases get no required entry point for now.** The model says only what they
    read, so no input or result type can be derived, and requiring a name alone is as weak as a
    naming convention. Revisit when #335 / #339 can derive result types from screens or API DTOs.


Added the same day:

14. **The generated code follows the Kamae principles** (`kamae` for TypeScript, `kamae-rs` for
    Rust, the same carried over to Go), inside each language's pull request of #363. The
    TypeScript contract of [compatibility](../compatibility.md) changes to the Kamae shapes in
    the coming major release, with a migration note. No external library in the output. State
    types are discriminated by `kind` in every language, and the tag column lives only in the
    row type. The shapes are in
    [Generated code style](./algebraic-codegen.md#generated-code-style). Personal data
    (`Sensitive<T>`) is not covered: the language cannot mark a column as personal yet.

## Spike results

| # | Checked | Result |
|---|---|---|
| S1 | ec-site in three languages, Rust and Go with `--conformance` | 10 ms; Rust 1924 lines, Go 1992, TS 1206; traces of 189 moves, 72 traces, 123 refusals, 13 interleavings |
| S2 | Rust: a trait per use case and a probe make a missing impl an error on the probe line under `tests/` | `tests/rdra_contract.rs:5:59 E0277`; cargo picks it up with no `mod` |
| S3 | Without the probe nothing fails | The service compiles with no ShipOrder impl |
| S4 | Rust: a missing method, a wrong argument type, a missed `match` arm, a forged sealed state | E0046, E0053, E0004, E0451 (E0451 only alone: other errors hide it) |
| S5 | Go: `var _ I = (*T)(nil)` per interface in a test file | `go test -run '^$' ./...` reports a missing method and a wrong type (with have / want) on each probe line; `go vet` stops at the first. A zero `domain.OrderShipped{}` and a type switch missing a state build |
| S6 | TypeScript: assignments to interfaces in a probe | `tsc --noEmit --strict` reports TS2739 (missing) and TS2322 (wrong type) per probe line. A hand-built state object passes; a private `unique symbol` brand fails a literal with TS2741, `as unknown as` passes. A method-style interface lets a narrower argument through, a property-style one does not |
| S7 | Stale output after a model change | `codegen --check` exits 1 with `model changed` |
| S8 | tree-sitter (Rust, Go, TypeScript grammars) | Types, traits, methods and `// rdra:` comments with line numbers in all three; 86 KB in 8 ms; no type resolution |
| S9 | Mapping compiler errors to model locations | Lines fall on probe lines (S2, S5, S6) and user-code errors name methods (S4); the mapping itself was not written |
| S10 | The harness fails a `todo!()` service | Not run on ec-site; relies on the 4 faults of `codegen_conformance.rs` |
| S11 | Capabilities, timer entry points, the ledger, the Go visitor | Design only |
| S12 | Compile time at real project size | Not measured |
| S13 | clinic-ops: generation and `--check` time; the port's shape for split and triggered use cases | 39 ms each, 59 ms with `--conformance-states 300`; 86 actions |
| S14 | `[code.rust]` in `rdra.toml` under the old `check` | Exit 0, no warning |
| S15 | `cargo check --test <name>` with no such test | `error: no test target named ...` |
| S16 | Fingerprint matching in the LSP, trace fingerprints, `tsconfig.rdra.json` | Design only |

## Implementation notes

What was built where the study left a choice open. This section grows with each phase.

**P0.** `codegen --format json` prints a view of the IR, not the IR: `format`
(`rdra-ish codegen json 1`), `check`, `types`, `states`, `usecases`, `functions`,
`obligations` (see [the CLI reference](../cli-reference.md#the-json-contract)). `--lang` is not
needed with it. A use case's `kind` is `functions`, `unprinted` (an action with no function
yet, #335) or `read-only` (no action: it writes nothing). Obligation names come from model
identifiers only: the invariant's name where `check` names one (marked `checked`), else
`Ref_<Entity>_<columns>`, `Unique_<Entity>_<columns>` (the name `check` gives a unique set it
holds), `OnDelete_*` / `OnUpdate_*`, `Derived_*`, `OneTransaction`, `RunMarks_<UseCase>`,
`CompleteRows_<Type>`, `RuleChecks`, `NotMet_<error>`. A second obligation of a checked
invariant (an `@check` the data layer lists and the rule checks list again) is dropped; two
unchecked ones of one name are numbered `_2`, `_3` in model order, which an unrelated
declaration cannot reach.

**P1.**

- `[code.rust]`, `[code.go]`, `[code.ts]` and `[code.obligations]` deny unknown keys. The
  table holds everything that changes the generated text (`traces`, `conformance_states`,
  `package`) and the binding keys of P2 (`probe`, `service`, `module`, `new`, `root`, `build`,
  `run`), so the Go and TypeScript pull requests add no key for what is shared.
- `codegen <INPUTS> --code` writes the configured files. It prints sites and the header's
  input paths relative to the directory of `rdra.toml`, so `check --code` generates the same
  text from any working directory; plain `codegen --check` still prints them relative to the
  working directory.
- `--code` without a level is `fresh`, so a bare flag never starts a command. The level is
  written with `=` (`--code=build`); `check --code model` reads `model` as an input.
- `check --code` runs after the model check passes; on a model that fails it, it checks nothing
  and says so. It reports each file, then `OK: code: ...` or `FAIL: code: ...` on stdout.
- The traces format is 3: line 2 is `# rdra-ish codegen fingerprint: model=<m> code=<h>`, a
  comment both harnesses skip. `h` hashes the lines, each ended by `\n`, without the
  fingerprint line, so a check streams the file. `fresh` generates the traces in memory
  (the BFS `codegen --conformance` runs) and compares that hash with the file's.
- The model hash moved to `rdra-ish-core` (`fingerprint.rs`). It sorts the source texts before
  joining them, so the LSP, which loads a workspace in another order than the CLI, computes
  the same value. Files generated before this change read as "model changed" once.
- The LSP reads the fingerprint of each file `[code]` names and shows an information
  diagnostic on the table's line of rdra.toml when its model hash differs, or the file is
  missing or unstamped. It starts nothing.
- A language plugs into `check --code` through `Backend` (`crates/rdra-ish-cli/src/code_check`):
  the files beyond the domain code and traces that `fresh` compares (the probe), `build`, and
  `run`. A language without them stops at the level before, with a note.

**P2, the shared part.** The names the Rust, Go and TypeScript pull requests share, so each
mirrors them instead of inventing its own:

- Contracts are in the IR. `PortDecl.contracts` holds one `ContractDecl` per use case with
  methods, in the order of the port's methods. It has `name` (`<UseCase>UseCase`, or
  `<UseCase>Contract` when a model type has that name), `usecase`, `label`, `site`, `error`
  (`<UseCase>Error`) and `methods` (indices into `PortDecl.methods`). `ConformancePort` is
  their composition with `seed` and `snapshot`: a Rust supertrait list, Go interface embedding,
  TypeScript `extends`.
- `domain.port` is built whenever `check` passed, so the contracts are printed without
  `--conformance` and the probe always has them. `DomainModel.harness` is true only with
  `--conformance`, and only then are `StoreSnapshot`, `ConformancePort` and `conform` printed.
  A model the harness cannot name the rows of gets no port without `--conformance`, and stays
  an error with it.
- The probe is a module of its own, `rdra_ish_emit::codegen::probe`.
  - `Probe { text, lines: Vec<ProbeLine> }` and `ProbeLine { line, usecase, contract }`, with
    `Probe::at(line)`.
  - `ProbeWriter::new(first)`, `.line(text)`, `.usecase(text, &ContractDecl)`, `.finish()`.
    It numbers the lines of the file as written: the fingerprint is line 2
    (`FINGERPRINT_LINE`).
  - `ContractIndex::new(&PortDecl, spell)` takes a method name in the language's spelling back
    to its contract and action. `.find(text)` returns the first whole-word method name a
    message holds, and `.methods_of(c)` lists a contract's methods.
- Compiler errors map in `crates/rdra-ish-cli/src/code_check/map.rs`.
  - `CompilerMessage { file, line, column, code, text }`.
  - `Finding { NotImplemented, Incomplete, Signature, Other }`.
  - `Mapping { probe_file, probe, index, contracts, service }`.
  - `map_messages(&[CompilerMessage], &Mapping, &dyn Backend)` routes each message. A message
    on a probe line goes to that line's use case; another that names a generated method goes
    to that method's use case.
  - The diagnostics are `CodeMissingUseCase_<UC>`, `CodeIncomplete_<UC>`,
    `CodeSignature_<UC>`, `CodeError_<UC>`, one per use case and kind, plus one
    `CodeUnmapped` that counts the rest.
  - A failed build that the backend reads no error from is `CodeBuildFailed`, and a failed run
    is `CodeConformance`.
- `Backend` has only language hooks: `probe`, `extra_files` (the stamped probe by default),
  `build_command`, `run_command`, `parse_messages`, `classify`, `method_names` and
  `implement_hint`. The shared `build` / `run` run rdra.toml's `build` / `run`, or else the
  backend's command, in `Target::root()`, then parse and map. `codegen --code` also writes
  the backend's extra files.
- Names a printer derives from model names (Go `<Entity>Visitor`) come from
  `derived_names(Lang, &DomainModel) -> Vec<DerivedName { key, name, by }>`. `build_for`
  claims them after every IR name. On a collision the name takes a trailing `_`, and the
  printer reads the claimed name with `DomainModel::derived(key)`. When that spelling is taken
  too, it is an error naming both claimants. `build_for(model, refine, port, Option<Lang>,
  site)` also claims `reserved_names(lang)` first. Rust derives none: every name it declares
  is an IR field.

**P2 and P4, Rust.**

- The probe is `[code.rust] probe`, under the service crate's `tests/`, so cargo builds it as a
  test target with no `mod` line. Each use case is one line,
  `const _: fn() = || { fn uc<T: svc::domain::ShipUseCase>() {} uc::<Service>(); }; // usecase Ship`.
  With `new` and `traces`, the probe also holds `#[test] fn rdra_conformance`, which calls
  `conform` on the traces the crate includes by `CARGO_MANIFEST_DIR`. That makes P4 a
  `cargo test --test <probe> -- rdra_conformance`.
- A service that implements `ConformancePort` in its library fails `cargo check` at that impl
  before the probe compiles (E0277 on the supertrait). The message names the contract, so it
  maps by the contract's name. Messages on probe lines are taken first; one per use case and
  kind is kept.
- Sites in the IR are relative to rdra.toml. The diagnostics show them relative to the
  working directory, as the check's other paths are.
- Entry points (#335): an action with no printed function adds an `EntryDecl { action, name,
  rows }` to its use case's contract. The rows are the ids of the action's params. Rust prints
  `fn run_<action>(&mut self, <rows>) -> Result<(), <UseCase>Error>`, and the use case gets
  an error type with only `NotEnabled`. On clinic-ops these were `CheckInPatient_2`,
  `ApplySelectedTreatmentFee`, `CreateCharge` and `RegisterPerformedProcedure`, until #335
  printed them; no sample has an entry point now.
- Tests:
  - `codegen_code_build.rs` plants three omissions in the conformance service crate (a use
    case not implemented, a method missing, an argument of another type). Each gives its named
    diagnostic and no `CodeUnmapped`. The four harness faults fail `--code=run`.
  - `codegen_builds.rs` `rust_probes_name_every_contract` compiles the probe of every sample
    against a service that implements nothing. It expects E0277 on exactly the line map's
    lines.

**P5.** The ledger is language-neutral (`crates/rdra-ish-cli/src/code_check/ledger.rs`).

- A marker is any line that holds `rdra: obligation <name> [file]`, so `//`, `#` and `--`
  comments all work and no parser is needed. It is found in the files under each table's
  `root`, outside the generated files and the usual build directories.
- A marker's `file` is relative to that root. Without one, the owner is the marker's own file.
  `[code.obligations]` paths are relative to rdra.toml.
- The ledger is kept once one owner exists: a project that has not started one gets a note,
  not one error per obligation. Once kept, adding `@unique` to the model adds exactly one
  diagnostic, the new obligation with no owner (`codegen_code_check.rs`).
- An owner of a name the model lacks is a warning, since a rename in the model would otherwise
  fail every build until the marker follows.

**P6, Rust (the S plan).**

- The IR has `DomainModel.capabilities: Vec<CapabilityDecl { actor, label, name }>`, one per
  actor that performs a use case with a contract (`rdra_ish_core::actors_for_usecase`: direct
  `performs`, plus the BUC's actors unless an event or a timer starts it). It also has
  `ContractDecl.caller: Option<CallerDecl { name, actors }>`. The names are
  `<Actor>Capability` (or `<Actor>Cap`) and `<UseCase>Caller` (or `<UseCase>UseCaseCaller`).
- Rust prints each capability as a struct with a private unit field, made only by `issue()`.
  Each caller is an enum of its actors' capabilities with `From` for each. Every `prepare_*`
  and `run_*` of the contract takes `by: <UseCase>Caller` first, and the harness calls as the
  first actor.
- `codegen_behaviour.rs` tests it on oidc-auth: `DenyDevice` (EndUser only) builds with
  `EndUserCapability::issue()`. Without a caller it fails with E0061, and with
  `DenyDeviceCaller::RelyingParty` it fails with E0599.
- **Not done: the R plan, a refused trace for a call by another actor.** In Rust such a call
  does not compile, so the harness has no way to make one, and the model names no dynamic
  principal that a service would check at run time. A refusal trace fits a language where a
  capability can be forged (a Go zero value, a TypeScript cast). Doing it in Rust would need a
  decision on a dynamic `Caller` that the service converts from its authenticated principal.

**P3 and P4, TypeScript.**

- The contracts are `type <UseCase>UseCase = { readonly prepareX: (..) => Promise<Result<..>>;
  ... }`: `readonly` members rather than `Readonly<{..}>`, because tsc prints an alias of a
  mapped type expanded and an alias of an object type by name. A message then says
  `from type 'ShipUseCase': prepareShip, commitShip`. An entry point (#335) is
  `run<Action>`. They are async: a TypeScript store is.
- `[code.ts] service` is `<file relative to rdra.toml>#<exported type>`. The probe imports the
  domain and the service as namespaces (type-only without the harness) by `.ts` specifiers,
  and has one line per use case: `export const _Ship = (it: Service): domain.ShipUseCase =>
  it;`. It is valid at runtime, so the same file holds the conformance test, which calls
  `domain.conform` with `new` (default `new service.<Type>()`) and prints
  `rdra-ish conformance: ...`.
- `codegen --code` also writes `tsconfig.rdra.json` in the root. It extends `[code.ts]
  tsconfig`, else the root's `tsconfig.json`, else sets `strict` itself. It adds `noEmit` and
  `allowImportingTsExtensions` and holds only the probe. `build` is `tsc --noEmit --pretty
  false -p tsconfig.rdra.json`. `run` is `node <probe>`: Node 22.18 strips the types without a
  flag. An older Node's `ERR_UNKNOWN_FILE_EXTENSION` becomes a `CodeConformance` that names
  the version.
- tsc has no code for "not implemented". TS2741 (one member), TS2739 (the list) and TS2740
  (four and "N more") are `Incomplete`. `missing_members` reads the list, so the shared rule
  turns a message that misses every member into `NotImplemented`, and so do messages that
  together miss every member of one contract (Rust's E0046 too, as Go's backend counts
  them). TS2322 is `Signature`.
- The probe and the domain code are ES modules. A package Node and tsc read as CommonJS
  (`"type": "commonjs"`, npm 11's `npm init -y`) fails `build` under `verbatimModuleSyntax`
  with TS1286 / TS1287 / TS1295 on every file, which the backend's `build_cause` turns into
  one `CodeCommonJs`; without that option, `run` fails on Node's `Cannot use import
  statement outside a module`, which `run_failure` names.
- `--seal` / `[code.ts] seal` adds `[$sealed]: true` to each state type, keyed by a `unique
  symbol` the module does not export. A literal outside the module fails with TS2741, and
  `as unknown as` still passes (`codegen_behaviour.rs`). `Symbol` is reserved for it.
- Tests:
  - `codegen_code_build_ts.rs` plants five omissions in `fixtures/codegen/ts_service`: a use
    case missing, one member missing, a narrower argument, another result type, a misspelled
    member. Each gives its named diagnostic and no `CodeUnmapped`. A service with no member
    gives `CodeMissingUseCase_*` for a two-member contract (TS2739) and a six-member one
    (TS2740). The four harness faults fail `--code=run`.
  - `codegen_builds.rs` `typescript_probes_name_every_contract` checks every sample's probe
    against a service type with no member. It expects TS2739 / TS2740 / TS2741 on exactly the
    line map's lines.

**P6, TypeScript (the S plan).**

- The same IR (`DomainModel.capabilities`, `ContractDecl.caller`). Each capability is
  `Readonly<{ kind: "<Actor>"; [$capability]: true }>`, where `$capability` is a `unique
  symbol` the module does not export, and its companion `<Actor>Capability.issue()` is the only
  way to make one without a type assertion. `<UseCase>Caller` is the union of its actors'
  capabilities, and every `prepare*` and `run*` member takes `by: <UseCase>Caller` first. The
  harness calls as the first actor.
- `codegen_behaviour.rs` tests it on oidc-auth. `DenyDevice` builds with
  `EndUserCapability.issue()`. Without a caller it fails with TS2554, with a RelyingParty
  capability with TS2345, and with an object literal with TS2741. `as unknown as` passes.
- A service whose `prepare*` ignores `by` still implements the contract, because TypeScript
  accepts a function with fewer parameters. A service that takes another caller type fails
  the probe with TS2322 (`CodeSignature_*`).
- The R plan is not done, as in Rust and Go (#337 stays open).
