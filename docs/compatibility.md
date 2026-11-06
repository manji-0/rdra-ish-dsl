# Compatibility

This page states what rdra-ish keeps stable across releases. The contract takes effect at
1.0.0. Until then a minor release may break any part of it, and the `### Breaking` section
of the [changelog](../CHANGELOG.md) records each break with its rewrite.

From 1.0.0 on, rdra-ish follows semantic versioning. A change that breaks a covered item
needs a major release. A minor release adds to the covered surface and fixes bugs. A patch
release fixes bugs only.

## What a major release protects

### The language

<!-- constrained-by ./language-reference.md -->
- **Syntax.** A model that loads keeps loading. A form is removed only in a major
  release, after the [deprecation period](#deprecation).
- **Meaning.** For a given model, `check` and `verify` report the same verdict within the
  same bounds: the same exit status and the same set of error names. The bounds are the
  values the [`bounds:` line](#bounds) prints.
- **Diagnostic names.** The names in `error[<Name>]`, `warning[<Name>]`, and
  `info[<Name>]` (see [Diagnostics](./state-derivation.md#diagnostics)) and the lint rule
  ids (such as `api-contract-missing`) keep their meaning. A finding keeps its severity
  when that severity decides the exit status: turning a warning into an error, or adding
  a lint error, is a breaking change. A new warning or info finding is not.
- **`// expect` headers.** The header grammar in
  [Sample expectations](./formal-verification.md#sample-expectations) and the names it
  matches are stable, so a sample's expectation keeps holding.

### The command line

<!-- constrained-by ./cli-reference.md -->
- **Subcommands and flags.** Each subcommand and flag in the
  [CLI reference](./cli-reference.md) keeps its name, its meaning, and the values it takes,
  such as each `--kind` and `--format`.
- **Exit status.** `0` is a clean run. `1` is a run that found errors, failed to load a
  model, or could not run a tool it needs (such as `verify` without `tlc`). `2` is a usage
  error: an unknown subcommand or flag, or a `--format` the kind does not take.
- **Machine-readable output.** The `--format json` keys of `lint`, `next`, `list`, and
  `states`, and the `--format csv` columns of `lint`, `list`, `states`, and `csv`, as the
  CLI reference and [Output formats](./state-derivation.md#output-formats) document them. A release may add a key or append a column. It does not
  rename, remove, or reorder one.
  In `next`, a stage's `slug` is its stable id. Its `stage` value (`"2"`, `"7A"`) is a
  display label that a change to the stage order renumbers.
- **The `bounds:` line and `tlc_result:` line.** Their form stays parseable as the CLI
  reference shows it.

### The Mermaid diagrams

<!-- constrained-by ./cli-reference.md#encoding-contract -->
A diagram is written so that a program can read every fact back from the text. That
encoding is covered, as [Encoding contract](./cli-reference.md#encoding-contract) states
it and `crates/rdra-ish-cli/tests/diagram_contract.rs` checks it on every sample:

- The `%%` header lines: `%% rdra-ish diagram=<kind> scope=<scope> model=<path>`,
  `%% layers:`, `%% kinds:`, and `%% omitted:`.
- Node ids `<kind>__<Id>`, the same id in every kind, and `variation__<Variation>__<value>`
  for a variation's value.
- `:::<kind>` on each node, one `classDef <kind>` per kind, and the Mermaid shape of each
  kind.
- The edge-label rule: an edge has no label exactly when the kinds of its ends imply one
  relation.
- The `%%` comment lines in the body, such as `%% desc` and `%% derived`.

The order of nodes and messages is covered where the encoding contract states one, as for
the robustness and sequence kinds. The layout a renderer draws, any order the contract does
not state, and the colors in a `classDef` are not covered.

### The TLA+ export

<!-- constrained-by ./formal-verification.md#mapping -->
These names in `export --kind tla` and `verify` output are stable, so a script or a `.cfg`
written by hand can name them:

- Invariants: `Forbidden_<E>_<n>`, `Inv_<E>_<n>`, `Required_<E>_<n>`, `Exclusive_<E>_<n>`,
  `MultiForbidden_<E>_<n>`, `MultiInv_<E>_<n>`, `Quantifier_<E>_<n>`, `Check_<E>_<column>_<n>`,
  `Unique_<Child>_<role>`, `MinCard_<Child>_<role>`, and `TypeOK`.
- Properties: `AfterAssert_<UC>_<n>`, each temporal property under its declared name, and
  `Fair_<P>`.
- Variables and constants: `<E>_exists`, `<E>_<col>`, `<Child>_owner_<Parent>`, `now`,
  `InstanceCount`, and `IntRange`.
- State-model specs of `export`: the modules `StateModel_<E>` with their cfgs
  `StateModel_<E>.cfg` and `StateModel_<E>_witnesses.cfg`, and their invariants
  `Deadlock_<E>_<col>_<s>` and `Witness_<E>_<col>_<s>`, which have the names of the
  findings `check` prints.

`<n>` is the rule's index among the model's rules of that kind, in load order, so it moves
when the model adds a rule of the same kind before it, on any entity. That is a change to
the model, not to the tool.

A name that another name already took gets the first free `_<k>` suffix, `k` from 2, that
no other item of the module is named. The suffix appears only on a collision, so a model
with no collision keeps every name above:

- A rule name yields to a temporal property's id and an earlier rule's name, in `check` and
  in the module alike. Ids may hold `_`, so `Cart.Line_qty` and `Cart_Line.qty` both give
  `Check_Cart_Line_qty_0`; the second check is `Check_Cart_Line_qty_0_2`.
- In the module, the properties, the rules, and `Fair_<P>` come first, in that order. A
  variable, an `<E>_Ids` set, or an action spelled like one of them yields: a property
  `Job_status` beside the column `Job.status` keeps its name, and the variable is
  `Job_status_2`. A use case's id does not move a rule's or a property's name.
- Among the rest, an `<E>_Ids` set comes before a variable, a variable before a later
  variable, and a variable before an action. A column `run_Go` beside the run mark of a
  split use case `Go` gives `Job_run_Go` and `Job_run_Go_2`.
- The `Rep_<var>` and `Succ_<action>` tables of a reduced cone, and its `S_class`, yield to
  every other name but a state-model spec's `Deadlock_` and `Witness_` invariants, which
  yield to every other name.
- The files `export` writes yield in this order: the whole-model module keeps the name `-o`
  gives it, then each `StateModel_<E>` and its `StateModel_<E>_witnesses` cfg, compared
  ignoring case.
- The files `verify` writes yield in this order: the whole-model module keeps the name `-o`
  gives it, then each check's `Cone_<name>`, then the `_Full` and `_WF` modules, the
  whole-model run `<whole>_<P>`, and the rerun cfg `<module>_rest<n>.cfg`. File names are
  compared ignoring case, as macOS and Windows compare them, so properties `Ready` and
  `ready` write `Cone_Ready.tla` and `Cone_ready_2.tla`.

The module layout is not covered: the operator bodies, the order of definitions, the
cone files `verify` writes, and how the use cases split into actions may change in a
minor release.

## What is not covered

- **The Rust crates' API.** `rdra-ish-core`, `rdra-ish-syntax`, `rdra-ish-emit`,
  `rdra-ish-lsp`, and `rdra-ish-cli` are internal. Their public items may change in a minor
  release. Depend on the CLI and its output, not on the crates.
- **Generated sample documents and their generators.** `design-doc.md` and `prd.md` in the
  samples, `scripts/gen-clinic-ops-docs.py`, and the other scripts under `scripts/`.
- **The agent skills' file layout.** The files under `skills/` and their names.
- **Message wording.** The text of a human-readable message may change. Its diagnostic
  name may not.
- **Performance and state counts.** Run time, memory, and the number of states a run
  explores or prints.
- **Diagram layout.** Where a renderer places the nodes, as above.

## Bounds

<!-- constrained-by ./formal-verification.md#approximations-and-limitations -->
Every result holds within the bounds its `bounds:` line prints: the instances per entity,
the Int range, and the state caps of `check` and of the `verify` BFS. A release may change
a default bound or cap. That is not a breaking change, and the changelog records it. A
result that changes for a model within the same printed bounds is a breaking change,
unless it fixes a bug that the changelog names. 2.0.0 is such a change: for a model with
`raises` and `triggers` between use cases, `check` and `verify` order the triggered use
case after its event within the same bounds, so a verdict can move while the `bounds:` line
stays as it was. The `### Breaking` entry for #112 in the [changelog](../CHANGELOG.md)
names which verdicts move and how to migrate.

To keep a result reproducible across releases, pass the bounds a command takes as flags
instead of relying on the defaults: `--instance-count`, `--int-range-max`, and
`--max-states` for `check` and `states`, and `--instance-count` and `--int-range-max` for
`verify`. The [Bounds](./state-derivation.md#bounds) table lists each command's defaults.
[Known verification gaps](./formal-verification.md#known-verification-gaps) lists what the
bounds leave unchecked.

## Deprecation

A form that a later major release removes goes through these steps:

1. A minor release deprecates it. The form still loads and means what it meant. Each use
   prints a warning that names the rewrite.
2. The form stays deprecated for at least one minor release.
3. `fmt --write`, or a migration script under `scripts/` when `fmt` cannot express the
   rewrite, turns each use into the new form.
4. A major release removes it. A removed form fails to load with a message that names its
   rewrite.

Before 1.0.0 there is no deprecation period: a removed form fails to load at once, and the
message names the rewrite, as `writes` and the top-level `when(...)` rule do in 0.4.0 and
`sets(UseCase, Entity, column == value)` does in 1.0.0.
