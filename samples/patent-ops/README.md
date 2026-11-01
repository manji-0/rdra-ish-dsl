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
  office action and its response are their own lifecycle (`OfficeAction`). One lifecycle with every
  response state times every fee and publication column passed the 4096-state cap of `check`. For
  the same reason, use cases that the 特許庁 starts (a notice, the annuity docket) do not read the
  application's state, and the filing does not move the drafting order: the filing receipt does.

## Diagnostics left in place

`check` reports nothing. `lint` reports `api-contract-missing` for every API (no API carries an
HTTP contract, as in clinic-ops) and `screen-no-transition` for three screens that are the only
screen of their BUC (`ExaminationRequestScreen`, `FeePaymentScreen`, `RewardScreen`). A screen
transition into them would come from another BUC's screen, and a BUC file cannot import another
BUC's module: `import buc.registration` looks for `buc/registration.rdra`, while the file that
declares `module buc.registration` is `buc/buc_registration.rdra`, the layout clinic-ops uses.

`scripts/gen-clinic-ops-docs.py target/debug/rdra-ish patent-ops` generates `design-doc.md` and
`prd.md` from the model, in the same layout as clinic-ops.
