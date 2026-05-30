# 店舗補充管理 要求分析 Step 5

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- derived-from ../step-4-entity-structure/requirements-analysis.md -->

## 1. 業務背景

店舗と組織の構造が固まったため、店舗補充の状態変化を整理する。補充予定日の変更は単なる日付更新ではなく、店舗の補充状態を「通常」から「予定済み」へ進める操作として扱う。予定済みの補充は、店舗都合によりブロックされる場合がある。

## 2. ライフサイクル要求

| 状態 | 説明 | 業務上の意味 |
|---|---|---|
| `normal` | 通常 | 補充予定日が未設定または通常運用中 |
| `scheduled` | 補充予定済み | 次回補充予定日が設定されている |
| `blocked` | 補充停止 | 店舗都合などにより予定補充を止めている |

## 3. イベント要求

| Event | 発生元 usecase | 状態変化 | 付随効果 |
|---|---|---|---|
| `RestockScheduled` | `ChangeNextRestockDate` | `normal -> scheduled` | `next_restock_date` を present にする |
| `RestockBlocked` | `BlockScheduledRestock` | `scheduled -> blocked` | `next_restock_date` を null にする |

## 4. 追加 usecase

| Use case | 目的 | 備考 |
|---|---|---|
| `BlockScheduledRestock` | 予定済み補充を停止する | 店舗休業や補充不可の判断を想定 |

## 5. 要求

| ID | 要求 | 優先度 |
|---|---|---|
| R-501 | 補充状態を `normal`, `scheduled`, `blocked` として到達検証できること | Must |
| R-502 | 補充予定日の設定と状態変化を同じイベントで説明できること | Must |
| R-503 | 補充停止時に予定日が残らないことを表現できること | Should |

## 6. レビュー観点

- `blocked` は終端状態として扱ってよいか。
- `scheduled -> normal` の取り消し操作が必要か。
- 補充予定日を `DateTime @null` として状態軸に含めることが分析上有効か。

## Summary

<!-- derived-from #2-ライフサイクル要求 -->
<!-- derived-from #3-イベント要求 -->
<!-- derived-from #6-レビュー観点 -->

Step 5 では、店舗補充の状態軸、イベント、列効果を追加して到達可能性をレビューする。
