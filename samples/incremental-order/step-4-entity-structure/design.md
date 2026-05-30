# 店舗補充管理 設計 Step 4: Entity Structure

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- derived-from ./requirements-analysis.md -->

この文書は Step 4 時点の RDRA DSL 設計サンプルです。clinic-ops の設計書と同じく、レビューに必要な生成物は本文へ埋め込みます。

## 1. 設計目的

columns、ER、境界越え coordination を追加する。

## 2. モデル構成

| 分類 | 対象 | 役割 |
|---|---|---|
| Entity | `Store` | id, code, name, organization_id |
| Entity | `Organization` | id, code, name |
| Relation | `Store -> Organization` | 店舗は 1 つの担当組織に属する |
| Coordination | `ChangeStoreParentOrganization` | 境界越え関係の整合性を調整する |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| code を @unique にする | 業務レビューでは ID よりコードで店舗・組織を識別するため |
| 変更履歴 entity は追加しない | 監査・履歴保持が未確定のため |
| coordinates を追加する | System 境界をまたぐ ER 関係を説明するため |

## 4. 生成成果物

生成コマンド例:

```sh
rdra-ish check samples/incremental-order/step-4-entity-structure/src
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind object-graph --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/object_graph_buc_store_restock
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind sequence --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/sequence_buc_store_restock
rdra-ish csv samples/incremental-order/step-4-entity-structure/src --kind matrix --out samples/incremental-order/step-4-entity-structure/out/usecase_matrix.csv
```

### 4.1 Layered Object Graph 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind object-graph --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/object_graph_buc_store_restock
```

```mermaid
flowchart LR
  subgraph layer_value[System Value]
    direction TB
    OpsStaff(["👤 Operations Staff"])
  end
  subgraph layer_environment[External Environment]
    direction TB
    StoreOperations["💼 Store Operations"]
    BucStoreRestock["📦 Maintain Store Restock"]
  end
  subgraph layer_boundary[System Boundary]
    direction TB
    ChangeNextRestockDate(["✅ Change Next Restock Date"])
    ChangeStoreParentOrganization(["✅ Change Store Parent Organization"])
    StoreMaintenanceScreen[["🖥️ Store Maintenance"]]
  end
  subgraph layer_system[System]
    direction TB
    OrganizationLookupApi["🔌 Organization Lookup API"]
    StoreAdminApi["🔌 Store Admin API"]
    Organization[("🗄️ Organization")]
    Store[("🗄️ Store")]
  end
  OpsStaff -->|performs| BucStoreRestock
  StoreAdminApi -.->|updates| Store
  OrganizationLookupApi -.->|reads| Organization
  BucStoreRestock -.->|belongs| StoreOperations
  BucStoreRestock -->|contains| ChangeNextRestockDate
  BucStoreRestock -->|contains| ChangeStoreParentOrganization
  Store ---|N:1| Organization
  StoreMaintenanceScreen -.->|shows| Store
  StoreMaintenanceScreen -.->|shows| Store
  StoreMaintenanceScreen -.->|shows| Organization
  ChangeNextRestockDate -.->|displays| StoreMaintenanceScreen
  ChangeNextRestockDate -.->|updates| Store
  ChangeStoreParentOrganization -.->|displays| StoreMaintenanceScreen
  ChangeStoreParentOrganization -.->|invokes| StoreAdminApi
  ChangeStoreParentOrganization -.->|invokes| OrganizationLookupApi
```

### 4.2 Sequence 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind sequence --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/sequence_buc_store_restock
```

```mermaid
sequenceDiagram
  box System Value
    actor OpsStaff as 👤 Operations Staff
  end
  box System Boundary
    participant StoreMaintenanceScreen as 🖥️ Store Maintenance
  end
  box System
    participant System as 🧩 システム
    participant OrganizationLookupApi as 🔌 Organization Lookup API
    participant StoreAdminApi as 🔌 Store Admin API
    participant Organization as 🗄️ Organization
    participant Store as 🗄️ Store
  end

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
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind er --format mermaid --out samples/incremental-order/step-4-entity-structure/out/er
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
    Int organization_id FK
  }
  Store }o--|| Organization : ""
```

### 4.5 Usecase CRUD matrix

```csv
UseCase,Organization,Store
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
  (no state axes)
  reachable: 1 / bound: 1
```

## 5. レビュー観点

- Store と Organization の関係が N:1 でよいか。
- coordinates の責務を ChangeStoreParentOrganization に置くことが自然か。
- 店舗コードと組織コードだけでレビューに十分か。

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

Step 4 の設計は、columns、ER、境界越え coordination を追加するための最小 DSL と生成成果物を提示する。
