# Proofs for Any Number of Rows (`verify --prove`)

`verify` checks a model at a fixed number of instances, so a rule that holds at two can
still break at three. `verify --prove` is experimental. It asks z3 whether each state rule
(`forbidden`, `invariant`, `required`, `@check`, quantifier rules, `Unique_*`, `MinCard_*`,
`DanglingKey_*`)
holds for **any** number of rows and **any** non-negative Int value, at any time (`now`
has no top). It needs `z3` on `PATH`, or
`--z3 <path>`, or `RDRA_Z3`. The repository's `nix develop` shell provides it.

The proof never finds a counterexample. The bounded engines do that, in the same run.

## Reading the Output

Each rule prints one line before TLC runs:

- `proved: <rule>` — the rule holds for any number of rows and any Int value.
- `unproved: <rule>: not inductive; ...` — a counterexample to induction (CTI) follows.
- `unproved: <rule>: an initial state violates it` — a real violation. `check` and
  `verify` report it with a trace.
- `unproved: <rule>: z3 gave up on <action>` — the check timed out.
- `out of scope: <rule>: <why>` — the encoding cannot state the rule.
- `[not enforced by the SQL schema: <why>]` after a name — `export --kind sql` writes no
  `CHECK` for the rule, so the application keeps it. Proved, it is an obligation the use
  cases discharge; unproved, say so when the user designs the database.
- `lemma: <text>` — a helper fact the proof kept: structural, or mined from the states BFS
  reached.

The `OK:` line ends with `; <n> of <m> rules proved for any number of rows and any Int
value`. An unproved
rule fails the run only with `--deny-unproved`.

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
names the step and the rows it binds. Each indented line is one row: its existence, its
columns before the step, and after `->` what the step changed. A split use case's run mark
(`_run_<UseCase>`) or an event mark (`_raised_<Event>`) shows too when the step or the rule
reads it. The rule's own rows are
listed too.

**The state before the step need not be reachable.** Decide which case it is:

1. **The state is reachable.** Then the rule really breaks, usually at more instances than
   the bounded run uses. Above, Ship and Reship each link a Shipment to one Order. `verify`
   at two instances reports the same violation with a trace. Fix the model, not the proof.
2. **The state is not reachable.** Then some fact that rules it out is missing from the
   proof. Write that fact as a lemma (below).

To tell them apart, look for a run that reaches the state before the step. A `verify`
counterexample for the same rule settles it.

## Writing a Lemma

A lemma is an ordinary `forbidden` or `invariant` with `.lemma()`. It states the fact that
rules the CTI's state out. In this model only Ship creates a Shipment, and it ships the
Order, so "an Order that is still placed has no Shipment" holds:

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

- **`check` checks a lemma.** It checks it as a rule, so a wrong lemma fails there with a
  trace: run `rdra-ish check` before trusting one. `verify` runs no TLC cone for a lemma; the
  proof uses a lemma only once it proves it.
- **The proof uses a lemma it can prove.** A lemma becomes a candidate fact, like a rule.
  It prints as `proved: lemma <name>` or `unproved: lemma <name>`, and `proof: your
  lemmas: ...` counts lemmas apart from the rules.
- **A lemma can read only what a rule can read.** It cannot name a split use case's run
  mark (`_run_<UseCase>`) or an event mark (`_raised_<Event>`). When a CTI turns on one,
  ask which columns keep that state unreachable one step earlier, and state that instead.
- **When a lemma's own CTI steps back one state, generalize it.** If a lemma about status
  `investigating` breaks at the step that enters `investigating`, widen it to every status
  that can lead there (`status in (received, responded, investigating)`), rather than adding
  a second lemma.

Propose at most one lemma per CTI. Check it with `rdra-ish check`, then rerun
`rdra-ish verify --prove`. Stop when the CTI's state is reachable (case 1), or after three
lemmas leave the rule unproved: report the rule with its last CTI.
