//! The value of a written row's `@derived` column, computed from what the function writes to
//! the columns its expression reads: a literal, a column of an argument row, or a value the
//! state type fixes. A column whose value the function cannot compute that way stays an
//! input, and the function's doc says why.

use rdra_ish_core::model::{
    CheckExpr, CmpOpModel, CmpRhs, ComparisonProp, DerivedExpr, EffectValue, EntityKey, ModelColumn,
};

use super::*;

/// What the function writes to one column the expression reads.
enum Operand<'a> {
    Int(i64),
    Bool(bool),
    /// An enum value, as the model spells it.
    Enum(&'a str),
    Null,
    /// The column of an argument row, copied unchanged.
    Field(usize, &'a Field),
    /// A value the caller passes: present, and nothing more is known of it.
    Passed(&'a str),
}

/// What the expression reads of the row: its columns, the fields the function writes, and
/// the tag and always-null columns of the state it ends in.
pub(super) struct RowValues<'a> {
    pub(super) model: &'a SemanticModel,
    pub(super) entity: EntityKey,
    pub(super) columns: &'a [ModelColumn],
    pub(super) fields: &'a [(Field, Assigned)],
    pub(super) tag: Option<&'a (String, EnumValue)>,
    pub(super) null: &'a [Field],
}

/// The value of `column` (a `@derived` one), or why the function cannot compute it.
pub(super) fn derived_value(
    values: &RowValues<'_>,
    column: &ModelColumn,
) -> Result<Assigned, String> {
    let expr = match &column.derived_expr {
        Some(DerivedExpr::Logical(expr)) => expr,
        Some(DerivedExpr::Arith(_)) => {
            return Err("an arithmetic @derived expression is not printed yet".to_string())
        }
        None => return Err("the column is not derived".to_string()),
    };
    let target = values
        .fields
        .iter()
        .find(|(f, _)| f.wire == column.name)
        .map(|(f, _)| f)
        .ok_or_else(|| "the column has no field".to_string())?;
    if target.ty != FieldType::Scalar(Scalar::Bool) {
        return Err("the column is not a Bool".to_string());
    }
    Ok(match simplify(values.check(expr)?) {
        Expr::True => Assigned::Lit {
            lit: Lit::Bool(true),
            wrap: target.nullable,
        },
        Expr::False => Assigned::Lit {
            lit: Lit::Bool(false),
            wrap: target.nullable,
        },
        expr => Assigned::Derived {
            expr,
            wrap: target.nullable,
        },
    })
}

impl RowValues<'_> {
    fn check(&self, expr: &CheckExpr) -> Result<Expr, String> {
        Ok(match expr {
            CheckExpr::Cmp(prop) => self.cmp(prop)?,
            CheckExpr::Not(inner) => Expr::Not(Box::new(self.check(inner)?)),
            CheckExpr::And(a, b) => Expr::And(vec![self.check(a)?, self.check(b)?]),
            CheckExpr::Or(a, b) => Expr::Or(vec![self.check(a)?, self.check(b)?]),
        })
    }

    fn column(&self, name: &str) -> Result<&ModelColumn, String> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("it reads {name}, which the entity does not declare"))
    }

    /// What the function writes to `column`.
    fn operand(&self, column: &str) -> Result<Operand<'_>, String> {
        if let Some((tag, value)) = self.tag {
            if tag == column {
                return Ok(Operand::Enum(&value.wire));
            }
        }
        if self.null.iter().any(|f| f.wire == column) {
            return Ok(Operand::Null);
        }
        if self.column(column)?.derived_expr.is_some() {
            return Err(format!("it reads {column}, another derived column"));
        }
        let (written, assigned) = self
            .fields
            .iter()
            .find(|(f, _)| f.wire == column)
            .ok_or_else(|| format!("it reads {column}, which the row type holds no field for"))?;
        Ok(match assigned {
            Assigned::Lit { lit, .. } => match lit {
                Lit::Int(n) => Operand::Int(*n),
                Lit::Bool(b) => Operand::Bool(*b),
                Lit::Enum { value, .. } => Operand::Enum(&value.wire),
            },
            Assigned::Null
            | Assigned::From {
                adapt: Adapt::AbsentNull,
                ..
            } => Operand::Null,
            Assigned::From {
                row,
                field,
                adapt: Adapt::Same | Adapt::Wrap,
            } => Operand::Field(*row, field),
            // A value the caller passes is present when it is wrapped or the column is never
            // null; a nullable input may be null, which no comparison here can read.
            Assigned::Input { wrap, .. } if *wrap || !written.nullable => {
                Operand::Passed(&written.wire)
            }
            Assigned::Input { .. } => {
                return Err(format!(
                    "it reads {column}, which the caller passes and may be null"
                ))
            }
            _ => return Err(format!("it reads {column}, which the function computes")),
        })
    }

    /// One comparison, `lhs op rhs`, on what the function writes to `lhs`.
    fn cmp(&self, prop: &ComparisonProp) -> Result<Expr, String> {
        // A comparison a `drives(..)` sets is a proposition of its own to the lifecycle, not
        // the comparison of the values.
        let key = prop.axis_key();
        if self
            .model
            .proposition_effects
            .iter()
            .any(|pe| pe.entity == self.entity && pe.prop.axis_key() == key)
        {
            return Err(format!("a drives(..) sets {}", prop.display()));
        }
        let column = self.column(&prop.lhs_column)?;
        let rhs: Vec<&EffectValue> = match &prop.rhs {
            CmpRhs::Value(v) => vec![v],
            CmpRhs::Values(vs) => vs.iter().collect(),
            CmpRhs::IntLit(_) => Vec::new(),
            CmpRhs::Column(_) => {
                return Err(format!("it compares two columns ({})", prop.display()))
            }
            CmpRhs::Now => return Err("it reads now".to_string()),
        };
        let null_test = rhs
            .iter()
            .all(|v| matches!(v, EffectValue::Null | EffectValue::Present))
            && !rhs.is_empty();
        // A null operand leaves any other comparison neither true nor false, which a Bool
        // field cannot hold: only a column that is never null is compared here.
        if column.is_nullable && !null_test {
            return Err(format!(
                "it compares {}, which can be null",
                prop.lhs_column
            ));
        }
        let operand = self.operand(&prop.lhs_column)?;
        let eq = |v: &EffectValue| self.eq(&operand, column, v);
        Ok(match (&prop.op, &prop.rhs) {
            (op, CmpRhs::IntLit(n)) | (op, CmpRhs::Value(EffectValue::Int(n))) => {
                self.int_cmp(&operand, *op, *n)?
            }
            (CmpOpModel::Eq | CmpOpModel::In, _) => {
                Expr::Or(rhs.iter().map(|v| eq(v)).collect::<Result<Vec<_>, _>>()?)
            }
            (CmpOpModel::Ne | CmpOpModel::NotIn, _) => Expr::And(
                rhs.iter()
                    .map(|v| eq(v).map(|e| Expr::Not(Box::new(e))))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            _ => {
                return Err(format!(
                    "it orders a column that is not an Int ({})",
                    prop.display()
                ))
            }
        })
    }

    fn int_cmp(&self, operand: &Operand<'_>, op: CmpOpModel, n: i64) -> Result<Expr, String> {
        let ir_op = match op {
            CmpOpModel::Eq => CmpOp::Eq,
            CmpOpModel::Ne => CmpOp::Ne,
            CmpOpModel::Lt => CmpOp::Lt,
            CmpOpModel::Le => CmpOp::Le,
            CmpOpModel::Gt => CmpOp::Gt,
            CmpOpModel::Ge => CmpOp::Ge,
            CmpOpModel::In | CmpOpModel::NotIn => {
                return Err("it compares an Int with in".to_string())
            }
        };
        Ok(match operand {
            Operand::Int(m) => {
                let holds = match ir_op {
                    CmpOp::Eq => *m == n,
                    CmpOp::Ne => *m != n,
                    CmpOp::Lt => *m < n,
                    CmpOp::Le => *m <= n,
                    CmpOp::Gt => *m > n,
                    CmpOp::Ge => *m >= n,
                };
                if holds {
                    Expr::True
                } else {
                    Expr::False
                }
            }
            Operand::Field(row, field)
                if field.ty == FieldType::Scalar(Scalar::Int) && !field.nullable =>
            {
                Expr::IntCmp {
                    row: *row,
                    field: (*field).clone(),
                    op: ir_op,
                    value: n,
                }
            }
            Operand::Passed(column) => {
                return Err(format!("it reads {column}, whose value the caller passes"))
            }
            _ => return Err("it compares a column that is not a plain Int".to_string()),
        })
    }

    /// `operand == value`.
    fn eq(
        &self,
        operand: &Operand<'_>,
        column: &ModelColumn,
        value: &EffectValue,
    ) -> Result<Expr, String> {
        let truth = |b: bool| if b { Expr::True } else { Expr::False };
        Ok(match (operand, value) {
            (Operand::Null, EffectValue::Null) => Expr::True,
            (Operand::Null, EffectValue::Present) => Expr::False,
            (Operand::Field(row, field), EffectValue::Null | EffectValue::Present) => {
                let is_null = if field.nullable {
                    Expr::IsNull {
                        row: *row,
                        field: (*field).clone(),
                    }
                } else {
                    Expr::False
                };
                if matches!(value, EffectValue::Null) {
                    is_null
                } else {
                    Expr::Not(Box::new(is_null))
                }
            }
            (_, EffectValue::Null) => Expr::False,
            (_, EffectValue::Present) => Expr::True,
            (Operand::Passed(column), _) => {
                return Err(format!("it reads {column}, whose value the caller passes"))
            }
            (Operand::Int(m), EffectValue::Int(n)) => truth(m == n),
            (Operand::Bool(a), EffectValue::Bool(b)) => truth(a == b),
            (Operand::Enum(a), EffectValue::EnumVariant(b)) => {
                truth(self.spelled(column, b).as_deref() == Some(*a))
            }
            (Operand::Field(row, field), EffectValue::Bool(b))
                if field.ty == FieldType::Scalar(Scalar::Bool) && !field.nullable =>
            {
                Expr::BoolEq {
                    row: *row,
                    field: (*field).clone(),
                    value: *b,
                }
            }
            (Operand::Field(row, field), EffectValue::EnumVariant(b)) if !field.nullable => {
                let FieldType::Enum(enum_ty) = &field.ty else {
                    return Err(format!(
                        "it compares {} with a value of another type",
                        field.wire
                    ));
                };
                let Some(wire) = self.spelled(column, b) else {
                    return Ok(Expr::False);
                };
                Expr::EnumEq {
                    row: *row,
                    field: (*field).clone(),
                    enum_ty: enum_ty.clone(),
                    value: EnumValue {
                        ident: type_ident(&wire),
                        wire,
                        label: None,
                    },
                }
            }
            (Operand::Field(row, field), EffectValue::Int(n)) => {
                self.int_cmp(&Operand::Field(*row, field), CmpOpModel::Eq, *n)?
            }
            _ => {
                return Err(format!(
                    "it compares {} with a value of another type",
                    column.name
                ))
            }
        })
    }

    /// `value` as the column declares it: the exact spelling, else one that differs in case
    /// only, as the lifecycle reads it.
    fn spelled(&self, column: &ModelColumn, value: &str) -> Option<String> {
        let declared = self.model.column_values(column)?;
        declared
            .iter()
            .find(|v| **v == value)
            .or_else(|| declared.iter().find(|v| v.eq_ignore_ascii_case(value)))
            .map(|v| v.to_string())
    }
}
