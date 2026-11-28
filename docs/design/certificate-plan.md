# Replacing "read from the code" with certificates: inventory and plan

Status: research draft against main at 6042d0af (2026-11-28). Phase 1a (C9, C10) is
implemented and measured (marked **Done** below); nothing else here is implemented or
measured. Every cost below is an estimate. Each phase has a measurement as its exit
criterion (see [Measurement](#measurement)).

## 0. Method and vocabulary

Sources searched: `docs/theory/{README,claims,cones,native,semantics,tlc-agreement,z3}.md`
and `proofs/lean/README.md`. I grepped them for *read from the code*, *by reading*, *reading of*,
*read off*, *is read*, *argued*, *not checked*, *no harness*, *not mechanised* and *P-full by
reading*: 188 lines, many restating one fact (A8's `Lemma6Hyp` fields recur in A5, A6, A10,
A11 and A15). I then read the claims inventory's Evidence column for "P-full by reading / from
the code / by construction" (A3, A12, A13, B3, B6) and added residual risk 1's common-mode
items (cone choice, property-cone staging, quotient tables, `TlaNames`). Deduplicated, that
gives **36 connection points** (C1–C36). Lines that are not readings of verdict-bearing code
are out of scope: test-only gaps (A14, A24, A27, B7, B9, B11, B13, B14, B18, N5, N13, N14,
T15), A26 (a statement of scope), and the semantics' own uses of "is read" (D5, D16, D17,
D21). Lines mapped to a point that their own text does not name: cones.md:448 (A3, counted
marks) → C8; cones.md:1422 and proofs/lean/README.md:978–981 (`stuttering_blocks`,
`same_partition`) → C7; proofs/lean/README.md:987 (`split.rs::acyclic`) → C6;
proofs/lean/README.md:527 and :620 (the dump code feeding the L checkers) → §4;
claims.md:286–291 (`rebuild`'s premises) → C15.

Certificate kinds, as in the brief: **R** a run-time check every build makes on the run's
own data; **L** a Lean-verified checker over dumped data in CI (corpus only); **TV**
translation validation, an independent re-derivation compared with the artefact;
**K** a Kani harness; **W** a witness the engine emits and a small checker validates.

Failure action. Where an unoptimised path exists (a reduction, a quotient), a failed
certificate falls back to it with a warning, as `split_faults` and `StubCheck` do, and a
`shadow` build panics with `shadow:`. Where there is none (the step relation, the
liveness search, the prover's encoding), the only sound action is `tool_error` for the
cone, and the rule is reported as not decided.

Two ways a fault can go wrong. Counterexamples already replay through the run's step relation
(N16). So the silent faults are (i) a wrong `ok`, (ii) a wrong `proved`, and (iii) a
counterexample that replays in a cone but is not a run of the model (A2). A reduction,
cone or encoding that **loses** steps gives (i) or (ii). One that **adds** steps gives a
spurious counterexample, which replay or a recheck usually exposes. The "Silent?" column
says which direction a fault at that point takes.

## 1. Inventory

"Path": D = default `verify`/`check`; P = `--prove`; T = `--with-tlc` only; S = symmetric
runs. "Today": what backs the fact now (R/M/D/A/K/read). "Silent?": whether a wrong
implementation gives a wrong verdict that nothing in the run sees.

### 1a. Cones, reduction, folds

| # | Claim(s) | Fact now read from the code | Rust items | Today | Silent? | Path |
|---|---|---|---|---|---|---|
| C1 | A8 (A5, A6, A10, A11, A15) | `Lemma6Hyp` syntactic fields: `kept_rd`, `folded_og`, `folded_names`, `other_names`, `eff_names`, `cover` (the "names" half), `eff_s`, `rise_eff`; and, outside the Lean model, `X`'s initial states are `C`'s projections and each bit starts where `C` starts (`fold_initial`) | `restrict.rs::restrict_to`; `reduce/fold.rs::TransitionSystem::fold_of` (`s_keys`, `touched_t`), `::fold_initial`; `reduce/extended.rs::Extended::build` | read; D via `lemma6.rs::check` on test cones | yes: a kept guard reading `T`, an effect naming a free `T` param, or a wrong start makes `X` lose steps or states, giving a wrong `ok` | D |
| C2 | A8 `exact`, A11 | `holds_fast` computes the bit `holdsB` (step (a)); node folds and bit groups are on paper | `fold.rs::Fold::holds`, `::holds_fast`, `::as_node`, `::strip_links`, `push_bits` | K (one action, small bounds); debug/shadow twin `fold/reference.rs` | yes: a bit that is false where it should be true drops an `S` step, giving a wrong `ok` | D |
| C3 | A8 (`local_`, `shape`, `new_writes`), A18 (`Built`, `WF`) | the code's actions are in the slot syntax of `SymActModel.lean`; `candidate_slots` filters by role first; a `New` row has its `Create`; an `Earlier` param has a run-mark conjunct; every term reads its own entity's row | `build/actions.rs`; `explore/moves.rs::candidate_slots`; `types.rs::Action`, `Pred`, `Effect` | read; M on the model | yes, but only through other points: if the syntax is wrong, the Lean theorems do not apply | D |
| C4 | A1, B6, `Local` (2) | a step changes only `Wr(a)` | `explore/plan.rs::check_writes`, `Plan::writes`; `moves.rs::each_successor`; `cone.rs::action_writes` | M in debug/shadow only (`Twin::Writes`); release unchecked | yes: an unlisted write lets a cone drop a writer, giving a wrong `ok` | D, P (step pruning) |
| C5 | `Local` (3) | what a step writes is a function of `Rd(a) ∪ Wr(a)` | `each_successor`, `Plan` | read; no harness (Kani on `each_successor` did not finish) | yes | D |
| C6 | A10 | the guards give `Pre` at each step; `Monotone.lean`'s write graph is not joined to the action model; `split.rs::acyclic` decides `Acyclic` | `split.rs::monotone_vars`, `::acyclic`; `narrow.rs`; `Reduction::rises_from` | read; R checks 5(d) statically, but with `monotone_vars` itself | yes: a non-monotone input breaks the quotient (wrong `ok`) | D |
| C7 | A9 | the sweeps' fallback partition meets `Sweep.Cert`; `stuttering_blocks` computes the marked set | `partition.rs::quotient_blocks`; `partition/reference.rs::stable`, `::stuttering_blocks`, `::same_partition` | read (the partition used is certified, but only the fast one's path is) | yes, on the fallback only | D |
| C8 | A12, A3 | no counter is an input; a counted mark keeps its raiser and handler out of `S` | `split.rs::split_at`, `::monotone_vars`; `cone.rs::plain_reads`; `build/events.rs::count_up`, `::count_down` | P-full by reading; M (`Split.lean`) | yes, in principle | D |
| C9 | A2 | `Trace::free_triggers` lists every step that runs an action reading a mark outside the cone; the exact cone's counterexample is vouched for | `trace.rs::free_triggers`, `::vouched`; `targets.rs::recheck_on`; `cone.rs::exact_target_cone` | **Done (1a):** TV + R, `vouch.rs` | **yes: a spurious counterexample prints as real**; replay inside the cone cannot see it | D |
| C10 | A16 | the lifted start is an initial state of the unreduced cone | `explore/graph.rs::ComponentRun::lifted_path`; `reduce/lift.rs::lift` | **Done (1a):** R, `vouch.rs::lifted_start` | partly: the trace replays from a non-initial state | D |
| C11 | residual risk 1 | the cone choice: the closure under Lemma 2, with its documented exceptions (a latch read inexactly stays out and its guards are dropped; existence vars join without their columns) | `cone.rs::cone`, `::target_cone`, `::property_cone`, `::keep_writers`, `::reach_seed`; `restrict.rs::restrict_to` | P-full; D (`assert_cones_exact` on samples) | yes: a cone missing a writer gives a wrong `ok` | D |
| C12 | A4, A5, native.md:853 | the staging of a property cone's runs (plain, then `_WF`, then `after`) | `cli/src/property_cone.rs`; `verify/native.rs::native_property_after` | argued, "for the order of its runs only" | yes, in principle | D |
| C13 | A13, A14 | budget rejection; dropped checks get their own cone | `reduce/mod.rs::TLA_CELL_BUDGET`, `drop_unreadable_checks` | read / test gap | A13 no (performance only); A14 covered by 5(e) in `split_faults` | D |
| C14 | A15 | nested reduction, run up to `H`, case 4 (one unobserved class read as the partition by input bytes) | `partition.rs::Classes::up_to_h`, `::divergence`; `hypotheses.rs::loop_faults` | P-sketch | yes, on those cones | D |
| C15 | N16 (panics list) | `Extended::rebuild` reproduces `Extended::of`; a restriction that keeps `Tick` keeps `now` | `extended.rs::Extended::rebuild`; `restrict.rs::restrict_to` | not checked / not argued | yes, on a cache hit | D |

### 1b. Symmetry and liveness

| # | Claim(s) | Fact now read from the code | Rust items | Today | Silent? | Path |
|---|---|---|---|---|---|---|
| C16 | A18, A21, T13 | `Canon::canonical` returns a member of the orbit as a function of the state (past `BRANCH_CAP` too); invariance under the group | `symmetry/canon.rs::Canon::canonical` (writes `perm`); `symmetry/mod.rs::Group::apply` | exhaustive unit test, `Twin::Canonical` (groups ≤ 1,296) | membership: **yes** (a non-member merges unrelated states, giving a wrong `ok`); invariance: no (only the state count changes; A21 needs a member chosen as a function of the state) | S |
| C17 | A18 | the Lean model is `Group::apply`, `breaks.rs::scan`, `each_successor`; (G6) outside guards; least BFS depth | `symmetry/breaks.rs::scan`, `Group::apply` | read; paper steps | yes: a missed break gives a wrong `ok` | S |
| C18 | A20 | `Hyp` holds when a class is marked as not moving | `breaks.rs`, `reduce/mod.rs::fixed_why` | read off the definition | no (marking more classes fixed is always sound if `Hyp` holds) | S |
| C19 | N9, N10, A23 | `avoid`'s code meets the Lean acceptance criterion: Tarjan's components, fair-component bits, stops, the backward walk, the cut at `expanded`; orbit version: `is_real`, π per edge | `explore/liveness.rs::ComponentRun::avoid`, `::expanded`, `::fair_steps`; `graph.rs::Adjacency::components`; `orbit/liveness.rs::OrbitRun::decide_liveness` | M on the criterion; code read | **yes: a missed fair cycle is a wrong `ok`**; a wrong extra one is a lasso that replay rejects | D, S |
| C20 | N11 | lasso checks share `moves`, `fair_instance`, `holds_bound`, `triggers` with the search | `lasso.rs::replay_lasso`, `::explain_property`, `::unfair_loop` | R, but shared | no for `ok`; yes for a lasso both sides misread | D |
| C21 | A23 (LiveEqui) | trigger and goal read the same on a state and its image | `lasso.rs::triggers`; `build/preds/temporal.rs::goal` | paper | yes, on symmetric liveness runs | S |

### 1c. Prover

| # | Claim(s) | Fact now read from the code | Rust items | Today | Silent? | Path |
|---|---|---|---|---|---|---|
| C22 | B1 | `Enc::compare` agrees with `expr.rs::compare` (nulls, two-valued reading) | `prove/encode.rs::Enc::compare`; `expr.rs::compare`, `::compare_numbers` | read; differential tests | **yes: a wrong `proved`** | P |
| C23 | B1 (Prop. 2) | the encoding over-approximates: every native init/step satisfies `Enc::initial`/`Enc::step` (effect order, deletes, forks, `Choose`) | `Enc::initial`, `::step`, `::in_domain`, `::one_of`, `::havoc`; `moves.rs::each_successor`, `::initial_states` | P-sketch; K for `one_of`; D (`proof_equivalence`) | **yes: a wrong `proved`** | P |
| C24 | B16, residual risk 2 | a wrong `proved` decides a memory-capped cone `ok`, and nothing at run time sees it | `cli/src/verify/bfs.rs::decide_by_proof`, `BfsCone::decided_by_proof` | nothing at run time | **yes** | P |
| C25 | B2 (Lemma U), B3 | every bound in the printed `--int-range-max` range builds the same system; folding does not depend on `β` | `core/build` (`int_max` sites, `scripts/check-int-bound-sites.py`); `build/preds/atoms.rs::lists_whole_domain`; `cli/src/prove.rs::int_range` | read; reference interpreter on its subset | **yes: `proved` claimed for a bound it does not hold at** | P |
| C26 | H1, H2 | z3 is sound and the printed SMT means the `Expr` | `prove/smt.rs`, `solver.rs` | A | yes (trust base) | P |
| C27 | H4, B12 | each answer is read for the check that asked it | `solver.rs::Z3::exec`, `Solver::check_sat` | A; no test | yes (fails closed on unknown lines) | P |

### 1d. Native store and build, TLC

| # | Claim(s) | Fact now read from the code | Rust items | Today | Silent? | Path |
|---|---|---|---|---|---|---|
| C28 | I1 | the build returns `T_β(M)` (semantics.md D1–D13) | `core/build/*`; `moves.rs::initial_states`, `::each_successor` | reference interpreter on a subset, ≤ 20,000 states, CI only; corpus headers | **yes, common mode for every engine** | all |
| C29 | I1 | semantics.md takes which models are well-formed from the code's error list | `diag.rs` | definitional | n/a | all |
| C30 | N2 | the state table inserts and looks a row up correctly | `explore/table.rs`, `arena.rs`, `segments.rs` | read; K on packing only | yes: a lost row is a missed state (wrong `ok`) | D |
| C31 | N8, N17 | `can_reach` reports the first state in BFS order; output is independent of `RDRA_JOBS` | `reach.rs`, `bfs.rs` | proof / empirical | no (choice of trace only) | D |
| C32 | native.md:848 | `lift_stop` finds a stop whenever one exists | `reduce/lift.rs::lift_stop` | not argued | no (completeness; failure is a fault, not a wrong trace) | D |
| C33 | T1 | no two params of one action that can bind one slot write one variable (`ConflictingEffects` does not enforce it) | `build/mod.rs::Builder::finish`, `Action::slots_written_twice` | assumed | under `--with-tlc` the engines may disagree (`tool_error`); natively the effect order decides | T, D |
| C34 | T1 | the printed module denotes the cone's system (per construct; the reduced step `bits`, `rise_expr` has no argument). TLC's distinct-state count is parsed (`StateModelRan::states`) and printed, but `state_model_disagreements` compares only witnesses and deadlocks | `emit/tla/module.rs::action`, `::effects`, `::bits`, `::rise_expr`; `emit/tla/pred.rs`; `cli/src/verify/state_models.rs` | P-sketch; D; TLC refinement on 21 corpus cones (`emit/tla/refine.rs`) | only if both engines are wrong the same way | T |
| C35 | residual risk 1 | `TlaNames` maps native states to TLA+ names injectively and totally | `emit/tla/names.rs::TlaNames`, `::decoder` | test `tla_names_cover_every_slot` | could hide a disagreement | T |
| C36 | residual risk 1 | the quotient tables the TLA+ module prints are the `Reduction`'s | `module.rs::reduction`, `Reduction::tla_cells`, `ClassTable` | read | could hide a disagreement | T |

## 2. Certificate per point

Each entry gives the kind, what the checker reads, what it shares with the checked code
(this is the residual trust), the failure action, and where it runs, which sets its cost.

**C1, R: `lemma6_faults` in a new `reduce/lemma6hyp.rs`, beside `split_faults`.** It reads
the built `Extended` (`X`'s actions, guards and initial states), each `Fold` (`act`,
`guards`, `t_params`, `key`) and the split's sets. It covers `FoldKey::Param` folds;
node folds (`FoldKey::Linked`) and bit groups stay with C2. It decides, without
`fold_of`'s bookkeeping (`s_keys`, `touched_t`):
`kept_rd`: `pred_reads(g) ⊆ S_v ∪ inputs` for each guard `X` keeps.
`folded_og`: each `fold.guards[i]` is an index into its action's guards.
`folded_names` / `other_names` / `eff_names`: a fresh walker `params_named(&Pred)` /
`params_named(&Effect)` over the AST checks that only `T` params and the key appear in
folded guards, that no other guard names a free `T` param (the link `v[k] = Index(p)` with `p`
the key aside), and that no effect does.
`cover`: every non-role conjunct of an `S_a` action is in `X`'s guards or in a fold.
`eff_s` and `rise_eff`: each effect's value reads (`Effect` operands plus `pred_reads` of
`.when`) lie in `S_v ∪ inputs`.
Initial states: `X`'s initial states are exactly the projections of `C`'s initial states
(`initial_states`, usually few), and each bit's start is `holds` on those states.
It shares `pred_reads` (Kani-proved for depth-1 shapes, T13), `eval` and the AST, but no
`fold_of` code. On failure it returns `Unreduced::Untrusted` and decides the cone
unreduced. It runs once per reduction, at a cost of about zero per state. Classes change:
those A8 fields and the initial-state gap of the Lean model go from read to R. Still read
for A8 after this: `exact` (C2), the syntax tie (C3), node folds and bit groups (C2), the
`cone_params` renumbering, and the nested case (C14).

**C2, R where small, K otherwise.** At reduction time, for each fold whose read
projection (`Fold::reads`, product of the vars' domain sizes × key slots) is at most 2^16
points, enumerate it and compare `holds_fast` with a spec evaluator transcribed from
`Lemma6Hyp.exact` / `holdsB`: enumerate every binding with the key at `c`, require presence for
each must-present `T` param, and evaluate the folded guards with `eval`. Do not reuse
`fold/reference.rs`, which is a twin and not the spec. The checker shares `eval` and
`is_present`. On a mismatch it decides the cone unreduced. The cost is once per fold. A
fold past the limit gets the
widened Kani harnesses (node folds, `<`/`=`/`!=` between columns, 3 `T` params, slot count 3,
a group's disjunction); the check stays read there, and the run says so through a counter
(`RDRA_ISH_SHADOW_STATS`-style).

**C3, R and L.** R: `slot_syntax_faults(&TransitionSystem)` at the end of `build`. It
checks that each action is in the grammar `SymActModel.lean` takes: role conjuncts first,
then `og`; every term reads a row of its own entity (`WF`); each `New` param has its
`Create`; each `Earlier` param has a run-mark conjunct (except `Earlier { step: 0 }` in `X`,
which A18 lists and which has to become an explicit grammar case). On failure it is a
`tool_error` for the run, since there is no fallback for the build, or it switches symmetry
and reduction off for the model, which is sound. Cost: once per build. L: dump each
corpus model's actions in that grammar and run a Lean decision procedure for `Built` and
the grammar, with `decide`, as `PartitionCheck.lean` does. In both, the remaining reading is
that `each_successor` executes the syntax as the Lean semantics does. That is C4, C5 and
C28.

**C4, R, gated on measurement.** Keep `Plan::writes` in release builds as a word mask
(`Vec<u64>`, one bit per byte, so RSS stays flat: it is per action, not per state). In
`each_raw_move`, after `holds`, check that `(s ^ next) & !mask` is zero word by word. The
successor and `s` are hot in cache, since the table hashes `next` next. Expect less than 1% of
instructions; the measurement decides. A failure is a `tool_error` for the cone (no
fallback: the whole system runs the same step). It shares only the packing. Classes
change: A1 and B6 go from M (debug) to R.

**C5, not certifiable at run time.** The functional dependence on `Rd ∪ Wr` needs two
states that agree on `Rd ∪ Wr` and differ elsewhere. C28's independent step comparison
covers it on its subset. Otherwise it stays read, with the note that C3 plus a Lean
semantics of the slot syntax would derive it.

**C6, R in `hypotheses.rs`.** `rise_faults` already walks each explored `T` step of the
reduced run and its input bytes before and after. Add: every changed input byte moves to a
value reachable from the old one in that var's write graph (`monotone_vars`' graph,
recomputed as `split_faults` does). The cost is per rise edge, which is already visited.
`acyclic`: have it return a topological order of the write graph (self-loops aside) as a
witness, and check that every edge respects it. That is linear and once per split. The
Lean join of `Monotone.lean` to the action model is separate Lean work, not a certificate.

**C7, R.** Run `partition/reference.rs::stable` on the sweeps' partition too when the
fast one falls back. If that fails, the cone is unreduced. This runs only on the fallback
path, so it costs about zero. For `stuttering_blocks`, check that the marked set is closed
under the sweep's two rules. That is linear, and it is enough if the certificate needs only
closure. Whether `Sweep.Marked` must be the least such set is to be confirmed in
`Sweep.lean`; if it must, leastness stays read.

**C8, R.** Two clauses in `split_faults`: no var that `is_counted` marks is an input (a
new `Clause::Counter`), and every raiser and handler of a counted mark is in `T_a`. Free.

**C9, TV and R, on counterexamples only. Done (1a).** `free_triggers` re-derived: for each step of
the trace, test `action_reads(a) ∩ marks ⊄ cone.vars` directly from the action, not from
the cone's bookkeeping, and require equality with `free_triggers`' list. Promote the
`debug_assert!`s of `targets.rs::recheck_on` and `cone.rs::exact_target_cone` to checks
that make the violation a `tool_error`. The cost applies only to cones with a candidate
counterexample. This closes the main silent path to a wrong `violated`. N11's shared lasso
code (C20) and A24's cap-stopped counterexamples are the others.
As built (`core/vouch.rs`): the free triggers are read again off each step's guards with
`pred_reads`, not through `action_reads`, which goes through `mark_reads`, the code the
list is built from. A mismatch, or a step whose action the system lacks, adds what the list
lacks, so the trace stays a candidate and `recheck_on` decides it (fallback); the run warns.
`exact_target_cone` falls back to the whole system when its cone leaves a mark out or frees
slots; an exact round whose counterexample is not vouched for is `Recheck::Fault`, a
`TraceFault` (`LassoFault::Unvouched`), so the cone's verdict is `tool_error`. Each fault
panics with `shadow:` in a debug or `shadow` build.

**C10, R. Done (1a).** In `ComponentRun::lifted_path`, check that the lifted path's first state is in
`initial_states` of the unreduced cone (enumerate it, or test it against the init
predicate). A failure is a `ReplayError`. This runs only on counterexamples.
As built: the check sits at the reduced exit of `reduce/lift.rs::lift`, so every caller
(`ComponentRun::lifted_path`, TLC replays, `lift_cycle`, the orbit traces, the inner lift)
goes through it. `vouch.rs::is_unreduced_initial` tests the state against the initial-state
definition and enumerates nothing. A failure is `ReplayError::NotInitial`; in `lift_cycle`
it makes the reduced lasso spurious, and the property is decided unreduced.
Measured for 1a (release, `RDRA_JOBS=1`, `verify` at the default bounds, stdout, stderr
and the TLA+ output byte-identical to main): patent-ops 45.13–45.17 → 45.05–45.09 G
instructions retired, max RSS 187–193 MB on both; clinic-ops 141.37–141.46 → 141.19–141.26
G instructions, max RSS 996–1002 MB → 1007–1013 MB. Neither sample has a counterexample,
so the checks run only in `lift` and `exact_target_cone`. A build of the branch with both
calls removed, so no new code runs on clinic-ops, gives 1011–1016 MB too: the 1% is the
build, not the checks.

**C11, TV.** Check the invariant Theorem A1 needs, not minimality:
`cone_faults(system, cone)` checks two things on the restricted system that `restrict_to`
returns. (1) Every action of the whole system that writes a var of the cone (by
`action_writes`) is kept. (2) Every guard and effect that the restricted system keeps reads
only vars the cone holds. Guards on a left-out latch must be gone; that is the documented
exception, and the havoc of dropped guards only adds steps. Existence vars that join without
their columns pass (2), because no kept term reads those columns. This needs no re-derivation
of the latch rule, so the cone's choices stay free and only the superset property is
certified. For an exact cone (`exact_target_cone`), also check (3): no guard was dropped.
It shares `action_reads` / `action_writes`, which C4 and the `pred_reads` Kani harness
certify. On failure the run uses the whole system or reports a `tool_error`. It runs once per
cone. Confirm (2) against `restrict_to`'s havoc of freed slots (A28) before writing it.

**C12, mostly not certifiable.** The ladder's soundness is a case analysis over verdicts.
Partial R: each property cone passes C11 against `property_seed`. The order of the runs stays
argued. A Lean model of the staging table, a small finite case split, would make it M.

**C13, C18, C27, C29, C31, C32: no certificate.** C13 is a performance knob (A14 is
covered by 5(e)). C18 needs a small Lean theorem (`Hyp` is monotone in the set of fixed
classes). C27 (H4) wants fuzzing of the answer reader, and H4 stays in the trust base. C29
is a definition. C31 and C32 affect only which trace is printed, or completeness.

**C14, not feasible as a certificate.** A run-time check would need the unreduced `C`,
which the reduction exists to avoid. What remains is Lean work (the nested induction) and
the corpus's `C ⇒ R` TLC refinement. An option for the owner: a policy knob that turns
off nested reductions and runs up to `H` in a "certified" mode.

**C15, R.** On a cache hit, compare `Extended::rebuild`'s system digest with one stored at
`Extended::of` time (one `u64` per entry). In `restrict_to`, assert that keeping `Tick`
keeps `now`, statically. Free.

**C16, R, gated on measurement.** After each `Canon::canonical`, check that
`Group::apply(perm, s) == out` and that `perm` respects the classes (moves only movable
slots, maps each class to itself). This proves membership in the orbit on the run's own
states. It costs O(width) per canonicalization, plus one scratch buffer per worker and no
per-state memory. A failure is a `tool_error` for the cone, or the cone is rerun without
symmetry. The other half of A21's reading, that the result is a function of the state, stays
read: `Canon` reuses scratch between calls. Add a K harness that calls `canonical` twice on
one state with the scratch left dirty by a different state in between, on 2+2 slots, with
the cut forced, and one for invariance (T13's gap). Invariance affects state counts only.

**C17, L, plus an R re-check written from the definitions.** L: dump each corpus cone's
group, actions and checks in the slot grammar, and run `SymCheck.lean`, a decidable (G1)–(G6)
checker proved to imply `a18_holds_iff`'s hypotheses. R: a second (G1)–(G6) pass written
from cones.md's definitions. It is a twin of `scan`, so label it as one, but it is written
against the spec text. A failure removes the class from the group, which is sound.

**C19, W, the core item.** `avoid` (`explore/liveness.rs`) already holds `comp` per state,
`in_n`, `stops` and `toward`. `Adjacency::components` numbers components "in the order they
close (a component comes before every component that reaches it)"; its test asserts sinks
first. So every edge goes from a number to one no larger. `liveness_search` then takes, per
binding, the first state with `toward != NONE`, `triggers` and the cause. A linear checker in
a new `explore/liveness/certify.rs`, run after each `avoid` call whose binding finds no start,
checks five things:
1. For every edge inside `N` (re-derived from `holds_bound(goal)` on both ends):
   `comp[from] >= comp[to]`. Then every cycle of `N` lies inside one computed component,
   whatever Tarjan did. A component coarser than an SCC can only add a fair cycle, which the
   lasso replay rejects.
2. Recompute `size`, `open` (from `expanded`, which is N10's cut), `always` (from
   `fair.enabled`) and `taken` (from the edges, `e.from != e.to`, the same component) in
   fresh code, then `fair_cycle`, and compare. Otherwise the check reuses the arrays it is
   meant to check. This needs transient arrays per component, which `avoid` already
   allocates; free them before the next binding so peak RSS stays the same.
3. Every stop is recomputed from the stop rule, and every state of a fair component has
   `toward != NONE`.
4. Closure of the backward walk: for every edge `from → to` inside `N`, if
   `toward[to] != NONE` then `toward[from] != NONE`. One pass over the edges.
5. No state has `toward != NONE ∧ triggers ∧ cause`.
`ok` then follows from the Lean criterion (`Search.lean`) on the recorded graph. The checker
shares `holds_bound`, `triggers`, `fair_instance`, `fair.enabled` and the edge list. A
failure is a `tool_error`. The cost is O(V+E) per binding that finds no start. Estimating it
against `avoid`'s own passes (the adjacency build, Tarjan, `taken`, the BFS) is part of
phase 4's measurement. The orbit search (`orbit/liveness.rs::OrbitRun::decide_liveness`) was
not opened for this plan. That it has the same shape, with `is_real` and π per edge, is to be
confirmed in 4b, and π is checked by `Group::apply(π, ·)` against the stored state. N9, N10
and the plain search's code move from read to W; A23's code moves after 4b.

**C20, TV on counterexamples.** Validate each printed lasso and trace with an independent
evaluator, the reference interpreter of C28 linked into the binary for models in its subset.
It costs only on counterexamples. Outside the subset it stays as today (R, shared).

**C21, R, gated on measurement.** On orbit runs, for each stored state `s` that is the
canonical form of a successor `t`, check that `holds_bound(goal, t, b) == holds_bound(goal,
s, π(b))` and the same for the trigger. This is per edge per property, so it could be
expensive. Start with the initial states and the states on each reported lasso, which is
cheap but partial, and measure the full check.

**C22, TV, every `--prove` run.** `Enc::compare` (`encode.rs:687`) takes operands
`Op::Var(sym, domain)`, `Op::Num`, `Op::Lit`, `Op::Row` and builds `eq`, `not`, `and`, `or`,
`smt::sub`, ordering and integer and Boolean literals, with a nullable Int as `v = 0` for null
and `v - 1` for the number. A ground evaluator `smt/eval.rs` for that fragment, after the
symbols are substituted with integers, is about 150 lines and is trusted. For each distinct
`(op, lhs operand kind and domain, rhs ...)` the model uses, enumerate every value pair at the
run's bound, nulls included (≤ 257² points). Encode each value as `Enc` encodes it (one
function, `value_term`, shared with `Enc`, which is the residual trust), and require
`eval(Enc::compare(..)) == expr::compare(..)`. This takes milliseconds. A failure makes the
prover output `tool_error`. B1's "tied by reading" goes to TV for comparisons.

**C23, W/TV, sampled honestly.** With the C22 evaluator extended to the step fragment
(selects over row arrays, quantifier-free after fixing `n`), check that every initial state
of the 1-instance mining twin (`mining_twin`, already built) satisfies `Enc::initial`
instantiated at its slots, and that every edge of that twin's graph satisfies `Enc::step`.
That twin is small by construction, so the cost is bounded. This certifies
over-approximation on that graph only; at larger instance counts it stays P-sketch, and the
output says so. Use z3 itself as the evaluator only in CI, since one query per edge is too
slow for every run. The encoding's universal rows make full instantiation the main effort
here.

**C24, W, narrow.** BFS already evaluates every rule on every stored state, and a rule it
refutes is not tried (`Verdict::Refuted`), so re-evaluating the proved rules adds nothing.
What adds something is the kept *helper lemmas*: mined lemmas (`prove/mine.rs` builds them
as `Pred`) and `.lemma()` rules. Evaluate each kept lemma with native `eval` on every state
BFS stored for the cone. A stored state that breaks one shows the encoding let a false
lemma survive Houdini, so every `proved` of that proof becomes a `tool_error`. To confirm:
mined lemmas are mined on `mining_twin` (one instance per entity), so check that their var
ids are the cone's, and remap them if not. CTI-strengthening clauses (`strengthen.rs`) may
exist only as `Expr`, and need C22's evaluator. It costs states × lemmas evaluations,
streamed, with no RSS, under `--prove` only. It does **not** close B16 or residual risk 2:
a wrong `proved` on a memory-capped cone whose explored states all satisfy the lemmas still
prints `ok`. C23 is the check that bears on a wrong `proved`.

**C25, TV, every `--prove` run.** `cli/src/prove.rs::int_range` already builds the model
at the candidate ends of the range (`guess`, `u8::MAX`, `u8::MAX - 1`), but it keeps only
the diagnostics. Keep the two end systems, and compare each with the run's
`TransitionSystem` with a structural comparator, `same_up_to_bound(a, b)`, written from
Lemma U's statement (z3.md). The two must agree on vars, actions, params, guards, effects and
invariants, except for (1) the sizes of `Int`/`NullableInt` domains and of key domains,
(2) a `Choose` that lists every value of an Int domain, and (3) the `Tick` guard's bound.
Do not compare digests: `int_max` enters at five build sites, so no single literal
can be abstracted. The cost is two builds the run already does, plus a walk. On a mismatch,
the printed range shrinks to the bounds that match, or the run reports a `tool_error`.
Lemma U and B3 go from read to TV. The comparator is new trusted code, but it is written
against the lemma's text.

**C26, CI diversity, optional.** Dump the final round's initiation and consecution
queries as SMT-LIB (`--emit-proof-smt`) and re-check them with cvc5 in CI on the samples
and the corpus. This lowers H1 to "z3 and cvc5 are not both wrong" and does not touch H2.

**C28, TV, three tiers.** (a) CI: widen `tests/reference/interp.rs`'s subset (the
refusals in `RefModel::read`) construct by construct, with each widening a PR. (b)
`verify --certify` (opt-in): move the interpreter from `tests/` to `core/src/semref/`
and compare it with the builder's graph on every cone under 20,000 states that is in its
subset. This doubles BFS cost on those cones, so it is opt-in. (c) Every run, cheap: for
every cone in the subset, compare the reference's successors of the initial states and
their successors (depth 2) with `each_successor`. This is a probe, not a certificate, and
the docs must say so. All three share the parser, `build_model` and the rule names. The
reference's subset bounds what (b) certifies.

**C30, K.** A harness that inserts k ≤ 4 packed rows into `table.rs` with forced tag
collisions and looks each up. The cheap R alternative, re-hashing every stored row at
the end and looking it up, costs one probe per state. Measure it; if it is ≤ 0.3% it can
ship as R.

**C33, R at build time.** For each action, each pair of params of one entity with no
distinctness guard between them that write the same column through two effects: refuse
with a new diagnostic (as `ConflictingEffects` does), or warn and have the TLA+ printer
refuse. Free.

**C34, TV under `--with-tlc`.** On each `StateModel_` module of a plain, unreduced,
non-symmetric cone, add to `state_model_disagreements` the requirement that TLC's
distinct-state count (`StateModelRan::states`, already parsed; `None` when TLC stopped early)
equals BFS's `job.states`. A mismatch is an `EngineDisagreement`. It is free,
and it turns T1's per-construct sketch into a per-run graph-size check on those cones. On a
reduced cone, compare with the class count if the printed module's states are the classes.
Check this in `module.rs::reduction` before enabling it there.

**C35, C36, R at print time.** Check that `TlaNames` is injective and total over the
slots it prints. Re-read the printed class tables from the `Reduction` and compare them.
Both are free and matter under `--with-tlc` only.

## 3. Phases

The order is (risk of a silent wrong verdict on the default path) × (cheapness). Each
phase is one PR unless split as shown. Effort is agent-days including tests, docs rows
and the planted-bug test that every certificate needs (as
`a_planted_split_fails_the_clause_it_breaks` does).

| Phase | PR | Points | Kind | Effort | Est. cost (instructions / RSS) | Classes that change | Depends on |
|---|---|---|---|---|---|---|---|
| 1 | 1a counterexample vouching (**Done**) | C9, C10 | TV, R | 2 d | 0 off counterexamples / 0 | A2 gap closed; A16 "lifted start" closed | none |
| 1 | 1b reduction syntax | C1, C7, C8, C15 | R | 5 d | once per reduction, ~0 / 0 | A8's 8 fields, half of `cover` and `X`'s initial states move from read to R (`FoldKey::Param` folds); A9 fallback and marking, A12, A3 → R; N16's two internal facts → R | none |
| 1 | 1c cone and build syntax | C11, C3 (R half), C33 | TV, R | 3 d | once per cone/build, ~0 / 0 | residual risk 1 "cone choice" → TV; A8/A18 syntax tie → R; T1 aliasing assumption → R | none |
| 2 | 2a prover TVs | C22, C25, C24 | TV, W | 5 d | `--prove` only: ms + two builds already made + states × helper lemmas / 0 | B1 compare → TV; B2 Lemma U, B3 → TV; false helper lemmas → W on explored states (B16 unchanged) | none |
| 3 | 3a step writes in release | C4 | R | 2 d + measure | per successor, target < 1% / 0 (per-action mask) | A1, B6 → R | none |
| 3 | 3b canonical membership | C16 (R + K) | R, K | 3 d + measure | per canonicalization, target < 1% / one scratch row per worker | A21 "member of the orbit" → R, "function of the state" → K; T13 canonical gap → K | none |
| 4 | 4a liveness witness, plain | C19 (plain) | W | 4 d | O(V+E) per binding with no start; target < 1% of the run / transient per-component arrays as `avoid`'s | N9, N10 code → W; T8 / residual risk 2 for liveness `ok` narrowed to the shared predicates | none |
| 4 | 4b liveness witness, orbit | C19 (orbit), C21 (initial + lasso states) | W, R | 3 d (after reading `orbit/liveness.rs`) | as 4a | A23 search code → W; LiveEqui partially R | 3b, 4a |
| 5 | 5a fold exactness | C2 | R / K | 4 d | once per fold (≤ 2^16 points) / table freed after | A8/A11 `exact` → R on small folds, K widened otherwise | 1b |
| 5 | 5b input monotonicity on rises | C6 | R | 2 d | per rise edge, already walked / 0 | A10 `Pre` → R on the run; `acyclic` → R (topological witness) | 1b |
| 6 | 6a Lean grammar checker | C3 (L), C18 | L, M | 5 d | CI only | A8/A18 syntax → L on corpus; A20 `Hyp` → M | 1c |
| 6 | 6b Lean (G1)–(G6) checker | C17 | L, R | 6 d | CI; R pass once per group | A18 model tie → L on corpus, twin-of-spec R | 6a |
| 7 | 7a encoding step on mining twin | C23 | W/TV | 6 d | `--prove`: one graph of the 1-instance twin / bounded | B1 Prop. 2 → TV at 1 instance; P-sketch elsewhere. The only check here that bears on a wrong `proved` itself | 2a |
| 7 | 7b TLC-side cheap checks | C34, C35, C36 | TV, R | 2 d | 0 | T1 → per-run state-count TV on plain cones | none |
| 8 | 8a reference interpreter in core | C28 (b, c), C20 | TV | 8 d | probe ~0; `--certify` ×2 on small cones | I1 → TV on the subset per run (opt-in), probe otherwise; N11 → TV on subset | none |
| 8 | 8b+ widen the subset | C28 (a) | TV | 2–3 d per construct group (about 5 groups) | CI | I1's "outside the subset" shrinks | 8a |
| 9 | optional | C26, C27, C30 | CI diversity, K | 5 d | CI | H1 → two solvers on CI data; N2 → K | 2a |

Total: about 75–80 agent-days (phases 1–4: 27; 5: 6; 6: 11; 7: 8; 8: 18–23; 9: 5).
Phases 1–4 cover the default-path points where a silent wrong verdict rests on a reading,
except C2 and C6 (phase 5), C5, C12, C14, C17 and the builder (C28). What remains for them is
listed in §4.

Why this order. Phase 1 is free at run time. It closes the main silent wrong-counterexample
path (C9) and the largest block of A8 readings. Phase 2 is nearly free and only under
`--prove`, so it ranks below phase 1 by risk but above 3 and 4 by cost. It does not
yet touch a wrong `proved` on a capped cone; 7a does, partly. Phases 3 and 4 put work on hot
paths, so each waits on a measurement. Phase 4 gives the liveness `ok` (T8) its first
run-time check. Phases 6–8 are larger and mostly help CI or opt-in runs.

## 4. What stays trusted at the end

- **Shared primitives** that the checkers read too: the parser, `build_model`, the AST types,
  `eval` / `holds_bound`, `pred_reads` (Kani on depth-1 shapes), `action_reads` /
  `action_writes` (cross-checked by C4 for writes only), `fair_instance`, the packing (Kani),
  and the edge list. Each certificate above names which of them it uses.
- **The builder outside the reference subset** (C28), and the functional-dependence half of
  `Local` (C5). The only independent check of these is the reference interpreter. The
  builder itself cannot get a cheaper certificate: the specification it meets is the
  semantics, so a certificate is a second implementation of D1–D13.
- **Paper sketches**: nested reductions and runs up to `H` (C14); the property-cone ladder
  order (C12); the reduced TLA+ step (C34 on reduced cones); `exact` on folds past the
  enumeration limit (C2); (G6) outside guards and least BFS depth (C17).
- **Checker code**: about 1,500 new lines in total, each checker small, written from the
  Lean or semantics text, and each with a planted-bug test.
- **Tools**: Lean 4.36.0 kernel; z3 (H1, H2, H4; cvc5 if C26 lands); TLC 1.8.0 and the
  JVM; Kani; rustc. Release builds still wrap integers silently (no change proposed).
- **CI-only coverage**: every L certificate (C3-L, C17-L, and the existing Lemma 5 and
  partition checks) covers the corpus and samples, not users' runs, unless the checker is
  later ported to Rust as an R pass. 6b proposes this for (G1)–(G6).
- **The dump code** feeding the L checkers (proofs/lean/README.md:527, :620: the dump calls
  `read_write_sets` as `split_faults` does, and numbers projections as it meets them). Keep
  each dump a serialization of the very struct the R check reads, so the L checker and the
  R check see one object.
- **`proved` past the explored states**: no run-time check can see a wrong `proved` on
  states BFS did not reach. 7a narrows this at one instance, and C26 narrows H1. B16 stays
  open.

## 5. Measurement

The exit criterion for any phase that touches a per-state or per-edge path (3a, 3b, 4a,
4b, 5b, and C21/C30 if enabled) is as follows:

- Build `--features perf`; run `scripts/perf-tables.py target/release/rdra-ish` on the
  samples at their `// expect` header flags at `RDRA_JOBS=1`, before and after. Report
  instructions retired (not wall time). Target: ≤ 1% total. The existing certificates
  are at 0.1–0.9%.
- Peak RSS from `/usr/bin/time -l` on the three largest samples at their header flags:
  no increase beyond run-to-run noise. Any new per-state array is a design failure; every
  design above reuses arrays the run already holds.
- A certificate over budget is not shipped in a weaker form (sampled) under the same
  name. Either the check gets cheaper, or the point stays "read" and the docs say so.

The phases that run once per build, cone, reduction or fold, or only on counterexamples
or under `--prove`, need only a planted-bug test and a note that they add no per-state
work. None needs a heavy local run: CI's `shadow` job and the samples cover them.

## 6. Docs changes per phase

Each PR updates its claims rows (Evidence gains the checker, and the Open gap loses its
"read from the code" clause), the README known-limitations bullet that `export_gate` ties to
each row, the cones.md `Lemma6Hyp` table, and README "What every run checks". Restate
residual risk 2 after 4a, for the liveness `ok` only. Its `proved` half stays.
