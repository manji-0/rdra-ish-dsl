# Algebraic domain types: projecting the model into TypeScript, Rust, and Go

<!-- constrained-by ../vision.md -->
<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../state-derivation.md -->
<!-- derived-from ../language-reference.md#data-layer -->

Status: proposal, for the maintainer to decide. Nothing here is implemented.

The model already states the shape of the domain: the values an actor chooses
(`variation`), the records the system keeps (`entity`), the keys between them (`relate`),
the states a record moves through (`transitions`), what each use case reads, requires, and
writes, and which combinations must never occur (rules). That is exactly what an
algebraic domain model in code states: product types, sum types, newtypes, and total
functions between them. This note proposes an export that prints those types and
functions for TypeScript, Rust, and Go from the one model, so the code and the reviewed
model cannot drift apart.

## What exists today

`export --kind typescript-states` (`crates/rdra-ish-emit/src/typescript.rs`) prints one
discriminated union per entity from the reachable patterns `states` derives. It is the
only code output, and running it on the samples shows why it cannot be the base of a code
generator as it stands:

| Observation (command) | Problem for code |
|---|---|
| `states skills/rdra-ish-verify/samples/int_stock.rdra --format typescript` prints `stock: 1` and `stock: 3` | An Int axis becomes a literal type taken from the bounded domain `0..5`. Real stock values are rejected by the type. |
| `export samples/ec-site --kind typescript-states` prints `processed_at: unknown` for `Payment` | A present value with no `present:<type>` in `sets` loses the column's declared type (`DateTime`). |
| The same output has two variants with `status: "authorized"` | One tag, several shapes. TypeScript accepts it; a Rust enum or a Go sealed interface needs one variant per tag. |
| The same output types `Order.delivered_at` as `null` in every variant, `delivered` included, and `Payment.gateway_ref` as `null` throughout | ec-site never `sets` those columns, so no reachable state holds a value. The type turns a gap in the model into a rule of the code: a delivered order can never record its delivery time. |
| Only state axes appear | `total`, `ordered_at`, the keys, and every column no rule reads are missing, so the union describes states, not records. |

The first two are bugs in the existing export and can be fixed on their own (see
[Phases](#phases), phase 0). The last three are design questions this note answers.

## Goals and non-goals

Goals:

- One export per language prints the **data types** (variations, entities, keys, DTOs),
  the **state sums** (one variant per state of an entity's state model), and the
  **transition functions** (one per use case), from the same intermediate form, so the
  three languages agree by construction.
- A generated function does exactly what the model says the use case does: it requires
  the source states and the condition guards, writes the effects, and returns the raised
  events. Values the model leaves abstract (`present`, a chosen variation value, a choice
  among arms) are the caller's input. The business computation stays in hand-written code
  that calls the generated core.
- Generated code compiles without a runtime library from rdra-ish.

Non-goals:

- Persistence, repositories, DDL. [vision.md](../vision.md) puts physical design out of
  scope, and DBML stays the last data output.
- HTTP clients and servers. OpenAPI generators (`openapi-typescript`, `progenitor`,
  `oapi-codegen`) already read `export --kind openapi`; this export prints the DTO types
  only, so a handler can be typed by them.
- Round trip. The output is generated only. There are no protected regions; a project
  wraps or imports the generated module and never edits it.

This widens what rdra-ish produces from review and verification artifacts to code a
build depends on. That is a scope decision the maintainer records in
[vision.md](../vision.md#decisions) before phase 1 lands.

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
| `@derived(expr)` | a function `orderTotalWithTax(o)` printed from the typed expression, not a stored field | a method | a method |
| `@check(expr)` | a check in the smart constructor `parseOrder` | `TryFrom` | `func NewOrder(..) (Order, error)` |
| `dto` | a record | a struct | a struct |

The scalars module holds the four types whose representation a project picks (a decimal
library, a time library). The export prints a default (`string` aliases in TypeScript,
newtypes over `String` in Rust and Go) and a flag, `--scalars <module path>`, makes the
generated file import the project's own instead.

### State sums

An entity with a lifecycle column is a sum with one variant per state of its state model,
the same set of states the state diagram draws (a `transitions` endpoint or a value a
creating use case sets). An entity with no lifecycle column is a plain record. The tag is
the column `typescript-states` already picks: `status`, else a `*_status` column, else the
first lifecycle column by name. Every other lifecycle column is a field typed by its own
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
every reachable pattern with that tag agrees on it, as `typescript-states` does today. It
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
  an export error naming both declarations.
- The CLI adds `export --kind ts-domain | rust-domain | go-domain` with `-o` as the output
  path, `--buc` scoping as `typescript-states` has, `--scalars`, and for Go `--package`.
  `typescript-states` stays as it is: its kind name is covered by
  [compatibility](../compatibility.md).
- The lifecycle gate (`crates/rdra-ish-cli/src/lifecycle_gate.rs`) classifies the
  outputs by what they read. Data types read no reachability and write with the note, as
  DBML does. State sums and functions refuse when the lifecycle has errors. Under
  `--refine reachable` they also refuse when a cone stopped at the state cap, because an
  undecided cone gives no complete set of patterns.

## How we know it is right

The project's rule is that every output is checked against the one semantics, on every
sample. For this export:

- **It compiles.** CI runs `tsc --noEmit`, `cargo check`, and `go vet` with the
  exhaustiveness linter on the output of every `samples.txt` root. The flake already has
  Node and Rust; it gains Go.
- **It agrees with the transition system.** A test in the style of `engine_equivalence`
  runs the generated TypeScript (Node is in the flake) on every edge BFS explores at
  1 instance: it concretizes the pre-state, calls the function, abstracts the result, and
  compares it with the BFS successor, and it checks that a guard the generated function
  rejects is a guard BFS has. Rust and Go print the same IR, so per-printer snapshot tests
  suffice for them.
- **The keys agree.** `key_facts` (`crates/rdra-ish-cli/tests/key_facts.rs`) gains the
  generated key fields as one more consumer of the `ForeignKey` fact set.

## Phases

0. **Fix `typescript-states`.** An Int axis prints the column's type, not a literal, and
   a present value with no `present:<type>` prints the column's declared type instead of
   `unknown`. Independent of the rest, and small.
1. **Data types.** Variations, entities as records, id newtypes, keys, `@null`, DTOs,
   scalars, in three languages, with the compile check. No lifecycle read.
2. **State sums.** One variant per state, declared refinement, tagged JSON in all three,
   and the candidate-rule listing in `next`.
3. **Transition functions** for use cases of one transaction group: source narrowing,
   `holds` guards without quantifiers, effects, choices, raised events, and the
   equivalence test.
4. **The rest of the action vocabulary:** `.has` / `.none`, `now`, triggered handlers,
   links, and use cases split into transaction groups.
5. **Rules as test oracles (optional).** Print each `forbidden`, `invariant`, `@check`,
   and `after(UC).assert` as a property for `fast-check`, `proptest`, or `rapid`, so a
   hand-written service that calls the generated core can be tested against the rules
   the model verified.

## Open questions for the maintainer

1. **Scope.** Does rdra-ish print code a build depends on? If yes, the decision goes into
   [vision.md](../vision.md#decisions).
2. **Default refinement.** Declared (recommended: sound regardless of bounds, and it
   pushes the modeler to state rules) or reachable (tighter, but bounded, and it
   codifies gaps such as ec-site's `delivered_at`).
3. **Rust encoding.** An enum of per-state structs with consuming transitions
   (recommended), or a typestate `Order<S>` with marker types. Typestate makes a
   transition a method but turns "an order in any state" into a trait object or an enum
   anyway.
4. **Go nullables.** `*T` (recommended: what `encoding/json` reads) or a generic
   `Option[T]`.
5. **A split use case.** One function per transaction group, linked by a run token the
   first returns and the next takes (recommended: the atomicity boundary stays visible),
   or one function that hides the steps.
6. **Stability.** Are the generated names covered by [compatibility](../compatibility.md)?
   Recommended: not until one minor release after phase 3, and then the type, variant,
   and function names, but not the formatting.
