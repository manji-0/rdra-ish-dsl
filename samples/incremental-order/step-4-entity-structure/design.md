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
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/object_graph_buc_store_restock
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind sequence --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/sequence_buc_store_restock
rdra-ish csv samples/incremental-order/step-4-entity-structure/src --kind matrix --out samples/incremental-order/step-4-entity-structure/out/usecase_matrix.csv
rdra-ish csv samples/incremental-order/step-4-entity-structure/src --kind screen-constraints --out samples/incremental-order/step-4-entity-structure/out/screen_constraints.csv
rdra-ish csv samples/incremental-order/step-4-entity-structure/src --kind actor-permission-audit --out samples/incremental-order/step-4-entity-structure/out/actor_permission_audit.csv
```

### 4.1 RDRA Layered Graph 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-4-entity-structure/out/object_graph_buc_store_restock
```

```mermaid
%% rdra-ish diagram=rdra scope=buc:BucStoreRestock model=samples/incremental-order/step-4-entity-structure/src
%% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム
%% kinds: actor business buc medium permission usecase screen api entity
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
    usecase__ChangeNextRestockDate["Change Next Restock Date"]:::usecase
    usecase__ChangeStoreParentOrganization["Change Store Parent Organization"]:::usecase
    screen__StoreMaintenanceScreen["Store Maintenance"]:::screen
  end
  subgraph layer_system["システム"]
    api__OrganizationLookupApi["Organization Lookup API"]:::api
    api__StoreAdminApi["Store Admin API"]:::api
    entity__Organization["Organization"]:::entity
    entity__Store["Store"]:::entity
  end
  actor__OpsStaff -.->|has_permission| permission__StoreMaintenanceWrite
  actor__OpsStaff -.->|performs| buc__BucStoreRestock
  api__OrganizationLookupApi -.->|reads| entity__Organization
  api__StoreAdminApi -.->|requires_permission| permission__StoreMaintenanceWrite
  api__StoreAdminApi -.->|updates| entity__Store
  buc__BucStoreRestock -->|belongs| business__StoreOperations
  buc__BucStoreRestock -->|contains| usecase__ChangeNextRestockDate
  buc__BucStoreRestock -->|contains| usecase__ChangeStoreParentOrganization
  entity__Store ---|N:1| entity__Organization
  screen__StoreMaintenanceScreen -.->|shows| entity__Organization
  screen__StoreMaintenanceScreen -.->|shows| entity__Store
  usecase__ChangeNextRestockDate -.->|displays| screen__StoreMaintenanceScreen
  usecase__ChangeNextRestockDate -.->|requires_medium| medium__OpsConsole
  usecase__ChangeNextRestockDate -.->|requires_permission| permission__StoreMaintenanceWrite
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
