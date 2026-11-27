# Verus specifications for the Rust code (`codegen --verus`)

<!-- constrained-by ./code-check.md#decisions -->

Status: V1, experimental (#444). `codegen --lang rust --verus <DIR>` writes, beside the
ordinary Rust output, a Verus crate in which a modified core is checked against the verified
model for any number of rows. The ordinary output does not change, byte for byte, and keeps no
dependency (code-check decision 14).

## The claim

When `verus DIR/main.rs` reports 0 errors:

1. **The model keeps its rules for any number of rows.** `model.rs` holds the model as Verus
   spec: per entity a map from id to row, per action an enabling condition and a step, the
   state rules, and an inductive set (the rules and one lemma per key: a child's key names a
   present parent). `run_keeps_rules` proves that every state any list of calls reaches from no
   rows keeps every rule. Unlike `check` and `verify`, which explore a bounded number of rows,
   this has no bound.
2. **The core does what the model says, on its rows.** `contract.rs` gives each function of
   the core a contract: on its rows (and on every row of a related entity it reads as a list),
   it returns `Ok` exactly when the action's guards hold, and the rows it returns are the rows
   after the action's step, with the expected sources the model's. `core.rs` is the core,
   as the methods of the trait the contracts are on, and is the file the user edits.
3. **The port moves the store as the model does.** Each `port_<action>` loads the rows, calls
   the core, and commits. Verus proves `final(store)@ == next(old(store)@, call)` and
   `Ok <==> enabled(...)`: the store's rows move exactly as one step of the model, or not at
   all, for any core that meets its contracts.

The proofs use no `assume` and no `admit`. A test plants `assert(false)` at the top of every
proof and every function and checks that each one fails (`an_assert_false_fails_in_every_proof`).

## The trust boundary

What Verus does not check, and what each part rests on:

| Trusted | Why it is trusted | What checks it instead |
|---|---|---|
| The store (`Store` in `port.rs`, `#[verifier::external_body]`) | Its functions stand for a database: `load_<e>` returns the row by id, `rows_<e>` every row once, and `commit_<fn>` writes in one transaction only where each row is still what the result expects. The bodies are an in-memory store. | #439's conformance tests (H3, H4) for a real store |
| That `model.rs` means what `check` and `verify` mean | The spec is printed from the `TransitionSystem`, the semantics BFS, TLA+ export and the code generators share, but Verus proves the spec, not the printer | The differential test, below |
| Verus, its z3, and the Rust compiler | | |
| Code outside `verus!` | `Display`, `Error`, `From`, `from_row` and `into_row` are plain Rust | Ordinary tests |

**The differential test.** `harness.rs` reads a dump of every state and move BFS finds at two
rows per entity, and for every state and every call on every slot runs the verified port over
the in-memory store, then compares the rows it leaves with the BFS successor, or with the state
when BFS has no such move. A difference is a difference between the spec (or the store) and
rdra-ish's semantics. Some creates run only in the spec, which takes any absent id where BFS
takes the lowest free slot, never reuses a deleted one, and creates nothing once every slot is
used. The harness counts those apart (`spec-only creates`). For a create into another slot it
checks only that BFS makes a create of the same call at the slot it picks, not the successor;
a create with no slot left is counted and not checked. A create over a present row is a
mismatch.

| Model | States | Calls | Agree | Spec-only creates | Mismatches |
|---|---|---|---|---|---|
| `tests/fixtures/verus/library.rdra` | 147 | 1,764 | 1,655 | 109 | 0 |
| `quantifier_guard.rdra` | 65 | 650 | 644 | 6 | 0 |
| `counting_rules.rdra` | 99 | 990 | 984 | 6 | 0 |
| `min_card_met.rdra` | 4 | 32 | 29 | 3 | 0 |

## What V1 covers

A model is in scope when every column the lifecycle tracks is an `Enum` or a `Bool` column or a
key (a `relate`), and every use case is one step that creates, updates, transitions or deletes
rows, guarded by conditions on its rows and by quantifiers and counts over related rows (`.has`,
`.none`, `.at_most` and the like). Everything else is refused with one
line per reason and exit 1, after the ordinary output is written:

- `Int` and nullable columns, `now`
- run marks, split use cases, events (raises and triggers)
- choices: variation columns, `.when` arms
- entities no use case creates (master data), optional keys, a deleted parent
- a quantifier inside a quantifier, a condition on a successor row

Verus may still reject a model in scope. A rule `verify` refutes fails its preservation lemma at
the action that breaks it (`one_to_one_shared`, `none_any_child_bypassed`). A rule that holds
but is not inductive with the key lemmas alone fails the same way: in `cross_order_payment`,
Cancel would break the rule from a state where a captured payment's order is still pending, a
state no run reaches. Proving such a rule needs a stronger invariant; the structured lemma of
V2 is where it will come from.

## Decisions

**The functions are trait methods, not free functions.** Each contract is the `ensures` of a
method of `pub trait DomainCore` in `contract.rs`; `core.rs` implements it, and each port call
is generic over `C: DomainCore`. A core that weakens what it promises fails at its method: the
error is the method's postcondition, with the core's body underneath it. The contracts sit in a
generated file the user does not edit, so the check never rests on a contract the user changed,
and a renamed or missing method is a compile error rather than a contract that silently goes.
Free functions would carry their `ensures` in the file the user edits.

**Fields are public; there is no seal and no checked constructor.** The ordinary Rust output
keeps state values sealed in their module (#331). Verus lets a public spec function read only
public fields, and the contracts and the trait are public, so the fields are `pub`. The Verus
crate is one crate in which the core and the port sit side by side, so `pub(crate)` would hide
nothing either. The seal is not needed: the port's proof holds for every value a core returns,
however it was made, because each contract states the row it must return, not that it was built
by a transition. Constructors with `ensures` would add a second statement of what the contract
already says.

**Errors say only that the action did not run.** A contract asks for `Err` exactly when the
guards fail, not which error. `Rejected` and `InvalidArgument` are returned as the ordinary
output returns them, but Verus does not check which one.

**Separate files, a separate toolchain.** The crate goes to its own directory and the ordinary
output does not change. Verus `0.2026.11.22.1ce6a59` runs on Rust 1.100.0; the repository's
toolchain and the flake's devShell stay as they are. CI installs both in
`.github/workflows/verus.yml`, which runs the Verus tests on main once a day, by hand, and on a
pull request that touches the generator. Without `verus` on PATH the tests that need it print
`skipped:`; `RDRA_REQUIRE_VERUS=1` turns the skip into a failure.

## The crate

| File | Edited by | Holds |
|---|---|---|
| `main.rs` | rdra-ish | the module list, and the harness's entry |
| `types.rs` | rdra-ish | the data types of the ordinary output |
| `prelude.rs` | rdra-ish | per entity, a list of rows as a map, and the lemmas that a quantifier or a count over the list is the one over the map |
| `model.rs` | rdra-ish | the state, one step per action, the rules, the inductive set, and the preservation lemmas |
| `contract.rs` | rdra-ish | what each function must do, and the trait |
| `core.rs` | the user | the functions |
| `port.rs` | rdra-ish | the trusted store, and one verified port call per action |
| `harness.rs` | rdra-ish | the differential test (not verified) |

A quantifier over related rows is a `while` loop with its invariant, and the proof hints the
lemmas need are printed in a fixed shape: for a count, the counted set after a step as an insert
or a removal of the written row; for an existential, its witness kept or gained. The crate for
`library.rdra` verifies in about 0.6 s.

## Next

- V2: a structured kept lemma, so a non-inductive rule can be proved.
- V3: the differential test as a product check beside #439's one-step judge; `fresh` detects an
  edit to a generated file.
- V4: the rest of the language: run marks, events, split use cases, `Int` and nullable columns,
  `now`, choices.
