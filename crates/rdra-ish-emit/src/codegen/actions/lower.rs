//! Lowering one action combination to a function: its inputs, rows, assignments, guards, and
//! the event payload it raises.

use rdra_ish_core::lifecycle::{Guard, Param};

use super::*;
use crate::codegen::ir::{EventIn, MarkDecl, MarkIn, MarkOut, MarkSource, RaisedEvent};

/// What every combination of one action shares.
pub(super) struct ActionCx<'a> {
    pub(super) model: &'a SemanticModel,
    pub(super) ts: &'a TransitionSystem,
    pub(super) action: &'a Action,
    pub(super) declared: &'a BTreeMap<String, Declared<'a>>,
    pub(super) raises: &'a [String],
    pub(super) site: &'a dyn Fn(&LocatedSpan) -> Option<String>,
    pub(super) usecase: &'a str,
    pub(super) steps: Option<(u8, u8)>,
}

/// Lowers one combination of source states (`picked`) and chosen arms to the function `name`.
/// Declares the run marks it takes and leaves in `marks`.
pub(super) fn function(
    cx: &ActionCx<'_>,
    picked: &[&Source<'_>],
    arms: &BTreeMap<usize, Vec<(String, String)>>,
    name: &str,
    marks: &mut Vec<MarkDecl>,
) -> Result<FnDecl, Skip> {
    // The fresh-name suffixes follow this order: marks, then the event, then the rows.
    let mut taken: Vec<String> = Vec::new();
    let (marks_in, mark_of) = cx.marks_in(marks, &mut taken)?;
    let (event_in, raised_on) = cx.event_in(&mut taken)?;
    let (rows, row_of) = cx.row_params(picked, &mut taken);
    let f = Frame {
        cx,
        picked,
        arms,
        rows,
        row_of,
        mark_of,
        raised_on,
    };
    let effects = cx.effects()?;

    let mut inputs: Vec<FnInput> = Vec::new();
    let mut checks: Vec<ArgCheck> = Vec::new();
    let outputs = f.outputs(&effects, &mut inputs, &mut checks)?;
    let targets = cx.targets(&outputs);
    let (conditions, elems) = f.conditions(&targets, &mut inputs, &mut checks)?;
    f.raised_on_checks(&mut checks)?;
    let raised = f.raised(&effects.raised_at, &inputs)?;
    let marks_out = f.marks_out(&effects.runs, &inputs, marks)?;
    Ok(FnDecl {
        name: name.to_string(),
        usecase: cx.usecase.to_string(),
        action: cx.action.name.clone(),
        step: cx.steps,
        chosen: Vec::new(),
        elems,
        marks_in,
        marks_out,
        doc: cx.doc(),
        rows: f.rows,
        inputs,
        checks,
        conditions,
        outputs,
        raises: raised,
        event_in,
        result: format!("{name}Result"),
    })
}

fn unsupported(why: &str) -> Skip {
    Skip::Action(why.to_string())
}

/// `base`, or `base_2`, `base_3`, ... for the first not yet taken; marks it taken.
fn fresh(base: String, taken: &mut Vec<String>) -> String {
    let mut n = base.clone();
    let mut i = 2;
    while taken.contains(&n) {
        n = format!("{base}_{i}");
        i += 1;
    }
    taken.push(n.clone());
    n
}

/// What the effects write, per (param, column), and the marks they leave.
struct Effects {
    writes: BTreeMap<(usize, String), Write>,
    deleted: Vec<usize>,
    /// The run mark each param is left at: `(param, step)`, step 0 for none.
    runs: Vec<(usize, u8)>,
    /// The rows each raise marks: `(event id, param)`.
    raised_at: Vec<(String, usize)>,
}

/// `(param, event field holding its id)` per row an event was raised on.
type RaisedOn = Vec<(usize, String)>;

/// The state and type a written row ends in.
struct Target {
    ty: String,
    tag: Option<(String, EnumValue)>,
    fields: Vec<Field>,
    /// The columns the state keeps null.
    null: Vec<Field>,
}

impl ActionCx<'_> {
    fn entity_id(&self, p: usize) -> String {
        self.model.entities[self.ts.entity(self.action.params[p].entity).key]
            .id
            .clone()
    }

    /// The id type and id field of an entity, which a run mark holds.
    fn id_of(&self, entity: &str) -> Option<(String, Field)> {
        let fields = match self.declared.get(entity)? {
            Declared::Record(r) => &r.fields,
            Declared::Sum(s) => &s.row_fields,
        };
        declared_id(entity, fields)
    }

    /// Declares the mark of `entity` at `step` in `marks`; returns its type.
    fn declare_mark(
        &self,
        marks: &mut Vec<MarkDecl>,
        entity: &str,
        step: u8,
        id_ty: String,
    ) -> String {
        let ty = format!(
            "{}{}Mark{step}",
            type_ident(self.usecase),
            type_ident(entity)
        );
        marks.push(MarkDecl {
            name: ty.clone(),
            usecase: self.usecase.to_string(),
            entity: entity.to_string(),
            step,
            id_ty,
        });
        ty
    }

    /// The marks of earlier steps, each an argument; `mark_of` maps a param to its mark.
    fn marks_in(
        &self,
        marks: &mut Vec<MarkDecl>,
        taken: &mut Vec<String>,
    ) -> Result<(Vec<MarkIn>, BTreeMap<usize, usize>), Skip> {
        let mut marks_in: Vec<MarkIn> = Vec::new();
        let mut mark_of: BTreeMap<usize, usize> = BTreeMap::new();
        for (p, param) in self.action.params.iter().enumerate() {
            if let ParamRole::Earlier { step } = param.role {
                let entity = self.entity_id(p);
                let (id_ty, _) = self
                    .id_of(&entity)
                    .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
                let ty = self.declare_mark(marks, &entity, step, id_ty);
                mark_of.insert(p, marks_in.len());
                marks_in.push(MarkIn {
                    name: fresh(format!("{}_mark", snake(&entity)), taken),
                    ty,
                    param: p,
                });
            }
        }
        Ok((marks_in, mark_of))
    }

    /// The event a triggered use case runs on: the `Raised` guard names it and its row.
    /// `raised_on` pairs each such row's param with the event field holding its id.
    fn event_in(&self, taken: &mut Vec<String>) -> Result<(Option<EventIn>, RaisedOn), Skip> {
        let mut event_in: Option<EventIn> = None;
        let mut raised_on: RaisedOn = Vec::new();
        for guard in &self.action.guards {
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
            let id = self.model.events[event].id.clone();
            if event_in.as_ref().is_some_and(|e| e.event != id) {
                return Err(unsupported("a step that two events order"));
            }
            let name = fresh(snake(&id), taken);
            event_in.get_or_insert(EventIn {
                name,
                ty: type_ident(&id),
                event: id.clone(),
            });
            let entity = self.entity_id(usize::from(*p));
            if self.id_of(&entity).is_none() {
                return Err(unsupported(
                    "an event raised on an entity with no one-column key",
                ));
            }
            raised_on.push((usize::from(*p), format!("{}_id", snake(&entity))));
        }
        Ok((event_in, raised_on))
    }

    /// Row params: every param but a created one or an earlier step's is an argument.
    fn row_params(
        &self,
        picked: &[&Source<'_>],
        taken: &mut Vec<String>,
    ) -> (Vec<RowParam>, BTreeMap<usize, usize>) {
        let mut row_of: BTreeMap<usize, usize> = BTreeMap::new();
        let mut rows: Vec<RowParam> = Vec::new();
        for (p, param) in self.action.params.iter().enumerate() {
            if matches!(param.role, ParamRole::New | ParamRole::Earlier { .. }) {
                continue;
            }
            let base = match param.role {
                ParamRole::Parent { .. } => format!("parent_{}", snake(&self.entity_id(p))),
                ParamRole::Triggered => format!("raised_{}", snake(&self.entity_id(p))),
                _ => snake(&self.entity_id(p)),
            };
            row_of.insert(p, rows.len());
            rows.push(RowParam {
                name: fresh(base, taken),
                param: Some(p),
                state: picked[p].variant.and_then(|v| {
                    let tag = match self.declared.get(&self.entity_id(p)) {
                        Some(Declared::Sum(sum)) => sum.tag.wire.clone(),
                        _ => return None,
                    };
                    Some((tag, v.value.wire.clone()))
                }),
                entity: self.entity_id(p),
                ty: picked[p].ty.clone(),
                fields: picked[p].fields.clone(),
                always_null: picked[p].always_null.clone(),
            });
        }
        (rows, row_of)
    }

    fn effects(&self) -> Result<Effects, Skip> {
        let ts = self.ts;
        let mut writes: BTreeMap<(usize, String), Write> = BTreeMap::new();
        let mut deleted: Vec<usize> = Vec::new();
        let mut runs: Vec<(usize, u8)> = Vec::new();
        let mut raised_at: Vec<(String, usize)> = Vec::new();
        for effect in &self.action.effects {
            match effect {
                Effect::Create { .. } => {}
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
                    let is_tag = matches!(self.declared.get(&self.entity_id(p)),
                        Some(Declared::Sum(sum)) if sum.tag.wire == *column);
                    if !is_tag {
                        let Domain::Enum(values) = &ts.var(*var).domain else {
                            return Err(unsupported(
                                "a transition on a column that is not an enum",
                            ));
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
                            let fk = self.model.foreign_key(*fk);
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
                    let all = values.len() == domain.size(ts);
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
        Ok(Effects {
            writes,
            deleted,
            runs,
            raised_at,
        })
    }

    /// The successor's tag of each param with a transition: `(tag var, target value)`.
    fn targets(&self, outputs: &[RowOut]) -> BTreeMap<usize, (VarId, Value)> {
        let ts = self.ts;
        let mut targets: BTreeMap<usize, (VarId, Value)> = BTreeMap::new();
        for effect in &self.action.effects {
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
        targets
    }

    fn doc(&self) -> Vec<String> {
        let usecase = self.usecase;
        let mut doc = vec![format!(
            "One run of the use case, from the states of the arguments' types. It requires what `{usecase}` requires and writes what it writes; the values the model leaves open are the inputs."
        )];
        if let Some((i, k)) = self.steps {
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
        doc
    }
}

/// One combination's arguments, which every phase after them reads.
struct Frame<'a> {
    cx: &'a ActionCx<'a>,
    picked: &'a [&'a Source<'a>],
    arms: &'a BTreeMap<usize, Vec<(String, String)>>,
    rows: Vec<RowParam>,
    /// The row of each param that is a row argument.
    row_of: BTreeMap<usize, usize>,
    /// The mark argument of each earlier step's param.
    mark_of: BTreeMap<usize, usize>,
    raised_on: RaisedOn,
}

impl Frame<'_> {
    /// The event field holding the id of the row `p` an event was raised on.
    fn event_field_of(&self, p: usize) -> Option<String> {
        self.raised_on
            .iter()
            .find(|(q, _)| *q == p)
            .map(|(_, f)| f.clone())
    }

    /// The field named `wire` of row `r`.
    fn field(&self, r: usize, wire: &str) -> Option<&Field> {
        self.rows[r].fields.iter().find(|f| f.wire == wire)
    }

    /// The row and source state a guard's param reads.
    fn bind(&self, at: At) -> Option<(usize, Option<&VariantDecl>)> {
        match at {
            At::Param(p) => {
                let p = usize::from(p);
                Some((*self.row_of.get(&p)?, self.picked[p].variant))
            }
            _ => None,
        }
    }

    /// A guard as a condition of the function: `None` when it always holds, a skipped
    /// combination when it never does.
    fn lower_guard(&self, pred: &Pred, quant: &mut Quant<'_>) -> Result<Option<Expr>, Skip> {
        let bind = |at: At| self.bind(at);
        let e = simplify(
            expr(self.cx.ts, pred, &self.rows, &bind, &mut Some(quant)).map_err(Skip::Action)?,
        );
        match e {
            Expr::True => Ok(None),
            Expr::False => Err(Skip::Combination),
            e => Ok(Some(e)),
        }
    }

    fn condition(&self, guard: &Guard, id: String, label: String, expr: Expr) -> CondGuard {
        CondGuard {
            id,
            label,
            expr,
            site: guard.site.span.as_ref().and_then(self.cx.site),
        }
    }

    /// Where a mark's id comes from: the mark argument, else the row argument, else the id
    /// input of the created row (`no_input` when it has none).
    fn mark_source(
        &self,
        p: usize,
        id_field: &Field,
        inputs: &[FnInput],
        no_input: &str,
    ) -> Result<MarkSource, Skip> {
        if let Some(m) = self.mark_of.get(&p) {
            return Ok(MarkSource::Mark(*m));
        }
        if let Some(r) = self.row_of.get(&p) {
            return Ok(MarkSource::Row {
                row: *r,
                field: id_field.clone(),
            });
        }
        // Neither a mark nor a row: a created row.
        let name = format!("{}_{}", snake(&self.cx.entity_id(p)), id_field.wire);
        let i = inputs
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| unsupported(no_input))?;
        Ok(MarkSource::Input(i))
    }

    /// The rows the function returns: each written row in its target state, or deleted.
    fn outputs(
        &self,
        effects: &Effects,
        inputs: &mut Vec<FnInput>,
        checks: &mut Vec<ArgCheck>,
    ) -> Result<Vec<RowOut>, Skip> {
        let mut outputs: Vec<RowOut> = Vec::new();
        for (p, param) in self.cx.action.params.iter().enumerate() {
            // A parent, an earlier step's row, and the row an event was raised on are read only.
            if matches!(
                param.role,
                ParamRole::Parent { .. } | ParamRole::Earlier { .. } | ParamRole::Triggered
            ) {
                continue;
            }
            let entity = self.cx.entity_id(p);
            if effects.deleted.contains(&p) {
                let r = self.row_of[&p];
                let id = declared_id(&entity, &self.picked[p].fields);
                outputs.push(RowOut {
                    name: format!("deleted_{}", snake(&entity)),
                    param: p,
                    kind: OutKind::Deleted { row: r, id },
                });
                continue;
            }
            let target = self.target(p, param, &effects.writes)?;
            let mut fields = Vec::new();
            for field in &target.fields {
                let assigned = self.assign(p, param, &entity, field, &effects.writes, inputs)?;
                fields.push((field.clone(), assigned));
            }
            self.check_null(p, param, &entity, &target.null, &effects.writes, checks)?;
            // Every effect on this param lands on a field of the target type.
            for (wp, column) in effects.writes.keys() {
                if *wp == p
                    && !target.fields.iter().any(|f| f.wire == *column)
                    && !target.null.iter().any(|f| f.wire == *column)
                    && target.tag.as_ref().is_none_or(|(t, _)| t != column)
                {
                    return Err(unsupported("a write to a column with no field"));
                }
            }
            outputs.push(RowOut {
                name: snake(&entity),
                param: p,
                kind: OutKind::Row {
                    ty: target.ty,
                    tag: target.tag,
                    fields,
                    always_null: target.null.iter().map(|f| f.wire.clone()).collect(),
                },
            });
        }
        Ok(outputs)
    }

    /// The state and type row `p` ends in.
    fn target(
        &self,
        p: usize,
        param: &Param,
        writes: &BTreeMap<(usize, String), Write>,
    ) -> Result<Target, Skip> {
        let cx = self.cx;
        match cx.declared.get(&cx.entity_id(p)) {
            Some(Declared::Sum(sum)) => {
                let value = if param.role == ParamRole::New {
                    let start = match writes.get(&(p, sum.tag.wire.clone())) {
                        Some(Write::Enum(value)) => value.wire.clone(),
                        _ => start_value(cx.ts, cx.action.params[p].entity, &sum.tag.wire)
                            .ok_or_else(|| unsupported("a created row with no start state"))?,
                    };
                    // A transition of the created row leaves its start state in the same
                    // step: the row ends where the arm from that state goes.
                    match self.arms.get(&p) {
                        Some(a) => a
                            .iter()
                            .find(|(f, _)| *f == start)
                            .map(|(_, t)| t.clone())
                            .ok_or(Skip::Combination)?,
                        None => start,
                    }
                } else {
                    let from = self.picked[p]
                        .variant
                        .map(|v| v.value.wire.clone())
                        .unwrap_or_default();
                    match self.arms.get(&p) {
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
                Ok(Target {
                    ty: variant.name.clone(),
                    tag: Some((sum.tag.wire.clone(), variant.value.clone())),
                    fields: variant.fields.clone(),
                    null: variant.always_null.clone(),
                })
            }
            Some(Declared::Record(r)) => Ok(Target {
                ty: r.name.clone(),
                tag: None,
                fields: r.fields.clone(),
                null: Vec::new(),
            }),
            None => Err(unsupported("an entity with no generated type")),
        }
    }

    /// What row `p` of `entity` holds in `target` after the action; may add an input.
    fn assign(
        &self,
        p: usize,
        param: &Param,
        entity: &str,
        target: &Field,
        writes: &BTreeMap<(usize, String), Write>,
        inputs: &mut Vec<FnInput>,
    ) -> Result<Assigned, Skip> {
        let cx = self.cx;
        Ok(match writes.get(&(p, target.wire.clone())) {
            Some(Write::Lit(lit)) => Assigned::Lit {
                lit: lit.clone(),
                wrap: target.nullable,
            },
            Some(Write::Enum(value)) => Assigned::Lit {
                lit: enum_lit(value.clone(), target)
                    .ok_or_else(|| unsupported("an enum value on a column that is not an enum"))?,
                wrap: target.nullable,
            },
            Some(Write::Switch(named)) => {
                let r = *self
                    .row_of
                    .get(&p)
                    .ok_or_else(|| unsupported("a second lifecycle column of a created row"))?;
                let source = self.field(r, &target.wire).cloned();
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
                    name: format!("{}_{}", snake(entity), target.wire),
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
                if self.event_field_of(*q).is_some() && !self.row_of.contains_key(q) =>
            {
                Assigned::EventId {
                    field: self.event_field_of(*q).expect("checked"),
                    param: *q,
                    wrap: target.nullable,
                }
            }
            Some(Write::Parent(q, _)) if self.mark_of.contains_key(q) => Assigned::MarkId {
                mark: self.mark_of[q],
                wrap: target.nullable,
            },
            Some(Write::Parent(q, parent_col)) if !self.row_of.contains_key(q) => {
                // The parent is created in the same run: its id is the caller's input,
                // shared with the created parent row.
                let parent = cx.entity_id(*q);
                let i = input_index(
                    inputs,
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
                let r = self.row_of[q];
                let field = self
                    .field(r, parent_col)
                    .cloned()
                    .ok_or_else(|| unsupported("a parent key column with no field"))?;
                Assigned::ParentId {
                    row: r,
                    field,
                    wrap: target.nullable,
                }
            }
            None if param.role == ParamRole::New => {
                match start_literal(cx.ts, cx.action.params[p].entity, &target.wire, target) {
                    Some(Some(lit)) => Assigned::Lit {
                        lit,
                        wrap: target.nullable,
                    },
                    Some(None) if target.nullable => Assigned::Null,
                    Some(None) => return Err(Skip::Combination),
                    None => {
                        let i = input_index(
                            inputs,
                            format!("{}_{}", snake(entity), target.wire),
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
                let r = self.row_of[&p];
                match self.field(r, &target.wire) {
                    Some(sf) => {
                        let adapt = match (sf.nullable, target.nullable) {
                            (a, b) if a == b => Adapt::Same,
                            (false, true) => Adapt::Wrap,
                            _ => Adapt::Unwrap(format!(
                                "rdra-ish: {entity}.{} is null, but the verified model keeps it present after {}",
                                target.wire, cx.usecase
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
        })
    }

    /// A column the target state keeps null: what the action leaves there must be null.
    fn check_null(
        &self,
        p: usize,
        param: &Param,
        entity: &str,
        null: &[Field],
        writes: &BTreeMap<(usize, String), Write>,
        checks: &mut Vec<ArgCheck>,
    ) -> Result<(), Skip> {
        let cx = self.cx;
        for target in null {
            match writes.get(&(p, target.wire.clone())) {
                None if param.role == ParamRole::New => {
                    if !matches!(
                        start_literal(cx.ts, cx.action.params[p].entity, &target.wire, target),
                        Some(None)
                    ) {
                        return Err(Skip::Combination);
                    }
                }
                None => {
                    let r = self.row_of[&p];
                    if let Some(sf) = self.field(r, &target.wire) {
                        if !sf.nullable {
                            return Err(Skip::Combination);
                        }
                        checks.push(ArgCheck::MustBeNull {
                            row: r,
                            field: sf.clone(),
                            message: format!(
                                "rdra-ish: {entity}.{} has a value, but the verified model keeps it null after {}",
                                target.wire, cx.usecase
                            ),
                        });
                    }
                }
                Some(Write::Null) => {}
                Some(_) => return Err(Skip::Combination),
            }
        }
        Ok(())
    }

    /// The guards as conditions and argument checks, and the quantifier elements they bind.
    /// A `.has` / `.none` ranges over a list of related rows, an input.
    fn conditions(
        &self,
        targets: &BTreeMap<usize, (VarId, Value)>,
        inputs: &mut Vec<FnInput>,
        checks: &mut Vec<ArgCheck>,
    ) -> Result<(Vec<CondGuard>, Vec<RowParam>), Skip> {
        let cx = self.cx;
        let row_type = |entity: &str| -> Option<(String, Vec<Field>)> {
            match cx.declared.get(entity)? {
                Declared::Record(r) => Some((r.name.clone(), r.fields.clone())),
                Declared::Sum(s) => Some((s.row.clone(), s.row_fields.clone())),
            }
        };
        let mut quant = Quant {
            model: cx.model,
            row_type: &row_type,
            base: 0,
            n_rows: self.rows.len(),
            elems: Vec::new(),
            stack: Vec::new(),
            lists: Vec::new(),
        };
        let mut conditions = Vec::new();
        for guard in &cx.action.guards {
            match guard.why {
                // The run mark: step 1 needs none (an obligation), a later step takes the mark
                // of the step before as an argument.
                // The event: the handler takes it as an argument (`event_in`).
                GuardWhy::Run { .. } | GuardWhy::Raised { .. } => {}
                // The from-states: the source state type holds the tag's; a second lifecycle
                // column's are a condition.
                // On a created row the arm from its start state was taken when its target was.
                GuardWhy::TransitionFrom if reads_only_created(&guard.pred, cx.action) => {}
                GuardWhy::TransitionFrom => {
                    if let Some(e) = self.lower_guard(&guard.pred, &mut quant)? {
                        conditions.push(self.condition(
                            guard,
                            "TransitionFrom".to_string(),
                            "the use case moves the column only from these states".to_string(),
                            e,
                        ));
                    }
                }
                // `.when` on an arm: the successor's tag is the function's target, so the guard
                // folds to the conditions of the arm taken.
                GuardWhy::ArmCondition { .. } => {
                    let pred = fold_next(&guard.pred, targets).ok_or_else(|| {
                        unsupported("a `.when` on a chosen value or a second lifecycle column")
                    })?;
                    if let Some(e) = self.lower_guard(&pred, &mut quant)? {
                        let (id, label) = arm_conditions(cx.model, cx.ts, cx.action, self.arms);
                        conditions.push(self.condition(guard, id, label, e));
                    }
                }
                GuardWhy::Owner => checks.push(self.link_check(guard)?),
                GuardWhy::Condition { cond, .. } => {
                    if let Some(e) = self.lower_guard(&guard.pred, &mut quant)? {
                        let c = &cx.model.conditions[cond];
                        conditions.push(self.condition(guard, c.id.clone(), c.label.clone(), e));
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
                inputs,
                name.clone(),
                FieldType::Rows(ty.clone()),
                format!("Every present {entity} row: the function keeps those its keys link."),
            ));
        }
        for c in &mut conditions {
            remap_lists(&mut c.expr, &list_input);
        }
        Ok((conditions, quant.elems))
    }

    /// An `Owner` guard: the child row's key names the parent row, or the event's row.
    fn link_check(&self, guard: &Guard) -> Result<ArgCheck, Skip> {
        let cx = self.cx;
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
        let VarKind::Owner { fk, .. } = &cx.ts.var(*var).kind else {
            return Err(unsupported("a link guard on a var that is not a key"));
        };
        let fk = cx.model.foreign_key(*fk);
        let [(child_col, parent_col)] = fk.columns.as_slice() else {
            return Err(unsupported("a key over several columns"));
        };
        let (c, q) = (usize::from(*c), usize::from(*q));
        // A child of the row the event was raised on: its key is the event's id.
        if let (Some(&cr), Some(event_field)) = (self.row_of.get(&c), self.event_field_of(q)) {
            if !self.row_of.contains_key(&q) {
                let Some(child_field) = self.field(cr, child_col).cloned() else {
                    return Err(unsupported("a link guard on a column with no field"));
                };
                return Ok(ArgCheck::EventRow {
                    row: cr,
                    field: child_field,
                    event_field,
                    message: format!(
                        "rdra-ish: the {} row is not a child of the row the event was raised on",
                        self.rows[cr].entity
                    ),
                });
            }
        }
        let (Some(&cr), Some(&qr)) = (self.row_of.get(&c), self.row_of.get(&q)) else {
            return Err(unsupported("a link guard on a created row"));
        };
        let child_field = self.field(cr, child_col).cloned();
        let parent_field = self.field(qr, parent_col).cloned();
        let (Some(child_field), Some(parent_field)) = (child_field, parent_field) else {
            return Err(unsupported("a link guard on a column with no field"));
        };
        Ok(ArgCheck::Link {
            child: cr,
            child_field,
            parent: qr,
            parent_field,
            message: format!(
                "rdra-ish: the {} row is not a child of the {} row",
                self.rows[cr].entity, self.rows[qr].entity
            ),
        })
    }

    /// A row the event was raised on is the event's row.
    fn raised_on_checks(&self, checks: &mut Vec<ArgCheck>) -> Result<(), Skip> {
        for (p, event_field) in &self.raised_on {
            if let Some(&r) = self.row_of.get(p) {
                let (_, id_field) = self.cx.id_of(&self.rows[r].entity).ok_or_else(|| {
                    unsupported("an event raised on an entity with no one-column key")
                })?;
                checks.push(ArgCheck::EventRow {
                    row: r,
                    field: id_field,
                    event_field: event_field.clone(),
                    message: format!(
                        "rdra-ish: the {} row is not the row the event was raised on",
                        self.rows[r].entity
                    ),
                });
            }
        }
        Ok(())
    }

    /// The events the use case raises, with the ids of the rows each raise marks.
    fn raised(
        &self,
        raised_at: &[(String, usize)],
        inputs: &[FnInput],
    ) -> Result<Vec<RaisedEvent>, Skip> {
        let cx = self.cx;
        let mut raised: Vec<RaisedEvent> = Vec::new();
        for event in cx.raises {
            let mut ids = Vec::new();
            for field in event_fields(cx.ts, cx.declared, event) {
                let at = raised_at
                    .iter()
                    .find(|(e, p)| e == event && cx.entity_id(*p) == field.entity)
                    .map(|(_, p)| *p);
                let Some(p) = at else {
                    return Err(unsupported("raises an event without marking its row"));
                };
                let (_, id_field) = cx.id_of(&field.entity).ok_or_else(|| {
                    unsupported("an event raised on an entity with no one-column key")
                })?;
                let source = self.mark_source(
                    p,
                    &id_field,
                    inputs,
                    "an event raised on a created row with no id input",
                )?;
                ids.push((field, source));
            }
            raised.push(RaisedEvent {
                ty: type_ident(event),
                ids,
            });
        }
        Ok(raised)
    }

    /// The marks the step leaves for the next one.
    fn marks_out(
        &self,
        runs: &[(usize, u8)],
        inputs: &[FnInput],
        marks: &mut Vec<MarkDecl>,
    ) -> Result<Vec<MarkOut>, Skip> {
        let cx = self.cx;
        let mut marks_out: Vec<MarkOut> = Vec::new();
        for &(p, step) in runs {
            if step == 0 {
                continue; // the run ends on this row: its mark is consumed
            }
            let entity = cx.entity_id(p);
            let (id_ty, id_field) = cx
                .id_of(&entity)
                .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
            let source = self.mark_source(
                p,
                &id_field,
                inputs,
                "a run mark on a created row with no id input",
            )?;
            let ty = cx.declare_mark(marks, &entity, step, id_ty);
            marks_out.push(MarkOut {
                name: format!("{}_mark", snake(&entity)),
                ty,
                param: p,
                step,
                source,
            });
        }
        Ok(marks_out)
    }
}
