# Refactoring plan

## Goal

Lower the cost of reading, reviewing and running the workspace without changing what any
command prints. The plan covers six concerns:

1. Algorithm cost, including where parallel work pays off.
2. Algebraic modeling: enums that make invalid states unrepresentable.
3. File split: one concern per file, modules in the crate that owns the concern.
4. Reviewability: short functions, no copy-paste blocks.
5. Workarounds: guards for states that cannot occur, patches that undo another path.
6. Comments that restate code, narrate history, or are wrong.

The four audits this plan draws on are in this folder: `lifecycle.md`, `core.md`,
`emit.md`, `frontends.md`. Finding ids below (`L3`, `C2`, `E8`, `F1`) point into them.

## Definition of done

A unit is done when all of these hold at its head:

- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`
  pass.
- `cargo test --all` passes with the CI skip list.
- `scripts/check-sample-artifacts.sh` reports no diff: every committed sample artifact
  (diagrams, CSV, SQL, OpenAPI, TLA+, state tables, design docs) is byte-identical.
- A perf unit also shows a before/after timing on the same sample, release build, with
  the bottleneck named.

The pins are the existing suites: insta snapshots, `engine_equivalence`,
`codegen_equivalence`, `diagram_contract`, `verify_cli`, `check_cli`, and the sample
artifact manifest. No unit adds a compatibility shim or keeps an old and a new path.

## Waves

Each wave lands as its own commits. Units inside a wave touch disjoint files and run in
parallel.

### Wave 1: subtract (zero output change)

| Unit | Crate | Findings | Change |
|---|---|---|---|
| 1a | emit | E1, E2, E4, E5, E9, E12, E15, E3, E14 | Dead gates in `codegen/actions.rs`, `node_kind_filter_name` replaced by `NodeRef::kind_tag`, one CRUD-matrix emitter and one writer epilogue in `csv.rs`, stale comments |
| 1b | core | C2, C8, C9, C10 | Delete the write-only `typed_predicates` shadow model, dead slotmap guards, `ConditionEdge`, wrong and restating comments |
| 1c | lifecycle | L1, L3, L6, L9, L10, L15, L21 | Delete the test-only component-exploration path (tests move to `explore_targets`), one `named_params`, banners, duplicated rules |
| 1d | cli, lsp, syntax | F4, F5, F6, F9, F16 (comment only), F19 | One PATH lookup, one TLA output-path resolver, one load-and-reject helper, one `diagram` argument struct, false AST comments |

### Wave 2: move (pure file split)

| Unit | Findings | Change |
|---|---|---|
| 2a | L13 | `lifecycle/reduce.rs` → `reduce/{fold, extended, partition, split, lift}.rs` |
| 2b | E7, E8 | `mermaid.rs` emitters into one file per diagram kind; `codegen/actions.rs` into lowering, predicate lowering, simplify, dispatch |
| 2c | F2, F10 | TLC adapter modules (`property_cone`, `tlc_env`, `tlc_output`, `tlc_replay`) into a library module; `state_findings.rs` out of `list_output/` |
| 2d | C14 | Index advisor out of `model/` into `analysis/` |

### Wave 3: cost (measured)

Baseline first: release build, `time` on `verify`, `list`, `diagram --all` for the largest
samples (`insurance-claim-ops`, `clinic-ops`).

| Unit | Findings | Change |
|---|---|---|
| 3a | F1, E20 | Run the lifecycle search once per command and share the `TransitionSystem` |
| 3b | C1 | `ImportScopes::scope_for` returns a reference instead of cloning the scope |
| 3c | C4, C5, C6 | Relation lookup maps built once; `infer_usecase_transactions` computed once |
| 3d | L2, L7, L8, L11, L12, L22 | Cache the sub-lifecycle in `lift`; precompute `reads_successor` and input bits per action |
| 3e | F14, F15 | Parse files in parallel; build verify cones in parallel. Only if 3a–3d leave parsing or planning on the critical path |

### Wave 4: types

| Unit | Findings | Change |
|---|---|---|
| 4a | C7, C15, C16 | `Predicate` and `ArgKind` enums replace the string-keyed catalog; declaration kinds as an enum |
| 4b | L4, L5, L18, L20 | `Fold.node` into `FoldKey::Linked`; named result for `reduced_with`; candidate status enum in `prove` |
| 4c | L19 | `Effect::Transition` and `Choose` arms as one vector of structs (core and emit) |
| 4d | F3, F7, F18 | `Decided`/`Undecided` for verify runs; deadlock exemption by diagnostic type; one invariant enum |
| 4e | E10, E11, E17, E19 | `Option` instead of empty-string and `usize::MAX` placeholders; declared types by key; scope visibility type; diagram kind enum |

### Wave 5: decompose

| Unit | Findings | Change |
|---|---|---|
| 5a | E16 | `codegen::function` (860 lines, 12 params) into a builder |
| 5b | L14, L16 | `Extended::build` by phase; one `Pred` term visitor |
| 5c | C11, C12, C13 | `system_diagnostics`, `process_relate_predicate`, one duplicate-definition pass |
| 5d | F11, F13 | LSP handler prologue in one place; code lens without quadratic scan |

## Out of scope: needs the owner's decision

These change output or accepted input. Each gets its own issue, not a refactoring commit.

- C3: diagnostic order follows `HashMap` iteration in `system.rs` and `access.rs`. Output
  can differ between runs. Fix: iterate in id order.
- C4 (part): `reachable_from_buc` returns an empty closure when a BUC id is shared with
  another kind.
- C19: processing continues after a type mismatch, so ill-typed relations enter the model.
- C20: temporal-property atoms keep unresolved names and guess.
- C21: four definitions of "the use cases of a BUC" disagree.
- C22: single-entity rules split conditions in two lists; cross-entity rules use one enum.
- F8: `diagram --all` swallows every render error.
- F12: LSP refresh can publish stale analysis.
- E22: ER relations sort by the `Debug` text of slotmap keys, so entity 10 comes before 2.

## Kept on purpose

- `syntax/removed.rs`: a table of migration messages for removed forms, not legacy code.
- The hand-rolled thread pool and BFS storage in `lifecycle/explore`.
- No printer trait over the codegen IR: `ir::DomainModel` is already the shared form.
