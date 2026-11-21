# Audit: crates/rdra-ish-core/src/lifecycle/

Goal: a behavior-preserving refactoring plan for the lifecycle layer. Covers efficiency (1), algebraic modeling (2), file split (3), reviewability (4), workarounds (5), and comments (6). Read-only audit. No cargo runs. Every efficiency claim is inferred from reading the code, and none is measured.

Scope read: all non-test files in depth for reduce.rs, types.rs, mod.rs, targets.rs, explore/{mod,bfs,moves,graph}.rs, cone.rs, restrict.rs, prove/mod.rs, build/mod.rs, and diag.rs. The rest (findings.rs, trace.rs, view.rs, build/*, prove/{encode,mine}.rs) was read through targeted greps for duplication, long functions, and comments. Test files were read only to see what pins behavior.

Key test pins used below:
- **R**: `lifecycle/tests/reduce.rs`. `assert_reduction_exact` compares reduced and unreduced verdicts and views over the sample models. `assert_real_path` checks that every lifted trace is a real path. There are also nested-reduction, fold, and cell-count tests.
- **D**: `scripts/reduction-diff.py`, which runs `verify` reduced and unreduced and compares the verdicts.
- **E**: the explore tests (`lifecycle/tests/{system,effects,mod}.rs`, `explore/tests.rs`).
- **P**: `rdra-ish-cli/tests/{prove_cli,proof_equivalence}.rs`.

Findings are ranked by value over risk, highest first.

---

## 1. The component-exploration path exists only for tests [obj 5, 3, 6]

- **Where**:
  - `explore/mod.rs:165-179` (`explore`, `Exploration`).
  - `restrict.rs:1-215` (`Component`, `Footprint`, `components`, `restrict(&Component)`, the `*_footprint` helpers).
  - `view.rs:189-213` (`Exploration::diags`, `Exploration::entity_views`).
  - `ComponentRun.component` (`explore/mod.rs:48`).
  - The module doc at `mod.rs:4-7`.
- **What is wrong**: production goes through `explore_targets` / `explore_targets_reduced` (`states.rs:260`, verify). `ts.explore(` is called only from `lifecycle/tests/{effects,mod,system}.rs` and `rdra-ish-cli/tests/codegen_equivalence.rs:270`. `ComponentRun.component` is written in `run()` and in `explore()` and is never read. The crate doc still describes this path as the pipeline: "`explore` runs a BFS per independent component (`components`, `restrict`), and `Exploration::entity_views` projects…".
- **Change**:
  - Delete `explore()`, `Exploration`, its `entity_views` and `diags`, `components()`, `restrict(&Component)`, `Footprint`, and the `component` field.
  - Migrate the 7 call sites to `ts.clone().run(caps)`, or to `explore_targets` where the tests want views.
  - Drop the tests that assert component splitting itself (`independent_entities_are_separate_components` and `comps.len()` in `tests/system.rs`).
  - Rewrite the `mod.rs` doc to describe the per-target cone path.
- **Size**: about -300 / +30.
- **Risk**: none on production output. Test coverage of component independence goes away with the feature. `codegen_equivalence.rs` needs a mechanical port to one whole-system run. A whole-system run can be bigger than the per-component runs, so keep `Caps::STATES` or raise it if that test caps out.
- **Independent**: yes.

## 2. `lift` re-explores and re-partitions the whole sub-lifecycle on every lifted trace [obj 1, 5]

- **Where**: `reduce.rs:1768-1792` (`lift`), with the copy at `reduce.rs:820-838` (`Quotient::of`). Callers are `explore/graph.rs:91` (`trace_to`) and `view.rs:314`, which builds one witness per reachable view pattern.
- **What is wrong**: each call does `Extended::rebuild(self, r)`, then `sub.clone().run(Caps::new(usize::MAX, usize::MAX))`, `split_edges`, `quotient_blocks`, `Succ::new`, and several `HashMap`s over all run states. It does this again recursively for each nested reduction (`sub.lift(..)` at 1866).
  - The work is identical every time for a given `Reduction`. It also repeats the run and partition `Quotient::of` already did, at lines 826-838, which are the same 13 lines as 1780-1792.
  - On a reduced entity cone, `entity_view` lifts once per pattern. That is up to `Caps::DEFAULT_PATTERNS` = 256 lifts, each costing a full O(N + E) exploration plus the stuttering partition, where N is the sub-lifecycle's state count (the doc at reduce.rs:39-44 cites cones of 2,428 to 24,000 classes).
  - Complexity is O(traces × (N + E) × refinement rounds) where O(traces + (N + E) × rounds) would do. This is inferred: the cost matters once a reduced cone has dozens of patterns or violations and N is in the thousands.
- **Change**: add a `LiftContext { ext, run, blocks, at, block_of_class, succ_index }` built once per `Reduction`. Hold it in a `OnceLock` on `Reduction`, or build it in `reduced_with` from the run `Quotient::of` already holds. Then extract `fn partition(run, ext) -> Vec<u32>` and have both `Quotient::of` and the context call it.
- **Size**: about +50 / -35.
- **Risk**:
  - `lift` must keep picking the same `s0` and the same `shortest_path`. Both are deterministic given the run, so the traces stay byte-identical.
  - Memory: the context keeps the sub-run alive as long as the reduced system. That is bounded by the caps already applied in `Quotient::of`.
  - There is one subtle difference. `Quotient::of` runs `raw.reduced(..)` under `caps`, while `lift` runs `Extended::rebuild` with `r.inner` and uncapped caps. These should be the same system, because `Quotient::of` returns `None` unless the run completed. Assert that once in debug builds.
  - Pinned by R (`a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace`, `the_job_cone_nests_two_reductions`, `a_class_move_of_a_folded_action_binds_the_cones_params`) and D.
- **Measure first**: time `verify` on the insurance-claim-ops sample with and without the cache.
- **Independent**: yes. Doing finding 13 first makes it easier.

## 3. `named_params` exists twice, and the reduce.rs copy is a clone-and-RefCell hack [obj 5]

- **Where**: `reduce.rs:969-974`, with the original at `cone.rs:20`.
- **What is wrong**:
  ```rust
  let named = std::cell::RefCell::new(Vec::new());
  let mut copy = pred.clone();
  super::restrict::renumber_pred(&mut copy, &|p: &mut u8| named.borrow_mut().push(*p));
  ```
  This clones the predicate in order to use a mutating renumberer as a read-only visitor. `cone::named_params` already does the same job without the clone. It also dedupes, which no caller in reduce.rs depends on: they use `any`, `is_empty`, and `filter` into a `BTreeSet`.
- **Change**: delete the reduce.rs copy and `use super::cone::named_params`.
- **Size**: -7.
- **Risk**: none. Same set of params, and order does not matter at the three call sites (1228, 1537). Pinned by R.
- **Independent**: yes. Finding 16 subsumes it.

## 4. `Fold.node` is `Some` exactly when `key` is `FoldKey::Linked` [obj 2, 5]

- **Where**: `reduce.rs:346-357`, `412`, `573`, `1290`, `1332`.
- **What is wrong**: `node: Option<Pred>` is documented as "For a `Linked` key…", and two sites guard it with `.expect("a linked fold has its node")`. The pair can represent `Param` with `Some`, or `Linked` with `None`.
- **Change**: move `body: Pred` into `FoldKey::Linked { guard, entity, link_var, s_param, body }`. `Fold::node_body()` then becomes a match.
- **Size**: about ±15.
- **Risk**: `FoldKey` derives `PartialEq`, and adding a `Pred` field still derives. It is `pub`, so check `rdra-ish-emit` matches. A grep of `FoldKey::Linked` there is needed before landing. Pinned by R (`a_quantifier_over_t_folds_into_a_bit_keyed_by_the_linked_row`).
- **Independent**: yes.

## 5. `reduced_with` returns `Result<TransitionSystem, bool>` [obj 2, 4]

- **Where**: `reduce.rs:1574-1664`.
- **What is wrong**: the outcome is `Err(true)` or `Err(false)`, and the doc has to explain that "`Err(true)` says the printed tables would pass `TLA_CELL_BUDGET`…". The bool carries a decision the type should name.
- **Change**: `enum NotReduced { OverBudget { retry_without_inputs: bool }, Nothing }`, or `Result<_, Retry>` with `enum Retry { WithoutInputs, No }`.
- **Size**: about ±12.
- **Risk**: none. The function is private. Pinned by R (`a_reduced_cone_counts_the_cells_its_tla_tables_print`, `an_identity_quotient_leaves_the_cone_unreduced`).
- **Independent**: yes.

## 6. Stale section banners and a history label [obj 6]

- **Where**:
  - `build/mod.rs:148`: `// ── R-var and R-start ──` is the last line of an `impl` with nothing under it. The code moved to vars.rs.
  - `build/vars.rs:332`: `// ── Actions: R-create, R-delete, R-update, R-bind ──` also closes an `impl` with nothing under it. The code moved to actions.rs.
  - `build/preds/temporal.rs:223`: `// ── Phase 1's frozen-constraint warning ──` narrates history.
  - The other `R-guard`, `R-after`, `R-prop`, and `R-now` banners name rule ids that appear nowhere in `docs/`.
- **Change**: delete the two orphan banners. Rename the rest to plain topic names, or delete them.
- **Size**: about -10.
- **Risk**: none.
- **Independent**: yes.

The rest of the lifecycle comments are good. They state invariants and reasons, not history. The `(#74)` and `(#112)` issue refs in reduce.rs are acceptable pointers. Leave as is.

## 7. `Guard::reads_successor()` walks the predicate tree on every binding of every state [obj 1, 2]

- **Where**: `explore/moves.rs:190-196` (`pre_guards_hold`, called per binding inside the odometer loop at 167) and `moves.rs:287-292` (the post-guard filter per successor). The definition is at `types.rs:391`, recursing through `Pred::reads_next` at `types.rs:416`.
- **What is wrong**: whether a guard reads the successor is fixed once the action is built. Yet it is recomputed as a full tree walk for each guard, binding, and state in the BFS hot loop. For an action with a deep `holds(..)` guard, that walk is about as expensive as evaluating the guard.
- **Cost**: O(states × Σ bindings × Σ guard size). It roughly doubles guard cost on states where the guard is evaluated. This is inferred, and it matters at the default 65,536 states per cone with multi-param actions.
- **Change**: either split `Action.guards` into `pre: Vec<Guard>` and `post: Vec<Guard>` at build (stronger modeling, larger diff), or cache `reads_next: bool` on `Guard`, set in its constructor (smaller diff).
  - `restrict`, `renumber_pred`, and `Remap::pred` never turn `At::Param` into `At::Next` or back, so the cached flag stays valid through restriction.
  - With the split variant, `GuardWhy::ArmCondition` and `Copy` guards land in `post` by construction.
- **Size**: about +15 / -8 for the cached flag, or about ±80 across build, restrict, findings, encode, and emit for the split.
- **Risk**: guard order inside pre and post is preserved, so evaluation order and the BFS edge order stay the same. Pinned by E, R, and every sample-based test.
- **Measure**: time one large sample's `states` run before and after.
- **Independent**: yes.

## 8. A reduced BFS recomputes the input bits several times per state [obj 1]

- **Where**: `reduce.rs:1923-1962` (`moves_of`), `304` (`moves_from`), `158` (`key`), `170` (`input_bytes`), `183` (`bit_values`), `406` (`Fold::holds`).
- **What is wrong**: for one state `s` the code computes:
  - `r.moves_from(s)`, which calls `class(s)`, `key(s)`, and `bit_values(s)`;
  - `r.input_bytes(s)`, which calls `bit_values(s)` again;
  - and for every T move whose inputs changed, `r.class(self, s)` again, which is a third `bit_values(s)`.
  - Each successor also gets `input_bytes(next)`.
  - `bit_values` costs groups × slots × folds × slots^free_T_params × guard evals (`Fold::holds` enumerates bindings).
- **Change**: compute `bit_values(s)` and the class once at the top of `moves_of`. Pass them into `moves_from`, and reuse them in the map closure.
- **Size**: about +15 / -8.
- **Risk**: none. The functions are pure. Pinned by R and D.
- **Cost**: inferred. It matters for cones with folds, the #112 case.
- **Independent**: yes.

## 9. The rule that `now` is one slot wide is written out 4 times [obj 5]

- **Where**: `mod.rs:118-121`, `reduce.rs:1615-1618`, `explore/bfs.rs:138-140`, `cone.rs:473-476`.
- **What is wrong**: `match var.kind { VarKind::Now => 1, _ => self.bounds.slots() }` appears in each. `layout()` already stores offsets but not widths.
- **Change**: add `pub(crate) fn width(&self, var) -> usize`, or store `widths` next to `offsets` in `layout()`, and use it at the 4 sites.
- **Size**: about +6 / -12.
- **Risk**: none.
- **Independent**: yes.

## 10. Check-reads, `Ltl` operands, and fairness tests are each copied [obj 5]

- **Where**:
  - `reduce.rs:1671-1678` (`reads_ok`) is the same closure as `restrict.rs:360-366` (`reads_inside`).
  - `match &p.formula { Always(x)|Eventually(x) => vec![x], LeadsTo(x,y) => vec![x,y] }` appears at `reduce.rs:1698`, `restrict.rs:399`, and `cone.rs:419` (as two calls).
  - `matches!(&a.origin, ActionOrigin::UseCase { uc, .. } if p.fair.contains(uc))` appears at `cone.rs:427` and `reduce.rs:1704`.
  - reduce.rs also spells `super::types::Pred` and `super::types::Ltl` in full (1671, 1698-1700) although `Pred` is imported.
- **Change**: add `Ltl::preds(&self) -> impl Iterator<Item=&Pred>`, `Ltl::map(&self, f)` (used by restrict.rs:404-408), `Property::is_fair(&Action) -> bool`, and `TransitionSystem::check_reads(binders, preds) -> VarSet`.
- **Size**: about +25 / -40.
- **Risk**: none. Pinned by R (dropped checks) and the property-cone tests in the CLI.
- **Independent**: yes.

## 11. `Quotient::of` maps initial states to run indices by linear scan [obj 1]

- **Where**: `reduce.rs:867-871`.
- **What is wrong**:
  ```rust
  let at = run.states.iter().position(|x| *x == canonical)
  ```
  This runs once per initial S state, so it is O(I × N), where I is the number of initial S states and N the number of run states.
  - I is the product of the domains of every `Start::Any` var, so a pre-existing entity with two variation columns at 2 slots already gives dozens. N is in the thousands.
  - The run's initial states come first in discovery order, so the scan stops after at most I steps. It is O(I²) in practice. Inferred to be minor.
- **Change**: build a `HashMap<&State, u32>` over `run.states.iter().take(I)` once. `lift` already builds the same map (`at`, 1793).
- **Size**: about ±5.
- **Risk**: none.
- **Independent**: yes. It merges naturally into finding 2.

## 12. `Extended::build` reconstructs the action mapping with an O(A²) scan [obj 5, 1]

- **Where**: `reduce.rs:513-524`.
- **What is wrong**:
  ```rust
  .position(|b| b.source == a.source).expect("an action of the extended sub-lifecycle is an action of the cone")
  ```
  This inverts a mapping that `restrict_to_mapped` just returned as `map.act: Vec<Option<ActId>>`.
- **Change**: build `acts` by inverting `map.act`, and drop the `expect`.
- **Size**: about ±8.
- **Risk**: an id collision on `source` would make the result differ. Restriction keeps `source`, so the two are equivalent. Pinned by R (`a_sub_lifecycle_action_that_shares_a_name_with_a_target_step_keeps_its_identity`).
- **Independent**: yes.

## 13. Split reduce.rs (2,183 lines) by phase [obj 3, 4]

- **Where**: `reduce.rs`.
- **What is wrong**: one file holds six concerns:
  - the `Reduction` API and state-key encoding (26-328);
  - fold and bit modeling (341-451);
  - extended sub-lifecycle construction (453-759);
  - partition algorithms (`quotient_blocks`, `stuttering_blocks`, `Adj`, `Succ`, `acyclic`: 761-910, 1965-2183);
  - split and fold analysis (912-1570);
  - reduced moves and trace lifting (1571-1963).
  
  A reviewer of a fold change pages through graph algorithms.
- **Change**: `reduce/{mod.rs, fold.rs, extended.rs, partition.rs, split.rs, lift.rs}`. This is a pure move with `pub(super)` visibility.
- **Size**: about 0 net, 2,183 lines moved.
- **Risk**: none. Land it as a move-only commit so `git diff -M` shows it as renames.
- **Independent**: yes. Do it before findings 2, 14, and 15.

## 14. `Extended::build` is 233 lines with two fold kinds and the flip synthesis inline [obj 4]

- **Where**: `reduce.rs:502-735`.
- **What is wrong**: the `FoldKey::Linked` arm (565-629) builds a stand-in predicate, a new param, and a bit group, then `continue`s. The `Param` arm (631-666) builds a different group. A third block (678-717) synthesizes the Rise and Fall actions.
- **Change**: extract `fn fold_linked(..)`, `fn fold_param(..)`, and `fn flip_actions(bit_vars) -> Vec<Action>`, with the shared `groups`, `bits`, `bit_vars`, and `variants` in a small `BitGroups` struct.
- **Size**: about ±60 moved.
- **Risk**: group order and `ActId` numbering of the flips must stay as they are, because the TLA output prints them. Keep the iteration order. Pinned by R and D.
- **Independent**: after finding 13.

## 15. `Succ::new` duplicates `Adj::new` [obj 5]

- **Where**: `reduce.rs:1975-1996` and `reduce.rs:2163-2178`.
- **What is wrong**: both build the same CSR prefix-sum and fill. `Succ` indexes edge ids, and `Adj` indexes targets.
- **Change**: `Succ { adj: Adj::new(n, &edges.map(|e| (e.from, k))) }`, or make `Adj` generic over the payload.
- **Size**: about -15.
- **Risk**: the order within a source must stay the edge-discovery order, because `shortest_path` picks the first path found. The counting-sort fill preserves it. Pinned by R (lifted trace equality).
- **Independent**: after finding 13.

## 16. The `Pred` term walk is hand-written about 8 times [obj 5, 4]

- **Where**:
  - `types.rs:416` (`reads_next`)
  - `reduce.rs:948` (`names_bound`)
  - `cone.rs:20` (`named_params`)
  - `cone.rs:77` (`pred_reads`)
  - `restrict.rs:~50-85` (`pred_footprint`, deleted by finding 1)
  - `restrict.rs:470` (`renumber_pred`, mutable)
  - `build/preds/mod.rs:33` (`collect_vars`)
  - `tests/cone.rs:424`
- **What is wrong**: each repeats the same 12-line shape: True/False, then Cmp, then In, then Not, then And/Or, then Forall/Exists. A new `Pred` variant means editing all of them.
- **Change**: add `impl Pred { fn for_each_term(&self, f: &mut impl FnMut(&Term)); fn for_each_quantifier(..) }`, or one visitor that reports `Term` and the quantified `EntId`, plus a `_mut` twin for `renumber_pred`. Rewrite each walker as a 3-line closure.
- **Size**: about +35 / -70.
- **Risk**: `pred_reads` must still insert a quantifier's `exists` var. Visit order does not matter because every walker collects into a set or an `any`. Pinned by every cone and restrict test.
- **Independent**: yes. Do finding 3 first or fold it in.

## 17. Variant grouping is keyed on `format!("{:?}")` strings [obj 5, 1]

- **Where**: `reduce.rs:574` and `637-645`.
- **What is wrong**:
  ```rust
  let signature = format!("{:?}", (&a.params, a.guards.iter().map(|g| &g.pred).collect::<Vec<_>>(), &a.effects, local_key));
  ```
  Equality is smuggled through Debug text because `Pred`, `Effect`, and `Param` do not derive `Hash`. It works, but the key depends on Debug output, which is not a stable contract. It also formats every action's whole guard and effect tree.
  - `&a.effects` includes each effect's `Site`, `LocatedSpan` included, so two variants with different spans never merge. That is the current behavior and must be kept.
- **Change**: derive `Hash` on `Term`, `Pred`, `CmpOp`, `At`, `Rhs`, `Effect`, `Param`, `ParamRole`, `Site`, and `ModelRef` (`LocatedSpan` must be `Hash`). Key on owned clones in a tuple.
- **Size**: about +10 / -12.
- **Risk**: the grouping must stay the same. Debug-equal implies field-equal for these derived types, and the converse holds too, so groups stay identical. Pinned by R (`a_class_move_of_a_folded_action_binds_the_cones_params`).
- **Independent**: yes.

## 18. `prove` keeps candidate status in a map named `fell` that also holds "waiting" [obj 2, 4]

- **Where**: `prove/mod.rs:134-140` and `168-368`.
- **What is wrong**: `Fall::Held` means "a fine lemma not yet in the set", but it lives in `fell: BTreeMap<usize, Fall>`. Several places compensate:
  - the result loop has `Some(Fall::Held) => unreachable!("only a fine lemma waits")`;
  - the retain at 300-306 has to special-case `Held`;
  - `!fell.values().any(|f| matches!(f, Fall::Held))` decides whether to run again.
  
  `prove()` itself is 200 lines with three phases (initiation, Houdini consecution, CTI extraction) inline.
- **Change**: `enum Status { Kept, Waiting, Fell(Fall) }` with `status: Vec<Status>` per candidate, and `Fall` without `Held`. Then split into `initiation()`, `consecution()`, and `extract_ctis()`.
- **Size**: about ±90.
- **Risk**: the solver call order must not change, because Z3 `Unknown` results can depend on the push/pop history. Keep the loop order exactly. Pinned by P and `tests/tx_groups.rs`.
- **Independent**: yes.

## 19. `Effect::Transition` and `Effect::Choose` keep arms in parallel vectors [obj 2]

- **Where**: `types.rs:489-507`. Consumers are `findings.rs:306,527-545,728`, `trace.rs:265,279`, `build/transitions.rs:55`, and `rdra-ish-emit/src/codegen/actions.rs:2081`.
- **What is wrong**: `arms`, `sites`, and `conds` are kept index-aligned. For `Choose`, the doc allows `sites` to be empty ("a creation's own choice of a variation value has none") and `conds` to be "empty, or empty throughout".
  - Consumers cope in different ways. `findings.rs:543` zips, which silently yields no arms when `sites` is empty.
  - `trace.rs:265` indexes `sites[i]` for `Transition`, but `trace.rs:279` uses `sites.get(i)` for `Choose`.
  - These are guards for a shape that a struct would make impossible.
- **Change**: `arms: Vec<Arm { from, to, site: Site, conds: Vec<(ConditionKey, String)> }>` and `values: Vec<Choice { value, site: Option<Site>, conds }>`. `successors` (moves.rs:224, 236) then reads `arm.from` and `arm.to`.
- **Size**: about ±150 across 28 match sites in core and emit.
- **Risk**: the `findings.rs:543` zip behavior on creation choices must be kept: skip values with no `.when` conds, which the existing `if conds.is_empty() { continue }` already does. TLA and codegen emitters read these fields, so run the emit golden and equivalence tests.
- **Independent**: yes. This is the largest diff here.

## 20. `Builder` keeps `diags` and `altered` as parallel vectors [obj 2]

- **Where**: `build/mod.rs:53-56`, `119-133`, `91-102`.
- **What is wrong**: `/// Parallel to `diags`: the entities of the actions each diagnostic altered.` Two vectors must stay in step, and `finish` re-zips them.
- **Change**: `diags: Vec<(Diag, Vec<EntityKey>)>`.
- **Size**: about ±12.
- **Risk**: none.
- **Independent**: yes.

## 21. Small dead guard and duplicate block in `targets.rs` [obj 5]

- **Where**: `targets.rs:607-609` and `targets.rs:243-251` / `264-272`.
- **What is wrong**:
  - `verdict()` starts with `if matches!(target, Target::Entity(_)) { return Vec::new(); }`, but its only caller (230-232) routes `Entity` elsewhere.
  - The "entities of written params, then push a `Diag`" block is written twice in `explore_targets_with`.
  - `entity_views` (749-756) dedupes with a `fold` plus `contains`, where an `IndexSet` or a `seen` set would do.
- **Change**: take `Target` minus `Entity` in `verdict` (an `enum CheckTarget { Invariant(usize), AfterAssert(usize) }`), or `debug_assert` the precondition. Extract `fn written_entities(&Action) -> Vec<EntityKey>`.
- **Size**: about ±20.
- **Risk**: none.
- **Independent**: yes.

## 22. `Reduction::rises_from` scans every rise per class [obj 1]

- **Where**: `reduce.rs:267-276`. The caller is `rdra-ish-emit/src/tla/module.rs:169`, once per class.
- **What is wrong**: `rises: HashMap<(u32, Box<[u8]>), u32>` is filtered by class and then sorted, so printing all classes is O(C × R). `TLA_CELL_BUDGET` bounds C × (obs + s_actions) + R ≤ 200,000. The worst case is about 4k classes × 20k rises, or 8×10⁷ hash-entry visits. Inferred: noticeable only near the budget.
- **Change**: store `rises: BTreeMap<(u32, Box<[u8]>), u32>` and use `range((class, [])..(class+1, []))`. The output comes back already sorted, in the same order as `sort_unstable` on `(&[u8], u32)` with the class fixed.
- **Size**: about ±10.
- **Risk**: `rise()` lookups become O(log R). The order the TLA printer sees is unchanged.
- **Independent**: yes.

## 23. Synthetic input bits are dressed up as run marks [obj 2, 5]

- **Where**: `reduce.rs:531-557` and `678-717`.
- **What is wrong**: a bit var is built as `VarKind::Run { usecase: format!("in_{name}") }`, read through `GuardWhy::Run { step: 0 }`, bound through `ParamRole::Earlier { step: 0 }`, and flipped by actions with `origin: ActionOrigin::Clock`. A later reader has to know that `Run`, `Earlier { step: 0 }`, and `Clock` may each mean "input bit". The code even writes:
  ```rust
  let VarKind::Run { entity, .. } = .. else { unreachable!("a bit is a run mark") }
  ```
  The encoding works only because these variants happen to give the right binding and printing semantics.
- **Change**: two options.
  - Minimal: keep the encoding, store the bit's entity next to `bit_vars` so the `unreachable!` goes away, and name the three choices in one doc comment on `new_bit`.
  - Full: `VarKind::InputBit { entity, name }`, `GuardWhy::InputBit`, and `ActionOrigin::InputFlip`. Every exhaustive match in core and emit then needs an arm, and `var_name` and the TLA names must render exactly as today.
- **Size**: minimal about ±10. Full about ±80.
- **Risk**: with the full variant, the TLA module text changes unless the name rendering is kept. No golden test pins the `_run_in_` names; only D compares verdicts. Recommend the minimal change unless a third synthetic kind shows up.
- **Independent**: yes.

---

## Leave as is

- **`diag.rs`** (1,077 lines): one flat enum with an exhaustive `severity()` and a long but flat `Display`. Splitting it would not reduce reader load. `trace()` and `trace_mut()` duplicate a 6-line match, which is not worth a macro.
- **`types.rs`**: plain data with good docs. `id_type!` has both `at` (pub(crate)) and `at_index` (pub). That is a visibility shim for emit, so it is fine. `Ltl::kind()` / `LtlKind` is used by the CLI.
- **`stuttering_blocks` / `quotient_blocks`** (reduce.rs:767, 2053): a naive fixed point that re-sweeps every splitter per round, so worst case O(rounds × (N + E)) and not Groote-Vaandrager's O(N·E). It runs once per cone, and finding 2 removes its repeats. Keep it unless profiling says otherwise.
- **`enabled_bindings` fast path** (`explore/moves.rs:118-185`) with its `slots > 16` fallback: a documented, allocation-free hot path. The fallback covers a legitimate bound, so it is not a workaround.
- **`targets.rs::in_parallel`**: a hand-rolled scoped pool with an order-preservation test, and rayon is not a dependency. Cones are dispatched in first-seen order, so one large cone dispatched last stretches wall time. Dispatching by descending `cone.vars.len()` is a guess-level improvement. Measure before acting.
- **BFS storage** (`explore/bfs.rs`): every state is stored in `states` and again as a `seen` key, which doubles state memory. It is bounded by caps of 65,536 states and small widths, so not worth `hashbrown::HashTable` yet.
- **Comments overall**: dense but they explain why. There is no history narration beyond finding 6. Leave.

## Suggested order

Land 13 (move-only split), then 3, 6, 5, 4, and 9 (trivial and independent). Then 2 together with 11, plus 8 and 7, each with a before-and-after timing on one large sample. Then 1. The larger model changes come last: 16, 17, 18, 19, and 23.
