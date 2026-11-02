# insurance-claim-ops

insurance-claim-ops models the claim work of a Japanese non-life insurer's 損害サービス部門 for motor
insurance: the accident report (to the insurer or through the 代理店) and the initial response, the
damage survey by an adjuster and the decision on the insured's liability, the repair estimate and
the approval before the repair starts, the settlement the insurer negotiates for the insured
(示談代行), the 自賠責 survey by the 損害保険料率算出機構 under the 一括払い, the claim and its 請求完了日,
the payment period and its extension, the decision to pay or not to pay with the senior approval
of a suspected claim and the explained denial, the payment with late interest, the recovery
through subrogation (請求権代位), and the complaint and the そんぽADRセンター procedures. Premiums and
underwriting, the calculation of the loss, mediation and lawsuits, the 自賠責 objection and dispute
procedures, interim payments, and the salvage of a total loss are out of scope; each has an
`out-of-scope` requirement.

The sample is the third domain written without changing the language (#51, condition 3). What
the language could not say, or said awkwardly, is listed under [Soak findings](#soak-findings).

## Sources of the business rules

The `//` comments in `shared/*.rdra` and `buc/*.rdra` cite a source as `<file>.txt:<line>` or
`<file>.md:<line>`. The files are copies of primary sources saved during the research on
2026-11-02: e-Gov law texts (保険法, 保険業法, 自動車損害賠償保障法, 民法, 弁護士法), the 金融庁 supervisory
guideline for insurers and its papers on the 2005 payment omissions, the 国土交通省 pages and the
自賠責保険普通保険約款, the 日本損害保険協会's 保険金支払に関するガイドライン, its そんぽADRセンター, adjuster, and
shared-database pages, the 損害保険料率算出機構's survey pages, and the 約款 and pages of 東京海上日動
(TAP), イーデザイン損保, and 損保ジャパン. They are not in this repository, because the copies are for
reading and are not redistributed.

- The copies live in `~/research/japan-insurance-claim-operations/sources/` on the machine where
  the research ran. A citation of `map.md` refers to
  `~/research/japan-insurance-claim-operations/map.md`.
- `~/research/japan-insurance-claim-operations/map.md` maps each file to its original URL in the
  table `原文（sources/）`.
- A line number refers to the saved copy, not to the page at the original URL. To follow a
  citation elsewhere, fetch the original URL and search for the quoted words.

The research left four gaps, and the comments say so where they apply:

- **One major insurer's 約款.** Of the large insurers only 東京海上日動's TAP 約款 was saved; イーデザイン損保
  and the 自賠責保険普通保険約款 confirm the 30-day period and the five extensions, and 損保ジャパン's FAQ the
  30 days. The other large insurers' 約款 were not reached (map.md:109).
- **No clause ends the contract on a total loss.** The model leaves the effect of a total loss on
  the contract out (`ReqNoSalvage`, map.md:110).
- **No internal case states.** The insurers publish the processes 事故受付, 初期対応, 損害確認, and
  保険金のお支払い (tokiomarine-songai-auto-taisei.md:44), not the states of a case. Every status name
  of `ClaimCase`, `Claim`, `Settlement`, and the others is the model's assumption (map.md:113).
- **No form of the 見積協定.** The adjuster's work is defined (sonpo-2024-adjuster-seido-gaiyou.txt:11),
  but not how the cost is agreed with the shop; the state `agreed` of `RepairEstimate` is the
  model's assumption (map.md:114).

Two facts about the periods matter for the model. The 30 days and the extensions of 60, 90, 120,
and 180 days are the 約款's (tokiomarine-2025-tap-yakkan.txt:4633, :4650-4658), not the law's: 保険法21条
caps a period at 相当の期間 (egov-hokenhou-420AC0000000056.txt:182), and 自賠法16条の9 gives the 被害者請求 no
day count, only 必要な期間 (egov-jibaihou-330AC0000000097.txt:233). Where the research found nothing
(the insurer's staff roles, its claim system, the screens, the goals' numbers), the comment says
the modeling is the model's assumption.

## Actors and external systems

A party is an actor when it takes part in a use case of the insurer: it reports, consents,
submits, or surveys, and the insurer's staff record it. It is an external system when it is an
organization that does its own work outside the insurer and exchanges documents or notices with
it. The language decides half of it: an `extsystem` talks only through events, holds no
permission, and no timer starts it.

| Party | Kind | Why |
|---|---|---|
| 契約者・被保険者 `PolicyHolder` | actor | reports the accident, consents to the 示談代行, claims, withdraws, complains |
| 被害者 `Victim` | actor | consents to deal with the insurer (TAP :2504), claims directly (:2493), complains |
| 代理店 `Agent` | actor | takes the report (tokiomarine-nagare-auto.md:44); kept out of the decision (`AdrAgentOutOfDecision`) |
| 修理工場 `RepairShop` | actor | submits the estimate and starts the repair in the insurer's process; no system between them was found |
| アジャスター `Adjuster` | actor | surveys and agrees the repair cost (sonpo-2024-adjuster-seido-gaiyou.txt:4, :11) |
| 事故受付担当, 損害サービス担当者, 支払決裁者, 上位決裁者 | actors | the insurer's staff; the split is the model's assumption, after the supervisory guideline's 決裁権限 (fsa-2026-kantoku-shishin-ins.txt:8376, :8379) |
| 損保料率機構 `Giroj` | external system | receives the 自賠責 documents and reports its survey (mlit-jibaiseki-procedure.md:58, giroj-survey-system.md:103) |
| そんぽADRセンター `SonpoAdrCenter` | external system | notifies complaints and runs its procedures outside the insurer (egov-hoken-gyouhou-407AC0000000105.txt:8835) |
| 自動車事故情報交換システム `AccidentInfoExchange` | external system | the shared database that says whether another insurer received the same accident (sonpo-kyodoriyou-0004-jidousha-jiko.md:97) |

## Modeling choices

- **A case is one accident under one 担保種目.** The guideline gives each 担保種目 its own handler
  (sonpo-2026-shiharai-guideline.txt:340), and each 条項's claim right arises at its own time
  (tokiomarine-2025-tap-yakkan.txt:4575-4584). `ClaimCase.coverage` decides which work applies: a
  settlement only for 対人・対物賠償, a repair estimate only for 車両・対物賠償, a 自賠責 survey only for
  対人賠償・人身傷害. Each is a guard on the creating use case and a rule along the key.
- **The case and the claim are separate state models.** `ClaimCase.status` follows the handling
  (事故受付, 初期対応済, 調査中, 示談交渉中, 損害確定, 完了) and `ClaimCase.liability` the decision on
  the insured's liability; `Claim.status` follows the claim to its decision and payment, and
  `Claim.period` the payment period. A case can have several claims (each 被害者 of an 対人賠償 case).
  The decision states sit on the claim, not on the case: a decision reads only the claim's
  columns, so the claim's cone stays small (below). The case closes on the events the claim
  raises (`EvClaimPaid`, `EvDenialNotified`, `EvClaimWithdrawn`), which impose no order (#30),
  so `check` also lets a case close while its claim is open; no rule reads both.
- **Periods are timers, not states.** `PaymentDeadline` starts `MarkPaymentOverdue`, which moves
  the period to 期限経過 and starts the late interest; `ClaimLimitation` starts `TimeBarClaim`;
  `SubrogationLimitation` starts `LapseSubrogation`. A timer has no parameter, so one timer
  stands for the 30 days and every extension, and the label lists them. The extension reason
  is the variation `ExtensionReason`, chosen by `ExtendPaymentPeriod`; no rule reads it.
- **The ADR's periods are not timers.** The 60 days to the dispute guidance and the 4 months
  to a proposal run inside the そんぽADRセンター, and a timer cannot start an external system. The
  60 days is in the flow; the 4 months is the NFR `AdrProposalTarget` on `RecordProposal`.
- **Who decides follows the suspicion.** `DecideClaim` (支払決裁者) runs only on a claim with no
  suspicion, `DecideSuspectedClaim` (上位決裁者) only on a suspected one and records the senior
  approval. The permissions differ, so the access check sees who may decide which.
- **The denial is explained before it is notified.** `ExplainDenialReason` records the
  explanation of the 約款 clause and the survey; `NotifyDenial` runs only after it.
- **One payment per claim.** `relate(Payment, Claim, 1:1)`, created by `PayClaim` with the claim's
  move to 支払済. The payment copies `late_interest == Claim.late_interest_due` (#69), and the
  rule `支払期限を過ぎた支払には遅延利息を付ける` reads both. 内払 is out of scope.
- **The subrogation starts from a paid claim.** `relate(Subrogation, Claim, 1:1)`; the guard
  `ClaimPaid` reads the claim the subrogation is created under and asks for no subrogation yet
  (`.none(Subrogation)`).
- **A use case that writes a parent and creates a child uses one API.** `RequestInvestigation`
  and `OpenSettlement` each invoke one API that creates the child and updates the case, so the
  step is one transaction group. With two APIs the creation ran as a second action with no
  guard and no order to the first (see Soak findings).
- **An OR is two guarded arms.** A case is assessed when the survey is reported and either the
  coverage is first-party or the liability is decided. `AssessDamage` declares the arm
  `investigating -> assessed` twice, once under `FirstPartyCoverage` and once under
  `LiabilityDetermined`. The fault still decides the recovery from the other party, so
  `CloseCase` needs it decided; `DetermineFault` runs until the case is assessed.
- **A use case that moves two columns lists an arm for each state of both.** `WithdrawClaim` and
  `TimeBarClaim` move the period from `not_started` to `ended` as well, or they would not run on a
  claim whose period has not started.

## Rules and properties

`rdra-ish list samples/insurance-claim-ops --kind rule` prints the labelled rules. Each was
checked by mutation: dropping its guard or effect from a copy of the sample makes `check` or
`verify` fail with the rule's name. 16 of 17 mutants fail; Soak findings names the one that does
not.

| Rule | Guard or effect it relies on |
|---|---|
| 修理着工は事前承認の後 | `RepairApproved` on `StartRepair` |
| 被害者の同意なく示談代行しない / 被保険者の同意なく示談代行しない | `BothConsented` on `BeginNegotiation` |
| 過失ゼロなら示談代行しない / 示談代行は対人・対物賠償の事故だけ | `SettlementEligible` on `OpenSettlement` |
| 損害調査の報告の前に損害を確定しない | `InvestigationReported` on `AssessDamage` |
| 賠償の事故は賠償責任を判断してから損害を確定する | the `.when(LiabilityDetermined)` arm of `AssessDamage` |
| 修理見積は車両・対物賠償の事故だけ / 自賠責の損害調査は人身の事故だけ | `RepairCoverage`, `InjuryCoverage` |
| 等級の変動を説明してから請求の意思を確かめる | `GradeExplained` on `SubmitClaim` |
| 不正請求の疑いがある請求は上位の決裁者が決める | `NoFraudSuspicion` on `DecideClaim`, `ClaimBeforeDecision` on `FlagSuspectedFraud` |
| 不払いは理由を説明してから通知する | `ReasonExplained` on `NotifyDenial` |
| 支払期限を過ぎた支払には遅延利息を付ける | the copy `sets(PayClaim, Payment, late_interest == Claim.late_interest_due)` |
| 支払後に求償権が移る (and the after-assert 求償は保険金を支払った請求についてだけ始める) | `ClaimPaid` on `StartSubrogation` |
| 特別調停案は会社が受諾する | `OrdinaryProposal` on `RejectProposal` |

`ClaimEventuallyDecided` (`請求完了した請求は、いずれ支払決定か不払決定に至る`) is a `leads_to` with
`.fair(DecideClaim, DecideSuspectedClaim)`, and `SubrogationEnds` a `leads_to` with
`.fair(LapseSubrogation)`. Both are decided on their cones.

## Verification

`check` reports no warning. `verify --deny-undecided` at 2 instances passes with `tlc_result: ok`
in 4 to 5 s locally (about 460 MB). The cones at 2 instances, from `lifecycle_measure -- 3000000 verify`:

| Cone (targets) | States |
|---|---:|
| `ClaimCase`, `Investigation`, `Settlement` (8 rules, `Unique_Investigation_ClaimCase`, `Unique_Settlement_ClaimCase`) | 60,193 |
| `SubrogationEnds` | 4,783 |
| `Payment` (`支払期限を過ぎた支払には遅延利息を付ける`) | 3,995 |
| `Subrogation` (`支払後に求償権が移る`, the after-assert) | 3,331 |
| `Claim` (5 rules, `Unique_Payment_Claim`, `ClaimEventuallyDecided`) | 2,975 |
| `Complaint`, `RepairEstimate`, `JibaisekiSurvey` | 483, 441, 189 |

Every cone is under the 200,000 states `engine_equivalence` compares at 2 instances, so none is in
`VERIFY_FALLBACK`.

## Known verification gaps

- **The settlement does not gate the liability claim.** A liability claim can be completed
  only with the 示談書 (tokiomarine-2025-tap-yakkan.txt:4576-4577, :4596), but the claim and the
  settlement are both children of the case, and a guard follows one key: a guard on
  `CompleteClaim` that reads `Settlement` reads any settlement, not the one of the claim's case.
  The rule is not in the model (see Soak findings).
- **The payment period is not tied to its reason.** `PaymentDeadline` cannot read
  `Claim.extension`, so nothing checks that a 90-day extension ends at 90 days; the model checks
  only that a claim paid after its deadline carries the late interest.
- **The case closes without reading its claims.** `CloseCase` runs on the claim's events, which
  give no order (#30).
- **A double payment is stopped by the lifecycle, not by the guard.** `Unique_Payment_Claim`
  checks it at 2 instances; the guard `.none(Payment)` on `PayClaim` is redundant (see Soak
  findings).
- **The ADR's periods and the 自賠責 survey's escalation run outside the insurer.** They are
  events whose order and timing the model does not check.

## Diagnostics left in place

`check` prints only its `note:` that two temporal properties are for `verify`. `lint` reports
nothing and scores 100. Every API carries a method and a path, and every screen is reached by a
`navigates`, some from another BUC's file through `import buc.<name>.{Screen}`.

## Soak findings

The model was written against the language at de6b7cc, with no change to the grammar, the
parser, the core, or the emitters. Each finding is rated (G) needs a grammar change, (B) a bug
in the tool, (V) verification capacity, (D) diagnostic, documentation, or diagram quality, or
(M) a modeling choice.

The forms #52 to #75 added were used as documented: `in` and `not in`, multi-arm `transitions`
with a `.when` per arm, two lifecycle columns on one entity (`ClaimCase.status` and `liability`,
`Claim.status` and `period`), `chooses`, a copy into a created row (#69), `.none(Entity)` and
`.has(Entity)` (#73), `leads_to(...).fair(...)` on cones (#67), an `after` assert, and `verify
--deny-undecided`. Every pair of entities has one key, so no `.by(role)` was needed; no rule
needed `drives` or a guarded `chooses`.

What the model could say but `verify` could not check, and what it could not say:

- **(G, V) A guard cannot follow two keys.** A liability claim is completed with the 示談書
  (tokiomarine-2025-tap-yakkan.txt:4576-4577, :4596): `Claim` and `Settlement` are both children
  of `ClaimCase`. The rule can be written,
  `forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, ClaimCase, Settlement)`,
  but its guard cannot: `holds(SettlementSigned, Settlement.status == agreed)` on `CompleteClaim`
  loads with no warning and reads an independent settlement, any settlement of any case. `check`
  passes it at 1 instance. At 2 instances the guard also merges the claim's cone (2,975 states)
  with the case's (60,193): `lifecycle_measure` stopped the claim's view past 20,000,000 states
  after 247 s, and `verify` stops the `Claim`, `Payment`, and `Subrogation` cones at the
  BFS cap of 2,097,152 states, leaves 11 state-model findings `not decided`, and had printed no
  TLC verdict when it was killed at 300 s. The rule and the guard are left out (Known
  verification gaps). Repro: this sample plus the condition and the `.along` rule above.
- **(M, D) One timer stands for every payment period.** The payment period is 30 days, or 60,
  90, 120, or 180 by the extension reason (tokiomarine-2025-tap-yakkan.txt:4633, :4650-4658). One
  timer stands for all of them, and `chooses(ExtendPaymentPeriod, Claim.extension)` records a
  reason that no rule and no timer reads; `chooses` on a column nothing reads adds no variable.
  Nothing checks that the deadline follows the reason. This was first rated (G), a timer with no
  parameter, but the language can say it today: one timer per period, each triggering its own
  use case guarded by a condition on the reason (`holds(Disaster, Claim.extension ==
  disaster_area)` applied to the 60-day use case, and so on). A reduced model of that shape
  passes `check` and `verify`. The model keeps one timer; the docs do not show the idiom.
- **(V) Without `.fair`, neither property is decided.** With the two `.fair(...)` removed, each
  cone finds a lasso that only the whole model's `WF_vars(Next)` rules out
  (`Cone_ClaimEventuallyDecided [ClaimEventuallyDecided]: not decided (2975 states,
  Cone_ClaimEventuallyDecided_WF: ok, needs the whole model's WF_vars(Next))`), and the
  whole-model run stops at the state cap: `RdraSpec_ClaimEventuallyDecided
  [ClaimEventuallyDecided]: not decided (2272892 states, stopped at the state cap of 2097152
  states)` after 143 s with 1,736,706 states queued, and `SubrogationEnds` at 2,270,739 states
  after 144 s. `--deny-undecided` then exits 1. Both properties are checked only because they
  name the use cases they rely on.
- **(D) The hint for a whole-model run stopped at the state cap names the time budget.** The
  run above prints `the whole-model run stopped at the state cap of 2097152 states after 2272892
  states; raise --temporal-budget or shrink the model`, 143 s into a 300 s budget. Raising the
  budget does not lift the cap, and no flag does.
- **(D/M) A use case that invokes two APIs is two actions, and the second runs without the
  guard and without the first.** `OpenSettlement` first invoked the case API (moving the case
  to 示談交渉中) and a settlement API (creating the settlement), guarded by
  `SettlementEligible` on the case. `check` reported `示談代行は対人・対物賠償の事故だけ` with the trace
  `RelayAccidentReport`, `OpenSettlement_2, transaction 2 of OpenSettlement`: the creation ran on
  a vehicle case still in 事故受付, with no guard and no first transaction. state-derivation.md says
  the first group that writes an entity the body reads carries the guard, but not that the other
  groups run with none and in any order. One API that creates the child and updates the case
  makes it one step.
- **(D) A use case that moves two lifecycle columns runs only where both have an arm.** A use
  case with `transitions(Go, E.status, a -> b)` and `transitions(Go, E.period, y -> z)` does not
  run at `(a, x)`. The language reference's section on several lifecycle columns does not say
  it. `WithdrawClaim` and `TimeBarClaim` therefore list `period not_started -> ended`; with a
  self-loop instead, `check` reported `Deadlock_Claim_period_not_started` on a withdrawn claim.
- **(D) The state diagram draws each column of a two-column move on its own.** In
  `out/state/Claim.mmd` the period block shows `未起算 --> 期限の管理を終了 : 請求の取下げ` and the status
  block `請求の案内済 --> 取下げ : 請求の取下げ`; nothing says the two arms are one step, or that
  `WithdrawClaim` cannot run unless both columns have an arm.
- **(D) A variation value that is a kind keyword gives a misleading error.** `variation V "V" {
  a "A"  field "B" }` fails with `a \`variation\` needs a body listing at least one value`, at
  the `variation` line, not at `field`; `Enum(a, field)` says `unexpected \`field\`; expected
  \`)\``. Neither says the word is reserved. The survey method is `site_visit`.
- **(D) `states --entity Claim` lists 39 use cases that never touch `Claim`** before its table,
  each `whether ResolveComplaint can run is not decided: no cone of a selected entity keeps it`.
- **(D) A column a screen shows and no use case writes draws no finding.** `Claim.completed_on`,
  mapped to the field 請求完了日, was an axis stuck at `null` in `states` until `CompleteClaim` set
  it; neither `check` nor `lint` noticed.
- **(M) The guard `.none(Payment)` on `PayClaim` is redundant.** Of the 17 mutants that drop one
  guard or effect, 16 fail `check` or `verify` with the rule they protect; dropping `.none(Payment)`
  fails nothing. The arm `decided_payment -> paid` is taken once per claim, so a second payment
  cannot be made, and `Unique_Payment_Claim` checks it at 2 instances. The guard stays as the
  statement of `支払済みの案件は再度支払わない`. Dropping `.none(Subrogation)` fails only `verify`
  (`Unique_Subrogation_Claim`), as the 1:1 `relate` predicts.
- **(M) An OR is written as one arm declared twice.** "First-party coverage or a decided
  liability" on `AssessDamage` is two `.when` lines on the same arm. It works (dropping one
  guard fails `賠償の事故は賠償責任を判断してから損害を確定する`), but only for an arm: a guard on the whole
  use case has no OR.
- **(M) The ADR's periods are not timers.** A timer cannot start an external system, and the
  insurer's records do not change when the 60 days or 4 months pass. They are labels and an NFR.
- **(M) The decision states are on the claim, not on the case.** The task's sketch put
  支払決定・不払決定・支払済・求償中 on the case. With the decision reading the case, the claim's cone
  would join the case's (see the first finding); the case closes on the claim's events instead,
  which give no order (#30).

`scripts/gen-clinic-ops-docs.py target/debug/rdra-ish insurance-claim-ops` generates
`design-doc.md` and `prd.md` from the model, in the same layout as the other ops samples.
