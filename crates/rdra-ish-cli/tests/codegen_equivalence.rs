//! The generated transition functions do what the transition system does, on every
//! `samples.txt` root that `check` passes. BFS explores the system at `Bounds::STATES`, and
//! for each move of a use case the generator printed:
//!
//! - **Forward.** A function exists for the source states of the bound rows; its conditions
//!   hold on the pre-state; none of its argument checks fails; and every column it writes
//!   that the system tracks holds, in the successor, what the function writes (the tag, a
//!   literal, null, an unchanged value, or a value for an input).
//! - **Converse.** A function over one existing row allows no move BFS does not make: in
//!   every reached state, on every present row in the function's source state whose
//!   conditions and checks hold, the system has a move of that action on that row.
//!
//! - **Events.** A raise marks, in the successor, the row whose id the event carries.
//! - **Keys and run marks.** A key the function sets from a parent or a run mark names, in
//!   the successor, the slot BFS bound; a mark the step leaves holds the step in the run var.
//! - **Rule checks.** No row of a reached state breaks a rule of its type's validator.
//!
//! The printers print this IR, and `codegen_builds.rs` compiles what they print.

use std::path::{Path, PathBuf};
use std::process::Command;

use rdra_ish_core::lifecycle::{
    ActionOrigin, Bounds, Caps, Domain, GuardWhy, ParamRole, Scope, State, TransitionSystem, Value,
    VarId, VarKind,
};
use rdra_ish_core::load_project;
use rdra_ish_emit::codegen::build;
use rdra_ish_emit::codegen::ir::{Adapt, ArgCheck, Assigned, CmpOp, Expr, FnDecl, Lit, OutKind};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn check_passes(sample: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["check", sample])
        .output()
        .expect("runs rdra-ish")
        .status
        .success()
}

/// The var of `entity.column` in `ts`.
fn column_var(ts: &TransitionSystem, entity: &str, column: &str) -> Option<VarId> {
    ts.var_ids().find(|v| match &ts.var(*v).kind {
        VarKind::Column {
            entity: e,
            column: c,
            ..
        } => ts.entity(*e).id == entity && c == column,
        _ => false,
    })
}

fn enum_name(ts: &TransitionSystem, var: VarId, value: Value) -> Option<String> {
    match &ts.var(var).domain {
        Domain::Enum(values) => values.get(usize::from(value.0)).cloned(),
        _ => None,
    }
}

/// The value of a row's column, by the row's slot; `None` when the system does not track it.
struct Rows<'a> {
    ts: &'a TransitionSystem,
    state: &'a State,
    /// `(entity id, slot)` per row of the function.
    rows: Vec<(String, usize)>,
}

impl Rows<'_> {
    fn get(&self, row: usize, column: &str) -> Option<(VarId, Value)> {
        let (entity, slot) = &self.rows[row];
        let var = column_var(self.ts, entity, column)?;
        Some((var, self.state.get(self.ts, var, *slot)))
    }

    /// `None` when the expression reads a column the system does not track.
    fn eval(&self, e: &Expr) -> Option<bool> {
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
            Expr::IntCmp {
                row,
                field,
                op,
                value,
            } => {
                let (var, v) = self.get(*row, &field.wire)?;
                let n = self.ts.var(var).domain.number(v)?;
                match op {
                    CmpOp::Eq => n == *value,
                    CmpOp::Ne => n != *value,
                    CmpOp::Lt => n < *value,
                    CmpOp::Le => n <= *value,
                    CmpOp::Gt => n > *value,
                    CmpOp::Ge => n >= *value,
                }
            }
        })
    }

    /// Whether the function's argument checks pass on these rows.
    fn checks_pass(&self, f: &FnDecl) -> bool {
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
fn matching<'f>(fns: &'f [FnDecl], action: &str, rows: &Rows<'_>) -> Vec<&'f FnDecl> {
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

#[test]
fn functions_agree_with_the_transition_system() {
    let mut checked_moves = 0usize;
    let mut checked_rows = 0usize;
    let mut checked_rules = 0usize;
    let mut checked_keys = 0usize;
    let mut checked_events = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for sample in samples() {
        if !check_passes(&sample) {
            continue;
        }
        let project = load_project(&[repo().join(&sample)], None).expect("loads");
        let model = project.model;
        let domain = build(&model, true, &|_| None).expect("builds");
        let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
        let exploration = built.ts.explore(Caps::STATES);
        for run in &exploration.runs {
            let ts = &run.system;
            for edge in run.edges() {
                let (act, params) = run.step(edge);
                let action = &ts.actions()[act.index()];
                let ActionOrigin::UseCase { .. } = action.origin else {
                    continue;
                };
                if domain.skipped.iter().any(|s| s.action == action.name) {
                    continue;
                }
                let pre = &run.states[edge.from as usize];
                let post = &run.states[edge.to as usize];
                let row_slots = |f: &FnDecl| -> Vec<(String, usize)> {
                    f.rows
                        .iter()
                        .map(|r| {
                            let e = action.params[r.param].entity;
                            (ts.entity(e).id.clone(), usize::from(params[r.param]))
                        })
                        .collect()
                };
                // The rows as the first function of the action names them; every function
                // of one action binds the same params.
                let Some(first) = domain.fns.iter().find(|f| f.action == action.name) else {
                    failures.push(format!(
                        "{sample}: BFS runs {} but the generator printed no function and listed no skip",
                        action.name
                    ));
                    continue;
                };
                let before = Rows {
                    ts,
                    state: pre,
                    rows: row_slots(first),
                };
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
                let [f] = fns.as_slice() else {
                    failures.push(format!(
                        "{sample}: {} from {:?} matches {} functions",
                        action.name,
                        first.rows.iter().map(|r| &r.state).collect::<Vec<_>>(),
                        fns.len()
                    ));
                    continue;
                };
                checked_moves += 1;
                for c in &f.conditions {
                    if before.eval(&c.expr) == Some(false) {
                        failures.push(format!(
                            "{sample}: BFS runs {} where its function rejects it by {}",
                            f.name, c.id
                        ));
                    }
                }
                if !before.checks_pass(f) {
                    failures.push(format!(
                        "{sample}: BFS runs {} where its function's argument checks fail",
                        f.name
                    ));
                }
                // The rows an event is raised on: the raised var of each holds true.
                for event in &f.raises {
                    for (field, _) in &event.ids {
                        let Some(e) = ts.entity_ids().find(|e| ts.entity(*e).id == field.entity)
                        else {
                            continue;
                        };
                        let raised = ts.var_ids().find(|v| {
                            matches!(&ts.var(*v).kind, VarKind::Raised { entity, event: ev }
                                if *entity == e && type_name(ev) == event.ty)
                        });
                        let Some(p) = action.params.iter().position(|q| q.entity == e) else {
                            continue;
                        };
                        checked_events += 1;
                        let slot = usize::from(params[p]);
                        if raised.map(|v| post.get(ts, v, slot)) != Some(Value::TRUE) {
                            failures.push(format!(
                                "{sample}: {} raises {} on {} but BFS marks no row",
                                f.name, event.ty, field.entity
                            ));
                        }
                    }
                }
                // The marks the step leaves: the run var of each row holds the step.
                for mark in &f.marks_out {
                    let e = action.params[mark.param].entity;
                    let slot = usize::from(params[mark.param]);
                    let run = ts.var_ids().find(|v| {
                        matches!(&ts.var(*v).kind, VarKind::Run { entity, usecase }
                            if *entity == e && *usecase == f.usecase)
                    });
                    let got = run.map(|v| post.get(ts, v, slot));
                    if got != Some(Value(mark.step)) {
                        failures.push(format!(
                            "{sample}: {} leaves the mark of {} at {:?}, the function at step {}",
                            f.name,
                            ts.entity(e).id,
                            got,
                            mark.step
                        ));
                    }
                }
                for out in &f.outputs {
                    let e = action.params[out.param].entity;
                    let entity = ts.entity(e).id.clone();
                    let slot = usize::from(params[out.param]);
                    let at = |column: &str, state: &State| {
                        column_var(ts, &entity, column).map(|v| (v, state.get(ts, v, slot)))
                    };
                    match &out.kind {
                        OutKind::Deleted { .. } => {
                            if post.get(ts, ts.entity(e).exists, slot) != Value::DELETED {
                                failures.push(format!("{sample}: {} deletes no row", f.name));
                            }
                        }
                        OutKind::Row {
                            tag,
                            fields,
                            always_null,
                            ..
                        } => {
                            if let Some((column, value)) = tag {
                                let got = at(column, post).and_then(|(v, x)| enum_name(ts, v, x));
                                if got.as_deref() != Some(value.wire.as_str()) {
                                    failures.push(format!(
                                        "{sample}: {} leaves {entity}.{column} at {got:?}, the function at {}",
                                        f.name, value.wire
                                    ));
                                }
                            }
                            for (target, assigned) in fields {
                                let owner_is = |q: usize| {
                                    let key = ts.var_ids().find(|v| match &ts.var(*v).kind {
                                        VarKind::Owner { child, fk, .. } => {
                                            *child == e
                                                && model
                                                    .foreign_key(*fk)
                                                    .columns
                                                    .iter()
                                                    .any(|(c, _)| *c == target.wire)
                                        }
                                        _ => false,
                                    });
                                    match key {
                                        None => true,
                                        Some(v) => {
                                            ts.var(v).domain.slot(post.get(ts, v, slot))
                                                == Some(params[q])
                                        }
                                    }
                                };
                                let key_parent = match assigned {
                                    Assigned::ParentId { row: r, .. } => Some(f.rows[*r].param),
                                    Assigned::MarkId { mark, .. } => Some(f.marks_in[*mark].param),
                                    Assigned::EventId { param, .. } => Some(*param),
                                    _ => None,
                                };
                                if let Some(q) = key_parent {
                                    checked_keys += 1;
                                    if !owner_is(q) {
                                        failures.push(format!(
                                            "{sample}: {} links {entity}.{} to another row than BFS",
                                            f.name, target.wire
                                        ));
                                    }
                                    continue;
                                }
                                let Some((var, got)) = at(&target.wire, post) else {
                                    continue;
                                };
                                let ok = match assigned {
                                    Assigned::Lit { lit, .. } => match lit {
                                        Lit::Enum { value, .. } => {
                                            enum_name(ts, var, got).as_deref()
                                                == Some(value.wire.as_str())
                                        }
                                        Lit::Bool(b) => (got == Value::TRUE) == *b,
                                        Lit::Int(n) => ts.var(var).domain.number(got) == Some(*n),
                                    },
                                    Assigned::Null => got == Value::NULL,
                                    Assigned::Switch { arms, .. } => {
                                        let from = at(&target.wire, pre)
                                            .and_then(|(v, x)| enum_name(ts, v, x));
                                        let to = arms
                                            .iter()
                                            .find(|(f, _)| Some(&f.wire) == from.as_ref())
                                            .map(|(_, t)| t.wire.clone());
                                        to.is_some() && to == enum_name(ts, var, got)
                                    }
                                    Assigned::From { adapt, .. } => match adapt {
                                        Adapt::AbsentNull => got == Value::NULL,
                                        _ => at(&target.wire, pre).map(|(_, x)| x) == Some(got),
                                    },
                                    Assigned::Input { .. } => match ts.var(var).domain {
                                        Domain::Nullable => got == Value::PRESENT,
                                        _ => true,
                                    },
                                    Assigned::ParentId { .. }
                                    | Assigned::MarkId { .. }
                                    | Assigned::EventId { .. } => true,
                                };
                                if !ok {
                                    failures.push(format!(
                                        "{sample}: {} writes {entity}.{} as {assigned:?}, BFS leaves {got:?}",
                                        f.name, target.wire
                                    ));
                                }
                            }
                            for column in always_null {
                                if let Some((_, got)) = at(column, post) {
                                    if got != Value::NULL {
                                        failures.push(format!(
                                            "{sample}: {} leaves {entity}.{column} non-null where the type keeps it null",
                                            f.name
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Rule checks: no row of a reached state breaks a rule of its type's validator,
            // since `check` passed and so no reachable state breaks a rule.
            for v in &domain.validators {
                let Some(e) = ts.entity_ids().find(|e| ts.entity(*e).id == v.entity) else {
                    continue;
                };
                for state in &run.states {
                    for slot in 0..ts.bounds.slots() {
                        if !state.is_present(ts, e, slot) {
                            continue;
                        }
                        let rows = Rows {
                            ts,
                            state,
                            rows: vec![(v.entity.clone(), slot)],
                        };
                        if let Some((tag, value)) = &v.row.state {
                            let holds = rows
                                .get(0, tag)
                                .and_then(|(var, x)| enum_name(ts, var, x))
                                .is_some_and(|name| name == *value);
                            if !holds {
                                continue;
                            }
                        }
                        checked_rules += v.rules.len();
                        for rule in &v.rules {
                            if rows.eval(&rule.expr) == Some(false) {
                                failures.push(format!(
                                    "{sample}: {} reports {} on a row of a reached state",
                                    v.ty, rule.name
                                ));
                            }
                        }
                    }
                }
            }

            // Converse: a function over one existing row allows no move BFS lacks.
            if !run.complete {
                continue;
            }
            for f in &domain.fns {
                let Some((act, action)) = ts
                    .action_ids()
                    .map(|a| (a, &ts.actions()[a.index()]))
                    .find(|(_, a)| a.name == f.action)
                else {
                    continue;
                };
                // A step with a run guard is excluded: "no mark yet" is an obligation of
                // the code that stores the marks, not something the function checks.
                if action.params.len() != 1
                    || action.params[0].role != ParamRole::Existing
                    || f.rows.len() != 1
                    || action
                        .guards
                        .iter()
                        .any(|g| matches!(g.why, GuardWhy::Run { .. }))
                {
                    continue;
                }
                let e = action.params[0].entity;
                let entity = ts.entity(e).id.clone();
                for (at, state) in run.states.iter().enumerate() {
                    for slot in 0..ts.bounds.slots() {
                        if !state.is_present(ts, e, slot) {
                            continue;
                        }
                        let rows = Rows {
                            ts,
                            state,
                            rows: vec![(entity.clone(), slot)],
                        };
                        if matching(std::slice::from_ref(f), &f.action, &rows).is_empty() {
                            continue;
                        }
                        let allowed = f
                            .conditions
                            .iter()
                            .all(|c| rows.eval(&c.expr) != Some(false))
                            && rows.checks_pass(f);
                        if !allowed {
                            continue;
                        }
                        checked_rows += 1;
                        let moved = run.edges().iter().any(|edge| {
                            edge.from as usize == at && {
                                let (a, p) = run.step(edge);
                                a == act && p == [slot as u8]
                            }
                        });
                        if !moved {
                            failures.push(format!(
                                "{sample}: {} allows a run on {entity}#{slot} in a state where BFS makes none",
                                f.name
                            ));
                        }
                    }
                }
            }
        }
    }
    failures.sort();
    failures.dedup();
    assert!(
        failures.is_empty(),
        "{} disagreement(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked_moves > 1000, "checked only {checked_moves} moves");
    assert!(
        checked_rules > 1000,
        "checked only {checked_rules} rule checks"
    );
    assert!(checked_keys > 100, "checked only {checked_keys} keys");
    assert!(
        checked_events > 100,
        "checked only {checked_events} raised events"
    );
    eprintln!(
        "checked {checked_moves} moves, {checked_rows} rows, {checked_rules} rule checks, {checked_keys} keys, and {checked_events} raised events"
    );
}

/// The type name codegen gives an id: PascalCase, `_` and `-` splitting words.
fn type_name(id: &str) -> String {
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
