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
//! - **Related rows.** A `.has` / `.none` condition ranges over every present row of the
//!   entity, and a key between two rows holds when the child's owner var names the parent.
//! - **Rule checks.** No row of a reached state breaks a rule of its type's validator.
//!
//! The printers print this IR, and `codegen_builds.rs` compiles what they print. The
//! evaluator it reads the functions' conditions with (`codegen::conformance::eval`) is the one
//! the conformance traces pick each move's function with.

mod codegen_cases;

use codegen_cases::{
    check_passes, column_var, enum_name, explore, matching, pick, row_slots, samples, type_name,
    Explored, Rows,
};
use rdra_ish_core::lifecycle::{
    ActionOrigin, Domain, GuardWhy, ParamRole, StateSlice, Value, VarKind,
};
use rdra_ish_emit::codegen::ir::{Adapt, Assigned, Lit, OutKind};

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
        let Explored { model, domain, run } = explore(&sample);
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
            let pre = &run.states.at(edge.from as usize);
            let post = &run.states.at(edge.to as usize);
            let f = match pick(&domain, &model, ts, action, params, pre, post) {
                Ok(f) => f,
                Err(why) => {
                    failures.push(format!("{sample}: {why}"));
                    continue;
                }
            };
            checked_moves += 1;
            // The function's own rows, with the rows its quantifiers bind.
            let before = Rows::new(
                ts,
                &model,
                pre,
                row_slots(ts, action, params, f),
                f.elems.iter().map(|e| e.entity.clone()).collect(),
            );
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
                    let Some(e) = ts.entity_ids().find(|e| ts.entity(*e).id == field.entity) else {
                        continue;
                    };
                    let raised = ts.var_ids().find(|v| {
                        matches!(&ts.var(*v).kind, VarKind::Raised { entity, event: ev, handler: None }
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
                let at = |column: &str, state: &StateSlice| {
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
                                Assigned::ParentId { row: r, .. } => f.rows[*r].param,
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
                                    Lit::Bool(b) => ts.var(var).domain.truth(got) == Some(*b),
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
                for slot in 0..ts.slots(e) {
                    if !state.is_present(ts, e, slot) {
                        continue;
                    }
                    let rows = Rows::new(
                        ts,
                        &model,
                        &state,
                        vec![(v.entity.clone(), slot)],
                        Vec::new(),
                    );
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
                for slot in 0..ts.slots(e) {
                    if !state.is_present(ts, e, slot) {
                        continue;
                    }
                    let rows = Rows::new(
                        ts,
                        &model,
                        &state,
                        vec![(entity.clone(), slot)],
                        f.elems.iter().map(|e| e.entity.clone()).collect(),
                    );
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
                            let (a, p) = run.step(&edge);
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
