# Semantics and notation

<!-- derived-from ../language-reference.md#entity-state-constraints -->
<!-- derived-from ../state-derivation.md#the-transition-system -->

This document gives the meaning of an rdra-ish model as a finite transition system. It also
gives the meaning of the model's rules and properties on that system, and of every verdict
the tools print. The theme documents listed in the [index](./README.md) state their guarantees
in this notation. The [claims inventory](./claims.md) refers to the definitions by number
(D1, D2, …), and each definition has an anchor of its own (`semantics.md#d1` … `#d25`).
The code it cites was read at commit `9857092a` (origin/main, 2026-11-28, the merge of #544).

**What the definitions rest on.** D1 to D13 and D24 define `T_β(M)` from the model's text.
Each clause names the surface construct it gives meaning to (an entity, a column and its
annotations, a `relate`, a `sets`, a `transitions` line, a `chooses`, a `.when`, a
`creates`, a `links`, a copy, a `raises`/`triggers` pair, a `deletes`, a split use case) and
says what that construct contributes to the variables, the initial states, or the steps.
The definitions follow [language-reference.md](../language-reference.md),
[state-derivation.md](../state-derivation.md), and the design notes
[event-order.md](../design/event-order.md) and [counted-events.md](../design/counted-events.md).
Where those notes leave a point open, the definition makes a choice, and a remark under the
definition marks it. No definition is stated in terms of what a function of the code
computes. Code appears only in **Implemented by** notes, which say where to look. Whether
the code computes `T_β(M)` is a claim with evidence, not a definition: claim
[I1](./claims.md#0-the-build) says that `TransitionSystem::build` returns `T_β(M, σ)`.

The definitions apply to a model `M` that loads and builds at `β` with no diagnostic of
severity error. Examples are `IntLiteralOutOfBounds`, `DomainTooLarge`, `ConflictingEffects`,
`UnboundCopySource`, and `AmbiguousOwnerLink`; the whole list is
[state-derivation.md, "Diagnostics"](../state-derivation.md#diagnostics). Which models are
well-formed is the language's static semantics. This document does not restate it, and
there it still relies on the code's list (`lifecycle::diag::StateDiag`). For a pair `(M, β)`
with an error, `T_β(M)` is undefined. The tools print the error and give no verdict.

D14 to D23 define the meaning of rules, properties, and verdicts on a transition system
`T`, and D25 that of the deadlock finding. They apply to `T_β(M)` and to every system derived from it (a cone, a reduced cone, a
quotient), and they cite code where a verdict is printed.

This document was checked against commit `9857092a` (origin/main, 2026-11-28; first
`b83b9544`, then `670eec79`, `fec53d6a`, `5f59fae2` and `5ae62f2c`, see [Errata](#errata)). Code is cited by Rust path (`lifecycle::types::Bounds`) or as
`file.rs::symbol`, with file paths relative to `crates/rdra-ish-core/src/lifecycle/` unless
they start with `cli/` (`crates/rdra-ish-cli/`), as in the [claims inventory](./claims.md).
**Implemented by** notes are pointers, not statements, and carry no read status. A code
citation in a statement was read at `9857092a` unless it carries `unread@pin`
([claims inventory](./claims.md#how-to-read-a-row)). Revisions are listed in
[Errata](#errata).

---

## Notation

<!-- derived-from ../state-derivation.md#variables -->

- `Ent` is the finite set of entities of `M` that the lifecycle holds. An entity
  published with `exposes(System, E).append()` or `.upsert()` is not in `Ent`.
- For a set `X`, `X⊥` is `X ∪ {⊥}`, where `⊥` is the value `null`.
- `[k] = {0, …, k-1}`. Slot `i` of entity `E` prints as `E#(i+1)`.
- A predicate `φ` with free *binders* `x₁ … x_k` over entities `E₁ … E_k` is evaluated
  under a *binding* `b ∈ [n_{E₁}] × … × [n_{E_k}]`. `φ_b(s)` is its value in state `s`.
- `s →_a s'` is a step of action `a`, and `s → s'` is a step of some action. `Reach(T)` is
  the set of states reachable from the initial states of `T`.
- `UC` is a use case of `M`. Its *writes* are the lines naming it as the writer: `creates`,
  `updates`, `deletes`, `sets`, `drives`, `links`, `relinks`, `unlinks`, `transitions`,
  `chooses`, and the CRUD lines of every API an `invokes(UC, Api)` names.
- The *join key* between two entities is the one `relate` between them. When there are
  several, it is the one a `.role(r)` of the line names. Several keys and no `.role` is the
  error `AmbiguousOwnerLink`
  ([Which key a join follows](../language-reference.md#which-key-a-join-follows)).

## Bounds

<!-- derived-from ../state-derivation.md#bounds -->

<a id="d1"></a>**D1 (Bounds).** A bound is `β = (n, m)`. `n : Ent → {1, …, 255}` gives the
slots per entity, and `m ∈ {1, …, 255}` is the top of the integer range `[m+1] = {0, …, m}`.
`--instance-count` sets `n` uniformly, `--instances E=k` overrides one entity, and
`--int-range-max` sets `m`. Every result the tools print names `β` on its `bounds:` line.
Defaults are `n ≡ 1`, `m = 5` for `states` and `check`, and `n ≡ 2`, `m = 5` for `verify` and
`export --kind tla`. A pair `(M, β)` is also ill-formed when some variable of D2 would have
more than 256 values: a nullable Int-like column at `m = 255`, a nullable Enum of 256
values, or an Enum of 257 (`DomainTooLarge`, since #470).

*Implemented by* `lifecycle::types::Bounds` (`instances`, `per_entity`, `int_max`,
`Bounds::STATES`, `Bounds::VERIFY`). Every call that turns the flags into bounds goes
through `cli/src/export.rs::checked_bounds` (for `check`, `states`, `verify`, and `export`).
It rejects `--instance-count 0` and `--int-range-max 0`, as well as values above 255. `Bounds::int_max` is a
public `u8` field, so a library caller could still build at `m = 0`. That bound is outside
D1 and has no meaning here. The domain-size check is `build/vars.rs::Builder::column_var`.

The z3 encoding of `verify --prove` reads neither `n` nor, for most Int terms, `m`
(theme (b)).

## State variables

<!-- derived-from ../state-derivation.md#variables -->

<a id="d2"></a>**D2 (Variables and domains).** `T_β(M)` has one variable per row of the
table below, held per slot of its entity. A variable `v` of entity `E` is therefore `n_E`
state components `v[0] … v[n_E - 1]`. `V` is the set of variables a given system holds.

An Int-like column (Int, Money, Decimal, Date, DateTime) is *read as a number* when any of
the following holds:

- a `sets` writes an integer to it;
- it is compared with an integer literal, with `now`, or by an order comparison (`<`, `<=`,
  `>`, `>=`) in a rule, a `@check`, a condition body that `applies_to` or `.when` uses, an
  `after` assertion, or a property;
- it is compared by `==` or `!=` with, or copied to or from, another Int-like column in a
  `sets` copy, a rule, or a condition.

A column is *named* when any of these, or a `chooses`, mentions it.

| Declaration in `M` | Variable | Domain | In `V` when |
|---|---|---|---|
| an entity `E ∈ Ent` | `Exists(E)` | `{absent, present, deleted}` | always |
| a column of type Enum | the column | its declared values, with `⊥` first under `@null` | always |
| a column of type Bool | the column | `{false, true}`; `{⊥, false, true}` under `@null` | always |
| a column of type Int, Money, Decimal, Date, or DateTime | the column | `[m+1]`, or `[m+1]⊥` under `@null` | it is read as a number |
| a column of such a type under `@null`, not read as a number | the column | `{⊥, present}` | always |
| a column of any other type under `@null` (String, Uuid, …) | the column | `{⊥, present}` | always |
| a column typed by a `variation` | the column | the variation's values | it is named |
| `drives(UC, E.c op r, b)` on a comparison `c op r` | the proposition | `{false, true}` | some `drives` names it |
| `relate(Child, Parent, …)` of shape N:1, 1:N, or 1:1 | the owner of `Child` | `[n_Parent]`, plus `⊥` under `.optional()` | always |
| a use case split into `k ≥ 2` transaction groups (D10) | the run mark `E._run_UC` | `{none, 1, …, k-1}` | a step before the last writes `E` |
| a tracked event `Ev` (D11) | the latch `E._raised_Ev` | `{false, true}` | `E` is a mark entity of `Ev` |
| `triggers(Ev, UC).counted(k)` (D11) | the counter `E._pending_Ev_UC` | `{0, …, k, over}` | `E` is a mark entity of `Ev` |
| a predicate reads `now` | `now`, one global component | `[m+1]` | always |

A state is an assignment `s` of a domain value to every component. Some columns have no
variable: a String column with no `@null`, a non-null Int-like column not read as a number,
a `@derived` column, a foreign-key column (the owner stands for it), and every column of a
published entity. A predicate that reads a Bool `@derived` column reads its defining
expression ([state-derivation.md, "Variables"](../state-derivation.md#variables)).

*Choice.* Two points follow the code (`build/naming.rs::Named`), because the notes say only
that such a column "exists when something names it". First, which columns are read as a
number. Second, that an Int-like `@null` column not read as a number is `{⊥, present}`. One
consequence: an `==` between two columns in a temporal property names them but does not
read them as numbers. The reference interpreter of P26 shares this reading with the build,
so claim I1 has no independent evidence for it.

*Implemented by* `lifecycle::types::VarKind`, `Domain`, `Domain::size`, and
`build/vars.rs`. The byte layout of a state (`TransitionSystem::offsets`, `width`) is not
part of the semantics.

<a id="d3"></a>**D3 (Base value).** Each variable has a base value `base(v)`:

- for a column, its `@default`; else `⊥` under `@null`; else the first declared Enum value,
  `false`, or `0`. On a `{⊥, present}` column, a non-null `@default` or a default function
  (`now()`, `uuid()`) gives `present`. On a number column, a `@default` that is no integer
  (`9.99`, a date) is schema text and gives the unset value;
- for `Exists`, `absent`;
- for a run mark, a latch, and a counter: `none`, `false`, and `0`;
- for a proposition of `drives`, `false`;
- for `now`, `0`;
- for an owner, `⊥` under `.optional()` and parent slot 0 otherwise. D4 overrides it on
  pre-existing children, and D8 sets it when a child is created.

A slot that is `absent` or `deleted` holds the base value in every column. A variation
column's base is its `@default`, else its first value. Only an absent or deleted slot shows
that base: a pre-existing row starts at any value (D4), and a created row takes any value the
use case does not set (D8). So the `@default` never pins a live row
(`DefaultOnVariationIgnored`).

*Implemented by* `VarDecl::base`.

## Initial states

<!-- derived-from ../state-derivation.md#variables -->

<a id="d4"></a>**D4 (Initial states).** An entity is *created* when some use case has
`creates(UC, E)`, directly or through an API that `invokes(UC, Api)` names, and
*pre-existing* otherwise. `I_β(M)` is the set of states in which every component is at its
base value, except that:

1. every slot of a pre-existing entity is `present`;
2. on a pre-existing slot, every variation column takes any value of its variation, chosen
   independently per slot ([vision.md](../vision.md#decisions) says why);
3. the owner of a pre-existing child starts by the shape of its `relate`:
   - **Any.** For an N:1 child, the owner takes any parent slot, chosen independently per
     slot, and also `⊥` under `.optional()`.
   - **Identity.** For a 1:1 child or the child of a `1..*:1` key, slot `i` is owned by
     parent slot `i`.
   - **Base.** The owner of a created entity, and an `.optional()` owner whose parent is
     created, start at base (D3): `⊥` (unlinked) under `.optional()`, and parent slot 0 on a
     mandatory key, which only absent rows hold.

   A mandatory owner of a pre-existing child whose parent is created still starts by Any or
   Identity, under a parent slot that is `absent`. The info
   `PreExistingChildOfCreatedParent` reports it.

`I_β(M)` is the product of these independent choices. No rule is assumed or checked to hold
in `I` before exploration. An initial state is a reachable state like any other, so a rule
an initial state breaks is violated with a trace of zero steps.

*Implemented by* `TransitionSystem::initial_states` (`explore/moves.rs`), `Birth`, and
`Start::{Any, Identity, Base}`.

## Predicates

<!-- derived-from ../language-reference.md#null -->

<a id="d5"></a>**D5 (Predicate evaluation).** A predicate is a comparison expression of the
language ([Comparison expressions in constraints](../language-reference.md#comparison-expressions-in-constraints)),
or a `.has`/`.none`/`.at_least`/`.at_most`/`.exactly` quantifier over related rows, combined
with `not`, `and`, and `or`. It is evaluated in two-valued logic: every predicate is true or
false in every state, and `not`, `and`, and `or` are Boolean.

- A quantifier over entity `E` ranges over the slots of `E` that are `present`. A count
  compares `|{i ∈ [n_E] : Exists(E)[i] = present ∧ φ(i)}|` with its bound.
- **Numbers and null.** When either side of a comparison is numeric (a column read as a
  number, an integer literal, or `now`), both sides are read as integers and `⊥` as "no
  number". `⊥` is `==` only to `⊥`, is `!=` to every number, and is never ordered. So when
  `x = ⊥`, `x < 3`, `x >= 1`, and `x == 3` are false and `x != 3` is true. Otherwise the two
  sides compare as values of one domain.
- **Two columns.** `==` and `!=` between two nullable columns hold only when both are
  non-null, as in SQL, so two `⊥` columns are not equal. `col == null` holds on `⊥`.
- **Owners.** An owner compared with a row is "linked to that row". An unlinked owner (`⊥`)
  is linked to no row.
- **Sets.** `t in (v₁, …)` is the disjunction of `t == vᵢ`, and `not in` is its negation.

A consequence the reader must keep in mind: `not (x < 3)` is true when `x = ⊥`, so `not`
does not commute with the SQL reading of a comparison. The language reference
([Null](../language-reference.md#null)) gives the full table: every comparison operator with
`⊥` on the left, the right, or both, and under `not`. The lint warning
`negated-order-admits-null` flags `not` over an ordering with a nullable operand in a
`@check` or a `property`, the only forms that take `not` (P62). A logical `@derived` column
is not a predicate under this definition: it reads its comparison as SQL does and is `⊥`
while an operand is `⊥`, under `not` too.

*Implemented by* `lifecycle::types::Pred`, `TransitionSystem::eval` (`expr.rs`),
`expr.rs::compare`, `build/preds/mod.rs::columns_cmp` (the two-column rule), and
`build/preds/atoms.rs::lists_whole_domain`; `lifecycle/tests/null_table.rs` computes the
reference's table through them and compares it with the page; `build/preds/checks.rs::passes`
is the `@derived` reading. `lists_whole_domain` folds a set that lists every value of a
finite domain to `true` (#71, #73) and never folds an Int set (#425). The fold does not
change a predicate's value under this definition.

## Actions

<!-- derived-from ../state-derivation.md#actions -->

<a id="d6"></a>**D6 (Action).** A use case with no write has no action. A use case with
writes has one action per *transaction group* of its writes. The groups, in order, are:

1. the writes of each invoked API, in `invokes` order;
2. the direct writes, split into the groups that `relate` keys connect;
3. each remaining isolated write.

A write to a published entity joins the use case's one internal group.

An action `a` has:

- parameters `p₁ … p_r`, one per entity its group writes plus the rows it reads (D7);
- guards `G_a`, a conjunction of predicates over the parameters (D7, D8, D9, D10, D11, and
  the guards below);
- effects `F_a` (D8), applied simultaneously to the pre-state.

**Conditions as guards.** `applies_to(Cond, UC)` with a `holds` body guards `UC`'s action,
or the first step of a split use case. `applies_to(Cond, Api)` guards the step that writes
through that API, in every use case that invokes it. The body's atoms read rows as follows:

- an atom on an entity the step writes reads that parameter, in the pre-state for an
  updated row and in the post-state for a created row;
- an atom on a parent the step only reads follows the join key;
- an atom on any other entity reads one existing row per entity, linked to the step's rows
  by a direct `relate` when there is one and independent otherwise. A row that no chain of
  links reaches is the warning `UnjoinedGuardRead`.

With `.along(A, B, …)`, each path entity the step does not bind is such a row, linked by each
hop's join key. `.has` and `.none` quantify over the related rows present in the pre-state
([state-derivation.md, "Conditions as guards"](../state-derivation.md#actions)).
`applies_to(Cond, System)` adds no guard.

*Implemented by* `lifecycle::types::Action` (`ActionOrigin::UseCase`, `Action::guards`,
`Action::effects`), `UsecaseTx::groups` (`tx.rs`), and `build/actions.rs`.

<a id="d7"></a>**D7 (Parameters and bindings).** A binding of `a` in `s` maps each parameter
to a slot. The role of a parameter comes from the line that introduces it:

| Line in `M` | Role | Admissible slot |
|---|---|---|
| `creates(UC, E)` | `New` | the lowest slot of `E` whose `Exists` is `absent`; none if there is none |
| `updates`, `deletes`, `sets`, `transitions`, `chooses`, `drives`, `links`/`relinks`/`unlinks` on an `E` the use case does not create | `Existing` | any `present` slot |
| a mandatory `relate` from a created child, a copy source that is the target's parent, a condition atom on a parent | `Parent{key}` | any `present` parent slot, with the guard that the child's owner on `key` names it, or for a created child, that the child is created under it |
| a row of `E` that an earlier step of a split use case wrote (D10) | `Earlier{i}` | the slot whose run mark is `i`, present or not |
| the first step of a handler with no row of a mark entity (D11) | `Triggered` | a `present` slot of a mark entity whose latch or counter admits the handler |

Two keys between one pair of entities give independent parent parameters. A parent the
action also writes is the child's parent only on the join key.

*Choice.* Taking the lowest absent slot for `New` is a canonical choice: slots are created in
index order, and a deleted slot is never reused (`Exists` never leaves `deleted`). The z3
encoding lets a creation take any absent row, which only adds steps
([Remarks](#remarks-on-the-definitions)).

*Implemented by* `ParamRole` and `explore/moves.rs::candidate_slots`.

<a id="d8"></a>**D8 (Effects).** Each construct contributes a guard and an effect on its
parameter `p`, applied to the pre-state `s`:

| Construct in `M` | Guard | Effect |
|---|---|---|
| `creates(UC, E)` | — | `Exists[p] := present`; every column of `p` stays at base unless a line below writes it |
| a variation column of a created row that `UC` does not write | — | one successor per value of the variation (a choice) |
| `deletes(UC, E)` | D9 | `Exists[p] := deleted`; every column of `p` returns to base; owners that name `p` keep naming it |
| `sets(UC, E.c == v)`, `v` a value, a number, `true`, `false`, or `null` | — | `c[p] := v` (a number `k` on a nullable column is `k`, not `⊥`) |
| `sets(UC, E.c == present)`, `c` of domain `{⊥, present}` | — | `c[p] := present` |
| `sets(UC, E.c == present)`, `c` a nullable Enum, Bool, or number column | — | one successor per non-null value of `c`'s domain, which for a number column is `[m+1]` (a choice) |
| `sets(UC, E.c == present)`, `c` a non-null column of any type | — | none |
| `sets(UC, E.c == vᵢ).when(Cᵢ)`, one or more lines for `c` | the step has some successor | one successor per line whose body `Cᵢ` holds, with `c[p] := vᵢ`; the body is read on the post-state for a created row and on the pre-state for an updated row |
| `sets(UC, E.c == F.d)` (copy) | `F`'s row `q` is a parameter of the step, an `Earlier` row, or `p`'s parent on the join key | `c[p] := d[q]`, with `d[q]` read before the step for an updated or earlier row and after it for a row the step creates |
| `drives(UC, E.c op r, b)` | — | the proposition of `c op r` on `p` becomes `b` |
| a created child under a parent the step binds | — | `owner[p] := q` |
| an existing child the step updates, under a parent the step creates | `owner[p] = ⊥` under `.optional()` | `owner[p] := q` |
| `links(UC, Child, Parent)` | `owner[p] = ⊥` | `owner[p] := q` |
| `relinks(UC, Child, Parent)` | `owner[p] ≠ ⊥` (none on a mandatory key) | `owner[p] := q` |
| `unlinks(UC, Child, Parent)` | `owner[p] = q` for the parent row the step or an earlier step of its run wrote, else `owner[p] ≠ ⊥` | `owner[p] := ⊥` |
| `transitions(UC, E.c, a -> b)`, one or more arms on `c` | `c[p]` is the source of some arm | one successor per arm whose source is `c[p]`, with `c[p] := b` (a choice when a source has several arms) |
| `transitions(UC, E.c, (a₁, a₂) -> b)` | as one arm per listed source | as one arm per listed source |
| `.when(C)` on an arm | — | the arm's successor is kept only when the body of `C` holds, read as for `sets … .when` |
| `chooses(UC, E.c)` | — | one successor per value of the variation |
| `chooses(UC, E.c, v₁, …)` with `.when(Cᵢ)` on values | — | one successor per listed value, a guarded value kept only when its body holds |
| a variable no line of the step writes | — | unchanged |

The effects other than choices apply together to the pre-state. Then every choice forks the
post-state, and two choices multiply. Arm, value, and copy guards filter the forked
successors. `Post_a(s, p)` is the set of successors of `s` under `(a, p)`. It is empty when
a pre-state guard fails or every successor is filtered out. A use case leaves one value per
column. Two unguarded values, a copy beside a value, or a `chooses` beside a `sets` of one
column is an error (`ConflictingEffects`, `ChoosesAndSets`), and so is any step that writes
one `(variable, parameter)` twice (#485).

*Choice.* The notes do not say how a `.when` arm guard combines with a second write of the
same column in one step. Since #485 such a step is an error, so the question does not arise.

*Implemented by* `lifecycle::types::Effect` (`Create`, `Delete`, `Assign`, `Transition`,
`Choose`), `GuardWhy::{ArmCondition, Copy, Link}`, `build/sets.rs`,
`build/transitions.rs`, `build/actions.rs`, and `explore/moves.rs::each_successor`.

<a id="d9"></a>**D9 (Delete guards).** For each `relate(Child, E, …)` whose `.on_delete` is
unset, `restrict`, or `no_action`, a `deletes(UC, E)` is guarded by: no present child names
the slot, apart from a child the same step deletes, unlinks, or relinks. The lifecycle does
not execute `cascade`, `set_null`, or `set_default`. The children of such a key stay present
and name a deleted parent, and the key gets no `DanglingKey_*`. The SQL export does write
`ON DELETE CASCADE`, `SET NULL`, or `SET DEFAULT`, so a database built from it removes the
children or clears their key where `T_β(M)` keeps them. Generated Go, Rust, and TypeScript
code executes no action either: it lists an unchecked `OnDelete_*` obligation. The warning
`DeleteLeavesChildren` (`cascade`) and the warning `DeleteLeavesChildKeys` (`set_null`,
`set_default`) name each use case that deletes such a parent and neither deletes the child
nor, for the latter two, moves its key (P63). An `E` with a `@soft_delete` column gets no
guard and neither warning.

*Implemented by* `GuardWhy::Restrict`; the warnings by
`build/preds/entity_rules.rs::cascade_deletes`.

<a id="d10"></a>**D10 (Split use cases and runs).** A use case whose writes form `k ≥ 2`
transaction groups (D6) is the actions `UC_1 … UC_k`. Each entity that a step before the
last writes gets a run mark, `E._run_UC`. It records the last finished step of a pending run
on that row:

- `UC_1` binds only rows whose mark is `none` and sets the mark to `1`;
- `UC_i` (`i > 1`) binds each such row as `Earlier{i-1}` and sets the mark to `i`, or to
  `none` at `i = k`;
- any action may run between two steps, since each step is its own transaction.

A run never aborts. A step whose guard fails waits forever, which shows as a deadlock or a
use case that never runs. A delete does not reset the mark, so a run finishes on a row an
earlier step deleted. A condition applied to the use case guards step 1 only. A condition
applied to an API guards that API's step.

*Implemented by* `VarKind::Run` and `build/actions.rs` (`run_marks`).

<a id="d11"></a>**D11 (Triggers).** `raises(UC, Ev)` and `triggers(Ev, UC2)` order the two
use cases. An event is *tracked* when every `raises` of it names a use case with an action.
It is *free* otherwise: it has no `raises`, an external system or a timer raises it, or a
raiser writes nothing. The *candidates* of a tracked event are the entities that the last
step of every raiser binds. Its *mark entities* are the candidates some handler reads.

- **Latch.** The last step of every raiser sets `E._raised_Ev := true` on each of its
  parameters on a mark entity `E`. It never waits for the handler.
- **Guard.** The first step of a handler `triggers(Ev, UC2)` is guarded by the latch on its
  own row of `E` when it has exactly one row of `E` that can be the raiser's. Otherwise it
  binds a `Triggered` parameter (D7), linked to its rows by the one direct key when there is
  one.
- **Variants.** A use case triggered by several tracked events has one copy of its first
  step per event, `UC2_by_Ev`.
- **Free events.** A free event has no mark and orders nothing.

**The latch is never reset, by design.** Nothing sets the latch back to `false`: neither the
handler, nor a delete, nor a second raise. So after the first raise on a row, the handler may
run there any number of times, including zero, with no further raise. The order the latch
imposes is "not before the first raise on these rows", nothing more. This is the decision
recorded in [event-order.md](../design/event-order.md#decisions-in-one-place), question 1 and
rule 9, with the reasons in its answer 1. A consuming mark multiplied the explored states
(clinic-ops: 4,215 states with the latch against 104,187 with consume-once). It also removed
the sub-lifecycle reduction from every clinic-ops cone. Shapes in which the raiser waits for
the handler hid violations. A modeler who needs one handler run per raise opts in with
`triggers(Ev, UC2).counted(k)`
([Counting raises](../language-reference.md#counting-raises-counted),
[counted-events.md](../design/counted-events.md)):

- **Counter.** On that edge a counter `E._pending_Ev_UC2` replaces the latch. A raise moves
  it from `i` to `i+1` and from `k` to `over`. The handler needs a counter other than `0` and
  moves it from `i` to `i-1`. `over` stays `over`, and the handler then runs as under the
  latch.
- **Overflow.** The state rule `EventOverflow_<E>_<Ev>_<UC2>` says no present row is at
  `over`.

*Implemented by* `VarKind::Raised`, `GuardWhy::Raised`, `InvKind::Overflow`, and
`build/events.rs` (`count_up`, `count_down`).

<a id="d12"></a>**D12 (Clock).** When some predicate reads `now`, the system has the action
`Tick`. Its guard is `now < m` and its effect is `now := now + 1`. A Date or DateTime column
is a point on that clock: `sets(UC, E.due == 3)` places it at 3.

*Implemented by* `ActionOrigin::Clock` and `build/vars.rs::Builder::tick`.

## The transition system

<a id="d13"></a>**D13 (Transition system).** `T_β(M) = (S, I, →)`, where `S` is the set of
assignments of D2, `I = I_β(M)` (D4), and `s → s'` holds iff some action `a` of D6, D10, D11,
or D12 and some binding `p` admissible in `s` (D7) give `s' ∈ Post_a(s, p)` (D8, D9). `S` is
finite for every `β`. A *step* is a triple `(s, (a, p), s')`. A step with `s' = s` is
possible (an action that rewrites a value with itself), and every liveness check treats it as
stuttering (D20).

*Implemented by* `TransitionSystem::build`. BFS (`check`, `states`, `verify`), the TLA+
printer (`export --kind tla`, `verify --with-tlc`), and the z3 encoding (`verify --prove`)
all read the one value it returns. None of them reads the model a second time. So the
engines share any departure of that value from `T_β(M)`, and agreement between them cannot
show one. That the value is `T_β(M, σ)` is claim [I1](./claims.md#0-the-build).

<a id="d24"></a>**D24 (Scope).** A *scope* `σ` is the set of use cases a run keeps. `--buc`
keeps the use cases reachable from the named BUCs. `--without-usecase`, `--without-buc`, and
`--actor` leave use cases out. `T_β(M, σ)` is `T_β(M)` with every action of a left-out use
case removed, and `T_β(M) = T_β(M, ALL)`. `--entity` removes no action; it selects which
checks run. Every verdict of a scoped run is a verdict of `T_β(M, σ)`, and the `OK:` line
says `in the selected scope`. The definitions below are stated for `T_β(M)` and apply to
`T_β(M, σ)` unchanged. Theme (b) cites D24 for what a proof under a scope covers (P4).

*Implemented by* `select.rs::Scope`, passed to `TransitionSystem::build`.

<a id="d14"></a>**D14 (Behaviors).** A *finite trace* is `s₀ → s₁ → … → s_k` with `s₀ ∈ I`. A
*behavior* is an infinite sequence `σ = s₀ s₁ …` with `s₀ ∈ I` in which, for each `i`,
`s_i → s_{i+1}` or `s_{i+1} = s_i` (a stuttering step), as in TLA+. A behavior *stops* at
`s` when from some point on it stutters at `s` forever. A state with no successor other than
itself is *terminal*, and a behavior may stop there under every fairness assumption below.

<a id="d15"></a>**D15 (Cone).** Let `W ⊆ V` be a set of variables closed under the reads of
the actions that write it (`cone.rs`). `T|W` is the system restricted to `W`: actions that
write nothing in `W` are dropped, the others keep their effects on `W`, and states are
projected to `W` (`TransitionSystem::restrict_to`). Theme (a) defines the closure. It states
when `Reach(T|W) = π_W(Reach(T))` holds and when only `⊇` holds. This document needs only
the definition, because every verdict below is defined on `T_β(M)` itself.

## State rules

<!-- derived-from ../state-derivation.md#checks -->

<a id="d16"></a>**D16 (State rule).** A state rule has binders `E₁ … E_k` and a body `φ`. It
holds in `s` iff `φ_b(s)` holds for every binding `b` of present slots. `T` satisfies it iff
it holds in every `s ∈ Reach(T)`. A rule over an entity with no present slot holds. Every
rule form denotes one: `forbidden`, `invariant`, `required`, `exclusive` (local and
multi-entity, where `.along` adds owner equalities to the body's guard), quantifier rules,
`@check`, `@unique` and N:M `.via` (`Unique_*`), 1:1 keys (`Unique_<Child>_<role>`),
cardinality ranges (`MinCard_*`, `MaxCard_*`), restricting keys of a deleted parent
(`DanglingKey_*`), and counted triggers (`EventOverflow_*`)
([state-derivation.md, "Checks"](../state-derivation.md#checks)).

The bodies of the written forms, with conditions `c₁ … c_r` read by D5 and every binder
ranging over present slots
([language-reference.md, "Entity state constraints"](../language-reference.md#entity-state-constraints)):

| Form | Body `φ` | Name |
|---|---|---|
| `forbidden(E…, c₁, …, c_r)` | `¬(c₁ ∧ … ∧ c_r)` | `Forbidden_<E>_<n>`, `MultiForbidden_<E>_<n>` |
| `invariant(E…).when(g₁…).then(r₁…)` | `(g₁ ∧ …) ⇒ (r₁ ∧ …)` | `Inv_<E>_<n>`, `MultiInv_<E>_<n>` |
| `required(E, c₁, …, c_r)` | `c₁ ∧ … ∧ c_r` | `Required_<E>_<n>` |
| `exclusive(E, c₁, …, c_r)` | `∧_{i<j} ¬(c_i ∧ c_j)` | `Exclusive_<E>_<n>` |
| `invariant(E).when(g…).has(R.c…)` / `.none(…)` / `.at_least(k, …)` | `g ⇒ ∃r. link(e, r) ∧ c` / `g ⇒ ¬∃r. …` / `g ⇒ Count ≥ k` | `Quantifier_<E>_<n>` |

`.along(A, B, …)` conjoins the owner equalities of the path to the guard side, so the rule
speaks only of linked rows. A `.when(Cond)` adds the comparisons of `Cond`'s `holds` body.

*Implemented by* `lifecycle::types::Invariant`, `InvKind`, and
`expr.rs::find_violation`.

<a id="d17"></a>**D17 (After assertion).** `after(UC).assert(ψ)` holds iff `ψ` holds in `s'`
for every `s ∈ Reach(T)` and every step `s →_a s'` with `s' ≠ s`, where `a` is the *last*
step of `UC` (of every variant `UC_by_Ev`). In `s'` the action's parameters are bound to
their slots, and the scope entities the action does not bind are universally quantified over
present slots of `s'`. States between two steps of a run are not checked, and neither is a
last step that changes nothing. The native run skips the check on a self-loop
(`explore/bfs.rs`, `Probe::Loop`, in every build). TLC checks the `PROPERTY`
`[][(\A p : UC(p) => post')]_vars`, which every stuttering step satisfies. This is the
intended reading, as in TLA+ (P22, fixed in #494): the language reference says so, and D18
reports an assert whose last step only ever stutters as vacuous.

<a id="d18"></a>**D18 (Vacuity).** A rule *holds vacuously* when no reachable state meets one
of its premises
([state-derivation.md, "Rules that hold vacuously"](../state-derivation.md#rules-that-hold-vacuously)).
Vacuity is a warning about a rule that holds and never changes a verdict. An after-assert's
premise is a step of its use case's last step with `s' ≠ s`, the steps D17 checks. A last
step that runs only with `s' = s` is reported (`UnmetPremise::OnlyStutters`), and so is one
that never runs (`targets.rs::unmet_of_assert`).

## Temporal properties

<!-- derived-from ../language-reference.md#temporal-path-properties -->
<!-- derived-from ../language-reference.md#order-and-stability-stable-unchanged-precedes -->
<!-- derived-from ../language-reference.md#goals-on-present-rows-present -->

A property has binders `E₁ … E_k`, a link predicate `ℓ` (the owner equalities of `.along`,
or `true` without it), a formula, and a set of fair use cases. For a binding `b`, define the
*trigger*

```
trig_b(s)  ≝  (∀j. s(Exists(E_j))[b_j] = present)  ∧  ℓ_b(s)
```

`pres_b(s)` is the first conjunct alone. Operands are evaluated by D5 on the slot whatever its
existence, so on an absent or deleted slot they read base values. The property holds iff it
holds for every binding. *Implemented by* `lifecycle::types::Property` and
`lasso.rs::TransitionSystem::triggers`.

<a id="d19"></a>**D19 (Safety forms).** On `T`:

| Form | Holds iff | Code |
|---|---|---|
| `always(x)` | `∀s ∈ Reach(T). trig_b(s) ⇒ x_b(s)` | `ComponentRun::unsafe_state` |
| `stable(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ p_b(s')` | `TransitionSystem::step_breaks` |
| `unchanged(c₁…c_r).when(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ ∀j. c_j(s') = c_j(s)` | `TransitionSystem::step_breaks` |
| `precedes(p, q)` | for every finite trace `s₀ … s_k`: `trig_b(s_k) ∧ q_b(s_k) ⇒ ∃j ≤ k. trig_b(s_j) ∧ p_b(s_j)` | `ComponentRun::unlatched_state` |

`stable` and `unchanged` read the link before the step only. Steps with `s' = s` break
neither form. `precedes` is the safety property "`q` not before `p`" per binding, and its
TLA+ form is a latch `P_seen` per binding.

<a id="d20"></a>**D20 (Fairness).** For a property `P` with `.fair(UC₁, …)`, a *fair
instance* is a pair `(a, κ)`: an action `a` of some `UC_i` (every step of a split use case),
and the slots `κ` its non-`New` parameters bind. `(a, κ)` is *enabled* at `s` when some
binding of `a` that extends `κ` has a successor `s' ≠ s`. It is *taken* by a step `s → s'`
with `s' ≠ s` whose action is `a` and whose binding extends `κ`. A move that changes no
variable neither enables nor takes an instance. This is how `⟨A⟩_vars` reads it in TLA+ and
how `lasso.rs::TransitionSystem::unfair_loop` reads the moves (P56, fixed in #475). So a
behavior that stays at one state forever through a self-loop of a fair instance does not take
it. Two fairness conditions on a behavior `σ` are used:

- `Fair_P`: every fair instance that is enabled at every state of `σ` from some point on is
  taken infinitely often. This is weak fairness per instance,
  `∧_{(a,κ)} WF_vars(∃ new slots : a(κ, …))` in the TLA+ output.
- `WF_vars(Next)`: if from some point on a step to another state is enabled at every state,
  infinitely many steps change the state.

There is no strong fairness. No `SF` form exists in the language, in the TLA+ printer, or in
`lasso.rs::Fairness`. Its two values are `Plain` (`Fair_P` alone) and `WfNext`
(`Fair_P ∧ WF_vars(Next)`). A property with no `.fair` has `Fair_P = true`. *Implemented by*
`Property::fair_instance`, `ParamRole::keys_fair_instance`, and
`explore/liveness.rs::fair_steps`.

<a id="d21"></a>**D21 (Liveness forms).** Let `goal` and `cause` be `goal = x`,
`cause = true` for `eventually(x)`, and `goal = y`, `cause = x` for `leads_to(x, y)`. Under a
fairness condition `F`, the property holds on `T` iff the following is true: for every
behavior `σ` of `T` that satisfies `F`, every binding `b`, and every `i` with
`trig_b(σ_i) ∧ cause_b(σ_i)`, some `j ≥ i` has `goal_b(σ_j)`. So `eventually(x)` is
`□(trig ⇒ ◇x)` per binding, not `◇x`, and `goal` is read at `j = i` too. `goal_b(σ_j)` does
not require `pres_b(σ_j)`. On a slot deleted between `i` and `j` it reads the base values D8
leaves, so a delete meets a goal those values satisfy and no other goal. This is the defined
semantics (open problem P6, decided as documented):
language-reference.md, "Temporal path properties", states it, and the TLA+ formulas
`[](present => <>x)` and `(present /\ x) ~> y` read it the same way.

With `.present()` (open problem P64), the goal asks for a present binding:

| Form | `cause` | `goal` |
|---|---|---|
| `eventually(x).present()` | `true` | `pres_b ∧ x` |
| `leads_to(x, y).present()` | `x` | `pres_b ∧ y` |

The definition above applies unchanged with this `goal`. `trig_b` is the same, so the link
`ℓ` stays in the trigger only. A deleted slot is never reused (D7), so once a bound slot is
deleted at some `k ≥ i` before any `j` with `goal_b(σ_j)`, no later `j` has `pres_b(σ_j)`,
and the behavior fails the property if it satisfies `F`. The TLA+ forms are
`[](present /\ link => <>(present /\ x))` and `(present /\ link /\ x) ~> (present /\ y)`.
`.present()` on any other form is the error `PresentOnNonGoalProperty`. *Implemented by*
`build/preds/temporal.rs::Builder::goal`, which conjoins `pres_b` into the goal predicate,
so the native search, the lasso check, and the TLA+ printer all read one predicate. `pres_b`
reads only the binders' `Exists` variables, which every property's cone already holds
(`cone.rs::check_reads`). So `.present()` changes neither the cone nor the reduction, and
the symmetry scan reads no row id in it.

The verdict a property is meant to have is the one under `WfNext`. The exported `Spec`
conjoins `WF_vars(Next)` whenever the model has an `eventually` or `leads_to`, and `P` is
printed as `Fair_P ⇒ (…)`. `Plain` is a stronger requirement (more behaviors). So `ok` under
`Plain` implies `ok` under `WfNext`, and a counterexample under `WfNext` is one under `Plain`.
`formal-verification.md`, "Temporal properties on a cone", uses exactly these two
implications to order its three runs. Theme (c) states how the native checker decides each
run (`ComponentRun::decide_liveness`).

<a id="d22"></a>**D22 (Recoverability).** `can_reach(g)` holds iff

```
∀s ∈ Reach(T). trig_b(s) ⇒ ∃ s = t₀ → t₁ → … → t_r.  (∀j. pres_b(t_j)) ∧ g_b(t_r)
```

That is CTL `AG (trig ⇒ E[pres U (pres ∧ g)])` per binding, decided on the explored graph by
one backward pass per binding (`reach.rs::reach_failure`, which since #478 asserts a complete
graph in every build). The trigger alone reads the link. No fairness applies, and an instance
may also loop forever. *Implemented by* `lifecycle::types::Reach`.

## Verdicts

<!-- derived-from ../formal-verification.md#what-verify-checks -->

A verdict is a statement about `T_β(M, σ)` and one property or rule. For `proved`, it is a
statement about every `T_{β'}(M, σ)` in the proof's scope. Below, "within `β`" means "of
`T_β(M, σ)`". Every word the tools print for a check is one of these. A verdict means what
this table says only as far as the build returns `T_β(M, σ)` (claim
[I1](./claims.md#0-the-build)). The tools' own output says nothing about that claim.

<a id="d23"></a>**D23 (Verdict vocabulary).**

| Printed | Where | Meaning |
|---|---|---|
| `ok` | a cone or property line of `verify` (`cli/src/tlc_output.rs::Outcome::Ok`) | The checks of the line hold on `T_β(M)` (D16, D17, D19, D21 under the fairness of D20, D22). For a line of a cone the verdict is derived from the cone, and theme (a) owns the step from cone to model. |
| `ok (checked in Cone_X)` | a covered safety cone (`cli/src/verify/bfs.rs`) | `ok`, decided by the run of a cone `Cone_X` that holds every variable, action, guard, and parameter of this one. |
| `counterexample` | a line, or `result:` (`RunResult::Counterexample`) | A finite trace (safety) or a lasso (liveness) of the explored system that breaks a check. It is a behavior of `T_β(M)` except where theme (a) says otherwise. A cone that frees slots may need more instances of an entity it does not read; such a counterexample is re-decided on the whole model for properties, or printed with the instances it needs. A cone that leaves a raised mark out may reach states the event order forbids; since #477 such a counterexample of a state rule or `after` is decided again, and since #509 by a refinement loop (`targets.rs::TransitionSystem::recheck_on`): the cone grows by the raisers of the events the trace runs without (`free_triggers`) and by the entities it creates past the bound (`over_bound`) until its run vouches for the trace or finds none, with the exact cone (`exact_target_cone`) as the last step (P1, fixed). Every printed trace, safety or liveness, replays through the step relation of the system it is about before it prints; one that does not is a `tool_error` (P20, fixed in #518 and #522). Since #529 and #531 the lifts' own look-ups are faults too (`explore/orbit.rs::lift_path`, `reduce/lift.rs::lift`, the lasso lift `explore/orbit/liveness.rs::Product::lift`), and since #541 so is the search for a class move's S path (`reduce/partition.rs::Succ::shortest_path`, which panicked until then); the `expect`s and `panic!`s left are listed in the index's P20 row with the reason each holds or the assumption it rests on; the two that read a lifted path (`reduce/lift.rs::lift_stop`'s state list, `explore/orbit.rs::run_orbits_annotating`'s last step of an `after` violation) hold whatever path of the system's own states is lifted. |
| `violated` | `property P (BFS on its cone): violated`; `can_reach (BFS on N cones): k checked, m violated` (`cli/src/verify/bfs.rs`) | `counterexample`, in the lines of the forms only BFS decides (`stable`, `unchanged`, `precedes`, `can_reach`). |
| `not decided` | a line (`cli/src/verify/native.rs::Answer::Stopped`, `RunResult::NotDecided`) | The run stopped before it found a counterexample. No claim is made either way. The line's note names the stop: `stopped at the state cap (--max-states) or the memory limit`, `stopped at the state cap of N states`, `stopped at the memory limit of L`, `stopped at the time budget of S s`, or, for a property its cone cannot decide, `needs the whole model's WF_vars(Next)` and `its counterexample may create instances past the bound`. |
| `tool_error` | a line or `result:` | A tool fault, not a fact about the model: a counterexample or trace that does not certify itself (`LassoFault` for a property, `LiftFault` for a state rule or `after`, `TraceFault` for a deadlock, a `can_reach` failure, or a witness; P20), a disagreement between the native checker and TLC, or a TLC failure without a violation. |
| `result: ok` | last lines | Every check of the run is `ok`, with no tool error, nothing undecided, and, under `--deny-unproved`, nothing unproved (`cli/src/verify/end.rs::End::result`). A trace that fails to replay counts as a tool error there since #522 and #524. |
| `result: not decided` | last lines | No failure and no tool error, but some finding a cap or budget left open, or an unproved rule under `--deny-unproved`. The exit status is 0 unless `--deny-undecided` (or `--deny-unproved`) is given. The `OK:` line then holds only for what was decided. |
| `MemoryCapReached_*`, `StateCapReached_*`, `PatternCapReached_*` | `check`, `states` | An error: the cone stopped, so its rules are not checked. `check` does not report it as a pass. |
| `proved: R` | `verify --prove` (`prove/mod.rs::Verdict::Proved`) | `R` belongs to a set of formulas that is inductive for the z3 encoding of the built system. That encoding widens D13: any absent row for `New`, and Int terms and `now` over all non-negative integers. Hence, if z3 is sound and the hypotheses of [z3.md, Theorem 1](./z3.md#the-soundness-theorem) hold, `T_{β'}(M, σ) ⊨ R` for every `β'` in the proof's scope `Sc(M, β)` ([z3.md, Definition 3](./z3.md#the-soundness-theorem)). That scope contains `β` whenever the run builds. Since #481 it is every bound at which the model builds (P43, fixed); its conditions are owned by z3.md (claim B2). Since #514 the `proved:` line, like every line that claims a proof, prints that scope: `for any number of rows and every --int-range-max from L to M`, where `L` and `M` are the least and the greatest `m'` at which the build reports no error (`cli/src/prove.rs::Claim::of`). Under a scope `σ` (D24) the encoded system is the scoped one, and since #483 and #514 the same claim ends `in the selected scope: <flags>` (P4); otherwise it ends `in the whole model`. |
| `unproved: R: not inductive; …` / `unproved: R: z3 gave up on A` / `unproved: R: an initial state violates it` | `verify --prove` (`Verdict::Unproved`, printed by `cli/src/prove.rs`) | Not proved. `NotInductive`: a counterexample to induction exists from a state where `R` and every kept candidate hold, not necessarily reachable (since #525 the printed line says so: "it, every proved rule, and every kept lemma hold", P5). `Unknown`: z3 answered `unknown` or ran out of its resource limit on the step of action `A` (this is the verdict the task list calls "unknown"). `Initial`: an initial state of the encoding, possibly with more rows or larger Int values than `β`, breaks `R`. None of these says anything about the bounded verdict. |
| `out of scope: R: why` | `verify --prove` (`Verdict::OutOfScope`) | The encoding cannot state `R`. No claim. |
| `refuted: R (BFS counterexample at k instances)` | `verify --prove` (`Verdict::Refuted`) | A safety cone of the same run reached a state that breaks `R`, so the prover did not try it (`cli/src/verify/refuted.rs`). The violation is BFS's error, not an unproved rule. Since #477 only a violation the cone vouches for, or one a rerun found again (`recheck_cones`, the refinement loop of the `counterexample` row, #509), refutes a rule (P1, fixed). `k` is the count of every entity of the cone when they share it; since #532, when the counts differ (under `--instances E=k`, or because the trace creates more rows of an entity the cone does not count than the bound), each is printed, as `Order=1 Payment=2 instances` (`cli/src/state_findings.rs::Instances`, P11). |
| `escalated: R: ok at k instances` / `counterexample at k instances` / `not decided at k instances` | `verify --prove` (`cli/src/verify/escalate.rs`) | For a rule BFS passed and the prover left unproved, BFS on its cone at one and two more instances per entity. `counterexample` there fails the run. It is a violation of `T_{β'}(M)` for some larger `β'` under the conditions of `counterexample` above. Since #477 a counterexample the cone cannot vouch for, one that runs a use case without its event or creates more instances than the cone counts, is decided again (`recheck_on`, refined since #509) and its instances are printed (P11, fixed): `k` is one number when every entity of the cone that decided has it, else each entity's own, as `ok at Loan=3 Member=2 instances` (#532). |
| `decided` | prose, design notes | Not printed as a verdict. A check is decided when its verdict is `ok` or `counterexample`. |

`stopped at the memory limit` and `stopped at the state cap` describe why a run is
`not decided`; they are not separate verdicts.

<a id="d25"></a>**D25 (Deadlock finding).** For an entity `E`, a lifecycle column `v` of `E`,
and a slot `k`, a reachable state `s` of the explored system with row `k` present is
*stuck at `v`* when no path from `s` takes an edge that changes `v` at `k` or deletes row
`k` (`findings.rs::TransitionSystem::can_leave`). A value `x` of `v` is *open* when `@end`
does not name it and some `transitions` arm of `v` leads from `x` to a value other than `x`
(`findings.rs::deadlocks`, which keeps a value only when `has_way_on` holds of its `exits`).
Every other value is an end state: one `@end` names, one no arm leaves, one whose only arms
lead back to itself, and one whose only way out is a use case that deletes the row. Such a
value gets no deadlock finding, even where a state holds it with no path out.
`Deadlock_<E>_<v>_<x>` reports the first reachable state, in BFS order, that is stuck at `v`
with row `k` holding an open value `x`. A deletion counts as leaving `x` only once `x` is
open: it is one of the edges `can_leave` reads, never what makes `x` open. The definition reads
paths, not single steps: a state whose only paths run forever through steps that leave `v`
and row `k` alone, a silent cycle, is stuck exactly as a state with no step left is. This is
on purpose. It is the reading the cone reduction keeps, since a divergence-blind stuttering
bisimulation keeps whether a path reaches an edge that changes `T_v` ([cones.md, A8,
consequence 3](./cones.md)) and does not keep whether a state has a successor (P34). TLC's
deadlock is the other reading: a state with no successor at all. The two differ on a silent
cycle, and the engine corpus records the cases (`cone_reduced_loop_unreached`).

---

## Prose that the code contradicts

Each row is a place where the modeler-facing prose says something that D1 to D25 and the
code do not. In each case the definition chose the code's behaviour. A departure of the code
from the definitions is a different matter: claim [I1](./claims.md#0-the-build) and the open
problems it names cover those. None of these rows is a soundness problem by itself. The
soundness questions are in the index's [Open problems](./README.md#open-problems).

| # | Prose | Definition and code | Note |
|---|---|---|---|
| S1 | `language-reference.md`, "Temporal path properties": `eventually(expr)` is `<>expr`. | D21: `□(present ∧ link ⇒ ◇x)` per binding (`lasso.rs::fails_at`, `explore/liveness.rs::liveness_search`). | `formal-verification.md`, "Mapping", states the code's form. Resolved with P6: the language reference now gives `[](present => <>expr)` and `(present /\ p) ~> q`. The doc comments of `crates/rdra-ish-core/src/model/constraints.rs::TemporalFormula::Eventually` and `::LeadsTo` still give `<>expr` and `p ~> q` at the pin. |
| S2 | `language-reference.md`, "Temporal path properties" and "Recoverability": `can_reach` is `AG (present => EF expr)`. | D22: `AG (trig ⇒ E[pres U (pres ∧ goal)])`; the path must keep the instance present (`reach.rs::reach_failure`). | `formal-verification.md`, "Recoverability on a cone", and `types.rs::Reach` state the code's form. The two differ when the goal holds on a deleted slot's base values. Resolved with P6: the language reference now gives the code's form. The doc comments of `crates/rdra-ish-core/src/model/constraints.rs::TemporalFormula::CanReach` and `crates/rdra-ish-syntax/src/ast.rs::AstTemporalFormula::CanReach`, and the comment the TLA+ module prints (`crates/rdra-ish-emit/src/tla/module.rs::checks`), give the code's form too since #498. |
| S6 | `language-reference.md`, "Entity state constraints": `after(UC).assert(…)` "checks the state each run of the use case leaves" and "must hold after every arm". | D17: a step with `s' = s` is not checked. The native run skips the `after` check on a self-loop (`explore/bfs.rs`, `Probe::Loop`, in every build), and the TLA+ `PROPERTY` `[][(\A p : UC(p) => post')]_vars` admits a stuttering step. | Resolved (P22, #494): the reference now says a last step that changes nothing is not checked, as in TLA+, and `Vacuous_AfterAssert_*` warns when the last step only ever runs that way. |
| S3 | `formal-verification.md`, "Mapping": `always` is `[](present => x)`. | D19: the trigger is `present ∧ link`; without `.along` the link is `true`. | Consistent once `.along` is read into `present`, as the "Scope" table of `language-reference.md` does. |
| S4 | `state-derivation.md`, "Variables": Int columns range over `0..5`. | D2: `[m+1]`, with `m` from `--int-range-max`; 5 is only the default. | The language reference and state-derivation.md, "Bounds", mention `--int-range-max`. Listed so that theme writers quote `m`, not 5. |
| S5 | `prove/mod.rs::Verdict::Proved` doc comment, until #514: "Holds for any number of rows, at the stated value bounds"; `cli/src/prove.rs::ProofTally::clause`, until #514: "any Int value". | D23 and z3.md, Definition 3: the bounds `Sc(M, β)`. Inside the encoding (`prove/encode.rs`, `Enc::in_domain`), Int terms and `now` range over every non-negative integer with no top. | Resolved (P2, #514; P43, #481): `Verdict::Proved`'s doc now cites Theorem 1 over `Sc(M, β)`, and `clause` ends with the `Claim::of` text, `for any number of rows and every --int-range-max from L to M` and the scope. The `--prove` help text (`cli/src/cli.rs`) said "any Int value" until #525. |

## Remarks on the definitions

- **Operands on absent rows.** D21 reads `goal` on a slot whatever its existence (D22 does
  not: its path, goal state included, keeps the row present), and D8 resets a deleted slot's columns to base. So `eventually(Order.status == draft)`, with
  `draft` the base value, is met by deleting the order. TLC reads the TLA+ formula the same
  way (`[](present => <>x)`), so the engines agree. Open problem P6 decided this as
  documented: language-reference.md, "Temporal path properties", now says so, with an
  example each way and how to state a goal a delete does not meet. `.present()` (D21, P64)
  states such a goal per property.
- **Initial states are not assumed to satisfy anything.** D4 makes the initial set a product.
  That includes N:1 owners of pre-existing children under a created parent that is still
  absent (`PreExistingChildOfCreatedParent`).
- **Lowest-slot creation is a symmetry choice.** D7 creates in index order. The TLA+ module
  does the same (`FirstAbsent`). The z3 encoding lets a creation take any absent row
  (`formal-verification.md`, "Proofs for any number of rows"), which only adds steps.
- **A latch, not a queue.** D11's latch lets a handler run any number of times after one
  raise, and a raiser never waits. For safety this over-approximates every consuming
  delivery. It can hide only a liveness or reachability finding that needs "one run per
  raise", which `.counted(k)` states.
- **The definitions are parametric in `β`.** `n` enters D2 (key domains), D4 (Identity
  starts), and D7 (slots). `m` enters D2 (Int domains), D8 (`present` on a number), and D12
  (`Tick`). Nothing else depends on `β`. Theme (b)'s Lemma U uses this.

## Errata

- **2026-11-28, re-pin to `9857092a`** (the merge of #544). The crates change after
  `5ae62f2c` only by #544 (`prove/encode.rs::Enc::one_of` and its call, P66, and tests). The
  one citation of `encode.rs` here, `Enc::in_domain` in S5, is unchanged; every reading at
  `5ae62f2c` carries over.
- **2026-11-28, fourth audit; re-pin to `5ae62f2c`** (the merge of #542). The crates change
  after `5f59fae2` only by #539 (a test) and #541 (`reduce/lift.rs::lift`,
  `reduce/partition.rs::Succ::shortest_path`), which were read. D23's `counterexample` row
  credited the fault of the S-path search to #529 and #531 under the old pin, where
  `shortest_path` still panicked; it now credits #541.
- **2026-11-28, second audit at `5f59fae2`.** The remark "Operands on absent rows" said D22 reads `goal`
  whatever the row's existence; D22's path keeps the row present, goal state included, as
  `reach.rs::reach_failure` does. S2 cited an `Ltl` in `model/constraints.rs`, which has
  none (`Ltl` is `types.rs::Ltl`), and paths outside `lifecycle/` without their crate; it now cites `TemporalFormula::CanReach` and the
  other two in full. S1 records that the `Eventually` and `LeadsTo` doc comments of
  `TemporalFormula` still give the superseded forms. D23's `counterexample` row no longer
  says none of the panics left is on the path being lifted.
- **2026-11-28, re-pin to `5f59fae2`** (the merge of #537). Every `file.rs::symbol` a statement
  cites was checked against `git diff fec53d6a 5f59fae2`, and each cited item a hunk touches
  was read again: `cli/src/verify/refuted.rs`, `cli/src/verify/escalate.rs` and
  `cli/src/state_findings.rs::Instances` (#532), `explore/orbit.rs::lift_path` and
  `reduce/lift.rs::lift` (#529), and `explore/orbit/liveness.rs` (#531). Changes: the
  `refuted` and `escalated` rows give the per-entity form of `k`; the `counterexample` row no
  longer says the lifts rest on `expect`s, which #529 and #531 made faults; D25's citation
  of `can_leave` follows this document's path convention (`findings.rs`), where it read
  `core/src/lifecycle/findings.rs`.
- **2026-11-28, final audit.** **Pin** moved from `670eec79` to `fec53d6a`. Every
  `file.rs::symbol` a statement cites was checked against `git diff 670eec79 fec53d6a`, and
  each cited item a hunk touches was read again: `cli/src/verify/end.rs::End::result` (the
  `unread@pin` mark on the `result: ok` row is gone), `targets.rs::TransitionSystem::recheck_on`
  and `unmet_of_assert`, `cli/src/verify/refuted.rs`, `cli/src/verify/escalate.rs` (the #477
  parts of D23, no longer `unread@pin`), `reduce/lift.rs::lift`, `cli/src/prove.rs::Claim`
  and `ProofTally::clause`, `prove/mod.rs::Verdict`,
  `build/preds/entity_rules.rs::cascade_deletes` (D9, as #501 left it), and
  `build/preds/temporal.rs::Builder::goal` (D21); `explore/liveness.rs`, `explore/moves.rs`
  and `reach.rs` changed only in integer casts. No other cited item changed, so its earlier
  read carries over. D25's citations are those of #520 and #525. Changes:
  - **D23**, `counterexample`, `refuted`, `escalated`: a counterexample the cone cannot
    vouch for is decided by a refinement loop since #509, not on the exact cone at once
    (P1). The `counterexample` row names the `expect`s the lift still rests on.
  - **D23**, `proved`: Definition 3 no longer restricts `m'` (P43, #481), and the line
    prints its scope (P2, P4; #514).
  - **D23**, `ok`: "the fairness of D21" was a slip for D20.
  - **D20** called P56 open; it was fixed in #475.
  - **S5** is resolved (#514, #481, #525).
- **2026-11-28, D25 (P34).** D25, the deadlock finding, was added in #520 without an
  erratum, and the lists of definitions still said "D1 to D24". They now say D1 to D25.
  #525 gave D25 the condition `findings.rs::deadlocks` reads: only an *open* value (one no
  `@end` names, with an arm to another value, `has_way_on`) can deadlock. As first written,
  D25 reported any reachable stuck state whose value was not an end state, which named as
  deadlocks the values whose only exit is a delete or a self-loop; the code never did.
- **2026-11-28, `.present()` (P64).** D21 gained the goal `pres_b ∧ goal` of
  `eventually(x).present()` and `leads_to(x, y).present()`, the owner's decision on P6: the
  reading that a delete meets a goal its base values satisfy stays the default, and the
  modifier opts out of it per property. No other definition changed.

- **2026-11-28, after the owner's review (P62, P63).** **D5** stated the two readings of
  null but gave no table, and said nothing of `@derived`; it now points to the reference's
  table, which a test checks against the evaluator, to the lint warning
  `negated-order-admits-null`, and to the three-valued `@derived` reading. **D9** said the
  info `DeleteLeavesChildren` reports a key the lifecycle does not execute; that was only
  `cascade`, and `set_null` and `set_default` had no diagnostic. D9 now says that the SQL
  export does execute the three actions, that `DeleteLeavesChildren` is a warning and
  `DeleteLeavesChildKeys` covers the other two, and that neither fires for a `@soft_delete`
  parent.
- **2026-11-28, after the owner's review (P60).** The first version said "the source of
  truth is the code". It defined `I_β(M)` as what `TransitionSystem::initial_states`
  computes (D4), `T_β(M)` as what `TransitionSystem::build` constructs (D13), and the
  variables and effects as the values of `VarKind`, `Domain`, and `Effect` (D2, D8). Under
  that reading a fault in the builder was part of the specification. "`T` is not
  `T_β(M)`" (P54 in native.md) was then false by definition, and Lemma U in z3.md compared
  two outputs of the build. D1 to D13 now define `T_β(M)` from the model's text, with code
  only in **Implemented by** notes. That the build implements them is claim I1. Changes
  made at the same time:
  - **D1** said `m ∈ {0, …, 255}`. `cli/src/export.rs::checked_bounds` rejects
    `--int-range-max 0`, and every CLI path goes through it, so `m ∈ {1, …, 255}`. D1 also
    records `DomainTooLarge` (#470), which makes a pair with a domain of more than 256
    values ill-formed.
  - **D13** held the scope `σ` as an unnumbered paragraph. It is now D24, so that theme (b)
    can cite it.
  - **D11** said "nothing resets the latch" without saying whether that was meant. It now
    states the design decision of event-order.md and its reasons, and points to
    `.counted(k)`.
  - **D23**, `proved`: the row said "for every `n ≥ 1` and every `m` at which the model
    builds". That was wider than z3.md's Definition 3, which leaves out some bounds (P43),
    and at `b83b9544` also `m' = 255` with a nullable Int (P40, now a build error). The row
    now cites `Sc(M, β)`. The `counterexample`, `refuted`, and `escalated` rows described
    P1 and P11 as open; both were fixed in #477.
  - **Pin** moved from `b83b9544` to `670eec79`. In between, #477, #478, #485, and #470
    changed code that D8, D22, and D23 cite. The changes were read for D8 (#485: one write
    per slot), D1 (#470), and D22 (#478). The #477 parts of D23 are marked unread@pin.
