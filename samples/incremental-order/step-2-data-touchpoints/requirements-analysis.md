# 店舗補充管理 要求分析 Step 2

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-2-data-touchpoints -->
<!-- derived-from ../step-1-buc-skeleton/requirements-analysis.md -->

## 1. 業務背景

業務操作の粒度が合意できたため、各 usecase がどの業務データを触るかを整理する。ここではデータ項目や API 境界は確定せず、CRUD の方向だけをレビューする。

## 2. データ関心

| 業務オブジェクト | 説明 | 現時点の粒度 |
|---|---|---|
| `Store` | 補充予定を持つ店舗 | ID のみ |
| `Organization` | 店舗の担当組織候補 | ID のみ |

## 3. Usecase ごとのデータ操作

| Use case | Store | Organization | 判断 |
|---|---|---|---|
| `ChangeNextRestockDate` | Update | - | 店舗単体の属性更新として扱う |
| `ChangeStoreParentOrganization` | Update | Read | 新しい担当組織を確認して店舗を更新する |

## 4. 要求

| ID | 要求 | 優先度 |
|---|---|---|
| R-201 | 補充予定日変更が `Store` を更新することを示せること | Must |
| R-202 | 担当組織変更が `Organization` を参照し、`Store` を更新することを示せること | Must |
| R-203 | 詳細項目が未確定でも CRUD matrix で業務データの関与を確認できること | Should |

## 5. 未決事項

- `Store` と `Organization` の関係は FK で表せるか。
- 担当組織変更は同一トランザクションで扱うべきか。
- 補充予定日の変更履歴が必要か。

## 6. レビュー観点

- 初期段階として直接 `UseCase -> Entity` CRUD で十分か。
- `Organization` を更新せず参照だけにしている判断が業務に合っているか。
- 追加すべき業務オブジェクト、たとえば変更申請や履歴がないか。

## Summary

<!-- derived-from #2-データ関心 -->
<!-- derived-from #3-usecase-ごとのデータ操作 -->
<!-- derived-from #6-レビュー観点 -->

Step 2 では、粗い entity と直接 CRUD によりデータ接点だけを確定する。
