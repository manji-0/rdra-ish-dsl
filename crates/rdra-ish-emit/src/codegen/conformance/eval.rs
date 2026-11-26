//! The generated functions' conditions and source states read on a state of the transition
//! system: which function of an action a move takes, and whether a function allows a run.
//! `tests/codegen_equivalence.rs` checks these against every move BFS makes; the traces read
//! them to pick each move's function and the calls the model refuses.

use std::cell::RefCell;

use rdra_ish_core::lifecycle::{Domain, StateSlice, TransitionSystem, Value, VarId, VarKind};
use rdra_ish_core::SemanticModel;

use crate::codegen::ir::{ArgCheck, CmpOp, Expr, FnDecl};

/// The var of `entity.column` in `ts`.
pub fn column_var(ts: &TransitionSystem, entity: &str, column: &str) -> Option<VarId> {
    ts.var_ids().find(|v| match &ts.var(*v).kind {
        VarKind::Column {
            entity: e,
            column: c,
            ..
        } => ts.entity(*e).id == entity && c == column,
        _ => false,
    })
}

/// The name of an enum var's value.
pub fn enum_name(ts: &TransitionSystem, var: VarId, value: Value) -> Option<String> {
    ts.var(var).domain.name(value).map(str::to_string)
}

/// The value of a row's column, by the row's slot; `None` when the system does not track it.
pub struct Rows<'a> {
    ts: &'a TransitionSystem,
    model: &'a SemanticModel,
    state: &'a StateSlice,
    /// `(entity id, slot)` per row of the function.
    rows: Vec<(String, usize)>,
    /// The entity of each row a quantifier binds, numbered after `rows`.
    elems: Vec<String>,
    /// The slot each quantifier's row is at while its body is evaluated.
    env: RefCell<Vec<usize>>,
}

impl<'a> Rows<'a> {
    pub fn new(
        ts: &'a TransitionSystem,
        model: &'a SemanticModel,
        state: &'a StateSlice,
        rows: Vec<(String, usize)>,
        elems: Vec<String>,
    ) -> Rows<'a> {
        let n = elems.len();
        Rows {
            ts,
            model,
            state,
            rows,
            elems,
            env: RefCell::new(vec![0; n]),
        }
    }

    fn at(&self, row: usize) -> (String, usize) {
        match self.rows.get(row) {
            Some(r) => r.clone(),
            None => {
                let k = row - self.rows.len();
                (self.elems[k].clone(), self.env.borrow()[k])
            }
        }
    }

    /// The var and value of row `row`'s `column`; `None` when the system does not track it.
    pub fn get(&self, row: usize, column: &str) -> Option<(VarId, Value)> {
        let (entity, slot) = self.at(row);
        let var = column_var(self.ts, &entity, column)?;
        Some((var, self.state.get(self.ts, var, slot)))
    }

    /// `body` on every present row of the entity a quantifier's row `elem` ranges over, as
    /// the condition reads it; `None` when the body reads a column the system does not track.
    fn over(&self, elem: usize, body: &Expr) -> Option<Vec<bool>> {
        let k = elem - self.rows.len();
        let entity = self.elems[k].clone();
        let e = self
            .ts
            .entity_ids()
            .find(|e| self.ts.entity(*e).id == entity)?;
        let mut out = Vec::new();
        for slot in 0..self.ts.slots(e) {
            if !self.state.is_present(self.ts, e, slot) {
                continue;
            }
            self.env.borrow_mut()[k] = slot;
            out.push(self.eval(body)?);
        }
        Some(out)
    }

    /// `None` when the expression reads a column the system does not track.
    pub fn eval(&self, e: &Expr) -> Option<bool> {
        Some(match e {
            Expr::True => true,
            Expr::False => false,
            Expr::Not(inner) => !self.eval(inner)?,
            Expr::And(items) => {
                let mut all = true;
                for i in items {
                    all &= self.eval(i)?;
                }
                all
            }
            Expr::Or(items) => {
                let mut any = false;
                for i in items {
                    any |= self.eval(i)?;
                }
                any
            }
            // A value the caller passes: the pre-state does not hold it.
            Expr::InputIsNull { .. } | Expr::InputEnumEq { .. } | Expr::InputBoolEq { .. } => {
                return None
            }
            Expr::IsNull { row, field } => self.get(*row, &field.wire)?.1 == Value::NULL,
            Expr::EnumEq {
                row, field, value, ..
            } => {
                let (var, v) = self.get(*row, &field.wire)?;
                // Null satisfies no comparison (#359); a plain Enum holds no null, and its first
                // value shares the encoding.
                match self.ts.var(var).domain {
                    Domain::NullableEnum(_) if v == Value::NULL => false,
                    _ => enum_name(self.ts, var, v)? == value.wire,
                }
            }
            Expr::BoolEq { row, field, value } => {
                let (var, v) = self.get(*row, &field.wire)?;
                self.ts.var(var).domain.truth(v) == Some(*value)
            }
            Expr::KeyEq {
                child,
                child_field,
                parent,
                ..
            } => {
                let (centity, cslot) = self.at(*child);
                let (_, pslot) = self.at(*parent);
                let e = self
                    .ts
                    .entity_ids()
                    .find(|e| self.ts.entity(*e).id == centity)?;
                let owner = self.ts.var_ids().find(|v| match &self.ts.var(*v).kind {
                    VarKind::Owner { child, fk, .. } => {
                        *child == e
                            && self
                                .model
                                .foreign_key(*fk)
                                .columns
                                .iter()
                                .any(|(c, _)| *c == child_field.wire)
                    }
                    _ => false,
                })?;
                self.ts
                    .var(owner)
                    .domain
                    .slot(self.state.get(self.ts, owner, cslot))
                    == Some(u8::try_from(pslot).ok()?)
            }
            Expr::Exists { elem, body, .. } => self.over(*elem, body)?.iter().any(|v| *v),
            Expr::Forall { elem, body, .. } => self.over(*elem, body)?.iter().all(|v| *v),
            Expr::Count {
                elem, body, op, n, ..
            } => {
                let count = self.over(*elem, body)?.iter().filter(|v| **v).count();
                compare(i64::try_from(count).ok()?, *op, i64::from(*n))
            }
            Expr::IntCmp {
                row,
                field,
                op,
                value,
            } => {
                let (var, v) = self.get(*row, &field.wire)?;
                let n = self.ts.var(var).domain.number(v)?;
                compare(n, *op, *value)
            }
        })
    }

    /// Whether the function's argument checks pass on these rows.
    pub fn checks_pass(&self, f: &FnDecl) -> bool {
        f.checks.iter().all(|c| match c {
            ArgCheck::MustBeNull { row, field, .. } => self
                .get(*row, &field.wire)
                .is_none_or(|(_, v)| v == Value::NULL),
            // A link is the binding BFS chose; the owner var is not a column.
            ArgCheck::Link { .. } | ArgCheck::EventRow { .. } => true,
        })
    }

    /// Whether `f` runs on these rows: no condition is false and every check passes.
    pub fn allows(&self, f: &FnDecl) -> bool {
        f.conditions
            .iter()
            .all(|c| self.eval(&c.expr) != Some(false))
            && self.checks_pass(f)
    }
}

/// The functions of `action` whose source states are the rows' states.
pub fn matching<'f>(fns: &'f [FnDecl], action: &str, rows: &Rows<'_>) -> Vec<&'f FnDecl> {
    fns.iter()
        .filter(|f| f.action == action)
        .filter(|f| {
            f.rows.iter().enumerate().all(|(r, row)| match &row.state {
                None => true,
                Some((tag, value)) => rows
                    .get(r, tag)
                    .and_then(|(var, v)| enum_name(rows.ts, var, v))
                    .is_some_and(|name| name == *value),
            })
        })
        .collect()
}

pub fn compare(a: i64, op: CmpOp, b: i64) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    }
}
