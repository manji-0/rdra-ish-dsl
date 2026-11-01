//! `sets(...)` effect value parsing.

use crate::analysis_diag::{push_error, push_error_cmp};
use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::DiagCtxt;
use crate::model::{
    ColumnType, EffectValue, EntityKey, ModelColumn, QualifiedModelColumnRef, SemanticModel,
    SetValue, Variation, VariationKey,
};
use rdra_ish_syntax::ast::{CmpOp, Comparison, Operand};
use slotmap::SlotMap;

use super::comparison::type_category;
use super::constraint::{find_entity_column, resolve_entity_qref};
use super::qref_util::qref_id;

fn is_pg_special_type(s: &str) -> bool {
    matches!(
        s,
        "json"
            | "jsonb"
            | "uuid"
            | "timestamptz"
            | "timestamp"
            | "timetz"
            | "time"
            | "date"
            | "interval"
            | "inet"
            | "cidr"
            | "macaddr"
            | "macaddr8"
            | "bytea"
            | "tsvector"
            | "tsquery"
            | "xml"
            | "point"
            | "line"
            | "lseg"
            | "box"
            | "path"
            | "polygon"
            | "circle"
            | "money"
            | "bit"
            | "varbit"
            | "int4range"
            | "int8range"
            | "numrange"
            | "tsrange"
            | "tstzrange"
            | "daterange"
    )
}

pub(crate) fn parse_effect_value(
    variations: &SlotMap<VariationKey, Variation>,
    col: &ModelColumn,
    lit: &str,
) -> Result<EffectValue, RdraError> {
    match lit {
        "null" => {
            return if col.is_nullable {
                Ok(EffectValue::Null)
            } else {
                Err(RdraError::NullOnNonNullable {
                    col: col.name.clone(),
                })
            };
        }
        "present" => {
            return if col.is_nullable {
                Ok(EffectValue::Present)
            } else {
                Err(RdraError::NullOnNonNullable {
                    col: col.name.clone(),
                })
            };
        }
        _ => {}
    }

    if is_pg_special_type(lit) {
        return Err(RdraError::PgTypeNameInSets {
            name: lit.to_string(),
        });
    }

    match &col.col_type {
        ColumnType::Enum(variants) => {
            if variants.iter().any(|v| v == lit) {
                Ok(EffectValue::EnumVariant(lit.to_string()))
            } else {
                Err(RdraError::InvalidEnumVariant {
                    col: col.name.clone(),
                    value: lit.to_string(),
                    allowed: variants.join(", "),
                })
            }
        }
        ColumnType::Variation(key) => {
            let variation = variations.get(*key);
            let values = variation.map(|v| v.values.as_slice()).unwrap_or(&[]);
            if values.iter().any(|v| v == lit) {
                Ok(EffectValue::EnumVariant(lit.to_string()))
            } else {
                Err(RdraError::InvalidVariationValue {
                    variation: variation.map(|v| v.id.clone()).unwrap_or_default(),
                    value: lit.to_string(),
                    allowed: values.join(", "),
                })
            }
        }
        ColumnType::Bool => match lit {
            "true" => Ok(EffectValue::Bool(true)),
            "false" => Ok(EffectValue::Bool(false)),
            _ => Err(RdraError::InvalidBoolValue {
                col: col.name.clone(),
                value: lit.to_string(),
            }),
        },
        ColumnType::Int | ColumnType::Money | ColumnType::Decimal => lit
            .parse::<i64>()
            .map(EffectValue::Int)
            .map_err(|_| RdraError::EffectOnNonStateColumn {
                col: col.name.clone(),
                col_type: format!("{:?} (expected integer literal)", col.col_type),
            }),
        _ => Err(RdraError::EffectOnNonStateColumn {
            col: col.name.clone(),
            col_type: format!("{:?}", col.col_type),
        }),
    }
}

/// `sets(UC, E, column == F.source)`: the target column of `E` and the column it copies, or
/// `None` when the comparison is no copy (another operator, or a right-hand side that is not
/// `Entity.column`). `Err` after reporting an unknown column or entity, or a source whose
/// values the target cannot hold.
pub(crate) fn resolve_copy(
    model: &SemanticModel,
    entity: EntityKey,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<(String, QualifiedModelColumnRef)>, ()> {
    let Operand::QualifiedColumn(source) = &cmp.rhs else {
        return Ok(None);
    };
    if cmp.op != CmpOp::Eq {
        return Ok(None);
    }
    let entity_id = &model.entities[entity].id;
    let column = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        Operand::QualifiedColumn(q) if qref_id(&q.entity).as_ref() == Some(entity_id) => {
            q.column.clone()
        }
        _ => return Ok(None),
    };
    let target = find_entity_column(model, entity, &column, cmp.span.clone(), ctx, diags);
    let source_entity = resolve_entity_qref(model, "sets", &source.entity, ctx, diags);
    let (Some(target), Some(source_entity)) = (target, source_entity) else {
        return Err(());
    };
    let Some(source_col) = find_entity_column(
        model,
        source_entity,
        &source.column,
        source.span.clone(),
        ctx,
        diags,
    ) else {
        return Err(());
    };
    if !holds_state(&target) {
        let err = RdraError::EffectOnNonStateColumn {
            col: target.name.clone(),
            col_type: format!("{:?}", target.col_type),
        };
        push_error(ctx, diags, cmp.span.clone(), err);
        return Err(());
    }
    if let Err(reason) = copyable(&target, &source_col) {
        let source_id = &model.entities[source_entity].id;
        let err = RdraError::SetsCopyMismatch {
            target: format!(
                "{entity_id}.{} ({})",
                target.name,
                type_text(model, &target)
            ),
            copied: format!(
                "{source_id}.{} ({})",
                source_col.name,
                type_text(model, &source_col)
            ),
            reason: reason.to_string(),
        };
        push_error_cmp(ctx, diags, cmp, err);
        return Err(());
    }
    Ok(Some((
        column,
        QualifiedModelColumnRef {
            entity: source_entity,
            column: source.column.clone(),
        },
    )))
}

/// After the foreign keys resolve: a copy from or into a relate's foreign key column, which
/// is the relate's owner rather than a state column, is `EffectOnNonStateColumn`.
pub(super) fn reject_key_copies(model: &SemanticModel, diags: &mut Vec<Diagnostic>) {
    for ce in &model.column_effects {
        let SetValue::Copy(source) = &ce.value else {
            continue;
        };
        let key = [(ce.entity, &ce.column), (source.entity, &source.column)]
            .into_iter()
            .find(|(ek, column)| model.column_fk(*ek, column).is_some());
        let Some((_, column)) = key else {
            continue;
        };
        let err = RdraError::EffectOnNonStateColumn {
            col: column.clone(),
            col_type: "a relate's foreign key".to_string(),
        };
        diags.push(match &ce.site {
            Some(site) => Diagnostic::error_at(err, site.clone()),
            None => Diagnostic::error(err),
        });
    }
}

/// A column's type as the DSL writes it, `@null` included.
fn type_text(model: &SemanticModel, col: &ModelColumn) -> String {
    let base = match &col.col_type {
        ColumnType::Enum(values) => format!("Enum({})", values.join(", ")),
        ColumnType::Variation(key) => model
            .variations
            .get(*key)
            .map_or_else(|| "variation".to_string(), |v| v.id.clone()),
        other => format!("{other:?}"),
    };
    if col.is_nullable {
        format!("{base} @null")
    } else {
        base
    }
}

/// A column a `sets` may write: one with a finite set of values, or a nullable one.
fn holds_state(col: &ModelColumn) -> bool {
    col.is_nullable || !matches!(col.col_type, ColumnType::String)
}

/// Whether every value `source` may hold is a value of `target`: one Enum's variants, one
/// variation, Bool, two Int-typed (Int, Money, Decimal) or two Date-typed (Date, DateTime)
/// columns, or two Strings; a nullable source needs a nullable target.
fn copyable(target: &ModelColumn, source: &ModelColumn) -> Result<(), &'static str> {
    if source.derived_expr.is_some() {
        return Err("have a @derived source, which no step holds as a value");
    }
    let same = match (&target.col_type, &source.col_type) {
        (ColumnType::Enum(a), ColumnType::Enum(b)) => a == b,
        (ColumnType::Variation(a), ColumnType::Variation(b)) => a == b,
        (ColumnType::Bool, ColumnType::Bool) | (ColumnType::String, ColumnType::String) => true,
        (a, b) => type_category(a) == type_category(b) && type_category(a) != "equality",
    };
    if !same {
        return Err("hold different sets of values");
    }
    if source.is_nullable && !target.is_nullable {
        return Err("differ in nullability: the source may be null and the target may not");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::RdraError;
    use crate::model::{ColumnType, EffectValue, ModelColumn};
    use slotmap::SlotMap;

    use super::parse_effect_value;

    fn model_column(name: &str, col_type: ColumnType) -> ModelColumn {
        ModelColumn {
            name: name.to_string(),
            col_type,
            is_pk: false,
            is_unique: false,
            is_indexed: false,
            is_nullable: false,
            default_val: None,
            label: None,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
            value_labels: Vec::new(),
        }
    }

    #[test]
    fn parse_effect_value_maps_nullable_enum_and_bool_values() {
        let mut nullable = model_column("metadata", ColumnType::String);
        nullable.is_nullable = true;
        let enum_col = model_column(
            "status",
            ColumnType::Enum(vec!["open".to_string(), "closed".to_string()]),
        );
        let bool_col = model_column("active", ColumnType::Bool);

        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &nullable, "null").unwrap(),
            EffectValue::Null
        );
        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &nullable, "present").unwrap(),
            EffectValue::Present
        );
        assert!(matches!(
            parse_effect_value(&SlotMap::with_key(), &nullable, "jsonb"),
            Err(RdraError::PgTypeNameInSets { .. })
        ));
        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &enum_col, "closed").unwrap(),
            EffectValue::EnumVariant("closed".to_string())
        );
        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &bool_col, "true").unwrap(),
            EffectValue::Bool(true)
        );
        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &bool_col, "false").unwrap(),
            EffectValue::Bool(false)
        );
    }

    #[test]
    fn parse_effect_value_rejects_invalid_state_effects() {
        let string_col = model_column("name", ColumnType::String);
        let enum_col = model_column("status", ColumnType::Enum(vec!["open".to_string()]));
        let bool_col = model_column("active", ColumnType::Bool);
        let int_col = model_column("count", ColumnType::Int);

        assert!(matches!(
            parse_effect_value(&SlotMap::with_key(), &string_col, "null"),
            Err(RdraError::NullOnNonNullable { .. })
        ));
        assert!(matches!(
            parse_effect_value(&SlotMap::with_key(), &enum_col, "closed"),
            Err(RdraError::InvalidEnumVariant { .. })
        ));
        assert!(matches!(
            parse_effect_value(&SlotMap::with_key(), &bool_col, "yes"),
            Err(RdraError::InvalidBoolValue { .. })
        ));
        assert_eq!(
            parse_effect_value(&SlotMap::with_key(), &int_col, "42").unwrap(),
            EffectValue::Int(42)
        );
        assert!(matches!(
            parse_effect_value(&SlotMap::with_key(), &int_col, "nope"),
            Err(RdraError::EffectOnNonStateColumn { .. })
        ));
    }
}
