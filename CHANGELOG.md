# Changelog

## Unreleased

### Breaking

- Every lifecycle finding now prints its stable name after the severity:
  `error[Deadlock_Order_status_paid]: deadlock: ...` instead of `error: deadlock: ...`, and
  `warning[...]:` and `info[...]:` likewise, in `check`, `states`, `verify` (its BFS
  findings and its rendering of each TLC counterexample), `export --kind tla`, and the
  generator gates. The `deferred` lines under `not checked:` gain the same `[<Name>]`, and
  `states --format table|json` print `[error][<Name>] ...` instead of `[error] ...`. The
  message after the prefix is unchanged. Load and consistency diagnostics keep the unnamed
  `error: ...` and `warning: ...`. A script that matches `^error: ` on lifecycle findings
  must match `^error\[`.
- In the `// expect` header grammar of `scripts/verify-samples.sh`, `fail` now needs at
  least one name, and the script reads each name from `error[<Name>]:` instead of from the
  message. An unnamed `error: ...` line in a run is a mismatch. `partial_scope.rdra` and the
  `check` header of `after_assert_unmet.rdra` now name their errors
  (`DerivedColumnRead_Audit_score`, `AfterAssert_DeliverOrder_0`).
- `states --entity` derives only the named entities' views, each on its own cone, with the
  invariants and after-asserts that read them; before, it filtered the whole run's stdout.
  The stdout of a run is unchanged. The exit status now counts only the selected targets
  and the build errors their cones read, so an error on another entity no longer fails it,
  and the stderr note that counted those errors is gone. `--entity` is now repeatable.
- The `note:` lines for a state, a deadlock, or a use case a run did not decide are now one
  `not checked:` block, in `check` and `states` on stderr and in `verify` on stdout. A
  whole run prints it only when a cap left something undecided; no sample does.
- `verify` ends a passing run with `OK: no errors within 2 instances per entity, Int 0..5`
  instead of `TLC: OK`, and `check` ends with `OK: no errors within 1 instance per entity,
  Int 0..5` instead of `OK: no errors`. A script that matches `TLC: OK` must match
  `OK: no errors within` or, better, `tlc_result: ok`, which is unchanged. The `OK: no errors`
  prefix of `check` still matches. The repo's own scripts and workflows match neither.
- One `bounds:` line, from one type, now heads `check` (stderr), `states` (stderr), and
  `verify` (stdout). `states` no longer names `--max-patterns` in it, only the state cap
  (`4096 states per cone`). `verify` no longer prints `bounds: InstanceCount=2 IntRange=0..5`;
  the line now reads `bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2,
  IntRange=0..5); entities a check does not read are unbounded`.
- A state of a state model that no reachable state holds is now `NoWitness`, an error in
  `check`, `states`, `verify`, and the generator gates. A state of a state model is a
  `transitions` endpoint or a value a creating use case sets, on a lifecycle column that is
  not a `variation`. Before, it was the `UnreachableEnumVariant` warning. No sample has
  one; a model with a `from` state that nothing enters now fails. Every other unheld Enum
  variant keeps the warning.
- `verify` runs BFS on every entity's cone before TLC, at the same bounds and at most
  2,097,152 states per cone, and a `NoWitness` there fails the run even when TLC passes
  (`tlc_result: ok`, no `OK:` line, exit 1). Its `bounds:` line gains `2097152 states per
  BFS cone`. On clinic-ops the pass adds about 11 s.
- An instance at a state of a state model that has a way out, from which no path moves it
  (no edge changes its column or deletes it), is now `Deadlock`, an error in `check`,
  `states`, `verify`, and the generator gates. An end state is one with no `transitions`
  arm to another state; a deletion leaves every state. The check is per instance, so a use
  case that keeps running elsewhere does not hide a stuck instance. When a way out lacks
  only a free slot of an entity it creates, it is a `deadlock at the bound` warning. Each
  state is reported once, with each way out and what blocks it, and a shortest path into
  the first stuck state. A capped cone or a `--buc` run leaves it undecided under
  `not checked:`. `verify`'s summary line gains `D deadlocks` before the use cases that never
  run. TLC's `CHECK_DEADLOCK` stays `FALSE`. On the samples: `cross_order_payment` left
  a cancelled Order's Payment pending forever, so its Cancel now deletes the Payment (the
  split variant keeps it, so its counterexample stays); `unwitnessed_state` also fails on
  `Deadlock_Order_status_shipped`. clinic-ops has none.

- `@check` and `@derived` take an unquoted, typed expression: `@check(qty >= 1)`,
  `@derived(total - discount)` or `@derived(qty >= 3)`. The quoted form is a parse
  error whose message is the rewrite. The expression must name the entity's own
  columns. Each `@check` is the invariant `Check_<Entity>_<column>_<n>` that `check`
  and `verify` test; a `@derived` column is no state var, and a `sets` on it is an
  error.
- A generated key column takes the parent key column's name: `{role}_{parent key column}`
  instead of `{parent}_id`. The default role is the parent id in lowercase, and every
  sample's parent key is `id`, so no sample output moves. A model whose parent key is
  not `id` gets different column names.
- Two relates from one child to one parent with the same role are `DuplicateRelateRole`
  at the relate. Before, the second key was silently dropped as an `FkConflict`. `FkConflict` now means a column the entity declares collides with a generated
  one, and its hint names `.role(...)` and `.key(...)`.
- A `.along(A, B)` over two keys between the same pair is `AmbiguousOwnerLink` unless
  `.by(role)` names one. Before, the lifecycle followed the first owner between the pair
  without a message. `.by` that names no key on the path is `UnknownOwnerRole`, and `.by`
  without `.along` is `ByWithoutAlong`.
- `.role(...)` or `.key(...)` on an N:M relate is `KeyOptionOnManyToMany`. An unknown
  `.on_delete(...)` or `.on_update(...)` action is `UnknownRefAction`.
- `CompositePkFkUnsupported` is gone. A relate to a composite `@pk` generates one key
  column per parent key column.
- A 1:1 relate now has a key column on its left entity. It appears in DBML, JSON Schema,
  the entity CSV, `list --kind entity`, and the ER diagrams on `ec-site`, `clinic-ops`,
  and `personal-info`, and DBML prints the ref as `Ref: Child.col - Parent.id`.
- Every 1:1 key is the invariant `Unique_<Child>_<role>`, and every key with a minimum
  of one child is `MinCard_<Child>_<role>`. Both fail `check` and `verify` where a model
  breaks them, so a model that passed can now fail.
- The `ec-site`, `personal-info`, and `clinic-ops` state models change, because
  `Unique_*` found a use case that ran twice and linked two children to one parent.
  `ec-site`: `InputPaymentInfo` moves the Order `pending -> awaiting_capture`, `Capture`
  leaves from `awaiting_capture`, and `Cancel` also takes `awaiting_capture -> cancelled`.
  `personal-info`: an account starts `provisioned`, `SetInitialPassword` moves it to
  `active`, and `SuspendAccount` and `DeleteAccount` leave from `active`. `clinic-ops`:
  `OpenEncounter` moves the Appointment `apptcheckedin -> apptinconsult`,
  `CompleteAppointment` leaves from `apptinconsult`, `CollectCopay` runs at check-in
  before any encounter, and `PaymentTransaction -> Claim` is `.optional()` like
  `Charge -> Claim`.
- `SeparateTxInferred` reads: `usecase 'X' writes 'E' in its own transaction after the
  FK-grouped writes`. The sequence diagrams and the lifecycle read the same transaction
  groups (`UsecaseTx::groups()`), so a `writes(..)` edge, and an entity a use case only
  `sets` or transitions, now draw as an update. The `rdra-ish-core` items
  `tx_groups`, `infer_lifecycle_transactions`, and `has_writes` are gone.
- `timing` is now `timer`, an RDRA タイマー that starts a use case through
  `triggers(Timer, UseCase | Buc)`. `belongs(...).when(...)` is removed (an unknown chain
  method whose hint names `timer`); `.by(...)` stays. `uses(Actor, ExtSystem)` is
  removed. An external system now meets use cases through events: `raises(ExtSystem,
  Event)` inbound and `triggers(Event, ExtSystem)` outbound, and an event delivered to
  an external system counts as consumed. `raises`, `triggers`, `covers`, and `performs`
  widen to take these kinds (`performs` also takes a `Step`); `triggers(Timer,
  ExtSystem)` is a `TypeMismatch`. `extsystem` moves to the system-value layer of the
  RDRA diagram, and `timer` stays in the external-environment layer. `export --kind
  asyncapi` lists raising and receiving external systems as the channel's `x-publisher`
  and `x-subscriber`, and the event-flow diagrams draw timers, external systems, and a
  `delivers` edge. `scripts/migrate-phase3.py` renames `timing`, drops
  `belongs(...).when(...)`, and deletes `uses` lines, printing the external systems
  that lost their only link. The removed words `timing` and `uses` fail to parse with a
  hint.
- `transitions(Entity.col, UseCase, from -> to)` names the use case that drives the
  edge, as the 遷移UC column of an RDRA state sheet does; `sets` takes a use case origin
  only. The event forms `transitions(E.col, Ev, a -> b)` and `sets(event::Ev, ...)` are a
  `TypeMismatch` whose hint names `scripts/migrate-phase3.py`. That script rewrites each
  event form into one line per use case that raises the event, deletes events left with
  no consumer together with their `raises`, keeps the events passed to `--keep`, and
  prints the transitions it cannot migrate because no use case raises their event.
  TLA actions are named `{Entity}_{UseCase}_{from}_to_{to}` (was
  `{Entity}_{Event}_{from}_to_{to}`), and one event raised by two use cases now yields
  one action per use case. State diagrams label edges by use case. An event whose raiser
  drives a transition but that nothing triggers now draws `EventNeverConsumed`.
- `variation` is a keyword again (it was a removed word), and a bare identifier in
  column type position now names a variation instead of failing to parse. An inline
  `Enum` that no `transitions` and no `sets` moves draws the `enum-attribute` warning.
- `condition` is a keyword again (it was a removed word). It stays usable as a column
  name and a module path segment.
- Unknown chain methods on predicates are an error (`unknown chain method '.x' on
  predicate 'p'`). Previously they were silently ignored, so a typo such as
  `belongs(U, B).wehn(T)` or the removed `belongs(...).where(...)` loaded with
  `OK: no errors`. The accepted methods per predicate are listed in the language
  reference under Chain methods.
- `PatternCapReached` (pattern truncation via `--max-patterns`) is `[error]` /
  fail-closed: constraints checked on a truncated set are not sound.
- `rdra-ish check` uses the same state-diagnostic severity as `states` (a reachable
  forbidden state or truncation → `error:` and exit 1). Previously those were printed
  as `warning:` before `OK: no errors`.
- `states --format csv` / `type-script` exit non-zero on Error-severity state
  diagnostics (exit no longer depends on scanning `[error]` in rendered text).
- Outputs that print the lifecycle refuse to emit when state diagnostics are Error
  severity: `states`, `list`, `diagram` of every kind except `er`, `csv` of every kind
  except `entity`, and `export --kind typescript-states`. `export --kind tla` and
  `verify` refuse on the Error diagnostics of the transition system they print. The data
  exports no longer refuse: `export` of `dbml`, `json-schema`, `openapi`, `asyncapi`,
  `mermaid-er`, and `plantuml-er`, `diagram --kind er`, and `csv --kind entity` read no
  reachability result, so they gate on model errors only and print one stderr note when
  the lifecycle has errors. The six verification samples with lifecycle errors now
  export DBML, JSON Schema, and the entity CSV.
- `ConflictingEffects` (same-origin dual assigns, including Int `sets`) is
  `[error]` / fail-closed (was warning with last-wins).
- Junction composite unique uses the N:M endpoint FK pair only (extra FKs on the
  junction are excluded). Sharing one `.via` across multiple N:M pairs is an error
  (`SharedJunctionVia`).
- `actors_for_usecase` unions BUC-level and direct `performs` (direct no longer
  hides BUC performers in permission analysis).
- Removed the kinds no review output consumed. A model that still uses one now
  fails to parse with a message naming the keyword and the replacement:
  - `usagescene`: use a `timer` with `triggers(Timer, UseCase)` for Business-BUC
    context.
  - `location` and the `.where(...)` clause of `belongs`: no output consumed
    them. Delete the declaration and the clause; keep `medium` / `.by(...)`.
  - `state`: states are the variants of the lifecycle Enum column. Delete the
    declaration; the analyzer still derives `{Entity}_{variant}` state nodes from
    `transitions` for the state diagram.
  - `concept`, `domain_object`, `aggregate`, `value_object` (alias
    `valueobject`): conceptual-model kinds were removed; model persistent
    structure with `entity` (and `dto` for payloads).
  - `maps_to` predicate: removed with the conceptual-model kinds.
  - `contains(Aggregate, DomainObject | ValueObject | Concept)` and the
    Concept / DomainObject / Aggregate / ValueObject targets of `decides` are
    gone; `decides` keeps its other targets.
  - `rdra-ish list --kind` no longer accepts `concept`, `domain-object`,
    `aggregate`, or `value-object`; `diagram --node-kind` / `--edge-kind` no
    longer accept the matching node kinds or `maps-to`. The `domain-object` and
    `value-object` lint orphan-node kinds and the `ReservedKind` diagnostic are
    gone, as are the `rdra-ish-core` types `Concept`, `DomainObject`,
    `Aggregate`, `ValueObject`, `UsageScene`, `Condition`,
    `ConceptualRef`, and `ConceptMapping`.
- `states`, `check`, `export --kind tla`, `verify`, the TypeScript state unions, and the
  state diagrams read one transition system (`rdra_ish_core::lifecycle`). The crate root no
  longer re-exports the state-pattern types of `state_pattern.rs`, which is deleted.
  `StateDiag` loses `CrossConstraintNotEvaluated`, `UndrivenComparisonProp`,
  `TemporalAssertionNotEvaluated`, and `QuantifierConstraintNotEvaluated`: those
  constraints now get real verdicts, so a model that failed `check` only on them can
  pass, and a model that passed can now fail on a violation they hid.
- `--allow-tla-only` is removed from `check`, `states`, `diagram`, `csv`, `list`, and
  `export`. No state diagnostic is TLA-only any more. (The flag was added earlier in this
  release and never shipped.)
- The TLA+ text changes shape. Every per-instance variable is a function over
  `<E>_Ids`, with an `<E>_exists` variable (`absent`, `present`, `deleted`) and
  `<Child>_owner_<Parent>` owner links. Each use case is one operator `<UC>(p0, ...)`,
  or `<UC>_1..k` for k transaction groups, in place of `{Entity}_{UseCase}_{from}_to_{to}`
  and `Entity_uc_X`. The clock action is `Tick` (was `TickNow`). The `.cfg` has one
  `INVARIANT` per constraint, named `<Kind>_<Entity>_<n>` (`Forbidden_Order_0`), and no
  `Safety`, and `after(UC).assert` is `PROPERTY AfterAssert_<UC>_<n>`. The synthetic
  `Assign_*` actions and the `[TLA_FATAL:…]` tags are gone: `export --kind tla` and
  `verify` fail on the Error diagnostics of the system (`IntLiteralOutOfBounds` replaces
  `int_literal_out_of_range`).
- `verify` checks each invariant and `after` assertion on its cone of influence, and the
  temporal properties on the whole model. `-o DIR` gets one `Cone_<check>.tla` / `.cfg`
  pair per cone and `cones.txt`, and the whole-model `RdraSpec` pair only when the model
  declares temporal properties.
- `DoubleModeledEnum` is an error (was a warning). A use case that does not create the
  entity and `sets` a column that `transitions` drive is refused instead of ignored.
- `ContradictoryDrivenComparison` is an error: a use case drives a comparison to one
  truth while its own Int `sets` make it the other. `UnbindableCondition` is an error: a `holds` body applied to a use case that writes
  nothing has no action to guard. `UnbindableScope`, `IntLiteralOutOfBounds`,
  `AssertOnInertUseCase`, and `UnsupportedComparison` are errors that every command
  reports from the same diagnostic stream.
- The `cross_order_payment` verdict flips from counterexample to `ok`: `Pay` is one atomic
  action over the linked Order and Payment, so `Cancel` cannot run between the two writes.
  `int_stock` now passes `check` (both engines evaluate `stock < selling` over `0..5`).
  CI expects both.
- A `relate` foreign key column is the owner variable, so it is no longer an axis of
  `states`. An `.optional()` relate lets the child exist unlinked.
- `states` output changes. An Int column that something names is an axis and prints its
  values (`stock[0..5]`, JSON domain `"0".."5"`). `TERMINAL` means a use case deletes the
  instance or no enabled use case moves it, and `VIA` is the least set of use cases on
  some path to the pattern. The pattern order changes for `ec-site` Order and Payment and
  `clinic-ops` Appointment. A constraint violation no longer carries the global-product
  or flow-order hint. `states` prints a `bounds:` line on stderr, and diagnostics that name
  no entity go to stderr as `warning:` or `error:`. `--max-patterns` caps each entity view
  within one target cone, and an entity that a target does not read has no slot limit.
- A `@default` on a column typed by a variation no longer pins the value. A created instance
  takes any value of the variation unless its use case sets it, and a pre-existing
  instance starts at any value. `@default` still sets the DBML default. In `clinic-ops`,
  `PaymentTransaction.method` is now chosen by `PostPayment`, and `CollectCopay` creates
  an insurance payment only.
- An entity that no use case creates starts present at its column defaults
  (`NoCreationPath`, info). The order of `raises` and `triggers` between use cases no longer
  guards a use case.

### Added

- Traceable verification (phase 6). Every constraint is its own named invariant, a state
  of a state model with no reachability witness and an instance that can never leave its
  state are errors, a use case that never runs is a warning, and a counterexample prints as
  use cases with their bound instances and source locations, from BFS and from TLC alike.
  `--buc` and `--entity` check part of a model, every result opens with its `bounds:` line,
  and CI checks and verifies every sample against the `// expect` header it carries.
- `rdra-ish files <INPUTS...>` prints the `.rdra` files the inputs load, one per line, in
  load order (each directory's entries sorted by file name).
- `scripts/verify-samples.sh <binary>` runs `check` and `verify` on every `samples.txt`
  root and compares each run with the `// expect` header of the root's first file: the
  exit status, the `tlc_result:` line, and the exact set of error names
  (`CrossForbidden_Order_0`, `AfterAssert_DeliverOrder_0`, `Witness_*`, `Deadlock_*`).
  `// expect verify --entity Order: tlc=ok` adds a run with flags. It prints the wall time
  and maximum RSS of each run. The nine counterexample samples now carry headers, which
  moves their source lines down by two (`cross_order_payment_split.rdra:41:1` is now
  `:43:1`). The grammar is in `docs/formal-verification.md` ("Sample expectations").
- `check`, `states`, and `verify` take `--buc <id>` and `--entity <id>`, both repeatable,
  to check part of a model. `--buc` keeps only the use cases reachable from the BUCs, so
  witnesses, deadlocks, and use cases that never run are not decided. `--entity` selects
  the entity's view and the invariants and after-asserts that read it; each runs on its
  exact cone, and a use case no selected cone keeps is not decided. A build error whose
  entities no selected cone reads is deferred: it is listed under `not checked:` and does
  not fail the run, while the whole run still fails on it. An error raised while a use
  case's guards or effects were built also involves the entities that use case writes, so
  a selection whose cones read that use case still fails on it. A scoped run prints a
  `scope:` line after `bounds:`, skips temporal properties in `verify`, and ends with `OK:
  no errors in the selected scope, within ...`. New sample `partial_scope.rdra`: its whole
  run fails on a `DerivedColumnRead` on Audit, and `verify --entity Order` passes with
  the error deferred.
- New sample `deadlock.rdra`: an Order stays `paid` forever because Ship's packing
  condition holds only while `pending`. `check` and `verify` fail on
  `Deadlock_Order_status_paid`, and TLC passes.
- Use cases that never run. An action (a use case, or one transaction group of a split use
  case such as `Pay_2`) that no edge of any entity cone that keeps it carries is the
  `NeverRuns` warning in `check`, `states`, and `verify`; a self-loop counts as running.
  It prints once per action, with the use case's site and only what blocks it: an instance
  no reachable state has (`needs:`), the instances never present at once, each guard that
  holds in no reachable state (`guard:` with its site), or a first line saying the guards
  each hold somewhere but never together. A capped cone or a `--buc` run leaves it
  undecided under `not checked:`. `verify` adds `state models (BFS on N entity cones): M state
  models, K states witnessed, J use cases that never run` to stdout. On the samples, Refund
  in `unwitnessed_state` warns (its guards never hold together), and AddLine in
  `min_card_met` warns in `check` only: at one instance an Order exists only with its one
  OrderLine slot taken. clinic-ops CollectCopay runs.
- Reachability witnesses. `states --format json` gives each pattern a `witness`: a
  shortest path from the start to the first state that holds the pattern, with each
  step's use case, bound instances, changes, and sites. `check` prints a `NoWitness` once,
  with each `transitions` arm or creation that would enter the state and its site. A cone
  that stops at a cap, or a `--buc` run, leaves its unwitnessed states undecided and
  lists them under `not checked:` with the cap or the scope. The witness logic is
  `state_model_findings`, a pure function of the system and a cone's `Graph`. New sample
  `unwitnessed_state.rdra`: `check` and `verify` fail on `Order.status = refunded`, and
  TLC passes.
- Logical data layer (phase 5). One `ForeignKey` fact per key-side `relate` feeds DBML,
  JSON Schema, the entity CSV, `list --kind entity`, the ER diagrams, the transaction
  graph, and the lifecycle owner variables. New relate syntax: `.role(x)` names a key,
  `.key(cols)` adopts columns the child declares, and a composite parent `@pk` yields a
  multi-column key (`Ref: Stock.(bin_site, bin_shelf) > Bin.(site, shelf)`).
  `.along(A, B).by(role)` picks the key a constraint follows. A child-side range
  (`1..*:1`, `1..1:1`) states a minimum, and `fmt` prints the card as written. New
  diagnostics: `DuplicateRelateRole`, `AmbiguousOwnerLink`, `UnknownOwnerRole`,
  `ByWithoutAlong`, `KeyArity`, `KeyColumnUndefined`, `KeyTypeMismatch`,
  `KeyColumnShared`, `KeyOptionOnManyToMany`, `UnknownRefAction`, `MinCardOutOfRange`,
  `MinCardOnParentSide`, `MinCardOnManyToMany`. See the `relate` section of the
  [language reference](docs/language-reference.md#relate-and-fk-generation).
- The lifecycle checks the new keys: `Unique_<Child>_<role>` for a 1:1,
  `MinCard_<Child>_<role>` for a minimum of one child, and `Check_<Entity>_<column>_<n>`
  for each `@check`, in BFS and in the TLA+ export. A pre-existing child's owner starts
  linked (`Any` under an N:1 key, `Identity` under a 1:1 or a `1..*` key). New state
  diagnostics: `PreExistingChildOfCreatedParent` (warning), `MinCardOfPreExistingParent`
  (warning), `MinCardNeedsCreate` (info), `DerivedColumnRead` (error). JSON Schema and
  the ER diagrams show a minimum of one child: `x-rdra-ish-fk-min-children`, and a
  one-or-more child end.
- Formal-verification samples `one_to_one_shared`, `min_card_unmet`, `min_card_met`,
  `check_unmet`, and `check_met`. CI expects a TLC counterexample for `one_to_one_shared`,
  `min_card_unmet`, and `check_unmet`, and a pass for `min_card_met` and `check_met`.
- The test `key_facts` walks every `samples.txt` root: each key-side `relate` yields one
  fact, no DBML ref is unresolved, DBML, JSON Schema, CSV, ER, transaction edges, and
  owner variables agree, DBML and JSON Schema export, and each `@check` has one `Check_*`
  invariant.
- `scripts/sample-artifacts.tsv` lists every committed `out/` file of `ec-site`,
  `clinic-ops`, `personal-info`, `step-6-business-rules`, and `api-contract` with the
  command that writes it, and `scripts/check-sample-artifacts.sh` regenerates all of
  them. `scripts/data-diff.sh LABEL` snapshots the DBML, JSON Schema, and entity CSV of
  every sample so two builds compare with `diff -r`.
- The 15 不整合 columns of the RDRA definition sheet are lint rules, held as one table
  (`RDRA_RULES` in `rdra_rules.rs`) with a witness model per row. The four "unconnected"
  columns are new `lint` warnings: `rdra-actor-no-buc`, `rdra-extsystem-no-buc`,
  `rdra-entity-no-buc`, `rdra-condition-no-buc`. rdra-ish adds `rdra-variation-no-buc`,
  `rdra-timer-no-buc`, and `rdra-transition-uc-no-buc` (a use case that `transitions` names
  must be in a BUC). The 11 "undefined" columns stay `check` errors and the table names the
  diagnostic for each. `orphan-node` no longer reports the kinds a rule owns, so one
  element draws one finding. `docs/rdra-ish-interpretation.md` maps each column to its rule
  id. The `ec-site` sample connects `ShippingSvc` through a new `EvOrderShipped` event, and every sample passes every rule.
- `condition Id "Label"` is an RDRA 条件, the combination of variations and a state
  model. `combines(Condition, Variation | Entity.column)` links it for traceability (the
  column must be a lifecycle column some `transitions` moves). `holds(Condition, cmp,
  ...)` gives it a body, an implicit conjunction of entity-qualified comparisons; the
  links are then derived from the columns the body names, and `combines` on the same
  condition is an error. `applies_to` now takes a condition as well as an `nfr`. New
  diagnostics: `ConditionCombinesWithBody`, `ConditionSecondBody`,
  `ConditionCombinesAttribute`, `AmbiguousCombinesTarget`; body values report
  `InvalidVariationValue` / `InvalidEnumVariant`. New lint `condition-no-rdra-anchor`.
  `list --kind condition` shows the body and the derived links, the RDRA diagram draws
  conditions in the system layer with edges to their variation, state model, and use
  cases (a `--buc` view includes the conditions applied to its use cases). A body
  guards the use cases it applies to (see the lifecycle layer below). The
  clinic-ops sample gains `CopayDue` (traceability only) and `InsuredVisit` (with a body).
- The RDRA object-graph diagram now lists `variation` nodes, which it already placed in
  the system layer but never drew.
- `variation Id "Label" { value "Label"? ... }` declares the values an actor chooses
  from (RDRA バリエーション). A column typed by its id (`method: PaymentMethod`) is an
  attribute, not a lifecycle: `states` gives it no axis and no unreachable-variant
  warnings, `transitions` rejects it, and `export --kind dbml` emits one `Enum` per
  variation shared by every column typed by it. JSON Schema and OpenAPI list its values
  as `enum`; ER diagrams and `csv` / `list --kind entity` show the variation id as the
  column type. `list --kind variation` lists them. The TLA+ export gives a variation
  column a variable only when a `sets`, constraint, `after` assertion, or `property`
  names it, with a free-choice `Init` over every value. New diagnostics:
  `UnknownColumnType`, `InvalidVariationValue` (`@default` and `sets` values).
- Lint `enum-attribute` warns on an inline `Enum` column that no `transitions` and no
  `sets` moves; the hint is to type it by a variation. The clinic-ops, ec-site and
  personal-info samples now type their attribute columns by variations
  (`PaymentMethod`, `ConsentType`, `OrderType`, `MessageChannel`, `AuditSeverity`,
  `CardBrand`), so `states` no longer reports their unchosen values unreachable.
- `export --kind tla` / `verify --backend tlc` accept `--instance-count` (default
  2) and `--int-range-max` (default 5). The `.tla` header comment and the `verify`
  output state the bounds. `--int-range-max 0` is rejected;
  `--instance-count 1` warns that singleton ids cannot distinguish instances.
- `relate(A, B, N:M).via(Junction)` — many-to-many with an explicit junction
  entity; auto-generates both FK columns on the junction and a composite unique
  on the FK pair when missing. Bare `N:M` is an error. Also synthesizes `N:1`
  edges `Junction→A` / `Junction→B` so TLA owner links, ER, and `.along(A,
  Junction, B)` see the path. Self N:M and `.via` equal to an endpoint are
  errors; unresolved `.via` does not also emit bare-`N:M` `NMRelation`.
- OpenAPI export: `{path}` parameters, unknown `auth` as `apiKey` +
  `x-rdra-ish-auth-scheme`, and `x-rdra-ish-requires-permission` from
  `requires_permission(Api, Permission)`.
- Lint `flow-cycle` for `precedes`/`branches` cycles; empty models score
  `coverage-score` 0 with missing-stage penalties (−8 each).
- `states --format typescript` accepts alias `type-script`; `--entity` prints a
  stderr note when other entities still have Error-severity diagnostics.
- TLA owner links keep every `(child, parent)` pair (junction with two parents
  uses `{Child}_{Parent}_owner`); owner-only entities still get `*_Ids`.
- Multi-hop `.along` adds binders for intermediate entities and warns instead of
  silently dropping the link filter.
- Import scopes keep same-id bindings of different kinds (e.g. `event Capture` +
  `usecase Capture`); `kind::Id` resolves across modules. CI runs `check` on
  sample models including `samples/ec-site`.
- Date / DateTime column-vs-column comparisons are Int arithmetic in `states` and in
  the TLA+ export.
- Sequence emitters (PlantUML / Mermaid) use `actors_for_usecase` (BUC ∪ direct
  performers) so a direct `performs` no longer hides BUC-level actors.
- `check --strict` promotes permission/access gaps to errors; `check` always
  notes outstanding temporal properties.
- OpenAPI emits `oneOf` (+ `x-rdra-ish-ambiguous-status`) for multiple
  `response` / `error_response` DTOs.
- The lifecycle layer, `crates/rdra-ish-core/src/lifecycle/`: existence states (`absent`,
  `present`, `deleted`), one atomic action per use case per transaction group, owner
  links from `relate`, a clock for `now`, and constraints as invariants over the present
  instances. [docs/state-derivation.md](docs/state-derivation.md) describes it.
- Each `states` view and each constraint verdict is explored on its exact cone of influence,
  the smallest set of variables the target reads. The cone's reachable states equal the
  whole system's projected onto it, which a test checks on `ec-site`, `personal-info`, and
  the formal-verification samples. `verify` runs TLC per cone in parallel, so clinic-ops
  finishes where a whole-model run did not.
- The test `engine_equivalence` compares BFS and TLC on every cone of every sample, at 1
  and 2 instances: the reached and initial states, the edge sets `(from, to, use case)`
  from TLC's `-dump dot,actionlabels`, the witness, never-runs, and deadlock findings by
  name, and the verdict of each invariant and `after` assertion; every TLC violation trace
  must replay through the BFS step relation. A cone over 200,000 BFS states at 2 instances
  is compared at 1 instance only and must be listed in `VERIFY_FALLBACK` (8 clinic-ops
  cones). The CI `formal-verify` job runs it in release with `RDRA_REQUIRE_TLC=1`. `scripts/engine-diff.sh` snapshots
  `states`, the TLA text, and `verify` per sample.
- `.optional()` on a `relate` is honoured: the owner may stay `null`, and creating the
  child leaves it unlinked.
- `applies_to(Condition, UseCase)` with a `holds` body, or `applies_to(Condition, Api)` for
  each use case that invokes the API, is a guard in `states`, `check`, `export --kind tla`,
  and `verify`. A comparison on a created entity constrains the creation. The `verify`
  notice `body stored, not yet checked` is gone.
- A use case whose writes form k > 1 transaction groups is k actions `<UC>_1..k` that
  interleave freely, and `after(UC).assert` holds after each step. No existing sample
  splits; `cross_order_payment_split.rdra` (`Pay` through two APIs) is the counterexample
  that CI expects.

### Fixed

- A use case that never runs because, once its guards hold, the instance bound leaves no
  free slot for an entity it creates now says so (`needs: a free Payment slot within the
  instance bound`). Before, it listed a guard that failed only in the states where a slot
  was still free.
- 91 of the 126 committed `out/` files under `ec-site`, `clinic-ops`, and `personal-info`
  no longer matched what the generator prints, because no CI step regenerated them. They
  are regenerated, and `check-sample-artifacts.sh` now covers them.
- `check` dropped the verdicts of the constraints that an entity without state columns
  binds, because its view returned before they were added. It now reports them.
- `MinCardNeedsCreate` fired for a use case that creates a parent and updates an existing
  child, although the action links that child and the minimum holds.
- A use case that creates a parent no longer takes an already-linked child from its
  parent when the key is `.optional()`. The action links the child only while it is
  unlinked. A mandatory key still moves the child.
- `diagram --kind er` refused on lifecycle errors while `export --kind mermaid-er` did
  not. Both now draw the data layer.
- A bad equality value in a multi-entity constraint (`forbidden(A, B, A.status == gone,
  ...)`) reported the same mistake two or three times, once of them as a missing entity
  qualifier on the value. It now reports it once.
- `export --kind tla` works on `samples/api-contract` (the generated `TypeOK` referred to
  an undefined name) and on `samples/clinic-ops` (it ended in `TLA_FATAL:init_pending_fallback`).
- `clinic-ops` Charge and Claim no longer form a creation cycle: a charge's claim is
  `.optional()`, because `GenerateClaimApi` creates the claim after the charges exist.
- State diagrams draw the `[*]` start edges from the creation status of the transition
  system. `clinic-ops` Appointment and `personal-info` UserAccount gain start edges they lacked.
- `verify -o <relative dir>` no longer breaks TLC's temporary directory.
- `verify` without `-o` deleted its spec dir before it printed the paths into it. A failed
  run now keeps the dir and ends with `specs: <dir> (kept because a check failed)`; a
  passing run still deletes it. With `-o`, a failed run ends with `specs: <dir>`.

### Changed

- CI checks and verifies every `samples.txt` root, clinic-ops included, in one
  `formal-verify` step that runs `scripts/verify-samples.sh` on the release binary
  (`timeout-minutes: 5`, job `timeout-minutes: 15`). It replaces the `check` job's sample
  loop and the five per-sample `verify` steps (the pass list, the counterexample loop, and
  the `unwitnessed_state`, `deadlock`, and `partial_scope` steps). `engine_equivalence`
  runs with `cargo test --release`: the debug build took 315 s on the runner, over its
  300 s budget. The Linux dev shell adds GNU `time` for the script.
- `samples/clinic-ops` now models Japanese hospital outpatient operations, each business
  rule cited to a primary source in a `//` comment. Payment moves after the consultation into
  a new `BucVisitPayment` (会計計算入力, 呼出, 窓口 or 自動精算機, 領収証), so `CollectCopay`,
  `CopayDue`, and `InsuredVisit` are gone. `Claim` becomes `ClaimStatement` (レセプト) with
  返戻, 査定, 再請求, 再審査請求, and 保留 states and monthly timers. `EligibilityService`,
  `PharmacyNetwork`, and `LabSystem` become `OnlineEligibilitySystem`, `EPrescriptionService`,
  and `OutsourcedLabSystem`, with `InsurancePharmacy` and `ClaimsReviewOrganization` added.
  `FrontDesk` and `BillingSpecialist` become `ReceptionClerk`, `AccountsClerk`, and
  `ClaimsClerk`, with `MedicalClerk` and `Pharmacist` added. The largest `verify` BFS cone
  drops from 1,707,212 states (`AccountBalance`) to 969,167 (`Charge`).
- `check` prints each violated constraint or `after` assertion once, with a shortest
  counterexample in model terms: the rule's `path:line:col`, then one line per step with
  the use case, its bound instances (`Place [Order#1, Payment#1]`), and its declaration
  site, and under each step the values it changed with the site of the `transitions` arm
  or `sets` that wrote them. A multi-entity rule used to print once per entity it binds.
  When the check's cone does not count an entity's slots and the trace creates more of
  them than the instance bound, a `note:` says the whole model may not reach it. `states`
  and the generator gates report one violation per rule instead of up to 16.
- `verify` prints each TLC counterexample of an invariant or `after` assertion in model
  terms, through the renderer `check` uses, before TLC's `violated:` line and raw trace.
  It decodes TLC's states and replays them through the step relation BFS uses, and the
  violation message comes from the same code as in `check`; only the header adds
  `from TLC`. A step that replays to no move fails closed: `verify` says BFS and TLC
  disagree and prints the raw trace. Temporal counterexamples stay raw.
- `verify` runs each cone's TLC with `-continue`, so every violated check of a cone prints
  its own counterexample, and the state count on a failing cone's line is the whole cone.
  Each cone JVM gets `-Xmx1g` and the temporal run `-Xmx2g`. `RDRA_TLC_HEAP` (such as
  `4g`) replaces both, and a `-Xmx` in your own `JAVA_TOOL_OPTIONS` wins over either. A
  run that runs out of memory is a `tool_error` whose message names both settings.
- Language reference separates reserved kinds and unsupported/limited topics;
  UC `precondition`/`postcondition`/`guard` are documented as review metadata
  only (use `after.assert` for machine-checked posts).
- Bare `relate(..., N:M)` without `.via` is an error (was a warning).
- `coverage-score` no longer rates empty models as 100; stage-readiness gaps
  reduce the score; finding penalties are size-normalized (`×25/elements`).
- `lint` no longer reports `orphan-node` or `naming-id` on derived state nodes:
  a transition now counts as a connection for the states it joins, and state ids
  (`{Entity}_{variant}`) are tool-generated, so the naming rule skips them.
## v0.2.0 - 2026-07-20

### Breaking

- Conditions use comparison expressions only: `col == val`, `stock < selling`
  (no tuples or flat `col, val` pairs).
- Transitions require an enum column: `transitions(Entity.col, Event, from -> to)`.
  Global `state` labels are no longer used as transition endpoints.
- Cross-entity rules use multi-entity `forbidden(...)` / `invariant(...)` (with
  optional `.along(...)`); `cross_*` and `forbidden_when` surface forms are removed.
  Quantifiers are `when(...).none/has(...)`.
- `relate(..., N:1)` cardinality is unquoted. Temporal connectives prefer
  `and` / `or` / `not` (`/\` `\/` `~` remain as aliases).
- PostgreSQL type names are rejected in `sets`; use `present` for nullable non-null.

### Added

- `rdra-ish export --kind tla` and `rdra-ish verify --backend tlc` for TLA+/TLC.
  Export writes both `.tla` and `.cfg`. Coverage includes Int / Money / Decimal /
  `now` arithmetic axes, multi-entity `forbidden`/`invariant` (with `.along` /
  `*_owner`), `when(...).none/has`, `after(...).assert` equality and comparison
  postconditions, and multi-instance temporal quantifiers
  (`[](\A…)`, `\A i: <>…`, `\A i: (p ~> q)`).
- `property` label string is optional (`property StockOk always(...)`).
- Skill `rdra-ish-verify` for TLA+/TLC formal verification workflow (when to use
  TLA vs `states`, DSL surface, export/verify, bundled `samples/*.rdra`).
  Install via `npx skills` / `gh skill`; see `skills/README.md`.
- All skills declare `license: MIT` for Agent Skills / `gh skill publish` validation.

### Fixed

- Fail-closed validation: empty `Enum()`, lexer junk, unknown predicates / wrong
  arity, duplicate columns, missing input paths, and generator commands that
  previously succeeded on invalid models.
- Parse errors print human-readable “unexpected …; expected …” messages (no
  chumsky Debug dumps); `N:M` / temporal hints match the current surface;
  `value_object` is accepted alongside `valueobject`.
- Language-ref multi-entity `invariant` examples are valid; `N:M` is documented
  as unsupported (use an intermediate entity); Stage 5 no longer requires
  standalone `state` declarations; `condition`/`variation` marked reserved.
- User-facing multi-entity state diagnostics say “multi-entity” (ids may still
  be `Cross*`); OpenAPI lint warns when APIs lack both method and path;
  README learning path leads with incremental modeling.
- TLA `.cfg` sets `CHECK_DEADLOCK FALSE` so terminal lifecycle states are not
  reported as TLC deadlocks; CI TLC smoke-check accepts `-help` non-zero exit.
  Multi-instance exports always `EXTENDS Integers` for `1..InstanceCount`.
- Import scopes honor `as` aliases, selective imports, and namespaced refs;
  same id may exist in different modules when referenced via aliases.
  Closed scope is per-file (siblings without `import` stay open-world);
  unknown modules, duplicate module paths, and conflicting `import` All
  bindings are errors; module-scoped resolution no longer falls back to
  another module's unique match.
- Formal-verify CI exports `PATH` in the TLC install step and requires
  TLC-native fingerprints (`tlc_result: ok` / `tlc_result: counterexample`).
- Duplicate `import … as` aliases are errors; Init prefers the first Enum
  variant when no `states`/`@default` seed exists.
- FV docs/skills align with unconstrained Assign/TickNow and `now_coupon`
  as an expected TLC fail; cli-reference documents fail-closed generators;
  language-reference documents composite `@pk` FK limits.
- `after.assert` exports as independent `[][action => post]_vars` properties
  (no primed injection into SpecActions); stuttering-only Next is fatal;
  TLA obligation failures use `[TLA_FATAL:…]` tags only (legacy substring
  matching removed); multi-post asserts fold to `Act => (p1 /\ p2)`.
- Init `"pending"` last-resort is `[TLA_FATAL:init_pending_fallback]` unless
  `pending` is a declared Enum variant; `diagram --node-kind`/`--edge-kind`
  reject unknown kinds; `scripts/install-tlc.sh` verifies tla2tools.jar SHA256.
- `diagram --kind diff` fail-closes on semantic errors in `--diff-base`.
- TLA+: unique action names for multi-edge events; unresolved/dropped properties
  and contradictory `after.assert` fail export/verify; `now` Safety is not baked
  into `Assign`/`TickNow`; sets-only entities emit actions.
- `states` exits non-zero when output contains `[error]`.
- Composite `@pk(a,b)` recorded on the entity; duplicate API method+path rejected.
- FV samples: `now_coupon` is an expected TLC counterexample (Safety is checked,
  not enforced by Next).

### Changed

- Formal-verification docs and skills use multi-entity `forbidden`/`invariant` surface
  names (diagnostic ids may still say `Cross*`). Int/`now` are documented as a separate
  TLC layer from BFS comparison propositions. Docs/skills point to `export --kind tla`
  for quantifiers, temporal `property`, and relation-scoped `.along`. Sibling skills
  route deep FV work to `rdra-ish-verify`. FV sample `.rdra` files are canonical under
  `skills/rdra-ish-verify/samples/` with repo path symlinks for tests. Samples table
  records TLC pass/fail intent; order snapshot tracks the skill sample.
- CI runs TLC (`formal-verify` job) against pass/fail FV samples.

## v0.1.7 - 2026-06-23

### Added

- Added typed semantic-model representations for state transitions, concept-to-entity
  mappings, and parsed predicates (`StateTransition`, `EntityLifecycle`,
  `ConceptualRef` / `ConceptMapping`, `TypedPredicate`).
- Added `EntityStateVariant` derivation for reachable lifecycle states as
  discriminated unions.
- Added TypeScript state-union export via `rdra-ish states --format typescript`
  and `rdra-ish export --kind typescript-states`.

### Changed

- Updated GitHub Actions workflows to Node.js 24.
- Added manual `workflow_dispatch` trigger for PyPI publishing.

## v0.1.6 - 2026-06-16

<!-- derived-from ./docs/language-reference.md#relationship-predicates -->
<!-- derived-from ./docs/cli-reference.md#export -->
<!-- derived-from ./docs/state-derivation.md#diagnostics -->

### Added

- Added first-class business-flow modeling with `flow`, `step`, `precedes`,
  `branches`, `excepts`, `repeats`, and `covers`.
- Added richer requirement, ADR, NFR, quality, constraint, conceptual-model,
  screen-field, DTO, and API contract vocabulary.
- Added data-modeling annotations for indexes, composite constraints, checks,
  FK optionality/actions, soft delete, history, tenant scope, and derived columns.
- Added OpenAPI, AsyncAPI, DBML, JSON Schema, Mermaid ER, and PlantUML ER exports.
- Added diagram filtering, view presets, diff diagrams, and description rendering.
- Added `rdra-ish lint` and `rdra-ish fmt` for model readiness and formatting.

### Changed

- System ownership can now be declared with `owns(System, Entity)` and compared
  against API-derived ownership diagnostics.
- Relation-scoped `.along(...)` cross constraints now evaluate representative
  operation-linked cases instead of always falling back to global cross-product
  behavior.
- Skill definitions under `skills/` now guide modeling with the expanded DSL
  vocabulary and release/export review paths.

### Breaking Changes

<!-- derived-from ./docs/language-reference.md#file-structure-and-comments -->

- Line comments in `.rdra` files use `//`.
- Legacy `#` line comments are not accepted by the parser. Existing models that used
  `#` comments must migrate those lines to `//`.

### Migration: `#` Comments to `//`

Use a mechanical line-comment replacement for `.rdra` sources:

```sh
find path/to/model -name '*.rdra' -exec perl -pi -e 's/^(\s*)#/$1\/\//' {} +
```

Review the result if your model uses `#` inside string literals or external snippets.
Block comments remain available as `/* ... */`.
