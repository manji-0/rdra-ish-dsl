# 店舗補充管理 要求分析 Step 3: Interaction Boundary

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- derived-from ../step-2-data-touchpoints/requirements-analysis.md -->

この文書は Step 3 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

データ接点が確認できたため、担当者がどの画面を通じて操作し、どの処理を API 境界として扱うべきかを整理する。補充予定日変更は単純更新だが、担当組織変更は組織の存在確認と店舗更新が同時に関わる。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `3` |
| 焦点 | 画面、API、System 境界を追加する |
| モデルルート | `samples/incremental-order/step-3-interaction-boundary/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Screen | `StoreMaintenanceScreen` | 店舗情報、補充予定、担当組織候補を確認しながら更新する |
| API | `StoreAdminApi` | 店舗情報の更新境界 |
| API | `OrganizationLookupApi` | 組織マスタの参照境界 |
| System | `StoreAdminSystem / OrganizationSystem` | 店舗情報と組織マスタの仮の所有境界 |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-301 | 担当者が店舗メンテナンス画面から対象店舗と組織候補を確認できること | Must |
| R-302 | 担当組織変更では店舗更新と組織参照を別 API 境界として見える化できること | Must |
| R-303 | 補充予定日変更は過剰な API 化を避け、直接更新として残せること | Should |

## 5. レビュー観点

- direct CRUD と API CRUD の混在が、この段階の分析として説明できるか。
- StoreAdminApi と OrganizationLookupApi を同じ System に入れるべきではないか。
- 次 step で relate(Store, Organization, "N:1") を置いたとき、coordination が必要になるか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `columns、ER、境界越え coordination を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 3 は、画面、API、System 境界を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
