//! What a passing conformance run says (#439): the coverage statement `check --code=run` and
//! `codegen --conformance` print. The claim is bounded conformance, never equivalence: the
//! safety rules the model verified over tracked columns hold for runs driven through the port,
//! within the traces' bound, under the assumptions H1-H5 the statement lists.

use super::Traces;

/// The assumptions the claim rests on, in one line.
pub const ASSUMES: &str = "assumes: H1 the service answers from the tracked columns and the call alone; H2 every reachable call is tried; H3 seed and snapshot reach the real store; H4 every write goes through the port, one call one transaction; H5 the rules read tracked columns only";

/// What no run of the traces can show, in one line.
pub const CANNOT_SHOW: &str = "cannot show: writes outside the port, locking, races, external effects, liveness, who issues a capability, data outside the model";

impl Traces {
    /// The coverage statement of these traces: a title naming `subject`, then indented lines in
    /// a fixed order. Counts only (no path, no time), so the text is stable. `unchecked` is what
    /// the harness does not compare (`PortDecl::unchecked`).
    pub fn coverage(&self, subject: &str, unchecked: &[String]) -> Vec<String> {
        let mut lines = vec![format!(
            "coverage of {subject}: bounded conformance to the model, not equivalence"
        )];
        let rows = if self.instances == 1 { "row" } else { "rows" };
        lines.push(format!(
            "  bound: {} {rows} created per entity, Int 0..{}",
            self.instances, self.int_max
        ));
        lines.push(if self.complete {
            format!(
                "  BFS: complete, {} states; {} reached by the traces' calls",
                self.explored, self.states
            )
        } else {
            format!(
                "  BFS: stopped at its cap of {} states; {} reached by the traces' calls",
                self.cap, self.states
            )
        });
        let mut moves = format!("  moves: {} of {} run", self.covered, self.moves);
        if self.left_out > 0 {
            let by: Vec<String> = self
                .left_out_by
                .iter()
                .map(|(what, n)| format!("{what} {n}"))
                .collect();
            moves.push_str(&format!(
                "; {} left out (a tick, or a use case with no function: {})",
                self.left_out,
                by.join(", ")
            ));
        }
        lines.push(moves);
        lines.push(if self.complete {
            let kinds: Vec<String> = self
                .kinds
                .iter()
                .map(|(kind, n)| match *kind {
                    "-" => format!("any kind {n}"),
                    kind => format!("{kind} {n}"),
                })
                .collect();
            let kinds = if kinds.is_empty() {
                String::new()
            } else {
                format!(" ({})", kinds.join(", "))
            };
            format!(
                "  refused calls: {} tried{kinds}; interleaved commits: {}",
                self.refusals, self.interleavings
            )
        } else if self.refusals == 0 {
            format!(
                "  refused calls: none tried (the BFS stopped at its cap); interleaved commits: {}",
                self.interleavings
            )
        } else {
            // A call on a row that does not exist is tried whatever the BFS reached.
            format!(
                "  refused calls: {} tried, each on a row that does not exist (the BFS stopped at its cap); interleaved commits: {}",
                self.refusals, self.interleavings
            )
        });
        if self.complete && self.untried.total() > 0 {
            lines.push(format!("  untried refusals: {}", self.untried_parts()));
        }
        if self.seeded > 0 {
            lines.push(format!(
                "  seeded at a reached state: {} traces, each then one call (H1: seed and path agree)",
                self.seeded
            ));
        }
        if !self.never_run.is_empty() {
            lines.push(format!(
                "  never run: {} ({})",
                self.never_run.len(),
                self.never_run.join(", ")
            ));
        }
        if self.complete && !self.never_refused.is_empty() {
            lines.push(format!(
                "  never refused: {} ({})",
                self.never_refused.len(),
                self.never_refused.join(", ")
            ));
        }
        if !unchecked.is_empty() {
            lines.push(format!("  not compared: {}", unchecked.join("; ")));
        }
        lines.push(format!("  {ASSUMES}"));
        lines.push(format!("  {CANNOT_SHOW}"));
        lines
    }

    /// The untried refusals, by why: `<n> (<a> the functions run, ...)`.
    fn untried_parts(&self) -> String {
        let u = &self.untried;
        let parts: Vec<String> = [(u.capped, "past 64 bindings of one call")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, why)| format!("{n} {why}"))
            .collect();
        format!("{} ({})", u.total(), parts.join(", "))
    }

    /// The warning of traces that are not complete, for `subject`; `None` when they are: the
    /// BFS stopped at its cap, the model refuses calls the traces do not try, or the traces
    /// never run an action of the port.
    pub fn incomplete(&self, subject: &str) -> Option<String> {
        let mut why = Vec::new();
        if !self.complete {
            why.push(format!(
                "the conformance BFS stopped at its cap of {} states: the moves past it are not run and no call is tried as refused, so a service that runs a call the model refuses passes",
                self.cap
            ));
        } else if self.untried.total() > 0 {
            why.push(format!(
                "the model refuses calls the traces do not try, {}: a service that runs one of them passes",
                self.untried_parts()
            ));
        }
        if !self.never_run.is_empty() {
            why.push(format!(
                "the traces never run {}: a service that does nothing on them passes",
                self.never_run.join(", ")
            ));
        }
        (!why.is_empty()).then(|| format!("{subject}: {}", why.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(complete: bool) -> Traces {
        Traces {
            explored: 40,
            states: 37,
            moves: 12,
            covered: 12,
            left_out: 3,
            left_out_by: [("a tick".to_string(), 2), ("Audit".to_string(), 1)]
                .into_iter()
                .collect(),
            complete,
            traces: 5,
            calls: 30,
            refusals: if complete { 8 } else { 0 },
            kinds: if complete {
                [("NotEnabled", 6), ("Rejected", 2)].into_iter().collect()
            } else {
                Default::default()
            },
            interleavings: 2,
            instances: 1,
            int_max: 5,
            cap: 4096,
            ..Traces::default()
        }
    }

    #[test]
    fn a_complete_suite_states_its_bound_and_counts() {
        let lines = stats(true).coverage("[code.rust]", &["the clock".to_string()]);
        assert_eq!(
            lines,
            [
                "coverage of [code.rust]: bounded conformance to the model, not equivalence",
                "  bound: 1 row created per entity, Int 0..5",
                "  BFS: complete, 40 states; 37 reached by the traces' calls",
                "  moves: 12 of 12 run; 3 left out (a tick, or a use case with no function: Audit 1, a tick 2)",
                "  refused calls: 8 tried (NotEnabled 6, Rejected 2); interleaved commits: 2",
                "  not compared: the clock",
                &format!("  {ASSUMES}"),
                &format!("  {CANNOT_SHOW}"),
            ]
        );
        assert_eq!(stats(true).incomplete("[code.rust]"), None);
    }

    /// A complete suite that leaves refusals of the model untried, or actions of the port
    /// unrun, says how many and which, and warns as a capped one does.
    #[test]
    fn untried_refusals_and_unrun_actions_are_counted_and_warned_about() {
        let mut t = stats(true);
        t.untried = super::super::Untried { capped: 2 };
        t.never_run = vec!["Audit".to_string()];
        t.never_refused = vec!["Place".to_string()];
        t.seeded = 3;
        let lines = t.coverage("[code.ts]", &[]);
        for line in [
            "  untried refusals: 2 (2 past 64 bindings of one call)",
            "  seeded at a reached state: 3 traces, each then one call (H1: seed and path agree)",
            "  never run: 1 (Audit)",
            "  never refused: 1 (Place)",
        ] {
            assert!(lines.contains(&line.to_string()), "{line}: {lines:?}");
        }
        let warning = t.incomplete("[code.ts]").expect("untried refusals warn");
        assert_eq!(
            warning,
            "[code.ts]: the model refuses calls the traces do not try, 2 (2 past 64 bindings of one call): a service that runs one of them passes; the traces never run Audit: a service that does nothing on them passes"
        );
    }

    #[test]
    fn a_capped_suite_says_no_refused_call_is_tried() {
        let t = stats(false);
        let lines = t.coverage("[code.go]", &[]);
        assert!(lines.contains(
            &"  BFS: stopped at its cap of 4096 states; 37 reached by the traces' calls"
                .to_string()
        ));
        assert!(lines.contains(
            &"  refused calls: none tried (the BFS stopped at its cap); interleaved commits: 2"
                .to_string()
        ));
        assert!(!lines.iter().any(|l| l.contains("not compared")));
        let warning = t.incomplete("[code.go]").expect("a capped suite warns");
        assert!(
            warning.starts_with("[code.go]: the conformance BFS stopped at its cap of 4096 states")
        );
    }

    /// The claim is bounded conformance: no line calls a passing suite equivalence.
    #[test]
    fn no_line_claims_equivalence() {
        for t in [stats(true), stats(false)] {
            let all = t.coverage("x", &[]).join("\n");
            let claims: Vec<&str> = all
                .lines()
                .filter(|l| l.contains("equivalen") && !l.contains("not equivalence"))
                .collect();
            assert!(claims.is_empty(), "{claims:?}");
        }
    }
}
