use super::bfs::STATE_MODEL_TLC_STATES;
use super::jobs::Job;
use super::native::{Native, Natives};
use crate::state_findings::{render_violation, Finding, TraceSource};
use rdra_ish_core::lifecycle::{
    Bounds, BoundsLine, Caps, Scope, Selection, Target, TransitionSystem,
};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use std::time::Duration;

/// A run line: `<module> [<checks>]: <verdict> (<states> states, <notes>)`, the same for
/// either engine.
pub(super) fn run_line(
    module: &str,
    checks: &[String],
    verdict: &str,
    states: Option<u64>,
    notes: &[String],
) -> String {
    let inside: Vec<String> = states
        .map(|n| format!("{n} states"))
        .into_iter()
        .chain(notes.iter().cloned())
        .collect();
    let inside = match inside.is_empty() {
        true => String::new(),
        false => format!(" ({})", inside.join(", ")),
    };
    format!("{module} [{}]: {verdict}{inside}", checks.join(", "))
}

/// The header of a run: the bounds line, the cone and property counts, and the fairness
/// assumptions. Returns the bounds line for the closing summary.
pub(super) fn print_plan(
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    (bounds, caps): (Bounds, Caps),
    (cones, temporal, with_tlc): (usize, bool, bool),
    budget: Duration,
) -> BoundsLine {
    let line = BoundsLine::verify(bounds, Some(caps)).with_scope(scope.line(model, selection));
    println!("{line}");
    let checked = |pick: fn(&Target) -> bool| selection.targets.iter().filter(|t| pick(t)).count();
    println!(
        "cones: {cones} for {} invariant(s) and {} after-assert(s)",
        checked(|t| matches!(t, Target::Invariant(_))),
        checked(|t| matches!(t, Target::AfterAssert(_)))
    );
    let explored = ts
        .properties()
        .iter()
        .filter(|p| p.formula.kind().is_explored())
        .count();
    match (temporal, ts.properties().len() - explored, explored) {
        (_, 0, 0) => println!("temporal properties: none"),
        (true, n, m) => {
            if n > 0 {
                println!(
                    "temporal properties: {n}, each checked on its cone, and on the whole model only when the cone does not decide it (budget {} s)",
                    budget.as_secs()
                );
            }
            match (m, with_tlc) {
                (0, _) => {}
                (m, false) => println!(
                    "order and stability properties: {m}, each decided by BFS on its cone"
                ),
                (m, true) => println!(
                    "order and stability properties: {m}, each decided by BFS on its cone, and by TLC on the same cone"
                ),
            }
        }
        (false, n, m) => println!(
            "temporal properties: {}, not checked: a scoped run checks none",
            n + m
        ),
    }
    match (scope.is_whole(), ts.reach_properties().len()) {
        (_, 0) => {}
        (true, n) => println!(
            "can_reach properties: {n}, decided natively by BFS on each one's cone (TLC cannot check AG EF)"
        ),
        (false, n) => println!("can_reach properties: {n}, not checked: a scoped run checks none"),
    }
    if with_tlc {
        println!("tlc: runs the same cones beside the native checker (--with-tlc), and the state models of each entity cone up to {STATE_MODEL_TLC_STATES} states; a check the two decide differently fails the run");
    }
    // A verdict under `.fair(...)` holds only on the runs that do not starve those use cases.
    for p in ts.properties().iter().filter(|p| !p.fair.is_empty()) {
        let ucs: Vec<&str> = p
            .fair
            .iter()
            .map(|uk| model.use_cases[*uk].id.as_str())
            .collect();
        println!(
            "  {} assumes weak fairness per instance on {}",
            p.name,
            ucs.join(", ")
        );
    }
    line
}

/// What the native checker decided of the `always`, `eventually`, and `leads_to` properties.
#[derive(Default)]
pub(super) struct Temporal {
    pub(super) violated: usize,
    /// Counterexamples that failed their own check: a tool bug.
    pub(super) faults: usize,
    /// Properties a cone run stopped at the state cap or the memory limit left undecided.
    pub(super) capped: usize,
}

/// Prints each property's cone line and then its whole-model line, renders each
/// counterexample, and reports each fault.
pub(super) fn report_properties(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    jobs: &[Job],
    natives: &Natives,
    whole_modules: &[String],
) -> Temporal {
    let mut out = Temporal::default();
    let mut report = |module: &str, checks: &[String], native: &Native| {
        println!("{}", native.line(module, checks));
        if let Some(diag) = &native.found {
            let text = render_violation(program, model, diag, TraceSource::Bfs)
                .expect("a property violation renders");
            eprintln!("{}", Finding::new(diag, &[], text));
            out.violated += 1;
        }
        if let Some(fault) = &native.fault {
            eprintln!("error[LassoFault]: {module} [{}]: {fault}; a tool bug, not a finding about the model", checks.join(", "));
            out.faults += 1;
        }
    };
    let mut capped = 0;
    for (job, native) in jobs.iter().zip(&natives.properties) {
        if let Some(native) = native {
            report(&job.module, &job.checks, native);
            capped += usize::from(native.cone_capped);
        }
    }
    for (i, native) in &natives.wholes {
        let checks = [ts.properties()[*i].name.clone()];
        report(&whole_modules[*i], &checks, native);
    }
    out.capped = capped;
    out
}
