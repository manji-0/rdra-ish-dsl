//! TX境界推論: FK連結成分からユースケースの書き込み順序とトランザクション境界を導出する。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::{EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

// ── 公開型 ────────────────────────────────────────────────────────────────────

/// 書き込み操作の種別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteKind {
    Creates,
    Updates,
    Deletes,
}

impl WriteKind {
    /// PlantUML/Mermaid メッセージラベル用文字列。
    pub fn label(&self) -> &'static str {
        match self {
            WriteKind::Creates => "create",
            WriteKind::Updates => "update",
            WriteKind::Deletes => "delete",
        }
    }
}

/// ユースケースによるエンティティへの1書き込み操作。
#[derive(Debug, Clone)]
pub struct UcWrite {
    pub entity: EntityKey,
    pub kind: WriteKind,
}

/// FK連結成分ごとのTXグループ。
#[derive(Debug, Clone)]
pub struct TxGroup {
    /// FK親→子順にソートされた書き込み列。
    pub ordered_writes: Vec<UcWrite>,
    /// true = FK由来の推論、false = 明示 @atomic（フェーズ2）。
    pub inferred: bool,
}

/// 1ユースケースのTX境界分析結果。
#[derive(Debug, Clone)]
pub struct UsecaseTx {
    pub usecase: UseCaseKey,
    /// FK連結の書き込みグループ（2エンティティ以上）。sequence図で group/end を付ける。
    pub fk_groups: Vec<TxGroup>,
    /// FK連結のないの孤立書き込み（1エンティティ）。
    pub isolated_writes: Vec<UcWrite>,
    /// isolated_writes のうち fk_groups が存在する文脈で孤立しているエンティティ。
    /// → 診断ヒントと sequence 図の note right の対象。
    pub singletons_note: Vec<EntityKey>,
}

impl UsecaseTx {
    /// このユースケースに書き込み操作が1つ以上あるか。
    pub fn has_writes(&self) -> bool {
        !self.fk_groups.is_empty() || !self.isolated_writes.is_empty()
    }
}

// ── 推論エントリポイント ──────────────────────────────────────────────────────

/// モデル内の全ユースケースについてTX境界を推論する。
///
/// アルゴリズム（ユースケースごと）:
/// 1. creates/updates/deletes 述語から書き込みエンティティ集合 W を収集。
/// 2. W に誘導されたFKサブグラフ（N:1 / 1:N / 1:1 エッジ、両端が W に含まれるもの）を構築。
/// 3. 無向グラフ上でBFSにより連結成分を計算。
/// 4. 各成分をカーン法でトポロジカルソート（FK親 → 子の順）。
/// 5. サイズ1の成分かつFK多成分グループが存在する場合 → singletons_note に記録。
pub fn infer_usecase_transactions(model: &SemanticModel) -> Vec<UsecaseTx> {
    let mut result = Vec::new();

    for (uc_key, _uc) in model.use_cases.iter() {
        // Step 1: 書き込み操作を収集
        let mut writes: Vec<UcWrite> = Vec::new();
        for rel in &model.relations {
            if rel.from != NodeRef::UseCase(uc_key) {
                continue;
            }
            let wk = match rel.kind {
                RelKind::Creates => WriteKind::Creates,
                RelKind::Updates => WriteKind::Updates,
                RelKind::Deletes => WriteKind::Deletes,
                _ => continue,
            };
            if let NodeRef::Entity(ek) = rel.to {
                writes.push(UcWrite {
                    entity: ek,
                    kind: wk,
                });
            }
        }

        if writes.is_empty() {
            continue;
        }

        let write_keys: HashSet<EntityKey> = writes.iter().map(|w| w.entity).collect();

        // Step 2: 書き込みエンティティ間のFKエッジを構築
        //   parents_of[child] = このユースケースの書き込み集合内でのFK親リスト
        //   adj[k]           = 無向隣接リスト（連結成分検出用）
        let mut parents_of: HashMap<EntityKey, Vec<EntityKey>> = HashMap::new();
        let mut adj: HashMap<EntityKey, Vec<EntityKey>> = HashMap::new();
        for &ek in &write_keys {
            parents_of.entry(ek).or_default();
            adj.entry(ek).or_default();
        }

        for rel in &model.relations {
            // (parent, child) のペアに正規化:
            //   N:1 → from=多(child), to=1(parent)   → parent=to, child=from
            //   1:N → from=1(parent), to=多(child)   → parent=from, child=to
            //   1:1 → from側がFKを持つ慣習            → parent=to, child=from
            let maybe: Option<(EntityKey, EntityKey)> = match &rel.kind {
                RelKind::RelateManyToOne => {
                    if let (NodeRef::Entity(child_k), NodeRef::Entity(parent_k)) =
                        (&rel.from, &rel.to)
                    {
                        Some((*parent_k, *child_k))
                    } else {
                        None
                    }
                }
                RelKind::RelateOneToMany => {
                    if let (NodeRef::Entity(parent_k), NodeRef::Entity(child_k)) =
                        (&rel.from, &rel.to)
                    {
                        Some((*parent_k, *child_k))
                    } else {
                        None
                    }
                }
                RelKind::RelateOneToOne => {
                    if let (NodeRef::Entity(child_k), NodeRef::Entity(parent_k)) =
                        (&rel.from, &rel.to)
                    {
                        Some((*parent_k, *child_k))
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some((parent, child)) = maybe {
                if write_keys.contains(&parent) && write_keys.contains(&child) {
                    adj.entry(parent).or_default().push(child);
                    adj.entry(child).or_default().push(parent);
                    parents_of.entry(child).or_default().push(parent);
                }
            }
        }

        // Step 3: BFSで連結成分を検出
        let mut component_of: HashMap<EntityKey, usize> = HashMap::new();
        let mut next_cid = 0usize;

        for &start in &write_keys {
            if component_of.contains_key(&start) {
                continue;
            }
            let cid = next_cid;
            next_cid += 1;
            let mut queue = VecDeque::new();
            queue.push_back(start);
            component_of.insert(start, cid);
            while let Some(cur) = queue.pop_front() {
                for &nb in adj.get(&cur).into_iter().flatten() {
                    if !component_of.contains_key(&nb) {
                        component_of.insert(nb, cid);
                        queue.push_back(nb);
                    }
                }
            }
        }

        // 連結成分ごとに書き込みをグループ化
        let mut comp_writes: HashMap<usize, Vec<UcWrite>> = HashMap::new();
        for w in &writes {
            let cid = component_of[&w.entity];
            comp_writes.entry(cid).or_default().push(w.clone());
        }

        // Step 4: 多エンティティ成分 → TxGroup（トポロジカルソート）、
        //         1エンティティ成分 → isolated_writes
        let mut fk_groups: Vec<TxGroup> = Vec::new();
        let mut isolated_writes: Vec<UcWrite> = Vec::new();

        let mut cids: Vec<usize> = comp_writes.keys().cloned().collect();
        cids.sort(); // 決定的な順序

        for cid in cids {
            let comp = comp_writes.remove(&cid).unwrap();
            if comp.len() >= 2 {
                let ordered = topological_sort_writes(comp, &parents_of, model);
                fk_groups.push(TxGroup {
                    ordered_writes: ordered,
                    inferred: true,
                });
            } else {
                isolated_writes.extend(comp);
            }
        }

        // entity id でソート（決定的出力のため）
        isolated_writes.sort_by_key(|w| {
            model
                .entities
                .get(w.entity)
                .map(|e| e.id.clone())
                .unwrap_or_default()
        });

        // Step 5: FK多成分グループが存在するときに孤立する書き込み → singletons_note
        let singletons_note: Vec<EntityKey> = if !fk_groups.is_empty() {
            isolated_writes.iter().map(|w| w.entity).collect()
        } else {
            Vec::new()
        };

        result.push(UsecaseTx {
            usecase: uc_key,
            fk_groups,
            isolated_writes,
            singletons_note,
        });
    }

    result
}

/// FK孤立書き込みに対する診断ヒントを生成する。
///
/// `infer_usecase_transactions` の結果を受け取り、`singletons_note` が空でない
/// ユースケースについて `SeparateTxInferred` warning を返す。
pub fn tx_diagnostics(model: &SemanticModel, txs: &[UsecaseTx]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for utx in txs {
        if utx.singletons_note.is_empty() {
            continue;
        }
        let uc_id = model
            .use_cases
            .get(utx.usecase)
            .map(|u| u.id.as_str())
            .unwrap_or("?");
        for &ek in &utx.singletons_note {
            let entity_id = model.entities.get(ek).map(|e| e.id.as_str()).unwrap_or("?");
            diags.push(Diagnostic::warning(RdraError::SeparateTxInferred {
                usecase: uc_id.to_string(),
                entity: entity_id.to_string(),
            }));
        }
    }
    diags
}

// ── 内部ヘルパー ──────────────────────────────────────────────────────────────

/// FK親→子の順にカーン法でトポロジカルソートする。
///
/// `parents_of` はモデル全体の親マップ（書き込み集合内のエッジのみ含む）。
/// `writes` はソート対象の1連結成分内の書き込み。
fn topological_sort_writes(
    writes: Vec<UcWrite>,
    parents_of: &HashMap<EntityKey, Vec<EntityKey>>,
    model: &SemanticModel,
) -> Vec<UcWrite> {
    let keys: HashSet<EntityKey> = writes.iter().map(|w| w.entity).collect();

    // 有向グラフ（parent→child）と入次数を構築
    let mut children_of: HashMap<EntityKey, Vec<EntityKey>> = HashMap::new();
    let mut in_degree: HashMap<EntityKey, usize> = HashMap::new();
    for &k in &keys {
        children_of.entry(k).or_default();
        in_degree.entry(k).or_insert(0);
    }
    for (&child, parents) in parents_of {
        if !keys.contains(&child) {
            continue;
        }
        for &parent in parents {
            if keys.contains(&parent) {
                children_of.entry(parent).or_default().push(child);
                *in_degree.entry(child).or_default() += 1;
            }
        }
    }

    // カーン法: 入次数0から開始、entity id でソートして決定的に
    let mut queue: Vec<EntityKey> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&k, _)| k)
        .collect();
    queue.sort_by_key(|&k| {
        model
            .entities
            .get(k)
            .map(|e| e.id.clone())
            .unwrap_or_default()
    });

    let write_map: HashMap<EntityKey, UcWrite> =
        writes.into_iter().map(|w| (w.entity, w)).collect();
    let mut result: Vec<UcWrite> = Vec::new();

    while !queue.is_empty() {
        queue.sort_by_key(|&k| {
            model
                .entities
                .get(k)
                .map(|e| e.id.clone())
                .unwrap_or_default()
        });
        let cur = queue.remove(0);
        if let Some(w) = write_map.get(&cur) {
            result.push(w.clone());
        }
        if let Some(children) = children_of.get(&cur) {
            let mut cs = children.clone();
            cs.sort_by_key(|&k| {
                model
                    .entities
                    .get(k)
                    .map(|e| e.id.clone())
                    .unwrap_or_default()
            });
            for child in cs {
                let d = in_degree.entry(child).or_default();
                if *d > 0 {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(child);
                    }
                }
            }
        }
    }

    // サイクルが存在する場合（FKグラフではありえないが防御的に）残りを追加
    for (k, w) in &write_map {
        if !result.iter().any(|r| r.entity == *k) {
            result.push(w.clone());
        }
    }

    result
}

// ── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errs) = parse(src);
        assert!(errs.is_empty(), "parse errors: {:?}", errs);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "model errors: {:?}", errors);
        model
    }

    /// PlaceOrder 相当: Order + OrderLine（FK連結）+ Cart（FK非連結）
    #[test]
    fn test_fk_group_and_singleton() {
        let src = r#"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Cart      "カート"   { id: Int @pk }
usecase PlaceOrder "注文を確定する"
relate(OrderLine, Order, "N:1")
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
updates(PlaceOrder, Cart)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert_eq!(txs.len(), 1);
        let utx = &txs[0];

        // FK連結グループ: {Order, OrderLine}
        assert_eq!(utx.fk_groups.len(), 1);
        let group = &utx.fk_groups[0];
        assert_eq!(group.ordered_writes.len(), 2);
        assert!(group.inferred);

        // FK親(Order)が先、子(OrderLine)が後
        let ids: Vec<&str> = group
            .ordered_writes
            .iter()
            .map(|w| model.entities.get(w.entity).unwrap().id.as_str())
            .collect();
        assert_eq!(ids, vec!["Order", "OrderLine"], "parent-before-child order");

        // 孤立書き込み: Cart
        assert_eq!(utx.isolated_writes.len(), 1);
        let cart_id = model
            .entities
            .get(utx.isolated_writes[0].entity)
            .unwrap()
            .id
            .as_str();
        assert_eq!(cart_id, "Cart");

        // singletons_note には Cart が入る
        assert_eq!(utx.singletons_note.len(), 1);
        let note_id = model
            .entities
            .get(utx.singletons_note[0])
            .unwrap()
            .id
            .as_str();
        assert_eq!(note_id, "Cart");
    }

    /// Capture 相当: Payment → Order (1:1) → 全体が1グループ
    #[test]
    fn test_one_to_one_single_group() {
        let src = r#"
entity Order   "注文" { id: Int @pk }
entity Payment "決済" { id: Int @pk }
usecase Capture "決済を確定する"
relate(Payment, Order, "1:1")
updates(Capture, Payment)
updates(Capture, Order)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert_eq!(txs.len(), 1);
        let utx = &txs[0];

        // 全部1グループ
        assert_eq!(utx.fk_groups.len(), 1);
        assert_eq!(utx.isolated_writes.len(), 0);
        assert_eq!(utx.singletons_note.len(), 0);

        // Order(親) → Payment(子) の順
        let ids: Vec<&str> = utx.fk_groups[0]
            .ordered_writes
            .iter()
            .map(|w| model.entities.get(w.entity).unwrap().id.as_str())
            .collect();
        assert_eq!(ids, vec!["Order", "Payment"]);
    }

    /// 全書き込みがFKなし → singletons_note は空（全部 isolated_writes）
    #[test]
    fn test_all_isolated_no_note() {
        let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
usecase DoSomething "何かする"
creates(DoSomething, A)
updates(DoSomething, B)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert_eq!(txs.len(), 1);
        let utx = &txs[0];

        assert_eq!(utx.fk_groups.len(), 0);
        assert_eq!(utx.isolated_writes.len(), 2);
        assert_eq!(
            utx.singletons_note.len(),
            0,
            "no note when no FK groups exist"
        );
    }

    /// 診断: singletons_note があれば warning が出る
    #[test]
    fn test_tx_diagnostics_warns_singleton() {
        let src = r#"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Cart      "カート"   { id: Int @pk }
usecase PlaceOrder "注文を確定する"
relate(OrderLine, Order, "N:1")
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
updates(PlaceOrder, Cart)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        let diags = tx_diagnostics(&model, &txs);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].is_warning);
        let msg = diags[0].error.to_string();
        assert!(msg.contains("PlaceOrder"), "should mention the usecase");
        assert!(msg.contains("Cart"), "should mention the entity");
    }
}
