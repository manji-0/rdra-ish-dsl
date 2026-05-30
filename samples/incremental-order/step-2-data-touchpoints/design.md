# 店舗補充管理 設計 Step 2

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-2-data-touchpoints -->
<!-- constrained-by ../../../docs/language-reference.md#relationship-predicates -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

各 usecase が触る業務データを DSL に表し、CRUD matrix でレビューできるようにする。現時点では直接 `UseCase -> Entity` CRUD を使う。これは初期分析の合法な表現であり、API 境界が分かるまでは無理に API を作らない。

## 2. モデル構成

| 要素 | ID | 配置 | 備考 |
|---|---|---|---|
| Entity | `Store` | `src/shared/entities.rdra` | `id` のみ |
| Entity | `Organization` | `src/shared/entities.rdra` | `id` のみ |
| CRUD | `updates(ChangeNextRestockDate, Store)` | BUC ファイル | 店舗単体更新 |
| CRUD | `reads(ChangeStoreParentOrganization, Organization)` | BUC ファイル | 担当組織候補の参照 |
| CRUD | `updates(ChangeStoreParentOrganization, Store)` | BUC ファイル | 店舗所属の更新 |

## 3. 設計判断

| 判断 | 理由 |
|---|---|
| entity は `id` のみで開始する | 項目定義よりも、業務操作とデータ接点の妥当性を先に確認するため |
| direct CRUD を採用する | API 境界やトランザクション責務がまだ未確定のため |
| `Organization` は Read のみ | 担当組織そのものを変更する業務ではないため |

## 4. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-2-data-touchpoints/src
rdra-ish csv samples/incremental-order/step-2-data-touchpoints/src --kind matrix
rdra-ish diagram samples/incremental-order/step-2-data-touchpoints/src --kind er --format mermaid
```

期待する CRUD matrix:

| Use case | Organization | Store |
|---|---|---|
| `ChangeNextRestockDate` | - | U |
| `ChangeStoreParentOrganization` | R | U |

## 5. レビュー観点

- CRUD の方向が業務説明と一致しているか。
- `ChangeStoreParentOrganization` に API 境界が必要かどうか、次ステップで判断できるだけの材料があるか。
- ER 図が粗すぎることを許容できるか。

## Summary

<!-- derived-from #2-モデル構成 -->
<!-- derived-from #3-設計判断 -->
<!-- derived-from #4-生成検証 -->

Step 2 の設計は、まだ抽象的な entity に直接 CRUD を付けてデータ接点を固定する。
