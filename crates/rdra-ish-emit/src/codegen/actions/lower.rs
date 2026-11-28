//! Lowering one action combination to a function: its inputs, rows, assignments, guards, and
//! the event payload it raises.

use rdra_ish_core::lifecycle::{Guard, ModelRef, Param};
use rdra_ish_core::model::LinkKind;

use super::*;
use crate::codegen::ir::{Chosen, EventIn, MarkDecl, MarkIn, MarkOut, MarkSource, RaisedEvent};

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

/// The arms a function takes of each param's state tag: `(from, to)` by value names.
pub(super) type TagArms = BTreeMap<usize, Vec<(String, String)>>;

/// The arms a function takes of each second lifecycle column it chose a target of, by
/// `(param, var)`.
pub(super) type ColumnArms = BTreeMap<(usize, VarId), Vec<(String, String)>>;

/// Lowers one combination of source states (`picked`) and chosen arms to the function `name`:
/// `arms` of each param's tag, `columns` of each second lifecycle column with a choice, and
/// `tag_choices` the params whose tag target is a choice among arms from one state.
/// Declares the run marks it takes and leaves in `marks`.
pub(super) fn function(
    cx: &ActionCx<'_>,
    picked: &[&Source<'_>],
    arms: &BTreeMap<usize, Vec<(String, String)>>,
    columns: &ColumnArms,
    tag_choices: &[usize],
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
        columns,
        tag_choices,
        rows,
        row_of,
        mark_of,
        raised_on,
    };
    let mut effects = cx.effects()?;
    // A second lifecycle column with a choice moves only by the arms to the chosen target.
    for ((p, var), a) in columns {
        if let VarKind::Column { column, .. } = &cx.ts.var(*var).kind {
            effects
                .writes
                .insert((*p, column.clone()), Write::Switch(a.clone()));
        }
    }

    let mut inputs: Vec<FnInput> = Vec::new();
    let mut checks: Vec<ArgCheck> = Vec::new();
    f.mark_row_checks(&mut checks)?;
    let mut doc = cx.doc();
    let outputs = f.outputs(&effects, &mut inputs, &mut checks, &mut doc)?;
    let targets = cx.targets(&outputs);
    let (conditions, elems) = f.conditions(&targets, &mut inputs, &mut checks)?;
    f.raised_on_checks(&mut checks)?;
    let raised = f.raised(&effects.raised_at, &inputs)?;
    let marks_out = f.marks_out(&effects.runs, &inputs, marks)?;
    let chosen = f.chosen();
    Ok(FnDecl {
        name: name.to_string(),
        usecase: cx.usecase.to_string(),
        action: cx.action.name.clone(),
        step: cx.steps,
        chosen,
        elems,
        marks_in,
        marks_out,
        doc,
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

/// Why a use case that raises or handles a counted event is not printed: a pending count is
/// a runtime queue, not a fact of the model (docs/design/counted-events.md).
const COUNTED: &str = "a counted event (triggers(..).counted(k))";

fn unsupported(why: &str) -> Skip {
    Skip::Action(why.to_string())
}

/// The name of the input that passes `entity`'s `column`: `{entity}_{column}`, the entity in
/// snake_case. A choice of a written row's column is found again by this name.
fn input_name(entity: &str, column: &str) -> String {
    format!("{}_{column}", snake(entity))
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
            if let Pred::Cmp(Term::Slot { var, .. }, ..) = &guard.pred {
                if let VarKind::Raised {
                    handler: Some(_), ..
                } = &self.ts.var(*var).kind
                {
                    return Err(unsupported(COUNTED));
                }
            }
            let Pred::Cmp(
                Term::Slot {
                    at: At::Param(p), ..
                },
                TsCmp::Eq,
                _,
            ) = &guard.pred
            else {
                return Err(unsupported(
                    "an event guard in a form the generator does not print",
                ));
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
            if param.role == ParamRole::New
                || matches!(param.role, ParamRole::Earlier { .. }) && !self.guard_reads(p)
            {
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

    /// Whether a guard other than the run mark reads param `p`: an earlier step's row it
    /// reads is an argument beside its mark, as it is now (the run's rows are read as they
    /// are when the step runs).
    fn guard_reads(&self, p: usize) -> bool {
        let at_p = |t: &Term| {
            matches!(t, Term::Slot { at: At::Param(q), .. } | Term::Index(At::Param(q))
                if usize::from(*q) == p)
        };
        self.action
            .guards
            .iter()
            .filter(|g| !matches!(g.why, GuardWhy::Run { .. }))
            .any(|g| g.pred.terms().any(at_p))
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
                    if let VarKind::Raised {
                        handler: Some(_), ..
                    } = &ts.var(*var).kind
                    {
                        return Err(unsupported(COUNTED));
                    }
                    let VarKind::Column { column, .. } = &ts.var(*var).kind else {
                        return Err(unsupported("a transition of a var that is not a column"));
                    };
                    let is_tag = matches!(self.declared.get(&self.entity_id(p)),
                        Some(Declared::Sum(sum)) if sum.tag.wire == *column);
                    if !is_tag {
                        let domain = &ts.var(*var).domain;
                        if domain.names().is_none() {
                            return Err(unsupported(
                                "a transition on a column that is not an enum",
                            ));
                        }
                        let named = a
                            .iter()
                            .map(|arm| {
                                let v = |x: Value| {
                                    domain
                                        .name(x)
                                        .expect("an arm names declared values")
                                        .to_string()
                                };
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
                            let w = literal(&ts.var(*var).domain, *v).ok_or_else(|| {
                                unsupported("a value the generator does not print")
                            })?;
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
                        // An optional key the step leaves or makes unlinked: its column is null.
                        (VarKind::Owner { fk, .. }, Rhs::Lit(v)) if *v == Value::NULL => {
                            let fk = self.model.foreign_key(*fk);
                            let [(child_col, _)] = fk.columns.as_slice() else {
                                return Err(unsupported("a key over several columns"));
                            };
                            writes.insert((p, child_col.clone()), Write::Null);
                        }
                        (VarKind::Run { .. }, Rhs::Lit(v)) => runs.push((p, v.0)),
                        (VarKind::Raised { event, .. }, Rhs::Lit(v)) if *v == Value::TRUE => {
                            raised_at.push((event.clone(), p));
                        }
                        (VarKind::Raised { .. }, _) => {}
                        (VarKind::Prop { .. }, _) => {
                            return Err(unsupported("drives a comparison proposition"))
                        }
                        _ => return Err(unsupported("an assignment the generator does not print")),
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
                        // Every value, null among them: the caller may pass null.
                        Domain::Nullable | Domain::NullableEnum(_) | Domain::NullableBool
                            if all =>
                        {
                            Write::InputOrNull
                        }
                        _ => return Err(unsupported("a choice among some values of a column")),
                    };
                    writes.insert((usize::from(*param), column.clone()), w);
                }
                Effect::Tick => return Err(unsupported("reads `now`")),
            }
        }
        // `sets(UC, E.column == F.source)`: the column takes the source row's value, in place
        // of the choice the effect forks.
        for guard in &self.action.guards {
            if guard.why != GuardWhy::Copy {
                continue;
            }
            let Pred::Cmp(
                Term::Slot {
                    var: target,
                    at: At::Next(c),
                },
                TsCmp::Eq,
                Term::Slot {
                    var: source,
                    at: At::Param(s),
                },
            ) = &guard.pred
            else {
                return Err(unsupported(
                    "a copy between a column that holds null and one that does not",
                ));
            };
            let (VarKind::Column { column: to, .. }, VarKind::Column { column: from, .. }) =
                (&ts.var(*target).kind, &ts.var(*source).kind)
            else {
                return Err(unsupported("a copy of a var that is not a column"));
            };
            writes.insert(
                (usize::from(*c), to.clone()),
                Write::Copy(usize::from(*s), from.clone()),
            );
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
            let domain = &ts.var(*var).domain;
            let Some(values) = domain.names() else {
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
                    targets.insert(p, (*var, domain.of_variant(i)));
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
    columns: &'a ColumnArms,
    /// The params whose tag target is a choice among arms from one state.
    tag_choices: &'a [usize],
    rows: Vec<RowParam>,
    /// The row of each param that is a row argument.
    row_of: BTreeMap<usize, usize>,
    /// The mark argument of each earlier step's param.
    mark_of: BTreeMap<usize, usize>,
    raised_on: RaisedOn,
}

impl Frame<'_> {
    /// The targets the function chose among arms from one value, for its dispatcher: the tag
    /// of each param in `tag_choices`, then each second lifecycle column with a choice. A
    /// target whose row, type, or value is not found is left out.
    fn chosen(&self) -> Vec<Chosen> {
        let row = |p: usize| self.rows.iter().position(|row| row.param == Some(p));
        let mut out = Vec::new();
        for p in self.tag_choices {
            let Some(r) = row(*p) else {
                continue;
            };
            let Some(Declared::Sum(sum)) = self.cx.declared.get(&self.rows[r].entity) else {
                continue;
            };
            let FieldType::Enum(tag_enum) = &sum.tag.ty else {
                continue;
            };
            let target = &self.arms[p][0].1;
            if let Some(v) = sum.variants.iter().find(|v| v.value.wire == *target) {
                out.push(Chosen {
                    row: r,
                    column: None,
                    enum_ty: tag_enum.clone(),
                    value: v.value.clone(),
                });
            }
        }
        for ((p, var), a) in self.columns {
            let VarKind::Column { column, .. } = &self.cx.ts.var(*var).kind else {
                continue;
            };
            let Some(r) = row(*p) else {
                continue;
            };
            let Some(FieldType::Enum(enum_ty)) = self.rows[r]
                .fields
                .iter()
                .find(|x| x.wire == *column)
                .map(|x| x.ty.clone())
            else {
                continue;
            };
            let Some((_, target)) = a.first() else {
                continue;
            };
            out.push(Chosen {
                row: r,
                column: Some(column.clone()),
                enum_ty,
                value: EnumValue {
                    ident: type_ident(target),
                    wire: target.clone(),
                    label: None,
                },
            });
        }
        out
    }

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

    /// What a read of param `p`'s successor stands for in this function (`fold_next`): the
    /// target state, a value the effects write, the row a key is set to, the column as the
    /// argument holds it when the effects leave it, its start value on a created row, or the
    /// input a choice became. `None` for a second lifecycle column and a created row's own key.
    fn next_read(&self, t: &Term, targets: &BTreeMap<usize, (VarId, Value)>) -> Option<NextRead> {
        let action = self.cx.action;
        let created = |p: u8| action.params[usize::from(p)].role == ParamRole::New;
        let (var, p) = match t {
            Term::Index(At::Next(p)) if !created(*p) => {
                return Some(NextRead::Term(Term::Index(At::Param(*p))))
            }
            Term::Slot {
                var,
                at: At::Next(p),
            } => (*var, *p),
            _ => return None,
        };
        if let Some((tag, to)) = targets.get(&usize::from(p)) {
            if *tag == var {
                return Some(NextRead::Value(*to));
            }
        }
        // A copied column holds the source row's value.
        for guard in action.guards.iter().filter(|g| g.why == GuardWhy::Copy) {
            if let Pred::Cmp(
                Term::Slot {
                    var: target,
                    at: At::Next(c),
                },
                TsCmp::Eq,
                source @ Term::Slot {
                    var: from,
                    at: At::Param(_),
                },
            ) = &guard.pred
            {
                // A literal compared with the read is of the target's domain: only a source
                // of the same domain reads it the same way.
                if *target == var && *c == p {
                    let ts = self.cx.ts;
                    return (ts.var(*from).domain == ts.var(var).domain)
                        .then(|| NextRead::Term(source.clone()));
                }
            }
        }
        for effect in &action.effects {
            match effect {
                Effect::Assign {
                    var: v, param, rhs, ..
                } if *v == var && *param == p => {
                    return Some(match rhs {
                        Rhs::Lit(x) => NextRead::Value(*x),
                        Rhs::Param(q) => NextRead::Term(Term::Index(At::Param(*q))),
                    })
                }
                Effect::Choose {
                    var: v,
                    param,
                    values,
                    ..
                } if *v == var && *param == p => {
                    return Some(match values.as_slice() {
                        [one] => NextRead::Value(one.value),
                        _ => NextRead::Input,
                    })
                }
                // A second lifecycle column: its chosen target, when the function has one.
                Effect::Transition { var: v, param, .. } if *v == var && *param == p => {
                    let a = self.columns.get(&(usize::from(p), var))?;
                    let (_, to) = a.first()?;
                    if a.iter().any(|(_, t)| t != to) {
                        return None;
                    }
                    let domain = &self.cx.ts.var(var).domain;
                    let i = domain.names()?.iter().position(|n| n == to)?;
                    return Some(NextRead::Value(domain.of_variant(i)));
                }
                _ => {}
            }
        }
        Some(if created(p) {
            NextRead::Value(self.cx.ts.var(var).base)
        } else {
            NextRead::Term(Term::Slot {
                var,
                at: At::Param(p),
            })
        })
    }

    /// A `TransitionFrom` guard of a second lifecycle column the function chose a target of,
    /// kept to the `from` values of the arms to that target.
    fn chosen_froms(&self, pred: &Pred) -> Pred {
        let Pred::Or(items) = pred else {
            return pred.clone();
        };
        let kept = items
            .iter()
            .filter(|item| {
                let Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(p),
                    },
                    TsCmp::Eq,
                    Term::Lit(v),
                ) = item
                else {
                    return true;
                };
                let Some(a) = self.columns.get(&(usize::from(*p), *var)) else {
                    return true;
                };
                let domain = &self.cx.ts.var(*var).domain;
                domain
                    .name(*v)
                    .is_some_and(|from| a.iter().any(|(f, _)| f == from))
            })
            .cloned()
            .collect();
        Pred::Or(kept)
    }

    /// A guard as a condition of the function: `None` when it always holds, a skipped
    /// combination when it never does. A read of a successor folds first (`next_read`).
    fn lower_guard(
        &self,
        pred: &Pred,
        targets: &BTreeMap<usize, (VarId, Value)>,
        quant: &mut Quant<'_>,
    ) -> Result<Option<Expr>, Skip> {
        let pred = fold_next(pred, &|t| self.next_read(t, targets)).ok_or_else(|| {
            unsupported("a condition on a second lifecycle column or a created row's own key after the use case")
        })?;
        let bind = |at: At| self.bind(at);
        let e = expr(self.cx.ts, &pred, &self.rows, &bind, &mut Some(quant))
            .map_err(Skip::Action)?
            .simplify();
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
        let name = input_name(&self.cx.entity_id(p), &id_field.wire);
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
        doc: &mut Vec<String>,
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
            let entity_key = self.cx.ts.entity(self.cx.action.params[p].entity).key;
            let columns = &self.cx.model.entities[entity_key].columns;
            // A derived column the effects do not write: computed below, from what the
            // other fields hold.
            let derived = |wire: &str| {
                columns.iter().find(|c| {
                    c.name == wire
                        && c.derived_expr.is_some()
                        && !effects.writes.contains_key(&(p, wire.to_string()))
                })
            };
            let mut fields = Vec::new();
            for field in &target.fields {
                let assigned = if derived(&field.wire).is_some() {
                    Assigned::Null
                } else {
                    self.assign(p, param, &entity, field, &effects.writes, inputs)?
                };
                fields.push((field.clone(), assigned));
            }
            for i in 0..fields.len() {
                let field = fields[i].0.clone();
                let Some(column) = derived(&field.wire) else {
                    continue;
                };
                let values = derived::RowValues {
                    model: self.cx.model,
                    entity: entity_key,
                    columns,
                    fields: &fields,
                    tag: target.tag.as_ref(),
                    null: &target.null,
                };
                fields[i].1 = match derived::derived_value(&values, column) {
                    Ok(assigned) => assigned,
                    Err(why) => {
                        let name = input_name(&entity, &field.wire);
                        let k = input_index(
                            inputs,
                            name.clone(),
                            InputType::Value(field.ty.clone()),
                            format!("{entity}.{} of the written row.", field.wire),
                        );
                        inputs[k].nullable = field.nullable;
                        doc.push(format!(
                            "`{name}` is an input: {entity}.{} is derived ({}), and this function does not compute it because {why}. Pass the value it has on the written row.",
                            field.wire,
                            column
                                .derived_expr
                                .as_ref()
                                .map(|e| e.display())
                                .unwrap_or_default()
                        ));
                        Assigned::Input {
                            input: k,
                            wrap: false,
                        }
                    }
                };
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
            Some(write @ (Write::Input | Write::InputOrNull)) => {
                // Null is the caller's to pass only where the target state lets the column be
                // null; a state that keeps it present takes a present value.
                let or_null = matches!(write, Write::InputOrNull) && target.nullable;
                inputs.push(FnInput {
                    name: input_name(entity, &target.wire),
                    ty: InputType::Value(target.ty.clone()),
                    nullable: or_null,
                    doc: format!("The value the use case writes to {entity}.{}.", target.wire),
                });
                Assigned::Input {
                    input: inputs.len() - 1,
                    wrap: target.nullable && !or_null,
                }
            }
            Some(Write::Copy(s, from)) => {
                let r = *self
                    .row_of
                    .get(s)
                    .ok_or_else(|| unsupported("a copy from a row that is not an argument"))?;
                let source = self
                    .field(r, from)
                    .cloned()
                    .ok_or_else(|| unsupported("a copy from a column with no field"))?;
                if source.ty != target.ty {
                    return Err(unsupported("a copy between columns of different types"));
                }
                let adapt = match (source.nullable, target.nullable) {
                    (a, b) if a == b => Adapt::Same,
                    (false, true) => Adapt::Wrap,
                    _ => {
                        return Err(unsupported(
                            "a copy from a column that may be null to one that may not",
                        ))
                    }
                };
                Assigned::From {
                    row: r,
                    field: source,
                    adapt,
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
                    input_name(&parent, parent_col),
                    InputType::Value(target.ty.clone()),
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
                            input_name(entity, &target.wire),
                            InputType::Value(target.ty.clone()),
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
        // The input a choice of a written row's column became, by the column's var.
        let chosen: Vec<FnInput> = inputs.clone();
        let next_input = |var: VarId| -> Option<(usize, FnInput)> {
            let VarKind::Column { entity, column, .. } = &cx.ts.var(var).kind else {
                return None;
            };
            let name = input_name(&cx.ts.entity(*entity).id, column);
            let i = chosen.iter().position(|x| x.name == name)?;
            Some((i, chosen[i].clone()))
        };
        let mut quant = Quant {
            model: cx.model,
            row_type: &row_type,
            next_input: &next_input,
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
                // A copy is a write (`effects`), not a condition.
                GuardWhy::Copy => {}
                // The from-states: the source state type holds the tag's; a second lifecycle
                // column's are a condition.
                // On a created row the arm from its start state was taken when its target was.
                GuardWhy::TransitionFrom if reads_only_created(&guard.pred, cx.action) => {}
                GuardWhy::TransitionFrom => {
                    let pred = self.chosen_froms(&guard.pred);
                    if let Some(e) = self.lower_guard(&pred, targets, &mut quant)? {
                        conditions.push(self.condition(
                            guard,
                            "TransitionFrom".to_string(),
                            "the use case moves the column only from these states".to_string(),
                            e,
                        ));
                    }
                }
                // `.when` on an arm: the successor's tag is the function's target, so the guard
                // folds to the conditions of the arm taken. On a chosen value it reads the input.
                GuardWhy::ArmCondition { .. } => {
                    if let Some(e) = self.lower_guard(&guard.pred, targets, &mut quant)? {
                        let (id, label) =
                            arm_conditions(cx.model, cx.ts, cx.action, self.arms, self.columns);
                        conditions.push(self.condition(guard, id, label, e));
                    }
                }
                // A key between rows an earlier step wrote, read as they are now: the step
                // waits (`Rejected`, a retry) until it holds, as `verify` explored it.
                GuardWhy::Owner if self.between_earlier_rows(guard) => {
                    if let Some(e) = self.lower_guard(&guard.pred, targets, &mut quant)? {
                        let Some(VarKind::Owner { fk, .. }) =
                            guard.pred.terms().find_map(|t| match t {
                                Term::Slot { var, .. } => Some(&cx.ts.var(*var).kind),
                                _ => None,
                            })
                        else {
                            return Err(unsupported("a link guard on a var that is not a key"));
                        };
                        let fk = cx.model.foreign_key(*fk);
                        let (child, parent) = (
                            &cx.model.entities[fk.child].id,
                            &cx.model.entities[fk.parent].id,
                        );
                        let role = fk.role_or(parent);
                        conditions.push(self.condition(
                            guard,
                            format!("Key_{child}_{role}"),
                            format!("the {child} still belongs to the {parent} of the run"),
                            e,
                        ));
                    }
                }
                GuardWhy::Owner => checks.push(self.link_check(guard)?),
                // `links` / `relinks` / `unlinks`: whether the child's key is set, a condition
                // `<Kind>_<Child>_<role>` of the step that writes the child.
                GuardWhy::Link => {
                    if let Some(e) = self.lower_guard(&guard.pred, targets, &mut quant)? {
                        let ModelRef::Link(i) = guard.site.fact else {
                            return Err(unsupported("a link guard with no `links` line"));
                        };
                        let link = &cx.model.links[i];
                        let (child, parent) = (
                            &cx.model.entities[link.child].id,
                            &cx.model.entities[link.parent].id,
                        );
                        let role = link
                            .key
                            .map(|fk| cx.model.foreign_key(fk).role_or(parent).to_string())
                            .unwrap_or_else(|| parent.clone());
                        let (kind, label) = match link.kind {
                            LinkKind::Links => {
                                ("Links", format!("the {child} is linked to no {parent} yet"))
                            }
                            LinkKind::Relinks => {
                                ("Relinks", format!("the {child} is linked to some {parent}"))
                            }
                            // Under the parent the step writes, when it writes one.
                            LinkKind::Unlinks
                                if matches!(
                                    guard.pred,
                                    Pred::Cmp(_, _, Term::Index(At::Param(_)))
                                ) =>
                            {
                                (
                                    "Unlinks",
                                    format!(
                                        "the {child} is linked to the {parent} the step writes"
                                    ),
                                )
                            }
                            LinkKind::Unlinks => {
                                ("Unlinks", format!("the {child} is linked to some {parent}"))
                            }
                        };
                        conditions.push(self.condition(
                            guard,
                            format!("{kind}_{child}_{role}"),
                            label,
                            e,
                        ));
                    }
                }
                GuardWhy::Condition { cond, .. } => {
                    if let Some(e) = self.lower_guard(&guard.pred, targets, &mut quant)? {
                        let c = &cx.model.conditions[cond];
                        conditions.push(self.condition(guard, c.id.clone(), c.label.clone(), e));
                    }
                }
                // A delete the key's children reject: no listed child row names the row.
                GuardWhy::Restrict { fk } => {
                    if let Some(e) = self.lower_guard(&guard.pred, targets, &mut quant)? {
                        let fk = cx.model.foreign_key(fk);
                        let (child, parent) = (
                            &cx.model.entities[fk.child].id,
                            &cx.model.entities[fk.parent].id,
                        );
                        let role = fk.role_or(parent);
                        conditions.push(self.condition(
                            guard,
                            format!("Restrict_{child}_{role}"),
                            format!("no {child} names the deleted {parent}"),
                            e,
                        ));
                    }
                }
                _ => return Err(unsupported("a guard the generator does not print")),
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
                InputType::Rows(ty.clone()),
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
            return Err(unsupported("a link guard the generator does not print"));
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
        // An earlier step's row a guard reads is an argument beside its mark (`guard_reads`),
        // so a key between it and a row of this step is checked on the row as it is now.
        let mark_only = |p: usize| self.mark_of.contains_key(&p) && !self.row_of.contains_key(&p);
        if mark_only(c) || mark_only(q) {
            return Err(unsupported(
                "a key between rows an earlier step wrote, whose marks carry ids, not keys",
            ));
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

    /// Whether an `Owner` guard joins two rows an earlier step wrote, each an argument.
    fn between_earlier_rows(&self, guard: &Guard) -> bool {
        let earlier = |t: &Term| match t {
            Term::Slot {
                at: At::Param(p), ..
            }
            | Term::Index(At::Param(p)) => {
                let p = usize::from(*p);
                self.mark_of.contains_key(&p) && self.row_of.contains_key(&p)
            }
            _ => false,
        };
        let terms: Vec<&Term> = guard.pred.terms().collect();
        !terms.is_empty() && terms.iter().all(|t| earlier(t))
    }

    /// An earlier step's row taken beside its mark is the row the mark names.
    fn mark_row_checks(&self, checks: &mut Vec<ArgCheck>) -> Result<(), Skip> {
        for (p, m) in &self.mark_of {
            let Some(&r) = self.row_of.get(p) else {
                continue;
            };
            let (_, id_field) = self
                .cx
                .id_of(&self.rows[r].entity)
                .ok_or_else(|| unsupported("a run mark on an entity with no one-column key"))?;
            checks.push(ArgCheck::MarkRow {
                row: r,
                field: id_field,
                mark: *m,
                message: format!(
                    "rdra-ish: the {} row is not the row its run mark names",
                    self.rows[r].entity
                ),
            });
        }
        Ok(())
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
