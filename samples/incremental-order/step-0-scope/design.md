# 店舗補充管理 設計 Step 0

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-0-scope-sketch -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

要求分析 Step 0 で合意した業務領域を RDRA DSL に落とし込み、以降の分析で参照できる最小モデルを作る。レビュー対象は BUC 名と上位業務の関係だけであり、詳細化を急がない。

## 2. モデル構成

| ファイル | 役割 |
|---|---|
| `src/shared/biz.rdra` | 業務領域 `StoreOperations` を定義する |
| `src/buc/buc_store_restock.rdra` | BUC `BucStoreRestock` を定義し、業務領域へ関連づける |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| `business` と `buc` のみ定義する | actor/usecase/entity を先に置くと、業務境界が固まる前に詳細化してしまうため |
| BUC ファイルを `buc_store_restock.rdra` とする | 後続ステップでも同じ BUC を継続的に具体化するため |
| `shared/biz.rdra` に業務領域を置く | 複数 BUC から参照される可能性がある安定語彙だから |

## 4. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-0-scope/src
rdra-ish list samples/incremental-order/step-0-scope/src --kind buc --format table
rdra-ish diagram samples/incremental-order/step-0-scope/src --kind rdra --format mermaid --buc BucStoreRestock
```

期待結果:

- `check` に error がない。
- `BucStoreRestock` が `StoreOperations` に属している。
- actor、usecase、entity がまだ出てこない。

## 5. レビュー観点

- `StoreOperations` が他の業務領域と衝突しない名前か。
- `BucStoreRestock` の粒度が後続の usecase を束ねる単位として自然か。
- 現段階で追加すべき共有語彙が本当に存在しないか。

## Summary

<!-- derived-from #2-モデル構成 -->
<!-- derived-from #3-設計判断 -->
<!-- derived-from #4-生成検証 -->

この設計は、最小の RDRA モデルで業務境界だけを固定する。
