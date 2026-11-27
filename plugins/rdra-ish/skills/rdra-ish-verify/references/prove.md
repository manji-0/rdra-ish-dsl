# Proofs for any number of rows (`verify --prove`)

<!-- derived-from ../../../docs/formal-verification.md#proofs-for-any-number-of-rows -->

`verify` checks a fixed number of instances, so a rule that holds at two can break at
three. `verify --prove` (experimental) asks z3 whether each state rule (`forbidden`,
`invariant`, `required`, `@check`, quantifiers, `Unique_*`, `MinCard_*`, `DanglingKey_*`)
holds for any number of rows, any non-negative Int, and any time. It needs `z3` on `PATH`,
`--z3 <path>`, or `RDRA_Z3`. The proof never finds a counterexample; the bounded engines in
the same run do.

## Reading the output

Each rule prints one line after the `proof (...)` header:

- `proved: <rule> (for any number of rows and every --int-range-max from <a> to <b>, in the
  whole model)`: the rule holds at any number of rows and at every Int range the model builds
  at: from the least one its Int literals fit to 255 (254 with a nullable Int column). No Int
  column, no range. Under a scope flag (`--buc`, `--entity`, `--without-usecase`,
  `--without-buc`, `--actor`) it ends `in the selected scope: <flags>`: the rule holds in the
  system without the use cases the scope drops, and may still break in the whole model.
- `unproved: <rule>: not inductive; ...`: a counterexample to induction (CTI) follows.
- `unproved: <rule>: an initial state violates it`: some start of any number of rows and
  any Int value violates it; `check` reports it with a trace when it lies within the bounds.
- `unproved: <rule>: z3 gave up on <action>`: the check ran out of z3 resource units. The
  budget counts solver work, so the answer is the same on every machine.
- `unproved: <rule>: the canary `false` survived initiation ...`: the proof found no state to
  start from, so no rule is proved. This is a bug in rdra-ish; report it.
- `out of scope: <rule>: <why>`: the encoding cannot state the rule.
- `refuted: <rule> (BFS counterexample at <n> instances)`: BFS violated the rule in the same
  run, with the trace it prints, so the proof did not try it. Fix the model, not the proof.
  Where the entities the counterexample needs have different counts (`--instances E=k`, or
  more rows of an entity than the cone counted), `<n>` lists each, as `Loan=4 Member=3`.
- `[not enforced by the SQL schema: <why>]` after a name: `export --kind sql` writes no
  `CHECK` for it, so the application keeps it.
- `lemma: <text>`: a helper fact the proof kept.
- `escalated: <rule>: ok at 3 instances, counterexample at 4 instances (BFS on its cone; ok
  at 2)`: the rule was unproved and BFS passed it, so BFS checked its cone again at one and two
  instances more. A counterexample there is an error with its trace (`counterexample (<k>
  steps, at <n> instances):`) and fails the run. `not decided at <n> instances` means the
  run outgrew ten times the store of the cone at the run's bounds, or met the memory limit or
  `--max-states`; it decides nothing. Under `--instances E=k` the counts there list each
  entity of the cone, as `ok at Loan=3 Member=2 instances`.

The `OK:` line ends with `; <n> of <m> rules proved for any number of rows and any Int
value`. An unproved rule fails the run with `--deny-unproved`, or when its escalation finds a
counterexample. A cone that BFS stopped
at the memory limit is decided when z3 proves every rule in it: its line reads `ok (...,
decided by proof: ...)`.

```text
$ rdra-ish verify --prove --instance-count 1 samples/one_to_one_shared.rdra
...
unproved: Unique_Shipment_Order: not inductive; Ship(Order#1, Shipment#1) breaks it from a state where it, every proved rule, and every kept lemma hold (not necessarily reachable)
  Order#1 (present) status=placed -> status=shipped
  Shipment#1 (absent -> present) Order=Order#2 -> Order=Order#1
  Shipment#2 (present) Order=Order#1
...
proof: 0 proved, 1 unproved, 0 out of scope
error[Unique_Shipment_Order]: invariant Unique_Shipment_Order is violated: 1:1 key is shared: two 'Shipment' rows belong to one 'Order' through 'Order', witnessed by Shipment[1](); Shipment[2]()
...
  counterexample (3 steps, at 2 instances):
...
escalated: Unique_Shipment_Order: counterexample at 2 instances (BFS on its cone; ok at 1)
...
```

## Reading a CTI

A CTI is one step from a state where the rule, every proved rule, and every kept lemma hold. The first line
names the step and the rows it binds. Each indented line is one row: present or absent, its
columns before the step, and after `->` what the step changed. A run mark
(`_run_<UseCase>`) or event mark (`_raised_<Event>`) shows when the step or rule reads it.

The state before the step need not be reachable. Decide which case it is:

1. **Reachable.** The rule really breaks, usually at more instances than the bounded run
   uses. Above, Ship and Reship each link a Shipment to one Order; the escalation finds it at
   two instances and reports it with a trace. Fix the model, not the proof.
2. **Not reachable.** A fact that rules the state out is missing from the proof. Write it
   as a lemma. When the prover found one itself, it prints a
   `lemma: from the CTI of <rule>: forbidden(...).lemma()` line; copying that text into the
   model keeps the fact in the model and lets `check` test it at bounds.

A `verify` counterexample for the same rule settles case 1.

## Writing a lemma

A lemma is a `forbidden` or `invariant` ending in `.lemma()`. Here only Ship creates a
Shipment, and it ships the Order, so a placed Order has no Shipment:

```rdra
entity Order "注文" {
  id: Int @pk
  status: Enum(placed, shipped) @default(placed)
}

entity Shipment "出荷" {
  id: Int @pk
}

usecase Place "注文確定"
usecase Ship "出荷"

creates(Place, Order)
creates(Ship, Shipment)
updates(Ship, Order)
transitions(Ship, Order.status, placed -> shipped)
relate(Shipment, Order, 1:1)

invariant(Order).when(status == placed).none(Shipment).label("a placed order has no shipment yet").lemma()
```

- `check` checks a lemma as a rule, so a wrong one fails there with a trace. The proof uses
  a lemma only once it proves it (`proved: lemma <name>`).
- When a CTI turns on a mark, a lemma can read it; an ordinary rule cannot. Write it among a
  `forbidden`'s conditions or in an `invariant`'s `.when(...)` and `.then(...)`:
  - `Door.in_run(Open)`: a step of the split use case Open is in progress on the row (its
    `_run_Open` is not `none`).
  - `Door.in_run(Open, 1)`: the row's run has done step 1 and not the next.
  - `Sign.raised(EvShut)`: the row's `_raised_EvShut` latch is set.

  For example, `forbidden(Light, Door, Light.state == off, Door.in_run(Open)).global().lemma()`
  says no light is off while a door is between Open's steps. Prefer columns when columns
  can state the fact: a mark is the only difference only when a reachable state has the
  same columns.
- When a lemma's own CTI steps back one state, widen it to every status that leads there
  rather than adding a second lemma.

Propose at most one lemma per CTI. Run `rdra-ish check`, then `rdra-ish verify --prove`
again. Stop when the CTI's state is reachable, or after three lemmas leave the rule
unproved; report the rule with its last CTI.
