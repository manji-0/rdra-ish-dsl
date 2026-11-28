# Defects found in rdra-ish, by how they were found

This record lists the defects of the verifier that the project history reports, and
classifies each by the activity that found it. It is roadmap item T3 of issue #594. It
describes the repository at commit `9e4a123` (2026-11-28).

The table is generated from [`defects.tsv`](./defects.tsv);
`python3 -I scripts/paper/defect-summary.py` prints the summary tables of this document from
it, and `--table` prints the full table.

## Sources

- `CHANGELOG.md`: every "Fixed" section from v0.2.0 to Unreleased, and the "Erratum" of
  v3.0.0.
- `git log` of the whole history (2,124 commits), subjects and bodies, including merge
  messages.
- The table of open problems P1 to P67 in `docs/theory/README.md` as it stood before commit
  `b1b4222d`, which replaced it with the current index (`git show b1b4222d^:docs/theory/README.md`).
  Its "Numbering" paragraph says where each block of items came from.
- `docs/theory/*.md`, `docs/design/*.md` (in particular `event-order.md`, "Landing plan"),
  `docs/engine-agreement.md` and `proofs/lean/README.md`.
- The GitHub issues and pull requests that these sources cite, read through the REST API,
  for the sentence that says how a defect was found.

## Method

**What counts.** A row is a *defect* (`D`) when a source says the tool did one of these:
printed a wrong `ok` or `proved`, missed a finding, printed a counterexample or finding the
model does not have, printed a wrong trace or instance count, crashed, ended a run as
`tool_error` without a fault of the model, skipped a cross-check silently, or gave output
that depended on the thread count. A row is a *doc/claim error* (`C`) when a source says a
document or definition stated a guarantee the code did not give, with no defect in the
code. Each row cites the sentence it rests on.

The scope is the verification pipeline: the builder of the transition system, cones,
the sub-lifecycle reduction, instance symmetry, BFS and the liveness search, the z3 prover,
the TLA+ printer, and the `verify` / `check` driver that runs them and compares them with
TLC. These are not counted:

- defects of `codegen`, `check --code`, the language server, diagrams, `list`, `states`
  display, `fmt`, and the SQL and DBML exports;
- changes of semantics the owner decided as design, even when a source calls the old
  reading surprising (for example #78, the guards of a split use case; #112 and #30, the
  event order; #199 and #212, deletes);
- missing run-time checks and evidence gaps with no defect behind them (P3, P7, P14, P19,
  P26, P35, P42), and defects of the test harnesses (P13, P15, P17, P25, P44);
- performance problems and incompleteness of the prover (for example #101, #231);
- vacuity and `info` findings, which no verdict reads (for example 086b4352, 0552fabb,
  2cb3e297);
- items closed as "not a bug" (P34) or as a duplicate (P51 is counted under P22);
- one-line or vague July 2026 fixes whose effect the commit does not state
  (ba5c088c, 4f259f1d, 6839cab8, 72a6f782, f1d25984, d826eed4, 39203571, 2186c485, 870fb46d).

**Discovery class.** The classes are those of the roadmap:

| Class | Meaning |
|---|---|
| 1 | Lean proof attempt or mechanization (`proofs/lean/`) |
| 2 | Paper proof: found while writing the theory documents (#464 to #468: the semantics and claims inventory, and the theme documents on cones, z3, the native verifier and TLC agreement, which state each guarantee as a proposition with its proof or evidence) — subclass `write-up` — or by a review or audit of those documents against the code — subclass `review/audit` |
| 3 | Reference interpreter (`core/tests/reference/`) |
| 4 | TLC differential: `engine_equivalence`, `verify --with-tlc`, and the engine corpus with its hand-written headers |
| 5 | Mutation run |
| 6 | Run-time self-certification (counterexample replay, partition certificate) |
| 7 | Shadow twin |
| 8 | Kani, Miri, or overflow-checked CI |
| 9 | Random-model `reduction-diff` |
| 10 | Unit or regression test, user report, code review (people or models), use of the tool on a sample model (`dogfooding`), found while fixing another issue, a lint |
| unstated | No source says how the defect was found |

A class is assigned only from a sentence in a source. The P-table's "Numbering" paragraph
places P1 to P21 in the survey for the claims inventory (#464), P22 to P24 in the
TLC-agreement document (#465), P30 and later in the cones document (#467), P40 and later in
the z3 document (#468), and P50 and later in the native-verifier document (#466); the pull
requests that fixed P32, P43 and P54 name the review of #467, #468 and #466 that found them,
and P66, P67 and #541 name the fourth, fifth and third theory audits. P60 comes from the
owner's review. Every other row cites its own source.

**Columns.** `ref` is the commit, issue (`#n` after a hash) or P-number, with the pull
request that fixed it. `version` is the first release whose CHANGELOG section holds the fix;
"(branch)" marks a defect found before its code merged. `date` is the date of the fixing
commit, or the release date when the source is a CHANGELOG entry alone. `shown` says whether
a run of the tool showed the wrong output: `yes`, `no` (a reading of the code only),
`test-only` (only on a forged input), or `unstated`.

**Effect.** `wrong ok` (includes a wrong `proved` and a missed finding), `wrong
counterexample` (includes a spurious finding and an understated instance count), `wrong
verdict` (the source states a wrong reading of the model with no direction, or both
directions), `crash` (a panic), `tool_error`, `missed check` (a comparison skipped with no
report), `other` (a wrong count, a merged report, output that depends on the thread count,
a lost verdict), `unstated`, and `claim` for the C rows.

## Findings

- 74 defects and 15 doc/claim errors are recorded. 30 defects are a wrong verdict that a run
  showed on some model (`wrong ok`, `wrong counterexample` or `wrong verdict` with `shown`
  yes).
- No defect is credited to a Lean proof (class 1), the reference interpreter (3), a
  mutation run (5), a run-time self-check (6) or a shadow twin (7). The history records
  these only as checks that a known or planted defect fails: the reference interpreter
  "Fails on P40/P54 and P43 before their fixes" (fc505d39), and each shadow twin, Kani
  harness and corpus rule is shown against a planted bug (`docs/engine-agreement.md`, "What
  the corpus catches"). Most of them are recent: shadow twins, Kani harnesses and the
  mutation runs date from 2026-11-24, the reference interpreter and the Lean proofs from
  2026-11-28. Kani is credited with one defect (f3df52b3, D45), which no reachable state
  exercises.
- The stub-parameter bug P67 (D69) was found by the fifth theory audit (PR #547: "Fixes
  audit 5's C1"), fixed at 10:25 on 2026-11-28. Its Lean counterpart,
  `ExampleLemma6.stub_bug` in `proofs/lean/RdraIsh/Lemma6Example.lean`, was committed at
  15:12 the same day (377bd06d): the Lean proof restates the defect after the fact and did
  not find it. The same holds for the reduction-diff `--stubs` generator (d2f01460) and the
  TLC refinement check (43cefaee), which were shown to catch it on the pre-fix build.
- Class 2 finds the defects that agreement between engines cannot see. Of its 13
  wrong-verdict defects, P40, P54 (D51, D52), P43, P66 (D59, D68), P4 (D60) and P67 (D69)
  printed a wrong `ok` or `proved` on which BFS and TLC agreed, or which no comparison with
  BFS reached. The common-mode defects in class 4 were found by the engine corpus's
  hand-written headers, not by agreement: #180 (D42), where "every engine reads the model
  the same wrong way".
- The differential tests in classes 4 and 9 find disagreements in the TLA+ printer and the
  driver: 3 of the 7 class 4 defects and 1 of the 2 class 9 defects are in TLA+ emit, and
  the class 4 defects of the event-order landing (D71 to D74) were found before merge.
- Of the 52 defects fixed in a release up to v3.0.0, 23 have no stated discovery, 19 were
  found by review, by use of the tool on a sample model, or while fixing another issue
  (class 10), 7 by TLC differential tests or the engine corpus (class 4), 2 by
  `reduction-diff` (class 9) and 1 by Kani (class 8). All 19 class 2 defects are fixed after
  v3.0.0 (Unreleased), as the theory documents date from 2026-11-28.

## Summary

Printed by `python3 -I scripts/paper/defect-summary.py`.

Rows: 89: 74 defects (D) and 15 doc/claim errors (C).
Defects with a wrong verdict shown on a model: 30.

**Defects by discovery class and effect**

| Discovery class | wrong ok | wrong counterexample | wrong verdict | crash | tool_error | missed check | other | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 1 Lean proof |  |  |  |  |  |  |  |  | 0 |
| 2 Paper proof (theory write-up or its audit) | 9 | 3 | 1 | 1 | 2 | 2 | 1 |  | 19 |
| 3 Reference interpreter |  |  |  |  |  |  |  |  | 0 |
| 4 TLC differential / engine corpus | 1 | 2 |  | 1 |  |  |  | 3 | 7 |
| 5 Mutation run |  |  |  |  |  |  |  |  | 0 |
| 6 Run-time self-certification |  |  |  |  |  |  |  |  | 0 |
| 7 Shadow twin |  |  |  |  |  |  |  |  | 0 |
| 8 Kani / Miri / overflow checks |  |  |  |  |  |  | 1 |  | 1 |
| 9 Random-model reduction-diff |  |  |  |  | 1 |  | 1 |  | 2 |
| 10 Test, user report, review, other | 7 | 3 | 6 | 1 | 3 |  | 1 |  | 21 |
| Unstated | 9 | 4 | 5 | 1 | 3 |  | 2 |  | 24 |
| **Total** | 26 | 12 | 12 | 4 | 9 | 2 | 6 | 3 | 74 |

**Wrong-verdict defects (wrong ok, wrong counterexample, wrong verdict) by discovery class and whether a run showed them**

| Discovery class | shown: yes | shown: no | shown: unstated | Total |
|---|--:|--:|--:|--:|
| 2 Paper proof (theory write-up or its audit) | 9 | 4 |  | 13 |
| 4 TLC differential / engine corpus | 3 |  |  | 3 |
| 10 Test, user report, review, other | 9 | 1 | 6 | 16 |
| Unstated | 9 |  | 9 | 18 |
| **Total** | 30 | 5 | 15 | 50 |

**Defects by component and effect**

| Component | wrong ok | wrong counterexample | wrong verdict | crash | tool_error | missed check | other | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| builder | 7 | 5 | 9 |  |  |  | 2 |  | 23 |
| cone | 4 | 2 |  |  |  |  |  |  | 6 |
| reduction | 2 | 1 | 1 | 4 |  |  |  | 1 | 9 |
| symmetry | 1 |  |  |  |  |  |  |  | 1 |
| BFS/liveness | 1 |  | 1 |  |  |  | 1 |  | 3 |
| prover/z3 | 4 |  |  |  | 1 |  |  |  | 5 |
| TLA+ emit | 3 | 2 | 1 |  | 6 |  |  | 2 | 14 |
| other | 4 | 2 |  |  | 2 | 2 | 3 |  | 13 |
| **Total** | 26 | 12 | 12 | 4 | 9 | 2 | 6 | 3 | 74 |

**Defects by component and discovery class**

| Component | 2 | 4 | 8 | 9 | 10 | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|
| builder | 2 | 1 |  |  | 13 | 7 | 23 |
| cone | 2 |  |  |  | 1 | 3 | 6 |
| reduction | 3 | 2 |  |  | 2 | 2 | 9 |
| symmetry | 1 |  |  |  |  |  | 1 |
| BFS/liveness | 2 |  | 1 |  |  |  | 3 |
| prover/z3 | 3 |  |  |  | 1 | 1 | 5 |
| TLA+ emit | 1 | 3 |  | 1 | 2 | 7 | 14 |
| other | 5 | 1 |  | 1 | 2 | 4 | 13 |
| **Total** | 19 | 7 | 1 | 2 | 21 | 24 | 74 |

**Subclasses (all rows)**

| Discovery class | Subclass | Defects | Claims |
|---|---|--:|--:|
| 2 Paper proof (theory write-up or its audit) | review/audit | 7 | 1 |
| 2 Paper proof (theory write-up or its audit) | write-up | 12 | 12 |
| 4 TLC differential / engine corpus | --with-tlc cross-check | 1 |  |
| 4 TLC differential / engine corpus | engine corpus header | 1 |  |
| 4 TLC differential / engine corpus | engine_equivalence | 5 |  |
| 8 Kani / Miri / overflow checks | Kani | 1 |  |
| 9 Random-model reduction-diff | reduction-diff | 2 |  |
| 10 Test, user report, review, other | doc sync | 1 |  |
| 10 Test, user report, review, other | dogfooding | 7 |  |
| 10 Test, user report, review, other | lint | 1 |  |
| 10 Test, user report, review, other | review | 9 | 1 |
| 10 Test, user report, review, other | while fixing | 3 |  |

**Doc/claim errors by discovery class**

| Discovery class | Claims |
|---|--:|
| 2 Paper proof (theory write-up or its audit) | 13 |
| 10 Test, user report, review, other | 1 |
| Unstated | 1 |
| **Total** | 15 |

## Full table

Printed by `python3 -I scripts/paper/defect-summary.py --table`. Quotations from Japanese issues are translated and marked so.

| ID | Ref | Version | Date | Component | Effect | Shown | Class | Description | Evidence |
|---|---|---|---|---|---|---|---|---|---|
| D01 | 9ae35cac | v0.2.0 | 2026-07-19 | TLA+ emit | wrong ok | unstated | unstated | The TLA+ export baked the `now` safety condition into the Assign/TickNow actions, so the check of it was vacuous. | commit 9ae35cac "harden TLA export against vacuous now"; CHANGELOG v0.2.0: "`now` Safety is not baked into `Assign`/`TickNow`" |
| D02 | b5530c12 | v0.2.0 | 2026-07-20 | TLA+ emit | wrong counterexample | yes | unstated | TLC reported terminal lifecycle states as deadlocks. | commit b5530c12: "Export CHECK_DEADLOCK FALSE so cancelled/delivered terminals do not fail verify on order samples." |
| D03 | e70eb323 | v0.2.0 | 2026-07-20 | TLA+ emit | tool_error | unstated | unstated | A multi-instance module used `1..InstanceCount` without extending Integers. | commit e70eb323: "Multi-entity exports use 1..InstanceCount and must import Integers even without Int axes." |
| D04 | 48731055 | v0.2.0 | 2026-07-20 | TLA+ emit | wrong verdict | unstated | unstated | `after.assert` was exported as primed assignments inside the actions instead of as a property. | commit 48731055: "Stop injecting primed assignments into SpecActions; export independent [][action => post] properties" |
| D05 | ad626a68 (#12) | v0.3.0 | 2026-10-27 | TLA+ emit | wrong ok | yes | unstated | The TLA+ export dropped the `sets` effects of an entity with transitions, so `verify` printed TLC: OK for a model with a reachable forbidden state. | issue #12: "`rdra-ish verify` prints `TLC: OK` and exits 0 for a model in which a `forbidden` state is reachable" |
| D06 | 02773ff8 (#12) | v0.3.0 | 2026-10-27 | TLA+ emit | wrong ok | unstated | unstated | TLA+ Init took only the first creation state, so a violation reachable only from a later one was missed. | commit 02773ff8: "a violation reachable only from a later one is no longer missed" |
| D07 | 0213c1f8 | v0.3.0 | 2026-10-28 | cone | wrong counterexample | yes | unstated | Creating an entity outside the target's cone used up its only slot, so on personal-info every card and address use case after the first never fired. | commit 0213c1f8: "on personal-info every card and address use case after the first never fired in `states`" |
| D08 | 66ae90cb | v0.3.0 | 2026-10-28 | cone | wrong ok | unstated | unstated | A predicate that read an owner variable put only the child in its component, so the restriction dropped the owner variable and every invariant that read it. | commit 66ae90cb: "restrict dropped the owner var (it needs both ends) and with it any invariant that read the owner alone" |
| D09 | CHANGELOG v0.3.0 | v0.3.0 | 2026-10-30 | other | wrong ok | unstated | unstated | `check` dropped the verdicts of the constraints that an entity without state columns binds. | CHANGELOG v0.3.0 Fixed: "`check` dropped the verdicts of the constraints that an entity without state columns binds" |
| D10 | CHANGELOG v0.3.0 | v0.3.0 | 2026-10-30 | TLA+ emit | tool_error | yes | unstated | `export --kind tla` failed on two samples: an undefined name in TypeOK, and an init fallback. | CHANGELOG v0.3.0 Fixed: "`export --kind tla` works on `samples/api-contract` ... and on `samples/clinic-ops` (it ended in `TLA_FATAL:init_pending_fallback`)" |
| D11 | 8435d700 | v0.3.0 | 2026-10-28 | builder | wrong verdict | unstated | unstated | A use case that creates a parent took an already-linked child from its parent through an `.optional()` key. | CHANGELOG v0.3.0 Fixed: "no longer takes an already-linked child from its parent when the key is `.optional()`" |
| D12 | 718c98ac | v0.4.0 | 2026-10-31 | cone | wrong ok | yes | unstated | An `after` assertion's cone dropped what the use case writes, so the step was a stutter and the assertion passed unread; the TLA+ cone also named a removed parameter (TLC semantic error). | commit 718c98ac: "`after(IssueCert).assert(CertificateOrder.status == executed)` passed even though IssueCert can run while the order is still requested" |
| D13 | CHANGELOG v0.4.0 (#46) | v0.4.0 | 2026-11-01 | builder | wrong verdict | unstated | unstated | A property atom `Entity.qty == limit` over two Int columns compared `qty` with no value and was false in every state. | CHANGELOG v0.4.0 Fixed: "the atom compared `qty` with no value at all and was false in every state (#46)" |
| D14 | CHANGELOG v0.4.0 | v0.4.0 | 2026-11-01 | builder | wrong ok | unstated | unstated | A one-entity `invariant(...).when(...).none(Related ...)` was dropped and never checked. | CHANGELOG v0.4.0 Fixed: "`invariant(Order).when(status == paid).none(Ticket.status == pending)` is checked ... it was dropped" |
| D15 | bbaee8c5 (#52) | v1.0.0 | 2026-11-01 | builder | wrong ok | yes | 10 (dogfooding) | Several `transitions` of one use case from one state kept only the first arm; `check` passed and called the other targets unreachable. | issue #52 (translated): "only the first transition is explored ... `check` is OK"; "occurred in patent-ops' `DecideFiling`" |
| D16 | 0940db4c (#63) | v1.0.0 | 2026-11-01 | builder | wrong verdict | yes | unstated | A `.has`/`.none` quantifier read the row the use case creates (or updates) instead of the quantified rows; TLC stopped on an undefined primed variable. | commit 0940db4c: "BFS let the use case run whatever the existing rows were ... TLC stopped with `Parcel_state is undefined`" |
| D17 | 6662a888 (#64) | v1.0.0 | 2026-11-01 | other | wrong ok | yes | 4 (engine_equivalence) | TLC with -continue reports one violated check per state, so `verify` never printed a rule that fails only where another rule of the cone fails too. | issue #64 (translated): "BFS and TLC reports disagree ... in the engine_equivalence cone comparison"; commit 6662a888: "TLC was the one missing a rule" |
| D18 | 69a3fb18 (#67) | v1.0.0 | 2026-11-02 | TLA+ emit | tool_error | yes | 10 (review) | A guard on a creating use case read a primed variable before the UNCHANGED that binds it, so every run holding the column ended tool_error. | CHANGELOG v1.0.0: "`verify` decides a model whose condition on a creating use case reads a column ... (#67 review)" |
| D19 | 918466f3 | v1.0.0 | 2026-11-02 | other | wrong counterexample | unstated | unstated | A TLC report that the emitter's TypeOK was violated was shown as a counterexample of the model. | commit 918466f3: "TLC reporting it violated is a bug in the generated TLA+, not a counterexample" |
| D20 | a3b2f553 | v1.0.0 | 2026-11-02 | other | other | unstated | unstated | A whole-model property run stopped after TLC had printed its verdict was reported `not decided`. | commit a3b2f553: "the property was still listed as not decided" |
| D21 | de6b7cc8 (#75) | v1.0.0 | 2026-11-02 | builder | wrong ok | unstated | 10 (dogfooding) | Two mandatory keys from one child to one parent shared one parent parameter, so violations that need the keys on two rows were missed. | issue #75 (translated): "`check` and `verify` miss rule violations there (a soundness hole)"; listed as left over from #73 (food-permit-ops) |
| D22 | CHANGELOG v1.0.0 (#75) | v1.0.0 | 2026-11-02 | builder | wrong verdict | unstated | 10 (dogfooding) | A `holds` comparison or copy source on a parent the use case binds ignored `.role` and read the bound parameter. | CHANGELOG v1.0.0: "Before, it read the bound parameter whatever `.role` named (#75)" |
| D23 | a7074152 (#73) | v1.0.0 | 2026-11-02 | builder | wrong verdict | yes | 10 (dogfooding) | An action relinked a parent parameter it only reads: `RegisterPermit` linked the application it read as renewing the permit it created. | CHANGELOG v1.0.0: "An action never relinks a parent parameter, a row it only reads (#73)"; commit a7074152: "Two binding fixes the second key exposed" (the `renews` key of food-permit-ops) |
| D24 | a7074152 (#73) | v1.0.0 | 2026-11-02 | builder | wrong counterexample | unstated | 10 (dogfooding) | A `holds` guard comparing a column of the parent of an `.optional()` key created the child unlinked, so the guard never held. | commit a7074152: "instead of creating it unlinked so the guard never holds"; "Two binding fixes the second key exposed" |
| D25 | 83151c25 (#76) | v1.0.0 | 2026-11-03 | builder | wrong ok | unstated | 10 (dogfooding) | A use case that writes a parent put it on every key of a child, so violations that need the keys apart were missed. | CHANGELOG v1.0.0: "`verify` missed every violation that needs the two apart"; issue #76 is "the rest of #75" |
| D26 | 07be83c2 (#81) | v1.0.0 | 2026-11-03 | builder | wrong counterexample | yes | 10 (review) | `holds(...).none(...).along(...)` bound every path entity as a witness, so `verify` reported a false deadlock. | issue #81 (translated): "Found in the pre-1.0 self-review (Sonnet)"; "`Deadlock_Member_status_active` ... should be able to withdraw" |
| D27 | 49682092 (#83) | v1.0.0 | 2026-11-03 | builder | wrong counterexample | yes | 10 (review) | An `after` assertion on a use case split into transaction groups was read after every step, giving a false violation. | issue #83 (translated): "Found in the pre-1.0 self-review (Fable)" |
| D28 | 80300305 (#84) | v1.0.0 | 2026-11-03 | builder | wrong ok | unstated | 10 (review) | A `@check` read a comparison over null as passing while every other rule form fails it. | issue #84 (translated): "Found in the pre-1.0 self-review (Fable)"; commit 80300305: "The same `qty >= 1` held as a `@check` and failed as an `invariant(..).then`" |
| D29 | 07f56d0e (#90) | v1.0.0 | 2026-11-03 | builder | wrong verdict | unstated | 10 (while fixing) | An invariant's `.has`/`.none` ignored `.along` and counted every related row in the system. | issue #90 (translated): "Found while working on #81"; CHANGELOG v1.0.0: "counted every loan in the system" |
| D30 | c3ea59df | v1.2.0 | 2026-11-04 | TLA+ emit | tool_error | yes | 9 (reduction-diff) | A reduced cone's `Succ_` table as a chain of `@@` overflowed the JVM stack, so `verify` reported tool_error on a cone 1.1 decided ok. | commit c3ea59df: "(review B of #74, a random model minimized; seed 2849 at 2829 classes and seed 2563 ...)" |
| D31 | 9b60c203 | v1.2.0 | 2026-11-04 | reduction | wrong ok | yes | unstated | The reduction related actions by name, so a sub-lifecycle action named like a target step lost its class moves (`ok` where the cone finds the violation) or panicked. | commit 9b60c203: "verify said ok where the cone finds the violation), or its params were read with the step's param list (an index panic in observe)" |
| D32 | 6bbace14 | v1.2.0 | 2026-11-04 | builder | other | yes | unstated | A generated action name equal to a use case id merged two actions in the findings. | commit 6bbace14: "printed `whether Go_1 can run is not decided` while both ran" |
| D33 | 87677745 (#103) | v1.2.0 | 2026-11-05 | builder | other | yes | 10 (review) | Two generated rule names collided, and `verify` listed one name as both ok and a counterexample. | CHANGELOG v1.2.0: "`verify` listed one name as `ok` and as a counterexample"; issue #103 is a search for such collisions |
| D34 | 2125833c (#103) | v1.2.0 | 2026-11-05 | TLA+ emit | tool_error | yes | 10 (review) | A use case named `Succ_...` or `Rep_...` defined a reduced cone's table name twice and SANY failed the cone. | CHANGELOG v1.2.0: "defined the name twice, and SANY failed the cone with `tool_error`" |
| D35 | 373db625, 8effed83 (#103) | v1.2.0 | 2026-11-05 | other | tool_error | yes | 10 (review) | The spec files `verify` writes overwrote each other (`P_WF` beside `P`; names equal ignoring case), giving tool_error or `not decided` on a violated property. | CHANGELOG v1.2.0: "(`tool_error`, or `not decided` on a violated `P`)" |
| D36 | 207ff92a (#110) | v1.2.0 | 2026-11-05 | builder | wrong counterexample | yes | unstated | The columns a `sets(..).when(Cond)` guard reads were not state variables, so the guard never held and `check` reported never-runs, a missing witness and a deadlock. | commit 207ff92a: "check reported the use case as never running, its target state as unwitnessed, and the source state as a deadlock (#110)" |
| D37 | 5d9be069 | v2.0.0 | 2026-11-06 | other | wrong ok | yes | unstated | A handler variant that only capped cones kept got no finding, so `--deny-undecided` passed on a question no cone answered. | commit 5d9be069: "`--deny-undecided` passed on a question no cone answered" |
| D38 | 22ea13c2 (#121) | v2.2.0 | 2026-11-16 | reduction | crash | yes | 10 (dogfooding) | The BFS of a reduced cone panicked (index out of bounds) on a class move whose T params the extended sub-lifecycle dropped. | PR #121: "It also fixes an engine bug that the new model exposed"; CHANGELOG v2.2.0 Fixed |
| D39 | 17992524 | v3.0.0 | 2026-11-21 | prover/z3 | tool_error | yes | unstated | A key compared with a nullable Int gave an ill-sorted SMT term, and an empty choice printed `(or )`; z3 rejected both. | commit 17992524: "aborted the proof with a z3 error ... printed `(or )`, which z3 rejects" |
| D40 | c56f35ff (#164) | v3.0.0 | 2026-11-22 | cone | wrong ok | yes | 10 (review) | Covering turned a violation into `ok (checked in ...)`: a covering cone could take a step as a stutter or drop a mark's guard. | commit c56f35ff: "A review found two models where `covering` turned a violation into `ok (checked in ...)`" |
| D41 | ad6a90f0 (#177) | v3.0.0 | 2026-11-23 | other | other | yes | 9 (reduction-diff) | A rule broken in several cones was counted once per cone, so the failure line depended on how cones overlap. | commit ad6a90f0: "the hooked reduction-diff run diverged on seeds 7, 8, 46, and 459 on that count alone" |
| D42 | 7aa6977d (#180, #185) | v3.0.0 | 2026-11-23 | builder | wrong counterexample | yes | 4 (engine corpus header) | An Int `@default` outside the range silently started at 0 in every engine, so every engine reported a violation the model does not have. | issue #180: "Every engine reads the model the same wrong way, so `verify --with-tlc` reports no disagreement"; model from the adversarial corpus (commit 8090d239 records it as `// known`) |
| D43 | 32bd88ec (#246, #258) | v3.0.0 | 2026-11-24 | other | wrong ok | yes | 10 (doc sync) | A `can_reach` whose cone stopped at a cap was counted nowhere: the run printed OK and passed `--deny-undecided`. | issue #246: "Found while syncing the docs with main (#245)" |
| D44 | 1d0bf9d5 (#261, #267) | v3.0.0 | 2026-11-24 | TLA+ emit | wrong counterexample | yes | 4 (--with-tlc cross-check) | The state-model `Deadlock_` invariant left out event marks a writing step reads, so TLC (and a hand run of the export) reported a deadlock that does not exist. | issue #261: "A bug the `--with-tlc` state-model cross-check of #201 found" |
| D45 | f3df52b3 (#279) | v3.0.0 | 2026-11-24 | BFS/liveness | other | no | 8 (Kani) | `store_bytes` could overflow its u128 sum near `width == usize::MAX`: a debug build panicked and a release build wrapped to a small count. | commit f3df52b3: "Kani found it (#275)"; "No state is that wide" |
| D46 | 7b9c8cad (#355, #360) | v3.0.0 | 2026-11-25 | builder | wrong verdict | yes | unstated | `==` between two nullable columns read null == null as true in every engine and z3, against the language reference and the SQL export. | issue #355: "the BFS engine, TLC and z3 all read `null == null` as true ... So `verify --prove` can prove a rule that the generated SQL CHECK rejects" |
| D47 | 3aa1ef06 (#359, #367) | v3.0.0 | 2026-11-25 | builder | wrong verdict | yes | 10 (while fixing) | An Enum or Bool column marked `@null` had no null in the lifecycle and started at its first value. | issue #359: "Found while fixing #355" |
| D48 | 70ec425a (#374, #381) | v3.0.0 | 2026-11-26 | builder | wrong verdict | yes | 10 (while fixing) | `@derived(qty in (...))` over a nullable Int was false, not null, while `qty` was null. | issue #374: "Found while fixing #359 (PR #367)" |
| D49 | d7c798ea (#425, #429) | Unreleased | 2026-11-27 | prover/z3 | wrong ok | yes | 10 (review) | The build read a set listing every Int value up to the range's top as the whole domain, so `--prove` proved a rule BFS refutes at a wider range. | issue #425: "Found by the z3 encoding audit (2026-11-27)"; CHANGELOG Unreleased Fixed |
| D50 | P21 (#469) | Unreleased | 2026-11-28 | other | tool_error | yes | 2 (write-up) | A native property run stopped at a cap was recorded as ok in the per-run TLC comparison, giving a false EngineDisagreement. | P-table P21 (README at b1b4222d^): "reproduced before the fix with `temporal_unmet.rdra` at `--max-states 3`"; P1-P21 "come from the survey for the claims inventory" |
| D51 | P40 (#470) | Unreleased | 2026-11-28 | builder | wrong ok | yes | 2 (write-up) | A nullable Int at `--int-range-max 255` wrapped its top value to null; a rule the model breaks printed `ok` and `proved`. | P-table P40: "`sets(Top, Item.n == 255)` with `forbidden(Item, n == 255)` printed `proved:` and `ok`"; PR #470 cites docs/theory/z3.md "Reproductions" (#468) |
| D52 | P54 (#470) | Unreleased | 2026-11-28 | builder | wrong ok | yes | 2 (review/audit) | An Enum of 256 (nullable) or 257 values wrapped its last value onto null or the first value; a broken rule printed `ok` at the default bounds. | P-table P54: "Reproduced at one instance by the review of native.md"; PR #470: "P54 (from the #466 review), at the default bounds" |
| D53 | P12 (#471) | Unreleased | 2026-11-28 | other | missed check | yes | 2 (write-up) | A TLC trace that did not replay through BFS's step relation silently skipped the comparison. | P-table P12: "a non-replaying trace dropped the comparison" (reproduced "with a `tlc` wrapper that corrupts one value") |
| D54 | P1 (#477, #509) | Unreleased | 2026-11-28 | cone | wrong counterexample | yes | 2 (write-up) | A cone that left a raised mark out was treated as exact: its counterexample was reported, a rule marked refuted, and escalation printed a counterexample at k instances. | P-table P1: "Consequence: a spurious `counterexample` and a rule never proved; not a wrong `ok`"; PR #477: "each reproduced on main first" |
| D55 | P11 (#477, #509, #532) | Unreleased | 2026-11-28 | other | wrong counterexample | no | 2 (write-up) | Escalation and refutation printed fewer instances than a counterexample needs (freed entities excluded; a maximum under per-entity counts). | P-table P11: "Escalation could understate the instances a counterexample needs" |
| D56 | P32 (#477) | Unreleased | 2026-11-28 | cone | wrong ok | yes | 2 (review/audit) | An entity cone that left a raised mark out could hide a deadlock, a missing witness or a never-runs finding. | P-table P32: "a wrong \"no finding\", that is, a wrong `ok`"; PR #477: "P32 (#467 review, item 2) ... On the reviewer's model, main reported no `Witness_Order_status_shipped`, no `Deadlock_Order_status_open`" |
| D57 | P50 (#479, #510) | Unreleased | 2026-11-28 | BFS/liveness | wrong ok | yes | 2 (write-up) | Packed-state bit offsets were u16 and saturated at bit 65,535, so wide states were stored wrongly and distinct successors merged. | P-table P50: "Reproduced on a 34-Int-column entity at 255 instances (69,870 bits): a spurious `counterexample` and a wrong `ok`"; PR #466 (native.md) lists P50 |
| D58 | P55 (#479) | Unreleased | 2026-11-28 | BFS/liveness | wrong verdict | no | 2 (review/audit) | The first-edge index was a bare `as u32` cast and could wrap past 2^32 edges, attributing edges to wrong sources. | P-table P55: "Not reproduced"; commit 02e5b48f "native.md review fixes; P54-P56" |
| D59 | P43 (#481) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (review/audit) | A `.when`-guarded `present` write to a nullable Int kept the run's top value in the encoding, so `forbidden(Item, n > 5)` was proved at range 5 and refuted at 7. | PR #481: "P43 is F1 of the #468 review: a wrong `proved` at ordinary bounds" |
| D60 | P4 (#483, #514, #523) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (write-up) | A proof under a scope flag proved the scoped system without saying so, and under `--entity` a deferred build error made a rule the model breaks print `proved:`. | P-table P4: "Reproduced: with `--without-usecase Close`, a rule the whole model violates printed `proved:`"; PR #523: "It was reproduced first, and it is a real bug" |
| D61 | P53 (#484) | Unreleased | 2026-11-28 | other | other | yes | 2 (write-up) | A safety cone rerun after a memory stop was not marked, so the printed state count depended on the thread count. | P-table P53: "`RDRA_JOBS=8` printed `(15150 states)` where 1 printed `(65793 states)`" |
| D62 | P24 (#485) | Unreleased | 2026-11-28 | TLA+ emit | tool_error | yes | 2 (write-up) | The TLA+ printer and the native step read a multi-target transition beside another write of the same slot differently. | P-table P24: "built that shape (reproduced: `EngineDisagreement` on main)" |
| D63 | P52 (#487) | Unreleased | 2026-11-28 | reduction | wrong counterexample | no | 2 (write-up) | A reduced safety trace whose lifted last state no longer violated the check was printed as a counterexample. | P-table P52: "kept the reduced message and printed the lifted trace with no fault" |
| D64 | P30 (#488) | Unreleased | 2026-11-28 | symmetry | wrong ok | no | 2 (write-up) | The symmetry scan did not check that no guard reads the owner key of an absent row, which the group does not relabel. | P-table P30: "No model was known to reach it" |
| D65 | P23 (#503) | Unreleased | 2026-11-28 | other | missed check | no | 2 (write-up) | The per-run comparison paired the native WF run on the reduced cone with TLC's `_WF` run on the unreduced cone. | P-table P23: "so Proposition 0 of tlc-agreement.md (both engines decide the same `T'`) failed on that path" |
| D66 | P65 (#513) | Unreleased | 2026-11-28 | reduction | wrong verdict | no | 10 (lint) | The monotonicity test listed a 256-value column's start values as an empty range, so a column set up and down read as monotone. | PR #513: "One cast was wrong" (found by denying clippy::cast_possible_truncation); P-table P65: "no wrong verdict was seen" |
| D67 | #541 | Unreleased | 2026-11-28 | reduction | crash | test-only | 2 (review/audit) | The search for the S path behind a class move ended in a `panic!` when no path existed. | PR #541: "Fixes W1 of the third theory audit"; "On main the test panics at `partition.rs:797`" (forged move) |
| D68 | P66 (#544) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (review/audit) | A `.when`-guarded `null` beside a `.when`-guarded `present` on one nullable Int was no step of the encoding, so a rule BFS and TLC refute in three steps printed `proved`. | P-table P66: "Found by reading in the fourth audit; reproduced at `d44128aa`" |
| D69 | P67 (#547) | Unreleased | 2026-11-28 | reduction | wrong ok | yes | 2 (review/audit) | A parameter on a stub kept a role no stub row meets in the extended sub-lifecycle, so folded S steps and rises were lost: a wrong `ok` on a reduced cone, and a panic in `recanon`. | PR #547: "Fixes audit 5's C1: default `verify` gave a wrong `ok`"; "`Forbidden_Order_0: ok (35 states)`. `RDRA_ISH_NO_REDUCE=1` finds a 7-step counterexample" |
| D70 | 59418550 (#569) | Unreleased | 2026-11-28 | reduction | crash | yes | unstated | `verify` panicked while building the extended sub-lifecycle: a fold keyed by a dropped row, and a rise guard naming a dropped created row. | commit 59418550: "`verify` panicked while building the extended sub-lifecycle, in two ways"; PR #569: no sample or corpus cone had the pattern |
| D71 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | reduction | unstated | yes | 4 (engine_equivalence) | (Before merge.) A fold keyed by a `New` parameter, whose slot has no identity before the step. | docs/design/event-order.md, landing plan: "`engine_equivalence` ... Its first full run found two defects: a fold keyed by a `New` parameter" |
| D72 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | reduction | crash | yes | 4 (engine_equivalence) | (Before merge.) A fold bit started false where the cone's initial state already raised it, leaving the initial S state without a class. | docs/design/event-order.md: "left the initial S state without a class and made `verify` of clinic-ops panic" |
| D73 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | TLA+ emit | unstated | yes | 4 (engine_equivalence) | (Before merge.) The class conjunct read a primed input before the UNCHANGED that binds it. | docs/design/event-order.md: "Two TLA+ defects came with them: the class conjunct read a primed input before the `UNCHANGED` that binds it" |
| D74 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | TLA+ emit | unstated | yes | 4 (engine_equivalence) | (Before merge.) The decoder took an input's value from the representative instead of the module's own variable. | docs/design/event-order.md: "the decoder took an input's value from the representative instead of the module's own var" |
| C01 | 09ba5d6f | v1.2.0 | 2026-11-05 | TLA+ emit | claim | unstated | unstated | The note `export --kind tla` prints said TLC finds every deadlock; the exported `.cfg` sets CHECK_DEADLOCK FALSE. | commit 09ba5d6f: "the export note no longer says TLC finds every deadlock" |
| C02 | P2 (#514, #525) | Unreleased | 2026-11-28 | prover/z3 | claim | yes | 2 (write-up) | What `proved` quantifies over was stated three ways; "any Int value" was false given P43. | P-table P2: "with the P43 construct \"any Int value\" was false, not merely loose" |
| C03 | P5 (#490, #525) | Unreleased | 2026-11-28 | prover/z3 | claim | unstated | 2 (write-up) | The proof notes overstated the CTI premise, the lemma limit, maximality, and the count range. | P-table P5: "The proof notes overstated details" |
| C04 | P6 (#475) | Unreleased | 2026-11-28 | BFS/liveness | claim | yes | 2 (write-up) | The language reference stated `eventually` as `<>expr`, but a goal is read on a deleted row at its base values. | P-table P6: "reproduced: with `pending` the default, `leads_to(status == paid, status == pending)` is `ok` on both engines" |
| C05 | P8 (#475) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | The notes said a Kani harness proves the canonical form invariant under the group; no such harness existed. | P-table P8: "The Kani claims for symmetry were overstated" |
| C06 | P9 (#475) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | Design notes kept stale statements on which symmetric readers are sound and on what a delete resets. | P-table P9: "Stale design statements" |
| C07 | P10 (#477) | Unreleased | 2026-11-28 | other | claim | unstated | 2 (write-up) | formal-verification.md said a stopped cone leaves its invariants not decided; the code reports a counterexample found before the stop. | P-table P10: "not a bug; prose fixed in #477" |
| C08 | P16 (#486, #497, #508) | v3.0.0 | 2026-11-28 | other | claim | unstated | 2 (write-up) | Gates the notes described were not in force; release-gate item 2 of v3.0.0 held on a narrower check than written. | P-table P16; CHANGELOG v3.0.0 Erratum (2026-11-28): "So item 2 held on that narrower check, not as written" |
| C09 | P20 (#518, #522, #529, #531, #541) | Unreleased | 2026-11-28 | BFS/liveness | claim | unstated | 2 (write-up) | "Every native counterexample replays" did not hold for safety traces: plain traces printed as stored, reduced lifts checked by debug_assert only. | P-table P20: "\"Every native counterexample replays\" did not hold for safety traces" |
| C10 | P22, P51 (#475, #494) | Unreleased | 2026-11-28 | BFS/liveness | claim | yes | 2 (write-up) | Definition D17 omitted that neither engine checks an `after` assertion on a step that changes nothing. | P-table P22; P51: "a use case whose every step is a self-loop and whose assertion fails after it gave `ok` on both engines" |
| C11 | P31 (#475, #516, #521) | Unreleased | 2026-11-28 | reduction | claim | unstated | 2 (write-up) | The reduction's soundness argument used, without stating it, the step that ties the extended sub-lifecycle to the cone (Lemma 6). | P-table P31: "The reduction's proof omitted the step that ties the extended sub-lifecycle to the cone" |
| C12 | P33 (#512, #516) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | Claim A20 said every rule that leaves a class out has a witness; `PinnedStart` and ten reasons had none. | P-table P33: "Group rules were kept without a witness" |
| C13 | P41 (#490) | Unreleased | 2026-11-28 | prover/z3 | claim | yes | 2 (write-up) | The notes said a `.lemma()` rule is never checked by BFS; a false one printed `refuted` and failed the run. | P-table P41: "a false one printed `refuted: lemma Forbidden_Door_0`" |
| C14 | P56 (#475) | Unreleased | 2026-11-28 | BFS/liveness | claim | unstated | 2 (review/audit) | Definition D20 let a stuttering self-loop take a fair instance, unlike the code and WF_vars. | P-table P56; commit 02e5b48f "native.md review fixes; P54-P56" |
| C15 | P60 (#493) | Unreleased | 2026-11-28 | builder | claim | unstated | 10 (review) | semantics.md defined the transition system as what the build constructs, so a builder fault was part of the specification. | P-table P60; numbering: "P25 and P60 and later come from the owner's review" |

## Threats to validity

**Discovery is often not recorded.** 24 of the 74 defects (32%) have no source that says
how they were found, among them every defect of v0.2.0 to v0.4.0 (D01 to D14). Most of these
fixes were committed directly to `main`, before pull requests carried descriptions, and
their commit messages state the defect and the fix only. The counts of the other classes
are lower bounds.

**The record favours the period it was written in.** The theory documents and their audits
(2026-11-28) number, reproduce and cite every item, so class 2 is recorded in full. Earlier
defects appear only when a commit message or a CHANGELOG entry names them, and defects
fixed inside a feature branch before merge are recorded only where a design note lists them
(D71 to D74). A defect found and fixed in one commit with a neutral message is missing.

**Class 2 mixes two activities.** Writing a theory document checks each claim against the
code, and many items were found by that reading rather than by a failed proof step; the
P-table does not say which. The audits and reviews (subclass `review/audit`) read the code
against the documents' propositions. A reader who counts only failed proof steps should
treat class 2 as an upper bound for "paper proof", and the subclass column lets the two be
split. Code reviews outside the theory documents (#67, #81, #83, #84, #425, c56f35ff) are
class 10, and several of them were made by language models ("self-review (Sonnet)",
"(Fable)").

**Classes follow the first finder named.** Several defects were found by one activity and
then reproduced or pinned by another: P40 and P54 are failed by the reference interpreter
and the engine corpus after the fact, P67 by Lean, reduction-diff `--stubs` and the TLC
refinement check. Only the first named finder is counted, so the later checks get no credit
for defects they would have caught.

**Inclusion is a judgement.** The line between a defect and a design change follows the
sources' own words. #355, #359 and #374 are counted because the issues say the engines
contradicted the language reference; #78 and #112 are not, because the issues ask the owner
to choose a semantics. The scope excludes code generation, the language server and the
exports, so the record says nothing about defects there. Effects are as the sources state
them; where a source shows a wrong reading without its direction, the row says `wrong
verdict`, and `shown` is `unstated` where the source neither reproduces nor rules out the
wrong output.

**Severity is not weighted.** A wrong `ok` on a model no user writes and a crash on a sample
count one each. The `shown` column separates the defects reproduced on some model from
those found by reading alone (P30, P55, P65, P11, P23, P52, and Kani's D45).
