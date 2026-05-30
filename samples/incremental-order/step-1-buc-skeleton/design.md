# 店舗補充管理 設計 Step 1

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

Step 0 の BUC に対して actor と usecase を追加し、業務操作の骨格をレビュー可能にする。まだ entity や screen を追加しないことで、業務操作の妥当性に集中する。

## 2. モデル構成

| 要素 | ID | 配置 |
|---|---|---|
| Actor | `OpsStaff` | `src/shared/actors.rdra` |
| BUC | `BucStoreRestock` | `src/buc/buc_store_restock.rdra` |
| UseCase | `ChangeNextRestockDate` | `src/buc/buc_store_restock.rdra` |
| UseCase | `ChangeStoreParentOrganization` | `src/buc/buc_store_restock.rdra` |

## 3. 関係設計

| 関係 | 意味 |
|---|---|
| `performs(OpsStaff, BucStoreRestock)` | 店舗運営担当者が BUC を実行する |
| `contains(BucStoreRestock, ChangeNextRestockDate)` | 補充予定日変更を BUC に含める |
| `contains(BucStoreRestock, ChangeStoreParentOrganization)` | 担当組織変更を BUC に含める |

## 4. 設計判断

- actor は個別ロールに分けず、まず `OpsStaff` に集約する。
- usecase は 2 つに分ける。補充予定日の変更は店舗単体の更新、担当組織変更は参照整合性を伴う可能性があり、後続の設計判断が異なるため。
- `screen` は未定義にする。画面名を先に決めると、業務操作ではなく UI 案に引っ張られるため。

## 5. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-1-buc-skeleton/src
rdra-ish diagram samples/incremental-order/step-1-buc-skeleton/src --kind rdra --format mermaid --buc BucStoreRestock
```

期待結果:

- BUC に 2 つの usecase が含まれる。
- actor から BUC への関係が確認できる。
- CRUD matrix はまだ作らない。

## 6. レビュー観点

- `ChangeStoreParentOrganization` を補充管理 BUC に含める理由が業務上説明できるか。
- 後続で分離すべき usecase がないか。
- actor が増える可能性を設計上妨げていないか。

## Summary

<!-- derived-from #2-モデル構成 -->
<!-- derived-from #3-関係設計 -->
<!-- derived-from #6-レビュー観点 -->

Step 1 の設計は、業務操作の輪郭だけを DSL に固定する。
