# 店舗補充管理 要求分析 Step 4: Entity Structure

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- derived-from ../step-3-interaction-boundary/requirements-analysis.md -->

この文書は Step 4 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

画面/API 境界が見えたため、Store と Organization の業務識別子と関係を設計する。店舗は 1 つの担当組織に属するため、ER と system 境界越えの調整責務を明示する。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `4` |
| 焦点 | columns、ER、境界越え coordination を追加する |
| モデルルート | `samples/incremental-order/step-4-entity-structure/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Entity | `Store` | id, code, name, organization_id |
| Entity | `Organization` | id, code, name |
| Relation | `Store -> Organization` | 店舗は 1 つの担当組織に属する |
| Coordination | `ChangeStoreParentOrganization` | 境界越え関係の整合性を調整する |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-401 | 店舗コードと組織コードを業務識別子として保持できること | Must |
| R-402 | 店舗が担当組織に属する関係を ER としてレビューできること | Must |
| R-403 | 境界をまたぐ関係に対して、調整する usecase を明示できること | Must |

## 5. レビュー観点

- Store と Organization の関係が N:1 でよいか。
- coordinates の責務を ChangeStoreParentOrganization に置くことが自然か。
- 店舗コードと組織コードだけでレビューに十分か。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `状態、イベント、sets を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 4 は、columns、ER、境界越え coordination を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
