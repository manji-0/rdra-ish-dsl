//! The guards of a function: successor-tag folding, the arms' `.when` conditions, and which
//! guards read only what the action creates.

use super::*;

/// `pred` with each read of a param's successor tag replaced by whether the function's target
/// is that value; `None` when it reads some other var of a successor.
pub(super) fn fold_next(pred: &Pred, targets: &BTreeMap<usize, (VarId, Value)>) -> Option<Pred> {
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
        Pred::Count {
            entity,
            body,
            op,
            n,
        } => Pred::Count {
            entity: *entity,
            body: Box::new(rec(body)?),
            op: *op,
            n: *n,
        },
        other => other.clone(),
    })
}

/// The id and label of the `.when` conditions of the arms the function takes.
pub(super) fn arm_conditions(
    model: &SemanticModel,
    ts: &TransitionSystem,
    action: &Action,
    chosen: &BTreeMap<usize, Vec<(String, String)>>,
) -> (String, String) {
    let mut ids: Vec<String> = Vec::new();
    for effect in &action.effects {
        let Effect::Transition { var, param, arms } = effect else {
            continue;
        };
        let Some(taken) = chosen.get(&usize::from(*param)) else {
            continue;
        };
        let Domain::Enum(values) = &ts.var(*var).domain else {
            continue;
        };
        let name = |v: Value| values[usize::from(v.0)].clone();
        for arm in arms {
            if !taken.contains(&(name(arm.from), name(arm.to))) {
                continue;
            }
            for (_, id) in &arm.conds {
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
pub(super) fn reads_only_created(pred: &Pred, action: &Action) -> bool {
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
        Pred::Forall { .. } | Pred::Exists { .. } | Pred::Count { .. } => false,
    }
}
