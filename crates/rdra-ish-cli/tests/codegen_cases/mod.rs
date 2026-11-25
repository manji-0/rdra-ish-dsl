//! What `codegen_equivalence.rs` and `codegen_differential.rs` share: the samples `check`
//! passes, BFS's run of each at `Bounds::STATES`, the IR built at the same caps, the
//! function of the IR each BFS edge runs, and the IR's conditions read on a BFS state.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rdra_ish_core::lifecycle::{
    Action, Bounds, Caps, ComponentRun, Domain, Scope, StateSlice, TransitionSystem, Value, VarId,
    VarKind,
};
use rdra_ish_core::{load_project, SemanticModel};
use rdra_ish_emit::codegen::build;
use rdra_ish_emit::codegen::ir::{ArgCheck, CmpOp, DomainModel, Expr, FnDecl, OutKind};

/// The caps the IR is built at, and the printed code generated at.
pub fn caps() -> Caps {
    Caps::for_instances(1)
}

/// A sample `check` passes: its model, the IR, and BFS's run of the whole system.
pub struct Explored {
    pub model: SemanticModel,
    pub domain: DomainModel,
    pub run: ComponentRun,
}

pub fn explore(sample: &str) -> Explored {
    let project = load_project(&[repo().join(sample)], None).expect("loads");
    let model = project.model;
    let domain = build(&model, Some(caps()), &|_| None).expect("builds");
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    // The whole system, not a cone, and charged to no pool: its first 65,536 states
    // sample the moves.
    let run = built.ts.clone().run(Caps::new(caps().patterns, 1 << 16));
    Explored { model, domain, run }
}

/// The `(entity id, slot)` of each row of `f`, as the edge's params bind them.
pub fn row_slots(
    ts: &TransitionSystem,
    action: &Action,
    params: &[u8],
    f: &FnDecl,
) -> Vec<(String, usize)> {
    f.rows
        .iter()
        .map(|r| {
            let param = r.param.expect("a function row binds a param");
            let e = action.params[param].entity;
            (ts.entity(e).id.clone(), usize::from(params[param]))
        })
        .collect()
}

/// The function of `domain` a BFS move of `action` with `params` from `pre` to `post` runs:
/// the one whose source states are the rows' states and, among several arms from one state,
/// whose target BFS took. `Err` says why there is not exactly one.
pub fn pick<'f>(
    domain: &'f DomainModel,
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    params: &[u8],
    pre: &StateSlice,
    post: &StateSlice,
) -> Result<&'f FnDecl, String> {
    // The rows as the first function of the action names them; every function of one
    // action binds the same params.
    let Some(first) = domain.fns.iter().find(|f| f.action == action.name) else {
        return Err(format!(
            "BFS runs {} but the generator printed no function and listed no skip",
            action.name
        ));
    };
    let before = Rows::new(
        ts,
        model,
        pre,
        row_slots(ts, action, params, first),
        Vec::new(),
    );
    let mut fns = matching(&domain.fns, &action.name, &before);
    // Several arms from one state: the function whose target BFS took.
    if fns.len() > 1 {
        fns.retain(|f| {
            f.outputs.iter().all(|o| match &o.kind {
                OutKind::Row {
                    tag: Some((column, value)),
                    ..
                } => {
                    let e = action.params[o.param].entity;
                    let slot = usize::from(params[o.param]);
                    column_var(ts, &ts.entity(e).id, column)
                        .and_then(|v| enum_name(ts, v, post.get(ts, v, slot)))
                        .is_some_and(|n| n == value.wire)
                }
                _ => true,
            })
        });
    }
    match fns.as_slice() {
        [f] => Ok(f),
        _ => Err(format!(
            "{} from {:?} matches {} functions",
            action.name,
            first.rows.iter().map(|r| &r.state).collect::<Vec<_>>(),
            fns.len()
        )),
    }
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub fn check_passes(sample: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["check", sample])
        .output()
        .expect("runs rdra-ish")
        .status
        .success()
}

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

pub fn enum_name(ts: &TransitionSystem, var: VarId, value: Value) -> Option<String> {
    match &ts.var(var).domain {
        Domain::Enum(values) => values.get(usize::from(value.0)).cloned(),
        _ => None,
    }
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
    env: std::cell::RefCell<Vec<usize>>,
}

impl Rows<'_> {
    pub fn new<'a>(
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
            env: std::cell::RefCell::new(vec![0; n]),
        }
    }

    pub fn at(&self, row: usize) -> (String, usize) {
        match self.rows.get(row) {
            Some(r) => r.clone(),
            None => {
                let k = row - self.rows.len();
                (self.elems[k].clone(), self.env.borrow()[k])
            }
        }
    }

    pub fn get(&self, row: usize, column: &str) -> Option<(VarId, Value)> {
        let (entity, slot) = self.at(row);
        let var = column_var(self.ts, &entity, column)?;
        Some((var, self.state.get(self.ts, var, slot)))
    }

    /// `body` on every present row of the entity a quantifier's row `elem` ranges over, as
    /// the condition reads it; `None` when the body reads a column the system does not track.
    pub fn over(&self, elem: usize, body: &Expr) -> Option<Vec<bool>> {
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
            Expr::IsNull { row, field } => self.get(*row, &field.wire)?.1 == Value::NULL,
            Expr::EnumEq {
                row, field, value, ..
            } => {
                let (var, v) = self.get(*row, &field.wire)?;
                enum_name(self.ts, var, v)? == value.wire
            }
            Expr::BoolEq { row, field, value } => {
                (self.get(*row, &field.wire)?.1 == Value::TRUE) == *value
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

/// The type name codegen gives an id: PascalCase, `_` and `-` splitting words.
pub fn type_name(id: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in id.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
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
