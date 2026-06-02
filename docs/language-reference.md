# Language Reference

This document is the complete reference for the `rdra-ish` DSL. The DSL describes
RDRA (Relationship-Driven Requirements Analysis) models: you declare model elements
as typed instances and connect them with predicate calls.

A source file (`.rdra`) consists of an optional `module` declaration, zero or more
`import` declarations, and a sequence of instance declarations, entity bodies,
predicate calls, and constraint declarations, in any order. Comments use `//` and run
to the end of the line.

---

## Instance Declarations

An instance declaration introduces a named element:

```
<kind> <Id> "Label"
```

- `<kind>` is one of the kinds in the table below.
- `<Id>` is a unique identifier within its kind (an upper-camel-case identifier by
  convention).
- `"Label"` is a human-readable display string used in generated diagrams and lists.

An instance may also carry an optional description:

```
<kind> <Id> "Label" description "Description text"
```

`description` is stored as element metadata for all instance kinds, including
`business`, `buc`, `usecase`, `system`, and `api`. Diagram emitters currently keep
using labels only; how descriptions should appear in diagrams is intentionally left
for a future view/design decision.

| kind | Description |
|---|---|
| `actor` | A human actor who performs use cases. |
| `extsystem` | An external system used by actors or use cases. |
| `system` | An internal system boundary. It groups APIs; its entity set is derived from those APIs' CRUD targets. |
| `requirement` | A requirement that motivates business use cases. |
| `business` | A business (a coarse-grained area of activity) that BUCs belong to. |
| `buc` | A business use case: a unit of business value composed of use cases. |
| `usagescene` | A usage scene describing a context of use. |
| `usecase` | A use case: a concrete interaction that reads/writes entities and displays screens. |
| `screen` | A screen (UI surface) displayed by use cases. |
| `event` | A domain event, used as the trigger of state transitions and as a `raises`/`triggers` endpoint. |
| `entity` | An entity, i.e. a database table. Entities may carry a column body. |
| `state` | A state machine node, linked to an entity's Enum column. |
| `condition` | A condition. |
| `variation` | A variation. |
| `api` | An API layer endpoint invoked by a use case; operates entities on behalf of the use case and defines an atomic data operation boundary. Appears in the RDRA layered graph and sequence diagram as a named API lane, but is intentionally omitted from the boundaryless graph. |
| `location` | A place, channel, organization point, or usage scene for Business-BUC context. |
| `timing` | A timing, trigger, or business situation for Business-BUC context. |
| `medium` | A physical medium, device, terminal, or operation medium for Business-BUC context. |
| `permission` | A permission or role-like authority assignable to actors. |

---

## Entity Bodies

An `entity` declaration may be followed by a brace-delimited body of column
definitions:

```
entity Order "Order" {
  id:           Int      @pk
  total:        Money
  ordered_at:   DateTime
  delivered_at: DateTime @null
  status:       Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
  note:         String   @null
}
```

Each column has the form `name: Type [annotations...]`.

### Column types

| Type | Description |
|---|---|
| `Int` | Integer. |
| `String` | Text. |
| `Money` | Monetary amount. |
| `DateTime` | Date and time. |
| `Date` | Date. |
| `Bool` | Boolean. A boolean column is a **state axis** (see [state-derivation.md](./state-derivation.md)). |
| `Decimal` | Decimal number. |
| `Enum(a, b, c)` | Enumeration over the listed variants. An Enum column is a **state axis** and can be linked to a state machine. |

### Column annotations

| Annotation | Description |
|---|---|
| `@pk` | Marks the column as the primary key. FK columns are auto-generated against this key by `relate`. |
| `@pk(a, b)` | Declares a composite primary key over the named columns. |
| `@unique` | Adds a unique constraint. |
| `@null` | Makes the column nullable. A nullable column is a **state axis** with values `{null, present}`. |
| `@default(v)` | Declares a default value `v`. Used as the seed value in state derivation. |
| `@label("...")` | Overrides the column's display label. |

---

## Relationship Predicates

Predicates connect declared instances. Arguments are referenced by their `<Id>`. When
an id is ambiguous across kinds (for example an `event` and a `usecase` share a name),
qualify the argument with a kind prefix — see [Kind-Qualified References](#kind-qualified-references).

| Predicate | Signature | Semantics |
|---|---|---|
| `performs` | `(Actor, UseCase \| Buc)` | The actor performs the given use case or BUC. |
| `uses` | `(Actor, ExtSystem)` | The actor uses the external system. |
| `invokes` | `(UseCase, Api)` | The use case invokes the API layer. Drives the `Screen → Api → Entity` lane in the sequence diagram. |
| `reads` | `(UseCase \| Api, Entity)` | The use case or API reads the entity. |
| `writes` | `(UseCase \| Api, Entity)` | The use case or API writes the entity (treated as an update for state derivation). |
| `creates` | `(UseCase \| Api, Entity)` | The use case or API creates the entity. Seeds the initial state pattern. |
| `updates` | `(UseCase \| Api, Entity)` | The use case or API updates the entity. Produces a transition operation in state derivation. |
| `deletes` | `(UseCase \| Api, Entity)` | The use case or API deletes the entity. Marks the source pattern terminal. |
| `displays` | `(UseCase, Screen)` | The use case displays the screen. |
| `shows` | `(Screen, Entity)` | The screen shows information from the entity. |
| `raises` | `(UseCase, Event)` | The use case raises the domain event. Links use cases to `transitions`. |
| `triggers` | `(Event, UseCase \| Buc)` | The event triggers a concrete use case or starts a BUC boundary. |
| `contains` | `(Buc, UseCase)` or `(System, Api)` | The use case composes the BUC, or the API belongs to the system boundary. |
| `coordinates` | `(UseCase, Entity, Entity)` | The use case coordinates consistency for a relation crossing system boundaries. The use case must invoke APIs on both system sides that operate the corresponding entities. |
| `belongs` | `(Buc, Business)` | The BUC belongs to the business. |
| `has_permission` | `(Actor, Permission)` | The actor has the permission type. This provides a base vocabulary for later UC/API permission constraints. |
| `requires_permission` | `(UseCase \| Api, Permission)` | The use case or API requires the permission type. |
| `requires_medium` | `(UseCase \| Api, Medium)` | The use case or API requires the operation medium. |
| `motivates` | `(Requirement, Buc)` | The requirement motivates the BUC. |
| `relate` | `(Entity, Entity, Card)` | Declares an ER relationship and auto-generates the FK columns. `Card` is one of `"1:1"`, `"1:N"`, `"N:1"`, `"N:M"`. |
| `transitions` | `(Event, State, State)` | A state machine edge: on the event, the entity moves from the first state to the second. |
| `sets` | `(UseCase \| Event, Entity, "col", "val")` | An explicit column effect, consumed by state pattern derivation. |
| `sets` | `(UseCase \| Event, Entity, col op rhs, true \| false)` | Drives the truth value of a comparison proposition (derived Bool axis) in state pattern derivation. |

### Access constraints

Actor permissions are declared with `permission` and attached to actors with
`has_permission(Actor, Permission)`. Use cases and APIs can then declare the
permission and medium they require:

```
permission ScheduleWrite "Schedule Write"
medium StaffTerminal "Staff Terminal"

has_permission(Staff, ScheduleWrite)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, ScheduleWrite)
```

Screen access constraints are derived rather than declared directly. A screen inherits
the constraints of each use case that displays it, and also the constraints of each API
that the use case invokes. Use `csv --kind screen-constraints` to inspect those
screen × use-case/API paths. `check` also compares `requires_permission` against
`has_permission` on the actors that can perform the use case path. For APIs, this
check is performed per invoking use case so a shared API must be authorized on every
modeled invocation path. Use `csv --kind actor-permission-audit` or
`list --kind actor-permission-audit` to inspect the full actor × permission projection:
`missing` means a required permission is not assigned to that actor, and `excess` means
the actor has a permission that no modeled performer path currently requires.

### `belongs` context

`belongs(Buc, Business)` declares the Business area a BUC belongs to. The mapping can
also carry optional When / Where / By context with method-chain clauses:

```
timing AppointmentRequested "Appointment Requested"
location FrontDesk "Front Desk"
medium FrontDeskTerminal "Front Desk Terminal"

belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .where(FrontDesk)
  .where("patient portal")
  .by(FrontDeskTerminal)
  .by("tablet")
```

- `.when(...)` records the timing, trigger, or business situation where the BUC applies.
- `.where(...)` records the place, channel, organization point, or usage scene.
- `.by(...)` records the physical medium, device, terminal, or other operation medium.
- Each argument may be a string literal or a reference to the corresponding typed element:
  `.when(timing::...)`, `.where(location::...)`, or `.by(medium::...)`.
- Multiple `.when(...)`, `.where(...)`, or `.by(...)` clauses accumulate as alternative context values.

### `relate` and FK generation

`relate(Child, Parent, "N:1")` generates a foreign-key column on the child entity
referencing the parent's primary key (for example `parent_id`). The cardinality drives
the crow's-foot notation in the ER diagram and the FK-induced graph used for
[transaction boundary inference](#see-also).

### `transitions` and the state machine

`transitions(event::E, state::From, state::To)` declares one edge of an entity's state
machine. States are linked to an entity's Enum column by matching state ids
(case-insensitively) against the Enum variants. The use case that drives a transition
is found through `raises(UseCase, Event)`. See
[state-derivation.md](./state-derivation.md) for how transitions feed derivation.

### `api` and the API layer

For sequence diagrams, `performs(Actor, UseCase)` is more specific than
`performs(Actor, Buc)`. If a use case has a direct actor, the sequence diagram uses
that actor as the participant for the use-case flow. If no direct actor is declared,
the emitter falls back to the actor on the containing BUC.

The `api` element models the backend API layer that sits between a screen and the
entities it operates. Declare an api once per API endpoint or boundary:

```
api OrderApi "Order API"
```

Then connect it to a use case with `invokes`, and to entities with CRUD predicates:

```
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
creates(OrderApi, OrderLine)
updates(PlaceOrder, Cart)   // direct write still allowed (mixed form)
```

**Sequence diagram behaviour** (`--kind sequence`):
- If a use case invokes at least one API, the sequence diagram renders
  `Actor → Screen → Api → Entity` lanes with the API as the source of DB writes.
- If an API is invoked by multiple use cases, the API's CRUD boundary applies to each
  invoking use case. Split read-only and write APIs when those operations have different
  consistency contracts.
- If a use case has no `invokes` (legacy), the existing `System` participant is used
  unchanged — full backward compatibility.
- API write groups are rendered as `transaction (API atomic boundary)`. Direct
  use-case writes still use FK-grouped transaction blocks
  (`group transaction (inferred from FK)`).

**System boundary behaviour**:
- `contains(System, Api)` assigns an API to an internal system boundary.
- A system's entity set is derived only from CRUD predicates on its APIs; there is no
  direct `system -> entity` ownership declaration.
- If entities derived for different systems have a `relate` edge between them, the tool
  warns unless a use case declares `coordinates(UseCase, EntityA, EntityB)`.
- A coordinating use case must invoke an API in each entity's system, and each invoked
  API must operate the corresponding entity.
- If the same API or entity is derived into multiple systems, the tool emits a warning
  because the ownership boundary is ambiguous.

**Other diagrams**:
- `api` nodes are included in the RDRA layered graph (`--kind rdra`), where they appear
  in the system layer.
- `api` nodes are intentionally omitted from the boundaryless relationship graph
  (`--kind boundaryless-graph`) to keep that view focused on business and data links.
- `api` nodes are also omitted from the event-flow diagram (`--kind event-flow`),
  because an API is not a participant in the raises/triggers/transitions causal chain.

**CSV/list output**:
- `list --kind api` — id/label table of all declared apis.
- `list --kind system` — id/label table of all declared systems.
- `csv --kind api` — CSV file of api id and label.
- `csv --kind api-matrix` — api × entity CRUD matrix.

**Diagnostics** (emitted as warnings when running `diagram --kind sequence`):
- `ApiNeverInvoked` — an api is declared but no use case invokes it.
- `ApiInvokedButNoEntity` — an api is invoked but operates no entity.
- `ApiInMultipleSystems` — an api is assigned to multiple systems.
- `EntityInMultipleSystems` — an entity is operated by APIs in multiple systems.
- `CrossSystemEntityRelation` — a `relate` edge crosses derived system entity sets.
- `CoordinationNotCrossSystem` — `coordinates` is declared for a pair that does not cross two system boundaries.
- `CoordinationMissingApi` — the coordinating use case does not invoke an API on one side of the boundary.

### `triggers` and event-driven BUC chaining

`triggers(event::E, buc::B)` declares that an event starts a downstream BUC. This is the
standard abstract form when the business handoff is known but the concrete entry use
case is not yet fixed, or when a BUC may later be changed from human-initiated to
event-initiated without reshaping the BUC itself.

`triggers(event::E, usecase::UC)` declares that an event causes a concrete downstream
use case to execute. This is the refined form when the entry action is known. The tool
validates that a triggered use case belongs to at least one BUC via `contains`; a warning
is emitted otherwise. Use `diagram --kind event-flow` to visualise the full
`raises -> Event -> triggers` chain alongside state transitions.

### Event-triggered BUCs

<!-- constrained-by #relationship-predicates -->
<!-- derived-from ./rdra-ish-interpretation.md#business-flow -->

The standard way to describe a BUC that starts from an event is:

1. Declare the target BUC.
2. Connect the event to that BUC with `triggers(Event, TargetBuc)`.
3. When the entry action becomes clear, add `contains(TargetBuc, EntryUseCase)` and
   optionally refine the flow with `triggers(Event, EntryUseCase)`.

The BUC target is the reviewable boundary of the handoff. The use-case target is the
more concrete entry point inside that boundary. Both forms may coexist: the BUC edge
says "this event starts that business-value slice", while the use-case edge says "this
is the current entry action".

```rdra
buc BucBillingClaims "Billing Claims"
usecase GenerateClaim "Generate Claim"
event EncounterSigned "Encounter Signed"

triggers(event::EncounterSigned, BucBillingClaims)
contains(BucBillingClaims, GenerateClaim)
triggers(event::EncounterSigned, GenerateClaim)
```

If the triggered use case is a system-triggered step, `performs` may be omitted. If it
declares `requires_permission` directly or through an invoked API, `check` reports a
warning until an actor path with the required permission is modeled.

---

## Value Vocabulary for `sets`

The fourth argument of `sets` is a string whose interpretation depends on the target
column's type:

```
// Enum column variant
sets(usecase::Capture, Payment, "status", "captured")

// Bool column
sets(usecase::Enable, Switch, "enabled", "true")

// Set a nullable column to non-null without recording a type
sets(usecase::Login, UserAccount, "last_login_at", "present")

// Set a nullable column to non-null, recording a PostgreSQL-specific type
sets(usecase::Deliver, Order, "delivered_at", "timestamptz")
sets(usecase::Tag,     Doc,   "metadata",     "jsonb")

// Set a nullable column to null
sets(usecase::Logout, Session, "token", "null")

// Event as origin: expanded to every UC that raises the event
// (equivalent to sets on each raising UC, but kept close to the event definition)
sets(event::EvDeliver, Order, "delivered_at", "timestamptz")
```

| Value | Target column | Meaning |
|---|---|---|
| Enum variant name | `Enum` column | Set the column to that variant. |
| `"true"` / `"false"` | `Bool` column | Set the boolean value. |
| `"present"` | `@null` column | Make the column non-null (value present), no type recorded. |
| `"null"` | `@null` column | Make the column null. |
| PostgreSQL type name | `@null` column | Make the column non-null **and** record the type for display (`jsonb`, `uuid`, `timestamptz`, `inet`, etc.). In reachability the typed-present value is equivalent to `present`; the type is only carried for output. |

### Driving comparison propositions with `sets`

A **comparison proposition** (`col op rhs`, e.g. `stock < selling`, `expired_at < now`) is
a derived Bool axis in the state space. Because continuous values such as `Int` or `DateTime`
are not tracked in the abstract state space, the truth value of a comparison proposition
**must be driven explicitly** using the four-argument form of `sets`:

```
// Tell the model that Sell causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Tell the model that Refund causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

The third argument is the bare comparison expression (no quotes); the fourth argument is
the bare boolean literal `true` or `false`. If no `sets` drives a comparison proposition,
it is treated as **always false** (the comparison never holds) throughout state derivation.

| Fourth argument | Meaning |
|---|---|
| `true` | The comparison holds after this use case / event executes. |
| `false` | The comparison does not hold after this use case / event executes. |

---

## Entity State Constraints

State constraints assert facts about the **reachable state space** of an entity. They
are evaluated after BFS state-pattern derivation: a constraint is only reported as
violated if a reachable pattern actually witnesses the violation. Unreachable bad
states never trigger a diagnostic.

There are two constraint forms, deliberately given different syntaxes that match their
semantics.

### `forbidden` — tuple-variadic forbidden states

```
// A single forbidden value
forbidden(Order, (status, cancelled))

// An AND-combination: forbidden only when both hold simultaneously
forbidden(Order, (status, delivered), (refunded, true))

// A comparison expression as a condition
forbidden(Stock, stock < selling)

// Mixing tuples and a comparison expression
forbidden(Stock, (status, on_sale), stock < selling)

// Comparison against the built-in `now` keyword
forbidden(Coupon, expired_at < now)
```

Each condition is either a `(column, value)` tuple or a comparison expression. All
conditions are combined with **AND**: the state is forbidden only when **every** condition
holds at once. If any reachable pattern satisfies all conditions, a
`ForbiddenStateViolated` diagnostic is emitted, naming the conditions and the offending
pattern.

**Design rationale.** A forbidden state is a *point* (or a sub-cube) in the finite
product state space — fundamentally a conjunction of column assignments. The
tuple-variadic form expresses exactly that: a flat list of `(column, value)` points,
read as "this exact combination must not exist." There is no antecedent/consequent
asymmetry to model, so a single flat list is the most direct encoding.

### `invariant` — method-chain required co-occurrence

```
invariant(Order)
  .when(status, delivered)
  .then(delivered_at, present)

invariant(Order)
  .when(status, delivered)
  .when(refunded, false)     // multiple .when() = AND
  .then(refund_id, null)

// Comparison expression in the guard
invariant(Coupon)
  .when(expired_at < now)
  .then(status, expired)

// Comparison expression as the requirement
invariant(Stock)
  .when(status, on_sale)
  .then(stock < selling)
```

An invariant is an implication. The `.when(...)` clauses are guards (the antecedent)
and the `.then(...)` clauses are requirements (the consequent). Each clause is either a
`(column, value)` pair or a bare comparison expression. Within each side the clauses are
combined with **AND**. The rule reads: **whenever all `.when()` guards hold, all
`.then()` requirements must also hold.**

For every reachable pattern that satisfies all the guards but violates any requirement,
an `InvariantViolated` diagnostic is emitted, naming the guards, the requirements, and
the offending pattern.

**Design rationale.** Unlike `forbidden`, an invariant has *two distinct sides* — a
guard and a requirement — joined by implication. A flat tuple list cannot express which
conditions are the trigger and which are the obligation. The method-chain form keeps
the two sides syntactically separate (`.when` vs `.then`) and lets each side accumulate
any number of AND-ed conditions unambiguously, while reading naturally left to right.

### Cross-Entity Constraints

Cross-entity constraints describe rules that mention columns from more than one entity.
They are parsed, type-checked, and checked by `states` after each participating
entity's reachable patterns have been derived.

Use `Entity.column` when a condition points at a column. The optional leading entity
arguments declare the intended scope; when omitted, the scope is inferred from qualified
column references.

```
cross_forbidden(Order, Payment,
  (Order.status, cancelled),
  Payment.amount > Order.total)

cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)

cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
```

`cross_forbidden(...)` accepts a flat AND-list of conditions. `cross_invariant(...)`
uses the same implication shape as `invariant`: `.when(...)` clauses are guards and
`.then(...)` clauses are required conditions. A condition is either:

- `(Entity.column, value)` for state-like equality, using the same value vocabulary as
  `sets`.
- A comparison expression such as `Order.total > Payment.amount` or
  `Coupon.expires_at < now`.

When more than one entity is in scope, bare column names are rejected so the rule stays
unambiguous. With a single-entity scope, bare columns are still accepted as a shorthand.

`states` evaluates cross-entity rules by taking the cross-product of each participating
entity's reached state patterns. Conditions that reference actual state axes can
produce `CrossForbiddenViolated` or `CrossInvariantViolated` diagnostics. Conditions
that require values absent from the abstract state space, such as ordinary numeric
amount comparisons, produce `CrossConstraintNotEvaluated` instead.

Adding `.along(EntityA, EntityB, ...)` declares that the rule is intended to apply only
to instances connected through the listed `relate` path, not to the global cross-product.
The current `states` engine validates that the path is declared, but it does not yet
track linked instance reachability. Such relation-scoped rules therefore produce
`CrossConstraintNotEvaluated` rather than falling back to global-product evaluation.

### Comparison Expressions in Constraints

`forbidden`, `cross_forbidden`, and the `.when()` / `.then()` clauses of `invariant` or
`cross_invariant` accept bare **comparison expressions** as conditions, in addition to
the `(column, value)` tuple form.

#### Syntax

```
col op rhs
```

Where:

- `col` is a bare column name on the left-hand side, or `Entity.column` in a
  cross-entity constraint.
- `op` is one of: `<`, `>`, `<=`, `>=`, `==`, `!=`.
- `rhs` is one of:
  - A bare column name or `Entity.column` reference: `stock >= selling`,
    `Payment.amount >= Order.total`
  - An integer literal: `quantity >= 0`
  - The built-in keyword `now`: `expired_at < now`

#### Type rules

| Operator | Permitted column types |
|---|---|
| `<`, `>`, `<=`, `>=` | `Int`, `Money`, `Decimal`, `Date`, `DateTime` |
| `==`, `!=` | All column types |

When `rhs` is a column reference, the left-hand and right-hand columns must belong to
the same type category (numeric with numeric, temporal with temporal). When `rhs` is
`now`, the left-hand column must be of type `Date` or `DateTime`.

#### Semantics and interaction with `sets`

A comparison expression is treated as a **derived Bool proposition axis** in the state
space. Because continuous-valued types (`Int`, `DateTime`, etc.) are abstracted away in
the state-pattern model, the runtime truth value of a comparison cannot be inferred
automatically. Instead, it must be driven explicitly with `sets`:

```
// Declare that the Sell use case causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Declare that the Refund use case causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

If no `sets` drives a given comparison proposition, the proposition is treated as
**always false** (the comparison never holds) for the purposes of state-pattern
derivation and constraint checking. In practice this means:

- A `forbidden` condition containing an undriven comparison is never triggered (the
  comparison is never true, so the conjunction is never satisfied).
- A `.when()` guard containing an undriven comparison is never satisfied, so the
  invariant never fires.
- A `.then()` requirement containing an undriven comparison is always violated whenever
  its guard is satisfied.

Always add matching `sets` calls whenever you use a comparison expression in a
constraint, or the constraint will silently have no effect (for `forbidden` / `.when`)
or will always fire (for `.then`).

### Bare identifiers

Column names and values inside `forbidden(...)` tuples and `.when()` / `.then()` clauses
are **bare identifiers**, not quoted strings (contrast with `sets`, whose column and
value are quoted strings). They use the same value vocabulary as `sets`: Enum variant
names, `true` / `false`, `present` / `null`, and PostgreSQL type names.

In **comparison expressions** (e.g. `stock < selling`, `expired_at < now`), column names
on both sides are also bare identifiers. The right-hand side may additionally be an
integer literal or the built-in keyword `now`. `now` is a reserved keyword in this
position — it does not refer to a column named `now`; it denotes the current
date/time and is only valid as the right-hand side of a comparison whose left-hand
column is of type `Date` or `DateTime`.

---

## Kind-Qualified References

When an id is unambiguous, you reference an instance by its bare id. When the same id is
declared for more than one kind — a common and natural situation, e.g. a `state Active`
and an `event Active`, or an `event Cancel` and a `usecase Cancel` — you must
disambiguate the reference with a kind prefix:

```
usecase::Foo      // the use case named Foo
event::Cancel     // the event named Cancel
state::Active      // the state named Active
```

Typical cases:

- In `transitions(event::Capture, state::Pending, state::Paid)`, the event and states
  may collide with use cases of the same name once modules are merged, so qualifiers are
  used.
- In `contains(BucOrder, usecase::Cancel)` and `raises(usecase::Cancel, event::Cancel)`,
  the `Cancel` use case and the `Cancel` event coexist.

Use the qualifier whenever the bare id would be ambiguous after all imported modules are
merged; the compiler reports an ambiguity error otherwise.

---

## Imports and Modules

A file may declare the module it belongs to and import other modules:

```
module shared.actors

import shared.actors             // flat import: brings all names into scope
import shared.actors as a        // namespaced import under alias `a`
import shared.actors.{Staff}     // selective import of a single name
import shared.actors.{Staff as S} // selective import with a local alias
```

- `module <path>` declares the dotted module path of the current file.
- `import <path>` brings the module's declarations into scope.
- `import <path> as <alias>` makes the imported names available under the alias namespace.
- `import <path>.{Name}` imports only the named declarations.
- `import <path>.{Name as Local}` imports a name under a local alias.

When the CLI loads a directory, it merges all `.rdra` files reachable from the entry
files into a single semantic model, resolving imports against the include paths derived
from the input layout.

---

## See Also

- [state-derivation.md](./state-derivation.md) — how reachable state patterns are
  derived, how predicates feed the BFS, and how `forbidden` / `invariant` are checked.
- [cli-reference.md](./cli-reference.md) — the `rdra-ish` command-line interface,
  including the `diagram --kind event-flow` command for event causality visualisation.
