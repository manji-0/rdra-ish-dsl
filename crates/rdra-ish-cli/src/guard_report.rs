//! `check --guards`: which rules each guard protects, and which rules depend on which guards.

use std::fmt::Write;

use rdra_ish_core::lifecycle::{
    Bounds, Caps, GuardDeps, GuardKind, GuardSubject, ModelRef, Scope, Target, TransitionSystem,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};

use crate::state_findings::location;

/// What a guard no rule depends on is, in the report's words. It never says the guard is
/// wrong: the model may hold the rule only in the guard.
pub(crate) const NO_RULE: &str = "no rule depends on this guard; it is either a business rule nobody wrote as a rule, or not needed";

/// The report for the selected rules of a model that passed `check`, at the same bounds,
/// caps, and memory limit.
pub(crate) fn run(
    program: &ResolvedProgram,
    model: &SemanticModel,
    scope: &Scope,
    bounds: Bounds,
    caps: Caps,
) -> String {
    let built = TransitionSystem::build(model, bounds, scope);
    let ts = &built.ts;
    let deps = ts.guard_dependencies(&built.selection.targets, caps);
    render(program, model, ts, &built.selection.targets, &deps)
}

fn render(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    targets: &[Target],
    deps: &[GuardDeps],
) -> String {
    let mut out = String::new();
    let protecting = deps.iter().filter(|d| !d.protects.is_empty()).count();
    let none = deps
        .iter()
        .filter(|d| d.protects.is_empty() && d.undecided.is_empty())
        .count();
    let open = deps.len() - protecting - none;
    let cones: usize = deps.iter().map(|d| d.cones).sum();
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    let open = match open {
        0 => String::new(),
        n => format!(", {n} not decided"),
    };
    let _ = writeln!(
        out,
        "guards: {}: {protecting} protect a rule, {none} no rule depends on{open}; {} explored again",
        plural(deps.len(), "guard"),
        plural(cones, "cone"),
    );
    for d in deps {
        let _ = write!(out, "guard: {}", guard_name(model, &d.guard));
        if let Some(at) = location(program, &d.guard.site) {
            let _ = write!(out, "   {at}");
        }
        out.push('\n');
        for &t in &d.protects {
            let _ = writeln!(out, "  protects: {}", rule_name(ts, t));
        }
        for (t, why) in &d.undecided {
            let _ = writeln!(
                out,
                "  not decided: {} ({})",
                rule_name(ts, *t),
                undecided_text(*why)
            );
        }
        if d.protects.is_empty() && d.undecided.is_empty() {
            let _ = writeln!(out, "  {NO_RULE}");
        }
    }
    let rules: Vec<Target> = targets
        .iter()
        .copied()
        .filter(|t| matches!(t, Target::Invariant(_) | Target::AfterAssert(_)))
        .collect();
    if rules.is_empty() {
        return out;
    }
    out.push_str("rules:\n");
    for t in rules {
        let on: Vec<String> = deps
            .iter()
            .filter(|d| d.protects.contains(&t))
            .map(|d| guard_name(model, &d.guard))
            .collect();
        let open: Vec<String> = deps
            .iter()
            .filter(|d| d.undecided.iter().any(|(u, _)| *u == t))
            .map(|d| guard_name(model, &d.guard))
            .collect();
        let mut line = match on.is_empty() {
            true => "no guard".to_string(),
            false => on.join("; "),
        };
        if !open.is_empty() {
            let _ = write!(line, ", not decided without {}", open.join("; "));
        }
        let _ = writeln!(out, "  {} depends on {line}", rule_name(ts, t));
    }
    out
}

/// `condition C on UC`, or `the .when arms of E.col on UC` (`values` for a `chooses` or
/// `sets` line).
pub(crate) fn guard_name(model: &SemanticModel, guard: &GuardSubject) -> String {
    let uc = &model.use_cases[guard.usecase].id;
    let column = |entity, column: &str, what: &str| {
        format!(
            "the .when {what} of {}.{column} on {uc}",
            model.entities[entity].id
        )
    };
    match &guard.kind {
        GuardKind::Condition(ck) => format!("condition {} on {uc}", model.conditions[*ck].id),
        GuardKind::Arms(ModelRef::Transition(i)) => {
            let st = &model.state_transitions[*i];
            column(st.entity, &st.column, "arms")
        }
        GuardKind::Arms(ModelRef::ColumnChoice(i)) => {
            let choice = &model.column_choices[*i];
            column(choice.entity, &choice.column, "values")
        }
        GuardKind::Arms(ModelRef::ColumnEffect(i)) => {
            let line = &model.column_effects[*i];
            column(line.entity, &line.column, "values")
        }
        GuardKind::Arms(_) => format!("the .when conditions on {uc}"),
    }
}

/// A rule by its `.label`, else by the name a violation prints.
fn rule_name(ts: &TransitionSystem, target: Target) -> String {
    let (name, label) = match target {
        Target::Invariant(i) => {
            let inv = &ts.invariants()[i];
            (&inv.name, &inv.label)
        }
        Target::AfterAssert(i) => {
            let a = &ts.after_asserts()[i];
            (&a.name, &a.label)
        }
        Target::Entity(_) | Target::Reach(_) => unreachable!("the report lists rules only"),
    };
    match label {
        Some(label) => format!("「{label}」"),
        None => name.clone(),
    }
}

fn undecided_text(why: rdra_ish_core::lifecycle::UndecidedWhy) -> String {
    use rdra_ish_core::lifecycle::UndecidedWhy;
    match why {
        UndecidedWhy::StateCap(cap) => {
            format!("without the guard its cone stopped at the state cap ({cap} states)")
        }
        UndecidedWhy::PatternCap(cap) => {
            format!("without the guard its cone stopped at the pattern cap ({cap} patterns)")
        }
        UndecidedWhy::MemoryCap(limit, raise) => format!(
            "without the guard its cone stopped at the memory limit ({limit}; raise it with {raise})"
        ),
        UndecidedWhy::TimeBudget => {
            "without the guard its cone stopped at its time budget".to_string()
        }
        UndecidedWhy::Scope | UndecidedWhy::Unselected => {
            unreachable!("a rule's own cone is explored")
        }
    }
}
