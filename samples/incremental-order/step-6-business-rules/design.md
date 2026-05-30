# 店舗補充管理 設計 Step 6

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-6-business-rules -->
<!-- constrained-by ../../../docs/state-derivation.md#operations -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

Step 5 で確認した状態到達パターンに業務ルールを重ね、モデルが不整合な状態を許さないことを検証する。ここでは `Store` の補充状態と次回補充予定日の整合性だけを扱う。

## 2. ルール設計

```rdra
forbidden(Store, (restock_status, blocked), (next_restock_date, present))

invariant(Store)
  .when(restock_status, scheduled)
  .then(next_restock_date, present)
```

| DSL | 対応する業務ルール |
|---|---|
| `forbidden` | blocked の店舗に予定日が残る状態を禁止する |
| `invariant` | scheduled の店舗には予定日が必ず存在する |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| blocked + present は `forbidden` で表す | 特定の組み合わせを禁止するルールだから |
| scheduled -> present は `invariant` で表す | 条件が成立したときの必須値を表すルールだから |
| normal + present はまだ禁止しない | 現モデルでは到達せず、将来の予定日事前入力要求を妨げないため |

## 4. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-6-business-rules/src
rdra-ish states samples/incremental-order/step-6-business-rules/src --entity Store
rdra-ish diagram samples/incremental-order/step-6-business-rules/src --kind state --format mermaid --buc BucStoreRestock
```

期待結果:

- `states --entity Store` に rule violation が出ない。
- `scheduled / present:timestamptz` が到達する。
- `blocked / null` が到達し、terminal として確認できる。

## 5. レビュー観点

- ルールが実業務の運用例外を潰していないか。
- `BlockScheduledRestock` の `sets(..., "null")` が BR-002 を満たしているか。
- 今後の再開操作を追加した場合、`blocked -> scheduled` の transition と `next_restock_date` の復元が必要になることを合意できるか。

## 6. 承認条件

| 観点 | 承認条件 |
|---|---|
| 要求 | BR-001, BR-002 が DSL 上の制約として表現されている |
| 設計 | 状態軸、イベント、ルールの責務が分かれている |
| 検証 | `check` と `states` が warning/error なしで確認済み |

## Summary

<!-- derived-from #2-ルール設計 -->
<!-- derived-from #4-生成検証 -->
<!-- derived-from #6-承認条件 -->

Step 6 の設計は、状態到達表と business rule を使って補充状態の整合性を承認可能にする。
