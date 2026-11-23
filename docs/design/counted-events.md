# Counted events: a handler runs once per raise

<!-- supersedes ./native-temporal-verification.md#goals-and-non-goals -->
<!-- derived-from ./event-order.md#the-rules -->
<!-- constrained-by ./event-order.md#cones-and-the-reduction -->
<!-- constrained-by ../compatibility.md -->

Design for #220. The event order of #112 records a raise as a latch: after the first raise
on a row, the triggered use case runs as often as its states and guards allow, and a second
raise queues nothing ([event order, rule 9](./event-order.md#the-rules)). The native plan
lists counting as a non-goal
([Goals and non-goals](./native-temporal-verification.md#goals-and-non-goals)). This note
reopens that non-goal for one opt-in form, `.counted(k)` on a `triggers` edge, and leaves
every model without it as it is: the same transition system, the same names, the same
verdicts.

## Decisions in one place

| # | Question | Decision |
|---|---|---|
| 1 | Syntax | `triggers(Ev, UC).counted(k)`, a chain method on the `triggers` line of an event and a use case, with one integer argument `k` from 1 to 16. |
| 2 | What is counted | One counter per counted edge (the event and its handler) per row of each mark entity of the event. Two handlers of one event never share a counter. |
| 3 | A raise | The last step of every raiser adds one to the counter on its row. |
| 4 | A handler run | The handler's first step runs only on a row whose counter is above zero and takes one off it. |
| 5 | A raise at `k` | The violation `EventOverflow_<E>_<Ev>_<UC>`, a state invariant. The raiser never waits. The counter moves to an absorbing value `over`. |
| 6 | After an overflow | The counter stays at `over`, a raise changes nothing, and the handler runs as under the latch: as often as its states and guards allow. |
| 7 | Cones | A counted mark is always exact. Every cone that holds a raiser or a handler holds the counter and every action that writes it. Nothing is havocked. |
| 8 | The reduction | A counted mark is never an input of an independent sub-lifecycle. The counter, its raisers, and its handler are always on one side of the split. |
| 9 | TLA+ | `<E>_pending_<Ev>_<UC>`, a function from `<E>_Ids` to the strings `"0"`..`"k"` and `"over"`. The overflow rule is one more `INVARIANT`. |
| 10 | z3 | In scope. The counter is an Enum var and its writes are transition arms, which the encoding already has. |
| 11 | Code generation | Out of scope. A use case that raises or handles a counted event is reported as unsupported, as other effects the generator does not print. |
| 12 | What is rejected | `.counted` on a timer, a BUC, or an external system; on an event that is free or a handler a free trigger also starts; on an edge with no mark entity; a raiser that binds a mark entity twice; and `k` outside 1..16. |

## Syntax

<!-- constrained-by ../language-reference.md#chain-methods -->

A chain method on `triggers`, the form every per-edge option of the language takes
(`.when` on `transitions`, `.apart()` on `creates`):

```rdra
raises(ReceivePayment, EvPaymentReceived)
triggers(EvPaymentReceived, PostToLedger).counted(2)
```

`k` is a bare integer. It is the first chain method that takes a number, so the parser
grows one argument form, an integer literal, for chain calls only. A predicate's own
arguments are unchanged. `k` must be at least 1, and at most 16, since every row holds one of
`k + 2` values and the state space grows with their product. The analysis reports
`InvalidChainArgument` for a missing, repeated, or non-integer argument and for a value
outside 1..16, and for `.counted` on a `triggers` line whose target is not a use case or
whose source is a timer.

The option is per edge because the counter belongs to the handler. Under the latch, one mark
per `(entity, event)` serves every handler, because nothing consumes it
([event order, decision 3](./event-order.md#decisions-in-one-place)). A counter is consumed,
so two handlers on one counter would take each other's raises. Each counted edge therefore
has its own counter, and a latch edge of the same event keeps the shared latch.

## Semantics

<!-- derived-from ./event-order.md#the-rules -->

Read with the [rules of the event order](./event-order.md#the-rules). Everything there
holds for a counted edge, except rules 3 (the mark), 6 (raising) and 9 (what is not said),
which become:

1. **The counter.** For each counted edge `triggers(Ev, UC).counted(k)` and each mark entity
   `E` of `Ev` the handler reads (rule 5 of the event order picks them), the system has
   `<E>._pending_<Ev>_<UC>` per slot, with the values `0`, `1`, .., `k`, and `over`, `0` at
   the start. A delete does not reset it.
2. **Raising.** The last step of each raiser takes the counter of its row from `i` to
   `i + 1` for `i < k`, from `k` to `over`, and leaves `over` as it is. It has no guard for
   it, so a raiser never waits.
3. **Handling.** The first step of the handler (each variant, `<UC>_by_<Ev>`, when several
   events trigger it) runs only on a row whose counter is not `0`. The guard is
   `GuardWhy::Raised`, as the latch's, so `NeverRuns`, the blocked-guard text of a deadlock,
   and a `Triggered` parameter read it as before. The step takes the counter from `i` to
   `i - 1` for `1 <= i <= k`, and leaves `over` as it is.
4. **Overflow.** For each counter, the state invariant `EventOverflow_<E>_<Ev>_<UC>` holds
   when no present row of `E` has the counter at `over`. A raise at `k` violates it, and the
   counterexample is the path of `k + 1` raises with fewer than one handler run between.

Why a violation and not a blocked raise. A raiser that waits for its handler hides
violations: the review's `wait.rdra` of #112 lost `Forbidden_Ticket_0` and gained a
spurious deadlock under every waiting shape
([answer 1](./event-order.md#answers-to-the-eight-questions)). The overflow is a fact the
author wants to see, so it is a rule the engines report, with a trace, and the raiser keeps
running. The bound `k` is part of the model: when `k` is the real capacity of a queue, the
overflow is a defect; when it is not, the overflow says that `k` is too small for the
model, and the author raises it.

Why `over` is absorbing and the handler keeps running there. After the overflow the count
is lost, and any value below `over` would invent a drain the model never stated. At `over`
the handler behaves as under the latch, so a counted edge never has fewer behaviors after an
overflow than the latch has, and nothing a later check would find is hidden by the
overflow itself. Before the overflow the counted handler is enabled only where the latch
would enable it too (a counter above zero means a raise happened on the row), so a counted
edge reaches a subset of the latch's states. That is the point of the option: a state that
needs two handler runs after one raise is reachable under the latch and not under the
counter. It is also why counting can only expose a defect through what needs a run to
happen (`leads_to`, `can_reach`, a deadlock, a missing witness) or through the overflow
rule, never through a `forbidden` the latch passes.

What is checkable now:

- every payment-received event is posted exactly once: `forbidden` the state that needs a
  second posting after one raise; it holds under the counter and fails under the latch;
- no event is lost when two arrive before the handler runs: `leads_to(raised twice,
  handled twice).fair(UC)`;
- a queue of capacity `k` never overflows: `EventOverflow_<E>_<Ev>_<UC>`.

## The transition system

<!-- dagayn: discusses crates/rdra-ish-core/src/lifecycle/build/events.rs::event_marks -->

No new effect and no new domain. The counter is a `VarKind::Raised` var with a `handler`
field: `None` for the latch, `Some(<UC>)` for a counted edge. Its domain is
`Domain::Enum(["0", .., "k", "over"])`, as the run mark of a split use case is an Enum of
numerals. `Domain::Int` would follow `--int-max` and be unbounded under z3. A raise and a
handler run are `Effect::Transition` arms on it: arms `i -> i+1` and `k -> over` on the
raiser, arms `i -> i-1` on the handler. A transition with no arm from the current value
leaves the var as it is in all three engines, which gives the absorbing `over`. The guard is
`counter != "0"`. The overflow rule is an `Invariant` of kind `InvKind::Overflow`, with no
premise, since it is synthesized and a vacuity warning on it would only be noise.

Reusing the transition effect means the explorer, the TLA+ printer, and the z3 encoding need
no new case. The sites that read transition arms as lifecycle arms (the `Arm_` findings, the
deadlock exits, the witnesses) select a view column first, and a mark is never a view axis,
so they do not see the counter's arms. The sites that pattern-match the latch's `TRUE`
(codegen, the trace's free-trigger note) check the `handler` field.

## Cones

<!-- constrained-by ./event-order.md#cones-and-the-reduction -->

A latch is havocked in a cone that does not hold every raiser by its own writes: the guard
is dropped, which is sound because a mark only rises, so a handler that ignores it reaches a
superset ([cone-local havoc](./event-order.md#cones-and-the-reduction)). A counter is not
monotone, and dropping its guard would also drop the handler's decrement, after which the
count means nothing. So a counted mark is never havocked:

- The handler's transition reads the counter, so the counter is a plain read of the handler
  (`plain_reads` counts a transition's var). Every cone that holds the handler holds the
  counter.
- Every raiser's transition reads it too. Every cone that holds a raiser holds the counter.
- Every action that writes a var of a closed cone is in the cone. So once the counter is in,
  every raiser and every handler variant of the edge is in, by their own writes.
- `cone()` checks this as a rule as well (`counted marks are exact`): a counted mark that a
  taken action reads is inserted with its reads whatever the other writers are. With the
  three facts above this changes nothing, and it keeps the cone exact if a later change
  moves the decrement out of the transition effect.

The cost is the coupling the issue names: a cone that holds a raiser of a counted edge also
holds the handler and everything the handler reads, where the latch let the raiser in
without its handler. The option is per edge so that a model pays it only where it asks for
the count.

## The reduction

<!-- constrained-by ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->

The reduction by an independent sub-lifecycle splits a cone into T, which the target
observes, and S, which is reduced to its classes. Since #112 round 4 an S action may read a T
var that only rises, a monotone input, and the quotient is stable under each rise
([round 4](./event-order.md#reduction-with-monotone-inputs-round-4)). A latch is such an
input. A counter is not, and an S action that read a T counter as an input would see a
value that falls, which the rise table cannot follow.

The argument that a counted mark never reaches that case:

1. The readers of a counter are its raisers, its handler variants, and its overflow rule.
   Every raiser and every handler also writes it (the transition reads and writes the var).
   So every action that reads the counter writes it.
2. `split_at` puts every writer of a T var in T. If the counter is in T, every action that
   reads it is in T, by 1, and no S action reads it.
3. If the counter is in S, it is an internal var of S, written and read only by S actions.
   The bisimulation argument of round 4 holds for any internal var of S, monotone or not:
   it constrains only what S reads of T.
4. So a counter is never an input, raw or folded, and the monotone test is never asked of
   it.

The code still says so on its own: `monotone_vars` reports a counted mark as not monotone
whatever arms the cone holds, so even a cone system that held a raiser without its handler
(the counter would look monotone there, `0 -> 1 -> .. -> over`) could not take it as an
input. And `split_at` seeds T with every counted mark the cone holds, together with its
raisers and its handler, which is the owner's rule from #220 ("a cone with a counted mark
keeps the raiser and the handler in T"). By 2 and 3 this costs only reductions in which the
whole counted edge would have been an S-internal handoff.

Tests (`lifecycle/tests/reduce.rs`):

- a counted mark is not monotone in a cone restricted to its raiser alone, where its arms
  alone are acyclic (fails with the rule off);
- in a model whose counted edge would otherwise land in S, the split keeps the counter, its
  raiser, and its handler in T, names the counter in no input and no fold, and the reduced
  cone decides what the unreduced cone decides (`assert_reduction_exact`).

## TLA+

| RDRA | TLA+ |
|---|---|
| A counted edge `triggers(Ev, UC).counted(k)` with mark entity `E` | `<E>_pending_<Ev>_<UC>`, a function from `<E>_Ids` to `{"0", .., "k", "over"}`; `Init` sets every id to `"0"`; `TypeOK` lists it. |
| The last step of a raiser, on its parameter `p` on `E` | `<E>_pending_<Ev>_<UC>' = [.. EXCEPT ![p] = CASE @ = "0" -> "1" [] .. [] @ = "k" -> "over" [] OTHER -> @]`, as any transition. |
| The first step of the handler | the conjunct `<E>_pending_<Ev>_<UC>[p] /= "0"` and the transition `"1" -> "0"`, .., `"k" -> "k-1"`. |
| `EventOverflow_<E>_<Ev>_<UC>` | `\A r \in <E>_Ids : <E>_exists[r] = "present" => <E>_pending_<Ev>_<UC>[r] /= "over"`, an `INVARIANT` of the cone's module. |

The printer reads the same `Action`s as the explorer, so `engine_equivalence` and the
corpus compare BFS and TLC on the counted cases as on every other.

## z3

In scope through the existing encoding: an Enum var with transition arms and a `!=` guard.
`--prove` reports `EventOverflow_*` like any other state rule. Induction alone seldom proves
it, since a state with the counter at `k` and a raiser enabled is a counterexample to
induction whether or not it is reachable, so the usual verdict is `unproved` with a CTI.
That is no disagreement ([rule 6](../engine-agreement.md#agreement-rules)). A rule that is
proved still holds at every bound, which the corpus checks with `bfs=`.

## Code generation

`codegen --lang ts|rust|go` prints a raise as an event value and a triggered use case as a
function that takes the event. A pending count has no counterpart there: the generator
would need a queue per row, which is a runtime decision, not a model fact. A use case that
raises or handles a counted event is reported as `unsupported: a counted event
(triggers(..).counted(k))`, as other effects the generator does not print, and the rest of
the model generates as before.

## Cost

Measured on the corpus models only; no sample uses the option. A counter multiplies the
states of every cone that holds it by at most `k + 2` per row, and it pulls the handler into
every cone of a raiser. The option is per edge and off by default.

## Non-goals

- **Counting on the latch.** An edge without `.counted` keeps the latch; there is no global
  switch.
- **A raiser that waits.** Rejected above.
- **Unbounded counts.** Every counter has a stated `k`, and the overflow says when the
  model passes it.
- **Ordering between raises.** A counter says how many raises are pending, not which raise
  each run handles. Two raises on one row are one queue of two, not two distinguishable
  messages.
