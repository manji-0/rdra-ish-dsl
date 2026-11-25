# State Patterns and Lifecycle Findings

<!-- derived-from ../../../docs/state-derivation.md -->

`check` and `states` explore the model's transition system by breadth-first search, the
system `verify` explores at two instances. Each use case is one atomic action per
transaction group it writes; an entity no use case creates starts present. The state axes
are inline Enum, Bool, and `@null` columns, and Int and variation columns something names.
`check` tests every rule and `after(...).assert` against the reachable states, and checks
each state of each state model for a witness and a way out. Every result holds within the
`bounds:` line it starts with: one instance per entity, Int `0..5`. Analyze states once the
entity structure says which fields change.

## Commands

```sh
rdra-ish check src/
rdra-ish check src/ --entity <EntityId>
rdra-ish check src/ --actor <ActorId>
rdra-ish states src/ --entity <EntityId>
rdra-ish states src/ --format json --entity <EntityId>
rdra-ish diagram src/ --kind state --format mermaid --buc <BucId>
```

- `--entity` derives the entity's view and the rules that read it, each on its exact cone;
  its findings are decided.
- `--buc` drops the use cases outside the BUC; what they could reach or unblock is listed
  under `not checked:`.
- `--actor`, `--without-buc`, and `--without-usecase` assume the left-out use cases never
  run and still decide every finding: `--actor Staff` finds a state staff cannot leave by
  their own use cases.

## Reading `states`

```text
$ rdra-ish states ../rdra-ish-verify/samples/order.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
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

`INITIAL`: a creating use case makes the pattern, or it starts a pre-existing entity.
`TERMINAL`: a use case deletes the row there, or nothing moves it. `VIA`: the
`BUC/use case` pairs on some path to it. A missing combination is a design question, not a
defect. `--format json` adds a shortest witness path per pattern.

## Reading findings

Each finding prints its name after the severity, then the sites involved:

```text
$ rdra-ish check ../rdra-ish-verify/samples/unwitnessed_state.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
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
`MultiForbidden_Order_0`, `Quantifier_Cert_0`, `Unique_*`, `MinCard_*`, `DanglingKey_*`,
`Check_*`, `AfterAssert_<UC>_<n>`) with the rule's site and the shortest counterexample:
one line per use case step with the rows it binds, and the values it changed.

| Finding | Meaning | Action |
|---|---|---|
| `error[Witness_<E>_<col>_<v>]` | No sequence of use cases reaches the state | Add the `creates`, arm, or condition that enters it, or drop the state |
| `error[Deadlock_<E>_<col>_<v>]` | An instance reaches a state with a way out and never leaves | Read `leaves` and `guard:`; fix the guard or add the use case. If staying is right, `@end(<v>)` on the column |
| `warning[Deadlock_...]: deadlock at the bound` | The way out needs a free slot of an entity it creates | Check at two instances with `verify` |
| `warning[Runs_<UC>]` | The use case never runs; `needs:` or `guard:` says why | Fix the guard or the missing row, or remove the use case |
| `warning[Vacuous_<rule>]` | The rule, `after`, or property holds and says nothing: no reachable state meets its premise | Fix a typo or a state no use case enters |
| `error[<rule>]` with a counterexample | A reachable state breaks the rule | Follow the steps: add the missing `sets` or arm, or fix the rule |
| `warning[UnreachableEnumVariant_...]` | A declared variant no state model holds and nothing reaches | Remove it, or add the arm |
| An Enum an actor merely picks reported unreachable | An attribute, not a lifecycle | Type it by a `variation` (`lint` `enum-attribute`) |
| `warning[UnjoinedWrite_<UC>_<A>_<B>]` | The use case writes two entities joined only through ones it does not write | Write the entity between, or mark a write `.apart()` |
| `UnbindableScope` | An `.along(...)` path no `relate` binds | Declare the `relate`, or drop `.along` |
| `AmbiguousOwnerLink` | Two keys join a pair that a rule, guard, or copy joins | Name one with `.role(role)` after that call |
| `UnbindableCondition` | A condition body applies to a use case that writes nothing | Apply it to a use case that writes |
| `UnboundCopySource` | A copy reads an entity the step binds to no row | Relate the target to the source, or copy where the source is written |
| `ContradictoryDrivenComparison` | A `drives` disagrees with the use case's Int `sets` | Fix the `sets` or drop the `drives` |
| `DoubleModeledEnum` | A use case that does not create the entity `sets` a column `transitions` drive | Use an arm |
| `UnwrittenConstrainedVar` (warning) | A rule reads a column no use case writes | Add the `sets`, or drop the rule |
| `NoCreationPath` (info) | No use case creates the entity, so it starts present | Add `creates` if that is wrong |
| `MinCardNeedsCreate` (info), `MinCardOfPreExistingParent` | A `1..*` parent is created, or starts, without a child | Create the child in the same transaction, or relax the card |
| `PreExistingChildOfCreatedParent` | A pre-existing child under a parent a use case creates | Add `creates` for the child, or `.optional()` |
| `lint` `multi-rule-unscoped`, `end-redundant` | A multi-entity rule with neither `.along` nor `.global()`; an `@end` value no arm leaves | Add the scope; drop the `@end` value |
| `PatternCapReached`, `MemoryCapReached`, or `StateCapReached` | The run stopped at a cap; no verdict on that cone is sound | Narrow with `--entity` or `--buc`, or raise `--max-patterns` or `--memory` (`memory` under `[check]` in `rdra.toml`, default `4GiB`). The message names the growing columns and use cases |
| `not checked:` | A scope or cap left findings undecided | Report them as not checked |

`check` evaluates no `property` and holds every rule at one instance: for both, use skill
`rdra-ish-verify` (`../../rdra-ish-verify/references/when-to-use.md`).
