# Semantics and notation

<!-- derived-from ../state-derivation.md#the-transition-system -->
<!-- derived-from ../language-reference.md#entity-state-constraints -->

This document fixes the meaning of an rdra-ish model as a finite transition system, the
meaning of its rules and properties on that system, and the meaning of every verdict the
tools print. The theme documents listed in the [index](./README.md) state their guarantees
in this notation, and the [claims inventory](./claims.md) refers to its definitions by
number (D1, D2, ...).

The source of truth is the code at commit `b83b9544ccdef3701bf90383cd56d10ea2297787`
(origin/main, 2026-11-28). Code is cited either by Rust path (`lifecycle::types::Bounds`)
or as `file.rs::symbol`, with file paths relative to `crates/rdra-ish-core/src/lifecycle/`
unless they start with `cli/` (`crates/rdra-ish-cli/`), as in the [claims inventory](./claims.md). The prose references [state-derivation.md](../state-derivation.md),
[language-reference.md](../language-reference.md), and
[formal-verification.md](../formal-verification.md) describe the same system for modelers.
Where they and the code disagree, this document follows the code and lists the
disagreement in [Prose that the code contradicts](#prose-that-the-code-contradicts).

---

## Notation

<!-- derived-from ../state-derivation.md#variables -->

- `Ent` is the finite set of entities of the model that the lifecycle holds. An entity
  published with `exposes(System, E).append()` or `.upsert()` is not in `Ent`.
- For a set `X`, `X⊥` is `X ∪ {⊥}`, where `⊥` is the value `null`.
- `[k] = {0, …, k-1}`. Slot `i` of entity `E` prints as `E#(i+1)`.
- A predicate `φ` with free *binders* `x₁ … x_k` over entities `E₁ … E_k` is evaluated
  under a *binding* `b ∈ [n_{E₁}] × … × [n_{E_k}]`; `φ_b(s)` is its value in state `s`.
- `s →_a s'` is a step of action `a`; `s → s'` is a step of some action. `Reach(T)` is the
  set of states reachable from the initial states of `T`.

## Bounds

<!-- derived-from ../state-derivation.md#bounds -->

**D1 (Bounds).** A bound is `β = (n, m)` with `n : Ent → {1, …, 255}` the slots per entity
and `m ∈ {0, …, 255}` the top of the integer range. It is `lifecycle::types::Bounds`:
`instances` is the uniform count, `per_entity` overrides it per entity
(`Bounds::of`), and `int_max` is `m`. Every result the tools print names `β` on its
`bounds:` line. Defaults: `n ≡ 1`, `m = 5` for `states` and `check` (`Bounds::STATES`);
`n ≡ 2`, `m = 5` for `verify` and `export --kind tla` (`Bounds::VERIFY`).

The z3 encoding of `verify --prove` is the one engine that does not read `n`; it reads
neither `n` nor, for most Int terms, `m` (theme (b)).

## State variables

<!-- derived-from ../state-derivation.md#variables -->

**D2 (Variables and domains).** `T_β(M)` has one variable per row of the table below, held
per slot of its entity, so a variable `v` of entity `E` is `n_E` state components
`v[0] … v[n_E - 1]`. `lifecycle::types::VarKind` names the kinds, `Domain` the domains,
and `Domain::size` their sizes. `V` is the set of variables a given system holds.

| Kind (`VarKind`) | Domain (`Domain`) | Present in `V` when |
|---|---|---|
| `Exists(E)` | `{absent, present, deleted}` | always |
| `Column` of type Enum | the declared values, `⊥` first under `@null` (`NullableEnum`) | always |
| `Column` of type Bool | `{false, true}`, `{⊥, false, true}` under `@null` | always |
| `Column` of another type, `@null` | `{⊥, present}` (`Nullable`) | always |
| `Column` of type Int, Money, Decimal, Date, DateTime | `[m+1]`, or `[m+1]⊥` under `@null` | some guard, effect, rule, or property reads or writes it |
| `Column` typed by a `variation` | the variation's values | something names it |
| `Prop` (a driven comparison) | `{false, true}` | `drives(UC, E.c op r, bool)` names it |
| `Owner` of a foreign key `Child → Parent` | `[n_Parent]`, plus `⊥` for `.optional()` | the entity is the child of an N:1, 1:N or 1:1 `relate` |
| `Run` mark `E._run_UC` | `{none, 1, …, k-1}` | `UC` splits into `k ≥ 2` transaction groups and a step before the last writes `E` |
| `Raised` latch `E._raised_Ev` | `{false, true}` | `E` is a mark entity of a tracked event `Ev` |
| `Raised` counter `E._pending_Ev_UC` | `{0, …, k, over}` | `triggers(Ev, UC).counted(k)` |
| `Now` (one global component) | `[m+1]` | some predicate compares a column with `now` |

A state is an assignment `s` of a domain value to every component. Its byte layout
(`TransitionSystem::offsets`, `width`) is an implementation detail. A String column with
no `@null`, a `@derived` column, a foreign-key column, and a published entity have no
variable; a predicate that reads a Bool `@derived` column reads its defining expression
(`state-derivation.md`, "Variables").

**D3 (Base value).** Each variable has a base value `base(v)` (`VarDecl::base`): the
column's `@default`, else `⊥` for a `@null` column, else the first Enum value, `false`,
or `0`; `absent` for `Exists`; `none`, `false`, or `0` for marks; `0` for `now`. A slot
that is `absent` or `deleted` holds the base value in every column (`Effect::Delete`).

## Initial states

**D4 (Initial states).** `I_β(M)` is computed by `TransitionSystem::initial_states`
(`explore/moves.rs`). Every component starts at its base value, except:

1. Every slot of a *pre-existing* entity (`Birth::PreExisting`: no use case creates it) is
   `present`.
2. On a pre-existing slot, a variable with `Start::Any` takes every value of its domain
   independently: a variation column, and the owner of a pre-existing N:1 child (which
   includes `⊥` when the key is optional). `I` is the product of these choices.
3. On a pre-existing slot, a variable with `Start::Identity` holds the slot's own index:
   the owner of a 1:1 child or of a child of a `1..*:1` key.

No rule is assumed or checked to hold in `I` before exploration: an initial state is a
reachable state like any other, so a rule an initial state breaks is violated with a trace
of zero steps.

## Predicates

<!-- derived-from ../language-reference.md#null -->

**D5 (Predicate evaluation).** A predicate is a `lifecycle::types::Pred` and is evaluated
by `TransitionSystem::eval` (`expr.rs`) in two-valued logic: every predicate is
true or false in every state, and `Not`, `And`, `Or` are Boolean.

- `Forall` / `Exists` over entity `E` range over the slots of `E` that are `present`;
  `Count{E, φ, op, k}` compares `|{i ∈ [n_E] : s(Exists(E))[i] = present ∧ φ(i)}|` with
  `k` by `op`.
- `Cmp(t₁, op, t₂)` is decided by `expr.rs::compare`. When either side is numeric (an
  Int-typed variable, an integer literal, or `now`), both sides are read as integers and
  `⊥` as "no number": `⊥` is `==` only to `⊥`, `!=` to every number, and never ordered,
  so `x < 3`, `x >= 1`, and `x == 3` are false and `x != 3` is true when `x = ⊥`.
  Otherwise the two sides compare as domain indices. An owner compared with a slot index
  is "linked to that slot", and an unlinked owner (`⊥`) is linked to none.
- A rule's `==` or `!=` between two columns is not one `Cmp`: the builder adds a
  "present" guard on each nullable operand (`build/preds/mod.rs::columns_cmp`), so two `⊥`
  columns are not equal there, as in SQL, while `col == null` is a term equality that
  holds on `⊥`.
- `In(t, vs)` is the disjunction of `t == v` over `vs`. A set that lists every value of an
  Enum, Bool, null-or-present, or existence column is folded to `true` when the system is
  built (#71, #73); an Int set is never folded (#425).

A consequence the reader must keep in mind: `not (x < 3)` is true when `x = ⊥`, so `not`
does not commute with the SQL reading of a comparison.

## Actions

<!-- derived-from ../state-derivation.md#actions -->

**D6 (Action).** Each use case with at least one write becomes one action per transaction
group (`lifecycle::types::Action`, `ActionOrigin::UseCase`); a use case with no write has
none. An action `a` has

- parameters `p₁ … p_r`, each an entity and a role (`ParamRole`),
- guards `G_a`, a conjunction of predicates over the parameters (`Action::guards`), each
  read on the pre-state, or on the pre-state and the successor when it reads `At::Next`,
- effects `F_a` (`Action::effects`), applied simultaneously to the pre-state.

**D7 (Binding).** A binding of `a` in `s` maps each parameter to a slot, as
`explore/moves.rs::candidate_slots` and the guards admit it:

| Role | Admissible slot |
|---|---|
| `New` | the lowest slot whose `Exists` is `absent` (none if there is none) |
| `Existing` | any `present` slot |
| `Parent{key}` | any `present` parent slot; a guard links it to the child through `key` |
| `Earlier{step}` | the slot whose run mark is `step`, present or not |
| `Triggered` | a `present` slot of a mark entity whose latch or counter admits the handler |

The choice of the lowest absent slot for `New` is a canonical choice: slots are created in
index order, and a deleted slot is never reused (`Exists` never leaves `deleted`).

**D8 (Effects).** Applied to the pre-state `s` under binding `p`:

| Effect | Post-state |
|---|---|
| `Create{p}` | `Exists[p] := present`; the columns of `p` are already at base |
| `Delete{p}` | `Exists[p] := deleted`; every column of `p` returns to base; owners that name `p` keep naming it |
| `Assign{v, p, Lit(x)}` | `v[p] := x` |
| `Assign{v, p, Param(q)}` | `v[p] := slot of q` (owner assignment: link, relink, created child) |
| `Transition{v, p, arms}` | for each arm `x → y` with `s(v)[p] = x`, one successor with `v[p] := y` |
| `Choose{v, p, values}` | one successor per listed value |
| `Tick` | `now := now + 1`, enabled only while `now < m` |

A transition or choice forks the post-state after every other effect, and two forks
multiply. Arm and value guards (`GuardWhy::ArmCondition`) filter the forked successors. A
variable no effect writes is unchanged. The set of successors of `s` under `(a, p)` is
`Post_a(s, p)`; it is empty when a pre-state guard fails or every successor is filtered.

**D9 (Delete guards).** For each key into the deleted entity whose `.on_delete` is unset,
`restrict`, or `no_action`, the delete is guarded by "no present child names the slot,
apart from a child the same step deletes, unlinks, or relinks" (`GuardWhy::Restrict`). A
`cascade`, `set_null`, or `set_default` key is not executed: its children stay present
and name a deleted parent.

**D10 (Split use cases and runs).** A use case whose writes form `k ≥ 2` transaction groups
(`UsecaseTx::groups`) is the actions `UC_1 … UC_k`. On every entity a step before the last
writes, the run mark `E._run_UC` records the last finished step of a pending run:

- `UC_1` binds only rows whose mark is `none` and sets it to `1`;
- `UC_i` (`i > 1`) binds each such row as `Earlier{i-1}` and sets the mark to `i`, or to
  `none` at `i = k`;
- any action may run between two steps, since each step is its own transaction.

A run never aborts: a step whose guard fails waits forever, which shows as a deadlock or a
use case that never runs. A delete does not reset the mark, so a run finishes on a row an
earlier step deleted.

**D11 (Triggers).** For a tracked event `Ev` (every `raises` of it names a use case with an
action) the last step of every raiser sets `E._raised_Ev := true` on each parameter on a
mark entity `E`, and never waits; nothing resets the latch. The first step of a handler
`triggers(Ev, UC)` is guarded by the latch on its row of `E` (`GuardWhy::Raised`), or binds a
`Triggered` parameter when it has no such row. For `.counted(k)` the counter replaces the
latch on that edge: a raise moves `i` to `i+1` and `k` to `over`, the handler needs a
counter other than `0` and moves `i` to `i-1`, and `over` stays. The state rule
`EventOverflow_<E>_<Ev>_<UC>` (`InvKind::Overflow`) says no present row is at `over`. A free
event (an external raiser, a timer, a raiser with no action) has no mark and orders nothing.

**D12 (Clock).** When some predicate reads `now`, the action `Tick` (`ActionOrigin::Clock`)
increments `now` until `m`. A Date or DateTime column is a point on that clock.

## The transition system

**D13 (Transition system).** `T_β(M) = (S, I, →)` where `S` is the set of assignments of
D2, `I = I_β(M)` (D4), and `s → s'` iff for some action `a` and binding `p` admissible in
`s`, `s' ∈ Post_a(s, p)`. `S` is finite for every `β`. A *step* is a triple `(s, (a, p),
s')`; a step with `s' = s` is possible (an action that rewrites a value with itself) and
is treated as stuttering by every liveness check (`explore/liveness.rs::fair_steps` keeps
only edges with `from ≠ to`).

`TransitionSystem::build` constructs `T_β(M)` once. BFS (`check`, `states`, `verify`), the
TLA+ printer (`export --kind tla`, `verify --with-tlc`), and the z3 encoding
(`verify --prove`) all read this one value; none of them reads the model a second time.

**Scope.** `TransitionSystem::build` also takes a `select.rs::Scope`. `--buc` keeps only the
use cases reachable from the named BUCs, and `--without-usecase`, `--without-buc`, and
`--actor` leave use cases out; the result is `T_β(M)` with those actions removed, written
`T_β(M, σ)` for a scope `σ`. `--entity` removes no action; it selects which checks run. Every
verdict of a scoped run is a verdict of `T_β(M, σ)`, and the `OK:` line says
`in the selected scope`. The definitions below are stated for `T_β(M)`; they apply to
`T_β(M, σ)` unchanged.

**D14 (Behaviors).** A *finite trace* is `s₀ → s₁ → … → s_k` with `s₀ ∈ I`. A *behavior* is
an infinite sequence `σ = s₀ s₁ …` with `s₀ ∈ I` and, for each `i`, `s_i → s_{i+1}` or
`s_{i+1} = s_i` (a stuttering step), as in TLA+. A behavior *stops* at `s` when it
stutters at `s` forever from some point on. A state with no successor other than itself is
*terminal*; a behavior may stop there under every fairness assumption below.

**D15 (Cone).** For a set of variables `W ⊆ V` closed under the reads of the actions that
write it (`cone.rs`), `T|W` is the system restricted to `W`: actions that write
nothing in `W` are dropped, the others keep their effects on `W`, and states are projected
to `W` (`TransitionSystem::restrict_to`). Theme (a) defines the closure and states when
`Reach(T|W) = π_W(Reach(T))` holds and when only `⊇` holds. This document needs only the
definition, because every verdict below is defined on `T_β(M)` itself.

## State rules

<!-- derived-from ../state-derivation.md#checks -->

**D16 (State rule).** A state rule is an `lifecycle::types::Invariant`: binders
`E₁ … E_k` and a body `φ`. It holds in `s` iff `φ_b(s)` for every binding `b` of
present slots (`expr.rs::find_violation` finds none). `T` satisfies it iff it holds in
every `s ∈ Reach(T)`. Every rule form compiles to one: `forbidden`, `invariant`,
`required`, `exclusive` (local and multi-entity, `.along` adds owner equalities to the
body's guard), quantifier rules, `Unique_*`, `MinCard_*`, `MaxCard_*`, `Check_*`,
`DanglingKey_*`, and `EventOverflow_*` (`InvKind`). A rule over an entity with no present
slot holds.

The bodies of the four written forms, with conditions `c₁ … c_r` read by D5 and every
binder ranging over present slots (`language-reference.md`, "Entity state constraints"):

| Form | Body `φ` | Name |
|---|---|---|
| `forbidden(E…, c₁, …, c_r)` | `¬(c₁ ∧ … ∧ c_r)` | `Forbidden_<E>_<n>`, `MultiForbidden_<E>_<n>` |
| `invariant(E…).when(g₁…).then(r₁…)` | `(g₁ ∧ …) ⇒ (r₁ ∧ …)` | `Inv_<E>_<n>`, `MultiInv_<E>_<n>` |
| `required(E, c₁, …, c_r)` | `c₁ ∧ … ∧ c_r` | `Required_<E>_<n>` |
| `exclusive(E, c₁, …, c_r)` | `∧_{i<j} ¬(c_i ∧ c_j)` | `Exclusive_<E>_<n>` |
| `invariant(E).when(g…).has(R.c…)` / `.none(…)` / `.at_least(k, …)` | `g ⇒ ∃r. link(e, r) ∧ c` / `g ⇒ ¬∃r. …` / `g ⇒ Count ≥ k` | `Quantifier_<E>_<n>` |

`.along(A, B, …)` conjoins the owner equalities of the path to the guard side, so the rule
speaks only of linked rows; a `.when(Cond)` adds the comparisons of `Cond`'s `holds` body.

**D17 (After assertion).** `after(UC).assert(ψ)` (`AfterAssert`) holds iff for every
`s ∈ Reach(T)` and every step `s →_a s'` of the *last* step `a` of `UC` (of every variant
`UC_by_Ev`), `ψ` holds in `s'` with the action's parameters bound to their slots and the
scope entities the action does not bind universally quantified over present slots of
`s'` (`AfterStep::post`), and `s' ≠ s`. States between two steps of a run are not checked,
and neither is a last step that changes nothing: the native run skips the check on a
self-loop (`explore/bfs.rs`, `Probe::Loop`, in every build), and TLC checks the `PROPERTY`
`[][(\A p : UC(p) => post')]_vars`, which every stuttering step satisfies. The language
reference promises a check after every run (S6, open problem P22).

**D18 (Vacuity).** A rule *holds vacuously* when no reachable state meets one of its
premises (`Invariant::premises`, `state-derivation.md`, "Rules that hold vacuously").
Vacuity is a warning about a rule that holds; it never changes a verdict.

## Temporal properties

<!-- derived-from ../language-reference.md#temporal-path-properties -->
<!-- derived-from ../language-reference.md#order-and-stability-stable-unchanged-precedes -->

A property (`lifecycle::types::Property`) has binders `E₁ … E_k`, a link predicate `ℓ`
(the owner equalities of `.along`, `true` without it), a formula (`Ltl`), and a set of fair
use cases. For a binding `b` define the *trigger*

```
trig_b(s)  ≝  (∀j. s(Exists(E_j))[b_j] = present)  ∧  ℓ_b(s)
```

(`lasso.rs::TransitionSystem::triggers`). `pres_b(s)` is the first conjunct
alone. Operands are evaluated by D5 on the slot whatever its existence: on an absent or
deleted slot they read base values. The property holds iff it holds for every binding.

**D19 (Safety forms).** On `T`:

| Form | Holds iff | Code |
|---|---|---|
| `always(x)` | `∀s ∈ Reach(T). trig_b(s) ⇒ x_b(s)` | `ComponentRun::unsafe_state` |
| `stable(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ p_b(s')` | `TransitionSystem::step_breaks` |
| `unchanged(c₁…c_r).when(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ ∀j. c_j(s') = c_j(s)` | `TransitionSystem::step_breaks` |
| `precedes(p, q)` | for every finite trace `s₀ … s_k`: `trig_b(s_k) ∧ q_b(s_k) ⇒ ∃j ≤ k. trig_b(s_j) ∧ p_b(s_j)` | `ComponentRun::unlatched_state` |

The link of `stable` and `unchanged` is read before the step only. Steps with `s' = s`
break neither form. `precedes` is the safety property "`q` not before `p`" per binding; its
TLA+ form is a latch `P_seen` per binding.

**D20 (Fairness).** For a property `P` with `.fair(UC₁, …)`, a *fair instance* is a pair
`(a, κ)` of an action `a` of some `UC_i` (every step of a split use case) and the slots
`κ` its non-`New` parameters bind (`Property::fair_instance`,
`ParamRole::keys_fair_instance`). `(a, κ)` is *enabled* at `s` when some binding of `a` that
extends `κ` has a successor `s' ≠ s`, and *taken* by a step `s → s'` with `s' ≠ s` whose
action is `a` and whose binding extends `κ`: a move that changes no variable neither
enables nor takes an instance, as `⟨A⟩_vars` reads it in TLA+ and
`lasso.rs::TransitionSystem::unfair_loop` reads the moves (open problem P56). So a behavior
that stays at one state forever through a self-loop of a fair instance does not take it. Two fairness conditions on a behavior `σ` are used:

- `Fair_P`: for every fair instance, if it is enabled at every state of `σ` from some
  point on, it is taken infinitely often. This is weak fairness per instance,
  `∧_{(a,κ)} WF_vars(∃ new slots : a(κ, …))` in the TLA+ output.
- `WF_vars(Next)`: if from some point on some step to another state is enabled at every
  state, infinitely many steps change the state.

There is no strong fairness: no `SF` form exists in the language, the TLA+ printer, or
`lasso.rs::Fairness`, whose two values are `Plain` (`Fair_P` alone) and
`WfNext` (`Fair_P ∧ WF_vars(Next)`). A property with no `.fair` has `Fair_P = true`.

**D21 (Liveness forms).** Let `goal` and `cause` be: `eventually(x)`: `cause = true`,
`goal = x`; `leads_to(x, y)`: `cause = x`, `goal = y`. Under a fairness condition `F`, the
property holds on `T` iff every behavior `σ` of `T` that satisfies `F`, every binding `b`,
and every `i` with `trig_b(σ_i) ∧ cause_b(σ_i)` have some `j ≥ i` with `goal_b(σ_j)`. So
`eventually(x)` is `□(trig ⇒ ◇x)` per binding, not `◇x`, and `goal` is read at `j = i`
too. `goal_b(σ_j)` does not require `pres_b(σ_j)`: on a slot deleted between `i` and `j`
it reads the base values D8 leaves, so a delete meets a goal those values satisfy and no
other goal. This is the defined semantics (open problem P6, decided as documented):
language-reference.md, "Temporal path properties", states it, and the TLA+ formulas
`[](present => <>x)` and `(present /\ x) ~> y` read it the same way.

The verdict a property is meant to have is the one under `WfNext`, because the exported
`Spec` conjoins `WF_vars(Next)` whenever the model has an `eventually` or `leads_to`, and
`P` is printed as `Fair_P ⇒ (…)`. `Plain` is a stronger requirement (more behaviors), so
`ok` under `Plain` implies `ok` under `WfNext`, and a counterexample under `WfNext` is one
under `Plain`. `formal-verification.md`, "Temporal properties on a cone", uses exactly these
two implications to order its three runs; theme (c) states how the native checker decides
each run (`ComponentRun::decide_liveness`).

**D22 (Recoverability).** `can_reach(g)` (`lifecycle::types::Reach`) holds iff

```
∀s ∈ Reach(T). trig_b(s) ⇒ ∃ s = t₀ → t₁ → … → t_r.  (∀j. pres_b(t_j)) ∧ g_b(t_r)
```

that is, CTL `AG (trig ⇒ E[pres U (pres ∧ g)])` per binding, decided on the explored graph
by one backward pass per binding (`reach.rs::reach_failure`). The link is read
in the trigger only. No fairness applies; an instance may also loop forever.

## Verdicts

<!-- derived-from ../formal-verification.md#what-verify-checks -->

A verdict is a statement about `T_β(M)` (or, for `proved`, about `T_β(M)` for every `n`) and
one property or rule. Below, "within `β`" means "of `T_β(M)`". Every word the tools print
for a check is one of these.

**D23 (Verdict vocabulary).**

| Printed | Where | Meaning |
|---|---|---|
| `ok` | a cone or property line of `verify` (`cli/src/tlc_output.rs::Outcome::Ok`) | The checks of the line hold on `T_β(M)` (D16, D17, D19, D21 under the fairness of D21, D22). For a line of a cone it is derived from the cone; theme (a) owns the step from cone to model. |
| `ok (checked in Cone_X)` | a covered safety cone (`cli/src/verify/bfs.rs`) | `ok`, decided by the run of a cone `Cone_X` that holds every variable, action, guard, and parameter of this one. |
| `counterexample` | a line, or `result:` (`RunResult::Counterexample`) | A finite trace (safety) or a lasso (liveness) of the explored system that breaks a check. It is a behavior of `T_β(M)` except where theme (a) says otherwise: a cone that frees slots may need more instances of an entity it does not read (re-decided on the whole model for properties), and a cone that leaves a raised mark out may show a run the event order forbids at every bound (open problem P1). A liveness lasso on an unreduced cone replays through the step relation before it prints; other traces are re-evaluated or lifted without a full replay (open problem P20). |
| `violated` | `property P (BFS on its cone): violated`; `can_reach (BFS on N cones): k checked, m violated` (`cli/src/verify/bfs.rs`) | `counterexample`, in the lines of the forms BFS alone decides (`stable`, `unchanged`, `precedes`, `can_reach`). |
| `not decided` | a line (`cli/src/verify/native.rs::Answer::Stopped`, `RunResult::NotDecided`) | The run stopped before it found a counterexample. No claim is made either way. The line's note names the stop: `stopped at the state cap (--max-states) or the memory limit`, `stopped at the state cap of N states`, `stopped at the memory limit of L`, `stopped at the time budget of S s`, or (for a property its cone cannot decide) `needs the whole model's WF_vars(Next)` and `its counterexample may create instances past the bound`. |
| `tool_error` | a line or `result:` | A tool fault, not a fact about the model: a lasso that does not certify itself (`LassoFault`), a disagreement between the native checker and TLC, or a TLC failure without a violation. |
| `result: ok` | last lines | Every check of the run is `ok`, no tool error, nothing undecided, and (under `--deny-unproved`) nothing unproved (`cli/src/verify/end.rs::End::result`). |
| `result: not decided` | last lines | No failure and no tool error, but some finding a cap or budget left open, or an unproved rule under `--deny-unproved`. Exit 0 unless `--deny-undecided` (or `--deny-unproved`) is given; the `OK:` line then holds only for what was decided. |
| `MemoryCapReached_*`, `StateCapReached_*`, `PatternCapReached_*` | `check`, `states` | An error: the cone stopped, so its rules are not checked; `check` does not report it as a pass. |
| `proved: R` | `verify --prove` (`prove/mod.rs::Verdict::Proved`) | `R` belongs to a set of formulas inductive for the z3 encoding (D13 widened: any absent row for `New`, Int terms and `now` over all non-negative integers). Hence, if z3 is sound, `T_{(n,m)}(M) ⊨ R` for every `n ≥ 1` and every `m` at which the model builds (claim B2). Under a scope the encoded system is the scoped one (open problem P4). |
| `unproved: R: not inductive; …` / `unproved: R: z3 gave up on A` / `unproved: R: an initial state violates it` | `verify --prove` (`Verdict::Unproved`, printed by `cli/src/prove.rs`) | Not proved: a counterexample to induction exists from a state where `R` and every kept candidate hold, not necessarily reachable (`NotInductive`); z3 answered `unknown` or ran out of its resource limit on the step of action `A` (`Unknown`; this is the verdict the task list calls "unknown"); or an initial state of the encoding, possibly with more rows or larger Int values than `β`, breaks `R` (`Initial`). Says nothing about the bounded verdict. |
| `out of scope: R: why` | `verify --prove` (`Verdict::OutOfScope`) | The encoding cannot state `R`; no claim. |
| `refuted: R (BFS counterexample at k instances)` | `verify --prove` (`Verdict::Refuted`) | A safety cone of the same run reached a state that breaks `R`, so the prover did not try it (`cli/src/verify/refuted.rs`). The violation is BFS's error, not an unproved rule. It is a reachable violation of `T` only when that cone is exact (open problem P1). |
| `escalated: R: ok at k instances` / `counterexample at k instances` / `not decided at k instances` | `verify --prove` (`cli/src/verify/escalate.rs`) | For a rule BFS passed and the prover left unproved, BFS on its cone at one and two more instances per entity. `counterexample` there fails the run; it is a violation of `T_{β'}(M)` for some larger `β'` under the conditions of `counterexample` above, and `k` can understate `β'` (open problem P11). |
| `decided` | prose, design notes | Not printed as a verdict. A check is decided when its verdict is `ok` or `counterexample`. |

`stopped at the memory limit` and `stopped at the state cap` describe why a run is
`not decided`; they are not separate verdicts.

---

## Prose that the code contradicts

Each row is a place where the modeler-facing prose says something the code does not do. The
code is what this document defines. None of these is a soundness problem by itself; the
soundness questions are in the index's [Open problems](./README.md#open-problems).

| # | Prose | Code | Note |
|---|---|---|---|
| S1 | `language-reference.md`, "Temporal path properties": `eventually(expr)` is `<>expr`. | `lasso.rs::fails_at` and `explore/liveness.rs::liveness_search`: `□(present ∧ link ⇒ ◇x)` per binding. | `formal-verification.md`, "Mapping", states the code's form. Resolved with P6: the language reference now gives `[](present => <>expr)` and `(present /\ p) ~> q`. |
| S2 | `language-reference.md`, "Temporal path properties" and "Recoverability": `can_reach` is `AG (present => EF expr)`. | `reach.rs::reach_failure`: `AG (trig ⇒ E[pres U (pres ∧ goal)])`; the path must keep the instance present. | `formal-verification.md`, "Recoverability on a cone", and `types.rs::Reach` state the code's form. The two differ when the goal holds on a deleted slot's base values. Resolved with P6: the language reference now gives the code's form. The doc comments of `model/constraints.rs::Ltl` and `syntax/ast.rs`, and the comment the TLA+ module prints, still say `AG (present => EF …)`. |
| S6 | `language-reference.md`, "Entity state constraints": `after(UC).assert(…)` "checks the state each run of the use case leaves" and "must hold after every arm". | D17: a step with `s' = s` is not checked. The native run skips the `after` check on a self-loop (`explore/bfs.rs`, `Probe::Loop`, in every build), and the TLA+ `PROPERTY` `[][(\A p : UC(p) => post')]_vars` admits a stuttering step. | Open problem P22: the two engines agree, so no verdict differs between them; a run whose last step changes nothing goes unchecked although the reference promises a check. |
| S3 | `formal-verification.md`, "Mapping": `always` is `[](present => x)`. | The trigger is `present ∧ link`; without `.along` the link is `true`. | Consistent once `.along` is read into `present`, as the "Scope" table of `language-reference.md` does. |
| S4 | `state-derivation.md`, "Bounds", and `language-reference.md`: Int columns range over `0..5`. | `Domain::Int` is `[m+1]` with `m = int_max`; `--int-range-max` sets `m` and 5 is only the default. | The language reference mentions `--int-range-max`; listed so that theme writers quote `m`, not 5. |
| S5 | `prove/mod.rs::Verdict::Proved` doc comment: "Holds for any number of rows, at the stated value bounds"; `cli/src/prove.rs::ProofTally::clause`: "any Int value". | `prove/encode.rs` (module doc, `Enc::in_domain`): Int terms and `now` range over every non-negative integer with no top, and a rule names only literals up to `m`. | formal-verification.md ("any non-negative Int value") matches the code. Open problem P2. |

## Remarks on the definitions

- **Operands on absent rows.** D21 and D22 read `goal` on a slot whatever its existence, and
  D8 resets a deleted slot's columns to base. So `eventually(Order.status == draft)` with
  `draft` the base value is met by deleting the order. TLC reads the TLA+ formula the same
  way (`[](present => <>x)`), so the engines agree. Open problem P6 decided it as
  documented: language-reference.md, "Temporal path properties", now says so, with an
  example each way and how to state a goal a delete does not meet.
- **Initial states are not assumed to satisfy anything.** D4 makes the initial set a product,
  including N:1 owners of pre-existing children under a created parent that is still
  absent (`PreExistingChildOfCreatedParent`).
- **Lowest-slot creation is a symmetry choice.** D7 creates in index order. The TLA+ module
  does the same (`FirstAbsent`); the z3 encoding lets a creation take any absent row
  (`formal-verification.md`, "Proofs for any number of rows"), which only adds steps.
