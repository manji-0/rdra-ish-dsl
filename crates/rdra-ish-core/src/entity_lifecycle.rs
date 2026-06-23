//! エンティティ単位のライフサイクル集約。
//!
//! `collect_entity_lifecycles` が `collect_event_flows` と同様の集約 API。
//! entity の status Enum と state ノードの対応、およびその entity に属する遷移を返す。

use crate::model::{ColumnType, EntityKey, SemanticModel, StateKey, StateTransition};
use std::collections::{HashMap, HashSet};

/// entity の Enum カラムと state 集合のマッピングを構築する。
/// State id を小文字化して Enum バリアントと照合し、
/// `status` カラムに対応する State キーの集合を返す。
///
/// 戻り値: (状態を表すカラム名, StateKey → バリアント文字列 のマップ)
pub fn link_entity_status_states(
    model: &SemanticModel,
    ek: EntityKey,
) -> Option<(String, HashMap<StateKey, String>)> {
    let entity = &model.entities[ek];

    let state_ids_lower: HashSet<String> =
        model.states.values().map(|s| s.id.to_lowercase()).collect();

    for col in &entity.columns {
        if let ColumnType::Enum(variants) = &col.col_type {
            let variants_lower: Vec<String> = variants.iter().map(|v| v.to_lowercase()).collect();
            let matching = variants_lower
                .iter()
                .filter(|v| state_ids_lower.contains(*v))
                .count();
            if matching * 2 >= variants.len() && matching > 0 {
                let mut map: HashMap<StateKey, String> = HashMap::new();
                for (sk, state) in model.states.iter() {
                    let lower = state.id.to_lowercase();
                    if variants_lower.contains(&lower) {
                        map.insert(sk, lower);
                    }
                }
                return Some((col.name.clone(), map));
            }
        }
    }
    None
}

/// 単一 entity のライフサイクル（状態集合と遷移グラフ）。
#[derive(Debug, Clone)]
pub struct EntityLifecycle {
    pub entity: EntityKey,
    /// status 軸となる Enum カラム名
    pub status_column: String,
    /// この entity に紐づく state ノード（Enum バリアント順）
    pub states: Vec<StateKey>,
    /// この entity の state に属する遷移
    pub transitions: Vec<StateTransition>,
    /// 到達グラフ上の初期状態（いずれの `to` にも登場しない `from`）
    pub initial: Vec<StateKey>,
    /// 到達グラフ上の終端状態（いずれの `from` にも登場しない `to`）
    pub terminal: Vec<StateKey>,
}

/// モデル内の全 entity についてライフサイクルを収集する。
///
/// status Enum と state ノードがリンクできる entity のみ返す。
/// 結果は entity id の辞書順。
pub fn collect_entity_lifecycles(model: &SemanticModel) -> Vec<EntityLifecycle> {
    let mut lifecycles: Vec<EntityLifecycle> = model
        .entities
        .keys()
        .filter_map(|ek| lifecycle_for_entity(model, ek))
        .collect();

    lifecycles.sort_by_key(|lc| model.entities[lc.entity].id.clone());
    lifecycles
}

fn lifecycle_for_entity(model: &SemanticModel, ek: EntityKey) -> Option<EntityLifecycle> {
    let (status_column, state_map) = link_entity_status_states(model, ek)?;
    let entity_states: HashSet<StateKey> = state_map.keys().copied().collect();

    let states = ordered_states(model, ek, &state_map);

    let transitions: Vec<StateTransition> = model
        .state_transitions
        .iter()
        .filter(|st| entity_states.contains(&st.from) && entity_states.contains(&st.to))
        .cloned()
        .collect();

    let to_set: HashSet<StateKey> = transitions.iter().map(|t| t.to).collect();
    let from_set: HashSet<StateKey> = transitions.iter().map(|t| t.from).collect();

    let mut initial: Vec<StateKey> = from_set.difference(&to_set).copied().collect();
    initial.sort_by_key(|sk| model.states[*sk].id.clone());

    let mut terminal: Vec<StateKey> = to_set.difference(&from_set).copied().collect();
    terminal.sort_by_key(|sk| model.states[*sk].id.clone());

    Some(EntityLifecycle {
        entity: ek,
        status_column,
        states,
        transitions,
        initial,
        terminal,
    })
}

/// Enum バリアント宣言順に state キーを並べる。
fn ordered_states(
    model: &SemanticModel,
    ek: EntityKey,
    state_map: &HashMap<StateKey, String>,
) -> Vec<StateKey> {
    let entity = &model.entities[ek];
    let variant_to_key: HashMap<&str, StateKey> = state_map
        .iter()
        .map(|(sk, variant)| (variant.as_str(), *sk))
        .collect();

    let mut ordered = Vec::new();
    for col in &entity.columns {
        if let ColumnType::Enum(variants) = &col.col_type {
            for variant in variants {
                let lower = variant.to_lowercase();
                if let Some(sk) = variant_to_key.get(lower.as_str()) {
                    ordered.push(*sk);
                }
            }
            break;
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        model
    }

    const ORDER_SRC: &str = r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
  delivered_at: DateTime @null
}
event EvCapture  "決済確定"
event EvShip     "発送開始"
event EvDeliver  "配達確認"
event EvCancel   "注文キャンセル"
state Pending   "注文受付"
state Paid      "決済完了"
state Shipped   "発送済"
state Delivered "配達完了"
state Cancelled "キャンセル"
transitions(EvCapture, Pending,  Paid)
transitions(EvShip,    Paid,     Shipped)
transitions(EvDeliver, Shipped,  Delivered)
transitions(EvCancel,  Pending,  Cancelled)
"#;

    #[test]
    fn collect_entity_lifecycles_order() {
        let model = model_from(ORDER_SRC);
        let lifecycles = collect_entity_lifecycles(&model);
        assert_eq!(lifecycles.len(), 1);

        let lc = &lifecycles[0];
        assert_eq!(model.entities[lc.entity].id, "Order");
        assert_eq!(lc.status_column, "status");
        assert_eq!(lc.states.len(), 5);
        assert_eq!(lc.transitions.len(), 4);

        let state_ids: Vec<_> = lc
            .states
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(
            state_ids,
            vec!["Pending", "Paid", "Shipped", "Delivered", "Cancelled"]
        );

        let initial_ids: Vec<_> = lc
            .initial
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(initial_ids, vec!["Pending"]);

        let terminal_ids: Vec<_> = lc
            .terminal
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(terminal_ids, vec!["Cancelled", "Delivered"]);
    }

    #[test]
    fn entity_without_status_enum_is_omitted() {
        let model = model_from(
            r#"
entity Item "Item" { id: Int @pk }
state Orphan "Orphan"
transitions(EvX, Orphan, Orphan)
event EvX "X"
"#,
        );
        assert!(collect_entity_lifecycles(&model).is_empty());
    }
}
