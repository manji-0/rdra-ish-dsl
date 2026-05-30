# 店舗補充管理 要求分析 Step 4

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- derived-from ../step-3-interaction-boundary/requirements-analysis.md -->

## 1. 業務背景

API/System 境界の仮説が置けたため、店舗と組織の関係をデータ構造として明示する。担当組織変更は店舗情報と組織マスタの関係を変更する業務であり、システム境界をまたぐ整合性確認が必要になる。

## 2. エンティティ要求

| Entity | 必須項目 | 説明 |
|---|---|---|
| `Store` | `id`, `code`, `name` | 補充対象の店舗 |
| `Organization` | `id`, `code`, `name` | 店舗を管理する組織 |

## 3. 関係要求

| 関係 | 意味 | 業務上の制約 |
|---|---|---|
| `Store -> Organization` | 店舗は 1 つの担当組織に属する | 変更時は新しい組織が有効であることを確認する |

## 4. System 境界要求

| 境界 | 対象 entity | 根拠 |
|---|---|---|
| `StoreAdminSystem` | `Store` | `StoreAdminApi` が更新する |
| `OrganizationSystem` | `Organization` | `OrganizationLookupApi` が参照する |

`Store` と `Organization` の関連は System 境界をまたぐため、担当組織変更 usecase が coordination 責務を持つ。

## 5. 要求

| ID | 要求 | 優先度 |
|---|---|---|
| R-401 | 店舗コードと組織コードを業務識別子として保持できること | Must |
| R-402 | 店舗が担当組織に属する関係を ER としてレビューできること | Must |
| R-403 | 境界をまたぐ関係に対して、調整する usecase を明示できること | Must |

## 6. レビュー観点

- `Store` と `Organization` の関係が `"N:1"` で正しいか。
- 担当組織変更の coordination は `ChangeStoreParentOrganization` で足りるか。
- 変更履歴 entity を Step 4 で追加すべきか、後続要求に回すべきか。

## Summary

<!-- derived-from #2-エンティティ要求 -->
<!-- derived-from #3-関係要求 -->
<!-- derived-from #4-system-境界要求 -->

Step 4 では、店舗と組織の構造、および境界越え関係の coordination 責務を明確にする。
