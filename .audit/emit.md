# rdra-ish-emit audit (crates/rdra-ish-emit/src, ~23.6k lines)

Goal: a behavior-preserving refactoring plan. Read-only audit; no cargo run. All paths are
relative to `crates/rdra-ish-emit/src/` unless noted. Efficiency labels: measured / inferred / guess
(nothing here is measured).

Test pins referenced below:
- codegen: `codegen/snapshots/*{typescript,rust,go}_snapshot.snap`, `codegen/tests.rs`,
  `crates/rdra-ish-cli/tests/codegen_{behaviour,builds,equivalence}.rs`
- mermaid: `mermaid/tests.rs`, `mermaid/snapshots/*er_mermaid_snapshot.snap`,
  `crates/rdra-ish-cli/tests/diagram_{contract,scope,stdout}.rs`
- csv: `csv.rs` tests (9 `test_*_csv` + `entity_csv_snapshot`)
- lib: `lib.rs` tests (`collect_object_graph_nodes_uses_layer_order_and_id_sorting`)

Ranked by value/risk (best first).

---

## 1. Dead gates and unused-param suppressions in codegen/actions.rs  [obj 5]
- `codegen/actions.rs:94-101`:
  ```rust
  let _ = trigger;
  let reason: Option<String> = None;
  let built = match reason { Some(reason) => Err(reason), None => functions(...) };
  ```
  Leftover of the "triggered by several events (phase 5)" gate removed in d3a1269. `reason` is always `None`.
- `codegen/actions.rs:255-264`: a loop over `action.params` whose every arm is `{}`; does nothing.
- `codegen/actions.rs:1449` `let _ = ts;` (param of `literal` never used), `:1973` `let _ = model;`
  (param of `event_fields` never used), `:1501` `let _ = declared.get(&type_ident(entity))?;`
  (lookup used only as an existence gate; caller already resolved the type), `asyncapi.rs:182`
  `transition_ids(_model, ...)`.
- `codegen/actions.rs:395-398` `ActionOrigin::Clock => unreachable!("only use case actions reach here")`:
  the caller (`:79`) already destructured `ActionOrigin::UseCase { uc, .. }`; pass `uc` in.
- Change: delete the dead code, drop the unused params, pass `uc: UseCaseKey` to `functions`.
- Size: ~-35 lines. Risk: none (no reachable behavior); pinned by the 3 codegen snapshots.
- Independent.

## 2. `node_kind_filter_name` duplicates `NodeRef::kind_tag`  [obj 5, 2]
- `lib.rs:475-504` is a verbatim copy of `rdra-ish-core/src/model/refs.rs:38` `kind_tag()` (same 26 strings).
- `lib.rs:506-516` `view_node_visible` compares the filter against `kind`, `normalize_filter_token(kind)`
  and `kind.replace('-', "_")`; every kind tag is lowercase with no `-`/`_`, so all three are the same
  string. It is `view.node_kinds.iter().any(|f| f == node.kind_tag())`.
- Test helper `lib.rs:825-854` `node_id` duplicates `SemanticModel::node_id`.
- Change: delete `node_kind_filter_name` (20 call sites -> `node.kind_tag()`), collapse the visibility
  check, use `model.node_id(..).unwrap()` in the test.
- Size: ~-70 lines, 20 one-token edits. Risk: none (same strings). Pinned by diagram_contract + mermaid tests.
- Independent.

## 3. csv.rs: two identical CRUD-matrix emitters and 9 copies of the writer epilogue  [obj 5, 4]
- `csv.rs:125-210` (UseCase x Entity) and `csv.rs:256-332` (Api x Entity) are the same algorithm; each
  has four copy-pasted arms:
  ```rust
  RelKind::Reads => { if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) { (*uk, *ek, 0b00010u8) } else { continue; } }
  ```
- Every emitter ends with
  ```rust
  let data = wtr.into_inner().map_err(|e| std::io::Error::other(e.to_string()))?;
  Ok(String::from_utf8(data).unwrap_or_default())
  ```
  (9 times). `unwrap_or_default` guards a state that cannot occur (csv of `String`s is UTF-8) and would
  silently turn it into an empty file.
- Change: `fn crud_bit(&RelKind) -> Option<u8>`, one `crud_matrix(header0, rows: Vec<(NodeRef, &str)>, model)`
  used by both emitters; one `fn finish(wtr) -> Result<String, EmitError>` with `expect("csv of String is UTF-8")`.
  `col_type_to_str` (`csv.rs:593`) is a one-line forwarder to `diagram_common::col_type_str`; call it directly.
- Size: ~-150 lines. Risk: low; same rows/order. Pinned by `test_relation_matrix_csv`, `test_api_entity_matrix_csv`, `entity_csv_snapshot`.
- Independent.

## 4. Stale and restating comments  [obj 6]
- `csv.rs:1` module doc lists 5 emitters; the file has 9.
- `csv.rs:135` `// bits: C=1, R=2, U=4, D=8, W=16`: nothing sets W.
- `codegen/ir.rs:445-446`: `Assigned::Switch` carries the doc line of `MarkId`
  ("The id of the row a mark argument names (a row an earlier step wrote).") above its own; move it to `MarkId` (`:459`).
- Restating section labels: `state_pattern.rs:307,339,352,364,396,404` (カラム幅の計算, ヘッダ行, セパレータ, データ行, フッタ, 診断),
  `mermaid.rs:784` `// BUCフィルタ`, `mermaid.rs:834` `// ER relations`, `csv.rs:127,131` `// ソートされた…一覧`, `csv.rs:190` `// rows`.
- `codegen/actions.rs:1573,1583` call `super::actions::snake` from inside `actions.rs`.
- Size: ~-20 lines. Risk: none. Independent.
- Otherwise the crate's comments are good: they state contracts, not history. No TODO/legacy notes found.

## 5. Obligation ids are recomputed at every push site  [obj 2, 4]
- `format!("O{}", domain.obligations.len() + 1)` at `codegen/mod.rs:79`, `:586`, `codegen/actions.rs:176`, `:199`, `codegen/validators.rs:149`.
  `Obligation.id` is a pure function of its index; storing it makes an out-of-order push produce a wrong id.
- Change: drop `Obligation.id`; `obligations_only` (`codegen/mod.rs:166`) prints `O{i+1}`. One `push` helper.
- Size: ~-25 lines. Risk: none (same order). Pinned by codegen snapshots; `codegen/tests.rs:84` reads only `invariant`.
- Independent.

## 6. `collect_object_graph_nodes` is 22 copy-pasted blocks; ADR exclusion patched by a caller  [obj 4, 5]
- `lib.rs:569-796`: 22 repetitions of
  ```rust
  let mut goals: Vec<_> = model.goals.iter().collect();
  goals.sort_by_key(|(_, g)| &g.id);
  nodes.extend(goals.into_iter().map(|(k, _)| NodeRef::Goal(k)).filter(|nr| is_visible(nr)));
  ```
- It silently leaves out ADRs; `mermaid.rs:197-205` patches that back in:
  `// No diagram draws an ADR, so collect_object_graph_nodes leaves them out.` then `nodes.extend(model.adrs.keys()...)`.
  Note the patch appends ADRs unsorted after everything, not in layer order.
- `lib.rs:538-567` `node_description` is a third per-kind accessor beside core's `node_id`/`node_label`.
- Change: a generic `fn sorted<K, V: HasId>(map, ctor) -> impl Iterator<NodeRef>` (or a macro) and a const
  list of kinds in draw order; take `include_adr: bool` (or a `DRAWN_KINDS` list without Adr) so the caller's
  patch goes away. Move `node_description` to core next to `node_label`.
- Size: ~-200 lines. Risk: low; order is pinned by `lib.rs:857` test and mermaid snapshots. Keep the ADR
  append order exactly as today when removing the patch (the `omitted:` header line goes through
  `kinds_by_layer`, which re-sorts by layer, so order of ADRs inside the vec does not reach output: inferred).
- Independent of #2 (but touches the same file; do #2 first).

## 7. File split: mermaid.rs holds four emitters; diagram_common.rs holds the event diagram  [obj 3]
- `mermaid.rs` (946 lines) is both the module root with shared helpers (`GraphEdge`, `EdgeSet`, `render_edges`,
  `buc_scope`, `kinds_by_layer`, `omitted_linked_kinds`) and four emitters: RDRA graph (`:41-480`),
  state (`:484-780`), ER (`:782-868`), event (`:872-898`), plus `entity_neighbourhood` (`:906`).
  The other ten diagram kinds each have their own `mermaid/<kind>.rs`.
- `diagram_common.rs:619-800` (`FlowNode`, `Declared`, `declare_flow_node`, `render_event_flows`,
  `render_event_flow`, `render_timer_flow`) is used only by `EventMermaidEmitter` (and `mermaid/tests.rs`).
- `lib.rs:363` `er_glyph` is used only by the ER emitter (`mermaid.rs:857`).
- Change: pure moves to `mermaid/rdra.rs`, `mermaid/state.rs`, `mermaid/er.rs` (+ `entity_neighbourhood`, `er_glyph`),
  `mermaid/event.rs` (+ the event-flow code from diagram_common). `mermaid.rs` keeps shared helpers only.
- Size: ~900 lines moved, ~0 net. Risk: none; `mermaid/tests.rs:2` import path changes.
- Independent; do after #2/#6 to avoid churn on the same lines.

## 8. File split: codegen/actions.rs (2136 lines) mixes four concerns  [obj 3]
- Concerns in one file: (a) action -> `FnDecl` (`add_functions`, `functions`, `function`), (b) Pred -> Expr lowering
  (`Quant`, `expr`, `resolve`, `atom`, `fold_next`, `reads_only_created`, `remap_lists`, `typed`; `expr` is also imported by
  `validators.rs:9`), (c) `simplify` (`:1776`), an `Expr` operation the printers reach as `super::actions::simplify`,
  (d) `dispatchers` (`:1861-1965`), and naming (`snake` at `:531`, while `type_ident`/`camel` live in `codegen/mod.rs`).
- `fails` is copied verbatim in `ts.rs:702`, `go.rs:707`, `rust.rs:741`:
  ```rust
  fn fails(e: &super::ir::Expr) -> super::ir::Expr { super::actions::simplify(super::ir::Expr::Not(Box::new(e.clone()))) }
  ```
- Change: `impl Expr { fn simplify(self); fn fails(&self) }` in `ir.rs`; `codegen/lower.rs` for (b); `codegen/dispatch.rs` for (d);
  `codegen/naming.rs` for `type_ident`/`snake`/`camel`/`string_literal`/`one_line`. Delete the 3 `fails` copies.
- Size: ~700 lines moved, ~-15 net. Risk: none. Independent; do before #13.

## 9. `RelKind::is_relate()` instead of six copies of the four-arm match  [obj 2]
- `matches!(kind, RelKind::RelateOneToOne | RelKind::RelateOneToMany | RelKind::RelateManyToOne | RelKind::RelateManyToMany)`
  at `lib.rs:528`, `mermaid.rs:245`, `:256`, `:842`, `:932`, `mermaid/information.rs:15`.
- Change: `RelKind::is_relate(&self)` in core (`model/refs.rs`), or a crate-local fn if core is off-limits.
- Size: ~-30 lines. Risk: none. Independent.

## 10. Placeholder values standing in for "absent" in codegen  [obj 2]
- `codegen/actions.rs:337-353`: `Source { ty: String::new(), fields: Vec::new(), always_null: Vec::new(), variant: None }`
  for an `Earlier` row and for a created row of a sum type; `function` then never reads `ty` for those params (they are
  skipped at `:668` and `:790`), so the empty string is an unreachable-in-practice illegal value.
- `codegen/actions.rs:1432` `Lit::Enum { ty: String::new(), ... }`: an enum literal with no type, patched later by
  `typed()` (`:1835`) or `start_literal` (`:1466`).
- `codegen/actions.rs:1585` `RowParam { param: usize::MAX, .. }` for quantifier elements.
- Change: `sources: Vec<Option<Vec<Source>>>` (None = not an argument); `Write::Enum(EnumValue)` separate from `Lit`,
  typed once at the target field; `param: Option<usize>` or a separate `ElemParam` in `FnDecl.elems`.
- Size: ~60 lines touched. Risk: low; pinned by codegen snapshots/equivalence. Independent of #11, easier after #8.

## 11. Declared types looked up by re-mangling names  [obj 2, 4]
- `declared: BTreeMap<String, Declared>` is keyed by the generated type name, so every lookup re-derives it:
  `declared.get(&type_ident(&entity_id(p)))` and variants at `codegen/actions.rs:250`, `:473`, `:592`, `:681`, `:716`, `:807`, `:1084`,
  `:1501`, `:1983`.
- The id type is rebuilt by string in 3 places: `format!("{}Id", type_ident(entity))` at `:596` (closure `id_of`), `:1504`
  (`declared_id`), `:1988` (`event_fields`), though `build` already recorded it in `domain.ids` / `id_of` (`codegen/mod.rs:232`).
- `codegen/actions.rs:118-124` recovers an event's id from its generated type name:
  `.find(|e| type_ident(&e.id) == ev.ty)`. `EventIn` should carry the event id (or `EventKey`).
- Change: key `declared` by `EntityKey` (or entity id) with an `EntityType { decl, id: Option<(String, Field)> }` built once;
  resolve `Vec<&EntityType>` per param at the top of `functions`; add `event: String` to `EventIn`.
- Size: ~120 lines touched. Risk: low-medium (a name collision case could differ if two ids mangle to one name, but
  `Names::claim` already rejects that). Pinned by codegen snapshots + `codegen_equivalence`. Do before #16.

## 12. Dead "writes" relation in the published edge contract  [obj 5]
- `diagram_common.rs:178` `const CRUD: [&str; 5] = ["reads", "writes", "creates", "updates", "deletes"];` and rows
  `:197` (rdra) and `:356` (usecase) declare a `writes` usecase/api -> entity relation. No `RelKind` maps to "writes"
  (`lib.rs:315` `object_graph_rel_label`) and no emitter writes that label (grep).
- Change: drop "writes" from `CRUD` and its two rows. `implied_relation` results do not change (four CRUD relations remain per pair).
- Size: -3 lines. Risk: `EDGE_RELATIONS`/`edge_relations` are `pub`; a consumer listing the contract would lose a row.
  Check `docs/` for a published table before removing (none found by grep: inferred dead). Independent.

## 13. Small duplications in the three printers  [obj 4]
- Element-row resolution `match f.rows.get(r) { Some(x) => .., None => f.elems[r - f.rows.len()] }` at `ts.rs:342`,
  `go.rs:363`, `rust.rs:349`: belongs on `FnDecl` as `row(r) -> &RowParam`.
- Two identical closures per printer: `let input_var = |i| ..; let input = |i| ..;` (`ts.rs:344-345`, `go.rs:365-366`, `rust.rs:351-352`).
- The `True/False/Not/And/Or` skeleton of `expr` is the same in all three (`ts.rs:556`, `go.rs:605`, `rust.rs:609`);
  only the atoms differ. Optional: a shared `fn junction(items, sep, print)`. Leave the per-language atoms and top-level
  `print` as is: `ir::DomainModel` is already the shared representation, and a printer trait over it would add a layer
  for three implementations that differ in nearly every line.
- Size: ~-40 lines. Risk: none. After #8.

## 14. Identical uniquifiers and inline-enum code in codegen/mod.rs  [obj 4]
- `distinct_idents` (`codegen/mod.rs:505`) and `distinct_fields` (`:556`) are the same loop over different fields.
- Entity inline Enum (`:455-464`) and DTO inline Enum (`:297-307`) repeat claim + `inline_enum` + push.
- Change: one `fn uniquify<T>(items, get: fn(&mut T) -> &mut String)`; one `inline_enum_type(names, domain, owner, column)`.
- Size: ~-30 lines. Risk: none. Independent.

## 15. `View` constructors restate all six fields  [obj 4]
- `lib.rs:126-181`: `whole`, `er`, `bucs`, `usecases` each spell out `state`, `scope`, `filter`, `model_path`, `node_kinds`, `edge_kinds`.
- Change: `Self { scope, ..Self::whole() }`. Size: ~-30 lines. Risk: none. Independent.

## 16. Decompose `function` (860 lines, 12 params)  [obj 4]
- `codegen/actions.rs:555-1415` is one function with ~20 mutable locals threaded through six phases (row params,
  marks, event binding, effect writes, outputs, guards, raises, marks out). Inside it:
  - the `bind` closure is defined three times identically (`:1110`, `:1138`, `:1233`);
  - "lower guard -> simplify -> True skip / False `Skip::Combination` / else push `CondGuard`" repeats three times (`:1115-1131`, `:1146-1163`, `:1240-1257`);
  - the `MarkSource` choice (mark, else row, else created-row input) repeats (`:1313-1328` raises, `:1346-1360` marks out);
  - "find the field named `wire` in row r" repeats about eight times.
- Change: a `FnBuilder<'a>` struct holding `rows`, `row_of`, `mark_of`, `inputs`, `taken`, `checks`, `conditions`, with one
  method per phase; `fn bind(&self) -> impl Fn(At)`, `fn push_condition(&mut self, expr, id, label, site) -> Result<(), Skip>`,
  `fn mark_source(&self, p, id_field) -> Result<MarkSource, Skip>`, `fn field(&self, r, wire) -> Option<Field>`.
  Keep the phase order exactly: `inputs` indices and `taken` name suffixes depend on push order.
- Size: ~500 lines touched, ~-100 net. Risk: medium; input order and fresh-name numbering are output. Pinned by the
  three codegen snapshots, `codegen_behaviour.rs` (runs generated code) and `codegen_equivalence.rs`. Do last in codegen, after #1, #8, #10, #11.

## 17. Scope visibility as `Option<HashSet<NodeRef>>` in seven places  [obj 2]
- `reachable.as_ref().is_none_or(|set| set.contains(nr))` or the `match { Some(set) => set.contains, None => true }` form at
  `mermaid.rs:438`, `:790-795`, `mermaid/condition.rs:58`, `mermaid/information.rs:31`, `mermaid/requirement.rs:68`,
  `mermaid/context.rs:122`, `mermaid/flow.rs:33`, `diagram_common.rs:67` (`scoped_node_visible`).
- Change: `enum NodeScope { All, Only(HashSet<NodeRef>) }` with `contains`. Keep each diagram's own constructor
  (`buc_scope` vs `reachable_from_bucs` vs `entity_neighbourhood` differ on purpose).
- Size: ~40 lines. Risk: none. Value modest: the idiom is short; do only alongside #7.

## 18. Node declaration line + desc line pair repeated  [obj 4]
- `node_line(..)` followed by `if let Some(desc) = desc_line(..) { push "{indent}{desc}\n" }` at `mermaid.rs:78-86`, `:94-103`,
  `mermaid/requirement.rs:45-59`, `diagram_common.rs:663-667`, and in business/flow/screen/information/condition.
- Change: `declare(out, model, node, indent, text: Option<String>)` in `diagram_common`, used everywhere.
- Size: ~-50 lines. Risk: low; indentation per site is output (pinned by mermaid tests, diagram_contract). After #7.

## 19. Stringly typed diagram kind  [obj 2]
- `"rdra"`, `"context"`, `"event"`... are passed as `&str` to `Header.diagram`, `edge_line(diagram, ..)`, `render_edges`,
  and are the key of `EDGE_RELATIONS` (`diagram_common.rs:187`). A typo is a silent "no implied relation".
- Change: `enum Diagram` with `as_str`; `EdgeRelation.diagram: Diagram`. `edge_relations`/`implied_relation` are `pub`
  and called with `&str` by `crates/rdra-ish-cli/tests/diagram_contract.rs:262,423`, so keep a `&str` entry point.
- Size: ~80 lines. Risk: low. Value moderate; rank low because the contract test already cross-checks every edge.

## 20. TransitionSystem built repeatedly per command  [obj 1]
- `TransitionSystem::build(model, Bounds::STATES, &Scope::ALL)` is called independently by `sql.rs:101` (only to read
  invariant names and entity ids), `codegen/mod.rs:326`, `diagram_common.rs:19` (state diagram start states); `prove`
  reaches it again through `sql::unenforced_rules`. The CLI also builds it for `check`.
- `sql.rs:115` `obligations()` and `sql.rs:150` (table loop) each call `entity_rules` for every entity: computed twice per document.
- Change: let callers pass the built system (the `StatesEmitter`/`Explored` path already exists for this); compute
  `entity_rules` once per entity in `document`.
- Size: ~60 lines, signature changes. Efficiency: guess; TS build cost lives in core and was not measured. Measure a
  `--all` export on the largest example first; skip if under a few ms. Independent.

## 21. Quadratic relation scans in diagram emitters  [obj 1]
- `mermaid/context.rs:101-114` `bucs_of` calls `buc_members` (3 full relation scans with `Vec::contains`) for every BUC, for every
  `performs` relation: O(P x B x R). The `Triggers` arm (`:166-176`) rescans all relations per relation: O(R^2).
- `mermaid.rs:756` `transition_label` scans all `state_transitions` per transition: O(T^2) per diagram.
- `codegen/mod.rs:595` scans all foreign keys per entity; `event_fields` (`codegen/actions.rs:1967`) scans all vars per raise per combination.
- Change (if a profile says so): precompute node -> BUCs and event -> raisers maps once per emit.
- Efficiency: inferred asymptotics; at model sizes of hundreds of relations these are sub-millisecond (guess).
  Rank low; do not refactor for speed without a measurement.

## 22. ER relation order sorts by Debug strings of slotmap keys  [obj 4; NOT behavior-preserving]
- `mermaid.rs:849` `er_rels.sort_by_key(|(_, r)| format!("{:?}{:?}", r.from, r.to));` orders by the text of
  `Entity(EntityKey(10v1))`, which sorts `10` before `2`: neither declaration order nor id order.
- Leave it in a behavior-preserving pass. Fixing it (sort by entity ids) changes `er_mermaid_snapshot.snap` for any
  model with 10+ entities; do it as its own deliberate change if wanted.
- `lib.rs:363` `er_glyph` scans all foreign keys per relation (O(R x F)); negligible (inferred).

---

## Leave as is
- `tla/` (outline read only): functions are short and single-purpose; names.rs owns the TLA naming contract.
- `mermaid/sequence.rs` (`play` is 227 lines but is one linear script of the diagram), `sql/reads.rs`, `json_schema.rs`,
  `openapi.rs`, `asyncapi.rs`, `dbml.rs`, `markdown.rs`.
- The per-target string quoting helpers (`diagram_common::quote`, `tla::quote`, `sql::string`, `codegen::string_literal`,
  `typescript::ts_string_literal`, `dbml_string`): each escapes for a different target; do not unify.
- `typescript.rs:175` `to_pascal_case` vs `codegen::type_ident`: they differ (space handling, `V` prefix, `"Entity"` fallback);
  merging changes output.
- The `ir::DomainModel` design: it already is the shared representation the three printers read. A further printer
  abstraction does not earn its place (see #13).
- Parallelism: no finding earns it. Every emitter is a single pass of string building over a model of hundreds of nodes
  (guess: low milliseconds). The only fan-out with independent units is "emit every kind" (`--all`), which lives in the CLI,
  not this crate; `Emitter` takes `&SemanticModel` and returns `String`, so the CLI can already run emitters in parallel
  without changes here.

## Suggested order
#1, #2, #4, #5, #9, #15, #12 (each tiny, zero risk) -> #3, #14 -> #6 -> #7, #8 (pure moves) -> #13, #17, #18, #10, #11 -> #16.
#19-#21 only with a measured or stated need; #22 only as a deliberate output change.
