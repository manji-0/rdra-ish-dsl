//! `sets(...)` effect value parsing.

use crate::diagnostics::RdraError;
use crate::model::{ColumnType, EffectValue, ModelColumn};

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

pub(crate) fn parse_effect_value(col: &ModelColumn, lit: &str) -> Result<EffectValue, RdraError> {
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

#[cfg(test)]
mod tests {
    use crate::diagnostics::RdraError;
    use crate::model::{ColumnType, EffectValue, ModelColumn};

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
            is_fk: false,
            fk_target: None,
            fk_optional: false,
            fk_on_delete: None,
            fk_on_update: None,
            check_constraints: Vec::new(),
            is_soft_delete: false,
            is_history: false,
            is_tenant_scope: false,
            derived_expr: None,
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
            parse_effect_value(&nullable, "null").unwrap(),
            EffectValue::Null
        );
        assert_eq!(
            parse_effect_value(&nullable, "present").unwrap(),
            EffectValue::Present
        );
        assert!(matches!(
            parse_effect_value(&nullable, "jsonb"),
            Err(RdraError::PgTypeNameInSets { .. })
        ));
        assert_eq!(
            parse_effect_value(&enum_col, "closed").unwrap(),
            EffectValue::EnumVariant("closed".to_string())
        );
        assert_eq!(
            parse_effect_value(&bool_col, "true").unwrap(),
            EffectValue::Bool(true)
        );
        assert_eq!(
            parse_effect_value(&bool_col, "false").unwrap(),
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
            parse_effect_value(&string_col, "null"),
            Err(RdraError::NullOnNonNullable { .. })
        ));
        assert!(matches!(
            parse_effect_value(&enum_col, "closed"),
            Err(RdraError::InvalidEnumVariant { .. })
        ));
        assert!(matches!(
            parse_effect_value(&bool_col, "yes"),
            Err(RdraError::InvalidBoolValue { .. })
        ));
        assert_eq!(
            parse_effect_value(&int_col, "42").unwrap(),
            EffectValue::Int(42)
        );
        assert!(matches!(
            parse_effect_value(&int_col, "nope"),
            Err(RdraError::EffectOnNonStateColumn { .. })
        ));
    }
}
