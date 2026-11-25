# Syntax

<!-- derived-from ../../../docs/language-reference.md -->

The declarations, the predicate signatures, and the mistakes `check` catches. The step
files show each construct in context.

## Declarations

```rdra
actor Customer "Customer"
business Commerce "Commerce"
requirement ReqOrder "Order processing must be reliable"
buc BucOrder "Process Order"
flow CheckoutFlow "Checkout Flow"
step ReviewCart "Review Cart"
usecase PlaceOrder "Place Order"
screen CheckoutScreen "Checkout"
field OrderIdField "Order ID"

api PlaceOrderApi "Place Order API"
  method POST
  path "/orders"

dto PlaceOrderRequest "Place Order Request"

entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted) @default(draft)
  submitted_at: DateTime @null
}
```

| Kind | Use |
|---|---|
| `actor` / `extsystem` | People and outside systems |
| `goal` / `requirement` / `adr` / `nfr` / `quality` / `constraint` | Outcomes, traceability, decisions, non-functional intent |
| `business` / `buc` / `flow` / `step` / `usecase` | Business value, flow order, action boundaries |
| `screen` / `field` / `api` / `dto` / `system` | UI items, payloads, implementation boundaries |
| `entity` | Logical data |
| `event` | The interface between use cases, or a use case and an external system |
| `timer` | A schedule that starts a use case |
| `variation` / `condition` | Values an actor chooses; the rule a use case applies under |
| `medium` / `permission` | Context and access vocabulary |

States are not declared: they are the variants a `transitions` names. The removed words
`state`, `location`, `timing`, `uses`, and `usagescene` fail to parse with a hint that
names the replacement.

## Predicates

| Predicate | Signature | Meaning |
|---|---|---|
| `performs` | `(Actor, UseCase\|Buc\|Step)` | The actor runs the use case, every use case of the BUC, or the step |
| `contains` | `(Buc, UseCase\|Flow)` / `(Flow, Step)` / `(System, Api)` / `(Screen, Field)` | Composition |
| `precedes` / `branches` / `excepts` / `repeats` | `(Step, Step)` | Flow order, alternative, exception, loop |
| `covers` | `(Step, UseCase\|Api\|Event\|Timer)` | The step covers model behavior |
| `belongs` | `(Buc, Business)` | BUC ownership; chain `.by(Medium\|"text")` |
| `creates` / `reads` / `updates` / `deletes` | `(UseCase\|Api, Entity)` | Data touchpoint. `reads` takes `.by(cols).order(col.desc).page(cursor\|offset)` or `.count()` / `.sum(col)`; a use case's write takes `.apart()` |
| `displays` / `shows` / `navigates` | `(UseCase, Screen)` / `(Screen, Entity)` / `(Screen, Screen)` | UI path; `navigates` takes `.via(UseCase)` |
| `invokes` | `(UseCase, Api)` | The use case calls the API |
| `maps_field` | `(Field\|Dto.field, Entity.column)` | The column a field carries |
| `request` / `response` / `error_response` | `(Api, Dto)` | API payload contract |
| `coordinates` / `compensates` | `(UseCase, Entity, Entity)` / `(UseCase, UseCase)` | Cross-system consistency; compensation |
| `owns` | `(System, Entity)` | Intended ownership before API CRUD exists |
| `raises` | `(UseCase\|ExtSystem, Event)` | Emits an event; from an external system it is inbound |
| `triggers` | `(Event\|Timer, UseCase\|Buc\|ExtSystem)` | Starts a use case or BUC; to an external system it is outbound. `.counted(n)` on an event |
| `outbox` | `(Event)` | Published outside the model; needs no consumer |
| `transitions` | `(UseCase, Entity.col, from -> to)` | Moves the Enum column; `(a, b) -> c` lists sources; `.when(Cond)` guards the arm |
| `sets` / `chooses` / `drives` | `(UseCase, Entity.col == val)` / `(UseCase, Entity.col[, value])` / `(UseCase, Entity.col op rhs, true\|false)` | Column effect; actor's pick of a variation value; comparison effect |
| `links` / `relinks` / `unlinks` | `(UseCase, Child, Parent)` | Gives, moves, or drops an existing row's parent |
| `forbidden` / `required` / `exclusive` | `(Entity, cond, ...)` | Forbidden, always-required, or mutually exclusive facts |
| `invariant` | `(Entity).when(...).then(...)`, or `.none/.has/.at_most/.at_least/.exactly(...)` | Required co-occurrence; quantifier over related rows |
| multi-entity `forbidden` / `invariant` | `(Entity, Entity, ..., Entity.col ...)` then `.along(...)[.role(r)]` or `.global()` | Rule across entities |
| `property` | `Name ["label"] always\|eventually\|leads_to\|stable\|unchanged\|precedes\|can_reach(...)` | Temporal or order property; only `verify` checks it |
| `after` | `(UseCase).assert(...)` | Postcondition after the use case's last step |
| `relate` | `(Entity, Entity, 1:1\|1:N\|N:1\|N:M)` | ER relation and generated key (below) |
| `has_permission` / `requires_permission` / `requires_medium` | `(Actor, Permission)` / `(UseCase\|Api, Permission\|Medium)` | Access |
| `applies_to` | `(Nfr\|Condition, UseCase\|Api\|System)` | NFR scope; a condition with a body guards the use case |
| `qualifies` / `constrains` | `(Nfr\|Constraint, Quality)` / `(Constraint, UseCase\|Api\|System\|Entity\|Dto)` | Quality category; guardrail scope |
| `combines` / `holds` | `(Condition, Variation\|Entity.column)` / `(Condition, Entity.col == value, ...)` | Trace link; condition body (implicit AND) |
| `motivates` / `achieves` / `decides` | `(Requirement, Buc)` / `(Requirement, Goal)` / `(Adr, target)` | Traceability; a non-goal is a requirement with `status "out-of-scope"` and a `rationale` |

End every rule (`forbidden`, `invariant`, `required`, `exclusive`, `after(...).assert`) with
`.label("...")` in the business's words.

`relate(Child, Parent, N:1)` generates the key `{role}_{parent key}` on the child
(`order_id`). `1:N` puts it on the right entity; `1:1` puts it on the left, unique. `N:M`
needs `.via(Junction)`. The child side may be a range: `1..*:1` says every parent has a
child. Chain `.optional()`, `.on_delete(..)`, `.on_update(..)`, `.role(x)`, `.key(cols)`
(adopt declared columns), or `.references(cols)` (a parent unique set).

## Common mistakes

- Swapping arguments: CRUD takes `(UseCase|Api, Entity)`.
- Naming an event in `transitions` or `sets`: both take the use case (`TypeMismatch`).
- Quoting values: write `flag == true`, `@check(qty >= 1)`, `Enum(a, b)`, not `"true"`.
- A `@default` that is not a value of the column. An Enum takes a declared value
  (`@default(open)`, not `Open`); a `Bool` takes `true` or `false`. A `@null` column starts
  at null, so drop `@default(null)`:

<!-- expect-error: invalid enum value -->
```rdra
entity Item "Item" {
  id: Int @pk
  color: Enum(red, blue) @null @default(null)
}
```

- An inline `Enum` for a value the actor picks: `states` reports its unchosen values
  unreachable. Declare a `variation`.
- A `module` line that does not match the file path.
- Hand-writing a key column that `relate` generates (`FkConflict`, unless `.key(...)`
  adopts it).
- `performs(Actor, Buc)` for an actor who runs only some of the BUC's use cases (see
  [layout.md](layout.md#who-performs-a-use-case)).
- An `api` without `invokes`, or a `system` without `contains(System, Api)`.
- A `field` without `contains(Screen, Field)`.
- Bare column names in a multi-entity rule: write `Entity.column`.
- Expecting `rdra-ish fmt src/` to edit files: it prints to stdout. `fmt --write src/`
  rewrites; `fmt --check src/` lists the files that differ.
- Looking for `csv` rows on stdout: `csv` writes `out.<kind>.csv` (or `-o <base>`, giving
  `<base>.<kind>.csv`) and prints `wrote <path>`.
