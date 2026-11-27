# Algebraic domain types: projecting the model into TypeScript, Rust, and Go

<!-- constrained-by ../vision.md -->
<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../state-derivation.md -->
<!-- derived-from ../language-reference.md#data-layer -->

Status (2026-11-26): accepted in scope on 2026-11-07 as an experimental extra
([vision.md](../vision.md#decisions)); every question is [decided](#decisions).

- **Landed.** [Phases](#phases) 0 to 5; every use case on the samples gets a function. The
  conformance harness, private state fields and one transaction per function (#348, for
  #330, #331, #332). `codegen --check` (#343). The printed Rust and Go run on every BFS edge
  of the samples (#342). The Kamae shapes in all three languages, `--format json`, `--code`,
  `--probe` and `--seal` (#366, #368, #369, for #363).
- **Open.** Validators for rules over several rows (#336), timer and fairness obligations
  (#338), intent in doc comments (#339), and the refused trace of P6 (#337). `--refine reachable`,
  `--part`, `--buc` and `--scalars` have not landed.
- **Superseded.** `export --kind typescript-states` and `states --format typescript`, removed
  on 2026-11-24 for `codegen --lang ts` (#238; see
  [The state unions before codegen](#the-state-unions-before-codegen)). Since then the
  TypeScript types are covered by
  [compatibility](../compatibility.md#the-typescript-types-of-codegen); the rest of the
  subcommand and its output stays experimental.

The model already states the shape of the domain: the values an actor chooses
(`variation`), the records the system keeps (`entity`), the keys between them (`relate`),
the states a record moves through (`transitions`), what each use case reads, requires, and
writes, and which combinations must never occur (rules). That is exactly what an
algebraic domain model in code states: product types, sum types, newtypes, and total
functions between them.

**Purpose.** rdra-ish prints a domain model in TypeScript, Rust, or Go that an agent
finishes later. The output must be consistent, conform to what `check` and `verify`
verified, and build as printed. Finishing the code (persistence, the values the model
leaves abstract, wiring) is the agent's job, not rdra-ish's. Writing files is a means, not
a goal: the output is one stream the agent reads and places.

## The state unions before codegen

This section is history. When this note was written, `export --kind typescript-states`
(`crates/rdra-ish-emit/src/typescript.rs`) printed one discriminated union per entity from
the reachable patterns `states` derives. `states --format typescript` printed the same
unions. It was the only code output. Running it on the samples showed why it could not be
the base of a code generator as it stood:

| Observation (command) | Problem for code |
|---|---|
| `states skills/rdra-ish-verify/samples/int_stock.rdra --format typescript` prints `stock: 1` and `stock: 3` | An Int axis becomes a literal type taken from the bounded domain `0..5`. Real stock values are rejected by the type. |
| `export samples/ec-site --kind typescript-states` prints `processed_at: unknown` for `Payment` | A present value with no `present:<type>` in `sets` loses the column's declared type (`DateTime`). |
| The same output has two variants with `status: "authorized"` | One tag, several shapes. TypeScript accepts it; a Rust enum or a Go sealed interface needs one variant per tag. |
| The same output types `Order.delivered_at` as `null` in every variant, `delivered` included, and `Payment.gateway_ref` as `null` throughout | ec-site never `sets` those columns, so no reachable state holds a value. The type turns a gap in the model into a rule of the code: a delivered order can never record its delivery time. |
| Only state axes appear | `total`, `ordered_at`, the keys, and every column no rule reads are missing, so the union describes states, not records. |

The first two were bugs in that export, fixed on their own (see [Phases](#phases),
phase 0). The last three are design questions this note answers.

The next release after v2.3.0 removed both outputs. `codegen --lang ts` prints a state type
for each entity with a lifecycle column. That state type answers the last three rows: one
interface for each tag value, narrowed only by rules, with every column of the entity.
Keeping a second TypeScript output that narrows by reachable patterns meant two answers to
one question, and one of them encodes the gaps of the model as rules of the code. The
owner chose `codegen` as the one way to get the types, and made those types part of the
compatibility contract.

## Goals and non-goals

Goals:

- **It builds.** The output for each language is one self-contained compilation unit (a TS
  module, a Rust module, a Go file) that compiles under strict settings as printed, with
  no runtime library from rdra-ish. Every place the agent must fill in compiles too
  (`throw new Error("TODO: ..")`, `todo!("..")`, `panic("TODO: ..")`).
- **It is consistent.** The **data types** (variations, entities, keys, DTOs), the
  **state sums** (one variant per state of an entity's state model), and the **transition
  functions** (one per action of the transition system) come from one intermediate form,
  so the three languages and every part of one output agree.
- **It conforms to the verification.** A generated function allows exactly what `verify`
  explored for the use case: the source states and guards it requires, the effects it
  writes, the events it raises, and, for a use case split into transactions, the order and
  the boundaries of its steps ([Use cases split into transactions](#split)). What the
  model verified but a type cannot enforce is printed as a named **obligation** that
  names the `verify` invariant it protects ([The agent contract](#agent-contract)).
- **The agent can finish it.** Values the model leaves abstract (`present`, a chosen
  variation value, a choice among arms) are the caller's input. Business computation,
  persistence, and transport are stubs the agent fills in.

Non-goals:

- Persistence, repositories, DDL. [vision.md](../vision.md) puts physical design out of
  scope, and DBML stays the last data output.
- HTTP clients and servers. OpenAPI generators (`openapi-typescript`, `progenitor`,
  `oapi-codegen`) already read `export --kind openapi`; this export prints the DTO types
  only, so a handler can be typed by them.
- Round trip. rdra-ish does not read code back. After generation the agent owns the code;
  it stays conformant through the agent contract and the generated conformance tests, not
  through protected regions. `check --code` ([code-check.md](./code-check.md)) keeps this:
  rdra-ish parses none of the user's code. It reads only its own generated files, the
  compiler's output, and the `rdra: obligation` marker lines of the obligation ledger
  (decided 2026-11-25).
- A file layout. rdra-ish does not decide where code goes in a project.

This widens what rdra-ish produces from review and verification artifacts to code a
build depends on. The maintainer decided on 2026-11-07 that this is in scope
([vision.md](../vision.md#decisions)).

Code generation is an experimental extra, outside [compatibility](../compatibility.md),
except for the TypeScript types of `codegen --lang ts`, which
[compatibility](../compatibility.md#the-typescript-types-of-codegen) covers since
2026-11-24. The other flags of the subcommand may change or go away in any release. So may
the rest of the generated code: its functions, the obligation list, the Rust and Go output,
and the JSON form. `rdra-ish --help`, the CLI reference, and the header of every output
say which part is covered. An agent therefore treats the output as a starting point
to finish, not as an interface to depend on across rdra-ish upgrades. After an upgrade,
it generates again and compares the new output with the old. Phase 1 adds the
subcommand and its output to the "What is not covered" list of the compatibility page.

## The mapping

### Data types

| Model | TypeScript | Rust | Go |
|---|---|---|---|
| `variation V { a b }` | `type V = "a" \| "b"` and `const V = ["a","b"] as const` | `enum V { A, B }` | `type V string` with `const VA V = "a"`, a `Valid()` method, and a `Values` slice |
| inline `Enum(..)` on an attribute | as a variation, named `<Entity><Column>` | same | same |
| `@pk` column of entity `E` | `type EId = number & { readonly __brand: "EId" }` | `struct EId(pub i64)` | `type EId int64` |
| key column a `relate` generates | the parent's id type (`customeraccount_id: CustomerAccountId`), from the same `ForeignKey` fact DBML reads | same | same |
| `@null` column | `T \| null` | `Option<T>` | `*T` |
| `Int`, `String`, `Bool` | `number`, `string`, `boolean` | `i64`, `String`, `bool` | `int64`, `string`, `bool` |
| `Money`, `Decimal`, `Date`, `DateTime` | names from a scalars module | same | same |
| `@derived(expr)` | a field with the expression in its doc, and an obligation to compute it; a transition function computes a logical one it can (see [Transition functions](#transition-functions)) | same | same |
| `@check(expr)` | an obligation naming `Check_<E>_<col>_<n>`; later a check in the smart constructor `parseOrder` | `TryFrom` | `func NewOrder(..) (Order, error)` |
| `dto` | a record | a struct | a struct |

The scalars module holds the four types whose representation a project picks (a decimal
library, a time library). The generator prints a default (branded `string` in TypeScript, a
newtype over `String` in Rust, a named `string` in Go), which the agent replaces. A later
flag, `--scalars <module path>`, makes the generated file import the project's own instead.

Phase 1 prints a `@check` and a `@derived` as obligations, not as code. The comparison
semantics of a check (how `null` and `present` compare) is the lifecycle's, and code that
evaluates it differently would claim a check it does not make. The conformance tests of
phase 3 print the evaluator together with the test that compares it with the lifecycle's
verdicts. A transition function computes a logical `@derived` column only where those
semantics cannot differ: comparisons of a column that is never null with a value, and
`== null` / `!= null`. Any other derived column stays an input of the function.

### State sums

An entity with a lifecycle column is a sum with one variant per value of its tag column's
inline `Enum`, a value that no use case reaches included. (This note first proposed the
states the state diagram draws, a `transitions` endpoint or a value a creating use case
sets; the generator prints every value, and
[compatibility](../compatibility.md#the-typescript-types-of-codegen) now covers that.) An entity with no lifecycle column is a plain record. The tag is
the column the removed `typescript-states` export picked: `status`, else a `*_status`
column, else the first lifecycle column by name. Every other lifecycle column is a field typed by its own
enum.

Each variant carries every column of the entity. The question is which columns a variant
**refines**: a nullable column that is always present, or always null, in that state.

**Declared refinement (recommended default).** A variant refines a column only when a
rule of the model says so:

```rdra
invariant(Order).when(status == delivered).then(delivered_at == present)
forbidden(Order, status == pending, delivered_at == present)
```

The first makes `delivered_at: DateTime` in `OrderDelivered`; the second makes it `null`
in `OrderPending`. Every refinement is a rule `check` and `verify` already check, so the
type is as sound as the verified model, and it does not depend on the bounds.

The default also makes the modeler close the gap the reachable types hide. Added to a copy
of ec-site, the `invariant` above fails `check` with a trace that ends at `DeliverOrder`,
since no use case writes `delivered_at`. After
`sets(DeliverOrder, Order.delivered_at == present)` the model passes, and the type then
says what the business means: a delivered order has a delivery time.

**Reachable refinement (opt-in, `--refine reachable`).** A variant refines a column when
every reachable pattern with that tag agrees on it, as the removed `typescript-states` did. It
is tighter, but the table above shows the cost: it is bounded, and it turns a column the
model never writes into a column the code can never write. It was never measured whether
the patterns at 1 instance per entity (`Bounds::STATES`, what `states` explores) are every
pattern reachable at 2 or more, which the type needs. That measurement comes before this
mode ships.

The difference between the two is useful by itself: each refinement the reachable states
show but no rule states is a candidate rule. `next` or `lint` can list them
("`Order.delivered_at` is present in every reachable `delivered` state; no rule says so").

Int, Money, Decimal, Date, and DateTime columns are never refined to a value, only to
present or null. The bounded integer domain is a verification device, not a type.

### Transition functions

A use case is one function. The intermediate form reads it from the
`TransitionSystem` action (`crates/rdra-ish-core/src/lifecycle/types.rs`), not from the
`SemanticModel`, so `states`, `verify`, and the generated code read one semantics, as the
TLA+ printer does ([vision.md](../vision.md#the-lifecycle-layer-has-one-semantics)). The
data types above come from the `SemanticModel`, as DBML's do, because the transition
system keeps only the columns some check reads.

| Action item | Generated |
|---|---|
| `Param` `Existing` of entity `E` | an argument of type `E`, narrowed by the guards below |
| `Param` `New` of `E` (`creates`) | the function returns a new `E`; its non-defaulted, non-derived columns are inputs |
| a logical `@derived` column of a written or created row | computed from what the function writes to the columns the expression reads: a literal folds it, a column copied from an argument row reads that row. Only comparisons of a column that is never null with a value, and `== null` / `!= null`, are printed, where the lifecycle's comparison semantics and the printed code's agree. Any other (an arithmetic one, two columns, `now`, a nullable operand, an input operand, a `drives(..)` proposition) is an input, with the reason in the function's doc |
| `Param` `Parent` | an argument of the parent's type; the child's key is set from it |
| `Guard` `Transition` (`from` states) | the argument type is the union of the source variants, so a call from another state does not compile |
| `Guard` `Condition` (`holds` body) | a generated predicate; the function returns `Err(Rejected { condition })` when it fails |
| `.has(..)` / `.none(..)` in a body | the related rows are an argument (`lines: readonly OrderLine[]`), filtered by the generated key |
| a guard that reads `now` | `now` is an argument |
| `Effect` `Transition`, one arm | the return type is the target variant |
| arms that share a `from` (a choice) | a choice argument of a generated enum (`DecideFilingOutcome`), and the return type is the union of the targets |
| `sets(.., col == present)` | an input of the column's type |
| `sets(.., col == null)`, an Enum or Bool value, or an Int literal | written by the generated body |
| `sets(.., E.col == F.src)` | copied from the source argument |
| `chooses` | an input of the variation type, narrowed to the listed values |
| `deletes` | the row is consumed and the result says it is deleted |
| `links`, `relinks`, `unlinks` | the key field is set or cleared |
| `raises(UC, Ev)` | the result carries the event value `EvOrderShipped` |
| `triggers(Ev, UC)` | the event is an argument: the raised mark of 2.0 ([event-order.md](./event-order.md)) becomes "the handler cannot be called without the event value" |

The function body is fully generated. Every effect of a use case is either a constant the
model names or a value the caller passes, so no hand-written code goes inside it.

A function is printed per **action**, not per use case. A use case of one transaction
group is one action and one function. A use case split into k groups is k actions and k
functions; [Use cases split into transactions](#split) decides how they connect.

### One example in three languages

ec-site's `ShipOrder` (`transitions(ShipOrder, Order.status, paid -> shipped)`,
`raises(ShipOrder, EvOrderShipped)`) and `Cancel` (`(pending, awaiting_capture) ->
cancelled`), with the two rules and the `sets` above added to the model.

TypeScript:

```ts
export type OrderId = number & { readonly __brand: "OrderId" };

interface OrderBase {
  id: OrderId;
  total: Money;
  tax: Money;
  ordered_at: DateTime;
  customeraccount_id: CustomerAccountId;
  shippingaddress_id: ShippingAddressId;
}
export type OrderPending = OrderBase & { status: "pending"; delivered_at: null };
export type OrderPaid = OrderBase & { status: "paid"; delivered_at: DateTime | null };
export type OrderShipped = OrderBase & { status: "shipped"; delivered_at: DateTime | null };
export type OrderDelivered = OrderBase & { status: "delivered"; delivered_at: DateTime };
// ... awaiting_capture, cancelled
export type Order = OrderPending | OrderAwaitingCapture | OrderPaid | OrderShipped
  | OrderDelivered | OrderCancelled;

export type EvOrderShipped = { event: "EvOrderShipped"; order: OrderId };

export function shipOrder(order: OrderPaid): { order: OrderShipped; raised: [EvOrderShipped] } {
  const next: OrderShipped = { ...order, status: "shipped" };
  return { order: next, raised: [{ event: "EvOrderShipped", order: order.id }] };
}

export function cancel(order: OrderPending | OrderAwaitingCapture): { order: OrderCancelled } {
  return { order: { ...order, status: "cancelled" } };
}
```

Rust, an enum of per-state structs. A transition takes its source by value, so the old
state cannot be used after the move:

```rust
pub struct OrderBase { pub id: OrderId, pub total: Money, /* .. */ }
pub struct OrderPaid { pub base: OrderBase, pub delivered_at: Option<DateTime> }
pub struct OrderShipped { pub base: OrderBase, pub delivered_at: Option<DateTime> }

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "status", rename_all = "snake_case"))]
pub enum Order { Pending(OrderPending), AwaitingCapture(OrderAwaitingCapture), Paid(OrderPaid),
                 Shipped(OrderShipped), Delivered(OrderDelivered), Cancelled(OrderCancelled) }

pub enum CancelFrom { Pending(OrderPending), AwaitingCapture(OrderAwaitingCapture) }

pub fn ship_order(order: OrderPaid) -> (OrderShipped, EvOrderShipped) { /* generated */ }
pub fn cancel(order: CancelFrom) -> OrderCancelled { /* generated */ }
```

Go has no sum types. A sealed interface (an unexported marker method) closes the set to
the package, and an exhaustiveness linter (`go-sumtype` or `exhaustive`) checks the
switches:

```go
type Order interface{ isOrder(); Status() OrderStatus }

type OrderPaid struct {
	OrderBase
	DeliveredAt *DateTime `json:"delivered_at"`
}
func (OrderPaid) isOrder()             {}
func (OrderPaid) Status() OrderStatus { return OrderStatusPaid }

func ShipOrder(order OrderPaid) (OrderShipped, EvOrderShipped) { /* generated */ }

// CancelFrom is OrderPending or OrderAwaitingCapture.
type CancelFrom interface{ Order; isCancelFrom() }
func Cancel(order CancelFrom) OrderCancelled { /* generated */ }

func UnmarshalOrder(data []byte) (Order, error) { /* reads "status", then the variant */ }
```

The JSON form is the same in all three: the record's columns with the tag column inline,
the row shape DBML and JSON Schema describe.

### Who makes a state

<!-- constrained-by #decisions -->

The examples above show public fields, as the first output had. Code could then build
`OrderPaid { .. }` without running the transition that leads there, or change a field a
transition keeps, and Go also took `OrderStatus("bogus")` and the zero value `OrderPaid{}`.
Since #331, Rust and Go make a state only in the generated code:

- A variant's fields are private in Rust and unexported in Go, and methods of the same name
  read them (`paid_at()` returns `&DateTime`, or `Option<&T>` for a nullable column;
  `PaidAt()` returns a copy, so a pointer does not reach into the state). The only
  constructors are the transition functions (whose created rows take the inputs the model
  leaves open) and `from_row` / `<Entity>FromRow`, which checks the row against the state's
  rules. A test outside the module or package (`codegen_behaviour.rs`) shows that the
  compiler refuses a struct literal of a state and a write to its field.
- Go cannot forbid the zero value of a struct, nor a conversion to a `string` type, from
  another package. Since #363 a Go state type (`OrderPaid`) is therefore an interface, whose
  one implementation is an unexported struct (`orderPaidState`): code outside the package
  cannot write a literal of it, so its zero value is `nil`, which is no state. A transition
  function refuses `nil`, or a value of a type of its own that embeds the interface, with
  `InvalidArgument`; `<Entity>ToRow(nil)` is the zero row. `<Entity>FromRow` refuses a value
  outside a value set and a scalar outside its shape in any column, a function refuses one as
  an input with `InvalidArgument`, and each value set and scalar has `Parse<Type>(s)`. Nothing
  panics. (Until #363 a state carried an unexported mark, and a function given the zero value
  panicked.)
- Go keeps this only when the code that finishes the output is in another package, so the
  generated file goes in a directory of its own, named by `--package`.

Records, row types, ids, and DTOs keep public fields: they are the shapes a store and the
wire hold, and a state is made from them only through `from_row`. Decision 4 stands:
the Rust encoding is still an enum of per-state structs whose transitions consume their
source.

<a id="split"></a>
## Generated code style

<!-- constrained-by ./code-check.md#decisions -->

The owner decided on 2026-11-25 (#363) that the generated code follows the Kamae principles:
`kamae` for TypeScript, `kamae-rs` for Rust, and the same principles carried over to Go. The
Rust, Go and TypeScript pull requests of #363 each bring their language to this section; until
one lands, that language's output differs from it as [Status of each language](#status-of-each-language) says.

### What every language shares

- **No external library.** The output is self-contained: its own small `Result` type and
  boundary parsers in TypeScript (no neverthrow, no zod), the standard library only in Rust (no
  thiserror) and in Go. No `unsafe` in Rust.
- **Domain shape and persistence shape are separate.** A state type is a sum of one type per
  state, told apart by a discriminant called `kind`. The model's tag column (`status`, ...)
  is not a field of the domain types: it lives only in the flat row type `<Entity>Row`, which
  holds every column as the data layer declares it, public and with the column's value type.
  `fromRow` / `from_row` / `<Entity>FromRow` maps a row to its state and fails with a typed
  error naming the column whose value its state rules out; the other direction
  (`toRow` / `into_row` / `<Entity>ToRow`) cannot fail. A record (an entity with no lifecycle
  column) has no states and is its own row type.
- **The `kind` discriminant** has one value per state, the state's tag value in PascalCase
  (`Pending`, `Paid`). It is a type of its own, `<Entity>Kind`, not the tag column's value set,
  so renaming a column or moving the tag does not change the domain type. In TypeScript it is
  the `kind` property of each state type; in Rust the variant of the state enum, read by
  `kind()`; in Go the `Kind()` method of the state interface. The IR names it
  (`SumDecl.kind`): `<Entity>Kind`, or `<Entity>StateKind` when a column's value set or a
  state already has that name.
- **A column named `kind`** of a state type would clash with the discriminant, so in the
  states' domain shape it takes the reserved-word escape, a trailing `_`: `kind_` (Rust field
  and accessor `kind_()`, Go `Kind_()`, TypeScript `kind_`). `<Entity>Row` keeps the wire name
  `kind`, and `fromRow` / `toRow` map between the two. A record (no states) keeps `kind`. The
  IR holds both spellings: `Field.wire` is the row's, `Field.name` (and
  `Field::domain_pascal()`) the domain's, so every printer spells it alike.
- **Ids and scalars are opaque.** An id type and each prelude scalar (`Money`, `Decimal`,
  `Date`, `DateTime`) wrap their representation privately and are made only through a parser
  that checks it and fails with a typed error: Rust `TryFrom<repr>` and `FromStr`, TypeScript
  `parse<Type>(value): Result<Type, InvalidValue>` with a branded type, Go `Parse<Type>(value)
  (<Type>, error)`. A value set parses its wire spelling the same way. Money and Decimal hold
  decimal text (`-?[0-9]+(\.[0-9]+)?`), Date `YYYY-MM-DD`, DateTime ISO 8601 with a time and an
  offset or `Z`; the check is the shape, not the calendar.
- **The row is a boundary too.** `fromRow` / `from_row` / `<Entity>FromRow` and
  `parse<Entity>Row` check the shape of every Money, Decimal, Date and DateTime column with the
  same parser `TryFrom` / `parse` / `Parse<Type>` use, and refuse a bad one with `InvalidRow`
  naming the column. In Rust the row's fields already have those types, so the check happens
  where the row is built (`Money::try_from`); in TypeScript and Go, whose rows hold the text,
  in `fromRow`.
- **Errors are values, typed per use case.** A transition function returns its result or a
  `<UseCase>Error` (Rust enum, TypeScript union on `kind`, Go error type usable with
  `errors.As`). Its cases are the ones the use case can fail with: `NotEnabled` (the rows are in
  states it does not run from; only the dispatcher returns it), `Rejected` (a condition applied
  to the use case does not hold), and `InvalidArgument` (the arguments break what the verified
  model keeps: a child row that does not name the parent passed with it, a row that is not the
  event's row, a column the target state keeps null that holds a value). No generated code
  panics, throws, or calls `unwrap` / `expect` / `panic`: a failure the caller can cause is an
  error value.
- **Transitions are pure and consume their source.** A function takes the source states by
  value and returns new values; it reads no clock and no store. The rows it writes come back in
  its result with the source each must still have when the write commits
  (`<row>_expected`).
- **Every public item is documented**: what it holds, the invariants its type keeps, the
  errors a function returns and why, and for a transition its source and target states.

### Rust

Following `kamae-rs`:

- Ids: `pub struct OrderId(i64)` with a private field, `impl From<i64>` (every `i64` is an id,
  so `TryFrom` comes with it) and `FromStr`, `get()` returning the representation, and
  `Display`. Prelude scalars over a private `String` with `TryFrom<String>`, `FromStr`,
  `as_str()` and `Display`; the shape checks are the module's own small functions.
- Value sets: `pub enum PaymentMethod`, with `as_str()`, `ALL`, `FromStr<Err = InvalidValue>`
  and `Display`.
- State types: `pub enum Order { Pending(OrderPending), ... }`, each variant's struct sealed
  (private fields, read by methods of the column's name; #331), `Order::kind() -> OrderKind`,
  and no `status()`. `OrderRow` has public fields, the tag `status: OrderStatus` among them;
  `impl TryFrom<OrderRow> for Order { type Error = InvalidRow; }` with `Order::from_row` as its
  named form, and `impl From<Order> for OrderRow` with `Order::into_row`.
- Transitions: `pub fn pay(order: OrderPending, order_paid_at: DateTime) -> Result<PayResult,
  PayError>`, and the dispatcher `pub fn run_pay(order: Order, ...) -> Result<PayOutcome,
  PayError>`. `pub enum PayError { NotEnabled(NotEnabled), Rejected(Rejected),
  InvalidArgument(InvalidArgument) }` holds only the cases the use case has, and every error
  type implements `std::error::Error` and `Display`.
- Lints: no crate-level `allow` of clippy. The output passes `cargo clippy -D warnings` as a
  library module; a lint that a shape needs (`too_many_arguments` on a function with many
  inputs) is allowed on that item with the reason in a comment.

### TypeScript

Following `kamae`, in the coming major release (the TypeScript types are under
[compatibility](../compatibility.md); the TypeScript pull request rewrites that contract and the
CHANGELOG gives the migration):

- `type` aliases, not interfaces, every property `readonly`; a state type is a union of
  `Readonly<{ kind: "Pending"; ... }>` cases.
- A companion object per type: `Order.fromRow(row): Result<Order, InvalidRow>`,
  `Order.toRow(state): OrderRow`, `OrderId.parse(value): Result<OrderId, InvalidValue>`.
- Branded ids and scalars (`string & { readonly __brand: "OrderId" }`), made only by `parse`.
- Pure transitions whose parameter types narrow the source state, returning
  `Result<PayResult, PayError>`; error unions discriminated by `kind`.
- A self-contained `Result<T, E>` (`{ ok: true; value: T } | { ok: false; error: E }`),
  `assertNever` for exhaustiveness, and no `as` except `as const` / `satisfies`.
- `parse<Entity>Row(value: unknown): Result<<Entity>Row, InvalidRow>` at the boundary.
- Function-property notation in contract types (`prepareShipOrder: (...) => ...`), so a narrower
  parameter type does not pass. Brand sealing of state types stays opt-in (decision 9 of
  [code-check.md](./code-check.md#decisions)).
- Async contracts: each `prepare*` / `commit*` returns `Promise<Result<..>>`, since a
  TypeScript service reads and writes its store asynchronously and a synchronous contract would
  force it to block or to fake its store. The Rust and Go contracts are sync, as their stores'
  clients are. The split into prepare and commit, the `Result`, and the error unions are the
  same in all three; the TypeScript `conform` awaits each call.

As built: a brand is made without a type assertion through a type predicate, the companion's
`is: (value: unknown) => value is OrderId`, which `parse` calls. A value set's companion has
`values` and `parse`. Every record and DTO gets `parse<Name>(value: unknown)` as well. The
module's private helpers (`$int`, `$nullable`, the tag table `$orderTag`) start with `$`,
which no model identifier can, and only the ones the module uses are printed. A parameter a
function does not read is printed `_name`. The output builds under `tsc --strict` with
`noUnusedLocals`, `noUnusedParameters`, `exactOptionalPropertyTypes`,
`noUncheckedIndexedAccess` and `noPropertyAccessFromIndexSignature`, since the probe's
`tsconfig.rdra.json` extends the project's own.

### Go

The same principles, with the standard library only:

- Sum types as sealed interfaces: `type Order interface { Kind() OrderKind; isOrder(); row()
  OrderRow }`, the unexported methods implemented only by the generated state structs, and the
  visitor of P7 of [code-check.md](./code-check.md#phases) for exhaustiveness. Each state is
  an interface too (`OrderPaid`: `Order`, its accessors, and `isOrderPaid()`), implemented by
  one unexported struct ([Who makes a state](#who-makes-a-state)); a type switch on the state
  interfaces tells them apart. The visitor is `OrderVisitor[R any]`, one method per state named
  by its kind (`Paid(OrderPaid) R`), and `MatchOrder(state, v) (R, error)`; a state the model
  gains is a method every implementation lacks, so the compiler names each place that matches
  states (`codegen_behaviour.rs` checks it). `<Entity>Kind` is a `string` type with one constant per state
  (`OrderKindPaid = "Paid"`).
- Unexported fields read by accessors of the column's name (`PaidAt()`, `Kind_()` for a
  column `kind`); a nullable column's accessor returns a copy. Ids and scalars are defined
  types made by `Parse<Type>(...) (<Type>, error)`; a scalar's `Valid()` checks the same shape.
  Records, rows, DTOs, events and marks keep exported fields.
- `<Entity>FromRow(row) (<Entity>, error)` validates the row structs (`<Entity>Row`, exported
  fields with `json` tags), which stay separate from the domain types; `<Entity>ToRow(state)`
  cannot fail. A state shares no pointer with a row.
- Typed errors usable with `errors.Is` / `errors.As`, and no `panic`. Every transition
  function and dispatcher returns `(<Result>, error)`, the error a `*PayError{Kind, Err}`:
  `Kind` is a `PayErrorKind` constant (`PayErrorNotEnabled`, `PayErrorRejected`,
  `PayErrorInvalidArgument`, only the ones the use case has), and `Err` the case's detail
  (`*NotEnabled`, `*Rejected`, `*InvalidArgument`), which `Unwrap` returns, so
  `errors.As(err, &rejected)` reaches it and `errors.Is(err, ErrRejected)` matches it. Go adds
  `InvalidArgument` where Rust's types already rule the argument out: a nil state, a value
  outside a value set, a scalar outside its shape. `*InvalidRow` and `*InvalidValue` likewise.
- Value semantics: a transition returns new values and changes none of its arguments.
- A doc comment on every exported item; the file starts with `// Code generated ... DO NOT
  EDIT.`, and is `gofmt` and `go vet` clean (`codegen_builds.rs` checks both on every sample).

Not covered: personal data (`Sensitive<T>`, `secrecy`). The language has no annotation that
marks a column as personal, so the generator cannot know which to wrap; tracked separately.

### Status of each language

- Rust: in these shapes since the Rust pull request of #363. `codegen_builds.rs` runs
  `clippy-driver -D warnings -D missing_docs` on the output of every sample, so no lint is
  allowed crate-wide; the few per-item allows name their reason (a function of more than seven
  arguments, an accessor named after a column such as `new`, the model's spelling of a name).
- TypeScript: in these shapes since the TypeScript pull request of #363. `codegen_builds.rs`
  runs `tsc --strict` with the strictest optional flags on the output of every sample, and
  checks the text for interfaces, type assertions and throws.
- Go: in these shapes since the Go pull request of #363. `codegen_builds.rs` runs `go vet` and
  `gofmt -l` on the output of every sample and checks that it calls no `panic`;
  `codegen_code_check_go.rs` runs `gofmt -l` on the probe, with a service package whose import
  path sorts before the domain's. The domain code also passes staticcheck's default checks
  with its `Code generated` line removed: a Bool test is `x` or `!x`, and a generic helper is
  printed only where something calls it (tested in the emit crate, not with staticcheck, which
  CI does not install). Names keep the model's spelling with an initial capital (`Id`,
  `PatientaccountId`, `OrderId`), not Go's initialisms (`ID`), which staticcheck's optional
  ST1003 asks for: the names follow the model's column and entity names one to one in all three
  languages, and a project that wants `ID` writes it in its own code. The conformance
  harness (`--conformance`) likewise prints a value parser (`confText`, `confSet`, ...) only
  where a call or a row reads one.

## Use cases split into transactions

### What verify assumed

`tx.rs` splits a use case whose writes span several transaction groups: the writes of each
invoked API in `invokes` order, then the FK-connected direct writes, then each isolated
write ([vision.md](../vision.md), rule 6; [Runs of a split use case](../state-derivation.md#runs)).
The use case becomes the actions `<UC>_1` .. `<UC>_k`, and `verify` explored them under
these rules:

1. The steps are separate transactions. Any other action can run between two of them,
   including a run of the same use case on other rows.
2. A run is the rows its steps bound. Each row a step before the last writes carries a run
   mark `<E>._run_<UC>`. Step 1 takes only rows without a mark. Step i > 1 takes the rows
   marked `i-1` and reads them **as they are now**, not as step 1 left them.
3. A row is in at most one run of a use case at a time.
4. A step whose guard fails when its turn comes waits. The run is never abandoned, and
   nothing is compensated. A row no step can move shows up as a deadlock or a use case
   that never runs.
5. A condition applied to the use case guards step 1. A condition applied to an API
   guards that API's step. A `.when` on an arm or a `chooses` value guards the step that
   writes its column.
6. An event the use case raises is raised by the last step. A triggered use case's first
   step reads the event's mark, so with several tracked events the first step has one
   copy per event (`<UC>_by_<Ev>`).
7. `after(UC).assert` holds after the last step, on the run's rows. A row deleted during
   the run still finishes it, because the mark, not the existence, matches it.

The samples have three split use cases, and each of them depends on one of these rules:

| Sample | Steps | What it depends on |
|---|---|---|
| `cross_order_payment_split` | `Pay_1` captures the Payment (CapturePaymentApi), `Pay_2` pays the Order (PayOrderApi) | Rule 1. `Cancel` runs between the steps, and `check` fails with `MultiForbidden_Order_0`. That is the defect the sample exists to show. |
| `split_run_order`, insurance-claim-ops `OpenSettlement` | step 1 moves the claim to `negotiating`, step 2 creates the Settlement under that claim | Rules 2 and 3. The sample's comment records the bug (#80) seen without them: a second settlement under a claim that had one, which breaks `Unique_Settlement_Claim`. |
| clinic-ops `CheckInPatient` | step 1 checks the Appointment in and updates the PatientAccount (CheckInPatientApi), step 2 creates the EligibilityCheck (OnlineEligibilityApi) | Rule 6. `EvAppointmentCheckedIn` is raised after step 2, once the EligibilityCheck exists. |

### Options

- **A. One function for the whole use case.** The function computes every step from one
  snapshot, and the caller commits it once or step by step.
- **B. One function per step, linked by a token in memory.** Step 1 returns the ids of its
  rows, and step 2 takes them. Nothing is stored between steps.
- **C. One function per step, linked by stored run marks.** Each step takes and returns
  the marks of the rows it binds, as the model does. The agent stores a mark with its row,
  in the transaction of the step.

### The cases

| Case | A | B | C |
|---|---|---|---|
| Another use case moves a later step's row between the steps (`Cancel` in `cross_order_payment_split`) | Computed from a stale snapshot, step 2 writes `paid` over `cancelled`, a move that no `transitions` has. Committing A as one transaction across two APIs is not what the model says. | Correct, if step 2 reloads the row | `pay2` reads the order as it is now. Its guard `pending` fails and it returns `Blocked`, which is the run the model explored. |
| A second run of the use case on the same row (`OpenSettlement` twice on one claim) | Not excluded | Not excluded. Two runs both reach step 2 and create two settlements, the #80 bug. | Excluded. Step 1 needs the row to have no mark, and the mark is unique per row. |
| The process stops between the steps | The run is lost or half committed | The token is lost, and the claim stays at `negotiating` with no settlement. The model has no such state for an unfinished run. | The marks are stored, so the run resumes at the step the mark names. |
| A step's guard fails when its turn comes | No notion of a turn | The caller has no record of a pending run to retry | `Blocked` leaves the marks as they were. The obligation is to retry, never to abandon (rule 4). |
| A split use case raises an event (`CheckInPatient`) | The event can leave before step 2 commits | Correct | Only the last step's result carries the event. |
| A triggered split use case | One function per event | Correct | The first step takes the event value, one function per `<UC>_by_<Ev>`. The later steps are shared. |
| A condition on the use case, an API, or an arm | Every guard is read on the snapshot | Correct by placement | Each guard is on the step rule 5 names. |
| A row deleted before a later step | Not representable | The row is gone, and so is the run | The later step takes `E \| Deleted<EId>`, as the mark still matches. |
| `after(UC).assert` | Checked on the snapshot | Checked after the last step | Checked on the run's rows after the last step. |
| Runs of two different use cases on one row | Not excluded | Not excluded | Allowed: there is one mark per use case and row, as in the model. |
| A split the modeler did not intend (two APIs over one store, an isolated write) | Tempting, and it would make the code more atomic than the model | Same as above | The code keeps the split. The fix belongs in the model: put the writes under one API, run `check` and `verify` again, and generate again. The use case is then one action and one function. |

### Decision: C, one function per step, linked by stored run marks

A is wrong when the steps commit separately, and only the model can say they don't. It
also hides the boundary that `verify` checked interleavings across. B is right only while
the process stays up and nobody starts a second run: it gives up rules 2 to 4, and the
#80 bug in the samples is exactly that. C is the model's own semantics written as code, so
each case above behaves in code as it did in `verify`. The decision has these parts:

- **One function per action.** The name comes from the action's name (`Pay_1` gives
  `pay1`, `pay_1`, `Pay1`), so the collisions the transition system already resolves
  (`Pay_1_2` next to a use case `Pay_1`) are resolved once.
- **A run mark is a generated record, not a column of the entity.** For each entity a step
  before the last writes there is `<UC>RunMark { <entity>_id, step }`, so the entity types
  keep the shape DBML and JSON Schema describe. The obligations say that a mark is unique
  per row (rule 3) and is committed in the same transaction as the writes of the step that
  sets, advances, or clears it.
- **Step 1 takes `null` for each mark**, so the caller has to look the mark up. It
  returns the marks at `1`.
- **Step i > 1 takes the current rows and their marks** typed at `i-1`. If a row does not
  match its mark, or the step's own rows do not match through the key, that is a
  programming error and the function throws or panics. If a guard fails, the function
  returns `Blocked` with the guard's text and site, and the marks are unchanged. The last
  step returns the marks cleared, the raised events, and the result of the `after`
  check.
- **Nothing is compensated.** No generated function undoes a step. If the business needs a
  way out of a stuck run, it is a use case in the model, which is verified like any other.
- **The scaffold has one service stub per use case** with an entry point per step and a
  `resume<UC>` stub that reads the stored marks and calls the next step. These are the
  places the agent fills in.

TypeScript for `cross_order_payment_split` (it compiles under `tsc --strict`, and the two
wrong calls at the end are type errors):

```ts
/** Run mark of Pay on a Payment row (`Payment._run_Pay`). Unique per row (obligation O2). */
export type PayRunMark = { usecase: "Pay"; payment_id: PaymentId; step: 1 };
export type Blocked = { blocked: true; guard: string; site: string };

/** Pay_1, transaction 1 of 2 of Pay (CapturePaymentApi). */
export function pay1(payment: PaymentPending, mark: null):
    { payment: PaymentCaptured; marks: { payment: PayRunMark } } {
  void mark;
  return {
    payment: { ...payment, status: "captured" },
    marks: { payment: { usecase: "Pay", payment_id: payment.id, step: 1 } },
  };
}

/** Pay_2, transaction 2 of 2 of Pay (PayOrderApi). Reads the rows as they are now. */
export function pay2(
  earlier: { payment: Payment | Deleted<PaymentId>; mark: PayRunMark },
  order: Order,
): { order: OrderPaid; marks: { payment: null } } | Blocked {
  if ("id" in earlier.payment && earlier.payment.id !== earlier.mark.payment_id) {
    throw new Error("rdra-ish: the Payment row is not the row of this run");
  }
  if ("id" in earlier.payment && earlier.payment.order_id !== order.id) {
    throw new Error("rdra-ish: the Order is not the Payment's order");
  }
  if (order.status !== "pending") {
    return { blocked: true, guard: "Order.status == pending",
             site: "cross_order_payment_split.rdra:37:1" };
  }
  return { order: { ...order, status: "paid" }, marks: { payment: null } };
}

// @ts-expect-error step 2 cannot run without the mark step 1 returned
pay2({ payment: captured }, cancelledOrder);
// @ts-expect-error step 1 cannot run on a captured payment
pay1(captured, null);
```

In Rust, a step takes its marks by value and returns the next ones, so a mark cannot be
used twice. In Go, the marks are structs with the step as a typed constant.

<a id="agent-contract"></a>
## The agent contract

The output is one stream, in this order. Each part is marked by a comment line, so an
agent can split it into files wherever the project wants:

1. **Header.** The model path, the rdra-ish version, the bounds and the result of the
   `check` the generator ran, a note that `verify` checks at larger bounds, and a note
   that the output is not covered by compatibility.
2. **Obligations.** A list of what the verification assumed but the types cannot
   enforce, each under a stable name (the invariant or rule it protects where `check` and
   `verify` name one) and with the source site. For
   example: O1 `Unique_Settlement_Claim`, a unique key on `settlement.claim_id`; O2 one run
   mark per row and use case, committed with its step; O3 retry a `Blocked` step and never
   abandon its run; O4 publish raised events only after the last step commits; O5 load
   every related row a `.none` guard reads; O6 `MinCard_*`; O7 call the generated
   validator of each multi-entity rule before commit.
3. **Core.** Data types, state sums, transition functions, rule validators. Regenerating
   from the model replaces it. The agent does not change its signatures or bodies; when
   the core is wrong, the model is wrong.
4. **Scaffold.** One service stub per use case: load the rows, call the core, persist the
   rows and the marks, publish the events. Every body is a TODO that compiles. The agent
   owns the scaffold.
5. **Conformance tests.** Each `forbidden`, `invariant`, `@check`, and `after(UC).assert`
   as a property for `fast-check`, `proptest`, or `rapid`, run against the core and,
   through the scaffold's ports, against the finished services. When an agent edit breaks
   one, the code has left what was verified. As built, the core's part is the rule checks,
   and the finished service's part is [the conformance harness](#conformance), for Rust
   and Go. [TypeScript too, since #368.]

`--format json` prints the same intermediate form and obligations as data, for an agent
that wants to generate in another language or check its work against the list.

Note (2026-11-25): what was built differs. The obligations were numbered `O1`, `O2`, ... in
model order until #363, so adding a `relate` renumbered every later one. Each now has a stable
name built from model identifiers only: the invariant name where `check` and `verify` have one
(`Unique_Payment_Order`, `MinCard_*`, `Check_*`), and otherwise a new one (`Ref_Product_category_id`
for a reference, `UniqueColumns_*`, `OnDelete_*`, `Derived_*`, `OneTransaction`, `RunMarks_*`,
`CompleteRows_*`, `RuleChecks`, `NotMet_*`). `--format json` landed as the data of P0 of
[code-check.md](./code-check.md): the types, states, use cases, functions and obligations with
their model locations, not the whole intermediate form.

<a id="conformance"></a>
### The conformance harness

<!-- derived-from #the-agent-contract -->

Item 5 as built (#330). The rule checks and `codegen_equivalence.rs` show that the core does
what the model does. Nothing showed that the service an agent writes around the core does:
a service that writes the store directly, stores only some of the rows a function returns,
or runs a use case from a state the model refuses passes every other check.
`codegen --lang rust|go --conformance <FILE>` prints a port and a harness after the core,
and writes the traces the harness runs to `FILE`. [Since #368 `--lang ts` prints them too.]

**The port.** The service implements `ConformancePort`: `seed` empties the store and stores
the rows of an initial state, `snapshot` returns everything the store holds (each entity's
rows in its `<Entity>Row` or record type, each pending run mark as `<UseCase><Entity>Run {
id, step }`, each event a triggered use case reads), and two methods per action run the
use case by the ids of its rows, the ids of its run marks, its event, its inputs, and its
choices: `prepare_<action>` / `Prepare<Action>` loads the rows and returns what the
dispatcher (or the one function) returns, writing nothing, and `commit_<action>` /
`Commit<Action>` stores that result in one transaction (since #332; #330 had one `run_*`
method). A list of related rows is not an argument: the service loads it. Either method
returns an error when the use case does not run. The port is how the harness drives and
reads the service, not a persistence layer: the service still decides how it stores rows.

**The oracle.** The expected values come from BFS, never from the generated core. `traces`
in `codegen/conformance` explores the whole transition system at `check`'s bounds
(`Bounds::STATES`), from its initial states, and keeps each move of a use case with a printed
function. It picks the move's function with the same evaluator `codegen_equivalence.rs`
checks the functions with (`codegen::conformance::eval`), and reads every argument the move
leaves open from the successor: the id of a created row is its slot plus one, a chosen value
is the value the successor holds, a column `present` gets a default value. A greedy walk then
takes every move between the states the moves reach, starting a new trace (a fresh port,
seeded) when it runs out of moves where it stands. After each call the harness cuts the
store down to what the system tracks (an enum's value, null or set, an integer, a key's
parent id, a pending mark, a raise a triggered use case reads), sorts the lines, and compares
them with the successor's. In each state reached first, the traces also try every use case
over one row that the model does not run on a present row, where BFS has no move and the
functions allow none (a source state no function takes, a condition that fails); the call
must fail and change nothing. A use case that creates a row is never tried as refused: at
one row per entity, BFS has no free slot after the first, but a real store does.

The traces also try what [one transaction per function](#atomicity) rules out. In a reached
state with two moves A and B (B may be A again), when B leaves a row A writes in another
state than A read it in, deletes it, or creates the row A creates, and the model has no move
of A after B, an `I` record prepares A, runs B, and commits A: the commit must fail and leave
the store in B's successor. One such trace is written per pair of A's function and B's use
case.

**Where the traces live: a data file, not code.** The traces are a line-oriented text file
that the generated harness parses, and the code stream holds no trace. Three reasons:

- *Size.* At `check`'s bounds, ec-site has 189 moves, but clinic-ops, patent-ops,
  food-permit-ops and oidc-auth reach the 65,536-state cap with 177,000 to 309,000 moves
  (clinic-ops' traces are 178 MB). Code of that size would not be read, reviewed, or
  compiled in reasonable time, and `codegen_builds.rs` compiles every sample's output.
- *The code stays the code.* The core, the port and the harness are the same at any trace
  bound; `--conformance-states` changes only the file. The agent regenerates the traces at
  other bounds without a code diff, and a diff of the code still shows a change of the model.
- *No library.* Each line is a record of whitespace-separated tokens (`S`/`A` a state and
  its rows, `T` a trace, `R` a seed row, `G` seeding done, `C` a call and the state it
  reaches, `X` a call that must be refused, `I` a prepared call, the call run before its
  commit, and the state it reaches), so the harness parses it with the standard
  library of Rust and Go, as the rest of the output builds without one.

The cost is a second file the agent keeps beside the code and reads in its test; the first
line names the format, and the harness refuses another.

**What it proves.** On `tests/fixtures/codegen/conformance.rdra`,
`tests/codegen_conformance.rs` runs an in-memory service built on the generated core in Rust
and in Go (the Go one from outside the package). It passes. Each of four planted faults
fails at the call that shows it: Pay writing the tag itself (`paid_at` and `qty` differ),
Ship dropping the shipment the core returns (the shipment row is missing), Cancel
running from `shipped` (a refused call ran), and every commit writing without its condition
(a second Place, prepared before the first committed, commits over it).

**What it does not prove.** The harness checks what is stored, not how: a service that
computes the same rows without calling the core passes. A column the system does not track
(a string, a `present` timestamp's value) is not compared. The traces cover the bounds of
`check`, one row per entity, so a service that is wrong only with two rows passes. A BFS
stopped at the cap covers the states it reached and tries no refused call, and the header
says so; `check --code=run` and `codegen` print a coverage statement and a warning for it,
and `--deny-incomplete` fails the check
([code-check.md](./code-check.md#what-a-passing-run-claims)). A counted trigger, a comparison proposition, and the clock are not compared, and no
trace takes a tick, so a move after one is left out; the header lists these. An
interleaving is tried only where B changes the state of a row A writes, so a service that
does not hold a non-tag column a condition read, a row it reads and does not write, or a run
mark or event it consumes still passes; and the harness is one thread, so it shows the
condition on the write, not the locks of a real store.

<a id="atomicity"></a>
### One transaction per function

<!-- derived-from #use-cases-split-into-transactions -->

`check` and `verify` explore each use case, and each step of a split one, as one atomic
action: they never see a state where another use case ran between a function's read and its
write. A service that reads a row, calls the core, and writes the result in a later
transaction can write over another call's change, such as `paid` over `cancelled`, a move no
`transitions` has. Since #332 the header states the obligation, and the results carry what a
store needs to keep it:

- Each function runs in one transaction with its reads. The store holds what the function
  read until the write commits (`SELECT .. FOR UPDATE`, or serializable isolation).
- Each result carries, per row it writes, `<row>_expected` (`<Row>Expected` in Go): the
  source the row must still have, `Absent` for a row the function creates, `In { column,
  value }` (the tag column and the state it read) for a state type, and `Present` for a
  record. The store writes the row only where it still is that (`UPDATE .. WHERE id = ? AND
  status = ?`, an `INSERT` that fails on a present id), and when a write matches no row, it
  rolls back and returns `NotEnabled`, the answer the model gives in the state the other call
  left.

The condition covers a row another call created, deleted, or moved to another state, which
is the race the model's tag guards. It does not cover another column a condition read (Big
reads `qty`), a row read and not written (a parent, a `.has` list), or a consumed run mark or
event; the held reads keep those, and the obligation says so.

### Committing a result

<!-- constrained-by #one-transaction-per-function -->
<!-- constrained-by #use-cases-split-into-transactions -->

Three services were built on the output in November 2026: a Rust one on SQLite, a Go one on
SQLite, and a TypeScript one on `node:sqlite`. Each wrote the same glue by hand: one entry
per result type that lists the rows the result writes, the run marks it stores, and the
events it raises. The Rust service wrote about 160 lines of macro tables, the Go service
used a regular-expression script and reflection, and the TypeScript service wrote about 550
lines. The last step of a split use case did not return the marks it ends, so the
obligation "delete the marks it took" could not be met from the result alone. Since this
change the output gives this list as data. The data has the same shape in all three languages, and
each language prints it in its own idiom.

**The values.** A result is committed as four parts. The model has one type of each, and
each type has one case per entity, per run mark, or per event:

| Concept | Rust | Go | TypeScript |
|---|---|---|---|
| A row of any entity, as the store holds it | `enum StoredRow { Order(OrderRow), Account(Account), .. }` | `StoredRow` interface, which every `<Entity>Row` and entity record implements | in `RowWrite` |
| The id of a row of any entity | `enum StoredId { Order(OrderId), .. }` | `StoredId` interface, which every id type implements | in `RowWrite` |
| A row write | `enum RowWrite { Put { id, row, expected }, Delete { id, expected } }` | `struct RowWrite { Entity, Id, Row, Delete, Expected }` | union on `kind` and `entity`: `{ kind: "put"; entity: "Order"; id: OrderId; row: OrderRow; expected } \| ..` |
| A run mark: use case, entity, id, step | `enum RunMark { OpenSettlementClaim { id, step }, .. }` | `struct RunMark { UseCase, Entity, Id, Step }` | union `{ usecase; entity; id; step }` |
| A mark write | `enum MarkWrite { Put(RunMark), Delete(RunMark) }` | `struct MarkWrite { Mark, Delete }` | `{ kind: "put" \| "delete"; mark: RunMark }` |
| An event of any type | `enum RaisedEvent { EvShipped(EvShipped), .. }` | `RaisedEvent` interface, which every event struct implements | union of the event types |
| What one commit stores | `struct Writes { usecase, rows, marks, events }` | `struct Writes { UseCase, Rows, Marks, Events }` | `Writes = { usecase, rows, marks, events }` |

A put stores the row: it inserts the row when `expected` is `Absent`, and otherwise updates
it where the row is still `expected`. A delete removes the row where it is still `expected`.
An entity with no one-column key has no id. Its `StoredId` case holds nothing (Go: `Id` is
nil, TypeScript: `id: null`), because the result cannot name the row. A mark put with step 1 inserts the mark, and the row must have
no mark of that use case (rule 3). A mark put with a later step advances the row's mark from
the step before. A mark delete removes the mark at the step named. The run ends on that row.

**The accessor.** Every result type and every outcome type of a dispatcher returns its
`Writes`. Rust has the trait `IntoWrites` (`fn into_writes(self) -> Writes`). Go has the
method `Writes() Writes` and the interface `Committable`. TypeScript has a companion object
per type: `PayResult.writes(r)` and `CancelOutcome.writes(o)`. The list is complete. It holds
every row the result writes or deletes, every mark it stores, advances, or ends, and every
event it raises, in this order: rows in the order of the result's fields, then marks, then
events. An outcome returns the list of the case that ran. A TypeScript outcome is a union with
no tag. The cases of one action write the same rows, marks, and events, and differ only in
the states, so its accessor reads the fields of the union and needs no tag.

**The last step ends its marks (G4).** A step of a split use case returns each mark it
takes and does not advance: `ended_<row>_mark`, with the type of the mark it took.
`into_writes` lists the mark as a delete. Its `step` is the step of the mark that was taken.
A step before the last advances its marks and lists each one as a put.

**The commit helper.** The service supplies a store with three operations. The generated
function then writes a `Writes` in order and stops at the first conditional write that
matches nothing:

- `write_row(RowWrite) -> bool`: false when the row is not `expected`.
- `write_mark(MarkWrite) -> bool`: false when the mark is not what the step needs.
- `record_event(RaisedEvent)`.

Each operation can fail with the store's own error. The helper is `commit_writes(store,
result)` in Rust (trait `CommitStore` with an associated `Error`, result
`Result<(), CommitFailed>`, where `CommitFailed` is `NotEnabled` or `StoreFailed`),
`CommitWrites(store, result) error` in Go (`*NotEnabled`, or `*StoreFailed` wrapping the
store's error), and `commitWrites(store, writes)` in TypeScript (async, generic in the
store's error, `Result<void, NotEnabled | StoreFailed>`). The helper does not begin or end
the transaction. The caller runs it inside the transaction of the call and rolls back on an
error, as [one transaction per function](#one-transaction-per-function) requires. Both
cases are cases of every `<UseCase>Error` ([Not found and store
failures](#not-found-and-store-failures)): Rust converts `CommitFailed` with `From`, Go
wraps the detail, and TypeScript returns it as it is. (Before that change the helper
returned the store's error itself: `CommitFailed::Store(E)` in Rust.)

**Names.** Each new type and helper name is claimed after the model's names. A model type
with the same name gets the trailing `_`, as the other derived names do. In Go a result field
named `Writes` would clash with the method, so the method becomes `Writes_` on every result
of that model.

**What changes.** The change adds items only. The new types and the accessors are added
after the dispatchers, and the result of a step that ends marks gains its `ended_*` fields.
No existing name, field, or function changes. The part adds about 15% to the output:
clinic-ops gains 3,898 of 25,212 lines in Rust, 2,344 of 21,770 in Go, and 2,592 of 17,364 in
TypeScript; ec-site gains 566, 336, and 300 lines. `tests/codegen_commit.rs` shows a service
in each language that commits every result through the helper and passes the conformance
harness, and fails it when its store keeps a mark the last step ends.

### Not found and store failures

<!-- constrained-by #committing-a-result -->

A `<UseCase>Error` had three cases: `NotEnabled`, `Rejected`, and `InvalidArgument`. A
service that loaded a row by id and found none had no case for it, nor did a store that
failed (a lost connection, a constraint the model does not know). Both services built on the
output in November 2026 answered `NotEnabled`. The HTTP layer then could not tell 404 from
409, and a store failure became a 409 or a panic.

The owner decided (2026-11-28) to add two cases to every `<UseCase>Error` in all three
languages, by default and with no opt-in flag:

- `NotFound { usecase, entity, id }`: a row the call names by id does not exist when it is
  loaded. `id` is the id as text, so one type serves every entity. The answer comes at load
  time. A commit whose conditional write matches no row stays `NotEnabled`, because the row
  may have been deleted or moved, and the model answers both the same way. A run mark or an
  event that is missing is `NotEnabled`: the row exists and is not in that run, or the
  trigger has not happened.
- `StoreFailed { usecase, cause }`: the store failed. `cause` is the store's error. In Rust
  it is an `Arc<dyn Error + Send + Sync>` that `source()` returns, so `<UseCase>Error` keeps
  `Clone`, `PartialEq` and `Eq` (two `StoreFailed` are equal when they carry the same cause),
  and `StoreFailed::new(usecase, e)` takes any error that converts into `Box<dyn Error + Send
  + Sync>`. In Go `*StoreFailed` has `Unwrap() error` returning `Cause`, so `errors.Is` and
  `errors.As` reach the store's error. In TypeScript `cause` is `unknown`.

Every use case gets both cases, even one that loads no row by id (a create with no parent),
so a `match` or `switch` and an HTTP mapping read the same for every use case: `NotFound`
404, `NotEnabled` 409, `Rejected` 422, `InvalidArgument` 400, `StoreFailed` 500.

The generated functions take rows already loaded, so they cannot tell a missing row from a
row in the wrong state, and never return `NotFound` or `StoreFailed`. A service's `prepare`
returns `NotFound` from its load and `StoreFailed` from its store; `commit` returns
`StoreFailed` from its store. The commit helper is the one generated code that meets the
store, and it returns `StoreFailed` (see below).

**The commit helper.** `CommitFailed::Store(E)` duplicated what `StoreFailed` now says, so it
is gone. In Rust `CommitFailed` is `NotEnabled(NotEnabled)` or `StoreFailed(StoreFailed)`,
with no type parameter, and converts into every `<UseCase>Error` by `From`, so
`commit_writes(store, result)?` returns the use case's own error. The store's `Error` must
convert into `Box<dyn Error + Send + Sync>`, which every `Error + Send + Sync + 'static` type
and `String` do; a service that needs the typed error back reads it with
`cause.downcast_ref`. In Go `CommitWrites` returns `*NotEnabled` or `*StoreFailed` wrapping
the store's error. In TypeScript `commitWrites` returns `Result<void, NotEnabled |
StoreFailed>`, which every `<UseCase>Error` union holds.

**The conformance harness.** The first trace calls, once, each use case whose call names one
existing row and no run mark or event, with an id no row has: `X NotFound <call>`. The
service must refuse it with `NotFound`. The kind check of #452 stays strict both ways:
`NotEnabled` does not pass a `NotFound` row, and `NotFound` does not pass a `NotEnabled` row,
so a service that answers every missing row with `NotEnabled` fails, as do the two services
above. These rows do not depend on the BFS, so a capped run tries them too, and they do not
count for `never refused`. A call that names a run mark or an event is left out, because
the service may refuse it for the missing mark or raise first. A `StoreFailed` on any row
(`C`, `X` of any kind, the prepare or the late commit of an `I` row) fails the run as a
failure of the service, not as a refusal. The traces format is 5.

**What breaks.** Adding a case breaks every exhaustive match on the error in Rust and
TypeScript: a `match` with no wildcard, or a `switch` that ends in `assertNever`. Go code
that switches on `Kind` keeps compiling. The CHANGELOG says what to add. `<UseCase>Error` and
the Rust and Go output are outside the compatibility contract, so this needs no major
release. The Verus output (`--verus`) keeps its three cases: it is a separate experimental
crate, and a cause of `dyn Error` inside `verus!` needs its own design.

## Architecture

```
SemanticModel ──► data types ─┐
                              ├─► DomainModel (language-neutral) ─► TS printer
TransitionSystem ─► functions ┘                                   ├► Rust printer
rules ────────► refinements ──┘                                   └► Go printer
```

- `DomainModel` is plain data in `rdra-ish-emit` (`codegen/ir.rs`): `TypeDecl::{Record,
  Sum, Enum, Id}`, `FnDecl { params, guards, effects, result }`, and the names, already
  resolved and checked for collisions. The printers only print it, as the TLA+ printer
  prints the transition system, so a choice made once (a refinement, a name) is the same
  in every language.
- Naming is decided in the IR: types PascalCase, Rust fields as written, Go fields
  exported with a `json` tag naming the column, keywords escaped (`r#type`, `Type_`). Two
  generated names that collide (an entity `OrderPaid` beside the variant `OrderPaid`) are
  a generator error naming both declarations.
- The CLI adds a subcommand, `rdra-ish codegen <inputs> --lang ts|rust|go`, that prints
  the stream to stdout, as the removed `states --format typescript` did. `-o <file>` is optional, and
  for Go `--package` names the package. Both have landed. Later phases add these: `--format json` prints the intermediate form and the obligations. `--part
  types|core|scaffold|tests` limits the stream, `--buc` scopes it as the removed
  `typescript-states` did, and `--scalars` names the scalars module. `export --kind
  typescript-states` and `states --format typescript` were removed on 2026-11-24 (see
  [The state unions before codegen](#the-state-unions-before-codegen)). [`--format json`
  landed with P0 of #363 (#366); `--part`, `--buc` and `--scalars` have not.]
- The generator runs the same `check` first and refuses when it fails, because the code
  would then encode a model that was not verified. `cross_order_payment_split` is refused
  this way. `--allow-failing` prints anyway and lists each failing invariant as an
  obligation the code does not meet.
- The lifecycle gate (`crates/rdra-ish-cli/src/lifecycle_gate.rs`) classifies the
  outputs by what they read. Data types read no reachability and write, as DBML does. State sums and functions refuse when the lifecycle has errors. Under
  `--refine reachable` they also refuse when a cone stopped at the state cap, because an
  undecided cone gives no complete set of patterns.

Note (2026-11-21): what was built differs. `ir.rs` has one declaration type per kind
(`EnumDecl`, `IdDecl`, `RecordDecl`, `SumDecl`), and `FnDecl` holds rows, inputs, argument
checks, conditions, and outputs. `codegen` does not go through the lifecycle gate: it runs
`check` at `Bounds::STATES` and refuses when it reports an error, unless `--allow-failing`.
`--refine reachable` has not landed.

## How we know it is right

The project's rule is that every output is checked against the one semantics, on every
sample. For this generator:

- **It compiles.** `crates/rdra-ish-cli/tests/codegen_builds.rs` runs `tsc --strict`,
  `rustc -D warnings`, and `go vet` on the output of every `samples.txt` root (landed for
  the data layer). Later phases add the exhaustiveness linter and the scaffold and tests.
  The dev shell provides TypeScript and Go, and `RDRA_REQUIRE_CODEGEN_TOOLS=1` (set by `ci.yml`) makes a missing compiler fail the test instead of skipping it.
- **It agrees with the transition system.** A test in the style of `engine_equivalence`
  runs the generated TypeScript (Node is in the flake) on every edge BFS explores at
  1 instance: it concretizes the pre-state, calls the function, abstracts the result, and
  compares it with the BFS successor, and it checks that a guard the generated function
  rejects is a guard BFS has. Rust and Go print the same IR, so per-printer snapshot tests
  suffice for them.
- **The split runs agree.** On the three split samples, the test also runs interleavings
  of steps and other actions, and checks that `Blocked` is returned exactly where BFS has
  no successor for the step.
- **The harness tells a faulty service apart.** `crates/rdra-ish-cli/tests/codegen_conformance.rs`
  runs a reference service and three planted faults through the Rust and Go harness
  ([The conformance harness](#conformance)), and `codegen_builds.rs` compiles the harness of
  every sample.
- **The keys agree.** `key_facts` (`crates/rdra-ish-cli/tests/key_facts.rs`) gains the
  generated key fields as one more consumer of the `ForeignKey` fact set.

Note (2026-11-25): what was built differs. `codegen_equivalence.rs` checks the IR, not a
printed language, against every BFS edge at 1 instance. Snapshots do not catch a Rust or Go
printer bug that still compiles, so `codegen_differential.rs` (#334) runs the printed Rust
and Go on the same edges. A driver generated from the IR builds each function's rows from
the BFS pre-state through `from_row`, calls the function, and returns its result as JSON.
Each column BFS tracks must hold the successor's value, and each column BFS does not track
must hold what the IR writes. In the converse scope of `codegen_equivalence.rs`, a row in
the function's source state with no BFS move must get `Rejected`. The dispatchers and
`NotEnabled` are not run yet, and neither is TypeScript.

## Phases

0. **Fix `typescript-states`.** Landed; the export was removed later. An Int axis prints the column's type, not a literal, and
   a present value with no `present:<type>` prints the column's declared type instead of
   `unknown`. Independent of the rest, and small.
1. **Data types.** Landed. Variations, entities as records, id newtypes, keys, `@null`, DTOs,
   scalars, in three languages, with the compile check. No lifecycle read. The `codegen`
   subcommand lands here, labelled experimental in `--help` and the CLI reference, and it
   joins the "What is not covered" list of [compatibility](../compatibility.md) with its
   flags and its output.
2. **State sums.** Landed: one variant per state, declared refinement, and the `check`
   gate with `--allow-failing`. Two deviations from the plan above. A variant is printed
   per declared value of the tag column, not per state of the state model: a superset is
   always sound for a type, and a value with no witness is already a `check` finding. The
   tagged JSON of the Rust and Go variants and the candidate-rule listing in `next` move
   to phase 3, with the scaffold that reads and writes rows.
3. **Transition functions and the agent contract.** The functions, the rule checks, the
   row types, the dispatchers, and the candidate rules have landed. The candidate
   rules are in the `codegen` header, not in `next`: `next`'s JSON is covered by
   compatibility and `codegen` is not, and the agent reads the header. The scaffold landed as two pure pieces instead of service stubs with persistence
   ports: a flat row type per state type with checked conversions both ways, and a
   dispatcher per use case over the entity types. A store's or a framework's ports differ
   per project, so the agent writes the service around them; what it cannot get wrong (which
   function runs from which states, which rows a state rules out) is generated. The tagged
   JSON is the row type: its columns, with the tag inline, are the JSON shape in all three
   languages (Go with `json` tags); the Rust output adds no serde dependency, so the agent
   derives it. `tests/codegen_behaviour.rs` runs the printed code of a fixture in each
   language.
   The rule oracles landed as rule checks in the generated code, not as property tests for
   `fast-check`, `proptest`, or `rapid`: the output then needs no test library to build,
   and the agent writes the tests that call them. A check is read from the invariant
   `check` and `verify` check, per state type, so a rule the state type guarantees drops
   out; `codegen_equivalence.rs` checks that no row of a reached state fails one. It does
   not yet check the converse, that a row breaking a rule fails its check, since a model
   `check` passes reaches no such row. What landed differs from the plan in four ways. A use case gets one function per
   combination of the source states of its rows, not one function over a union of sources:
   each function's types are then concrete in all three languages, and Rust and Go need no
   per-use-case input enums. A use case using vocabulary not printed yet gets no function
   and a line in the header, so the output stays buildable and says what it leaves out (on
   the samples: 436 functions, 67 use cases listed). A row that breaks a rule the verified
   model keeps makes the function throw or panic instead of returning a value of a type the
   rule rules out. And the equivalence test is in Rust over the IR, not in Node over the
   printed TypeScript: `crates/rdra-ish-cli/tests/codegen_equivalence.rs` checks every move
   BFS makes on the samples `check` passes against the functions, and that a function over
   one row allows no move BFS lacks. It found one bug before landing: a created row that a
   transition of the same use case moves started at its start state. The printers print the
   IR the test reads, and `codegen_builds.rs` compiles them. The plan was, for use cases of one transaction
   group: source narrowing, `holds` guards without quantifiers, effects, choices, raised
   events, the obligations, the scaffold, the conformance tests, and the equivalence
   test. The conformance tests come here, not later, because they are what keeps an
   agent's edits within what was verified.
4. **Use cases split into transactions.** Landed, as [decided](#split), with three
   deviations. Step 1 takes no `null` per mark: whether a row holds a mark is in the store,
   not in a value the caller can pass, so it is an obligation, beside "one mark per row"
   and "never abandon a run". A later step takes the earlier rows only as their marks: in
   the samples a later step reads an earlier row mostly for its key, which the mark carries,
   and a row deleted between the steps then needs no `Deleted` type. A later step whose
   guard reads an earlier row takes that row too, as it is now, and checks that its mark
   names it ([choice 1](#choice-1-a-key-between-two-rows-of-an-earlier-step)): a key
   between two earlier rows is a condition, a key between an earlier row and a row the step
   binds itself is checked as any other argument row's key. A step
   that does not run returns `NotEnabled` or `Rejected`, which for a later step means
   "retry", not a new `Blocked` type, and `resume<UC>` is left to the code that reads the
   stored marks. `codegen_equivalence.rs` checks every step BFS runs, the mark each step
   leaves in the run var, and every key a function sets against the owner var; the
   interleavings are BFS's own, since it explores every action between two steps. No
   sample or fixture has a condition of a later step on an earlier row's columns, so that
   shape is untested.
5. **The rest of the action vocabulary.** Triggered use cases have landed: an event
   carries the id of each row its raise marks (the `Raised` vars of the event order), and a
   triggered use case takes the event as an argument and checks its row against it, so it
   cannot run without the event, as the 2.0 event order requires. A row the use case reads
   only through the event is an argument in its flat row type. A key between two rows of
   an earlier step is a condition on those rows, which the step takes as they are now. A
   choice among arms is
   one function per target state, not a choice argument as planned: the target fixes the
   result's state type, so a function per target keeps every type concrete, and the
   dispatcher takes the choice. A `.when` on an arm folds into a condition of that function:
   its guard reads the successor's tag, which the function's target fixes. A second
   lifecycle column is a branch on its value, its `from` states a `TransitionFrom`
   condition; a choice among its arms is one function per target too. A `.has` / `.none` ranges over a list of the related
   entity's rows, an input the caller fills with every present row, and the function keeps
   those the key links; an obligation in the header says the list must be complete.
   That left 19 use cases on the samples (11 on the four domain samples, 8 on oidc-auth);
   #335 prints all of them ([Use cases without a function](#use-cases-without-a-function)).

### Use cases without a function

Status (2026-11-26): all 19 printed. The owner decided the two choices below on #335.

A use case gets no function when one of its actions uses a part of the vocabulary the
generator does not print. The header lists it with the reason, and the agent that finishes the
output writes it freehand, out of reach of `check --code`. The reason the header gives is the
first construct the generator met, so the table groups the 19 use cases of 3.0.0 by every
construct that kept them out, as `TransitionSystem` builds their actions. A use case with two
constructs is listed under the first and named again under the second.

| What kept it out | Use cases | Printed as |
|---|---|---|
| A created row's optional key that no step links: the action assigns it null | clinic-ops `RegisterPerformedProcedure`; food-permit-ops `SubmitCounterApplication`, `ReceiveOnlineApplication` (also in `CreateCharge`, `ApplySelectedTreatmentFee`, `StartDeviceAuthorization`, `RequestAuthentication`) | The key column is null in the created row |
| A condition that reads the key of the row the step creates, which the step sets to a parent it binds (`.along(Created, Parent)`) | clinic-ops `CreateCharge`; oidc-auth `StartBackchannelAuthentication`, `StartDeviceAuthorization`, `RedeemCode` | The read folds to the parent row: the condition is the parent's state. `RedeemCode` binds one Grant per key, so the condition asks that the three parents be the code's grant |
| A condition or a `.when` on a value the caller chooses for the row the step writes | clinic-ops `ApplySelectedTreatmentFee`; patent-ops `DraftResponse`; food-permit-ops `ProposeDisposition`; oidc-auth `RequestAuthentication` | The condition reads the input the choice already was (`caseScope === "free"`). A nullable column chosen present or null (`RequestAuthentication`'s `nonce`) is a nullable input |
| `links(UC, Child, Parent)` on an existing row | oidc-auth `ApproveDevice`, `AuthenticateEndUser`, `ReuseSession`, `SelectAccount` | The condition `Links_<Child>_<role>`: the child's key is null (`relinks`: set; `unlinks`: set, to the bound parent when the step writes one). The function sets the key to the parent's id (`unlinks`: null) |
| A copy, `sets(UC, E.column == F.source)` | food-permit-ops `RegisterPermit`; insurance-claim-ops `PayClaim` | The column takes the source row's value. A copy between two types, or from a nullable column into one that is not, stays unprinted |
| A key between two rows an earlier step wrote | clinic-ops `CheckInPatient_2` | The step takes both rows as they are now, beside their marks, checks each is the row its mark names (`InvalidArgument`), and the key between them is the condition `Key_<Child>_<role>` ([choice 1](#choice-1-a-key-between-two-rows-of-an-earlier-step), option B) |
| A choice among arms of a second lifecycle column | insurance-claim-ops `DetermineFault` | One function per target (`DetermineFaultFromInvestigatingToAtFault`); the dispatcher takes the target as `<row>_<column>_to` ([choice 2](#choice-2-a-choice-among-arms-of-a-second-lifecycle-column), option A) |

One more construct had no use case on the samples behind it: a guard that compares a nullable
Enum or Bool column with a value (`tests/fixtures/codegen/nullable_enum_bool.rdra`). Under
#359 null satisfies no comparison and every `!=`, so it prints as `x == Some(v)` (Rust),
`x != nil && *x == v` (Go) and `x === v` (TypeScript), and its negation admits null. A column
the source state keeps null satisfies no comparison either, so the comparison folds to false.

The printed functions keep the guarantees of the others: `codegen_differential.rs` runs every
BFS move of the samples through the Rust and Go functions, and `codegen_equivalence.rs` checks
that a move picks one function. A condition on an input is not read on a BFS state
(`conformance::eval` leaves it undecided); the differential run passes the input the
successor holds, so the condition is checked there.

#### Choice 1: a key between two rows of an earlier step

<!-- constrained-by ../state-derivation.md#runs -->

`CheckInPatient` step 1 checks the Appointment in and updates its PatientAccount; step 2
creates the EligibilityCheck. Step 2 holds the two rows only by their run marks, which carry
ids, and `verify` guards it with "the Appointment still belongs to the PatientAccount" (an
`Owner` guard between two `Earlier` params). The function has no key to compare. The options:

- **A. The Appointment's mark carries its key** (`CheckInPatientAppointmentMark1 { id,
  patientaccount_id }`), and step 2 compares it with the PatientAccount mark. The mark is the
  key as step 1 read it, and a `relinks` between the steps would not show. No use case on the
  samples relinks an Appointment, but the model does not rule one out.
- **B. Step 2 takes the Appointment and the PatientAccount rows as well**, as they are now,
  and checks the key between them. This is what `verify` reads (rule 2 of
  [What verify assumed](#what-verify-assumed): the rows as they are now), at the cost of two
  more arguments and a read the store must hold in the step's transaction.
- **C. An obligation** ("the marks of one run name rows that are still linked") and no check.

Recommendation: B. It is the only option that checks what `verify` checked, and the
`OneTransaction` obligation already covers holding the read rows.

Decision (owner, on #335): B. `checkInPatient2` takes `appointment: AppointmentRow` and
`patientAccount: PatientAccountRow` before the marks. A row that is not the one its mark names
is `InvalidArgument`; an Appointment no longer on the PatientAccount is `Rejected` by
`Key_Appointment_PatientAccount`, which for a later step means "retry". A later step takes an
earlier row only where a guard other than the run mark reads it.

#### Choice 2: a choice among arms of a second lifecycle column

`DetermineFault` moves `ClaimCase.liability` (not the state tag) from `undetermined` to
`at_fault` or `no_fault`. A choice among arms of the tag is one function per target, since
the target fixes the result's state type; a second column fixes no type. The options:

- **A. One function per target** (`DetermineFaultToAtFault`, `DetermineFaultToNoFault`), as
  for the tag, and the dispatcher takes the target. Consistent with the tag; the number of
  functions grows with the arms.
- **B. One function with the target as an input** of the column's enum type, and
  `InvalidArgument` for a value no arm from the row's value reaches. One function, but an
  input type wider than the choice, which Rust and TypeScript then check at run time.

Recommendation: A. It keeps the rule that a function's argument types admit only what the
model allows, and the dispatcher's choice argument already exists.

Decision (owner, on #335): A. `DetermineFault` is six functions, one per source state and
target (`DetermineFaultFromInvestigatingToAtFault` .. `DetermineFaultFromAssessedToNoFault`),
and `runDetermineFault` takes `claimCaseLiabilityTo`. The `TransitionFrom` condition keeps the
`from` values of the arms to the function's target.

## Decisions

The maintainer decided these on 2026-11-07:

1. **Scope and stability.** The generator is in scope as an experimental extra. Neither
   the subcommand nor its output is covered by compatibility (see
   [Goals and non-goals](#goals-and-non-goals)). On 2026-11-24 the owner narrowed this:
   the TypeScript types (scalars, ids, value sets, state types, and row types) are covered
   by [compatibility](../compatibility.md#the-typescript-types-of-codegen), and the rest
   stays experimental.
2. **Use cases split into transactions.** One function per step, linked by stored run
   marks ([the decision](#split)).
3. **Default refinement.** Declared refinement. It is sound regardless of the bounds, and
   it pushes the modeler to state rules. Reachable refinement stays opt-in, after the
   measurement [State sums](#state-sums) names.
4. **Rust encoding.** An enum of per-state structs, with transitions that consume their
   source. Typestate (`Order<S>`) was not chosen: "an order in any state" would still
   need an enum or a trait object.
5. **Go nullables.** `*T`, which `encoding/json` reads as it is.
