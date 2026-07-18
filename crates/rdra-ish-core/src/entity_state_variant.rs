//! 導出済み状態パターンを discriminated union 形式に変換する API。

use crate::entity_lifecycle::link_entity_status_states;
use crate::model::{EntityKey, SemanticModel};
use crate::state_pattern::{
    derive_state_patterns, AbstractValue, AxisKind, EntityStateResult, ReachablePattern, StateAxis,
};
use std::collections::BTreeMap;

/// 状態バリアント内のフィールド値（status 以外の軸）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StateFieldValue {
    Enum(String),
    Bool(bool),
    Null,
    Present { pg_type: Option<String> },
}

impl StateFieldValue {
    pub fn from_abstract(val: &AbstractValue, pg_type: Option<&str>) -> Self {
        match val {
            AbstractValue::Enum(s) => StateFieldValue::Enum(s.clone()),
            AbstractValue::Bool(b) => StateFieldValue::Bool(*b),
            AbstractValue::Null => StateFieldValue::Null,
            AbstractValue::Present => StateFieldValue::Present {
                pg_type: pg_type.map(str::to_string),
            },
        }
    }
}

/// 単一の到達可能状態（discriminator + 付随フィールド）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityStateVariant {
    /// 識別子（通常は status Enum バリアント）
    pub discriminator: String,
    /// discriminator 以外の状態軸
    pub fields: BTreeMap<String, StateFieldValue>,
    pub is_initial: bool,
    pub is_terminal: bool,
}

/// entity 単位の状態バリアント集合。
#[derive(Debug, Clone)]
pub struct EntityStateVariants {
    pub entity: EntityKey,
    pub entity_id: String,
    pub discriminator_column: String,
    pub variants: Vec<EntityStateVariant>,
}

/// 全 entity の到達可能状態を discriminated union 形式で収集する。
pub fn collect_entity_state_variants(
    model: &SemanticModel,
    buc_filter: &[String],
    cap: usize,
) -> Vec<EntityStateVariants> {
    derive_state_patterns(model, buc_filter, cap)
        .into_iter()
        .filter_map(|result| entity_state_variants_for_result(model, result))
        .collect()
}

fn entity_state_variants_for_result(
    model: &SemanticModel,
    result: EntityStateResult,
) -> Option<EntityStateVariants> {
    let entity = model
        .entities
        .iter()
        .find(|(_, e)| e.id == result.entity_id)
        .map(|(k, _)| k)?;

    let discriminator_column = link_entity_status_states(model, entity)
        .map(|(col, _)| col)
        .or_else(|| first_enum_axis_column(&result.axes))?;

    let variants = result
        .patterns
        .into_iter()
        .map(|reachable| to_entity_state_variant(&result.axes, &discriminator_column, reachable))
        .collect();

    Some(EntityStateVariants {
        entity,
        entity_id: result.entity_id,
        discriminator_column,
        variants,
    })
}

fn first_enum_axis_column(axes: &[StateAxis]) -> Option<String> {
    axes.iter()
        .find(|ax| matches!(ax.kind, AxisKind::Enum(_)))
        .map(|ax| ax.column.clone())
}

fn to_entity_state_variant(
    axes: &[StateAxis],
    discriminator_column: &str,
    reachable: ReachablePattern,
) -> EntityStateVariant {
    let discriminator = reachable
        .pattern
        .values
        .get(discriminator_column)
        .map(|val| match val {
            AbstractValue::Enum(s) => s.clone(),
            other => other.display_with_type(None),
        })
        .unwrap_or_else(|| "_".to_string());

    let mut fields = BTreeMap::new();
    for (column, value) in &reachable.pattern.values {
        if column == discriminator_column {
            continue;
        }
        let pg_type = axes.iter().find_map(|ax| {
            if ax.column == *column {
                if let AxisKind::Nullable { pg_type } = &ax.kind {
                    return pg_type.as_deref();
                }
            }
            None
        });
        fields.insert(
            column.clone(),
            StateFieldValue::from_abstract(value, pg_type),
        );
    }

    EntityStateVariant {
        discriminator,
        fields,
        is_initial: reachable.is_initial,
        is_terminal: reachable.is_terminal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use crate::state_pattern::DEFAULT_PATTERN_CAP;
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
usecase PlaceOrder  "注文確定"
usecase CapturePay  "決済確定"
usecase ShipOrder   "発送"
usecase DeliverOrder "配達確認"
usecase CancelOrder "注文キャンセル"
event EvCapture  "決済確定"
event EvShip     "発送開始"
event EvDeliver  "配達確認"
event EvCancel   "注文キャンセル"
creates(PlaceOrder, Order)
updates(CapturePay, Order)
updates(ShipOrder, Order)
updates(DeliverOrder, Order)
updates(CancelOrder, Order)
raises(CapturePay, EvCapture)
raises(ShipOrder, EvShip)
raises(DeliverOrder, EvDeliver)
raises(CancelOrder, EvCancel)
state Pending   "注文受付"
state Paid      "決済完了"
state Shipped   "発送済"
state Delivered "配達完了"
state Cancelled "キャンセル"
transitions(Order.status, EvCapture, pending -> paid)
transitions(Order.status, EvShip, paid -> shipped)
transitions(Order.status, EvDeliver, shipped -> delivered)
transitions(Order.status, EvCancel, pending -> cancelled)
sets(usecase::DeliverOrder, Order, delivered_at == present)
"#;

    #[test]
    fn collect_entity_state_variants_order() {
        let model = model_from(ORDER_SRC);
        let collections = collect_entity_state_variants(&model, &[], DEFAULT_PATTERN_CAP);
        assert_eq!(collections.len(), 1);

        let order = &collections[0];
        assert_eq!(order.entity_id, "Order");
        assert_eq!(order.discriminator_column, "status");
        assert_eq!(order.variants.len(), 5);

        let pending = order
            .variants
            .iter()
            .find(|v| v.discriminator == "pending")
            .expect("pending");
        assert!(pending.is_initial);
        assert!(!pending.is_terminal);
        assert_eq!(
            pending.fields.get("delivered_at"),
            Some(&StateFieldValue::Null)
        );

        let delivered = order
            .variants
            .iter()
            .find(|v| v.discriminator == "delivered")
            .expect("delivered");
        assert!(delivered.is_terminal);
        assert_eq!(
            delivered.fields.get("delivered_at"),
            Some(&StateFieldValue::Present { pg_type: None })
        );
    }
}
