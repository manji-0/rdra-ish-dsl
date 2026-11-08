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
use rdra_ish_core::model::Raiser;
use rdra_ish_core::SemanticModel;

use super::ir::{
    Adapt, ArgCheck, Assigned, CmpOp, CondGuard, DispatchCase, DispatchDecl, DomainModel,
    EnumValue, EventDecl, Expr, Field, FieldType, FnDecl, FnInput, Lit, OutKind, RecordDecl,
    RowOut, RowParam, Skipped, SumDecl, VariantDecl,
};
use super::{type_ident, CodegenError, Names};

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
            let ActionOrigin::UseCase {
                uc, step, trigger, ..
            } = &action.origin
            else {
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
            let _ = trigger;
            let reason: Option<String> = None;
            let built = match reason {
                Some(reason) => Err(reason),
                None => functions(
                    model, ts, action, &declared, &raises, steps, &mut marks, site,
                ),
            };
            match built {
                Ok(list) if list.is_empty() => skipped.push(Skipped {
                    action: action.name.clone(),
                    reason: "no source state the verified model runs it from".to_string(),
                }),
                Ok(list) => {
                    for event in &raises {
                        let fields = event_fields(model, ts, &declared, event);
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
                            let id = model
                                .events
                                .values()
                                .find(|e| type_ident(&e.id) == ev.ty)
                                .map(|e| e.id.clone())
                                .unwrap_or_default();
                            let fields = event_fields(model, ts, &declared, &id);
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
        let id = format!("O{}", domain.obligations.len() + 1);
        domain.obligations.push(super::ir::Obligation {
            id,
            invariant: None,
            text: format!(
                "{uc} runs as separate transactions linked by run marks ({}): store each mark with its row in the transaction of the step that returns it; keep at most one mark of {uc} per row and start step 1 only on a row with none; never abandon a run, but retry a step that returns NotEnabled or Rejected until it runs.",
                names.join(", ")
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

fn declared_types(domain: &DomainModel) -> BTreeMap<String, Declared<'_>> {
    let mut out = BTreeMap::new();
    for record in &domain.records {
        if record.kind == super::ir::RecordKind::Entity {
            out.insert(record.name.clone(), Declared::Record(record));
        }
    }
    for sum in &domain.sums {
        out.insert(sum.name.clone(), Declared::Sum(sum));
    }
    out
}

/// The functions of one action, or why it gets none.
#[allow(clippy::too_many_arguments)]
fn functions(
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
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
            .get(&type_ident(id))
            .ok_or_else(|| format!("entity {id} has no generated type"))
    };

    // Which params are rows of the function, by role.
    for param in &action.params {
        match param.role {
            ParamRole::New
            | ParamRole::Existing
            | ParamRole::Parent { .. }
            | ParamRole::Earlier { .. } => {}
            ParamRole::Triggered => {}
        }
    }
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
                .map(|(f, t)| {
                    (
                        values[usize::from(f.0)].clone(),
                        values[usize::from(t.0)].clone(),
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
    let uc = match &action.origin {
        ActionOrigin::UseCase { uc, .. } => *uc,
        ActionOrigin::Clock => unreachable!("only use case actions reach here"),
    };
    let usecase = &model.use_cases[uc];

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
            match function(
                model,
                ts,
                action,
                &picked,
                &chosen,
                declared,
                raises,
                site,
                &name,
                &usecase.id,
                steps,
                marks,
            ) {
                Ok(mut f) => {
                    f.doc
                        .insert(0, format!("{} ({}).", usecase.label, usecase.id));
                    for (p, open) in &options {
                        if open.len() < 2 || picked[*p].variant.is_none() {
                            continue;
                        }
                        let Some(r) = f.rows.iter().position(|row| row.param == *p) else {
                            continue;
                        };
                        let Some(Declared::Sum(sum)) = declared.get(&type_ident(&f.rows[r].entity))
                        else {
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

#[allow(clippy::too_many_arguments)]
fn function(
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    picked: &[&Source<'_>],
    arms: &BTreeMap<usize, Vec<(String, String)>>,
    declared: &BTreeMap<String, Declared<'_>>,
    raises: &[String],
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
    name: &str,
    usecase: &str,
    steps: Option<(u8, u8)>,
    marks: &mut Vec<super::ir::MarkDecl>,
) -> Result<FnDecl, Skip> {
    let unsupported = |why: &str| Skip::Action(why.to_string());
    let entity_id = |p: usize| {
        model.entities[ts.entity(action.params[p].entity).key]
            .id
            .clone()
    };

    // Row params: every param but a created one is an argument.
    let mut row_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut rows: Vec<RowParam> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    let fresh = |base: String, taken: &mut Vec<String>| {
        let mut n = base.clone();
        let mut i = 2;
        while taken.contains(&n) {
            n = format!("{base}_{i}");
            i += 1;
        }
        taken.push(n.clone());
        n
    };
    // The id type and id field of an entity, which a run mark holds.
    let id_of = |entity: &str| -> Option<(String, Field)> {
        let fields = match declared.get(&type_ident(entity))? {
            Declared::Record(r) => &r.fields,
            Declared::Sum(s) => &s.row_fields,
        };
        let want = format!("{}Id", type_ident(entity));
        fields
            .iter()
            .find(|f| f.ty == FieldType::Id(want.clone()))
            .map(|f| (want.clone(), f.clone()))
    };
    let mark_ty =
        |entity: &str, step: u8| format!("{}{}Mark{step}", type_ident(usecase), type_ident(entity));
    let mut marks_in: Vec<super::ir::MarkIn> = Vec::new();
    let mut mark_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (p, param) in action.params.iter().enumerate() {
        if let ParamRole::Earlier { step } = param.role {
            let entity = entity_id(p);
            let (id_ty, _) = id_of(&entity)
                .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
            let ty = mark_ty(&entity, step);
            marks.push(super::ir::MarkDecl {
                name: ty.clone(),
                usecase: usecase.to_string(),
                entity: entity.clone(),
                step,
                id_ty,
            });
            mark_of.insert(p, marks_in.len());
            marks_in.push(super::ir::MarkIn {
                name: fresh(format!("{}_mark", snake(&entity)), &mut taken),
                ty,
                param: p,
            });
        }
    }
    // The event a triggered use case runs on: the `Raised` guard names it and its row.
    let mut event_in: Option<super::ir::EventIn> = None;
    let mut raised_on: Vec<(usize, String)> = Vec::new();
    for guard in &action.guards {
        let GuardWhy::Raised { event } = guard.why else {
            continue;
        };
        let Pred::Cmp(
            Term::Slot {
                at: At::Param(p), ..
            },
            TsCmp::Eq,
            _,
        ) = &guard.pred
        else {
            return Err(unsupported("an event guard this phase does not print"));
        };
        let id = model.events[event].id.clone();
        if event_in.as_ref().is_some_and(|e| e.ty != type_ident(&id)) {
            return Err(unsupported("a step that two events order"));
        }
        let name = fresh(snake(&id), &mut taken);
        event_in.get_or_insert(super::ir::EventIn {
            name,
            ty: type_ident(&id),
        });
        let entity = entity_id(usize::from(*p));
        if id_of(&entity).is_none() {
            return Err(unsupported(
                "an event raised on an entity with no one-column key",
            ));
        }
        raised_on.push((usize::from(*p), format!("{}_id", snake(&entity))));
    }
    let event_field_of = |p: usize| {
        raised_on
            .iter()
            .find(|(q, _)| *q == p)
            .map(|(_, f)| f.clone())
    };
    for (p, param) in action.params.iter().enumerate() {
        if matches!(param.role, ParamRole::New | ParamRole::Earlier { .. }) {
            continue;
        }
        let base = match param.role {
            ParamRole::Parent { .. } => format!("parent_{}", snake(&entity_id(p))),
            ParamRole::Triggered => format!("raised_{}", snake(&entity_id(p))),
            _ => snake(&entity_id(p)),
        };
        row_of.insert(p, rows.len());
        rows.push(RowParam {
            name: fresh(base, &mut taken),
            param: p,
            state: picked[p].variant.and_then(|v| {
                let tag = match declared.get(&type_ident(&entity_id(p))) {
                    Some(Declared::Sum(sum)) => sum.tag.wire.clone(),
                    _ => return None,
                };
                Some((tag, v.value.wire.clone()))
            }),
            entity: entity_id(p),
            ty: picked[p].ty.clone(),
            fields: picked[p].fields.clone(),
            always_null: picked[p].always_null.clone(),
        });
    }

    // What the effects write, per (param, column).
    let mut writes: BTreeMap<(usize, String), Write> = BTreeMap::new();
    let mut created: Vec<usize> = Vec::new();
    let mut deleted: Vec<usize> = Vec::new();
    // The run mark each param is left at: `(param, step)`, step 0 for none.
    let mut runs: Vec<(usize, u8)> = Vec::new();
    // The rows each raise marks: `(event id, param)`.
    let mut raised_at: Vec<(String, usize)> = Vec::new();
    for effect in &action.effects {
        match effect {
            Effect::Create { param } => created.push(usize::from(*param)),
            Effect::Delete { param } => deleted.push(usize::from(*param)),
            Effect::Transition {
                var,
                param,
                arms: a,
                ..
            } => {
                let p = usize::from(*param);
                let VarKind::Column { column, .. } = &ts.var(*var).kind else {
                    return Err(unsupported("a transition of a var that is not a column"));
                };
                let is_tag = matches!(declared.get(&type_ident(&entity_id(p))),
                    Some(Declared::Sum(sum)) if sum.tag.wire == *column);
                if !is_tag {
                    let Domain::Enum(values) = &ts.var(*var).domain else {
                        return Err(unsupported("a transition on a column that is not an enum"));
                    };
                    let named = a
                        .iter()
                        .map(|(f, t)| {
                            let v = |x: Value| values[usize::from(x.0)].clone();
                            (v(*f), v(*t))
                        })
                        .collect();
                    writes.insert((p, column.clone()), Write::Switch(named));
                }
            }
            Effect::Assign {
                var, param, rhs, ..
            } => {
                let p = usize::from(*param);
                match (&ts.var(*var).kind, rhs) {
                    (VarKind::Column { column, .. }, Rhs::Lit(v)) => {
                        let w = literal(&ts.var(*var).domain, *v, ts)
                            .ok_or_else(|| unsupported("a value this phase does not print"))?;
                        writes.insert((p, column.clone()), w);
                    }
                    (VarKind::Owner { fk, .. }, Rhs::Param(q)) => {
                        let fk = model.foreign_key(*fk);
                        let [(child_col, parent_col)] = fk.columns.as_slice() else {
                            return Err(unsupported("a key over several columns"));
                        };
                        writes.insert(
                            (p, child_col.clone()),
                            Write::Parent(usize::from(*q), parent_col.clone()),
                        );
                    }
                    (VarKind::Run { .. }, Rhs::Lit(v)) => runs.push((p, v.0)),
                    (VarKind::Raised { event, .. }, Rhs::Lit(v)) if *v == Value::TRUE => {
                        raised_at.push((event.clone(), p));
                    }
                    (VarKind::Raised { .. }, _) => {}
                    (VarKind::Prop { .. }, _) => {
                        return Err(unsupported("drives a comparison proposition (phase 5)"))
                    }
                    _ => return Err(unsupported("an assignment this phase does not print")),
                }
            }
            Effect::Choose {
                var, param, values, ..
            } => {
                let VarKind::Column { column, .. } = &ts.var(*var).kind else {
                    return Err(unsupported("a choice of a value that is not a column"));
                };
                let domain = &ts.var(*var).domain;
                let all = values.len() == domain.size(ts.bounds);
                let w = match domain {
                    Domain::Nullable if values == &[Value::PRESENT] => Write::Input,
                    Domain::Enum(_) | Domain::Bool if all => Write::Input,
                    _ => return Err(unsupported("a choice among some values (phase 5)")),
                };
                writes.insert((usize::from(*param), column.clone()), w);
            }
            Effect::Tick => return Err(unsupported("reads `now` (phase 5)")),
        }
    }

    let mut inputs: Vec<FnInput> = Vec::new();
    let mut checks: Vec<ArgCheck> = Vec::new();
    let mut outputs: Vec<RowOut> = Vec::new();

    for (p, param) in action.params.iter().enumerate() {
        // A parent, an earlier step's row, and the row an event was raised on are read only.
        if matches!(
            param.role,
            ParamRole::Parent { .. } | ParamRole::Earlier { .. } | ParamRole::Triggered
        ) {
            continue;
        }
        let entity = entity_id(p);
        if deleted.contains(&p) {
            let r = row_of[&p];
            let id = declared_id(declared, &entity, &picked[p].fields);
            outputs.push(RowOut {
                name: format!("deleted_{}", snake(&entity)),
                param: p,
                kind: OutKind::Deleted { row: r, id },
            });
            continue;
        }
        // The target state and type.
        let (target_ty, tag, target_fields, target_null): (String, _, Vec<Field>, Vec<Field>) =
            match declared.get(&type_ident(&entity)) {
                Some(Declared::Sum(sum)) => {
                    let value = if param.role == ParamRole::New {
                        let start = match writes.get(&(p, sum.tag.wire.clone())) {
                            Some(Write::Lit(Lit::Enum { value, .. })) => value.wire.clone(),
                            _ => start_value(ts, action.params[p].entity, &sum.tag.wire)
                                .ok_or_else(|| unsupported("a created row with no start state"))?,
                        };
                        // A transition of the created row leaves its start state in the same
                        // step: the row ends where the arm from that state goes.
                        match arms.get(&p) {
                            Some(a) => a
                                .iter()
                                .find(|(f, _)| *f == start)
                                .map(|(_, t)| t.clone())
                                .ok_or(Skip::Combination)?,
                            None => start,
                        }
                    } else {
                        let from = picked[p]
                            .variant
                            .map(|v| v.value.wire.clone())
                            .unwrap_or_default();
                        match arms.get(&p) {
                            Some(a) => a
                                .iter()
                                .find(|(f, _)| *f == from)
                                .map(|(_, t)| t.clone())
                                .unwrap_or(from),
                            None => from,
                        }
                    };
                    let variant = sum
                        .variants
                        .iter()
                        .find(|v| v.value.wire == value)
                        .ok_or_else(|| unsupported("a state the tag column does not declare"))?;
                    (
                        variant.name.clone(),
                        Some((sum.tag.wire.clone(), variant.value.clone())),
                        variant.fields.clone(),
                        variant.always_null.clone(),
                    )
                }
                Some(Declared::Record(r)) => (r.name.clone(), None, r.fields.clone(), Vec::new()),
                None => return Err(unsupported("an entity with no generated type")),
            };

        let mut fields = Vec::new();
        for target in &target_fields {
            let write = writes.get(&(p, target.wire.clone()));
            let assigned = match write {
                Some(Write::Lit(lit)) => Assigned::Lit {
                    lit: typed(lit, target).ok_or_else(|| {
                        unsupported("an enum value on a column that is not an enum")
                    })?,
                    wrap: target.nullable,
                },
                Some(Write::Switch(named)) => {
                    let r = *row_of
                        .get(&p)
                        .ok_or_else(|| unsupported("a second lifecycle column of a created row"))?;
                    let source = rows[r]
                        .fields
                        .iter()
                        .find(|f| f.wire == target.wire)
                        .cloned();
                    let (Some(source), FieldType::Enum(enum_ty)) = (source, &target.ty) else {
                        return Err(unsupported("a second lifecycle column with no enum field"));
                    };
                    if source.nullable || target.nullable {
                        return Err(unsupported("a nullable second lifecycle column"));
                    }
                    let value = |w: &String| EnumValue {
                        ident: type_ident(w),
                        wire: w.clone(),
                        label: None,
                    };
                    Assigned::Switch {
                        row: r,
                        field: source,
                        enum_ty: enum_ty.clone(),
                        arms: named.iter().map(|(f, t)| (value(f), value(t))).collect(),
                    }
                }
                Some(Write::Null) if target.nullable => Assigned::Null,
                Some(Write::Null) => return Err(Skip::Combination),
                Some(Write::Input) => {
                    inputs.push(FnInput {
                        name: format!("{}_{}", snake(&entity), target.wire),
                        ty: target.ty.clone(),
                        nullable: false,
                        doc: format!("The value the use case writes to {entity}.{}.", target.wire),
                    });
                    Assigned::Input {
                        input: inputs.len() - 1,
                        wrap: target.nullable,
                    }
                }
                Some(Write::Parent(q, _))
                    if event_field_of(*q).is_some() && !row_of.contains_key(q) =>
                {
                    Assigned::EventId {
                        field: event_field_of(*q).expect("checked"),
                        param: *q,
                        wrap: target.nullable,
                    }
                }
                Some(Write::Parent(q, _)) if mark_of.contains_key(q) => Assigned::MarkId {
                    mark: mark_of[q],
                    wrap: target.nullable,
                },
                Some(Write::Parent(q, parent_col)) if !row_of.contains_key(q) => {
                    // The parent is created in the same run: its id is the caller's input,
                    // shared with the created parent row.
                    let parent = entity_id(*q);
                    let i = input_index(
                        &mut inputs,
                        format!("{}_{parent_col}", snake(&parent)),
                        target.ty.clone(),
                        format!("{parent}.{parent_col} of the created row."),
                    );
                    Assigned::Input {
                        input: i,
                        wrap: target.nullable,
                    }
                }
                Some(Write::Parent(q, parent_col)) => {
                    let r = row_of[q];
                    let field = rows[r]
                        .fields
                        .iter()
                        .find(|f| f.wire == *parent_col)
                        .cloned()
                        .ok_or_else(|| unsupported("a parent key column with no field"))?;
                    Assigned::ParentId {
                        row: r,
                        field,
                        wrap: target.nullable,
                    }
                }
                None if param.role == ParamRole::New => {
                    match start_literal(ts, action.params[p].entity, &target.wire, target) {
                        Some(Some(lit)) => Assigned::Lit {
                            lit,
                            wrap: target.nullable,
                        },
                        Some(None) if target.nullable => Assigned::Null,
                        Some(None) => return Err(Skip::Combination),
                        None => {
                            let i = input_index(
                                &mut inputs,
                                format!("{}_{}", snake(&entity), target.wire),
                                target.ty.clone(),
                                format!("{entity}.{} of the created row.", target.wire),
                            );
                            inputs[i].nullable = target.nullable;
                            Assigned::Input {
                                input: i,
                                wrap: false,
                            }
                        }
                    }
                }
                None => {
                    let r = row_of[&p];
                    let source = &rows[r];
                    match source.fields.iter().find(|f| f.wire == target.wire) {
                        Some(sf) => {
                            let adapt = match (sf.nullable, target.nullable) {
                                (a, b) if a == b => Adapt::Same,
                                (false, true) => Adapt::Wrap,
                                _ => Adapt::Unwrap(format!(
                                    "rdra-ish: {entity}.{} is null, but the verified model keeps it present after {usecase}",
                                    target.wire
                                )),
                            };
                            Assigned::From {
                                row: r,
                                field: sf.clone(),
                                adapt,
                            }
                        }
                        None if target.nullable => Assigned::From {
                            row: r,
                            field: target.clone(),
                            adapt: Adapt::AbsentNull,
                        },
                        None => return Err(Skip::Combination),
                    }
                }
            };
            fields.push((target.clone(), assigned));
        }
        // A column the target state keeps null: what the action leaves there must be null.
        for target in &target_null {
            match writes.get(&(p, target.wire.clone())) {
                None if param.role == ParamRole::New => {
                    if !matches!(
                        start_literal(ts, action.params[p].entity, &target.wire, target),
                        Some(None)
                    ) {
                        return Err(Skip::Combination);
                    }
                }
                None => {
                    let r = row_of[&p];
                    if let Some(sf) = rows[r].fields.iter().find(|f| f.wire == target.wire) {
                        if !sf.nullable {
                            return Err(Skip::Combination);
                        }
                        checks.push(ArgCheck::MustBeNull {
                            row: r,
                            field: sf.clone(),
                            message: format!(
                                "rdra-ish: {entity}.{} has a value, but the verified model keeps it null after {usecase}",
                                target.wire
                            ),
                        });
                    }
                }
                Some(Write::Null) => {}
                Some(_) => return Err(Skip::Combination),
            }
        }
        // Every effect on this param lands on a field of the target type.
        for (wp, column) in writes.keys() {
            if *wp == p
                && !target_fields.iter().any(|f| f.wire == *column)
                && !target_null.iter().any(|f| f.wire == *column)
                && tag.as_ref().is_none_or(|(t, _)| t != column)
            {
                return Err(unsupported("a write to a column with no field"));
            }
        }
        outputs.push(RowOut {
            name: snake(&entity),
            param: p,
            kind: OutKind::Row {
                ty: target_ty,
                tag,
                fields,
                always_null: target_null.iter().map(|f| f.wire.clone()).collect(),
            },
        });
    }

    // The successor's tag of each param with a transition: `(tag var, target value)`.
    let mut targets: BTreeMap<usize, (VarId, Value)> = BTreeMap::new();
    for effect in &action.effects {
        let Effect::Transition { var, param, .. } = effect else {
            continue;
        };
        let p = usize::from(*param);
        let Domain::Enum(values) = &ts.var(*var).domain else {
            continue;
        };
        let to = outputs.iter().find_map(|o| match &o.kind {
            OutKind::Row {
                tag: Some((tag, value)),
                ..
            } if o.param == p
                && matches!(&ts.var(*var).kind, VarKind::Column { column, .. } if column == tag) =>
            {
                Some(value.wire.clone())
            }
            _ => None,
        });
        if let Some(to) = to {
            if let Some(i) = values.iter().position(|v| *v == to) {
                targets.insert(p, (*var, Value(u8::try_from(i).expect("small domain"))));
            }
        }
    }

    // Guards.
    let mut conditions = Vec::new();
    for guard in &action.guards {
        match guard.why {
            // The run mark: step 1 needs none (an obligation), a later step takes the mark of
            // the step before as an argument.
            // The event: the handler takes it as an argument (`event_in`).
            GuardWhy::Run { .. } | GuardWhy::Raised { .. } => {}
            // The from-states: the source state type holds the tag's; a second lifecycle
            // column's are a condition.
            // On a created row the arm from its start state was taken when its target was.
            GuardWhy::TransitionFrom if reads_only_created(&guard.pred, action) => {}
            GuardWhy::TransitionFrom => {
                let bind = |at: At| match at {
                    At::Param(p) => {
                        let p = usize::from(p);
                        Some((*row_of.get(&p)?, picked[p].variant))
                    }
                    _ => None,
                };
                let e = simplify(expr(ts, &guard.pred, &rows, &bind).map_err(Skip::Action)?);
                match e {
                    Expr::True => {}
                    Expr::False => return Err(Skip::Combination),
                    e => conditions.push(CondGuard {
                        id: "TransitionFrom".to_string(),
                        label: "the use case moves the column only from these states".to_string(),
                        expr: e,
                        site: guard.site.span.as_ref().and_then(site),
                    }),
                }
            }
            // `.when` on an arm: the successor's tag is the function's target, so the guard
            // folds to the conditions of the arm taken.
            GuardWhy::ArmCondition { .. } => {
                let pred = fold_next(&guard.pred, &targets).ok_or_else(|| {
                    unsupported("a `.when` on a chosen value or a second lifecycle column")
                })?;
                let bind = |at: At| match at {
                    At::Param(p) => {
                        let p = usize::from(p);
                        Some((*row_of.get(&p)?, picked[p].variant))
                    }
                    _ => None,
                };
                let e = simplify(expr(ts, &pred, &rows, &bind).map_err(Skip::Action)?);
                match e {
                    Expr::True => {}
                    Expr::False => return Err(Skip::Combination),
                    e => {
                        let (id, label) = arm_conditions(model, ts, action, arms);
                        conditions.push(CondGuard {
                            id,
                            label,
                            expr: e,
                            site: guard.site.span.as_ref().and_then(site),
                        });
                    }
                }
            }
            GuardWhy::Owner => {
                let Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(c),
                    },
                    TsCmp::Eq,
                    Term::Index(At::Param(q)),
                ) = &guard.pred
                else {
                    return Err(unsupported("a link guard this phase does not print"));
                };
                let VarKind::Owner { fk, .. } = &ts.var(*var).kind else {
                    return Err(unsupported("a link guard on a var that is not a key"));
                };
                let fk = model.foreign_key(*fk);
                let [(child_col, parent_col)] = fk.columns.as_slice() else {
                    return Err(unsupported("a key over several columns"));
                };
                let (c, q) = (usize::from(*c), usize::from(*q));
                // A child of the row the event was raised on: its key is the event's id.
                if let (Some(&cr), Some(event_field)) = (row_of.get(&c), event_field_of(q)) {
                    if !row_of.contains_key(&q) {
                        let Some(child_field) = rows[cr]
                            .fields
                            .iter()
                            .find(|f| f.wire == *child_col)
                            .cloned()
                        else {
                            return Err(unsupported("a link guard on a column with no field"));
                        };
                        checks.push(ArgCheck::EventRow {
                            row: cr,
                            field: child_field,
                            event_field,
                            message: format!(
                                "rdra-ish: the {} row is not a child of the row the event was raised on",
                                rows[cr].entity
                            ),
                        });
                        continue;
                    }
                }
                let (Some(&cr), Some(&qr)) = (row_of.get(&c), row_of.get(&q)) else {
                    return Err(unsupported("a link guard on a created row"));
                };
                let child_field = rows[cr]
                    .fields
                    .iter()
                    .find(|f| f.wire == *child_col)
                    .cloned();
                let parent_field = rows[qr]
                    .fields
                    .iter()
                    .find(|f| f.wire == *parent_col)
                    .cloned();
                let (Some(child_field), Some(parent_field)) = (child_field, parent_field) else {
                    return Err(unsupported("a link guard on a column with no field"));
                };
                checks.push(ArgCheck::Link {
                    child: cr,
                    child_field,
                    parent: qr,
                    parent_field,
                    message: format!(
                        "rdra-ish: the {} row is not a child of the {} row",
                        rows[cr].entity, rows[qr].entity
                    ),
                });
            }
            GuardWhy::Condition { cond, .. } => {
                let bind = |at: At| match at {
                    At::Param(p) => {
                        let p = usize::from(p);
                        Some((*row_of.get(&p)?, picked[p].variant))
                    }
                    _ => None,
                };
                let expr = expr(ts, &guard.pred, &rows, &bind).map_err(Skip::Action)?;
                let expr = simplify(expr);
                match expr {
                    Expr::True => {}
                    Expr::False => return Err(Skip::Combination),
                    expr => {
                        let c = &model.conditions[cond];
                        conditions.push(CondGuard {
                            id: c.id.clone(),
                            label: c.label.clone(),
                            expr,
                            site: guard.site.span.as_ref().and_then(site),
                        });
                    }
                }
            }
            _ => return Err(unsupported("a guard this phase does not print")),
        }
    }

    let doc = vec![format!(
        "One run of the use case, from the states of the arguments' types. It requires what `{usecase}` requires and writes what it writes; the values the model leaves open are the inputs."
    )];
    // A row the event was raised on is the event's row.
    for (p, event_field) in &raised_on {
        if let Some(&r) = row_of.get(p) {
            let (_, id_field) = id_of(&rows[r].entity).ok_or_else(|| {
                unsupported("an event raised on an entity with no one-column key")
            })?;
            checks.push(ArgCheck::EventRow {
                row: r,
                field: id_field,
                event_field: event_field.clone(),
                message: format!(
                    "rdra-ish: the {} row is not the row the event was raised on",
                    rows[r].entity
                ),
            });
        }
    }

    // The events the use case raises, with the ids of the rows each raise marks.
    let mut raised: Vec<super::ir::RaisedEvent> = Vec::new();
    for event in raises {
        let mut ids = Vec::new();
        for field in event_fields(model, ts, declared, event) {
            let at = raised_at
                .iter()
                .find(|(e, p)| e == event && entity_id(*p) == field.entity)
                .map(|(_, p)| *p);
            let Some(p) = at else {
                return Err(unsupported("raises an event without marking its row"));
            };
            let (_, id_field) = id_of(&field.entity).ok_or_else(|| {
                unsupported("an event raised on an entity with no one-column key")
            })?;
            let source = if let Some(m) = mark_of.get(&p) {
                super::ir::MarkSource::Mark(*m)
            } else if let Some(r) = row_of.get(&p) {
                super::ir::MarkSource::Row {
                    row: *r,
                    field: id_field.clone(),
                }
            } else if action.params[p].role == ParamRole::New {
                let name = format!("{}_{}", snake(&field.entity), id_field.wire);
                let i = inputs.iter().position(|x| x.name == name).ok_or_else(|| {
                    unsupported("an event raised on a created row with no id input")
                })?;
                super::ir::MarkSource::Input(i)
            } else {
                return Err(unsupported("raises an event on the row of another event"));
            };
            ids.push((field, source));
        }
        raised.push(super::ir::RaisedEvent {
            ty: type_ident(event),
            ids,
        });
    }

    // The marks the step leaves for the next one.
    let mut marks_out: Vec<super::ir::MarkOut> = Vec::new();
    for (p, step) in runs {
        if step == 0 {
            continue; // the run ends on this row: its mark is consumed
        }
        let entity = entity_id(p);
        let (id_ty, id_field) = id_of(&entity)
            .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
        let source = if let Some(m) = mark_of.get(&p) {
            super::ir::MarkSource::Mark(*m)
        } else if let Some(r) = row_of.get(&p) {
            super::ir::MarkSource::Row {
                row: *r,
                field: id_field.clone(),
            }
        } else {
            let name = format!("{}_{}", snake(&entity), id_field.wire);
            let i = inputs
                .iter()
                .position(|x| x.name == name)
                .ok_or_else(|| unsupported("a run mark on a created row with no id input"))?;
            super::ir::MarkSource::Input(i)
        };
        let ty = mark_ty(&entity, step);
        marks.push(super::ir::MarkDecl {
            name: ty.clone(),
            usecase: usecase.to_string(),
            entity: entity.clone(),
            step,
            id_ty,
        });
        marks_out.push(super::ir::MarkOut {
            name: format!("{}_mark", snake(&entity)),
            ty,
            param: p,
            step,
            source,
        });
    }
    let mut doc = doc;
    if let Some((i, k)) = steps {
        doc.push(format!(
            "Transaction {i} of {k} of {usecase}: a separate transaction. {}{}",
            if i > 1 {
                "It continues the run its mark arguments name, on the rows as they are now. "
            } else {
                "Call it only on rows that hold no mark of this use case. "
            },
            if i < k {
                "Store the marks it returns with their rows, in the same transaction."
            } else {
                "It ends the run: delete the marks it took, in the same transaction."
            }
        ));
    }
    Ok(FnDecl {
        name: name.to_string(),
        usecase: usecase.to_string(),
        action: action.name.clone(),
        step: steps,
        chosen: Vec::new(),
        marks_in,
        marks_out,
        doc,
        rows,
        inputs,
        checks,
        conditions,
        outputs,
        raises: raised,
        event_in,
        result: format!("{name}Result"),
    })
}

/// What an effect writes to a column.
#[derive(Clone, Debug)]
enum Write {
    Lit(Lit),
    Null,
    /// A second lifecycle column: `(from, to)` per arm.
    Switch(Vec<(String, String)>),
    /// A value the model leaves open: the caller passes it.
    Input,
    /// The key of the parent param, by its key column.
    Parent(usize, String),
}

/// The write of a literal value of `domain`.
fn literal(domain: &Domain, value: Value, ts: &TransitionSystem) -> Option<Write> {
    Some(match domain {
        Domain::Enum(values) => {
            let wire = values.get(usize::from(value.0))?.clone();
            Write::Lit(Lit::Enum {
                ty: String::new(),
                value: EnumValue {
                    ident: type_ident(&wire),
                    wire,
                    label: None,
                },
            })
        }
        Domain::Bool => Write::Lit(Lit::Bool(value == Value::TRUE)),
        Domain::Nullable if value == Value::NULL => Write::Null,
        Domain::Nullable => Write::Input,
        Domain::Int | Domain::NullableInt => match domain.number(value) {
            Some(n) => Write::Lit(Lit::Int(n)),
            None => Write::Null,
        },
        Domain::Existence | Domain::Instance { .. } => {
            let _ = ts;
            return None;
        }
    })
}

/// The start value of a created row's column, by its var: `Some(Some(lit))`, `Some(None)` for
/// null, `None` when no var holds the column (the caller passes it).
fn start_literal(
    ts: &TransitionSystem,
    e: EntId,
    column: &str,
    target: &Field,
) -> Option<Option<Lit>> {
    let var = ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Column { entity, column: c, .. } if *entity == e && c == column)
    })?;
    let decl = ts.var(var);
    match literal(&decl.domain, decl.base, ts)? {
        Write::Lit(Lit::Enum { value, .. }) => {
            let FieldType::Enum(ty) = &target.ty else {
                return None;
            };
            Some(Some(Lit::Enum {
                ty: ty.clone(),
                value,
            }))
        }
        Write::Lit(lit) => Some(Some(lit)),
        Write::Null => Some(None),
        Write::Input | Write::Parent(..) | Write::Switch(_) => None,
    }
}

/// The start value of a created row's tag.
fn start_value(ts: &TransitionSystem, e: EntId, tag: &str) -> Option<String> {
    let var = ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Column { entity, column, .. } if *entity == e && column == tag)
    })?;
    let decl = ts.var(var);
    match &decl.domain {
        Domain::Enum(values) => values.get(usize::from(decl.base.0)).cloned(),
        _ => None,
    }
}

/// The id type and id field of an entity's type, when it has one.
fn declared_id(
    declared: &BTreeMap<String, Declared<'_>>,
    entity: &str,
    fields: &[Field],
) -> Option<(String, Field)> {
    let _ = declared.get(&type_ident(entity))?;
    fields.iter().find_map(|f| match &f.ty {
        FieldType::Id(ty) if *ty == format!("{}Id", type_ident(entity)) => {
            Some((ty.clone(), f.clone()))
        }
        _ => None,
    })
}

/// Where a slot reference of a predicate points: a row, and its state when it has one.
pub(super) type Bind<'a, 'd> = &'a dyn Fn(At) -> Option<(usize, Option<&'d VariantDecl>)>;

/// A predicate on the rows `bind` names.
pub(super) fn expr(
    ts: &TransitionSystem,
    pred: &Pred,
    rows: &[RowParam],
    bind: Bind<'_, '_>,
) -> Result<Expr, String> {
    let rec = |p: &Pred| expr(ts, p, rows, bind);
    Ok(match pred {
        Pred::True => Expr::True,
        Pred::False => Expr::False,
        Pred::Not(p) => Expr::Not(Box::new(rec(p)?)),
        Pred::And(ps) => Expr::And(ps.iter().map(rec).collect::<Result<_, _>>()?),
        Pred::Or(ps) => Expr::Or(ps.iter().map(rec).collect::<Result<_, _>>()?),
        Pred::In(term, values) => Expr::Or(
            values
                .iter()
                .map(|v| atom(ts, term, TsCmp::Eq, v, rows, bind))
                .collect::<Result<_, _>>()?,
        ),
        Pred::Cmp(a, op, b) => atom(ts, a, *op, b, rows, bind)?,
        Pred::Forall { .. } | Pred::Exists { .. } => {
            return Err("a condition over related rows, `.has` or `.none` (phase 5)".into())
        }
    })
}

fn atom(
    ts: &TransitionSystem,
    lhs: &Term,
    op: TsCmp,
    rhs: &Term,
    rows: &[RowParam],
    bind: Bind<'_, '_>,
) -> Result<Expr, String> {
    let unsupported = || "a condition atom this phase does not print".to_string();
    let Term::Slot { var, at } = lhs else {
        return Err(unsupported());
    };
    let VarKind::Column { column, .. } = &ts.var(*var).kind else {
        return Err("a condition on a comparison proposition (phase 5)".into());
    };
    let (r, state) = bind(*at).ok_or_else(unsupported)?;
    let row = &rows[r];
    let domain = &ts.var(*var).domain;
    let negate = |e: Expr| match op {
        TsCmp::Eq => Ok(e),
        TsCmp::Ne => Ok(Expr::Not(Box::new(e))),
        _ => Err(unsupported()),
    };
    match (domain, rhs) {
        (Domain::Enum(values), Term::Lit(v)) => {
            let wire = values.get(usize::from(v.0)).ok_or_else(unsupported)?;
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
        (Domain::Bool, Term::Lit(v)) => {
            let field = row
                .fields
                .iter()
                .find(|f| f.wire == *column)
                .ok_or_else(unsupported)?;
            negate(Expr::BoolEq {
                row: r,
                field: field.clone(),
                value: *v == Value::TRUE,
            })
        }
        (Domain::Nullable, Term::Lit(v)) => {
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
            if field.ty != FieldType::Scalar(super::ir::Scalar::Int) || field.nullable {
                return Err("a comparison on a column that is not a plain Int".into());
            }
            let value = match rhs {
                Term::Int(n) => *n,
                Term::Lit(v) => domain.number(*v).ok_or_else(unsupported)?,
                _ => unreachable!(),
            };
            let op = match op {
                TsCmp::Eq => CmpOp::Eq,
                TsCmp::Ne => CmpOp::Ne,
                TsCmp::Lt => CmpOp::Lt,
                TsCmp::Le => CmpOp::Le,
                TsCmp::Gt => CmpOp::Gt,
                TsCmp::Ge => CmpOp::Ge,
            };
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

/// Folds the constant parts of an expression.
pub(super) fn simplify(e: Expr) -> Expr {
    match e {
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

/// A literal typed for its target field: an enum value takes the field's enum type.
fn typed(lit: &Lit, target: &Field) -> Option<Lit> {
    match (lit, &target.ty) {
        (Lit::Enum { value, .. }, FieldType::Enum(ty)) => Some(Lit::Enum {
            ty: ty.clone(),
            value: value.clone(),
        }),
        (Lit::Enum { .. }, _) => None,
        (other, _) => Some(other.clone()),
    }
}

/// The input named `name`, added when the function has none by that name yet.
fn input_index(inputs: &mut Vec<FnInput>, name: String, ty: FieldType, doc: String) -> usize {
    if let Some(i) = inputs.iter().position(|x| x.name == name) {
        return i;
    }
    inputs.push(FnInput {
        name,
        ty,
        nullable: false,
        doc,
    });
    inputs.len() - 1
}

/// One dispatcher per action whose functions take a state type or are several.
fn dispatchers(fns: &[FnDecl], domain: &DomainModel) -> Vec<DispatchDecl> {
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
            for (r, tag_enum, target) in &f.chosen {
                let name = format!("{}_to", f.rows[*r].name);
                let i = match choices.iter().position(|(n, _)| *n == name) {
                    Some(i) => i,
                    None => {
                        choices.push((name, tag_enum.clone()));
                        choices.len() - 1
                    }
                };
                choice.push((i, target.clone()));
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

/// The payload of an event: the id of each row its raise marks, by the system's `Raised`
/// vars. An event no use case raise orders anything by has none.
fn event_fields(
    model: &SemanticModel,
    ts: &TransitionSystem,
    declared: &BTreeMap<String, Declared<'_>>,
    event: &str,
) -> Vec<super::ir::EventField> {
    let _ = model;
    let mut out: Vec<super::ir::EventField> = Vec::new();
    for v in ts.var_ids() {
        let VarKind::Raised { entity, event: ev } = &ts.var(v).kind else {
            continue;
        };
        if ev != event {
            continue;
        }
        let id = ts.entity(*entity).id.clone();
        let fields = match declared.get(&type_ident(&id)) {
            Some(Declared::Record(r)) => &r.fields,
            Some(Declared::Sum(s)) => &s.row_fields,
            None => continue,
        };
        let want = format!("{}Id", type_ident(&id));
        if !fields.iter().any(|f| f.ty == FieldType::Id(want.clone())) {
            continue;
        }
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

/// `pred` with each read of a param's successor tag replaced by whether the function's target
/// is that value; `None` when it reads some other var of a successor.
fn fold_next(pred: &Pred, targets: &BTreeMap<usize, (VarId, Value)>) -> Option<Pred> {
    let rec = |p: &Pred| fold_next(p, targets);
    // The target a read of a successor's tag stands for; `None` for any other read of a
    // successor, which this phase does not fold.
    let next_tag = |t: &Term| -> Option<Value> {
        let Term::Slot {
            var,
            at: At::Next(p),
        } = t
        else {
            return None;
        };
        let (tag, to) = targets.get(&usize::from(*p))?;
        (var == tag).then_some(*to)
    };
    let reads_next = |t: &Term| {
        matches!(
            t,
            Term::Slot {
                at: At::Next(_),
                ..
            } | Term::Index(At::Next(_))
        )
    };
    let truth = |b: bool| if b { Pred::True } else { Pred::False };
    Some(match pred {
        Pred::Cmp(a, op, b) if reads_next(a) || reads_next(b) => {
            let (Some(to), Term::Lit(v), TsCmp::Eq) = (next_tag(a), b, op) else {
                return None;
            };
            truth(to == *v)
        }
        Pred::In(a, vs) if reads_next(a) => truth(vs.contains(&Term::Lit(next_tag(a)?))),
        Pred::Not(p) => Pred::Not(Box::new(rec(p)?)),
        Pred::And(ps) => Pred::And(ps.iter().map(rec).collect::<Option<_>>()?),
        Pred::Or(ps) => Pred::Or(ps.iter().map(rec).collect::<Option<_>>()?),
        Pred::Forall { entity, body } => Pred::Forall {
            entity: *entity,
            body: Box::new(rec(body)?),
        },
        Pred::Exists { entity, body } => Pred::Exists {
            entity: *entity,
            body: Box::new(rec(body)?),
        },
        other => other.clone(),
    })
}

/// The id and label of the `.when` conditions of the arms the function takes.
fn arm_conditions(
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    chosen: &BTreeMap<usize, Vec<(String, String)>>,
) -> (String, String) {
    let mut ids: Vec<String> = Vec::new();
    for effect in &action.effects {
        let Effect::Transition {
            var,
            param,
            arms,
            conds,
            ..
        } = effect
        else {
            continue;
        };
        let Some(taken) = chosen.get(&usize::from(*param)) else {
            continue;
        };
        let Domain::Enum(values) = &ts.var(*var).domain else {
            continue;
        };
        let name = |v: Value| values[usize::from(v.0)].clone();
        for ((f, t), arm) in arms.iter().zip(conds) {
            if !taken.contains(&(name(*f), name(*t))) {
                continue;
            }
            for (_, id) in arm {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
    }
    let labels: Vec<String> = ids
        .iter()
        .filter_map(|id| {
            model
                .conditions
                .values()
                .find(|c| &c.id == id)
                .map(|c| c.label.clone())
        })
        .collect();
    (ids.join(" or "), labels.join(" / "))
}

/// Whether every slot `pred` reads is a param the action creates.
fn reads_only_created(pred: &Pred, action: &Action) -> bool {
    let created = |t: &Term| match t {
        Term::Slot {
            at: At::Param(p), ..
        }
        | Term::Index(At::Param(p)) => action.params[usize::from(*p)].role == ParamRole::New,
        Term::Slot { .. } | Term::Index(_) => false,
        _ => true,
    };
    match pred {
        Pred::True | Pred::False => true,
        Pred::Cmp(a, _, b) => created(a) && created(b),
        Pred::In(a, vs) => created(a) && vs.iter().all(created),
        Pred::Not(p) => reads_only_created(p, action),
        Pred::And(ps) | Pred::Or(ps) => ps.iter().all(|p| reads_only_created(p, action)),
        Pred::Forall { .. } | Pred::Exists { .. } => false,
    }
}
