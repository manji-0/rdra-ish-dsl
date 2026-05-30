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
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/rdra_buc_store_restock
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind sequence --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/sequence_buc_store_restock
rdra-ish csv samples/incremental-order/step-5-lifecycle/src --kind matrix --out samples/incremental-order/step-5-lifecycle/out/usecase_matrix.csv
```

### 4.1 RDRA 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/rdra_buc_store_restock
```

```mermaid
graph TD
  OpsStaff(["👤 Operations Staff"])
  BlockScheduledRestock(["✅ Block Scheduled Restock"])
  ChangeNextRestockDate(["✅ Change Next Restock Date"])
  ChangeStoreParentOrganization(["✅ Change Store Parent Organization"])
  BucStoreRestock["📦 Maintain Store Restock"]
  Organization[("🗄️ Organization")]
  Store[("🗄️ Store")]
  StoreMaintenanceScreen[["🖥️ Store Maintenance"]]
  RestockBlocked{"⚡ Restock Blocked"}
  RestockScheduled{"⚡ Restock Scheduled"}
  OpsStaff --> BucStoreRestock
  BucStoreRestock --> StoreOperations
  BucStoreRestock --> ChangeNextRestockDate
  BucStoreRestock --> ChangeStoreParentOrganization
  BucStoreRestock --> BlockScheduledRestock
  Store --- Organization
  StoreMaintenanceScreen -.->|shows| Store
  StoreMaintenanceScreen -.->|shows| Store
  StoreMaintenanceScreen -.->|shows| Store
  StoreMaintenanceScreen -.->|shows| Organization
  ChangeNextRestockDate -.->|updates| Store
  ChangeNextRestockDate -.->|raises| RestockScheduled
  ChangeNextRestockDate -.->|displays| StoreMaintenanceScreen
  ChangeStoreParentOrganization -.->|displays| StoreMaintenanceScreen
  BlockScheduledRestock -.->|updates| Store
  BlockScheduledRestock -.->|raises| RestockBlocked
  BlockScheduledRestock -.->|displays| StoreMaintenanceScreen
```

### 4.2 Sequence 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind sequence --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-5-lifecycle/out/sequence_buc_store_restock
```

```mermaid
sequenceDiagram
  box システム価値
    actor OpsStaff as 👤 Operations Staff
  end
  box システム境界
    participant StoreMaintenanceScreen as 🖥️ Store Maintenance
    participant OrganizationLookupApi as 🔌 Organization Lookup API
    participant StoreAdminApi as 🔌 Store Admin API
  end
  box システム
    participant System as 🧩 システム
    participant Organization as 🗄️ Organization
    participant Store as 🗄️ Store
  end

  Note over OpsStaff,Store: ✅ Block Scheduled Restock
  OpsStaff->System: ✅ Block Scheduled Restock
  activate System
  System->>Store: update
  System-->>OpsStaff: 🖥️ Store Maintenance
  deactivate System

  Note over OpsStaff,Store: ✅ Change Next Restock Date
  OpsStaff->System: ✅ Change Next Restock Date
  activate System
  System->>Store: update
  System-->>OpsStaff: 🖥️ Store Maintenance
  deactivate System

  Note over OpsStaff,Store: ✅ Change Store Parent Organization
  OpsStaff->>StoreMaintenanceScreen: ✅ Change Store Parent Organization
  StoreMaintenanceScreen->>StoreAdminApi: ✅ Change Store Parent Organization
  activate StoreAdminApi
  OrganizationLookupApi->>Organization: read
  StoreAdminApi->>Store: update
  StoreAdminApi-->>StoreMaintenanceScreen: 🖥️ Store Maintenance
  StoreMaintenanceScreen-->>OpsStaff: 🖥️ Store Maintenance
  deactivate StoreAdminApi
```

### 4.3 ER 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind er --format mermaid --out samples/incremental-order/step-5-lifecycle/out/er
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

### 4.4 State 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind state --format mermaid --out samples/incremental-order/step-5-lifecycle/out/state
```

```mermaid
stateDiagram-v2
  [*] --> Normal
  state "🔄 Normal" as Normal
  state "🔄 Scheduled" as Scheduled
  state "🔄 Blocked" as Blocked
  Normal --> Scheduled : ⚡ Restock Scheduled
  Scheduled --> Blocked : ⚡ Restock Blocked
```

### 4.5 Usecase CRUD matrix

```csv
UseCase,Organization,Store
BlockScheduledRestock,,U
ChangeNextRestockDate,,U
ChangeStoreParentOrganization,,
```

### 4.6 API CRUD matrix

```csv
Api,Organization,Store
OrganizationLookupApi,R,
StoreAdminApi,,U
```

### 4.7 Store 状態到達表

```text
Entity: Store (Store)
  axes: restock_status[normal|scheduled|blocked], next_restock_date[null|present:timestamptz]

  RESTOCK_STATUS  NEXT_RESTOCK_DATE    INITIAL  TERMINAL  VIA
  ──────────────  ───────────────────  ───────  ────────  ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  normal          null                 yes      no        BucStoreRestock/ChangeStoreParentOrganization
  scheduled       present:timestamptz  no       no        BucStoreRestock/ChangeNextRestockDate, BucStoreRestock/ChangeStoreParentOrganization
  blocked         null                 no       yes       BucStoreRestock/BlockScheduledRestock, BucStoreRestock/ChangeNextRestockDate, BucStoreRestock/ChangeStoreParentOrganization

  reachable: 3 / bound: 6
  diagnostics:
    [info] no creates(...) found; seeded from column defaults
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
