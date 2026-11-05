# patent-ops

patent-ops models the patent application work of a Japanese company's intellectual property
department: invention disclosures and employee inventions, the decision to file, drafting with an
outside patent firm, filing and the formality check, the examination request, office actions, the
first fees and registration, annuities, and the inventor's reward. The Japan Patent Office (特許庁)
is an external system reached through events. Foreign filing and PCT, the contents of appeals and
lawsuits, and designs and trademarks are out of scope; each has an `out-of-scope` requirement.

## Sources of the business rules

The `//` comments in `shared/*.rdra` and `buc/*.rdra` cite a source as `<file>.txt:<line>` or
`<file>.md:<line>`. The files are copies of primary sources (e-Gov law texts, JPO procedure guides
and fee tables, the 35条6項 guideline) saved during the research on 2026-11-01. They are not in this
repository, because the copies are for reading and are not redistributed.

- The copies live in `~/research/japan-patent-operations/sources/` on the machine where the
  research ran.
- `~/research/japan-patent-operations/map.md` maps each file to its original URL in the table
  `原文（sources/）`.
- A line number refers to the saved copy, not to the page at the original URL. To follow a
  citation elsewhere, fetch the original URL and search for the quoted words.

No primary source describes a company's own procedure before filing. The disclosure, the search,
the committee, and the review of drafts follow university documents and two secondary sources,
marked in the comments: 〔まとめ〕 for a vendor article and 〔論文〕 for a journal article. Where the
research found nothing (the docketing reminders, the screens, the shared portal with the firm, the
reward occasions, the goals' numbers), the comment says the modeling is the model's assumption.

## Modeling choices

- Periods are timers, not states. A deadline timer starts the work due by then (for example
  `FirstFeeDeadline` starts `PayFirstAnnuities`); an expiry timer starts the use case that records
  what the law makes happen (`ExamRequestDeadline` starts `DeemWithdrawnNoRequest`). The number of
  each period is in the timer's label, and its source in the comment.
- Fees are master data (`FeeSchedule`, Money columns), with the amounts in its comment. The amounts
  change by 政令 and the lifecycle only needs to know that a payment was made, so fees are not state.
  `Patent.surcharge` is the one fee column a rule reads: it is present after a late payment.
- The patent attorney is an actor, not an external system (`AdrAttorneyAsActor`), so the review of
  drafts and the responses to office actions show in the business flows.
- `PatentApplication` stays `examining` from the examination request to the decision, and the
  office action and its response are their own lifecycle (`OfficeAction`), so the response states
  are not multiplied by every fee and publication column. An office action arrives only for an
  application under examination (`UnderExamination`), and the IP department closes an office
  action's docket only after the examination is over (`ExaminationOver`). The filing takes the
  approved drafts: `DraftsApproved` guards it, it requires the application's drafting order, and
  it moves the order to `filed` in the same step, so one order cannot back two applications. The
  receipt records the number and date and leaves the order alone.
- `GrantEnds` states that a granted application ends registered or dismissed, under
  `.fair(RecordRegistration, RecordRegistrationDismissal)` (特許法66条, 18条). `verify` checks it
  on the cone of what it reads and of the two use cases (the application's status and fee
  columns and the patent record `RecordRegistration` opens; 4,105 states at 2 instances), not
  on the whole model, which explored 14 million states in 11 minutes without finishing (#59,
  #67).

## Known verification gaps

The state space limits what this model says and how far `verify` checks it (#51, condition 5).
Each figure is a cone size from `crates/rdra-ish-core/examples/lifecycle_measure.rs`.

- **`OfficeAction` is compared with TLC at 1 instance only.** The two guards that read the application
  (`UnderExamination`, `ExaminationOver`) put its state into the office action's cone, and the
  required link to the drafting order (`DraftsApproved`) adds the order and the disclosure. The
  cone passes 8,388,608 states unreduced at 2 instances and is 1,341,707 reduced by its
  independent sub-lifecycle (#74). `verify` runs TLC on the reduced cone (its
  invariants hold) and its BFS, under the cap of 8,388,608 states, decides whether an office action
  can get stuck at `open`, `drafted`, `overdue`, or `answered` (about 25 s and 0.8 GB in all). Before the cap
  was raised from 2,097,152, those four checks were listed under `not checked:`.
  `engine_equivalence` compares the office action's cones with TLC at 1 instance only
  (`VERIFY_FALLBACK`). The cones of `PatentApplication` and `Patent` (293,833 and 202,569
  reduced; the filing guard's link adds the drafting order to them) are under its cap of
  300,000 states and are compared at 2.

## Diagnostics left in place

`check` reports nothing. `lint` reports `api-contract-missing` for every API (no API carries an
HTTP contract, as in clinic-ops). A screen that is the only one of its BUC (`ExaminationRequestScreen`,
`FeePaymentScreen`, `RewardScreen`) is reached by a `navigates` from another BUC's screen, so
`lint` does not report `screen-no-transition` for it. A BUC file imports another BUC's module by
its `module` name, as `buc_inventor_reward.rdra` does with `import buc.registration.{FeePaymentScreen,
RecordRegistration}`. The file name does not matter: the module is `buc.registration` and the file
is `buc/buc_registration.rdra`.

`scripts/gen-clinic-ops-docs.py target/debug/rdra-ish patent-ops` generates `design-doc.md` and
`prd.md` from the model, in the same layout as clinic-ops.
