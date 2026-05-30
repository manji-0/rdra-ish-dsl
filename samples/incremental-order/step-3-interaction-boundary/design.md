# 店舗補充管理 設計 Step 3

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- constrained-by ../../../docs/language-reference.md#api-and-the-api-layer -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

画面、API、System の境界を追加し、usecase がどこで UI から処理境界へ渡るかを確認できるようにする。Step 2 の direct CRUD は残しつつ、整合性境界が必要な担当組織変更だけ API 化する。

## 2. モデル構成

| 分類 | ID | 役割 |
|---|---|---|
| Screen | `StoreMaintenanceScreen` | 店舗補充管理の操作画面 |
| API | `StoreAdminApi` | 店舗情報の更新境界 |
| API | `OrganizationLookupApi` | 組織マスタの参照境界 |
| System | `StoreAdminSystem` | 店舗情報を扱う内部境界 |
| System | `OrganizationSystem` | 組織マスタを扱う内部境界 |

## 3. 関係設計

| Use case | Screen | API | Entity operation |
|---|---|---|---|
| `ChangeNextRestockDate` | `StoreMaintenanceScreen` | なし | `updates(ChangeNextRestockDate, Store)` |
| `ChangeStoreParentOrganization` | `StoreMaintenanceScreen` | `StoreAdminApi` | `updates(StoreAdminApi, Store)` |
| `ChangeStoreParentOrganization` | `StoreMaintenanceScreen` | `OrganizationLookupApi` | `reads(OrganizationLookupApi, Organization)` |

## 4. 設計判断

- `ChangeNextRestockDate` は direct CRUD のまま残す。店舗単体更新であり、独立 API を導入するほどの整合性境界がまだない。
- `ChangeStoreParentOrganization` は `invokes` を使う。組織参照と店舗更新が関わり、後続で system 境界診断の対象になるため。
- read-only API は sequence 図では目立ちにくいため、API matrix を正式なレビュー成果物に含める。

## 5. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-3-interaction-boundary/src
rdra-ish diagram samples/incremental-order/step-3-interaction-boundary/src --kind sequence --format mermaid --buc BucStoreRestock
rdra-ish csv samples/incremental-order/step-3-interaction-boundary/src --kind api-matrix
```

期待結果:

- sequence 図で `ChangeStoreParentOrganization` が Screen -> API -> Entity の経路を持つ。
- `ChangeNextRestockDate` は legacy `System` lane の直接更新として残る。
- API matrix で `OrganizationLookupApi` が `Organization` を Read する。

## 6. レビュー観点

- direct CRUD と API CRUD の混在が、この段階の分析として説明できるか。
- `StoreAdminApi` と `OrganizationLookupApi` を同じ System に入れるべきではないか。
- 次ステップで `relate(Store, Organization, "N:1")` を置いたとき、coordination が必要になるか。

## Summary

<!-- derived-from #2-モデル構成 -->
<!-- derived-from #3-関係設計 -->
<!-- derived-from #6-レビュー観点 -->

Step 3 の設計は、UI/API 境界を追加しつつ、まだ単純更新は直接 CRUD として保持する。
