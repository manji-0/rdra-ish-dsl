//! Transition functions from the transition system's actions (docs/design/algebraic-codegen.md,
//! "Transition functions"). The functions read the one system `states`, `check`, and `verify`
//! read, so they do what the verified model does.
//!
//! An action gets one function per combination of the source states of its rows, so every
//! type in a function is concrete. An action that uses a part of the vocabulary this phase
//! does not print yet is listed as skipped, with the reason, and gets no function: the output
//! stays buildable and says what it leaves out.

use std::collections::BTreeMap;

use rdra_ish_core::collect_event_flows;
use rdra_ish_core::lifecycle::{
    Action, ActionOrigin, At, CmpOp as TsCmp, Domain, Effect, EntId, GuardWhy, ParamRole, Pred,
    Rhs, Term, TransitionSystem, Value, VarId, VarKind,
};
use rdra_ish_core::location::LocatedSpan;
use rdra_ish_core::model::{Raiser, UseCaseKey};
use rdra_ish_core::SemanticModel;

use super::ir::{
    Adapt, ArgCheck, Assigned, CmpOp, CondGuard, DispatchCase, DispatchDecl, DomainModel,
    EnumValue, EventDecl, Expr, Field, FieldType, FnDecl, FnInput, Lit, OutKind, RecordDecl,
    RowOut, RowParam, Skipped, SumDecl, VariantDecl,
};
use super::{type_ident, CodegenError, Names};

mod dispatch;
mod guards;
mod lower;
mod pred;
mod values;

use dispatch::dispatchers;
use guards::{arm_conditions, fold_next, reads_only_created};
use lower::{function, ActionCx};
pub(super) use pred::{expr, simplify};
use pred::{remap_lists, Quant};
use values::{declared_id, enum_lit, input_index, literal, start_literal, start_value, Write};

/// The most functions one action gets; past it the action is skipped.
const MAX_COMBINATIONS: usize = 16;

/// The type of an entity, as `build` declared it.
enum Declared<'d> {
    Record(&'d RecordDecl),
    Sum(&'d SumDecl),
}

/// One source state of a row: the type, its fields, and (for a state type) its variant.
#[derive(Clone)]
struct Source<'d> {
    ty: String,
    fields: Vec<Field>,
    always_null: Vec<String>,
    variant: Option<&'d VariantDecl>,
}

/// Why an action or one of its combinations gets no function.
enum Skip {
    /// The whole action: the vocabulary is not printed yet.
    Action(String),
    /// This combination: the verified model never runs the action from it.
    Combination,
}

pub(super) fn add_functions(
    model: &SemanticModel,
    ts: &TransitionSystem,
    domain: &mut DomainModel,
    names: &mut Names,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<(), CodegenError> {
    let flows = collect_event_flows(model);
    let mut events: BTreeMap<String, EventDecl> = BTreeMap::new();
    let mut fns = Vec::new();
    let mut skipped = Vec::new();
    let mut marks: Vec<super::ir::MarkDecl> = Vec::new();
    // The number of steps of each split use case.
    let mut steps_of: BTreeMap<String, u8> = BTreeMap::new();
    for action in ts.actions() {
        if let ActionOrigin::UseCase {
            uc, step: Some(i), ..
        } = &action.origin
        {
            let n = steps_of.entry(model.use_cases[*uc].id.clone()).or_insert(0);
            *n = (*n).max(*i);
        }
    }
    {
        let declared = declared_types(domain);
        for action in ts.actions() {
            let ActionOrigin::UseCase { uc, step, .. } = &action.origin else {
                continue;
            };
            let steps = step.map(|i| (i, steps_of[&model.use_cases[*uc].id]));
            // A split use case raises its events at its last step.
            let raises: Vec<String> = flows
                .iter()
                .filter(|f| f.raised_by.contains(&Raiser::UseCase(*uc)))
                .filter(|_| steps.is_none_or(|(i, k)| i == k))
                .map(|f| model.events[f.event].id.clone())
                .collect();
            match functions(
                model, ts, action, *uc, &declared, &raises, steps, &mut marks, site,
            ) {
                Ok(list) if list.is_empty() => skipped.push(Skipped {
                    action: action.name.clone(),
                    reason: "no source state the verified model runs it from".to_string(),
                }),
                Ok(list) => {
                    for event in &raises {
                        let fields = event_fields(ts, &declared, event);
                        events.entry(event.clone()).or_insert_with(|| EventDecl {
                            name: type_ident(event),
                            doc: model
                                .events
                                .values()
                                .find(|e| &e.id == event)
                                .map(|e| vec![e.label.clone()])
                                .unwrap_or_default(),
                            fields,
                        });
                    }
                    for f in &list {
                        if let Some(ev) = &f.event_in {
                            let id = ev.event.clone();
                            let fields = event_fields(ts, &declared, &id);
                            events.entry(id.clone()).or_insert_with(|| EventDecl {
                                name: ev.ty.clone(),
                                doc: model
                                    .events
                                    .values()
                                    .find(|e| e.id == id)
                                    .map(|e| vec![e.label.clone()])
                                    .unwrap_or_default(),
                                fields,
                            });
                        }
                    }
                    fns.extend(list);
                }
                Err(reason) => skipped.push(Skipped {
                    action: action.name.clone(),
                    reason,
                }),
            }
        }
    }
    for event in events.values() {
        names.claim(event.name.clone(), format!("the event {}", event.name))?;
    }
    for f in &fns {
        names.claim(f.name.clone(), format!("the function of {}", f.usecase))?;
        names.claim(f.result.clone(), format!("the result of {}", f.name))?;
    }
    let dispatchers = dispatchers(&fns, domain);
    for d in &dispatchers {
        names.claim(d.name.clone(), format!("the dispatcher of {}", d.usecase))?;
        names.claim(d.outcome.clone(), format!("the outcome of {}", d.name))?;
    }
    domain.dispatchers = dispatchers;
    // The run marks, and what a split use case asks of the code that stores them.
    marks.sort_by(|a, b| a.name.cmp(&b.name));
    marks.dedup();
    let mut split: Vec<String> = marks.iter().map(|m| m.usecase.clone()).collect();
    split.sort();
    split.dedup();
    for uc in &split {
        let names: Vec<&str> = marks
            .iter()
            .filter(|m| &m.usecase == uc)
            .map(|m| m.name.as_str())
            .collect();
        domain.obligations.push(super::ir::Obligation {
            invariant: None,
            text: format!(
                "{uc} runs as separate transactions linked by run marks ({}): store each mark with its row in the transaction of the step that returns it; keep at most one mark of {uc} per row and start step 1 only on a row with none; never abandon a run, but retry a step that returns NotEnabled or Rejected until it runs.",
                names.join(", ")
            ),
            site: None,
        });
    }
    // A list a `.has` / `.none` ranges over must hold every present row of its entity.
    let mut lists: Vec<String> = fns
        .iter()
        .flat_map(|f| f.inputs.iter())
        .filter_map(|i| match &i.ty {
            FieldType::Rows(ty) => Some(ty.clone()),
            _ => None,
        })
        .collect();
    lists.sort();
    lists.dedup();
    for ty in lists {
        domain.obligations.push(super::ir::Obligation {
            invariant: None,
            text: format!(
                "a function input of {ty} rows holds every present row of the entity, read in the function's transaction: a `.has` / `.none` condition reads them all."
            ),
            site: None,
        });
    }
    for m in &marks {
        names.claim(
            m.name.clone(),
            format!("the run mark of {} at step {}", m.usecase, m.step),
        )?;
    }
    domain.marks = marks;
    domain.events = events.into_values().collect();
    domain.fns = fns;
    domain.skipped = skipped;
    Ok(())
}

/// The generated type of each entity, by entity id.
fn declared_types(domain: &DomainModel) -> BTreeMap<String, Declared<'_>> {
    let mut out = BTreeMap::new();
    for record in &domain.records {
        if record.kind == super::ir::RecordKind::Entity {
            out.insert(record.id.clone(), Declared::Record(record));
        }
    }
    for sum in &domain.sums {
        out.insert(sum.entity.clone(), Declared::Sum(sum));
    }
    out
}

/// The functions of one action, or why it gets none.
#[allow(clippy::too_many_arguments)]
fn functions(
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    uc: UseCaseKey,
    declared: &BTreeMap<String, Declared<'_>>,
    raises: &[String],
    steps: Option<(u8, u8)>,
    marks: &mut Vec<super::ir::MarkDecl>,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<Vec<FnDecl>, String> {
    let entity_key = |e: EntId| ts.entity(e).key;
    let type_of = |e: EntId| -> Result<&Declared<'_>, String> {
        let id = &model.entities[entity_key(e)].id;
        declared
            .get(id)
            .ok_or_else(|| format!("entity {id} has no generated type"))
    };

    for guard in &action.guards {
        match guard.why {
            GuardWhy::TransitionFrom | GuardWhy::Owner => {}
            GuardWhy::Condition { later: None, .. } if !guard.reads_successor() => {}
            GuardWhy::Raised { .. } => {}
            GuardWhy::Clock => return Err("reads `now` (phase 5)".into()),
            GuardWhy::Link => return Err("links, relinks, or unlinks a row (phase 5)".into()),
            GuardWhy::Copy => return Err("copies a value from another row (phase 5)".into()),
            GuardWhy::ArmCondition { .. } => {}
            GuardWhy::Run { .. } => {}
            GuardWhy::Condition { .. } => {
                return Err(
                    "a condition on what the use case creates or a later step writes (phase 5)"
                        .into(),
                )
            }
        }
    }

    // The tag var of each param's entity, when it is a state type.
    let tag_var = |p: usize| -> Option<VarId> {
        let e = action.params[p].entity;
        let Declared::Sum(sum) = type_of(e).ok()? else {
            return None;
        };
        ts.var_ids().find(|v| {
            matches!(&ts.var(*v).kind, VarKind::Column { entity, column, .. }
                if *entity == e && *column == sum.tag.wire)
        })
    };

    // The arms each param's tag takes: (from, to) by value names.
    let mut arms: BTreeMap<usize, Vec<(String, String)>> = BTreeMap::new();
    for effect in &action.effects {
        if let Effect::Transition {
            var,
            param,
            arms: a,
            ..
        } = effect
        {
            let p = usize::from(*param);
            let Domain::Enum(values) = &ts.var(*var).domain else {
                return Err("a transition on a column that is not an enum".into());
            };
            let named: Vec<(String, String)> = a
                .iter()
                .map(|arm| {
                    (
                        values[usize::from(arm.from.0)].clone(),
                        values[usize::from(arm.to.0)].clone(),
                    )
                })
                .collect();
            if Some(*var) != tag_var(p) {
                // A second lifecycle column: a branch on its value, not a state type.
                let mut froms: Vec<&String> = named.iter().map(|(f, _)| f).collect();
                froms.sort();
                froms.dedup();
                if froms.len() != named.len() {
                    return Err("a choice among arms of a second lifecycle column".into());
                }
                continue;
            }
            arms.insert(p, named);
        }
    }

    // The source states of each param.
    let mut sources: Vec<Vec<Source<'_>>> = Vec::new();
    for (p, param) in action.params.iter().enumerate() {
        let declared = type_of(param.entity)?;
        let list = match (param.role, declared) {
            // A row of an earlier step: the step reads it through its run mark only.
            (ParamRole::Earlier { .. }, _) => vec![Source {
                ty: String::new(),
                fields: Vec::new(),
                always_null: Vec::new(),
                variant: None,
            }],
            // The row an event was raised on: read only, in any state, as its flat row.
            (ParamRole::Triggered, Declared::Sum(sum)) => vec![Source {
                ty: sum.row.clone(),
                fields: sum.row_fields.clone(),
                always_null: Vec::new(),
                variant: None,
            }],
            (ParamRole::New, Declared::Record(r)) => vec![record_source(r)],
            (ParamRole::New, Declared::Sum(_)) => vec![Source {
                ty: String::new(),
                fields: Vec::new(),
                always_null: Vec::new(),
                variant: None,
            }],
            (_, Declared::Record(r)) => vec![record_source(r)],
            (_, Declared::Sum(sum)) => sum
                .variants
                .iter()
                .filter(|v| match arms.get(&p) {
                    Some(a) => a.iter().any(|(from, _)| *from == v.value.wire),
                    None => true,
                })
                .map(variant_source)
                .collect(),
        };
        sources.push(list);
    }

    let combos = combinations(&sources);
    if combos.len() > MAX_COMBINATIONS {
        return Err(format!(
            "{} combinations of source states (more than {MAX_COMBINATIONS})",
            combos.len()
        ));
    }
    let varies: Vec<bool> = sources.iter().map(|s| s.len() > 1).collect();
    let usecase = &model.use_cases[uc];
    let cx = ActionCx {
        model,
        ts,
        action,
        declared,
        raises,
        site,
        usecase: &usecase.id,
        steps,
    };

    let mut out = Vec::new();
    for combo in combos {
        let picked: Vec<&Source<'_>> = combo
            .iter()
            .enumerate()
            .map(|(p, i)| &sources[p][*i])
            .collect();
        let mut base = type_ident(&action.name);
        let suffix: Vec<String> = picked
            .iter()
            .zip(&varies)
            .filter(|(_, v)| **v)
            .filter_map(|(s, _)| s.variant.map(|v| v.value.ident.clone()))
            .collect();
        if !suffix.is_empty() {
            base = format!("{base}From{}", suffix.join("And"));
        }
        // The arms each param can take from its source: several from one state are a
        // choice, and each choice is a function of its own.
        let mut options: Vec<(usize, Vec<(String, String)>)> = Vec::new();
        for (p, a) in &arms {
            let from = picked[*p].variant.map(|v| v.value.wire.clone());
            let open: Vec<(String, String)> = match &from {
                Some(from) => a.iter().filter(|(f, _)| f == from).cloned().collect(),
                None => a.clone(),
            };
            options.push((*p, open));
        }
        let mut choices: Vec<BTreeMap<usize, Vec<(String, String)>>> = vec![BTreeMap::new()];
        for (p, open) in &options {
            let mut next = Vec::new();
            for chosen in &choices {
                if open.len() > 1 && picked[*p].variant.is_some() {
                    for arm in open {
                        let mut c = chosen.clone();
                        c.insert(*p, vec![arm.clone()]);
                        next.push(c);
                    }
                } else {
                    let mut c = chosen.clone();
                    c.insert(*p, arms[p].clone());
                    next.push(c);
                }
            }
            choices = next;
        }
        if choices.len() > MAX_COMBINATIONS {
            return Err(format!(
                "{} choices among arms (more than {MAX_COMBINATIONS})",
                choices.len()
            ));
        }
        for chosen in choices {
            let mut name = base.clone();
            let to: Vec<String> = options
                .iter()
                .filter(|(p, open)| open.len() > 1 && picked[*p].variant.is_some())
                .filter_map(|(p, _)| chosen[p].first().map(|(_, t)| type_ident(t)))
                .collect();
            if !to.is_empty() {
                name = format!("{name}To{}", to.join("And"));
            }
            match function(&cx, &picked, &chosen, &name, marks) {
                Ok(mut f) => {
                    f.doc
                        .insert(0, format!("{} ({}).", usecase.label, usecase.id));
                    for (p, open) in &options {
                        if open.len() < 2 || picked[*p].variant.is_none() {
                            continue;
                        }
                        let Some(r) = f.rows.iter().position(|row| row.param == Some(*p)) else {
                            continue;
                        };
                        let Some(Declared::Sum(sum)) = declared.get(&f.rows[r].entity) else {
                            continue;
                        };
                        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
                            continue;
                        };
                        let target = &chosen[p][0].1;
                        if let Some(v) = sum.variants.iter().find(|v| v.value.wire == *target) {
                            f.chosen.push((r, tag_enum.clone(), v.value.clone()));
                        }
                    }
                    out.push(f);
                }
                Err(Skip::Combination) => {}
                Err(Skip::Action(reason)) => return Err(reason),
            }
        }
    }
    Ok(out)
}

fn record_source(record: &RecordDecl) -> Source<'_> {
    Source {
        ty: record.name.clone(),
        fields: record.fields.clone(),
        always_null: Vec::new(),
        variant: None,
    }
}

fn variant_source(variant: &VariantDecl) -> Source<'_> {
    Source {
        ty: variant.name.clone(),
        fields: variant.fields.clone(),
        always_null: variant.always_null.iter().map(|f| f.wire.clone()).collect(),
        variant: Some(variant),
    }
}

/// Every pick of one source per param.
fn combinations(sources: &[Vec<Source<'_>>]) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new()];
    for list in sources {
        let mut next = Vec::new();
        for prefix in &out {
            for i in 0..list.len() {
                let mut c = prefix.clone();
                c.push(i);
                next.push(c);
            }
        }
        out = next;
    }
    out
}

/// snake_case of an id: `CustomerAccount` is `customer_account`.
pub(super) fn snake(id: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in id.chars() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            prev_lower = false;
            continue;
        }
        if ch.is_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
            prev_lower = false;
        } else {
            out.push(ch);
            prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// The payload of an event: the id of each row its raise marks, by the system's `Raised`
/// vars. An event no use case raise orders anything by has none.
fn event_fields(
    ts: &TransitionSystem,
    declared: &BTreeMap<String, Declared<'_>>,
    event: &str,
) -> Vec<super::ir::EventField> {
    let mut out: Vec<super::ir::EventField> = Vec::new();
    for v in ts.var_ids() {
        let VarKind::Raised { entity, event: ev } = &ts.var(v).kind else {
            continue;
        };
        if ev != event {
            continue;
        }
        let id = ts.entity(*entity).id.clone();
        let fields = match declared.get(&id) {
            Some(Declared::Record(r)) => &r.fields,
            Some(Declared::Sum(s)) => &s.row_fields,
            None => continue,
        };
        let Some((want, _)) = declared_id(&id, fields) else {
            continue;
        };
        let field = super::ir::EventField {
            name: format!("{}_id", snake(&id)),
            entity: id,
            id_ty: want,
        };
        if !out.contains(&field) {
            out.push(field);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
