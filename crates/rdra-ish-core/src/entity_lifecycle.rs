//! エンティティ単位のライフサイクル集約。
//!
//! `collect_entity_lifecycles` が `collect_event_flows` と同様の集約 API。
//! entity の status Enum と state ノードの対応、およびその entity に属する遷移を返す。

use crate::model::{ColumnType, EntityKey, SemanticModel, StateKey, StateTransition};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Pick the lifecycle status column for an entity from its transitions.
/// Prefers `status` / `*_status`, then lexicographically first column.
fn preferred_lifecycle_column(model: &SemanticModel, ek: EntityKey) -> Option<String> {
    let cols: BTreeSet<String> = model
        .state_transitions
        .iter()
        .filter(|st| st.entity == ek)
        .map(|st| st.column.clone())
        .collect();
    if cols.is_empty() {
        return None;
    }
    cols.iter()
        .find(|c| *c == "status" || c.ends_with("_status"))
        .cloned()
        .or_else(|| cols.iter().next().cloned())
}

/// Resolve a transition/state variant string to the declared Enum casing.
fn declared_variant_casing(
    model: &SemanticModel,
    ek: EntityKey,
    column: &str,
    variant: &str,
) -> String {
    model
        .entities
        .get(ek)
        .and_then(|e| e.columns.iter().find(|c| c.name == column))
        .and_then(|c| match &c.col_type {
            ColumnType::Enum(variants) => variants
                .iter()
                .find(|v| *v == variant || v.eq_ignore_ascii_case(variant))
                .cloned(),
            _ => None,
        })
        .unwrap_or_else(|| variant.to_string())
}

/// entity の Enum カラムと state 集合のマッピングを構築する。
///
/// `transitions(Entity.col, ...)` 由来の遷移があれば、その列と entity-scoped state
/// (`{Entity}_{variant}`) を優先する。なければ旧来のグローバル state id 照合にフォールバック。
/// Map values use the **declared Enum casing** (not lowercased).
pub fn link_entity_status_states(
    model: &SemanticModel,
    ek: EntityKey,
) -> Option<(String, HashMap<StateKey, String>)> {
    let entity = &model.entities[ek];
    let entity_id = &entity.id;

    if let Some(column) = preferred_lifecycle_column(model, ek) {
        let prefix = format!("{entity_id}_");
        let mut map: HashMap<StateKey, String> = HashMap::new();
        for st in model
            .state_transitions
            .iter()
            .filter(|st| st.entity == ek && st.column == column)
        {
            for variant_name in [&st.from, &st.to] {
                let declared = declared_variant_casing(model, ek, &column, variant_name);
                let prefixed_id = format!("{prefix}{declared}");
                if let Some((sk, _)) = model.states.iter().find(|(_, s)| {
                    s.id == prefixed_id
                        || s.id.eq_ignore_ascii_case(variant_name)
                        || s.id == format!("{prefix}{variant_name}")
                }) {
                    map.insert(sk, declared);
                }
            }
        }
        if !map.is_empty() {
            return Some((column, map));
        }
    }

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
                    if let Some(declared) = variants.iter().find(|v| v.to_lowercase() == lower) {
                        map.insert(sk, declared.clone());
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
/// status Enum と state ノードがリンクできる entity、および
/// `sets` / 制約のみで駆動される entity（遷移なし）を返す。
/// 結果は entity id の辞書順。
pub fn collect_entity_lifecycles(model: &SemanticModel) -> Vec<EntityLifecycle> {
    let mut lifecycles: Vec<EntityLifecycle> = model
        .entities
        .keys()
        .filter_map(|ek| lifecycle_for_entity(model, ek))
        .collect();

    let covered: HashSet<EntityKey> = lifecycles.iter().map(|l| l.entity).collect();
    for ek in model.entities.keys() {
        if covered.contains(&ek) {
            continue;
        }
        if let Some(lc) = lifecycle_from_effects_only(model, ek) {
            lifecycles.push(lc);
        }
    }

    lifecycles.sort_by_key(|lc| model.entities[lc.entity].id.clone());
    lifecycles
}

/// Entities that have sets/constraints but no `transitions` still need a TLA
/// lifecycle so effects and Safety are not silently dropped.
fn lifecycle_from_effects_only(model: &SemanticModel, ek: EntityKey) -> Option<EntityLifecycle> {
    let has_effects = model.column_effects.iter().any(|e| e.entity == ek)
        || model.proposition_effects.iter().any(|e| e.entity == ek)
        || model.entity_invariants.iter().any(|i| i.entity == ek)
        || model.forbidden_constraints.iter().any(|f| f.entity == ek)
        || model.required_constraints.iter().any(|r| r.entity == ek)
        || model.exclusive_constraints.iter().any(|x| x.entity == ek);
    if !has_effects {
        return None;
    }

    let entity = model.entities.get(ek)?;
    let status_column = preferred_effect_status_column(model, ek, entity)?;

    Some(EntityLifecycle {
        entity: ek,
        status_column,
        states: Vec::new(),
        transitions: Vec::new(),
        initial: Vec::new(),
        terminal: Vec::new(),
    })
}

fn preferred_effect_status_column(
    model: &SemanticModel,
    ek: EntityKey,
    entity: &crate::model::Entity,
) -> Option<String> {
    // Prefer Enum columns touched by sets.
    let effect_cols: HashSet<&str> = model
        .column_effects
        .iter()
        .filter(|e| e.entity == ek)
        .map(|e| e.column.as_str())
        .collect();
    if let Some(col) = entity.columns.iter().find(|c| {
        effect_cols.contains(c.name.as_str()) && matches!(c.col_type, ColumnType::Enum(_))
    }) {
        return Some(col.name.clone());
    }
    entity
        .columns
        .iter()
        .find(|c| {
            (c.name == "status" || c.name.ends_with("_status"))
                && matches!(c.col_type, ColumnType::Enum(_))
        })
        .or_else(|| {
            entity
                .columns
                .iter()
                .find(|c| matches!(c.col_type, ColumnType::Enum(_)))
        })
        .map(|c| c.name.clone())
        .or_else(|| effect_cols.iter().next().map(|s| (*s).to_string()))
}

fn lifecycle_for_entity(model: &SemanticModel, ek: EntityKey) -> Option<EntityLifecycle> {
    let (status_column, state_map) = link_entity_status_states(model, ek)?;

    let states = ordered_states(model, ek, &status_column, &state_map);

    let transitions: Vec<StateTransition> = model
        .state_transitions
        .iter()
        .filter(|st| st.entity == ek && st.column == status_column)
        .cloned()
        .collect();

    // Keys are declared casing; look up case-insensitively.
    let variant_to_sk: HashMap<String, StateKey> = state_map
        .iter()
        .map(|(sk, v)| (v.to_lowercase(), *sk))
        .collect();

    let to_variants: HashSet<String> = transitions.iter().map(|t| t.to.to_lowercase()).collect();
    let from_variants: HashSet<String> =
        transitions.iter().map(|t| t.from.to_lowercase()).collect();

    let mut initial: Vec<StateKey> = from_variants
        .difference(&to_variants)
        .filter_map(|v| variant_to_sk.get(v).copied())
        .collect();
    initial.sort_by_key(|sk| model.states[*sk].id.clone());

    let mut terminal: Vec<StateKey> = to_variants
        .difference(&from_variants)
        .filter_map(|v| variant_to_sk.get(v).copied())
        .collect();
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
    status_column: &str,
    state_map: &HashMap<StateKey, String>,
) -> Vec<StateKey> {
    let entity = &model.entities[ek];
    let variant_to_key: HashMap<String, StateKey> = state_map
        .iter()
        .map(|(sk, variant)| (variant.to_lowercase(), *sk))
        .collect();

    let mut ordered = Vec::new();
    if let Some(col) = entity.columns.iter().find(|c| c.name == status_column) {
        if let ColumnType::Enum(variants) = &col.col_type {
            for variant in variants {
                if let Some(sk) = variant_to_key.get(&variant.to_lowercase()) {
                    ordered.push(*sk);
                }
            }
            return ordered;
        }
    }
    // Fallback: any Enum column order (legacy global-state path).
    for col in &entity.columns {
        if let ColumnType::Enum(variants) = &col.col_type {
            for variant in variants {
                if let Some(sk) = variant_to_key.get(&variant.to_lowercase()) {
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
        assert!(
            diags.iter().all(|d| d.is_warning),
            "unexpected errors: {diags:?}"
        );
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
transitions(Order.status, EvCapture, pending -> paid)
transitions(Order.status, EvShip,    paid -> shipped)
transitions(Order.status, EvDeliver, shipped -> delivered)
transitions(Order.status, EvCancel,  pending -> cancelled)
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
            vec![
                "Order_pending",
                "Order_paid",
                "Order_shipped",
                "Order_delivered",
                "Order_cancelled"
            ]
        );

        let initial_ids: Vec<_> = lc
            .initial
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(initial_ids, vec!["Order_pending"]);

        let terminal_ids: Vec<_> = lc
            .terminal
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(terminal_ids, vec!["Order_cancelled", "Order_delivered"]);
    }

    #[test]
    fn pascal_case_enum_variants_still_compute_initial_terminal() {
        let model = model_from(
            r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(Draft, Published) @default(Draft)
}
event Publish "公開"
transitions(Doc.status, Publish, Draft -> Published)
"#,
        );
        let lifecycles = collect_entity_lifecycles(&model);
        assert_eq!(lifecycles.len(), 1);
        let lc = &lifecycles[0];
        let initial_ids: Vec<_> = lc
            .initial
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        let terminal_ids: Vec<_> = lc
            .terminal
            .iter()
            .map(|sk| model.states[*sk].id.as_str())
            .collect();
        assert_eq!(initial_ids, vec!["Doc_Draft"]);
        assert_eq!(terminal_ids, vec!["Doc_Published"]);
    }

    #[test]
    fn prefers_status_column_when_multiple_transition_columns_exist() {
        let model = model_from(
            r#"
entity Ticket "チケット" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  phase: Enum(intake, work) @default(intake)
}
event Close "閉じる"
event StartWork "作業開始"
transitions(Ticket.phase, StartWork, intake -> work)
transitions(Ticket.status, Close, open -> closed)
"#,
        );
        let lc = &collect_entity_lifecycles(&model)[0];
        assert_eq!(lc.status_column, "status");
        assert!(model
            .state_transitions
            .iter()
            .any(|st| st.column == "phase"));
        assert_eq!(lc.transitions.len(), 1);
        assert_eq!(lc.transitions[0].column, "status");
    }

    #[test]
    fn entity_without_status_enum_is_omitted() {
        let model = model_from(
            r#"
entity Item "Item" { id: Int @pk }
event EvX "X"
"#,
        );
        assert!(collect_entity_lifecycles(&model).is_empty());
    }
}
