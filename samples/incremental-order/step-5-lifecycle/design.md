# 店舗補充管理 設計 Step 5: Lifecycle

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- derived-from ./requirements-analysis.md -->

この文書は Step 5 時点の RDRA DSL 設計サンプルです。clinic-ops の設計書と同じく、レビューに必要な生成物は本文へ埋め込みます。

## 1. 設計目的

状態、イベント、sets を追加する。

## 2. モデル構成

| 分類 | 対象 | 役割 |
|---|---|---|
| State axis | `restock_status` | normal, scheduled, blocked |
| Nullable axis | `next_restock_date` | null / present |
| Event | `RestockScheduled` | normal -> scheduled と予定日 present |
| Event | `RestockBlocked` | scheduled -> blocked と予定日 null |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| restock_status を Enum にする | 主要な業務状態を状態軸として導出するため |
| next_restock_date を @null にする | 予定日の有無を状態パターンとして検証するため |
| sets を event に付ける | usecase 操作ではなく状態遷移イベントの効果として説明するため |

## 4. 生成成果物

生成コマンド例:

```sh
rdra-ish check samples/incremental-order/step-5-lifecycle/src
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/object_graph_buc_store_restock
rdra-ish csv samples/incremental-order/step-5-lifecycle/src --kind matrix --out samples/incremental-order/step-5-lifecycle/out/usecase_matrix.csv
rdra-ish csv samples/incremental-order/step-5-lifecycle/src --kind screen-constraints --out samples/incremental-order/step-5-lifecycle/out/screen_constraints.csv
rdra-ish csv samples/incremental-order/step-5-lifecycle/src --kind actor-permission-audit --out samples/incremental-order/step-5-lifecycle/out/actor_permission_audit.csv
```

### 4.1 RDRA Layered Graph 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/object_graph_buc_store_restock
```

<!-- embed: out/object_graph_buc_store_restock.mmd -->
```mermaid
%% rdra-ish diagram=rdra scope=buc:BucStoreRestock model=samples/incremental-order/step-5-lifecycle/src
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

### 4.3 ER 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind er --format mermaid --out samples/incremental-order/step-5-lifecycle/out/er
```

<!-- embed: out/er.mmd -->
```mermaid
%% rdra-ish diagram=er scope=whole model=samples/incremental-order/step-5-lifecycle/src
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

### 4.4 State 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind state --format mermaid --out samples/incremental-order/step-5-lifecycle/out/state
```

<!-- embed: out/state.mmd -->
```mermaid
%% rdra-ish diagram=state scope=whole model=samples/incremental-order/step-5-lifecycle/src
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

### 4.5 Usecase CRUD matrix

<!-- embed: out/usecase_matrix.csv -->
```csv
UseCase,Organization,Store
BlockScheduledRestock,,U
ChangeNextRestockDate,,U
ChangeStoreParentOrganization,,
```

### 4.6 API CRUD matrix

<!-- embed: out/api_matrix.csv -->
```csv
Api,Organization,Store
OrganizationLookupApi,R,
StoreAdminApi,,U
```

### 4.7 Store 状態到達表

<!-- embed: out/states_store.txt -->
```text
Entity: Store (Store)
  axes: restock_status[normal|scheduled|blocked], next_restock_date[null|present]

  RESTOCK_STATUS  NEXT_RESTOCK_DATE  INITIAL  TERMINAL  VIA
  ──────────────  ─────────────────  ───────  ────────  ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  normal          null               yes      no        BucStoreRestock/ChangeStoreParentOrganization
  scheduled       present            no       no        BucStoreRestock/ChangeNextRestockDate, BucStoreRestock/ChangeStoreParentOrganization
  blocked         null               no       yes       BucStoreRestock/BlockScheduledRestock, BucStoreRestock/ChangeNextRestockDate, BucStoreRestock/ChangeStoreParentOrganization

  reachable: 3 / bound: 6
  diagnostics:
    [info][NoCreationPath_Store] no creates(...) found; seeded from column defaults
```

## 5. レビュー観点

- normal -> scheduled -> blocked 以外の遷移が必要か。
- blocked から normal へ戻す UC を今入れるべきか。
- next_restock_date の present/null が業務状態を十分に説明しているか。

## 6. 承認条件

| 観点 | 承認条件 |
|---|---|
| 要求 | requirements-analysis.md の Must 要求を説明できる |
| 設計 | この step で追加した DSL 要素の責務を説明できる |
| 生成物 | 埋め込み成果物が現在の DSL から生成されている |
| 次 step | 次に具体化する情報と、まだ具体化しない情報を区別できる |

## Summary

<!-- derived-from #2-モデル構成 -->
<!-- derived-from #3-設計判断 -->
<!-- derived-from #4-生成成果物 -->

Step 5 の設計は、状態、イベント、sets を追加するための最小 DSL と生成成果物を提示する。
