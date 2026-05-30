# 店舗補充管理 要求分析 Step 3

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- derived-from ../step-2-data-touchpoints/requirements-analysis.md -->

## 1. 業務背景

データ接点が確認できたため、担当者がどの画面を通じて操作し、どの処理を API 境界として扱うべきかを整理する。補充予定日変更は単純更新だが、担当組織変更は組織の存在確認と店舗更新が同時に関わる。

## 2. 画面要求

| Screen | 対象 usecase | 目的 |
|---|---|---|
| `StoreMaintenanceScreen` | 全 usecase | 店舗情報、補充予定、担当組織候補を確認しながら更新する |

## 3. API 境界要求

| API | 操作 | 理由 |
|---|---|---|
| `StoreAdminApi` | `Store` の更新 | 店舗側の永続化責務をまとめる |
| `OrganizationLookupApi` | `Organization` の参照 | 担当組織候補の存在確認を店舗更新から分ける |

## 4. System 境界仮説

| System | API | データ関心 |
|---|---|---|
| `StoreAdminSystem` | `StoreAdminApi` | 店舗情報 |
| `OrganizationSystem` | `OrganizationLookupApi` | 組織マスタ |

この時点では、実際のシステム分割ではなく、整合性境界をレビューするための仮説として扱う。

## 5. 要求

| ID | 要求 | 優先度 |
|---|---|---|
| R-301 | 担当者が店舗メンテナンス画面から対象店舗と組織候補を確認できること | Must |
| R-302 | 担当組織変更では店舗更新と組織参照を別 API 境界として見える化できること | Must |
| R-303 | 補充予定日変更は過剰な API 化を避け、直接更新として残せること | Should |

## 6. レビュー観点

- `OrganizationLookupApi` が read-only API として妥当か。
- `StoreAdminSystem` と `OrganizationSystem` の分け方が業務境界を表しているか。
- sequence 図で確認すべき書き込み経路と、API matrix で確認すべき読み取り経路を分けてレビューできるか。

## Summary

<!-- derived-from #2-画面要求 -->
<!-- derived-from #3-api-境界要求 -->
<!-- derived-from #4-system-境界仮説 -->

Step 3 では、画面と API/System 境界を追加し、単純更新と境界越え操作を分けて扱う。
