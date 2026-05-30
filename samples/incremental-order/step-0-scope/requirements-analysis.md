# 店舗補充管理 要求分析 Step 0: Scope Sketch

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-0-scope-sketch -->
<!-- derived-from ../README.md -->

この文書は Step 0 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

店舗運営部では、店舗ごとの補充予定と担当組織の見直しを段階的にモデル化したい。初期段階では actor、usecase、entity を急いで置かず、業務領域と BUC の境界だけをレビュー対象にする。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `0` |
| 焦点 | 業務領域と BUC 名だけを固定する |
| モデルルート | `samples/incremental-order/step-0-scope/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| 業務領域 | `StoreOperations` | 店舗運営業務 |
| BUC | `BucStoreRestock` | 店舗補充情報を維持する業務単位 |
| 対象外 | `-` | 発注実行、在庫引当、配送計画、外部倉庫連携 |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-001 | 店舗補充管理を店舗運営業務の一部として扱えること | Must |
| R-002 | 補充予定変更と担当組織変更を今後の分析対象に含められること | Must |
| R-003 | 画面やデータ設計に入る前に、業務名と境界をレビューできること | Should |

## 5. レビュー観点

- StoreOperations が他の業務領域と衝突しない名前か。
- BucStoreRestock の粒度が後続の usecase を束ねる単位として自然か。
- 現段階で追加すべき共有語彙が本当に存在しないか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `actor と user-visible usecase を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 0 は、業務領域と BUC 名だけを固定する段階として、後続の具体化で壊してはいけない要求境界を固定する。
