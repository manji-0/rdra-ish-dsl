# Target shape of rdra-ish

This note defines what rdra-ish is: an RDRA requirements model that also carries a logical
data model and a lifecycle that `verify` checks. It records where the DSL stands against the
RDRA primary sources, the semantics it commits to, and the decisions behind them.

<!-- constrained-by ./language-reference.md -->
<!-- constrained-by ./formal-verification.md -->
<!-- constrained-by ./state-derivation.md -->

## rdra-ish covers three layers of work that RDRA keeps apart

The author's diagram of upstream work (techRAMEN deck, slide 2) puts RDRA in the
requirements step only. ER diagrams, logical and physical data models, and decision tables
come later, in 仕様化 and 設計. The author's RDRAAgent produces the logical data model in a
separate step, RDRASpec, with fixed rules (`21_論理データ生成.md`).

rdra-ish covers three of these steps in one model. The "ish" in the name is that choice.

| Layer | Source of truth | What the modeler writes | What the tool guarantees |
|---|---|---|---|
| Requirements | RDRA 3.0 (`RDRA.md`, the RDRA definition sheet) | RDRA elements and their connections | Every connection the RDRA rules require exists, and nothing is undefined or unconnected |
| Logical data | The author's RDRASpec mapping | Entities, columns, keys, relations | A normalized relational model that DBML, SQL DDL, JSON Schema, and OpenAPI can be generated from |
| Lifecycle | rdra-ish | Effects, guards, constraints, properties per use case | Within the stated bounds, no reachable state breaks a constraint, and every declared state is reachable |

Physical database design beyond the schema is out of scope. DBML and the SQL DDL of
`export --kind sql` (tables, keys, indexes, and `CHECK` constraints) are the last data
outputs. Domain code is in scope. `codegen` prints the data types, states, and transition
functions in TypeScript, Rust, or Go for an agent to finish, and `check --code` holds the
finished code to the model ([design](./design/algebraic-codegen.md)). Verification is
bounded model checking by rdra-ish's own checker, and the bounds are part of every result.
TLC (`--with-tlc`) checks the same cones as a second opinion, and `verify --prove`
(experimental) tries to prove each state rule for any number of rows with z3. Unbounded
proof with TLAPS is out of scope.

## The requirements layer conforms to RDRA 3.0

The author's `RDRA.md` lists the elements and the rules that connect them. The table
compares each element with the DSL.

| RDRA 3.0 element | rdra-ish | Status |
|---|---|---|
| アクター | `actor` | Met. |
| アクター群, 外部システム群 | Missing | Deferred. Only diagrams would read a group, and a diagram can group by file or tag. |
| 外部システム | `extsystem`, in the system-value layer, linked to use cases through events: `raises(ExtSystem, Event)` inbound, `triggers(Event, ExtSystem)` outbound | Met, as the RDRA rule 「外部システム」は「イベント」で「UC」と連携する requires. |
| 要求 | `requirement`, traced by `motivates(requirement, buc)` and `achieves(requirement, goal)`. Its holder is only the free-text `stakeholder` and `owner` metadata. | Open: a link from a requirement to the `actor` who holds it. |
| 業務, BUC | `business`, `buc` | Met. |
| アクティビティ | `step` inside `flow`, performed through `performs(Actor, Step)`, drawn as a lane | Met. |
| UC | `usecase`, the first argument of `transitions(UseCase, Entity.col, a -> b)` | Met. The UC is the trigger of state transitions. |
| 画面 | `screen`, `field` | Met. |
| イベント | `event`, the interface between use cases and between a use case and an external system. `transitions` refuses an event. An event a use case raises orders the use cases it triggers (rule 3). | Met. |
| タイマー | `timer`, connected by `triggers(Timer, UseCase \| Buc)` | Met. |
| コンテキスト | Missing. A `system`, which groups APIs and `owns` entities, is the closest thing. | Deferred until a check reads it, such as the author's heuristic that a UC spanning several contexts is important. |
| 情報 | `entity`, at the logical level | Met. RDRASpec maps 情報 to entity one to one. |
| 状態モデル, 状態 | An `Enum` column that `transitions` moves. Its variants are the states, and `@end` marks the ones a row may stay at. | Met. States are derived from the state column. |
| 条件 | `condition`, linked by `combines` to variations and lifecycle columns and by `applies_to` to use cases. An optional `holds` body guards those use cases. | Met. |
| バリエーション | `variation`, a closed value set that a column names as its type in place of `Enum(...)` | Met. |
| 利用シーン | Removed | RDRA 3.0 dropped it. |

Two consequences of the table matter beyond conformance.

A variation is the distinction between a state axis and an attribute. A column typed by a
variation is an attribute: an actor chooses its value, and nothing checks whether each value
is reachable (`PaymentTransaction.method` in clinic-ops). A column with `transitions` is a
state axis. RDRA draws the same line between 状態モデル and バリエーション.

A condition is where typed guards come from. The author defines a condition as a
combination of variations and states (「条件」は「バリエーション」と「状態モデル」の組合せで出来ている).
A condition with a body over those two things compiles to a guard on the use cases it is
linked to. A condition without a body is a trace link.

The 15 inconsistency columns of the RDRA definition sheet are rules, one row each in
`RDRA_RULES` (`crates/rdra-ish-core/src/rdra_rules.rs`). The 11 "undefined" columns are load
errors and the 4 "unconnected" columns are lint warnings. rdra-ish adds three rules of its
own ([the language reference](./language-reference.md#rdra-lint-rules) lists them).

## The logical data layer follows the author's RDRASpec rules

The author's `21_論理データ生成.md` says how 情報 becomes data. One 情報 is one entity. A
parent and child are linked by the child holding the parent's ID. State models and
variations become enums, and an entity holds a column for its state. The DSL keeps these
rules and adds what a normalized relational model needs:

- A 1:1 relation generates a foreign key column on its left entity. The column is unique,
  and nullable only under `.optional()`. Verification checks the uniqueness as the
  invariant `Unique_<Child>_<role>`.
- A relation can name its role with `.role(x)`, so two relations to the same parent do not
  collide on `{parent}_id`. The key columns are `{role}_{parent key column}`. Two keys of
  one child with the same role are `DuplicateRelateRole`.
- A relation can adopt columns the child declares with `.key(cols)`, and it can reference a
  composite key: a parent `@pk(site, shelf)` yields one child column per key column.
  `.along(A, B).role(role)` picks the key a constraint follows when two keys join the same
  pair.
- `@check(expr)` and `@derived(expr)` parse into typed expressions. Each `@check` is the
  invariant `Check_<Entity>_<column>_<n>`. A `@derived` column is never a state variable: a
  logical one is read as its expression, and an arithmetic one cannot be read by a
  constraint.
- A relation's child side states a minimum and a maximum cardinality, such as `1..*:1` or
  `0..3:1`, checked as `MinCard_<Child>_<role>` and `MaxCard_<Child>_<role>`.
- `@unique` columns and column sets are checked as `Unique_<Entity>_<columns>` where the
  lifecycle holds every column's value.
- A column typed by a variation is an attribute, not a state axis.

## The lifecycle layer has one semantics

`rdra-ish-core` defines one transition system in `crates/rdra-ish-core/src/lifecycle/`.
`states`, `check`, `verify`, the TLA+ export, `codegen`, and the state diagrams all read
it. The TLA+ emitter only prints it, so the native checker and TLC cannot disagree on what a
use case does. [State derivation](./state-derivation.md) describes how the tool explores
the system.

The transition system follows these rules:

1. Every entity instance has an existence state: absent, present, or deleted. Every
   instance of an entity that some use case creates starts absent. An entity that no use
   case creates is pre-existing, as master data is: its instances start present at the
   column defaults, and its variation columns start at any value. `states` reports such an
   entity as `NoCreationPath` (info).
2. A use case is one atomic action. Its precondition combines the states it requires (the
   author's 「ある状態の時にUCが実行できる」) with the guards of its linked conditions. Its
   effect is every `creates`, `deletes`, `sets`, and transition it causes, across all
   entities, applied in one step. A `deletes` of a parent waits while a present child names
   it through a key whose `.on_delete` is `restrict`, `no_action`, or unset, as the database
   would.
3. A transition names the use case that causes it, as the 遷移UC column of the RDRA state
   sheet does. An event raised by an external system or by another use case triggers use
   cases. It does not move a state by itself. An event a use case raises orders the use
   cases it triggers: the raiser's last step records the event on its rows, and a triggered
   use case runs only on a row that carries the record, so it never runs before the raise.
   The record is a latch that counts nothing, unless the edge is `.counted(k)`, which runs
   the handler once per raise with up to `k` raises pending. A timer and an event an
   external system raises order nothing.
4. `creates` sets the initial values of a new instance, with `@default` filling the rest.
   The state that a creation sets is a start state, as a transition with no source state is
   in the RDRA state sheet. A condition that tests a column of the created instance
   constrains the creation: the instance must satisfy it after the use case runs. A
   `creates` of a child whose `relate` is `.optional()` can leave the child unlinked.
5. A column typed by a variation takes a value the actor chooses. The checker explores
   every value, so a variant needs no dedicated use case to be reachable. `@default` on such
   a column sets the DBML default only.
6. The transaction groups that `tx.rs` infers bound atomicity. A use case whose effects
   span k groups is k actions, named `<UseCase>_1` to `<UseCase>_k`, and one run of the use
   case takes them in order on the rows its earlier steps bound. The checker interleaves
   every other action between two steps of a run. `after(UseCase).assert` holds after the
   last of the k steps, on the run's rows.

## Verification is bounded, fails closed, and points at the model

A passing `verify` must mean something, and a failing one must say where to look. `check`
and `states` check the state rules, the `after` assertions, and the state models by state
exploration, and evaluate no `property`. `verify` checks the same rules and assertions at
two instances, and every `property` form.

- A constraint that reads a column no action writes draws `UnwrittenConstrainedVar`, and
  one that holds only because nothing it reads exists draws `Vacuous_<rule>`.
- Every constraint is its own named invariant, so a violation names the rule that broke.
- Every state in a state model gets a reachability witness. A state with no witness is an
  error.
- A state with no outgoing transition is an end state, and so is one that its column marks
  `@end`. A deadlock is an instance at a non-end state that no path can move off.
- A counterexample prints as use cases with their bound instances and source locations.
- `verify --buc` and `verify --entity` check part of a model.
- Every result opens with its `bounds:` line: the instances per entity, the Int range, the
  state cap, and the memory limit. A run that stops at a bound reports `not decided`.
- CI runs `check` and `verify` on every sample, and each sample states the result it
  expects in its `// expect` header.

## One stage order replaces three

One order exists: `ORDER` in `crates/rdra-ish-core/src/refinement/order.rs`. `lint`
`stage-readiness`, `rdra-ish next`, and the stage map of
[incremental-modeling.md](./incremental-modeling.md#stage-map) read it, and the test
`stage_docs_are_in_sync` keeps the docs in step. It has ten stages, and it does not follow
the author's three phases and nine steps step for step:

| Stage | Name | RDRA 3.0 or rdra-ish |
|---|---|---|
| 0 | Scope sketch | Basis: businesses and the BUCs they hold |
| 1 | BUC skeleton | Basis: actors, BUCs, activities; requirements: use cases |
| 2 | Screens | Requirements: screens |
| 3 | Data touchpoints | Basis: information, as the entities each use case touches |
| 4 | API and access boundary | rdra-ish: APIs, systems, permissions, and media |
| 5 | Entity structure | RDRASpec: columns and keys; specification: variations |
| 6 | Lifecycle | Basis: state models; requirements: events and timers; specification: conditions |
| 7A to 7C | Local guardrails, local obligations, advanced rules | rdra-ish: effects, guards, and constraints that `verify` checks |

It departs from the author's order for three reasons:

- **A state needs a use case.** A state model is a lifecycle column that
  `transitions(UseCase, Entity.column, from -> to)` moves, so it cannot be written before
  the use cases that move it. An `Enum` column that nothing moves is an attribute, and
  `lint` warns on it (`enum-attribute`). So states come at stage 6.
- **Each stage is a gate a tool decides.** `next` places each BUC at the last stage of the
  met prefix of the required stages. The author's steps include reviews, such as checking
  use cases against information and states, that no check can decide.
- **rdra-ish adds layers RDRA keeps apart.** The API and access boundary (stage 4) and the
  rules that `verify` checks (stages 7A to 7C) have no RDRA step.

Within these constraints the order follows RDRA's layers from the outside in: what the actor
sees (screens) before what the system keeps (entities), and the system boundary before the
data structure behind it.

## Decisions

The maintainer decided these questions on 2026-10-27:

- `condition` and `variation` stay. The primary source puts them at the center of the
  システム layer: phase 3 of the author's method (steps 7 to 9) exists to find them, and 6
  of the 15 inconsistency checks refer to them. A variation-typed column separates
  attributes from state axes.
- A use case drives a transition, as in RDRA.
- コンテキスト waits until a check reads it.
- `location` is deleted, and `state` is not declared. RDRA 3.0 has no `location`, and a
  state is derived from its column.
- `extsystem` moves to the system-value layer to match RDRA. Every other layer placement
  that differs from RDRA is documented in `rdra-ish-interpretation.md`.

On 2026-10-28:

- An entity that no use case creates is pre-existing: its instances start present (rule
  1). The strict reading, where such an entity never exists, would make every constraint on
  master data hold vacuously.
- Rule 2 lists a use case's preconditions as its required states and the guards of its
  linked conditions. The tool infers no order guards from `raises` and `triggers`.
- Outputs that do not print the lifecycle do not wait for it or explore it. Only the state
  diagram and the TLA+ export read it ([Lifecycle errors](./cli-reference.md#lifecycle-errors)).
- A minimum cardinality is always checked, and a 1:1 key is unique as an invariant,
  `Unique_*`, not as a guard on the use case that links.
- A use case that creates a parent and updates an existing child links the child. For an
  optional key it links only while the child is unlinked; for a mandatory key the link is a
  move.
- A card side has one spelling: the range goes on the child side (`1..*:1`), and a parent
  that may be absent is `.optional()`. A range on the parent side is `MinCardOnParentSide`.
- A quoted `@check("...")` or `@derived("...")` is a parse error whose message is the
  unquoted rewrite.

On 2026-10-29:

- A deadlock is per instance: an instance at a non-end state is stuck when no path from that
  state changes its column or deletes it. The literal reading, a state with no enabled
  action, misses most stuck instances, because one action that stays enabled anywhere masks
  them all.
- A deadlock whose way out lacks only a free slot of an entity it creates is a warning
  (`deadlock at the bound`), not an error: the bound blocks it, not the model.
- A state of a state model is a `transitions` endpoint or a value a creating use case sets.
  Such a state with no witness is an error. Every other unheld Enum variant keeps the
  `UnreachableEnumVariant` warning. A use case that never runs is a warning.
- Under `--buc`, a missing witness, a use case that never runs, and a deadlock are not
  decided: the use cases outside the BUCs could reach, enable, or unblock a state. They are
  listed under `not checked:`. `--entity` keeps exact cones, so its findings are decided.
- The test `engine_equivalence` compares the native checker with TLC on every cone of every
  sample at 2 instances, except the cones too large for TLC there, which it compares at 1
  instance and names ([Known verification gaps](./formal-verification.md#known-verification-gaps)).
- Each sample states its expected result in `// expect` lines of its first file in load
  order, so CI keeps no second list of samples.

On 2026-11-01, for 1.0 (#51), superseded by the next entry: events and timers imposed no
order on the use cases they trigger (#30).

On 2026-11-06, for 2.0 (#112):

- A use case that a use case's event triggers runs only after a raise on its rows (rule 3).
  A timer and an event an external system raises stay free. A cone that does not hold every
  raiser leaves the mark out and runs the handler unordered there, which is sound for an
  OK. [docs/design/event-order.md](./design/event-order.md) records the decisions and the
  measurements.

On 2026-11-07:

- rdra-ish prints domain code that a build depends on: buildable TypeScript, Rust, or Go
  that conforms to what `check` and `verify` verified, which an agent finishes. A use case
  split into transactions is one function per step, linked by stored run marks.
  [docs/design/algebraic-codegen.md](./design/algebraic-codegen.md) records the design.
- Code generation is experimental and outside [compatibility](./compatibility.md), except
  as the 2026-11-24 entry narrows it.

On 2026-11-21:

- The stage order stays the ten stages of `refinement/order.rs`
  ([One stage order replaces three](#one-stage-order-replaces-three)).

On 2026-11-23 ([#168](https://github.com/manji-0/rdra-ish-dsl/pull/168) records the reasons):

- `verify` decides every check with a native checker by default and starts no JVM.
  `--with-tlc` runs TLC beside it and fails the run when the two decide a check differently.
  `--backend tlc` stays as a deprecated spelling of `--with-tlc` until the next major
  release after the one that changes the default.
- `check` and `states` do safety verification by state exploration only. Every `property`
  form and every inductive proof belongs to `verify`.
- `main` is the early channel. [Compatibility](./compatibility.md) covers releases, and the
  `Unreleased` section of the changelog lists what `main` changed since the last one. Every
  run states its verdict on a `result:` line; `tlc_result:` prints beside it only under
  `--with-tlc`.

On 2026-11-24:

- `codegen --lang ts` is the one way to get TypeScript state types. `export --kind
  typescript-states` and `states --format typescript`, which typed an entity by the patterns
  `states` reaches, are removed.
- The TypeScript types of `codegen` are covered by
  [compatibility](./compatibility.md#the-typescript-types-of-codegen). The rest of `codegen`
  stays experimental. `export --kind json-schema` stays as it is.

## Sources

- The author's element and connection rules:
  [RDRA.md in RDRAAgent_v0.6](https://github.com/kanzaki/RDRAAgent_v0.6/blob/HEAD/RDRA_Knowledge/1_RDRA/RDRA.md)
- The four layers: [rdra.jp article1](https://www.rdra.jp/article1) and the
  [RDRA3.0 deck](https://speakerdeck.com/kanzaki/rdra3-dot-0wozhi-rou)
- Steps and phases: [要件定義の進め方](https://www.rdra.jp/要件定義の進め方)
- Scope of conditions and variations: [RDRAビジュアル分析](https://www.rdra.jp/rdraビジュアル分析)
- Inconsistency checks: [RDRA定義分析Sheet](https://www.rdra.jp/rdraツール/rdra定義分析sheet)
- Position of RDRA in upstream work: [RDRAとは (techRAMEN deck)](https://speakerdeck.com/kanzaki/rdratoha-techramen)
