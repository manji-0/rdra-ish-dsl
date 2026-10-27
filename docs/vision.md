# Target shape of rdra-ish

This note defines what rdra-ish is when it is finished: an RDRA requirements model
that also carries a logical data model and a lifecycle that TLC can check. It records
where today's DSL stands against the RDRA primary sources, the semantics the finished
DSL commits to, and the order of work that gets there.

<!-- derived-from ./rdra-ish-interpretation.md -->
<!-- constrained-by ./language-reference.md -->
<!-- constrained-by ./formal-verification.md -->
<!-- constrained-by ./state-derivation.md -->

## Two findings change the starting point

`verify` can report success without checking anything. When an entity has one
`transitions`, the TLA+ export drops every `sets` effect of a use case that raises no
event (`crates/rdra-ish-emit/src/tla.rs:1154`). A column that no exported action
writes stays at its initial value, so a `forbidden` rule on it holds vacuously. A
nine-line model with a reachable forbidden state gets `TLC: OK` and exit code 0. The
same model also shows that `states` and the TLA export start from different initial
states. Until both are fixed, a passing `verify` proves nothing about a model that
mixes events and plain `sets`.

The removal of `condition` and `variation` in 181f6cb contradicts RDRA 3.0. The
author defines the システム layer as 「システムで管理する情報、状態、ビジネスルール」,
and 条件 and バリエーション are the business-rule half of it. Phase 3 of the author's
method (Steps 7 to 9) exists to find them, and 6 of the 15 inconsistency checks in the
official RDRA definition sheet refer to them. The removal of `usagescene` matches the
author's own change from 2.0 to 3.0 and stays removed.

## rdra-ish covers three layers of work that RDRA keeps apart

The author's diagram of upstream work (techRAMEN deck, slide 2) puts RDRA in the
requirements step only. ER diagrams, logical and physical data models, and decision
tables come later, in 仕様化 and 設計. The author's RDRAAgent produces the logical data
model in a separate step, RDRASpec, with fixed rules (`21_論理データ生成.md`).

rdra-ish deliberately covers three of these steps in one model. The "ish" in the name
is that choice.

| Layer | Source of truth | What the modeler writes | What the tool guarantees |
|---|---|---|---|
| Requirements | RDRA 3.0 (`RDRA.md`, the RDRA definition sheet) | RDRA elements and their connections | Every connection the RDRA rules require exists, and nothing is undefined or unconnected |
| Logical data | The author's RDRASpec mapping | Entities, columns, keys, relations | A normalized relational model that DBML, JSON Schema, and OpenAPI can be generated from |
| Lifecycle | rdra-ish | Effects, guards, constraints, properties per use case | Within the stated bounds, no reachable state breaks a constraint, and every declared state is reachable |

Physical database design is out of scope. DBML is the last data output, and the DSL
gets no DDL emitter. Unbounded proof (TLAPS, inductive invariants) is also out of
scope. Verification means bounded model checking with TLC, and the bounds are part of
every result.

## The requirements layer conforms to RDRA 3.0

The author's `RDRA.md` lists the elements and the rules that connect them. The table
compares each element with today's DSL and states the target.

| RDRA 3.0 element | Today | Target |
|---|---|---|
| アクター | `actor` | Keep. |
| アクター群, 外部システム群 | Missing | Defer. Only diagrams would read a group, and a diagram can group by file or tag. |
| 外部システム | `extsystem`, linked only by `uses(actor, extsystem)` and placed in the external-environment layer | Move to the system-value layer. Connect it to use cases through events, as the RDRA rule 「外部システム」は「イベント」で「UC」と連携する requires. |
| 要求 | `requirement`, traced by `motivates(requirement, buc)` | Keep the BUC trace. Add the RDRA link from a requirement to the actor who holds it. |
| 業務, BUC | `business`, `buc` | Keep. |
| アクティビティ | `step` inside `flow` | Keep. Add the link from an activity to the actor who performs it. |
| UC | `usecase` | Keep. The UC becomes the trigger of state transitions (see the lifecycle layer). |
| 画面 | `screen`, `field` | Keep. |
| イベント | `event` | Keep as the interface to external systems. It no longer drives transitions directly. |
| タイマー | Missing. `timing` exists but nothing reads it. | Turn `timing` into タイマー. A timer triggers a use case, which gives the kind a consumer. |
| コンテキスト | Missing. `system owns entity` is the closest thing. | Defer. A context adds value only if a check reads it, such as the author's heuristic that a UC spanning several contexts is important. |
| 情報 | `entity`, at the logical level | Keep one kind. The author's own RDRASpec maps 情報 to entity one to one. |
| 状態モデル, 状態 | An `Enum` column plus `transitions`. `state` is declared but no predicate takes it. | Derive states from the state column. Stop declaring `state`, and exclude derived states from orphan and naming lint. |
| 条件 | Removed in 181f6cb | Restore. A condition links to use cases, variations, and a state model. Its body stays optional, so RDRA's traceability-only use works. |
| バリエーション | Removed in 181f6cb | Restore as a named value set that an `Enum` column can use as its type. |
| 利用シーン | Removed | Stays removed. RDRA 3.0 dropped it. |

Two consequences of the table matter beyond conformance.

A variation is the missing distinction between a state axis and an attribute. Today
every `Enum` column is treated as a state axis, so `PaymentTransaction.method` in
clinic-ops shows up as unreachable variants. In the target, a column typed by a
variation is an attribute: an actor chooses its value, and nothing checks whether
each value is reachable. A column with `transitions` is a state axis. RDRA draws the
same line between 状態モデル and バリエーション.

A condition is where typed guards come from. The author defines a condition as a
combination of variations and states (「条件」は「バリエーション」と「状態モデル」の組合せで出来ている).
A condition with a body over those two things compiles to a guard on the use cases
that it is linked to. A condition without a body is a trace link and nothing more.

The 15 inconsistency columns of the RDRA definition sheet become lint rules. Each
column is an "undefined" or "unconnected" check, such as an information item that no
BUC uses, or a transition use case that no BUC defines.

## The logical data layer follows the author's RDRASpec rules

The author's `21_論理データ生成.md` already says how 情報 becomes data. One 情報 is
one entity. A parent and child are linked by the child holding the parent's ID. State
models and variations become enums, and an entity holds a column for its state. The
DSL keeps these rules and adds what a normalized relational model needs:

- A 1:1 relation generates a foreign key with a unique constraint. Today it generates
  nothing (`crates/rdra-ish-core/src/analysis/fk.rs:13-35`).
- A relation can name its role, so two relations to the same parent stop colliding on
  `{parent}_id` (`fk.rs:152-170`).
- A foreign key can be declared explicitly, and it can reference a composite key.
- `@check` and `@derived` parse into typed expressions. Today they are stored as
  strings. A typed `@check` is also an invariant that verification checks.
- A relation can state a minimum cardinality, such as `1..*`, and verification checks
  it.
- An `Enum` column typed by a variation is an attribute, not a state axis.

## The lifecycle layer has one semantics

Today `states` (a BFS over patterns) and the TLA export are two separate readings of
the same model. They disagree on the initial state and on which effects exist. The
target defines one transition system in `rdra-ish-core`, and both `states` and the
TLA emitter read it.

The transition system follows these rules:

1. Every entity instance has an existence state: absent, present, or deleted. Every
   instance starts absent.
2. A use case is one atomic action. Its precondition combines the states it requires
   (the author's 「ある状態の時にUCが実行できる」) with the guards of its linked
   conditions. Its effect is every `creates`, `deletes`, `sets`, and transition it
   causes, across all entities, applied in one step. Today the same event becomes one
   action per entity, which allows counterexamples that no model can fix.
3. A transition names the use case that causes it, as the 遷移UC column of the RDRA
   state sheet does. An event raised by an external system or by another use case
   triggers use cases. It does not move a state by itself.
4. `creates` sets the initial values of a new instance, with `@default` filling the
   rest. The state that a creation sets must be a start state of the state model. If
   it is not, `check` reports an error.
5. A column typed by a variation takes a value the actor chooses. The checker
   explores every value, so a variant needs no dedicated use case to be reachable.
6. The transaction groups that `tx.rs` infers bound atomicity. A use case whose
   effects span two groups is two actions, and the checker explores the state in
   between.

## Verification is bounded, fails closed, and points at the model

A passing `verify` must mean something, and a failing one must say where to look:

- Every constrained column is written by at least one exported action. If no action
  writes it, `verify` fails before TLC runs.
- Every constraint is its own `INVARIANT`, named after the constraint. A violation
  names the rule that broke.
- Every state in a state model gets a reachability witness. A state with no witness
  is an error, not a lint warning.
- A state with no outgoing transition is an end state, as in the RDRA state sheet. A
  deadlock check reports a non-end state with no enabled action.
- A counterexample is printed as a sequence of use case names and source locations,
  not TLA action names.
- `verify --buc` and `verify --entity` check part of a model, so one entity that
  cannot be exported no longer blocks the whole run.
- Every result states its bounds: the instance count and the integer range.
- CI runs `verify` on every sample, including `samples/clinic-ops`. Today it runs only
  `samples/formal-verification`.

## One stage order replaces three

The README, `stage-readiness` in `lint.rs`, and `incremental-modeling.md` describe
three different orders. The target keeps one order, which follows the author's three
phases and nine steps for the requirements layer and then adds the two rdra-ish
layers:

1. Build the basis for discussion: actors and businesses, BUCs and activities,
   information, and state models (RDRA Steps 1 to 4).
2. Assemble requirements: use cases, screens, events, and timers, then review use cases
   against information and states (RDRA Steps 5 and 6).
3. Make the requirements specifiable: conditions and variations (RDRA Steps 7 to 9).
4. Derive the logical data model (RDRASpec).
5. Add effects, guards, and constraints, and run `verify`.

`stage-readiness` checks the RDRA connections of each stage, not only whether any
element of the stage exists.

## The work lands in six phases

Each phase ends in a check that the next phase depends on. Removal comes before
addition.

1. **Soundness.** Export every `sets` effect, fail when a constrained column has no
   writer, and agree on one initial state. The check is a regression sample that
   reproduces today's vacuous `TLC: OK` and must now fail.
2. **Subtraction.** Delete `location`, stop declaring `state`, and fix the docs that
   describe rules the code does not have. The check is that clinic-ops lint drops the
   66 orphan findings on derived states.
3. **RDRA conformance.** Restore `condition` and `variation`, turn `timing` into a
   timer, make use cases the trigger of transitions, and add the definition-sheet
   lint rules. The check is that clinic-ops passes every inconsistency rule.
4. **One transition system.** Build the shared transition system with existence
   states and atomic use case actions, and move `states` and the TLA emitter onto it.
   The check is that both engines report the same reachable states on every sample.
5. **Logical data gaps.** Add the relation and key features in the data layer list.
   The check is DBML output for every sample with no unresolved foreign keys.
6. **Traceable verification.** Add per-constraint invariants, witnesses, deadlock
   checks, counterexamples in model terms, and partial verification. Then make CI
   verify every sample. The check is CI.

## Decisions

The maintainer decided these questions on 2026-10-27:

- `condition` and `variation` come back. The primary source puts them at the center
  of the システム layer, and a variation-typed column separates attributes from state
  axes.
- A use case drives a transition, as in RDRA. Every existing
  `transitions(E.col, Event, a -> b)` changes to name a use case.
- コンテキスト waits until a check reads it.
- `location` is deleted. RDRA 3.0 has no counterpart.
- `extsystem` moves to the system-value layer to match RDRA. Every other layer
  placement that differs from RDRA is documented in `rdra-ish-interpretation.md`.

## Sources

- The author's element and connection rules:
  [RDRA.md in RDRAAgent_v0.6](https://github.com/kanzaki/RDRAAgent_v0.6/blob/HEAD/RDRA_Knowledge/1_RDRA/RDRA.md)
- The four layers: [rdra.jp article1](https://www.rdra.jp/article1) and the
  [RDRA3.0 deck](https://speakerdeck.com/kanzaki/rdra3-dot-0wozhi-rou)
- Steps and phases: [要件定義の進め方](https://www.rdra.jp/要件定義の進め方)
- Scope of conditions and variations: [RDRAビジュアル分析](https://www.rdra.jp/rdraビジュアル分析)
- Inconsistency checks: [RDRA定義分析Sheet](https://www.rdra.jp/rdraツール/rdra定義分析sheet)
- Position of RDRA in upstream work: [RDRAとは (techRAMEN deck)](https://speakerdeck.com/kanzaki/rdratoha-techramen)
