//! Mermaid emitters: RDRA全体図、ER図、状態遷移図。
//!
//! plantuml.rs と同じ3エミッタをMermaid記法で出力する。
//! ヘルパー関数 (node_id / node_label / col_type_str) は plantuml モジュールから再利用。

use crate::plantuml::{col_type_str, node_id, node_label};
use crate::{EmitError, Emitter, Scope, View};
use rdra_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::HashSet;

// ── RDRA全体図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct RdraMermaidEmitter;

impl Emitter for RdraMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_core::reachable_from_bucs(model, buc_ids)),
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
            Scope::Bucs(buc_ids) => Some(rdra_core::reachable_from_bucs(model, buc_ids)),
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
            Scope::Bucs(buc_ids) => Some(rdra_core::reachable_from_bucs(model, buc_ids)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_core::build_model;
    use rdra_syntax::parse;

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
