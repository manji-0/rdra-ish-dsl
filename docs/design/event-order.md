# Event order: a raised mark that a triggered use case needs

<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../state-derivation.md#runs -->
<!-- derived-from ../language-reference.md#triggers-and-event-driven-buc-chaining -->

Design for #112, the 2.0 item of #107 and option 2 of #30. It was decided on paper, measured
with a throwaway prototype, and landed on `main` as the units of the [landing
plan](#landing-plan). It replaces the decision of 2026-11-01 in
[vision.md](../vision.md#decisions) ("Events and timers impose no order"), and the next
release is 2.0.0. This is the second round: an
adversarial review of the first found a guard stronger than the order implies, a sample
verdict misread as a defect, per-variant warnings that were false, an unmeasured cone
shape, and measurement errors. Each is corrected below and marked "round 2".

In 1.x `raises(UC, Ev)` and `triggers(Ev, UC2)` add no guard. `UC2` runs whenever its
required states and guards hold, also before `UC` ever ran
([state derivation](../state-derivation.md#actions)). The check never misses a violation,
and it reports runs the event order prevents (#30). After this change the system records,
per row, that the event was raised, and a triggered use case runs only on rows that
carry that record, and only after it.

## Decisions in one place

The eight questions of #112, each with its answer, and the four decisions the review added.
[Answers](#answers-to-the-eight-questions) gives the reasons and the numbers.

| # | Question | Decision |
|---|---|---|
| 1 | Boolean or counter; does a second raise mean a second run | A boolean per row that is set and never cleared (a latch). After the first raise the use case runs as 1.x let it, any number of times. A second raise changes nothing. |
| 2 | Which rows carry the mark; which rows the triggered use case runs on | The rows of the entities every raiser's last step binds. The triggered use case reads the mark on its own row of such an entity when it has exactly one row of it and that row can be the raiser's (rule 5). When it binds none, or binds the entity twice, or through a key the raiser does not use, it binds one raised row as an extra parameter, linked to its own rows by the one direct key between them when there is one, and otherwise any raised row. |
| 3 | Several use cases triggered by one event | One mark per `(entity, event)`, read by every triggered use case. Nothing consumes it, so nothing is shared out. |
| 4 | Timers | Unchanged. A timer is a free trigger. Its use case runs whenever `now` and its guards allow, and a use case a timer also starts needs no mark. |
| 5 | Events an external system raises | Free. The event may occur at any time, so the use case runs as in 1.x. An event with no `raises`, or raised by a use case with no action, is free too. |
| 6 | Deadlock and witness | The mark is not a view axis, so `states` keeps its shape. A use case that waits for a mark that never comes is `NeverRuns` with the mark as the blocking guard, and a state the triggered use case had to leave is a deadlock as today. A set mark is never a finding by itself. |
| 7 | Cones and the #74 reduction | The mark is a var of its entity. A cone that already holds the raiser (by its own writes) reads the mark exactly and the order holds there. A cone that does not hold the raiser drops the handler's mark guard instead of pulling the raiser in, because a mark only ever rises, so "possibly raised" is a sound superset; the use cases a view is about keep their marks exact. The #74 reduction sees the mark as any other var the raiser writes and the handler reads. |
| 8 | TLA+ | `<E>_raised_<Ev>`, a function from `<E>_Ids` to `BOOLEAN`, `FALSE` in `Init`; the raiser assigns `TRUE`, the triggered operator conjoins `= TRUE`. One printer, so BFS and TLC agree as before. |
| 9 | What "triggered" means (round 2) | A use case that a tracked event triggers runs only after that event, on its rows. There is no opt-out syntax. A use case that may also start on its own gets a second event, raised where it may start, and the docs say so. |
| 10 | A use case with several tracked events (round 2) | One action per event, `<UC>_by_<Ev>`, and the findings are per use case: it runs when any variant runs and takes an arm when any variant takes it. A variant that never runs while another does is an info naming the event. |
| 11 | The two coupled samples (round 2) | The samples are rewritten, not the language. insurance-claim-ops raises `EvSurveyReported` for the no-settlement path and `EvClaimTimeBarred` for a lapsed claim. food-permit-ops: keeps its model and its README says the ledger now moves only after the decision's event; at 2 instances the two deadlock questions of the `BusinessPermit` view stay undecided under the cap (they are decided at 1 instance by `check`), and whether to free the four ledger edges instead is open question 4. |
| 12 | Guards the order makes redundant | They stay. A latch says the event was raised on the row, not that the row is still in the state the raise left it in. |

The cost, measured on the samples at 2 instances, one run at a time on a machine that
was never idle ([Measurements](#measurements)): `verify` of clinic-ops gets faster (34 s to 18 s) and
patent-ops does not move; food-permit-ops decides every invariant and assert (`tlc_result: ok`) in 82 s with the
havoc and leaves the two deadlock questions of the permit ledger's view undecided at the
8,388,608-state cap; insurance-claim-ops does not decide its claim-side cones within
600 s under any form measured. Both are the cost of ordering a handoff between two
lifecycles that 1.x explored apart, and whether those samples free the handoff edges or
state the undecided findings is the one question left to the maintainer
([open questions](#open-questions-for-the-maintainer)).

## The data shape

<!-- dagayn: discusses crates/rdra-ish-core/src/lifecycle/types.rs::VarKind -->

Named first, as the rules below are written in its terms. All of it is plain data in
`lifecycle/types.rs` that `build` fills and the explorer and the TLA+ printer read.

```rust
pub enum VarKind {
    // ... existing variants ...
    /// Whether `event` was raised on this row. The last step of every use case that
    /// raises it sets it, the first step of every use case the event triggers reads it,
    /// nothing clears it, a delete does not reset it (as `Run`), and it is not a view axis.
    /// `Domain::Bool`, base `FALSE`, start `Base`.
    Raised { entity: EntId, event: String },
}

pub enum ParamRole {
    // ... existing variants ...
    /// A present row of a raised-mark entity the triggered use case does not otherwise
    /// bind, selected by the mark (as `Earlier` is selected by its run mark), and linked
    /// to one of the action's other params by the one direct key between the two
    /// entities when such a key exists.
    Triggered,
}

pub enum GuardWhy {
    // ... existing variants ...
    /// `<E>._raised_<Ev>` on the bound row is `true`: the event was raised on it.
    Raised { event: String },
}

pub enum ActionOrigin {
    UseCase {
        uc: UseCaseKey,
        buc: Option<BucKey>,
        step: Option<u8>,
        /// The tracked event this variant of the first step runs on, when the use case
        /// has several; `None` otherwise.
        trigger: Option<EventKey>,
    },
    Clock,
}
```

The names are `<E>._raised_<Ev>` in `states`, traces, and `StateCapReached`,
`<E>_raised_<Ev>` in TLA+, and `<UC>_by_<Ev>` for an action variant. `trace.rs` prints the var as
`raised <Ev>`. `ModelRef::Relation(i)` of the `triggers` line is the site of the guard,
and the `raises` line the site of the effect. A finding about a use case with variants
names the use case (`Runs_AssessDamage`, `Arm_AssessDamage_investigating_assessed`), never
a variant; `ActionOrigin::UseCase { uc, step }` is what groups the variants.

Nothing is model-wide. Every new var has one value per slot, as every var but `now` has,
so `layout`, `project`, `restrict`, `reduce`, the TLA+ names, and the BFS spread need no
new width case. The 21 `VarKind::Now` match sites stay as they are.

## The rules

Quotable as the 2.0 semantics. "Binds" means has a parameter on, in any role; "its row of
`E`" means a parameter in a role other than `Parent` ([roles](../state-derivation.md#actions)).

1. **Tracked and free events.** An event is *tracked* when every `raises` of it names a
   use case that has an action. An event with no `raises`, with an external-system
   raiser, or with a raiser that writes nothing is *free*. A timer is free.
2. **What runs freely.** A use case that a free event, a timer, or nothing triggers runs
   whenever its required states and guards hold, as in 1.x. `performs(Actor, UC)` adds
   no trigger and frees nothing.
3. **The mark.** For each tracked event `Ev` with at least one `triggers(Ev, UC)` whose
   `UC` has an action, the system has `<E>._raised_<Ev>` per slot of each *mark entity*
   `E` of the event: `false` at the start, `true` once a raiser ran on the row, never
   `false` again. A delete does not reset it (the slot never revives).
4. **Mark entities.** Let `C` be the entities that the last step of every raiser of `Ev`
   binds, in the first raiser's parameter order. The mark entities of `Ev` are the members
   of `C` that some triggered use case reads under rule 5. When `C` is empty (the raisers
   bind disjoint entities), the event is free and `check` reports `UnanchoredTrigger`
   (info) on each `triggers` line.
5. **How the triggered use case reads it** (round 2). The order says that the handler's
   run follows a raiser's run on that run's rows. Which of the handler's parameters is
   such a row is a modelling convention, and the guard may use only the parameters the
   convention names, so that it is never stronger than the order. For `triggers(Ev, UC)`,
   a parameter `p` of the first step of `UC` on a member `E` of `C` *corresponds* to the
   raiser's row of `E` when it is the step's only parameter on `E`, and either `p` is its
   row of `E`, or `p` is a `Parent` through a key that some raiser's last step also binds
   `E` through. The first step of `UC` reads the mark on:
   - every corresponding parameter, when it has one;
   - otherwise one extra `Triggered` parameter on the first member of `C` that one direct
     key links to a row `UC` writes or reads as a parent, with the owner guard of that
     key, as a condition's existing instance is linked ([Joins](../state-derivation.md#joins));
   - otherwise one extra `Triggered` parameter on the first member of `C`, independent
     of the use case's rows, and `check` reports `UnjoinedTrigger` (info) on the
     `triggers` line: any raised row enables the use case, as any present row satisfies
     an `UnjoinedGuardRead`.

   A parameter that does not correspond gets no guard. Two parameters on `E`
   (`relate(Transfer, Account).role(src)` and `.role(dst)`) say nothing about which one
   the event was about, and a parent through a key the raiser never binds (`dst` where
   the raiser bound `src`) is a different row, so requiring the mark on each would hide a
   run the order allows. The review's model
   (`hide.rdra`, [Measurements](#three-models-from-the-review)) is the proof. The round-1
   rule read the mark on every parameter of `E`.

   Each such parameter carries the guard `<E>._raised_<Ev>[p] = true`
   (`GuardWhy::Raised`). The later steps of a split use case carry nothing new, since
   they continue the run by its run marks.
6. **Raising.** The last step of each raiser sets `<E>._raised_<Ev>` to `true` on every
   parameter it has on a mark entity, including a `Parent` and an `Earlier` one. It has
   no guard for it, so a raiser never waits for a handler.
7. **One action per tracked event, one use case to the findings.** A use case that
   several tracked events trigger has one copy of its first step per event, named
   `<UC>_by_<Ev>`, each with that event's guards from rule 5. A use case one tracked
   event triggers keeps its name. A use case that a free trigger also starts has no
   variants and no mark guard (rule 2). The variants are one use case to every finding
   (round 2): the use case runs when any variant runs, and takes an arm when any variant
   takes it, so `Runs_<UC>` and `Arm_<UC>_<from>_<to>` are reported once, under the use
   case's name, and only when no variant runs or takes the arm. A variant that never runs
   while another does is an info naming its event.
8. **What the order means** (round 2). A use case a tracked event triggers runs only after
   the first raise on its rows, and not before, whatever actor performs it. A use case
   that in the domain also starts without that event states where it starts: a second
   `raises` on the use case that marks that point, with a second `triggers` to it. The
   language has no opt-out on `triggers`; dropping the use-case edge
   (`triggers(Ev, Buc)` alone) keeps the diagram and frees the use case, which is a model
   change a reviewer can see, not a modifier that hides one.
9. **What is not said.** The mark counts no events. Two raises on a row are one mark,
   and a triggered use case that ran once can run again without a new raise, as it can
   today. The order is "not before the first raise on these rows", nothing more.

### Checks and findings

- **Deadlock.** Unchanged, per present instance and state column. A row whose only way
  out is a triggered use case that waits for a mark is stuck, and `check` prints
  `guard: <Ev> was raised on the bound <E>` among what blocks it, as it prints a
  condition. A set mark is not a state model and is never a deadlock.
- **Witness and `NeverRuns`.** A witness trace contains the raiser before the triggered
  use case. A use case whose mark never holds is `NeverRuns` with that guard, naming
  the `triggers` line. A self-triggering use case, and an event cycle with no free
  entry, never start, and this is the finding they get.
- **Per use case, not per variant** (round 2). `targets.rs` merges the `RunsSoFar` of the
  variants of one `(uc, step)` before it settles them: fired if any fired, an arm taken if
  any variant's edge takes it, the first `NeverRuns` or `ArmNeverTaken` kept and renamed
  to the use case. `Subject::Runs` and `Subject::Arm` of the undecided list carry the use
  case's name for the same reason. The review's `arm.rdra` (a `Handle` that `Ev1` starts
  at `x` and `Ev2` at `z`, with an arm `x -> y .when(IsX)`) got
  `Arm_Handle_by_Ev2_x_y` in round 1 and gets nothing now; a copy in which no variant
  takes the arm gets `Arm_Handle_x_y`.
- **`after(UC).assert`.** Checked after the last step of each variant of rule 7, so once
  per run as today. #30's assertion holds (measured below).
- **Temporal properties.** Marks are vars like any other in a property's cone.
  `.fair(UC)` on a triggered use case makes each variant weakly fair, and `ENABLED` of
  a variant reads its mark, so the property's cone holds the raiser's writes to it and
  the raiser's reads with them (`property_cone` already keeps a fair action's reads).
- **Bounds line.** Unchanged. Nothing new is bounded, since the mark has no cap and
  the raiser no wait. A cone that dropped a mark guard says so in its `bounds:` line, as
  `cone_frees_slots` says an entity is unbounded.

### Cones and the reduction

- A mark belongs to its entity (`own_existence`, `existences`, `var_entity` treat
  `Raised` as `Run`). The exact cone of a handler reads the mark, so it pulls the raiser's
  reads, which are what the raiser's guards read and the existence of the entities it
  selects, transitively along a chain of events. In food-permit-ops the `BusinessPermit`
  view holds 2 entities today and 6 under that closure (`PermitApplication`, `Inspection`,
  `Disposition`, `SiteVisit`, `MonitoringPlan` join it through `EvPermitGranted`,
  `EvRenewalGranted`, `EvRevocationDecided`, `EvSuspensionDecided`, `EvDispositionDecided`,
  and `EvViolationFound`), which is the coupling its README avoided by moving the ledger
  on events instead of in the deciding use case.
- **Cone-local havoc** (round 2). A mark only ever rises, so a system in which the handler
  ignores the mark reaches a superset of the states of one in which it waits. The cone
  closure (`cone.rs`) therefore treats a mark read as *exact* only when every action that
  writes it is already in the cone by its own writes (or is kept), and otherwise leaves the
  mark out; `restrict_to` then drops the guards on it and, for a `Triggered` parameter,
  the parameter with its existence and link guards. For a closed cone nothing is dropped:
  the reduction's exactness test (`reduced_domain_cones_decide_what_the_cones_decide`,
  53 or more cones reduced with the reduced and unreduced verdicts equal) passes on the
  prototype with the marks off and with the latch and the havoc on (`tmp/r2/cargo-test-*.log`). The handler runs there as in 1.x. The
  cone of #30's assertion holds `ExecuteCertIssue`, which writes the order's status the
  assertion reads, so it stays exact and the assertion holds. The use cases an entity
  view is about (its `keep` set: every action with a written parameter on the entity)
  keep their marks exact whatever the cone holds, so a deadlock or `NeverRuns` of the
  entity is judged with the order. Without that exemption the ClaimCase view of
  insurance-claim-ops lost `Deadlock_ClaimCase_status_assessed`, because `CloseCase`'s
  raisers write only the claim ([Measurements](#the-havoc-on-the-samples)). What the
  havoc gives up is the precision of 1.x, in those cones, for the actions that reached
  the cone transitively: an invariant, an assert, or a property checked on a havoc cone
  can report a run the order prevents, as 1.x did, and never misses one; a witness or
  deadlock of the view's entity is exact; a `NeverRuns` of a transitively pulled handler
  is still decided on that handler's own entity views, which keep it exact.
- In the #74 reduction the raiser writes the mark and the triggered use case only reads
  it. An action that writes a T var is T, so where the triggered use case is T the mark
  is T and the raiser joins T only if it reads T; where the raiser reads only S it stays
  S and the mark is an observed var of S (`obs`), rendered through the `Rep_` table.
  Consuming the mark would make the triggered use case a writer and pull the raiser's
  whole lifecycle into T, which is what the measurement of the consume-once variant
  shows (clinic-ops loses every reduction).
- `restrict` and components need nothing new. An action's footprint already lists its
  params' entities and its guards' vars, so a raiser and a triggered use case that share no entity join
  one component through the `Triggered` param's entity, as a condition's existing
  instance joins them today.

### TLA+

| RDRA | TLA+ |
|---|---|
| A tracked event `Ev` with mark entity `E` | `<E>_raised_<Ev>`, a function from `<E>_Ids` to `BOOLEAN`; `Init` sets every id to `FALSE`; `TypeOK` lists it. |
| The last step of a raiser, on each of its parameters `p` on `E` | `<E>_raised_<Ev>' = [<E>_raised_<Ev> EXCEPT ![p] = TRUE]`; `UNCHANGED` elsewhere. |
| The first step of a triggered use case, on each parameter `p` of rule 5 | the conjunct `<E>_raised_<Ev>[p] = TRUE`. A `Triggered` parameter adds `<E>_exists[p] = "present"` and, when linked, `<Child>_owner_<Parent>[c] = p` or `= c`. |
| A use case with several tracked events | one operator per variant, `<UC>_by_<Ev>`, each in `Next`. |
| A cone that dropped a mark guard | the operator without the conjunct and, for a `Triggered` parameter, without that parameter; the var is not in the module. |
| A reduced cone whose mark is in S | `Rep_<E>_raised_<Ev>` and the operator over the class var, as for any observed var. |

The explorer and the printer read one `Action`, so `engine_equivalence` keeps
comparing BFS and TLC on every sample, and the #30 model is added to the samples for it.

## Answers to the eight questions

**1. Shape of the mark.** A latch, because consumption is what costs, and because a raiser
that waits for the handler hides violations. The prototype measured the shapes below on the
same models ([Measurements](#measurements)). At `states` bounds clinic-ops explores 2,405
states over its 39 targets today; 104,187 with consume once (43 times 1.2.0, 25 times the
latch), 156,103 with the counter (65 and 37 times), 612,659 with a nondeterministic clear
whose raiser waits (254 and 145 times), and 4,215 with the latch. Every consuming shape
also removes the #74 reduction from every clinic-ops cone (the `CarePlan` view goes from
13 reduced states to 41,868, unreduced), and at `verify` bounds the consume-once `verify`
of clinic-ops was stopped after 393 s and 11.6 GB still in its BFS pass, where 1.2.0
finishes in 34 s. The latch keeps the reduction (clinic-ops reduced states 924 to 858)
because the handler never writes the mark.

On soundness (round 2; round 1 had this wrong): what hides a violation is a raiser that
waits while the mark is set, not the shape of the clear. The review's `wait.rdra` (a
`Bump` that moves a ticket `a -> b -> c` and raises `EvBumped`, an `Audit` the event
starts that needs the ticket still at `a`, and `forbidden(Ticket, status == c)`) shows
it: under consume once and under the nondeterministic clear with a waiting raiser the
mark stays set after `a -> b`, `Audit` can never clear it, the second `Bump` waits forever,
`c` is unreachable, and `check` reports a spurious `Deadlock_Ticket_status_b` and
`Witness_Ticket_status_c` instead of `Forbidden_Ticket_0`. Under 1.2.0, the latch, the
saturating raiser, and the nondeterministic clear with a saturating raiser the violation
is reported. The nondeterministic clear with a saturating raiser is the sound abstraction
of a queue; it is measured below for the record (round 1 measured only the waiting
variant and called it "nondet"). The latch misses nothing today's semantics explores
after the raise, and the issue's question "does a second raise mean a second run" gets
the answer "the second run was always allowed, and still is".

**2. Rows.** Per row, never model-wide, because a model-wide mark does not fix #30 at
`verify`'s bounds. With two certificate orders, `ExecuteCertIssue` on order 1 sets the
mark and `IssueCert` on order 2, still `requested`, consumes it, and
`AfterAssert_IssueCert_0` fires as it does today. The per-row prototype passes #30 at 1
and at 2 instances (TLC `ok`). The rows are the raiser's because the event is about the
run that raised it, as a run mark is about the rows its steps bound
([Runs of a split use case](../state-derivation.md#runs)); marking every entity the
raisers share, not one, lets the triggered use case tie each of its own rows to the
raiser's. The intersection over raisers is needed where several use cases raise one
event (`EvAppointmentScheduled` from `BookAppointment` and `ScheduleFollowUpVisit`): a
mark on an entity one raiser does not bind would never be set by that raiser. The
`Triggered` parameter exists because 16 of the 26 tracked trigger edges in the samples
share no entity between raiser and handler (`CancelAppointment` moves an `Appointment`,
`SendAppointmentNotice` creates a `Notification` under the `PatientAccount`); 12 of
them are one key apart, 3 are further (`ReviewLabResult` on a `LabResult` four keys from
the `PatientAccount` that `NotifyPatientResult` reads, `RecordViolation` on a `SiteVisit`
two keys from the `BusinessPermit`), and one, `RenewPermit` to `ExtendPermitTerm`, has
two keys between `PermitApplication` and `BusinessPermit` and no `.role` to pick one.
Those 4 are `UnjoinedTrigger`, ordered, with any raised row enabling the handler. The
correspondence rule of rule 5 moves no sample edge: every handler that binds a raiser's
entity binds it once, as its row or through the raiser's key (measured, the edge table
below is unchanged from round 1). A longer join (an `.along` path) is a possible
follow-up, not part of this change.

**3. Several handlers.** One mark per `(entity, event)` is enough because nothing clears
it. `EvClinicalOrderReviewed` starts `NotifyCriticalResult` and `NotifyPatientResult`;
both read `ClinicalOrder._raised_EvClinicalOrderReviewed`. The "each once" and "any
one" readings of the issue only differ for a consumed mark.

**4. Timers.** Unchanged. A timer has no raiser to anchor a mark on, and its use case is
already placed by `now` and its guards
([Clock](../state-derivation.md#actions)). A use case that both a timer
and a tracked event start (`RecordClaimReturn`, by `EvClaimReturned` and
`ReturnDownloadCheck`) is free, since the timer path would otherwise be cut.

**5. External systems.** Free, because the model has no action to set the mark and a
synthesized "the event arrives" action would add a step and states without adding
precision, because the event can arrive at any time, so the handler can run at any time. In
clinic-ops 12 of the 24 trigger edges are of this kind (`ClaimsReviewOrganization`,
`OutsourcedLabSystem`). An event raised both by a use case and by an external system is
free for the same reason.

**6. Deadlock and witness.** The mark is not a view axis, as a run mark is not, so the
`states` output keeps its patterns and the witness traces grow by the raiser. A use case
that cannot run before its mark is reported by the existing `NeverRuns` scan with the
mark guard, and a state only a triggered use case leaves is reported by the existing
deadlock scan with the mark guard among what blocks it. Measured, insurance-claim-ops
reports `Deadlock_ClaimCase_status_assessed` with the 12-step path in which a claim is
time-barred and so never raises the event that closes its case, and
`Deadlock_ClaimCase_status_investigating` for the first-party path on which no
settlement event ever starts `AssessDamage`. Round 1 called both model defects. The
review corrected the second (round 2): the README (its "An OR is two guarded arms"
paragraph) and `buc_investigation.rdra` describe a path on which the case is assessed
with no settlement, so under 1.x `AssessDamage` was meant to start there on its own, and
the deadlock is the change of meaning of `triggers` from "also started by the event" to
"only after the event" (rule 8) applied to a valid model. The decision is to keep the 2.0
meaning and rewrite the sample: `ReportSurvey` raises `EvSurveyReported`, which starts
`AssessDamage` beside the two settlement events, and the guards still pick the arm.
The first deadlock is a defect the order exposed (a time-barred claim leaves its case
assessed forever) and the sample now raises `EvClaimTimeBarred` from `TimeBarClaim`,
which starts `CloseCase`. With both, `check` of the rewritten sample passes under the
latch at 1 instance with no new warning (measured). No new finding kind is needed.

**7. Cones.** The mark is its entity's var, so it is in exactly the cones that read the
handler's writes or the mark itself, and (round 2) its raiser's reads join only the cones
that already hold the raiser. Measured at `states` bounds under the exact closure, the
`BusinessPermit` view of food-permit-ops grows from 10 vars and 64 states to 33 vars and
5,874 states, since `RevokePermit` and `SuspendBusiness` now read the `Disposition`
rows; the `ClaimCase` view of insurance-claim-ops from 13 vars and 159 states to 28
vars and 2,197; clinic-ops' largest view from 752 to 1,314 states. The havoc keeps
clinic-ops below 1.2.0 (2,295 states) and cuts food-permit-ops' growth by two thirds, and
in insurance-claim-ops it cannot help where the raiser writes the entity the cone is
about ([Measurements](#the-havoc-on-the-samples)). The reduction keeps working. The `CarePlan` view of clinic-ops
reduces 1,314 states to 15 with a 61-state, 11-class sub-lifecycle. In the reduction the
mark is observed (`obs`) when its raiser is S and its handler T, which is the common
case; the consume-once variant put every mark in T because the handler wrote it, and that
is why it lost the reduction.

**8. TLA+.** A per-id `BOOLEAN` function, printed by the same `effects` and guard code
that prints a run mark, so no new printer path. BFS and TLC agree by construction, and
`verify --backend tlc` on the #30 model at 2 instances prints `tlc_result: ok` under
the prototype where 1.2.0 prints a counterexample.

## Measurements

Everything below comes from the throwaway prototype on the branch
`proto/112-event-order-r2` (three commits on `main` after #111, not for merging); its
`tmp/` holds the #30 model, the review's three models, and the scripts that produced each
table (`tmp/r2/check-models.sh`, `tmp/r2/checks-all.sh` with `tmp/run-checks.sh` and
`tmp/summarize.py`, `tmp/r2/edges.sh`, `tmp/r2/tim.sh` and `tmp/r2/tim-all.sh`,
`tmp/r2/measure-all.sh` with `tmp/measure-diff.py`, `tmp/r2/rw-insurance.py`).
The prototype reuses `VarKind::Run` with a synthetic name for the mark, `ParamRole::Earlier`
for the `Triggered` parameter, and environment knobs for the variants (`RDRA_MARK_CONSUME=latch`
for the design, `RDRA_MARK_HAVOC=2` for the havoc, `RDRA_MARK=off` for 1.2.0 behavior in
the same binary); it reaches `check`, `states`, `verify`, and the TLA+ export, so the
numbers are of the real engines. It keeps one mark per `(entity, event, handler)` where
rule 3 keeps one per `(entity, event)` (three clinic-ops events have two handlers), so its
state counts are an upper bound for the design. Release builds on `main` at `1d4e2924`
(1.2.0 plus #111). Machine: 18 cores, 128 GB. The variants:

| Variant | Mark | Raiser | Handler |
|---|---|---|---|
| base | none (1.2.0) | | |
| latch (chosen) | boolean | sets | reads |
| latch + havoc (chosen) | boolean | sets | reads where its cone holds the raiser, else unguarded |
| consume | boolean | waits while set, then sets | reads and clears |
| counter | 0..2 | waits at 2, else adds one | reads and subtracts one |
| nondet, waiting | boolean | waits while set, then sets | reads, then clears or not |
| nondet, saturating (round 2) | boolean | sets | reads, then clears or not |
| shared-only | latch on shared entities only, no `Triggered` parameter | | |

### #30

| Run | 1.2.0 | latch | latch + havoc |
|---|---|---|---|
| `check` (1 instance) | `error[AfterAssert_IssueCert_0]`, trace RequestCert, IssueCert | `OK` | `OK` |
| `check` (2 instances) | the same error | `OK` | `OK` |
| `verify` (2 instances, TLC) | `tlc_result: counterexample`, `AfterAssert_IssueCert_0` | `tlc_result: ok` | `tlc_result: ok` |

The consume variant passes both too. A model-wide boolean would pass only the first
(see answer 2).

### Three models from the review

`check` at 2 instances (`tmp/r2/check-models.sh`), round 2 of the prototype.

| Model | What it tests | 1.2.0 | round 1 (latch) | round 2 (latch) | round 2 (latch + havoc) |
|---|---|---|---|---|---|
| `hide.rdra`: `VerifyAccount` raises `EvVerified`; `SendTransfer` creates a `Transfer` with `.role(src)` and `.role(dst)` to `Account`; `forbidden` "no transfer to an unverified `dst`" | rule 5 on a handler with two parameters of the mark entity | `error[MultiForbidden_Transfer_0]` | `OK` (hidden: the mark was required on `src` and `dst`) | the error, with `Account[2]` unverified as `dst` (`Account` is `UnjoinedTrigger`) | the error |
| `wait.rdra`: `Bump` moves `a -> b -> c` and raises `EvBumped`; `Audit` needs `status == a`; `forbidden(status == c)` | a raiser that waits for the handler | `error[Forbidden_Ticket_0]` | the error (latch); consume and waiting nondet: `Deadlock_Ticket_status_b`, `Witness_Ticket_status_c`, no forbidden | the error, plus `warning[Runs_Audit]` (true: the mark rises only after `a -> b`) | the same |
| `arm.rdra`: `Handle` by `Ev1` (raised at `x`) and `Ev2` (raised after `x -> z`), arm `x -> y .when(IsX)` | per-variant findings | `OK` | `warning[Arm_Handle_by_Ev2_x_y]` (false: `Handle` takes `x -> y` by `Ev1`) | `OK` | `OK` |
| `arm-neg.rdra`: `arm.rdra` with `Ping` moving `x -> z` too, so no variant takes `x -> y` | the aggregation's negative control | `OK` | | `warning[Arm_Handle_x_y]`, `error[Witness_Item_status_y]` | the same |

### `check` verdicts, 31 sample roots

Under the latch, with and without the havoc, 29 of 31 roots keep their exit status,
error names, and warning names at 1 instance (`tmp/r2/checks-all.sh`). The two that
change, before the sample rewrite:

| Sample | 1.2.0 | Latch (round 2) | Why |
|---|---|---|---|
| insurance-claim-ops | `check` ok | exit 1: `Deadlock_ClaimCase_status_assessed`, `Deadlock_ClaimCase_status_investigating`; warning `Arm_AssessDamage_investigating_assessed` (round 1 printed it twice, once per variant) | `assessed`: the path RelayAccidentReport, ..., SendClaimForms, TimeBarClaim, AssessDamage leaves a time-barred claim that never raises `EvClaimPaid`, `EvDenialNotified`, or `EvClaimWithdrawn`, so `CloseCase` never starts: a defect. `investigating`: `AssessDamage` is started only by `EvSettlementAgreed` and `EvSettlementEnded`, and a first-party claim has no settlement: the meaning change of rule 8 on a valid path. After the rewrite (answer 6) `check` is ok under the latch, with and without the havoc. |
| food-permit-ops | 2 warnings | 3 warnings: `Runs_ExtendPermitTerm` added | Its only raiser `RenewPermit` never runs at 1 instance in 1.2.0 either (`Runs_RenewPermit`, the renewal needs a second `PermitApplication` slot), so now the handler cannot either. The instance bound, not the order. At 2 instances both run. |

The `performs`-frees variant keeps `Deadlock_ClaimCase_status_assessed` (`CloseCase` has
no `performs`) and leaves 2 of the 26 tracked edges marked. The waiting nondet variant adds
`StateCapReached_CarePlan` and `StateCapReached_PatientMessage` on clinic-ops at the
default 65,536 cap, so it fails `check` where the others pass.

The tracked edges per domain sample, from the prototype's dump (`RDRA_MARK_DUMP=1`,
`tmp/r2/edges.sh`), unchanged by the correspondence rule:

| Sample | `triggers(Ev, UC)` edges | free (external, timer, no action) | tracked, handler binds a raiser entity | tracked, `Triggered` parameter linked by one key | tracked, `Triggered` parameter unjoined |
|---|---|---|---|---|---|
| clinic-ops | 24 | 15 | 3 (`OpenEncounter`, `CreateCarePlan`, `CreateCharge`) | 4 (`SendAppointmentNotice` by each of its three events, `SendIntakeForms`) | 2 (`NotifyCriticalResult`, `NotifyPatientResult`) |
| patent-ops | 18 | 15 | 3 | 0 | 0 |
| food-permit-ops | 8 | 2 | 2 | 2 | 2 (`ExtendPermitTerm`, `ProposeDisposition`) |
| insurance-claim-ops | 15 | 7 | 2 (`StartSubrogation`, `SendClaimForms`) | 6 (`CloseCase` by each of its three events, `FileComplaint`, `AssessDamage` by each of its two) | 0 |
| insurance-claim-ops, rewritten | 17 | 7 | 2 | 8 (`CloseCase` by four, `AssessDamage` by three, `FileComplaint`) | 0 |
| ec-site | 1 | 1 | 0 | 0 | 0 |

### States at `check` and `states` bounds

Sum and largest over all targets (entity views, invariants, after-asserts) of each
sample, 1 instance, Int 0..5, cap 1,000,000, from `lifecycle_measure`. "reduced" is the
sum with each cone reduced as `verify` explores it. Round 2 re-ran base, latch, and
latch + havoc, and added the saturating nondet; the consume, counter, waiting-nondet, and
shared-only columns are round 1's (same code paths, same bounds).

| Sample | base sum / max / reduced | latch, exact | latch + havoc (chosen) | nondet, saturating | consume | counter | nondet, waiting | shared-only |
|---|---|---|---|---|---|---|---|---|
| clinic-ops (39 targets) | 2,405 / 752 / 924 | 4,215 / 1,314 / 858 | 2,295 / 514 / 838 | 42,791 / 18,616 / 41,459 | 104,187 / 41,868 / 100,770 | 156,103 / 62,792 / 150,958 | 612,659 / 266,748 / 599,882 | 1,565 / 392 / 738 |
| patent-ops (23) | 6,189 / 2,107 / 1,995 | 6,189 / 2,107 / 1,995 | 6,189 / 2,107 / 1,995 | 45,243 / 16,621 / 12,231 | 6,373 / 2,107 / 2,961 | 6,373 / 2,107 / 2,961 | 45,575 / 16,621 / 12,239 | 6,189 / 2,107 / 1,995 |
| food-permit-ops (22) | 2,919 / 901 / 2,189 | 21,432 / 5,874 / 15,822 | 6,333 / 4,759 / 6,292 | 116,226 / 36,618 / 94,873 | 24,882 / 6,738 / 20,952 | 24,882 / 6,738 / 20,952 | 119,502 / 36,618 / 98,149 | 3,306 / 631 / 1,354 |
| insurance-claim-ops (22) | 34,905 / 3,335 / 3,111 | 39,407 / 3,997 / 23,060 | 32,356 / 3,997 / 19,083 | 97,951 / 14,013 / 97,951 | 39,250 / 4,057 / 39,250 | 39,250 / 4,057 / 39,250 | 100,202 / 14,317 / 100,202 | 33,749 / 3,219 / 3,139 |
| the other 27 roots | unchanged | unchanged | unchanged | unchanged | unchanged | unchanged | unchanged | unchanged |

The base and exact-latch columns reproduce round 1's to the state (`tmp/r2/measure/*-states-*.txt`).

Patent-ops is unchanged under the latch because each of its three handlers already
runs only from the state the raiser leaves: `RequestDrafting` under `DecidedToFile`,
`FileApplication` under `DraftsApproved` and from `approved`, `AcceptDisclosure` from
`submitted`. The mark cuts nothing there. Insurance-claim-ops' reduced sum grows
from 3,111 to 23,060 under the exact latch because the `Claim` views lose their reduction: the
case's side now reads the claim's marks and the claim's side reads the case, so no side is
independent; its unreduced sum barely moves. Shared-only is smaller than base because it
orders without pulling any new entity into a cone, and it orders 10 of 26 edges.
Under the havoc (mode 2) clinic-ops is smaller than 1.2.0 at these bounds (2,295 against
2,405), because the ordered handlers cut states and the dropped marks pull nothing;
food-permit-ops keeps a third of the exact latch's growth (6,333 against 21,432) and
insurance-claim-ops most of it (32,356 against 39,407, the reduced sum 19,083 against
23,060), for the reason given under the `verify` table: its raisers are in the claim's
cones by their own writes.

### States at `verify` bounds

2 instances, Int 0..5, cap 1,000,000 per cone in `lifecycle_measure` (`verify` itself
caps at 8,388,608). "capped" counts the targets whose unreduced cone passed the cap
(round 2: counted from the logs by `tmp/measure-diff.py`; round 1's column did not
reproduce from its logs and is withdrawn).

| Sample (targets at 2 instances) | base: unreduced sum / reduced sum / capped targets (distinct cones) | latch, exact | latch + havoc (chosen) |
|---|---|---|---|
| clinic-ops (40) | 8,651,874 / 2,783,706 / 4 (4: `Charge`, `EligibilityCheck`, `Prescription`, `Inv_Prescription_4`) | 8,169,020 / 1,506,215 / 5 (5: `CarePlan`, `Charge`, `Notification`, `PatientMessage`, `Prescription`) | 8,169,020 / 1,459,637 / 5 (the same five) |
| patent-ops (24) | 8,662,877 / 4,085,041 / 7 (5) | the same | the same |
| food-permit-ops (27) | 3,107,636 / 2,228,244 / 1 (1: `PermitApplication`) | 22,000,263 / 22,000,263 / 22 (13) | 4,096,152 / 4,089,812 / 3 (3: `BusinessPermit`, `Disposition`, `AfterAssert_SuspendBusiness_0`) |
| insurance-claim-ops (33) | 15,705,600 / 7,705,574 / 15 (9: `Claim`, `Payment`, `Subrogation` and six invariants or asserts of the claim) | 28,001,201 / 28,001,201 / 28 (14) | 17,535,334 / 17,535,334 / 17 (11: `Claim`, `ClaimCase`, `Complaint`, `Payment`, `Subrogation` and six of the claim's) |
| insurance-claim-ops, rewritten (33) | the same as above (the marks are off) | 28,001,201 / 28,001,201 / 28 (14) | 17,639,914 / 17,639,914 / 17 (11) |

Round 1's column did not reproduce from its logs and is withdrawn; the review's own count
of those logs (clinic 4 and 5, patent 5 and 5, food 1 and 14, insurance 11 and 16 for base
and latch) was of a build before #111 and is not the reference either. Round 2 counts from
`tmp/r2/measure/*-verify-*.txt` with the parser in `tmp/r2/vtable.py`: targets, with the
distinct cones in parentheses, properties excluded. The unreduced sums are capped at 1,000,000 per target and
so say only how many targets passed the cap; the reduced sum is the number `verify`'s
BFS pass explores when the reduction applies (clinic-ops, patent-ops), and equals the
unreduced sum where no cone reduces (food-permit-ops and insurance-claim-ops under the
order, which is the cost named in answer 7). Patent-ops is identical in every column
because its three handlers already ran only from the raiser's target state.

Under the exact latch clinic-ops shrinks: `OpenEncounter` waits for the check-in, so the
`Appointment` view halves (188,699 to 97,139 states) and most cones with it; the three
notification views grow (`Notification` 7 to over 1,000,000 unreduced, 8,723 reduced;
`PatientMessage` 381 to 197,719 reduced, 151,141 with the havoc) because they now hold
the appointment and lab lifecycles that raise their events (the per-view lines of
`tmp/r2/measure/{base,latch,havoc2}-verify-clinic-ops.txt`). Food-permit-ops and insurance-claim-ops grow because
the ledger reads the decision and the case reads the claim, and both sides read each
other, so no side is independent and the #74 reduction has nothing to split.

The havoc in mode 2 brings food-permit-ops back to 3 capped targets (every invariant and
all but one assert decide; `BusinessPermit` and `Disposition` are views, whose own use
cases keep their marks exact, and `AfterAssert_SuspendBusiness_0` keeps `SuspendBusiness`
with its mark). It barely moves insurance-claim-ops (28 to 17 capped targets, 11 cones)
for a reason the first round did not see: a cone that holds the raiser *by its own
writes* keeps the mark exact, and in every cone of the claim's side the raisers
(`PayClaim`, `NotifyDenial`, `WithdrawClaim`) are present because they write
`Claim.status`. `CloseCase` then reads the claim's mark inside the claim's cones, the
case's side reads the claim's side, and the #74 reduction that decided those cones in
1.2.0 (the `Claim` view, 754,247 reduced states from over 1,000,000) has no independent
side left. The cost is not the transitive pull of the raiser but the loss of the
reduction, and no havoc keyed on the raiser's presence can restore it. A third form, which drops the marks of every use case but the view's own even when the
raiser is present (`RDRA_MARK_HAVOC=3`), was tried for that reason and rejected: it frees
the handlers in the other views, so clinic-ops loses its speed-up (34 s, as 1.2.0),
food-permit-ops caps the same `BusinessPermit` view, and insurance-claim-ops' BFS pass
does not finish in 600 s (the free handlers add states faster than the dropped marks
remove them).

### `verify --deny-undecided`, wall time and memory

Release binaries, `tlc` from the nix devShell on `PATH`, 2 instances, the default caps,
`/usr/bin/time -l` for the wall time and the peak resident set, `uptime` recorded before
each run, runs strictly one after another with nothing else of ours running, a 600 s
watchdog that kills the run's own process group (`tmp/r2/tim.sh`, `tmp/r2/tim-all.sh`;
round 1's script killed only direct children and left TLC alive, overwrote its logs, and
ran under load 13 to 19). One collision remains in round 2: `tim.sh` names a log by the
sample's basename, so the mode-3 run of the rewritten insurance copy overwrote the log of
the unrewritten one under the same label (`campaign3.tsv` keeps both rows, computed before
the overwrite; the mode-3 insurance cell below is the unrewritten sample's row). The 1-minute load average before each run is in the table;
it was 1.7 to 4.7 for the first seven rows and rose to 7 to 17 during the later ones, because one process of another session held one core at 100 percent throughout and other sessions started work during the campaign; the machine was never idle, and the numbers of the later rows are upper bounds. One run per cell; the spread between the two base runs of clinic-ops in
round 1 was 34 s and 41 s under load, so differences under about 20 percent are noise.

| Sample | 1.2.0 | latch, exact | latch + havoc (chosen; the prototype's mode 2) | havoc in every cone (mode 1, rejected) | havoc of every use case but the view's own (mode 3, rejected) |
|---|---|---|---|---|---|
| clinic-ops | 34 s, 1,297 MB, every check decided, `tlc_result: ok` (load 1.7) | 20 s, 627 MB, ok (2.0) | 18 s, 496 MB, ok (2.2) | not run | 34 s, 1,293 MB, ok (2.8) |
| patent-ops | 24 s, 739 MB, ok (2.4) | 24 s, 769 MB, ok (3.6) | 24 s, 786 MB, ok (4.7) | not run | 24 s, 718 MB, ok (5.0) |
| food-permit-ops | 18 s, 602 MB, ok (4.1) | killed at 600 s (3.7): the BFS pass left 15 findings undecided (the `BusinessPermit`, `ChangeNotice`, `Disposition`, `Inspection`, `PermitApplication`, and `SuccessionNotice` cones at the 8,388,608 cap), TLC still running | 82 s, 2,934 MB, `tlc_result: ok`, exit 1: 2 findings undecided, both deadlock questions of the `BusinessPermit` view, whose cone passed the cap (8.8) | 10 s, 736 MB, ok, every check decided (10.6) | 85 s, 2,918 MB, ok, the same 2 `BusinessPermit` findings undecided (5.7) |
| insurance-claim-ops | 52 s, 1,179 MB, ok (5.1) | killed at 600 s (5.3): 29 findings undecided (the `Claim`, `ClaimCase`, `Complaint`, `Investigation`, `Payment`, `Settlement`, and `Subrogation` cones at the cap), TLC still running | killed at 600 s (7.6): 22 findings undecided (`Claim` 9, `ClaimCase` 7, `Complaint` 4, `Payment`, `Subrogation`), TLC still running on 13 specs | killed at 600 s (12.9): 11 findings undecided (`Claim` 9, `Payment`, `Subrogation`); `Deadlock_ClaimCase_status_investigating` reported, `Deadlock_ClaimCase_status_assessed` lost | killed at 600 s (5.1) with the BFS pass still running |
| insurance-claim-ops, rewritten | 53 s, 1,182 MB, ok (17.0) | not run (the exact closure is a superset of the unrewritten one's) | killed at 600 s (9.5) | killed at 600 s (9.1) | killed at 600 s |

Clinic-ops gets faster because `OpenEncounter` now waits for the check-in and the
appointment cones halve, and the havoc trims it further by dropping the notification
handlers' marks in the cones that lack the appointment lifecycle. Patent-ops does not
move, as its states did not. Food-permit-ops with the havoc decides its 14 invariants and
2 asserts (TLC `ok`) and leaves the two deadlock questions of the `BusinessPermit` view
undecided, because that view's own use cases keep their marks and pull the decision
chain (27 vars over `BusinessPermit`, `Disposition`, `Inspection`, `PermitApplication`,
with no independent side to reduce, since the disposition belongs to the permit and the
renewal application points at it); mode 1 decides it in 10 s by freeing those use cases
too, at the price answer 7 names. Insurance-claim-ops decides nothing new within 600 s
under any form, for the reason under the `verify` table: its raisers are in the claim's
cones by their own writes, so the marks there are exact and the #74 reduction, which is
what decided those cones in 1.2.0, is lost. The rewritten sample changes the verdicts at
1 instance (the two deadlocks are gone) and not the cost at 2. The limiter in every
killed run is the BFS pass, one process on one core at 100 percent (`ps` during the
runs), exploring cones of over 8,388,608 states; TLC never started in them. Memory
peaks at 2.9 GB (food-permit-ops with the havoc), well under the machine's 128 GB, so
none of the runs was memory-bound.

### The havoc on the samples

`check` at 1 instance on the 31 roots gives the same verdicts with the havoc as without
it (`tmp/r2/checks-all.sh`: the exit codes, error names, and warning names of every root
agree between the exact latch and the havoc). The dump (`RDRA_MARK_DUMP=1` prints
`havoc: <action> drops <mark>` per cone) shows where it acts on insurance-claim-ops: the
three `CloseCase` variants drop the claim's marks in the cones of the `Complaint`,
`Payment`, and `Subrogation` views and of the invariants that read none of the case,
and `FileComplaint` drops `EvDenialNotified`'s mark in the `Complaint` view.

The exemption for the view's own use cases is what keeps the findings. With the havoc in
every cone (`RDRA_MARK_HAVOC=1`) `CloseCase` also drops its marks in the `ClaimCase`
view, whose cone lacks `PayClaim`, `NotifyDenial`, and `WithdrawClaim` (they write only
the claim), so the view sees `CloseCase` as always able to run from `assessed` and
`Deadlock_ClaimCase_status_assessed`, the real defect, is lost; the `investigating`
deadlock survives because the settlement use cases write `ClaimCase.status` and so are
in that cone. With the exemption both are reported, as the table of `check` verdicts
shows.

At 2 instances the havoc decides what the exact latch could not where the handler's
cone lacks the raiser (every invariant and assert of food-permit-ops, in 82 s), and
nothing where the raiser is present by its own writes (the claim's cones of
insurance-claim-ops) or where the view's own use cases pull the raiser (the
`BusinessPermit` view). What remains undecided in the two samples is therefore not a
cone shape the design can change without giving up the order on those edges; the two
forms that would decide them, the havoc in every cone and the removal of the handoff
edges, both make the order disappear exactly where #112 wants it. That trade is open
question 4.

### Sample guards that the order makes redundant

The guards on triggered use cases that read exactly the raiser's target state:
`DecidedToFile` on `RequestDrafting` and `DraftsApproved` on `FileApplication`
(patent-ops), `ChargeOfSignedEncounter` on `CreateCharge` (clinic-ops), `ClaimPaid` on
`StartSubrogation` (insurance-claim-ops), `DispositionDecided` on `PublishViolation`
(food-permit-ops). Measured on patent-ops with the two guards dropped: `check` passes
under 1.2.0 and under the latch at 1 instance; the 2-instance `verify` of that copy
was not run. A latch
does not replace them. It says the event was raised on the row, not that the row is
still in the state the raise left it in (an `InventionDisclosure` decided and then
withdrawn keeps its mark). The guards stay, as the language reference already advises
("the guard also documents the precondition the event handler relies on"), and the
README passages that cite #30 for them are rewritten to say that.

## Why the cones stopped reducing (round 3)

<!-- derived-from #cones-and-the-reduction -->
<!-- derived-from #verify---deny-undecided-wall-time-and-memory -->

A diagnosis of the 24 findings that `verify --deny-undecided` at 2 instances leaves
undecided under the chosen form (2 on food-permit-ops, 22 on insurance-claim-ops before its
rewrite, 21 after it), measured on the prototype at `8fd48331` plus one env-gated dump
(`RDRA_SPLIT_DUMP=1` in `reduce.rs`, which prints each cone's split: the anchor, T, S, `obs`,
and per T action the entities it writes and the T vars and marks it reads) and two example
binaries (`rdra-ish-emit/examples/view_cone_tla.rs`, which prints an entity view's cone as
TLC checks a cone, and `rdra-ish-core/examples/symmetry_count.rs`). Release builds, one run
at a time, `uptime` recorded before each (the 1-minute load was 3.0 to 5.9). The findings,
their kinds, and their cones are in the table; the mechanism, the timing, the soundness
argument for a two-phase `verify`, and the TLC and symmetry measurements follow it.

### The 24 findings

Every undecided finding is a deadlock question ("can some row get stuck at this value") or
an arm question of an entity view. None is an invariant, a `forbidden`, a `required`, an
`exclusive`, a `check`, a quantifier, a `MinCard` or `Unique`, an `after(..).assert`, or a
temporal property: with the havoc those all decided on both samples (`tlc_result: ok` on
food-permit-ops in 82 s; on insurance-claim-ops TLC was still running on 13 specs when the round-2 run was killed at 600 s, and the stamped 30-minute run of this round was still in its BFS phase when this section was written; its log is `d112-tv-havoc2-ins.log` in the job directory). A view's findings are
decided by the BFS pass on the view's cone, and that cone holds the whole coupled lifecycle.
The columns: the cone's vars and entities at 1.2.0 and under the latch with the havoc
(`lifecycle_measure` at `verify` bounds with `RDRA_CONE_VARS=1`, diffed); which
`triggers` edge brought each new var; whether the raiser is in the cone by its own writes,
so the havoc did not apply; and the #74 split at 1.2.0 and now, with the read that joined
the former S side to T.

| # | Finding | Kind | Cone (entities) | Vars 1.2.0 → now | What pulled the new vars in | Raiser in the cone by its own writes | #74 split at 1.2.0 → now |
|---|---|---|---|---|---|---|---|
| 1 | `Deadlock_Claim_status_guided` | deadlock | Claim view: Claim, ClaimCase, Investigation, Payment, Settlement (same 5 at 1.2.0) | 25 → 33 | the 8 marks: `Claim._raised_<Ev>` for the four `triggers(Ev, CloseCase)` edges, `ClaimCase._raised_EvInitialResponded` (`SendClaimForms`), `Settlement._raised_EvSettlement{Agreed,Ended}` and `Investigation._raised_EvSurveyReported` (`AssessDamage`) | yes: `PayClaim`, `NotifyDenial`, `WithdrawClaim`, `TimeBarClaim` write `Claim.status` | T = Claim, Payment (12 vars), S = ClaimCase, Investigation, Settlement (13 vars, 63,823 states, 443 classes), 754,247 reduced states → S empty (all 33 vars in T). `CloseCase_by_<Ev>` writes ClaimCase and reads `Claim._raised_<Ev>[p]`, `Claim._exists[p]`, `Claim._owner_ClaimCase[p]` on its `Triggered` parameter; all three are T vars (Claim is the anchor), so the case's side joins T |
| 2 | `Deadlock_Claim_status_submitted` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 3 | `Deadlock_Claim_status_completed` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 4 | `Deadlock_Claim_status_decided_payment` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 5 | `Deadlock_Claim_status_decided_denial` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 6 | `Deadlock_Claim_period_not_started` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 7 | `Deadlock_Claim_period_running` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 8 | `Deadlock_Claim_period_extended` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 9 | `Deadlock_Claim_period_overdue` | deadlock | as 1 | as 1 | as 1 | as 1 | as 1 |
| 10 | `Deadlock_ClaimCase_status_received` | deadlock | ClaimCase view: ClaimCase, Investigation, Settlement at 1.2.0; plus Claim and Payment now | 13 → 30 | the four `triggers(Ev, CloseCase)` edges: `CloseCase` is a use case of the view, so its marks stay exact whatever the cone holds, and the closure then adds its `Triggered` parameter's `Claim._exists`, `Claim._owner_ClaimCase`, the four marks, and the raisers' reads (`Claim.status`, `period`, `grade_explained`, `fraud_suspected`, `reason_explained`, `Payment._exists`, `Payment._owner_Claim`); the three `AssessDamage` marks and `SendClaimForms`'s mark join as above | no (the raisers write only the claim), but `CloseCase` is kept, so the exemption keeps its marks exact | T = all 13 vars, no S, 63,823 states, decided → S empty (30 vars in T). The claim's lifecycle is new in the cone and every claim-side action reads the case: `SendClaimForms` reads `ClaimCase._exists` (its parent) and the case's mark, `CompleteClaim` reads `ClaimCase._exists`, `ClaimCase.coverage`, `Settlement._exists`, `Settlement.status`; a child lifecycle reads its parent, so it cannot be S when the parent is the anchor |
| 11 | `Deadlock_ClaimCase_status_responded` | deadlock | as 10 | as 10 | as 10 | as 10 | as 10 |
| 12 | `Deadlock_ClaimCase_status_investigating` | deadlock | as 10 | as 10 | as 10 | as 10 | as 10 |
| 13 | `Deadlock_ClaimCase_status_negotiating` | deadlock | as 10 | as 10 | as 10 | as 10 | as 10 |
| 14 | `Deadlock_ClaimCase_status_assessed` | deadlock | as 10 | as 10 | as 10 | as 10 | as 10 |
| 15 | `Deadlock_ClaimCase_liability_undetermined` | deadlock | as 10 | as 10 | as 10 | as 10 | as 10 |
| 16 | `Arm_AssessDamage_investigating_assessed` (before the rewrite; the rewritten sample decides it) | arm | as 10 | as 10 | as 10 | as 10 | as 10 |
| 17 | `Deadlock_Complaint_status_received` | deadlock | Complaint view: Complaint, ClaimCase at 1.2.0; plus Claim, Investigation, Payment, Settlement now | 5 → 35 | `triggers(EvDenialNotified, FileComplaint)`: `FileComplaint` is the view's own use case, its mark on Claim stays exact, and `NotifyDenial`'s reads bring the claim's lifecycle, which brings the case's through the four `CloseCase` edges and `SendClaimForms` | no (`NotifyDenial` writes only the claim), but `FileComplaint` is kept | T = Complaint (4 vars), S = ClaimCase (1 var, `_exists`) → T = Complaint (4 vars), S = the 31-var coupled lifecycle, `obs` = 4. S exists but does not reduce: its own split is empty (rows 1 and 10) and it passes the cap, so `Quotient::of` returns `None` and the cone runs unreduced |
| 18 | `Deadlock_Complaint_status_adr_complaint` | deadlock | as 17 | as 17 | as 17 | as 17 | as 17 |
| 19 | `Deadlock_Complaint_status_adr_dispute` | deadlock | as 17 | as 17 | as 17 | as 17 | as 17 |
| 20 | `Deadlock_Complaint_status_proposal_received` | deadlock | as 17 | as 17 | as 17 | as 17 | as 17 |
| 21 | `Deadlock_Payment_status_instructed` | deadlock | Payment view: the same 5 entities as the Claim view | 25 → 33 | as 1 | yes (as 1) | as 1 (T = Claim, Payment; S = the case's side, 63,823/443; 814,191 reduced → S empty) |
| 22 | `Deadlock_Subrogation_status_open` | deadlock | Subrogation view: Subrogation plus the 5 above (same 6 at 1.2.0) | 25 → 34 | as 1, plus `Claim._raised_EvClaimPaid` for `triggers(EvClaimPaid, StartSubrogation)` | yes (`PayClaim`) | T = Subrogation (3 vars), S = the 5 others (22 vars; S itself splits as the Claim view does, 573,751 states, 224 classes), 372 reduced states → T = Subrogation, S = 31 vars with no inner split, over the cap, unreduced (as 17) |
| 23 | `Deadlock_BusinessPermit_status_valid` | deadlock | BusinessPermit view: BusinessPermit, PermitApplication at 1.2.0; plus Disposition and Inspection now | 10 → 28 | `triggers(EvRevocationDecided, RevokePermit)` and `triggers(EvSuspensionDecided, SuspendBusiness)`: both are the view's own use cases, their `Triggered` parameter brings `Disposition._exists`, `Disposition._owner_BusinessPermit`, the marks, and `DecideRevocation`/`DecideSuspension`'s reads (`Disposition.kind`, `procedure`, `publication`, `status`); `triggers(EvRenewalGranted, ExtendPermitTerm)` and `triggers(EvPermitGranted, RegisterPermit)` bring `RenewPermit`/`GrantPermit`'s reads (`PermitApplication.channel`, `fee_paid`, `status`, `_owner_renews`, `Inspection._exists`, `_owner_PermitApplication`, `status`) | no (the raisers write Disposition and PermitApplication), but the handlers are kept | T = all 10 vars, no S, 7,813 states → S empty (28 vars in T). Disposition is a child of BusinessPermit: `ProposeDisposition` reads `BusinessPermit._exists`, so the disposition's side reads the anchor and joins T; the permit application's side read it already at 1.2.0 |
| 24 | `Deadlock_BusinessPermit_status_suspended` | deadlock | as 23 | as 23 | as 23 | as 23 | as 23 |

The hypothesis of the round ("the claim raises events the case's handlers consume and
vice versa, so each side reads the other's mark var and neither can be the independent
sub-lifecycle") is half right. Its conclusion holds: no cone of the two samples that holds
both lifecycles splits. Its cause is not the operative one. The four `CloseCase` edges alone
cost every claim-side cone its reduction, because the handler, on the case's side, reads the
raised claim row (its mark, its existence, its owner key), and in a cone anchored at the
claim those are T vars; a read of T makes the reader T (`split_at`, reduce.rs), and an
entity's vars share a side, so the case's lifecycle follows. The reverse edge
(`triggers(EvInitialResponded, SendClaimForms)`) is not what costs the case's cones: a copy
of the sample with only that edge removed leaves the `ClaimCase` view at 29 vars with S
empty, because the claim's lifecycle, once in the cone, reads the case through its parent
key and its guards (`SendClaimForms` reads `ClaimCase._exists`; `CompleteClaim` reads
`ClaimCase.coverage` and `Settlement.status`), as it did at 1.2.0 when it was simply not in
the cone. A copy with the four `CloseCase` edges removed (the design's option (b)) restores
every reduction: the `Claim` view splits again (836,427 reduced states, S 63,823 states in
607 classes, the `SendClaimForms` mark an observed var), the `ClaimCase` view is back to 16
vars and 63,823 states, and the `Complaint` and `Subrogation` views reduce through a nested
split (39,205 and 372 states).

The rule that decides it is the direction of reads. The #74 reduction lets T read S
(`obs`) and never S read T. In these models a child's lifecycle reads its parent (its
existence through the owner key, and the parent's columns in its guards) and a parent's
lifecycle reads nothing of its children. At 1.2.0 a cone anchored at a child (the claim, the
payment) therefore split off the parent's side, and a cone anchored at a parent held no
child at all. The latch orders an event that flows from the child to the parent
(`PayClaim` on the claim starts `CloseCase` on the case; `DecideRevocation` on the
disposition starts `RevokePermit` on the permit), so the parent's handler now reads the
child's row. In the child-anchored cones that read is S reading T, and the split is gone. In
the parent-anchored cones the child's lifecycle enters for the first time, and reads the
parent as it always did. Where the mark sits changes nothing: a mark on the parent's row
written by the child's raiser is a T action writing an S var, which the split forbids as
well. The reduction has one side that may observe the other, and a handoff ordered in the
other direction has no side to be.

The dump also separates two ways a cone ends unreduced that the round-2 text conflated. In the `Claim`, `ClaimCase`, `Payment`, and `BusinessPermit` views and in every claim invariant S is empty: `split_at` swallowed it. In the `Complaint` and `Subrogation` views and the `SubrogationEnds` property S exists (the 31-var coupled lifecycle) but `Quotient::of` returns `None`, because S's own split is empty and S passes the cap; `verify` then explores that S side to the 8,388,608 cap, discards it, and explores the unreduced cone to the cap again. The havoc's exemption for a view's own use cases is what brings the child's lifecycle into the parent-anchored views (`ClaimCase`, `Complaint`, `BusinessPermit`): the handler is kept, its marks stay exact, and the closure adds the raisers' reads. That exemption is also what reports `Deadlock_ClaimCase_status_assessed`, so it is not a knob: a sound deadlock verdict of the case needs the claim's lifecycle, and the question is only whether that lifecycle can be held as a quotient.

### Where the wall time goes

`verify` runs four phases in sequence: it builds the reduced cone of every invariant and
after-assert (`cone_jobs`, which attempts the S quotient of each cone under the 8,388,608
cap before the first line prints), explores the entity views by BFS (`state_models`), runs
TLC on the invariant cones up to 8 at once with one worker each (`run_cones`), and reruns
on the whole model only the temporal properties a cone left undecided
(`run_undecided_wholes`; it never reruns a deadlock or witness question). The timestamps
below are seconds from the start, from a run whose every output line was stamped
(`$CLAUDE_JOB_DIR`'s `d112-tv-*.log`), peak RSS from `/usr/bin/time -l`.

| Run | `cone_jobs` (plan printed) | BFS on the views (`state models` line) | specs written | TLC on the cones | whole-model reruns | total, RSS |
|---|---|---|---|---|---|---|
| insurance-claim-ops, 1.2.0 | 4 s | 9 s | 18 s | 18 s → 54 s (12 specs; the largest 707,071 states) | none | 54 s, 1,252 MB, ok (load 3.0) |
| food-permit-ops, latch + havoc | 0 s | 42 s (the `BusinessPermit` cone to the cap) | 43 s | 43 s → 82 s (`Cone_AfterAssert_SuspendBusiness_0`, 1,232,883 states) | none | 83 s, 3,077 MB, 2 undecided (load 5.9) |
| insurance-claim-ops, latch + havoc | 47 s (the S side of the `Subrogation`-shaped cones explored to the cap and discarded) | in progress when written (see `d112-tv-havoc2-ins.log`) | in progress | in progress | none expected (the two properties' cones are capped, and only a temporal property is rerun) | killed at 600 s in round 2 with 22 findings undecided and TLC still on 13 specs (load 4.0) |

Two things in the first row of the latch run are new. The `cone_jobs` phase grows from 4 s to 47 s before anything prints, because the S side of the `Subrogation`-shaped cones is explored to the cap and thrown away (the `None` branch above). And the BFS phase holds five view cones that reach the cap (`Claim`, `ClaimCase`, `Complaint`, `Payment`, `Subrogation`) where 1.2.0 had none; at food-permit-ops' rate for one such cone (42 s, 3 GB) that phase alone is several minutes and the memory of the largest cone. The whole-model rerun is not a cost here: it runs only for a temporal property a cone left undecided, never for a deadlock or an arm.

### A two-phase `verify` is sound for the safety-shaped checks

The prototype diff against `main` (`git diff origin/main proto/112-event-order-r2 -- crates`)
changes the transition relation in three places and nowhere else: a raiser's last step gains
`Assign` effects that set marks (`events.rs`), a handler's first step gains guards that read
them and, where it binds no raiser row, one `Earlier`-role parameter with an existence
guard, a link guard, and the mark guard (`events.rs`), and a handler with several tracked
events becomes one action per event, `<UC>_by_<Ev>`, each the first step's copy with its
own guards (`events.rs`). A mark is a new var, so projected onto the 1.x vars a raiser's
step is the same step, a handler's variant is the same step under one more conjunct, and
the union of the variants is the step under a disjunction of conjuncts. By induction on the
length of a run, every reachable state of the ordered system projects onto a reachable
state of the 1.x system, and every enabled ordered step onto an enabled 1.x step. Nothing
else moves: the `Triggered` parameter is `Earlier`, which `is_written()` excludes, so the
`keep` set of a view (`target_seed`) and the actions a view judges for `Runs_` and arms
(`findings.rs`) are the 1.x ones; the explorer binds an `Earlier` parameter to every slot
and lets the guards select (`moves.rs`), and the TLA+ printer adds no conjunct of its own
for it (`module.rs`), so BFS and TLC agree on it; `restrict_to` drops only guards on vars
outside the cone, which for a closed 1.x cone is none (the reduction's exactness test passes
with the marks on and off); the `merge_variants` of `targets.rs` touches the findings, not
the relation. The after-assert is checked after every variant of the last step
(`temporal.rs`), over the same binders, so once per run as today. One row set does change:
an `after(UC).assert` whose scope names an entity `UC` binds only through the new
`Triggered` parameter was universally bound at 1.x (`At::Bound`, with the
`UnboundAfterRead` info) and now reads the raised row, because `read_param` takes any row
parameter and `Earlier` is one. That is a smaller row set over a smaller state set, so the
implication below still holds for it, and it is a change of meaning the docs must state. No
sample assert has that shape (the two of food-permit-ops read only `BusinessPermit`).

So for every check that is a universal statement over reachable states, "OK at 1.x" implies
"OK under the order": `forbidden`, `invariant`, `required`, `exclusive`, the cross forms,
`check`, a quantifier, `Unique`, `MinCard`, and `after(..).assert`. A phase 1 that runs the
1.x cones (the marks off, every handler free) decides all of them where 1.x decides them,
and only a 1.x counterexample needs a strict rerun on the ordered cone, which may confirm
it (the violation is reachable in order) or clear it (#30). The kinds that are not monotone:
a witness (`NoWitness`), `Runs_`, an arm, and a choice are existential, so a 1.x witness may
be a run the order prevents; a deadlock ("some row at this value that no path moves") is a
statement about paths, and the order removes paths that moved a row (hiding nothing at
1.x can show as a deadlock in order, `Deadlock_ClaimCase_status_assessed`) and removes
reachable rows (a 1.x deadlock may be unreachable in order), so neither verdict carries;
`eventually` and `leads_to` under fairness flip both ways too, since the order shrinks the
enabled set the fairness ranges over and the set of runs it ranges over. Measured: with the
marks off (which is phase 1) `verify --deny-undecided` of all four flagship samples prints
`tlc_result: ok` with every check decided (clinic 34 s, patent 24 s, food 18 s, insurance
52 to 54 s; the round-2 table and `d112-tv-base-ins.log`), so no sample target needs a
strict rerun; on the three review models `check` at 2 instances with the marks off and with
the latch and the havoc gives #30 `AfterAssert_IssueCert_0` → OK (a free counterexample the
strict rerun clears), `hide.rdra` `MultiForbidden_Transfer_0` → the same error (a free
counterexample the strict rerun confirms), `wait.rdra` `Forbidden_Ticket_0` → the same error
plus `Runs_Audit` (true under the order). Of the 24 undecided findings phase 1 decides 0:
every one is a deadlock or an arm.

### TLC on the view cones, and instance symmetry

The explorer keys a state by its bytes (`seen: HashMap<State, u32>` in `explore/bfs.rs`, `admit`) and nothing under `lifecycle/explore/` canonicalizes two interchangeable instances of an entity; the only canonical form in the engine is `reduce.rs`'s `canonical_initial`, which picks a class representative, not a slot order. The swap symmetry is partly broken already: a create takes the lowest absent slot (`FirstAbsent`) and a deleted slot is never reused, so for 2 instances the states with slot 2 present and slot 1 absent do not exist, and owner keys couple the swaps of a child and its parent. The ceiling is 2^k for k entities in the cone (32 for the `Claim` view, 16 for `BusinessPermit`), the floor well under it. `symmetry_count` measures the factor on the first 1,000,000 BFS states of the `Claim`, `ClaimCase`, and `BusinessPermit` cones under the latch and of the `Claim` cone at 1.2.0; it runs after the timed `verify` in the same chain and writes `d112-sym.txt`. The TLC runs on the exported view cones (`View_Claim`, `View_BusinessPermit`, `View_ClaimCase`, printed by `view_cone_tla` as `verify` prints an invariant cone, `-workers auto`, 12 GB heap, 30 minutes each, one at a time) are queued behind them (`m3.sh`, logs `d112-tlc-*.log`). Both numbers were still pending when this section was committed; the chain keeps running and the files name the result.

### What follows

Two changes, of which the second is the root-level one.

1. **A two-phase `verify`** for the safety-shaped checks: phase 1 runs the 1.x cones (the marks off), and only a check that phase 1 refutes is rerun on its ordered cone. Sound by the argument above. It returns the TLC phase of every sample to its 1.2.0 cost (insurance-claim-ops' 13 specs that never finished become the 12 that finish in 36 s) and clears the `cone_jobs` growth, since the ordered cones are built only for the reruns. It decides none of the 24, which are all view findings.

2. **Let the #74 reduction's S side read monotone T vars.** The 24 are lost because a handler on the parent's side reads the child's raised row, and the split allows reads in one direction only. Every var the handoff makes S read only rises: a mark (false to true), an existence (absent, present, deleted), an owner key (written once at the create). Treat such reads as *inputs* of S: compute S's quotient on S extended with its inputs, where an input may rise at any time, and make a rise a visible step of the stuttering equivalence (a branching bisimulation with labelled visible moves, in place of `stuttering_blocks` over an unlabelled graph), so that a T raiser's write of the mark maps to the class move the rise labels. S stays closed, T still observes S through `obs`, and the reduced cone keeps the same verdicts on T, which is what the deadlock question of the view's entity reads. On the `Claim` view this restores T = Claim, Payment and S = the case's side with 8 input bits (the four marks on two rows) plus the claim's existence and key as inputs of `CloseCase`; on the `ClaimCase`, `Complaint`, and `BusinessPermit` views it lets the child's lifecycle be S with the parent's existence and the handler's mark as its inputs, provided the child's guards read nothing else of the parent that is not monotone (`CompleteClaim` reads `ClaimCase.coverage`, written once at the create, and `Settlement.status`, which is not; that read keeps the claim's side out of S in the case's view unless Settlement joins T, which the dump shows it can, as it did at 1.2.0). The cost to measure before building it is the size of S with free inputs: the case's side has 63,823 states, the input combinations are at most 256 by 9 by 9 but reachable ones are far fewer because `CloseCase` reads them only as "a raised claim of this case exists"; that count, on the prototype, is the first unit of the implementation. Expected effect: clinic-ops and patent-ops unchanged (no cone of theirs lost a reduction: 18 s and 24 s); food-permit-ops' two `BusinessPermit` findings decided if the disposition's side quotients against `BusinessPermit._exists`; insurance-claim-ops' 21 findings decided at a multiple of 1.2.0's 54 s set by the input factor, within the 40-minute `formal-verify` budget if that factor stays under about 10, and `engine-equivalence` unchanged because the reduced cone prints as today through the `Rep_` table.

What does not earn its place: a larger view cap (8,388,608 states already cost 3 GB on one cone, and the coupled cones are products of two lifecycles), instance symmetry alone (a factor of at most 2^k and measured below it, against cones whose size the TLC runs put well past the cap), and the havoc in every cone or the removal of the handoff edges, both of which the maintainer and the round-2 measurements already set aside because they give up the order where #112 wants it.

## Reduction with monotone inputs (round 4)

<!-- derived-from #why-the-cones-stopped-reducing-round-3 -->
<!-- constrained-by #cones-and-the-reduction -->

The root fix the round-3 diagnosis asked for, built on the prototype branch
`proto/112-event-order-r2` (commit `77d76376` and the one after it) and measured as the
earlier rounds were: release builds, one run at a time, `uptime` before each timed run, the
state counts as the evidence and the wall times as upper bounds (the 1-minute load was 1.5
to 4.2). The mechanism, the soundness argument, the counts, the timing, and what is still
undecided follow; the implementation plan at the end lands it on `main` in units a reviewer
can check one by one.

### What changed in the split

The #74 split allows reads in one direction: T reads S through `obs`, S reads nothing of T.
Round 4 lets an S action read T vars that only rise. A var is *monotone* when the writes of
the cone system, taken as edges between the var's values, form no cycle
(`TransitionSystem::monotone_vars` in `reduce.rs`): a create takes an existence from
`absent` to `present` and a delete from `present` to `deleted`; a delete resets the entity's
columns and owners, which makes those cyclic; an assignment or a choice in the action that
creates the row starts from the base value, elsewhere from any value; a transition is its
arms; the clock ticks up. The rule is checked on the cone, not declared, so a model that
adds a backward arm loses the reduction and nothing else. A mark (0 to 1 under the latch,
never cleared), an existence, an owner key, and a status whose arms all point forward pass;
`Settlement.status` of insurance-claim-ops passes, which the `ClaimCase` split below
relies on. The split (`Split::split_at`) now keeps an S action that reads T vars
if every such var is monotone, and records those vars as the cone's *inputs*.

Two shapes of read. A *raw* input is a T var an S action reads through its own parameters:
`CloseCase` reads `Claim._exists` and `Claim._owner_ClaimCase` of a claim it binds. A
*folded* input is a T read an S action makes through a parameter the handoff added: a
handler variant `CloseCase_by_EvPaid` binds the raised claim with an `Earlier` parameter and
reads its existence, its owner key, and its mark. Folding that read gives one Boolean per
slot of the key parameter (the case), `in_g[c] = ∃ claim p : exists[p] ∧ owner[p] = c ∧
mark[p] = 1` (`Fold`, `Split::fold_of`), and the variants of one handler that fold to the
same signature share the bit. The folded bit is what the handler's guard reads, so the
extended system below sees two bits per case, not four marks times two existences times two
keys per claim. Where a read fits neither shape (a quantifier with no T parameter, as in
`CompleteClaim` of the `ClaimCase` view) the var stays a raw input, which is the case the
counts below show as still over the cap.

### The extended sub-lifecycle and its quotient

The quotient is computed on S *extended* with its inputs: `restrict_to_mapped(S ∪ I_raw,
havoc = false)` keeps the S actions whole and keeps every T action that writes a raw input,
restricted to its effects on `S ∪ I_raw` and the guards it still has there, so a rise is a
labelled step bounded by what T must observe to make it, not a free flip. Each folded bit
is a Boolean var of the extended system (`VarKind::Run { usecase: "in_<g>" }`) with two
free actions per bit, `Rise_in_g` and `Fall_in_g` (`Extended::build`); the fall exists so
the quotient stays correct if a bit's fold is not monotone, at the cost of states the model
never reaches. The partition (`quotient_blocks`) is the coarsest one that is (1)
divergence-blind stuttering stable under the silent edges (the S actions, as today through
`stuttering_blocks`) and (2) strongly stable under the rise edges: two states of a block
reach the same set of blocks by a rise, iterated with (1) to a fixpoint. The class of an S
state is read off at an input value: `Reduction::class(ts, s)` keys by the S bytes, the raw
input bytes, and the bit values computed from the T state (`Fold::holds`), and the
representative of a class is a state of the extended system, so `Rep_` still prints as
before. The #101 identity rule applies unchanged.

The reduced cone moves as before for S steps and class moves. A T step whose inputs
changed recanonicalizes the class through the `rises` table, `(class, new input bytes) →
class` (`Reduction::recanon`), in stages: first the raw inputs, then each bit slot in
order (`Reduction::stages`), because one T step can change a raw input and a bit at once
(`PayClaim` writes `Claim.status` and the paid mark). A rise the table lacks is a panic
naming the action, the class, and the known rises; none fired on the samples or the random
models. The lift (`Reduction::lift`) replays the same stages on the uncapped extended run:
a T step with a rise moves the cursor along the rise edge whose target has the new input
bytes, and a class move takes the shortest silent path. The TLA+ printer
(`emit/tla/module.rs`) prints the bits as operators `In_g(c) == ∃ p ∈ Ids : exists[p] =
"present" ∧ owner[p] = c ∧ mark[p] = "1"` over the T vars, the table as `Rise ==
<<(key :> class) @@ ..., ...>>` (one cell per class, so the module now `EXTENDS TLC` for
`:>` and `@@`), and in every T action that has bits or writes a raw input the conjunct
`S_class' = LET c1 == IF raw' = raw THEN S_class ELSE Rise[S_class][raw'] IN LET c2 ==
... IN ...` after the action's `UNCHANGED`, which binds the primed inputs the operators
read.

The prototype's environment switches (`RDRA_INPUTS`, `RDRA_SPLIT_DUMP`, `RDRA_MARK_CONSUME`,
`RDRA_MARK_HAVOC`) selected the variants measured in this round; none of them landed.

### Why it decides what the cone decides

The relation `R = {((t, s), (t, rep(class(s, inp(t)))))}` between the cone and the reduced
cone is a bisimulation up to the observed vars, by the two stability conditions. An S step
of the cone from `(t, s)` to `(t, s')` is a silent edge of the extended system at the same
inputs; (1) gives that `rep(class(s))` reaches `rep(class(s'))` by a silent path with the
same `obs` along it, which is a class move. A T step from `(t, s)` to `(t', s)` that leaves
the inputs alone reads only T and `obs`, equal on both sides, so it is enabled on the
representative too and both land in `R` again. A T step that raises an input is a rise edge
of the extended system from `(s, inp(t))` to `(s, inp(t'))`; (2) gives that the
representative has a rise to the same block, and `recanon` takes it, so `(t', rep(class(s,
inp(t'))))` is the successor on the reduced side. The converse directions hold because a
representative is a state of the extended system and every edge from it is an edge of the
cone at some T state with those inputs, which is the T state at hand. Initial states match
by `canonical_initial`. The monotone rule is not used in this argument: it bounds the number
of reachable input histories (each input changes at most as many times as its domain has
values), which is what keeps the extended system finite and small. A fold is sound by the
same argument with the bit in place of the vars it folds, because the folded guard is a
function of those vars and the key.

So every check that is a formula over T and `obs` keeps its verdict: the invariants and
the forbidden states (a reachability question, `EF`), the arms (an `EF` of an enabled
step), the witnesses and `Runs_` (an `EF` of a step), and the view's deadlock, which
`findings.rs` asks as "some row at this value from which no path changes the flag"
(`observe`): `EF (v ∧ AG ¬change)`, over the T vars of the view's entity on the cone's
paths. The bisimulation preserves the `EF` half because it preserves every path of the cone
up to the stuttering of S, and the `AG ¬change` half although the equivalence is
divergence-blind, because `change` is a predicate over observed vars that no S step
touches, so an S divergence shows the same `change` on both sides. The exactness test (`assert_reduction_exact`) checks the lifted run of every finding
against the cone's run, now on a model whose S reads folded and raw inputs (the new test
below), so what the argument says is also what the explorer does.

Proven by argument: the bisimulation under conditions (1) and (2). Tested, not proven:
that `quotient_blocks` computes a partition that satisfies (1) and (2) (the exactness
test, the reduction-diff on random models, and the TLC agreement below), that the staged
recanonicalization matches the lift (the exactness test), and that the TLA+ printing is the
explorer's relation (`engine_equivalence`).

### The counts (unit 1)

Measured by `lifecycle_measure` at the `verify` bounds (2 instances, Int 0..5, cap 8,388,608
states per cone) under the latch and the havoc, first with raw inputs only
(`split-r4raw-*.txt`), then with the folds (`split-r4fold-*.txt`), both at commit
`77d76376`, before the fixes of `739bb108` (which make a fold raw when its key is a created
row or its initial value is not one, neither of which these views have); each line is one
view's reduced cone, the extended system's states and classes, and the reduction's wall
time.

| view (sample) | raw inputs: reduced cone, ext states / classes | folded: reduced cone, ext states / classes, ms |
|---|---|---|
| `BusinessPermit` (food) | 4,648,771, 16,419 / 143 | 4,648,771, 16,419 / 143, 35,319 |
| `Disposition` (food) | 138,246, 3,498 / 870 | 138,246, 15,184 / 1,248, 801 |
| `PermitApplication` (food) | 52,597, 92 / 62 | 52,597, 348 / 228, 210 |
| `Claim` (insurance) | > 8,388,608 (not reduced) | 836,427, 255,292 / 2,428, 14,214 |
| `Payment` (insurance) | > 8,388,608 | 903,987, 255,292 / 2,428, 14,572 |
| `Complaint` (insurance) | > 8,388,608 | 39,205, 633,787 / 145, 9,983 |
| `Subrogation` (insurance) | > 8,388,608 | 372, 633,787 / 224, 10,569 |
| `ClaimCase` (insurance) | > 8,388,608 | > 8,388,608 (S = Claim, Payment; 6 raw inputs) |
| `AfterAssert_PayClaim_0` (insurance) | > 8,388,608 | 783,427, 255,292 / 2,428, 8,377 |
| `ClaimEventuallyDecided` (insurance) | > 8,388,608 | 633,787, 255,292 / 2,428, 7,282 |

The stop rule (over 5,000,000 states, or classes near states) fires on none of the folded
systems; the largest, 633,787 states in 145 to 224 classes, is the case's side under the
claim's two bits. The raw form is the advisor's prediction confirmed: with a mark, an
existence, and a key per claim as free inputs of the case's side, every insurance cone
passes the cap, and the fold (one bit per case, "a raised claim of this case exists") is
the existential abstraction the brief named as the fallback.

### Timing (unit 4)

`verify --deny-undecided` at 2 instances, release build, one sample at a time, under
`RDRA_MARK_CONSUME=latch RDRA_MARK_HAVOC=2` with the round-4 split (`tv.sh`, logs
`d112-r4/tv-*.log`, `/usr/bin/time -l`). The 1.2.0 column is the round-2 table (marks
off). Every sample was timed on the final prototype binary (`739bb108`); food-permit-ops
and patent-ops were timed before its fixes too, and the two runs print the same per-cone
state counts.

| sample | 1.2.0 wall | round 2 (latch, havoc 2) | round 4 wall / RSS / undecided | uptime before |
|---|---|---|---|---|
| clinic-ops | 34 s | 34 s, 0 undecided | 18 s / 0.54 GB / 0, `tlc_result: ok` (after the fixes of `739bb108`; before them a panic at 8 s) | load 24.8 (1-minute, after the `engine_equivalence` run) |
| patent-ops | 24 s | 24 s, 0 undecided | 24 to 26 s / 0.76 GB / 0, `tlc_result: ok` | load 3.89 and 7.08 |
| food-permit-ops | 18 s | 83 s, 3.0 GB, 2 undecided | 35 to 37 s / 1.66 GB / 0, `tlc_result: ok` | load 2.99 and 10.0 |
| insurance-claim-ops | 52 to 54 s, 1.25 GB | killed at 600 s, 21 undecided | 230 to 238 s (BFS 131 to 139 s, TLC the rest) / 4.8 GB / 6, `tlc_result: ok` | load 3.68 and 18.3 |

insurance-claim-ops' six undecided findings are the `ClaimCase` view's deadlocks (`status`
= received, responded, investigating, negotiating, assessed; `liability` = undetermined), all
"the ClaimCase cone stopped at the state cap"; every other finding of the four samples is
decided, and TLC agrees on every reduced cone it ran, including the cones with a `Rise`
table (`Cone_Forbidden_Claim_0` and `_4`, `Cone_MultiForbidden_Payment_3`,
`Cone_AfterAssert_PayClaim_0`, `Cone_ClaimEventuallyDecided` with 633,787 states; food's
`Cone_Forbidden_Disposition_2` and `Cone_Inv_Disposition_0` with 23,034). The insurance wall
time is 4.3 times 1.2.0's and the RSS 3.8 times; the BFS half is the `ClaimCase` cone
running to the cap (133 s alone in the unit-1 measurement).

`engine_equivalence` with `RDRA_REQUIRE_TLC=1` under the same env: 59 of 61 tests pass; the run was killed by the system (`SIGKILL`, 84.5 GB RSS, the
1-minute load at 106) after 96 minutes while `bfs_and_tlc_reach_the_same_states_and_verdicts`
and `tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds` ran the sample cones
through TLC in parallel (`eqv2.log`, `eqv2.time`). The 30-minute CI budget is not met by this
run: the samples test now hands TLC the reduced insurance cones of 633,787 to 903,987 states
and the capped `ClaimCase` cone, which `VERIFY_FALLBACK` does not yet name. The eight tests
that failed on the first full run (`eqv.log`: three cones at one instance, one at two)
exposed two defects, fixed in `739bb108` and green since; see unit 3..

### Soundness evidence (unit 3)

The gates in the order they ran, all release builds under the latch and the havoc unless
said otherwise.

- The new test `a_sub_lifecycle_that_reads_inputs_of_t_is_reduced_and_decides_the_same`
  (`lifecycle/tests/reduce.rs`): a case with claims that raise `EvPaid` and `EvDenied` to
  `CloseCase` (one bit group, two variants, raw inputs for the claim's key and existence),
  and an order whose shipment is packed by the order's side and shipped by `EvPaid` (one
  bit, raw inputs). At both bounds the reduced cone is exact (`assert_reduction_exact` lifts
  every finding's run onto the cone), the finding names are equal, the deadlocks
  `Deadlock_Claim_status_open` (two instances) and `Deadlock_Claim_status_paid` are found
  before and after the rise that moves the class, exactly one forbidden state is violated on
  each side, and the reduced cone is smaller. Green.
- `reduced_domain_cones_decide_what_the_cones_decide` and the other reduce tests of the core
  crate: 7 passed with the latch and the havoc, 7 passed with the defaults
  (`core-latch.log`, `core-default.log`).
- `engine_equivalence`: 59 of 61 green, the samples test killed by the system as above. Its
  first full run found two defects: a fold keyed by a `New` parameter (`ConfigureRoom`'s room
  in clinic-ops), whose slot has no identity before the step, and a bit that the cone's
  initial state already raises (an entity no use case creates is present from the start,
  so `ManageProviderSchedule`'s parent exists) while the bit var started false, which left
  the initial S state without a class and made `verify` of clinic-ops panic. A fold is now
  none when its key is `New`, and a bit takes its initial value from the cone's initial
  states (`fold_initial`) or stays raw when they disagree. Two TLA+ defects came with them:
  the class conjunct read a primed input before the `UNCHANGED` that binds it, and the
  decoder took an input's value from the representative instead of the module's own var.
- The four round-2 models at two instances (`r2models.log`): `hide.rdra` the
  `MultiForbidden_Transfer_0` error, `wait.rdra` the `Forbidden_Ticket_0` error and
  `Runs_Audit`, `arm.rdra` OK, `i30.rdra` OK, as in round 2.
- `scripts/reduction-diff.py` with the generator extended to emit `event`, `raises`, and
  `triggers` across a relation (from a second random generator, so the seeds keep their
  earlier models; 16 of the first 20 seeds now raise an event): the 31 CI seeds, 22 of them with a reduced cone, 9 at three instances, no timeout,
  one divergence, seed 417 (`rdiff.log`). It is not a verdict: BFS agrees on both sides
  (`Deadlock_E1_status_s0`, three `Runs_`, one witness) and the seed passes with
  `RDRA_INPUTS=0`; with inputs on, TLC runs out of its 1 GB heap on
  `Cone_Quantifier_E1_0` (24,000 states unreduced), whose `Rise` table has 38,400 cells on
  one 2.85 MB line, because the cone's S reads `E0_exists` and `E0_status` raw at three
  instances and the table is keyed by every input tuple a class can rise to. The model has
  no event; the read is a plain parent guard that the one-direction split used to keep on
  T. The fix is in the printing (a function per class over the input vars it changes, or
  the fold's bit in place of a raw column with several values), not in the quotient. As
  the prototype stands, this seed fails `formal-verify`'s reduction-diff step.

So both CI gates the brief names are red on the prototype as it stands, for one reason: the
reduction now hands TLC work it never saw, a `Rise` table of 38,400 cells and reduced
insurance cones of 633,787 to 903,987 states that `VERIFY_FALLBACK` does not name. The
quotient's verdicts are not in question on any model measured; the size of what is printed
for TLC is. The plan's unit 4 carries the guard.

### What is still undecided, and why

Six, all on insurance-claim-ops, all deadlocks of the `ClaimCase` view, down from 24 in round
3 (2 on food-permit-ops, 22 on insurance-claim-ops before its rewrite). The cause is one
read.

The `ClaimCase` view splits T = the case's side (17 vars over `ClaimCase`, `Investigation`,
`Settlement`) and S = `Claim`, `Payment` (13 vars), with six raw inputs: `ClaimCase._exists`
and the `SendClaimForms` mark (read by `SendClaimForms` through the handoff parameter, which
folds), and `Settlement._exists`, `Settlement._owner_ClaimCase`, `ClaimCase.coverage`,
`Settlement.status`, read by `CompleteClaim`'s arm condition through two quantifiers over
the case and its settlement with no T parameter of its own:

```text
∃ ClaimCase b0 : owner[p0] = b0 ∧ coverage[b0] ∈ {2, 3}
∨ ∃ ClaimCase b0 : ∃ Settlement b1 : owner[p0] = b0 ∧ S.owner[b1] = b0 ∧ S.status[b1] = 2
```

`fold_of` folds reads made through a parameter and returns `None` for a quantifier, so the
four vars stay raw, and the claim's side times their histories passes the cap. The fold
that covers it is a *node fold*: a maximal sub-predicate not under a quantifier of S whose
reads are T except the link atom `owner[p0] = Index(b0)`, keyed by the T row the link
names (`FoldKey::Linked { entity: ClaimCase, var: Claim._owner_ClaimCase, param }`), becomes
one bit per case, `in[c] = coverage[c] ∈ {2, 3} ∨ ∃ Settlement b1 : S.owner[b1] = c ∧
S.status[b1] = 2`, and the predicate reads `∃ c : owner[p0] = c ∧ in[c]`. With it the view
has no raw input and two bits per case, so its extended system is the claim's side times at
most 16 bit values, which the `Claim` view's 255,292-state extended system (the same two
sides, the other way round) puts under the cap with room. It is the first unit of the plan
below that is not yet built.

### Implementation plan

Each unit ends in a state the CI gates check (`formal-verify` runs the exactness test and
the reduction-diff seeds, `engine-equivalence` runs TLC on every sample), and the proto
branch is the reference for every diff.

Round 5 built units 4 and 5 and replaced this list by the landing plan at the end of
"The size guard and the node fold (round 5)".

1. `monotone_vars` and the split with raw inputs (`Split::split_at`, `inputs`), behind the
   `RDRA_INPUTS` switch, with a unit test on a two-entity model that the split keeps an S
   action reading a monotone T var and drops one reading a cyclic var.
2. The extended system and the quotient with rise edges (`Extended`, `quotient_blocks`,
   `Reduction::rises`, `recanon`, `stages`), the lift, and the exactness test on the
   `CLAIM_CASE` and `ORDER_SHIP` models; `reduced_domain_cones_decide_what_the_cones_decide`
   green.
3. The fold (`Fold`, `fold_of`, the bit vars and the `Rise_`/`Fall_` actions of
   `Extended::build`, `Fold::holds`); the counts of unit 1 reproduced by `lifecycle_measure`.
4. The TLA+ printing (`In_g`, `Rise`, the staged `LET`, `EXTENDS TLC`, the conjunct after
   `UNCHANGED`) with a size guard: a cone whose `Rise` table passes a cell count, or whose
   raw input has more than two values at the bound, is not reduced (or prints the table as
   a function per class); `VERIFY_FALLBACK` names the reduced sample cones TLC cannot
   finish; `engine_equivalence` green with `RDRA_REQUIRE_TLC=1` inside 30 minutes and
   reduction-diff seed 417 agreeing. Without the guard units 1 to 3 land green and this
   unit turns CI red.
5. The node fold for quantifier reads, which decides the `ClaimCase` view; its count first.
6. The reduction-diff generator emits `event`, `raises`, and `triggers`; the seed list of
   `formal-verify` gains the seeds that exercise a rise.
7. The docs: `docs/formal-verification.md`'s "Reduction by an independent sub-lifecycle"
   gains the inputs, the monotone rule, and the fold; this document's "Cones and the
   reduction" is superseded by that section for the split rule.
8. Delete the diagnostics (`RDRA_SPLIT_DUMP`, `view_split.rs`, `view_cone_tla.rs`,
   `symmetry_count.rs`) or keep `RDRA_SPLIT_DUMP=1` alone as the one documented dump.

## The size guard and the node fold (round 5)

<!-- derived-from #reduction-with-monotone-inputs-round-4 -->
<!-- constrained-by #cones-and-the-reduction -->

Units 4 and 5 of the round-4 plan, built on `proto/112-event-order-r2` (commit `bb9aa7ce`
and the ones after it) and measured as before: release builds, `uptime` before each timed
run, one heavy job at a time except where a line says otherwise, every job under a watchdog
that kills its process tree past 16 GB or its wall cap (`d112-r5/guard.sh`, logs
`d112-r5/*.log`, tree RSS samples `d112-r5/*.rss`).

### Why the two gates were red (unit 4, the measurement)

Two causes, one per gate; neither is the quotient's verdicts.

*reduction-diff seed 417.* The cone `Cone_Quantifier_E1_0` at three instances splits, with
inputs on, into T = `E0` (2 vars) and S = `E1, E2` (7 vars) with every S var observed (obs =
S) and two raw inputs. The quotient is then the identity: 24,000 extended states, 24,000
classes. The #101 identity rule did not fire because the nested reduction of the extended
system (40 states, 40 classes, itself over an inner one of 1 state) counted as "an inner
reduction shrank S". The module TLC got had seven `Rep_` tables of 24,000 cells each, five
`Succ_` tables of the same length, and a `Rise` table of 38,400 cells over 14 distinct
input keys, 7.4 MB on one 2.85 MB line; TLC ran out of its 1 GB heap while processing the
module, before any state. The `Rise` table was the largest table, not the only one.

*engine_equivalence.* `tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds` on
insurance-claim-ops alone (`m-tlcalone-ins.log`, `RDRA_EQV_TRACE=1`): the state-model BFS
takes 115 s and `export --kind tla` 52 s (it runs `reduced_cone` on every entity cone again,
in a child process, including the `ClaimCase` view's extended system to the 8,388,608 cap);
the small `StateModel_` modules take 1 to 8 s of TLC each; `StateModel_ClaimCase`, the
unreduced cone (the view's split did not fit under the cap in round 4), did not finish in
the 40-minute cap, and while it ran the test process grew from 2.0 GB to 7.2 GB, about
1 GB per 6 minutes, with TLC's JVM steady at 1.3 GB: the test captures the whole output of
a `-continue` run, which prints a trace for every state that violates a deadlock
invariant. Last round's 84.5 GB was this growth under eight workers and two tests at once.
`bfs_and_tlc_reach_the_same_states_and_verdicts` (`m-reach-ins.log`): 61 s and a 3.6 GB tree
peak on the round-4 code with 45 s of it the `ClaimCase` view's reduction attempt to the cap,
31 s and 1.9 GB with the node fold; it fails on the `VERIFY_FALLBACK` list alone, which
lacked the capped `ClaimCase` cone (and, since round 4's inputs, names food-permit-ops'
`PermitApplication` where `BusinessPermit` is now the capped one).

So unit 4's guard is necessary for seed 417 and not sufficient for `engine_equivalence`:
only a `ClaimCase` view that reduces removes the 8,388,608-state explorations and the
unreduced module from both tests, which is unit 5.

### The size guard

Three rules in `TransitionSystem::reduced` (`reduce.rs`), each with its dump line under
`RDRA_SPLIT_DUMP=1`:

1. A split whose S is wholly observed (`split.s ⊆ split.obs`) is not explored: the quotient
   would be the identity whatever the inputs do.
2. A quotient that is the identity reduces nothing unless a nested reduction has fewer
   classes than states at some depth (`Reduction::shrinks`), so an identity inner reduction
   no longer licenses an identity outer one.
3. A reduced cone whose TLA+ tables would pass `TLA_CELL_BUDGET` = 100,000 cells
   (`Reduction::tla_cells`: one cell per class for each observed var and each S action, plus
   one per rise) is not used. If its split had inputs or bits, the split without inputs is
   tried (round 3's), and the cone stays unreduced otherwise. The budget's two measured
   points: the insurance-claim-ops cones of 2,428 classes print about 45,000 cells and run
   under 1 GB; the 24,000-class module above printed 326,400 and did not. The budget keeps
   the BFS from a reduction too, which costs nothing on the samples (no sample cone passes
   it) and is the simple form; printing the tables as functions per class was not needed
   once rules 1 and 2 removed the module that motivated it. The upper point measured is the
   `ClaimCase` view's 104,832 cells (below); between it and 326,400 the budget is a guess.

Seed 417 after the guard: the cone is not reduced (rule 1), the cone anchored at
`E0._exists, status` reduces to 600 classes (5,760 cells) where it had 24,000, TLC runs
the 24,000-state cone inside 1 GB, and the reduced and unreduced runs agree
(`s417/verify2.log`, 18 s).

### The node fold

`FoldKey::Linked` (`reduce.rs`). An S action with no T param whose one T-reading guard reads
T only through *nodes* folds the node into a bit keyed by the row the node's link names. A
node is a quantifier over a T entity E, or an And/Or of such quantifiers over one E, whose
bodies read T except link atoms `v[p] = Index(row)` with `v` an S owner var and `p` an S
param, all on one `(v, p)`, each a conjunct of its body (under And and nested existentials,
never under Or, Not, or a universal, where `link ∧ bit` would not be the node); conjuncts
of a body that read S alone and name no bound row
(`reviewed[p0] = true` beside `status[b0] = open` in a condition that joins the claim to
its case) leave the quantifier. The node stands in the guard under And/Or alone, possibly
more than once (the same node each time, as the two arms of `Archive` read "the case is
closed"), so the quantifier can be pulled out to the action. The bit at slot `c` holds when
row `c` of E is present and the body holds with `c` as the bound row (`Fold::holds`, which
evaluates the body with `Bound(0) = c`). The extended sub-lifecycle binds the row as a
param `q` of any slot (`ParamRole::Earlier`, as a linked T param already was) and reads
`v[p] = q ∧ bit[q]`, with the factored conjuncts beside them, where the node stood
(`substitute_node`); the guard is correct because `∃q` distributes over the Or and And it
stands under. The `S_class'` conjunct of a T action is printed last, after the guards on
the successor (seed 85, below); that order is safe because no guard reads an observed S
var primed (the printer primes `At::Next` terms alone, which are on the action's own rows). Actions whose nodes agree (entity, link var, body) share the bit var. The
TLA+ printer writes the bit as `In_g_k(c) == LET b0 == c IN E_exists[b0] = "present" /\
(body)`. A T param an effect references (`SendClaimForms` sets the claim's owner to its
`Parent` case) may now key a fold too: the param stays in the extended sub-lifecycle bound
to any slot, and the bit carries its presence and guards. `RDRA_NODE_FOLD=0` keeps such
reads raw, as in round 4.

`CompleteClaim`'s arm condition, the read that kept the `ClaimCase` view's four raw inputs,
is one node: an Or of two quantifiers over the case, the first reading `coverage`, the
second a nested quantifier over the settlement. With it and `SendClaimForms`'s fold the
view has no raw input and two bit groups per case.

What the fold costs. A bit flips freely in the extended system (`Rise_` and `Fall_`), so k
bit slots admit 2^k input histories, while a raw input rises only by the T actions that
write it, restricted to their guards on S and the inputs, which couples the rise to S. On a
model where the case's side has three readers of the case's status (`d112-r5/nf/case_archive.rdra`:
"open" read by `Pay` and `Deny`, "closed" by `Archive`, the case's presence by `File`)
the folds give 3 groups × 2 slots = 6 free bits and an extended system of 69,888 states
in 40,192 classes (723,456 cells, over the budget, so the cone falls back to the split
without inputs and stays unreduced), where the raw inputs (`Case._exists`, `status`) gave
4,220 states in 1,868 classes and a reduction. On the test model `CASE_FOLD` (one node) the
two are 688 / 336 against 660 / 360. The fold is the right instrument where the raw history
space is the larger (the `ClaimCase` view, below) and the wrong one where several bits
derive from one var; the measured form keeps the round-4 rule (fold what folds) and the
budget catches the bad case. A ladder that tries the folds first and the raw inputs second
is the obvious refinement and is left to the landing plan.

### The counts and the timing

The `ClaimCase` view with the node fold (`lifecycle_measure` at the `verify` bounds, cap
8,388,608, `RDRA_SPLIT_DUMP=1`, `count-ins.log`, uptime load 8.2 at the start, run beside
the reach measurement below): T = the case's side (17 vars), S = `Claim`, `Payment` (13
vars), no raw input, two bit groups; the extended system has 93,904 states in 4,368 classes
(392,912 silent edges, 375,616 rise edges), where round 4 passed the cap. Its tables print
104,832 cells (4,368 classes × 6 observed vars and 18 S actions), which the first budget of
100,000 refused, so the budget is 200,000. TLC run by hand on the view's exported module
(`export --kind tla`, `StateModel_ClaimCase.tla`, 1.6 MB, the `Rise` table 17,472 cells on
one 860 KB line; `tlc-claimcase.log`, `-Xmx1g`, one worker) processes the module, generates
52,687 distinct states in the first 3 s, and stops after 21 s on
`Deadlock_ClaimCase_status_assessed`, at a peak of 912 MB: the budget's upper point holds,
and TLC alone reports the deadlock `verify`'s BFS reports below. (`engine_equivalence`
does not run that module: the view is in `VERIFY_FALLBACK`, which `tlc_alone` skips.) The other insurance cones are as in round 4: the claim's
and the payment's views 255,292 states in 2,428 classes (72,840 cells), the complaint's and
the subrogation's 633,787 in 145 and 224.

`verify --deny-undecided` at 2 instances (`tv5.sh`, `/usr/bin/time -l`, release, latch
and havoc 2, one sample at a time right after the gates chain started its build, so the
walls are upper bounds):

| sample | round 4 wall / RSS / undecided | round 5 wall / RSS / undecided | uptime before |
|---|---|---|---|
| clinic-ops | 18 s / 0.54 GB / 0 | 17 s / 0.52 GB / 0, `tlc_result: ok` | load 8.80 |
| patent-ops | 24 to 26 s / 0.76 GB / 0 | 28 s / 0.78 GB / 0, `tlc_result: ok` | load 8.28 |
| food-permit-ops | 35 to 37 s / 1.66 GB / 0 | 43 s / 1.59 GB / 0, `tlc_result: ok` | load 9.90 |
| insurance-claim-ops | 230 to 238 s / 4.8 GB / 6 | 213 s / 2.03 GB / 0, `tlc_result: ok`, 1 deadlock | load 7.36 |

So the four samples decide everything at 2 instances, and `formal-verify`'s sample step
is red on the prototype until the sample unit of the plan lands: `verify-samples.sh`
expects insurance-claim-ops' `verify` to pass, and it now exits 1 on one deadlock.
insurance-claim-ops' one verdict is
`Deadlock_ClaimCase_status_assessed`: the trace opens two cases, assesses the second, and
files both claims (the bound's two `Claim` slots) under the first, so the second case has
no claim whose event could close it and stays assessed. It is a verdict of the bounded
model, not of the reduction: the lifted trace is a real path, and the view's cone is exact
by the argument of round 4 and by the engine comparison at one instance (`engine_equivalence`
compares the view's reduced cone of 763 states with TLC's graph and verdicts there, and at two
instances the cone is past the test's 300,000-state cap on both sides, so it is compared at
one). `assert_reduction_exact` skips a cone whose unreduced run passes 200,000 states, so
the view's exactness at two instances rests on the argument, not on a replay. It is the finding the README already names for
a case whose claim lapsed, now for a case that never gets a claim. A larger
`--instance-count` does not hide it, since the bound is the same for every entity and at any
count every claim slot can be filed under the first case; and a
case that reads its claims to be assessed or closed, in the three shapes the sample unit
measured (a `.has(Claim)` guard on `AssessDamage`, a `.none(Claim)` guard on
`SendClaimForms`, and the forms moving the case so that a case without a claim waits on a
slot), takes the claim's or the case's cone past 16 GB at 2 instances; so the sample states
it. 1.2.0 did not report it because `CloseCase` then needed no event.


### Soundness evidence

- `a_quantifier_over_t_folds_into_a_bit_keyed_by_the_linked_row` (`lifecycle/tests/reduce.rs`,
  model `CASE_FOLD`): a claim is paid only when reviewed and its case is open (a condition
  with an S atom and a T atom, joined to the case by the owner); the case closes on the
  claim's payment and is archived only when no claim is open. In the case's view the
  sub-lifecycle reduces with one `Linked` fold (`Pay`) and no raw input; the reduced cone
  is exact at both bounds (`assert_reduction_exact`); the verdicts equal the unreduced
  cone's, among them `Deadlock_Case_status_closed`, which exists only because the bit fell
  when the case closed (a claim still open then is never paid), and
  `Deadlock_Case_status_open`; the reduced cone is smaller. Green (0.03 s inside the 115 s `reduce` filter run, which includes the domain test).
- `reduced_domain_cones_decide_what_the_cones_decide` and the other reduce tests of the
  core crate: 8 passed with the latch and the havoc (`core-latch2.log`, 99 s), 8 with the
  defaults (`core-default2.log`, 100 s). The round-4 test's order-ship expectations moved
  with the effect-referenced key: `Prepare` folds too, so two bits and no raw input.
- `engine_equivalence` with `RDRA_REQUIRE_TLC=1`, release, latch and havoc 2, the whole
  file in one `cargo test` under the guard: the first full run (`eqv-full.log`, uptime
  load 11.6) took 344 s with a tree peak of 8.2 GB and failed one test of 61 on the
  `VERIFY_FALLBACK` list alone (the `ClaimCase` view and the food-permit-ops entry);
  `tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds` passed (it skips the
  `ClaimCase` view, now in the fallback list; the hand run above is the module's
  measurement). With the list corrected
  `bfs_and_tlc_reach_the_same_states_and_verdicts` passed alone in 199 s (1.8 GB RSS, 4.6
  GB tree, `reach-full.log`; its trace sums to 909 s of TLC and 106 s of BFS and
  reduction over 8 workers). The full file then
  passed twice, 61 of 61: before the seed-85 printing fix in 240 s (tree peak 7.5 GB,
  `eqv-full2.log`) and after it in 245 s (3.3 GB RSS, 6.9 GB tree, 2,130 s of user CPU,
  uptime load 12.6, `eqv-full3.log`). On the runner's 4 cores the CPU alone is about 9
  minutes if it parallelizes as here; the 30-minute budget holds with the margin of the
  serial parts, which the trace bounds by the longest cone's TLC run of 45 s. A guess until
  CI runs it. On the final build, with the conjunctive link rule, every gate ran once more
  (`chain5.sh`): the reduce tests 8 of 8 in both envs, insurance-claim-ops' `verify` 183 s
  and 1.9 GB with the one deadlock and nothing undecided, the 31 seeds with no divergence,
  and `engine_equivalence` 61 of 61 in 244 s (3.3 GB RSS, 7.6 GB tree, `eqv-full4.log`).
- `scripts/reduction-diff.py` with CI's 31 seeds (`rdiff-ci.log`): 31 compared, 16 with a
  reduced cone (22 in round 4; the observed-S rule and the identity rule now leave the
  useless ones unreduced), 9 at three instances, no timeout, no divergence, 25 s at 8
  jobs; seed 417 agrees. The same 31 seeds after the seed-85 fix (`rdiff-ci2.log`): no divergence, no timeout.
- 50 more seeds whose models raise and trigger an event (`event-seeds.txt`, seeds 60 to
  130 outside CI's list; `rdiff-ev.log`): 41 compared with no divergence, 23 of them with
  a reduced cone, 11 at three instances; 9 passed the 120 s per-run timeout. Rerun at 600 s
  (`rdiff-slow.log`): 65, 66, 80, 107 agree (129 to 287 s); 84, 101, 116, 118 time out on
  the *reduced* side, where the sub-lifecycle's exploration to the 8,388,608 cap costs more
  than the plain run (a cap for the extended system, or the ladder above, is the fix and is
  left open); seed 85 diverged on its temporal property `P0` alone, `violated` unreduced
  and `tool_error` reduced (562 s). The cause is in the printing, not the quotient: TLC
  evaluates an action's conjuncts in order, and the `S_class'` conjunct, placed before the
  arm conditions, looked the `Rise` table up for a step the arm condition rules out
  (`T4_0`'s `s1 -> s2` arm under a condition that needs `s0` or `s2`), on a cell the table
  rightly lacks, and TLC stops on the exception. The conjunct now comes last. After it the two sides agree on seed 85 (`rdiff-85.log`, `P0` violated on both, 7 minutes for the pair at 2 instances).
- The four round-2 models at two instances (`r2models.log`): `hide.rdra` the
  `MultiForbidden_Transfer_0` error, `wait.rdra` the `Forbidden_Ticket_0` error, `arm.rdra`
  OK, `i30.rdra` OK, as in rounds 2 and 4.

Proven by argument: the node fold's bit is a function of the T vars and the row, so the
round-4 bisimulation applies with the bit in place of the vars (the fold is sound by the
same argument as the param fold), and the stand-in is equivalent to the node because the
row's existential distributes over the And and Or it stands under, and the link's
existential commutes with the body's conjunction and nested existentials. Tested, not
proven: that `node_in` and `strip_links` accept a node only where that holds (the
`CASE_FOLD` and `CLAIM_CASE` models, the samples; a link under Or, Not, or a universal is
refused by construction, a shape no DSL join emits today), that the size guard keeps TLC inside its heap (two points measured, the
budget between them), and that the printing is the explorer's relation (seed 85 is the
case the earlier tests missed: an arm condition that is never true).

### Landing plan

<!-- supersedes #implementation-plan -->

Each unit is one PR on `main` after #111, with the check that proves it and the CI job it
runs in; the budgets are `engine-equivalence` ≤ 30 minutes and `formal-verify` ≤ 40
minutes on the runner (4 cores, 16 GB), against the 18-core machine here where the full
`engine_equivalence` run took 344 s (5.7 minutes, 2,369 s of user CPU) with a peak tree RSS of 8.2 GB (3.8 GB for the cargo process itself) (8 workers, so the
RSS here overstates the runner's and the wall understates it; the runner's wall is bounded
by the sum of the per-cone TLC times, which the trace lists). The prototype branch is the
reference diff for every unit; the switches (`RDRA_INPUTS`, `RDRA_NODE_FOLD`,
`RDRA_SPLIT_DUMP=2`) do not land.

1. **The marks and the latch as the default** (rounds 1 and 2, units 1 to 7 of the first
   plan). `RDRA_MARK_CONSUME=latch RDRA_MARK_HAVOC=2` becomes the one behavior; the env
   knobs go. Check: `cargo test --all` in the `check` job; `scripts/verify-samples.sh` in
   `formal-verify`, with the samples' `// expect verify` headers as the next unit sets them.
2. **The samples.** insurance-claim-ops states `Deadlock_ClaimCase_status_assessed` in its
   README and `// expect verify` header (a larger `--instance-count` does not hide it: at any
   count every claim slot can be filed under the first case); food-permit-ops and the
   README passages of round 2's unit 7.
   Check: `verify-samples.sh` green, `check-sample-artifacts.sh`.
3. **`monotone_vars` and the split with raw inputs** (round-4 unit 1), with the unit test on
   a two-entity model. Check: the `check` job's `cargo test`.
4. **The extended system, the quotient with rise edges, the lift, and the exactness test**
   (round-4 unit 2). Check: `reduced_domain_cones_decide_what_the_cones_decide` in
   `formal-verify`; `a_sub_lifecycle_that_reads_inputs_of_t_is_reduced_and_decides_the_same`.
5. **The size guard** (this round's unit 4): `shrinks`, the observed-S skip, and
   `TLA_CELL_BUDGET` with the retry without inputs. Check: reduction-diff seed 417 in
   `formal-verify`, and `tla_cells` asserted on the `CASE_FOLD` cone. This unit lands before
   any fold so that no intermediate state hands TLC a module it cannot process.
6. **The folds**: the param fold (round-4 unit 3, with the effect-referenced key) and the
   node fold (this round's unit 5), with their printing (`In_g`, `Rise`, the staged `LET`,
   `EXTENDS TLC`). Check: `a_quantifier_over_t_folds_into_a_bit_keyed_by_the_linked_row`;
   `engine_equivalence` with `VERIFY_FALLBACK` as this round sets it (the `ClaimCase` view
   added) inside 30 minutes; `verify --deny-undecided` on the four samples inside
   `formal-verify`'s 20-minute sample step (213 s for insurance-claim-ops here).
7. **The reduction-diff generator with events** (round-4 unit 6) and the seed list of
   `formal-verify` extended by the event-bearing seeds that reduce (23 of the 41 event-bearing seeds that finished reduce a cone; the list is in `rdiff-ev.log`).
   Check: the step inside its 10-minute timeout (the 31 CI seeds take 25 s here at 8 jobs, the slowest 21 s; the 50 event-bearing seeds 5 minutes with nine of them past the 120 s per-run timeout, below).
8. **Docs**: `docs/formal-verification.md`'s reduction section gains the inputs, the
   monotone rule, the folds, and the budget; this document's "Cones and the reduction" is
   superseded by it; `VERIFY_FALLBACK`'s table there gains the `ClaimCase` row. Check:
   `check-skill-snippets.py`, dagayn links.
9. **Delete the diagnostics** (`RDRA_SPLIT_DUMP=2`, `view_split.rs`, `view_cone_tla.rs`,
   `symmetry_count.rs`, `RDRA_EQV_SAMPLES`) and the dead `sub_system`, keeping
   `RDRA_SPLIT_DUMP=1` and `RDRA_EQV_TRACE` as the two documented dumps. Check: `cargo
   clippy -- -D warnings`, which the prototype fails today (the round-2 `events.rs` types
   and the dead method).

Left open for the maintainer: the ladder that tries the folds first and the raw inputs
second when the folds pass the budget (the `case_archive` shape above), and whether the
`engine_equivalence` test should stop capturing a `-continue` run's output in memory (a
file, or `-continue` only under `CONTINUE_STATES`, which is what made last round's run
reach 84.5 GB); neither blocks the units above.

## Alternatives rejected

- **A model-wide boolean per event** (`raised_<Ev>`, width 1 like `now`). Fails #30 at
  2 instances (answer 2). Would need a second width-1 var kind through the 21
  `VarKind::Now` match sites and a `Component` field beside `now`. Rejected on both.
- **A per-row boolean on one anchor entity only.** Less precise than marking every
  shared entity where the handler binds several of the raiser's entities
  (`OpenEncounter` binds the `Appointment` and the `PatientAccount` that `CheckInPatient`
  bound), for no saving, since a mark the handler reads on its own row costs no binding. The
  one-anchor form survives as the fallback of rule 5.
- **The mark on every parameter of the entity** (round 1's rule 5). Hides a violation
  when the handler has two parameters on the entity or binds it through a key the raiser
  does not (`hide.rdra`). Rejected on soundness.
- **A disjunction over the handler's parameters on the entity** instead of the unjoined
  fallback (round 2). Still asserts that one of the handler's own rows is the raised row,
  which the order does not say: a transfer between two unverified accounts, made after
  some third account was verified, is a run the order allows and the disjunction blocks.
  The unjoined `Triggered` parameter blocks nothing the order allows.
- **Consume once** (the issue's sketch). 43 times the `states` states of clinic-ops
  (25 times the latch's), the #74 reduction lost on every clinic-ops cone, `verify` of
  clinic-ops unfinished after 393 s and 11.6 GB where 1.2.0 takes 34 s, a raiser that
  waits for the handler, and that wait hides violations (`wait.rdra`). The precision it
  buys, "one run per event", the samples never needed. Every verdict change it produces,
  the latch produces too.
- **A bounded counter per row** (0..2, the raiser waiting at 2). 65 times the states of
  clinic-ops (37 times the latch's) for consume-once's semantics plus one queued event,
  and the same waiting raiser. Rejected with it.
- **A nondeterministic clear** (the handler may leave the mark set). With a waiting
  raiser it hides violations as consume once does, and costs 254 times clinic-ops'
  states (145 times the latch's) and `StateCapReached` at `check`'s default cap. With a
  saturating raiser it is the sound abstraction of a queue, measured in round 2 at 18 times clinic-ops' states (10 times the latch's), 7 times patent-ops' (where the latch costs nothing), 40 times food-permit-ops', and 3 times insurance-claim-ops', and it
  reports "handler ran twice after one raise" runs that consume-once rules out, which is
  the precision consuming was meant to buy; it buys nothing over the latch.
- **`performs` as a free trigger.** Leaves 2 of 26 tracked edges ordered, because the
  samples' handlers are performed by actors who act on the event (a provider creates the
  care plan after the encounter is signed). The change would then not apply to most
  models.
- **An opt-out modifier on `triggers`** (`.unordered()` or the like) for a use case that
  also starts on its own (round 2). Rejected by the maintainer: the model states where
  the use case starts with a second event (rule 8), and the coupled samples are rewritten
  rather than exempted.
- **Havoc in every cone** (`RDRA_MARK_HAVOC=1`, round 2). Drops the mark guards of the
  view's own use cases too, and the ClaimCase view of insurance-claim-ops then loses
  `Deadlock_ClaimCase_status_assessed` (measured). The chosen form exempts the view's
  `keep` actions.
- **One action with an OR over the per-event guards** instead of `<UC>_by_<Ev>` variants.
  Same states (variants change edges, not states). A `Triggered` parameter of the event
  whose guard did not hold would be bound to any present row of its entity in every
  trace, and a `NeverRuns` could not say which event never leads to the use case. With
  the findings aggregated per use case (rule 7) the variants cost nothing a user sees
  except the action names in traces.
- **No fallback when raiser and handler share no entity** (shared-only). Orders 10 of
  the 26 tracked edges and leaves the 16 that go through a key, which include every
  notification in clinic-ops, as unordered as today.
- **A synthesized arrival action for external-system events.** Adds a step and the
  states between arrival and handling, and nothing a check could see, since the
  arrival can happen at any time.

## Implementation plan

Each unit is one commit with the check that proves it, in this order. The units build on
`main` after #111; nothing lands on a 1.x release branch.

1. **Types and names, no behavior.** Add `VarKind::Raised`, `ParamRole::Triggered`,
   `GuardWhy::Raised`, and `ActionOrigin::UseCase::trigger`; extend every match
   (`cone.rs`, `restrict.rs`, `trace.rs`, `explore/bfs.rs`, `explore/scan.rs`,
   `explore/moves.rs` for the `Triggered` binding, `tla/names.rs`, `tla/module.rs`,
   `list_output/state_findings.rs`). Nothing builds them yet. Check: `cargo test --all`
   and `scripts/check-sample-artifacts.sh` unchanged.
2. **Classification.** A pure function in `lifecycle/build/events.rs` (new) from the
   model and the built actions to the list of tracked trigger edges, each with its mark
   entities and how the handler reads them (rules 1, 4, 5 with the correspondence rule),
   plus the `UnanchoredTrigger` and `UnjoinedTrigger` infos. Check: unit tests on small
   models for each branch of rule 5 (a corresponding row parameter, a corresponding
   parent, two parameters on the entity, a parent through the wrong key, the linked and
   the unjoined fallback), a timer-and-event use case, an external raiser, a raiser with
   no action, two raisers with and without a common entity, a self-trigger; `hide.rdra`
   as a test that the forbidden state is reported.
3. **The marks.** Push the vars, the raiser effects, the handler guards and `Triggered`
   parameters, and the `<UC>_by_<Ev>` variants, inserted after the first step in
   `uc_actions`; `after_asserts` reads every variant of the last step. Check: the #30
   model as `samples/formal-verification/event_order.rdra` with `// expect check: ok`
   and `// expect verify: tlc=ok`, and `engine_equivalence` on it (BFS and TLC agree at
   1 and 2 instances); the existing lifecycle tests updated where a trace now contains
   the raiser; `wait.rdra` as a test that `Forbidden_Ticket_0` is reported.
4. **Findings per use case.** Merge the variants of one `(uc, step)` in `targets.rs`
   before the runs and arms settle (rule 7), name the findings after the use case, and
   add the per-variant info. Check: `arm.rdra` reports nothing and `arm-neg.rdra` reports
   `Arm_Handle_x_y`, as lifecycle tests.
5. **Findings text.** The `NeverRuns` and deadlock guard text for `GuardWhy::Raised`,
   the `needs:` line for a `Triggered` parameter, the `bounds:` line's havoc note.
   Check: `cargo test` snapshot of the insurance-claim-ops deadlock message on the
   unrewritten sample (kept as a test model).
6. **Cone-local havoc.** `plain_reads` and `mark_reads` in `cone.rs`, the closure that
   adds a mark only when its writers are in or the reader is kept, and the guard and
   parameter drop in `restrict_to`. Check: a lifecycle test that an invariant cone
   without the raiser has no mark var and its handler no mark guard, that the #30 assert
   cone keeps both, and that the ClaimCase view keeps `CloseCase`'s guard;
   `lifecycle_measure` at `verify` bounds reproduces the table above within the cap.
7. **Samples.** insurance-claim-ops rewritten as the prototype's `tmp/r2/rw-insurance.py` did it (two events,
   two `raises`, two `triggers`, the README's four passages); food-permit-ops:
   the two README passages that cite #30 (the ledger "joined by events" paragraphs) rewritten to say the ledger moves only after the decision's event, and the sample's `verify` expectation settled per open question 4 (the two undecided findings named in the README and in `docs/formal-verification.md`'s sample list, or the four `triggers(Ev, <ledger use case>)` lines removed); the README passages of clinic-ops and patent-ops that cite #30 for a
   guard; `scripts/sample-artifacts.tsv` and the samples' `out/` regenerated. Check:
   `scripts/verify-samples.sh target/release/rdra-ish` green with `tlc`,
   `scripts/reduction-diff.py` on CI's seeds, `scripts/check-sample-artifacts.sh`.
8. **Docs and changelog.** Listed in the next section. Check: `check-skill-snippets.py`
   (its transcripts contain the #30 example), dagayn links resolve.
9. **Release.** Version 2.0.0 in `Cargo.toml`, the `### Breaking` entry, the
   `docs/compatibility.md` note that 2.0.0 changes a verdict within the same bounds.

### Documentation changes

- `docs/vision.md`: rule 3 of the lifecycle layer ("the order in which a model triggers
  use cases adds no guard of its own") and a Decisions entry dated for 2.0 that points
  here and retires the 2026-11-01 entry.
- `docs/state-derivation.md`: a row in the Variables table for `<E>._raised_<Ev>`, a
  paragraph "Triggers" under Actions beside "Runs of a split use case" with rules 1 to 9,
  the Joins pointer for the `Triggered` parameter, the cone-local havoc under Cones, and
  the `UnanchoredTrigger` and `UnjoinedTrigger` infos in Diagnostics.
- `docs/language-reference.md`: "`triggers` and event-driven BUC chaining" loses the
  "adds no guard" paragraphs and the guard rewrite; the `OrderExecuted` example becomes
  "a guard that documents a precondition the handler relies on"; a paragraph "A use case
  that also starts on its own" with the second-event pattern of rule 8 and the
  insurance-claim-ops `EvSurveyReported` as the example; Timers and External systems say
  they are free triggers.
- `docs/formal-verification.md`: the "Events and timers impose no order" gap is replaced
  by "The order is by row and the mark counts no events" (rule 9, the `UnjoinedTrigger`
  edges, and the cones in which the havoc leaves a handler unguarded), the Mapping table
  gets the TLA+ rows above, and `VERIFY_FALLBACK` is remeasured.
- `docs/compatibility.md`: the `bounds:` line is unchanged; the Breaking entry explains
  which verdicts moved.
- `CHANGELOG.md` `### Breaking`: a triggered use case waits for its event on its rows;
  which samples change; `<UC>_by_<Ev>` action names in traces, with the findings still
  per use case; the new infos.
- `samples/*/README.md`: the passages that cite #30, as the plan's unit 7 lists them.

## Open questions for the maintainer

1. **Latch, not consumption.** #112 sketched a mark the handler consumes. This design
   keeps the mark set, on the measurements above. If "one run per event" is wanted
   regardless of cost, the saturating nondet variant is the sound one (the consume and
   counter variants hide violations through their waiting raiser), and its numbers are
   in the tables.
2. **The four `UnjoinedTrigger` edges, now five with `hide.rdra`'s shape.** Ordered but
   not tied to the handler's rows. A later `.along(..)` on `triggers`, or a `.role(..)`
   on `triggers` for the two-keys case, would tie them; whether that is worth syntax is
   open.
3. **The havoc's exemption.** The view's own use cases keep their marks exact; every
   other reader in a cone that lacks the raiser is unguarded there. An invariant or
   assert that reads two entities joined only by an event is then checked as 1.x checked
   it. If a model wants an invariant checked with the order, it reads something the
   raiser writes, which pulls the raiser in. Whether that needs a sentence in the
   invariant's `bounds:` line, or a flag, is the maintainer's call; the design prints the
   note.
4. **The undecided view findings of the two coupled samples at 2 instances.** With the
   chosen design `verify --deny-undecided` of food-permit-ops leaves the two deadlock
   questions of the `BusinessPermit` view undecided (every invariant and assert decides,
   82 s), and insurance-claim-ops leaves its claim-side views and invariants undecided
   within 600 s, rewritten or not. `check` at 1 instance decides all of them. The
   options, each measured above: (a) keep the order and state the undecided findings in
   the samples' READMEs and in `docs/formal-verification.md`, which today says
   `--deny-undecided` passes on every sample; (b) remove the use-case edges that join
   the two lifecycles (`triggers(EvRevocationDecided, RevokePermit)` and its three
   siblings in food-permit-ops; the four `triggers(.., CloseCase)` in insurance-claim-ops),
   which returns those samples to 1.2.0's cost (18 s and 52 s) and to 1.x's meaning on
   exactly the edges #112 orders, with the event still raised and shown; (c) a larger
   cap or 1 instance for those two samples' `verify` expectation. The design recommends
   (a): the findings are real questions about a lifecycle that 2.0 no longer lets the
   check split, and (b) would make the samples show the feature nowhere but clinic-ops.
   The maintainer's "rewrite the samples so `verify` decides" is (b), so this document
   does not apply it without that confirmation.

Decided since round 1: the meaning of `triggers` and the second-event pattern (rule 8,
decision 9), the variant name `<UC>_by_<Ev>` with per-use-case findings (decision 10),
and the rewrite of the coupled samples instead of an opt-out (decision 11).
