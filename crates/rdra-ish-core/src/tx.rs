//! TX境界推論: FK連結成分からユースケースの書き込み順序とトランザクション境界を導出する。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::push_model_decl_diagnostic;
use crate::model::{ApiKey, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey};

// ── 公開型 ────────────────────────────────────────────────────────────────────

/// 書き込み操作の種別。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteKind {
    Creates,
    Updates,
    Deletes,
}

impl WriteKind {
    /// Mermaid メッセージラベル用文字列。
    pub fn label(&self) -> &'static str {
        match self {
            WriteKind::Creates => "create",
            WriteKind::Updates => "update",
            WriteKind::Deletes => "delete",
        }
    }

    /// Which kind wins when a use case writes one entity several ways.
    fn rank(&self) -> u8 {
        match self {
            WriteKind::Creates => 0,
            WriteKind::Deletes => 1,
            WriteKind::Updates => 2,
        }
    }
}

/// ユースケースによるエンティティへの1書き込み操作。
#[derive(Debug, Clone)]
pub struct UcWrite {
    pub entity: EntityKey,
    pub kind: WriteKind,
    /// この書き込みを担う API ノード（`invokes(uc, api)` + `creates(api, entity)` 由来）。
    /// `None` の場合は usecase が直接 entity に書き込む（レガシー形式）。
    pub via_api: Option<ApiKey>,
}

/// Why a group's writes commit together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    /// Two or more direct writes connected through foreign keys: inferred as one transaction.
    Fk,
    /// Two or more writes of one invoked API: its explicit atomic boundary.
    Api,
    /// One write on its own.
    Isolated,
}

impl TxKind {
    /// The note a sequence diagram puts on the group's frame; an isolated write has no frame.
    pub fn frame_label(self) -> Option<&'static str> {
        match self {
            TxKind::Fk => Some("transaction (inferred from FK)"),
            TxKind::Api => Some("transaction (API atomic boundary)"),
            TxKind::Isolated => None,
        }
    }
}

/// One transaction of a use case.
#[derive(Debug, Clone)]
pub struct TxGroup {
    pub kind: TxKind,
    /// FK親→子順にソートされた書き込み列。
    pub ordered_writes: Vec<UcWrite>,
}

/// 1ユースケースのTX境界分析結果。
#[derive(Debug, Clone)]
pub struct UsecaseTx {
    pub usecase: UseCaseKey,
    groups: Vec<TxGroup>,
}

impl UsecaseTx {
    /// The use case's transactions in transaction order (vision rule 6): the API-bounded and
    /// FK-connected groups, then each isolated write. Never empty.
    pub fn groups(&self) -> impl Iterator<Item = &TxGroup> {
        self.groups.iter()
    }

    /// Every write, in transaction order.
    pub fn writes(&self) -> impl Iterator<Item = &UcWrite> {
        self.groups().flat_map(|g| g.ordered_writes.iter())
    }

    /// Entities written in their own transaction after an FK-inferred group: the
    /// `SeparateTxInferred` warning and the sequence diagram's note.
    pub fn separate_writes(&self) -> impl Iterator<Item = EntityKey> + '_ {
        let fk_inferred = self.groups().any(|g| g.kind == TxKind::Fk);
        self.groups()
            .filter(move |g| fk_inferred && g.kind == TxKind::Isolated)
            .flat_map(|g| g.ordered_writes.iter().map(|w| w.entity))
    }
}

// ── 推論エントリポイント ──────────────────────────────────────────────────────

/// Every write each use case makes, grouped into transactions (vision rule 6). The lifecycle
/// runs one action per group and sequence diagrams draw the same groups.
///
/// Writes are `creates`, `updates`, `writes` (an update) and `deletes` edges from the use case
/// or an API it invokes, plus an implied update for every entity the use case only `sets` or
/// transitions. One entity keeps its strongest kind (`creates`, then `deletes`, then
/// `updates`), API-origin first.
///
/// Grouping (ユースケースごと):
/// 1. invokes(uc, api) で呼ばれる API ごとの書き込み集合は FK 接続の有無に関係なく
///    1 つの原子境界として扱う。
/// 2. 直接 usecase に付いた書き込みはレガシー形式として収集し、API 経由の同一 entity
///    書き込みがあれば API 側を優先する。
/// 3. 直接書き込みは FK 連結成分から transaction 境界を推論する。
pub fn infer_usecase_transactions(model: &SemanticModel) -> Vec<UsecaseTx> {
    let mut result = Vec::new();

    for (uc_key, _uc) in model.use_cases.iter() {
        let mut grouped: Vec<TxGroup> = Vec::new();
        let mut isolated: Vec<TxGroup> = Vec::new();
        let mut api_written_entities: HashSet<EntityKey> = HashSet::new();

        let mut invoked_apis: Vec<ApiKey> = Vec::new();
        for rel in &model.relations {
            if rel.from == NodeRef::UseCase(uc_key) && rel.kind == RelKind::Invokes {
                if let NodeRef::Api(ak) = rel.to {
                    invoked_apis.push(ak);
                }
            }
        }
        invoked_apis.sort_by_key(|ak| {
            model
                .apis
                .get(*ak)
                .map(|a| a.id.clone())
                .unwrap_or_default()
        });
        invoked_apis.dedup();

        for ak in &invoked_apis {
            let mut api_writes: Vec<UcWrite> = Vec::new();
            for rel in &model.relations {
                if rel.from != NodeRef::Api(*ak) {
                    continue;
                }
                let Some(wk) = write_kind(&rel.kind) else {
                    continue;
                };
                if let NodeRef::Entity(ek) = rel.to {
                    api_written_entities.insert(ek);
                    api_writes.push(UcWrite {
                        entity: ek,
                        kind: wk,
                        via_api: Some(*ak),
                    });
                }
            }
            dedup_writes(&mut api_writes, model);
            if api_writes.len() >= 2 {
                grouped.push(TxGroup {
                    kind: TxKind::Api,
                    ordered_writes: topological_sort_writes(
                        api_writes,
                        &fk_parent_map(model),
                        model,
                    ),
                });
            } else {
                isolated.extend(api_writes.into_iter().map(isolated_group));
            }
        }

        let mut direct_writes: Vec<UcWrite> = Vec::new();
        for rel in &model.relations {
            if rel.from != NodeRef::UseCase(uc_key) {
                continue;
            }
            let Some(wk) = write_kind(&rel.kind) else {
                continue;
            };
            if let NodeRef::Entity(ek) = rel.to {
                if api_written_entities.contains(&ek) {
                    continue;
                }
                direct_writes.push(UcWrite {
                    entity: ek,
                    kind: wk,
                    via_api: None,
                });
            }
        }
        for ek in effect_entities(model, uc_key) {
            if api_written_entities.contains(&ek) || direct_writes.iter().any(|w| w.entity == ek) {
                continue;
            }
            direct_writes.push(UcWrite {
                entity: ek,
                kind: WriteKind::Updates,
                via_api: None,
            });
        }
        let (fk, singles) = infer_fk_transactions_for_writes(direct_writes, model);
        grouped.extend(fk);
        isolated.extend(singles);

        if grouped.is_empty() && isolated.is_empty() {
            continue;
        }
        grouped.extend(isolated);
        result.push(UsecaseTx {
            usecase: uc_key,
            groups: grouped,
        });
    }

    result
}

fn write_kind(rel: &RelKind) -> Option<WriteKind> {
    match rel {
        RelKind::Creates => Some(WriteKind::Creates),
        RelKind::Updates | RelKind::Writes => Some(WriteKind::Updates),
        RelKind::Deletes => Some(WriteKind::Deletes),
        _ => None,
    }
}

fn isolated_group(w: UcWrite) -> TxGroup {
    TxGroup {
        kind: TxKind::Isolated,
        ordered_writes: vec![w],
    }
}

/// Entities a use case changes through `sets` or `transitions`.
fn effect_entities(model: &SemanticModel, uc: UseCaseKey) -> Vec<EntityKey> {
    let mut out: Vec<EntityKey> = model
        .column_effects
        .iter()
        .filter(|e| e.origin == uc)
        .map(|e| e.entity)
        .chain(
            model
                .proposition_effects
                .iter()
                .filter(|e| e.origin == uc)
                .map(|e| e.entity),
        )
        .chain(
            model
                .state_transitions
                .iter()
                .filter(|t| t.usecase == uc)
                .map(|t| t.entity),
        )
        .collect();
    out.sort();
    out.dedup();
    out
}

/// FK孤立書き込みに対する診断ヒントを生成する: [`UsecaseTx::separate_writes`] ごとに
/// `SeparateTxInferred` warning を返す。
pub fn tx_diagnostics(model: &SemanticModel, txs: &[UsecaseTx]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for utx in txs {
        let uc_id = model
            .use_cases
            .get(utx.usecase)
            .map(|u| u.id.clone())
            .unwrap_or_else(|| "?".to_string());
        for ek in utx.separate_writes() {
            let entity_id = model
                .entities
                .get(ek)
                .map(|e| e.id.clone())
                .unwrap_or_else(|| "?".to_string());
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "usecase",
                &uc_id,
                RdraError::SeparateTxInferred {
                    usecase: uc_id.clone(),
                    entity: entity_id,
                },
                true,
            );
        }
    }
    diags
}

// ── 内部ヘルパー ──────────────────────────────────────────────────────────────

/// The FK-connected groups of the direct writes, then each remaining write on its own,
/// sorted by entity id.
fn infer_fk_transactions_for_writes(
    writes: Vec<UcWrite>,
    model: &SemanticModel,
) -> (Vec<TxGroup>, Vec<TxGroup>) {
    let mut writes = writes;
    dedup_writes(&mut writes, model);

    let write_keys: HashSet<EntityKey> = writes.iter().map(|w| w.entity).collect();

    let mut adj: HashMap<EntityKey, Vec<EntityKey>> = HashMap::new();
    for &ek in &write_keys {
        adj.entry(ek).or_default();
    }

    for (parent, child) in fk_edges(model) {
        adj.entry(parent).or_default().push(child);
        adj.entry(child).or_default().push(parent);
    }

    let mut component_of: HashMap<EntityKey, usize> = HashMap::new();
    let mut next_cid = 0usize;
    let mut start_keys: Vec<EntityKey> = write_keys.iter().copied().collect();
    start_keys.sort_by_key(|ek| {
        model
            .entities
            .get(*ek)
            .map(|e| e.id.clone())
            .unwrap_or_default()
    });

    for start in start_keys {
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
                if let std::collections::hash_map::Entry::Vacant(e) = component_of.entry(nb) {
                    e.insert(cid);
                    queue.push_back(nb);
                }
            }
        }
    }

    let mut comp_writes: HashMap<usize, Vec<UcWrite>> = HashMap::new();
    for w in &writes {
        let cid = component_of[&w.entity];
        comp_writes.entry(cid).or_default().push(w.clone());
    }

    let mut fk_groups: Vec<TxGroup> = Vec::new();
    let mut isolated_writes: Vec<UcWrite> = Vec::new();
    let mut cids: Vec<usize> = comp_writes.keys().cloned().collect();
    cids.sort();

    for cid in cids {
        let comp = comp_writes.remove(&cid).unwrap();
        if comp.len() >= 2 {
            fk_groups.push(TxGroup {
                kind: TxKind::Fk,
                ordered_writes: topological_sort_writes(comp, &fk_parent_map(model), model),
            });
        } else {
            isolated_writes.extend(comp);
        }
    }

    isolated_writes.sort_by_key(|w| {
        model
            .entities
            .get(w.entity)
            .map(|e| e.id.clone())
            .unwrap_or_default()
    });

    (
        fk_groups,
        isolated_writes.into_iter().map(isolated_group).collect(),
    )
}

fn dedup_writes(writes: &mut Vec<UcWrite>, model: &SemanticModel) {
    writes.sort_by_key(|w| {
        (
            model
                .entities
                .get(w.entity)
                .map(|e| e.id.clone())
                .unwrap_or_default(),
            w.kind.rank(),
            w.via_api.is_none(),
        )
    });
    writes.dedup_by_key(|w| w.entity);
}

fn fk_parent_map(model: &SemanticModel) -> HashMap<EntityKey, Vec<EntityKey>> {
    let mut parents_of: HashMap<EntityKey, Vec<EntityKey>> = HashMap::new();
    for (parent, child) in fk_edges(model) {
        parents_of.entry(child).or_default().push(parent);
        parents_of.entry(parent).or_default();
    }
    parents_of
}

/// `(parent, child)` per foreign key: the graph that orders a use case's writes.
pub fn fk_edges(model: &SemanticModel) -> impl Iterator<Item = (EntityKey, EntityKey)> + '_ {
    model.foreign_keys().map(|(_, fk)| (fk.parent, fk.child))
}

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
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errs) = parse(src);
        assert!(errs.is_empty(), "parse errors: {:?}", errs);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "model errors: {:?}", errors);
        model
    }

    /// The one use case's groups in transaction order, as entity ids.
    fn groups(src: &str) -> Vec<(TxKind, Vec<String>)> {
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert_eq!(txs.len(), 1);
        txs[0]
            .groups()
            .map(|g| {
                let ids = g
                    .ordered_writes
                    .iter()
                    .map(|w| model.entities[w.entity].id.clone())
                    .collect();
                (g.kind, ids)
            })
            .collect()
    }

    fn separate(src: &str) -> Vec<String> {
        let model = model_from(src);
        infer_usecase_transactions(&model)
            .iter()
            .flat_map(|utx| utx.separate_writes())
            .map(|ek| model.entities[ek].id.clone())
            .collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    const PLACE_ORDER: &str = r#"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Cart      "カート"   { id: Int @pk }
usecase PlaceOrder "注文を確定する"
relate(OrderLine, Order, N:1)
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
updates(PlaceOrder, Cart)
"#;

    /// PlaceOrder 相当: Order + OrderLine（FK連結、親が先）の後に Cart（FK非連結）。
    #[test]
    fn test_fk_group_and_singleton() {
        assert_eq!(
            groups(PLACE_ORDER),
            vec![
                (TxKind::Fk, ids(&["Order", "OrderLine"])),
                (TxKind::Isolated, ids(&["Cart"])),
            ]
        );
        assert_eq!(separate(PLACE_ORDER), ids(&["Cart"]));
    }

    /// Capture 相当: Payment → Order (1:1) → 全体が1グループ、親 Order が先。
    #[test]
    fn test_one_to_one_single_group() {
        let src = r#"
entity Order   "注文" { id: Int @pk }
entity Payment "決済" { id: Int @pk }
usecase Capture "決済を確定する"
relate(Payment, Order, 1:1)
updates(Capture, Payment)
updates(Capture, Order)
"#;
        assert_eq!(groups(src), vec![(TxKind::Fk, ids(&["Order", "Payment"]))]);
        assert!(separate(src).is_empty());
    }

    /// 共通親を経由する sibling writes は、親自身を書かなくても1つのFK連結成分。
    #[test]
    fn test_sibling_writes_connected_through_unwritten_common_parent() {
        let src = r#"
entity Terminal "端末" { id: Int @pk }
entity CertificateOrder "証明書発注" { id: Int @pk }
entity ClientCertificate "クライアント証明書" { id: Int @pk }
usecase IssueCertificate "証明書を発行する"
relate(CertificateOrder, Terminal, N:1)
relate(ClientCertificate, Terminal, N:1)
creates(IssueCertificate, CertificateOrder)
creates(IssueCertificate, ClientCertificate)
"#;
        assert_eq!(
            groups(src),
            vec![(TxKind::Fk, ids(&["CertificateOrder", "ClientCertificate"]))]
        );
    }

    /// 全書き込みがFKなし → 全部 isolated で、別TXの警告対象はない。
    #[test]
    fn test_all_isolated_no_note() {
        let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
usecase DoSomething "何かする"
creates(DoSomething, A)
updates(DoSomething, B)
"#;
        assert_eq!(
            groups(src),
            vec![
                (TxKind::Isolated, ids(&["A"])),
                (TxKind::Isolated, ids(&["B"])),
            ]
        );
        assert!(separate(src).is_empty(), "no note when no FK groups exist");
    }

    #[test]
    fn test_tx_diagnostics_warns_singleton() {
        let model = model_from(PLACE_ORDER);
        let txs = infer_usecase_transactions(&model);
        let diags = tx_diagnostics(&model, &txs);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].is_warning);
        let msg = diags[0].error.to_string();
        assert!(
            msg.starts_with(
                "usecase 'PlaceOrder' writes 'Cart' in its own transaction after the FK-grouped writes"
            ),
            "{msg}"
        );
    }

    /// api 経由の書き込みは API の原子境界で1グループ、FK親→子の順。
    #[test]
    fn test_api_writes_collected_via_invokes() {
        let src = r#"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
usecase Checkout "チェックアウト"
api OrderApi "注文API"
invokes(Checkout, OrderApi)
creates(OrderApi, Order)
creates(OrderApi, OrderLine)
relate(OrderLine, Order, N:1)
"#;
        assert_eq!(
            groups(src),
            vec![(TxKind::Api, ids(&["Order", "OrderLine"]))]
        );
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert!(txs[0].writes().all(|w| w.via_api.is_some()));
    }

    /// api 経由と直接の混在: 重複 entity は api-origin が優先される
    #[test]
    fn test_api_and_direct_write_deduplicated() {
        let src = r#"
entity Cart "カート" { id: Int @pk }
usecase Checkout "チェックアウト"
api CartApi "カートAPI"
invokes(Checkout, CartApi)
updates(CartApi, Cart)
updates(Checkout, Cart)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        let writes: Vec<&UcWrite> = txs[0].writes().collect();
        assert_eq!(writes.len(), 1, "Cart should appear only once after dedup");
        assert!(
            writes[0].via_api.is_some(),
            "api-origin should be preferred over direct write"
        );
    }

    /// 複数UCで共有されるAPIでは、APIのCRUD境界がそのまま各呼び出しUCに適用される。
    /// 読み取りだけの usecase に見える場合は、書き込みAPIを分けてモデリングする。
    #[test]
    fn test_shared_api_writes_apply_to_each_invoking_usecase() {
        let src = r#"
entity Appointment "予約" { id: Int @pk }
entity ProviderSchedule "予定枠" { id: Int @pk }
usecase SearchAvailability "空き枠検索"
usecase BookAppointment "予約する"
api SchedulingApi "予約API"
invokes(SearchAvailability, SchedulingApi)
invokes(BookAppointment, SchedulingApi)
reads(SchedulingApi, Appointment)
updates(SchedulingApi, ProviderSchedule)
updates(BookAppointment, ProviderSchedule)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        let mut uc_ids: Vec<&str> = txs
            .iter()
            .map(|tx| model.use_cases.get(tx.usecase).unwrap().id.as_str())
            .collect();
        uc_ids.sort();
        assert_eq!(uc_ids, vec!["BookAppointment", "SearchAvailability"]);
        for tx in &txs {
            let writes: Vec<&UcWrite> = tx.writes().collect();
            assert_eq!(writes.len(), 1);
            assert!(
                writes[0].via_api.is_some(),
                "API-origin writes should be rendered from the API lane"
            );
        }
    }

    /// APIはFK非連結の複数エンティティ書き込みでも、1つの原子境界として扱う。
    #[test]
    fn test_api_groups_unrelated_writes_as_atomic_boundary() {
        let src = r#"
entity Store "店舗" { id: Int @pk }
entity StoreOrgAssignmentHistory "店舗組織履歴" { id: Int @pk }
usecase ChangeStoreParentOrg "親組織を変更する"
api StoreParentOrgApi "店舗親組織API"
invokes(ChangeStoreParentOrg, StoreParentOrgApi)
updates(StoreParentOrgApi, Store)
creates(StoreParentOrgApi, StoreOrgAssignmentHistory)
"#;
        let g = groups(src);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, TxKind::Api, "explicit atomicity, not FK inference");
        assert_eq!(g[0].1.len(), 2);
    }

    /// API-bounded and FK groups come first, then the isolated writes.
    #[test]
    fn groups_are_in_transaction_order() {
        let src = r#"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Audit     "監査"     { id: Int @pk }
entity Ledger    "台帳"     { id: Int @pk }
entity Mail      "通知"     { id: Int @pk }
usecase Place "注文確定"
api BookApi "記帳"
api MailApi "通知"
relate(OrderLine, Order, N:1)
invokes(Place, BookApi)
invokes(Place, MailApi)
creates(BookApi, Ledger)
creates(BookApi, Audit)
creates(MailApi, Mail)
creates(Place, Order)
creates(Place, OrderLine)
updates(Place, Audit)
"#;
        assert_eq!(
            groups(src),
            vec![
                (TxKind::Api, ids(&["Audit", "Ledger"])),
                (TxKind::Fk, ids(&["Order", "OrderLine"])),
                (TxKind::Isolated, ids(&["Mail"])),
            ]
        );
        assert_eq!(separate(src), ids(&["Mail"]));
    }

    fn writes_of(model: &SemanticModel, txs: &[UsecaseTx]) -> Vec<(String, String, &'static str)> {
        let mut out: Vec<_> = txs
            .iter()
            .flat_map(|utx| {
                utx.writes().map(move |w| {
                    (
                        model.use_cases[utx.usecase].id.clone(),
                        model.entities[w.entity].id.clone(),
                        w.kind.label(),
                    )
                })
            })
            .collect();
        out.sort();
        out
    }

    /// Writes count `writes` edges, implied updates, and keep the strongest kind per entity.
    #[test]
    fn writes_include_writes_edges_and_implied_updates() {
        let src = r#"
entity Order   "注文" { id: Int @pk  status: Enum(open, closed) }
entity Ledger  "台帳" { id: Int @pk }
entity Audit   "監査" { id: Int @pk  seen: Bool }
usecase Close "締める"
usecase Book  "記帳する"
usecase Open  "開く"
transitions(Order.status, Close, open -> closed)
sets(Close, Audit, seen == true)
writes(Book, Ledger)
updates(Open, Order)
creates(Open, Order)
"#;
        let model = model_from(src);
        let txs = infer_usecase_transactions(&model);
        assert_eq!(
            writes_of(&model, &txs),
            vec![
                ("Book".into(), "Ledger".into(), "update"),
                ("Close".into(), "Audit".into(), "update"),
                ("Close".into(), "Order".into(), "update"),
                ("Open".into(), "Order".into(), "create"),
            ]
        );
    }
}
