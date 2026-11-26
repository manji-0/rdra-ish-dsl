//! Lifecycle findings in the editor: `check`'s exploration at one instance per entity, run in
//! the background after each analysis, under a memory limit and a time budget (#214).
//!
//! The run is the one `check` makes (`derive_states_checked` at `Bounds::STATES` and the
//! default caps), one cone at a time on one thread. Each finding becomes a diagnostic at the
//! declaration it is about, and each invariant and `after(..).assert` gets a code lens with
//! its outcome. A cone the budget or the limit stopped is information, not an error.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rdra_ish_core::lifecycle::{
    derive_states_checked, Bounds, Caps, CheckOutcome, Scope, Site, StateDiag, Subject,
    TransitionSystem, Undecided,
};
use rdra_ish_core::location::LocatedSpan;
use rdra_ish_core::memory::{MemoryPool, MemorySize};
use rdra_ish_core::{one_at_a_time, WorkspaceAnalysis};
use serde_json::Value;
use tower_lsp::lsp_types::{
    CodeLens, Command, Diagnostic, DiagnosticSeverity, NumberOrString, Range,
};

use crate::convert::span_to_range;

/// Where the findings come from, as the editor shows it: they hold at one instance per entity,
/// which `verify` (two instances) may not confirm.
pub const SOURCE: &str = "rdra-ish check (1 instance)";

/// `lifecycleCheck` in the initialization options or in the `rdra-ish` settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Run the check after each analysis.
    pub enabled: bool,
    /// What the run's state stores may hold.
    pub memory: MemorySize,
    /// When the run stops, counted from its start.
    pub budget: Duration,
    /// How long the text must stay unchanged after an analysis before the check starts.
    pub quiet: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            memory: MemorySize::from_bytes(512 << 20).expect("not zero"),
            budget: Duration::from_millis(2000),
            quiet: Duration::from_millis(200),
        }
    }
}

impl Settings {
    /// These settings with the fields `value` sets: `enabled`, `memory` (`"512MiB"`),
    /// `timeBudgetMs`, and `quietMs`. A field that is missing or does not parse keeps its value.
    pub fn with(self, value: &Value) -> Self {
        let ms = |key: &str, keep: Duration| {
            value
                .get(key)
                .and_then(Value::as_u64)
                .map_or(keep, Duration::from_millis)
        };
        Settings {
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(self.enabled),
            memory: value
                .get("memory")
                .and_then(Value::as_str)
                .and_then(|text| text.parse().ok())
                .unwrap_or(self.memory),
            budget: ms("timeBudgetMs", self.budget),
            quiet: ms("quietMs", self.quiet),
        }
    }

    /// The `lifecycleCheck` object of initialization options or of a settings change, whether
    /// under an `rdra-ish` section or at the top.
    pub fn section(value: &Value) -> Option<&Value> {
        value.get("rdra-ish").unwrap_or(value).get("lifecycleCheck")
    }

    /// The caps of a run that starts now: `check`'s defaults at one instance, charged to one
    /// pool of `memory` for the server's life, and stopped at the budget.
    fn caps(&self) -> Caps {
        Caps::for_instances(1)
            .charged_to(pool(self.memory))
            .until(Instant::now() + self.budget)
    }
}

/// The pool of a memory limit. Runs go one at a time, so one pool per limit serves every run;
/// a pool lives as long as the process, so a new limit leaks one.
fn pool(limit: MemorySize) -> &'static MemoryPool {
    static POOLS: OnceLock<Mutex<HashMap<u64, &'static MemoryPool>>> = OnceLock::new();
    let mut pools = POOLS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pools.entry(limit.bytes()).or_insert_with(|| {
        MemoryPool::for_command(
            limit,
            "`rdra-ish.lifecycleCheck.memory` in the editor settings",
        )
    })
}

/// One run's findings by source file, with ranges in the texts the run read.
#[derive(Debug, Default)]
pub struct Findings {
    pub diagnostics: BTreeMap<PathBuf, Vec<Diagnostic>>,
    pub lenses: BTreeMap<PathBuf, Vec<RuleLens>>,
}

/// The outcome of the checks declared on one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleLens {
    pub range: Range,
    pub outcome: CheckOutcome,
}

impl RuleLens {
    pub fn to_code_lens(&self) -> CodeLens {
        let word = match self.outcome {
            CheckOutcome::Holds => "ok",
            CheckOutcome::Violated => "violated",
            CheckOutcome::Undecided => "not decided",
        };
        CodeLens {
            range: self.range,
            command: Some(Command {
                title: format!("check: {word} (1 instance)"),
                command: String::new(),
                arguments: None,
            }),
            data: None,
        }
    }
}

/// Runs `check`'s exploration of `analysis` under `settings` and places what it finds.
pub fn check(analysis: &WorkspaceAnalysis, settings: &Settings) -> Findings {
    let caps = settings.caps();
    let checked =
        one_at_a_time(|| derive_states_checked(&analysis.model, &Scope::ALL, Bounds::STATES, caps));
    let place = Placer {
        analysis,
        ts: &checked.ts,
    };
    let mut findings = Findings::default();

    // Each finding once: the report holds those of the build and the verdicts, each view those
    // that name its entity, and a finding on two entities is on two views. A finding that
    // carries no entity of its own (`NoCreationPath`, a stop) is one per entity.
    let mut seen: Vec<(&StateDiag, Option<&str>)> = Vec::new();
    for d in &checked.report.diags {
        let entity = d
            .entities
            .first()
            .map(|k| analysis.model.entities[*k].id.as_str());
        seen.push((&d.diag, entity));
    }
    for view in &checked.report.views {
        for diag in &view.diagnostics {
            seen.push((diag, Some(view.entity_id.as_str())));
        }
    }
    let mut placed: Vec<(PathBuf, Diagnostic)> = Vec::new();
    for (i, (diag, entity)) in seen.iter().enumerate() {
        let same = |(earlier, at): &(&StateDiag, Option<&str>)| {
            earlier == diag && (!of_its_entity(diag) || at == entity)
        };
        if seen[..i].iter().any(same) {
            continue;
        }
        let Some((path, range)) = place.diag(diag, *entity) else {
            continue;
        };
        let severity = match (is_stop(diag), diag.severity()) {
            (true, _) | (_, rdra_ish_core::lifecycle::Severity::Info) => {
                DiagnosticSeverity::INFORMATION
            }
            (_, rdra_ish_core::lifecycle::Severity::Warning) => DiagnosticSeverity::WARNING,
            (_, rdra_ish_core::lifecycle::Severity::Error) => DiagnosticSeverity::ERROR,
        };
        let message = match entity {
            Some(id) if names_no_entity(diag) => format!("entity {id}: {diag}"),
            _ => diag.to_string(),
        };
        // The name `check` prints: on an entity's view, with the entity's id where the kind
        // takes it.
        let name = diag.name(entity.as_slice());
        placed.push((
            path,
            Diagnostic {
                range,
                severity: Some(severity),
                code: Some(NumberOrString::String(name)),
                source: Some(SOURCE.to_string()),
                message,
                ..Default::default()
            },
        ));
    }
    for undecided in &checked.report.undecided {
        let Some((path, range)) = place.undecided(undecided) else {
            continue;
        };
        placed.push((
            path,
            Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::INFORMATION),
                source: Some(SOURCE.to_string()),
                message: undecided.to_string(),
                ..Default::default()
            },
        ));
    }
    for (path, diagnostic) in placed {
        let list = findings.diagnostics.entry(path).or_default();
        if !list.contains(&diagnostic) {
            list.push(diagnostic);
        }
    }
    for list in findings.diagnostics.values_mut() {
        list.sort_by(|a, b| {
            (a.range.start.line, a.range.start.character, &a.message).cmp(&(
                b.range.start.line,
                b.range.start.character,
                &b.message,
            ))
        });
    }

    for verdict in &checked.checks {
        let Some((path, range)) = place.line(&verdict.site) else {
            continue;
        };
        let lenses = findings.lenses.entry(path).or_default();
        match lenses.iter_mut().find(|l| l.range == range) {
            // Several checks from one declaration: the worst outcome stands for them.
            Some(lens) => lens.outcome = worst(lens.outcome, verdict.outcome),
            None => lenses.push(RuleLens {
                range,
                outcome: verdict.outcome,
            }),
        }
    }
    for lenses in findings.lenses.values_mut() {
        lenses.sort_by_key(|l| (l.range.start.line, l.range.start.character));
    }
    findings
}

fn worst(a: CheckOutcome, b: CheckOutcome) -> CheckOutcome {
    let rank = |o| match o {
        CheckOutcome::Holds => 0,
        CheckOutcome::Undecided => 1,
        CheckOutcome::Violated => 2,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

/// A run stopped at a cap, the memory limit, or the time budget: what it found holds, and what
/// it did not reach is undecided, so the stop itself is information.
fn is_stop(diag: &StateDiag) -> bool {
    matches!(
        diag,
        StateDiag::PatternCapReached { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::MemoryCapReached { .. }
            | StateDiag::TimeBudgetReached { .. }
    )
}

/// A finding that names no entity of its own: it is about the entity whose view reports it,
/// and goes on that entity's declaration, once per entity.
fn of_its_entity(diag: &StateDiag) -> bool {
    matches!(
        diag,
        StateDiag::NoCreationPath | StateDiag::UnreachableEnumVariant { .. }
    ) || is_stop(diag)
}

/// The finding's message names no entity or says "this entity", so the diagnostic names it.
fn names_no_entity(diag: &StateDiag) -> bool {
    of_its_entity(diag)
        || matches!(
            diag,
            StateDiag::PreExistingChildOfCreatedParent { .. }
                | StateDiag::IdentityStartSlots { .. }
                | StateDiag::MinCardOfPreExistingParent { .. }
                | StateDiag::MinCardNeedsCreate { .. }
        )
}

/// Turns the sites of a run's findings into ranges of the source files they are in.
struct Placer<'a> {
    analysis: &'a WorkspaceAnalysis,
    ts: &'a TransitionSystem,
}

impl Placer<'_> {
    /// The source file and range of `span`.
    fn span(&self, span: &LocatedSpan) -> Option<(PathBuf, Range)> {
        let (path, text, _) = self.analysis.program.sources.get(span.source_id)?;
        Some((path.clone(), span_to_range(text, span.span.clone())))
    }

    fn site(&self, site: &Site) -> Option<(PathBuf, Range)> {
        self.span(site.span.as_ref()?)
    }

    /// The first line of `site`, from its start to the end of that line: where a lens goes.
    fn line(&self, site: &Site) -> Option<(PathBuf, Range)> {
        let (path, mut range) = self.site(site)?;
        if range.end.line != range.start.line {
            let span = site.span.as_ref()?;
            let text = &self.analysis.program.sources[span.source_id].1;
            let mut start = span.span.start.min(text.len());
            while !text.is_char_boundary(start) {
                start -= 1;
            }
            let end = text[start..].find('\n').map_or(text.len(), |n| start + n);
            range = span_to_range(text, start..end);
        }
        Some((path, range))
    }

    fn declaration(&self, kind: &str, id: &str) -> Option<(PathBuf, Range)> {
        self.span(&self.analysis.model.decl_sites.located(kind, id)?)
    }

    fn entity(&self, id: &str) -> Option<(PathBuf, Range)> {
        self.declaration("entity", id)
    }

    /// The declaration of the use case an action of the system comes from.
    fn action(&self, name: &str) -> Option<(PathBuf, Range)> {
        let action = self.ts.actions().iter().find(|a| a.name == name);
        action
            .and_then(|a| self.site(&a.site))
            .or_else(|| self.declaration("usecase", name))
    }

    /// Where a finding goes: the rule, the use case, or the line it is about, else the
    /// declaration of the entity it is reported on. Every kind is named, so a new kind does
    /// not compile until it says where it goes.
    fn diag(&self, diag: &StateDiag, entity: Option<&str>) -> Option<(PathBuf, Range)> {
        let first = |sites: &[Site]| sites.iter().find_map(|s| self.site(s));
        let at = match diag {
            StateDiag::ConstraintViolated { site, .. }
            | StateDiag::AfterAssertViolated { site, .. }
            | StateDiag::VacuousRule { site, .. }
            | StateDiag::PropertyViolated { site, .. }
            | StateDiag::GoalUnreachable { site, .. }
            | StateDiag::NeverRuns { site, .. }
            | StateDiag::ArmNeverTaken { site, .. }
            | StateDiag::ValueNeverChosen { site, .. }
            | StateDiag::TriggerNeverRuns { site, .. }
            | StateDiag::DoubleModeledEnum { site, .. }
            | StateDiag::PreExistingChildOfCreatedParent { site, .. }
            | StateDiag::IdentityStartSlots { site, .. }
            | StateDiag::CountAboveBound { site, .. }
            | StateDiag::MinCardOfPreExistingParent { site, .. }
            | StateDiag::MinCardNeedsCreate { site, .. }
            | StateDiag::AssertOnInertUseCase { site, .. }
            | StateDiag::FairOnInertUseCase { site, .. }
            | StateDiag::IntLiteralOutOfBounds { site, .. }
            | StateDiag::UnbindableScope { site, .. }
            | StateDiag::AmbiguousOwnerLink { site, .. }
            | StateDiag::UnboundCopySource { site, .. }
            | StateDiag::UnbindableCondition { site, .. }
            | StateDiag::UnwrittenConstrainedVar { site, .. }
            | StateDiag::UnsupportedComparison { site, .. }
            | StateDiag::DerivedColumnRead { site, .. }
            | StateDiag::UnjoinedGuardRead { site, .. }
            | StateDiag::UnjoinedWrite { site, .. }
            | StateDiag::UnboundAfterRead { site, .. }
            | StateDiag::UnanchoredTrigger { site, .. }
            | StateDiag::UnjoinedTrigger { site, .. }
            | StateDiag::UncountedTrigger { site, .. }
            | StateDiag::DeleteLeavesChildren { site, .. }
            | StateDiag::GuardReadsLaterStep { site, .. }
            | StateDiag::DefaultOnVariationIgnored { site, .. } => self.site(site),
            StateDiag::ConflictingEffects { sites, .. }
            | StateDiag::ContradictoryDrivenComparison { sites, .. }
            | StateDiag::DrivenComparisonColumnWritten { sites, .. } => first(sites),
            // A state of a state model: its entity's declaration, where the state model is.
            StateDiag::Deadlock { entity, .. } | StateDiag::NoWitness { entity, .. } => {
                self.entity(entity)
            }
            StateDiag::UnreachableEnumVariant { .. }
            | StateDiag::NoCreationPath
            | StateDiag::PatternCapReached { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::MemoryCapReached { .. }
            | StateDiag::TimeBudgetReached { .. } => None,
        };
        at.or_else(|| self.entity(entity?))
            .or_else(|| self.first_line())
    }

    /// Where an undecided finding goes: the entity whose state model it reads, the use case
    /// that may not run, or the `can_reach` property.
    fn undecided(&self, undecided: &Undecided) -> Option<(PathBuf, Range)> {
        let at = match &undecided.subject {
            Subject::State { entity, .. } | Subject::Deadlock { entity, .. } => self.entity(entity),
            Subject::Runs { action }
            | Subject::Arm { action, .. }
            | Subject::Choice { action, .. } => self.action(action),
            Subject::Reach { name } => (self.ts.reach_properties().iter())
                .find(|r| r.name == *name)
                .and_then(|r| self.site(&r.site)),
        };
        at.or_else(|| self.first_line())
    }

    /// The first line of the first source file, for a finding with no site of its own.
    fn first_line(&self) -> Option<(PathBuf, Range)> {
        let (path, text, _) =
            (self.analysis.program.sources.iter()).min_by(|a, b| a.0.cmp(&b.0))?;
        let end = text.find('\n').unwrap_or(text.len());
        Some((path.clone(), span_to_range(text, 0..end)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_read_what_they_set_and_keep_the_rest() {
        let base = Settings::default();
        assert!(base.enabled);
        let options = json!({"lifecycleCheck": {"enabled": false, "timeBudgetMs": 50}});
        let read = base.with(Settings::section(&options).unwrap());
        assert!(!read.enabled);
        assert_eq!(read.budget, Duration::from_millis(50));
        assert_eq!(read.memory, base.memory);

        let change = json!({"rdra-ish": {"lifecycleCheck": {"memory": "1GiB", "enabled": "no"}}});
        let read = base.with(Settings::section(&change).unwrap());
        assert_eq!(read.memory.bytes(), 1 << 30);
        assert!(
            read.enabled,
            "a value that does not parse keeps the setting"
        );
        assert!(Settings::section(&json!({"other": 1})).is_none());
    }
}
