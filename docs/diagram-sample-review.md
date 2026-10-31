# Diagram Sample Review Guide

<!-- constrained-by ./cli-reference.md#diagram -->
<!-- constrained-by ./incremental-modeling.md -->
<!-- derived-from ../samples/incremental-order/step-6-business-rules/design.md -->

This guide embeds one generated diagram of each `diagram --kind` and explains how to read
it during review. The kinds follow the RDRA 3.0 diagrams, in the order a review usually
walks them: system context and requirements, business, use cases and flows, information
and conditions, lifecycle and events, then the data model and the whole graph.

Most examples use `samples/incremental-order/step-6-business-rules`, a small model with
business scope, permissions, UI, APIs, entities, lifecycle, and two systems. It declares
no requirement, flow, condition, or event, so those sections use `samples/clinic-ops`
and `samples/ec-site`.

Regenerate the artifacts with:

```sh
bash scripts/check-sample-artifacts.sh
```

The same command runs in CI, so intentional emitter or DSL changes appear as a normal
diff of the `out/` files and of the blocks below.

`rdra-ish` writes Mermaid text (`.mmd`) only, for a reader that redraws it: a person
with Mermaid CLI (`mmdc`), or a tool such as an AI that draws SVG from the text. The
text carries the meaning, so shapes, colours, and direction are free to change.

## Reading Any Diagram

<!-- derived-from #diagram-sample-review-guide -->
<!-- constrained-by ./cli-reference.md#encoding-contract -->

Every kind follows the [encoding contract](./cli-reference.md#encoding-contract):

- The `%%` header comes first: `%% rdra-ish diagram=<kind> scope=<scope> model=<paths>`,
  then `%% layers:` (in the layered kinds), `%% kinds:` (the node kinds the body draws),
  and `%% omitted:` (the linked kinds the diagram leaves out).
- Each node id is `<kind>__<DSL id>`, the same in every kind, so
  `usecase__ChangeNextRestockDate` in one diagram is that use case in all of them. Each
  node carries `:::<kind>`.
- Each edge label is the DSL predicate name. `-->` is hierarchy or flow order, `---` is a
  `relate`, and `-.->` is every other association.
- An edge followed by `%% derived <from> -> <to> via <ids>` was derived through the nodes
  named; any other edge is declared. `%% desc <id>: <text>` carries a description.

## System Context

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind context \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/context
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/context.mmd -->
```mermaid
%% rdra-ish diagram=context scope=whole model=samples/incremental-order/step-6-business-rules/src
%% layers: value=システム価値 system=システム
%% kinds: actor system
%% omitted: buc permission
flowchart LR
  subgraph layer_value["システム価値"]
    actor__OpsStaff["Operations Staff"]:::actor
  end
  subgraph layer_system["システム"]
    system__OrganizationSystem["Organization System"]:::system
    system__StoreAdminSystem["Store Admin System"]:::system
  end
  actor__OpsStaff -.->|performs| system__OrganizationSystem
  %% derived actor__OpsStaff -> system__OrganizationSystem via buc__BucStoreRestock
  actor__OpsStaff -.->|performs| system__StoreAdminSystem
  %% derived actor__OpsStaff -> system__StoreAdminSystem via buc__BucStoreRestock
  classDef actor stroke:#333;
  classDef system stroke:#333;
```

How to read it:

- Actors and external systems sit in システム価値, the systems in システム.
- Every edge is derived. `OpsStaff` performs `BucStoreRestock`, whose use cases invoke an
  API of each system, so the actor reaches both systems, each edge `via` the BUC.
- An external system edge reads `raises <Event>` or `triggers <Event>`. When no system
  is declared, or none contains an API the use cases invoke, the edge ends at
  `system__Model`, with a `%% synthesized` line that says why.

Review points:

- Is every actor who uses the system drawn with an edge? An actor without one performs
  nothing yet.
- Does each system an actor reaches match the system that owns the work?

## Requirement Model

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/clinic-ops \
  --kind requirement \
  --format mermaid \
  --out samples/clinic-ops/out/requirement
```

<!-- embed: ../samples/clinic-ops/out/requirement.mmd -->
```mermaid
%% rdra-ish diagram=requirement scope=whole model=samples/clinic-ops
%% layers: value=システム価値 environment=システム外部環境 system=システム
%% kinds: requirement nfr quality constraint buc system entity
%% omitted: (none)
flowchart LR
  subgraph layer_value["システム価値"]
    requirement__ReqEligibilityEveryVisit["受付のたびに保険資格を確認する"]:::requirement
    requirement__ReqPayAfterConsultation["会計は診察の後に一度で行う"]:::requirement
    requirement__ReqSwiftCheckIn["Patients are checked in and roomed without repeated data entry"]:::requirement
    nfr__ScheduleLookupLatency["Schedule lookup latency"]:::nfr
  end
  subgraph layer_environment["システム外部環境"]
    quality__Auditability["Auditability"]:::quality
    quality__Performance["Performance"]:::quality
    quality__Privacy["Privacy"]:::quality
    constraint__BillingAuditTrail["Billing audit trail"]:::constraint
    constraint__NoPharmacySteering["特定の保険薬局において調剤を受けるべき旨の指示等を行わない"]:::constraint
    constraint__PatientDataPrivacy["Patient data privacy"]:::constraint
    buc__BucVisitCheckIn["受付（初診・再診）"]:::buc
    %% desc buc__BucVisitCheckIn: 到着確認、資格確認、紹介状の受付、診察室への案内。会計は診察の後に行う。
    buc__BucVisitPayment["計算・会計"]:::buc
    %% desc buc__BucVisitPayment: 診察後の会計計算入力、患者の呼出、窓口または自動精算機での精算、領収証・明細書の交付。
  end
  subgraph layer_system["システム"]
    system__ClinicOpsSystem["Clinic Operations System"]:::system
    entity__InsurancePolicy["保険資格"]:::entity
    entity__PatientProfile["Patient Profile"]:::entity
    entity__PaymentTransaction["Payment Transaction"]:::entity
    entity__Prescription["処方箋"]:::entity
  end
  constraint__BillingAuditTrail -.->|constrains| entity__PaymentTransaction
  constraint__BillingAuditTrail -.->|constrains| system__ClinicOpsSystem
  constraint__BillingAuditTrail -.->|qualifies| quality__Auditability
  constraint__NoPharmacySteering -.->|constrains| entity__Prescription
  constraint__PatientDataPrivacy -.->|constrains| entity__InsurancePolicy
  constraint__PatientDataPrivacy -.->|constrains| entity__PatientProfile
  constraint__PatientDataPrivacy -.->|qualifies| quality__Privacy
  nfr__ScheduleLookupLatency -.->|applies_to| system__ClinicOpsSystem
  nfr__ScheduleLookupLatency -.->|qualifies| quality__Performance
  requirement__ReqEligibilityEveryVisit -.->|motivates| buc__BucVisitCheckIn
  requirement__ReqPayAfterConsultation -.->|motivates| buc__BucVisitPayment
  requirement__ReqSwiftCheckIn -.->|motivates| buc__BucVisitCheckIn
  classDef requirement stroke:#333;
  classDef nfr stroke:#333;
  classDef quality stroke:#333;
  classDef constraint stroke:#333;
  classDef buc stroke:#333;
  classDef system stroke:#333;
  classDef entity stroke:#333;
```

How to read it:

- Requirements and NFRs sit in システム価値, qualities and constraints in
  システム外部環境, with what they bind in its own layer.
- `motivates` runs from a requirement to a BUC; `applies_to` from an NFR to a use case,
  API, or system; `constrains` from a constraint to what it limits; and `qualifies` from
  an NFR or constraint to its quality.
- The DSL has no link from a requirement to the actor who holds it, so none is drawn.

Review points:

- Does every requirement motivate a BUC?
- Does every NFR and constraint name what it applies to and the quality it serves?

## Business Context and BUCs

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind business \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/business
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/business.mmd -->
```mermaid
%% rdra-ish diagram=business scope=whole model=samples/incremental-order/step-6-business-rules/src
%% layers: value=システム価値 environment=システム外部環境
%% kinds: actor business buc
%% omitted: medium permission usecase screen api entity
flowchart LR
  subgraph layer_value["システム価値"]
    actor__OpsStaff["Operations Staff"]:::actor
  end
  subgraph layer_environment["システム外部環境"]
    subgraph business__StoreOperations["Store Operations"]
      %% desc business__StoreOperations: Store operations owns restock planning and store master maintenance.
      buc__BucStoreRestock["Maintain Store Restock"]:::buc
      %% desc buc__BucStoreRestock: Keep restock dates and store organization assignments operationally correct.
    end
  end
  actor__OpsStaff -.->|performs| buc__BucStoreRestock
  classDef actor stroke:#333;
  classDef business stroke:#333;
  classDef buc stroke:#333;
```

How to read it:

- Each business is `subgraph business__<id>` holding its BUCs, in システム外部環境.
- `performs(Actor, Buc)` is declared. An actor of one of the BUC's use cases or steps, and
  an external system whose event starts or leaves the BUC, are derived, each with its
  `%% derived` line. `--buc` narrows the diagram to the RDRA BUC diagram.

Review points:

- Does every BUC belong to the business that owns its value?
- Is every actor of the BUC drawn, directly or through its use cases?

## UC複合図

<!-- derived-from #business-context-and-bucs -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind usecase \
  --format mermaid \
  --buc BucStoreRestock \
  --out samples/incremental-order/step-6-business-rules/out/usecase_buc_store_restock
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/usecase_buc_store_restock.mmd -->
```mermaid
%% rdra-ish diagram=usecase scope=buc:BucStoreRestock model=samples/incremental-order/step-6-business-rules/src
%% layers: value=システム価値 boundary=システム境界 system=システム
%% kinds: actor usecase screen entity state
%% omitted: buc medium permission api
flowchart LR
  subgraph layer_value["システム価値"]
    actor__OpsStaff["Operations Staff"]:::actor
  end
  subgraph layer_boundary["システム境界"]
    usecase__BlockScheduledRestock["Block Scheduled Restock"]:::usecase
    usecase__ChangeNextRestockDate["Change Next Restock Date"]:::usecase
    usecase__ChangeStoreParentOrganization["Change Store Parent Organization"]:::usecase
    screen__StoreMaintenanceScreen["Store Maintenance"]:::screen
  end
  subgraph layer_system["システム"]
    entity__Organization["Organization"]:::entity
    entity__Store["Store"]:::entity
    state__Store_blocked["blocked"]:::state
    state__Store_normal["normal"]:::state
    state__Store_scheduled["scheduled"]:::state
  end
  actor__OpsStaff -.->|performs| usecase__BlockScheduledRestock
  %% derived actor__OpsStaff -> usecase__BlockScheduledRestock via buc__BucStoreRestock
  actor__OpsStaff -.->|performs| usecase__ChangeNextRestockDate
  %% derived actor__OpsStaff -> usecase__ChangeNextRestockDate via buc__BucStoreRestock
  actor__OpsStaff -.->|performs| usecase__ChangeStoreParentOrganization
  %% derived actor__OpsStaff -> usecase__ChangeStoreParentOrganization via buc__BucStoreRestock
  screen__StoreMaintenanceScreen -.->|shows| entity__Organization
  screen__StoreMaintenanceScreen -.->|shows| entity__Store
  usecase__BlockScheduledRestock -.->|displays| screen__StoreMaintenanceScreen
  usecase__BlockScheduledRestock -.->|transitions Store.restock_status scheduled->blocked| state__Store_blocked
  usecase__BlockScheduledRestock -.->|updates| entity__Store
  usecase__ChangeNextRestockDate -.->|displays| screen__StoreMaintenanceScreen
  usecase__ChangeNextRestockDate -.->|transitions Store.restock_status normal->scheduled| state__Store_scheduled
  usecase__ChangeNextRestockDate -.->|updates| entity__Store
  usecase__ChangeStoreParentOrganization -.->|displays| screen__StoreMaintenanceScreen
  usecase__ChangeStoreParentOrganization -.->|reads| entity__Organization
  %% derived usecase__ChangeStoreParentOrganization -> entity__Organization via api__OrganizationLookupApi
  usecase__ChangeStoreParentOrganization -.->|updates| entity__Store
  %% derived usecase__ChangeStoreParentOrganization -> entity__Store via api__StoreAdminApi
  classDef actor stroke:#333;
  classDef usecase stroke:#333;
  classDef screen stroke:#333;
  classDef entity stroke:#333;
  classDef state stroke:#333;
```

How to read it:

- Each use case of the BUC sits in システム境界 with the screens it displays; the
  entities they show and the use case reads and writes, and the states it moves, sit in
  システム.
- The APIs are folded into their use cases: `ChangeStoreParentOrganization` updates
  `Store` and reads `Organization` through `StoreAdminApi` and `OrganizationLookupApi`,
  which the `%% derived ... via api__...` lines name. `omitted:` lists the API,
  permission, and medium kinds left out.
- A `transitions` edge runs from the use case to the state it enters.

Review points:

- Does each use case display a screen and touch the information its business step
  needs?
- Are direct entity writes intentional, or should they move behind an API?

## Business Flow

<!-- derived-from #uc複合図 -->

Generated from:

```sh
rdra-ish diagram samples/clinic-ops \
  --kind flow \
  --format mermaid \
  --buc BucVisitCheckIn \
  --out samples/clinic-ops/out/buc/flow_visit_check_in
```

<!-- embed: ../samples/clinic-ops/out/buc/flow_visit_check_in.mmd -->
```mermaid
%% rdra-ish diagram=flow scope=buc:BucVisitCheckIn model=samples/clinic-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__VisitCheckInFlow["受付の流れ"]
    subgraph lane__VisitCheckInFlow__Nurse["看護師"]
      %% lane lane__VisitCheckInFlow__Nurse: actor__Nurse
      step__RoomingStep["診察室への案内"]:::step
    end
    subgraph lane__VisitCheckInFlow__ReceptionClerk["医事課 受付担当"]
      %% lane lane__VisitCheckInFlow__ReceptionClerk: actor__ReceptionClerk
      step__ArrivalStep["到着・予約票の確認"]:::step
      step__DeclarationStep["資格を確認できない（申立書・障害時モード）"]:::step
      step__EligibilityStep["資格確認"]:::step
      step__ReferralStep["紹介状あり（初診）"]:::step
    end
  end
  usecase__AssignRoom["Assign Room"]:::usecase
  usecase__CheckInPatient["受付（初診・再診）"]:::usecase
  usecase__ConfirmEligibilityInOutageMode["システム障害時モードでの資格確認"]:::usecase
  usecase__ReceiveEligibilityInvalid["資格確認結果（無効）の取込"]:::usecase
  usecase__ReceiveEligibilityNoInfo["資格確認結果（該当資格なし）の取込"]:::usecase
  usecase__ReceiveEligibilityValid["資格確認結果（有効）の取込"]:::usecase
  usecase__ReceiveReferralLetter["紹介状受付処理"]:::usecase
  usecase__RecheckEligibility["資格の再確認"]:::usecase
  usecase__RecordEligibilityDeclaration["被保険者資格申立書の受領"]:::usecase
  usecase__VerifyArrival["予約票・診察券の確認"]:::usecase
  step__ArrivalStep -.->|covers| usecase__VerifyArrival
  step__ArrivalStep -->|precedes| step__EligibilityStep
  step__DeclarationStep -.->|covers| usecase__ConfirmEligibilityInOutageMode
  step__DeclarationStep -.->|covers| usecase__RecordEligibilityDeclaration
  step__DeclarationStep -->|precedes| step__RoomingStep
  step__EligibilityStep -->|branches| step__ReferralStep
  step__EligibilityStep -.->|covers| usecase__CheckInPatient
  step__EligibilityStep -.->|covers| usecase__ReceiveEligibilityInvalid
  step__EligibilityStep -.->|covers| usecase__ReceiveEligibilityNoInfo
  step__EligibilityStep -.->|covers| usecase__ReceiveEligibilityValid
  step__EligibilityStep -.->|covers| usecase__RecheckEligibility
  step__EligibilityStep -.->|excepts| step__DeclarationStep
  step__EligibilityStep -->|precedes| step__RoomingStep
  step__ReferralStep -.->|covers| usecase__ReceiveReferralLetter
  step__ReferralStep -->|precedes| step__RoomingStep
  step__RoomingStep -.->|covers| usecase__AssignRoom
  classDef flow stroke:#333;
  classDef step stroke:#333;
  classDef lane stroke:#333;
  classDef usecase stroke:#333;
```

How to read it:

- Each flow is `subgraph flow__<id>` holding its steps. `precedes` and `branches` are
  `-->`, `excepts` is `-.->`, and `repeats` is `-->` back to an earlier step.
- `covers` runs from a step to the use cases (and APIs, events, or timers) that carry it.
- A step that one actor performs (`performs(Actor, Step)`) sits in that actor's lane,
  `subgraph lane__<Flow>__<Actor>`, whose first line names the actor. Here the receptionist
  carries the arrival, eligibility, and referral steps, and the nurse rooms the patient.

Review points:

- Does every step cover a use case, and every exception rejoin the flow?
- Is each step in the lane of the person who does it, and does the hand-off between lanes
  match the work?
- Is the business order in the flow the order the use cases can run in?

## Information Model

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind information \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/information
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/information.mmd -->
```mermaid
%% rdra-ish diagram=information scope=whole model=samples/incremental-order/step-6-business-rules/src
%% kinds: entity statemodel
%% omitted: columns usecase screen api
flowchart LR
  entity__Organization["Organization"]:::entity
  entity__Store["Store"]:::entity
  statemodel__Store_restock_status["Store.restock_status"]:::statemodel
  entity__Store ---|N:1| entity__Organization
  entity__Store -.->|statemodel| statemodel__Store_restock_status
  classDef entity stroke:#333;
  classDef statemodel stroke:#333;
```

How to read it:

- Each entity is drawn without columns. `---|N:1|` is a `relate` with its cardinality,
  from its left end.
- `-.->|statemodel|` links an entity to each column that `transitions` drive, the
  `statemodel__<Entity>_<column>` id `--kind state` uses. An `Enum` column without
  transitions is no state model.
- `-.->|variation <column>|` links an entity to the variation that column takes.

Review points:

- Do the entities and their relations match the business vocabulary of the BUCs?
- Does each entity with a lifecycle have the state model the use cases move?

## Variations and Conditions

<!-- derived-from #information-model -->

Generated from:

```sh
rdra-ish diagram samples/clinic-ops \
  --kind condition \
  --format mermaid \
  --buc BucVisitPayment \
  --out samples/clinic-ops/out/buc/condition_visit_payment
```

<!-- embed: ../samples/clinic-ops/out/buc/condition_visit_payment.mmd -->
```mermaid
%% rdra-ish diagram=condition scope=buc:BucVisitPayment model=samples/clinic-ops
%% kinds: condition statemodel variation value usecase
%% omitted: (none)
flowchart LR
  condition__CopayCappedAtLimit["窓口負担は負担割合で計算し、限度額情報があれば限度額までとする"]:::condition
  condition__UnreferredFirstVisit["紹介状なしの初診で、除外に当たらない"]:::condition
  %% holds condition__UnreferredFirstVisit: Charge.kind == selectedtreatment, Appointment.referral == unreferred
  statemodel__Appointment_status["Appointment.status"]:::statemodel
  subgraph variation__BurdenRatio["負担割合"]
    variation__BurdenRatio__thirty["3割"]:::value
    variation__BurdenRatio__twenty["2割"]:::value
  end
  subgraph variation__ChargeKind["診療費の種類"]
    variation__ChargeKind__insured["保険診療"]:::value
    variation__ChargeKind__selectedtreatment["選定療養（自費）"]:::value
  end
  subgraph variation__LimitCategory["限度額適用区分"]
    variation__LimitCategory__notobtained["限度額情報なし"]:::value
    variation__LimitCategory__obtained["適用区分を取得済"]:::value
  end
  subgraph variation__ReferralCategory["紹介の有無"]
    variation__ReferralCategory__revisit["再診"]:::value
    variation__ReferralCategory__referred["紹介状あり初診"]:::value
    variation__ReferralCategory__unreferred["紹介状なし初診"]:::value
    variation__ReferralCategory__exempt["紹介状なし初診（救急搬送等の除外）"]:::value
  end
  usecase__ApplySelectedTreatmentFee["選定療養費の算定"]:::usecase
  usecase__CalculateVisitBill["会計計算入力"]:::usecase
  condition__CopayCappedAtLimit -.->|applies_to| usecase__CalculateVisitBill
  condition__CopayCappedAtLimit -.->|combines| statemodel__Appointment_status
  condition__CopayCappedAtLimit -.->|combines| variation__BurdenRatio
  condition__CopayCappedAtLimit -.->|combines| variation__LimitCategory
  condition__UnreferredFirstVisit -.->|applies_to| usecase__ApplySelectedTreatmentFee
  condition__UnreferredFirstVisit -.->|combines| variation__ChargeKind
  %% derived condition__UnreferredFirstVisit -> variation__ChargeKind from holds
  condition__UnreferredFirstVisit -.->|combines| variation__ReferralCategory
  %% derived condition__UnreferredFirstVisit -> variation__ReferralCategory from holds
  classDef condition stroke:#333;
  classDef statemodel stroke:#333;
  classDef variation stroke:#333;
  classDef value stroke:#333;
  classDef usecase stroke:#333;
```

How to read it:

- Each condition links `-.->|combines|` to the variations and state models it combines,
  and `-.->|applies_to|` to the use cases it guards.
- `CopayCappedAtLimit` declares its links with `combines`. `UnreferredFirstVisit` has a
  `holds` body, printed on its `%% holds` line, so its links are read from the body and
  each is followed by `%% derived ... from holds`.
- Each variation is a subgraph holding its values as `:::value` nodes. The whole model
  also draws the variations no condition uses.

Review points:

- Does every condition apply to the use case whose rule it selects?
- Does every value of a combined variation have a rule, or is one missing?

## State Model

<!-- derived-from #information-model -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind state \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/state
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/state.mmd -->
```mermaid
%% rdra-ish diagram=state scope=whole model=samples/incremental-order/step-6-business-rules/src
%% kinds: statemodel state usecase
%% omitted: (none)
stateDiagram-v2
  state "Store.restock_status" as statemodel__Store_restock_status {
    [*] --> state__Store_normal
    state "normal" as state__Store_normal
    state "scheduled" as state__Store_scheduled
    state "blocked" as state__Store_blocked
    state__Store_normal --> state__Store_scheduled : usecase__ChangeNextRestockDate
    state__Store_scheduled --> state__Store_blocked : usecase__BlockScheduledRestock
    state__Store_blocked --> [*]
  }
```

How to read it:

- Each state model (a lifecycle column) is one composite state,
  `statemodel__Store_restock_status` labelled `Store.restock_status`.
- `[*] -->` marks the start state, the value the lifecycle gives the column on creation
  (or its default for an entity that starts present). `--> [*]` marks an end state, one
  with no transition to another state.
- Each edge is labelled with the id of the use case that drives the transition, as the
  遷移UC column of an RDRA state sheet is; `usecase__BlockScheduledRestock` is the same
  node in the UC複合図.
- This diagram shows lifecycle topology. Use `rdra-ish states` for rule satisfaction,
  reachable combinations, and diagnostics. `state` is the one kind that refuses to write
  while `check` reports a lifecycle error.

Review points:

- `blocked` is an end state: is it terminal by design, or is a way out missing?
- Are business rules such as `scheduled` requiring `next_restock_date` covered by
  state-pattern diagnostics?

## System-to-System Events

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/ec-site \
  --kind event \
  --format mermaid \
  --out samples/ec-site/out/event
```

<!-- embed: ../samples/ec-site/out/event.mmd -->
```mermaid
%% rdra-ish diagram=event scope=whole model=samples/ec-site
%% kinds: event usecase extsystem
%% omitted: state transitions
flowchart LR
  event__EvOrderShipped["Order Shipped"]:::event
  usecase__ShipOrder["Ship Order"]:::usecase
  usecase__ShipOrder -.->|raises| event__EvOrderShipped
  extsystem__ShippingSvc["Shipping Service"]:::extsystem
  event__EvOrderShipped -.->|triggers| extsystem__ShippingSvc
  event__EvPaymentAuthorized["Payment Authorized"]:::event
  extsystem__PaymentGW["Payment Gateway"]:::extsystem
  extsystem__PaymentGW -.->|raises| event__EvPaymentAuthorized
  usecase__Capture["Capture Payment"]:::usecase
  event__EvPaymentAuthorized -.->|triggers| usecase__Capture
  classDef event stroke:#333;
  classDef usecase stroke:#333;
  classDef extsystem stroke:#333;
```

How to read it:

- `raises` runs from a use case or an external system to an event, and `triggers` from
  an event (or a timer) to the use case, BUC, or external system it starts.
- States are left to `--kind state`; `%% omitted: state transitions` says a drawn use
  case drives one.
- The command also prints a warning for an event never raised or never consumed.

Review points:

- Does every event that crosses the system boundary have both ends?
- Do events represent domain facts rather than UI button clicks?

## ER Diagram

<!-- derived-from #information-model -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind er \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/er
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/er.mmd -->
```mermaid
%% rdra-ish diagram=er scope=whole model=samples/incremental-order/step-6-business-rules/src
%% kinds: entity
%% omitted: (none)
erDiagram
  Organization {
    Int id PK
    String code
    String name
  }
  Store {
    Int id PK
    String code
    String name
    Enum restock_status
    DateTime next_restock_date
    Int organization_id FK
  }
  Store }o--|| Organization : ""
```

How to read it:

- Treat `entity` as the logical data model, not the conceptual model the information
  model draws. Entity names in the `erDiagram` body are the DSL ids.
- Cardinality comes from `relate`; generated FK columns show where the ownership of a
  reference lands.
- State axes such as `restock_status` are data columns and lifecycle review inputs.

Review points:

- Is `Store -> Organization` optionality correct for business operations?
- Are indexes, uniqueness, tenant scope, history, or soft-delete annotations needed
  before this becomes a migration design?

## RDRA Graph

<!-- derived-from #reading-any-diagram -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind rdra \
  --format mermaid \
  --buc BucStoreRestock \
  --out samples/incremental-order/step-6-business-rules/out/object_graph_buc_store_restock
```

<!-- embed: ../samples/incremental-order/step-6-business-rules/out/object_graph_buc_store_restock.mmd -->
```mermaid
%% rdra-ish diagram=rdra scope=buc:BucStoreRestock model=samples/incremental-order/step-6-business-rules/src
%% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム
%% kinds: actor business buc medium permission usecase screen api entity state
%% omitted: (none)
flowchart LR
  subgraph layer_value["システム価値"]
    actor__OpsStaff["Operations Staff"]:::actor
  end
  subgraph layer_environment["システム外部環境"]
    business__StoreOperations["Store Operations"]:::business
    %% desc business__StoreOperations: Store operations owns restock planning and store master maintenance.
    buc__BucStoreRestock["Maintain Store Restock"]:::buc
    %% desc buc__BucStoreRestock: Keep restock dates and store organization assignments operationally correct.
    medium__OpsConsole["Operations Console"]:::medium
    permission__StoreMaintenanceWrite["Store Maintenance Write"]:::permission
  end
  subgraph layer_boundary["システム境界"]
    usecase__BlockScheduledRestock["Block Scheduled Restock"]:::usecase
    usecase__ChangeNextRestockDate["Change Next Restock Date"]:::usecase
    usecase__ChangeStoreParentOrganization["Change Store Parent Organization"]:::usecase
    screen__StoreMaintenanceScreen["Store Maintenance"]:::screen
  end
  subgraph layer_system["システム"]
    api__OrganizationLookupApi["Organization Lookup API"]:::api
    api__StoreAdminApi["Store Admin API"]:::api
    entity__Organization["Organization"]:::entity
    entity__Store["Store"]:::entity
    state__Store_blocked["blocked"]:::state
    state__Store_normal["normal"]:::state
    state__Store_scheduled["scheduled"]:::state
  end
  actor__OpsStaff -.->|has_permission| permission__StoreMaintenanceWrite
  actor__OpsStaff -.->|performs| buc__BucStoreRestock
  api__OrganizationLookupApi -.->|reads| entity__Organization
  api__StoreAdminApi -.->|requires_permission| permission__StoreMaintenanceWrite
  api__StoreAdminApi -.->|updates| entity__Store
  buc__BucStoreRestock -->|belongs| business__StoreOperations
  buc__BucStoreRestock -->|contains| usecase__BlockScheduledRestock
  buc__BucStoreRestock -->|contains| usecase__ChangeNextRestockDate
  buc__BucStoreRestock -->|contains| usecase__ChangeStoreParentOrganization
  entity__Store ---|N:1| entity__Organization
  screen__StoreMaintenanceScreen -.->|shows| entity__Organization
  screen__StoreMaintenanceScreen -.->|shows| entity__Store
  usecase__BlockScheduledRestock -.->|displays| screen__StoreMaintenanceScreen
  usecase__BlockScheduledRestock -.->|requires_medium| medium__OpsConsole
  usecase__BlockScheduledRestock -.->|requires_permission| permission__StoreMaintenanceWrite
  usecase__BlockScheduledRestock -.->|transitions Store.restock_status scheduled->blocked| state__Store_blocked
  usecase__BlockScheduledRestock -.->|updates| entity__Store
  usecase__ChangeNextRestockDate -.->|displays| screen__StoreMaintenanceScreen
  usecase__ChangeNextRestockDate -.->|requires_medium| medium__OpsConsole
  usecase__ChangeNextRestockDate -.->|requires_permission| permission__StoreMaintenanceWrite
  usecase__ChangeNextRestockDate -.->|transitions Store.restock_status normal->scheduled| state__Store_scheduled
  usecase__ChangeNextRestockDate -.->|updates| entity__Store
  usecase__ChangeStoreParentOrganization -.->|displays| screen__StoreMaintenanceScreen
  usecase__ChangeStoreParentOrganization -.->|invokes| api__OrganizationLookupApi
  usecase__ChangeStoreParentOrganization -.->|invokes| api__StoreAdminApi
  usecase__ChangeStoreParentOrganization -.->|requires_medium| medium__OpsConsole
  usecase__ChangeStoreParentOrganization -.->|requires_permission| permission__StoreMaintenanceWrite
  classDef actor stroke:#333;
  classDef business stroke:#333;
  classDef buc stroke:#333;
  classDef medium stroke:#333;
  classDef permission stroke:#333;
  classDef usecase stroke:#333;
  classDef screen stroke:#333;
  classDef api stroke:#333;
  classDef entity stroke:#333;
  classDef state stroke:#333;
```

How to read it:

- `rdra` is the base graph: every element in its RDRA layer and every relation, the
  APIs, permissions, and media included. The other kinds are views of it, and a node or
  declared edge they draw is the one drawn here.
- Read the four subgraphs as the RDRA layers システム価値, システム外部環境,
  システム境界, and システム.
- Narrow a dense graph with `--node-kind` and `--edge-kind`; `omitted:` names what they
  hid.

Review points:

- Does every required permission have a matching actor-side grant?
- Are the API paths of each use case the ones the UC複合図 folded?

## Summary

<!-- derived-from #system-context -->
<!-- derived-from #uc複合図 -->
<!-- derived-from #information-model -->
<!-- derived-from #state-model -->

Walk the kinds in the order above: the system context and requirements say who and why,
the business, UC複合図, and flow say what happens, the information model and conditions
say what it acts on, and the state model and events say how it changes. `er` and `rdra`
hold the detail behind each. The transaction and API-boundary warnings come from
`rdra-ish check`.
