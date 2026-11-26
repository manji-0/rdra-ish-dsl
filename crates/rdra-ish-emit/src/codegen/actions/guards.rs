//! The guards of a function: successor-tag folding, the arms' `.when` conditions, and which
//! guards read only what the action creates.

use super::*;

/// What a read of a written row's successor stands for in one function.
pub(super) enum NextRead {
    /// A value the function fixes: a literal it writes, its one choice, or its target state.
    Value(Value),
    /// A term of the arguments: a column the function leaves as it is, or the row a key it
    /// sets names.
    Term(Term),
    /// A value the caller passes: the read stays, and the condition reads the input.
    Input,
}

/// `pred` with each read of a param's successor replaced by what `next` says it stands for:
/// the function's target state, a value it writes, a term of its arguments, or an input.
/// Comparisons between two fixed values fold. `None` when some read has no such meaning.
pub(super) fn fold_next(pred: &Pred, next: &dyn Fn(&Term) -> Option<NextRead>) -> Option<Pred> {
    let rec = |p: &Pred| fold_next(p, next);
    let reads_next = |t: &Term| {
        matches!(
            t,
            Term::Slot {
                at: At::Next(_),
                ..
            } | Term::Index(At::Next(_))
        )
    };
    // The term a read becomes: a literal for a fixed value, the input read kept as it is.
    let subst = |t: &Term| -> Option<Term> {
        if !reads_next(t) {
            return Some(t.clone());
        }
        Some(match next(t)? {
            NextRead::Value(v) => Term::Lit(v),
            NextRead::Term(t) => t,
            NextRead::Input => t.clone(),
        })
    };
    let truth = |b: bool| if b { Pred::True } else { Pred::False };
    Some(match pred {
        Pred::Cmp(a, op, b) if reads_next(a) || reads_next(b) => {
            let (a, b) = (subst(a)?, subst(b)?);
            match (&a, op, &b) {
                (Term::Lit(x), TsCmp::Eq, Term::Lit(y)) => truth(x == y),
                (Term::Lit(x), TsCmp::Ne, Term::Lit(y)) => truth(x != y),
                (Term::Lit(_), _, Term::Lit(_)) => return None,
                (Term::Index(x), TsCmp::Eq, Term::Index(y)) if x == y => Pred::True,
                // The slot first, as the atoms read it.
                (Term::Lit(_), TsCmp::Eq | TsCmp::Ne, Term::Slot { .. }) => {
                    Pred::Cmp(b.clone(), *op, a.clone())
                }
                _ => Pred::Cmp(a, *op, b),
            }
        }
        Pred::In(a, vs) if reads_next(a) => match subst(a)? {
            lit @ Term::Lit(_) => truth(vs.contains(&lit)),
            a => Pred::In(a, vs.clone()),
        },
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
        // A chosen value is the caller's: any value's `.when` may be the one it takes.
        if let Effect::Choose { values, .. } = effect {
            for (_, id) in values.iter().flat_map(|c| &c.conds) {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            continue;
        }
        let Effect::Transition { var, param, arms } = effect else {
            continue;
        };
        let Some(taken) = chosen.get(&usize::from(*param)) else {
            continue;
        };
        let domain = &ts.var(*var).domain;
        if domain.names().is_none() {
            continue;
        }
        let name = |v: Value| {
            domain
                .name(v)
                .expect("an arm names declared values")
                .to_string()
        };
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
