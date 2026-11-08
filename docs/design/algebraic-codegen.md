# Algebraic domain types: projecting the model into TypeScript, Rust, and Go

<!-- constrained-by ../vision.md -->
<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../state-derivation.md -->
<!-- derived-from ../language-reference.md#data-layer -->

Status: accepted in scope on 2026-11-07 as an experimental extra
([vision.md](../vision.md#decisions)). Neither the `codegen` subcommand nor its output is
covered by [compatibility](../compatibility.md). Phases 0 to 2 and the functions of phase 3 have landed (see
[Phases](#phases)), and every question is [decided](#decisions).

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
  model verified but a type cannot enforce is printed as a numbered **obligation** that
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
  through protected regions.
- A file layout. rdra-ish does not decide where code goes in a project.

This widens what rdra-ish produces from review and verification artifacts to code a
build depends on. The maintainer decided on 2026-11-07 that this is in scope
([vision.md](../vision.md#decisions)).

Code generation is an experimental extra, outside [compatibility](../compatibility.md).
The `codegen` subcommand, its flags, and its exit status may change or go away in any
release. So may the generated code: its names, its shape, the obligation list, and the
JSON form. `rdra-ish --help` and the CLI reference label the subcommand experimental, and
the header of every output says so. An agent therefore treats the output as a starting point
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
| `@derived(expr)` | a field with the expression in its doc, and an obligation to compute it; later a function printed from the typed expression | same | same |
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
verdicts.

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

<a id="split"></a>
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
2. **Obligations.** A numbered list of what the verification assumed but the types cannot
   enforce, each naming the invariant or rule it protects and the source site. For
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
   one, the code has left what was verified.

`--format json` prints the same intermediate form and obligations as data, for an agent
that wants to generate in another language or check its work against the list.

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
  the stream to stdout, as `states --format typescript` does. `-o <file>` is optional, and
  for Go `--package` names the package. Both have landed. Later phases add these: `--format json` prints the intermediate form and the obligations. `--part
  types|core|scaffold|tests` limits the stream, `--buc` scopes it as `typescript-states`
  does, and `--scalars` names the scalars module. `export --kind
  typescript-states` stays as it is: its kind name is covered by
  [compatibility](../compatibility.md).
- The generator runs the same `check` first and refuses when it fails, because the code
  would then encode a model that was not verified. `cross_order_payment_split` is refused
  this way. `--allow-failing` prints anyway and lists each failing invariant as an
  obligation the code does not meet.
- The lifecycle gate (`crates/rdra-ish-cli/src/lifecycle_gate.rs`) classifies the
  outputs by what they read. Data types read no reachability and write with the note, as
  DBML does. State sums and functions refuse when the lifecycle has errors. Under
  `--refine reachable` they also refuse when a cone stopped at the state cap, because an
  undecided cone gives no complete set of patterns.

## How we know it is right

The project's rule is that every output is checked against the one semantics, on every
sample. For this generator:

- **It compiles.** `crates/rdra-ish-cli/tests/codegen_builds.rs` runs `tsc --strict`,
  `rustc -D warnings`, and `go vet` on the output of every `samples.txt` root (landed for
  the data layer). Later phases add the exhaustiveness linter and the scaffold and tests.
  The dev shell provides TypeScript and Go, and under `CI` a missing compiler fails the test.
- **It agrees with the transition system.** A test in the style of `engine_equivalence`
  runs the generated TypeScript (Node is in the flake) on every edge BFS explores at
  1 instance: it concretizes the pre-state, calls the function, abstracts the result, and
  compares it with the BFS successor, and it checks that a guard the generated function
  rejects is a guard BFS has. Rust and Go print the same IR, so per-printer snapshot tests
  suffice for them.
- **The split runs agree.** On the three split samples, the test also runs interleavings
  of steps and other actions, and checks that `Blocked` is returned exactly where BFS has
  no successor for the step.
- **The keys agree.** `key_facts` (`crates/rdra-ish-cli/tests/key_facts.rs`) gains the
  generated key fields as one more consumer of the `ForeignKey` fact set.

## Phases

0. **Fix `typescript-states`.** Landed. An Int axis prints the column's type, not a literal, and
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
   and "never abandon a run". A later step does not take the earlier rows themselves, only
   their marks: in the samples a later step reads an earlier row only for its key, which the
   mark carries, and a row deleted between the steps then needs no `Deleted` type. A step
   that does not run returns `NotEnabled` or `Rejected`, which for a later step means
   "retry", not a new `Blocked` type, and `resume<UC>` is left to the code that reads the
   stored marks. `codegen_equivalence.rs` checks every step BFS runs, the mark each step
   leaves in the run var, and every key a function sets against the owner var; the
   interleavings are BFS's own, since it explores every action between two steps. A
   condition of a later step that reads an earlier row's columns is not printed yet.
5. **The rest of the action vocabulary.** Triggered use cases have landed: an event
   carries the id of each row its raise marks (the `Raised` vars of the event order), and a
   triggered use case takes the event as an argument and checks its row against it, so it
   cannot run without the event, as the 2.0 event order requires. A row the use case reads
   only through the event is an argument in its flat row type. A link between two rows of
   an earlier step is not printed (the marks carry ids, not keys). A choice among arms is
   one function per target state, not a choice argument as planned: the target fixes the
   result's state type, so a function per target keeps every type concrete, and the
   dispatcher takes the choice. A `.when` on an arm folds into a condition of that function:
   its guard reads the successor's tag, which the function's target fixes. A second
   lifecycle column is a branch on its value, its `from` states a `TransitionFrom`
   condition. A `.has` / `.none` ranges over a list of the related
   entity's rows, an input the caller fills with every present row, and the function keeps
   those the key links; an obligation in the header says the list must be complete.
   That leaves 11 use cases on the samples: a `.when` on a chosen value (2), a choice on a
   second lifecycle column (1), copies (2), a condition on what a use case creates (2),
   comparison propositions and other assignments (3), and a link between two rows of an
   earlier step (1).

## Decisions

The maintainer decided these on 2026-11-07:

1. **Scope and stability.** The generator is in scope as an experimental extra. Neither
   the subcommand nor its output is covered by compatibility (see
   [Goals and non-goals](#goals-and-non-goals)).
2. **Use cases split into transactions.** One function per step, linked by stored run
   marks ([the decision](#split)).
3. **Default refinement.** Declared refinement. It is sound regardless of the bounds, and
   it pushes the modeler to state rules. Reachable refinement stays opt-in, after the
   measurement [State sums](#state-sums) names.
4. **Rust encoding.** An enum of per-state structs, with transitions that consume their
   source. Typestate (`Order<S>`) was not chosen: "an order in any state" would still
   need an enum or a trait object.
5. **Go nullables.** `*T`, which `encoding/json` reads as it is.
