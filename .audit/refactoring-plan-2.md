# Refactoring plan, round 2

<!-- derived-from ./refactoring-plan.md -->

## Goal and bar

Round 2 audits the code written after the first audit, decides each row the first round
deferred, and orders the work into PR-sized units. It does not change code.

The bar is the first plan's [definition of done](./refactoring-plan.md#definition-of-done):
no behavior change, every committed sample artifact byte-identical, no shim, no old and new
path kept side by side.

Codegen output is not part of any committed sample artifact (`samples/*/out` holds only
`.mmd`, `.json` and `states_*.txt`). A codegen unit therefore proves itself the way step 5a
did: run `codegen ts|rust|go`, plain and with `--allow-failing`, over every `samples.txt`
root with a base binary and a head binary, and diff stdout, stderr, exit code and files.

## Base and scope

- The first audit read base `401fb82f`. Its plan was added in `75b445d6`, whose parent is
  `401fb82f`. Main gained only `9d6b98df` (nix flake) between that base and the merge of
  PR #151 (`e7a8ace1`), and that commit touches no crate. So nothing landed unread.
- Round 2 covers `git diff e7a8ace1 e784d02a -- crates`: 421 files, +76,637 / -7,459 lines.
- Line numbers are against `e784d02a`. A citation marked `@396` was read on PR #396's head
  `b1f4dd30`. #396 then merged, and main is now `b1f4dd30`, so `@396` lines are main's
  lines. #396 changed only emit `codegen/` and the cli codegen tests; every other citation
  also holds on main. `conformance/mod.rs:483` (G13) was re-checked on main.
- Four read-only audits ran in parallel: codegen; cli check/verify and cli tests; core
  `lifecycle/explore` and `reduce`; LSP, other new core code, and the deferred rows. I
  re-read every file:line cited in this plan, and the three facts the owner list rests on
  (X6's design-doc lines, X13's `docs/cli-reference.md:1935-1936`, V11's two
  `OnDeadlock::Refuse` sites).

## In-flight work: files wave 1 must not touch

Built from the branch diffs (`git diff --stat HEAD...<branch> -- crates`), not from memory.

| Work | Files |
|---|---|
| Native temporal Phase 4 (`origin/native-temporal/phase-4`) | cli `property_cone.rs`, `verify/{bfs,compare,jobs,mod,native,tlc}.rs`; cli tests `engine_corpus.rs`, `engine_equivalence.rs`, `verify_cli.rs`, `cone_ladder.rs` (new), `engine_corpus/*.rdra`; core `lifecycle/{cone,trace}.rs`, `lifecycle/explore/properties.rs`, `lifecycle/tests/{checks,cone,events,explored}.rs`; the `symmetry__snapshot__group_rules.snap` snapshot |
| Phase 5 (`origin/native-temporal/phase-5`) | cli `verify/native.rs`; cli tests `engine_corpus.rs`, `engine_equivalence.rs`, `verify_cli.rs`; core `lifecycle/explore/{liveness,mod,properties,reduced_lasso}.rs`, `lifecycle/mod.rs` |
| #323 (`origin/tlc-batch-verify`) | cli `memory.rs`, `tlc_batch.rs`, `verify/{state_models,tlc_tool}.rs`; cli tests `engine_equivalence.rs`, `verify_cli.rs` |


#396 merged while this plan was written, so it no longer blocks anything. None of the
three branches touches `.config/nextest.toml`.

## Findings

Ids are new and do not overlap the first round: `G` codegen, `V` cli check/verify, `X`
core explore and reduce, `P` LSP and other core, `T` test helpers. Obj numbers follow the
first plan; 7 is naming across the three codegen languages, 8 is test helpers copied
across test binaries. "Inferred" means read from code, not measured. "Guess" means not
confirmed.

### Codegen (emit `codegen/`, cli `codegen_cmd.rs`)

| Id | Finding | Evidence | Obj | Size | Kind |
|---|---|---|---|---|---|
| G1 | The True/False/Not/And/Or junction is written in all three printers. `fails` is copied three times. Rust has two one-item And/Or arms that `simplify` makes dead (inferred). This is first-audit E13, still open. Round 1 declined a shared junction because it adds a callback layer; that reason stands. Wave 1 does only `fails`, `simplify` and the dead arms. | `ts.rs:1292`@396 `Expr::True => "true".to_string(),`; `go.rs:1412`@396 (same line); `rust.rs:1348`@396 `Expr::And(items) if items.len() == 1 => go(&items[0]),`; `fn fails(e: &super::ir::Expr) -> super::ir::Expr {` at `ts.rs:1436`@396, `rust.rs:1518`@396, `go.rs:1575`@396 | 4, 5, 7 | S | BP |
| G2 | `snake` lives in the lowering module. `codegen::snake` is a wrapper around it. The other identifier helpers live in `mod.rs`. | `actions.rs:638`@396 `pub(super) fn snake(id: &str) -> String {`; `mod.rs:142` `pub fn snake(id: &str) -> String {` | 3, 7 | S | BP |
| G3 | Placeholders stand for "not known yet". `Source.ty` is first-audit E10, still open; round 1 skipped it because `Option` adds branches at the readers, and that stands. `SumDecl.kind` is new and can be built after the name is claimed, with no new branch. | `actions.rs:407`@396 `ty: String::new(),`; `mod.rs:769` `kind: String::new(),` (overwritten at `mod.rs:592`) | 2 | S | BP |
| G4 | `Expr::EnumEq.enum_ty` and `Chosen.enum_ty` copy a type the field already holds. Nothing makes them agree. | `pred.rs:275`@396 `enum_ty: enum_ty.clone(),` | 2 | S | BP |
| G5 | `functions` is 304 lines with six phases. `FnDecl.chosen` is filled in after `function()` returns, and a failed lookup `continue`s silently (possible latent bug, guess). | `actions.rs:297`@396 `fn functions(`; `actions.rs:551`@396 `continue;` | 4, 2 | M | BP |
| G6 | The input name `{entity}_{column}` is built by four separate `format!` calls. `Frame::next_read` works only because two of them agree. | `lower.rs:801`@396 `let name = format!("{}_{}", snake(&entity), field.wire);`; `lower.rs:1158`@396 | 2, 4 | S | BP |
| G7 | `FieldType::Rows` is valid only on a function input, but every `Field` can hold it. Five `unreachable!` arms exist only for it. | `ir.rs:61`@396 `Rows(String),`; `go_port.rs:762` `FieldType::Rows(_) => unreachable!("a list of rows is no port argument"),` | 2 | M | BP |
| G8 | `harness: bool` beside `port: Option<PortDecl>` allows harness-without-port. TS calls `print_contracts` from `ts.rs`; Go and Rust call it inside `print_port`. | `ir.rs:211`@396 `pub harness: bool,`; `ir.rs:220`@396 `pub port: Option<PortDecl>,`; `ts.rs:79`@396 `super::ts_port::print_contracts(domain, &mut body);` | 2, 7 | S | BP |
| G9 | Three copies of `params` walk the port arguments in the same order. The port IR uses `(String, String)` tuples throughout. | `go_port.rs:723` and `rust_port.rs:585` `fn params(m: &super::ir::PortMethod) -> Vec<String> {`; `ts_port.rs:14` `fn params(m: &PortMethod) -> Vec<(String, String)> {` | 4, 2 | S–M | BP |
| G10 | The prepare/commit/run method names are written in emit and again in cli `code_check`. The emit helpers have different names per language (`entry_method`, `entry_name`, none for Rust). | `go_port.rs:148` `pub(super) fn entry_method(`; `ts_port.rs:173` `pub(super) fn entry_name(`; `code_check/rust.rs:97` `vec![format!("prepare_{snake}"), format!("commit_{snake}")]` | 7, 3 | S | BP |
| G11 | The printers are long and name the same phases differently. `print_functions` is 408–462 lines in each language; each `print_port` is 379–462. TS prints errors inline; Go and Rust have `print_errors`. Rust uses singular `print_scalar`, Go plural `print_scalars`. | `rust.rs:831`@396 `print_functions` (462 lines); `go_port.rs:260` `print_port` (462 lines); `ts.rs:890`@396 `for e in &domain.errors {` | 4, 7 | L (moved; ~0 net) | BP |
| G12 | Small copies: id repr lookup ×2, integer comparison ×2 (`derived.rs` vs `conformance/eval.rs::compare`), probe prologue ×3, `format!("{}_expected", ..)` ×6. | `go_port.rs:749` `.map_or(Scalar::Int, \|i\| i.repr)`; `go_port.rs:185` `let port = domain.port.as_ref()?;` | 4 | S | BP |
| G13 | `--conformance` builds the transition system twice in one `generate_all`. Same theme as E20. | `mod.rs:608` `let built = rdra_ish_core::lifecycle::TransitionSystem::build(`; `conformance/mod.rs:483` `let built = TransitionSystem::build(model, Bounds::STATES, &Scope::ALL);` | 1 | S–M | BP, measure first |
| G14 | Go and Rust each carry a fallback error name that cannot run, and the two fallbacks disagree. | `go.rs:826`@396 `.unwrap_or_else(\|\| format!("{}Error", super::type_ident(usecase)))`; `rust.rs:890`@396 `.map_or_else(\|\| "InvalidArgument".to_string(), \|e\| e.name.clone());` | 5, 7 | S | BP |
| G15 | The three printers declare different sets of use-case errors when some use cases have no function. A TS entry-only model may name an undeclared error type (guess, no snapshot has entries). | `rust.rs:860`@396, `ts.rs:840`@396 and `go.rs:990`@396, each `if domain.fns.is_empty() {` with different bodies | 7 | — | DECISION |
| G16 | `codegen_cmd.rs` also holds the generated-file stamp check and path helpers. `one_line` there means "first line"; `codegen::one_line` means "joined into one line". | `codegen_cmd.rs:259` `pub(crate) fn label(`; `:271` `pub(crate) fn shown(`; `:578` `fn one_line(message: &str) -> String {` | 3, 7 | S–M | BP |

### cli check and verify

| Id | Finding | Evidence | Obj | Size | Kind |
|---|---|---|---|---|---|
| V1 | `GoBackend` passes findings between calls in two `RefCell`s. `classify` is right only if compile calls `probe` and `parse_messages` in that order. | `code_check/go.rs:26` `missing: RefCell<Vec<String>>,`; `:28` `methods: RefCell<Vec<(String, Vec<String>)>>,` | 2 | S–M | BP |
| V2 | Two lexical path normalizers and two `relative` helpers differ at the edges (`"."` vs `""`, when to pop). | `code_check/go.rs:53` `fn normalize(path: &Path) -> PathBuf {`; `code_check/ts.rs:51` `fn normal(path: &Path) -> PathBuf {`; `relative` at `go.rs:69` and `ts.rs:27` | 4 | S | BP |
| V3 | `code_check::run` is 145 lines and calls `extra_files` twice per target. | `code_check/mod.rs:357` `pub(crate) fn run(`; `:395` and `:405` `backend.extra_files(&t)` | 1, 4 | S–M | BP |
| V4 | `check` and `states` repeat three cap flags with the same help text. | `cli.rs:185` `max_patterns: Option<usize>,`, `:189`, `:194`; again at `cli.rs:354`, `:358`, `:363` | 4 | S | BP |
| V5 | `End` lists the failure counters twice, in `result()` and `finish()`. | `verify/end.rs:57` `let tool = self.temporal.faults + self.tlc.disagreements + self.tlc.tool_errors;` | 4 | S | BP |
| V6 | `plan.rs` re-derives the heap size, the rank and the TLC arguments that `tlc.rs` launches with. The rank has already diverged (harmless today because `target` ignores it). | `verify/plan.rs:145`, `:151`; `verify/tlc.rs:693`, `:697` | 4, 2 | S | BP, after Phase 4 |
| V7 | `cli/memory.rs` holds TLC heap sizing beside the command memory limit. The worker cap 8 is written twice. | `memory.rs:254` `false => (cores / in_flight.max(1)).clamp(1, 8),`; `verify/schedule.rs:131` `pub(super) const WORKER_CAP: usize = 8;` | 3, 4 | M | BP, after #323 |
| V8 | `schedule.rs` has two schedulers with the same wait rule. The job reorder after a run is O(n²). | `schedule.rs:48` `while held.runs > 0 && held.bytes.saturating_add(bytes) > self.bytes {`; `:445`; `:378` `done.sort_by_key(\|(job, _)\| jobs.iter().position(\|j\| j == job));` | 4, 1 | S (sort) / M (merge) | sort BP; merge after #323 and a DECISION |
| V9 | `write_whole_specs` scans its own index text to learn what it wrote. | `verify/specs.rs:78` `let listed = format!("{}.tla: ", whole_modules[i]);` | 2, 4 | S | BP |
| V10 | `print_plan` prints the run header; `write_plan` is `--plan`. | `verify/report.rs:34` `pub(super) fn print_plan(`; `verify/plan.rs:117` `pub(super) fn write_plan(` | 7 | S | BP, after Phase 4 (called from `verify/mod.rs:216`) |
| V11 | `OnDeadlock::Refuse` has no production user. Only a test stub reaches it. | `lifecycle_gate.rs:196` `OnDeadlock::Refuse => Reads::States,` | 5 | S | DECISION |

### core `lifecycle/explore`, `reduce`, helpers

| Id | Finding | Evidence | Obj | Size | Kind |
|---|---|---|---|---|---|
| X1 | `lift` repeats the partition code of `Classes::of_graph` line for line, and re-explores the sub-lifecycle on every call (first-audit L2, still open; `lift_cycle` calls it in a doubling loop). | `reduce/lift.rs:59-60` and `reduce/partition.rs:405-406`, both `let (silent, rise_edges) = split_edges(run, &ext.rise);` / `let blocks = quotient_blocks(n, &silent, &rise_edges, &initial);`; `lift.rs:43` `plain = sub.clone().run(Caps::new(usize::MAX, usize::MAX));` | 4, 1 | S (dedupe) / M (cache) | BP; cache measure first |
| X2 | `Reduction` deep-copies the cached quotient tables. Two structs carry the same fields and two functions count their bytes. | `reduce/mod.rs:557` `classes.reps.clone(),`; `reduce/cache.rs:576`, `:591` `tables_bytes(` | 1, 2 | M | BP, measure first |
| X3 | `Steps` states "has an edge list" twice. One combination is `unreachable!`; another reads as "no parent". | `explore/edges.rs:229` `edges: Option<EdgeList>,`; `:230` `parents: Parents,`; `:343` `(None, Parents::Edge(_)) => unreachable!("a run without edges keeps its steps"),` | 2 | S–M | BP |
| X4 | `Bfs::take` is about 120 lines. Four stop paths repeat one block. | `explore/bfs.rs:703` `fn take(`; arms at `:771` `Probe::Hit(to) => {`, `:783` `Probe::SameAs(k) => {` | 4 | S | BP |
| X5 | `moves/reference.rs` is named as a twin but holds production code (`bindings`, `bindings_for`, `pre_guards_hold`). One test re-checks what the debug twin already asserts. Twins use four gating policies and `shadow.rs` does not say which. | `explore/moves.rs:12` `mod reference;` (ungated); test at `explore/tests.rs:55` | 3, 5, 8 | S | BP |
| X6 | The shared-table probe path (`table.rs`, `Index::Shared`, `Probe::Hit`/`Shared`, `Recent`; about 500 lines) is off in every release build. The design doc says part 3 kept it off. | `explore/bfs.rs:57` `#[cfg_attr(not(test), allow(dead_code))]` on `Probing::On` | 5 | L if deleted | DECISION |
| X7 | `QuotientCache` uses `Option<u64>` as a state tag. Two key structs repeat three fields. Find-or-insert is written twice. | `reduce/cache.rs:260` `declined_in: Option<u64>,`; `:104` and `:207` `charged: bool,` | 2, 4 | S | BP |
| X8 | Byte-accounting helpers are copied verbatim between `store.rs` and `cache.rs`. | `explore/store.rs:13` and `reduce/cache.rs:526` `fn block(n: usize) -> u64 {`; `store.rs:126` `fn buckets(n: u128) -> u128 {` vs the expression at `cache.rs:533` | 4 | S | BP |
| X9 | `Pred` walks written by hand after L16 added `nodes()`/`terms()`. `perf.rs` re-implements the Fx hash step. `moves.rs` has three `#[cfg(feature = "perf")]` pairs. | `explore/plan.rs:179` `fn reads_one_row(`; `explore/perf.rs:190` `fn walk(`; `perf.rs:103` `let fx = \|h: u64, w: u64\| ...` | 4 | S | BP; the `perf` off stub waits for Phase 5 (`explore/mod.rs:14-15` gates `mod perf;`) |
| X10 | The candidate-slot rule per `ParamRole` is written three times. | `explore/moves.rs:173` `ParamRole::Existing \| ParamRole::Parent { .. } \| ParamRole::Triggered => {`; `moves/reference.rs:42`; `scan.rs:40` | 4 | S | BP |
| X11 | `u32::MAX` means "no number", and that works only because the lookup is out of bounds. | `explore/probe.rs:248` `return u32::MAX;` consumed at `bfs.rs:403` | 2 | S | BP |
| X12 | Stale and narrating comments. The `reads_successor` comment is stale since #277. | `lifecycle/types.rs:602` `/// The explorer asks this of every guard in every state, so it walks the tree in place:`; `explore/edges.rs:6` | 6 | S | BP |
| X13 | Release builds read four measurement env vars, and `docs/cli-reference.md` documents them. | `reduce/partition.rs:342` `std::env::var_os("RDRA_ISH_S_UP_TO_H_STATS")`; `cache.rs:480`; `partition.rs:623`; `symmetry/mod.rs:283` | 5 | S | DECISION |

### LSP and other core code

| Id | Finding | Evidence | Obj | Size | Kind |
|---|---|---|---|---|---|
| P1 | "Did this run stop at a cap" is written five times over `StateDiag`. | `lifecycle/targets.rs:172` `fn stopped_at(`; `lifecycle/guards.rs:173` `fn cap_of(`; `lifecycle/view.rs:229` `fn is_cap(`; `lsp/src/lifecycle.rs:275` `fn is_stop(` | 4, 3 | S | BP |
| P2 | The LSP spells out the source site of every diag kind in a 34-arm match. Two `matches!` lists beside it fail silently on a new kind. Core should own `StateDiag::sites()`. | `lsp/src/lifecycle.rs:358` `fn diag(&self, diag: &StateDiag, entity: Option<&str>)`; `of_its_entity` `:287`, `names_no_entity` `:295` | 3, 6 | S–M | BP |
| P3 | LSP state ties the published diagnostics and the lifecycle findings to the analysis by hand. `publishing: Mutex<()>` guards no data. | `lsp/src/backend.rs:38` `analysis_published: HashMap<...>`; `:40` `lifecycle: Option<Findings>,`; `:87` `publishing: Arc<Mutex<()>>,` | 2 | M | BP |
| P4 | A `catch_unwind` inside `spawn_blocking`. The `JoinError` already carries the panic. | `lsp/src/backend.rs:777` `std::panic::catch_unwind(std::panic::AssertUnwindSafe(\|\| {` | 5 | S | BP |
| P5 | Each `TransitionSystem::build` computes `infer_usecase_transactions` twice and `applied_bodies` once per use case. Feeds C6. | `lifecycle/build/actions.rs:21` `let mut txs = infer_usecase_transactions(model);`; `build/vars.rs:19` `for utx in infer_usecase_transactions(model) {` | 1 | S–M | BP, measure first |
| P6 | `explore_targets_with` is about 245 lines with six phases. | `lifecycle/targets.rs:258` `pub(super) fn explore_targets_with(` | 4 | M | BP |
| P7 | Backward and forward graph walks are copied between `reach.rs` and `findings.rs`. | `lifecycle/reach.rs:50` `for &from in adjacency.preds(at) {`; `lifecycle/findings.rs:460` `for &p in adjacency.preds(at) {` | 4 | S | BP |
| P8 | Two predicates are copied in `entity_rules.rs`. | `build/preds/entity_rules.rs:207` and `:336` `matches!(eff, Effect::Delete { param } if a.params[usize::from(*param)].entity == ...)` | 4 | S | BP |
| P9 | `MemoryPool::with_room` charges one level up. A pool carved from a carved pool would not charge the root. Every caller carves from a root today (inferred). | `core/src/memory.rs:200` `parent.held.fetch_add(bytes, Ordering::Relaxed);` | 2 | S | BP |
| P10 | The LSP repeats the cli's model-hash expression and the generated-file list. | `lsp/src/code_fingerprints.rs:25` `let model = fingerprint::model_hash(...)`; `cli/src/codegen_cmd.rs:75` | 3 | S | BP |
| P11 | `run_budgeted` is `in_parallel` plus a budget hold. | `verify/schedule.rs:94` `pub(super) fn run_budgeted<R: Send>(`; `core/src/parallel.rs:97` `pub fn in_parallel` | 4 | S | BP, after #323 |
| P12 | `one_at_a_time` is an ambient thread-local. It is correct today; a future `thread::spawn` would escape it. | `core/src/parallel.rs:25` `static ONE_AT_A_TIME: std::cell::Cell<bool>` | 2 | M | BP, after Phase 5 |
| P13 | `Domain` doubles each variant for nullability, and `has_null()` leaves out `Instance { optional: true }`. | `lifecycle/types.rs:220` `pub enum Domain {` (91 matches in 18 files) | 2 | L | BP, after #396 and wave 1 |

### Test helpers

| Id | Finding | Evidence | Size | When |
|---|---|---|---|---|
| T1 | cli test binaries copy the same helpers: `repo()` in 20 binaries (11 canonical, 9 plain), `scratch(test)` ×7, `samples()` ×6, `edit` ×4, `cut` ×2, `text` ×2 + ×2, a tool-skip assert ×7. `codegen_builds.rs` defines `repo`/`samples` beside `mod codegen_cases;`, which already has both. | `verify_cli.rs:16` and `memory_cli.rs:11` `scratch`; `codegen_builds.rs:19`@396 vs `codegen_cases/mod.rs:124`@396 | M (~-300) | Wave 1 for the binaries not in flight; the rest later |
| T2 | LSP tests: hand-rolled `initialize` ×2, `didChange` without wait ×4, temp-dir cleanup skipped on assert failure in 10 tests (count of `remove_dir_all`), `wait_for_findings` re-implements `common::wait_for_diagnostics`. | `lsp/tests/manifest_error.rs:30`; `lsp/tests/lifecycle_check.rs:67` `fn wait_for_findings(` | S | Wave 1 |
| T3 | core lifecycle tests: `names`/`strings` ×3, entity-by-id lookup ×5, property-by-name ×2, `TransitionSystem::build(&model_from(src), ..)` ×44. | `tests/lasso.rs:106` and `tests/liveness.rs:41` `fn names(xs: &[&str]) -> Vec<String> {`; `tests/explored.rs:86` `fn strings(` | S | Wave 1 except `explored.rs`, `checks.rs` |

## First-round rows: decisions

The first plan deferred C4–C6, L2, L7, L8 and E20 "until a profile shows them", and
skipped C13, part of C15, part of C16, E10 `Source.ty`, L11, the E13 junction and part of
F13. Unit 3e scheduled F15 only "if 3a–3d leave planning on the critical path", and only
F14 landed. C17, C18, C23, C24, L17, L23, E6, E21 and F17 were never scheduled. This round
may not run benchmarks, so no perf row moves to "do" on a guess.

| Row | Now | Decision | Reason |
|---|---|---|---|
| C3 | Fixed by `661c2902` (`system.rs:408` `fn by_id`) | Drop | Done |
| C4 | Mostly fixed by `43c0a9d8` (`resolver.rs:498` builds `forward` once; `:481` looks up by kind). Two relation scans per BUC remain (`resolver.rs:526`). | Drop the row; the remainder goes with C5 | The costly part and the ambiguity fix are done |
| C5 | Open. `access.rs:731` `if triggered_use_cases(model).contains(&usecase) {`; `system.rs:362` O(R²) | Keep deferred, first in the profile pass | Cost grew (E21 now calls `applied_bodies` per transition), still unmeasured |
| C6 | Open, 8 production callers (2 inside every `TransitionSystem::build`, see P5) | Keep deferred, with P5 in the profile pass | Unmeasured |
| C13 | Ghost node fixed by `86c15e5d`. `seen_ids` stays at `resolver.rs:368`. | Drop | Step 5c kept the pre-pass on purpose: removing it reorders output |
| C15 rest | Open. `location.rs:158` `format!("{kind}:{id}")`; `lint/mod.rs:42` `pub subject_kind: String,`; `lint/mod.rs:196` `is_consistency_diag` lists 23 variants; new string kinds at `lsp/src/lifecycle.rs:344` `self.declaration("entity", id)` | Split. Do the `DeclIndex` key `(Kind, String)` and the new LSP string kinds (6l). Keep `is_consistency_diag` and `LintIssue.subject_kind` as they are. | Round 1 skipped them because the unit did not own both crates and a category table adds a table. 6l owns core and LSP; the key change removes a `format!`/`split_once` pair and adds no table. The table reason still holds for `is_consistency_diag`. |
| C16 rest | Inline-enum labels skipped in step 4c (changes type equality) | Keep skipped | Round-1 reason stands |
| F13 rest | Step 5d did part; the rest was not re-checked | Keep deferred | Not post-audit code |
| C17 | Open (`analysis/mod.rs:32` clones each AST item) | Keep deferred, profile pass | Unmeasured |
| C18 | Open (`entity_lifecycle.rs:34` per-state recompute) | Keep deferred, profile pass | Unmeasured |
| C23 | Open, grown: 29 top-level core modules (was 22) | Keep deferred until Phase 4/5 land | A module move now collides with `lifecycle/mod.rs` |
| C24 | Parse part done by `fc532b3b` (`resolver.rs:77` `in_parallel`) | Drop | Done; the rest was "not warranted" |
| L2 | Open (`reduce/lift.rs:26`, `:43`) | Split: do the partition dedupe (6g); keep the lift cache deferred | The dedupe is BP and cheap; the cache is perf and unmeasured |
| L7 | Fixed on the hot path by `00ce20d5` (#277): `explore/plan.rs:94` asks once per action | Drop; fix the stale comment (X12) | Done |
| L8 | Mostly fixed by `bb656c67` (#271). One double `push_bits` per state remains (`reduce/mod.rs:760` then `:764`). | Drop the row; the remainder goes into the profile pass | Done except a small residue |
| L11 | Open (`partition.rs:455`) | Drop | Step 5b: an index costs more than the scan |
| L17 | Open, now at two sites (`reduce/extended.rs:266`, `:314` `let signature = format!("{:?}", (entity, link_var, body));`) | Do, in 6g | BP, small |
| L23 | Open (`reduce/extended.rs:118` `unreachable!("a bit is a run mark")`) | Do the minimal form, in 6g | BP, small |
| E6 | Open (`emit/src/lib.rs:526` `collect_object_graph_nodes`, ~25 blocks; ADR patch at `mermaid.rs:191`) | Keep deferred | Not post-audit code; low value next to wave 1 |
| E10 | `RowParam.param` and `Lit::Enum` done; `Source.ty` remains (`actions.rs:407`@396) | Keep skipped | Round-1 reason stands: `Option` adds branches at readers that already skip those params. 6b fixes only the new `SumDecl.kind` placeholder. |
| E13 rest | Junction and `fails` still copied (G1) | Split. Do `Expr::fails`, `Expr::simplify` and the dead Rust arms (6a). Keep the junction per printer. | The junction needs a print callback, which round 1 declined. `fails` and `simplify` need none. |
| E20 | Open, 8 production builds (sql, codegen, diagram_common, conformance, export, guard_report, prove, states) | Keep deferred, profile pass, with G13 | Unmeasured |
| E21 | Open, worse (`mermaid/state.rs:283` `fn transition_label` reaches `applied_bodies` per transition) | Keep deferred, profile pass | Unmeasured |
| E22 | Fixed by `42818bc4` (`mermaid/er.rs:64`) | Drop | Done |
| F15 | Partly done by `ec3029f2` (`verify/jobs.rs:173` prints in parallel). The reduction stays serial on purpose (`verify/jobs.rs:159-162`). | Drop | The rest is deliberate, and the file is in flight |
| F17 | Open (`syntax/src/format.rs:381` and 5 siblings, 47 `meta_value` lines) | Keep deferred | Not post-audit code |

F8, F12, C19–C22 and the step-4c/5c output-changing items were not re-checked. They stay
with the owner (see below).

## Wave 1

Ranked by value over risk. Every unit avoids the in-flight files above. Units that may
run at the same time touch disjoint files; a unit that shares a file with another lists
it under "after".

| Rank | Unit | What moves | Why | Size | Risk | Proof of no change | After |
|---|---|---|---|---|---|---|---|
| 1 | 6a codegen subtract | G1 part (`Expr::fails` and `Expr::simplify` into `ir.rs`, drop the two dead Rust arms), G2, G12, G14, G6 (`input_name`) | three copies of `fails`, two dead arms, a lookup that works only because two `format!` calls agree | S | low (the dead arms are an inference; the snapshots check it) | 6 language snapshots + `json_snapshot`; `codegen_equivalence`, `codegen_differential`; `codegen_builds` in CI with `RDRA_REQUIRE_CODEGEN_TOOLS=1`; base/head codegen diff over `samples.txt` | — |
| 2 | 6f explore subtract | X5, X9 (without the `perf` off stub), X10, X11, X12 | production code filed as a twin, hand-written walks next to `nodes()`/`terms()`, an unnamed sentinel, a stale comment | S | none | `bfs/tests.rs`, `plan_tests.rs`, `explore/tests.rs`; `check-sample-artifacts.sh`; `scripts/engine-diff.sh` base vs head | — |
| 3 | 6h `StateDiag` owns its facts | P1 (`StateDiag::stopped_why`), P2 (`StateDiag::sites`), P6 (split `explore_targets_with`), P7, P8, P9 | five copies of one cap test; the LSP spells out a core fact in 34 arms; a 245-line function | M | low (P6 keeps its signature; in-flight tests call it) | core `lifecycle/tests/{budget,guards,reach,delete_restrict,liveness,reduce,vacuous}.rs`; `lsp/tests/lifecycle_check.rs`; sample artifacts; `engine-diff.sh` | — |
| 4 | 6i cli test helpers | T1 for every cli test binary except `engine_equivalence`, `verify_cli`, `engine_corpus`, `cone_ladder`. Topic files under `tests/common/`, included per binary by `#[path]`. Each binary keeps its own tool probe. | about 300 copied lines; `codegen_builds.rs` shadows the `repo`/`samples` of the module it already includes | M | low | each binary passes; test count per binary unchanged; CI with `RDRA_REQUIRE_*=1` shows no new skips | —; done: #401 |
| 5 | 6j verify small | V4 (`CapArgs` flatten), V5, V9, V8 sort fix only | the same flags and counters written twice; a scan of its own output | S | low | `verify_cli` and `check_cli` pass unchanged; `--help` of `check`, `states`, `verify` byte-identical | —; done: #403 (V4, V5, V8 sort; V9 skipped: needs `verify/{mod,tlc}.rs`) |
| 6 | 6c code_check cleanup | V1, V2 + G16 (one `cli/src/paths.rs`, a `generated_file.rs`, `first_line`), V3, P10 | state passed between calls in `RefCell`s; two path normalizers that differ at the edges; `extra_files` twice | M | low (the `"."` edge case when the two `relative` merge) | `codegen_check`, `codegen_code_check{,_go}`, `codegen_code_build{,_ts}`; LSP `code_fingerprints` tests | —; done: #406 (G16, V2 `relative` only, V3, P10 hash only; V1 and the V2 normalizers skipped) |
| 7 | 6b codegen types | G3 (`SumDecl.kind` only), G4, G7 (`InputType`), G8 (`Option<Port>`; TS `print_port` calls `print_contracts` like Go and Rust), G9 (`Arg`, `PortMethod::args()`) | five `unreachable!` arms, a flag that must agree with an `Option`, tuples documented by position | M | low–med (`json.rs:147` must keep `{"kind":"rows"}`) | as 6a | 6a |
| 8 | 6g reduce cleanup | X1 dedupe only (one `partition` fn for `of_graph` and `lift`), X7, X8, L17, L23 | 13 lines copied verbatim; an `Option` as a state tag; two verbatim byte helpers | S | low (X8: the byte formulas decide where a capped run stops; move, do not change) | reduce tests (`a_class_move_of_a_folded_action_binds_the_cones_params` and the lift tests named under L2), `cache.rs` tests, `store_bytes_*`, `memory_cli`, sample artifacts, `engine-diff.sh` | — |
| 9 | 6k explore types | X3 (`Steps` enum, `edges.rs` only), X4 (`resolve` and `stop` out of `take`) | an `unreachable!` pair of fields; a 120-line function with four copies of one stop | S–M | low | as 6f; Kani store proofs; `memory_cli` | 6f |
| 10 | 6m LSP state | P3, P4, T2 | the lock guards no data; three fields reset by hand; a second `catch_unwind` | M | low (keep the lock order between `state` and `publishing`) | all `lsp/tests`, both "burst of edits" tests | — |
| 11 | 6d codegen `functions` | G5: pass the chosen targets through `Frame`, split `functions` into named phases, one `event_decl` helper. Keeps today's silent skip on a failed lookup. | a 304-line function; `chosen` attached after construction | M | low–med (function names and `chosen` order are output) | as 6a | 6b |
| 12 | 6l declaration kinds | C15 split: `DeclIndex` keyed on `(Kind, String)`; `Kind` instead of strings at `lsp/src/lifecycle.rs:344` and `:352` | a typo in a kind string drops a location silently | M | low | lint tests, LSP definition tests, `check`/`lint` byte diff over `samples.txt` | 6h |
| 13 | 6e printer phases and names | G11 (same phase names in all three languages, one plural scheme), G10 (one method-name helper per language, exposed from emit; cli `code_check` calls it). Free functions only. | 400–460-line printer functions; the same phase named three ways; method names written in two crates | L (moved; ~0 net) | low | as 6a, plus `codegen_code_check{,_go}` and `codegen_builds` `*_probes_name_every_contract` | 6a, 6b, 6c, 6d |
| 14 | 6n core test helpers | T3, except `tests/{checks,cone,events,explored}.rs` | the same three helpers in three files | S | none | core tests pass, same count | — |

Strands that can run in parallel: codegen (6a → 6b → 6d → 6e, with 6c before 6e),
explore (6f → 6k; 6g alone), lifecycle and LSP (6h → 6l; 6m alone), tests (6i, 6n),
verify (6j).

### Profile pass (before any perf row)

One measured pass on a release build with callgrind, as step 3 did. Targets: `check`,
`lint`, `states`, `diagram --all`, `export --kind sql`, `codegen --conformance` and
`verify` (no TLC) on `insurance-claim-ops` and `clinic-ops`. It decides C5, C6 + P5, C17,
C18, E20 + G13, E21, L2's lift cache (X1), X2 and the L8 residue. A row moves to "do" only
when it shows up in the profile. This pass runs benchmarks, so it needs the owner's go.

## Later: after in-flight work lands

| Gate | Unit | Notes |
|---|---|---|
| Phase 4, Phase 5 and #323 all landed | 7a split `engine_equivalence.rs` (4,470 lines) | One binary with modules: `tests/engine_equivalence/main.rs` + `support.rs` (~710), `batch.rs` (~190), `samples.rs` (~710), `fixtures_guards.rs` (~780), `fixtures_paths.rs` (~940), `temporal.rs` (~620), `tlc_alone.rs` (~415). Line ranges are from the audit and will move. One binary keeps the `batch_pool()` OnceLock and `ci.yml:282` `--test engine_equivalence`. Replace the five inline TLC skips with the existing `require_tlc()`. |
| same | 7b split `verify_cli.rs` (2,300 lines) | `tests/verify_cli/main.rs` + `support.rs`, `counterexamples.rs`, `bounds.rs`, `specs.rs`, `cones.rs`, `engines.rs`, `symmetry.rs`. |
| with 7a/7b | nextest and CI entries that must move | `.config/nextest.toml` `[profile.pr] default-filter`: line 49 `(binary_id(rdra-ish-cli::verify_cli) & (` with 7 `test(=...)` names on lines 50–56; line 58 `(binary_id(rdra-ish-cli::engine_equivalence) & (` with 3 names on lines 59–61; line 57 names `engine_corpus`. Each `test(=name)` becomes `test(=module::name)`. A stale entry lets the slow test run in `pr`, and `scripts/check-test-times.py` fails main. Also `ci.yml:282` and `:288`, `mutants.yml:82`, and the `release_only` substring filters in `scripts/diff-test.sh:29-37`. Tests named to sort first (comments cite #305) need a module that sorts first, or a re-measure. #323 may add entries; re-read the file at that time. |
| same | 7c T1 for the in-flight binaries | `engine_equivalence`, `verify_cli`, `engine_corpus`, `cone_ladder` move to `tests/common/`. |
| Phase 4 | 7d V6, V10 | One `sized_heap`, `want_of` and `launch_args` in `tlc.rs`; `Rank` as a struct. Rename `print_plan` to `print_header` (its caller is `verify/mod.rs:216`). |
| #323 | 7e V7, P11, V8 merge | Move TLC sizing out of `cli/memory.rs`; `run_budgeted` on `in_parallel`; remove `#[allow(dead_code)]` on `Budget::bytes` (`schedule.rs:39`). Whether `Budget` becomes a pool is a timing DECISION. |
| Phase 5 | 7f explore follow-ups | Fold `Bfs.ends` into X3's `Parents` arm (`explore/mod.rs:249`); move `liveness_reference` beside its only caller; P12 thread count in `Caps`; X9's `perf` off stub (`explore/mod.rs:14-15` gates `mod perf;`). Guess: one generic `shortest_path` might serve `liveness.rs`, `orbit/liveness.rs` and `lift.rs`. |
| Phase 4 | 7g T3 rest | `tests/{checks,cone,events,explored}.rs`. |
| Phase 4/5 and wave 1 | 7h C23 module layout | Group the 29 top-level core modules. Pure move. |
| Wave 1 codegen units | 7i P13 `Domain { base, nullable }` | L, medium risk, touches codegen `values`/`pred`/`lower` and `conformance`. |

## Needs the owner's decision

- X6: keep the parked shared-table probe path (~500 lines, off in release) with a tracking
  issue, or delete it and keep the design doc as the record.
- X13: move the four `RDRA_ISH_*_STATS` switches under the `perf` feature. That changes
  release behavior when they are set, and `docs/cli-reference.md` documents them.
- V11: drop `OnDeadlock::Refuse`, `Reads::States` and `reject_state_errors`, or keep the case.
- G15: should all three languages declare every use-case error? Add an entry-only snapshot
  first; the TS case may be a real bug (guess).
- G5: a failed `chosen` lookup ships a function without its choice argument (guess). 6d
  keeps today's silent skip. Turning it into `Skip::Action` changes output and is a
  separate change for the owner.
- V1: whether Rust and TS should also union missing members across messages, as Go does.
- Unifying the test tool probes (`which tlc` vs PATH `is_file` vs `RDRA_Z3`) changes when
  tests skip. Wave 1 keeps each probe.
- G1: a shared junction helper with a print callback. Round 1 declined it; wave 1 keeps
  the junction per printer unless the owner wants it.
- The profile pass runs benchmarks.
- Not re-checked here, still the owner's: F8, F12 (guess: the #214/#397 generation check
  may have closed it), C19–C22, the step-4c `fk.rs` enum-label item, the step-5c ghost
  node (fixed since by `86c15e5d`).

## Not to do

- No printer trait over the codegen IR. The three languages diverge more than in round 1.
  G1 and G11 use free helpers and shared names only.
- Do not split `engine_equivalence.rs` or `verify_cli.rs` before Phase 4, Phase 5 and #323
  land. All three rewrite them.
- Do not split the monoliths into separate binaries. Each would get its own TLC batch pool,
  and `ci.yml` would need one `--test` per binary.
- Do not merge `fingerprint.rs` (FNV) with `fasthash.rs` (Fx). FNV values are part of
  generated-file stamps.
- Do not merge `arena.rs`/`table.rs` bucket formulas with `store.rs`/`cache.rs`. They start
  from a capacity, not a count, and decide where a memory-capped run stops.
- Do not replace dense `u32::MAX` sentinels in hot arrays with `Option<u32>`. It doubles
  their size. Name them instead (X11).
- Do not remove `seen_ids` (C13). It reorders output.
- Do not unify `local` (TS, Go) with `ident` (Rust). They are different rules.
- Do not move TS row printing to match Go and Rust. It changes output.
- No perf change without a profile that shows it.

## Status

| Unit | State | Notes |
|---|---|---|
| 6a | Done (lane A PR) | G1 part: `Expr::simplify` and `Expr::fails` in `ir.rs`, three `fails` copies and the two one-item Rust arms gone. G2: `snake` lives in `mod.rs`. G6: `input_name` for all six `{entity}_{column}` input names in `lower.rs`. G12: `DomainModel::id_repr` (Go and TS ports; Rust's `is_some_and` differs and stays), `CmpOp::holds` (replaces `eval::compare` and the copy in `derived.rs`), `DomainModel::port_with_contracts` (three probe prologues), `RowOut::expected_name` (six `format!`). G14: `DomainModel::error_name` expects the error type; the Go and Rust fallbacks are gone. |
| 6b | Done (lane A PR) | G3: `sum_decl` returns the sum as a closure that takes the discriminant's name, so `SumDecl.kind` is never a placeholder; names are claimed in the same order. G4: `Expr::EnumEq` drops `enum_ty`; the printers read it from the field (`Field::enum_ty`). `Chosen.enum_ty` stays: it has no field, and deriving it needs a domain lookup (sum tag or row field) at both readers, which adds a lookup and a failure branch. G7: `InputType { Value, Rows }` on `FnInput`, `FieldType::Rows` gone, port inputs are `PortInput` (a `FieldType`), the five `unreachable!` arms and two dead `Rows` arms (`is_copy`, TS `reader`) gone; `json.rs` keeps `{"kind":"rows"}`. G8: `harness` moved into `PortDecl`; `DomainModel::harness()`; TS `print_port` calls `print_contracts` as Go and Rust do. G9: `Arg { name, ty }` replaces the port's `(String, String)` tuples; `PortMethod::args()` yields `PortArg` in the one order, and the three `params` and the three harness argument readers walk it. cli `codegen_differential` follows `InputType`. |
| 6d | Done (lane A PR) | G5 (refactor part): `functions` is 76 lines (was 304) over named phases: `printable_guards`, `transition_arms`, `param_sources`, `combination_name`, `arm_options`, `arm_choices`, `choice_name`. The chosen targets reach `function` (`tag_choices` on `Frame`), and `Frame::chosen` builds `FnDecl.chosen` with the rest of the function instead of after it returns. One `event_decl` for both event declarations. The silent skip on a failed lookup is kept, as the plan says; changing it is the owner's G5 item. |
| 6e | Done (lane A PR) | G10: `method_names` and `entry_name` in each port printer, exposed as `codegen::method_names(lang, m)` and `codegen::entry_name(lang, name)`; every emit site and cli `code_check` (Rust, Go, TS backends) asks them. G11: `print_port` is `print_contracts`, `print_snapshot`, `print_port_trait`, `print_conform`, `print_calls`, `print_seed_rows`, `print_abstraction` in all three; `print_functions` is `print_events`, `print_marks`, `print_errors` (new in TS), and per function `print_result`, `print_function`. Moved verbatim; free functions only. Still long: `print_function` (TS 308, Go 343, Rust 369 lines; its closures are shared by the checks, conditions, and outputs), and TS/Go `print_conform` (one raw string). Rust's per-item `print_scalar`/`print_id`/`print_enum`/`print_sum` keep their singular names: the caller loops. |
| G5 (owner item) | Closed: does not reproduce | Each of the seven lookups in `Frame::chosen` (row of the param, sum type, tag enum, target variant; for a column: column var, row, enum field, first arm) was replaced by a panic in a throwaway build: the codegen harness over every `samples.txt` root and codegen fixture, the emit lib tests, and `codegen_equivalence`/`codegen_differential`/`codegen_behaviour`/`codegen_contract`/`codegen_conformance` reached none. By construction a tag choice needs a variant source, so the param is an existing row of a state type whose variants hold every tag value; a column choice is on an enum column with at least one arm. The new test `every_choice_a_function_is_named_for_is_chosen` keeps it so: every `To<Target>` in a name is in `chosen`, and no dispatcher has two cases with the same states and choices (a planted wrong target fails it on patent-ops). |
