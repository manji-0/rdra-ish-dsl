# 店舗補充管理 設計 Step 5

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- constrained-by ../../../docs/state-derivation.md#operations -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

`Store` にライフサイクル軸を追加し、usecase と event を通じて到達可能な状態パターンを検証する。Step 4 までの構造設計に、業務状態の変化を重ねる。

## 2. 状態軸設計

| Entity | Column | 型 | 初期値 | 用途 |
|---|---|---|---|---|
| `Store` | `restock_status` | `Enum(normal, scheduled, blocked)` | `normal` | 補充状態 |
| `Store` | `next_restock_date` | `DateTime @null` | `null` | 次回補充予定日 |

## 3. イベント・遷移設計

| Event | Transition | Effect |
|---|---|---|
| `RestockScheduled` | `Normal -> Scheduled` | `sets(event::RestockScheduled, Store, "next_restock_date", "timestamptz")` |
| `RestockBlocked` | `Scheduled -> Blocked` | `sets(event::RestockBlocked, Store, "next_restock_date", "null")` |

## 4. Usecase 変更

| Use case | 追加関係 | 理由 |
|---|---|---|
| `ChangeNextRestockDate` | `raises(..., event::RestockScheduled)` | 予定日の変更を補充予定化イベントとして扱う |
| `BlockScheduledRestock` | `updates(..., Store)` / `raises(..., event::RestockBlocked)` | 店舗の補充状態を停止へ進める |

## 5. 設計判断

- `RestockScheduled` は `ChangeNextRestockDate` から発生させる。日付更新と状態遷移を分けるより、業務イベントとして一体でレビューしやすいため。
- `BlockScheduledRestock` は API 化しない。現時点では店舗単体の状態更新であり、境界越えの整合性がないため。
- `DateTime @null` を使い、予定日の有無を状態到達表で確認する。

## 6. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-5-lifecycle/src
rdra-ish states samples/incremental-order/step-5-lifecycle/src --entity Store
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind event-flow --format mermaid
```

期待する到達パターン:

| restock_status | next_restock_date | 意味 |
|---|---|---|
| `normal` | `null` | 初期状態 |
| `scheduled` | `present:timestamptz` | 補充予定あり |
| `blocked` | `null` | 補充停止、予定日なし |

## 7. レビュー観点

- 到達しない組み合わせが業務上も不要か。
- `blocked` に入った後の再開操作を後続要求として扱うか。
- Step 6 で business rule として固定すべき組み合わせが見えているか。

## Summary

<!-- derived-from #2-状態軸設計 -->
<!-- derived-from #3-イベント遷移設計 -->
<!-- derived-from #6-生成検証 -->

Step 5 の設計は、状態到達表を使って補充状態と予定日の関係を検証する。
