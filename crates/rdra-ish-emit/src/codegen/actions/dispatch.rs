//! The dispatcher of an action whose functions take a state type or are several.

use super::*;

/// One dispatcher per action whose functions take a state type or are several.
pub(super) fn dispatchers(fns: &[FnDecl], domain: &DomainModel) -> Vec<DispatchDecl> {
    let mut actions: Vec<&str> = Vec::new();
    for f in fns {
        if !actions.contains(&f.action.as_str()) {
            actions.push(&f.action);
        }
    }
    let sum_of = |entity: &str| domain.sums.iter().find(|s| s.entity == entity);
    let mut out = Vec::new();
    for action in actions {
        let group: Vec<&FnDecl> = fns.iter().filter(|f| f.action == action).collect();
        let first = group[0];
        let any_state = first.rows.iter().any(|r| r.state.is_some());
        if group.len() == 1 && !any_state {
            continue;
        }
        let rows: Vec<(String, String)> = first
            .rows
            .iter()
            .map(|r| {
                let ty = match sum_of(&r.entity) {
                    Some(sum) if r.state.is_some() => sum.name.clone(),
                    _ => r.ty.clone(),
                };
                (r.name.clone(), ty)
            })
            .collect();
        let mut inputs: Vec<FnInput> = Vec::new();
        let mut choices: Vec<(String, String)> = Vec::new();
        let mut cases = Vec::new();
        let pascal = type_ident(action);
        for f in &group {
            let mut taken = Vec::new();
            for input in &f.inputs {
                let i = match inputs.iter().position(|x| x.name == input.name) {
                    Some(i) => i,
                    None => {
                        inputs.push(input.clone());
                        inputs.len() - 1
                    }
                };
                taken.push(i);
            }
            let states = f
                .rows
                .iter()
                .map(|r| {
                    let (tag, value) = r.state.as_ref()?;
                    let sum = sum_of(&r.entity)?;
                    let v = sum.variants.iter().find(|v| v.value.wire == *value)?;
                    Some((r.ty.clone(), tag.clone(), v.value.clone()))
                })
                .collect();
            let mut choice = Vec::new();
            for c in &f.chosen {
                let name = f.choice_name(c);
                let i = match choices.iter().position(|(n, _)| *n == name) {
                    Some(i) => i,
                    None => {
                        choices.push((name, c.enum_ty.clone()));
                        choices.len() - 1
                    }
                };
                choice.push((i, c.value.clone()));
            }
            let case = f.name.strip_prefix(&pascal).unwrap_or(&f.name);
            cases.push(DispatchCase {
                function: f.name.clone(),
                result: f.result.clone(),
                case: if case.is_empty() {
                    "Ran".to_string()
                } else {
                    case.to_string()
                },
                rejects: !f.conditions.is_empty(),
                states,
                inputs: taken,
                choice,
            });
        }
        // The arguments after the rows: the run marks, then the event.
        let mut marks: Vec<(String, String)> = first
            .marks_in
            .iter()
            .map(|m| (m.name.clone(), m.ty.clone()))
            .collect();
        if let Some(ev) = &first.event_in {
            marks.push((ev.name.clone(), ev.ty.clone()));
        }
        out.push(DispatchDecl {
            marks,
            choices,
            name: format!("Run{pascal}"),
            usecase: first.usecase.clone(),
            outcome: format!("{pascal}Outcome"),
            rows,
            inputs,
            rejects: cases.iter().any(|c| c.rejects),
            cases,
        });
    }
    out
}
