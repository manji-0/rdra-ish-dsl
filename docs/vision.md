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

- A 1:1 relation generates a foreign key column on its left entity. The column is
  unique, and nullable only under `.optional()`. Verification checks the uniqueness as
  the invariant `Unique_<Child>_<role>`.
- A relation can name its role with `.role(x)`, so two relations to the same parent
  stop colliding on `{parent}_id`. The key columns are `{role}_{parent key column}`
  (`generate_key_columns` in `crates/rdra-ish-core/src/analysis/fk.rs`). Two keys of
  one child with the same role are `DuplicateRelateRole`.
- A relation can adopt columns the child declares with `.key(cols)`, and it can
  reference a composite key: a parent `@pk(site, shelf)` yields one child column per key
  column, and DBML prints the composite `Ref`. `.along(A, B).by(role)` picks the key a
  constraint follows when two keys join the same pair.
- `@check(expr)` and `@derived(expr)` parse into typed expressions. A quoted form is a
  parse error. Each `@check` is the invariant `Check_<Entity>_<column>_<n>`, which BFS
  and TLC check alike with SQL null semantics. A `@derived` column is never a state
  variable: a logical one is read as its expression, and an arithmetic one cannot be
  read by a constraint.
- A relation can state a minimum cardinality on its child side, such as `1..*:1`.
  Verification checks it as the invariant `MinCard_<Child>_<role>`, which is always on.
- An `Enum` column typed by a variation is an attribute, not a state axis.

## The lifecycle layer has one semantics

`rdra-ish-core` defines one transition system in `crates/rdra-ish-core/src/lifecycle/`.
`states`, `check`, the TLA+ export, `verify`, the TypeScript state unions, and the state
diagrams all read it. The TLA+ emitter only prints it. [State derivation](./state-derivation.md)
describes how the tool explores the system.

The transition system follows these rules:

1. Every entity instance has an existence state: absent, present, or deleted. Every
   instance of an entity that some use case creates starts absent. An entity that no
   use case creates is pre-existing, as master data is: its instances start present at
   the column defaults, and its variation columns start at any value. `states` reports
   such an entity as `NoCreationPath` (info), because the model has no creation path to
   draw.
2. A use case is one atomic action. Its precondition combines the states it requires
   (the author's 「ある状態の時にUCが実行できる」) with the guards of its linked
   conditions. Its effect is every `creates`, `deletes`, `sets`, and transition it
   causes, across all entities, applied in one step.
3. A transition names the use case that causes it, as the 遷移UC column of the RDRA
   state sheet does. An event raised by an external system or by another use case
   triggers use cases. It does not move a state by itself, and the order in which a
   model triggers use cases adds no guard of its own.
4. `creates` sets the initial values of a new instance, with `@default` filling the
   rest. The state that a creation sets is a start state, as a transition with no
   source state is in the RDRA state sheet. A transition may also enter that state. A
   condition that tests a column of the created instance constrains the creation: the
   instance must satisfy it after the use case runs. A `creates` of a child whose
   `relate` is `.optional()` can leave the child unlinked from its parent.
5. A column typed by a variation takes a value the actor chooses. The checker
   explores every value, so a variant needs no dedicated use case to be reachable.
   `@default` on such a column sets the DBML default only. It never pins the choice.
6. The transaction groups that `tx.rs` infers bound atomicity. A use case whose
   effects span k groups is k actions, named `<UseCase>_1` to `<UseCase>_k`. The checker
   interleaves them freely with every other action, so it explores the states in
   between. `after(UseCase).assert` holds after each of the k steps.

## Verification is bounded, fails closed, and points at the model

A passing `verify` must mean something, and a failing one must say where to look:

- Every effect in the model is exported. If a constraint reads a column that no
  action writes, `verify` warns that the constraint checks only the initial value.
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

1. **Soundness.** Export every `sets` effect, warn when a constrained column has no
   writer, and agree on one initial state. The check is a regression sample that
   reproduces today's vacuous `TLC: OK` and must now fail.
2. **Subtraction.** Delete `location`, stop declaring `state`, and fix the docs that
   describe rules the code does not have. The check is that clinic-ops lint drops the
   66 orphan findings on derived states.
3. **RDRA conformance.** Restore `condition` and `variation`, turn `timing` into a
   timer, make use cases the trigger of transitions, and add the definition-sheet
   lint rules. The check is that clinic-ops passes every inconsistency rule.
4. **One transition system.** Done. The shared transition system has existence
   states and atomic use case actions, and `states` and the TLA emitter read it. The
   check is that both engines report the same reachable states on every sample. It
   passed: breadth-first search and TLC agree on the states and on every
   invariant and `after` verdict of every cone of every sample, and CI runs that test
   (`engine_equivalence`) with TLC required.
5. **Logical data gaps.** Done. The data layer list above holds. One `ForeignKey` fact
   per key-side `relate` feeds DBML, JSON Schema, the entity CSV, the ER diagrams, the
   transaction graph, and the lifecycle owner variables. The check is the test
   `key_facts` (`crates/rdra-ish-cli/tests/key_facts.rs`), which walks every
   `samples.txt` root. Coverage: each key-side `relate` yields exactly one fact, on the
   side its cardinality names, unique if and only if it is 1:1. Resolution: every DBML
   `Ref` names a table column that exists, so no foreign key is unresolved. Agreement:
   the DBML refs, the JSON Schema entries, the entity CSV rows, the ER markers, the
   transaction edges, and the owner variables all equal the fact set. Exportability: DBML
   and JSON Schema export on every sample, including the ones whose lifecycle has
   errors. Each `@check` yields exactly one `Check_*` invariant. The formal-verification
   samples `one_to_one_shared`, `min_card_unmet`, `min_card_met`, `check_unmet`, and
   `check_met` show the `Unique_*`, `MinCard_*`, and `Check_*` invariants fail and pass
   under both engines, and CI runs them.
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

The maintainer decided these questions on 2026-10-28, during phase 4:

- An entity that no use case creates is pre-existing: its instances start present.
  Rule 1 states the exception. The strict reading, where such an entity never exists,
  would make every constraint on master data hold vacuously, which is the phase 1 bug
  again.
- The guards that the tool inferred from the order of `raises` and `triggers` are
  deleted. Rule 2 lists a use case's preconditions as its required states and the
  guards of its linked conditions. Inferred order guards are neither, and they cut
  reachable states from the model. Removing them changed no `states` output and no `check` exit code on
  any sample.

The maintainer decided these questions on 2026-10-28, during phase 5:

- The data exports do not wait for the lifecycle. DBML, JSON Schema, OpenAPI, AsyncAPI,
  the ER exports and diagram, and `csv --kind entity` read no reachability result, so
  they gate on model errors and print one note when the lifecycle has errors. Only
  outputs that print the lifecycle (`export --kind typescript-states`, the state and
  sequence diagrams, and the other CSV kinds) still refuse. The six verification samples
  with lifecycle errors now export DBML, JSON Schema, and the entity CSV.
- A minimum cardinality is always checked. `1..*:1` adds `MinCard_*` with no switch.
- A 1:1 key is unique as an invariant, `Unique_*`, not as a guard on the use case that
  links.
- A use case that creates a parent and updates an existing child links the child. For an
  optional key it links only while the child is unlinked, so a linked child keeps its
  first parent. For a mandatory key the child always has a parent, so the link is a move.
- A card side has one spelling. The range goes on the child side (`1..*:1`), and a
  parent that may be absent is `.optional()`. A range on the parent side is
  `MinCardOnParentSide`.
- A quoted `@check("...")` or `@derived("...")` is a parse error whose message is the
  unquoted rewrite.
- `Unique_*` found three double-create bugs in the samples: a second run of
  `InputPaymentInfo` (ec-site), `SetInitialPassword` (personal-info), and `OpenEncounter`
  (clinic-ops) each linked a second child to one parent. Each use case now also moves
  its parent (the order to `awaiting_capture`, the account to `active`, the appointment
  to `apptinconsult`), so a second run fails its guard. `PaymentTransaction` to `Claim`
  became optional in clinic-ops, because a copay precedes any claim.

## Sources

- The author's element and connection rules:
  [RDRA.md in RDRAAgent_v0.6](https://github.com/kanzaki/RDRAAgent_v0.6/blob/HEAD/RDRA_Knowledge/1_RDRA/RDRA.md)
- The four layers: [rdra.jp article1](https://www.rdra.jp/article1) and the
  [RDRA3.0 deck](https://speakerdeck.com/kanzaki/rdra3-dot-0wozhi-rou)
- Steps and phases: [要件定義の進め方](https://www.rdra.jp/要件定義の進め方)
- Scope of conditions and variations: [RDRAビジュアル分析](https://www.rdra.jp/rdraビジュアル分析)
- Inconsistency checks: [RDRA定義分析Sheet](https://www.rdra.jp/rdraツール/rdra定義分析sheet)
- Position of RDRA in upstream work: [RDRAとは (techRAMEN deck)](https://speakerdeck.com/kanzaki/rdratoha-techramen)
