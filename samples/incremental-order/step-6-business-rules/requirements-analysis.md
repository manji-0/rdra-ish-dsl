# 店舗補充管理 要求分析 Step 6

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-6-business-rules -->
<!-- derived-from ../step-5-lifecycle/requirements-analysis.md -->

## 1. 業務背景

状態到達パターンが確認できたため、到達してはいけない組み合わせと、必ず同時に成立すべき組み合わせを業務ルールとして固定する。ここでは、補充状態と次回補充予定日の整合性を対象にする。

## 2. 業務ルール

| Rule | 内容 | 根拠 |
|---|---|---|
| BR-001 | 補充予定済みの店舗には次回補充予定日が必要 | 担当者が予定日なしの scheduled 店舗を運用できないため |
| BR-002 | 補充停止中の店舗に次回補充予定日を残してはいけない | 停止中に予定日が残ると発注・確認作業が誤誘導されるため |

## 3. 期待する状態

| restock_status | next_restock_date | 判定 |
|---|---|---|
| `normal` | `null` | OK |
| `scheduled` | `present` | OK |
| `blocked` | `null` | OK |
| `scheduled` | `null` | NG |
| `blocked` | `present` | NG |

## 4. 要求

| ID | 要求 | 優先度 |
|---|---|---|
| R-601 | scheduled 状態で予定日がないパターンを検出できること | Must |
| R-602 | blocked 状態で予定日が残るパターンを禁止できること | Must |
| R-603 | 状態到達表をレビューしてルール違反の有無を判断できること | Should |

## 5. 未決事項

- `normal` かつ `present` を禁止すべきか。現モデルでは到達しないが、将来の操作追加時には確認が必要。
- `blocked` から `scheduled` へ戻す再開操作を追加するか。
- 予定日変更の承認ルールを状態制約として扱うか、別 entity として扱うか。

## 6. レビュー観点

- `forbidden` と `invariant` の両方を使う理由が説明できるか。
- Step 5 の `sets` が業務ルールを満たす到達パターンを作っているか。
- ルール違反が出た場合、制約が強すぎるのか、イベント効果が不足しているのかを切り分けられるか。

## Summary

<!-- derived-from #2-業務ルール -->
<!-- derived-from #3-期待する状態 -->
<!-- derived-from #6-レビュー観点 -->

Step 6 では、補充状態と予定日の整合性を business rule として固定し、状態到達表で検証する。
