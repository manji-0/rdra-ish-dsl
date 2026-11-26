//! A transition-system predicate as a codegen expression, and the simplifier over it.

use super::*;

/// Where a slot reference of a predicate points: a row, and its state when it has one.
pub(in crate::codegen) type Bind<'a, 'd> =
    &'a dyn Fn(At) -> Option<(usize, Option<&'d VariantDecl>)>;

/// The flat row type and fields of an entity, by its id.
pub(in crate::codegen) type RowTypeOf<'a> = &'a dyn Fn(&str) -> Option<(String, Vec<Field>)>;

/// What a quantifier (`.has` / `.none`) needs: the rows it binds and the lists they come from.
pub(in crate::codegen) struct Quant<'a> {
    pub model: &'a SemanticModel,
    /// The flat row type and fields of an entity.
    pub row_type: RowTypeOf<'a>,
    /// The input a written row's column was chosen into, by the column's var: a read of the
    /// successor `fold_next` leaves reads it.
    pub next_input: &'a dyn Fn(VarId) -> Option<(usize, FnInput)>,
    /// The `Bound` index of the first quantifier's row.
    pub base: usize,
    /// The function's rows: a quantifier's row is numbered after them.
    pub n_rows: usize,
    pub elems: Vec<RowParam>,
    /// The row each enclosing quantifier binds, outermost first.
    pub stack: Vec<usize>,
    /// `(input name, row type)` per list a quantifier ranges over.
    pub lists: Vec<(String, String)>,
}

/// A predicate on the rows `bind` names. `quant` lets it range over related rows.
pub(in crate::codegen) fn expr(
    ts: &TransitionSystem,
    pred: &Pred,
    rows: &[RowParam],
    bind: Bind<'_, '_>,
    quant: &mut Option<&mut Quant<'_>>,
) -> Result<Expr, String> {
    Ok(match pred {
        Pred::True => Expr::True,
        Pred::False => Expr::False,
        Pred::Not(p) => Expr::Not(Box::new(expr(ts, p, rows, bind, quant)?)),
        Pred::And(ps) => {
            let mut out = Vec::new();
            for p in ps {
                out.push(expr(ts, p, rows, bind, quant)?);
            }
            Expr::And(out)
        }
        Pred::Or(ps) => {
            let mut out = Vec::new();
            for p in ps {
                out.push(expr(ts, p, rows, bind, quant)?);
            }
            Expr::Or(out)
        }
        Pred::In(term, values) => {
            let mut out = Vec::new();
            for v in values {
                out.push(atom(ts, term, TsCmp::Eq, v, rows, bind, quant)?);
            }
            Expr::Or(out)
        }
        Pred::Cmp(a, op, b) => atom(ts, a, *op, b, rows, bind, quant)?,
        Pred::Forall { entity, body }
        | Pred::Exists { entity, body }
        | Pred::Count { entity, body, .. } => {
            let Some(q) = quant.as_deref_mut() else {
                return Err(
                    "a condition over related rows, `.has`, `.none` or a count (phase 5)".into(),
                );
            };
            let id = ts.entity(*entity).id.clone();
            let (ty, fields) = (q.row_type)(&id)
                .ok_or_else(|| format!("related rows of {id}, which has no generated type"))?;
            let list = format!("{}_rows", snake(&id));
            let input = match q.lists.iter().position(|(n, _)| *n == list) {
                Some(i) => i,
                None => {
                    q.lists.push((list, ty.clone()));
                    q.lists.len() - 1
                }
            };
            let k = q.elems.len();
            q.elems.push(RowParam {
                name: format!("{}_{k}", snake(&id)),
                param: None,
                state: None,
                entity: id,
                ty,
                fields,
                always_null: Vec::new(),
            });
            let elem = q.n_rows + k;
            q.stack.push(elem);
            let inner = expr(ts, body, rows, bind, quant);
            if let Some(q) = quant.as_deref_mut() {
                q.stack.pop();
            }
            let body = Box::new(inner?);
            match pred {
                Pred::Exists { .. } => Expr::Exists { input, elem, body },
                Pred::Count { op, n, .. } => Expr::Count {
                    input,
                    elem,
                    body,
                    op: ir_op(*op),
                    n: *n,
                },
                _ => Expr::Forall { input, elem, body },
            }
        }
    })
}

/// The row `at` names: a function row by `bind`, or the row of an enclosing quantifier.
pub(super) fn resolve<'d>(
    at: At,
    rows: &[RowParam],
    bind: Bind<'_, 'd>,
    quant: &Option<&mut Quant<'_>>,
) -> Option<(usize, Option<&'d VariantDecl>, RowParam)> {
    if let (At::Bound(k), Some(q)) = (at, quant.as_deref()) {
        let k = usize::from(k);
        if k >= q.base {
            let elem = *q.stack.get(k - q.base)?;
            return Some((elem, None, q.elems[elem - q.n_rows].clone()));
        }
    }
    let (r, state) = bind(at)?;
    Some((r, state, rows.get(r)?.clone()))
}

pub(super) fn atom(
    ts: &TransitionSystem,
    lhs: &Term,
    op: TsCmp,
    rhs: &Term,
    rows: &[RowParam],
    bind: Bind<'_, '_>,
    quant: &mut Option<&mut Quant<'_>>,
) -> Result<Expr, String> {
    let unsupported = || "a condition atom this phase does not print".to_string();
    // A key between two rows: the child's key names the parent.
    let key = match (lhs, op, rhs) {
        (Term::Slot { var, at }, TsCmp::Eq, Term::Index(parent))
        | (Term::Index(parent), TsCmp::Eq, Term::Slot { var, at }) => match &ts.var(*var).kind {
            VarKind::Owner { fk, .. } => Some((*fk, *at, *parent)),
            _ => None,
        },
        _ => None,
    };
    if let Some((fk, child_at, parent_at)) = key {
        let q = quant.as_deref().ok_or_else(unsupported)?;
        let fk = q.model.foreign_key(fk);
        let [(child_col, parent_col)] = fk.columns.as_slice() else {
            return Err("a key over several columns".into());
        };
        let quant_ref: &Option<&mut Quant<'_>> = quant;
        let (c, _, crow) = resolve(child_at, rows, bind, quant_ref).ok_or_else(unsupported)?;
        let (p, _, prow) = resolve(parent_at, rows, bind, quant_ref).ok_or_else(unsupported)?;
        let child_field = crow.fields.iter().find(|f| f.wire == *child_col);
        let parent_field = prow.fields.iter().find(|f| f.wire == *parent_col);
        let (Some(child_field), Some(parent_field)) = (child_field, parent_field) else {
            return Err("a key on a column with no field".into());
        };
        return Ok(Expr::KeyEq {
            child: c,
            child_field: child_field.clone(),
            parent: p,
            parent_field: parent_field.clone(),
        });
    }
    // The same row: two rows of one entity whose primary keys agree.
    if let (Term::Index(a), TsCmp::Eq, Term::Index(b)) = (lhs, op, rhs) {
        let q = quant.as_deref().ok_or_else(unsupported)?;
        let quant_ref: &Option<&mut Quant<'_>> = quant;
        let (ra, _, rowa) = resolve(*a, rows, bind, quant_ref).ok_or_else(unsupported)?;
        let (rb, _, rowb) = resolve(*b, rows, bind, quant_ref).ok_or_else(unsupported)?;
        let entity = q
            .model
            .entities
            .values()
            .find(|e| e.id == rowa.entity && rowa.entity == rowb.entity)
            .ok_or_else(unsupported)?;
        let [pk] = entity.primary_key.as_slice() else {
            return Err("a row over several key columns".into());
        };
        let field = |row: &RowParam| row.fields.iter().find(|f| f.wire == *pk).cloned();
        let (Some(fa), Some(fb)) = (field(&rowa), field(&rowb)) else {
            return Err("a key on a column with no field".into());
        };
        return Ok(Expr::KeyEq {
            child: ra,
            child_field: fa,
            parent: rb,
            parent_field: fb,
        });
    }
    let Term::Slot { var, at } = lhs else {
        return Err(unsupported());
    };
    // A key the step links or unlinks: `owner == null` / `owner != null`.
    if let (VarKind::Owner { fk, .. }, Term::Lit(v)) = (&ts.var(*var).kind, rhs) {
        if *v != Value::NULL {
            return Err(unsupported());
        }
        let q = quant.as_deref().ok_or_else(unsupported)?;
        let fk = q.model.foreign_key(*fk);
        let [(child_col, _)] = fk.columns.as_slice() else {
            return Err("a key over several columns".into());
        };
        let quant_ref: &Option<&mut Quant<'_>> = quant;
        let (r, _, row) = resolve(*at, rows, bind, quant_ref).ok_or_else(unsupported)?;
        let is_null = match row.fields.iter().find(|f| f.wire == *child_col) {
            Some(f) if f.nullable => Expr::IsNull {
                row: r,
                field: f.clone(),
            },
            Some(_) => Expr::False,
            None => return Err("a key on a column with no field".into()),
        };
        return match op {
            TsCmp::Eq => Ok(is_null),
            TsCmp::Ne => Ok(Expr::Not(Box::new(is_null))),
            _ => Err(unsupported()),
        };
    }
    let VarKind::Column { column, .. } = &ts.var(*var).kind else {
        return Err("a condition on a comparison proposition (phase 5)".into());
    };
    // A value the caller chooses for a written row: the condition reads the input.
    if let At::Next(_) = at {
        let (input, decl) = quant
            .as_deref()
            .and_then(|q| (q.next_input)(*var))
            .ok_or_else(unsupported)?;
        return input_atom(&ts.var(*var).domain, op, rhs, input, &decl);
    }
    let quant_ref: &Option<&mut Quant<'_>> = quant;
    let (r, state, row) = resolve(*at, rows, bind, quant_ref).ok_or_else(unsupported)?;
    let domain = &ts.var(*var).domain;
    let negate = |e: Expr| match op {
        TsCmp::Eq => Ok(e),
        TsCmp::Ne => Ok(Expr::Not(Box::new(e))),
        _ => Err(unsupported()),
    };
    match (domain, rhs) {
        (Domain::Enum(_) | Domain::NullableEnum(_), Term::Lit(v)) if domain.name(*v).is_some() => {
            let wire = &domain.name(*v).ok_or_else(unsupported)?.to_string();
            // A column the source state keeps null satisfies no comparison (#359).
            if row.always_null.contains(column) {
                return negate(Expr::False);
            }
            // The tag of a state type is fixed by the source state.
            if let Some(variant) = state {
                if !row.fields.iter().any(|f| f.wire == *column) {
                    let holds = variant.value.wire == *wire;
                    return negate(if holds { Expr::True } else { Expr::False });
                }
            }
            let field = row
                .fields
                .iter()
                .find(|f| f.wire == *column)
                .ok_or_else(unsupported)?;
            let FieldType::Enum(enum_ty) = &field.ty else {
                return Err(unsupported());
            };
            // On a nullable field the printers compare with a present value: null satisfies
            // no `==`, and so every `!=` (#359).
            negate(Expr::EnumEq {
                row: r,
                field: field.clone(),
                enum_ty: enum_ty.clone(),
                value: EnumValue {
                    ident: type_ident(wire),
                    wire: wire.clone(),
                    label: None,
                },
            })
        }
        (Domain::Bool | Domain::NullableBool, Term::Lit(v)) if domain.truth(*v).is_some() => {
            if row.always_null.contains(column) {
                return negate(Expr::False);
            }
            let field = row
                .fields
                .iter()
                .find(|f| f.wire == *column)
                .ok_or_else(unsupported)?;
            negate(Expr::BoolEq {
                row: r,
                field: field.clone(),
                value: domain.truth(*v) == Some(true),
            })
        }
        // `col == null` / `col == present` on any nullable column.
        (Domain::Nullable | Domain::NullableEnum(_) | Domain::NullableBool, Term::Lit(v)) => {
            let is_null = match row.fields.iter().find(|f| f.wire == *column) {
                None if row.always_null.contains(column) => Expr::True,
                None => return Err(unsupported()),
                Some(f) if !f.nullable => Expr::False,
                Some(f) => Expr::IsNull {
                    row: r,
                    field: f.clone(),
                },
            };
            let e = if *v == Value::NULL {
                is_null
            } else {
                Expr::Not(Box::new(is_null))
            };
            negate(e)
        }
        (Domain::Int, Term::Int(_) | Term::Lit(_)) => {
            let field = row
                .fields
                .iter()
                .find(|f| f.wire == *column)
                .ok_or_else(unsupported)?;
            if field.ty != FieldType::Scalar(crate::codegen::ir::Scalar::Int) || field.nullable {
                return Err("a comparison on a column that is not a plain Int".into());
            }
            let value = match rhs {
                Term::Int(n) => *n,
                Term::Lit(v) => domain.number(*v).ok_or_else(unsupported)?,
                _ => unreachable!(),
            };
            let op = ir_op(op);
            Ok(Expr::IntCmp {
                row: r,
                field: field.clone(),
                op,
                value,
            })
        }
        _ => Err(unsupported()),
    }
}

/// `input op rhs` on an input the caller chooses a column's value into. A null input
/// satisfies no comparison with a value, and so every `!=` (#359).
fn input_atom(
    domain: &Domain,
    op: TsCmp,
    rhs: &Term,
    input: usize,
    decl: &FnInput,
) -> Result<Expr, String> {
    let unsupported = || "a condition on a chosen value this phase does not print".to_string();
    let e = match (domain, rhs) {
        (Domain::Enum(_) | Domain::NullableEnum(_), Term::Lit(v)) if domain.name(*v).is_some() => {
            let wire = domain.name(*v).ok_or_else(unsupported)?.to_string();
            let FieldType::Enum(enum_ty) = &decl.ty else {
                return Err(unsupported());
            };
            Expr::InputEnumEq {
                input,
                nullable: decl.nullable,
                enum_ty: enum_ty.clone(),
                value: EnumValue {
                    ident: type_ident(&wire),
                    wire,
                    label: None,
                },
            }
        }
        (Domain::Bool | Domain::NullableBool, Term::Lit(v)) if domain.truth(*v).is_some() => {
            Expr::InputBoolEq {
                input,
                nullable: decl.nullable,
                value: domain.truth(*v) == Some(true),
            }
        }
        (Domain::Nullable | Domain::NullableEnum(_) | Domain::NullableBool, Term::Lit(v)) => {
            let is_null = if decl.nullable {
                Expr::InputIsNull { input }
            } else {
                Expr::False
            };
            if *v == Value::NULL {
                is_null
            } else {
                Expr::Not(Box::new(is_null))
            }
        }
        _ => return Err(unsupported()),
    };
    match op {
        TsCmp::Eq => Ok(e),
        TsCmp::Ne => Ok(Expr::Not(Box::new(e))),
        _ => Err(unsupported()),
    }
}

/// Folds the constant parts of an expression.
pub(in crate::codegen) fn simplify(e: Expr) -> Expr {
    match e {
        Expr::Exists { input, elem, body } => match simplify(*body) {
            Expr::False => Expr::False,
            body => Expr::Exists {
                input,
                elem,
                body: Box::new(body),
            },
        },
        Expr::Forall { input, elem, body } => match simplify(*body) {
            Expr::True => Expr::True,
            body => Expr::Forall {
                input,
                elem,
                body: Box::new(body),
            },
        },
        Expr::Count {
            input,
            elem,
            body,
            op,
            n,
        } => Expr::Count {
            input,
            elem,
            body: Box::new(simplify(*body)),
            op,
            n,
        },
        Expr::Not(inner) => match simplify(*inner) {
            Expr::True => Expr::False,
            Expr::False => Expr::True,
            Expr::Not(x) => *x,
            other => Expr::Not(Box::new(other)),
        },
        Expr::And(items) => {
            let mut kept = Vec::new();
            for item in items.into_iter().map(simplify) {
                match item {
                    Expr::True => {}
                    Expr::False => return Expr::False,
                    other => kept.push(other),
                }
            }
            match kept.len() {
                0 => Expr::True,
                1 => kept.pop().expect("one item"),
                _ => Expr::And(kept),
            }
        }
        Expr::Or(items) => {
            let mut kept = Vec::new();
            for item in items.into_iter().map(simplify) {
                match item {
                    Expr::False => {}
                    Expr::True => return Expr::True,
                    other => kept.push(other),
                }
            }
            match kept.len() {
                0 => Expr::False,
                1 => kept.pop().expect("one item"),
                _ => Expr::Or(kept),
            }
        }
        other => other,
    }
}

/// Points each quantifier at the input its list became.
pub(super) fn remap_lists(e: &mut Expr, to: &[usize]) {
    match e {
        Expr::Not(x) => remap_lists(x, to),
        Expr::And(xs) | Expr::Or(xs) => xs.iter_mut().for_each(|x| remap_lists(x, to)),
        Expr::Exists { input, body, .. }
        | Expr::Forall { input, body, .. }
        | Expr::Count { input, body, .. } => {
            *input = to[*input];
            remap_lists(body, to);
        }
        _ => {}
    }
}

/// A lifecycle comparison as the generated code's.
fn ir_op(op: TsCmp) -> CmpOp {
    match op {
        TsCmp::Eq => CmpOp::Eq,
        TsCmp::Ne => CmpOp::Ne,
        TsCmp::Lt => CmpOp::Lt,
        TsCmp::Le => CmpOp::Le,
        TsCmp::Gt => CmpOp::Gt,
        TsCmp::Ge => CmpOp::Ge,
    }
}
