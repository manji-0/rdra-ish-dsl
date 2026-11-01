# food-permit-ops

food-permit-ops models the food business permit work of a Japanese 保健所 (a prefecture, a city with
its own 保健所, or a special ward): the consultation before construction and the classification of the
business (permit, notification, or neither), the permit application on paper or online, the fee, the
facility inspection with its improvement and reinspection, the permit or its refusal with reasons,
the permit certificate, the term and its renewal, the change, succession, and closure notices, the
business notification, the annual monitoring plan and site visits, and the adverse dispositions
(improvement order, suspension, revocation) with their hearing or explanation and the publication of
violations. The national 食品衛生申請等システム is an external system reached through events. The
contents of an appeal or a lawsuit, food labeling, imported food, food poisoning investigations, and
permits under other laws are out of scope; each has an `out-of-scope` requirement.

The sample is the second domain written without changing the language (#51, condition 3). What
the language could not say, or said awkwardly, is listed under [Soak findings](#soak-findings).

## Sources of the business rules

The `//` comments in `shared/*.rdra` and `buc/*.rdra` cite a source as `<file>.txt:<line>` or
`<file>.md:<line>`. The files are copies of primary sources (e-Gov law texts of the 食品衛生法, its
施行令 and 施行規則, the 行政手続法 and the 行政不服審査法; notices, Q&A, and the FAQ of the 食品衛生申請等システム
from the 厚生労働省; ordinances, fee tables, 審査基準, 処分基準, and procedure pages of prefectures and
cities) saved during the research on 2026-11-01 and 02. They are not in this repository, because the
copies are for reading and are not redistributed.

- The copies live in `~/research/japan-food-permit-operations/sources/` on the machine where the
  research ran. A citation of `map.md` refers to `~/research/japan-food-permit-operations/map.md`.
- `~/research/japan-food-permit-operations/map.md` maps each file to its original URL in the table
  `原文（sources/）`.
- A line number refers to the saved copy, not to the page at the original URL. To follow a
  citation elsewhere, fetch the original URL and search for the quoted words.

The research left three gaps, and the comments say so where they apply:

- **No statute requires the inspection before the permit.** The law stops at 基準に合うと認めるとき
  (食品衛生法55条2項). The inspection, its improvement, and the reinspection come from the operating
  documents of 大阪市 and 東京都.
- **The national guideline on the length of the term was not found.** The 2019 notice announced it
  as 別途通知する予定. The term is 5 years or more (法55条3項); the model cites the 熊本県 要領 (5 years, up
  to 7 by its assessment).
- **The online status order is not documented.** The 食品衛生申請等システム FAQ names the states
  申請内容確認待, 検査待, and 発行済, but not their order. That the content check comes before the
  inspection is the model's assumption, as is the withdrawal of a paper application at any point
  before the decision.

Where the research found nothing (the 保健所's staff roles, the ledger system, the screens, the goals'
numbers), the comment says the modeling is the model's assumption.

## Modeling choices

- **Periods are timers, not states.** An expiry timer starts the use case that records what happens
  when the period passes: `PermitTermEnd` starts `ExpirePermit`, `SuspensionTermEnd` starts
  `LiftSuspension`, `AppealPeriodEnd` starts `FinalizeDisposition`, and `PublicationPeriodEnd`
  starts `EndPublication`. The number of each period is in the timer's label and its source in
  the comment.
- **The 標準処理期間 is an NFR, not a timer** (`StandardProcessingPeriod`, applied to `GrantPermit` and
  `DenyPermit`). 行政手続法6条 only asks the 行政庁 to try to set it, and nothing happens to the
  application when it passes. A timer must start a use case, and no use case records anything at
  that moment, so the period is a service level of the decision.
- **Fees are master data** (`FeeSchedule`), with the amounts in its comment. `PermitApplication.fee_paid`
  is the one fee column a rule reads. The fee is never refunded, so no use case clears it.
- **The kind of business decides the classification.** `ClassifyBusiness` has three arms, each
  guarded by the group of `Consultation.business_type` it belongs to, so the inspector records the
  result rather than choosing it.
- **The procedure follows the kind of disposition.** `ProposeDisposition` chooses the procedure, and
  `chooses(...).when(...)` offers 弁明 only for a disposition that is not a 取消 and the omission only
  for an improvement order. Two `forbidden` rules state 行政手続法13条 on the result.
- **`BusinessPermit.operating` carries the 無許可営業 rules.** It records whether the business under
  the permit is open. The use cases that end or suspend a permit close the business, and two
  `forbidden` rules say that no business is open on a suspended, expired, revoked, or closed
  permit. A business that never had a permit is not modeled: there is no row to hold it.
- **The decision and the ledger are joined by events.** `DecideRevocation` and `DecideSuspension`
  move the disposition and raise an event that starts `RevokePermit` or `SuspendBusiness` on the
  permit; `GrantPermit` raises the event that starts `RegisterPermit`; `RenewPermit` the one that
  starts `ExtendPermitTerm`. Written as one use case that moves both, each pair put the other
  entity's columns into the cone and `verify` stopped at its state cap (see below). Events impose
  no order (#30), so `check` also lets the ledger move without the decision; no rule reads both.
- **The disposition has two lifecycle columns.** `Disposition.status` follows the procedure
  (処分案 to 確定) and `Disposition.publication` the publication (未公表, 公表中, 公表終了, and
  公表対象外 for a disposition not made). Each has its own `transitions` and its own state model;
  `PublishViolation` runs only on a decided disposition, `DropDisposition` moves both columns, and
  two `forbidden` rules read both.

## Known verification gaps

The state space limits what this model says and how far `verify` checks it (#51, condition 5).
Each figure is a cone size at the 2 instances of `verify`, from
`crates/rdra-ish-core/examples/lifecycle_measure.rs`, or a TLC run.

- **The one temporal property is checked on its cone.** `HygieneManagerEventuallyAppointed`
  says that a valid permit without a 食品衛生責任者 ends with one appointed or with the permit's
  end, under `.fair(AppointHygieneManager)`. `verify` checks it on the cone of what it reads
  and of `AppointHygieneManager` (three columns, 111 states at 2 instances), not on the whole
  model, where TLC found 6,877,496 distinct states in 300 s with 5,949,713 still queued (#67).
  A property without `.fair` would still go to the whole model, which `verify` now stops at
  its budget as not decided.
- **A renewal is not tied to the permit it continues.** `ApplyRenewal` and `RenewPermit` should run
  only while the permit is valid. The guard puts every column of `BusinessPermit` into the cone of
  `PermitApplication` and `Inspection`, which grows from 14,197 to 429,485 states. That is under the
  `verify` cap of 2,097,152, but over the 200,000 above which `engine_equivalence` compares a cone
  at 1 instance only. The model leaves the guard out. Only `ExtendPermitTerm`, a step from `valid` to
  `valid`, keeps an expired permit from being extended, so `after(ExtendPermitTerm)` holds by
  construction.
- **The decision and the ledger are joined by events.** In the first draft the decision of a 取消
  or 停止 moved the disposition and the permit in one use case, and the renewal wrote the permit too.
  The cones of `PermitApplication`, `Inspection`, and `BusinessPermit` each passed 3,000,000 states,
  and `verify` left seven stuck-state findings undecided at its cap. With the event between them, `check` also lets the ledger record a 取消
  or 停止 that no decision made (#30). No rule reads both entities.
- **A business with no permit is not modeled.** The 無許可営業 rules read `BusinessPermit.operating`,
  so they cover a business that keeps operating after its permit ends, not one that never applied.
- **The 欠格事由 are not state.** 法55条2項ただし書 allows a refusal within 2 years of a revocation; the
  model records the reason only in `DenyPermit`'s description.

## Diagnostics left in place

`check` and `lint` report nothing, and `lint` scores 100. Every API carries a method and a path, so
`api-contract-missing` does not fire, and every screen is reached by a `navigates`, some from another
BUC's file through `import buc.<name>.{Screen}`.

## Soak findings

The model was written against the language as it stood at d8f9094, with no change to the grammar,
the parser, the core, or the emitters. Each finding is rated (G) needs a grammar change, (B) a bug
in the tool, (D) diagnostic, documentation, or diagram quality, or (M) a modeling choice.

- **(G/B) A temporal property was checked only on the whole model.** A property that reads one
  entity was undecidable in practice, so the first soak left it out. #67 checks a property on
  its cone, and `HygieneManagerEventuallyAppointed` is in the model.
- **(G) An entity has one lifecycle column.** A second `transitions` column
  (`Disposition.publication`) is dropped with an unnamed `warning:` ("one Enum lifecycle column per
  entity is supported"), and the language reference does not say so. The publication was moved by
  `sets` under guards instead. Fixed by #68: an entity may have several lifecycle columns, and the
  publication is now a `transitions` lifecycle.
- **(D) A creating use case guarded on an optional parent it only reads never runs.**
  `Runs_ApplyRenewal` said only "in no reachable state do its guards hold". The cause, an
  `.optional()` child created unlinked unless the use case writes the parent, is in
  `state-derivation.md` but not in the warning.
- **(M) A guard on another entity pulls all of its columns into the cone** (14,197 to 429,485
  states for the renewal), and a decision that writes two entities merges their cones. Both were
  traded for events and a looser model (the gaps above).
- **(M/D) A 1:1 `relate` does not stop a second child.** `ScheduleInspection` could create two
  inspections for one application; `check` at 1 instance cannot see it, and `verify` reported
  `Unique_Inspection_PermitApplication` with a 5-step trace. A `.none(...)` on the guard fixed it.
- **(D) `list --kind rule` prints value labels for `==` but not for `!=`, `in`, or `not in` in a
  rule**, while a condition's `in` gets them (`kind != improvement_order`,
  `status in (expired, revoked, closed)`).
- **(D) An entity-scoped state diagram names another entity's set-only column** in its header
  (`--entity BusinessPermit` printed `%% set-only: Disposition.publication`). Since #68 the
  publication is a lifecycle column, so this sample no longer shows it.
- **(D) An API that creates and updates makes every use case that invokes it a creator**, which shows
  as a deadlock that "needs a free slot". Eight APIs were split into a create API and an update API.
- **(M) A created row cannot copy a value from the row that caused it**, so `RegisterPermit` cannot
  give the permit the application's `business_type`; the permit's type is a free choice.

`scripts/gen-clinic-ops-docs.py target/debug/rdra-ish food-permit-ops` generates `design-doc.md` and
`prd.md` from the model, in the same layout as clinic-ops and patent-ops.
