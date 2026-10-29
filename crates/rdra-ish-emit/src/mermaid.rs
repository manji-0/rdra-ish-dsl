//! Mermaid emitters: RDRA全体図、ER図、状態遷移図、event-flow図。

use crate::diagram_common::{
    col_type_str, lifecycle_start_states, node_id, node_label, quote, render_descriptions,
    render_event_flows,
};
use crate::{
    collect_object_graph_nodes, object_graph_layer, object_graph_rel_label, prefixed_node_label,
    view_node_visible, view_relation_visible, EmitError, Emitter, Scope, View, OBJECT_GRAPH_LAYERS,
};
use rdra_ish_core::model::{EntityKey, NodeRef, RelKind, SemanticModel, StateKey};
use std::collections::{HashMap, HashSet};

// ── RDRA グラフ図エミッタ (Mermaid) ─────────────────────────────────────────

pub struct RdraMermaidEmitter;

impl Emitter for RdraMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            let scoped = match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            };
            scoped && view_node_visible(view, nr)
        };

        let visible_nodes = collect_object_graph_nodes(model, &is_visible);
        let visible_set: HashSet<NodeRef> = visible_nodes.iter().cloned().collect();

        let mut out = String::new();
        out.push_str("flowchart LR\n");

        for layer in OBJECT_GRAPH_LAYERS {
            out.push_str(&format!(
                "  subgraph {}[{}]\n",
                layer.mermaid_id(),
                layer.label()
            ));
            out.push_str("    direction TB\n");
            for nr in visible_nodes
                .iter()
                .filter(|nr| object_graph_layer(nr) == layer)
            {
                if let (Some(id), Some(label)) = (node_id(model, nr), node_label(model, nr)) {
                    let label = quote(&prefixed_node_label(nr, label));
                    let line = match nr {
                        NodeRef::Actor(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Requirement(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Adr(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Nfr(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::ExtSystem(_) => format!("    {}[/\"{}\"/]\n", id, label),
                        NodeRef::Quality(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Constraint(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Business(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Buc(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Flow(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Step(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Timer(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Medium(_) => format!("    {}[/\"{}\"/]\n", id, label),
                        NodeRef::Permission(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Variation(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Condition(_) => format!("    {}{{\"{}\"}}\n", id, label),
                        NodeRef::UseCase(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Screen(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::Field(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::Event(_) => format!("    {}{{\"{}\"}}\n", id, label),
                        NodeRef::Api(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Dto(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::System(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Entity(_) => format!("    {}[(\"{}\")]\n", id, label),
                        NodeRef::State(_) => format!("    {}(\"{}\")\n", id, label),
                    };
                    out.push_str(&line);
                }
            }
            out.push_str("  end\n");
        }

        let mut relations: Vec<_> = model.relations.iter().collect();
        relations.sort_by_key(|r| format!("{:?}{:?}{:?}", r.from, r.kind, r.to));
        for rel in relations {
            if !visible_set.contains(&rel.from) || !visible_set.contains(&rel.to) {
                continue;
            }
            if !view_relation_visible(view, &rel.kind) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let label = object_graph_rel_label(&rel.kind);
                let line = match rel.kind {
                    RelKind::Performs | RelKind::Contains | RelKind::Owns => {
                        format!("  {} -->|{}| {}\n", from_id, label, to_id)
                    }
                    RelKind::RelateOneToOne
                    | RelKind::RelateOneToMany
                    | RelKind::RelateManyToOne
                    | RelKind::RelateManyToMany => {
                        format!("  {} ---|{}| {}\n", from_id, label, to_id)
                    }
                    _ => format!("  {} -.->|{}| {}\n", from_id, label, to_id),
                };
                out.push_str(&line);
            }
        }

        if view.show_descriptions {
            render_descriptions(&mut out, model, &visible_nodes);
        }

        while out.ends_with("\n\n") {
            out.pop();
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
            Scope::Whole | Scope::UseCases(_) => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        let state_id_to_key: HashMap<String, StateKey> = model
            .states
            .iter()
            .map(|(sk, s)| (s.id.clone(), sk))
            .collect();

        let resolve_sk = |entity: EntityKey, variant: &str| -> Option<StateKey> {
            let entity_id = &model.entities.get(entity)?.id;
            let state_id = format!("{entity_id}_{variant}");
            state_id_to_key.get(&state_id).copied()
        };

        let transitions: Vec<_> = model
            .state_transitions
            .iter()
            .filter_map(|t| {
                let from_sk = resolve_sk(t.entity, &t.from)?;
                let to_sk = resolve_sk(t.entity, &t.to)?;
                Some((from_sk, to_sk, t))
            })
            .filter(|(from_sk, to_sk, _)| {
                is_visible(&NodeRef::State(*from_sk)) && is_visible(&NodeRef::State(*to_sk))
            })
            .collect();

        if transitions.is_empty() {
            return Ok("stateDiagram-v2\n".to_string());
        }

        // 初期状態 = 遷移系 (lifecycle) が作成時に与える開始値
        let initial_states = lifecycle_start_states(
            model,
            transitions
                .iter()
                .map(|(_, _, t)| (t.entity, t.column.clone())),
            resolve_sk,
        );

        let mut out = String::new();
        out.push_str("stateDiagram-v2\n");

        for initial in &initial_states {
            if let Some(id) = node_id(model, &NodeRef::State(*initial)) {
                out.push_str(&format!("  [*] --> {}\n", id));
            }
        }

        let mut sorted: Vec<_> = transitions.iter().collect();
        sorted.sort_by_key(|&&(from_sk, to_sk, t)| {
            format!(
                "{}{}{}",
                node_id(model, &NodeRef::State(from_sk)).unwrap_or(""),
                node_id(model, &NodeRef::State(to_sk)).unwrap_or(""),
                node_id(model, &NodeRef::UseCase(t.usecase)).unwrap_or(""),
            )
        });

        // ノード名ラベル（state "label" as id）を出力してから遷移を出力
        let mut defined: HashSet<String> = HashSet::new();
        for &&(from_sk, to_sk, _) in &sorted {
            for sk in [from_sk, to_sk] {
                let nr = NodeRef::State(sk);
                if let (Some(id), Some(label)) = (node_id(model, &nr), node_label(model, &nr)) {
                    if defined.insert(id.to_string()) {
                        out.push_str(&format!(
                            "  state \"{}\" as {}\n",
                            prefixed_node_label(&nr, label),
                            id
                        ));
                    }
                }
            }
        }

        for &&(from_sk, to_sk, t) in &sorted {
            let from_nr = NodeRef::State(from_sk);
            let to_nr = NodeRef::State(to_sk);
            let usecase_nr = NodeRef::UseCase(t.usecase);
            if let (Some(from_id), Some(to_id), Some(uc_label)) = (
                node_id(model, &from_nr),
                node_id(model, &to_nr),
                node_label(model, &usecase_nr),
            ) {
                out.push_str(&format!(
                    "  {} --> {} : {}\n",
                    from_id,
                    to_id,
                    prefixed_node_label(&usecase_nr, uc_label)
                ));
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
            Scope::Whole | Scope::UseCases(_) => None,
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
                let type_str = col_type_str(model, &col.col_type);
                if col.is_pk {
                    out.push_str(&format!("    {} {} PK\n", type_str, col.name));
                } else if model.column_fk(*k, &col.name).is_some() {
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
            .enumerate()
            .filter(|(_, r)| {
                matches!(
                    r.kind,
                    RelKind::RelateOneToOne
                        | RelKind::RelateOneToMany
                        | RelKind::RelateManyToOne
                        | RelKind::RelateManyToMany
                )
            })
            .collect();
        er_rels.sort_by_key(|(_, r)| format!("{:?}{:?}", r.from, r.to));

        for &(index, rel) in &er_rels {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let Some(glyph) = crate::er_glyph(model, index) else {
                    continue;
                };
                out.push_str(&format!("  {from_id} {glyph} {to_id} : \"\"\n"));
            }
        }

        Ok(out)
    }
}

// ── イベントフロー図エミッタ (Mermaid) ───────────────────────────────────────

pub struct EventFlowMermaidEmitter;

impl Emitter for EventFlowMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let mut out = String::from("flowchart LR\n");
        render_event_flows(&mut out, model, &view.scope);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::{declare_flow_node, model_from, render_event_flow, FlowNode};
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    #[test]
    fn test_rdra_mermaid_layers() {
        let src = r#"
actor Customer "顧客"
buc BucOrder "注文業務"
usecase PlaceOrder "注文する"
screen OrderScreen "注文画面"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, OrderScreen)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("flowchart LR"));
        assert!(result.contains("subgraph layer_value[System Value]"));
        assert!(result.contains("subgraph layer_environment[External Environment]"));
        assert!(result.contains("subgraph layer_boundary[System Boundary]"));
        assert!(result.contains("subgraph layer_system[System]"));
        assert!(result.contains("Customer([\"👤 顧客\"])"));
        assert!(result.contains("BucOrder[\"📦 注文業務\"]"));
        assert!(result.contains("OrderScreen[[\"🖥️ 注文画面\"]]"));
        assert!(result.contains("OrderApi[\"🔌 注文API\"]"));
        assert!(result.contains("Order[(\"🗄️ 注文\")]"));
        let boundary_pos = result
            .find("subgraph layer_boundary[System Boundary]")
            .unwrap();
        let system_pos = result.find("subgraph layer_system[System]").unwrap();
        let screen_pos = result.find("OrderScreen[[\"🖥️ 注文画面\"]]").unwrap();
        let api_pos = result.find("OrderApi[\"🔌 注文API\"]").unwrap();
        assert!(boundary_pos < screen_pos);
        assert!(screen_pos < system_pos);
        assert!(system_pos < api_pos);
        assert!(result.contains("Customer -->|performs| BucOrder"));
        assert!(result.contains("PlaceOrder -.->|invokes| OrderApi"));
        assert!(result.contains("OrderApi -.->|creates| Order"));
    }

    #[test]
    fn test_rdra_mermaid_applies_node_and_edge_filters() {
        let src = r#"
actor Customer "Customer"
usecase PlaceOrder "Place order"
api OrderApi "Order API"
entity Order "Order" { id: Int @pk }
performs(Customer, PlaceOrder)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
        let model = model_from(src);
        let view = View::whole().with_graph_filters(
            vec!["usecase".to_string(), "api".to_string()],
            vec!["invokes".to_string()],
        );

        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("PlaceOrder([\"✅ Place order\"])"));
        assert!(result.contains("OrderApi[\"🔌 Order API\"]"));
        assert!(result.contains("PlaceOrder -.->|invokes| OrderApi"));
        assert!(!result.contains("Customer(["));
        assert!(!result.contains("Order[(\""));
        assert!(!result.contains("performs"));
        assert!(!result.contains("creates"));
    }

    #[test]
    fn test_er_mermaid_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, N:1)
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
relate(Order, Customer, N:1)
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn a_child_side_minimum_draws_one_or_more_at_the_child_end() {
        let src = "entity Order \"O\" { id: Int @pk }\nentity Line \"L\" { id: Int @pk }\nentity Tag \"T\" { id: Int @pk }\nrelate(Line, Order, 1..*:1)\nrelate(Order, Tag, 1:1..*)\n";
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let mermaid = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        assert!(
            mermaid.contains("  Line }|--|| Order : \"\"\n"),
            "{mermaid}"
        );
        assert!(mermaid.contains("  Order ||--|{ Tag : \"\"\n"), "{mermaid}");
    }

    #[test]
    fn test_state_mermaid_emit() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
}
usecase Publish "公開する"
transitions(Doc.status, Publish, draft -> published)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("stateDiagram-v2"));
        assert!(result.contains("[*] --> Doc_draft"));
        assert!(result.contains("Doc_draft --> Doc_published"));
        assert!(result.contains("公開する"));
    }

    #[test]
    fn test_state_start_is_the_creation_status_not_a_graph_source() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, review, published) @default(draft)
}
usecase Submit "提出"
usecase Publish "公開"
usecase Reopen "戻す"
creates(Submit, Doc)
sets(Submit, Doc, status == review)
transitions(Doc.status, Publish, review -> published)
transitions(Doc.status, Reopen, published -> review)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("[*] --> Doc_review"), "{result}");
        assert!(!result.contains("[*] --> Doc_draft"), "{result}");
    }

    #[test]
    fn test_state_start_of_a_preexisting_entity_is_its_base() {
        let src = r#"
entity Acct "口座" {
  id: Int @pk
  status: Enum(active, frozen) @default(active)
}
usecase Freeze "凍結"
usecase Thaw "解除"
transitions(Acct.status, Freeze, active -> frozen)
transitions(Acct.status, Thaw, frozen -> active)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("[*] --> Acct_active"), "{result}");
        assert!(!result.contains("[*] --> Acct_frozen"), "{result}");
    }

    #[test]
    fn test_rdra_mermaid_draws_a_condition_with_its_derived_links() {
        let src = r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  id: Int @pk
  method: Method
  status: Enum(open, paid) @default(open)
}
usecase Collect "Collect"
transitions(Pay.status, Collect, open -> paid)
condition Insured "Insured"
holds(Insured, Pay.method == card, Pay.status == paid)
applies_to(Insured, Collect)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("Insured{\"🔀 Insured\"}"), "{result}");
        assert!(result.contains("Method[\"🎚️ Method\"]"), "{result}");
        assert!(result.contains("Insured -.->|combines| Method"), "{result}");
        assert!(result.contains("Insured -.->|combines| Pay"), "{result}");
        assert!(
            result.contains("Insured -.->|applies_to| Collect"),
            "{result}"
        );
    }

    #[test]
    fn test_event_flow_mermaid_renders_timers_and_extsystems() {
        let src = r#"
usecase Book "Book"
usecase Receive "Receive"
usecase Sweep "Sweep"
extsystem Gateway "Gateway"
extsystem Lab "Lab"
timer Nightly "Nightly"
event EvBooked "Booked"
event EvResult "Result"
raises(Book, EvBooked)
triggers(EvBooked, Gateway)
raises(Lab, EvResult)
triggers(EvResult, Receive)
triggers(Nightly, Sweep)
"#;
        let model = model_from(src);
        let result = EventFlowMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(
            result.contains("xs__Lab -.->|raises| ev__EvResult"),
            "{result}"
        );
        assert!(
            result.contains("ev__EvBooked -.->|delivers| xs__Gateway"),
            "{result}"
        );
        assert!(
            result.contains("tm__Nightly -.->|triggers| uc__Sweep"),
            "{result}"
        );
    }

    #[test]
    fn test_event_flow_mermaid_renders_triggered_buc() {
        let src = r#"
buc BucBillingClaims "Billing Claims"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#;
        let model = model_from(src);
        let result = EventFlowMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();

        assert!(result.contains("ev__EncounterSigned -.->|triggers| buc__BucBillingClaims"));
        assert!(result.contains("buc__BucBillingClaims[[\"📦 Billing Claims\"]]"));
    }

    #[test]
    fn test_event_flow_helpers_deduplicate_declarations_and_render_transitions() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
usecase ReviewClaim "Review Claim"
buc BucBillingClaims "Billing Claims"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(pending, signed) @default(pending)
}
entity Claim "Claim" {
  id: Int @pk
  status: Enum(open, reviewed) @default(open)
}
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, ReviewClaim)
triggers(EncounterSigned, BucBillingClaims)
transitions(Encounter.status, SignEncounter, pending -> signed)
transitions(Claim.status, ReviewClaim, open -> reviewed)
"#;
        let model = model_from(src);
        let flows = rdra_ish_core::collect_event_flows(&model);
        let flow = flows
            .iter()
            .find(|flow| {
                model
                    .events
                    .get(flow.event)
                    .map(|event| event.id == "EncounterSigned")
                    .unwrap_or(false)
            })
            .unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();

        render_event_flow(&mut out, &mut declared, &model, &None, flow);
        render_event_flow(&mut out, &mut declared, &model, &None, flow);

        assert_eq!(
            out.matches("ev__EncounterSigned{\"⚡ Encounter Signed\"}")
                .count(),
            1
        );
        assert!(out.contains("uc__SignEncounter -.->|raises| ev__EncounterSigned"));
        assert!(out.contains("ev__EncounterSigned -.->|triggers| uc__ReviewClaim"));
        assert!(out.contains("ev__EncounterSigned -.->|triggers| buc__BucBillingClaims"));
        // The event leads to the transition its triggered use case drives, not to its raiser's.
        assert!(out.contains("st__Claim_open -->|⚡ Encounter Signed| st__Claim_reviewed"));
        assert!(!out.contains("st__Encounter_pending -->"));
    }

    #[test]
    fn test_event_flow_declare_helpers_emit_prefixed_nodes_once() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
event EncounterSigned "Encounter Signed"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(pending, signed) @default(pending)
}
raises(SignEncounter, EncounterSigned)
transitions(Encounter.status, SignEncounter, pending -> signed)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let usecase = model
            .use_cases
            .iter()
            .find(|(_, usecase)| usecase.id == "SignEncounter")
            .map(|(key, _)| key)
            .unwrap();
        let buc = model
            .bucs
            .iter()
            .find(|(_, buc)| buc.id == "BucBillingClaims")
            .map(|(key, _)| key)
            .unwrap();
        let state = model
            .states
            .iter()
            .find(|(_, state)| state.id == "Encounter_pending")
            .map(|(key, _)| key)
            .unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();

        assert_eq!(
            declare_flow_node(&mut out, &mut declared, &model, FlowNode::Event(flow.event))
                .as_deref(),
            Some("ev__EncounterSigned")
        );
        assert_eq!(
            declare_flow_node(&mut out, &mut declared, &model, FlowNode::UseCase(usecase))
                .as_deref(),
            Some("uc__SignEncounter")
        );
        assert_eq!(
            declare_flow_node(&mut out, &mut declared, &model, FlowNode::Buc(buc)).as_deref(),
            Some("buc__BucBillingClaims")
        );
        assert_eq!(
            declare_flow_node(&mut out, &mut declared, &model, FlowNode::State(state)).as_deref(),
            Some("st__Encounter_pending")
        );

        assert!(out.contains("ev__EncounterSigned{\"⚡ Encounter Signed\"}"));
        assert!(out.contains("uc__SignEncounter([\"✅ Sign Encounter\"])"));
        assert!(out.contains("buc__BucBillingClaims[[\"📦 Billing Claims\"]]"));
        assert!(out.contains("st__Encounter_pending(\"🔄 pending\")"));
    }

    #[test]
    fn test_event_flow_visibility_skips_hidden_event_scope() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();
        let reachable = Some(HashSet::new());

        render_event_flow(&mut out, &mut declared, &model, &reachable, &flow);

        assert!(out.is_empty());
        assert!(declared.is_empty());
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
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        };
        let result = ErMermaidEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }

    #[test]
    fn test_mermaid_show_description_renders_annotations() {
        let src = r#"
actor Customer "Customer" description "Places orders"
usecase Browse "Browse" description "Finds products"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let view = View::whole().with_descriptions(true);

        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("Customer_description[\"Places orders\"]"));
        assert!(result.contains("Customer_description -. description .- Customer"));
        assert!(result.contains("Browse_description[\"Finds products\"]"));
        assert!(result.contains("Browse_description -. description .- Browse"));
    }
}
