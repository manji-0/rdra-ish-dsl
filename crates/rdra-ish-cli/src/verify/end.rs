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

/// A count that fails the run, and what it counts.
struct Failure {
    count: usize,
    /// A tool error, not a finding of the model.
    tool: bool,
    what: &'static str,
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
    /// Rules BFS violated at more instances than the run's, checked there because the proof
    /// left them unproved (`escalate`).
    pub(super) escalated: usize,
}

impl End {
    /// The verdict the `result:` line states for the whole run, so that `ok` means exit 0 with
    /// nothing undecided: a tool error first, then a failure (a violation, or a state-model or
    /// `can_reach` error of BFS), then anything a cap, the budget, or `--deny-unproved` left
    /// undecided.
    pub(super) fn result(&self, undecided: usize) -> RunResult {
        let count = |tool: bool| -> usize {
            (self.failures().iter())
                .filter(|f| f.tool == tool)
                .map(|f| f.count)
                .sum()
        };
        let (tool, failed) = (count(true), count(false));
        let open = self.capped() + undecided + self.unproved_denied();
        match (tool, failed, open) {
            (0, 0, 0) => RunResult::Ok,
            (0, 0, _) => RunResult::NotDecided,
            (0, _, _) => RunResult::Counterexample,
            _ => RunResult::ToolError,
        }
    }

    /// Each count that fails the run, in the order the failure message lists them.
    fn failures(&self) -> [Failure; 9] {
        let failure = |count, tool, what| Failure { count, tool, what };
        [
            failure(
                self.bfs.errors,
                false,
                "state-model error(s) from BFS on the entity cones",
            ),
            failure(
                self.bfs.reach_errors,
                false,
                "can_reach error(s) from BFS on their cones",
            ),
            failure(
                self.safety.violated,
                false,
                "invariant or after-assert violation(s)",
            ),
            failure(
                self.escalated,
                false,
                "invariant or after-assert violation(s) at more instances than the run's",
            ),
            failure(
                self.explored.violated + self.temporal.violated,
                false,
                "property violation(s)",
            ),
            failure(
                self.temporal.faults
                    + self.safety.faults
                    + self.explored.faults
                    + self.bfs.trace_faults,
                true,
                "counterexample(s) or trace(s) that fail their own check (a tool bug)",
            ),
            failure(
                self.tlc.disagreements,
                true,
                "check(s) the native checker and TLC decide differently (a tool bug)",
            ),
            failure(
                self.tlc.tool_errors,
                true,
                "TLC run(s) that failed without a model violation",
            ),
            failure(
                self.proof.as_ref().map_or(0, |p| p.faults),
                true,
                "check(s) of the proof's output that failed (a tool bug)",
            ),
        ]
    }

    /// Findings a cone stopped at the state cap or the memory limit left.
    fn capped(&self) -> usize {
        self.bfs.capped + self.explored.capped + self.safety.capped + self.temporal.capped
    }

    /// Rules left unproved when `--deny-unproved` makes that fail the run.
    fn unproved_denied(&self) -> usize {
        self.proof
            .as_ref()
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
        let failed: Vec<String> = (self.failures().iter())
            .filter(|f| f.count > 0)
            .map(|f| format!("{} {}", f.count, f.what))
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
        match &self.proof {
            Some(p) if p.unproved > 0 && p.deny_unproved => {
                bail!(
                    "verification incomplete: {} of {} rules unproved {} (--deny-unproved)",
                    p.unproved,
                    p.rules,
                    p.claim.as_str()
                );
            }
            Some(p) => println!("{}; {}", line.ok(), p.clause()),
            None => println!("{}", line.ok()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end() -> End {
        End {
            bfs: Bfs::default(),
            explored: Explored::default(),
            safety: Safety::default(),
            temporal: Temporal::default(),
            tlc: TlcEnd::default(),
            deny_undecided: false,
            proof: None,
            escalated: 0,
        }
    }

    /// A deadlock's, a `can_reach` failure's, or a witness's trace that fails to replay is a
    /// fault of the tool (claim N16 of docs/theory/claims.md): the run's result is
    /// `tool_error`, as for a `LiftFault`, and never a finding about the model.
    #[test]
    fn a_trace_fault_is_a_tool_error() {
        assert_eq!(end().result(0), RunResult::Ok);
        let mut faulty = end();
        faulty.bfs.trace_faults = 1;
        assert_eq!(faulty.result(0), RunResult::ToolError);
        faulty.bfs.errors = 1;
        assert_eq!(
            faulty.result(0),
            RunResult::ToolError,
            "a tool error comes first"
        );
        let mut failed = end();
        failed.bfs.errors = 1;
        assert_eq!(failed.result(0), RunResult::Counterexample);
    }

    /// A check of the proof's own output that fails (a wrong comparison, a build outside
    /// Lemma U, a kept lemma a reached state breaks) is a tool error, never a finding.
    #[test]
    fn a_proof_fault_is_a_tool_error() {
        let mut faulty = end();
        faulty.proof = Some(ProofTally {
            proved: 0,
            rules: 1,
            unproved: 1,
            deny_unproved: false,
            claim: crate::prove::Claim::plain("for any number of rows, in the whole model"),
            faults: 1,
        });
        assert_eq!(faulty.result(0), RunResult::ToolError);
        if let Some(p) = &mut faulty.proof {
            p.faults = 0;
        }
        assert_eq!(faulty.result(0), RunResult::Ok);
    }
}
