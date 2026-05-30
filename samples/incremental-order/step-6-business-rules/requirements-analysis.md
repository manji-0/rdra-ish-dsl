# 店舗補充管理 要求分析 Step 6: Business Rules

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-6-business-rules -->
<!-- derived-from ../step-5-lifecycle/requirements-analysis.md -->

この文書は Step 6 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

状態到達表が出せるようになったため、業務上許されない状態組み合わせを DSL に落とす。scheduled には予定日が必要で、blocked には予定日が残ってはいけない。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `6` |
| 焦点 | forbidden と invariant で状態制約を追加する |
| モデルルート | `samples/incremental-order/step-6-business-rules/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Rule | `BR-001` | scheduled の店舗には next_restock_date が必要 |
| Rule | `BR-002` | blocked の店舗に next_restock_date を残してはいけない |
| DSL | `invariant` | scheduled -> present |
| DSL | `forbidden` | blocked + present を禁止 |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-601 | scheduled 状態で予定日がないパターンを検出できること | Must |
| R-602 | blocked 状態で予定日が残るパターンを禁止できること | Must |
| R-603 | 状態到達表をレビューしてルール違反の有無を判断できること | Should |

## 5. レビュー観点

- BR-001, BR-002 が DSL 上の制約として表現されているか。
- 状態到達表で違反パターンが出ていないことを確認できるか。
- normal + present を禁止しない判断が業務上許容できるか。

## 6. 次 step への確認

この step で、要求、設計、状態検証、業務ルールを一通りレビューできる。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 6 は、forbidden と invariant で状態制約を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
