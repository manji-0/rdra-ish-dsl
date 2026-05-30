//! PlantUML emitters: RDRA全体図、BUC別図、ER図、状態遷移図、sequence図。

use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{
    ActorKey, ApiKey, BucKey, ColumnType, EntityKey, NodeRef, RelKind, ScreenKey, SemanticModel,
    UseCaseKey,
};
use rdra_ish_core::tx::infer_usecase_transactions;
use std::collections::{HashMap, HashSet};

// ── RDRA全体図エミッタ ────────────────────────────────────────────────────────

pub struct RdraPlantUmlEmitter;

impl Emitter for RdraPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ: Scope::Bucs の場合は到達可能ノードのみに絞る
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push('\n');

        // actors
        let mut actor_ids: Vec<_> = model.actors.iter().collect();
        actor_ids.sort_by_key(|(_, a)| &a.id);
        for (k, actor) in &actor_ids {
            if is_visible(&NodeRef::Actor(*k)) {
                out.push_str(&format!("actor \"{}\" as {}\n", actor.label, actor.id));
            }
        }
        out.push('\n');

        // usecases
        let mut uc_ids: Vec<_> = model.use_cases.iter().collect();
        uc_ids.sort_by_key(|(_, u)| &u.id);
        for (k, uc) in &uc_ids {
            if is_visible(&NodeRef::UseCase(*k)) {
                out.push_str(&format!("usecase \"{}\" as {}\n", uc.label, uc.id));
            }
        }
        out.push('\n');

        // bucs (as rectangles)
        let mut buc_ids: Vec<_> = model.bucs.iter().collect();
        buc_ids.sort_by_key(|(_, b)| &b.id);
        for (k, buc) in &buc_ids {
            if is_visible(&NodeRef::Buc(*k)) {
                out.push_str(&format!("rectangle \"{}\" as {}\n", buc.label, buc.id));
            }
        }
        out.push('\n');

        // ext_systems (as components)
        let mut ext_ids: Vec<_> = model.ext_systems.iter().collect();
        ext_ids.sort_by_key(|(_, e)| &e.id);
        for (k, ext) in &ext_ids {
            if is_visible(&NodeRef::ExtSystem(*k)) {
                out.push_str(&format!("component \"{}\" as {}\n", ext.label, ext.id));
            }
        }
        out.push('\n');

        // entities (as databases)
        let mut ent_ids: Vec<_> = model.entities.iter().collect();
        ent_ids.sort_by_key(|(_, e)| &e.id);
        for (k, ent) in &ent_ids {
            if is_visible(&NodeRef::Entity(*k)) {
                out.push_str(&format!("database \"{}\" as {}\n", ent.label, ent.id));
            }
        }
        out.push('\n');

        // screens (as boundary)
        let mut scr_ids: Vec<_> = model.screens.iter().collect();
        scr_ids.sort_by_key(|(_, s)| &s.id);
        for (k, scr) in &scr_ids {
            if is_visible(&NodeRef::Screen(*k)) {
                out.push_str(&format!("boundary \"{}\" as {}\n", scr.label, scr.id));
            }
        }
        out.push('\n');

        // events (as control)
        let mut ev_ids: Vec<_> = model.events.iter().collect();
        ev_ids.sort_by_key(|(_, e)| &e.id);
        for (k, ev) in &ev_ids {
            if is_visible(&NodeRef::Event(*k)) {
                out.push_str(&format!("control \"{}\" as {}\n", ev.label, ev.id));
            }
        }
        out.push('\n');

        // states (as collections)
        let mut st_ids: Vec<_> = model.states.iter().collect();
        st_ids.sort_by_key(|(_, s)| &s.id);
        for (k, st) in &st_ids {
            if is_visible(&NodeRef::State(*k)) {
                out.push_str(&format!("collections \"{}\" as {}\n", st.label, st.id));
            }
        }
        out.push('\n');

        // relations (両端のノードが visible なもののみ出力)
        let mut relations: Vec<_> = model.relations.iter().collect();
        relations.sort_by_key(|r| format!("{:?}{:?}", r.from, r.to));
        for rel in &relations {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            // API ノードは RDRA 全体図には出さない
            if matches!(&rel.from, NodeRef::Api(_)) || matches!(&rel.to, NodeRef::Api(_)) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let arrow = match &rel.kind {
                    RelKind::Performs => format!("{} --> {}", from_id, to_id),
                    RelKind::Uses => format!("{} --> {}", from_id, to_id),
                    RelKind::Reads => {
                        format!("{} ..> {} : reads", from_id, to_id)
                    }
                    RelKind::Writes => {
                        format!("{} ..> {} : writes", from_id, to_id)
                    }
                    RelKind::Creates => {
                        format!("{} ..> {} : creates", from_id, to_id)
                    }
                    RelKind::Updates => {
                        format!("{} ..> {} : updates", from_id, to_id)
                    }
                    RelKind::Deletes => {
                        format!("{} ..> {} : deletes", from_id, to_id)
                    }
                    RelKind::Displays => {
                        format!("{} ..> {} : displays", from_id, to_id)
                    }
                    RelKind::Shows => {
                        format!("{} ..> {} : shows", from_id, to_id)
                    }
                    RelKind::Raises => {
                        format!("{} ..> {} : raises", from_id, to_id)
                    }
                    RelKind::Triggers => {
                        format!("{} ..> {} : triggers", from_id, to_id)
                    }
                    RelKind::Contains => {
                        format!("{} ..> {} : contains", from_id, to_id)
                    }
                    RelKind::Belongs => {
                        format!("{} ..> {} : belongs", from_id, to_id)
                    }
                    RelKind::Motivates => {
                        format!("{} ..> {} : motivates", from_id, to_id)
                    }
                    RelKind::Transitions => {
                        // 状態遷移図エミッタで扱うのでここではスキップ
                        continue;
                    }
                    RelKind::Invokes => {
                        // API層は概要図には出さない
                        continue;
                    }
                    RelKind::RelateOneToOne => {
                        format!("{} -- {}", from_id, to_id)
                    }
                    RelKind::RelateOneToMany => {
                        format!("{} -- {}", from_id, to_id)
                    }
                    RelKind::RelateManyToOne => {
                        format!("{} -- {}", from_id, to_id)
                    }
                    RelKind::RelateManyToMany => {
                        format!("{} -- {}", from_id, to_id)
                    }
                };
                out.push_str(&arrow);
                out.push('\n');
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── 状態遷移図エミッタ ─────────────────────────────────────────────────────────

pub struct StateDiagramEmitter;

impl Emitter for StateDiagramEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ: Scope::Bucs の場合は到達可能ノードのみに絞る
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        // state_transitions は完全な (event, from, to) 三つ組
        // BUCフィルタ適用: from/to が両方 visible な遷移のみ
        let transitions: Vec<_> = model
            .state_transitions
            .iter()
            .filter(|t| is_visible(&t.from) && is_visible(&t.to))
            .collect();

        if transitions.is_empty() {
            return Ok("@startuml\n@enduml\n".to_string());
        }

        // 初期状態 = いずれの to にも登場しない from
        let to_set: HashSet<&NodeRef> = transitions.iter().map(|t| &t.to).collect();
        let mut initial_states: Vec<&NodeRef> = transitions
            .iter()
            .map(|t| &t.from)
            .filter(|nr| !to_set.contains(nr))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        initial_states.sort_by_key(|nr| node_id(model, nr).unwrap_or(""));

        let mut out = String::new();
        out.push_str("@startuml\n");

        for initial in &initial_states {
            if let Some(id) = node_id(model, initial) {
                out.push_str(&format!("[*] --> {}\n", id));
            }
        }

        // 遷移（event, from, to の三つ組をそのまま出力）
        let mut sorted: Vec<_> = transitions.iter().collect();
        sorted.sort_by_key(|t| {
            format!(
                "{}{}{}",
                node_id(model, &t.from).unwrap_or(""),
                node_id(model, &t.to).unwrap_or(""),
                node_id(model, &t.event).unwrap_or(""),
            )
        });
        for t in &sorted {
            if let (Some(from_id), Some(to_id), Some(ev_label)) = (
                node_id(model, &t.from),
                node_id(model, &t.to),
                node_label(model, &t.event),
            ) {
                out.push_str(&format!("{} --> {} : {}\n", from_id, to_id, ev_label));
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── ER図エミッタ ──────────────────────────────────────────────────────────────

pub struct ErPlantUmlEmitter;

impl Emitter for ErPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ: Scope::Bucs の場合は到達可能ノードのみに絞る
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push('\n');

        // entities
        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        for (k, ent) in &ents {
            if !is_visible(&NodeRef::Entity(*k)) {
                continue;
            }
            out.push_str(&format!("entity \"{}\" as {} {{\n", ent.label, ent.id));

            // PKs first
            let pks: Vec<_> = ent.columns.iter().filter(|c| c.is_pk).collect();
            for col in &pks {
                let type_str = col_type_str(&col.col_type);
                out.push_str(&format!("  *{} : {} <<PK>>\n", col.name, type_str));
            }

            // Separator
            if !pks.is_empty() {
                out.push_str("  --\n");
            }

            // Non-PK columns
            for col in ent.columns.iter().filter(|c| !c.is_pk) {
                let type_str = col_type_str(&col.col_type);
                if col.is_fk {
                    out.push_str(&format!("  {} : {} <<FK>>\n", col.name, type_str));
                } else {
                    out.push_str(&format!("  {} : {}\n", col.name, type_str));
                }
            }

            out.push_str("}\n");
        }

        out.push('\n');

        // ER relations (relate only)
        // Collect entity id → key mapping
        let entity_key_map: std::collections::HashMap<&str, EntityKey> = model
            .entities
            .iter()
            .map(|(k, e)| (e.id.as_str(), k))
            .collect();

        let mut er_rels: Vec<_> = model
            .relations
            .iter()
            .filter(|r| {
                matches!(
                    r.kind,
                    RelKind::RelateOneToOne
                        | RelKind::RelateOneToMany
                        | RelKind::RelateManyToOne
                        | RelKind::RelateManyToMany
                )
            })
            .collect();
        er_rels.sort_by_key(|r| format!("{:?}{:?}", r.from, r.to));

        for rel in &er_rels {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let _ = entity_key_map; // suppress unused warning
                let line = match &rel.kind {
                    RelKind::RelateManyToOne => {
                        format!("{} }}o--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToMany => {
                        format!("{} ||--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToOne => {
                        format!("{} ||--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToMany => {
                        format!("{} }}o--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    _ => continue,
                };
                out.push_str(&line);
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── sequence図エミッタ ────────────────────────────────────────────────────────

/// 書き込み系ユースケースのシーケンス図を生成する。
///
/// FK連結成分を `group transaction (inferred from FK)` で囲み、
/// FK非連結の孤立書き込みには `note right` で診断ヒントを付ける。
/// `--buc` による絞り込み（`Scope::Bucs`）に対応。
pub struct SequenceDiagramEmitter;

impl Emitter for SequenceDiagramEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole => None,
        };
        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        // ── アクター解決マップ構築 ─────────────────────────────────────────
        let mut uc_to_bucs: HashMap<UseCaseKey, Vec<BucKey>> = HashMap::new();
        let mut buc_to_actors: HashMap<BucKey, Vec<ActorKey>> = HashMap::new();
        let mut uc_to_screens: HashMap<UseCaseKey, Vec<ScreenKey>> = HashMap::new();
        let mut uc_to_apis: HashMap<UseCaseKey, Vec<ApiKey>> = HashMap::new();

        for rel in &model.relations {
            match &rel.kind {
                RelKind::Contains => {
                    if let (NodeRef::Buc(bk), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                        uc_to_bucs.entry(*uk).or_default().push(*bk);
                    }
                }
                RelKind::Performs => {
                    if let (NodeRef::Actor(ak), NodeRef::Buc(bk)) = (&rel.from, &rel.to) {
                        buc_to_actors.entry(*bk).or_default().push(*ak);
                    }
                }
                RelKind::Displays => {
                    if let (NodeRef::UseCase(uk), NodeRef::Screen(sk)) = (&rel.from, &rel.to) {
                        uc_to_screens.entry(*uk).or_default().push(*sk);
                    }
                }
                RelKind::Invokes => {
                    if let (NodeRef::UseCase(uk), NodeRef::Api(ak)) = (&rel.from, &rel.to) {
                        uc_to_apis.entry(*uk).or_default().push(*ak);
                    }
                }
                _ => {}
            }
        }
        let mut direct_actor_of: HashMap<UseCaseKey, Vec<ActorKey>> = HashMap::new();
        for rel in &model.relations {
            if rel.kind == RelKind::Performs {
                if let (NodeRef::Actor(ak), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                    direct_actor_of.entry(*uk).or_default().push(*ak);
                }
            }
        }

        // ── TX境界推論 ────────────────────────────────────────────────────
        let uc_txs = infer_usecase_transactions(model);
        let uc_tx_map: HashMap<UseCaseKey, &rdra_ish_core::UsecaseTx> =
            uc_txs.iter().map(|t| (t.usecase, t)).collect();

        // ── 表示対象ユースケース（書き込みありのもの、可視なもの） ─────────
        let mut uc_list: Vec<(UseCaseKey, &rdra_ish_core::model::UseCase)> = model
            .use_cases
            .iter()
            .filter(|(k, _)| {
                is_visible(&NodeRef::UseCase(*k))
                    && uc_tx_map.get(k).map(|t| t.has_writes()).unwrap_or(false)
            })
            .collect();
        uc_list.sort_by_key(|(_, u)| u.id.as_str());

        if uc_list.is_empty() {
            return Ok("@startuml\n' no write-heavy usecases found\n@enduml\n".to_string());
        }

        // ── 必要な参加者を収集 ─────────────────────────────────────────────
        let mut actor_keys: HashSet<ActorKey> = HashSet::new();
        let mut entity_keys: HashSet<EntityKey> = HashSet::new();
        let mut screen_keys: HashSet<ScreenKey> = HashSet::new();
        let mut api_keys: HashSet<ApiKey> = HashSet::new();
        let mut has_legacy_uc = false; // API のない usecase が1つでもあれば System レーンを出す

        for (uk, _) in &uc_list {
            for &bk in uc_to_bucs.get(uk).into_iter().flatten() {
                for &ak in buc_to_actors.get(&bk).into_iter().flatten() {
                    actor_keys.insert(ak);
                }
            }
            for &ak in direct_actor_of.get(uk).into_iter().flatten() {
                actor_keys.insert(ak);
            }
            if let Some(tx) = uc_tx_map.get(uk) {
                for g in &tx.fk_groups {
                    for w in &g.ordered_writes {
                        entity_keys.insert(w.entity);
                    }
                }
                for w in &tx.isolated_writes {
                    entity_keys.insert(w.entity);
                }
            }
            for &sk in uc_to_screens.get(uk).into_iter().flatten() {
                screen_keys.insert(sk);
            }
            if let Some(apis) = uc_to_apis.get(uk) {
                for &ak in apis {
                    api_keys.insert(ak);
                }
            } else {
                has_legacy_uc = true;
            }
        }

        // ── 出力組み立て ────────────────────────────────────────────────────
        let mut out = String::from("@startuml\n!theme plain\n\n");

        let mut actors_sorted: Vec<(ActorKey, &rdra_ish_core::model::Actor)> = model
            .actors
            .iter()
            .filter(|(k, _)| actor_keys.contains(k))
            .collect();
        actors_sorted.sort_by_key(|(_, a)| a.id.as_str());
        for (_, actor) in &actors_sorted {
            out.push_str(&format!("actor \"{}\" as {}\n", actor.label, actor.id));
        }

        // System レーン: レガシー UC が1件でもあれば維持（後方互換）
        if has_legacy_uc {
            out.push_str("participant \"システム\" as System\n");
        }

        // API 参加者宣言（画面の前に挿入: actor → api → screen → entity の左→右順）
        let mut apis_sorted: Vec<(ApiKey, &rdra_ish_core::model::Api)> = model
            .apis
            .iter()
            .filter(|(k, _)| api_keys.contains(k))
            .collect();
        apis_sorted.sort_by_key(|(_, a)| a.id.as_str());
        for (_, api) in &apis_sorted {
            out.push_str(&format!("control \"{}\" as {}\n", api.label, api.id));
        }

        let mut ents_sorted: Vec<(EntityKey, &rdra_ish_core::model::Entity)> = model
            .entities
            .iter()
            .filter(|(k, _)| entity_keys.contains(k))
            .collect();
        ents_sorted.sort_by_key(|(_, e)| e.id.as_str());
        for (_, ent) in &ents_sorted {
            out.push_str(&format!("database \"{}\" as {}\n", ent.label, ent.id));
        }

        let mut scrs_sorted: Vec<(ScreenKey, &rdra_ish_core::model::Screen)> = model
            .screens
            .iter()
            .filter(|(k, _)| screen_keys.contains(k))
            .collect();
        scrs_sorted.sort_by_key(|(_, s)| s.id.as_str());
        for (_, scr) in &scrs_sorted {
            out.push_str(&format!("boundary \"{}\" as {}\n", scr.label, scr.id));
        }
        out.push('\n');

        // ── ユースケースごとのシーケンス ──────────────────────────────────
        for (uk, uc) in &uc_list {
            out.push_str(&format!("== {} ==\n", uc.label));

            let actor_id: Option<String> = uc_to_bucs
                .get(uk)
                .and_then(|bucs| bucs.first())
                .and_then(|bk| buc_to_actors.get(bk))
                .and_then(|actors| actors.first())
                .and_then(|ak| model.actors.get(*ak))
                .map(|a| a.id.clone())
                .or_else(|| {
                    direct_actor_of
                        .get(uk)
                        .and_then(|actors| actors.first())
                        .and_then(|ak| model.actors.get(*ak))
                        .map(|a| a.id.clone())
                });
            let actor_ref = actor_id.as_deref().unwrap_or("System");

            let screen_id: Option<String> = uc_to_screens
                .get(uk)
                .and_then(|s| s.first())
                .and_then(|sk| model.screens.get(*sk))
                .map(|s| s.id.clone());
            let screen_label: Option<String> = uc_to_screens
                .get(uk)
                .and_then(|s| s.first())
                .and_then(|sk| model.screens.get(*sk))
                .map(|s| s.label.clone());

            let invoked_apis = uc_to_apis.get(uk);

            if let Some(apis) = invoked_apis.filter(|a| !a.is_empty()) {
                // ── API有りパス ──────────────────────────────────────────────
                // 最初のAPIを代表として使用（複数APIは各書き込みの via_api で振り分け）
                let first_api_id = model
                    .apis
                    .get(apis[0])
                    .map(|a| a.id.as_str())
                    .unwrap_or("System");

                // Actor → Screen（あれば）→ API
                if let Some(ref sid) = screen_id {
                    out.push_str(&format!("{} -> {} : {}\n", actor_ref, sid, uc.label));
                    out.push_str(&format!("{} -> {} : {}\n", sid, first_api_id, uc.label));
                } else {
                    out.push_str(&format!(
                        "{} -> {} : {}\n",
                        actor_ref, first_api_id, uc.label
                    ));
                }
                out.push_str(&format!("activate {}\n", first_api_id));

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        out.push_str("group transaction (inferred from FK)\n");
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                // via_api に対応するAPIのID、なければ最初のAPIを使用
                                let src = w
                                    .via_api
                                    .and_then(|ak| model.apis.get(ak))
                                    .map(|a| a.id.as_str())
                                    .unwrap_or(first_api_id);
                                out.push_str(&format!(
                                    "{} -> {} : {}\n",
                                    src,
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            let src = w
                                .via_api
                                .and_then(|ak| model.apis.get(ak))
                                .map(|a| a.id.as_str())
                                .unwrap_or(first_api_id);
                            out.push_str(&format!("{} -> {} : {}\n", src, ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str("note right : FK非連結 — 別TX？@atomicで明示を\n");
                            }
                        }
                    }
                }

                // API → Screen（あれば）→ Actor へ返す
                if let Some(ref sid) = screen_id {
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        first_api_id,
                        sid,
                        screen_label.as_deref().unwrap_or("")
                    ));
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        sid,
                        actor_ref,
                        screen_label.as_deref().unwrap_or("")
                    ));
                } else {
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        first_api_id, actor_ref, uc.label
                    ));
                }
                out.push_str(&format!("deactivate {}\n", first_api_id));
            } else {
                // ── レガシーパス（System ライン）─────────────────────────────
                out.push_str(&format!("{} -> System : {}\n", actor_ref, uc.label));
                out.push_str("activate System\n");

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        out.push_str("group transaction (inferred from FK)\n");
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                out.push_str(&format!(
                                    "System -> {} : {}\n",
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            out.push_str(&format!("System -> {} : {}\n", ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str("note right : FK非連結 — 別TX？@atomicで明示を\n");
                            }
                        }
                    }
                }

                if let Some(ref sid) = screen_id {
                    if let Some(ref slabel) = screen_label {
                        out.push_str(&format!("System --> {} : {}\n", actor_ref, slabel));
                    } else {
                        out.push_str(&format!("System --> {} : {}\n", actor_ref, sid));
                    }
                }

                out.push_str("deactivate System\n");
            }

            out.push('\n');
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── イベントフロー図エミッタ (PlantUML) ──────────────────────────────────────

pub struct EventFlowPlantUmlEmitter;

impl Emitter for EventFlowPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole => None,
        };
        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        // UC / Event / State が同じ ID を持てるので種別ごとにプレフィックスを付ける
        let ev_mid = |id: &str| format!("ev__{}", id);
        let uc_mid = |id: &str| format!("uc__{}", id);
        let st_mid = |id: &str| format!("st__{}", id);

        let flows = rdra_ish_core::collect_event_flows(model);

        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push_str("left to right direction\n\n");

        let mut declared: HashSet<String> = HashSet::new();

        for flow in &flows {
            let ev_nr = NodeRef::Event(flow.event);
            if !is_visible(&ev_nr) {
                continue;
            }
            let ev = match model.events.get(flow.event) {
                Some(e) => e,
                None => continue,
            };
            let ev_id = ev_mid(&ev.id);

            if declared.insert(ev_id.clone()) {
                out.push_str(&format!("card \"{}\" as {}\n", ev.label, ev_id));
            }

            let mut raised_by = flow.raised_by.to_vec();
            raised_by
                .sort_by_key(|&uk| model.use_cases.get(uk).map(|u| u.id.as_str()).unwrap_or(""));
            for uk in raised_by {
                let uc_nr = NodeRef::UseCase(uk);
                if !is_visible(&uc_nr) {
                    continue;
                }
                let uc = match model.use_cases.get(uk) {
                    Some(u) => u,
                    None => continue,
                };
                let uid = uc_mid(&uc.id);
                if declared.insert(uid.clone()) {
                    out.push_str(&format!("usecase \"{}\" as {}\n", uc.label, uid));
                }
                out.push_str(&format!("{} ..> {} : raises\n", uid, ev_id));
            }

            let mut triggers = flow.triggers_ucs.to_vec();
            triggers
                .sort_by_key(|&uk| model.use_cases.get(uk).map(|u| u.id.as_str()).unwrap_or(""));
            for uk in triggers {
                let uc_nr = NodeRef::UseCase(uk);
                if !is_visible(&uc_nr) {
                    continue;
                }
                let uc = match model.use_cases.get(uk) {
                    Some(u) => u,
                    None => continue,
                };
                let uid = uc_mid(&uc.id);
                if declared.insert(uid.clone()) {
                    out.push_str(&format!("usecase \"{}\" as {}\n", uc.label, uid));
                }
                out.push_str(&format!("{} ..> {} : triggers\n", ev_id, uid));
            }

            let mut transitions = flow.transitions.to_vec();
            transitions.sort_by_key(|(from_sk, _)| {
                model
                    .states
                    .get(*from_sk)
                    .map(|s| s.id.as_str())
                    .unwrap_or("")
            });
            for (from_sk, to_sk) in transitions {
                let from_st = match model.states.get(from_sk) {
                    Some(s) => s,
                    None => continue,
                };
                let to_st = match model.states.get(to_sk) {
                    Some(s) => s,
                    None => continue,
                };
                let fid = st_mid(&from_st.id);
                let tid = st_mid(&to_st.id);
                if declared.insert(fid.clone()) {
                    out.push_str(&format!("state \"{}\" as {}\n", from_st.label, fid));
                }
                if declared.insert(tid.clone()) {
                    out.push_str(&format!("state \"{}\" as {}\n", to_st.label, tid));
                }
                out.push_str(&format!("{} --> {} : {}\n", fid, tid, ev.label));
            }
        }

        out.push_str("\n@enduml\n");
        Ok(out)
    }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

pub(crate) fn node_id<'a>(model: &'a SemanticModel, node: &NodeRef) -> Option<&'a str> {
    match node {
        NodeRef::Actor(k) => model.actors.get(*k).map(|a| a.id.as_str()),
        NodeRef::ExtSystem(k) => model.ext_systems.get(*k).map(|e| e.id.as_str()),
        NodeRef::Requirement(k) => model.requirements.get(*k).map(|r| r.id.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.id.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.id.as_str()),
        NodeRef::UsageScene(k) => model.usage_scenes.get(*k).map(|u| u.id.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.id.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.id.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.id.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.id.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.id.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.id.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.id.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.id.as_str()),
    }
}

pub(crate) fn node_label<'a>(model: &'a SemanticModel, node: &NodeRef) -> Option<&'a str> {
    match node {
        NodeRef::Actor(k) => model.actors.get(*k).map(|a| a.label.as_str()),
        NodeRef::ExtSystem(k) => model.ext_systems.get(*k).map(|e| e.label.as_str()),
        NodeRef::Requirement(k) => model.requirements.get(*k).map(|r| r.label.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.label.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.label.as_str()),
        NodeRef::UsageScene(k) => model.usage_scenes.get(*k).map(|u| u.label.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.label.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.label.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.label.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.label.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.label.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.label.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.label.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.label.as_str()),
    }
}

pub(crate) fn col_type_str(ct: &ColumnType) -> &'static str {
    match ct {
        ColumnType::Int => "Int",
        ColumnType::String => "String",
        ColumnType::Money => "Money",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Bool => "Bool",
        ColumnType::Decimal => "Decimal",
        ColumnType::Enum(_) => "Enum",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        model
    }

    #[test]
    fn test_rdra_plantuml_emit() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = RdraPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("@startuml"));
        assert!(result.contains("actor \"顧客\" as Customer"));
        assert!(result.contains("usecase \"商品を探す\" as Browse"));
        assert!(result.contains("Customer --> Browse"));
        assert!(result.contains("@enduml"));
    }

    #[test]
    fn test_er_plantuml_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, "N:1")
"#;
        let model = model_from(src);
        let view = View::er();
        let result = ErPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("@startuml"));
        assert!(result.contains("entity \"注文\" as Order"));
        assert!(result.contains("*id : Int <<PK>>"));
        assert!(result.contains("customer_id : Int <<FK>>"));
        assert!(result.contains("}o--||"));
        assert!(result.contains("@enduml"));
    }

    #[test]
    fn test_er_plantuml_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, "N:1")
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErPlantUmlEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_er_plantuml_buc_filter() {
        // BUCフィルタを使うとBUCが触れるエンティティのみが出力される
        let src = r#"
buc BucA "業務A"
usecase UcA "ユースケースA"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
contains(BucA, UcA)
reads(UcA, EntityA)
"#;
        let model = model_from(src);
        // BUCフィルタあり
        let view = View {
            scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
            filter: crate::Filter::Er,
        };
        let result = ErPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }

    #[test]
    fn test_sequence_fk_group_and_singleton() {
        let src = r#"
actor Customer "顧客"
buc BucOrder "注文を処理する"
usecase PlaceOrder "注文を確定する"
screen OrderCompleteScreen "注文完了画面"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Cart      "カート"   { id: Int @pk }
relate(OrderLine, Order, "N:1")
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
updates(PlaceOrder, Cart)
displays(PlaceOrder, OrderCompleteScreen)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = SequenceDiagramEmitter.emit(&model, &view).unwrap();

        // 参加者宣言
        assert!(result.contains("actor \"顧客\" as Customer"));
        assert!(result.contains("database \"注文\" as Order"));
        assert!(result.contains("database \"注文明細\" as OrderLine"));
        assert!(result.contains("database \"カート\" as Cart"));
        assert!(result.contains("boundary \"注文完了画面\" as OrderCompleteScreen"));

        // UCセクション見出し
        assert!(result.contains("== 注文を確定する =="));

        // アクター → System メッセージ
        assert!(result.contains("Customer -> System : 注文を確定する"));

        // FK連結グループ（Order → OrderLine の順）
        assert!(result.contains("group transaction (inferred from FK)"));
        assert!(result.contains("System -> Order : create"));
        assert!(result.contains("System -> OrderLine : create"));
        assert!(result.contains("end\n"));

        // FK非連結書き込みと note
        assert!(result.contains("System -> Cart : update"));
        assert!(result.contains("note right : FK非連結"));

        // 画面レスポンス
        assert!(result.contains("System --> Customer : 注文完了画面"));

        // Order が OrderLine より前に出現する
        let order_pos = result.find("System -> Order : create").unwrap();
        let orderline_pos = result.find("System -> OrderLine : create").unwrap();
        assert!(
            order_pos < orderline_pos,
            "Order(parent) must precede OrderLine(child)"
        );

        // スナップショット
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_sequence_buc_filter() {
        // BUCフィルタで絞り込んだとき、対象BUCのUCのみ出力される
        let src = r#"
actor Customer "顧客"
buc BucA "BUC-A"
buc BucB "BUC-B"
usecase UcA "ユースケースA"
usecase UcB "ユースケースB"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
performs(Customer, BucA)
contains(BucA, UcA)
creates(UcA, EntityA)
performs(Customer, BucB)
contains(BucB, UcB)
creates(UcB, EntityB)
"#;
        let model = model_from(src);
        let view = View::bucs(vec!["BucA".to_string()]);
        let result = SequenceDiagramEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("ユースケースA"), "BucA's UC should appear");
        assert!(
            !result.contains("ユースケースB"),
            "BucB's UC should be excluded"
        );
    }

    #[test]
    fn test_rdra_plantuml_multi_buc_filter() {
        // 複数BUC指定で両BUCの到達ノードが出力される
        let src = r#"
buc BucA "業務A"
buc BucB "業務B"
usecase UcA "ユースケースA"
usecase UcB "ユースケースB"
usecase UcC "ユースケースC"
contains(BucA, UcA)
contains(BucB, UcB)
"#;
        let model = model_from(src);
        let view = View::bucs(vec!["BucA".to_string(), "BucB".to_string()]);
        let result = RdraPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("BucA"), "BucA should be included");
        assert!(result.contains("BucB"), "BucB should be included");
        assert!(
            result.contains("UcA"),
            "UcA should be included (reachable from BucA)"
        );
        assert!(
            result.contains("UcB"),
            "UcB should be included (reachable from BucB)"
        );
        assert!(
            !result.contains("UcC"),
            "UcC should be excluded (unreachable)"
        );
    }
}
