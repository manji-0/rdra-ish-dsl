# 店舗補充管理 設計 Step 4

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- constrained-by ../../../docs/language-reference.md#api-and-the-api-layer -->
<!-- derived-from ./requirements-analysis.md -->

## 1. 設計目的

粗い entity をレビュー可能な ER 構造に具体化し、System 境界をまたぐ関係を診断できる状態にする。ここで初めて `relate` と `coordinates` を追加する。

## 2. エンティティ設計

| Entity | Column | 型 | 用途 |
|---|---|---|---|
| `Store` | `id` | `Int @pk` | 内部識別子 |
| `Store` | `code` | `String @unique` | 業務識別子 |
| `Store` | `name` | `String` | 表示名 |
| `Organization` | `id` | `Int @pk` | 内部識別子 |
| `Organization` | `code` | `String @unique` | 業務識別子 |
| `Organization` | `name` | `String` | 表示名 |

## 3. 関係設計

```rdra
relate(Store, Organization, "N:1")
coordinates(ChangeStoreParentOrganization, Store, Organization)
```

`relate` により店舗から組織への FK が生成される。`Store` と `Organization` は別 System に由来するため、`coordinates` で担当組織変更 usecase の整合性責務を明示する。

## 4. 設計判断

| 判断 | 理由 |
|---|---|
| `code` を `@unique` にする | 業務レビューでは ID よりコードで店舗・組織を識別するため |
| 変更履歴 entity は追加しない | 現時点の要求では監査・履歴保持が未確定のため |
| `coordinates` を追加する | System 境界をまたぐ ER 関係を無警告で説明するため |

## 5. 生成・検証

```sh
rdra-ish check samples/incremental-order/step-4-entity-structure/src
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind er --format mermaid
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind sequence --format mermaid --buc BucStoreRestock
```

期待結果:

- ER 図に `Store -> Organization` の関係が出る。
- system diagnostics で cross-system relation の warning が出ない。
- `coordinates` に必要な両側 API invocation が満たされている。

## 6. レビュー観点

- `OrganizationLookupApi` が read-only でも coordination の片側 API として十分か。
- 今後、担当組織変更に承認や履歴が必要になった場合、別 API/Entity を追加する余地があるか。
- `Store` の属性が Step 5 の lifecycle 追加を妨げないか。

## Summary

<!-- derived-from #2-エンティティ設計 -->
<!-- derived-from #3-関係設計 -->
<!-- derived-from #5-生成検証 -->

Step 4 の設計は、ER と System 境界を接続し、境界越えの整合性責務をレビュー可能にする。
