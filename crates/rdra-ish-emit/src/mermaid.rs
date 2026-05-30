//! Mermaid emitters: RDRA全体図、ER図、状態遷移図。
//!
//! plantuml.rs と同じ3エミッタをMermaid記法で出力する。
//! ヘルパー関数 (node_id / node_label / col_type_str) は plantuml モジュールから再利用。

use crate::plantuml::{col_type_str, node_id, node_label};
use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{
    ActorKey, BucKey, EntityKey, NodeRef, RelKind, ScreenKey, SemanticModel, UseCaseKey,
};
use rdra_ish_core::tx::infer_usecase_transactions;
use std::collections::{HashMap, HashSet};

// ── RDRA全体図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct RdraMermaidEmitter;

impl Emitter for RdraMermaidEmitter {
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

        let mut out = String::new();
        out.push_str("graph TD\n");

        // actors
        let mut actors: Vec<_> = model.actors.iter().collect();
        actors.sort_by_key(|(_, a)| &a.id);
        for (k, actor) in &actors {
            if is_visible(&NodeRef::Actor(*k)) {
                out.push_str(&format!("  {}([\"👤 {}\"])\n", actor.id, actor.label));
            }
        }

        // usecases
        let mut ucs: Vec<_> = model.use_cases.iter().collect();
        ucs.sort_by_key(|(_, u)| &u.id);
        for (k, uc) in &ucs {
            if is_visible(&NodeRef::UseCase(*k)) {
                out.push_str(&format!("  {}([\"{}\"])\n", uc.id, uc.label));
            }
        }

        // bucs
        let mut bucs: Vec<_> = model.bucs.iter().collect();
        bucs.sort_by_key(|(_, b)| &b.id);
        for (k, buc) in &bucs {
            if is_visible(&NodeRef::Buc(*k)) {
                out.push_str(&format!("  {}[\"📦 {}\"]\n", buc.id, buc.label));
            }
        }

        // ext_systems
        let mut exts: Vec<_> = model.ext_systems.iter().collect();
        exts.sort_by_key(|(_, e)| &e.id);
        for (k, ext) in &exts {
            if is_visible(&NodeRef::ExtSystem(*k)) {
                out.push_str(&format!("  {}[/\"{}\"/]\n", ext.id, ext.label));
            }
        }

        // entities
        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);
        for (k, ent) in &ents {
            if is_visible(&NodeRef::Entity(*k)) {
                out.push_str(&format!("  {}[(\"🗄 {}\")]\n", ent.id, ent.label));
            }
        }

        // screens
        let mut scrs: Vec<_> = model.screens.iter().collect();
        scrs.sort_by_key(|(_, s)| &s.id);
        for (k, scr) in &scrs {
            if is_visible(&NodeRef::Screen(*k)) {
                out.push_str(&format!("  {}[[\"{}\"]]\n", scr.id, scr.label));
            }
        }

        // events
        let mut evs: Vec<_> = model.events.iter().collect();
        evs.sort_by_key(|(_, e)| &e.id);
        for (k, ev) in &evs {
            if is_visible(&NodeRef::Event(*k)) {
                out.push_str(&format!("  {}{{\"{}\"}}\n", ev.id, ev.label));
            }
        }

        // states
        let mut sts: Vec<_> = model.states.iter().collect();
        sts.sort_by_key(|(_, s)| &s.id);
        for (k, st) in &sts {
            if is_visible(&NodeRef::State(*k)) {
                out.push_str(&format!("  {}(\"{}\")  \n", st.id, st.label));
            }
        }

        // relations
        let mut relations: Vec<_> = model.relations.iter().collect();
        relations.sort_by_key(|r| format!("{:?}{:?}", r.from, r.to));
        for rel in &relations {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let arrow = match &rel.kind {
                    RelKind::Performs | RelKind::Uses => {
                        format!("  {} --> {}\n", from_id, to_id)
                    }
                    RelKind::Reads => {
                        format!("  {} -.->|reads| {}\n", from_id, to_id)
                    }
                    RelKind::Writes => {
                        format!("  {} -.->|writes| {}\n", from_id, to_id)
                    }
                    RelKind::Creates => {
                        format!("  {} -.->|creates| {}\n", from_id, to_id)
                    }
                    RelKind::Updates => {
                        format!("  {} -.->|updates| {}\n", from_id, to_id)
                    }
                    RelKind::Deletes => {
                        format!("  {} -.->|deletes| {}\n", from_id, to_id)
                    }
                    RelKind::Displays => {
                        format!("  {} -.->|displays| {}\n", from_id, to_id)
                    }
                    RelKind::Shows => {
                        format!("  {} -.->|shows| {}\n", from_id, to_id)
                    }
                    RelKind::Raises => {
                        format!("  {} -.->|raises| {}\n", from_id, to_id)
                    }
                    RelKind::Triggers => {
                        format!("  {} -.->|triggers| {}\n", from_id, to_id)
                    }
                    RelKind::Contains => {
                        format!("  {} --> {}\n", from_id, to_id)
                    }
                    RelKind::Belongs => {
                        format!("  {} --> {}\n", from_id, to_id)
                    }
                    RelKind::Motivates => {
                        format!("  {} -.->|motivates| {}\n", from_id, to_id)
                    }
                    RelKind::Transitions => {
                        // 状態遷移図エミッタで扱うのでスキップ
                        continue;
                    }
                    RelKind::RelateOneToOne
                    | RelKind::RelateOneToMany
                    | RelKind::RelateManyToOne
                    | RelKind::RelateManyToMany => {
                        format!("  {} --- {}\n", from_id, to_id)
                    }
                };
                out.push_str(&arrow);
            }
        }

        Ok(out)
    }
}

// ── 状態遷移図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct StateMermaidEmitter;

impl Emitter for StateMermaidEmitter {
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

        let transitions: Vec<_> = model
            .state_transitions
            .iter()
            .filter(|t| is_visible(&t.from) && is_visible(&t.to))
            .collect();

        if transitions.is_empty() {
            return Ok("stateDiagram-v2\n".to_string());
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
        out.push_str("stateDiagram-v2\n");

        for initial in &initial_states {
            if let Some(id) = node_id(model, initial) {
                out.push_str(&format!("  [*] --> {}\n", id));
            }
        }

        let mut sorted: Vec<_> = transitions.iter().collect();
        sorted.sort_by_key(|t| {
            format!(
                "{}{}{}",
                node_id(model, &t.from).unwrap_or(""),
                node_id(model, &t.to).unwrap_or(""),
                node_id(model, &t.event).unwrap_or(""),
            )
        });

        // ノード名ラベル（state "label" as id）を出力してから遷移を出力
        let mut defined: HashSet<String> = HashSet::new();
        for t in &sorted {
            for nr in [&t.from, &t.to] {
                if let (Some(id), Some(label)) = (node_id(model, nr), node_label(model, nr)) {
                    if defined.insert(id.to_string()) && id != label {
                        out.push_str(&format!("  state \"{}\" as {}\n", label, id));
                    }
                }
            }
        }

        for t in &sorted {
            if let (Some(from_id), Some(to_id), Some(ev_label)) = (
                node_id(model, &t.from),
                node_id(model, &t.to),
                node_label(model, &t.event),
            ) {
                out.push_str(&format!("  {} --> {} : {}\n", from_id, to_id, ev_label));
            }
        }

        Ok(out)
    }
}

// ── ER図エミッタ (Mermaid) ────────────────────────────────────────────────────

pub struct ErMermaidEmitter;

impl Emitter for ErMermaidEmitter {
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

        let mut out = String::new();
        out.push_str("erDiagram\n");

        // entities
        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        for (k, ent) in &ents {
            if !is_visible(&NodeRef::Entity(*k)) {
                continue;
            }
            out.push_str(&format!("  {} {{\n", ent.id));

            for col in &ent.columns {
                let type_str = col_type_str(&col.col_type);
                if col.is_pk {
                    out.push_str(&format!("    {} {} PK\n", type_str, col.name));
                } else if col.is_fk {
                    out.push_str(&format!("    {} {} FK\n", type_str, col.name));
                } else {
                    out.push_str(&format!("    {} {}\n", type_str, col.name));
                }
            }

            out.push_str("  }\n");
        }

        // ER relations
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
                // Mermaid erDiagram の基数記法:
                //   ||--||  1対1
                //   ||--o{  1対多
                //   }o--||  多対1
                //   }o--o{  多対多
                let line = match &rel.kind {
                    RelKind::RelateOneToOne => {
                        format!("  {} ||--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToMany => {
                        format!("  {} ||--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToOne => {
                        format!("  {} }}o--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToMany => {
                        format!("  {} }}o--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    _ => continue,
                };
                out.push_str(&line);
            }
        }

        Ok(out)
    }
}

// ── シーケンス図エミッタ (Mermaid) ───────────────────────────────────────────

pub struct SequenceMermaidEmitter;

impl Emitter for SequenceMermaidEmitter {
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

        // UC→BUC、BUC→Actor、UC→Screen マップ
        let mut uc_to_bucs: HashMap<UseCaseKey, Vec<BucKey>> = HashMap::new();
        let mut buc_to_actors: HashMap<BucKey, Vec<ActorKey>> = HashMap::new();
        let mut uc_to_screens: HashMap<UseCaseKey, Vec<ScreenKey>> = HashMap::new();
        let mut direct_actor_of: HashMap<UseCaseKey, Vec<ActorKey>> = HashMap::new();

        for rel in &model.relations {
            match &rel.kind {
                RelKind::Contains => {
                    if let (NodeRef::Buc(bk), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                        uc_to_bucs.entry(*uk).or_default().push(*bk);
                    }
                }
                RelKind::Performs => match (&rel.from, &rel.to) {
                    (NodeRef::Actor(ak), NodeRef::Buc(bk)) => {
                        buc_to_actors.entry(*bk).or_default().push(*ak);
                    }
                    (NodeRef::Actor(ak), NodeRef::UseCase(uk)) => {
                        direct_actor_of.entry(*uk).or_default().push(*ak);
                    }
                    _ => {}
                },
                RelKind::Displays => {
                    if let (NodeRef::UseCase(uk), NodeRef::Screen(sk)) = (&rel.from, &rel.to) {
                        uc_to_screens.entry(*uk).or_default().push(*sk);
                    }
                }
                _ => {}
            }
        }

        let uc_txs = infer_usecase_transactions(model);
        let uc_tx_map: HashMap<UseCaseKey, &rdra_ish_core::UsecaseTx> =
            uc_txs.iter().map(|t| (t.usecase, t)).collect();

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
            return Ok("sequenceDiagram\n%% no write-heavy usecases found\n".to_string());
        }

        // 必要な参加者を収集
        let mut actor_keys: HashSet<ActorKey> = HashSet::new();
        let mut entity_keys: HashSet<EntityKey> = HashSet::new();
        let mut screen_keys: HashSet<ScreenKey> = HashSet::new();

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
        }

        let mut out = String::from("sequenceDiagram\n");

        // 参加者宣言
        let mut actors_sorted: Vec<(ActorKey, &rdra_ish_core::model::Actor)> = model
            .actors
            .iter()
            .filter(|(k, _)| actor_keys.contains(k))
            .collect();
        actors_sorted.sort_by_key(|(_, a)| a.id.as_str());
        for (_, actor) in &actors_sorted {
            out.push_str(&format!("  actor {} as {}\n", actor.id, actor.label));
        }
        out.push_str("  participant System as システム\n");

        let mut ents_sorted: Vec<(EntityKey, &rdra_ish_core::model::Entity)> = model
            .entities
            .iter()
            .filter(|(k, _)| entity_keys.contains(k))
            .collect();
        ents_sorted.sort_by_key(|(_, e)| e.id.as_str());
        for (_, ent) in &ents_sorted {
            out.push_str(&format!("  participant {} as {}\n", ent.id, ent.label));
        }

        let mut scrs_sorted: Vec<(ScreenKey, &rdra_ish_core::model::Screen)> = model
            .screens
            .iter()
            .filter(|(k, _)| screen_keys.contains(k))
            .collect();
        scrs_sorted.sort_by_key(|(_, s)| s.id.as_str());
        for (_, scr) in &scrs_sorted {
            out.push_str(&format!("  participant {} as {}\n", scr.id, scr.label));
        }
        out.push('\n');

        // セクション見出し用: 最初と最後の参加者ID
        let first_id = actors_sorted
            .first()
            .map(|(_, a)| a.id.as_str())
            .unwrap_or("System");
        let last_id = scrs_sorted
            .last()
            .map(|(_, s)| s.id.as_str())
            .or_else(|| ents_sorted.last().map(|(_, e)| e.id.as_str()))
            .unwrap_or("System");

        // ユースケースごとのシーケンス
        for (uk, uc) in &uc_list {
            if first_id == last_id {
                out.push_str(&format!("  Note over {}: {}\n", first_id, uc.label));
            } else {
                out.push_str(&format!(
                    "  Note over {},{}: {}\n",
                    first_id, last_id, uc.label
                ));
            }

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

            out.push_str(&format!("  {}->System: {}\n", actor_ref, uc.label));
            out.push_str("  activate System\n");

            if let Some(tx) = uc_tx_map.get(uk) {
                let singletons_set: HashSet<EntityKey> =
                    tx.singletons_note.iter().cloned().collect();

                for group in &tx.fk_groups {
                    out.push_str("  rect rgb(245,245,245)\n");
                    out.push_str("    Note right of System: transaction (inferred from FK)\n");
                    for w in &group.ordered_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            out.push_str(&format!("    System->>{}: {}\n", ent.id, w.kind.label()));
                        }
                    }
                    out.push_str("  end\n");
                }

                for w in &tx.isolated_writes {
                    if let Some(ent) = model.entities.get(w.entity) {
                        out.push_str(&format!("  System->>{}: {}\n", ent.id, w.kind.label()));
                        if singletons_set.contains(&w.entity) {
                            out.push_str(
                                "  Note right of System: FK非連結 — 別TX？@atomicで明示を\n",
                            );
                        }
                    }
                }
            }

            if let Some(sk) = uc_to_screens.get(uk).and_then(|s| s.first()) {
                if let Some(scr) = model.screens.get(*sk) {
                    out.push_str(&format!("  System-->>{}: {}\n", actor_ref, scr.label));
                }
            }

            out.push_str("  deactivate System\n\n");
        }

        Ok(out)
    }
}

// ── イベントフロー図エミッタ (Mermaid) ───────────────────────────────────────

pub struct EventFlowMermaidEmitter;

impl Emitter for EventFlowMermaidEmitter {
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
        out.push_str("flowchart LR\n");

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
                out.push_str(&format!("  {}{{\"{}\"}}\n", ev_id, ev.label));
            }

            // raises: UC -.->|raises| Event
            let mut raised_by: Vec<_> = flow.raised_by.iter().copied().collect();
            raised_by.sort_by_key(|&uk| {
                model.use_cases.get(uk).map(|u| u.id.as_str()).unwrap_or("")
            });
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
                    out.push_str(&format!("  {}([\"{}\"])\n", uid, uc.label));
                }
                out.push_str(&format!("  {} -.->|raises| {}\n", uid, ev_id));
            }

            // triggers: Event -.->|triggers| UC
            let mut triggers: Vec<_> = flow.triggers_ucs.iter().copied().collect();
            triggers.sort_by_key(|&uk| {
                model.use_cases.get(uk).map(|u| u.id.as_str()).unwrap_or("")
            });
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
                    out.push_str(&format!("  {}([\"{}\"])\n", uid, uc.label));
                }
                out.push_str(&format!("  {} -.->|triggers| {}\n", ev_id, uid));
            }

            // transitions: From -->|event_label| To
            let mut transitions: Vec<_> = flow.transitions.iter().copied().collect();
            transitions.sort_by_key(|(from_sk, _)| {
                model.states.get(*from_sk).map(|s| s.id.as_str()).unwrap_or("")
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
                    out.push_str(&format!("  {}(\"{}\")\n", fid, from_st.label));
                }
                if declared.insert(tid.clone()) {
                    out.push_str(&format!("  {}(\"{}\")\n", tid, to_st.label));
                }
                out.push_str(&format!("  {} -->|{}| {}\n", fid, ev.label, tid));
            }
        }

        Ok(out)
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
    fn test_rdra_mermaid_emit() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("graph TD"));
        assert!(result.contains("Customer"));
        assert!(result.contains("顧客"));
        assert!(result.contains("Browse"));
        assert!(result.contains("商品を探す"));
        assert!(result.contains("Customer --> Browse"));
    }

    #[test]
    fn test_er_mermaid_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, "N:1")
"#;
        let model = model_from(src);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        assert!(result.contains("erDiagram"));
        assert!(result.contains("Order {"));
        assert!(result.contains("Int id PK"));
        assert!(result.contains("Customer {"));
        assert!(result.contains("Int id PK"));
        assert!(result.contains("}}o--||") || result.contains("}o--||"));
    }

    #[test]
    fn test_er_mermaid_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, "N:1")
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_state_mermaid_emit() {
        // transitions の引数順は (event, from, to)
        let src = r#"
state Draft "下書き"
state Published "公開"
event Publish "公開する"
transitions(Publish, Draft, Published)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("stateDiagram-v2"));
        assert!(result.contains("[*] --> Draft"));
        assert!(result.contains("Draft --> Published"));
        assert!(result.contains("公開する"));
    }

    #[test]
    fn test_er_mermaid_buc_filter() {
        let src = r#"
buc BucA "業務A"
usecase UcA "ユースケースA"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
contains(BucA, UcA)
reads(UcA, EntityA)
"#;
        let model = model_from(src);
        let view = View {
            scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
            filter: crate::Filter::Er,
        };
        let result = ErMermaidEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }
}
