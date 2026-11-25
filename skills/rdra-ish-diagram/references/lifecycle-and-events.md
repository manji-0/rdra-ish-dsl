# State models and events

Use these views for state machines and event causality. They draw the declared
transitions and events, not reachability: `check` decides whether each state is reached
and left. Read its findings with `../../rdra-ish-buc-analyze/references/state-patterns.md`,
and choose between `check` and `verify` with
`../../rdra-ish-verify/references/when-to-use.md`.

```sh
rdra-ish diagram src/ --kind state --format mermaid
rdra-ish diagram src/ --kind state --entity Order --format mermaid
rdra-ish list src/ --kind transition --format markdown
rdra-ish diagram src/ --kind event --format mermaid
```

## State diagram (`state`)

- Without a scope the diagram is an index: one composite state per state model, holding
  the note `<n> 状態 / <m> 遷移 (--entity <Entity>)`. Run that `--entity` for the states.
- `--entity` and `--buc` draw each state model whole:
  `state "Order.status" as statemodel__Order_status { ... }`, the states in the Enum's
  order.
- An arrow is a `transitions` arm, labelled with its use case's label and, in `[...]`, the
  conditions that guard it. The `%% transition usecase__<id>` line before it names the use
  case. A use case that moves two lifecycle columns of one entity adds
  `(Entity.other_column と同時)` to each edge: the two edges are one step.
- `[*] -->` marks a start state; `--> [*]` an end state (no way out, or declared by
  `@end(...)`).
- `class` lines colour states by the lifecycle check, as the `%% legend:` line says:
  `stateunreachable`, `statedeadlock`, `stateend`.
- `%% omitted:` names `Enum` columns without `transitions`. One that a `sets` writes is
  named on `%% set-only:` instead: a state axis of `check`, not a state model.
- `state` is the one kind that explores the lifecycle. It refuses to write while `check`
  reports a lifecycle error other than a deadlock, which it colours.

## Transition table (`list --kind transition`)

The table behind the arrows: `entity, column, from, to, usecase, guards, actors, site`.
`from` and `to` read `label (value)`, `usecase` reads `label (id)`, `guards` are the
condition labels, and `actors` perform the use case. `site` is the `transitions` line.

## Event diagram (`event`)

- Every edge is unlabelled; its end kinds name the relation. Edges into an event are
  `raises` (from a use case or external system). Edges out of an event or a timer are
  `triggers` (to a use case, BUC, or external system).
- `%% omitted: state transitions` says a drawn use case moves a state; `state` draws it.
- Read the warnings on stderr (an event never raised or never consumed) before trusting
  the diagram as complete.
- For the event contract, run `rdra-ish export src/ --kind asyncapi --out out/asyncapi.json`.
