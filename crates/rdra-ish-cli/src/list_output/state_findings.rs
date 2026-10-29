//! State diagnostics as `check` and the generator gates print them, and the one renderer of
//! counterexample traces.

use std::fmt;

use rdra_ish_core::lifecycle::{
    derive_states_built, ActionOrigin, Blocked, BoundsLine, Caps, Change, GuardWhy, InvKind,
    InvText, Loop, ModelRef, ParamRole, Scope, Severity, Site, StateDiag, Trace, TraceStep,
};
use rdra_ish_core::{format_location, ResolvedProgram, SemanticModel};

/// A lifecycle finding as `check`, `states`, `verify`, and the generator gates print it:
/// `error[<name>]: <message>`, with `warning` or `info` for the other severities.
#[derive(Debug)]
pub(crate) struct Finding {
    pub(crate) severity: Severity,
    /// `StateDiag::name`.
    pub(crate) name: String,
    pub(crate) message: String,
}

impl Finding {
    /// `diag` reported under the entity ids `entities`.
    pub(crate) fn new(diag: &StateDiag, entities: &[&str], message: String) -> Self {
        Finding {
            severity: diag.severity(),
            name: diag.name(entities),
            message,
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{word}[{}]: {}", self.name, self.message)
    }
}

/// Every diagnostic of the lifecycle views at the `states` defaults: those on an entity view
/// first, then each violated constraint or assert once with its counterexample and each
/// unwitnessed state once, then those that name no entity.
pub(crate) fn state_consistency_findings(
    program: &ResolvedProgram,
    model: &SemanticModel,
) -> Vec<Finding> {
    state_findings(program, model, &Scope::ALL).findings
}

/// What `check` prints of the lifecycle: the bounds and scope, the findings, and the
/// `not checked:` block when the scope or a cap left something out.
pub(crate) struct StateFindings {
    pub(crate) bounds: BoundsLine,
    pub(crate) findings: Vec<Finding>,
    pub(crate) not_checked: Option<String>,
}

/// The findings of `state_consistency_findings` for `scope`, with its bounds line and what
/// it did not check.
pub(crate) fn state_findings(
    program: &ResolvedProgram,
    model: &SemanticModel,
    scope: &Scope,
) -> StateFindings {
    let (report, ts) = derive_states_built(model, scope, Caps::DEFAULT_PATTERNS);
    let on_views = report.views.iter().flat_map(|result| {
        let diags = result.diagnostics.iter().filter(|d| !has_block(d));
        diags.map(move |diag| {
            Finding::new(
                diag,
                &[&result.entity_id],
                format!(
                    "state derivation for entity '{}': {}",
                    result.entity_id,
                    state_diag_message(diag)
                ),
            )
        })
    });
    let violations = report.diags.iter().filter_map(|d| {
        let text = render_violation(program, model, &d.diag, TraceSource::Bfs)?;
        Some(Finding::new(&d.diag, &[], text))
    });
    let unattributed = report
        .unattributed()
        .filter(|d| !has_block(d))
        .map(|diag| Finding::new(diag, &[], state_diag_message(diag)));
    let outside = report
        .outside_views(model)
        .filter(|(_, diag)| !has_block(diag))
        .map(|(ids, diag)| {
            Finding::new(
                diag,
                &ids,
                format!(
                    "state derivation for entity '{}': {}",
                    ids.join("', '"),
                    state_diag_message(diag)
                ),
            )
        });
    let findings = on_views
        .chain(outside)
        .chain(violations)
        .chain(unattributed)
        .collect();
    let not_checked = report.not_checked(&ts, model);
    StateFindings {
        bounds: report.bounds_line(model),
        findings,
        not_checked: (!not_checked.is_empty()).then(|| not_checked.to_string()),
    }
}

/// `render_violation` prints it once, as a block with sites, instead of once per view.
fn has_block(diag: &StateDiag) -> bool {
    diag.trace().is_some()
        || matches!(
            diag,
            StateDiag::NoWitness { .. } | StateDiag::NeverRuns { .. }
        )
}

/// `use case Refund`, or `transaction 2 of Pay (Pay_2)` for one group of a split use case.
fn action_label(model: &SemanticModel, action: &str, origin: &ActionOrigin) -> String {
    if let ActionOrigin::UseCase { uc, .. } = origin {
        let id = &model.use_cases[*uc].id;
        if let Some(n) = action.strip_prefix(&format!("{id}_")) {
            return format!("transaction {n} of {id} ({action})");
        }
    }
    format!("use case {action}")
}

/// A never-run action, its use case site, and only what blocks it: the instances no reachable
/// state has, or each guard that holds in no reachable state with its site.
fn render_never_runs(
    program: &ResolvedProgram,
    model: &SemanticModel,
    diag: &StateDiag,
) -> Option<String> {
    let StateDiag::NeverRuns {
        action,
        origin,
        site,
        why,
    } = diag
    else {
        return None;
    };
    let reason = diag.to_string();
    let reason = reason.strip_prefix(action.as_str()).unwrap_or(&reason);
    let mut rows = vec![(
        format!("  declared: {}", action_label(model, action, origin)),
        location(program, site),
    )];
    rows.extend(blocked_rows(program, model, why, "  "));
    Some(format!(
        "{}{reason}{}",
        action_label(model, action, origin),
        aligned(rows)
    ))
}

/// What blocks an action, one row each: the instances it lacks, or the guards that never
/// hold with their sites. Nothing when each guard holds somewhere, only never together.
fn blocked_rows(
    program: &ResolvedProgram,
    model: &SemanticModel,
    why: &Blocked,
    indent: &str,
) -> Vec<(String, Option<String>)> {
    match why {
        Blocked::NoInstance(params) | Blocked::Apart(params) => params
            .iter()
            .map(|(entity, role)| {
                let text = match role {
                    ParamRole::New => {
                        format!("{indent}needs: a free {entity} slot within the instance bound")
                    }
                    ParamRole::Existing | ParamRole::Parent => {
                        format!("{indent}needs: an existing {entity}")
                    }
                };
                (text, None)
            })
            .collect(),
        Blocked::Guards(guards) => guards
            .iter()
            .map(|(why, site)| {
                (
                    format!("{indent}guard: {}", guard_text(model, *why, site)),
                    location(program, site),
                )
            })
            .collect(),
        Blocked::Crowded(entities) => entities
            .iter()
            .map(|entity| {
                (
                    format!("{indent}needs: a free {entity} slot within the instance bound"),
                    None,
                )
            })
            .collect(),
        Blocked::Together => Vec::new(),
    }
}

/// A deadlock, each way out of its state with what blocks it, and the path into it.
fn render_deadlock(
    program: &ResolvedProgram,
    model: &SemanticModel,
    diag: &StateDiag,
) -> Option<String> {
    let StateDiag::Deadlock {
        entity,
        value,
        exits,
        trace,
        ..
    } = diag
    else {
        return None;
    };
    let mut rows = Vec::new();
    for x in exits {
        let how = match &x.to {
            Some(to) => format!("{value} -> {to}"),
            None => format!("deletes the {entity}"),
        };
        rows.push((
            format!("  leaves {value}: {} ({how})", x.action),
            location(program, &x.site),
        ));
        rows.extend(blocked_rows(program, model, &x.why, "    "));
        if x.why == Blocked::Together {
            rows.push((
                "    its guards never hold together while stuck".to_string(),
                None,
            ));
        }
    }
    let mut out = format!("{diag}{}", aligned(rows));
    out.push_str(&trace_block(
        program,
        model,
        ("reached by", "path"),
        trace,
        "",
    ));
    Some(out)
}

/// A guard as the model states it: the `from` states of the use case's transitions on the
/// column, the condition, the relate, or the clock.
fn guard_text(model: &SemanticModel, why: GuardWhy, site: &Site) -> String {
    match (why, &site.fact) {
        (GuardWhy::TransitionFrom, ModelRef::Transition(i)) => {
            let st = &model.state_transitions[*i];
            let froms: Vec<&str> = model
                .state_transitions
                .iter()
                .filter(|t| {
                    t.usecase == st.usecase && t.entity == st.entity && t.column == st.column
                })
                .map(|t| t.from.as_str())
                .collect();
            let entity = &model.entities[st.entity].id;
            let uc = &model.use_cases[st.usecase].id;
            format!(
                "{entity}.{} is {} (transitions of {uc})",
                st.column,
                froms.join(" or ")
            )
        }
        (GuardWhy::Condition { cond }, _) => format!("condition {}", model.conditions[cond].id),
        (GuardWhy::Owner, _) => "the bound instances belong to each other (relate)".to_string(),
        (GuardWhy::Clock, _) => "now has room to tick".to_string(),
        (GuardWhy::TransitionFrom, _) => {
            "the column is in a from state of its transitions".to_string()
        }
    }
}

fn location(program: &ResolvedProgram, site: &Site) -> Option<String> {
    format_location(program, site.span.as_ref()?)
}

/// The engine that found a counterexample; only the trace header names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraceSource {
    Bfs,
    Tlc,
}

/// A violation, the rule's site, and its counterexample: one step per line with its use case
/// site, and under it each changed value with the site of the effect that wrote it. An
/// unwitnessed state prints with the arms that would enter it, and a never-run action with
/// what blocks it. `None` for any other diagnostic.
pub(crate) fn render_violation(
    program: &ResolvedProgram,
    model: &SemanticModel,
    diag: &StateDiag,
    source: TraceSource,
) -> Option<String> {
    if let Some(text) = render_never_runs(program, model, diag) {
        return Some(text);
    }
    if let Some(text) = render_deadlock(program, model, diag) {
        return Some(text);
    }
    if let StateDiag::NoWitness {
        entity,
        column,
        value,
        into,
    } = diag
    {
        let mut rows: Vec<(String, Option<String>)> = into
            .iter()
            .map(|entry| {
                let how = match &entry.from {
                    Some(from) => format!("{from} -> {value}"),
                    None => "creates it".to_string(),
                };
                (
                    format!(
                        "  state model: {entity}.{column}, entered by {} ({how})",
                        entry.action
                    ),
                    location(program, &entry.site),
                )
            })
            .collect();
        if rows.is_empty() {
            rows.push((
                format!("  state model: {entity}.{column}; no use case enters {value}"),
                None,
            ));
        }
        return Some(format!("{diag}{}", aligned(rows)));
    }
    let (head, site, trace) = match diag {
        StateDiag::ConstraintViolated {
            name, site, trace, ..
        } => (
            format!("invariant {name} is violated: {}", state_diag_message(diag)),
            site,
            trace,
        ),
        StateDiag::AfterAssertViolated { site, trace, .. }
        | StateDiag::PropertyViolated { site, trace, .. } => {
            (state_diag_message(diag), site, trace)
        }
        _ => return None,
    };
    let mut out = head;
    if let Some(at) = location(program, site) {
        out.push_str(&format!("\n  rule: {at}"));
    }
    let from = match source {
        TraceSource::Bfs => "",
        TraceSource::Tlc => ", from TLC",
    };
    out.push_str(&trace_block(
        program,
        model,
        ("counterexample", "counterexample"),
        trace,
        from,
    ));
    match diag {
        StateDiag::PropertyViolated {
            lasso: Loop::Stutter,
            ..
        } => out.push_str("\n    then nothing changes forever"),
        StateDiag::PropertyViolated {
            lasso: Loop::Back { to },
            ..
        } => out.push_str(&format!("\n    then loops back to step {to}")),
        _ => {}
    }
    Some(out)
}

/// `  <header> (N steps<from>):`, then the start and each step with its changes and sites.
/// `noun` names the trace in the note on a trace past the bound.
fn trace_block(
    program: &ResolvedProgram,
    model: &SemanticModel,
    (header, noun): (&str, &str),
    trace: &Trace,
    from: &str,
) -> String {
    let steps = trace.steps.len();
    let plural = if steps == 1 { "" } else { "s" };
    let mut out = format!("\n  {header} ({steps} step{plural}{from}):");
    let mut rows: Vec<(String, Option<String>)> = Vec::new();
    if !trace.start.is_empty() {
        rows.push(("    start".to_string(), None));
        rows.extend(trace.start.iter().map(|c| change_row(program, c)));
    }
    for (i, step) in trace.steps.iter().enumerate() {
        rows.push((
            format!("    {}. {}", i + 1, step_text(model, step)),
            location(program, &step.site),
        ));
        rows.extend(step.changes.iter().map(|c| change_row(program, c)));
    }
    out.push_str(&aligned(rows));
    for entity in &trace.over_bound {
        out.push_str(&format!(
            "\n  note: the {noun} creates more {entity} instances than the instance bound allows; this check's cone does not count {entity} slots, so the whole model may not reach it at this bound"
        ));
    }
    out
}

/// One line per row, each row's site in a column after the longest text.
fn aligned(rows: Vec<(String, Option<String>)>) -> String {
    let width = rows
        .iter()
        .map(|(text, _)| text.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for (text, at) in rows {
        match at {
            Some(at) => {
                let pad = width - text.chars().count();
                out.push_str(&format!("\n{text}{}   {at}", " ".repeat(pad)));
            }
            None => out.push_str(&format!("\n{text}")),
        }
    }
    out
}

/// `Pay_1, transaction 1 of Pay [Payment#1]`.
fn step_text(model: &SemanticModel, step: &TraceStep) -> String {
    let mut text = step.action.clone();
    if let ActionOrigin::UseCase { uc, .. } = &step.origin {
        let id = &model.use_cases[*uc].id;
        if let Some(n) = step.action.strip_prefix(&format!("{id}_")) {
            text.push_str(&format!(", transaction {n} of {id}"));
        }
    }
    if !step.bound.is_empty() {
        let bound: Vec<String> = step.bound.iter().map(ToString::to_string).collect();
        text.push_str(&format!(" [{}]", bound.join(", ")));
    }
    text
}

fn change_row(program: &ResolvedProgram, change: &Change) -> (String, Option<String>) {
    let indent = "         ";
    match change {
        Change::Create { at, values } if values.is_empty() => (format!("{indent}+ {at}"), None),
        Change::Create { at, values } => {
            let values: Vec<String> = values.iter().map(|(f, v)| format!("{f}={v}")).collect();
            (format!("{indent}+ {at} ({})", values.join(", ")), None)
        }
        Change::Delete { at } => (format!("{indent}- {at}"), None),
        Change::Set {
            at,
            field,
            from,
            to,
            cause,
        } => (
            format!("{indent}{at}.{field}: {from} -> {to}"),
            cause.as_ref().and_then(|site| location(program, site)),
        ),
        Change::Tick { from, to } => (format!("{indent}now: {from} -> {to}"), None),
    }
}

/// Fail closed when state derivation has Error-severity diagnostics (generators).
pub(crate) fn reject_state_errors(
    program: &ResolvedProgram,
    model: &SemanticModel,
) -> anyhow::Result<()> {
    let mut has_error = false;
    for finding in state_consistency_findings(program, model) {
        if finding.severity == Severity::Error {
            eprintln!("{finding}");
            has_error = true;
        }
    }
    if has_error {
        anyhow::bail!("model has state errors; refusing to emit artifacts");
    }
    Ok(())
}

/// Data-layer outputs read no reachability result, so lifecycle errors only earn a note.
pub(crate) fn note_state_errors(program: &ResolvedProgram, model: &SemanticModel) {
    let errors = state_consistency_findings(program, model)
        .into_iter()
        .filter(|finding| finding.severity == Severity::Error)
        .count();
    if errors > 0 {
        eprintln!(
            "note: the lifecycle has {errors} error(s) (see `rdra-ish check`); the data layer does not depend on them"
        );
    }
}

/// A constraint violation in the wording `check` has always used for its kind.
fn violation_message(kind: &InvKind, text: &InvText, p: &str) -> Option<String> {
    Some(match (kind, text) {
        (InvKind::Forbidden, InvText::Conditions(c)) => {
            format!("forbidden state is reachable: {c} witnessed by {p}")
        }
        (InvKind::Required, InvText::Conditions(c)) => {
            format!("required state is missing: {c} is not satisfied by {p}")
        }
        (InvKind::Exclusive, InvText::Conditions(c)) => {
            format!("exclusive state conditions co-occur: {c} witnessed by {p}")
        }
        (InvKind::Invariant, InvText::WhenThen { guards, requireds }) => {
            format!("invariant violated: when {guards} then {requireds} is broken by {p}")
        }
        (InvKind::Check, InvText::Conditions(c)) => {
            format!("check constraint is false: {c} is broken by {p}")
        }
        (
            InvKind::CrossForbidden,
            InvText::Cross {
                entities,
                conditions,
            },
        ) => format!(
            "multi-entity forbidden state is reachable across [{entities}]: {conditions} witnessed by {p}"
        ),
        (
            InvKind::CrossInvariant,
            InvText::CrossWhenThen {
                entities,
                guards,
                requireds,
            },
        ) => format!(
            "multi-entity invariant violated across [{entities}]: when {guards} then {requireds} is broken by {p}"
        ),
        (
            InvKind::Quantifier,
            InvText::Quantifier {
                anchor,
                related,
                constraint,
            },
        ) => format!(
            "to-many quantifier constraint is violated from '{anchor}' to '{related}': {constraint} witnessed by {p}"
        ),
        (
            InvKind::Unique,
            InvText::Unique {
                child,
                parent,
                role,
            },
        ) => format!(
            "1:1 key is shared: two '{child}' rows belong to one '{parent}' through '{role}', witnessed by {p}"
        ),
        (
            InvKind::MinCard,
            InvText::MinCard {
                child,
                parent,
                role,
            },
        ) => format!(
            "minimum children unmet: a '{parent}' has no '{child}' through '{role}', witnessed by {p}"
        ),
        _ => return None,
    })
}

pub(crate) fn state_diag_message(diag: &StateDiag) -> String {
    match diag {
        StateDiag::UnreachableEnumVariant { column, variant } => format!(
            "enum variant '{}.{}' is unreachable; add a create/default/transition/sets path or remove the variant",
            column, variant
        ),
        StateDiag::ConflictingEffects { usecase, column, .. } => format!(
            "usecase '{}' assigns conflicting effects to column '{}'; the last effect wins",
            usecase, column
        ),
        StateDiag::DoubleModeledEnum { column, .. } => format!(
            "enum column '{}' is driven by both transitions and sets; transitions are treated as the source of truth",
            column
        ),
        StateDiag::NoCreationPath => {
            "no creates path; state derivation is seeded from defaults only".to_string()
        }
        StateDiag::PatternCapReached { cap, bound } => format!(
            "pattern cap reached at {} while the theoretical state-space bound is {} (constraints on truncated set are not sound; raise --max-patterns or narrow --buc)",
            cap, bound
        ),
        StateDiag::ConstraintViolated {
            kind,
            text,
            pattern_desc,
            ..
        } => violation_message(kind, text, pattern_desc).unwrap_or_else(|| diag.to_string()),
        StateDiag::AfterAssertViolated {
            anchor,
            requireds,
            actual,
            ..
        } => format!(
            "temporal assertion violated after '{anchor}': expected {requireds}, but found {actual}"
        ),
        StateDiag::StateCapReached { .. }
        | StateDiag::NoWitness { .. }
        | StateDiag::NeverRuns { .. }
        | StateDiag::Deadlock { .. }
        | StateDiag::ContradictoryDrivenComparison { .. }
        | StateDiag::AssertOnInertUseCase { .. }
        | StateDiag::IntLiteralOutOfBounds { .. }
        | StateDiag::UnbindableScope { .. }
        | StateDiag::UnbindableCondition { .. }
        | StateDiag::UnwrittenConstrainedVar { .. }
        | StateDiag::PreExistingChildOfCreatedParent { .. }
        | StateDiag::MinCardOfPreExistingParent { .. }
        | StateDiag::MinCardNeedsCreate { .. }
        | StateDiag::UnsupportedComparison { .. }
        | StateDiag::DerivedColumnRead { .. }
        | StateDiag::PropertyViolated { .. }
        | StateDiag::UnresolvedPropertyAtom { .. } => diag.to_string(),
    }
}
