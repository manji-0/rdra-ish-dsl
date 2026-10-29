# State Pattern Analysis

Use this reference when reviewing lifecycle completeness, reachable state patterns,
states the model cannot reach or leave, use cases that never run, entity constraints,
cross-entity rules, or a capped exploration.

## Concept

`check` and `states` explore one transition system by breadth-first search, the same
system `verify` hands to TLC. Each use case is one atomic action per transaction group
it writes, and an entity that no use case creates starts present. State axes come from
inline Enum, Bool, and nullable columns, and from Int columns that a `sets` or a rule
names. A column typed by a `variation` is an attribute the actor chooses: an axis only
when something names it, and its unreached values draw no warning.

The output is executable feedback about what the modeled use cases can actually reach.
`check` evaluates every `forbidden`, `invariant`, `required`, `exclusive`, multi-entity,
quantifier, key (`Unique_*`, `MinCard_*`), and `@check` rule and every
`after(...).assert` against the reachable states, and checks every state of each state
model (each value a `transitions` names or a creating use case sets) for a witness and a
way out. Every result holds within the `bounds:` line it starts with: one instance per
entity and Int `0..5`.

Use state analysis after entity structure is stable enough to know which fields can
change. Do not invent lifecycle detail during early BUC skeleton work.

## Commands

```sh
rdra-ish check src/
rdra-ish check src/ --entity <EntityId>
rdra-ish states src/
rdra-ish states src/ --entity <EntityId>
rdra-ish states src/ --format csv
rdra-ish states src/ --format json --entity <EntityId>
rdra-ish diagram src/ --kind state --format mermaid --buc <BucId>
rdra-ish diagram src/ --kind event-flow --format mermaid
```

`--entity` derives only the named entity's view and the rules that read it, each on
its exact cone, so its findings are decided. `--buc` drops the use cases outside the
BUC; whatever they could reach or unblock is then listed under `not checked:` instead
of reported.

## Reading Table Output

From the bundled sample of skill `rdra-ish-verify`:

```text
$ rdra-ish states ../rdra-ish-verify/samples/order.rdra
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
Entity: Order (注文)
  axes: status[pending|paid|shipped|delivered|cancelled], delivered_at[null|present]

  STATUS     DELIVERED_AT  INITIAL  TERMINAL  VIA
  ─────────  ────────────  ───────  ────────  ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  pending    null          yes      no        BucOrderLifecycle/PlaceOrder
  paid       null          no       no        BucOrderLifecycle/CapturePay, BucOrderLifecycle/PlaceOrder
  cancelled  null          no       yes       BucOrderLifecycle/CancelOrder, BucOrderLifecycle/PlaceOrder
  shipped    null          no       no        BucOrderLifecycle/CapturePay, BucOrderLifecycle/PlaceOrder, BucOrderLifecycle/ShipOrder
  delivered  present       no       yes       BucOrderLifecycle/CapturePay, BucOrderLifecycle/DeliverOrder, BucOrderLifecycle/PlaceOrder, BucOrderLifecycle/ShipOrder

  reachable: 5 / bound: 10
```

- `INITIAL`: a creating use case produces the pattern, or it is the start of a
  pre-existing entity.
- `TERMINAL`: a use case deletes the instance in that pattern, or no enabled use case
  moves it.
- `VIA`: the `BUC/use case` pairs on some path to the pattern.
- Missing combinations may be good or bad. Treat them as design questions, not
  automatic defects. `states --format json` adds a `witness` per pattern: a shortest
  path of use cases to it.

## Reading Findings

Every lifecycle finding prints its stable name after the severity, then the source
sites of the model elements involved:

```text
$ rdra-ish check ../rdra-ish-verify/samples/unwitnessed_state.rdra
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
error[Witness_Order_status_refunded]: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it
  state model: Order.status, entered by Refund (shipped -> refunded)   ../rdra-ish-verify/samples/unwitnessed_state.rdra:31:1
error[Deadlock_Order_status_shipped]: deadlock: Order#1 stays at status = shipped forever, and shipped is not an end state
  leaves shipped: Refund (shipped -> refunded)   ../rdra-ish-verify/samples/unwitnessed_state.rdra:31:1
    guard: condition RefundWindow                ../rdra-ish-verify/samples/unwitnessed_state.rdra:36:1
...
warning[Runs_Refund]: use case Refund can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Refund   ../rdra-ish-verify/samples/unwitnessed_state.rdra:16:1
```

A broken rule prints as `error[<rule name>]` (`Forbidden_Order_0`, `Inv_Order_0`,
`CrossForbidden_Order_0`, `Quantifier_Cert_0`, `Unique_*`, `MinCard_*`, `Check_*`,
`AfterAssert_<UC>_<n>`) with the rule's site and a shortest counterexample: one line
per use case step with its bound instances (`Place [Order#1, Payment#1]`) and site, and
under it the values that step changed.

## Common Findings

| Signal | Interpretation | Next action |
|---|---|---|
| `error[Witness_<E>_<col>_<v>]` | No sequence of use cases reaches that state | Add the `creates`, `transitions` arm, or condition that enters it, or drop the state |
| `error[Deadlock_<E>_<col>_<v>]` | An instance reaches a state that has a way out, and nothing ever moves it off | Read `leaves` and the `guard:` lines: fix the guard, or add the missing use case |
| `warning[Deadlock_...]: deadlock at the bound` | The way out needs a free slot of an entity it creates | The instance bound, not the model; check with `verify` at two instances |
| `warning[Runs_<UC>]` | The use case never runs; `needs:` or `guard:` lines say why | Fix the guard or the missing instance, or remove the use case |
| `error[<rule name>]` with a counterexample | A reachable state breaks the rule | Follow the steps: add the missing `sets` or transition, or fix the rule |
| Enum/Bool/nullable column never appears in axes | No modeled lifecycle effect | Add `sets`, `transitions`, or accept that it is not stateful |
| Enum variants reported unreachable that an actor simply chooses (payment method, channel) | The column is an attribute, not a lifecycle | Type it by a `variation`; `lint` `enum-attribute` finds these |
| Unexpected terminal pattern | Missing exit use case | Add a transition or effect, or mark the end state as intentional |
| Too many combinations | Effects are too broad | Make `sets` more precise or add rules |
| `PatternCapReached` / `StateCapReached` error | The exploration stopped at a cap, so no rule verdict is sound | Raise `--max-patterns` on `states`, or narrow with `--buc` |
| `not checked:` block | A scope or a cap left targets or findings undecided | Report them as not checked, never as passed |
| `UnbindableScope` | `.along(...)` names entities that no `relate` binds | Error. Declare the `relate`, or drop `.along` for a rule over every pair |
| `AmbiguousOwnerLink` | Two keys join the `.along` pair | Error. Name one with `.by(role)` |
| `UnbindableCondition` | A condition body is applied to a use case that writes nothing | Error. Apply it to a use case that writes, or drop the body |
| `ContradictoryDrivenComparison` | A use case drives a comparison one way while its own Int `sets` make it the other | Error. Fix the `sets` or drop the driving `sets` |
| `DoubleModeledEnum` | A use case that does not create the entity `sets` a column that `transitions` drive | Error. Use a `transitions` arm instead |
| `UnwrittenConstrainedVar` | A rule reads a column that no use case writes | Warning. The rule checks only the start value; add the `sets` or drop the rule |
| `NoCreationPath` | No use case creates the entity | Info. It starts present; add `creates` if that is wrong |
| `MinCardNeedsCreate` | A use case creates the parent of a `1..*` key and no child in the same transaction group | Info. `MinCard_*` fails after it; create a child in the same use case, or relax the card |
| `MinCardOfPreExistingParent` | The parent of a `1..*` key starts present but every child starts absent | Warning. `MinCard_*` fails at the start; add a `creates` for the parent, or relax the card |
| `PreExistingChildOfCreatedParent` | No use case creates the entity, but some use case creates the parent of its mandatory `relate` | Warning. It starts owned by a parent that does not exist yet; add a `creates` for the child, or make the `relate` `.optional()` |

## Formal verification (TLA+)

For the dedicated workflow, load skill **`rdra-ish-verify`**. Use `verify` when the
model has:

- `property` (`always`, `eventually`, `leads_to`), which `check` and `states` do not
  evaluate
- rules that need two instances: quantifiers over several rows, `.along` between
  entities with more than one linked row, and 1:1 keys (`Unique_*`)

```sh
rdra-ish verify src/ --backend tlc   # requires tlc on PATH
```

## How To Fix

- Use `transitions(Entity.col, UseCase, from -> to)` when a real lifecycle state
  machine exists; name the use case that drives each edge.
- Use `sets(UseCase, Entity, column == value)` for Enum values without a
  state machine, Bool flags, nullable and Int columns, and derived lifecycle effects.
- Use `triggers(Event, Buc)` for downstream BUC handoff, then refine to
  `triggers(Event, UseCase)` when the entry use case is known.

## Reporting Tips

Report state findings by their name and with the path that shows them: the entity, the
state, whether the pattern is initial or terminal, and the use cases on the
counterexample or witness. Name the `bounds:` line, and list anything under
`not checked:` as not checked.
