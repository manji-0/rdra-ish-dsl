# Formal Verification (TLA+ / TLC)

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->
<!-- derived-from ./language-reference.md#temporal-path-properties -->
<!-- derived-from ./cli-reference.md#export -->
<!-- derived-from ./cli-reference.md#verify -->

RDRA-ish prints its lifecycle transition system as **TLA+** and optionally checks it
with **TLC**. `rdra-ish states` and `check` explore the same system by breadth-first
search (BFS) at one instance per entity. `verify` runs TLC on it at two instances by
default and adds the temporal properties that BFS does not evaluate. See
[state-derivation.md](./state-derivation.md) for the system itself.

## Commands

```
rdra-ish export <INPUTS...> --kind tla [-o <OUT>] [--instance-count N] [--int-range-max N]
rdra-ish verify <INPUTS...> --backend tlc [-o <OUT_DIR>] [--instance-count N] [--int-range-max N]
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` for the whole model |
| `verify --backend tlc` | Print one spec per cone of influence, then run `tlc` / `tlc2` from `PATH` |
| `states`, `check` | BFS on the same system, with no TLC dependency |

`--instance-count` (default `2`) sets `InstanceCount` (`1..=255`; `1` warns that
singleton ids cannot distinguish instances). `--int-range-max` (default `5`) sets
`IntRange == 0..N` (`1..=255`). Both commands fail on the Error diagnostics of the
transition system they print, for example `IntLiteralOutOfBounds` or `DoubleModeledEnum`.

If `-o` ends with `.tla`, the sibling `.cfg` is written next to it and the file stem is the
module name. If `-o` is a directory (or has no extension), `export` writes
`<dir>/RdraSpec.tla` and `<dir>/RdraSpec.cfg`. `verify -o <dir>` writes one
`Cone_<first check>.tla` and `.cfg` pair per cone, the whole-model `RdraSpec` pair
when the model has temporal properties, and `cones.txt`.
Without `-o`, `verify` writes them to a temp dir. It deletes the dir when every check
passes, and keeps it and prints `specs: <dir>` when one fails. The TLC heap settings
(`RDRA_TLC_HEAP`, `JAVA_TOOL_OPTIONS`) are in [cli-reference.md](./cli-reference.md#verify).

The `.cfg` names `SPECIFICATION Spec`, `INVARIANT TypeOK`, one `INVARIANT` per
constraint, one `PROPERTY` per `after` assertion and temporal property, and
`CHECK_DEADLOCK FALSE`. Lifecycle models often end in terminal states with no enabled
action, and `Spec` allows stuttering.

## Mapping

| RDRA | TLA+ |
|---|---|
| Entity | `<E>_Ids == 1..InstanceCount` and `<E>_exists`, a function over the ids with values `"absent"`, `"present"`, `"deleted"` |
| Enum, Bool, `@null`, Int, Money, Decimal, Date, or DateTime column the system reads or writes; a variation-typed column that something names | `<E>_<col>`, a function over `<E>_Ids`. Int-like columns range over `IntRange` (nullable ones add `"null"`). |
| `relate` N:1, 1:N, 1:1 | `<Child>_owner_<Parent>`, a function from child ids to parent ids. An `.optional()` relate adds `0` for an unlinked child. |
| Use case | One operator `<UC>(p0, ...)` per transaction group of `UsecaseTx::groups()` (`<UC>_1`, `<UC>_2`, ... when the writes form several groups). The sequence diagrams draw the same groups. `Next` is the disjunction of `\E` over the parameters. |
| `creates` | `FirstAbsent(<E>_exists, p)` and `<E>_exists' = ... "present"`. A deleted slot is never reused. |
| `deletes` | `<E>_exists'` becomes `"deleted"`, columns and owners return to the base value. |
| `sets`, `transitions(E.col, UC, a -> b)` | Primed assignments. A transition guards `col = a`. `UNCHANGED` lists every variable the action does not assign. |
| Variation column of a created instance | `\E v0 \in {values} : col' = ...`, so TLC explores every value. `@default` does not pin it. |
| `applies_to(Condition, UseCase \| Api)` with a `holds` body | A guard conjunct on the use case's operator. A condition on a created instance is evaluated on the primed variable, after the assignments. |
| Comparison proposition driven by `sets(..., cmp, bool)` | A Boolean variable. An undriven comparison is TLC arithmetic on the Int columns. |
| `col < now` | A global `now \in IntRange` and a `Tick` action that adds one while `now < max`. |
| Entity-local and multi-entity `forbidden`, `invariant`, `required`, `exclusive` | One `INVARIANT` each, named `<Kind>_<Entity>_<n>` (`Forbidden_Order_0`, `Inv_Order_0`, `CrossForbidden_Order_0`). Each ranges over the present instances. |
| `.along(...)` | Quantified over `<E>_Ids`, joined by `<Child>_owner_<Parent>` equalities. |
| `col: T @check(expr)` | `INVARIANT Check_<Entity>_<column>_<n>` over the present instances. A nullable operand is guarded: `qty: Int @null @check(qty >= 1)` prints `(E_qty[b0] = -1) \/ (E_qty[b0] # -1 /\ E_qty[b0] >= 1)`. |
| `relate(Child, Parent, 1:1)` | `INVARIANT Unique_<Child>_<role>` (role defaults to the parent id): `\A b0, b1 \in <Child>_Ids`, two present children are the same or have different owners. An unlinked optional child owns nothing. |
| `relate(Child, Parent, 1..*:1)` (child-side minimum) | `INVARIANT MinCard_<Child>_<role>`: `\A b0 \in <Parent>_Ids`, a present parent has `\E b1 \in <Child>_Ids` present with `<Child>_owner_<Parent>[b1] = b0`. Checked in every state, so a parent created without a child in the same transaction, or a deleted last child, is a counterexample. |
| Pre-existing child of a `relate` | `Init` links the owner: `\in [<Child>_Ids -> <Parent>_Ids]` for N:1 (start `Any`), `[i \in <Child>_Ids \|-> i]` for 1:1 and for a child-side minimum (start `Identity`). An `.optional()` key under a created parent starts unlinked (`Base`). |
| An action that creates a parent and updates an existing child | The action links the child. An `.optional()` key guards the assignment with an unlinked owner, so a linked child keeps its first parent. A mandatory key assigns without a guard, which moves the child. |
| `when(...).none/has(...)` | `\A` / `\E` over the ids, named `Quantifier_<Entity>_<n>`. A `relate` joins the sides by owner; without one the related side is independent. |
| `after(UC).assert(...)` | `PROPERTY AfterAssert_<UC>_<n>`: `[][(\A p : UC(p) => post')]_vars`. It checks every step of the use case's actions. |
| `property` with `always`, `eventually`, `leads_to` | A named formula and a `PROPERTY`, quantified per present instance |
| Any `eventually` / `leads_to` property | `Spec` includes `WF_vars(Next)` |
| Domain of every variable | `INVARIANT TypeOK`, kept apart from the business rules |

Example temporal and Int syntax:

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

sets(Restock, Item, stock == 3)
forbidden(Item, stock < selling)

after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)

when(Cert, status == revoked).none(Assign.status == active)
// equivalent: when(Cert.status == revoked).none(Assign.status == active)

forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured)
  .along(Order, Payment)
```

Operators:

- `always(expr)` → `[]expr`
- `eventually(expr)` → `<>expr`
- `leads_to(p, q)` → `p ~> q`
- Connectives inside expressions: prefer `and` / `or` / `not` (`/\` `\/` `~` aliases)
- Property label string is optional (`property StockOk always(...)`)

`rdra-ish states` does not evaluate path properties, and `check` prints a `note:` that it
did not model-check them. Use `export` or `verify`.

## What `verify` checks

`verify` restricts the system to the cone of influence of each invariant and
`after` assertion, prints that cone, and runs TLC on each cone in parallel. A cone
reaches exactly the whole model's states projected onto it, so the union of the cone
verdicts is the whole-model verdict. Cones over the same variables share one run.
This keeps clinic-ops tractable: a whole-model run at `InstanceCount=2` took 62 s and
11 GB on ec-site and did not finish in 300 s on clinic-ops.

Temporal properties need fairness on the whole system, which does not restrict to a
cone. `verify` checks them on the whole model, and only when the model declares any.

Before TLC runs, `verify` explores each entity's cone with BFS at the same bounds, capped at
2,097,152 states per cone (`Caps::VERIFY`, named in the `bounds:` line). Each state of a
state model must have a witness there (see
[state-derivation.md](./state-derivation.md#state-models-and-witnesses)). A `NoWitness`
error fails `verify` even when every TLC check passes: the run prints `tlc_result: ok` and
no `OK:` line, and exits 1. A deadlock, an instance that no path moves off a state with a
way out (see [state-derivation.md](./state-derivation.md#deadlocks)), fails it the same way;
one whose way out lacks only a free slot is a warning at the bound. TLC's own deadlock check
stays off: every bounded model ends with no enabled action, and an always-enabled loop would
hide a stuck instance. The same pass finds each use case that never runs (see
[state-derivation.md](./state-derivation.md#use-cases-that-never-run)) and prints it as a
warning, which does not fail the run. A cone that stops at the cap decides nothing about its
unwitnessed states, its deadlocks, or its actions with no edge, and `verify` lists each
under `not checked:`. The
pass ends with a `state models (BFS on N entity cones): ...` summary line. On clinic-ops
this pass takes about 13 s and 350 MB; its largest cone is `Charge` (969,167
states).

```
$ rdra-ish verify samples/formal-verification/order.rdra --backend tlc
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, checked on the whole model (fairness does not restrict to a cone)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
RdraSpec [PaidLeadsToShipped, EventuallyTerminal]: ok (31 states)
tlc_result: ok
OK: no errors within 2 instances per entity, Int 0..5
```

A violated invariant or `after` assertion prints in model terms first, the same way
`check` prints it: the rule's `path:line:col`, one line per step with the use case, its
bound instances (`Place [Order#1, Payment#1]`), and its site, and under each step the
values it changed. `verify` gets that trace by decoding TLC's states and replaying them
through the step relation BFS uses, and builds the message with the code `check` uses.
TLC's own `violated: Error: ... is violated.` line and its trace follow, with one
`name[slot] = value` line per variable and slot. The name is the constraint's name, so it
points at the rule that broke. A step that no move of the cone explains is a tool bug, and
`verify` says the engines disagree instead of printing the model-term block. The full
output is in [cli-reference.md](./cli-reference.md#verify). A temporal property's
counterexample prints only TLC's trace.

### Part of a model

<!-- derived-from ./cli-reference.md#partial-runs -->
`verify --entity E` checks the entity's view and the invariants and after-asserts that
read `E`, each on its own cone, so each verdict is the whole-model verdict for that target.
`verify --buc B` builds the system with only the use cases reachable from `B`, so it cannot
decide whether a state has a witness, a use case runs, or an instance gets stuck. A scoped
run checks no temporal property, because it has no whole model. A build error that no
selected cone reads is deferred: one entity that cannot be exported does not block the
rest. The whole run still fails on it:

```
$ rdra-ish verify samples/formal-verification/partial_scope.rdra --entity Order
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded
scope: --entity Order
cones: 1 for 1 invariant(s) and 0 after-assert(s)
temporal properties: none, no whole-model run
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
state models (BFS on 1 entity cone): 1 state model, 2 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0]: ok (7 states)
tlc_result: ok
OK: no errors in the selected scope, within 2 instances per entity, Int 0..5
```

The rules for each flag and for deferral are in
[cli-reference.md](./cli-reference.md#partial-runs).

## Approximations and limitations

- **Bounds.** A `verify` OK holds for the stated `InstanceCount` and `IntRange` and says
  nothing outside them. The `.tla` header comment and the `bounds:` line of `verify`
  state both, and so does the final `OK: no errors within ...` line. A scoped run says
  `OK: no errors in the selected scope, within ...` and holds only for the selected
  targets. A deleted slot is never reused, so an entity is created at most
  `InstanceCount` times.
- **Conditions are checked.** A `holds` body is a guard on each use case it applies to.
  A body applied to a use case that writes nothing is `UnbindableCondition`, an error.
- **One use case across entities.** A use case is one atomic action per transaction
  group it writes (`Pay(p0, p1)` moves the Order and its owned Payment in one
  step), so `cross_order_payment` passes. A use case whose writes span k groups is k
  actions `<UC>_1..k` that interleave freely, so `cross_order_payment_split` (`Pay`
  through two APIs) fails. `quantifier_none` still fails.
- **Int arithmetic.** Comparisons and `sets(..., col == N)` on Int, Money, Decimal,
  Date, and DateTime columns use TLC integers on `IntRange` (default `0..5`, override
  with `--int-range-max`). A literal outside that window is
  `IntLiteralOutOfBounds` in `check` and `states` too: raise `--int-range-max` or lower
  the literal. `@default(0)` is accepted for Int columns.
- **`now`.** `now` is a global Int clock with a `Tick` action. Nothing pins a column
  that `now` is compared to, so the clock can tick past it and TLC finds a violation of
  `forbidden(col < now)` (see `now_coupon.rdra`).
- **`WF_vars(Next)`.** Fairness is on the whole `Next` disjunction, not per action. That
  suffices for simple `eventually` and `leads_to` on small models. It does not replace
  per-action weak fairness.
- **`.along`.** An `.along` path needs a `relate` that binds each adjacent pair. Without
  one, the rule is `UnbindableScope`, an error in every command, not a dropped filter.
- **Quantifiers.** `has` and `none` range over the `InstanceCount` ids. Where a `relate`
  declares the relationship, an owner function links the related instances. Otherwise the
  related side is independent.
- `check` and `states` run BFS on the same system at one instance per entity, so `check`
  finds the same violations as TLC in that window. The test `engine_equivalence`
  (`crates/rdra-ish-cli/tests/engine_equivalence.rs`) runs BFS and TLC on every cone of
  every sample and compares the reached states and the verdict of each invariant and
  `after` assertion. Every trace TLC prints for a violation must replay through the BFS
  step relation and end in the check TLC named. CI sets `RDRA_REQUIRE_TLC=1`, so a
  missing `tlc` fails the job.
- User-facing state diagnostics for multi-entity rules say **multi-entity**. The
  `Cross*` prefix in some constraint names is historical.

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` and `rdra-ish check` for quick feedback.
3. `rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla` and inspect the Spec and `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH` (`nix develop` provides it).
5. `rdra-ish verify samples/formal-verification/order.rdra --backend tlc -o /tmp/rdra-tla` to run TLC.

Agent skill: `skills/rdra-ish-verify/` (routing + references + bundled samples).
Install: see [`skills/README.md`](../skills/README.md) (`npx skills add` / `gh skill install`).

## Samples

Canonical files: `skills/rdra-ish-verify/samples/` (each file is **standalone** —
do not `check` the whole directory; ids such as `Order` collide). In this repo,
`samples/formal-verification/` and `samples/formal-verification-fail/` are
symlinks to those skill-bundled files (CLI tests keep working after skill install).

| Sample | `check` and TLC | Focus |
|---|---|---|
| `order.rdra` | pass, pass | Lifecycle invariants + `after.assert` + `leads_to` / `eventually` + `WF_vars` |
| `int_stock.rdra` | pass, pass | Int columns, arithmetic `forbidden`, Int temporal property |
| `now_coupon.rdra` | fail, fail | `now` and `Tick`; the invariant finds `expired_at < now` |
| `cross_order_payment.rdra` | pass, pass | Multi-instance + `.along` (Pay is one atomic action, so Cancel cannot interleave; Cancel deletes the pending Payment) |
| `cross_order_payment_split.rdra` | fail, fail | The same model with Pay through two APIs: two transaction groups, so Cancel interleaves between `Pay_1` and `Pay_2`; Cancel does not delete the Payment here |
| `quantifier_none.rdra` | fail, fail | `when(...).none` (Revoke while Assign stays active) |
| `untriggered_sets.rdra` | fail, fail | `sets` from a use case that drives no transition (Flag then Ship reaches the forbidden state) |
| `after_assert_unmet.rdra` | fail, fail | `after(DeliverOrder).assert` without the `sets` that makes it true |
| `one_to_one_shared.rdra` | pass, fail | `relate(.., 1:1)` adds `Unique_Shipment_Order`; Ship and Reship each link a Shipment to the same Order, which takes two instances |
| `min_card_unmet.rdra` | fail, fail | `relate(OrderLine, Order, 1..*:1)` adds `MinCard_OrderLine_Order`; Place creates an Order without a line |
| `min_card_met.rdra` | pass, pass | The same model with Place creating the Order and its first line in one transaction. At one instance `check` warns that AddLine never runs: an Order exists only with the one OrderLine slot taken. At two instances it runs |
| `check_unmet.rdra` | fail, fail | `qty: Int @default(1) @check(qty >= 1)` adds `Check_CartLine_qty_0`; ClearQty sets `qty` to 0 |
| `check_met.rdra` | pass, pass | The same check with a use case that keeps `qty` in range; `forbidden` reads the derived `bulk: Bool @derived(qty >= 3)` as its expression |
| `unwitnessed_state.rdra` | fail, pass | `refunded` has no witness: Refund needs `shipped` and a refund window that holds only while `pending`. TLC passes; the BFS witness check fails `check` and `verify`, a shipped Order is a deadlock (Refund is its only way out), and both warn that Refund never runs (its guards never hold together) |
| `deadlock.rdra` | fail, pass | An Order stays `paid` forever: Ship leaves `paid` only while a packing condition holds, and it holds only while `pending`. Express ships a pending Order, so `shipped` has a witness. TLC passes; the BFS deadlock check fails `check` and `verify`, and both warn that Ship never runs |
| `partial_scope.rdra` | fail, not run | Audit's `forbidden` reads the arithmetic `@derived` column `score`: a `DerivedColumnRead` build error stops `check` and `verify` before TLC. With `--entity Order` both defer it under `not checked:` and pass; TLC checks `Forbidden_Order_0` |
| `fail/order.rdra` | fail, fail | Intentionally unsafe (reachable invariant violation) |

BFS and TLC give the same verdict on every row at the same bounds. `check` runs at one
instance per entity, where `Unique_*` holds vacuously.
