# 店舗補充管理 要求分析 Step 1: BUC Skeleton

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->
<!-- derived-from ../step-0-scope/requirements-analysis.md -->

この文書は Step 1 時点の要求分析サンプルです。抽象度を保ったまま、次に具体化する対象だけをレビューできる粒度にしています。

## 1. 業務背景

業務境界が合意できたため、店舗運営担当者が実行する業務操作を洗い出す。まだデータや画面は確定せず、業務の言葉で usecase の粒度をレビューする。

## 2. この step の焦点

| 観点 | 内容 |
|---|---|
| Step | `1` |
| 焦点 | actor と user-visible usecase を追加する |
| モデルルート | `samples/incremental-order/step-1-buc-skeleton/src` |

## 3. 要求スコープ

| 分類 | 対象 | 意味 |
|---|---|---|
| Actor | `OpsStaff` | 店舗補充予定と担当組織を維持する |
| UseCase | `ChangeNextRestockDate` | 店舗の次回補充予定日を変更する |
| UseCase | `ChangeStoreParentOrganization` | 店舗の担当組織を変更する |

## 4. 要求一覧

| ID | 要求 | 優先度 |
|---|---|---|
| R-101 | 店舗運営担当者が BUC の実行主体として表現されること | Must |
| R-102 | 補充予定日の変更と担当組織変更を別 usecase としてレビューできること | Must |
| R-103 | データや画面が未確定でも、業務操作の抜け漏れを確認できること | Should |

## 5. レビュー観点

- 2 つの usecase が業務担当者にとって別作業として認識されるか。
- 補充停止や店舗閉鎖など、別 usecase 候補をこの段階で入れるべきか。
- actor を店舗運営担当者だけにしてよいか。

## 6. 次 step への確認

次 step では、ここで合意した語彙を保持したまま `粗い entity と direct CRUD を追加する`。

## Summary

<!-- derived-from #3-要求スコープ -->
<!-- derived-from #4-要求一覧 -->

Step 1 は、actor と user-visible usecase を追加する段階として、後続の具体化で壊してはいけない要求境界を固定する。
