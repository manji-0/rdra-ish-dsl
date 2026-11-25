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

- `proved: <rule>`: the rule holds for any number of rows and any Int value.
- `unproved: <rule>: not inductive; ...`: a counterexample to induction (CTI) follows.
- `unproved: <rule>: an initial state violates it`: a real violation; `check` reports it
  with a trace.
- `unproved: <rule>: z3 gave up on <action>`: the check ran out of z3 resource units. The
  budget counts solver work, so the answer is the same on every machine.
- `out of scope: <rule>: <why>`: the encoding cannot state the rule.
- `[not enforced by the SQL schema: <why>]` after a name: `export --kind sql` writes no
  `CHECK` for it, so the application keeps it.
- `lemma: <text>`: a helper fact the proof kept.

The `OK:` line ends with `; <n> of <m> rules proved for any number of rows and any Int
value`. An unproved rule fails the run only with `--deny-unproved`. A cone that BFS stopped
at the memory limit is decided when z3 proves every rule in it: its line reads `ok (...,
decided by proof: ...)`.

```text
$ rdra-ish verify --prove samples/one_to_one_shared.rdra
...
unproved: Unique_Shipment_Order: not inductive; Ship(Order#1, Shipment#1) breaks it from a state where it and every proved rule hold (not necessarily reachable)
  Order#1 (present) status=placed -> status=shipped
  Shipment#1 (absent -> present) Order=Order#2 -> Order=Order#1
  Shipment#2 (present) Order=Order#1
...
proof: 0 proved, 1 unproved, 0 out of scope
...
```

## Reading a CTI

A CTI is one step from a state where the rule and every kept lemma hold. The first line
names the step and the rows it binds. Each indented line is one row: present or absent, its
columns before the step, and after `->` what the step changed. A run mark
(`_run_<UseCase>`) or event mark (`_raised_<Event>`) shows when the step or rule reads it.

The state before the step need not be reachable. Decide which case it is:

1. **Reachable.** The rule really breaks, usually at more instances than the bounded run
   uses. Above, Ship and Reship each link a Shipment to one Order; `verify` at two
   instances reports it with a trace. Fix the model, not the proof.
2. **Not reachable.** A fact that rules the state out is missing from the proof. Write it
   as a lemma.

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
- A lemma reads only what a rule reads: no run mark or event mark. When a CTI turns on one,
  state the columns that keep that state unreachable instead.
- When a lemma's own CTI steps back one state, widen it to every status that leads there
  rather than adding a second lemma.

Propose at most one lemma per CTI. Run `rdra-ish check`, then `rdra-ish verify --prove`
again. Stop when the CTI's state is reachable, or after three lemmas leave the rule
unproved; report the rule with its last CTI.
