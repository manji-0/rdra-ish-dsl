# 店舗補充管理 要求分析 Step 5: Lifecycle

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- derived-from ../step-4-entity-structure/requirements-analysis.md -->

この文書は Step 5 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

Store の構造が固まったため、補充状態と次回補充予定日の lifecycle を検証する。補充予定日の設定は scheduled への遷移、補充停止は blocked への遷移として扱う。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `5` |
| 焦点 | 状態、イベント、sets を追加する |
| モデルルート | `samples/incremental-order/step-5-lifecycle/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| State axis | `restock_status` | normal, scheduled, blocked |
| Nullable axis | `next_restock_date` | null / present |
| Event | `RestockScheduled` | normal -> scheduled と予定日 present |
| Event | `RestockBlocked` | scheduled -> blocked と予定日 null |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-501 | 補充状態を normal, scheduled, blocked として到達検証できること | Must |
| R-502 | 補充予定日の設定と状態変化を同じイベントで説明できること | Must |
| R-503 | 補充停止時に予定日が残らないことを表現できること | Should |

## 5. レビュー観点

- normal -> scheduled -> blocked 以外の遷移が必要か。
- blocked から normal へ戻す UC を今入れるべきか。
- next_restock_date の present/null が業務状態を十分に説明しているか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `forbidden と invariant で状態制約を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 5 は、状態、イベント、sets を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
