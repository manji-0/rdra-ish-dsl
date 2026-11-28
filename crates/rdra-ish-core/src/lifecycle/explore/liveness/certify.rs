//! The certificate of a liveness search that finds no counterexample for a binding (claims N9
//! and N10 of docs/theory/claims.md). `avoid` leaves, per state, its component (`comp`), the
//! fair components (`fair_cycle`), the stops and the backward walk (`toward`); the search
//! finds no start when no state with `toward != NONE` is triggered with the cause. This
//! checker holds that witness to N9's criterion on the recorded graph in one pass over the
//! states and one over the edges, without reading `avoid`'s own `N`, adjacency or bit sets:
//!
//! - `Order`: every edge of `N`, re-derived from the goal on both ends, goes from a component
//!   to one numbered no higher, so every cycle of `N` lies inside one component, whatever
//!   Tarjan did.
//! - `FairComponent`: each component's fair flag is the one its size, its states at or past
//!   `expanded`, the fair instances enabled at all of its states and the instances its edges
//!   take give, recomputed here.
//! - `Stop`: each state's stop flag is the stop rule's.
//! - `End`: every stop and every state of a fair component has `toward != NONE`.
//! - `Closure`: for every edge of `N`, `toward != NONE` at its target gives it at its source.
//! - `Start`: no state with `toward != NONE` is triggered with the cause.
//!
//! Then no triggered state of `N` with the cause reaches an end inside `N`: a fair strongly
//! connected set of `N` lies in one component (`Order`), whose recomputed flag is set since a
//! larger component has fewer instances enabled throughout and more taken (`FairComponent`),
//! so its states, like the stops (`Stop`), have `toward` set (`End`), and so has every state
//! that reaches one inside `N` (`Closure`). On a complete graph that is N9's `ok`. The checker
//! shares the graph's states and edges, `holds_bound`, `triggers`, `fair_steps` (the
//! instances and where each is enabled) and `expanded` with the search.

use std::fmt;

use crate::lifecycle::lasso::Fairness;
use crate::lifecycle::narrow::id32;
use crate::lifecycle::types::{Pred, Property};

use super::{Avoid, FairSteps, StopRule, NONE};
use crate::lifecycle::explore::{ComponentRun, Edge};

/// The clause of the certificate a search's witness fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchClause {
    /// An edge of `N` goes to a component numbered higher, or a state of `N` has no
    /// component.
    Order,
    /// A component's fair flag is not the one its states and edges give.
    FairComponent,
    /// A state's stop flag is not the stop rule's.
    Stop,
    /// A stop or a state of a fair component has no walk toward an end.
    End,
    /// An edge of `N` leads to a state with a walk toward an end from one without.
    Closure,
    /// A state with a walk toward an end is triggered with the cause.
    Start,
}

const CLAUSES: usize = 6;

/// One fault of a search's witness: its clause and the first place that breaks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFault {
    pub clause: SearchClause,
    what: String,
}

impl fmt::Display for SearchFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.clause, self.what)
    }
}

impl ComponentRun {
    /// The faults of `avoid`, the search of property `p` at binding `b`, against N9's
    /// criterion: at most one per clause, the first in state or edge order, in clause order.
    /// Empty when the witness shows that no triggered state of `N` with `cause` reaches an
    /// end inside `N`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn search_faults(
        &self,
        p: &Property,
        cause: Option<&Pred>,
        goal: &Pred,
        b: &[u8],
        fair: &FairSteps,
        rule: StopRule,
        avoid: &Avoid,
    ) -> Vec<SearchFault> {
        let ts = &self.system;
        let n = self.states.len();
        let expanded = self.expanded();
        let count = avoid.fair_cycle.len();
        let w = fair.words;
        let mut first: [Option<String>; CLAUSES] = Default::default();
        let mut fault = |clause: SearchClause, what: &dyn Fn() -> String| {
            first[clause as usize].get_or_insert_with(what);
        };
        if avoid.comp.len() != n || avoid.toward.len() != n || avoid.stops.len() != n {
            fault(SearchClause::Order, &|| {
                "the witness is not one entry per state".into()
            });
            return collect(first);
        }
        let mut in_n = vec![false; n];
        let mut size = vec![0u32; count];
        let mut open = vec![false; count];
        let mut always = vec![!0u64; count * w];
        for (s, st) in self.states.iter().enumerate() {
            let at = id32(s);
            let failing = !ts.holds_bound(goal, &st, b);
            in_n[s] = failing;
            let reaches = avoid.toward[s] != NONE;
            if reaches && ts.triggers(p, b, &st) && cause.is_none_or(|x| ts.holds_bound(x, &st, b))
            {
                fault(SearchClause::Start, &|| {
                    format!("state {s} is triggered with the cause")
                });
            }
            let stop = failing
                && s < expanded
                && match rule {
                    StopRule::Fair(Fairness::Plain) => none_enabled(fair, at),
                    StopRule::Fair(Fairness::WfNext) => !fair.moves_on[s],
                    StopRule::Looping(looping) => {
                        none_enabled(fair, at) && (looping[s] || !fair.moves_on[s])
                    }
                };
            if stop != avoid.stops[s] {
                fault(SearchClause::Stop, &|| {
                    format!("state {s} is marked {} but is {stop}", avoid.stops[s])
                });
            }
            if !failing {
                continue;
            }
            let c = avoid.comp[s] as usize;
            if c >= count {
                fault(SearchClause::Order, &|| {
                    format!("state {s} has no component")
                });
                continue;
            }
            size[c] += 1;
            open[c] |= s >= expanded;
            for (acc, en) in always[c * w..][..w].iter_mut().zip(fair.enabled(at)) {
                *acc &= en;
            }
            if !reaches && (stop || avoid.fair_cycle[c]) {
                fault(SearchClause::End, &|| {
                    format!("state {s} is an end with no walk")
                });
            }
        }
        let mut taken = vec![0u64; count * w];
        // `for_each`, not `for`: a run's edges fold a state at a time (`EdgeIter::fold`).
        self.edges().iter().for_each(|e: Edge| {
            let (from, to) = (e.from as usize, e.to as usize);
            if !(in_n[from] && in_n[to]) {
                return;
            }
            let (cf, ct) = (avoid.comp[from], avoid.comp[to]);
            if cf < ct {
                fault(SearchClause::Order, &|| {
                    format!("edge {from} -> {to} goes from component {cf} to {ct}")
                });
            } else if from != to && cf == ct && (cf as usize) < count {
                let a = fair.instance[e.binding as usize];
                if a != NONE {
                    taken[cf as usize * w + a as usize / 64] |= 1 << (a % 64);
                }
            }
            if avoid.toward[to] != NONE && avoid.toward[from] == NONE {
                fault(SearchClause::Closure, &|| {
                    format!("edge {from} -> {to} leaves the walk")
                });
            }
        });
        for c in 0..count {
            let range = c * w..(c + 1) * w;
            let fair_flag = size[c] >= 2
                && !open[c]
                && always[range.clone()]
                    .iter()
                    .zip(&taken[range])
                    .all(|(en, tk)| en & !tk == 0);
            if fair_flag != avoid.fair_cycle[c] {
                fault(SearchClause::FairComponent, &|| {
                    format!(
                        "component {c} is marked {} but is {fair_flag}",
                        avoid.fair_cycle[c]
                    )
                });
            }
        }
        collect(first)
    }
}

/// No fair instance is enabled at `at`.
fn none_enabled(fair: &FairSteps, at: u32) -> bool {
    fair.enabled(at).iter().all(|w| *w == 0)
}

fn collect(first: [Option<String>; CLAUSES]) -> Vec<SearchFault> {
    const ORDER: [SearchClause; CLAUSES] = [
        SearchClause::Order,
        SearchClause::FairComponent,
        SearchClause::Stop,
        SearchClause::End,
        SearchClause::Closure,
        SearchClause::Start,
    ];
    ORDER
        .into_iter()
        .zip(first)
        .filter_map(|(clause, what)| what.map(|what| SearchFault { clause, what }))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    use crate::lifecycle::explore::{Caps, PropertyVerdict};
    use crate::lifecycle::tests::system_from;
    use crate::lifecycle::types::Ltl;
    use crate::lifecycle::{Bounds, LassoFault};

    /// Paid orders that may be held and released before they ship; `Ship` is enabled at
    /// both, so under `Plain` no fair behavior keeps an order paid or held forever, while the
    /// hold cycle is a component of two states that is not fair. `Pay` alone moves a pending
    /// order, so a stop under `Plain` lies before the trigger.
    const HOLD: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, held, shipped) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Hold "保留"
usecase Release "保留解除"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Hold, Order.status, paid -> held)
transitions(Release, Order.status, held -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Ship, Order.status, held -> shipped)
property Ships leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
property Shipped eventually(Order.status == shipped)
property Unpaid leads_to(Order.status == pending, Order.status == shipped).fair(Ship)
"#;

    /// Property `name` of `HOLD` on its complete graph at binding 0 under `fairness`, and the
    /// clauses `plant`'s change to its witness breaks.
    fn planted(
        name: &str,
        fairness: Fairness,
        plant: impl FnOnce(&ComponentRun, &mut Avoid, &[bool]),
    ) -> BTreeSet<SearchClause> {
        let run = system_from(HOLD, Bounds::VERIFY).run(Caps::VERIFY);
        assert!(run.complete);
        let ts = &run.system;
        let i = (ts.properties().iter())
            .position(|p| p.name == name)
            .expect("a property of HOLD");
        let p = &ts.properties()[i];
        let (cause, goal) = match &p.formula {
            Ltl::Eventually(g) => (None, g),
            Ltl::LeadsTo(x, g) => (Some(x), g),
            other => panic!("{:?}", other.kind()),
        };
        let b = ts.binder_slots(&p.binders).next().unwrap_or_default();
        let fair = run.fair_steps(i);
        let rule = StopRule::Fair(fairness);
        let mut avoid = run.avoid(&fair, goal, &b, rule);
        let in_n: Vec<bool> = (run.states.iter())
            .map(|s| !ts.holds_bound(goal, &s, &b))
            .collect();
        plant(&run, &mut avoid, &in_n);
        (run.search_faults(p, cause, goal, &b, &fair, rule, &avoid)
            .iter())
        .map(|f| f.clause)
        .collect()
    }

    /// The first edge of `run` inside `N` that `keep` accepts, as `(from, to)`.
    fn edge_in(run: &ComponentRun, in_n: &[bool], keep: impl Fn(u32, u32) -> bool) -> (u32, u32) {
        let mut found = None;
        run.edges().iter().for_each(|e: Edge| {
            let inside = in_n[e.from as usize] && in_n[e.to as usize];
            if found.is_none() && inside && keep(e.from, e.to) {
                found = Some((e.from, e.to));
            }
        });
        found.expect("an edge of N the plant needs")
    }

    /// The witness of every `ok` here passes; the search of a violated property finds a start,
    /// which the `Start` clause names.
    #[test]
    fn an_unplanted_witness_passes_and_a_start_fails_it() {
        for fairness in [Fairness::Plain, Fairness::WfNext] {
            assert_eq!(planted("Ships", fairness, |_, _, _| ()), BTreeSet::new());
        }
        let start = BTreeSet::from([SearchClause::Start]);
        assert_eq!(planted("Shipped", Fairness::Plain, |_, _, _| ()), start);
        assert_eq!(planted("Unpaid", Fairness::Plain, |_, _, _| ()), start);
    }

    /// Each clause fails on a witness planted to break it.
    #[test]
    fn a_planted_witness_fails_the_clause_it_breaks() {
        // Two components joined by an edge of `N` swap numbers: the edge then climbs.
        let order = planted("Ships", Fairness::Plain, |run, avoid, in_n| {
            let (from, to) = edge_in(run, in_n, |f, t| {
                avoid.comp[f as usize] != avoid.comp[t as usize]
            });
            let (a, b) = (avoid.comp[from as usize], avoid.comp[to as usize]);
            for c in avoid.comp.iter_mut() {
                *c = if *c == a {
                    b
                } else if *c == b {
                    a
                } else {
                    *c
                };
            }
            avoid.fair_cycle.swap(a as usize, b as usize);
        });
        assert!(order.contains(&SearchClause::Order), "{order:?}");
        // The hold cycle, a component of two states that is not fair, is marked fair.
        let fair = planted("Ships", Fairness::Plain, |run, avoid, in_n| {
            let (from, _) = edge_in(run, in_n, |f, t| {
                f != t && avoid.comp[f as usize] == avoid.comp[t as usize]
            });
            avoid.fair_cycle[avoid.comp[from as usize] as usize] = true;
        });
        assert!(fair.contains(&SearchClause::FairComponent), "{fair:?}");
        // A stop is marked as none.
        let stop = planted("Ships", Fairness::Plain, |_, avoid, _| {
            let s = avoid.stops.iter().position(|&s| s).expect("a stop");
            avoid.stops[s] = false;
        });
        assert_eq!(stop, BTreeSet::from([SearchClause::Stop]));
        // A stop loses its walk, as a search that drops a fair end would leave it.
        let end = planted("Ships", Fairness::Plain, |_, avoid, _| {
            let s = avoid.stops.iter().position(|&s| s).expect("a stop");
            avoid.toward[s] = NONE;
        });
        assert!(end.contains(&SearchClause::End), "{end:?}");
        // A state that is no end loses the walk its successor has.
        let closure = planted("Ships", Fairness::Plain, |run, avoid, in_n| {
            let (from, _) = edge_in(run, in_n, |f, t| {
                f != t && !avoid.stops[f as usize] && avoid.toward[t as usize] != NONE
            });
            avoid.toward[from as usize] = NONE;
        });
        assert_eq!(closure, BTreeSet::from([SearchClause::Closure]));
    }

    /// Every property of `HOLD`, with `Ship` fair and with nothing fair, decides under both
    /// fairnesses with no fault: `Unpaid` and `Shipped` fail, and `Ships` holds where `Ship`
    /// is fair, and fails under `WfNext` without it through the hold cycle.
    #[test]
    fn every_property_of_hold_decides_without_a_fault() {
        for src in [HOLD.to_string(), HOLD.replace(".fair(Ship)", "")] {
            let run = system_from(&src, Bounds::VERIFY).run(Caps::VERIFY);
            for i in 0..run.system.properties().len() {
                for fairness in [Fairness::Plain, Fairness::WfNext] {
                    let name = &run.system.properties()[i].name;
                    match run.decide_liveness(i, fairness) {
                        Ok(PropertyVerdict::Holds | PropertyVerdict::Violated(_)) => {}
                        other => panic!("{name} under {fairness:?}: {other:?}"),
                    }
                }
            }
        }
    }

    /// A search that drops every fair component misses the hold cycle, which under
    /// `WfNext` is the only end, and finds no start: the certificate makes that a fault
    /// instead of an `ok`.
    #[test]
    fn a_search_that_drops_a_fair_cycle_is_a_fault_not_an_ok() {
        let ts = system_from(HOLD.replace(".fair(Ship)", "").as_str(), Bounds::VERIFY);
        let i = (ts.properties().iter())
            .position(|p| p.name == "Ships")
            .expect("a property of HOLD");
        let run = ts.run(Caps::VERIFY);
        assert!(matches!(
            run.decide_liveness(i, Fairness::WfNext),
            Ok(PropertyVerdict::Violated(_))
        ));
        let dropped = dropping("fair_components", || {
            run.decide_liveness(i, Fairness::WfNext)
        });
        match dropped {
            Err(LassoFault::Search(fault)) => assert_eq!(fault.clause, SearchClause::FairComponent),
            other => panic!("{other:?}"),
        }
    }
}
