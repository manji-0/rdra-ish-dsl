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

| kind | Description |
|---|---|
| `actor` | A human actor who performs use cases. |
| `extsystem` | An external system used by actors or use cases. |
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
| `reads` | `(UseCase, Entity)` | The use case reads the entity. |
| `writes` | `(UseCase, Entity)` | The use case writes the entity (treated as an update for state derivation). |
| `creates` | `(UseCase, Entity)` | The use case creates the entity. Seeds the initial state pattern. |
| `updates` | `(UseCase, Entity)` | The use case updates the entity. Produces a transition operation in state derivation. |
| `deletes` | `(UseCase, Entity)` | The use case deletes the entity. Marks the source pattern terminal. |
| `displays` | `(UseCase, Screen)` | The use case displays the screen. |
| `shows` | `(Screen, Entity)` | The screen shows information from the entity. |
| `raises` | `(UseCase, Event)` | The use case raises the domain event. Links use cases to `transitions`. |
| `triggers` | `(Event, UseCase)` | The event triggers the use case. |
| `contains` | `(Buc, UseCase)` | The use case composes the BUC. Establishes the BUC-of-use-case mapping. |
| `belongs` | `(Buc, Business)` | The BUC belongs to the business. |
| `motivates` | `(Requirement, Buc)` | The requirement motivates the BUC. |
| `relate` | `(Entity, Entity, Card)` | Declares an ER relationship and auto-generates the FK columns. `Card` is one of `"1:1"`, `"1:N"`, `"N:1"`, `"N:M"`. |
| `transitions` | `(Event, State, State)` | A state machine edge: on the event, the entity moves from the first state to the second. |
| `sets` | `(UseCase \| Event, Entity, "col", "val")` | An explicit column effect, consumed by state pattern derivation. |

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

### `triggers` and event-driven BUC chaining

`triggers(event::E, usecase::UC)` declares that an event causes a downstream use case to
execute — capturing cross-BUC choreography. The tool validates that the triggered use
case belongs to at least one BUC via `contains`; a warning is emitted otherwise. Use
`diagram --kind event-flow` to visualise the full `raises → Event → triggers` chain
alongside state transitions.

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
```

Each `(column, value)` tuple is a condition. The tuples are combined with **AND**: the
state is forbidden only when **every** tuple holds at once. If any reachable pattern
satisfies all the tuples, a `ForbiddenStateViolated` diagnostic is emitted, naming the
conditions (formatted as `col=val AND col=val`) and the offending pattern.

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
```

An invariant is an implication. The `.when(...)` clauses are guards (the antecedent)
and the `.then(...)` clauses are requirements (the consequent). Within each side the
clauses are combined with **AND**. The rule reads: **whenever all `.when()` guards
hold, all `.then()` requirements must also hold.**

For every reachable pattern that satisfies all the guards but violates any requirement,
an `InvariantViolated` diagnostic is emitted, naming the guards, the requirements, and
the offending pattern.

**Design rationale.** Unlike `forbidden`, an invariant has *two distinct sides* — a
guard and a requirement — joined by implication. A flat tuple list cannot express which
conditions are the trigger and which are the obligation. The method-chain form keeps
the two sides syntactically separate (`.when` vs `.then`) and lets each side accumulate
any number of AND-ed conditions unambiguously, while reading naturally left to right.

### Bare identifiers

Column names and values inside `forbidden(...)` tuples and `.when()` / `.then()` clauses
are **bare identifiers**, not quoted strings (contrast with `sets`, whose column and
value are quoted strings). They use the same value vocabulary as `sets`: Enum variant
names, `true` / `false`, `present` / `null`, and PostgreSQL type names.

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
