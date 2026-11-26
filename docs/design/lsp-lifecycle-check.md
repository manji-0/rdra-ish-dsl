# Lifecycle findings in the editor

Status (2026-11-26): built for #214, in the `Unreleased` part of the changelog. The default
(on), the time budget (2 s), and the memory limit (512 MiB) are owner decisions of
2026-11-26 ([Decisions](#decisions)).

`rdra-ish-lsp` used to run the analysis alone: `consistency_diagnostics` and
`lint_review_diagnostics`. A rule violation, a deadlock, or a `Vacuous_` warning showed only
when the user ran `check` in a terminal. The server now runs the exploration `check` runs,
in the background after each analysis, and shows its findings where they are declared.

## What runs

- **The entry `check` uses.** `derive_states_checked` builds the system at
  `Bounds::STATES` (one instance per entity, `Int 0..5`) under `Scope::ALL` and explores each
  target's cone with `Caps::for_instances(1)`, the pattern cap `check` uses. It is
  `derive_states_built` with the outcome of each invariant and `after(..).assert` added, so
  the findings are the ones `check` prints.
- **One cone at a time, on one core.** The run goes inside `rdra_ish_core::one_at_a_time`,
  which makes `jobs()` and `bfs_threads()` return 1 on its thread: `in_parallel` runs the cones
  in turn on the caller's thread, and a BFS run takes no helper. `RDRA_JOBS` does not change
  it.
- **Under a budget.** The caps are charged to a pool of the memory limit (default 512 MiB)
  and stop at a deadline (default 2 s from the start of the run, `Caps::until`). A cone that
  stops leaves what it reads undecided, as a cone stopped at a cap does in `check`.
- **Only on a model that loaded.** When the load has an error, `check` explores nothing, and
  neither does the server. The analysis's own warnings do not stop the check.
- **After a quiet moment.** The check starts 200 ms (`quietMs`) after the analysis
  publishes, so a burst of typing runs the analysis of each edit and the check of the last.

## Which result is shown

The rule of #157 holds for the check too. Each refresh takes a generation number, and only
the latest generation publishes.

- A newer refresh drops a check that has not started, during its quiet moment or while it
  waits for the running check.
- A check that ran publishes only if its generation is still the latest when it takes the
  publishing lock. Otherwise its result is dropped.
- Checks run one at a time (an async mutex). One memory pool per limit serves every run, so
  a run never meets the bytes of another.

**Cancellation.** A check that has started is not stopped in the middle of a BFS. Today the
only way to stop a run is the deadline, which the BFS reads every 256 states
(`explore/bfs.rs`). A stale run therefore runs to its end or to its budget, and the next
check waits for it. With the default budget of 2 s that is at most 2 s of one core. On every
sample a whole check takes under 0.1 s, so in practice nothing waits. A true abort needs a
cancel flag on `Caps` read beside the deadline in `bfs.rs`. That file is being changed by
#382, #385 and #388, so the flag is left for after they land.

**Publishing.** `publishDiagnostics` replaces a file's list, so the server keeps the
analysis diagnostics it last published per file and publishes each file the check has
findings in as the union of the two. The next analysis publishes its own diagnostics alone,
which withdraws the findings of the last check until the new check publishes. Findings never
outlive the text they were found on; the cost is that they blink out for the quiet moment
plus the check on each edit.

## Where a finding goes

Every finding kind is named in one exhaustive `match` (`Placer::diag` in
`crates/rdra-ish-lsp/src/lifecycle.rs`), so a new kind does not compile until it says where
it goes.

| Finding | Range |
|---|---|
| A violated invariant, `forbidden`, or `after(..).assert` (`ConstraintViolated`, `AfterAssertViolated`) | the rule's declaration (its `site`) |
| `Vacuous_` (`VacuousRule`) | the rule's declaration |
| A use case that never runs (`NeverRuns`, `TriggerNeverRuns`) | the use case's declaration |
| An arm or a chosen value never taken (`ArmNeverTaken`, `ValueNeverChosen`) | the `transitions` or `chooses` line |
| A deadlock, an unwitnessed state, an unreachable enum value (`Deadlock`, `NoWitness`, `UnreachableEnumVariant`) | the entity's declaration, where its state model is |
| A build finding with a site (`UnjoinedWrite`, `IntLiteralOutOfBounds`, and the rest) | its site; the first of several |
| A finding with no site (`NoCreationPath`, a cap or the budget reached) | the declaration of the entity it is reported on |
| An undecided finding | the entity's declaration for a state or a deadlock, the use case's for a run, an arm or a choice, the property's for a `can_reach` |

A finding whose site has no span, and no entity to fall back on, goes on the first line of
the first source file, so that none is dropped.

A finding is shown once even when two entity views report it. The kinds that carry no
entity of their own (`NoCreationPath`, `UnreachableEnumVariant`, and the stops) are shown
once per entity, and their message starts with `entity <id>:`.

The severity is `check`'s, with two changes: a cone that stopped (a cap, the memory limit,
or the budget) is information, and so is each undecided finding. The diagnostic's `source`
is `rdra-ish check (1 instance)` and its `code` is the name `check` prints, such as
`Forbidden_ClinicalOrder_1` or `NoCreationPath_Provider`.

## Code lenses

Each invariant, `forbidden`, and `after(..).assert` gets one lens on the first line of its
declaration: `check: ok (1 instance)`, `check: violated (1 instance)`, or
`check: not decided (1 instance)`. A declaration that builds several checks shows the worst
outcome. The outcome comes from the target's own cone (`CheckVerdict`):

- violated when the report holds its counterexample;
- ok when its cone ran to the end;
- not decided otherwise.

Temporal properties get no lens, since `check` does not check them. The server sends
`workspace/codeLens/refresh` after a check publishes when the client supports it.

The lens says "1 instance" because `verify` explores two. A rule that holds at one instance
may fail at two, and a deadlock at one may clear at two.

## Settings

`initializationOptions.lifecycleCheck`, or `lifecycleCheck` under the `rdra-ish` section of
`workspace/didChangeConfiguration`:

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Run the check after each analysis. |
| `timeBudgetMs` | `2000` | When a run stops, from its start. |
| `memory` | `"512MiB"` | What the run's state stores may hold. |
| `quietMs` | `200` | How long after an analysis the check starts. |

The VS Code extension passes `rdra-ish.lifecycleCheck.*` as these and sends changes.

## Measurements

Release build, Apple silicon, 2026-11-26, with `quietMs` 0 so the figures hold no wait. An
edit added a line above the rules each time, and the time is until the server answered
`codeLens` with the new text's lenses. Medians of nine edits after the first:

| Sample | Edit to analysis published | Edit to lenses | `check` alone, `RDRA_JOBS=1` (CPU, peak RSS) |
|---|---|---|---|
| clinic-ops | 29 ms | 64 ms | 0.04 s, 24 MB |
| food-permit-ops | 10 ms | 40 ms | 0.02 s, 20 MB |
| insurance-claim-ops | 9 ms | 66 ms | 0.06 s, 21 MB |

So the check adds 30 to 60 ms of one core after each analysis on these samples, and its
memory is far under the 512 MiB limit. With the default `quietMs` of 200, findings show
about a quarter of a second after typing stops. The LSP test in a debug build, with its
quiet moment of 50 ms, sees the rule broken by a guard edit on clinic-ops about 0.5 s after
the edit.

## Changes in rdra-ish-core

None touches `explore/` or `memory.rs`.

- `parallel.rs`: `one_at_a_time`, a scoped thread-local override of `jobs()` and
  `bfs_threads()`.
- `lifecycle/targets.rs` and `guards.rs`: `UndecidedWhy::TimeBudget`. A deadline on the
  entity cones of `explore_targets` used to panic (`an incomplete run stopped at a cap`),
  since `verify` sets one only on whole-model runs.
- `lifecycle/states.rs`: `derive_states_checked`, `Checked`, `CheckVerdict` and
  `CheckOutcome`. `derive_states_built` now calls it and drops the outcomes.

## A verdict cache later

#210 would cache each cone's verdict by its restricted system. The check here is a pure
function of the model, the bounds and the caps, and it reads its cones through
`explore_targets`. A cache inside `explore_targets_with` would serve the server with no
change here. Its key must leave out the deadline and the pool, which do not change a
complete cone's verdict.

## Decisions

The owner decided these on 2026-11-26, recorded on #214:

- **On by default.** The check runs after each analysis unless `enabled` is `false`.
- **A time budget of 2 s and a memory limit of 512 MiB.** The samples need under 0.1 s and
  under 30 MB at one instance ([Measurements](#measurements)).

## Open questions

- **Keep old findings until new ones arrive.** That would not blink, but it would show
  findings at lines the edit moved.
- **Mid-BFS cancel.** As described under Cancellation, after #382, #385 and #388.
- **Counterexample traces.** The CLI's trace renderer lives in `rdra-ish-cli` and is not
  reused. A trace could go in `relatedInformation`, one entry per step at its use case.
