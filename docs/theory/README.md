# The theory of rdra-ish's verification

This directory states what rdra-ish's verification tools decide, under which assumptions,
and on what evidence, for a reader trained in formal methods. Each document defines its
terms before it uses them, states its guarantees as propositions, and says for each one
whether it rests on a proof, a mechanical check, a differential test, a run-time check, or
an assumption.

This document describes main at `91524792`.

It is the entry point: what the verifier guarantees, engine by engine; what those guarantees
rest on; what every run checks; the residual risks; the known limitations; and a map of the
documents. Each statement points at the row of the [claims inventory](./claims.md) that
states it with its evidence and tests. Code is cited by path and symbol, with the
abbreviations of the [claims inventory](./claims.md#how-to-read-a-row): `core/` is
`crates/rdra-ish-core/src/lifecycle/`, `cli/` is `crates/rdra-ish-cli/`, and `emit/` is
`crates/rdra-ish-emit/src/`.

## What the verifier guarantees

A model `M` at bounds `β` denotes a transition system `T_β(M)`, defined from the model's
text in [semantics.md](./semantics.md) (D1 to D13; D24 under a scope). `check` and `verify`
decide its rules and properties and print one verdict per check: `ok`, `counterexample`,
`not decided`, or, from the prover, `proved`; a fault of the tool itself is `tool_error`
([semantics.md, D23](./semantics.md#d23)). Every engine reads the one system the build
returns, so every guarantee below is about that system, and is about `T_β(M)` exactly when
the build implements the definition (claim [I1](./claims.md#0-the-build)).

### The model and its system

- **The build implements the definition** for every model, bound and scope at which it
  reports no error (I1). The build refuses what it cannot encode faithfully: a column domain
  of more than 256 values (`DomainTooLarge`), a state past the bits a packed state addresses
  (`StateTooWide`), and an action that writes one `(var, param)` twice
  (`ConflictingEffects`). Evidence: per-construct readings, the hand-written corpus headers,
  and a reference interpreter that rebuilds the system from the `SemanticModel` without the
  builder on a subset of the language.
- **Readings a modeler should know.** Each is a decision, stated in the language reference
  and pinned by a test, under which every verdict is right for the model as written:
  - null in a comparison is two-valued, so `x != 3` holds on a null `x`; a lint flags
    `not` over an ordering of a nullable column (I2);
  - verification executes no referential action: `.on_delete(cascade)`, `set_null` and
    `set_default` delete the one row and leave the children, with a warning (I3);
  - a liveness goal is read on a slot whatever its existence, so deleting a row can meet a
    goal its base values satisfy; `.present()` asks for a present row (N9);
  - an `after` assertion does not check a step that changes nothing, as in TLA+ (N5, N18);
  - one raise runs a handler any number of times unless `.counted(k)` says otherwise
    ([semantics.md, D11](./semantics.md#d11)).

### Native BFS

The native verifier explores the graph of a cone, a reduced cone, or a quotient by
symmetry, and decides each check on that one graph ([native.md](./native.md); claims
N1 to N18).

- A complete run stores exactly the reachable states, each with a shortest-path parent
  (N1); the packed store reads back every state it stores (N2); the memory charge is
  deterministic and never overflows (N3).
- Any number of threads computes the one-thread run: the same states, numbering, edges,
  findings and stop point, except for a stop by the wall-clock budget (N4, N17).
- Safety: a state rule, `always`, or `after` assertion is violated iff a stored state or move
  violates it, reported with a shortest trace (N5); `stable`, `unchanged`, `precedes`
  (N6, N7); `can_reach` on a complete graph (N8); deadlocks as D25 defines them (N14).
- Liveness under weak fairness by fair strongly connected components (N9); on a stopped
  graph only a counterexample decides (N10); up to symmetry, the same verdict as the plain
  run (N12).
- A cap or the budget gives `not decided`, never `ok` (N15, A24). A vacuity warning never
  changes a verdict (N18).
- Every printed counterexample replays through the step relation of its run before it
  prints; one that does not is a tool error, never a counterexample (N11, N13, N16, A16).

### Cones, the reduction, and symmetry

The verifier decides most checks on a cone of influence, often reduced by an independent
sub-lifecycle and explored up to slot permutations ([cones.md](./cones.md); claims A1 to
A28).

- An exact cone decides what the whole system decides, and a trace lifts (A1). A cone that
  frees slots or leaves a raised mark out over-approximates: its `ok` is sound, and its
  counterexample counts only once a refined or exact cone vouches for it (A2, A6, A28).
  Entity cones and `can_reach` cones are exact (A2, A7).
- A property cone decides under its own fairness by the three-run protocol (A4, A5).
- The sub-lifecycle reduction is a divergence-blind stuttering bisimulation, relative to the
  stability of its partition, which every build certifies, and to Lemma 6 (A8 to A15, A22).
  `_WF` runs on a reduced cone are sound relative to the same hypotheses (A15).
- Instance symmetry is a bisimulation under the group the scan computes; leaving a class out
  costs states, never a verdict (A18 to A23).
- Bound escalation turns a counterexample at `n+1` or `n+2` instances into a failure only
  when it is a trace of the larger system, and prints the instances per entity (A25).
- A covered cone's `ok` is the covering cone's (A27). The memory limit bounds the BFS stores,
  not the process (A26).

### Proofs with z3

`--prove` encodes the system for any number of rows and runs Houdini with mined lemmas and
CTI clauses ([z3.md](./z3.md); claims B1 to B18).

- The encoding over-approximates the system at every bound in the range the model builds
  at (B1). A `proved` rule holds for any number of rows and every `--int-range-max` in the
  printed range, under the printed scope (B2). The prover proves nothing from an
  unsatisfiable premise (B2).
- Initiation and consecution are checked on every candidate; `unknown` never becomes
  `proved`; mined lemmas, CTI clauses and `.lemma()` rules add nothing to the trust base
  (B4, B5, B9 to B12, B17).
- A cached certificate is used only after it re-checks (B15). The verdicts do not depend on
  the number of solvers (B14).
- A rule BFS refutes is not tried, and refuting it changes no other verdict; a proof can
  decide a cone stopped at the memory limit (B16). `proved` covers state rules only (B18).

### Agreement with TLC

`verify --with-tlc` prints each cone as a TLA+ module and runs TLC beside BFS
([tlc-agreement.md](./tlc-agreement.md); claims T1 to T16).

- The printed module denotes the cone's system (T1). Every check and run both engines decide
  gets the same verdict, every TLC trace replays through BFS's step relation, and a
  disagreement ends the run as `tool_error` (T2, T3, T4, T6).
- The evidence that the two readings agree: the sample comparison of states, edges with
  their params, and findings (T5); the engine corpus with hand-written verdicts (T7);
  random models reduced against unreduced (T9); symmetric against plain runs (T10); batch
  JVMs against processes (T11); the scheduling knobs (T12); Kani harnesses (T13); shadow
  twins (T14); mutation runs (T15); the Int range (T16).

## Trust base and assumptions

<a id="trust-base"></a>

The guarantees above hold given these components and assumptions, none of which the project
verifies:

- **z3** answers `unsat` only for unsatisfiable queries (H1), and the prover takes each
  answer for the check that asked it (H4); z3 5.1.0 is deterministic under a fixed resource
  limit (B13, B15). Every (b) row assumes H1 and H4.
- **TLC** (1.7.4, 1.8.0) and the JVM are correct. Every (d) row assumes it; CI runs the
  devShell's 1.8.0 pre-release on JDK 25.
- **The build** returns `T_β(M)` (I1). It is shared by every engine, together with the
  parser, `build_model`, the choice of a cone, the staging of a property cone's runs, the
  quotient tables of a reduced cone, and `TlaNames`.
- **Paper arguments** that are sketches, not proofs: Lemma 6 of the reduction (A8), the
  per-construct argument for the TLA+ printer (T1), the theorems marked `P-sketch` in the
  [summary by evidence class](./claims.md#summary-by-evidence-class).
- **Kani** and the Rust compiler. CI's verification jobs build with overflow checks (profile
  `ci-checked`), so a fixed-width integer that wraps panics there; release builds wrap
  silently.

## What every run checks

<a id="run-time-checks"></a>

Every run, in every build:

- replays every printed counterexample through the step relation of the system it is about,
  and reports one that does not replay as a tool error (N16); the lifts' own look-ups are
  faults too (A16, N13);
- refuses, at build time, a column domain past a byte (`DomainTooLarge`), a state past the
  packed width (`StateTooWide`), and an action that writes one slot twice
  (`ConflictingEffects`) (I1, N2, T1);
- panics on an edge number past `u32` instead of wrapping it (`edges.rs::edge_number`, N1);
- certifies the stability of the reduction's partition and falls back to the sweeps'
  partition when the certificate fails (`partition/reference.rs::stable`, A8);
- refuses to prove anything from an unsatisfiable premise (`prove/mod.rs::premise`, B2);
- uses a cached certificate only after it re-checks (B15);
- with TLC, replays every TLC trace through BFS's step relation (T2).

The internal invariants left as `expect`s and `panic!`s on a lift's way stop the run and
print no wrong trace; they are listed, with the reason each holds, in
[claims.md, "Panics left on a lift's way"](./claims.md#panics-left-on-a-lifts-way).

## Residual risks

<a id="conclusion"></a>

<!-- derived-from ./tlc-agreement.md#residual-risk -->

Two residual risks outweigh the rest. They are the first two items of
[tlc-agreement.md, "Residual risk"](./tlc-agreement.md#residual-risk), and the other theme
documents arrive at them from their own side.

1. **Common-mode faults.** The builder (I1), the choice of a cone, the staging of a property
   cone's runs, the quotient tables of a reduced cone, and the names that map native states
   to TLA+ ones (`TlaNames`) are read by both engines, so a fault in any of them moves the
   native verifier and TLC alike, and agreement with TLC cannot show it. A fault of the
   prover's encoding that no sample or corpus case exercises is not common-mode, but no
   comparison of a proof with BFS meets it either. The reference interpreter checks the
   builder without reading its output, on a subset of the language, and checks z3's
   `proved` at `m + 1` and `m + 3` (I1, B1). Outside that subset, and for the cone choice,
   the property-cone ladder, the quotient tables, and `TlaNames`, only hand-written
   expectations, such as the engine corpus's `expect` lines, see such a fault, and only on
   the models they name (T7).
2. **No run-time check of an `ok` or a `proved` as such.** A counterexample is checked
   before it prints (N16); an `ok` and a `proved` are not. The causes of a wrong one that
   are known are refused or checked in every run (see
   [What every run checks](#what-every-run-checks)). A fault of another kind that gives a
   wrong `ok` or `proved` passes unnoticed in the run. The evidence against it is offline:
   the corpus, the reference interpreter, and the differential tests, which CI's
   verification jobs run with overflow checks. A wrong `proved` can also make a cone stopped
   at the memory limit print `ok` (B16).

## Known limitations

<a id="open-problems"></a>

<!-- derived-from ./claims.md -->

Each item is open at this commit, and each names the claims rows that state it. The list is
what has been found, not a bound on what exists.

**The build and the language**

- Outside the reference interpreter's subset, only the corpus headers check model to `T`,
  on the models they name; the reference shares the parser, `build_model`, the rule names,
  and which Int columns are numbers (I1, T7). The subset is listed in I1.
- Nothing checks at run time that a bound in a proof's printed `--int-range-max` range
  builds the same system; Lemma U is a reading of the code (B2, I1).
- semantics.md takes which models are well-formed from the code's list of error diagnostics
  (I1).
- Verification executes no referential action, while the SQL export writes
  `ON DELETE CASCADE` / `SET NULL` / `SET DEFAULT`, so a rule over the children can be `ok`
  in the model and broken against that database; executing them is the owner's call (I3).
- No sample or corpus model draws the `negated-order-admits-null` warning (I2).

**Cones, the reduction, and symmetry**

- Lemma 6, which ties the extended sub-lifecycle to the cone, is argued, not proved; the
  tests check its fold equivalence and its silent steps on the models and bounds they run
  (A8, A11; A5, A6, A10, A15 rest on it).
- A release build does not check that a step changes only the variables `Wr(a)` names; debug
  and `shadow` builds do (A1, B6).
- No test makes the reduction reject a quotient past the TLA+ cell budget (A13); A14 and A27
  have no test of their own.
- No Kani harness proves the canonical form invariant under the group; it rests on an
  exhaustive unit test and a twin (A18, T13).
- That a counterexample a cap-stopped safety cone finds is real has no test (A24).
- Most of the lift's look-up faults are reached by no test (A16).
- The memory limit does not bound the process (A26).

**Proofs with z3**

- `Enc::compare` re-implements `expr.rs::compare` and is tied to it by reading and
  differential tests only; no code under `core/prove/` has a Kani harness or a twin (B1).
- `proof_equivalence` skips 130 comparisons on the samples, cones whose BFS passes its state
  cap (B1).
- The checks z3 gives up on by itself are counted, not asserted (B16).
- No test checks that an unstateable rule is `out of scope` or a guard is dropped (B7), plants
  a false mined lemma (B9), or asserts that `proved` covers state rules only (B18); B6 has
  no test. Determinism across machines and solver counts is empirical (B13, B14).

**The native verifier**

- The loom model, test jitter, and probe harness of native-bfs-parallel.md are not built, and
  no workflow runs Miri (N4).
- The naive oracle shares the step relation and the evaluator, and does not cover entity
  cones up to symmetry (N8, N14).
- No test checks an `always` trace against an expected one (N5), drives a stored path that
  fails to replay through `trace_violations` (N16), or runs the second half of
  `a_path_that_does_not_replay_is_a_fault` (N13).
- No Kani harness inserts into the state table and looks a row up (N2).
- The internal invariants of [Panics left on a lift's way](./claims.md#panics-left-on-a-lifts-way)
  are unchecked; among them, that `Extended::rebuild` reproduces `Extended::of` is not
  checked (N16).

**Agreement with TLC**

- The 25 `VERIFY_FALLBACK` cones meet TLC at 1 instance only, and oidc-auth's browser flow at
  2 instances rests on the native checker alone (T5).
- The printed reduced step has no per-construct argument, and that no two params of one
  action that can bind one slot write one variable is assumed, not checked (T1).
- TLC's deadlock check is one-sided (T6); batch JVMs are compared with processes only on
  cones of at most 5,000 states (T11).
- A call above a shadow twin's cap is not compared (T14).
- The TLC-side mutation run's kill rate is not recorded, and the mutation runs start only by
  hand (T15).

**Process**

- None of the CI checks is required: main has no branch protection and the repository no
  ruleset (`gh api`, checked 2026-11-28), so merging on green checks is a convention
  (T9, T11, T15; [tlc-agreement.md, "Process gates"](./tlc-agreement.md#5-process-gates)).
- The proposal to restate items 2, 3 and 7 of the release gate to match what runs
  ([native-temporal-verification.md, "Migration", "The release gate"](../design/native-temporal-verification.md#migration))
  awaits the owner's decision.

## Documents

| Document | Contents |
|---|---|
| [Semantics and notation](./semantics.md) | The transition system `T_β(M)` a model denotes, defined from the model's text, the meaning of every rule and property on it, fairness, scopes, and the verdict vocabulary. Definitions D1 to D25. |
| [Claims inventory](./claims.md) | One row per guarantee the tools make or the docs claim, with its evidence classes, code, tests, and open gap: I1 to I3 (the build), A1 to A28, B1 to B18, N1 to N18, T1 to T16, and the summary by evidence class. |
| [(a) Cones of influence and reductions](./cones.md) | The cone of a check and its closure; when a cone's reachable set is the projection of the model's and when it is a superset; the reduction by an independent sub-lifecycle and its quotient; `_WF` runs on reduced cones; instance symmetry (the group, orbits, entity cones, `DeletedRowOwner`); bound escalation to N+1 and N+2 instances; memory caps and the meaning of `not decided`. Claims A1 to A28. |
| [(b) Theorem proving with z3](./z3.md) | The SMT encoding (`prove/smt.rs`, `prove/encode.rs`), what is folded and how Int terms are bounded; induction with Houdini; the lemma candidate shapes; what `proved` covers for any number of rows and what `.lemma()` adds to the trust base; rlimit determinism; the certificate cache; how proof and BFS refutation interact. Claims B1 to B18. |
| [(c) The native verifier](./native.md) | BFS and the packed state store; parallel exploration and why it computes the one-thread result; safety, liveness under fairness, the temporal operators, `can_reach`; lifting a counterexample from a reduced or symmetric run to the model. Claims N1 to N18. |
| [(d) Agreement with TLC](./tlc-agreement.md) | The TLA+ translation, the agreement rules, the engine corpus, the sample and random-model comparisons, shadow twins, Kani harnesses, the mutation run, and known disagreements. Claims T1 to T16. |

Each theme document takes its rows of the [claims inventory](./claims.md) as its table of
contents: it states each of its claims as a proposition in the notation of
[semantics.md](./semantics.md), gives the proof or proof sketch or names the check that
stands for one, states the assumptions, and lists threats to validity and related work.

### Reading order

1. [semantics.md](./semantics.md), whole. Everything else is stated in its notation.
2. [claims.md](./claims.md), to see what is claimed and how well each claim is supported.
3. (c), then (a). The native verifier decides a check on one explored graph; the cone
   document says why a graph of a cone, a reduced cone, or a quotient by symmetry decides
   what the model's graph would.
4. (b), which is independent of (a) and (c) except for the interaction with BFS
   refutation and bound escalation.
5. (d), last, since it is the evidence for claims the others make.

### Relation to the existing notes

<!-- derived-from ../formal-verification.md -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../state-derivation.md -->

The notes below are written for modelers and contributors, and record the design decisions
and measurements behind each feature. This directory does not replace them. Where a theme
document and a note disagree, the theme document states the code's behavior; the note is
corrected in its own pull request.

| Note | Audience and role | Theme |
|---|---|---|
| [formal-verification.md](../formal-verification.md) | Modelers: what `check`, `verify`, `--with-tlc`, and `--prove` print and decide. | all |
| [engine-agreement.md](../engine-agreement.md) | Contributors: the agreement rules and the tests that hold the engines to them. | (d) |
| [state-derivation.md](../state-derivation.md) | Modelers and contributors: the transition system, bounds, cones, and findings. | semantics, (a) |
| [design/cone-soundness.md](../design/cone-soundness.md) | The soundness arguments for cones, the sub-lifecycle reduction, and `_WF` on a reduced cone. | (a) |
| [design/instance-symmetry.md](../design/instance-symmetry.md) | Design and measurements of the quotient by slot permutations. | (a), (c) |
| [design/native-temporal-verification.md](../design/native-temporal-verification.md) | Design of the native temporal checker and its migration from TLC. | (c), (d) |
| [design/native-bfs-parallel.md](../design/native-bfs-parallel.md) | Design and measurements of parallel BFS and its shared table. | (c) |
| [design/counted-events.md](../design/counted-events.md) | Counted triggers: semantics, cones, TLA+, z3. | semantics, (a), (b) |
| [design/counting-rules.md](../design/counting-rules.md) | Counted quantifiers and per-entity instance counts. | semantics, (a), (b) |
| [design/event-order.md](../design/event-order.md) | Raised marks, and the rounds of the reduction (monotone inputs, the size guard, the node fold). | semantics, (a) |
| [design/tlc-scheduling.md](../design/tlc-scheduling.md) | How TLC runs are scheduled under `verify --with-tlc`. | (d) |
