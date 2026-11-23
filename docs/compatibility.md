# Compatibility

This page states what rdra-ish keeps stable across releases. The contract takes effect at
1.0.0. Until then a minor release may break any part of it, and the `### Breaking` section
of the [changelog](../CHANGELOG.md) records each break with its rewrite.

From 1.0.0 on, rdra-ish follows semantic versioning. A change that breaks a covered item
needs a major release. A minor release adds to the covered surface and fixes bugs. A patch
release fixes bugs only.

The page covers releases. Between two releases, `main` may change a default, an output
line, or a verdict, and the `Unreleased` section of the changelog lists each such change.
This page states what the next release keeps, so it changes on `main` in the same change as
the behavior it describes.

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
  matches are stable, so a sample's expectation keeps holding. The next release changes the
  grammar: a `verify` line reads `ok` or `fail <Names>`, and `tlc=` stays only on a line
  whose flags hold `--with-tlc`.

### The command line

<!-- constrained-by ./cli-reference.md -->
- **Subcommands and flags.** Each subcommand and flag in the
  [CLI reference](./cli-reference.md) keeps its name, its meaning, and the values it takes,
  such as each `--kind` and `--format`. Of `codegen`, only `--lang ts` and `-o` are
  covered, as [The TypeScript types of `codegen`](#the-typescript-types-of-codegen) states.
  The next release removes `export --kind typescript-states` and
  `states --format typescript`, so it is a major release. Each now fails as a usage error
  that names its rewrite, `codegen <INPUTS> --lang ts -o <FILE>.ts`.
- **The default of `verify`.** `verify` decides every check with its native checker and
  starts no JVM. `--with-tlc` runs TLC beside it and fails the run when the two decide a
  check differently. `--backend tlc` is a deprecated spelling of `--with-tlc` that prints a
  warning, and the major release after the one that changed the default removes it. The
  next release makes these changes, so it is a major release.
- **Exit status.** `0` is a clean run. `1` is a run that found errors, failed to load a
  model, or could not run a tool it needs (such as `verify --with-tlc` without `tlc`). `2`
  is a usage error: an unknown subcommand or flag, or a `--format` the kind does not take.
- **Machine-readable output.** The `--format json` keys of `lint`, `next`, `list`, and
  `states`, and the `--format csv` columns of `lint`, `list`, `states`, and `csv`, as the
  CLI reference and [Output formats](./state-derivation.md#output-formats) document them. A release may add a key or append a column. It does not
  rename, remove, or reorder one.
  In `next`, a stage's `slug` is its stable id. Its `stage` value (`"2"`, `"7A"`) is a
  display label that a change to the stage order renumbers.
- **The `bounds:`, `result:`, and `tlc_result:` lines.** Their form stays parseable as the
  CLI reference shows it. Every `verify` run that reaches its checks prints `result:` with
  `ok`, `not decided`, `counterexample`, or `tool_error`: the verdict of the whole run.
  `ok` means exit 0 with nothing undecided, so a script that reads only this line never
  misses a failure; `counterexample` means a check or a state-model error of the entity
  cones failed the run; `not decided` means nothing failed but a cap, the budget, or
  `--deny-unproved` left something open (exit 0, or 1 under `--deny-undecided`);
  `tool_error` means a tool bug or a failed TLC run. `tlc_result:` has the form
  `ok|counterexample|tool_error`, gives TLC's verdict, and prints only under `--with-tlc`.

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

### The TypeScript types of `codegen`

<!-- constrained-by ./cli-reference.md#codegen -->
`codegen` is experimental except for the TypeScript types it prints. The contract covers
`rdra-ish codegen <INPUTS...> --lang ts [-o <FILE>]` on a model that `check` passes.
For such a model, the command writes one TypeScript module to stdout, or to `<FILE>`
with `-o`, and exits 0, unless the model hits a `NameCollision` (see the list of what is
not covered below). The module declares the types below with these names and shapes.
`crates/rdra-ish-cli/tests/codegen_contract.rs` pins these types for a model that uses
every column type, every naming rule, and every rule form below.

A type name is a model id in PascalCase. `_`, `-`, and spaces split the id into words. The
first letter of each word becomes upper case, and the other letters stay as written. A
name that does not start with an ASCII capital letter gets the prefix `V`. When two values
of one tag column give the same name, the first value keeps it. The later values get the
suffix `2`, `3`, and so on.

- **Scalars.** `Money`, `Decimal`, `Date`, and `DateTime`, each declared as
  `string & { readonly __brand: "<Name>" }`.
- **Id types.** `<Entity>Id` for each entity with a one-column primary key of a type other
  than `Enum` or a variation. It is the key column's type from the table under
  **Properties**, with `& { readonly __brand: "<Entity>Id" }`. For example, an `Int` key gives
  `number & { readonly __brand: "OrderId" }`. An entity with a key of two or more
  columns has no id type.
- **Value sets.** `<Variation>` for each variation, and `<Entity><Column>` for each inline
  `Enum` column of an entity. Each is a union of string literals, one for each value as it
  is written in the model. Each also has the constant
  `<Name>Values: readonly <Name>[]`, which holds every value.
- **State types.** An entity whose tag column is an inline `Enum` with at least one value
  is a state type. The tag column is chosen from the columns that `transitions` names:
  `status`, else a column whose name ends in `_status`, else the first such column by name.
  Each value `v` of the tag column gets an interface `<Entity><V>`, including a value that
  no use case reaches. The interface has the property `<tag>: "<v>"`, and one property for
  each other column of the entity. `<Entity>` is the union of these interfaces. An entity
  whose `Enum` only `sets` writes, with no `transitions`, is not a state type.
- **Row types.** `<Entity>Row` for each state type: an interface with one property for each
  column of the entity. The tag column takes its value set, and no column is narrowed.
- **Properties.** The name of a property is the column name. The property type follows
  the column type:

  | Column | TypeScript |
  |---|---|
  | `Int` | `number` |
  | `String` | `string` |
  | `Bool` | `boolean` |
  | `Money`, `Decimal`, `Date`, `DateTime` | the scalar of the same name |
  | a variation | its value set |
  | an inline `Enum` | `<Entity><Column>` |
  | the entity's one-column primary key | `<Entity>Id` |
  | a key column to a parent's one-column primary key | the parent's `<Parent>Id` |

- **Nullability.** A `@null` column is `T | null`. In a state type, a rule of the entity
  narrows the column for each state that the rule names. The column is then `T` where the
  rule makes it present, and `null` where the rule makes it null. Here `x` is a `@null`
  column other than the tag, `v` is a value of the tag column, and only these rules narrow:
  - `required(E, .., x == present, ..)` makes `x` present in every state, and
    `x == null` makes it null in every state. Each such conjunct narrows, whatever the
    other conjuncts are.
  - `forbidden(E, tag == v, x == present)` makes `x` null in `v`, and `x == null` makes
    `x` present in `v`. The tag atoms may be several `tag == v` and `tag in (..)` atoms,
    which name the states they all allow, or none, which names every state. Besides them,
    the rule holds exactly one atom, on `x`.
  - `invariant(E).when(tag == v).then(.., x == present, ..)` makes `x` present in `v`,
    and `x == null` makes `x` null. The `when` holds at least one tag atom of the forms
    above and no other atom. Each such conjunct of the `then` narrows, whatever the other
    conjuncts are.

  A `forbidden` or `invariant` with a `.when(Cond)` guard narrows nothing, and neither
  does a rule on a derived column. When two rules narrow one column in opposite ways in one
  state, the column stays `T | null` in that state. A minor release does not add a rule
  form that narrows, because that would change the type of a column in a model that the
  contract already covers.

The contract does not cover the following:

- the comments and doc comments;
- the order of declarations and properties;
- the header and its obligation and candidate lists;
- `InvalidRow`, `<entity>FromRow`, and `<entity>ToRow`;
- the transition functions, results, events, run marks, rule checks, `Rejected`,
  `NotEnabled`, and dispatchers;
- the interfaces of entities that are not state types, and of DTOs;
- which models fail with a `NameCollision`: the experimental declarations share the
  namespace, so a new one can collide with a model's name;
- the output of `--allow-failing`, `--package`, and `--lang rust` or `--lang go`.

### The TLA+ export

<!-- constrained-by ./formal-verification.md#mapping -->
These names in `export --kind tla` and `verify` output are stable, so a script or a `.cfg`
written by hand can name them:

- Invariants: `Forbidden_<E>_<n>`, `Inv_<E>_<n>`, `Required_<E>_<n>`, `Exclusive_<E>_<n>`,
  `MultiForbidden_<E>_<n>`, `MultiInv_<E>_<n>`, `Quantifier_<E>_<n>`, `Check_<E>_<column>_<n>`,
  `Unique_<Child>_<role>`, `MinCard_<Child>_<role>`, `EventOverflow_<E>_<Ev>_<UC>` (a
  [counted trigger](./state-derivation.md#counted-triggers)), and `TypeOK`.
- Properties: `AfterAssert_<UC>_<n>`, each temporal property under its declared name, and
  `Fair_<P>`. A `precedes` property adds the variable `<P>_seen` and the operator
  `<P>_latch` in a module whose `.cfg` checks it.
- Variables and constants: `<E>_exists`, `<E>_<col>`, `<Child>_owner_<Parent>`, `now`,
  `InstanceCount`, and `IntRange`. `<E>_vars` is the tuple of an entity's variables, for an
  entity with two or more; it is claimed last, so a column `vars` keeps `<E>_vars`.
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
  variable, a variable before an action, and an action before a `<P>_seen` or `<P>_latch`. A column `run_Go` beside the run mark of a
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
- **Which counterexample prints.** `verify` prints the native checker's counterexample, the
  shortest trace or lasso its search finds. It may differ from the one TLC picks, and from
  one release to the next.
- **Performance and state counts.** Run time, memory, and the number of states a run
  explores or prints.
- **Diagram layout.** Where a renderer places the nodes, as above.
- **The rest of `codegen`.** All of `codegen` except
  [its TypeScript types](#the-typescript-types-of-codegen) is experimental. Its other
  flags, its Rust and Go output, and the parts of the TypeScript output that the section
  excludes (names, shapes, the obligation list) may change or be removed in any release.
- **`verify --prove` and `.lemma()`.** The flag and its companions `--deny-unproved` and
  `--z3` are experimental: the `proved:` / `unproved:` / `lemma:` lines, which rules prove,
  and the clause the `OK:` line gains may change or go away in any release. So is the
  `.lemma()` marker on a rule: `check` checks a lemma like any rule, but `verify` runs no
  cone for one, so a lemma that only fails at two instances passes `verify`. Do not mark
  a rule the model must keep as a lemma. Apart from lemmas, `verify` without `--prove` is
  unchanged.

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
`--max-patterns` (its default follows `--instance-count`) and `--max-states` (default none) for `check` and `states`, and `--instance-count` and `--int-range-max` for
`verify`. The [Bounds](./state-derivation.md#bounds) table lists each command's defaults.
The [memory limit](./cli-reference.md#memory-limit) (`--memory`) can leave a cone
undecided as `--max-states` does; its count of a cone's bytes is the same on every
machine, but a release may change it.
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
