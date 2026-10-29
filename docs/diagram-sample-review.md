# Diagram Sample Review Guide

<!-- constrained-by ./cli-reference.md#diagram -->
<!-- constrained-by ./incremental-modeling.md -->
<!-- derived-from ../samples/incremental-order/step-6-business-rules/design.md -->

This guide embeds representative generated diagrams from the incremental order sample
and explains how to read each one during review. The examples use
`samples/incremental-order/step-6-business-rules`, a small model that still includes
business scope, permissions, UI, APIs, entities, lifecycle events, and state rules.

Regenerate the artifacts with:

```sh
bash scripts/check-sample-artifacts.sh
```

The same command runs in CI, so intentional emitter or DSL changes should appear as a
normal diff under `samples/incremental-order/step-6-business-rules/out/`.

CI snapshots the Mermaid text (`.mmd`), the only diagram format `rdra-ish` writes.
An image is a rendering of that text by an external tool such as Mermaid CLI (`mmdc`);
keep rendered images as local review conveniences.

## RDRA Layered Graph

<!-- derived-from #diagram-sample-review-guide -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind rdra \
  --format mermaid \
  --buc BucStoreRestock \
  --out samples/incremental-order/step-6-business-rules/out/object_graph_buc_store_restock
```

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

- The `%%` header names the kind, the BUC scope, and the node kinds drawn. Each node id
  is `<kind>__<DSL id>`, so `usecase__ChangeNextRestockDate` is the same element in
  every diagram that draws it.
- Read the four subgraphs left to right as the RDRA layers システム価値, システム外部環境,
  システム境界, and システム.
- Start from the actor and BUC, then follow `contains` (a solid arrow, as every
  hierarchy edge is) into concrete use cases.
- Treat permissions and media as review constraints on the use-case/API path, not as
  decorative labels.
- A `transitions` edge runs from the use case to the state it enters and names the
  column and the from/to values, so the lifecycle each use case drives reads off this
  graph before the state diagram.
- `%% desc` lines carry each element's description; a review that needs the prose
  reads them, and a renderer ignores them.

Review points:

- Does every use case in the BUC still explain the BUC's business value?
- Are direct entity writes intentional, or should they move behind an API boundary?
- Does every required permission have a matching actor-side grant?
- Are event-raising use cases paired with downstream flow or state review?

## ER Diagram

<!-- derived-from #rdra-layered-graph -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind er \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/er
```

```mermaid
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

- Treat `entity` as the logical data model, not the conceptual model.
- Cardinality comes from `relate`; generated FK columns show where the ownership of a
  reference lands.
- State axes such as `restock_status` are data columns and lifecycle review inputs.

Review points:

- Is `Store -> Organization` optionality correct for business operations?
- Are state columns explicit enough for state derivation and rule review?
- Do entity names still match the business vocabulary used in BUCs and use cases?
- Are indexes, uniqueness, tenant scope, history, or soft-delete annotations needed
  before this becomes a migration design?

## State Diagram

<!-- derived-from #er-diagram -->

Generated from:

```sh
rdra-ish diagram samples/incremental-order/step-6-business-rules/src \
  --kind state \
  --format mermaid \
  --out samples/incremental-order/step-6-business-rules/out/state
```

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
  node in the layered graph.
- This diagram shows lifecycle topology. Use `rdra-ish states` for rule satisfaction,
  reachable combinations, and diagnostics.

Review points:

- `blocked` is an end state: is it terminal by design, or is a way out missing?
- Are business rules such as `scheduled` requiring `next_restock_date` covered by
  state-pattern diagnostics?
- Do events represent domain facts rather than UI button clicks?
- Are event-triggered downstream BUCs or use cases missing from the model?

## Summary

<!-- derived-from #rdra-layered-graph -->
<!-- derived-from #er-diagram -->
<!-- derived-from #state-diagram -->

Use the layered graph to confirm coverage and interaction boundaries, the ER diagram
to review logical data structure, and the state diagram to review lifecycle topology.
A useful review usually walks them in that order: business value first, then data
shape, then lifecycle rules. The transaction and API-boundary warnings come from
`rdra-ish check`.
