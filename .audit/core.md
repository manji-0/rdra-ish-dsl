# rdra-ish-core audit (outside lifecycle/)

Goal: a behavior-preserving refactoring plan for `crates/rdra-ish-core/src` minus `lifecycle/`,
ranked by value over risk. Read-only. No cargo runs, so every complexity figure is **inferred**
from code unless marked **guess**. Line numbers are against the working tree at audit time.

Scope read: model/ (all), typed_predicate.rs, predicate.rs, analysis/mod.rs,
analysis/predicate_process/{mod,args,relations}.rs, analysis/{instance,fk(head),property,condition(parts),comparison(parts)}.rs,
resolver.rs, import_scope.rs (head), location.rs, system.rs, tx.rs, access.rs, event_flow.rs,
entity_lifecycle.rs, workspace.rs, lint/{mod,structure(head)}.rs, refinement/{mod(parts),facts(parts),docs(head)}.rs,
rdra_rules.rs (BucClosure), diagnostics.rs (outline). Skimmed outlines only for the rest.

Legend: Obj = objective number from the brief (1 efficiency, 2 algebra, 3 file split,
4 reviewability, 5 workarounds, 6 comments). "BP" = behavior-preserving. "DECISION" = changes
observable behavior; needs an owner's call before it is a refactor.

---

## Ranked findings

### 1. `ImportScopes::scope_for` clones a whole file scope on every name lookup (Obj 1) — BP

`import_scope.rs:60`
```rust
pub fn scope_for(&self, source_id: SourceId) -> FileScope {
    self.by_source.get(&source_id).cloned().unwrap_or(FileScope { unrestricted: true, ..})
```
`resolve_flat_bindings` (l.95) and `resolve_namespaced` (l.123) call it, and
`analysis/arg_resolve.rs:71,117,279,309` call those once per resolved reference. Each call
deep-copies `flat: HashMap<String, Vec<FlatBinding>>`, `namespaces` and `imported`.
Cost O(refs x names visible in the file), all heap allocation. Matters for multi-file projects
whose files import large modules (e.g. 20k references x 2k visible names = 4e7 string clones);
single-file builds hit the cheap unrestricted branch. Inferred.

Change: return `&FileScope`, with a `static OPEN: FileScope` (or `Option<&FileScope>` and treat
`None` as open). Callers already only read it.
Diff: ~20 lines. Risk: none (pure read path). Pinned by `resolver/tests.rs`, `analysis/integration_tests/*`
(import resolution). Independent. Do first.

Follow-up (Obj 2, optional, same file): `unrestricted: bool` plus `FlatBinding { module: None,
kind: None }` placeholders encode "open scope" as a flag and sentinels. An
`enum FileScope { Open, Scoped { .. } }` deletes the four `if scope.unrestricted` branches.
~60 lines, BP.

### 2. `SemanticModel::typed_predicates` is write-only; delete the shadow model (Obj 2, 4, 5) — BP

Every predicate processor pushes a `TypedPredicate` (`predicate_process/mod.rs:184`,
`relations.rs:146`, `transitions.rs:172,363`, `effects.rs:74,205,342`, `structure.rs:32`,
`rules.rs` x4, `condition.rs:447,530`, `constraint/rules.rs:64,161`, `constraint/temporal.rs:164`),
but no production code reads it: the only readers are `collect_typed_predicates` (no caller
outside its own test) and integration-test assertions. Grep across all crates: `TypedPredicate`
appears only in core; 12 of the 13 target enums in `model/refs.rs` (`PerformTarget`,
`TriggerSource`, `ContainerRef`, `DecidesTarget`, ...) have zero uses outside
`typed.rs`/`typed_predicate.rs` (`Raiser` is the exception, used by `event_flow`).

So every fact is written twice, once loosely (`Relation { from: NodeRef, to: NodeRef, kind }`)
and once typed, and only the loose copy is consumed. A reader has to learn both and check they agree.

Change (recommended): delete `model/typed.rs`, `typed_predicate.rs`, the 12 unused target
enums and their `from_node_ref`, the push sites, and the `lib.rs` re-exports; port the ~6
test assertions to `relations`/fact vectors.
Diff: about -900 / +60. Risk: public-API removal (`build_typed_predicate`,
`collect_typed_predicates`, `typed_predicate_name`, `TypedPredicate`), with no consumer in the
workspace. Tests to port: `typed_predicate.rs` tests, `integration_tests/{condition,predicate,publication,instance}.rs`.
Independent; makes #7 smaller.

The alternative is to make the typed union the only model and delete `Relation` instead. It is
the better algebra because it makes illegal pairs unrepresentable, but it migrates 176
`.relations` uses across emit/cli/lsp. Do not keep both. If the team wants typed relations,
migrate the callers and delete `Relation` in one wave. Until then, deletion is the honest state.

### 3. Diagnostic order follows `HashMap` iteration, so it can change between runs (Obj 2/5) — DECISION (small)

`system.rs:108,124,140` iterate `HashMap<ApiKey|EntityKey, Vec<SystemKey>>`; `access.rs:381,402`
iterate `HashMap<UseCaseKey|ApiKey, Vec<PermissionKey>>`. `std` `HashMap` uses a per-process
random seed, and nothing downstream sorts diagnostics (no sort in `workspace.rs`, `project.rs`, or the
CLI). With two or more such warnings, CLI and LSP output order can vary from run to run. Inferred.

Change: iterate in id order (`BTreeMap` keyed by id, or collect and sort by id before pushing).
Diff: ~20 lines. Risk: output order becomes stable. That is a visible change from "random",
so a snapshot with two such warnings would pin one order. Pinned by `system.rs` tests and
`access/tests.rs` (single-diag cases today). Independent.

### 4. BUC reachability scans every relation per visited node, and `BucClosure` runs it per BUC (Obj 1, 3) — BP except the ambiguity fix

`resolver.rs:476-561` `reachable_from_buc`: `while let Some(current) = queue.pop() { for rel in &model.relations ...`
is O(V_reached x R). `rdra_rules.rs:330` `BucClosure::of` calls `reachable_from_bucs` over every
BUC in each lint run (`collect_findings`, which runs from `analyze_project`, the LSP path), giving
O(B x V x R). For example 50 BUCs x 500 reached nodes x 5k relations = 1.25e8 `NodeRef` compares. Emit
scoping (`mermaid.rs`, `diagram_common.rs`, `information.rs`) and `lifecycle/select.rs` call it too.
Inferred.

Change: build `HashMap<&NodeRef, Vec<&Relation>>` (forward edges, Relate* excluded) once per
call and BFS over it. That is O(R + V). For `BucClosure`, build it once and reuse it across BUCs.
Diff: ~40 lines. Risk: none if the adjacency keeps relation order, because the result is a `HashSet`.
Pinned by `resolver/tests.rs:843`, `rdra_rules.rs:551`, and emit scope snapshots.

Also in this function: `model.symbols.get(buc_id)` returns `None` when the id is ambiguous across
kinds (`symbol.rs:155`, `LookupResult::Ambiguous`). A BUC named like an entity therefore gets an
**empty closure**, and `BucClosure::of` (l.330) round-trips `BucKey -> id -> symbol lookup` to hit
exactly that. Make the core function take `BucKey` and keep a string wrapper that uses
`lookup_qualified(&Kind::Buc, id)`. **DECISION**: this fixes false lint findings in that case.

Move (Obj 3): reachability is a model-graph query, not file resolution. Move
`reachable_from_buc(s)` next to `business_flow::buc_members` (or a new `scope.rs`) and keep the
`lib.rs` re-export. Also delete the stray `use std::collections::VecDeque;` at `resolver.rs:574`
(move it to the top). Pure move.

### 5. Relation queries are linear scans repeated per element (Obj 1) — BP

About 100 `model.relations` scans outside lifecycle (access.rs 14, refinement/facts.rs 13,
lint/structure.rs 11, system.rs 10, event_flow.rs 9). The costly ones are scans inside per-element loops:

- `access.rs:713` `actors_for_usecase`: 3 full scans plus `triggered_use_cases(model)` (l.727),
  which builds a fresh `HashSet` over all relations on every call. `system_may_call` (l.491)
  rebuilds that set again. These run per use case in `derive_actor_input_inferences`,
  `permission_diagnostics`, and `collect_actor_permission_requirements` (per invoking use case of
  each API). Total O(U x R) several times over.
- `tx.rs:134` `infer_usecase_transactions`: per use case, one scan for `invokes`, one scan **per
  invoked API**, one for direct writes. `fk_parent_map(model)` is rebuilt per API group (l.182)
  and per FK component (l.409), and the full FK adjacency is rebuilt per use case (l.355). Total
  O(U x (A_u + 2) x R + U x F).
- `event_flow.rs:314` `api_diagnostics`: two scans per API, O(A x R). `event_diagnostics` does
  O((T_e + T_t) x R) for `belongs_to_buc`.
- `system.rs:16` boundaries: one scan per system. `usecase_invokes_api_for_entity_in_system`:
  O(R^2) per coordination.
- `analysis/condition.rs:230` `applied_bodies`: one scan per API-targeted `applies_to`, and
  `guard_labels` (l.275) recomputes all of it per use case, so per-transition emit callers pay
  O(U x A x R).

Change: in each function, build the maps once at the top (`invokes_by_uc`, `apis_by_uc`,
`performers_by_target`, `bucs_by_uc`, a `triggered` set, `fk_parent_map`, FK adjacency) and look up
from them. Prefer local maps over a cached index on `SemanticModel`: `relations` is a pub `Vec`
that tests push into, so a cached index could go stale. Each map must keep relation order (`Vec`
in push order), because `invoked_apis` is documented as "the order of the use case's `invokes`
lines" and `applied_bodies` as "relation order".
Diff: ~150 lines across 5 files, each file independent. Risk: ordering only. Pinned by
`access/tests.rs`, `tx.rs` tests (l.552+), `system.rs` tests, `event_flow.rs` tests, emit
sequence/robustness snapshots.
Measure before and after with `rdra-ish lint` and `check` on the largest example. No timing exists today.

### 6. `infer_usecase_transactions` is recomputed per consumer (Obj 1) — BP

Callers: `workspace.rs:63`, `publication.rs:355` (runs inside `build_model`), `lint/conditions.rs:249`,
`model/query.rs:386` (`guard_wants` under `index_suggestions`), `surface.rs:77`, and two lifecycle
sites. One `analyze_project` runs it at least 3 times, plus once per `index_suggestions` call.
Inferred.

Change: after #5 makes it cheap, decide whether it still matters. If it does, compute it once
per analysis and pass `&[UsecaseTx]` into `tx_diagnostics` (already takes it),
`lint_unguarded_triggered`, and `index_suggestions`. Avoid a `OnceCell` on the model, for the same
staleness reason as #5.
Diff: ~40 lines. Risk: low. Depends on #5 for its priority.

### 7. The predicate catalog is a string keyed in eight parallel tables (Obj 2, 4) — BP

A predicate name is a `&str` matched separately in:
`predicate.rs:29` `predicate_signature`, `:101` `predicate_chain_methods`, `:152`
`KNOWN_PREDICATES`; `predicate_process/args.rs:166` `arity_error`; `args.rs:25` (which
predicates leave trailing args unresolved); `relations.rs:12` `relation_kind_for_predicate`;
`predicate_process/mod.rs:146` dispatch; `typed_predicate.rs:20` (gone after #2). Adding a
predicate means editing 6-8 places with no exhaustiveness check. Argument kinds are strings with
sentinel values that leak across crates:
```rust
"transitions" => Some(vec![vec!["usecase"], vec!["_col"], vec!["_val"]]),
...
if matches!(kinds.as_slice(), ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"])
```
and the LSP filters on `kind.starts_with('_')` (`rdra-ish-lsp/src/predicates.rs:12,29`).
`validate_contains_pair` (`args.rs:86`) holds a second pair table that the signature cannot express.

Change: `enum Predicate { Performs, Reads, .. }` with `ALL`, `name()`, `parse(&str)`,
`signature() -> &'static [&'static [ArgKind]]`, `chain_methods()`, `arity() -> Arity { Exact(n),
AtLeast(n), OneOf(&[n]) }`, `rel_kind() -> Option<RelKind>`; `enum ArgKind { Node(&'static str),
Column, Value, Card }`. Parse once in `process_predicate`. Keep `predicate_signature(&str)` as a
wrapper for the LSP and emit test, or migrate those 5 sites in the same wave.
Diff: ~+250/-250. Risk: medium. `WrongArity.expected` and `TypeMismatch.expected` strings
(`expected_kinds.join("|")`) must stay byte-identical. Pinned by
`integration_tests/rule_audit.rs:472`, `integration_tests/diagnostics.rs`, `rdra-ish-emit/src/diagram_common.rs:821`,
`rdra-ish-lsp/src/{predicates,hover,completion}.rs` tests. Independent of #2, easier after it.

### 8. Guards for dangling slotmap keys that cannot occur (Obj 5, 4) — BP

Nothing removes from the model's slotmaps (grep: no `.remove(` on them in core), and keys come
only from the same model. Yet:
```rust
// model/mod.rs:27  "or `None` when its key no longer resolves"
pub fn node_id(&self, node: &NodeRef) -> Option<&str>
// system.rs:436-466, tx.rs (x6), event_flow.rs (x6)
model.apis.get(key).map(|a| a.id.clone()).unwrap_or_default()
// entity_lifecycle.rs:113 "A transition on an entity the model does not hold is skipped."
```
That is about 23 `get(k).map(|e| e.id.clone())` sites in core and 11 `node_id(..).unwrap_or(..)`
sites in cli/lsp/emit. Sixteen of them are `sort_by_key` closures that allocate a `String` per
comparison.

Change: `node_id`/`node_label` return `&str` (index, not `get`); add `entity_id(k) -> &str`-style
accessors (or use `model.entities[k].id` directly, as `access.rs` already does); sort with
`sort_by(|a, b| id(a).cmp(id(b)))`. Delete the four per-file `*_id` helpers in `system.rs:436-466`
and the `"?"` fallbacks in `tx.rs:316,324`.
Diff: ~150 lines (core ~100, other crates ~30). Risk: a key from a different model would now
panic instead of printing `""`. No test builds keys by hand (grep for `Key::default()`/`null()`
is empty). Pinned by every snapshot test, since ordering is unchanged when ids are unique. Independent.

### 9. `ConditionEdge` duplicates `CombinesTarget` (Obj 2) — BP

`analysis/condition.rs:22` and `model/refs.rs:396` define the same sum:
```rust
pub enum ConditionEdge { Variation(VariationKey), StateModel { entity: EntityKey, column: String } }
pub enum CombinesTarget { Variation(VariationKey), StateModel { entity: EntityKey, column: String } }
```
`condition_edges` (l.97-105) converts one to the other variant by variant.
Change: keep one type (in `model`), named for what it is (e.g. `ConditionTarget`). Delete the
conversion. Diff: ~35 use sites (18 + 16) renamed, -20 lines. Risk: none. Independent.

### 10. Comments that restate code, narrate history, or are wrong (Obj 6) — BP

- `tx.rs:540` `// サイクルが存在する場合（FKグラフではありえないが防御的に）`: false. A self-relate
  (`relate(Employee, Employee, N:1)`) or two mutual keys make a cycle, and a self-edge keeps
  in-degree above 0. The guard is legitimate; rewrite the comment to say when it fires.
- `tx.rs:45,126` call direct use-case writes "レガシー形式", but `creates(UseCase, Entity)` is a
  supported, undeprecated form. Drop "legacy". Same with "legacy open scope" in `import_scope.rs:27,316`.
- `analysis/predicate_process/relations.rs:321` `// L4: ...`: the stale ticket label adds nothing.
- `model/constraints.rs:286` `/// \`leads_top == q\` → \`p ~> q\``: garbled; should be `leads_to(p, q)`.
- `event_flow.rs:278-284`: the doc on `api_route_diagnostics` describes `api_diagnostics`
  (ApiNeverInvoked / ApiInvokedButNoEntity) and claims a one-pass scan. `api_diagnostics` is
  O(A x R) and has no doc. Move and correct it.
- Comments that restate code: `event_flow.rs:178,334` (`// どの UC からも raises されていない`,
  `// 未呼出しなら entity 操作の確認は不要`), `analysis/comparison.rs:151`
  (`/// \`ast::CmpOp\` → \`model::CmpOpModel\` への変換。`), `model/effects.rs:157-162`
  (`/// 効果を起こす usecase`, `/// 対象 entity のキー`, `/// 対象カラム名`).
- 24 `// ── 公開型 ──`-style banners in 9 non-lifecycle files (`tx.rs`, `model/{constraints,symbol,comparison}.rs`,
  `analysis/comparison.rs`, `analysis/constraint/{mod,entity_conditions}.rs`, `diagnostics.rs`, a test file).
  Most mark "public types" or "tests". Delete them.
Diff: ~-60 lines. Risk: none. Independent.

### 11. `system_diagnostics` is a 250-line function with three identical error blocks (Obj 4) — BP

`system.rs:88-334`. `CoordinationNotCrossSystem` is built identically at l.249, 264, 282 for "left
missing", "right missing", and "not exactly one each or same system".
Change: one slice-pattern match:
```rust
let (Some([l]), Some([r])) = (by_entity.get(&c.left).map(Vec::as_slice), ..) else { push; continue };
if l == r { push; continue }
```
Split the function into one fn per rule (api-in-many, entity-in-many, owner, outside-owner,
cross-system relation, coordination), keeping the push order. Diff: about -60. Risk: diag order
must stay the same (after #3 it is defined). Pinned by `system.rs` tests (l.475+).

### 12. `process_relate_predicate` re-checks the same facts in four blocks (Obj 4, 5) — BP

`analysis/predicate_process/relations.rs:254-421`: four separate `if kind == RelKind::RelateManyToMany`
blocks, `"?"` fallbacks for non-entity endpoints (l.325-332), and `synthesize_via_n1` /
`via_for_synth` (l.388-404) that re-match `(Entity, Entity, Some(Entity))` already established.
`parse_card` (l.194) runs a small state machine on the strings `"1"`, `"N"`, `"M"`, `"?"`.
Change: match `(from, to)` to `(EntityKey, EntityKey)` once, split out `relate_many_to_many(..)`
with early returns, and use `enum Side { One, Many }` in `parse_card`.
Diff: about -60. Risk: keep the error order (role/key errors before NMRelation, then
SelfNMRelation, then ViaMustBeDistinct). Pinned by `integration_tests/{keys,diagnostics,build_model}.rs`.
The `"?"` fallback exists because of #19.

### 13. Duplicate-definition detection runs twice, and the single-file path leaves a ghost node (Obj 5) — BP for multi-file

`resolver.rs:371-441` pre-filters duplicate instances with `seen_ids`. `analysis/instance.rs:87-384`
inserts the node into its slotmap **before** `symbols.insert_in_module` reports the duplicate, so
`build_model` (single file) keeps a second `Entity`/`Actor`/... with no symbol and no decl site
that emitters would iterate. The resolver pre-filter is the workaround that hides this on the
real path.
Change: check the symbol table before inserting (`SymbolTable::contains(id, kind, module)`), then
delete `seen_ids`. Diff: ~40 lines. Risk: multi-file `DuplicateDefinition` errors move from
"before all model diagnostics" to item order, with the same message and span. Pinned by
`resolver/tests.rs`, `integration_tests/diagnostics.rs`.

### 14. Analysis code lives in `model/` (Obj 3) — BP (pure move)

`model/query.rs:210-520` (`impl SemanticModel { wants, guard_wants, index_suggestions, ..}`) is an
index advisor that calls `crate::tx::infer_usecase_transactions` and `crate::analysis::applied_bodies`
(l.386-388). The model layer depends upward on the analysis layer. Move the impl block to
`analysis/index_advice.rs` (or top-level), and keep `Query`, `OrderKey`, `Page`, `Reader`, and `IndexSuggestion`
in `model/query.rs`. Diff: ~310 lines moved. Risk: none. Independent; do it after the content changes
to avoid conflicts.

Same kind, smaller: `api_route_diagnostics` and `api_diagnostics` live in `event_flow.rs:285,314`
but check APIs. Move them to `system.rs` or a new `api.rs`. `predicate.rs` (catalog) is used only by
`analysis/predicate_process` and the LSP; after #7 it belongs under `analysis/`, with the `lib.rs`
re-export kept.

### 15. Declaration kinds are strings throughout (Obj 2) — BP

`location.rs:132-159` `DeclIndex` keys on `format!("{kind}:{id}")`, allocating per lookup, and
`iter()` splits on `':'`. `push_model_decl_diagnostic(model, diags, "api", id, err, true)` has 48
call sites with string literals and a `bool` severity. A typo in a literal silently drops the location.
`FlatBinding.kind: Option<String>` (`import_scope.rs:14`); `LintIssue.subject_kind: String`, compared
with `== "buc"` (`lint/mod.rs:115`). `model/symbol.rs:229` `node_ref_matches_kind` re-derives a
`NodeRef` to `Kind` table that `NodeRef::kind_tag` already spells as strings.
Change: key on `rdra_ish_syntax::ast::Kind` (Copy); add `NodeRef::kind() -> Option<Kind>` (State has
none), and `DeclIndex = HashMap<(Kind, String), DeclSite>`. Same wave: `lint/mod.rs:196`
`is_consistency_diag` is a hand-kept list of 23 `RdraError` variants. Make it an exhaustive
`RdraError::category()` in `diagnostics.rs` so a new diagnostic cannot be forgotten.
Diff: ~150 lines, cross-crate (LSP go-to-definition uses `DeclIndex::iter`). Risk: low. Pinned
by LSP definition tests and lint tests.

### 16. Parallel arrays for enum values and their labels (Obj 2) — BP

`model/refs.rs:508,530`: `ColumnType::Enum(Vec<String>)` plus `ModelColumn.value_labels: Vec<Option<String>>`,
and likewise `Variation.values` plus `labels`. Nothing ties the lengths together. Readers index by
position (`model/mod.rs:114`, `refs.rs:601`), and one pads defensively:
`rdra-ish-emit/src/codegen/mod.rs:224` `.zip(variation.labels.iter().chain(std::iter::repeat(&None)))`.
`value_labels` is meaningless on non-Enum columns.
Change: `struct EnumValue { value: String, label: Option<String> }`; `ColumnType::Enum(Vec<EnumValue>)`;
`Variation.values: Vec<EnumValue>`; `column_values()` returns an iterator of `&str`.
Diff: ~120 lines, about 20 of them in emit. Risk: low. Pinned by emit sql/dbml/mermaid/codegen snapshots.

### 17. `build_model*` clones every AST item (Obj 1, 5) — BP

`analysis/mod.rs:31` `ast.items.iter().cloned().map(|item| (0, item))` and `resolver.rs:435,439`
`located_items.push((source_id, item.clone()))` deep-copy the whole AST (instances with columns,
predicate chains) before a pass that only reads it. O(AST) copy and double peak memory. Small
next to analysis cost (guess).
Change: `build_model_items_with_scopes(items: &[(SourceId, &Item)], ..)`. Diff: ~20 lines. Risk: none.

### 18. Small quadratic spots worth fixing only if measured (Obj 1) — BP

- `entity_lifecycle.rs:46-58`: `derive_lifecycle_states` calls `state_id` per new state, and that
  calls `lifecycle_columns`, which scans all transitions. O(S x T).
- `entity_lifecycle.rs:73` `state_key` is a linear scan over all states. `state_label` (emit, per
  value) makes it O(values x states).
- `typed_predicate.rs:233` `sort_by_key(|p| format!("{name}:{p:?}"))` formats per comparison
  (gone with #2).
- `model/foreign_key.rs:191` `key_path` BFS with `Vec` membership: O(V x F) per call. Inputs are
  small, so leave it.
Change only after a profile shows them. Each is a 5-15 line local fix.

---

## Needs a decision (not behavior-preserving)

### 19. Processing continues after a `TypeMismatch`, so ill-typed relations enter the model (Obj 2, 5) — DECISION

`predicate_process/mod.rs:138-144`: `validate_predicate_arg_types` only reports, while
`validate_contains_pair` / `validate_trigger_pair` stop processing. `process_relation_predicate`
(`relations.rs:94`) then pushes e.g. `performs(Entity, Screen)` into `model.relations`. This is
the root of the `if let (NodeRef::X, NodeRef::Y) = .. else { continue }` guards in nearly every
consumer and of the `"?"` fallbacks in #12. Make it return `bool` and stop like its siblings.
Effect: fewer cascaded diagnostics after a type error; LSP models lose the bad edge. ~10 lines.

### 20. Temporal-property atoms keep unresolved names, with a heuristic fallback (Obj 2, 5) — DECISION

`model/constraints.rs:246-268` stores `entity: Option<String>` and `column: String`;
`analysis/property.rs:233-249` falls back to spelling-based guesses when the column does not resolve
(`// Heuristic: ==/!= with bare ident → enum/bool literal.`). The lifecycle re-resolves through
`property_entity` (`lifecycle/build/naming.rs:282`, `preds/temporal.rs:170`) and reports there.
Resolve in `analysis/property.rs`, store `QualifiedModelColumnRef`, and report unknown columns at
build time. Effect: the error appears in `check`/LSP without the lifecycle stage. Medium-large,
touches lifecycle.

### 21. Four definitions of "the use cases of a BUC" disagree (Obj 4, 5) — DECISION

- `business_flow.rs:9` `buc_members`: BUC contains UC, plus BUC contains Flow, Flow contains Step,
  Step covers UC.
- `refinement/facts.rs:305` `buc_use_cases`: the same set by BFS.
- `access.rs:791` `bucs_containing_usecase`: direct `contains(Buc, UseCase)` only.
- `event_flow.rs:208,240` `belongs_to_buc`: direct only.

So `performs(Actor, Buc)` lends the actor to use cases the BUC contains directly, but not to use
cases its flow steps cover. Permission and actor-input results differ from what the refinement
stages and hand-offs consider BUC members. Pick one definition (`buc_members`) and route all four
through it. `refinement/facts.rs:331` `use_case_entities` also duplicates the "entities a UC
touches directly or via invoked APIs" logic in `access.rs:581` and `tx.rs`.

### 22. Single-entity rule types split conditions in two lists; cross-entity rules use one enum (Obj 2) — DECISION (large)

`model/constraints.rs:9-76`: `ForbiddenConstraint`, `RequiredConstraint`, and `ExclusiveConstraint` are
the same five fields, and each keeps `conditions: Vec<(String, EffectValue)>` plus
`comparisons: Vec<ComparisonProp>`, evaluated as one AND. `EntityInvariant` doubles it
(`guards` + `guard_comparisons`, `requireds` + `required_comparisons`). Cross-entity rules use one
`Vec<CrossEntityCondition>` enum (`Equals | Comparison`) for the same idea. The target is one
`EntityCondition` enum and one `EntityRule { kind, entity, conditions, site, label }` for the three
same-shaped structs (`RuleRef` indices adjust). The lifecycle is the main consumer, so this needs
its owner. Large.

`EffectValue::TypedPresent(String)` (`model/effects.rs:37`) is related. It equals `Present` for every
check but not under derived `PartialEq`/`Hash`, so callers must call `normalize()` (9 sites) or
match both (6 sites). Only `lifecycle/build/vars.rs:177` reads the type name. Moving the name
out of the value removes that trap.

### 23. Top-level module sprawl (Obj 3) — optional, BP

22 top-level modules mix the front end (`project`, `resolver`, `import_scope`, `workspace`,
`location`), derived views for emitters (`business_flow`, `screen_flow`, `event_flow::collect_*`,
`entity_state_variant`, `rules`, `surface`, `access::derive_*`), and consistency checks
(`access::permission_diagnostics`, `system`, `tx::tx_diagnostics`, `event_flow::*_diagnostics`,
`publication`). `access.rs` (918 lines) holds both views and checks. A `frontend/ views/ checks/`
regroup with unchanged `lib.rs` re-exports is pure moves, but it is large review noise for modest
gain. Recommend only the targeted moves in #4 and #14, plus splitting `access.rs` into
views and `permission_diagnostics`. Do the full regroup only if the team wants it.

### 24. Parallelism: not warranted in core yet (Obj 1) — guess

No workspace crate depends on rayon; `lifecycle/targets.rs` and the CLI use threads. Candidates in
scope: parsing per BFS wave in `resolver_overlaid` (files are independent, but the work is I/O plus a
fast parser), the five independent read-only passes in `consistency_diagnostics`, and
per-BUC `BucFacts::collect` in `refinement/mod.rs:353`. Each is dominated by the O(n x R) scans in #4
and #5. Fix those, measure, and then consider `std::thread::scope` for `consistency_diagnostics` only
if it still shows up. Guess.

---

## Leave as is

- `diagnostics.rs` (803 lines): one flat `RdraError` catalog with `thiserror` messages. Size is not a problem here.
- `model/foreign_key.rs`: `FkId`, `Join { Unrelated, Key, Ambiguous }`, `UniqueSource`, `Multiplicity`
  are good ADTs. `join_key` is the single join rule. Keep.
- `model/mod.rs` `Publication` / `Exposed` / `PublishMode` / `CallerKind`: clean.
- `model/mod.rs:28-89` `node_id`/`node_label` 26-arm matches: verbose but greppable. A macro would
  cost more reader load than it saves. Only change the return type (#8).
- `analysis/instance.rs:24-85` metadata-on-wrong-kind checks: legitimate boundary validation (the
  AST carries every metadata block). Keep.
- `analysis/fk.rs`: clear two-phase key resolution. Keep.
- `lint/structure.rs:7` hand-written DFS for flow cycles: reports steps in DFS order, and swapping in
  `petgraph::tarjan_scc` could change which steps are reported. Keep.
- `refinement/stages.rs` `const StageSpec` tables: data, not logic. Keep.
- `refinement/mod.rs:3` `#[cfg(test)] mod docs;`: a doc-generation check living in src. Moving it is fine
  but optional.

## Suggested sequence

1 → 3 → 10 → 9 → 2 → 8 → 4 → 5 (per file) → 6 (if still needed) → 11 → 12 → 13 → 7 → 15 → 16 → 17 → 14 (moves last).
Decisions 19-22 go to the model owner. #19 is small and makes #12 simpler if accepted.
Every item lists its pinning tests. Run `cargo test -p rdra-ish-core` plus the emit, cli, and lsp suites after
each item that touches a public type (2, 7, 8, 9, 15, 16).
