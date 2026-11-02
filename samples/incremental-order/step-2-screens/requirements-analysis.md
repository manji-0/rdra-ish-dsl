# 店舗補充管理 要求分析 Step 2: Screens

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-2-screens -->
<!-- derived-from ../step-1-buc-skeleton/requirements-analysis.md -->

この文書は Step 2 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

業務操作が見えたため、担当者が各 usecase をどの画面で行うかを確認する。画面はアクターが目にして操作するシステム境界であり、どのデータに触れるかより先に、この仕事で何を見て何をするかを固める。データや API はまだ確定しない。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `2` |
| 焦点 | usecase ごとの画面を追加する |
| モデルルート | `samples/incremental-order/step-2-screens/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Screen | `StoreMaintenanceScreen` | 店舗の補充予定と担当組織を確認しながら更新する |
| Displays | `ChangeNextRestockDate -> StoreMaintenanceScreen` | 補充予定日の変更を店舗メンテナンス画面で行う |
| Displays | `ChangeStoreParentOrganization -> StoreMaintenanceScreen` | 担当組織の変更を同じ画面で行う |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-201 | 担当者が両方の usecase をどの画面で行うかを示せること | Must |
| R-202 | 補充予定日変更と担当組織変更を 1 つの店舗メンテナンス画面で扱えること | Must |
| R-203 | データ項目が未確定でも、担当者の操作の流れをレビューできること | Should |

## 5. レビュー観点

- 補充予定日変更と担当組織変更を同じ画面で行ってよいか。
- 店舗一覧など、店舗メンテナンス画面へ遷移する前の画面が必要か。
- イベントやタイマーで始まり、画面を持たない usecase がないか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `粗い entity と direct CRUD を追加し、画面が示す entity を結ぶ`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 2 は、usecase ごとの画面を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
