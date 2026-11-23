# Counting rules and per-entity instance counts

<!-- constrained-by ../vision.md -->
<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../language-reference.md#conditions -->
<!-- derived-from ../language-reference.md#cardinality-ranges -->
<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->
<!-- derived-from ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->

Status: proposed for [#216](https://github.com/manji-0/rdra-ish-dsl/issues/216). The owner
agreed to per-entity instance counts as its prerequisite. The note shapes `Bounds` so
[#205](https://github.com/manji-0/rdra-ish-dsl/issues/205) can derive per-parent child counts
from the same place; it does not implement #205.

Each section states the choice, then the alternative it rejects. The choices the owner
should review are collected in [Decisions to review](#decisions-to-review).

## The problem

`.has(...)` says "some related row" and `.none(...)` says "no related row". A cardinality
range ends at `1` or `*`. Rules that count cannot be written: a member holds at most 3 open
loans, a claim needs at least 2 estimates. A modeler falls back to a counter column that no
rule ties to the rows it counts.

Counting needs bounds that can show a count. With one global instance count, `at_most(3)`
cannot fail below 4 instances, and raising the count for every entity multiplies every
cone. So the work comes in two parts:

1. Per-entity instance counts: `--instances Loan=4`.
2. Counted quantifiers and a high end above 1 in cardinality ranges.

## Per-entity instance counts

### CLI and `rdra.toml`

```sh
rdra-ish verify model/ --instances Loan=4
rdra-ish verify model/ --instance-count 2 --instances Loan=4 --instances Member=1
```

`--instances <Entity>=<n>` is repeatable, with `n` from 1 to 255. It sets the slot count
of one entity, and `--instance-count` keeps setting every other entity's. `check`,
`states`, `verify`, and `export --kind tla` take it. An id that names no entity is an
error, which lists the entity ids.

```toml
[verify]
memory = "16GiB"
instances = { Loan = 4 }

[check]
instances = { Loan = 2 }
```

`instances` under `[check]` applies to `check` and `states`. Under `[verify]` it applies to
`verify` and to `export --kind tla`, which writes the modules `verify` checks. A flag
replaces the table's entry for the same entity and keeps the others. The generators
(`codegen`, `next`, and the outputs that read explored states) run at `Bounds::STATES`
and read neither table, so a verification setting never changes generated output. This
differs from `memory`, which reaches the generators through `[check]`: memory changes
whether a run finishes, not what it explores.

Rejected:

- **`--instance-count Loan=4`, one flag for both forms.** The flag is a single number with
  a default today. A value that is a number or a list of pairs is harder to read in help
  text and in scripts.
- **A count in the model, `entity Loan @instances(4)`.** A bound is an input to
  verification, not a fact of the domain. The model would carry a number that only
  `verify` reads, and two runs at different bounds would need two models.

### `Bounds` and the transition system

```rust
pub struct Bounds {
    /// Slots of every entity without an entry in `per_entity`.
    pub instances: NonZeroU8,
    pub int_max: u8,
    /// Slots by entity id: `--instances` and the `instances` tables.
    pub per_entity: BTreeMap<String, NonZeroU8>,
}

pub struct EntityDecl {
    // ...
    /// This entity's slot count under the system's bounds.
    pub slots: NonZeroU8,
}

impl TransitionSystem {
    pub fn slots(&self, e: EntId) -> usize;
}
```

`Bounds` keeps the user's input by entity id, so it is no longer `Copy` (it is `Clone`).
The build resolves each entity's count into its `EntityDecl`. `restrict_to` clones the
declaration, so a cone and its sub-lifecycles keep the counts while entity ids are
renumbered. `Bounds::slots()` is removed, so the compiler lists every site that assumed one
count. Each site reads `ts.slots(e)` for the entity it ranges over.

#205 then derives a per-parent child's count in the same build step that resolves
`per_entity`, and writes it into the same `EntityDecl::slots`. Nothing downstream changes.

The sites that are not a rename:

- `Domain::size` takes the system, not the bounds. An owner var's domain is its parent's
  slots, so its size is the parent's count.
- `Start::Identity` links a pre-existing child's slot `i` to parent slot `i`. With unequal
  counts that is not a permutation, so the build reports `IdentityStartSlots`, an error,
  when a pre-existing child that starts this way (a 1:1 key or a `1..` range under a
  pre-existing parent) has a count different from its parent's.
- A param fold of the reduction binds each free T param over its own entity's slots, not
  `slots ^ params`.
- The premise check (`meets`) and violation search step each binder over its own count.
- `Caps::for_instances` takes the largest count of the system.

Rejected: **one state layout at the largest count, with slots past an entity's count kept
absent.** It leaves fewer sites to change but more to get wrong. A pre-existing entity's
extra slots would have to start absent. A scan for "some slot absent" would always find
one. An owner of a pre-existing N:1 child (`Start::Any`) could name a parent slot that
never exists. TLC, which would see `E_Ids == 1..k`, would explore a different state space.

### Reports and TLA+

With no `--instances` and no table, every output is byte-identical to today's.

- The bounds line reads `instances=2 (Loan=4) int_range=0..5`.
- The TLA+ header reads `\* Bounds: InstanceCount=2 Loan=4 IntRange=0..5; ...`, and the
  entity's set is `Loan_Ids == 1..4`. The other sets stay `1..InstanceCount`.

## Counted quantifiers

<!-- blocked-by #per-entity-instance-counts -->

### Syntax

```rdra
invariant(Member).when(status == active).at_most(3, Loan.status == open).along(Member, Card, Loan)
invariant(Claim).when(status == assessing).at_least(2, Estimate)
holds(RoomFree, Room.open == true).at_most(1, Booking.status == confirmed)
applies_to(RoomFree, Book)
```

The forms are `.at_most(n, ...)`, `.at_least(n, ...)`, and `.exactly(n, ...)`. The first
argument is an integer literal. The arguments after it are the arguments `.has` and
`.none` take: a bare entity (`Estimate`, every linked row), or conditions on the counted
entity. The forms go where `.has` and `.none` go, on an `invariant` and on a `holds` body,
with the same `.along(...)` and `.role(...)`.

Rejected:

- **A count expression, `.count(Loan.status == open) <= 3`.** It needs a new operand kind
  in the comparison grammar, and it suggests that counts compare with columns, which the
  checker cannot bound.
- **`.has(3, ...)` or `.has_at_most(3, ...)`.** The three names read as English and say
  the comparison once.

### What a count counts

A count counts **distinct present rows of the counted entity** that are linked to the
anchor's row and match the conditions. The link is the one `.has` and `.none` read: the
owner link of a direct `relate`, the path's chain under `.along`, or every row when no key
joins the two entities (with `UnjoinedGuardRead` on a guard, as for `.has`).

Under `.along(A, B, C)` a row of `C` counts once when some chain of `B` rows links it to the
anchor's `A` row. The count is not a count of chains. In the IR the counted entity is the
count's binder, and the hops between are existentials inside it:

```text
Count { entity: C, op, n, body: Exists { entity: B, body: link(A,B) /\ link(B,C) /\ conds } }
```

`.has` and `.none` nest their binders in path order with the counted entity innermost. A
count cannot reuse that nesting. With it, a `C` row reached through two `B` rows would
count twice.

### Guards count the rows before the step

A guard reads the state before the use case runs, as `.has` and `.none` do. On a use case
that creates the counted entity, the row being created is not counted. A guard that keeps
"at most 3 open loans" true across `Borrow` is therefore `.at_most(2, ...)`:

```rdra
condition UnderLimit "貸出中が2冊以下の会員だけ借りられる"
holds(UnderLimit, Member.status == active).at_most(2, Loan.status == open).along(Member, Card, Loan)
applies_to(UnderLimit, Borrow)
invariant(Member).at_most(3, Loan.status == open).along(Member, Card, Loan)
```

The language reference says this beside the forms. Rejected: **counting the created row
in a guard.** A guard would then read the successor, which a `.has` guard never does, and
`.at_most(3)` on the guard would mean something different from `.at_most(3)` on the rule.

### Edge forms

| Written | Result |
|---|---|
| `.at_least(0, ...)` | `QuantifierCountSpelling`, an error: it holds for every row. Remove it. |
| `.at_most(0, ...)`, `.exactly(0, ...)` | `QuantifierCountSpelling`: write `.none(...)`. |
| `.at_least(1, ...)` | `QuantifierCountSpelling`: write `.has(...)`. |
| `.at_most(1, ...)`, `.exactly(1, ...)` | Accepted. No other form says them. |
| no number, a non-integer, or above 255 | `QuantifierCountInvalid`, an error. |

Rejected: **accepting the respelled forms as synonyms.** The language has one spelling per
rule elsewhere (a parent-side range is `MinCardOnParentSide` with its fix), and the rule
table and violation text would print two spellings of one rule.

### Cardinality ranges above 1

The child side of a card accepts `lo..hi` with `lo` in {0, 1} and `hi` in {1, 2, .., 255,
`*`}. A `hi` of 2 or more is the many side, as `*` is, and adds the invariant
`MaxCard_<Child>_<role>`: every present parent has at most `hi` present children linked
through the key.

```rdra
relate(Loan, Member, 0..3:1)   // a member has at most 3 loans
relate(Line, Order, 1..5:1)    // MinCard_Line_Order and MaxCard_Line_Order
```

`lo` stays 0 or 1. A step creates at most one row of an entity, so a parent created with
its first child cannot have a second in the same step, and `2..*` would fail as each
parent is created. `0..0` and any other form stay `MinCardOutOfRange`. The ER, DBML, SQL,
and Mermaid outputs draw the side as many, as for `*`; no DDL states a maximum.

Rejected: **`lo` above 1.** See above. It can return with a creation that writes several
children.

### Vacuity

`at_most(n)` and `exactly(n)` can fail only in a state with `n + 1` rows of the counted
entity. Each gets the premise "`n + 1` rows of the counted entity", a count over every
present row with no link or condition. When no reachable state meets it, the rule is
`Vacuous_<rule>` (`VacuousRule`, a warning), as `Unique_*` is when no state has two rows.
`MaxCard_*` gets the same premise. When the counted entity's count is `n` or less, the
premise text names the bound and the flag that lifts it:

```text
warning[Vacuous_Quantifier_Member_0]: Quantifier_Member_0 holds vacuously: no reachable state has a row of Member with 4 rows of Loan (Loan has 2 slots; --instances Loan=4 gives it 4)
```

`at_least(n)` with fewer than `n` slots of the counted entity is the opposite: the rule
fails in every state where it applies, and its counterexample is an artifact of the
bound. The build reports `CountAboveBound`, a warning, naming the entity, `n`, and the
flag. The counterexample is still reported; the warning says why it is not the domain's.

Rejected: **reporting nothing at one instance, as `Unique_*` does.** `Unique_*` drops its
premise at one instance because the bounds line says "one instance". A count's bound is
`n + 1`, which no line states, so the finding has to.

## Engines

<!-- constrained-by #counted-quantifiers -->

### IR

```rust
pub enum CountBound { AtMost, AtLeast, Exactly }
pub enum QuantifierKind { Has, None, Count { bound: CountBound, n: u8 } }

pub enum Pred {
    // ...
    /// The present slots of `entity` for which `body` holds, compared with `n`; the body
    /// sees the slot as the next `Bound`.
    Count { entity: EntId, body: Box<Pred>, op: CmpOp, n: u8 },
}
```

The build emits `Le` (`at_most`), `Ge` (`at_least`), and `Eq` (`exactly`). Every engine
handles every `CmpOp`, so a `Not` around a count needs no special case.

Rejected: **a `CountOp` enum of three values.** It rules out three operators the build
never emits, but each engine would then map it to the comparison it already prints for
`CmpOp`.

### Native checker

`eval` counts the present slots of the entity, over that entity's own slot count, for
which the body holds, and compares the count with `n`. It counts every slot: an entity has
at most 255, and a count that stops early is one more place for an off-by-one.

### TLA+

```tla
Cardinality({b1 \in Loan_Ids : (Loan_exists[b1] = "present") /\ (...)}) <= 3
```

`FiniteSets` joins `EXTENDS` only in a module that prints a count, so other modules stay
byte-identical. `Cardinality` joins the reserved names, so no entity or var is printed as
it.

### z3

A count over an uninterpreted sort is not first order, but a count against a constant is.
"At least `k` rows" is `k` distinct present rows that each satisfy the body:

```smt
(exists ((c0 E1) (c1 E1) (c2 E1) (c3 E1))
  (and (distinct c0 c1 c2 c3)
       (let ((b1 c0)) (and present body)) ... (let ((b1 c3)) (and present body))))
```

Then `<= n` is `not (at least n + 1)`, `>= n` is `at least n`, and `= n` is both. Each is
equivalent to the count, not an approximation of it, in either polarity, so a proof stays
sound under `Not` and inside a guard. Past 8 witnesses the formula is `out of scope`, as
every construct the encoding does not express is.

Rejected: **`out of scope` for every count.** That is the issue's baseline. The unrolling is
exact and small, and an `at_most` rule kept by an `at_most` guard is the case a proof
helps most: no bound shows it for any number of rows.

### Cone and reduction

The cone of a target reads the vars a predicate reads. A count reads what an `Exists` over
the same entity reads: the entity's existence var and the vars of its body. `pred_reads`
gets a `Count` arm beside `Forall | Exists`, which inserts the existence var. `Pred::terms`
alone is not enough: `.at_most(3, Loan)` has no terms.

The reduction must not treat a count like an `Exists` when it rewrites one:

- A **node fold** (`FoldKey::Linked`) pulls an existential out of a guard into a fresh
  param bound to any slot: `∃ q. link(q) ∧ bit[q]`. That is valid because `∃` is
  upward-monotone in the matching rows. A count is not: `at_most` is downward and `exactly`
  neither. `as_node` matches only `Exists`, and `strip_links` returns `None` on a `Count`, so
  a guard with a count is never node-folded.
- A **param fold** (`FoldKey::Param`) evaluates the folded guards whole with `eval` for each
  binding of the key. A count there is evaluated exactly, so the bit is exact.
- **Monotone inputs** are monotone vars, not monotone predicates. An S action reads a
  monotone T var raw and its guard is evaluated exactly on each input value, so the
  polarity of a count in the guard does not matter.

So a count is read raw: its reads are those of an `Exists`, and nothing rewrites it. The
generator of `scripts/reduction-diff.py` emits no counts, and the corpus cannot test the
reduction: TLC reads the reduced cone's tables too. The unit test
`a_count_over_t_in_a_guard_reduces_exactly` takes the case fold of the reduction tests,
whose guard over the sub-lifecycle reduces, puts `at_most`, `at_least`, and `exactly` in
place of its `.none`, and checks that each reduced cone decides what the plain cone decides
at 1, 2, and 3 instances.

### Code generation

A guard lowers to `Expr::Count { input, elem, body, op, n }`:

- TypeScript: `rows.filter((r) => body).length <= 3`
- Rust: `rows.iter().filter(|r| body).count() <= 3`
- Go: a closure that counts in a loop.

`MaxCard_*` is an obligation in the validators, as `MinCard_*` and `Unique_*` are.

## Tests

- Corpus `safety_at_most_bound.rdra`: an unguarded `at_most(3)` is ok at 1, 2, and 3
  instances and a counterexample at 4, beside a guarded rule that holds at every count and
  that z3 proves. The corpus read verdicts at 1 to 3 instances; a `bfs=` of four entries
  now adds a run at 4 (`docs/engine-agreement.md`).
- Corpus `safety_at_least_guard.rdra`: an `at_least` guard on a use case and the rule it
  keeps, against a delete, and the same pair with nothing deleted, which z3 proves.
- Corpus `safety_count_per_entity.rdra`: counts under `--instances Loan=4 --instances
  Shelf=3` through TLC, a counted entity with more slots than its anchor and an anchor
  with more slots than the counted entity.
- Corpus `safety_count_guard_order.rdra`: an `at_most` and an `at_least` guard on one row
  that only the exact counts keep apart.
- Corpus `safety_max_card_range.rdra`: `0..2:1`, ok at 1 and 2 instances and a
  counterexample at 3.
- A count bug planted once in `eval` must fail the corpus.
- `samples/formal-verification/counting_rules.rdra` with its `// expect` header, run at
  the default bounds and at `--instances Loan=4`.

## Decisions to review

1. Two stacked PRs: per-entity counts first, counting rules on top.
2. `--instances E=n`, repeatable, beside `--instance-count`; `instances = { E = n }` under
   `[check]` and `[verify]`; the generators read neither.
3. `Bounds::per_entity` by entity id, resolved into `EntityDecl::slots`; `Bounds` is
   `Clone`, not `Copy`.
4. `IdentityStartSlots`: unequal counts for a pre-existing child that starts linked one to
   one are an error.
5. A count counts distinct rows of the counted entity, not chains.
6. Guards count the rows before the step; the cap on a creating use case is `n - 1`.
7. `at_least(0)`, `at_least(1)`, `at_most(0)`, and `exactly(0)` are errors with the form to
   write.
8. Ranges `lo..hi` with `hi` up to 255 and `lo` still 0 or 1.
9. Vacuity premise of `n + 1` rows; `CountAboveBound` warning for `at_least` above the
   bound.
10. z3 unrolls up to 8 witnesses.
11. The corpus header's `bfs=` takes a fourth entry, the native verdict at 4 instances,
    so a count that needs four rows states its counterexample at the uniform bound rather
    than through `--instances`.
