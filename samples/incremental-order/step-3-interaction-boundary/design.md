# 店舗補充管理 設計 Step 3: Interaction Boundary

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- derived-from ./requirements-analysis.md -->

この文書は Step 3 時点の RDRA DSL 設計サンプルです。clinic-ops の設計書と同じく、レビューに必要な生成物は本文へ埋め込みます。

## 1. 設計目的

画面、API、System 境界に加えて、Business-BUC context と UC/API の権限・媒体制約を追加する。

## 2. モデル構成

| 分類 | 対象 | 役割 |
|---|---|---|
| Screen | `StoreMaintenanceScreen` | 店舗情報、補充予定、担当組織候補を確認しながら更新する |
| API | `StoreAdminApi` | 店舗情報の更新境界 |
| API | `OrganizationLookupApi` | 組織マスタの参照境界 |
| System | `StoreAdminSystem / OrganizationSystem` | 店舗情報と組織マスタの仮の所有境界 |
| Context | `OpsConsole` | BUC が適用される操作媒体 |
| Access | `StoreMaintenanceWrite` | 店舗保守操作に必要な権限 |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| ChangeNextRestockDate は direct CRUD のまま残す | 店舗単体更新であり、独立 API を導入するほどの整合性境界がまだない |
| ChangeStoreParentOrganization は invokes を使う | 組織参照と店舗更新が関わり、後続で system 境界診断の対象になるため |
| read-only API を API matrix で確認する | API ごとの CRUD と、`check` が出す境界診断で境界をレビューするため |
| Screen 制約は直接書かない | `displays` と `invokes` から UC/API の `requires_*` を導出できるため |

## 4. 生成成果物

生成コマンド例:

```sh
rdra-ish check samples/incremental-order/step-3-interaction-boundary/src
rdra-ish diagram samples/incremental-order/step-3-interaction-boundary/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-3-interaction-boundary/out/object_graph_buc_store_restock
rdra-ish csv samples/incremental-order/step-3-interaction-boundary/src --kind matrix --out samples/incremental-order/step-3-interaction-boundary/out/usecase_matrix.csv
rdra-ish csv samples/incremental-order/step-3-interaction-boundary/src --kind screen-constraints --out samples/incremental-order/step-3-interaction-boundary/out/screen_constraints.csv
rdra-ish csv samples/incremental-order/step-3-interaction-boundary/src --kind actor-permission-audit --out samples/incremental-order/step-3-interaction-boundary/out/actor_permission_audit.csv
```

### 4.0 Screen 制約パターン

<!-- embed: out/screen_constraints.csv -->
```csv
screen_id,usecase_id,api_id,required_permissions,required_media
StoreMaintenanceScreen,ChangeNextRestockDate,,StoreMaintenanceWrite,OpsConsole
StoreMaintenanceScreen,ChangeStoreParentOrganization,OrganizationLookupApi,StoreMaintenanceWrite,OpsConsole
StoreMaintenanceScreen,ChangeStoreParentOrganization,StoreAdminApi,StoreMaintenanceWrite,OpsConsole
```

### 4.1 Actor 権限割当監査

<!-- embed: out/actor_permission_audit.csv -->
```csv
actor_id,actor_label,permission_id,permission_label,assigned,required,status,required_usecase_ids,required_api_paths
OpsStaff,Operations Staff,StoreMaintenanceWrite,Store Maintenance Write,true,true,ok,ChangeNextRestockDate|ChangeStoreParentOrganization,ChangeStoreParentOrganization->StoreAdminApi
```

### 4.2 RDRA Layered Graph 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-3-interaction-boundary/src --kind rdra --format mermaid --buc BucStoreRestock --out samples/incremental-order/step-3-interaction-boundary/out/object_graph_buc_store_restock
```

<!-- embed: out/object_graph_buc_store_restock.mmd -->
```mermaid
%% rdra-ish diagram=rdra scope=buc:BucStoreRestock model=samples/incremental-order/step-3-interaction-boundary/src
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
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef business fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef buc fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef medium fill:#f1f3f5,stroke:#333,color:#000;
  classDef permission fill:#f1f3f5,stroke:#333,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef screen fill:#e5dbff,stroke:#5f3dc4,color:#000;
  classDef api fill:#c5f6fa,stroke:#0b7285,color:#000;
  classDef entity fill:#fff9db,stroke:#5c940d,color:#000;
```

### 4.4 ER 図

生成コマンド:

```sh
rdra-ish diagram samples/incremental-order/step-3-interaction-boundary/src --kind er --format mermaid --out samples/incremental-order/step-3-interaction-boundary/out/er
```

<!-- embed: out/er.mmd -->
```mermaid
%% rdra-ish diagram=er scope=whole model=samples/incremental-order/step-3-interaction-boundary/src
%% kinds: entity
%% omitted: (none)
erDiagram
  Organization {
    Int id PK
  }
  Store {
    Int id PK
  }
```

### 4.5 Usecase CRUD matrix

<!-- embed: out/usecase_matrix.csv -->
```csv
UseCase,Organization,Store
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
  (no state axes)
  reachable: 1 / bound: 1
```

## 5. レビュー観点

- direct CRUD と API CRUD の混在が、この段階の分析として説明できるか。
- StoreAdminApi と OrganizationLookupApi を同じ System に入れるべきではないか。
- 次 step で relate(Store, Organization, N:1) を置いたとき、coordination が必要になるか。

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

Step 3 の設計は、画面、API、System 境界を追加するための最小 DSL と生成成果物を提示する。
