//! Every declared `transitions` arm is an edge of the derived system, or a diagnostic says why
//! it is not: its use case never runs, its `from` state is unreachable, or the instance is
//! stuck at `from` and the deadlock names the arm. An arm that is neither would drop a move of
//! the model with nothing reported (#52).

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{ActionOrigin, Domain, Value, VarKind};
use crate::model::SemanticModel;

const BASE: &str = r#"
actor A "A"
usecase Submit "Submit"
usecase Decide "Decide"
usecase Reopen "Reopen"
performs(A, Submit)
performs(A, Decide)
entity Doc "届出" {
  id: Int @pk
  status: Enum(searched, file, secret, drop) @default(searched)
  route: Enum(none, fast, slow) @default(none)
  flag: Bool @default(false)
}
entity Log "記録" { id: Int @pk }
creates(Submit, Doc)
"#;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Expect {
    /// Every arm is an edge.
    Edges,
    /// Some arm is not an edge, and a diagnostic names why.
    Explained,
}
use Expect::*;

/// One form per way a use case can declare its arms.
const FORMS: &[(&str, Expect)] = &[
    // one arm
    ("transitions(Decide, Doc.status, searched -> file)", Edges),
    // a choice of three
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, searched -> drop)",
        Edges,
    ),
    // a choice beside a deterministic arm from another state
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, file -> drop)",
        Edges,
    ),
    // a choice with a sets on another column and a creation of another entity
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
sets(Decide, Doc, flag == true)
creates(Decide, Log)",
        Edges,
    ),
    // two choice columns
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
transitions(Decide, Doc.route, none -> fast)
transitions(Decide, Doc.route, none -> slow)",
        Edges,
    ),
    // a choice whose arms lead back: a cycle through the choice
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
transitions(Reopen, Doc.status, drop -> searched)",
        Edges,
    ),
    // a choice in two transaction groups of one use case
    (
        "entity Board \"審議\" { id: Int @pk  phase: Enum(open, closed, held) @default(open) }
creates(Submit, Board)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
transitions(Decide, Board.phase, open -> closed)
transitions(Decide, Board.phase, open -> held)",
        Edges,
    ),
    // a guard that holds
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
condition Ready \"Ready\"
holds(Ready, Doc.status == searched)
applies_to(Ready, Decide)",
        Edges,
    ),
    // a guard that never holds: the use case never runs
    (
        "transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> drop)
condition Never \"Never\"
holds(Never, Doc.flag == true)
applies_to(Never, Decide)",
        Explained,
    ),
    // an arm from a state nothing reaches
    ("transitions(Decide, Doc.status, secret -> file)", Explained),
    // an arm guard that holds once another use case sets the flag (#62)
    (
        "condition Flagged \"Flagged\"
holds(Flagged, Doc.flag == true)
sets(Reopen, Doc, flag == true)
transitions(Decide, Doc.status, searched -> file).when(Flagged)
transitions(Decide, Doc.status, searched -> drop)",
        Edges,
    ),
    // an arm guard on another entity, beside an unguarded arm
    (
        "entity Gate \"関門\" { id: Int @pk  open: Bool @default(false) }
creates(Reopen, Gate)
sets(Reopen, Gate, open == true)
condition Open \"Open\"
holds(Open, Gate.open == true)
transitions(Decide, Doc.status, searched -> file).when(Open)
transitions(Decide, Doc.status, searched -> drop)",
        Edges,
    ),
    // an arm guard that never holds: the other arm runs, a warning names the guarded one
    (
        "condition Never \"Never\"
holds(Never, Doc.flag == true)
transitions(Decide, Doc.status, searched -> file).when(Never)
transitions(Decide, Doc.status, searched -> drop)",
        Explained,
    ),
    // the only arm, guarded by a guard that never holds: the use case never runs
    (
        "condition Never \"Never\"
holds(Never, Doc.flag == true)
transitions(Decide, Doc.status, searched -> file).when(Never)",
        Explained,
    ),
];

/// The arms of `model` that no edge of the system takes. An arm is taken when an action of
/// its use case moves some instance's column from `from` to `to`.
fn untaken_arms(model: &SemanticModel, bounds: Bounds) -> Vec<usize> {
    let built = TransitionSystem::build(model, bounds, &Scope::ALL);
    let mut taken = vec![false; model.state_transitions.len()];
    for comp in built.ts.components() {
        let run = built.ts.restrict(&comp).run(Caps::new(usize::MAX, 200_000));
        assert!(run.complete, "a component does not finish");
        let ts = &run.system;
        for (i, st) in model.state_transitions.iter().enumerate() {
            let Some(var) = ts.var_ids().find(|&v| {
                matches!(&ts.var(v).kind, VarKind::Column { entity, column, .. }
                    if ts.entity(*entity).key == st.entity && *column == st.column)
            }) else {
                continue;
            };
            let Domain::Enum(values) = &ts.var(var).domain else {
                continue;
            };
            let index = |name: &str| values.iter().position(|v| v.eq_ignore_ascii_case(name));
            let (Some(from), Some(to)) = (index(&st.from), index(&st.to)) else {
                continue;
            };
            let (from, to) = (Value(from as u8), Value(to as u8));
            taken[i] |= run.graph().edges().any(|(a, b, act)| {
                let ActionOrigin::UseCase { uc, .. } = ts.actions()[act.index()].origin else {
                    return false;
                };
                let (pre, post) = (&run.states[a as usize], &run.states[b as usize]);
                uc == st.usecase
                    && (0..bounds.slots())
                        .any(|k| pre.get(ts, var, k) == from && post.get(ts, var, k) == to)
            });
        }
    }
    (0..taken.len()).filter(|&i| !taken[i]).collect()
}

/// Whether a diagnostic of `diags` says why arm `i` of `model` is not taken.
fn explained(model: &SemanticModel, diags: &[Diag], i: usize) -> bool {
    let st = &model.state_transitions[i];
    let uc = &model.use_cases[st.usecase].id;
    let entity = &model.entities[st.entity].id;
    diags.iter().any(|d| match &d.diag {
        StateDiag::NeverRuns { action, .. } => {
            action == uc
                || action
                    .strip_prefix(uc.as_str())
                    .is_some_and(|r| r.starts_with('_'))
        }
        StateDiag::ArmNeverTaken {
            action, from, to, ..
        } => action == uc && from.eq_ignore_ascii_case(&st.from) && to.eq_ignore_ascii_case(&st.to),
        StateDiag::UnreachableEnumVariant { column, variant } => {
            *column == st.column && variant.eq_ignore_ascii_case(&st.from)
        }
        StateDiag::Deadlock {
            entity: e,
            column,
            value,
            exits,
            ..
        } => {
            e == entity
                && *column == st.column
                && value.eq_ignore_ascii_case(&st.from)
                && exits.iter().any(|x| {
                    x.to.as_deref()
                        .is_some_and(|to| to.eq_ignore_ascii_case(&st.to))
                })
        }
        _ => false,
    })
}

/// The arms no edge takes at `Bounds::STATES`, each explained by a diagnostic. With `verify`,
/// two instances per entity take every arm one instance takes.
fn audit(name: &str, model: &SemanticModel, verify: bool) -> Vec<usize> {
    let diags = super::super::derive_states(model, &Scope::ALL, Caps::STATES).diags;
    let untaken = untaken_arms(model, Bounds::STATES);
    for &i in &untaken {
        let st = &model.state_transitions[i];
        assert!(
            explained(model, &diags, i),
            "{name}: arm {} {}.{} {} -> {} is no edge and no diagnostic says why",
            model.use_cases[st.usecase].id,
            model.entities[st.entity].id,
            st.column,
            st.from,
            st.to,
        );
    }
    if verify {
        let two = untaken_arms(model, Bounds::VERIFY);
        assert!(
            two.iter().all(|i| untaken.contains(i)),
            "{name}: arms {two:?} untaken at two instances, {untaken:?} at one"
        );
    }
    untaken
}

#[test]
fn every_form_takes_its_arms_or_says_why() {
    for (form, expect) in FORMS {
        let model = model_from(&format!("{BASE}\n{form}\n"));
        let untaken = audit(form, &model, true);
        match expect {
            Edges => assert!(untaken.is_empty(), "{form}: arms {untaken:?} untaken"),
            Explained => assert!(!untaken.is_empty(), "{form}: every arm taken"),
        }
    }
}

#[test]
fn every_sample_takes_its_arms_or_says_why() {
    // Whole components of the larger samples do not finish at two instances.
    for sample in SAMPLES {
        audit(sample, &load(sample), false);
    }
}
