use super::bfs::{Bfs, Explored, Safety};
use super::report::Temporal;
use crate::prove::ProofTally;
use crate::tlc_output::Outcome;
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
    /// The verdict the `result:` line states: of the invariants, the after-asserts, and the
    /// properties, every form and `can_reach` included. The state-model findings of the
    /// entity cones fail the run beside it and do not move it.
    pub(super) fn outcome(&self) -> Outcome {
        if self.temporal.faults > 0 {
            return Outcome::ToolError;
        }
        let violated = self.safety.violated
            + self.explored.violated
            + self.temporal.violated
            + self.bfs.reach_errors;
        match violated {
            0 => Outcome::Ok,
            _ => Outcome::Counterexample,
        }
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
                self.bfs.capped + self.explored.capped + self.safety.capped + self.temporal.capped,
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
