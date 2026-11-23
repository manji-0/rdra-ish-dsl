use super::bfs::{Bfs, Explored, Safety};
use super::report::Temporal;
use crate::prove::ProofTally;
use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::BoundsLine;

/// What TLC adds to the end of a `--with-tlc` run.
#[derive(Default)]
pub(super) struct TlcEnd {
    /// TLC runs that failed without a model violation.
    pub(super) tool_errors: usize,
    /// Checks the two engines decide differently.
    pub(super) disagreements: usize,
}

/// What the `result:` line says of the whole run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RunResult {
    Ok,
    /// Nothing failed, but a cap, the budget, or `--deny-unproved` left something undecided.
    NotDecided,
    Counterexample,
    ToolError,
}

impl RunResult {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            RunResult::Ok => "ok",
            RunResult::NotDecided => "not decided",
            RunResult::Counterexample => "counterexample",
            RunResult::ToolError => "tool_error",
        }
    }
}

/// How a run ends.
pub(super) struct End {
    pub(super) bfs: Bfs,
    pub(super) explored: Explored,
    /// What BFS decided of the safety cones.
    pub(super) safety: Safety,
    /// What the native checker decided of `always`, `eventually`, and `leads_to`.
    pub(super) temporal: Temporal,
    pub(super) tlc: TlcEnd,
    pub(super) deny_undecided: bool,
    /// What `--prove` proved, when it ran.
    pub(super) proof: Option<ProofTally>,
}

impl End {
    /// The verdict the `result:` line states for the whole run, so that `ok` means exit 0 with
    /// nothing undecided: a tool error first, then a failure (a violation, or a state-model or
    /// `can_reach` error of BFS), then anything a cap, the budget, or `--deny-unproved` left
    /// undecided.
    pub(super) fn result(&self, undecided: usize) -> RunResult {
        let tool = self.temporal.faults + self.tlc.disagreements + self.tlc.tool_errors;
        let failed = self.bfs.errors
            + self.bfs.reach_errors
            + self.safety.violated
            + self.explored.violated
            + self.temporal.violated;
        let open = self.capped() + undecided + self.unproved_denied();
        match (tool, failed, open) {
            (0, 0, 0) => RunResult::Ok,
            (0, 0, _) => RunResult::NotDecided,
            (0, _, _) => RunResult::Counterexample,
            _ => RunResult::ToolError,
        }
    }

    /// Findings a cone stopped at the state cap or the memory limit left.
    fn capped(&self) -> usize {
        self.bfs.capped + self.explored.capped + self.safety.capped + self.temporal.capped
    }

    /// Rules left unproved when `--deny-unproved` makes that fail the run.
    fn unproved_denied(&self) -> usize {
        self.proof
            .filter(|p| p.deny_unproved)
            .map_or(0, |p| p.unproved)
    }

    /// A violation, an error of BFS, or a tool error fails the run. A temporal property the
    /// budget left undecided is listed, as the BFS cap lists the findings it leaves; what the
    /// bounds left undecided is a warning, and an error under `--deny-undecided`.
    pub(super) fn finish(&self, line: &BoundsLine, undecided: &[String]) -> Result<()> {
        if !undecided.is_empty() {
            println!(
                "not checked: {} not decided",
                match undecided.len() {
                    1 => "1 temporal property".to_string(),
                    n => format!("{n} temporal properties"),
                }
            );
            for u in undecided {
                println!("{u}");
            }
        }
        let failed: Vec<String> = [
            (
                self.bfs.errors,
                "state-model error(s) from BFS on the entity cones",
            ),
            (
                self.bfs.reach_errors,
                "can_reach error(s) from BFS on their cones",
            ),
            (
                self.safety.violated,
                "invariant or after-assert violation(s)",
            ),
            (
                self.explored.violated + self.temporal.violated,
                "property violation(s)",
            ),
            (
                self.temporal.faults,
                "counterexample(s) that fail their own check (a tool bug)",
            ),
            (
                self.tlc.disagreements,
                "check(s) the native checker and TLC decide differently (a tool bug)",
            ),
            (
                self.tlc.tool_errors,
                "TLC run(s) that failed without a model violation",
            ),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !failed.is_empty() {
            bail!("verification failed: {}", failed.join(" and "));
        }
        let open = [
            (
                self.capped(),
                "finding a cone stopped at the state cap or the memory limit left",
                "findings a cone stopped at the state cap or the memory limit left",
            ),
            (
                undecided.len(),
                "temporal property the budget left",
                "temporal properties the budget left",
            ),
        ]
        .iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect::<Vec<_>>();
        if !open.is_empty() {
            let what = format!("{} undecided", open.join(" and "));
            if self.deny_undecided {
                bail!("verification incomplete: {what} (--deny-undecided)");
            }
            eprintln!("warning: {what}; the OK line holds only for what was decided (--deny-undecided fails the run)");
        }
        match self.proof {
            Some(p) if p.unproved > 0 && p.deny_unproved => {
                bail!(
                    "verification incomplete: {} of {} rules unproved for any number of rows and any Int value (--deny-unproved)",
                    p.unproved,
                    p.rules
                );
            }
            Some(p) => println!("{}; {}", line.ok(), p.clause()),
            None => println!("{}", line.ok()),
        }
        Ok(())
    }
}
