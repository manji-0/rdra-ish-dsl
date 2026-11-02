# 店舗補充管理 要求分析 Step 3: Data Touchpoints

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-data-touchpoints -->
<!-- derived-from ../step-2-screens/requirements-analysis.md -->

この文書は Step 3 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

業務操作と画面が見えたため、各 usecase がどのデータに触れ、画面がどのデータを示すかを確認する。項目定義や API 境界はまだ確定せず、CRUD matrix でデータ関与だけをレビューする。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `3` |
| 焦点 | 粗い entity と direct CRUD を追加し、画面が示す entity を結ぶ |
| モデルルート | `samples/incremental-order/step-3-data-touchpoints/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Entity | `Store` | 補充予定を持つ店舗 |
| Entity | `Organization` | 店舗の担当組織候補 |
| CRUD | `ChangeNextRestockDate -> Store` | 店舗単体の更新 |
| CRUD | `ChangeStoreParentOrganization -> Organization/Store` | 組織参照と店舗更新 |
| Shows | `StoreMaintenanceScreen -> Store/Organization` | 画面が店舗と担当組織候補を示す |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-301 | 補充予定日変更が Store を更新することを示せること | Must |
| R-302 | 担当組織変更が Organization を参照し、Store を更新することを示せること | Must |
| R-303 | 詳細項目が未確定でも CRUD matrix で業務データの関与を確認できること | Should |
| R-304 | 店舗メンテナンス画面が対象店舗と組織候補を示すことを確認できること | Must |

## 5. レビュー観点

- Store と Organization 以外に、この段階で必要な業務データがあるか。
- 担当組織変更で Organization を更新しない判断が正しいか。
- direct CRUD のまま次 step に進んでも論点が失われないか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `API、System 境界と権限・媒体の制約を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 3 は、粗い entity と direct CRUD を追加し、画面が示す entity を結ぶ段階として、後続の具体化で壊してはいけない要求境界を固定する。
