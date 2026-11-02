# Changelog

## Unreleased

### Added

- `list --kind screen`, `event`, and `timer` list the declared screens, events, and timers
  as `id,label`, sorted by id, in every `--format` (#70), as `flow`, `step`, and `system`
  do. Before, `--kind` rejected them.

- A set that lists every value of its column (`Child.status in (<every value>)`) is true,
  and its `not in` false, when the lifecycle builds its system (#71). `.none(Child.status in
  (<every value>))` therefore says "no child row" and reads only the link, so a creator
  guarded that way pulls no column of the child into the cones that keep it. The cone of
  `samples/food-permit-ops`'s `Disposition` with such a guard on `RegisterPermit` falls from
  248,032 states to 23,156 at 2 instances, and the sample now registers one permit per
  application (`relate(BusinessPermit, PermitApplication, 1:1)` with the guard
  `NoPermitYet`). The fixtures `none_any_child` and `none_any_child_bypassed` cover it.
  `lifecycle_measure` takes a project path in place of a `samples.txt` line and prints each
  cone's variables under `RDRA_CONE_VARS=1`.

- `verify` warns on stderr when it leaves a finding undecided, a temporal property past
  `--temporal-budget` or a finding a BFS cone left at its state cap: `warning: 2 temporal
  properties the budget left undecided; the OK line holds only for what was decided
  (--deny-undecided fails the run)`. `verify --deny-undecided` makes that an error
  (`verification incomplete: ...`, exit 1). Findings a `--buc` or `--entity` scope leaves
  open do not count.
- `sets(UseCase, Entity, column == Source.column)` copies a value from another row (#69).
  The use case leaves in the column the value the source column holds in the row the step
  binds, read as a `holds` atom on that entity reads it: a row the use case writes in the
  same transaction group (before the step on an update, as the step leaves it on a
  creation), or the target's parent through a `relate` (the parent a child is created
  under, or the one an updated child's foreign key names; the use case waits while an
  `.optional()` key is unlinked). Any other source is `UnboundCopySource`, an error: an
  entity the step does not write that is not the parent, a child of the target, an entity of
  another transaction group, or the parent of a child created unlinked. The two columns must
  hold one set of values (one Enum, one variation, Bool, Int-typed, Date-typed, or nullable
  Strings; a nullable source needs a nullable target), else `SetsCopyMismatch`. A copy and a
  literal `sets` of one column are `ConflictingEffects`, and a copy with `chooses` is
  `ChoosesAndSets`, and a copy into a lifecycle column is `DoubleModeledEnum`. `check`,
  `states`, the TLA+ export, and `verify` read it the same way: a choice of the column's
  value pinned by the guard `col'[p] = source[q]`, which a cone reads only when it holds the
  copied column. The sequence diagram and `list --kind surface` show the source, and the LSP
  renames it. Before, such a `sets` was `SetsNoEffect`.
  `samples/food-permit-ops` registers each permit under its application
  (`relate(BusinessPermit, PermitApplication, N:1)`), copies the application's
  `business_type`, and checks that the two agree.

- An entity may have several lifecycle columns (#68). Each Enum column that `transitions`
  names is its own state model in `check`, `states`, `lint`, the TLA+ export, `verify`, and
  the state diagram (one block per column under `--entity`); guards, constraints, and
  `after(UC).assert` read any of them. Before, `transitions` on a second column reported
  `entity 'E' already has transitions on column ...` and the diagrams merged the states of
  a value two columns share. The warning (`MultipleLifecycleColumns`) is removed. An entity
  with several lifecycle columns names each state by its column,
  `{Entity}_{column}_{value}` (`state__{Entity}_{column}_{value}` in diagrams); an entity
  with one keeps `{Entity}_{value}`. `export --kind typescript` tags such an entity's
  union by `status`, else a `*_status` column, else the first lifecycle column by name.

- `verify` checks each temporal property on a cone of its own, and on the whole model only
  when the cone does not decide it (#67). The cone holds what the property reads and every
  action of the use cases its `.fair` lists, whole (their guards, bound instances, transition
  sources, created slots, and writes), closed under the reads of the actions that write into
  it, so weak fairness on those actions restricted to the cone is weak fairness on them in
  the whole model and every other action stutters for the cone. The whole model's
  `WF_vars(Next)` does not restrict to a cone, so an `eventually` or `leads_to` gets up to
  two cone runs: `Cone_<P>` without `WF_vars(Next)`, whose `ok` is the verdict, and
  `Cone_<P>_WF` with it, whose counterexample is the verdict and whose `ok` decides only
  when the cone holds every written var. Otherwise, or when the cone frees an entity's slots
  and finds a counterexample, the property runs on the whole model as `RdraSpec_<P>`, which
  now prints TLC's progress and stops at `--temporal-budget` seconds (default 300) or
  2,097,152 distinct states, reporting the property as `not decided` under `not checked:`
  instead of running on. The `temporal properties:` line says `each checked on its cone, and
  on the whole model only when the cone does not decide it (budget N s)`, and a scoped run
  says `not checked: a scoped run checks none`. `docs/formal-verification.md` states the
  condition under "Temporal properties on a cone", and
  `a_property_cone_decides_what_the_whole_model_decides` in `engine_equivalence` compares
  the cone verdict with the whole-model verdict on the temporal samples and on fixtures for
  an entity looping outside the cone, a fair use case whose guard reads another entity, a
  guard that toggles forever, a cone that frees slots, and a cone that holds every written
  var. food-permit-ops gets back `HygieneManagerEventuallyAppointed`, checked on a 223-state
  cone (its whole model found 6.9 million states in 300 s without finishing), and patent-ops
  gains `GrantEnds`, checked on a 4,105-state cone (its whole model explored 14 million
  states in 11 minutes without finishing). `lifecycle_measure` lists property cones too.

- `drives(UseCase, Entity, comparison, true | false)` drives a comparison proposition true or
  false (#51, condition 4). It is what the four-argument `sets` did: `sets` assigned a value
  with three arguments and drove a comparison with four, two meanings told apart by arity.
  Now `sets` only assigns, and `drives` only drives. A `drives` records the same proposition
  effect, so `states`, `check`, and the TLA+ export are unchanged. A value equality driven
  true (`drives(Pay, Order, status == paid, true)`) still sets the value; driven false it is
  `DrivesFalseOnEquals` (was `SetsFalseOnEquals`). A `drives` with a third argument that is
  not a comparison, or a fourth that is not `true` or `false`, is `DrivesNoEffect`. A `sets`
  with a comparison is `SetsNoEffect`, whose hint now names `drives`. A set membership in
  either is `EffectMembership` (was `SetsMembership`), naming the predicate.

- A deprecation stage for legacy syntax (#51, condition 2). A deprecated form still loads: the
  parser rewrites it to the current form and the load reports a `DeprecatedSyntax` warning
  naming the rewritten line, the version that deprecated it, and the version that removes it
  (`write <new form> (deprecated since X, removed in Y; ...)`). `lint` lists
  it under the rule `deprecated-syntax`; `check` exits as before unless `--deny-deprecated`
  makes it an error; `fmt` prints the current form, so `fmt --write` is the migration. A form
  is deprecated in a minor release and removed no earlier than the next major one.
  `LEGACY_FORMS` in `crates/rdra-ish-syntax/src/removed.rs` lists every legacy form with its
  status; the forms removed in 0.3.0 and 0.4.0 stay load errors. The four-argument `sets` is
  the first deprecated form (see Deprecated).
  `language-reference.md` "Removed forms" is now "Deprecated and removed forms".

- `.fair(UseCase, ...)` on an `eventually` / `leads_to` property assumes each listed use case is
  weakly fair per instance it acts on (#59). `WF_vars(Next)` alone holds on a run where one
  instance loops forever, so `leads_to(granted, registered or dismissed)` failed while another
  application amended its filing endlessly. The TLA+ export adds `Fair_<P>`, one
  `\A p0 \in E_Ids : WF_vars(UC(p0))` per action of each listed use case (a created slot bound
  inside with `\E`, each action of a split use case), and prints the property as
  `Fair_<P> => (...)`, which `Spec` satisfies exactly when `Spec /\ Fair_<P>` satisfies the
  property; `Spec`, the `.cfg`, and properties without `.fair` are unchanged. Per instance
  rather than per use case, because `WF_vars(\E p : UC(p))` is met by the use case running on
  any instance; weak fairness only; no model-wide switch, which would change every property's
  verdict. `verify` prints `<P> assumes weak fairness per instance on ...`; `check` and `states`
  evaluate no temporal property, with or without `.fair`. `.fair` on `always` is
  `FairOnSafetyProperty`; an empty `.fair`, a non-use-case argument, a duplicate, or a second
  `.fair` is `FairRequiresUseCases`; any other method is `UnknownChainMethod`; a listed use
  case with no effect is `FairOnInertUseCase`. `rule_audit` covers the property forms, and
  `samples/formal-verification/temporal_fair.rdra` checks the same `leads_to` with and without
  `.fair`. patent-ops declares no temporal property: a whole-model run at two instances
  explored 14 million states in 11 minutes without finishing.

- `chooses(UseCase, Entity.column)` lets the actor pick a variation column's value again on an
  update (#54). After the use case the column holds any value of the variation, one run per
  value, as on a creation: `states`, `check`, the TLA+ export (`\E v \in {..} : col' = [col
  EXCEPT ![p] = v]` on the existing row), and `verify` share it, two `chooses` in one use case
  choose independently, and a counterexample step cites the `chooses` line (a
  `sets(.., col == present)` step now cites its `sets` too). A `chooses` counts as a write of
  the entity, as a `sets` does; on the creating use case it is the creation's own choice. It is
  a predicate of its own rather than `sets(.., col == any)`, since a variation may name a value
  `any` and `sets` fixes one value. Only a variation-typed column is chosen: an inline `Enum` or
  `Bool` is `ChoosesNotVariation`, whose hint names `transitions` arms or `sets`; a second
  argument that is not `Entity.column` is `InvalidChoosesColumnRef`; `chooses` and `sets` of one
  column in one use case is `ChoosesAndSets`. The sequence diagram writes the choice as
  `col in (A, B)`. `rule_audit` covers every form, and `engine_equivalence` compares a fixture
  of choices with TLC.
- A guard per chosen value (#65): `chooses(UseCase, Entity.column, value)` lines list the
  values the actor may pick, and `.when(Cond)` on a line offers that value only while the
  condition's `holds` body holds, read as a `transitions(..).when(Cond)` arm guard reads it
  (the state the use case starts from; a created instance after the step). The value-less
  `chooses(UC, E.col)` still offers every value; both forms for one use case and column are
  `ChoosesMixedForms`, a value outside the variation is `InvalidVariationValue` listing the
  values, and `.when` on the value-less form is `ChoosesWhenWithoutValue`. A value listed
  twice is enabled by either line, as a repeated arm is. The guard is built as an arm guard
  is, `\/ (col' = v /\ body)` over the listed values, so `states`, `check`, the TLA+ export,
  and `verify` share it: a use case with no enabled value does not run (`NeverRuns` names the
  value guard), and a guarded value never chosen while its use case runs is the new
  `ValueNeverChosen` warning (`Choice_<UseCase>_<Entity>_<column>_<value>`; under a cap or
  `--buc`, `not checked:`). `.when` takes the condition `transitions` takes
  (`WhenRequiresCondition`, `WhenConditionWithoutBody`); the condition `combines` the chosen
  variation and applies to the use case in `list --kind condition`. The sequence diagram writes
  each guard after its value,
  `amendment in (限定のない補正 [最初の拒絶理由通知である], 削除・減縮・誤記の訂正・釈明に限る補正)`,
  and a counterexample step cites the line of the value it picked. `rule_audit` and `transition_audit` cover every form, and
  `engine_equivalence` compares a fixture of value guards with TLC. A `.when` condition body,
  on an arm or a value, now names the columns it reads, so they become variables: an arm guard
  on a variation column nothing else read never held before. `samples/patent-ops` merges
  `DraftResponse` and `DraftLimitedResponse` into one `DraftResponse` that chooses
  `amendment`, `free` only on a first notice (特許法17条の2), and
  `Forbidden_OfficeAction_3` holds. Its `Exclusive_OfficeAction_0` now shares the cone of
  `Forbidden_OfficeAction_3` (1,034 states at 1 instance), so `verify` runs 11 cones, not 12.
- A guard per `transitions` arm (#62):
  `transitions(UC, Entity.col, from -> to).when(Cond)` takes the arm only while the `holds`
  body of the condition holds on the state the use case starts from. The use case runs when
  some arm is enabled (and its `applies_to` guards hold), and the choice among the enabled
  arms stays free. Unlike `applies_to`, which guards every step of the use case, `.when`
  guards one arm: the use case still takes its other arms while the condition fails.
  `.when` takes one condition, once; anything else is `WhenRequiresCondition`, an undefined
  name is `UndefinedSymbol`, and a condition with no `holds` body (a `combines`-only trace
  link) is `WhenConditionWithoutBody`. `states`, `check`, the TLA+ export, and `verify` share
  the semantics: a column with a guarded arm gets one guard on the pre-state and the
  successor, `\/ (col = a /\ col' = b /\ body)` over its arms, which the explorer applies to
  the forked choice and TLC reads after the assignment. A guarded arm that is never taken
  while its use case runs and its `from` state is reached is the new `ArmNeverTaken`
  warning (`Arm_<UseCase>_<from>_<to>`; under a cap or `--buc` it is listed under
  `not checked:` instead), and a use case whose every arm is blocked is
  `NeverRuns` naming the arm guard. The state diagram writes the arm's guard in `[...]`
  after the use case's own, the sequence diagram heads the arm's `alt` branch (or the
  single write) with it, `list --kind transition` adds it to `guards`, and the condition
  `combines` the arm's state model (`derived ... from when`), so the condition reaches its
  BUC and `list --kind condition` and `list --kind rule` name the use case it guards.
  `rule_audit` and `transition_audit` cover every `.when` form, and `engine_equivalence`
  compares a fixture of arm guards (one on another entity, one that never holds) with TLC.
  `samples/patent-ops` merges `DecideToFile` and `DecideNotToFile` into one `DecideFiling`
  (出願可否の決定) with three arms from `searched`; only `decided_file` is guarded, by
  `RightsAcquired`, and the rule 「会社が特許を受ける権利を取得していない発明は出願を決めない」
  still holds (it breaks when the `.when` is removed). Its largest cone at 1 instance stays at
  1,034 states.
- Set membership in comparisons: `status in (paid, shipped)` holds when the column is one of
  the values (`==` on any), and `status not in (draft, cancelled)` when it is none (`!=` on
  each). It is accepted wherever a comparison is: `forbidden`, `required`, `exclusive`,
  `invariant` with `.when/.then/.has/.none` (one entity or several, `Entity.col in (...)`),
  `holds`, `after(...).assert`, `@check`, `@derived`, and `property`. Each value is read by the
  left-hand column's type as a bare right-hand side of `==` is; the column is Enum,
  variation-typed, Bool, or `@null`, and an Int, Money, or Decimal column takes integer
  literals (`qty in (1, 2)`). A value of another type is `SetValueNotAValue`, which lists the
  column's values, a repeated value is `DuplicateSetValue`, any other column type is
  `SetColumnNotEnumerable`, and `in ()` is a parse error. `sets` leaves one value, so `sets`
  with `in` in either form is `SetsMembership`. `states`, `check`, and TLC read the same
  disjunction, and the TLA+ export writes `\in {"a", "b"}` and `\notin`. `list --kind rule`,
  `list --kind condition`, the sequence diagram's guards, and violations print the set as
  written. `in` is a keyword only before its `(`, so a column may still be named `in`; the
  VS Code grammar highlights `in` / `not in` there. The long `!=` chains of patent-ops
  (`StillPending`, `PriorityBasePending`, `DivisionPossible`, and the rule
  「審査請求のない出願は審査されない」) are rewritten with `in` / `not in`, and the guard of
  `DeemPriorityBaseWithdrawn` now names the states in which the earlier application is no
  longer pending (#53).

- `samples/patent-ops`, a second domain written without changing the language (#51): the
  patent application work of a Japanese company's IP department with an outside patent firm
  and the 特許庁 as an external system, from the invention disclosure through filing, the
  formality check, the examination request, office actions, registration, and annuities, to
  the inventor's reward. 9 BUCs, 65 use cases, 14 timers for the statutory periods, 11 labelled
  rules, and a `//` citation of the primary source for each rule, period, and fee. It is in
  `samples.txt` (`check: ok`, `verify: tlc=ok`) and its diagrams, tables, `design-doc.md`, and
  `prd.md` are in `scripts/sample-artifacts.tsv`. `scripts/gen-clinic-ops-docs.py` takes the
  sample as a second argument (`clinic-ops` by default; its output is unchanged).
- `samples/food-permit-ops`, the second soak of #51 condition 3, written without changing the
  grammar, the parser, the core, or the emitters: the food business permit work of a Japanese
  保健所, from the consultation and the classification of the business (permit, notification, or
  neither) through the application on paper or through the 食品衛生申請等システム, the facility
  inspection, the permit or its refusal, the term and its renewal, the change, succession, and
  closure notices, and the business notification, to the monitoring plan, site visits, and the
  adverse dispositions with their hearing or explanation and the publication of violations.
  8 BUCs, 52 use cases, 11 entities, 10 state models, 6 timers, 12 labelled rules, and 236
  `//` citations of primary sources. It is in `samples.txt` (`check: ok`, `verify: tlc=ok`), its
  artifacts, `design-doc.md`, and `prd.md` are in `scripts/sample-artifacts.tsv`, and its README
  lists the soak findings. It states no temporal property: the one it was written with reads a
  421-state entity but runs on the whole model, which TLC did not finish.
- `check --max-states <n>` and `states --max-states <n>` set the joint-state cap of each
  target cone, which the `bounds:` line prints (#60). `check` also takes `--max-patterns`,
  as `states` does. A cone past the cap is still the `StateCapReached` error, and its
  message now names what the cone's reached states vary in: each variable with the values
  it reached out of its domain (`Int 0..5` for an Int column, `clock` for `now`), and the
  use cases that first reached the most states. A column of another entity in that list
  is one a guard pulled into the cone.

### Changed

- The incremental modeling stages put screens before data (#72). The interaction boundary
  stage is split in two: Screens (`screen`, `displays`, `navigates`, `field`) comes right
  after the BUC skeleton, and API and access boundary (`api`, `invokes`, `dto`, `system`,
  `permission`, `medium`, `has_permission`, `requires_*`) comes after data touchpoints,
  which now also adds `shows` and `maps_field`. The RDRA screen sits with the use case on
  the system boundary, nearest the actor, so what the actor sees is settled before the data
  it touches. `displays` stays with the screens, not with the data: it is what ties a use
  case to its screen. Stage numbers, slugs, and anchors move:

  | Before | After |
  |---|---|
  | 2 Data touchpoints (`data-touchpoints`) | 3 Data touchpoints (`data-touchpoints`) |
  | 3 Interaction boundary (`interaction-boundary`) | 2 Screens (`screens`) and 4 API and access boundary (`api-access-boundary`) |
  | 4 Entity structure | 5 Entity structure |
  | 5 Lifecycle | 6 Lifecycle |
  | 6A / 6B / 6C rules | 7A / 7B / 7C rules |

  The Screens gate asks that every use case `displays` a screen or be started by an event
  or timer; an `invokes` no longer stands in for a screen, so a BUC whose actor-performed
  use case has only an API now stops at stage 1 and is asked for the screen. The API and
  access boundary gate asks that every API a use case invokes belong to a `system`, and
  keeps the permission and medium review note; a BUC with no API passes it as it stands.
  Data touchpoints adds a review note for a displayed screen with no `shows`. In
  `rdra-ish next --format json`, the `slug` values stay the stable ids; `interaction-boundary`
  is gone, `screens` and `api-access-boundary` are new, every `stage` number from 2 on
  shifts, and a gap or note `kind` can now be `screen` or `api`. The `rdra-ish-write` step
  files are renumbered to match (`02-screens.md` to `07-rules.md`), and the
  `incremental-order` sample gains `step-2-screens` and renumbers the later steps.
- `diagram` without `-o` prints the Mermaid text to stdout instead of writing `./out.mmd`
  (#70). `-o <path>` still writes `<path>.mmd` and prints `wrote <path>.mmd`. A script that
  relied on the default `out.mmd` passes `-o out`.
- A quoted value of an Enum or a variation is `QuotedValue`, after #48's rule that a token
  is bare and a quoted string is free text: `chooses(Pay, Order.kind, "a")` recorded `a`, and
  `@default("open")` on an Enum or variation column loaded as `@default(open)`. The hint names
  the bare rewrite (`write chooses(Pay, Order.kind, a)`). Every other place a value is
  written (`transitions` endpoints, comparisons, `in` lists, `sets`, `Enum(...)`, `variation`
  values) already took no quoted value; `rule_audit` now covers the quoted forms. A quoted
  `@default` of an Int or String column is unchanged.
- A deadlock at the bound and a `NeverRuns` warning name the API when a use case creates the
  entity it needs a free slot for only through an API it invokes (#55): `deadlock at the
  bound: ... ResubmitDisclosure needs a free InventionDisclosure slot within the instance
  bound; ResubmitDisclosure creates InventionDisclosure via api 'DisclosureApi'`, and
  `... no free slot for an instance it creates; it creates InventionDisclosure via api
  'DisclosureApi'`; the `needs:` row ends in `(created via api 'DisclosureApi')`. A direct
  `creates` prints as before. `Blocked::NoInstance`, `Apart`, and `Crowded` now hold a
  `Need { entity, role, via_api }`, and a lifecycle `Param` carries the API it is created
  through.

- A use case a timer or an event starts, with no `performs(Actor, UseCase)`, is the system's
  work for the permission check too (#58): its own and its APIs' `requires_permission` no
  longer report `UseCasePermissionNoActor` or `ApiPermissionNoActor`, as for a use case
  exposed with `.caller(system)`. `actors_for_usecase` already left such a use case out of its
  BUC's actors. A started use case that an actor performs is checked as before, and the two
  hints now say to start a system-run use case with `triggers` instead of removing the
  requirement. patent-ops' timer-started use cases that wrote rows directly to avoid the
  warning go through the API the actors' use cases use: `DeemWithdrawnNoRequest` through
  `ApplicationDocketApi`, `FinalizeRejection` through `DecisionApi`, and
  `RollOverAnnuityYear`, `EnterGracePeriod`, `LapseUnpaidPatent`, and `ExpirePatentTerm`
  through `AnnuityApi`; their sequence, use case, and robustness diagrams show the API, and
  their `states` and `verify` results are unchanged.
- `performs(Actor, Buc)` keeps meaning the actor performs every use case of the BUC (#58).
  Taking part in a BUC is already written `performs(Actor, UseCase)` or
  `performs(Actor, Step)`, which `list --kind buc-roles` and the context diagram read, so no
  `participates` predicate is added. When a permission is missing only because of a
  `performs(Actor, Buc)`, `ActorPermissionMissing`'s hint now names the BUC and those forms:
  "`performs(Clerk, BucFees)` makes Clerk perform every use case of BucFees: to say Clerk
  only takes part in BucFees, write `performs(Clerk, UseCase)` for the use cases it runs or
  `performs(Clerk, Step)` for its steps instead".

- An `import` resolves by the `module` a loaded file declares, whatever the file is called
  (#56). `buc/buc_registration.rdra` with `module buc.registration` now answers
  `import buc.registration.{FeePaymentScreen}`; it was `IO error reading
  'buc/registration.rdra': module file not found in include paths`. A file not loaded yet is
  still found at `<a>/<b>.rdra` under the include paths, and the import graph (and its
  `CircularImport` check) follows module-resolved imports too. An import no loaded file
  declares is now one error, `UnknownImportModule`, instead of that IO error plus
  `UnknownImportModule`; its hint lists the loaded files that declare a similar module
  (`similar modules: buc.registration (buc/buc_registration.rdra)`). The CLI and the LSP share
  the resolver. patent-ops links three screens across BUCs with `navigates`
  (`DecisionScreen` to `FeePaymentScreen`, `ApplicationDocketScreen` to
  `ExaminationRequestScreen`, `FeePaymentScreen` to `RewardScreen`), which clears its three
  `screen-no-transition` infos.

- `lint` `condition-no-rdra-anchor` no longer flags a `holds` body that guards a use case
  (#57): one that `applies_to` a use case or an API, or that a `transitions` arm or a `chooses`
  value names in `.when`. A guard can be written only as a condition, so a guard that reads
  only `Bool` or nullable columns had to compare the state its use case already moves from to
  silence the warning. A body that guards nothing (none, or only `applies_to` a system) and
  names neither a variation column nor a lifecycle column is still flagged; the message says
  so and the hint names `applies_to`. patent-ops drops six such comparisons
  (`FirstFeesPaid`, `FirstFeesUnpaid`, `NotExtendedWithinTerm`, `YearUnpaid`, `KeptRight`,
  `InventorChecked`), each the only source state of the one use case its condition applies
  to; its `check`, `states`, and `verify` results are unchanged, and its condition diagrams
  lose the link those comparisons drew to the state model.

- The default joint-state cap of a cone in `check`, `states`, and the generators is 65,536,
  up from 4,096 (#60). It no longer follows `--max-patterns` (it was 16 states per allowed
  pattern). A capped cone of 65,536 states costs about 0.1 s and 24 MB on a release build
  and about 1.4 s on a debug build; no sample's cone comes near it, so their results and
  run times are unchanged. `derive_states`, `derive_states_built`, `BoundsLine::states`,
  `collect_entity_state_variants`, and the state-pattern and TypeScript emitters take a
  `Caps` (`Caps::STATES` by default) where they took a pattern count.
- Several `transitions` of one use case from one state on one column are a choice: one run
  of the use case takes exactly one arm, the actor's decision (#52). Every arm carries the
  use case's other effects (`sets`, creations, deletions, other transitions); two choice
  columns in one use case choose independently, one run per pair of arms; a condition
  applied to the use case guards every arm; `after(UC).assert` must hold after every arm.
  `states`, `check`, the TLA+ export (`\E v \in (CASE col = a -> {b, c} ...)`), and `verify`
  share this semantics, and `engine_equivalence` compares a fixture with choices against TLC.
  A model with such arms now reaches the states the later arms enter, and may report new
  findings there; no sample had them, so every sample's `states` and `verify` counts are
  unchanged. A `sets` stays deterministic: two values for one column in one use case are
  still the `ConflictingEffects` error, whose message now says to keep one value or split
  the use case.
- The sequence diagram names each transitioned column's source states once
  (`alt Patent.status in (存続中, 追納期間中)` where it printed `Patent.status == 存続中 and
  Patent.status == 追納期間中`) and draws a choice as an `alt` with one branch per arm after
  the write.
- `samples/patent-ops` folds `DecideToKeepSecret` and `DecideNotToFile` into one
  `DecideNotToFile` (出願しない決定) whose two arms from `searched` are the committee's choice,
  and, once `.when` guards one arm (#62), folds `DecideToFile` in too as `DecideFiling`.
- `samples/patent-ops` restores two guards it dropped to fit the old 4,096-state cap of `check`
  (#60): an office action arrives only for an application under examination
  (`UnderExamination`, citing 特許法48条の2), and the docket of an answered office action closes
  only after the examination is over (`ExaminationOver`). Its largest cone at 1 instance grows
  from 131 to 1,034 states. At the 2 instances of `verify` the `OfficeAction` cone reaches
  4,818,909 states: TLC checks its invariants (`verify` takes about 75 s, up from 4 s), but the
  BFS cap of 2,097,152 leaves its four stuck-state checks under `not checked:`, and
  `engine_equivalence` compares three of its cones at 1 instance only (`VERIFY_FALLBACK`). The
  third guard, that the filing checks the drafts were approved, stays out: it needs a required
  link from the application to its drafting order, which takes three cones past 10,000,000
  states at 2 instances. The README names both as known verification gaps (#51, condition 5).

### Deprecated

- `sets(UseCase, Entity, comparison, true | false)`, the four-argument `sets`, is deprecated
  since 0.5.0 and removed in 2.0.0 (#51, condition 4); write `drives(...)` with the same
  arguments. It still loads as `drives` with a `DeprecatedSyntax` warning, `lint` lists it as
  `deprecated-syntax`, and `rdra-ish fmt --write` rewrites it. Every four-argument `sets` in
  the docs, skills, and tests is rewritten; no sample used it.

### Fixed

- The `Runs_<UseCase>` warning of a use case that creates a child under an `.optional()`
  `relate` and guards on the parent it does not write says why (#70): `note: it creates App
  without writing Permit, so the App is unlinked and no Permit is its parent for a guard to
  read; write Permit in the use case (`updates`), or drop `.optional()` from the `relate``.
  Before, it said only that its guards hold in no reachable state. `StateDiag::NeverRuns`
  gains an `unlinked` field.
- `diagram --kind state --entity <Entity>` names only that entity's columns in the
  `omitted:` and `set-only:` header lines (#70). It also listed the Enum columns of every
  other entity in scope, such as `%% set-only: Disposition.publication` under
  `--entity BusinessPermit`.
- `list --kind rule` prints the values of `!=`, `in (...)`, and `not in (...)` in a
  `forbidden`, `invariant`, `required`, or `exclusive` by their labels, as it did for `==`
  and as `list --kind condition` does (#70). `Doc.status in (paid, shipped)` reads
  `Doc.status in (支払済, 発送済)` when the Enum carries those labels.
- `states` `VIA` no longer lists a use case that only touches the entity without changing it
  on every row (#70). A use case that reaches the entity through an API that `updates` it,
  with no `transitions`, `sets`, or `chooses` on its columns, led into no row, yet each row
  after the entity's first carried it. `VIA` lists the use cases that create or move
  an instance into a row, those that lead into a row that reaches it, and those that delete
  an instance in it. `samples/personal-info` `CreditCard` loses `SetPrimaryCard`, for
  example, and the JSON `via` and the CSV `via` column follow.
- A multi-entity `==` or `!=` between two Int-typed or two Date-typed columns
  (`Permit.fee != Application.fee`) left both columns without an Int variable, so the rule
  was `UnsupportedComparison`. They are Int variables now, as in an order comparison.

- A use case with several `transitions` from one state explored only the first arm and
  dropped the others with no diagnostic; their targets were reported as
  `UnreachableEnumVariant`, and a trace step cited the first arm whatever arm it took (#52).
  An audit test checks that every declared arm is an edge of the derived system, or that a
  `NeverRuns`, `UnreachableEnumVariant` of its source, or a `Deadlock` naming it says why.
- A `.has(...)` or `.none(...)` read the wrong instance when its related entity was already
  bound (#63). On a `holds` guard applied to a use case that creates the related entity, it
  read the instance being created instead of the existing rows: `states` and `check` let the
  use case run whatever the existing rows were, and the TLA+ export read the new instance
  primed (`Parcel_state'[p1]`) before the action defined it, so TLC stopped with
  `Parcel_state is undefined` at two instances. On a use case that updates the related entity
  it read that one row, and on an `invariant` whose related entity is the anchor's own it read
  the anchor row. The quantifier now always reads its own instance, over the rows present
  before the step. A model may report findings it did not before (a rule on the anchor's own
  entity), or lose findings that came from the guard not holding.
- `verify` missed a rule that fails only in states (or steps) where another rule of the same
  cone also fails (#64). TLC with `-continue` reports one violated check per state or step,
  the first in the cfg, so the later rule never printed, while `check` and `states` (BFS)
  reported both. A cone run with a violation is now rerun without the checks it has
  reported until a rerun reports no new one, and the engine-equivalence test does the same.
  `verify` may now report findings it did not before.
- The `rdra`, `usecase`, and `condition` diagrams lost the condition-to-use-case edge of a
  `.when(Cond)` guard on a `transitions` arm or a `chooses` value (#62, #65 made it a derived
  relation, which the diagrams skipped). It is drawn again as a dashed `-.->|when|` edge,
  followed by `%% derived <condition> -> <usecase> via transitions <Entity>.<column>
  <from>-><to>` or `via chooses <Entity>.<column> <value>`. A condition to a use case is now
  two relations in those diagrams (`EDGE_RELATIONS`), so `applies_to` to a use case is
  labelled there too, `-.->|applies_to|`; to an API or a system it stays unlabelled.
- Small display faults (#61). A violated rule printed each comparison with `=true` after it,
  `then status != oa=true AND status != granted=true`; it now prints it as written, `then
  status != oa AND status != granted`, and the same for `<`, `in (...)`, and `not in (...)`
  (#53), in a single-entity and a multi-entity rule. The robustness diagram labelled each
  timer-to-control edge with the timer's own label, which the timer node already shows; a
  timer to a control is one relation (`triggers`), so per #43 the edge is unlabelled,
  `timer__X -.-> usecase__Y`. The state diagram listed an Enum column that a `sets` writes,
  such as patent-ops' `OfficeAction.kind` and `Patent.last_payment`, as `omitted: ... (Enum
  without transitions)`, though `check` and `states` read it as a state axis. It is named on
  its own header line, `%% set-only: <Entity>.<column> ...`, and `omitted:` keeps the Enum
  columns nothing moves. It is still no state model: without `transitions` it has no states
  to draw, as `information` and `condition` already treat it.
- The LSP treats a reference in a chain-method argument like one in a predicate argument
  (#66). With the cursor on `Cond` in `.when(Cond)` (#62, #65), on a use case in
  `.via(UseCase)` or `.fair(UseCase, ...)` (#59), on an entity in `.via(Junction)` or
  `.along(E, ...)`, on a medium in `.by(medium)`, or on the entity of an `Entity.column` in
  `.when`, `.then`, `.has`, `.none`, or `.assert`, rename, references, go-to-definition,
  hover, and linked editing found no symbol. A property was not read at all, so renaming a
  use case or an entity left its name unchanged in `.fair(...)` and in the property's
  formula. The features now read one list of the references in a file, and a test renames
  a symbol inside every chain method that `predicate_chain_methods` lists, so a new method
  must be added to the test before it passes.

## v0.4.0 - 2026-11-01

Upgrading from v0.3.0: run `python3 scripts/migrate-syntax-v2.py <model dir>` to rewrite
`transitions`, `maps_field`, `writes`, and `acceptance criteria`, then rewrite each top-level
`when(...)` rule as the load error says. A model whose rules loaded but recorded nothing now
fails to load. `writes` is gone, so `csv --kind matrix` and `api-matrix` print `U` where they
printed `W`, the business-inputs operation `write` is `update`, and `--edge-kind writes` is
an error. The Mermaid flowchart contract changed: each kind has its own node shape, and an
edge whose ends imply its relation has no label. `engine_equivalence` still compares 8
clinic-ops cones with TLC at 1 instance only (`VERIFY_FALLBACK`).

### Breaking

- A rule over related rows has one form, `invariant(...).when(...).none/has(...)`; a
  guard stays `holds(C, ...).none/has(...)`. The top-level `when(...)` rule is removed:
  `when(Entity, conditions...).none(...)` and `when(Entity.col == v).none(...)` fail to
  load with a message that gives the rewrite, as in ``write
  `invariant(Cert).when(status == revoked).none(Assign.status == active)` ``. Its entity
  is the leading argument, or else the entity of the first qualified column, and the rest
  of the chain, `.label(...)` included, carries over as written. The rewritten rule
  checks the same thing under the same name, `Quantifier_<Entity>_<n>`, as before. The
  `quantifier_none` and `quantifier_guard` samples, the docs, and the skills use the
  `invariant` form (#47).
- Every predicate takes the use case first and names a column as `Entity.column`.
  `transitions(UseCase, Entity.column, from -> to)` replaces `transitions(Entity.column,
  UseCase, from -> to)`, and `maps_field(Field, Entity.column)` replaces the three-argument
  `maps_field(Field, Entity, "column")` (also written `column` or `Entity.column`). The old
  forms fail to parse with a message that gives the rewritten line, for example ``
  `transitions` takes the use case first; write `transitions(Pay, Order.status, pending ->
  paid)` ``. `maps_field` now reports an unknown entity (`UndefinedSymbol`), an unknown
  column (`UnknownColumn`), and an argument that is not `Entity.column` (`TypeMismatch`).
  `scripts/migrate-syntax-v2.py <workspace>` rewrites `.rdra` files in place, prints each
  edit, and changes nothing on a second run; it takes any single file too, such as a
  Markdown page, for review. The samples, test fixtures, docs, and skills are migrated
  (#48).
- `writes(UseCase | Api, Entity)` is removed. It was `updates` under another name, so it
  fails to parse with ``use `updates` (or `creates`/`deletes`)``, and
  `scripts/migrate-syntax-v2.py` rewrites it to `updates`. Two outputs differ for a model
  that used it: the `csv --kind matrix` and `api-matrix` cell reads `U` where it read `W`,
  and the `operation` column of `csv --kind business-inputs` and `list --kind
  business-inputs` reads `update` where it read `write`. `--edge-kind writes` is an
  unknown edge kind. State derivation, transactions, and
  the publication checks already treated it as `updates` (#48).
- The two-word `acceptance criteria "..."` (and `acceptance "..."`) is removed. Write
  `acceptance_criteria "..."`; the old spelling fails to parse with a message that gives
  the rewrite, and `rdra-ish fmt` writes `acceptance_criteria` (#48).

- The left-hand column's type decides what a bare right-hand side of `col == x` or
  `col != x` is. On an Enum, variation, or Bool column, `x` is a value of the column, also
  when the entity has a column named `x`; before, such an `x` was the column. A rule that
  compared two of these columns bare now reads the value when `x` is one, and otherwise
  fails to load with `ComparisonRhsNotAValue`, whose hint gives the qualified spelling:
  ``'previous' is not a value of column 'status' (pending, paid, cancelled), and a bare
  right-hand side of 'status' is a value`` / ``hint: write `Order.previous` to compare
  columns``. Write the column as `Entity.column`. On a single-entity rule the bare form
  already failed `check` and `states` with `UnsupportedComparison`; on a cross-entity rule
  over two columns with one set of values it compared the columns (#46).

### Added

- `navigates(Screen, Screen)` declares a screen transition, with an optional
  `.via(UseCase)` naming the use case whose operation causes it. A `.via` that names
  anything but one use case, or a second `.via`, is `NavigatesViaRequiresUseCase`.
  `diagram --kind screen` draws the 画面遷移図: every screen, a solid edge per `navigates`
  labelled with its use case (`%% via` gives the use case id), and where no `navigates` is
  declared a dashed `業務上の順番` edge, derived from two consecutive steps of a flow
  (`precedes`, `branches`, `excepts`) in one actor's lane whose use cases display the two
  screens, with a `%% derived ... via <flow and step ids>` line. `--buc` keeps the screens
  the BUC's use cases display. `lint` adds the `info` rule `screen-no-transition` for a
  screen neither kind of edge reaches. `rdra_ish_core::screen_navigations` and
  `business_order` give the derivation. clinic-ops declares the transitions of 受付, 診察,
  and 会計, ships `out/screen.mmd` and one per BUC under `out/buc/`, and the design document
  shows each BUC's screen transitions (#44).
- `list --kind transition` writes the state-transition table behind `diagram --kind
  state`: one row per transition with `entity, column, from, to, usecase, guards, actors,
  site`. `from` and `to` read `label (value)` and `usecase` reads `label (id)`. `guards` are
  the labels of the conditions whose `holds` body guards the use case (what the diagram
  writes in `[...]`), and `actors` are the use case's performers, where an event- or
  timer-started use case takes only its explicit `performs`. `site` is the `transitions`
  line as `file:line`. Every `--format` works. `rdra_ish_core::state_models`,
  `state_label`, and `rdra_ish_core::analysis::guard_labels` give the derivation the diagram
  and the table share (#42).
- `list --kind buc-roles` writes who takes part in which BUC: a row per actor, then per
  external system, and a column per BUC, ordered by business. A cell says `担当` (an actor
  performs the BUC, a use case in it, or a step in it), `イベント送信` (an external system
  raises an event that starts the BUC's use cases), or `イベント受信` (an event a use case in
  the BUC raises reaches the external system), joined with `、` when both. Every `--format`
  works, so `--format markdown` pastes into a PRD or a design document. The columns are
  the BUC labels, so the table has as many columns as the model has BUCs.
  `rdra_ish_core::buc_handoffs` and `buc_participants` give the derivation.
- `diagram --kind state --entity <Entity>` (repeatable, not with `--buc`) draws only that
  entity's state models, every lifecycle column, whole. An unknown entity, or one with no
  state model, is an error. clinic-ops ships one per entity under `out/state/`.
- A full state diagram colours its states by what the lifecycle check found: `classDef`s
  `stateunreachable` (a declared value no reachable state holds, drawn with no transition),
  `statedeadlock` (a state an instance cannot leave although it has a way out), and
  `stateend` (no transition out), with a `%% legend:` comment. `diagram --kind state` no
  longer refuses a model whose only errors are deadlocks; it colours them.
- `list --kind actor` and `list --kind buc` write each element's description, `buc` also
  the business it belongs to and the use cases it contains (`id,label,business,usecases,description`).
  `list --kind usecase` ends with `bucs,screens,apis`, and `list --kind requirement` gains a
  `bucs` column with the BUCs the requirement motivates, so a document generator reads the
  model through the CLI instead of parsing `.rdra`.
- A `goal` element holds the outcome the requirements serve: its description states the
  problem, and `metric`, `baseline`, `target`, and `window` say how it is measured.
  `achieves(Requirement, Goal)` links a requirement to a goal; a non-goal is a requirement
  with `status "out-of-scope"` and a `rationale`. `list --kind goal` writes the goals with
  the requirements that achieve each, `list --kind requirement` gains a `goals` column, and
  `diagram --kind requirement` draws goals with their `achieves` edges. `lint` adds the
  `info` rules `requirement-no-goal` and `goal-unmeasured`. clinic-ops has three goals,
  with numbers the model assumes. `goal` is now a keyword, so it no longer names an element;
  a column may still be named `goal`, and so may one named `condition` or `variation`, and
  a comparison now reads such a column (`goal == present`, `CarePlan.goal == present`),
  which was a syntax error for `condition` and `variation` before.
- `diagram --kind sequence --usecase <id>` writes a Mermaid `sequenceDiagram` of one use case,
  the second step of the design reading after the robustness diagram: the starter, the screen, each control, the guards as an `alt` with a `rejected` branch, each
  transaction's reads and writes with the `sets` values and the lifecycle move, and the
  events it raises. It needs exactly one `--usecase`. The kind is no longer in the removed
  list; clinic-ops ships three under `out/uc/`.
- `diagram --kind robustness` draws the use cases as an ICONIX robustness diagram:
  actors reach the screens they use, a screen reaches the API its use case invokes (or the
  use case itself when it has no API or writes entities directly), and a control reaches
  the entities it reads and writes, labelled `CRUD`. External systems and timers sit at the
  boundary, and an event that joins two controls is a dotted edge. The grammar leaves out
  an actor-to-control and a boundary-to-entity edge, so a BUC reads in about half the
  edges of its UC複合図 (clinic-ops billing: 46 against 88). `--buc` and `--usecase` scope it; clinic-ops ships one
  per BUC.
- A business rule can carry a label: `.label("...")` on `forbidden`, `invariant` (with
  `.then(...)` or `.has/.none(...)`), `required`, `exclusive`, and `after(...).assert(...)`, as in
  `invariant(Appointment).when(status == checkedin).then(checked_in_at == present).label("受付済の予約には受付時刻がある")`.
  `list --kind rule` writes every rule and condition as one table with the columns
  `kind,label,entities,rule,applies_to,site` (any `--format`), so a PdM can take the
  acceptance criteria of a PRD from the model. A violation of a labelled rule opens with
  `「label」: `, and the invariant names, which the TLA+ export shares, do not change.
  `lint` adds the `info` rule `rule-unlabeled` for a rule without a label. clinic-ops
  labels each of its rules in Japanese.
- `list --format markdown`, `lint --format markdown`, and `csv --format markdown` write a
  Markdown table with the CSV columns, so a requirement list, a CRUD matrix, or a permission
  audit pastes into a design doc or a PRD as it is. `rdra_ish_emit::markdown` renders the
  table for every command.
- A condition guard may say that no related row, or some related row, matches:
  `holds(NoActiveAssignment, ClientCertificate.status == active).none(TerminalCertAssignment.status == active)`
  allows `Revoke` only while no linked assignment is active, and `.has(...)` requires one.
  The rows are the ones a `relate` links to the guarded instance; without a relate the
  guard ranges over every row. `states`, `check`, and the TLA+ export read it as a universal
  or existential over the related ids, `fmt` keeps the chain, and a lifecycle column named
  in it links the condition to the state model. See `quantifier_guard.rdra`.
- An inline `Enum` value may carry a label, as a variation value does:
  `status: Enum(pending "受付前", paid "支払済")`. The UC複合図, the state diagram, and the
  RDRA diagram draw a state by its label, `states --format table` adds a `labels:` line
  under `axes:`, and `fmt` keeps the labels. clinic-ops labels every lifecycle value in
  Japanese.
- An owner model publishes what other models may reference. `exposes(System, UseCase)`
  with `.caller(actor | system | any)`, `exposes(System, Event)`, and
  `exposes(System, Entity).append() | .upsert()` publish a use case, an event, and a
  projection entity; `shares(System, Entity)` lets other models name an internal entity at
  the cost of a `SharedInternalEntity` warning. `check` enforces the published-entity
  rules (`PublishedEntityMutated`, `PublishedEntityReadByOwner`,
  `PublishedEntityKeysInternal`, `PublishedNotInSourceTx`, `InternalEntityExposed`,
  `PublishedEntityNeverWritten`, `DuplicateExposure`, `SharedEntityAlsoExposed`,
  `ExposesChainInvalid`, `ExposesCallerInvalid`). A direct write to a published entity
  joins the use case's one internal transaction, so the lifecycle and the TLA+ export run
  it in the same action as the internal change (one `Login`, not `Login_1` and `Login_2`),
  and `SeparateTxInferred` never fires for it; a write through an API stays in that API's
  transaction. Published entities leave the owner's
  lifecycle, and a use case exposed with `.caller(system)` or `.caller(any)` is not
  reported as having no actor. `rdra-ish list <model> --kind surface` prints the
  published surface as canonical text, sorted by kind and id. A model without `exposes`
  keeps every output unchanged; the one grammar change is that `.caller(actor)` and
  `.caller(system)` parse, where `actor` and `system` were keywords only.
- One project loader in `rdra-ish-core` (`load_project`) now serves the CLI, the LSP, and the
  tests. It looks for an `rdra.toml` manifest above the inputs and parses it, but nothing
  reads it yet. The CLI loads and prints a model without a manifest as before. A malformed
  manifest, or one without `[model] name`, fails the load and names the file and line,
  whatever the order of the inputs. The search is documented under "Project manifest" in the
  CLI reference.
- `rdra-ish next` places each BUC on the incremental modeling stage map and prints the
  next stage: its main question, the elements that fail its gate (such as a use case with
  no CRUD or an entity with only its key), the review notes, the `check` findings the
  stage owns, the questions to ask, and the commands that review it. A gate clause is
  blocking or advisory: a failed advisory clause (a use case with no touchpoint once
  another has one, a use case with no `requires_*` when the model declares permissions or
  media, a touched entity with no `relate`, a `sets` on a non-Enum column) is a note that
  never holds a BUC back. A stage whose clauses pass but which owns a `check` error (a
  `Witness_`, `Deadlock_`, or violated rule on an entity the BUC touches) reads `[!]`
  blocked. The rule stages 6A to 6C are optional: a BUC with a lifecycle is complete, and
  `next` offers them. An event-started BUC (`triggers(Event, Buc)`, no use case yet)
  passes the skeleton and is asked for its entry use case; a rule counts for a BUC through
  a column the BUC moves and belongs to exactly one tier, so a comparison `forbidden` is an
  advanced rule only. `--buc` narrows it to some BUCs; `--format json` adds a stable `slug`
  per stage, `required_complete`, `verdict`, `notes`, and `findings`, and `next.kind` is
  `required` or `optional`.
- The Stage Map, the "Ask the user" and "Validation" of each stage in
  `docs/incremental-modeling.md`, the gate table of the CLI reference, and the stage
  tables of the `rdra-ish-write`, `rdra-ish-buc-create`, and `rdra-ish-buc-update` skills
  are generated from one stage spec in `rdra-ish-core`; a test fails when they drift, and
  `RDRA_ISH_BLESS=1 cargo test -p rdra-ish-core stage_docs` rewrites them. The texts are
  reconciled: `belongs` is a stage-0 addition, stage 5 asks whether the Enum variants are
  the states to see, and lifecycle and rule review runs `check src/` and
  `states src/ --entity <Entity>` instead of `states --buc`, which leaves `Witness_`,
  `Deadlock_`, and `Runs_` undecided. The 6C section is titled "Advanced Rules"
  (anchor `#stage-6c-advanced-rules`).

### Changed

- `rdra-ish fmt` keeps a model's layout readable. Consecutive `import` lines print as one
  block. A declaration whose only clause is `description` stays on one line while the line
  is at most 120 display columns wide (a wide character such as Japanese counts as two);
  a longer one, or one with several clauses, puts each clause on its own indented line. A
  declaration that prints on several lines is set off by blank lines; between one-line
  declarations, `fmt` keeps the author's blank lines (several become one) and adds none,
  so a `//` citation above a declaration stays attached to it. Ids are not aligned. An
  entity or variation whose only clause is `description` no longer prints the clause
  inside its braces, where it failed to parse. Every `.rdra` file under `samples/` and
  `skills/` is formatted, and `scripts/check-sample-artifacts.sh`, which CI runs, fails
  when `fmt --check samples skills` does. Line sites quoted from
  `cross_order_payment_split.rdra` move down by one (#50).
- Every construct that takes a comparison reads its bare right-hand side by one rule:
  `forbidden`, `required`, `exclusive`, `invariant` (`.when`, `.then`, `.has`, `.none`),
  `holds`, `after(...).assert(...)`, `sets`, `property`, `@check`, and `@derived`. A
  value of an Enum, variation, or Bool column, and `null` or `present` of a `@null`
  column, is a value; any other bare name is a column. So `forbidden(E, status == paid,
  paid == false)` reads the value `paid`, where `check` failed with ``status == paid:
  status is not an Int-typed column``, and Enum values need no prefix to keep apart from
  column names. `!=` takes a value too (`forbidden(Order, status != paid)`, `holds(C,
  Order.status != cancelled)`), and two Enum, variation, or Bool columns of one entity
  compare with `==` / `!=` as `status != Order.previous` (#46).
- The clinic-ops sample names its Enum values without the per-column prefix they needed to
  stay apart from column names: `apptawaitingcalc` is `awaiting_calc`, `stmtpaid` is
  `paid`, and `payposted` and `chargeposted` are both `posted`, a value being unique only
  within its column. The 61 values of 13 columns keep their labels, so `check` and
  `states` print the same, and the diagrams, `design-doc.md`, and `prd.md` change only
  where they print a value id (#46).

- The flowchart encoding contract changes: a node is no longer always `id["label"]`, and an
  edge no longer always `|label|`. Every flowchart kind (`rdra`, `context`, `requirement`,
  `business`, `usecase`, `robustness`, `event`, `flow`, `information`, `condition`,
  `screen`) draws a node as `<kind>__<Id><open>"<label>"<close>:::<kind>` in its kind's
  Mermaid shape, so kinds tell apart in black and white and without colour vision:
  `actor` `([..])`, `usecase` `(..)`, `api` `{{..}}`, `entity` `[(..)]`, `extsystem` and
  `statemodel` `[[..]]`, `event` `>..]`, `timer` `((..))`, `state` `(((..)))`,
  `condition` `{..}`, `goal` `[/../]`, `requirement` `[/..\]`, `nfr`, `constraint`, and
  `variation` `[\..\]`, `business` `[\../]`, and every other kind (`screen`, `buc`,
  `step`, `system`, `value`, ...) `[..]` as before. Ids, `:::<kind>` tags, quoting, and
  `classDef` colours are unchanged, and a subgraph keeps `subgraph <id>["<label>"]`. An
  edge is written `<from> <arrow> <to>`, without a label, when in that kind the two end
  kinds have exactly one relation and the label would only name it; the relation is read
  back from the ends. Gone are `performs`, `invokes`, `displays`, `shows`, `navigates`
  (in `rdra`), `raises`, `triggers`, `contains`, `covers`, `belongs`, `compensates`,
  `motivates`, `achieves`, `applies_to`, `combines`, `owns`, `qualifies`, `constrains`,
  `maps_field`, `has_permission`, `requires_permission`, `requires_medium`, and `decides`
  in `rdra` and wherever else they are drawn; `achieved_by` in `requirement`; `uses`
  (actor to screen) in `robustness`; every label of `event` and `condition`; `statemodel`
  in `information`. Labels that say more than the ends stay: the CRUD verb, `request` / `response` / `error_response`, a step order, a
  relate's cardinality, a transition, the robustness use case, CRUD letters, and events,
  the BUCs of a `context` actor edge (a `performs`, its relation still read from the
  ends), the events of a `context` external-system edge and a `business` hand-off,
  `variation <column>`, and every `screen` edge. `rdra_ish_emit::NODE_SHAPES` /
  `node_shape` and `EDGE_RELATIONS` / `implied_relation` are the tables, and the contract
  test reads both. clinic-ops: the `rdra` diagram of `BucClinicalEncounter` keeps 159 of
  its 584 labels, the robustness diagram 19 of 24, the context diagram all 21, and the
  UC複合図 of the BUC 17 of 35 (#43).
- Every metadata clause takes a quoted string or a bare token: `description`, the
  requirement, ADR, goal, NFR, and constraint clauses, the API and field clauses, and the
  use case review clauses. `priority must`, `target 300`, and `slo 99.9` parse, where
  `priority` and `target` took only a string before. `rdra-ish fmt` writes each value the
  way it was written, so `metric "p95"` is no longer rewritten to `metric p95`. A clause
  may follow any other clause, and `source` on a `field` is the field's source whether
  quoted or bare (a quoted `source "..."` on a field was read as requirement metadata)
  (#48).
- `diagram --kind business` is a business flow. Each business is a subgraph holding its BUCs,
  and an edge `BUC -->|<event label>| BUC` is a hand-off: a use case contained in the first
  BUC raises an event that triggers a use case contained in the second, or
  `triggers(Event, Buc)`. It is drawn once per `(from, to, event)` with a `%% derived ... via
  event__<Id>` line, and a hand-off with no event has no edge. The actor and external-system
  nodes and the `performs`, `raises`, and `triggers` edges are gone (see `list --kind
  buc-roles`), as are the layer subgraphs and the `%% layers:` header line. A BUC of several
  businesses sits in the first by id with `belongs` edges to the others. `--buc` draws the
  named BUCs, the BUCs they hand off to or from, and their businesses. clinic-ops: 31 nodes (with the businesses) and
  37 edges become 14 nodes and 8 hand-offs.
- `diagram --kind requirement` is a value tree, left to right: goals, the requirements that
  achieve them, and the BUCs those requirements motivate. A goal's label adds a second line
  with its metric and `baseline → target`. A requirement with `status "out-of-scope"` sits in
  `subgraph group_out_of_scope["やらないこと"]` with dashed edges. `achieves` is drawn from the
  goal as `achieved_by`, with a `%% derived` line, so the goal sits left of the requirement.
  NFRs, constraints, qualities, entities, and systems leave the diagram, and the `omitted:`
  line names them; `list --kind nfr` and `list --kind constraint` give the tables. The
  `%% layers:` header line is gone. `--buc` keeps the BUCs, the requirements that motivate
  them, and their goals. clinic-ops: 39 nodes and 36 edges become 27 and 27.
- `diagram --kind context` draws the system boundary and nothing else. An actor has one
  edge to the system, labelled with the labels of the BUCs it performs (`、`-joined), where
  it had a `performs` edge. An external system has one edge per direction, labelled with the
  labels of the events exchanged, where it had one edge per event labelled `raises <Id>` or
  `triggers <Id>`. The RDRA layer subgraphs and the `%% layers:` header line are gone. Node
  ids, `%% kinds:`, `%% omitted:`, `%% derived` (now `via` the BUC or event ids behind the
  label), and the synthesized `system__Model` stay. clinic-ops: 28 edges become 21.
- `diagram --kind state` without a scope is an index: each state model is a composite state
  holding one note, `<n> 状態 / <m> 遷移 (--entity <Entity>)`, with no states or transitions
  and so no `usecase` kind in the header. Read a model with `--entity`, or `--buc`, which
  still draws full models.
- `samples/clinic-ops/design-sample.md`, which followed RDRA's four layers, is replaced by
  two documents for two readers. `design-doc.md` (engineers) reads structure, processing,
  then specification: the system context, a robustness diagram, use case table, and folded
  sequence diagrams per BUC, then a state diagram per entity, data, rules, CRUD and
  permissions, non-functional requirements, and events, with the UC複合図 in an appendix.
  `prd.md` (PdM) holds the goals and metrics, the users and the BUCs each takes part in
  (`list --kind buc-roles`), the requirement value tree and the requirements by goal with
  the BUCs they motivate, the non-goals, the business flow and each BUC's flow, and the
  rules. `scripts/gen-clinic-ops-docs.py` builds both from `list`/`csv` output and the
  committed diagrams, so it no longer parses `.rdra`; clinic-ops commits a sequence diagram
  for each of its 96 use cases, its whole ER diagram, the Prescription state table, and its
  AsyncAPI export.
- A rule or a condition body shows an `Enum` or variation value by its label when it has
  one: `ClinicalOrder.status == オーダ済` (was `== clordered`). This holds for the `rule`
  column of `list --kind rule`, the `body` column of `list --kind condition`, the guards of
  `diagram --kind sequence`, and the `%% holds` lines of `diagram --kind condition`. A
  sequence diagram's state guard now names the entity by id (`Appointment.status == 会計待ち`,
  was `予約・受診.status == 会計待ち`), as the other guards do.
- `diagram --kind state` labels each transition with the use case's label, then `[...]`
  with the label of each condition whose `holds` body guards it: `採血 [医師が確定したオーダ]`
  (was `usecase__CollectSpecimen`). The use case's id moves to a `%% transition <id>` line
  before the arrow, so a tool that read the id from the label reads that line instead.
  `rdra_ish_core::analysis::applied_bodies` gives the guarding bodies to the lifecycle and
  the diagram alike.
- Every flowchart diagram gives each RDRA element family its own light fill in its
  `classDef` lines (was `stroke:#333` for every kind), so actors, BUCs, use cases,
  screens, events, information, and conditions tell apart when Mermaid renders the `.mmd`
  as it is. Node shapes, edge styles, and the `:::<kind>` tags do not change, and the
  colour carries no meaning beyond the tag.
- The UC複合図 (`diagram --kind usecase`) no longer draws an actor of a BUC or a step
  performing a use case that an event or a timer starts. Such a use case is the system's
  work, and only a `performs(Actor, UseCase)` draws who performs it. In clinic-ops,
  `CreateCharge` loses the レセプト担当 it never had, the received eligibility results lose
  the receptionist of their step, and the kiosk settlement, which the payment gateway's
  event records, keeps the patient only in its flow lane. In ec-site, `Capture` loses the
  customer of its BUC.
- `csv --kind actor-permission-audit`, `csv --kind screen-constraints`, and the permission
  diagnostics follow the same rule: a use case an event or a timer starts is performed only
  by the actors `performs(Actor, UseCase)` names, not by every actor of its BUC. The rule
  lives in `rdra_ish_core::triggered_use_cases`, which the UC複合図 also reads. A model in
  which a person does the work of such a use case declares that performer; clinic-ops now
  names the レセプト担当 on the monthly claim, resend, and result use cases.
- The LSP loads a workspace through the CLI's rules, so the editor now agrees with `check`.
  It no longer adds the workspace roots as include paths, so a deep import such as
  `import m.a.shared` from `m/a/b/c.rdra` reports "module file not found in include paths"
  where it reported nothing before. It now also loads `.rdra` files reached through
  symlinks, so opening `samples/formal-verification` as a workspace shows the
  duplicate-definition errors the CLI already shows. A malformed `rdra.toml` clears the
  workspace's diagnostics and shows an error that names the file and line.
- The `lint` `stage-readiness` row gives the stage of each BUC and the stage after it,
  from the same gates as `next` (`BucOrder stage 2 (next 3)`; `required stages complete`
  once stage 5 is met), instead of which of six layers have any element. `coverage-score`
  calls those six layers and still subtracts 8 for each one with no element; its message
  says `missing layers` (was `missing stages`).
- `lint` reports `buc-empty` as `info`, not `warning`, on a BUC that has not passed the
  skeleton stage, with `expected before stage 1 BUC skeleton` in its hint; the LSP no
  longer publishes it, so a correct stage-0 model has no editor warning.

### Fixed

- A `sets` that records no effect is `SetsNoEffect`, an error. `sets(UC, E, status != paid)`,
  `sets(UC, E, total < 3)` without `true` or `false`, a fourth argument other than `true` or
  `false`, and a `sets` with no comparison all loaded without a diagnostic and changed
  nothing, so the use case they describe left the column as it was.
- `@check` and `@derived` take an Enum value: `@check(status == open or closed_at ==
  present)` loads, where `open` was read as a column and failed with `right-hand side
  column 'open' not found`. A `property` atom `Entity.qty == limit` over two Int columns
  compares the columns; before, `==` read any bare name as a value, and the atom compared
  `qty` with no value at all and was false in every state (#46).

- `check` and `verify` evaluate `after(UC).assert(...)` on a step that writes only entities
  the assertion does not read. The assertion's cone kept the use case but dropped what it
  writes, so in the cone the step changed nothing, and neither the BFS nor TLC's
  `[][A]_vars` checks a step that changes nothing: the assertion passed without being
  read. The TLA+ cone also named a param the cone had removed (`p1'`), so TLC stopped with
  a semantic error. Such a model can now fail where it passed. A violation now also shows
  a parent the assertion reads.
- A multi-entity `forbidden` or `invariant` whose `.along(...)` path passes through entities
  the rule does not name, as in `forbidden(AccessSession, ClientCertificate, ...)
  .along(AccessSession, Terminal, TerminalCertAssignment, ClientCertificate)`, is checked
  over the pairs that some middle instances connect. It was `UnbindableScope` ("names an
  entity outside the scope"), although the language reference already allowed a junction
  entity there.
- A rule that the model records nothing for is an error. Such rules loaded without a
  diagnostic, were never checked, and were missing from `list --kind rule`, so a model with
  one now fails to load where it passed (#45):
  - `invariant(Order).when(status == paid)`, with neither `.then(...)` nor
    `.has/.none(...)`, is `InvariantWithoutConsequence`, and `invariant(...).then(...)`
    without `.when(...)` is `InvariantWithoutWhen`.
  - `forbidden(Order)` and `required(Order)` with no condition are
    `RuleWithoutCondition`, `exclusive(...)` with fewer than two conditions is
    `ExclusiveNeedsTwo`, and `after(UC)` without `.assert(...)` is `AfterWithoutAssert`.
  - A `.has(...)` or `.none(...)` on `invariant` or `holds` that names no condition on a
    related entity, as `.none()` or `.none(Ticket)`, is `QuantifierWithoutCondition`, and
    `invariant().none(...)` with no entity in its scope or its `.when(...)` is
    `QuantifierScopeUnknown`.
  - `maps_field(F, Order.nosuch)` is `UnknownColumn` and `maps_field(F, Nosuch.status)`
    is `UndefinedSymbol`; a qualified column was not read at all (#48 then made
    `maps_field(Field, Entity.column)` the only form).
- `invariant(Order).when(status == paid).none(Ticket.status == pending)` is checked. A
  one-entity `invariant` takes the related entity of `.has/.none(...)` from its qualified
  columns, as `invariant(Order, Ticket)` does, and prints the same rule and TLA+ spec; it
  was dropped. `invariant().when(Order.status == paid).none(...)` takes its entity from the
  `.when(...)` columns; it was dropped as well.

## v0.3.0 - 2026-10-30

### Breaking

The `diagram` command was redesigned around the RDRA 3.0 diagrams. It writes Mermaid
text only, for a reader (a person, or a tool that draws SVG) that redraws it, so each
diagram carries the RDRA meaning in text: a `%%` header, `<kind>__<DSL id>` node ids
shared by every kind, `:::<kind>` tags, Japanese RDRA layer names, DSL predicate
names as edge labels, and `%% derived` / `%% desc` comment lines. The first eight
entries below are that redesign; the new kinds are under Added.

- `diagram` writes Mermaid text (`.mmd`) only, and `--format` defaults to `mermaid` (was
  `puml`). `--format puml`, `--format svg`, and `--format png` are removed and fail with
  a message that names `--format mermaid`; a `diagram` command without `--format` now
  writes `<out>.mmd` instead of `<out>.puml`. To get an image, render the `.mmd` with
  Mermaid CLI (`mmdc`) or another Mermaid-aware tool. `export --kind plantuml-er` is
  removed and fails with a message that names `--kind mermaid-er`. The `rdra-ish-render`
  crate, the `rdra_ish_emit::plantuml` module, `DiffPlantUmlEmitter`, the
  `plantuml.jar` lookup, and `PLANTUML_JAR` are gone, as are the 15 `samples/*/out/*.puml`
  artifacts. The dev shell no longer provides PlantUML or OpenSSL.
- `diagram --kind rdra` writes the diagram encoding contract (`docs/cli-reference.md`,
  "Encoding contract"), so anything that parsed the old `.mmd` breaks. The file opens with
  `%%` header lines (`rdra-ish diagram=rdra scope=... model=...`, `layers:`, `kinds:`,
  `omitted:`); node ids are `<kind>__<DSL id>` (`usecase__Capture`, was `Capture`); labels
  are the DSL labels without the emoji prefix, and every node is `["..."]` tagged
  `:::<kind>`, with one `classDef` per kind at the end; the layers are
  `subgraph layer_value["システム価値"]` and so on, without `direction TB`. Edge labels stay
  the DSL predicate names, but the line style now follows the RDRAGraph relation kind:
  `performs` is `-.->` (was `-->`), and `belongs` and the flow order `precedes`,
  `branches`, and `repeats` are `-->` (were `-.->`). A fact declared twice is drawn once.
  Use cases now draw `transitions <Entity>.<column> <from>-><to>` edges to the state they
  enter, and a `--buc` scope holds those states. A `combines` edge derived from a
  condition's `holds` is followed by a `%% derived` line.
- `diagram --kind state` writes the encoding contract. It opens with the `%%` header
  (`omitted:` names the `Enum` columns no `transitions` drive), and draws one composite
  state per state model, `state "Order.status" as statemodel__Order_status { ... }`,
  instead of every state at the top level. State ids are `state__<Entity>_<variant>`
  (was `<Entity>_<variant>`), the same as in `rdra`, and follow the `Enum` order; labels
  are the variant without the emoji prefix. A transition is labelled with the use case id
  (`usecase__Capture`, was `✅ Capture Payment`), and `<state> --> [*]` marks each end
  state, one with no transition to another state. `--buc` now draws each state model one
  of the BUC's use cases moves; it drew an empty diagram before.
- `diagram --kind event-flow` is renamed `--kind event` and fails with a message that
  names it. The diagram writes the encoding contract: the `%%` header, node ids
  `event__`, `usecase__`, `buc__`, `extsystem__`, and `timer__` (were `ev__`, `uc__`,
  `buc__`, `xs__`, and `tm__`), DSL labels without emoji, every node `["..."]` tagged
  `:::<kind>`, and one `classDef` per kind. An event delivered to an external system is
  labelled `triggers`, its DSL predicate (was `delivers`). The state nodes and the
  state-to-state edges labelled with the event are gone; `--kind state` draws the
  transitions, and `%% omitted: state transitions` says a drawn use case drives one. An
  event's raiser outside a `--buc` scope is no longer declared as a node without an edge.
  `rdra_ish_emit::mermaid::EventFlowMermaidEmitter` is now `EventMermaidEmitter`. The
  artifact keeps its name, `samples/clinic-ops/out/event_flow.mmd`.
- `diagram --kind er` and `export --kind mermaid-er` open with the encoding contract's
  `%%` header (`%% rdra-ish diagram=er scope=... model=...`, `%% kinds: entity`,
  `%% omitted: (none)`; the export's header has no `model=`). The `erDiagram` body is
  unchanged, entity names included.
- `diagram --kind boundaryless-graph`, `--kind business-area`, and `--kind technical-area`
  are removed, and each fails with a message that names its replacement.
  `boundaryless-graph` drew the `rdra` graph without its layers and API nodes; use
  `--kind rdra`, narrowed with `--node-kind` / `--edge-kind`. `business-area` drew the
  rows of `csv --kind business-inputs`, and `technical-area` the API x entity CRUD of
  `csv --kind api-matrix`; use those tables. In `rdra_ish_emit::mermaid`,
  `RdraMermaidEmitter` is now the `rdra` emitter (it was `ObjectGraphMermaidEmitter`);
  the old boundaryless `RdraMermaidEmitter`, `BusinessAreaMermaidEmitter`, and
  `TechnicalAreaMermaidEmitter` are gone.
- `diagram --kind sequence` and `diagram --kind diff` are removed, and each fails with a
  message that names its replacement. The transaction, API, and system-boundary warnings
  that `sequence` printed are `check`'s, which still prints them; `csv --kind
  api-matrix` lists the entities each API writes. For a model diff, write `--kind rdra`
  for both models and compare the two `.mmd` files with `diff` or `git diff`. The
  `--diff-base` option goes with them (clap now rejects it as an unknown argument),
  `--usecase` applies to `--kind usecase` only, and `--node-kind` and `--edge-kind` apply
  to `rdra` only.
  The 109 sequence artifacts (`samples/clinic-ops/out/uc/`, the `sequence_*.mmd` files
  under `samples/*/out/`) and the sequence sections of
  `samples/clinic-ops/design-sample.md` are gone, as are `rdra_ish_emit::diff` and
  `SequenceMermaidEmitter`.
- `diagram --show-description` and `--view-preset` are removed (clap rejects them as
  unexpected arguments). `rdra` always writes each description as a
  `%% desc <id>: <text>` line after its node, instead of a linked note node; narrow a
  diagram with `--node-kind` and `--edge-kind`. `View::with_descriptions` and
  `View::show_descriptions` are replaced by `View::with_model_path` and
  `View::model_path`, the input paths the header prints.
- `verify` checks each temporal property in its own TLC run, because TLC does not name the
  property a temporal counterexample violates. The runs use the module
  `<whole>_<Property>` (`RdraSpec_NeverShipped`), the same module text and fairness as the
  whole-model module, and a `.cfg` with that one `PROPERTY`; they start after the cone runs,
  at most two at a time. Stdout prints one `RdraSpec_<Property> [<Property>]: <result>`
  line per property instead of one `RdraSpec [<P1>, <P2>]: <result>` line, and its
  `temporal properties:` line now says `each checked in its own run on the whole model`.
  `cones.txt` lists the property modules instead of `RdraSpec`. `-o` still writes the
  whole-model `RdraSpec.tla` / `.cfg` (or `X.tla` / `X.cfg` for `-o X.tla`) with every
  `PROPERTY`, which `verify` no longer runs; `export --kind tla` is unchanged.
- A violated temporal property now prints in model terms, as
  `error[<Property>]: property <Property> is violated: ...`, before TLC's raw trace: the
  instance it fails for, the step that shows it, the lasso's steps with their use cases,
  bound instances, changes, and sites, then `then loops back to step K` or `then nothing
  changes forever`. Before, it printed only TLC's raw trace, and the name of the violated
  property appeared nowhere. A lasso that does not replay or does not violate the property
  prints the engine-disagreement line. The raw trace now also prints TLC's
  `Back to state K: <action>` line, which it dropped before. New sample
  `temporal_unmet.rdra` (`verify: tlc=counterexample fail NeverShipped
  PendingLeadsToDelivered`).
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

- `diagram --kind context` draws the RDRA system context under the encoding contract:
  the system with each actor that performs one of its BUCs, use cases, or steps, and
  each external system that raises an event one of its use cases consumes or receives
  one they raise. Every edge is derived (`%% derived ... via`). With several systems an
  edge ends at the system containing an API the use cases behind it invoke; with none,
  or none traceable, at a synthesized `system__Model`. ec-site, clinic-ops, and
  incremental-order step 6 (two systems) gain `out/context.mmd`.

- `diagram --kind requirement` draws the RDRA requirement model under the encoding
  contract: requirements with the BUCs they `motivate`, NFRs with what they `apply_to`,
  constraints with what they `constrain`, and the qualities both `qualify` for.
  clinic-ops gains `out/requirement.mmd`.

- `diagram --kind business` draws the RDRA business context and business use cases
  under the encoding contract: each business as `subgraph business__<id>` in the
  システム外部環境 layer holding its BUCs, and the actors and external systems of the
  システム価値 layer each BUC involves. `performs(Actor, Buc)` is drawn as declared;
  an actor performing one of the BUC's use cases or steps, an external system raising an
  event that starts the BUC or one of its use cases (`raises <Event>`), and an event a
  use case of the BUC raises to an external system (`triggers <Event>`) are derived,
  each followed by `%% derived ... via <node>`. `--buc` narrows it to the RDRA BUC
  diagram. ec-site, clinic-ops, personal-info, and incremental-order step 6 gain
  `out/business.mmd`.

- `diagram --kind flow` draws the RDRA business flow under the encoding contract, as
  `flowchart TD`: each flow as `subgraph flow__<id>` holding its steps, the declared
  `precedes`, `branches`, `excepts`, and `repeats` edges between steps, and `covers`
  edges from a step to the use cases, APIs, events, and timers it covers. A step one
  actor performs sits in a `subgraph lane__<Flow>__<Actor>` swimlane; a step several
  actors perform has a `performs` edge from each. clinic-ops gains
  `out/buc/flow_<buc>.mmd` for its four BUCs with a flow.

- `diagram --kind usecase` draws the RDRA UC複合図 under the encoding contract: each use
  case with the actors who perform it, the screens it displays and the entities they show,
  the entities it reads and writes, the events it raises or that trigger it with their
  external systems, the timers that trigger it, the conditions that apply to it, and the
  states it moves, in the `rdra` layers and with the `rdra` ids. An invoked API's CRUD is
  drawn from the use case and an actor of the use case's BUC or step performs it, each
  followed by `%% derived ... via <node>`. `--buc` keeps the use cases a BUC contains
  (directly or through a flow's steps), and `--usecase <id>` (repeatable, not with
  `--buc`) keeps the named ones; `--usecase` on another kind and an unknown id are
  errors. The BUC sections of `samples/clinic-ops/design-sample.md` embed it:
  `samples/clinic-ops/out/buc/usecase_*.mmd` replace the `rdra --buc` artifacts
  `out/buc/object_graph_*.mmd`, and ec-site gains `out/usecase_buc_*.mmd` and
  `out/usecase_capture.mmd`. The error for the removed `--kind sequence` now names
  `--kind usecase` first.

- `diagram --kind information` draws the RDRA information model under the encoding
  contract: every entity of the scope, each `relate` as `---|<card>|` from its left
  end (the relates `--kind er` draws), each state model (a column that `transitions`
  drive) as `statemodel__<Entity>_<column>` linked `-.->|statemodel|`, and each
  variation a column takes linked `-.->|variation <column>|`. Columns are left to `er`.
  ec-site, clinic-ops, personal-info, and incremental-order step 6 gain
  `out/information.mmd`.

- `diagram --kind condition` draws the RDRA variations and conditions under the
  encoding contract: each condition with an `-.->|combines|` edge to each variation and
  state model (`statemodel__<Entity>_<column>`) it combines, a `%% holds` line for its
  body, and `%% derived ... from holds` after a link read from that body; its
  `applies_to` edges to use cases, APIs, and systems; and each variation as
  `subgraph variation__<id>` holding its values as `variation__<Variation>__<value>`
  nodes tagged `:::value`. The whole model draws every variation. clinic-ops and
  ec-site gain `out/condition.mmd`.

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
- The `rdra` diagram now lists `variation` nodes, which it already placed in
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

- On Windows, every diagnostic site, `check` counterexample step, and import cycle named
  the file by its verbatim path (`\\?\C:\...\order.rdra`), because the canonical path came
  from `std::fs::canonicalize`. Paths are now resolved with `dunce`, so Windows users see
  `C:\...\order.rdra`; Linux and macOS output is unchanged. A clippy `disallowed-methods`
  rule keeps `std::fs::canonicalize` out of the workspace, and
  `cargo test --workspace` passes on Windows.
- `fmt` deleted every `//` and `/* */` comment, and `fmt --write` wrote the file back
  without them, because the lexer skipped comments before the formatter saw the source.
  The lexer now keeps each comment with its span, and `fmt` prints it where it was: a
  comment after code stays at the end of that line, and a comment on its own line stays
  above the item, column, or variation value that follows it, with a blank line after it
  kept between items. A comment between the metadata lines of an element or inside a
  wrapped predicate call moves to its own line above that element. A second `fmt` changes
  nothing. All 437 comments in the `.rdra` files under `samples/` and `skills/` survive.
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

- `diagram` of every kind but `state` draws a model whose lifecycle has errors, with one
  stderr note, as `--kind er` already did; before, `rdra` and `event` refused to write.
  Those kinds draw declared facts only. `--kind state` reads each state model's start states from the
  lifecycle and still refuses. The note now ends `this output does not read the
  lifecycle` (was `the data layer does not depend on them`) for every output that prints
  it.
- `scripts/check-sample-artifacts.sh` also checks the documents that embed the
  artifacts. Every `samples/incremental-order` step artifact is in
  `scripts/sample-artifacts.tsv`; `scripts/embed-sample-artifacts.py` copies each into the
  block under its `<!-- embed: <path> -->` line in the step `design.md` files and
  `docs/diagram-sample-review.md`, and fails when a command those documents print is not
  the manifest row for its `--out`. The script then runs
  `scripts/gen-clinic-ops-design-sample.py`, and a diff in any of these documents fails
  it. The step 0-5 `sequence_buc_store_restock.mmd` files are gone.
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
