//! Lowering one action combination to a function: its inputs, rows, assignments, guards, and
//! the event payload it raises.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn function(
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
    marks: &mut Vec<crate::codegen::ir::MarkDecl>,
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
        let fields = match declared.get(entity)? {
            Declared::Record(r) => &r.fields,
            Declared::Sum(s) => &s.row_fields,
        };
        declared_id(entity, fields)
    };
    let mark_ty =
        |entity: &str, step: u8| format!("{}{}Mark{step}", type_ident(usecase), type_ident(entity));
    let mut marks_in: Vec<crate::codegen::ir::MarkIn> = Vec::new();
    let mut mark_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (p, param) in action.params.iter().enumerate() {
        if let ParamRole::Earlier { step } = param.role {
            let entity = entity_id(p);
            let (id_ty, _) = id_of(&entity)
                .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
            let ty = mark_ty(&entity, step);
            marks.push(crate::codegen::ir::MarkDecl {
                name: ty.clone(),
                usecase: usecase.to_string(),
                entity: entity.clone(),
                step,
                id_ty,
            });
            mark_of.insert(p, marks_in.len());
            marks_in.push(crate::codegen::ir::MarkIn {
                name: fresh(format!("{}_mark", snake(&entity)), &mut taken),
                ty,
                param: p,
            });
        }
    }
    // The event a triggered use case runs on: the `Raised` guard names it and its row.
    let mut event_in: Option<crate::codegen::ir::EventIn> = None;
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
        if event_in.as_ref().is_some_and(|e| e.event != id) {
            return Err(unsupported("a step that two events order"));
        }
        let name = fresh(snake(&id), &mut taken);
        event_in.get_or_insert(crate::codegen::ir::EventIn {
            name,
            ty: type_ident(&id),
            event: id.clone(),
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
            param: Some(p),
            state: picked[p].variant.and_then(|v| {
                let tag = match declared.get(&entity_id(p)) {
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
                let is_tag = matches!(declared.get(&entity_id(p)),
                    Some(Declared::Sum(sum)) if sum.tag.wire == *column);
                if !is_tag {
                    let Domain::Enum(values) = &ts.var(*var).domain else {
                        return Err(unsupported("a transition on a column that is not an enum"));
                    };
                    let named = a
                        .iter()
                        .map(|arm| {
                            let v = |x: Value| values[usize::from(x.0)].clone();
                            (v(arm.from), v(arm.to))
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
                        let w = literal(&ts.var(*var).domain, *v)
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
                    Domain::Nullable if matches!(&values[..], [c] if c.value == Value::PRESENT) => {
                        Write::Input
                    }
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
            let id = declared_id(&entity, &picked[p].fields);
            outputs.push(RowOut {
                name: format!("deleted_{}", snake(&entity)),
                param: p,
                kind: OutKind::Deleted { row: r, id },
            });
            continue;
        }
        // The target state and type.
        let (target_ty, tag, target_fields, target_null): (String, _, Vec<Field>, Vec<Field>) =
            match declared.get(&entity) {
                Some(Declared::Sum(sum)) => {
                    let value = if param.role == ParamRole::New {
                        let start = match writes.get(&(p, sum.tag.wire.clone())) {
                            Some(Write::Enum(value)) => value.wire.clone(),
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
                    lit: lit.clone(),
                    wrap: target.nullable,
                },
                Some(Write::Enum(value)) => Assigned::Lit {
                    lit: enum_lit(value.clone(), target).ok_or_else(|| {
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

    // Guards. A `.has` / `.none` ranges over a list of related rows, an input.
    let row_type = |entity: &str| -> Option<(String, Vec<Field>)> {
        match declared.get(entity)? {
            Declared::Record(r) => Some((r.name.clone(), r.fields.clone())),
            Declared::Sum(s) => Some((s.row.clone(), s.row_fields.clone())),
        }
    };
    let mut quant = Quant {
        model,
        row_type: &row_type,
        base: 0,
        n_rows: rows.len(),
        elems: Vec::new(),
        stack: Vec::new(),
        lists: Vec::new(),
    };
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
                let e = simplify(
                    expr(ts, &guard.pred, &rows, &bind, &mut Some(&mut quant))
                        .map_err(Skip::Action)?,
                );
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
                let e = simplify(
                    expr(ts, &pred, &rows, &bind, &mut Some(&mut quant)).map_err(Skip::Action)?,
                );
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
                let expr = expr(ts, &guard.pred, &rows, &bind, &mut Some(&mut quant))
                    .map_err(Skip::Action)?;
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

    // The lists the quantifiers range over are inputs: every row of the entity, which the
    // function filters by key.
    let mut list_input: Vec<usize> = Vec::new();
    for (name, ty) in &quant.lists {
        let entity = ty.trim_end_matches("Row");
        list_input.push(input_index(
            &mut inputs,
            name.clone(),
            FieldType::Rows(ty.clone()),
            format!("Every present {entity} row: the function keeps those its keys link."),
        ));
    }
    for c in &mut conditions {
        remap_lists(&mut c.expr, &list_input);
    }
    let elems = quant.elems;
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
    let mut raised: Vec<crate::codegen::ir::RaisedEvent> = Vec::new();
    for event in raises {
        let mut ids = Vec::new();
        for field in event_fields(ts, declared, event) {
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
                crate::codegen::ir::MarkSource::Mark(*m)
            } else if let Some(r) = row_of.get(&p) {
                crate::codegen::ir::MarkSource::Row {
                    row: *r,
                    field: id_field.clone(),
                }
            } else if action.params[p].role == ParamRole::New {
                let name = format!("{}_{}", snake(&field.entity), id_field.wire);
                let i = inputs.iter().position(|x| x.name == name).ok_or_else(|| {
                    unsupported("an event raised on a created row with no id input")
                })?;
                crate::codegen::ir::MarkSource::Input(i)
            } else {
                return Err(unsupported("raises an event on the row of another event"));
            };
            ids.push((field, source));
        }
        raised.push(crate::codegen::ir::RaisedEvent {
            ty: type_ident(event),
            ids,
        });
    }

    // The marks the step leaves for the next one.
    let mut marks_out: Vec<crate::codegen::ir::MarkOut> = Vec::new();
    for (p, step) in runs {
        if step == 0 {
            continue; // the run ends on this row: its mark is consumed
        }
        let entity = entity_id(p);
        let (id_ty, id_field) = id_of(&entity)
            .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
        let source = if let Some(m) = mark_of.get(&p) {
            crate::codegen::ir::MarkSource::Mark(*m)
        } else if let Some(r) = row_of.get(&p) {
            crate::codegen::ir::MarkSource::Row {
                row: *r,
                field: id_field.clone(),
            }
        } else {
            let name = format!("{}_{}", snake(&entity), id_field.wire);
            let i = inputs
                .iter()
                .position(|x| x.name == name)
                .ok_or_else(|| unsupported("a run mark on a created row with no id input"))?;
            crate::codegen::ir::MarkSource::Input(i)
        };
        let ty = mark_ty(&entity, step);
        marks.push(crate::codegen::ir::MarkDecl {
            name: ty.clone(),
            usecase: usecase.to_string(),
            entity: entity.clone(),
            step,
            id_ty,
        });
        marks_out.push(crate::codegen::ir::MarkOut {
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
        elems,
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
